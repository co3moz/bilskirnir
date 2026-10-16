//! Pins down how a volume is opened: the helper container reaches nothing but the volume and
//! works as the desktop user, its paths stay inside the volume, the manager's own volume is
//! refused to helpers and desktops alike, and a request is checked before Docker is asked
//! anything.
use super::*;
use crate::tests::app;
use axum::{body::Body, http::Request};
use std::sync::Arc;
use tower::ServiceExt;

fn volume() -> Volume {
    Volume {
        name: "project".into(),
        image: "sha256:manager".into(),
    }
}

fn command() -> Vec<String> {
    ["python3", "-c", "print(1)", "/volume/notes"]
        .map(str::to_owned)
        .to_vec()
}

/// The value that follows `flag`, for flags given once.
fn after<'a>(args: &'a [String], flag: &str) -> &'a str {
    let at = args.iter().position(|arg| arg == flag).unwrap();
    &args[at + 1]
}

#[test]
fn a_helper_reaches_nothing_but_the_volume() {
    for access in [Access::Read, Access::Write, Access::Archive] {
        let args = helper_args(&volume(), access, "wsld-volume-test", false, &command());
        let joined = args.join(" ");
        assert_eq!(&args[..2], ["run", "--rm"]);
        assert_eq!(after(&args, "--network"), "none");
        assert_eq!(after(&args, "--cap-drop"), "ALL");
        assert_eq!(after(&args, "--security-opt"), "no-new-privileges");
        assert!(args.contains(&"--read-only".into()));
        // One mount, the volume; never the Docker socket or a host path.
        assert_eq!(args.iter().filter(|arg| *arg == "--mount").count(), 1);
        assert!(after(&args, "--mount").starts_with("type=volume,src=project,dst=/volume"));
        assert!(!joined.contains("docker.sock"), "{joined}");
        for forbidden in [
            "--privileged",
            "--volume",
            "-v",
            "--device",
            "--pid",
            "--ipc",
        ] {
            assert!(
                !args.iter().any(|arg| arg == forbidden),
                "{forbidden} in {joined}"
            );
        }
        assert!(args.contains(&"sha256:manager".into()));
        assert!(!args.contains(&"--interactive".into()));
    }
    let piped = helper_args(&volume(), Access::Read, "h", true, &command());
    assert!(piped.contains(&"--interactive".into()));
}

#[test]
fn reading_is_the_desktop_user_on_a_read_only_mount() {
    let args = helper_args(&volume(), Access::Read, "h", false, &command());
    assert_eq!(after(&args, "--user"), "1001:1001");
    assert!(after(&args, "--mount").ends_with(",readonly"));
    assert!(!args.contains(&"--cap-add".into()));
    // The command follows the image as it was given.
    let image = args.iter().position(|arg| arg == "sha256:manager").unwrap();
    assert_eq!(after(&args, "--entrypoint"), "python3");
    assert_eq!(&args[image + 1..], &command()[1..]);
}

#[test]
fn writing_claims_an_empty_volume_then_runs_as_the_desktop_user() {
    let args = helper_args(&volume(), Access::Write, "h", true, &command());
    assert_eq!(after(&args, "--user"), "0:0");
    assert!(!after(&args, "--mount").contains("readonly"));
    let added: Vec<&str> = args
        .windows(2)
        .filter(|pair| pair[0] == "--cap-add")
        .map(|pair| pair[1].as_str())
        .collect();
    assert_eq!(added, ["CHOWN", "SETUID", "SETGID"]);
    // The claim script runs first and hands the whole command on.
    let image = args.iter().position(|arg| arg == "sha256:manager").unwrap();
    assert_eq!(args[image + 1], "-c");
    assert_eq!(args[image + 2], CLAIM);
    assert_eq!(&args[image + 3..], command());
    // It gives up root before it runs anything, and only claims a root-owned, empty volume.
    let drop = CLAIM.find("os.setuid(DESKTOP)").unwrap();
    assert!(CLAIM.find("os.setgid(DESKTOP)").unwrap() < drop);
    assert!(CLAIM.find("os.execvp").unwrap() > drop);
    assert!(CLAIM.contains("info.st_uid == 0 and not os.listdir(VOLUME)"));
}

