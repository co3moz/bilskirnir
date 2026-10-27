//! Pins down that failed sign-ins are counted per address: one guessing neighbour is refused
//! after its attempts while another address still reaches the password check.
use super::*;
use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode},
};
use tower::ServiceExt;

async fn attempt(app: &Arc<App>, from: Option<&str>) -> StatusCode {
    let mut request = Request::builder()
        .method("POST")
        .uri("/api/login")
        .header("host", "localhost:8080")
        .header("content-type", "application/json")
        .header("x-thoric-request", "1")
        .body(Body::from(r#"{"username":"admin","password":"wrong"}"#))
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

#[tokio::test]
async fn failed_sign_ins_are_limited_per_address() {
    let mut app = crate::tests::app(true);
    let salt = argon2::password_hash::SaltString::encode_b64(b"thoric-test!").unwrap();
    Arc::get_mut(&mut app).unwrap().config.password_hash = Some(
        argon2::PasswordHasher::hash_password(&Argon2::default(), b"correct horse", &salt)
            .unwrap()
            .to_string(),
    );
    for _ in 0..LOGIN_ATTEMPTS {
        assert_eq!(
            attempt(&app, Some("192.168.1.20")).await,
            StatusCode::UNAUTHORIZED
        );
    }
    assert_eq!(
        attempt(&app, Some("192.168.1.20")).await,
        StatusCode::TOO_MANY_REQUESTS
    );
    // Another address, and a request without a recorded one, still reach the password check.
    assert_eq!(
        attempt(&app, Some("192.168.1.21")).await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(attempt(&app, None).await, StatusCode::UNAUTHORIZED);
}
