mod support;

use std::net::Ipv4Addr;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use mikrotik_dns_switcher::app::{AppState, router};
use mikrotik_dns_switcher::client_ip::Trust;
use mikrotik_dns_switcher::range::AddressRange;
use mikrotik_dns_switcher::routeros::Endpoint;
use mikrotik_dns_switcher::service::Settings;
use serde_json::{Value, json};
use support::{MockRouter, RouterState, row};
use tokio::sync::Mutex;
use tower::ServiceExt;

const COMMENT: &str = "DNS-Switcher-Managed";
const CLIENT: &str = "192.168.88.42";
const MAC: &str = "AA:BB:CC:DD:EE:42";

fn app(mock: &MockRouter, password: &str) -> axum::Router {
    configured(mock, password, |_| {}).0
}

/// The app, with a chance to change its settings first.
fn configured(
    mock: &MockRouter,
    password: &str,
    change: impl FnOnce(&mut AppState),
) -> (axum::Router, Arc<AppState>) {
    let mut state = AppState {
        endpoint: Endpoint {
            host: "127.0.0.1".into(),
            username: "switcher".into(),
            password: password.into(),
            tls_ca: None,
            port: Some(mock.port),
        },
        settings: Settings {
            comment: COMMENT.into(),
            custom_dns: Ipv4Addr::new(192, 168, 88, 113),
            max_managed_leases: 32,
        },
        trust: Trust {
            proxy: Some(Ipv4Addr::new(172, 19, 0, 100)),
            allow_target_ip: false,
        },
        allowed_range: Some(AddressRange::parse("192.168.88.20-192.168.88.99").unwrap()),
        allowed_hosts: Vec::new(),
        lock_wait: std::time::Duration::from_secs(5),
        router_lock: Mutex::new(()),
    };
    change(&mut state);
    let state = Arc::new(state);
    (router(state.clone()), state)
}

async fn call(
    app: &axum::Router,
    method: &str,
    path: &str,
    real_ip: Option<&str>,
    from: &str,
) -> (StatusCode, Value, Vec<u8>) {
    let mut req = Request::builder().method(method).uri(path);
    if let Some(ip) = real_ip {
        req = req.header("x-real-ip", ip);
    }
    let mut req = req.body(Body::empty()).unwrap();
    req.extensions_mut().insert(ConnectInfo(
        format!("{from}:50000")
            .parse::<std::net::SocketAddr>()
            .unwrap(),
    ));
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes().to_vec();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        bytes,
    )
}

/// A request that reached the switcher through the trusted proxy.
async fn via_proxy(app: &axum::Router, method: &str) -> (StatusCode, Value) {
    let (s, v, _) = call(app, method, "/api/dns", Some(CLIENT), "172.19.0.100").await;
    (s, v)
}

fn router_with_arp() -> RouterState {
    RouterState {
        arp: vec![row(&[("address", CLIENT), ("mac-address", MAC)])],
        ..Default::default()
    }
}

fn ok(status: &str) -> Value {
    json!({ "ok": true, "data": { "ip": CLIENT, "status": status } })
}

#[tokio::test]
async fn a_new_device_is_switched_to_custom_dns_and_back() {
    let mock = MockRouter::start(router_with_arp()).await;
    let app = app(&mock, "secret");

    assert_eq!(
        via_proxy(&app, "GET").await,
        (StatusCode::OK, ok("DEFAULT"))
    );
    assert_eq!(via_proxy(&app, "PUT").await, (StatusCode::OK, ok("CUSTOM")));
    assert_eq!(via_proxy(&app, "GET").await, (StatusCode::OK, ok("CUSTOM")));

    {
        let st = mock.state.lock().unwrap();
        assert_eq!(st.options.len(), 1);
        assert_eq!(st.options[0]["name"], "Custom DNS Server");
        assert_eq!(st.options[0]["code"], "6");
        assert_eq!(st.options[0]["value"], "0xc0a85871");
        assert_eq!(st.leases.len(), 1);
        assert_eq!(st.leases[0]["address"], CLIENT);
        assert_eq!(st.leases[0]["mac-address"], MAC);
        assert_eq!(st.leases[0]["comment"], COMMENT);
        assert_eq!(st.leases[0]["dhcp-option"], st.options[0][".id"]);
    }

    // Enabling twice changes nothing; the option is reused.
    assert_eq!(via_proxy(&app, "PUT").await, (StatusCode::OK, ok("CUSTOM")));
    assert_eq!(mock.state.lock().unwrap().leases.len(), 1);

    assert_eq!(
        via_proxy(&app, "DELETE").await,
        (StatusCode::OK, ok("DEFAULT"))
    );
    assert!(mock.state.lock().unwrap().leases.is_empty());
    assert_eq!(
        via_proxy(&app, "DELETE").await,
        (StatusCode::OK, ok("DEFAULT"))
    );
}

