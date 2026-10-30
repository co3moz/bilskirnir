//! Pins down that a connection reaching the manager on its address in a desktop's private network
//! is refused before any route sees it, whatever the request, while loopback and the networks
//! Thoric did not make are answered; that an address Docker does not name for the manager is
//! refused as well; and that Docker is asked once for what it already named, and at most once a
//! second for an address it did not.
use super::*;
use serde_json::json;

const MANAGEMENT: &str = "172.18.0.2";
const PRIVATE: &str = "172.20.0.3";
const PRIVATE_V6: &str = "fd00:20::3";

fn manager(networks: Value) -> Value {
    json!({"Id": "manager", "NetworkSettings": {"Networks": networks}})
}

fn ip(address: &str) -> IpAddr {
    address.parse().unwrap()
}

#[test]
fn only_the_networks_thoric_made_are_refused() {
    let container = manager(json!({
        "thoric_management": {"NetworkID": "management", "IPAddress": MANAGEMENT, "GlobalIPv6Address": ""},
        "thoric-1-private": {"NetworkID": "private", "IPAddress": PRIVATE, "GlobalIPv6Address": PRIVATE_V6},
        "none-yet": {"NetworkID": "other", "IPAddress": ""},
    }));
    assert_eq!(
        addresses(&container, &HashSet::from(["private"])),
        HashMap::from([
            (ip(MANAGEMENT), true),
            (ip(PRIVATE), false),
            (ip(PRIVATE_V6), false),
        ])
    );
    // An address on two networks, which Docker should never give, is refused for the desktop's.
    let shared = manager(json!({
        "a": {"NetworkID": "management", "IPAddress": MANAGEMENT},
        "b": {"NetworkID": "private", "IPAddress": MANAGEMENT},
    }));
    assert_eq!(
        addresses(&shared, &HashSet::from(["private"])),
        HashMap::from([(ip(MANAGEMENT), false)])
    );
    assert!(addresses(&json!({}), &HashSet::new()).is_empty());
}

#[tokio::test]
async fn a_manager_outside_a_container_answers_everyone() {
    let app = crate::test_support::app(false);
    assert!(app.config.manager.is_none());
    assert!(app.interfaces.answers(&app, ip(PRIVATE)).await.unwrap());
}

#[cfg(unix)]
mod with_docker {
    use super::*;
    use crate::{App, auth::Connection, test_support::write_script};
    use axum::{
        body::Body,
        extract::ConnectInfo,
        http::{Request, StatusCode},
    };
    use std::{net::SocketAddr, path::PathBuf, sync::Arc};
    use tower::ServiceExt;

