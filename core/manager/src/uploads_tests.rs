//! Pins down where an upload may land, a canonical folder in a writable place and nowhere else,
//! and that the upload route checks the session, the origin and the size before any Docker work.
//! Also that the body arrives before the container is held, so only the delivery waits for a
//! reset or a snapshot.
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

/// A running container `e` behind a stand-in Docker CLI whose upload helper writes what it reads
/// to `uploaded`.
#[cfg(unix)]
fn running_container(app: &mut Arc<App>) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("thoric-uploads-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(dir.join("uploads")).unwrap();
    let docker = dir.join("docker");
    crate::test_support::write_script(
        &docker,
        &format!(
            "#!/bin/sh\ncase \"$1 $2\" in\n  'container inspect') echo '[{{\"Id\":\"c0ffee\",\"Config\":{{\"Labels\":{{\"thoric.managed\":\"true\",\"thoric.environment\":\"e\"}}}},\"State\":{{\"Running\":true}},\"Mounts\":[]}}]' ;;\n  'exec --interactive') cat > '{dir}/uploaded' ;;\n  *) exit 1 ;;\nesac\n",
            dir = dir.display()
        ),
    );
    let config = &mut Arc::get_mut(app).unwrap().config;
    config.docker_bin = docker.display().to_string();
    config.data = dir.clone();
    let env = model::Container {
        id: "e".into(),
        container: "thoric-e".into(),
        private_network: "thoric-e-private".into(),
        egress_network: "thoric-e-egress".into(),
        source_image_id: "sha256:desktop".into(),
        created_at: 1,
        spec: spec(),
        last_error: None,
        desktop: None,
        working_image: None,
        setup: None,
    };
    app.db.put(model::CONTAINERS, "e", &env).unwrap();
    dir
}

#[cfg(unix)]
#[tokio::test]
async fn the_body_arrives_before_the_container_is_held_and_only_the_delivery_waits() {
    let mut app = app(false);
    let dir = running_container(&mut app);
    // A reset holds the container while the client sends its file.
    let reset = app.resource_lock("container:e").lock_owned().await;
    let (chunks, received) = tokio::sync::mpsc::channel::<std::io::Result<&'static [u8]>>(1);
    let body = Body::from_stream(futures_util::stream::unfold(
        received,
        |mut received| async move { received.recv().await.map(|chunk| (chunk, received)) },
    ));
    let request = Upload {
        destination: "/home/desktop/Downloads".into(),
        name: "notes.txt".into(),
        size: 10,
        replace: false,
    };
    let upload = tokio::spawn({
        let app = app.clone();
        async move { save_file(&app, "e".into(), request, body, std::future::pending()).await }
    });
    // Every chunk is taken while the container is still held, which a channel of one shows.
    for chunk in [&b"hello"[..], &b" you"[..], &b"!"[..]] {
        tokio::time::timeout(Duration::from_secs(2), chunks.send(Ok(chunk)))
            .await
            .expect("the body is read without the container")
            .unwrap();
    }
    drop(chunks);
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(!upload.is_finished(), "the delivery waits for the reset");
    drop(reset);
    let saved = upload.await.unwrap().unwrap().0;
    assert_eq!(saved["bytes"], 10);
    assert_eq!(std::fs::read(dir.join("uploaded")).unwrap(), b"hello you!");
    // The staged copy is gone once it is delivered.
    assert_eq!(std::fs::read_dir(dir.join("uploads")).unwrap().count(), 0);
    std::fs::remove_dir_all(dir).unwrap();
}
