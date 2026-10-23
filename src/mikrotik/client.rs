use crate::mikrotik::protocol::{decode_length, encode_sentence};
use encoding_rs::WINDOWS_1252;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct RouterosClient {
    stream: Arc<Mutex<TcpStream>>,
    tag_counter: Arc<Mutex<u64>>,
}

impl RouterosClient {
    pub async fn connect(host: &str, port: u16, user: &str, pass: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let stream = TcpStream::connect((host, port)).await?;
        let client = RouterosClient {
            stream: Arc::new(Mutex::new(stream)),
            tag_counter: Arc::new(Mutex::new(0)),
        };
        client.login(user, pass).await?;
        Ok(client)
    }

    async fn next_tag(&self) -> String {
        let mut c = self.tag_counter.lock().await;
        *c += 1;
        format!("t{}", *c)
    }

    async fn send_sentence(&self, sentence: &[Option<String>]) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut buf = Vec::new();
        for s in sentence {
            buf.extend_from_slice(&encode_sentence(s.as_deref()));
        }
        let mut stream = self.stream.lock().await;
        stream.write_all(&buf).await?;
        Ok(())
    }

    async fn read_response(&self, tag: &str) -> Result<Vec<HashMap<String, String>>, Box<dyn std::error::Error + Send + Sync>> {
        let mut results = Vec::new();
        let mut current: HashMap<String, String> = HashMap::new();
        loop {
            let mut stream = self.stream.lock().await;
            let mut read_buf = vec![0u8; 4096];
            let n = tokio::time::timeout(std::time::Duration::from_secs(10), stream.read(&mut read_buf)).await??;
            if n == 0 {
                break;
            }
            drop(stream);
            let mut data = read_buf[0..n].to_vec();
            let mut offset = 0;
            while offset < data.len() {
                if data[offset] == 0x00 {
                    offset += 1;
                    continue;
                }
                if let Some((len, len_bytes)) = decode_length(&data[offset..]) {
                    if offset + len_bytes + len > data.len() {
                        break;
                    }
                    let start = offset + len_bytes;
                    let slice = &data[start..start + len];
                    let (decoded, _, _) = WINDOWS_1252.decode(slice);
                    let s = decoded.into_owned();
                    if s == "!done" {
                        if current.get(".tag") == Some(&tag.to_string()) {
                            let mut c = current.clone();
                            c.remove(".tag");
                            if !c.is_empty() {
                                results.push(c);
                            }
                            return Ok(results);
                        } else if !current.is_empty() {
                            let mut c = current.clone();
                            c.remove(".tag");
                            if !c.is_empty() {
                                results.push(c);
                            }
                            current.clear();
                        } else {
                            return Ok(results);
                        }
                    } else if s == "!empty" {
                        return Ok(results);
                    } else if s == "!re" {
                        if !current.is_empty() {
                            let mut c = current.clone();
                            c.remove(".tag");
                            if !c.is_empty() {
                                results.push(c);
                            }
                            current.clear();
                        }
                    } else if s.starts_with(".tag=") {
                        current.insert(".tag".to_string(), s[5..].to_string());
                    } else if s.starts_with('=') {
                        if let Some(eq1) = s.find('=') {
                            let rest = &s[eq1 + 1..];
                            if let Some(eq2) = rest.find('=') {
                                let key = rest[0..eq2].to_string();
                                let value = rest[eq2 + 1..].to_string();
                                current.insert(key, value);
                            } else if let Some(eq2) = s[1..].find('=') {
                                let key = s[1..1+eq2].to_string();
                                let value = s[2+eq2..].to_string();
                                current.insert(key, value);
                            }
                        }
                    }
                    offset = start + len;
                } else {
                    break;
                }
            }
        }
        Ok(results)
    }

    async fn login(&self, user: &str, pass: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let tag = self.next_tag().await;
        self.send_sentence(&[Some("/login".to_string()), Some(format!("=name={}", user)), Some(format!("=password={}", pass)), Some(format!(".tag={}", tag)), None]).await?;
        let _ = self.read_response(&tag).await?;
        Ok(())
    }

    pub async fn write(&self, cmd: &[&str]) -> Result<Vec<HashMap<String, String>>, Box<dyn std::error::Error + Send + Sync>> {
        let tag = self.next_tag().await;
        let mut sentence: Vec<Option<String>> = Vec::new();
        for c in cmd {
            sentence.push(Some(c.to_string()));
        }
        sentence.push(Some(format!(".tag={}", tag)));
        sentence.push(None);
        self.send_sentence(&sentence).await?;
        self.read_response(&tag).await
    }

    pub async fn close(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut stream = self.stream.lock().await;
        let _ = stream.shutdown().await;
        Ok(())
    }
}
