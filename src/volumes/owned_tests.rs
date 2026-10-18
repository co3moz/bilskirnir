//! Pins down the volumes created for one container: their labels name the container and the
//! parameter, a listing reports that owner or that it was deleted, and deleting the container
//! removes only those, never a volume another container mounts, and only when asked.
use super::*;
#[cfg(unix)]
use crate::model::CONTAINERS;
use crate::model::Mount;

fn container(id: &str, mounts: &[(&str, &str)]) -> Container {
    let mut spec: crate::model::ContainerSpec =
        serde_json::from_value(json!({"name": format!("Desktop {id}"), "image": "sha256:ws"}))
            .unwrap();
    spec.mounts = mounts
        .iter()
        .map(|(volume, target)| Mount {
            volume: volume.to_string(),
            target: target.to_string(),
            readonly: false,
        })
        .collect();
    Container {
        id: id.into(),
        container: format!("wsld-{id}"),
        private_network: format!("wsld-{id}-private"),
        egress_network: format!("wsld-{id}-egress"),
        source_image_id: "sha256:ws".into(),
        created_at: 0,
        spec,
        last_error: None,
        desktop: None,
        working_image: None,
        setup: None,
    }
}

#[test]
fn a_volume_created_for_a_container_carries_its_owner() {
    let owner = owner_labels("e", "UNITY_WORKSPACE");
    let args = |filter| crate::volumes::create_args("wsld-e-unity-workspace", filter, &owner);
    assert_eq!(
        args(Some("io.wsl-desktop.managed=true")).join(" "),
        "volume create --label io.wsl-desktop.managed=true --label io.wsl-desktop.environment=e --label io.wsl-desktop.parameter=UNITY_WORKSPACE wsld-e-unity-workspace"
    );
    // It is created like any other volume, so another VOLUME_FILTER's label is on it too.
    assert_eq!(
        args(Some("team=unity")).join(" "),
        "volume create --label io.wsl-desktop.managed=true --label team=unity --label io.wsl-desktop.environment=e --label io.wsl-desktop.parameter=UNITY_WORKSPACE wsld-e-unity-workspace"
    );
    // The owner label is the one the container carries.
    let desktop = crate::tests::app(false).container_args(&container("e", &[]));
    assert!(desktop.contains(&format!("{OWNER}=e")), "{desktop:?}");
    let labelled = |labels: Value| json!({"Name": "wsld-e-unity-workspace", "Labels": labels});
    assert!(owned_by(
        &labelled(json!({LABEL: "true", OWNER: "e", PARAMETER: "UNITY_WORKSPACE"})),
        "e"
    ));
    assert!(!owned_by(
        &labelled(json!({LABEL: "true", OWNER: "f"})),
        "e"
    ));
    assert!(!owned_by(&labelled(json!({OWNER: "e"})), "e"));
    assert!(!owned_by(&labelled(Value::Null), "e"));
}

#[test]
fn a_listing_names_the_owner_or_says_it_was_deleted() {
    let line =
        |name: &str, labels: &str| json!({"Name": name, "Driver": "local", "Labels": labels});
    let mut volumes = vec![
        line(
            "wsld-e-unity-workspace",
            "io.wsl-desktop.environment=e,io.wsl-desktop.managed=true,io.wsl-desktop.parameter=UNITY_WORKSPACE",
        ),
        line(
            "wsld-gone-cache",
            "io.wsl-desktop.parameter=CACHE,io.wsl-desktop.managed=true,io.wsl-desktop.environment=gone",
        ),
        // Created by the manager, but for no container, or by someone else entirely.
        line("projects", "io.wsl-desktop.managed=true"),
        line("foreign", "io.wsl-desktop.environment=e"),
        line("plain", ""),
        json!({"Name": "unlabelled"}),
    ];
    mark_owners(&mut volumes, &[container("e", &[])]);
    assert_eq!(
        volumes[0]["owner"],
        json!({"container_id": "e", "parameter": "UNITY_WORKSPACE", "name": "Desktop e", "deleted": false})
    );
    assert_eq!(
        volumes[1]["owner"],
        json!({"container_id": "gone", "parameter": "CACHE", "name": null, "deleted": true})
    );
    for volume in &volumes[2..] {
        assert_eq!(volume["owner"], Value::Null, "{volume}");
    }
    // A label value with a comma in it spoils no other label.
    let mut odd = vec![line(
        "wsld-e-x",
        "note=a,b,io.wsl-desktop.managed=true,io.wsl-desktop.environment=e",
    )];
    mark_owners(&mut odd, &[container("e", &[])]);
    assert_eq!(odd[0]["owner"]["container_id"], "e");
    assert_eq!(odd[0]["owner"]["parameter"], Value::Null);
}

