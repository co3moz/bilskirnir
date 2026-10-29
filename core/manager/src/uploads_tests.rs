//! Pins down where an upload may land, a canonical folder in a writable place and nowhere else,
//! and that the upload route checks the session, the origin and the size before any Docker work.
use super::*;
use crate::test_support::{app, spec};
use crate::{model, router};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;

#[test]
fn uploads_only_accept_canonical_paths_in_writable_locations() {
    let mut spec = spec();
    spec.mounts = vec![
        model::Mount {
            volume: "projects".into(),
            target: "/shared".into(),
            readonly: false,
        },
        model::Mount {
            volume: "reference".into(),
            target: "/shared/reference".into(),
            readonly: true,
        },
    ];
    let places = [crate::compose::Place {
        label: "Unity project".into(),
        path: "/srv/unity/MyGame".into(),
    }];
    let check = |spec: &model::ContainerSpec, target: &str, name: &str| {
        check_target(&spec.mounts, &places, target, name)
    };
    // The whole home is the desktop user's, hidden folders too.
    for target in [
        "/home/desktop",
        "/home/desktop/Downloads",
        "/home/desktop/Downloads/project",
        "/home/desktop/Downloads-fake",
        "/home/desktop/Documents/notes",
        "/home/desktop/.config/autostart",
        "/shared",
        "/shared/assets",
        "/srv/unity/MyGame",
        "/srv/unity/MyGame/Assets",
    ] {
        assert!(check(&spec, target, "file & notes.txt").is_ok(), "{target}");
    }
    for target in [
        "/",
        "/etc",
        "/tmp",
        "/home",
        "/home/other",
        "/home/desktopper",
        "/sharedish",
        "/shared/reference",
        "/shared/reference/sub",
        "/srv/unity",
        "/srv/unity/MyGameOld",
        "/home/desktop/Downloads/../.ssh",
        "/home/desktop/",
        "/shared//assets",
        "/shared/.",
        "relative/folder",
    ] {
        assert!(check(&spec, target, "file.txt").is_err(), "{target}");
    }
    for name in [
        "",
        ".",
        "..",
        "../file",
        "file/name",
        "file\\name",
        "file\0name",
    ] {
        assert!(check(&spec, "/shared", name).is_err());
    }
    // A read-only volume inside the home closes that folder and nothing else.
    spec.mounts.push(model::Mount {
        volume: "downloads".into(),
        target: "/home/desktop/Downloads".into(),
        readonly: true,
    });
    assert!(check(&spec, "/home/desktop/Downloads", "file.txt").is_err());
    assert!(check(&spec, "/home/desktop/Downloads/sub", "file.txt").is_err());
    assert!(check(&spec, "/home/desktop/Documents", "file.txt").is_ok());
}

#[tokio::test]
async fn uploads_enforce_authentication_origin_and_size_before_docker() {
    let uri =
        "/api/containers/unknown/files?destination=/home/desktop/Downloads&name=test.txt&size=1";
    for (auth, csrf, size, expected) in [
        (true, true, 1, StatusCode::UNAUTHORIZED),
        (false, false, 1, StatusCode::FORBIDDEN),
        (false, true, 1048577, StatusCode::PAYLOAD_TOO_LARGE),
    ] {
        let uri = uri.replace("size=1", &format!("size={size}"));
        let mut request = Request::builder()
            .method("POST")
            .uri(uri)
            .header("host", "localhost:8080")
            .header("content-type", "application/octet-stream");
        if csrf {
            request = request.header("x-thoric-request", "1");
        }
        let response = router(app(auth))
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), expected);
    }
}
