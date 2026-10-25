//! End to end against a real RouterOS (a MikroTik CHR virtual machine), the way the
//! router is set up in production: API-SSL with a certificate signed by a private
//! CA, and a least-privilege `dns-switcher` account.
//!
//! Ignored by default; `e2e/run-chr.sh` boots a CHR and runs it. It needs:
//!   CHR_API_PORT  the CHR's plain API (8728), reachable on 127.0.0.1
//!   CHR_TLS_PORT  the CHR's API-SSL (8729), reachable on 127.0.0.1

use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use mikrotik_dns_switcher::app::{AppState, router};
use mikrotik_dns_switcher::client_ip::Trust;
use mikrotik_dns_switcher::range::AddressRange;
use mikrotik_dns_switcher::routeros::{Connection, Endpoint};
use mikrotik_dns_switcher::service::Settings;
use serde_json::Value;
use tokio::sync::Mutex;
use tower::ServiceExt;

const CLIENT: &str = "192.168.88.42";
const MAC: &str = "AA:BB:CC:DD:EE:42";
const COMMENT: &str = "DNS-Switcher-Managed";
const PASSWORD: &str = "e2e-pass-word";
// The group exactly as the production router defines it.
const POLICY: &str = "read,write,api,!local,!telnet,!ssh,!ftp,!reboot,!policy,!test,!winbox,!password,!web,!sniff,!sensitive,!romon,!rest-api";

fn port(var: &str) -> u16 {
    std::env::var(var)
        .unwrap_or_else(|_| panic!("{var} must be set (see e2e/run-chr.sh)"))
        .parse()
        .unwrap()
}

fn endpoint(user: &str, password: &str, port: u16, ca: Option<Vec<u8>>) -> Endpoint {
    Endpoint {
        host: "127.0.0.1".into(),
        username: user.into(),
        password: password.into(),
        tls_ca: ca,
        port: Some(port),
    }
}

/// The CHR takes a while to boot under emulation.
async fn admin(api_port: u16) -> Connection {
    let admin = endpoint("admin", "", api_port, None);
    for attempt in 1..=60 {
        match admin.connect().await {
            Ok(mut session) => {
                session.set_timeout(Duration::from_secs(120));
                return session;
            }
            Err(e) => {
                eprintln!("waiting for the router ({attempt}/60): {e}");
                tokio::time::sleep(Duration::from_secs(10)).await;
            }
        }
    }
    panic!("the router never came up");
}

async fn run(s: &mut Connection, words: &[&str]) -> Vec<std::collections::HashMap<String, String>> {
    match s.run(words).await {
        Ok(reply) => reply.rows,
        Err(e) => panic!("{words:?}: {e}"),
    }
}

/// What bootstrap.yml does on the real router: a CA, a certificate for the LAN
/// address signed by it, API-SSL switched on, the CA exported.
async fn provision(s: &mut Connection) -> Vec<u8> {
    // Already provisioned (re-running against a CHR that is still up).
    if !run(s, &["/certificate/print", "?name=router-ca"])
        .await
        .is_empty()
    {
        return export_ca(s).await;
    }
    run(
        s,
        &[
            "/certificate/add",
            "=name=router-ca",
            "=common-name=router-ca",
            "=key-usage=key-cert-sign,crl-sign",
            "=days-valid=3650",
        ],
    )
    .await;
    run(s, &["/certificate/sign", "=number=router-ca"]).await;
    run(
        s,
        &[
            "/certificate/add",
            "=name=api-ssl",
            "=common-name=127.0.0.1",
            "=subject-alt-name=IP:127.0.0.1",
            "=key-usage=digital-signature,key-encipherment,tls-server",
            "=days-valid=365",
        ],
    )
    .await;
    run(
        s,
        &["/certificate/sign", "=number=api-ssl", "=ca=router-ca"],
    )
    .await;
    run(
        s,
        &[
            "/ip/service/set",
            "=numbers=api-ssl",
            "=certificate=api-ssl",
            "=disabled=no",
        ],
    )
    .await;

    run(
        s,
        &[
            "/user/group/add",
            "=name=dns-switcher",
            &format!("=policy={POLICY}"),
        ],
    )
    .await;
    run(
        s,
        &[
            "/user/add",
            "=name=dns-switcher",
            "=group=dns-switcher",
            &format!("=password={PASSWORD}"),
        ],
    )
    .await;
    run(
        s,
        &[
            "/ip/arp/add",
            &format!("=address={CLIENT}"),
            &format!("=mac-address={MAC}"),
            "=interface=ether1",
        ],
    )
    .await;
    export_ca(s).await
}