#[test]
fn a_volume_another_container_mounts_stays() {
    let mut composed = container("c", &[]);
    composed.setup = Some(
        serde_json::from_value(json!({
            "status": "completed", "image": "img", "parameters": {}, "secrets": [],
            "mounts": [{"volume": "wsld-e-cache", "target": "/cache", "readonly": false}],
        }))
        .unwrap(),
    );
    let others = [
        container("b", &[("wsld-e-shared", "/shared")]),
        composed,
        container("d", &[("projects", "/projects")]),
    ];
    let owned = ["wsld-e-cache", "wsld-e-shared", "wsld-e-workspace"].map(str::to_owned);
    let (free, shared) = removable(owned.to_vec(), &others);
    assert_eq!(free, ["wsld-e-workspace"]);
    let kept = shared
        .iter()
        .map(|(name, other)| (name.as_str(), other.id.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(kept, [("wsld-e-cache", "c"), ("wsld-e-shared", "b")]);
}

/// A manager whose Docker CLI is a shell script answering `cases`, the arms of a `case` over
/// its arguments, with any other call failing. Each call is appended to `calls` in its
/// directory, where the script also runs.
#[cfg(unix)]
fn fake_docker(cases: &str) -> (std::sync::Arc<App>, std::path::PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!(
        "bilskirnir-owned-{}",
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let manager = json!([{
        "Id": "manager",
        "Config": {"Labels": {"io.wsl-desktop.manager": "true"}},
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
    let config = &mut std::sync::Arc::get_mut(&mut app).unwrap().config;
    config.docker_bin = docker.display().to_string();
    config.manager = Some("bilskirnir-manager".into());
    (app, dir)
}

#[cfg(unix)]
fn job(app: &App, id: &str) {
    let job = crate::model::Job {
        id: id.into(),
        kind: "test".into(),
        resource: None,
        status: "running".into(),
        created_at: 0,
        finished_at: None,
        log: String::new(),
        result: None,
        progress: None,
        stages: Vec::new(),
    };
    app.db.put("job", id, &job).unwrap();
}

#[cfg(unix)]
fn calls(dir: &std::path::Path) -> String {
    std::fs::read_to_string(dir.join("calls")).unwrap_or_default()
}

#[cfg(unix)]
fn volume_json(name: &str, owner: &str) -> String {
    json!([{"Name": name, "Labels": {LABEL: "true", OWNER: owner, PARAMETER: "UNITY_WORKSPACE"}}])
        .to_string()
}

#[cfg(unix)]
#[tokio::test]
async fn the_creation_job_makes_the_volume_before_the_record_and_a_retry_never_remakes_it() {
    let volume = "wsld-f00d-unity-workspace";
    let image =
        json!([{"Id": "sha256:ws", "Config": {"Labels": {"io.wsl-desktop.desktop": "true"}}}]);
    let cases = format!(
        "  'image inspect sha256:ws') echo '{image}' ;;\n  'volume inspect {volume}') [ -f made ] && echo '{own}' || exit 1 ;;\n  'volume inspect wsld-cafe-unity-workspace') echo '{foreign}' ;;\n  'volume create '*' {volume}') touch made; echo {volume} ;;\n  'volume ls --format {{{{.Name}}}}') if [ -f made ]; then echo {volume}; fi ;;\n  'ps -a --format {{{{.Names}}}}'|'network ls --format {{{{.Name}}}}') ;;",
        own = volume_json(volume, "f00d"),
        foreign = volume_json("wsld-cafe-unity-workspace", "someone-else"),
    );
    let (app, dir) = fake_docker(&cases);
    let compose: crate::compose::Compose = serde_json::from_value(json!({
        "parameters": [{"name": "UNITY_WORKSPACE", "type": "volume", "path": "/home/desktop/Projects", "required": true, "new_volume": true}],
        "script": "true",
    }))
    .unwrap();
    let record = crate::model::ImageRecord {
        id: "ws".into(),
        name: "Workspace".into(),
        image_id: "sha256:ws".into(),
        reference: "bilskirnir/custom:ws".into(),
        parent_image_id: None,
        source_container: None,
        kind: "build".into(),
        created_at: 0,
        description: String::new(),
        desktop: None,
        build: None,
        compose: Some(compose),
        icon: None,
        template_id: None,
    };
    app.db.put("image", "ws", &record).unwrap();
    job(&app, "create-job");
    let spec = container("f00d", &[]).spec;

    // A volume of that name that is not this container's own stops the job, which saves nothing.
    let refused = app
        .create_container("cafe", spec.clone(), Default::default(), "create-job")
        .await
        .unwrap_err()
        .to_string();
    assert!(
        refused.contains("choose a volume for UNITY_WORKSPACE"),
        "{refused}"
    );
    assert!(app.db.list::<Container>(CONTAINERS).unwrap().is_empty());
    assert!(!calls(&dir).contains("volume create"), "{}", calls(&dir));

    // Provisioning stops at the networks, once the volume and the record are there.
    let failed = app
        .create_container("f00d", spec, Default::default(), "create-job")
        .await
        .unwrap_err();
    assert!(failed.to_string().contains("Docker"), "{failed:#}");
    assert!(
        calls(&dir).contains(&format!(
            "volume create --label io.wsl-desktop.managed=true --label io.wsl-desktop.environment=f00d --label io.wsl-desktop.parameter=UNITY_WORKSPACE {volume}\n"
        )),
        "{}",
        calls(&dir)
    );
    let saved: Container = app.db.get(CONTAINERS, "f00d").unwrap();
    assert!(saved.last_error.is_some());
    assert_eq!(
        saved.compose_mounts(),
        [Mount {
            volume: volume.into(),
            target: "/home/desktop/Projects".into(),
            readonly: false
        }]
    );
    let log = app
        .db
        .get::<crate::model::Job>("job", "create-job")
        .unwrap()
        .log;
    assert!(
        log.contains(&format!("Created the volume {volume} for UNITY_WORKSPACE.")),
        "{log}"
    );

    // Once the volume is gone, a retry stops instead of making an empty one in its place.
    std::fs::remove_file(dir.join("made")).unwrap();
    std::fs::remove_file(dir.join("calls")).unwrap();
    job(&app, "retry-job");
    let missing = app
        .retry_container("f00d", "retry-job")
        .await
        .unwrap_err()
        .to_string();
    assert!(
        missing.contains(&format!("Configured volume {volume} is missing")),
        "{missing}"
    );
    assert!(!calls(&dir).contains("create"), "{}", calls(&dir));
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn deleting_a_container_removes_its_own_volumes_only_when_asked() {
    let listing = "volume ls --filter label=io.wsl-desktop.managed=true --filter label=io.wsl-desktop.environment=e --format {{.Name}}";
    let cases = format!(
        "  'ps -a --format {{{{.Names}}}}'|'network ls --format {{{{.Name}}}}') ;;\n  '{listing}') printf 'wsld-e-workspace\\nwsld-e-cache\\nwsld-e-shared\\nspoofed\\nbilskirnir-data\\n' ;;\n  'volume inspect wsld-e-workspace') echo '{workspace}' ;;\n  'volume inspect wsld-e-cache') echo '{cache}' ;;\n  'volume inspect wsld-e-shared') echo '{shared}' ;;\n  'volume inspect spoofed') echo '{spoofed}' ;;\n  'volume inspect bilskirnir-data') echo '{data}' ;;\n  'volume rm wsld-e-workspace') echo wsld-e-workspace ;;\n  'volume rm wsld-e-cache') echo 'Error response from daemon: volume is in use' >&2; exit 1 ;;",
        workspace = volume_json("wsld-e-workspace", "e"),
        cache = volume_json("wsld-e-cache", "e"),
        shared = volume_json("wsld-e-shared", "e"),
        spoofed = volume_json("spoofed", "other"),
        data = volume_json("bilskirnir-data", "e"),
    );
    let (app, dir) = fake_docker(&cases);
    let env = container("e", &[]);
    let other = container("b", &[("wsld-e-shared", "/shared")]);
    app.db.put(CONTAINERS, "b", &other).unwrap();

    // By default every volume stays, and none is even looked for.
    app.db.put(CONTAINERS, "e", &env).unwrap();
    job(&app, "keep-job");
    let kept = app.remove_container(&env, "keep-job", false).await.unwrap();
    assert_eq!(kept, RemovedVolumes::default());
    assert!(!calls(&dir).contains("volume"), "{}", calls(&dir));
    let result = crate::api::removal("e", "Desktop e", false, kept);
    assert_eq!(
        result,
        json!({"deleted": "e", "name": "Desktop e", "volumes_preserved": true, "images_preserved": true})
    );
    let log = app
        .db
        .get::<crate::model::Job>("job", "keep-job")
        .unwrap()
        .log;
    assert!(
        log.contains("Named volumes and saved images were preserved."),
        "{log}"
    );
    assert!(app.db.get::<Container>(CONTAINERS, "e").is_err());

    // Asked, only its own volumes go: not one another container mounts, one Docker refuses,
    // one whose labels name another container, or the manager's own.
    std::fs::remove_file(dir.join("calls")).unwrap();
    app.db.put(CONTAINERS, "e", &env).unwrap();
    job(&app, "delete-job");
    let removed = app
        .remove_container(&env, "delete-job", true)
        .await
        .unwrap();
    assert_eq!(
        removed,
        RemovedVolumes {
            deleted: vec!["wsld-e-workspace".into()],
            kept: vec!["wsld-e-shared".into(), "wsld-e-cache".into()],
        }
    );
    let made = calls(&dir);
    let removals = made
        .lines()
        .filter(|call| call.starts_with("volume rm"))
        .collect::<Vec<_>>();
    assert_eq!(
        removals,
        ["volume rm wsld-e-cache", "volume rm wsld-e-workspace"],
        "{made}"
    );
    let log = app
        .db
        .get::<crate::model::Job>("job", "delete-job")
        .unwrap()
        .log;
    for line in [
        "Keeping the volume wsld-e-shared: \"Desktop b\" mounts it too.",
        "Deleted the volume wsld-e-workspace.",
        "Keeping the volume wsld-e-cache: Docker: Error response from daemon: volume is in use",
        "Deleted \"Desktop e\" and the volumes created for it. Other named volumes and saved images were preserved.",
    ] {
        assert!(log.contains(line), "{line} is missing from {log}");
    }
    assert_eq!(
        crate::api::removal("e", "Desktop e", true, removed),
        json!({
            "deleted": "e", "name": "Desktop e", "volumes_preserved": false, "images_preserved": true,
            "volumes_deleted": ["wsld-e-workspace"], "volumes_kept": ["wsld-e-shared", "wsld-e-cache"],
        })
    );
    // The other container and its volume are untouched.
    assert!(app.db.get::<Container>(CONTAINERS, "b").is_ok());
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn the_volume_list_reports_owners_from_the_records() {
    let line = json!({"Name": "wsld-e-unity-workspace", "Driver": "local",
        "Labels": "io.wsl-desktop.managed=true,io.wsl-desktop.environment=e,io.wsl-desktop.parameter=UNITY_WORKSPACE"});
    let (app, dir) = fake_docker(&format!(
        "  'volume ls --format {{{{json .}}}}') echo '{line}' ;;"
    ));
    let owner = |listed: Value| listed[0]["owner"].clone();
    let listed = crate::api::volumes(axum::extract::State(app.clone()))
        .await
        .unwrap()
        .0;
    assert_eq!(owner(listed)["deleted"], true);
    app.db.put(CONTAINERS, "e", &container("e", &[])).unwrap();
    let listed = crate::api::volumes(axum::extract::State(app.clone()))
        .await
        .unwrap()
        .0;
    assert_eq!(owner(listed.clone())["name"], "Desktop e");
    // The owner comes from the records, so the listing asks Docker nothing more.
    assert!(!calls(&dir).contains("volume inspect"), "{}", calls(&dir));
    std::fs::remove_dir_all(dir).unwrap();
}
