//! Pins down deleting an image: one a container record starts from, unless another saved image
//! keeps it, a catalog image that is not downloaded and an unknown record are refused before
//! Docker is asked to remove anything; a
//! saved image goes by its own tag, or by its ID once that tag is gone, and its record goes with
//! it; an image registered from the host keeps its Docker image; Docker's refusal keeps the record
//! and its meaning; nothing is forced or pruned, and a busy image is refused instead of waited for.
use super::*;
use axum::{body::Body, http::Request};
use tower::ServiceExt;

fn container(id: &str, source: &str) -> Container {
    let spec =
        serde_json::from_value(json!({"name": format!("Desktop {id}"), "image": source})).unwrap();
    Container {
        id: id.into(),
        container: format!("bilskirnir-{id}"),
        private_network: format!("bilskirnir-{id}-private"),
        egress_network: format!("bilskirnir-{id}-egress"),
        source_image_id: source.into(),
        created_at: 0,
        spec,
        last_error: None,
        desktop: None,
        working_image: None,
        setup: None,
    }
}

fn record(id: &str, kind: &str, image_id: &str, reference: &str) -> ImageRecord {
    ImageRecord {
        id: id.into(),
        name: format!("Image {id}"),
        image_id: image_id.into(),
        reference: reference.into(),
        parent_image_id: None,
        source_container: None,
        kind: kind.into(),
        created_at: 0,
        description: String::new(),
        desktop: None,
        build: None,
        compose: None,
        icon: None,
        template_ids: Vec::new(),
    }
}

#[test]
fn the_containers_that_start_from_an_image_are_named_with_what_frees_it() {
    let containers = [
        container("a", "sha256:one"),
        container("b", "sha256:two"),
        container("c", "sha256:two"),
    ];
    let names = |image: &str| {
        started_from(image, &containers)
            .iter()
            .map(|container| container.id.as_str())
            .collect::<Vec<_>>()
    };
    assert_eq!(names("sha256:one"), ["a"]);
    assert_eq!(names("sha256:two"), ["b", "c"]);
    assert!(names("sha256:three").is_empty());
    assert_eq!(
        in_use(&started_from("sha256:one", &containers)),
        "\"Desktop a\" was created from this image and needs it to reset; delete that container first"
    );
    assert_eq!(
        in_use(&started_from("sha256:two", &containers)),
        "\"Desktop b\", \"Desktop c\" were created from this image and need it to reset; delete those containers first"
    );
}

#[test]
fn only_a_deleted_line_means_the_image_went() {
    // The image's last tag: Docker deletes the image and its layers no other image has.
    assert!(deleted_image(
        "Untagged: bilskirnir/custom:a\nDeleted: sha256:c0ffee\n"
    ));
    // Another tag, a child image or a container still holds it.
    assert!(!deleted_image("Untagged: bilskirnir/custom:a\n"));
    assert!(!deleted_image(""));
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
        status(
            "conflict: unable to delete c0ffee (must be forced) - image is referenced in multiple repositories"
        ),
        StatusCode::CONFLICT
    );
    assert_eq!(
        status(
            "conflict: unable to remove repository reference \"x\" (must force) - container 1234 is using its referenced image c0ffee"
        ),
        StatusCode::CONFLICT
    );
    assert_eq!(status("something else"), StatusCode::BAD_REQUEST);
}

#[test]
fn the_route_is_documented() {
    // Checked from here, since a change to the route is what leaves the guide behind.
    assert!(
        include_str!("../../../docs/API.md").contains("| DELETE | `/api/images/{id}` |"),
        "docs/API.md must list DELETE /api/images/{{id}}"
    );
}

fn request(id: &str) -> Request<Body> {
    Request::builder()
        .method("DELETE")
        .uri(format!("/api/images/{id}"))
        .header("host", "localhost:8080")
        .header("x-bilskirnir-request", "1")
        .body(Body::empty())
        .unwrap()
}