#[tokio::test]
async fn a_dynamic_lease_is_replaced_by_a_static_one() {
    let mut state = router_with_arp();
    state.leases.push(row(&[
        (".id", "*D"),
        ("address", CLIENT),
        ("mac-address", MAC),
        ("dynamic", "true"),
    ]));
    let mock = MockRouter::start(state).await;
    let app = app(&mock, "secret");

    assert_eq!(
        via_proxy(&app, "GET").await,
        (StatusCode::OK, ok("DEFAULT"))
    );
    assert_eq!(via_proxy(&app, "PUT").await, (StatusCode::OK, ok("CUSTOM")));
    let st = mock.state.lock().unwrap();
    assert_eq!(st.leases.len(), 1);
    assert_eq!(st.leases[0]["dynamic"], "false");
}

#[tokio::test]
async fn disabling_a_dynamic_lease_leaves_it_alone() {
    let mut state = router_with_arp();
    state.leases.push(row(&[
        (".id", "*D"),
        ("address", CLIENT),
        ("mac-address", MAC),
        ("dynamic", "true"),
    ]));
    let mock = MockRouter::start(state).await;
    assert_eq!(
        via_proxy(&app(&mock, "secret"), "DELETE").await,
        (StatusCode::OK, ok("DEFAULT"))
    );
    assert_eq!(mock.state.lock().unwrap().leases.len(), 1);
}

#[tokio::test]
async fn someone_elses_static_lease_is_reported_and_never_touched() {
    let mut state = router_with_arp();
    state.leases.push(row(&[
        (".id", "*7"),
        ("address", CLIENT),
        ("dynamic", "false"),
        ("comment", "m1mbp"),
    ]));
    let mock = MockRouter::start(state).await;
    let app = app(&mock, "secret");

    assert_eq!(
        via_proxy(&app, "GET").await,
        (StatusCode::OK, ok("UNMANAGED"))
    );
    for method in ["PUT", "DELETE"] {
        let (status, body) = via_proxy(&app, method).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{method}");
        assert_eq!(body["error"]["code"], "UNEXPECTED_ERROR");
    }
    assert_eq!(mock.state.lock().unwrap().leases.len(), 1);
}

#[tokio::test]
async fn enabling_needs_an_arp_entry() {
    let mock = MockRouter::start(RouterState::default()).await;
    let (status, body) = via_proxy(&app(&mock, "secret"), "PUT").await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(
        body["error"],
        json!({ "code": "UNEXPECTED_ERROR", "message": "Something went wrong" })
    );
    assert!(mock.state.lock().unwrap().leases.is_empty());
}

#[tokio::test]
async fn addresses_outside_the_range_never_reach_the_router() {
    let mock = MockRouter::start(router_with_arp()).await;
    let app = app(&mock, "secret");
    for ip in [
        "192.168.88.19",
        "192.168.88.100",
        "10.0.0.1",
        "::1",
        "garbage",
    ] {
        let (status, body, _) = call(&app, "PUT", "/api/dns", Some(ip), "172.19.0.100").await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{ip}");
        assert_eq!(body["error"]["code"], "ADDRESS_NOT_ALLOWED");
    }
    assert_eq!(mock.state.lock().unwrap().logins, 0);
}

#[tokio::test]
async fn the_forwarded_header_counts_only_from_the_trusted_proxy() {
    let mock = MockRouter::start(router_with_arp()).await;
    let app = app(&mock, "secret");
    // Another host claiming to be CLIENT: it is judged by its own address.
    let (status, body, _) = call(&app, "PUT", "/api/dns", Some(CLIENT), "192.168.88.200").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "ADDRESS_NOT_ALLOWED");
    // The same host asking for itself is fine.
    let (status, body, _) = call(&app, "GET", "/api/dns", None, CLIENT).await;
    assert_eq!((status, body), (StatusCode::OK, ok("DEFAULT")));
}

