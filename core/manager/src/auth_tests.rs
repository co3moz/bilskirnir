//! Pins down that failed sign-ins are counted per address and successes not at all: one guessing
//! neighbour is refused after its attempts while another address still reaches the password check,
//! the administrator signs in as often as they like, the address behind the HTTPS proxy counts in
//! place of the proxy's, and nobody else can name an address of their choosing. Also the Host,
//! Origin and session checks every route gets, whether sign-in is on or off, that signing out
//! ends the media connections of the session, and that a password hash an earlier release made
//! still signs in.
use super::*;
use crate::router;
use crate::test_support::app;
use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode},
};
use std::{sync::Arc, time::Instant};
use tower::ServiceExt;

const PASSWORD: &str = "correct horse";
const CADDY: &str = "172.18.0.3";

/// A signed-in installation whose password hash is cheap to check, so a test can try it often.
fn with_password() -> Arc<App> {
    let mut app = crate::test_support::app(true);
    let cheap = Argon2::new(
        argon2::Algorithm::Argon2id,
        argon2::Version::V0x13,
        argon2::Params::new(8, 1, 1, None).unwrap(),
    );
    Arc::get_mut(&mut app).unwrap().config.password_hash = Some(
        argon2::PasswordHasher::hash_password_with_salt(
            &cheap,
            PASSWORD.as_bytes(),
            b"thoric-test!",
        )
        .unwrap()
        .to_string(),
    );
    app
}

async fn sign_in(
    app: &Arc<App>,
    password: &str,
    from: Option<&str>,
    forwarded_for: Option<&str>,
) -> StatusCode {
    let mut request = Request::builder()
        .method("POST")
        .uri("/api/login")
        .header("host", "localhost:8080")
        .header("content-type", "application/json")
        .header("x-thoric-request", "1");
    if let Some(client) = forwarded_for {
        request = request.header("x-forwarded-for", client);
    }
    let mut request = request
        .body(Body::from(
            serde_json::json!({"username": "admin", "password": password}).to_string(),
        ))
        .unwrap();
    if let Some(from) = from {
        let address: SocketAddr = format!("{from}:40000").parse().unwrap();
        request.extensions_mut().insert(ConnectInfo(Connection {
            peer: address,
            local: None,
        }));
    }
    crate::router(app.clone())
        .oneshot(request)
        .await
        .unwrap()
        .status()
}

async fn attempt(app: &Arc<App>, from: Option<&str>) -> StatusCode {
    sign_in(app, "wrong", from, None).await
}

/// What [`with_password`] stored before argon2 0.6: the PHC string an earlier release made.
const EARLIER_HASH: &str =
    "$argon2id$v=19$m=8,t=1,p=1$dGhvcmljLXRlc3Qh$KTST4D05cy2bFJH8k9FpmhvxIwHFcVEtlGeiSFuj0Os";