async fn answer(app: Arc<App>, id: &str) -> (StatusCode, Value) {
    let response = crate::router(app).oneshot(request(id)).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn an_unknown_image_is_missing() {
    // The test app's Docker CLI does not exist, so any call would fail differently.
    let (status, body) = answer(crate::tests::app(false), "c0ffee").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
}

#[tokio::test]
async fn an_agent_deletes_through_the_same_route() {
    let app = crate::tests::mcp_app();
    let call =
        |image_id: &str| json!({"name": "delete_image", "arguments": {"image_id": image_id}});
    let (_, body) = crate::tests::mcp_request(app.clone(), "tools/call", call("c0ffee")).await;
    assert_eq!(body["result"]["isError"], true, "{body}");
    assert_eq!(body["result"]["content"][0]["text"], "Record not found");
    // An import keeps its Docker image, so no Docker is needed to delete its record.
    app.db
        .put(
            "image",
            "h",
            &record("h", "import", "sha256:h", "someone/desktop:1"),
        )
        .unwrap();
    let (_, body) = crate::tests::mcp_request(app.clone(), "tools/call", call("h")).await;
    assert_eq!(body["result"]["isError"], false, "{body}");
    assert_eq!(
        body["result"]["structuredContent"],
        json!({"id": "h", "name": "Image h", "deleted": true, "image_removed": false})
    );
    assert!(app.db.list::<ImageRecord>("image").unwrap().is_empty());
}

#[tokio::test]
async fn a_busy_image_is_refused_instead_of_waited_for() {
    let app = crate::tests::app(false);
    app.db
        .put(
            "image",
            "a",
            &record("a", "build", "sha256:a", "bilskirnir/custom:a"),
        )
        .unwrap();
    // A download or a build of the image holds this lock for minutes.
    let lock = app.resource_lock("image:a");
    let guard = lock.lock().await;
    let (status, body) = answer(app.clone(), "a").await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(
        body["error"],
        "Another operation on this image is running; delete it once that finishes"
    );
    drop(guard);
    assert_eq!(app.db.list::<ImageRecord>("image").unwrap().len(), 1);
}

/// A manager whose Docker CLI is a shell script answering `cases`, the arms of a `case` over its
/// arguments, with any other call failing. Image listings answer with nothing, so an image whose
/// inspect fails is absent. Each call is appended to `calls` in its directory.
#[cfg(unix)]
fn fake_docker(cases: &str) -> (Arc<App>, std::path::PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!(
        "bilskirnir-image-removal-{}",
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let script = "#!/bin/sh\ncd '@DIR@'\necho \"$*\" >> calls\ncase \"$*\" in\n  'image ls'*) ;;\n@CASES@\n  *) exit 1 ;;\nesac\n"
        .replace("@DIR@", &dir.display().to_string())
        .replace("@CASES@", cases);
    let docker = dir.join("docker");
    std::fs::write(&docker, script).unwrap();
    std::fs::set_permissions(&docker, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut app = crate::tests::app(false);
    Arc::get_mut(&mut app).unwrap().config.docker_bin = docker.display().to_string();
    (app, dir)
}

#[cfg(unix)]
fn removals(dir: &std::path::Path) -> Vec<String> {
    std::fs::read_to_string(dir.join("calls"))
        .unwrap_or_default()
        .lines()
        .filter(|call| call.starts_with("image rm") || call.contains("prune"))
        .map(str::to_owned)
        .collect()
}

#[cfg(unix)]
fn inspected(reference: &str, id: &str) -> String {
    format!("  'image inspect {reference}') echo '[{{\"Id\":\"{id}\"}}]' ;;\n")
}

#[cfg(unix)]
#[tokio::test]
async fn a_saved_image_goes_by_its_own_tag_unless_a_container_starts_from_it() {
    let mut cases = [
        ("bilskirnir/custom:used", "sha256:used"),
        ("bilskirnir/custom:free", "sha256:free"),
        ("bilskirnir/snapshot:shared", "sha256:shared"),
        // A tag that someone moved to another image no longer names this record's.
        ("bilskirnir/snapshot:moved", "sha256:other"),
        ("bilskirnir/imported:held", "sha256:held"),
        ("bilskirnir/imported:copy", "sha256:used"),
        ("bilskirnir/custom:parent", "sha256:parent"),
        // The containerd store keeps an image a snapshot names after saying it deleted it.
        ("sha256:parent", "sha256:parent"),
    ]
    .map(|(reference, id)| inspected(reference, id))
    .concat();
    cases.push_str(concat!(
        "  'image rm bilskirnir/custom:parent') printf 'Untagged: bilskirnir/custom:parent\\nDeleted: sha256:parent\\n' ;;\n",
        "  'image rm bilskirnir/imported:copy') echo 'Untagged: bilskirnir/imported:copy' ;;\n",
        "  'image rm bilskirnir/custom:free') printf 'Untagged: bilskirnir/custom:free\\nDeleted: sha256:free\\n' ;;\n",
        "  'image rm bilskirnir/snapshot:shared') echo 'Untagged: bilskirnir/snapshot:shared' ;;\n",
        "  'image rm sha256:moved') printf 'Deleted: sha256:moved\\n' ;;\n",
        "  'image rm sha256:gone') echo 'Error response from daemon: No such image: sha256:gone' >&2; exit 1 ;;\n",
        "  'image rm bilskirnir/imported:held') echo 'Error response from daemon: conflict: unable to remove repository reference \"bilskirnir/imported:held\" (must force) - container 1234 is using its referenced image held' >&2; exit 1 ;;\n",
    ));
    let (app, dir) = fake_docker(&cases);
    for (id, kind, reference) in [
        ("used", "build", "bilskirnir/custom:used"),
        ("free", "build", "bilskirnir/custom:free"),
        ("shared", "snapshot", "bilskirnir/snapshot:shared"),
        ("moved", "snapshot", "bilskirnir/snapshot:moved"),
        ("gone", "snapshot", "bilskirnir/snapshot:gone"),
        ("held", "archive", "bilskirnir/imported:held"),
        ("host", "import", "someone/desktop:1"),
        ("parent", "build", "bilskirnir/custom:parent"),
    ] {
        let image = record(id, kind, &format!("sha256:{id}"), reference);
        app.db.put("image", id, &image).unwrap();
    }
    // An archive of the image `used`, imported on the host it came from.
    let copy = record("copy", "archive", "sha256:used", "bilskirnir/imported:copy");
    app.db.put("image", "copy", &copy).unwrap();
    app.db
        .put(CONTAINERS, "a", &container("a", "sha256:used"))
        .unwrap();
    // A container that starts from the host's image does not keep its record.
    app.db
        .put(CONTAINERS, "h", &container("h", "sha256:host"))
        .unwrap();

    // The copy's own tag goes while the record `used` keeps the container's source.
    let (status, body) = answer(app.clone(), "copy").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["image_removed"], false, "{body}");

    // Now nothing else keeps it: refused before Docker is asked to remove anything.
    let (status, body) = answer(app.clone(), "used").await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(
        body["error"],
        "\"Desktop a\" was created from this image and needs it to reset; delete that container first"
    );
    assert_eq!(removals(&dir), ["image rm bilskirnir/imported:copy"]);

    // Docker's refusal keeps the record.
    let (status, body) = answer(app.clone(), "held").await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(
        body["error"]
            .as_str()
            .unwrap()
            .contains("is using its referenced image"),
        "{body}"
    );

    for (id, removed) in [
        ("free", true),
        // Another tag names the image, so only Bilskirnir's own tag goes.
        ("shared", false),
        ("moved", true),
        // The image went already; the record is all that is left.
        ("gone", false),
        ("host", false),
        // Docker printed a deletion, but the image still inspects.
        ("parent", false),
    ] {
        let (status, body) = answer(app.clone(), id).await;
        assert_eq!(status, StatusCode::OK, "{id}: {body}");
        assert_eq!(
            body,
            json!({"id": id, "name": format!("Image {id}"), "deleted": true, "image_removed": removed})
        );
    }
    assert_eq!(
        removals(&dir),
        [
            "image rm bilskirnir/imported:copy",
            "image rm bilskirnir/imported:held",
            "image rm bilskirnir/custom:free",
            "image rm bilskirnir/snapshot:shared",
            "image rm sha256:moved",
            "image rm sha256:gone",
            "image rm bilskirnir/custom:parent",
        ]
    );
    let mut left = app
        .db
        .list::<ImageRecord>("image")
        .unwrap()
        .into_iter()
        .map(|image| image.id)
        .collect::<Vec<_>>();
    left.sort();
    assert_eq!(left, ["held", "used"]);
    assert_eq!(app.db.list::<Container>(CONTAINERS).unwrap().len(), 2);
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn a_catalog_image_loses_only_its_download() {
    let catalog = crate::catalog::catalog();
    let [xfce, gnome] = [&catalog[0], &catalog[1]];
    let mut cases = inspected(&xfce.reference, "sha256:xfce");
    cases.push_str(&format!(
        "  'image rm {}') printf 'Untagged: {}\\nDeleted: sha256:xfce\\n' ;;\n",
        xfce.reference, xfce.reference
    ));
    let (app, dir) = fake_docker(&cases);

    // Not downloaded: there is nothing to delete.
    let (status, body) = answer(app.clone(), &gnome.id).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["error"], "This catalog image is not downloaded");

    app.db
        .put(CONTAINERS, "a", &container("a", "sha256:xfce"))
        .unwrap();
    let (status, body) = answer(app.clone(), &xfce.id).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(removals(&dir).is_empty(), "{:?}", removals(&dir));

    app.db.remove(CONTAINERS, "a").unwrap();
    let (status, body) = answer(app.clone(), &xfce.id).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body,
        json!({"id": xfce.id, "name": xfce.name, "deleted": true, "image_removed": true})
    );
    assert_eq!(removals(&dir), [format!("image rm {}", xfce.reference)]);
    std::fs::remove_dir_all(dir).unwrap();
}

/// A snapshot or import tagged before its record was saved is removed at startup; every tag a
/// record names, and every other repository, stays.
#[cfg(unix)]
#[tokio::test]
async fn tags_without_a_record_are_dropped_at_startup() {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!(
        "bilskirnir-orphans-{}",
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let script = "#!/bin/sh\ncd '@DIR@'\necho \"$*\" >> calls\ncase \"$*\" in\n  'image ls --format {{.Repository}}:{{.Tag}} bilskirnir/snapshot') printf 'bilskirnir/snapshot:kept\nbilskirnir/snapshot:lost\nbilskirnir/snapshot:<none>\n' ;;\n  'image ls --format {{.Repository}}:{{.Tag}} bilskirnir/imported') printf 'bilskirnir/imported:orphan\n' ;;\n  'image rm '*) ;;\n  *) exit 1 ;;\nesac\n"
        .replace("@DIR@", &dir.display().to_string());
    let docker = dir.join("docker");
    std::fs::write(&docker, script).unwrap();
    std::fs::set_permissions(&docker, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut app = crate::tests::app(false);
    Arc::get_mut(&mut app).unwrap().config.docker_bin = docker.display().to_string();
    app.db
        .put(
            "image",
            "kept",
            &record(
                "kept",
                "snapshot",
                "sha256:kept",
                "bilskirnir/snapshot:kept",
            ),
        )
        .unwrap();
    let dropped = app.drop_orphan_tags().await.unwrap();
    assert_eq!(
        dropped,
        ["bilskirnir/snapshot:lost", "bilskirnir/imported:orphan"]
    );
    assert_eq!(
        removals(&dir),
        [
            "image rm bilskirnir/snapshot:lost",
            "image rm bilskirnir/imported:orphan"
        ]
    );
    std::fs::remove_dir_all(&dir).unwrap();
}
