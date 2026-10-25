//! Pins down what the backup routes promise as a whole: one backup or restore at a time, a file
//! name that sorts by date, routes the API guide lists and no MCP tool that reaches them. The
//! records the other tests of this module share are built here.
use super::*;
use crate::{
    compose::Pinned,
    desktop_mcp::Endpoint,
    model::{Container, ImageRecord},
};
use serde_json::json;

/// A container record as the manager saves one, mounting `volumes` at folders of its home.
pub(super) fn container(id: &str, name: &str, image: &str, volumes: &[&str]) -> Container {
    serde_json::from_value(json!({
        "id": id,
        "container": format!("bilskirnir-{id}"),
        "private_network": format!("bilskirnir-{id}-private"),
        "egress_network": format!("bilskirnir-{id}-egress"),
        "source_image_id": format!("sha256:source-of-{id}"),
        "created_at": 10,
        "spec": {
            "name": name,
            "image": image,
            "hostname": format!("host-{id}"),
            "mac_address": "02:42:ac:11:00:02",
            "machine_id": "0123456789abcdef0123456789abcdef",
            "mounts": volumes.iter().enumerate().map(|(i, volume)| json!({
                "volume": volume, "target": format!("/home/desktop/v{i}"),
            })).collect::<Vec<_>>(),
        },
        "setup": {"status": "completed", "image": "img", "parameters": {"EMAIL": "a@example.com"}, "secrets": ["PASSWORD"]},
    }))
    .unwrap()
}

/// A saved image record of `kind`.
pub(super) fn image(id: &str, name: &str, kind: &str) -> ImageRecord {
    serde_json::from_value(json!({
        "id": id,
        "name": name,
        "image_id": format!("sha256:image-{id}"),
        "reference": format!("bilskirnir/{kind}:{id}"),
        "parent_image_id": null,
        "source_container": null,
        "kind": kind,
        "created_at": 5,
        "description": "",
    }))
    .unwrap()
}

/// The secret value the fixtures' composes hold, which no file may carry without being asked.
pub(super) const SECRET: &str = "correct-horse-battery-staple";

/// A pinned compose with one ordinary and one secret value.
pub(super) fn pinned() -> Pinned {
    serde_json::from_value(json!({
        "compose": {
            "parameters": [{"name": "EMAIL"}, {"name": "PASSWORD", "secret": true}],
            "script": "echo set up",
        },
        "values": {"EMAIL": "a@example.com", "PASSWORD": SECRET},
    }))
    .unwrap()
}

/// An endpoint covering `containers`, with a token hash.
pub(super) fn endpoint(name: &str, containers: &[&str]) -> Endpoint {
    serde_json::from_value(json!({
        "name": name,
        "automatic": false,
        "grants": containers.iter().enumerate().map(|(i, id)| json!({
            "container_id": id, "alias": format!("desk-{i}"), "scopes": ["view"], "servers": "all",
        })).collect::<Vec<_>>(),
        "token_hash": "ab".repeat(32),
        "token_created_at": 3,
        "created_at": 1,
    }))
    .unwrap()
}