#[tokio::test]
async fn router_trouble_is_a_generic_500() {
    let mock = MockRouter::start(router_with_arp()).await;
    let (status, body) = via_proxy(&app(&mock, "wrong"), "GET").await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["error"]["code"], "UNEXPECTED_ERROR");
    assert!(!body.to_string().contains("password"));

    // Nothing listening at all.
    let dead = MockRouter {
        port: 1,
        state: Default::default(),
    };
    let (status, _) = via_proxy(&app(&dead, "secret"), "GET").await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn health_and_front_end() {
    let mock = MockRouter::start(RouterState::default()).await;
    let app = app(&mock, "secret");

    let (status, body, _) = call(&app, "GET", "/health", None, "127.0.0.1").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["ok"], true);
    assert_eq!(
        mock.state.lock().unwrap().logins,
        0,
        "health must not touch the router"
    );

    for (path, needle) in [
        ("/", "<html"),
        ("/index.html", "<html"),
        ("/app.js", "DNS_STATUS"),
        ("/styles.css", "{"),
    ] {
        let (status, _, bytes) = call(&app, "GET", path, None, "127.0.0.1").await;
        assert_eq!(status, StatusCode::OK, "{path}");
        assert!(String::from_utf8_lossy(&bytes).contains(needle), "{path}");
    }
    let (status, ..) = call(&app, "GET", "/nope", None, "127.0.0.1").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, ..) = call(&app, "POST", "/api/dns", None, "127.0.0.1").await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
}

fn dynamic_lease() -> support::Row {
    row(&[
        (".id", "*D"),
        ("address", CLIENT),
        ("mac-address", MAC),
        ("dynamic", "true"),
    ])
}

async fn expect_error(app: &axum::Router, status: StatusCode, code: &str) {
    let (s, body) = via_proxy(app, "PUT").await;
    assert_eq!(
        (s, body["error"]["code"].as_str()),
        (status, Some(code)),
        "{body}"
    );
}

#[tokio::test]
async fn a_poisoned_arp_entry_cannot_bind_another_devices_mac() {
    let mut state = router_with_arp();
    // The address was handed out to a different MAC than the ARP table now claims.
    let mut lease = dynamic_lease();
    lease.insert("mac-address".into(), "11:22:33:44:55:66".into());
    state.leases.push(lease);
    let mock = MockRouter::start(state).await;

    expect_error(
        &app(&mock, "secret"),
        StatusCode::INTERNAL_SERVER_ERROR,
        "UNEXPECTED_ERROR",
    )
    .await;
    let st = mock.state.lock().unwrap();
    assert_eq!(st.leases.len(), 1, "the dynamic lease is left in place");
    assert_eq!(st.leases[0]["dynamic"], "true");
    assert!(st.options.is_empty());
}

#[tokio::test]
async fn a_failed_mac_lookup_does_not_cost_the_device_its_lease() {
    let mock = MockRouter::start(RouterState {
        leases: vec![dynamic_lease()],
        ..Default::default()
    })
    .await;
    expect_error(
        &app(&mock, "secret"),
        StatusCode::INTERNAL_SERVER_ERROR,
        "UNEXPECTED_ERROR",
    )
    .await;
    assert_eq!(mock.state.lock().unwrap().leases.len(), 1);
}

