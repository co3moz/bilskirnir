//! Pins down deleting a volume by itself: the manager's own volume, a missing one, one
//! `VOLUME_FILTER` leaves out and one a container record mounts are refused with their own status
//! before Docker is asked to remove anything, Docker's own refusal keeps its meaning, nothing is
//! forced or pruned, and a delete waits for the volume's lock.
use super::*;
use crate::model::Mount;
use axum::{body::Body, http::Request};
use tower::ServiceExt;

fn container(id: &str, mounts: &[&str], compose: &[&str]) -> Container {
    let mount = |volume: &&str| Mount {
        volume: volume.to_string(),
        target: format!("/mnt/{volume}"),
        readonly: false,
    };
    let mut spec: crate::model::ContainerSpec =
        serde_json::from_value(json!({"name": format!("Desktop {id}"), "image": "sha256:ws"}))
            .unwrap();
    spec.mounts = mounts.iter().map(mount).collect();
    let setup = (!compose.is_empty()).then(|| {
        serde_json::from_value(json!({
            "status": "completed", "image": "img", "parameters": {}, "secrets": [],
            "mounts": compose.iter().map(mount).collect::<Vec<_>>(),
        }))
        .unwrap()
    });
    Container {
        id: id.into(),
        container: format!("bilskirnir-{id}"),
        private_network: format!("bilskirnir-{id}-private"),
        egress_network: format!("bilskirnir-{id}-egress"),
        source_image_id: "sha256:ws".into(),
        created_at: 0,
        spec,
        last_error: None,
        desktop: None,
        working_image: None,
        setup,
    }
}

#[test]
fn a_record_that_mounts_the_volume_is_named_with_what_frees_it() {
    let records = [
        container("a", &["projects"], &[]),
        container("b", &[], &["bilskirnir-b-cache"]),
        container("c", &["projects"], &["installs"]),
    ];
    let found = |name: &str| mounted_by(name, &records).map(|record| record.id.as_str());
    assert_eq!(found("projects"), Some("a"));
    assert_eq!(found("bilskirnir-b-cache"), Some("b"));
    assert_eq!(found("installs"), Some("c"));
    assert_eq!(found("bilskirnir-gone-cache"), None);
    // A volume of the settings can be taken out there; one the compose was given cannot.
    assert_eq!(
        mounted("projects", &records[0]),
        "\"Desktop a\" mounts this volume; remove it from that container's volumes, or delete the container, first"
    );
    assert_eq!(
        mounted("bilskirnir-b-cache", &records[1]),
        "\"Desktop b\" mounts this volume for its compose; delete that container first"
    );
}

#[test]
fn dockers_refusal_keeps_its_meaning() {
    let status = |message: &str| {
        refused(anyhow::anyhow!(
            "Docker: Error response from daemon: {message}"
        ))
        .0
    };
    assert_eq!(
        status("remove busy: volume is in use - [c0ffee]"),
        StatusCode::CONFLICT
    );
    assert_eq!(status("get gone: no such volume"), StatusCode::NOT_FOUND);
    assert_eq!(status("something else"), StatusCode::BAD_REQUEST);
}

fn request(name: &str) -> Request<Body> {
    Request::builder()
        .method("DELETE")
        .uri(format!("/api/volumes/{name}"))
        .header("host", "localhost:8080")
        .header("x-bilskirnir-request", "1")
        .body(Body::empty())
        .unwrap()
}