#[tokio::test]
async fn a_password_hash_an_earlier_release_made_still_signs_in() {
    // The same password, salt and parameters give the very same string.
    assert_eq!(
        with_password().config.password_hash.as_deref(),
        Some(EARLIER_HASH)
    );
    let mut app = app(true);
    Arc::get_mut(&mut app).unwrap().config.password_hash = Some(EARLIER_HASH.into());
    assert_eq!(
        sign_in(&app, PASSWORD, Some("192.168.1.20"), None).await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        attempt(&app, Some("192.168.1.20")).await,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn failed_sign_ins_are_limited_per_address() {
    let app = with_password();
    for _ in 0..limiter::ATTEMPTS {
        assert_eq!(
            attempt(&app, Some("192.168.1.20")).await,
            StatusCode::UNAUTHORIZED
        );
    }
    assert_eq!(
        attempt(&app, Some("192.168.1.20")).await,
        StatusCode::TOO_MANY_REQUESTS
    );
    // Not even the right password passes while the address is refused.
    assert_eq!(
        sign_in(&app, PASSWORD, Some("192.168.1.20"), None).await,
        StatusCode::TOO_MANY_REQUESTS
    );
    // Another address, and a request without a recorded one, still reach the password check.
    assert_eq!(
        attempt(&app, Some("192.168.1.21")).await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(attempt(&app, None).await, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn signing_in_never_uses_up_the_attempts() {
    let app = with_password();
    let from = Some("192.168.1.20");
    for _ in 0..2 * limiter::ATTEMPTS {
        assert_eq!(
            sign_in(&app, PASSWORD, from, None).await,
            StatusCode::NO_CONTENT
        );
    }
    // A success also forgets the failures before it.
    for _ in 0..limiter::ATTEMPTS - 1 {
        assert_eq!(attempt(&app, from).await, StatusCode::UNAUTHORIZED);
    }
    assert_eq!(
        sign_in(&app, PASSWORD, from, None).await,
        StatusCode::NO_CONTENT
    );
    for _ in 0..limiter::ATTEMPTS {
        assert_eq!(attempt(&app, from).await, StatusCode::UNAUTHORIZED);
    }
    assert_eq!(attempt(&app, from).await, StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn the_https_proxy_counts_by_the_address_it_forwards() {
    let app = &with_password();
    app.https_proxy.set(vec![CADDY.parse().unwrap()]);
    let through = move |client| sign_in(app, "wrong", Some(CADDY), Some(client));
    for _ in 0..limiter::ATTEMPTS {
        assert_eq!(through("192.168.1.20").await, StatusCode::UNAUTHORIZED);
    }
    assert_eq!(through("192.168.1.20").await, StatusCode::TOO_MANY_REQUESTS);
    // Another browser behind the same proxy still signs in.
    assert_eq!(
        sign_in(app, PASSWORD, Some(CADDY), Some("192.168.1.21")).await,
        StatusCode::NO_CONTENT
    );
}

#[tokio::test]
async fn nobody_else_can_name_the_address_that_counts() {
    let app = with_password();
    app.https_proxy.set(vec![CADDY.parse().unwrap()]);
    // A desktop on its private network, which the manager shares, names a new address each time.
    for n in 0..limiter::ATTEMPTS {
        let named = format!("10.0.0.{n}");
        assert_eq!(
            sign_in(&app, "wrong", Some("172.19.0.2"), Some(&named)).await,
            StatusCode::UNAUTHORIZED
        );
    }
    assert_eq!(
        sign_in(&app, "wrong", Some("172.19.0.2"), Some("10.0.0.99")).await,
        StatusCode::TOO_MANY_REQUESTS
    );
}

#[tokio::test]
async fn sign_in_without_a_password_is_an_error() {
    let app = crate::test_support::app(true);
    assert_eq!(
        sign_in(&app, PASSWORD, Some("192.168.1.20"), None).await,
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[tokio::test]
async fn untrusted_host_is_rejected_even_with_matching_origin_and_no_login() {
    let app = app(false);
    let response = router(app)
        .oneshot(
            Request::builder()
                .uri("/api/session")
                .header("host", "evil.example:8080")
                .header("origin", "http://evil.example:8080")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[test]
fn host_validation_accepts_local_ips_and_explicit_lan_names() {
    let mut app = app(false);
    Arc::get_mut(&mut app)
        .unwrap()
        .config
        .allowed_hosts
        .push("desktop.lan".into());
    for host in [
        "localhost:8080",
        "127.0.0.1:8088",
        "192.168.1.4:8080",
        "[::1]:8080",
        "[fd00::1]:8080",
        "desktop.lan:8443",
    ] {
        let mut h = axum::http::HeaderMap::new();
        h.insert("host", host.parse().unwrap());
        assert!(app.host_allowed(&h), "{host}");
    }
    for host in [
        "example.org",
        "localhost.evil.com",
        "1.1.1.1",
        "user@localhost",
        "0.0.0.0",
    ] {
        let mut h = axum::http::HeaderMap::new();
        h.insert("host", host.parse().unwrap());
        assert!(!app.host_allowed(&h), "{host}");
    }
}

#[tokio::test]
async fn logout_revokes_waiting_media_lease_and_expiry_ends_it() {
    let app = app(true);
    let expiry = Instant::now() + std::time::Duration::from_secs(60);
    app.sessions.lock().unwrap().insert("media".into(), expiry);
    let lease = app.session_revoked(Some("media".into()));
    tokio::pin!(lease);
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(5), &mut lease)
            .await
            .is_err()
    );
    let response = router(app.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/logout")
                .header("host", "localhost:8080")
                .header("cookie", "thoric_session=media")
                .header("x-thoric-request", "1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    tokio::time::timeout(std::time::Duration::from_millis(100), lease)
        .await
        .unwrap();
    app.sessions.lock().unwrap().insert(
        "short".into(),
        Instant::now() + std::time::Duration::from_millis(15),
    );
    tokio::time::timeout(
        std::time::Duration::from_millis(100),
        app.session_revoked(Some("short".into())),
    )
    .await
    .unwrap();
}

#[test]
fn websocket_origin_requires_exact_host_and_port() {
    let mut h = axum::http::HeaderMap::new();
    h.insert("host", "localhost:8080".parse().unwrap());
    h.insert("origin", "http://localhost:8080".parse().unwrap());
    assert!(origin_matches(&h));
    h.insert("origin", "https://evil.example".parse().unwrap());
    assert!(!origin_matches(&h));
    h.insert("origin", "http://localhost:8081".parse().unwrap());
    assert!(!origin_matches(&h));
    h.remove("origin");
    assert!(!origin_matches(&h));
}

#[tokio::test]
async fn login_disabled_does_not_disable_csrf_protection() {
    let router = router(app(false));
    let response = router
        .oneshot(
            Request::builder()
                .header("host", "localhost:8080")
                .method("POST")
                .uri("/api/containers")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn valid_session_still_rejects_cross_origin_mutation() {
    let app = app(true);
    app.sessions.lock().unwrap().insert(
        "token".into(),
        Instant::now() + std::time::Duration::from_secs(60),
    );
    let response = router(app)
        .oneshot(
            Request::builder()
                .header("host", "localhost:8080")
                .method("POST")
                .uri("/api/logout")
                .header("cookie", "thoric_session=token")
                .header("x-thoric-request", "1")
                .header("host", "localhost:8080")
                .header("origin", "https://evil.example")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn expired_session_is_not_accepted() {
    let app = app(true);
    app.sessions.lock().unwrap().insert(
        "expired".into(),
        Instant::now() - std::time::Duration::from_secs(1),
    );
    let response = router(app)
        .oneshot(
            Request::builder()
                .header("host", "localhost:8080")
                .uri("/api/jobs")
                .header("cookie", "thoric_session=expired")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn another_site_cannot_start_work_with_a_get() {
    let get = |uri: &'static str, site: Option<&'static str>| {
        let mut request = Request::builder()
            .header("host", "localhost:8080")
            .method("GET")
            .uri(uri);
        if let Some(site) = site {
            request = request.header("sec-fetch-site", site);
        }
        router(app(false)).oneshot(request.body(Body::empty()).unwrap())
    };
    for uri in [
        "/api/backup",
        "/api/containers/c1/files/download?path=/home/desktop/project",
        "/api/images/i1/archive",
        "/api/volumes/projects/archive",
        "/api/volumes/projects/files/download?path=/",
    ] {
        // An <img> or a link of another site, also one on another port of this host.
        for site in ["cross-site", "same-site"] {
            let response = get(uri, Some(site)).await.unwrap();
            assert_eq!(response.status(), StatusCode::FORBIDDEN, "{uri} {site}");
        }
        // The dashboard, an address opened by hand and a client that is no browser get on to
        // the route, which answers for itself.
        for site in [Some("same-origin"), Some("none"), None] {
            let response = get(uri, site).await.unwrap();
            assert_ne!(response.status(), StatusCode::FORBIDDEN, "{uri} {site:?}");
        }
    }
    // A read that starts nothing stays as it was.
    let response = get("/api/containers", Some("cross-site")).await.unwrap();
    assert_ne!(response.status(), StatusCode::FORBIDDEN);
}