#[test]
fn paths_stay_inside_the_volume() {
    assert_eq!(files::inside("/"), "/volume");
    assert_eq!(
        files::inside("/project/notes.txt"),
        "/volume/project/notes.txt"
    );
    assert_eq!(files::outside("/volume"), "/");
    assert_eq!(files::outside("/volume/project"), "/project");
    // Only the mount itself is translated.
    assert_eq!(files::outside("/volumes/x"), "/volumes/x");
}

#[test]
fn the_managers_own_volume_is_recognized() {
    let manager = serde_json::json!({"Mounts": [
        {"Type": "bind", "Name": "", "Source": "/var/run/docker.sock"},
        {"Type": "volume", "Name": "bilskirnir-data", "Destination": "/data"},
    ]});
    assert!(manager_volume(&manager, "bilskirnir-data"));
    assert!(!manager_volume(&manager, "project"));
    assert!(!manager_volume(&serde_json::json!({}), "bilskirnir-data"));
}

#[test]
fn a_desktop_cannot_mount_the_managers_own_volume() {
    let manager = serde_json::json!({"Mounts": [
        {"Type": "volume", "Name": "bilskirnir-data", "Destination": "/data"},
    ]});
    let mount = |volume: &str, target: &str| Mount {
        volume: volume.into(),
        target: target.into(),
        readonly: true,
    };
    assert!(refuse_manager_volumes(&manager, &[]).is_ok());
    assert!(refuse_manager_volumes(&manager, &[mount("project", "/workspace")]).is_ok());
    // Read-only does not make it safe: the database holds the sessions.
    let error = refuse_manager_volumes(
        &manager,
        &[
            mount("project", "/workspace"),
            mount("bilskirnir-data", "/opt/data"),
        ],
    )
    .unwrap_err();
    assert_eq!(
        error.to_string(),
        "bilskirnir-data is the manager's own volume and cannot be mounted in a desktop"
    );
}

#[test]
fn helper_failures_keep_their_meaning() {
    let conflict = helper_error(b"CONFLICT: File already exists.\n");
    assert_eq!(conflict.0, StatusCode::CONFLICT);
    assert_eq!(conflict.1, "File already exists.");
    let denied = helper_error(b"File operation failed: [Errno 13] Permission denied: 'a'\n");
    assert_eq!(denied.0, StatusCode::FORBIDDEN);
    assert!(denied.1.contains("UID 1001"));
    // Docker leads with its reason and ends with advice that explains nothing.
    let docker = helper_error(
        b"docker: Error response from daemon: no such image.\nSee 'docker run --help'.\n",
    );
    assert_eq!(
        docker.1,
        "docker: Error response from daemon: no such image."
    );
    assert_eq!(helper_error(b"").0, StatusCode::BAD_GATEWAY);
}

async fn post(app: Arc<App>, uri: &str, body: serde_json::Value) -> (StatusCode, String) {
    let response = crate::router(app)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("host", "localhost:8080")
                .header("content-type", "application/json")
                .header("x-wsld-request", "1")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 65536)
        .await
        .unwrap();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

#[tokio::test]
async fn requests_are_checked_before_docker_is_asked() {
    let app = app(false);
    use serde_json::json;
    for (uri, body) in [
        (
            "/api/volumes/project/files/list",
            json!({"path": "/a/../b"}),
        ),
        ("/api/volumes/project/files/read", json!({"path": "/"})),
        (
            "/api/volumes/project/files/write",
            json!({"path": "/", "data": ""}),
        ),
        ("/api/volumes/project/files/mkdir", json!({"path": "/"})),
        ("/api/volumes/project/files/delete", json!({"path": "/"})),
        (
            "/api/volumes/project/files/move",
            json!({"source": "/", "destination": "/b"}),
        ),
        ("/api/volumes/-bad/files/list", json!({"path": "/"})),
    ] {
        let (status, text) = post(app.clone(), uri, body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}: {text}");
    }
    // A valid request needs the manager's container for its helper image.
    let (status, text) = post(
        app.clone(),
        "/api/volumes/project/files/list",
        json!({"path": "/"}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{text}");
    assert!(text.contains("MANAGER_CONTAINER"));
}

#[tokio::test]
async fn volume_files_need_a_sign_in() {
    let (status, _) = post(
        app(true),
        "/api/volumes/project/files/list",
        serde_json::json!({"path": "/"}),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