    /// A Docker that names the manager's networks from `manager.json` and the ones Thoric made
    /// from `made`, both in the returned folder, and records its arguments in `calls`.
    fn docker(app: &mut Arc<App>, networks: Value, made: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "thoric-interfaces-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("manager.json"),
            json!([manager(networks)]).to_string(),
        )
        .unwrap();
        std::fs::write(dir.join("made"), made).unwrap();
        write_script(
            &dir.join("docker"),
            &format!(
                "#!/bin/sh\ncd '{dir}'\necho \"$*\" >> calls\ncase \"$*\" in\n  'container inspect thoric-manager') cat manager.json ;;\n  'network ls --quiet --no-trunc --filter label=thoric.managed=true') cat made ;;\n  *) exit 1 ;;\nesac\n",
                dir = dir.display()
            ),
        );
        let config = &mut Arc::get_mut(app).unwrap().config;
        config.docker_bin = dir.join("docker").display().to_string();
        config.manager = Some("thoric-manager".into());
        dir
    }

    fn calls(dir: &std::path::Path) -> usize {
        std::fs::read_to_string(dir.join("calls"))
            .unwrap_or_default()
            .lines()
            .count()
    }

    fn networks() -> Value {
        json!({
            "thoric_management": {"NetworkID": "management", "IPAddress": MANAGEMENT},
            "thoric-1-private": {"NetworkID": "private", "IPAddress": PRIVATE},
        })
    }

    async fn status(app: &Arc<App>, local: &str, path: &str) -> StatusCode {
        let mut request = Request::builder()
            .uri(path)
            .header("host", "localhost:8080")
            .body(Body::empty())
            .unwrap();
        request.extensions_mut().insert(ConnectInfo(Connection {
            peer: "172.20.0.4:40000".parse::<SocketAddr>().unwrap(),
            local: Some(SocketAddr::new(ip(local), 8080)),
        }));
        crate::router(app.clone())
            .oneshot(request)
            .await
            .unwrap()
            .status()
    }

    #[tokio::test]
    async fn a_desktop_reaches_no_route_on_its_private_network() {
        let mut app = crate::test_support::app(false);
        let dir = docker(&mut app, networks(), "private\nunrelated\n");
        // Sign-in is off: on any other network these would be answered.
        for path in ["/healthz", "/api/containers", "/api/session", "/mcp", "/"] {
            assert_eq!(
                status(&app, PRIVATE, path).await,
                StatusCode::FORBIDDEN,
                "{path}"
            );
        }
        assert_eq!(status(&app, MANAGEMENT, "/healthz").await, StatusCode::OK);
        assert_eq!(status(&app, "127.0.0.1", "/healthz").await, StatusCode::OK);
        // An IPv4 connection to a dual-stack listener names its address mapped into IPv6.
        assert_eq!(
            status(&app, &format!("::ffff:{PRIVATE}"), "/healthz").await,
            StatusCode::FORBIDDEN
        );
        // Docker was asked once, and the addresses it named were kept.
        assert_eq!(calls(&dir), 2);
    }

    #[tokio::test]
    async fn an_address_docker_does_not_name_is_refused_and_looked_up_at_most_once_a_second() {
        let mut app = crate::test_support::app(false);
        let dir = docker(&mut app, networks(), "private\n");
        let link_local = "fe80::42:acff:fe14:3";
        assert!(!app.interfaces.answers(&app, ip(link_local)).await.unwrap());
        assert_eq!(calls(&dir), 2);
        // Asked again at once, Docker is not.
        assert!(!app.interfaces.answers(&app, ip(link_local)).await.unwrap());
        assert_eq!(calls(&dir), 2);
        assert!(app.interfaces.answers(&app, ip(MANAGEMENT)).await.unwrap());
        assert_eq!(calls(&dir), 2);
    }

    #[tokio::test]
    async fn a_network_joined_later_is_looked_up_before_its_first_answer() {
        let mut app = crate::test_support::app(false);
        let dir = docker(&mut app, networks(), "private\n");
        assert!(app.interfaces.answers(&app, ip(MANAGEMENT)).await.unwrap());
        // The manager joins a new desktop's private network after the lookup.
        let joined = "172.21.0.3";
        let mut later = networks();
        later["thoric-2-private"] = json!({"NetworkID": "private-2", "IPAddress": joined});
        std::fs::write(
            dir.join("manager.json"),
            json!([manager(later)]).to_string(),
        )
        .unwrap();
        std::fs::write(dir.join("made"), "private\nprivate-2\n").unwrap();
        *app.interfaces.looked_up.lock().await = None;
        assert!(!app.interfaces.answers(&app, ip(joined)).await.unwrap());
        assert_eq!(calls(&dir), 4);
    }

    #[tokio::test]
    async fn without_docker_an_unknown_address_is_not_answered() {
        let mut app = crate::test_support::app(false);
        let dir = docker(&mut app, networks(), "private\n");
        std::fs::remove_file(dir.join("manager.json")).unwrap();
        assert!(app.interfaces.answers(&app, ip(MANAGEMENT)).await.is_err());
        assert!(!status(&app, MANAGEMENT, "/healthz").await.is_success());
        // Loopback needs no Docker, so the image's health check still answers.
        assert_eq!(status(&app, "127.0.0.1", "/healthz").await, StatusCode::OK);
    }
}
