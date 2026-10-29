//! Pins down that failed sign-ins are counted per address and successes not at all: one guessing
//! neighbour is refused after its attempts while another address still reaches the password
//! check, the administrator signs in as often as they like, the address behind the HTTPS proxy
//! counts in place of the proxy's, and nobody else can name an address of their choosing.
use super::*;
use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode},
};
use tower::ServiceExt;

const PASSWORD: &str = "correct horse";
const CADDY: &str = "172.18.0.3";

/// A signed-in installation whose password hash is cheap to check, so a test can try it often.
fn app() -> Arc<App> {
    let mut app = crate::tests::app(true);
    let salt = argon2::password_hash::SaltString::encode_b64(b"thoric-test!").unwrap();
    let cheap = Argon2::new(
        argon2::Algorithm::Argon2id,
        argon2::Version::V0x13,
        argon2::Params::new(8, 1, 1, None).unwrap(),
    );
    Arc::get_mut(&mut app).unwrap().config.password_hash = Some(
        argon2::PasswordHasher::hash_password(&cheap, PASSWORD.as_bytes(), &salt)
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
        request.extensions_mut().insert(ConnectInfo(address));
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

#[tokio::test]
async fn failed_sign_ins_are_limited_per_address() {
    let app = app();
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
    let app = app();
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
    let app = &app();
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
    let app = app();
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
    let app = crate::tests::app(true);
    assert_eq!(
        sign_in(&app, PASSWORD, Some("192.168.1.20"), None).await,
        StatusCode::INTERNAL_SERVER_ERROR
    );
}
