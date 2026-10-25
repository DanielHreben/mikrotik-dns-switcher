//! A small RouterOS API client. One connection serves one logical operation:
//! connect, log in, run commands, drop.

mod codec;

use std::collections::HashMap;
use std::io;
use std::sync::Arc;
use std::time::Duration;

use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, ServerName};
use tokio::io::{AsyncRead, AsyncWrite, BufStream};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_rustls::TlsConnector;

pub use codec::{encode_len, read_sentence, write_sentence};

pub const PLAIN_PORT: u16 = 8728;
pub const TLS_PORT: u16 = 8729;
const TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("router I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("router did not answer within {}s", TIMEOUT.as_secs())]
    Timeout,
    #[error("router refused the command: {0}")]
    Trap(String),
    #[error("router closed the session: {0}")]
    Fatal(String),
    #[error("unexpected reply from router: {0}")]
    Protocol(String),
    #[error("invalid TLS configuration: {0}")]
    Tls(String),
}

/// How to reach the router.
#[derive(Clone, Debug)]
pub struct Endpoint {
    pub host: String,
    pub username: String,
    pub password: String,
    /// PEM of the CA that signed the router's API-SSL certificate. `Some` means
    /// API-SSL on 8729 with the certificate verified; `None` means the plain API on
    /// 8728, for local development only.
    pub tls_ca: Option<Vec<u8>>,
    /// Overrides the API port (tests, non-standard setups).
    pub port: Option<u16>,
}

/// One row of a `print` reply, or the `=ret=` of an `add`.
pub type Row = HashMap<String, String>;

#[derive(Debug, Default)]
pub struct Reply {
    pub rows: Vec<Row>,
    pub ret: Option<String>,
}

pub struct Session<S> {
    stream: BufStream<S>,
}

pub type Connection = Session<Box<dyn Stream>>;

pub trait Stream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Stream for T {}

pub fn tls_connector(ca_pem: &[u8]) -> Result<TlsConnector, Error> {
    let mut roots = rustls::RootCertStore::empty();
    let mut count = 0;
    for cert in CertificateDer::pem_slice_iter(ca_pem) {
        let cert = cert.map_err(|e| Error::Tls(e.to_string()))?;
        roots.add(cert).map_err(|e| Error::Tls(e.to_string()))?;
        count += 1;
    }
    if count == 0 {
        return Err(Error::Tls("no certificate found in the CA file".into()));
    }
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| Error::Tls(e.to_string()))?
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(TlsConnector::from(Arc::new(config)))
}

impl Endpoint {
    /// Validates the TLS settings without touching the network, so a bad CA file
    /// stops the service at startup instead of failing every request.
    pub fn check(&self) -> Result<(), Error> {
        if let Some(ca) = &self.tls_ca {
            tls_connector(ca)?;
        }
        Ok(())
    }

    pub async fn connect(&self) -> Result<Connection, Error> {
        timeout(TIMEOUT, self.connect_inner())
            .await
            .map_err(|_| Error::Timeout)?
    }

    async fn connect_inner(&self) -> Result<Connection, Error> {
        let stream: Box<dyn Stream> = match &self.tls_ca {
            None => Box::new(
                TcpStream::connect((self.host.as_str(), self.port.unwrap_or(PLAIN_PORT))).await?,
            ),
            Some(ca) => {
                let tcp =
                    TcpStream::connect((self.host.as_str(), self.port.unwrap_or(TLS_PORT))).await?;
                let name = ServerName::try_from(self.host.clone())
                    .map_err(|e| Error::Tls(format!("{}: {e}", self.host)))?;
                Box::new(tls_connector(ca)?.connect(name, tcp).await?)
            }
        };
        let mut session = Session::new(stream);
        session.login(&self.username, &self.password).await?;
        Ok(session)
    }
}

impl<S: AsyncRead + AsyncWrite + Unpin> Session<S> {
    pub fn new(stream: S) -> Self {
        Session {
            stream: BufStream::new(stream),
        }
    }

    pub async fn login(&mut self, user: &str, password: &str) -> Result<(), Error> {
        // The RouterOS 6.43+ login: name and password in one command. This
        // switcher targets RouterOS 7.
        self.run(&[
            "/login",
            &format!("=name={user}"),
            &format!("=password={password}"),
        ])
        .await
        .map(drop)
    }

    /// Sends one command and reads until its `!done`. A `!trap` is the router
    /// refusing the command; `!fatal` ends the session.
    pub async fn run(&mut self, words: &[&str]) -> Result<Reply, Error> {
        timeout(TIMEOUT, self.run_inner(words))
            .await
            .map_err(|_| Error::Timeout)?
    }

