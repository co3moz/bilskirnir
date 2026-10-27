//! Pins down what an image icon accepts, how it travels as JSON, that a picture this release
//! cannot use never costs a record or an archive the rest of its fields, and how the icon route
//! lets browsers keep a picture until it changes.
use super::*;
use axum::{body::Body, http::Request};
use serde_json::json;
use tower::ServiceExt;

fn record(id: &str, icon: Option<Icon>) -> ImageRecord {
    ImageRecord {
        id: id.into(),
        name: format!("{id} image"),
        image_id: format!("sha256:{id}"),
        reference: format!("thoric/custom:{id}"),
        parent_image_id: None,
        source_container: None,
        kind: "build".into(),
        created_at: 1,
        description: String::new(),
        desktop: None,
        build: None,
        compose: None,
        icon,
        template_ids: Vec::new(),
    }
}

#[test]
fn an_icon_is_a_png_of_at_most_256_pixels_a_side() {
    for (width, height) in [(1, 1), (256, 256), (64, 256), (256, 1)] {
        assert!(sample(width, height).validate().is_ok(), "{width}x{height}");
    }
    for (width, height) in [(0, 16), (16, 0), (257, 256), (256, 4096)] {
        let refused = sample(width, height).validate().unwrap_err().to_string();
        assert!(
            refused.contains("256 by 256"),
            "{width}x{height}: {refused}"
        );
    }
    let mut gif = sample(16, 16).0;
    gif[..6].copy_from_slice(b"GIF89a");
    assert!(Icon(gif).validate().is_err());
    assert!(Icon(b"\x89PNG".to_vec()).validate().is_err());
    let mut large = sample(256, 256).0;
    large.resize(ICON_LIMIT + 1, 0);
    assert!(Icon(large).validate().is_err());
}

#[test]
fn json_carries_an_icon_as_a_png_data_url() {
    let icon = sample(32, 32);
    let text = serde_json::to_value(&icon).unwrap();
    assert!(
        text.as_str()
            .unwrap()
            .starts_with("data:image/png;base64,iVBORw0KGgo")
    );
    assert_eq!(serde_json::from_value::<Icon>(text).unwrap(), icon);
    for refused in [
        json!("data:image/gif;base64,R0lGODlh"),
        json!("iVBORw0KGgo="),
        json!("data:image/png;base64,not base64!"),
        json!(42),
        // Refused by its length alone, before it is decoded.
        json!(format!("{DATA_URL}{}", "A".repeat(ICON_LIMIT * 2))),
    ] {
        assert!(
            serde_json::from_value::<Icon>(refused.clone()).is_err(),
            "{refused}"
        );
    }
}

#[test]
fn a_saved_icon_this_release_cannot_use_reads_as_none() {
    let mut saved = json!(record("a", None));
    for icon in [
        json!("data:image/webp;base64,UklGRg=="),
        serde_json::to_value(sample(512, 512)).unwrap(),
        json!(null),
    ] {
        saved["icon"] = icon.clone();
        let read: ImageRecord = serde_json::from_value(saved.clone()).unwrap();
        assert_eq!(read.icon, None, "{icon}");
        assert_eq!(read.name, "a image");
    }
    saved["icon"] = serde_json::to_value(sample(48, 48)).unwrap();
    let read: ImageRecord = serde_json::from_value(saved).unwrap();
    assert_eq!(read.icon, Some(sample(48, 48)));
}

#[test]
fn the_version_follows_the_picture() {
    let icon = sample(16, 16);
    assert_eq!(icon.version(), sample(16, 16).version());
    assert_eq!(icon.version().len(), 16);
    assert_ne!(icon.version(), sample(16, 17).version());
}

#[test]
fn a_snapshot_keeps_the_icon_of_the_image_its_container_came_from() {
    let records = [
        record("plain", None),
        record("unity", Some(sample(64, 64))),
        record("other", Some(sample(32, 32))),
    ];
    assert_eq!(inherited(&records, "sha256:unity"), Some(sample(64, 64)));
    assert_eq!(inherited(&records, "sha256:plain"), None);
    // A container from a catalog image has no saved image to inherit from.
    assert_eq!(inherited(&records, "sha256:catalog"), None);
}

async fn fetch(app: Arc<App>, uri: &str, tag: Option<&str>) -> Response {
    let mut request = Request::builder().uri(uri).header("host", "localhost:8080");
    if let Some(tag) = tag {
        request = request.header("if-none-match", tag);
    }
    crate::router(app)
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap()
}

#[tokio::test]
async fn the_icon_route_serves_the_picture_a_list_names() {
    let app = crate::tests::app(false);
    app.db.put("image", "a", &record("a", None)).unwrap();
    let missing = fetch(app.clone(), "/api/images/a/icon", None).await;
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);

    let icon = sample(128, 128);
    let Json(summary) = set(
        State(app.clone()),
        Path("a".into()),
        Json(SetIcon { icon: icon.clone() }),
    )
    .await
    .unwrap();
    // Lists name the version; the picture itself only comes from the icon route.
    assert_eq!(summary["icon"], icon.version());
    let Json(listed) = crate::api::images(State(app.clone())).await.unwrap();
    let saved = listed
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["id"] == "a")
        .unwrap();
    assert_eq!(saved["icon"], icon.version());

    let named = format!("/api/images/a/icon?v={}", icon.version());
    let response = fetch(app.clone(), &named, None).await;
    assert_eq!(response.status(), StatusCode::OK);
    let headers = response.headers().clone();
    assert_eq!(headers["content-type"], "image/png");
    assert_eq!(
        headers["cache-control"],
        "private, max-age=31536000, immutable"
    );
    assert_eq!(headers["x-content-type-options"], "nosniff");
    let body = axum::body::to_bytes(response.into_body(), ICON_LIMIT)
        .await
        .unwrap();
    assert_eq!(body.as_ref(), icon.0.as_slice());

    // Without the current version a browser asks again, and keeps its copy while it matches.
    let stale = fetch(app.clone(), "/api/images/a/icon?v=0000", None).await;
    assert_eq!(stale.headers()["cache-control"], "private, no-cache");
    let tag = headers["etag"].to_str().unwrap();
    let unchanged = fetch(app.clone(), "/api/images/a/icon", Some(tag)).await;
    assert_eq!(unchanged.status(), StatusCode::NOT_MODIFIED);

    let Json(summary) = remove(State(app.clone()), Path("a".into())).await.unwrap();
    assert!(summary.get("icon").is_none());
    let gone = fetch(app.clone(), "/api/images/a/icon", Some(tag)).await;
    assert_eq!(gone.status(), StatusCode::NOT_FOUND);
    // Other API responses are still never stored.
    assert_eq!(gone.headers()["cache-control"], "no-store");
}

#[tokio::test]
async fn only_a_saved_image_takes_a_usable_icon() {
    let app = crate::tests::app(false);
    app.db.put("image", "a", &record("a", None)).unwrap();
    let catalog = set(
        State(app.clone()),
        Path("ubuntu-xfce".into()),
        Json(SetIcon {
            icon: sample(16, 16),
        }),
    )
    .await
    .unwrap_err();
    assert_eq!(catalog.0, StatusCode::NOT_FOUND);
    let large = set(
        State(app.clone()),
        Path("a".into()),
        Json(SetIcon {
            icon: sample(1024, 1024),
        }),
    )
    .await
    .unwrap_err();
    assert_eq!(large.0, StatusCode::BAD_REQUEST);
    let saved: ImageRecord = app.db.get("image", "a").unwrap();
    assert_eq!(saved.icon, None);
}
