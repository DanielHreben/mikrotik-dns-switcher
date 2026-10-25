//! API-SSL the way the real router has it: a private CA, and a certificate for the
//! router's LAN address (IP SAN) signed by it.

use std::net::IpAddr;
use std::sync::Arc;

use mikrotik_dns_switcher::routeros::{Endpoint, Error, read_sentence, write_sentence};
use rcgen::{BasicConstraints, CertificateParams, DnType, IsCa, KeyPair, KeyUsagePurpose, SanType};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

struct Pki {
    ca_pem: String,
    leaf: CertificateDer<'static>,
    leaf_key: Vec<u8>,
}

fn pki(leaf_ip: &str) -> Pki {
    let mut ca_params = CertificateParams::default();
    ca_params
        .distinguished_name
        .push(DnType::CommonName, "router-ca");
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca_params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    let ca_key = KeyPair::generate().unwrap();
    let ca = ca_params.self_signed(&ca_key).unwrap();

    let mut leaf_params = CertificateParams::default();
    leaf_params
        .distinguished_name
        .push(DnType::CommonName, leaf_ip);
    leaf_params.subject_alt_names = vec![SanType::IpAddress(leaf_ip.parse::<IpAddr>().unwrap())];
    let leaf_key = KeyPair::generate().unwrap();
    let leaf = leaf_params.signed_by(&leaf_key, &ca, &ca_key).unwrap();

    Pki {
        ca_pem: ca.pem(),
        leaf: leaf.der().clone(),
        leaf_key: leaf_key.serialize_der(),
    }
}

/// A TLS server that accepts the login and answers one `/ip/arp/print`.
async fn serve(pki: &Pki) -> u16 {
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(pki.leaf_key.clone()));
    let config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_no_client_auth()
    .with_single_cert(vec![pki.leaf.clone()], key)
    .unwrap();
    let acceptor = TlsAcceptor::from(Arc::new(config));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((tcp, _)) = listener.accept().await {
            let acceptor = acceptor.clone();
            tokio::spawn(async move {
                let Ok(mut tls) = acceptor.accept(tcp).await else {
                    return;
                };
                while let Ok(sentence) = read_sentence(&mut tls).await {
                    let first = String::from_utf8_lossy(&sentence[0]).into_owned();
                    let words: Vec<Vec<u8>> = if first == "/login" {
                        vec![b"!done".to_vec()]
                    } else {
                        vec![b"!re".to_vec(), b"=mac-address=AA:BB:CC:DD:EE:FF".to_vec()]
                    };
                    write_sentence(&mut tls, &words).await.unwrap();
                    if first != "/login" {
                        write_sentence(&mut tls, &[b"!done".to_vec()])
                            .await
                            .unwrap();
                    }
                }
            });
        }
    });
    port
}

fn endpoint(ca: &str, port: u16) -> Endpoint {
    Endpoint {
        host: "127.0.0.1".into(),
        username: "u".into(),
        password: "p".into(),
        tls_ca: Some(ca.as_bytes().to_vec()),
        port: Some(port),
    }
}

#[tokio::test]
async fn connects_when_the_certificate_is_signed_by_the_configured_ca() {
    let pki = pki("127.0.0.1");
    let port = serve(&pki).await;
    let mut session = endpoint(&pki.ca_pem, port).connect().await.unwrap();
    let reply = session.run(&["/ip/arp/print"]).await.unwrap();
    assert_eq!(reply.rows[0]["mac-address"], "AA:BB:CC:DD:EE:FF");
}

#[tokio::test]
async fn refuses_a_certificate_from_another_ca() {
    let router = pki("127.0.0.1");
    let stranger = pki("127.0.0.1");
    let port = serve(&router).await;
    let result = endpoint(&stranger.ca_pem, port).connect().await;
    assert!(matches!(result, Err(Error::Io(_))), "{:?}", result.err());
}

#[tokio::test]
async fn refuses_a_certificate_for_another_address() {
    let pki = pki("192.168.88.1");
    let port = serve(&pki).await;
    let result = endpoint(&pki.ca_pem, port).connect().await;
    assert!(matches!(result, Err(Error::Io(_))), "{:?}", result.err());
}

#[test]
fn startup_check_rejects_a_ca_file_without_a_certificate() {
    assert!(endpoint("garbage", 1).check().is_err());
    assert!(endpoint(&pki("127.0.0.1").ca_pem, 1).check().is_ok());
}
