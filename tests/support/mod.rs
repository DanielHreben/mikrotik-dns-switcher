//! An in-process stand-in for the router's API: just the commands the switcher
//! sends, over a plain TCP listener.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use mikrotik_dns_switcher::routeros::{read_sentence, write_sentence};
use tokio::net::{TcpListener, TcpStream};

pub type Row = HashMap<String, String>;

pub fn row(pairs: &[(&str, &str)]) -> Row {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

#[derive(Default)]
pub struct RouterState {
    pub leases: Vec<Row>,
    pub arp: Vec<Row>,
    pub options: Vec<Row>,
    pub next_id: u32,
    pub logins: u32,
    pub commands: Vec<String>,
}

pub struct MockRouter {
    pub port: u16,
    pub state: Arc<Mutex<RouterState>>,
}

fn w(s: &str) -> Vec<u8> {
    s.as_bytes().to_vec()
}

async fn reply(stream: &mut TcpStream, kind: &str, attrs: &Row) {
    let mut words = vec![w(kind)];
    let mut keys: Vec<_> = attrs.keys().collect();
    keys.sort();
    words.extend(keys.into_iter().map(|k| w(&format!("={k}={}", attrs[k]))));
    write_sentence(stream, &words).await.unwrap();
}

fn query(words: &[String]) -> Vec<(String, String)> {
    words
        .iter()
        .filter_map(|x| x.strip_prefix('?')?.split_once('='))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

fn attrs(words: &[String]) -> Row {
    words
        .iter()
        .filter_map(|x| x.strip_prefix('=')?.split_once('='))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

async fn handle(mut stream: TcpStream, state: Arc<Mutex<RouterState>>) {
    let mut logged_in = false;
    while let Ok(sentence) = read_sentence(&mut stream).await {
        let words: Vec<String> = sentence
            .iter()
            .map(|x| String::from_utf8_lossy(x).into_owned())
            .collect();
        let Some(command) = words.first().cloned() else {
            continue;
        };
        if command == "/login" {
            let a = attrs(&words);
            if a.get("name").map(String::as_str) == Some("switcher")
                && a.get("password").map(String::as_str) == Some("secret")
            {
                logged_in = true;
                state.lock().unwrap().logins += 1;
                reply(&mut stream, "!done", &Row::new()).await;
            } else {
                reply(
                    &mut stream,
                    "!trap",
                    &row(&[("message", "invalid user name or password (6)")]),
                )
                .await;
                reply(&mut stream, "!done", &Row::new()).await;
            }
            continue;
        }
        if !logged_in {
            write_sentence(&mut stream, &[w("!fatal"), w("not logged in")])
                .await
                .unwrap();
            return;
        }

        let outcome = run(&state, &command, &words);
        match outcome {
            Ok((rows, ret)) => {
                for r in &rows {
                    reply(&mut stream, "!re", r).await;
                }
                reply(&mut stream, "!done", &ret).await;
            }
            Err(message) => {
                reply(&mut stream, "!trap", &row(&[("message", &message)])).await;
                reply(&mut stream, "!done", &Row::new()).await;
            }
        }
    }
}

/// Runs one command against the state; no awaiting while the lock is held.
fn run(
    state: &Mutex<RouterState>,
    command: &str,
    words: &[String],
) -> Result<(Vec<Row>, Row), String> {
    let mut st = state.lock().unwrap();
    st.commands.push(words.join(" "));
    let q = query(words);
    let matches = |r: &Row| q.iter().all(|(k, v)| r.get(k) == Some(v));
    let mut ret = Row::new();
    let rows = match command {
        "/ip/dhcp-server/lease/print" => st.leases.iter().filter(|r| matches(r)).cloned().collect(),
        "/ip/arp/print" => st.arp.iter().filter(|r| matches(r)).cloned().collect(),
        "/ip/dhcp-server/option/print" => {
            st.options.iter().filter(|r| matches(r)).cloned().collect()
        }
        "/ip/dhcp-server/option/add" | "/ip/dhcp-server/lease/add" => {
            st.next_id += 1;
            let id = format!("*{:X}", st.next_id);
            let mut new = attrs(words);
            new.insert(".id".into(), id.clone());
            new.insert("dynamic".into(), "false".into());
            if command.contains("lease") {
                st.leases.push(new);
            } else {
                st.options.push(new);
            }
            ret.insert("ret".into(), id);
            vec![]
        }
        "/ip/dhcp-server/lease/remove" => {
            let id = attrs(words).get(".id").cloned().unwrap_or_default();
            let before = st.leases.len();
            st.leases.retain(|l| l.get(".id") != Some(&id));
            if st.leases.len() == before {
                return Err("no such item (4)".into());
            }
            vec![]
        }
        _ => return Err("no such command".into()),
    };
    Ok((rows, ret))
}

impl MockRouter {
    pub async fn start(state: RouterState) -> MockRouter {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let state = Arc::new(Mutex::new(state));
        let shared = state.clone();
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                tokio::spawn(handle(stream, shared.clone()));
            }
        });
        MockRouter { port, state }
    }
}
