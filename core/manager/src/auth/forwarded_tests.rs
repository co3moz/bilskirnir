//! Pins down whose `X-Forwarded-For` a sign-in believes: only the HTTPS proxy's, read from Docker
//! as the running `https` service of the manager's own Compose project, and then only the address
//! the proxy itself added.
use super::*;
use axum::http::HeaderValue;
use serde_json::json;

const CADDY: [u8; 4] = [172, 18, 0, 3];

fn proxy() -> HttpsProxy {
    let proxy = HttpsProxy::default();
    assert!(proxy.set(vec![IpAddr::from(CADDY)]));
    proxy
}

fn headers(values: &[&str]) -> HeaderMap {
    let mut headers = HeaderMap::new();
    for value in values {
        headers.append("x-forwarded-for", HeaderValue::from_str(value).unwrap());
    }
    headers
}

fn ip(text: &str) -> Option<IpAddr> {
    Some(text.parse().unwrap())
}

#[test]
fn the_proxy_names_the_client() {
    let caddy = Some(IpAddr::from(CADDY));
    let proxy = proxy();
    assert_eq!(
        client(caddy, &headers(&["192.168.1.20"]), &proxy),
        ip("192.168.1.20")
    );
    // A browser may send a header of its own; the proxy's entry is the last one.
    assert_eq!(
        client(caddy, &headers(&["10.9.9.9, 192.168.1.20"]), &proxy),
        ip("192.168.1.20")
    );
    assert_eq!(
        client(caddy, &headers(&["10.9.9.9", "fd00::20"]), &proxy),
        ip("fd00::20")
    );
    assert_eq!(
        client(caddy, &headers(&["192.168.1.20:51000"]), &proxy),
        ip("192.168.1.20")
    );
    assert_eq!(
        client(caddy, &headers(&["::ffff:192.168.1.20"]), &proxy),
        ip("192.168.1.20")
    );
    // An IPv4 connection on an IPv6 socket is the same proxy.
    assert_eq!(
        client(ip("::ffff:172.18.0.3"), &headers(&["192.168.1.20"]), &proxy),
        ip("192.168.1.20")
    );
    // Without a readable entry the proxy's own address counts.
    assert_eq!(client(caddy, &headers(&[]), &proxy), caddy);
    assert_eq!(client(caddy, &headers(&["unknown"]), &proxy), caddy);
}

#[test]
fn nobody_else_names_a_client() {
    let proxy = proxy();
    for peer in ["172.19.0.2", "192.168.1.30", "172.18.0.1", "127.0.0.1"] {
        assert_eq!(
            client(ip(peer), &headers(&["192.168.1.20"]), &proxy),
            ip(peer)
        );
    }
    assert_eq!(client(None, &headers(&["192.168.1.20"]), &proxy), None);
    assert_eq!(
        client(
            ip("172.18.0.3"),
            &headers(&["192.168.1.20"]),
            &HttpsProxy::default()
        ),
        ip("172.18.0.3")
    );
}

#[test]
fn a_change_of_address_is_noticed() {
    let proxy = proxy();
    assert!(!proxy.set(vec![IpAddr::from(CADDY), IpAddr::from(CADDY)]));
    assert!(proxy.set(vec![IpAddr::from([172, 18, 0, 4])]));
    assert!(!proxy.contains(IpAddr::from(CADDY)));
    assert!(proxy.set(Vec::new()));
}

#[test]
fn a_container_has_an_address_on_each_network() {
    let info = json!({"NetworkSettings": {"Networks": {
        "thoric_management": {"IPAddress": "172.18.0.3", "GlobalIPv6Address": "fd00:18::3"},
        "other": {"IPAddress": "", "GlobalIPv6Address": ""},
    }}});
    let mut found = addresses(&info);
    found.sort();
    assert_eq!(
        found,
        vec![IpAddr::from(CADDY), "fd00:18::3".parse::<IpAddr>().unwrap()]
    );
    assert!(addresses(&json!({})).is_empty());
}

/// A manager whose Docker CLI is a shell script that answers for the manager container of
/// Compose project `thoric` and for the running `https` service `listed` names.
#[cfg(unix)]
fn fake_docker(listed: &str) -> (Arc<App>, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "thoric-forwarded-{}",
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("manager.json"),
        json!([{"Id": "manager", "Config": {"Labels": {PROJECT_LABEL: "thoric"}}}]).to_string(),
    )
    .unwrap();
    std::fs::write(
        dir.join("caddy.json"),
        json!([{"Id": "caddy", "NetworkSettings": {"Networks": {
            "thoric_management": {"IPAddress": "172.18.0.3", "GlobalIPv6Address": ""},
        }}}])
        .to_string(),
    )
    .unwrap();
    let script = format!(
        "#!/bin/sh\ncd '{}'\ncase \"$*\" in\n  'container inspect thoric-manager') cat manager.json ;;\n  'ps --quiet --no-trunc --filter label=com.docker.compose.project=thoric --filter label=com.docker.compose.service=https') printf '{listed}' ;;\n  'container inspect caddy') cat caddy.json ;;\n  *) exit 1 ;;\nesac\n",
        dir.display()
    );
    let docker = dir.join("docker");
    crate::test_support::write_script(&docker, &script);
    let mut app = crate::test_support::app(true);
    let config = &mut Arc::get_mut(&mut app).unwrap().config;
    config.docker_bin = docker.display().to_string();
    config.manager = Some("thoric-manager".into());
    (app, dir)
}

#[cfg(unix)]
#[tokio::test]
async fn the_proxy_is_the_https_service_of_the_managers_project() {
    let (app, dir) = fake_docker("caddy\\n");
    spawn(app.clone());
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while !app.https_proxy.contains(IpAddr::from(CADDY)) {
        assert!(
            std::time::Instant::now() < deadline,
            "the proxy was not read"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(
        https_proxy(&app, "thoric").await.unwrap(),
        vec![IpAddr::from(CADDY)]
    );
    std::fs::remove_dir_all(dir).unwrap();

    // Without the https profile nothing is trusted.
    let (app, dir) = fake_docker("");
    assert!(https_proxy(&app, "thoric").await.unwrap().is_empty());
    std::fs::remove_dir_all(dir).unwrap();
}