#[tokio::test]
async fn the_number_of_managed_leases_is_limited() {
    let mut state = router_with_arp();
    state.leases.push(row(&[
        (".id", "*9"),
        ("address", "192.168.88.50"),
        ("dynamic", "false"),
        ("comment", COMMENT),
    ]));
    let mock = MockRouter::start(state).await;
    let (app, _) = configured(&mock, "secret", |s| s.settings.max_managed_leases = 1);

    expect_error(&app, StatusCode::TOO_MANY_REQUESTS, "LIMIT_REACHED").await;
    assert_eq!(mock.state.lock().unwrap().leases.len(), 1);
    // A device that already has custom DNS is not affected by the limit.
    let (status, _, _) = call(&app, "GET", "/api/dns", None, "192.168.88.50").await;
    assert_eq!(status, StatusCode::OK);
    // Leases that are not ours do not count.
    let (app, _) = configured(&mock, "secret", |s| s.settings.comment = "other-app".into());
    let (status, _) = via_proxy(&app, "PUT").await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn an_old_dns_address_in_our_option_is_corrected() {
    let mut state = router_with_arp();
    state.options.push(row(&[
        (".id", "*1"),
        ("name", "Custom DNS Server"),
        ("code", "6"),
        ("value", "0x08080808"),
        ("comment", COMMENT),
    ]));
    let mock = MockRouter::start(state).await;
    assert_eq!(
        via_proxy(&app(&mock, "secret"), "PUT").await,
        (StatusCode::OK, ok("CUSTOM"))
    );
    let st = mock.state.lock().unwrap();
    assert_eq!(st.options.len(), 1);
    assert_eq!(st.options[0]["value"], "0xc0a85871");
    assert_eq!(st.leases[0]["dhcp-option"], "*1");
}

#[tokio::test]
async fn an_option_of_that_name_that_is_not_ours_is_left_alone() {
    let mut state = router_with_arp();
    state.options.push(row(&[
        (".id", "*1"),
        ("name", "Custom DNS Server"),
        ("code", "6"),
        ("value", "0x08080808"),
        ("comment", "something else"),
    ]));
    let mock = MockRouter::start(state).await;
    expect_error(
        &app(&mock, "secret"),
        StatusCode::INTERNAL_SERVER_ERROR,
        "UNEXPECTED_ERROR",
    )
    .await;
    let st = mock.state.lock().unwrap();
    assert_eq!(st.options[0]["value"], "0x08080808");
    assert!(st.leases.is_empty());
}

#[tokio::test]
async fn a_busy_router_gives_a_503_instead_of_a_long_wait() {
    let mock = MockRouter::start(router_with_arp()).await;
    let (app, state) = configured(&mock, "secret", |s| {
        s.lock_wait = std::time::Duration::from_millis(100)
    });
    let _held = state.router_lock.lock().await;
    let (status, body) = via_proxy(&app, "GET").await;
    assert_eq!(
        (status, body["error"]["code"].as_str()),
        (StatusCode::SERVICE_UNAVAILABLE, Some("BUSY"))
    );
    assert_eq!(mock.state.lock().unwrap().logins, 0);
}

async fn with_host(
    app: &axum::Router,
    path: &str,
    host: Option<&str>,
) -> axum::http::Response<Body> {
    let mut req = Request::builder().uri(path).header("x-real-ip", CLIENT);
    if let Some(host) = host {
        req = req.header("host", host);
    }
    let mut req = req.body(Body::empty()).unwrap();
    req.extensions_mut().insert(ConnectInfo(
        "172.19.0.100:50000"
            .parse::<std::net::SocketAddr>()
            .unwrap(),
    ));
    app.clone().oneshot(req).await.unwrap()
}

#[tokio::test]
async fn only_the_configured_host_names_are_served() {
    let mock = MockRouter::start(router_with_arp()).await;
    let (app, _) = configured(&mock, "secret", |s| {
        s.allowed_hosts = vec!["switcher.home".into()]
    });
    for host in ["switcher.home", "Switcher.Home:3000"] {
        for path in ["/", "/api/dns"] {
            let res = with_host(&app, path, Some(host)).await;
            assert_eq!(res.status(), StatusCode::OK, "{host} {path}");
        }
    }
    // A name an attacker's page rebinds to the service's address.
    for host in [
        Some("evil.example"),
        Some("192.168.88.100:3000"),
        Some("[::1]:3000"),
        None,
    ] {
        for path in ["/", "/api/dns"] {
            let res = with_host(&app, path, host).await;
            assert_eq!(res.status(), StatusCode::FORBIDDEN, "{host:?} {path}");
        }
    }
    assert_eq!(
        mock.state.lock().unwrap().logins,
        2,
        "only the allowed API calls logged in"
    );
    // The container health check asks for localhost.
    assert_eq!(
        with_host(&app, "/health", Some("localhost:3000"))
            .await
            .status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn responses_forbid_framing_and_caching_of_answers() {
    let mock = MockRouter::start(router_with_arp()).await;
    let app = app(&mock, "secret");
    for path in ["/", "/app.js", "/nope", "/health", "/api/dns"] {
        let res = with_host(&app, path, Some("x")).await;
        let h = res.headers();
        assert_eq!(h["x-frame-options"], "DENY", "{path}");
        assert_eq!(h["x-content-type-options"], "nosniff", "{path}");
        assert!(
            h["content-security-policy"]
                .to_str()
                .unwrap()
                .contains("frame-ancestors 'none'"),
            "{path}"
        );
    }
    let res = with_host(&app, "/api/dns", Some("x")).await;
    assert_eq!(res.headers()["cache-control"], "no-store");
}