    async fn run_inner(&mut self, words: &[&str]) -> Result<Reply, Error> {
        let words: Vec<Vec<u8>> = words.iter().map(|w| w.as_bytes().to_vec()).collect();
        write_sentence(&mut self.stream, &words).await?;

        let mut reply = Reply::default();
        let mut trap = None;
        loop {
            let sentence = read_sentence(&mut self.stream).await?;
            let Some((kind, attrs)) = sentence.split_first() else {
                continue;
            };
            let attrs = parse_attrs(attrs);
            match kind.as_slice() {
                b"!re" => reply.rows.push(attrs),
                b"!done" => {
                    reply.ret = attrs.get("ret").cloned();
                    return match trap {
                        Some(message) => Err(Error::Trap(message)),
                        None => Ok(reply),
                    };
                }
                b"!trap" => {
                    trap = Some(attrs.get("message").cloned().unwrap_or_default());
                }
                b"!fatal" => {
                    // The reason is a bare word, not a `=key=value` one.
                    let reason = sentence
                        .get(1)
                        .map(|w| String::from_utf8_lossy(w).into_owned());
                    return Err(Error::Fatal(reason.unwrap_or_default()));
                }
                other => {
                    return Err(Error::Protocol(String::from_utf8_lossy(other).into_owned()));
                }
            }
        }
    }
}

/// `=key=value` words into a map. Words without the `=` form (a `!fatal`
/// message, `.tag=`) are skipped here.
fn parse_attrs(words: &[Vec<u8>]) -> Row {
    words
        .iter()
        .filter_map(|word| {
            let word = String::from_utf8_lossy(word);
            let rest = word.strip_prefix('=')?;
            let (key, value) = rest.split_once('=')?;
            Some((key.to_string(), value.to_string()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_attribute_words() {
        let words = [
            b"=.id=*5".to_vec(),
            b"=comment=a=b".to_vec(),
            b"=flag=".to_vec(),
            b".tag=1".to_vec(),
        ];
        let row = parse_attrs(&words);
        assert_eq!(row[".id"], "*5");
        assert_eq!(row["comment"], "a=b");
        assert_eq!(row["flag"], "");
        assert_eq!(row.len(), 3);
    }

    #[test]
    fn rejects_an_empty_or_invalid_ca() {
        assert!(tls_connector(b"").is_err());
        assert!(tls_connector(b"not a pem").is_err());
    }

    async fn serve(replies: Vec<Vec<&'static [u8]>>) -> Session<tokio::io::DuplexStream> {
        let (client, mut server) = tokio::io::duplex(4096);
        tokio::spawn(async move {
            read_sentence(&mut server).await.unwrap();
            for sentence in replies {
                let words: Vec<Vec<u8>> = sentence.iter().map(|w| w.to_vec()).collect();
                write_sentence(&mut server, &words).await.unwrap();
            }
        });
        Session::new(client)
    }

    #[tokio::test]
    async fn collects_rows_and_ret() {
        let mut s = serve(vec![
            vec![b"!re", b"=.id=*1", b"=address=10.0.0.5"],
            vec![b"!re", b"=.id=*2"],
            vec![b"!done", b"=ret=*9"],
        ])
        .await;
        let reply = s.run(&["/ip/arp/print"]).await.unwrap();
        assert_eq!(reply.rows.len(), 2);
        assert_eq!(reply.rows[0]["address"], "10.0.0.5");
        assert_eq!(reply.ret.as_deref(), Some("*9"));
    }

    #[tokio::test]
    async fn a_trap_is_an_error_even_though_done_follows() {
        let mut s = serve(vec![
            vec![b"!trap", b"=message=failure: already have such entry"],
            vec![b"!done"],
        ])
        .await;
        match s.run(&["/ip/dhcp-server/lease/add"]).await {
            Err(Error::Trap(m)) => assert_eq!(m, "failure: already have such entry"),
            other => panic!("expected a trap, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn fatal_ends_the_session() {
        let mut s = serve(vec![vec![b"!fatal", b"timeout"]]).await;
        assert!(matches!(s.run(&["/x"]).await, Err(Error::Fatal(m)) if m == "timeout"));
    }

    #[tokio::test]
    async fn a_router_that_hangs_up_is_an_error() {
        let mut s = serve(vec![]).await;
        assert!(matches!(s.run(&["/x"]).await, Err(Error::Io(_))));
    }
}