async fn answer(app: Arc<App>, name: &str) -> (StatusCode, Value) {
    let response = crate::router(app).oneshot(request(name)).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn an_invalid_name_is_refused_before_docker() {
    // The test app's Docker CLI does not exist, so any call would fail differently.
    let (status, body) = answer(crate::tests::app(false), "-bad").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error"], "Invalid volume name");
}

#[test]
fn the_route_is_documented() {
    // Checked from here, since a change to the route is what leaves the guide behind.
    assert!(
        include_str!("../../../../docs/API.md").contains("| DELETE | `/api/volumes/{name}` |"),
        "docs/API.md must list DELETE /api/volumes/{{name}}"
    );
}

#[tokio::test]
async fn a_volume_called_import_is_not_taken_for_the_import_route() {
    // `POST /api/volumes/import` is a route of its own, which wins over `{name}`.
    let response = crate::router(crate::tests::app(false))
        .oneshot(request("import"))
        .await
        .unwrap();
    assert_ne!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}

/// A manager whose Docker CLI is a shell script answering `cases`, the arms of a `case` over
/// its arguments, with any other call failing. Its data volume is `bilskirnir-data`. Each call
/// is appended to `calls` in its directory.
#[cfg(unix)]
fn fake_docker(cases: &str) -> (Arc<App>, std::path::PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!(
        "bilskirnir-remove-{}",
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let manager = json!([{
        "Id": "manager",
        "Config": {"Labels": {"bilskirnir.manager": "true"}},
        "Mounts": [{"Type": "volume", "Name": "bilskirnir-data", "Destination": "/data"}],
    }]);
    let script = "#!/bin/sh\ncd '@DIR@'\necho \"$*\" >> calls\ncase \"$*\" in\n  'container inspect bilskirnir-manager') echo '@MANAGER@' ;;\n@CASES@\n  *) exit 1 ;;\nesac\n"
        .replace("@DIR@", &dir.display().to_string())
        .replace("@MANAGER@", &manager.to_string())
        .replace("@CASES@", cases);
    let docker = dir.join("docker");
    std::fs::write(&docker, script).unwrap();
    std::fs::set_permissions(&docker, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut app = crate::tests::app(false);
    let config = &mut Arc::get_mut(&mut app).unwrap().config;
    config.docker_bin = docker.display().to_string();
    config.manager = Some("bilskirnir-manager".into());
    (app, dir)
}

#[cfg(unix)]
fn calls(dir: &std::path::Path) -> String {
    std::fs::read_to_string(dir.join("calls")).unwrap_or_default()
}

/// Volumes that exist, each inspected by name, and listed for a failed inspect to mean absent.
#[cfg(unix)]
const VOLUMES: &[&str] = &[
    "bilskirnir-data",
    "orphan",
    "busy",
    "projects",
    "bilskirnir-b-cache",
    "import",
];

#[cfg(unix)]
fn cases(extra: &str) -> String {
    let mut cases = VOLUMES
        .iter()
        .map(|name| format!("  'volume inspect {name}') echo '[{{\"Name\":\"{name}\"}}]' ;;\n"))
        .collect::<String>();
    cases.push_str(&format!(
        "  'volume ls --format {{{{.Name}}}}') printf '{}\\n' ;;\n",
        VOLUMES.join("\\n")
    ));
    cases.push_str(extra);
    cases
}

#[cfg(unix)]
#[tokio::test]
async fn only_a_volume_nothing_mounts_is_removed() {
    let (app, dir) = fake_docker(&cases(
        "  'volume rm orphan') echo orphan ;;\n  'volume rm import') echo import ;;\n  'volume rm busy') echo 'Error response from daemon: remove busy: volume is in use - [c0ffee]' >&2; exit 1 ;;",
    ));
    app.db
        .put(CONTAINERS, "a", &container("a", &["projects"], &[]))
        .unwrap();
    app.db
        .put(
            CONTAINERS,
            "b",
            &container("b", &[], &["bilskirnir-b-cache"]),
        )
        .unwrap();

    // Refused before Docker is asked to remove anything.
    for (name, status, message) in [
        (
            "bilskirnir-data",
            StatusCode::FORBIDDEN,
            "This is the manager's own volume; it cannot be deleted",
        ),
        ("absent", StatusCode::NOT_FOUND, "Record not found"),
        (
            "projects",
            StatusCode::CONFLICT,
            "\"Desktop a\" mounts this volume; remove it from that container's volumes, or delete the container, first",
        ),
        (
            "bilskirnir-b-cache",
            StatusCode::CONFLICT,
            "\"Desktop b\" mounts this volume for its compose; delete that container first",
        ),
    ] {
        let (answered, body) = answer(app.clone(), name).await;
        assert_eq!((answered, body["error"].as_str()), (status, Some(message)));
    }
    assert!(!calls(&dir).contains("volume rm"), "{}", calls(&dir));

    // Docker refuses one another container uses; the refusal stays a conflict.
    let (status, body) = answer(app.clone(), "busy").await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(
        body["error"].as_str().unwrap().contains("volume is in use"),
        "{body}"
    );

    let (status, body) = answer(app.clone(), "orphan").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body, json!({"name": "orphan", "deleted": true}));
    // The import route's path is a volume name too.
    let (status, body) = answer(app.clone(), "import").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body, json!({"name": "import", "deleted": true}));
    let removals = calls(&dir)
        .lines()
        .filter(|call| call.starts_with("volume rm") || call.contains("prune"))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    assert_eq!(
        removals,
        ["volume rm busy", "volume rm orphan", "volume rm import"]
    );
    // The records are untouched.
    assert_eq!(app.db.list::<Container>(CONTAINERS).unwrap().len(), 2);
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn a_manager_outside_docker_owns_no_volume() {
    let (mut app, dir) = fake_docker(&cases("  'volume rm orphan') echo orphan ;;"));
    Arc::get_mut(&mut app).unwrap().config.manager = None;
    let (status, body) = answer(app, "orphan").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(calls(&dir), "volume inspect orphan\nvolume rm orphan\n");
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn a_volume_the_filter_leaves_out_is_missing() {
    let (mut app, dir) = fake_docker(
        "  'volume inspect outside') echo '[{\"Name\":\"outside\",\"Labels\":{}}]' ;;\n  'volume inspect kept') echo '[{\"Name\":\"kept\",\"Labels\":{\"bilskirnir.managed\":\"true\"}}]' ;;\n  'volume rm kept') echo kept ;;",
    );
    Arc::get_mut(&mut app).unwrap().config.volume_filter =
        crate::config::volume_filter(None).unwrap();
    // Made outside Bilskirnir, so the default filter hides it: deleting it fails as opening it does.
    let (status, body) = answer(app.clone(), "outside").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    let (status, body) = answer(app, "kept").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let removals = calls(&dir)
        .lines()
        .filter(|call| call.starts_with("volume rm"))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    assert_eq!(removals, ["volume rm kept"]);
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn a_delete_waits_for_the_volumes_lock() {
    let (app, dir) = fake_docker(&cases("  'volume rm orphan') echo orphan ;;"));
    let lock = app.resource_lock("volume:orphan");
    let guard = lock.lock().await;
    // An upload or an extract into the volume holds this lock until it ends.
    let waiting = tokio::time::timeout(
        std::time::Duration::from_millis(300),
        remove(State(app.clone()), Path("orphan".into())),
    )
    .await;
    assert!(waiting.is_err(), "the delete did not wait for the lock");
    assert!(!calls(&dir).contains("volume"), "{}", calls(&dir));
    drop(guard);
    let deleted = remove(State(app.clone()), Path("orphan".into()))
        .await
        .unwrap()
        .0;
    assert_eq!(deleted["deleted"], true);
    std::fs::remove_dir_all(dir).unwrap();
}