async fn export_ca(s: &mut Connection) -> Vec<u8> {
    run(
        s,
        &[
            "/certificate/export-certificate",
            "=numbers=router-ca",
            "=file-name=router-ca",
        ],
    )
    .await;
    let file = run(
        s,
        &["/file/print", "?name=router-ca.crt", "=.proplist=contents"],
    )
    .await;
    let pem = file[0]["contents"].replace("\\n", "\n");
    assert!(pem.contains("BEGIN CERTIFICATE"), "exported CA: {pem:?}");
    pem.into_bytes()
}

async fn call(app: &axum::Router, method: &str) -> (StatusCode, Value) {
    let mut req = Request::builder()
        .method(method)
        .uri("/api/dns")
        .header("x-real-ip", CLIENT)
        .body(Body::empty())
        .unwrap();
    req.extensions_mut().insert(ConnectInfo(
        "172.19.0.100:50000"
            .parse::<std::net::SocketAddr>()
            .unwrap(),
    ));
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&body).unwrap())
}

fn status(body: &Value) -> &str {
    body["data"]["status"]
        .as_str()
        .unwrap_or_else(|| panic!("no status in {body}"))
}

#[tokio::test]
#[ignore = "needs a running CHR; see e2e/run-chr.sh"]
async fn switches_dns_on_a_real_routeros() {
    let mut admin = admin(port("CHR_API_PORT")).await;
    let ca = provision(&mut admin).await;
    // Say why, if the account cannot get in; the app only reports a generic 500.
    if let Err(e) = endpoint(
        "dns-switcher",
        PASSWORD,
        port("CHR_TLS_PORT"),
        Some(ca.clone()),
    )
    .connect()
    .await
    {
        panic!("dns-switcher cannot log in over API-SSL: {e}");
    }

    let app = router(Arc::new(AppState {
        endpoint: endpoint("dns-switcher", PASSWORD, port("CHR_TLS_PORT"), Some(ca)),
        settings: Settings {
            comment: COMMENT.into(),
            custom_dns: Ipv4Addr::new(192, 168, 88, 113),
        },
        trust: Trust {
            proxy: Some(Ipv4Addr::new(172, 19, 0, 100)),
            allow_target_ip: false,
        },
        allowed_range: Some(AddressRange::parse("192.168.88.20-192.168.88.99").unwrap()),
        router_lock: Mutex::new(()),
    }));

    let (code, body) = call(&app, "GET").await;
    assert_eq!((code, status(&body)), (StatusCode::OK, "DEFAULT"), "{body}");

    let (code, body) = call(&app, "PUT").await;
    assert_eq!((code, status(&body)), (StatusCode::OK, "CUSTOM"), "{body}");

    let leases = run(
        &mut admin,
        &["/ip/dhcp-server/lease/print", &format!("?address={CLIENT}")],
    )
    .await;
    eprintln!("lease on the router: {leases:?}");
    assert_eq!(leases.len(), 1);
    assert_eq!(leases[0]["comment"], COMMENT);
    assert_eq!(leases[0]["mac-address"], MAC);
    assert_eq!(leases[0]["dynamic"], "false");
    let options = run(
        &mut admin,
        &["/ip/dhcp-server/option/print", "?name=Custom DNS Server"],
    )
    .await;
    eprintln!("option on the router: {options:?}");
    assert_eq!(options[0]["code"], "6");
    assert_eq!(options[0]["value"], "0xc0a85871");

    let (code, body) = call(&app, "GET").await;
    assert_eq!((code, status(&body)), (StatusCode::OK, "CUSTOM"), "{body}");
    let (code, body) = call(&app, "PUT").await;
    assert_eq!((code, status(&body)), (StatusCode::OK, "CUSTOM"), "{body}");
    assert_eq!(
        run(
            &mut admin,
            &["/ip/dhcp-server/lease/print", &format!("?address={CLIENT}")]
        )
        .await
        .len(),
        1
    );

    let (code, body) = call(&app, "DELETE").await;
    assert_eq!((code, status(&body)), (StatusCode::OK, "DEFAULT"), "{body}");
    assert!(
        run(
            &mut admin,
            &["/ip/dhcp-server/lease/print", &format!("?address={CLIENT}")]
        )
        .await
        .is_empty()
    );

    // Someone else's static lease is left alone.
    run(
        &mut admin,
        &[
            "/ip/dhcp-server/lease/add",
            &format!("=address={CLIENT}"),
            &format!("=mac-address={MAC}"),
            "=comment=m1mbp",
        ],
    )
    .await;
    let (code, body) = call(&app, "GET").await;
    assert_eq!(
        (code, status(&body)),
        (StatusCode::OK, "UNMANAGED"),
        "{body}"
    );
    assert_eq!(call(&app, "PUT").await.0, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(
        call(&app, "DELETE").await.0,
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        run(
            &mut admin,
            &["/ip/dhcp-server/lease/print", &format!("?address={CLIENT}")]
        )
        .await
        .len(),
        1
    );
}
