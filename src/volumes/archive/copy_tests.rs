//! Pins down which copies are refused before any job, helper or new volume exists.
use super::*;
use crate::tests::app;
use axum::{body::Body, http::Request};
use tower::ServiceExt;

#[tokio::test]
async fn a_copy_is_refused_before_anything_starts() {
    let app = app(false);
    for (source, body, expected) in [
        ("project", r#"{"name":"-bad"}"#, StatusCode::BAD_REQUEST),
        ("project", r#"{"name":"a"}"#, StatusCode::BAD_REQUEST),
        ("-bad", r#"{"name":"copy"}"#, StatusCode::BAD_REQUEST),
        (
            "project",
            r#"{"name":"copy","replace":true}"#,
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        ("project", "{}", StatusCode::UNPROCESSABLE_ENTITY),
        // Without MANAGER_CONTAINER no helper image is known, so Docker is not asked.
        ("project", r#"{"name":"copy"}"#, StatusCode::CONFLICT),
    ] {
        let response = crate::router(app.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/volumes/{source}/copy"))
                    .header("host", "localhost:8080")
                    .header("content-type", "application/json")
                    .header("x-wsld-request", "1")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), expected, "{source} {body}");
    }
    assert!(app.db.list::<crate::model::Job>("job").unwrap().is_empty());
    assert_eq!(app.archives.available_permits(), 2);
}