/// A manager whose Docker CLI is a shell script running `cases`, arms of a `case` over its
/// arguments, in a new directory of its own, where each call is appended to `calls` and the
/// manager container is `bilskirnir-manager` with its data volume `bilskirnir-data`.
#[cfg(unix)]
pub(super) fn fake_docker(cases: &str) -> (std::sync::Arc<App>, std::path::PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!(
        "bilskirnir-backup-{}",
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("manager.json"),
        json!([{
            "Id": "manager",
            "Image": "sha256:manager",
            "Config": {"Labels": {"bilskirnir.manager": "true"}},
            "Mounts": [{"Type": "volume", "Name": "bilskirnir-data", "Destination": "/data"}],
            "NetworkSettings": {"Networks": {}},
        }])
        .to_string(),
    )
    .unwrap();
    let script = format!(
        "#!/bin/sh\ncd '{}'\nprintf '%s\\n' \"$*\" >> calls\ncase \"$*\" in\n  'container inspect bilskirnir-manager') cat manager.json ;;\n{cases}\n  *) exit 1 ;;\nesac\n",
        dir.display()
    );
    let docker = dir.join("docker");
    std::fs::write(&docker, script).unwrap();
    std::fs::set_permissions(&docker, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut app = crate::tests::app(false);
    let config = &mut std::sync::Arc::get_mut(&mut app).unwrap().config;
    config.docker_bin = docker.display().to_string();
    config.manager = Some("bilskirnir-manager".into());
    config.archive_limit = 64 << 20;
    (app, dir)
}

#[cfg(unix)]
/// A running job of `kind` for the work under test to log to.
pub(super) fn job(app: &App, id: &str, kind: &str) {
    let job: crate::model::Job = serde_json::from_value(json!({
        "id": id, "kind": kind, "status": "running", "created_at": 0, "finished_at": null,
        "log": "", "result": null,
    }))
    .unwrap();
    app.db.put("job", id, &job).unwrap();
}

#[cfg(unix)]
/// A `docker save` of a one-layer image as Docker writes one, naming `name` in the places Docker
/// reads names, with `labels` on its configuration.
pub(super) fn saved_image(labels: serde_json::Value, name: &str, layer: &[u8]) -> Vec<u8> {
    use crate::archives::tar::entry;
    let config = json!({"architecture": "amd64", "os": "linux", "config": {"Labels": labels}});
    let manifest = json!([{"Config": "config.json", "RepoTags": [name], "Layers": ["layer.tar"]}]);
    let mut archive = Vec::new();
    archive.extend(entry("layer.tar", layer, 0));
    archive.extend(entry("config.json", config.to_string().as_bytes(), 0));
    archive.extend(entry("manifest.json", manifest.to_string().as_bytes(), 0));
    archive.extend(entry(
        "repositories",
        json!({name: {"latest": "x"}}).to_string().as_bytes(),
        0,
    ));
    archive.extend([0; 1024]);
    archive
}

/// Every call the fake Docker CLI received.
#[cfg(unix)]
pub(super) fn calls(dir: &std::path::Path) -> String {
    std::fs::read_to_string(dir.join("calls")).unwrap_or_default()
}

#[test]
fn a_backup_file_is_named_by_its_time_in_utc() {
    assert_eq!(file_name(0), "bilskirnir-backup-19700101-000000.tar.gz");
    // 2026-10-22 13:45:06 UTC, and a leap day.
    assert_eq!(
        file_name(1_792_676_706),
        "bilskirnir-backup-20261022-134506.tar.gz"
    );
    assert_eq!(
        file_name(951_782_400),
        "bilskirnir-backup-20000229-000000.tar.gz"
    );
}

#[test]
fn only_one_backup_or_restore_runs_at_a_time() {
    let app = crate::tests::app(false);
    let held = installation(&app).unwrap();
    let refused = installation(&app).unwrap_err();
    assert_eq!(refused.0, StatusCode::CONFLICT);
    drop(held);
    assert!(installation(&app).is_ok());
}

/// The API guide lists every route, and MCP has no tool that backs up or restores: a restore
/// creates containers and images at once, and a backup carries the installation's secrets.
#[test]
fn the_routes_are_documented_and_out_of_mcps_reach() {
    let api = include_str!("../../../docs/API.md");
    for route in [
        "| GET | `/api/backup` |",
        "| POST | `/api/backup/check` |",
        "| POST | `/api/backup/restore` |",
    ] {
        assert!(api.contains(route), "docs/API.md must list {route}");
    }
    let mcp = include_str!("mcp.rs");
    assert!(
        !mcp.contains("backup"),
        "core/manager/src/mcp.rs must not reach backups"
    );
}
