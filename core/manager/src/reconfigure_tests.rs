//! Pins down which settings change in place and which recreate the container, how a partial
//! request becomes a full set of settings, that a recreated container gets every setting as a
//! creation flag, from the commit that carries its files, that a change mounting the manager's
//! own volume is refused before the desktop stops, and that `VOLUME_FILTER` holds only a newly
//! chosen volume to its label.
use super::*;

fn spec() -> ContainerSpec {
    serde_json::from_value(
        json!({"name": "Work", "image": "ghcr.io/co3moz/thoric/ubuntu-xfce:24.04"}),
    )
    .unwrap()
}
fn request(value: Value) -> Map<String, Value> {
    value.as_object().unwrap().clone()
}

#[test]
fn name_internet_and_limits_change_in_place_while_creation_settings_restart() {
    let old = ContainerSpec {
        cpus: Some(2.0),
        memory_mb: Some(2048),
        ..spec()
    };
    let live = merged_spec(
        &old,
        &request(json!({"name": "Renamed", "internet": false, "cpus": 4, "memory_mb": 4096})),
    )
    .unwrap();
    assert_eq!(
        classify(&old, &live),
        Changes {
            live: vec!["name", "internet", "cpus", "memory_mb"],
            restart: vec![]
        }
    );
    // Adding a limit to an unlimited desktop also works in place.
    let unlimited = spec();
    let limited = merged_spec(&unlimited, &request(json!({"cpus": 1.5}))).unwrap();
    assert_eq!(classify(&unlimited, &limited).live, ["cpus"]);
    // Docker ignores a request to remove a limit, so only a new container drops it.
    let removed = merged_spec(&old, &request(json!({"cpus": null, "memory_mb": null}))).unwrap();
    assert_eq!(
        classify(&old, &removed),
        Changes {
            live: vec![],
            restart: vec!["cpus", "memory_mb"]
        }
    );
    let created = merged_spec(
        &old,
        &request(json!({
            "resolution": "1280x720", "gpu": true, "audio": true, "microphone": true, "sudo": false,
            "gamepad": true, "shm_mb": 1024, "mounts": [{"volume": "work", "target": "/workspace"}]
        })),
    )
    .unwrap();
    assert_eq!(
        classify(&old, &created).restart,
        [
            "resolution",
            "gpu",
            "audio",
            "microphone",
            "sudo",
            "gamepad",
            "shm_mb",
            "mounts"
        ]
    );
    assert_eq!(classify(&old, &old), Changes::default());
}

#[test]
fn a_new_identity_recreates_the_container_and_none_cannot_replace_one() {
    let old = ContainerSpec {
        hostname: Some("thoric-e".into()),
        mac_address: Some("02:00:00:00:00:01".into()),
        machine_id: Some("8aed3e38ba435c50deadb0896ace189b".into()),
        ..spec()
    };
    let taken_over = merged_spec(
        &old,
        &request(json!({
            "hostname": "unity-build", "mac_address": "02-00-00-00-00-02",
            "machine_id": "0123456789ABCDEF0123456789ABCDEF"
        })),
    )
    .unwrap();
    assert_eq!(
        classify(&old, &taken_over),
        Changes {
            live: vec![],
            restart: vec!["hostname", "mac_address", "machine_id"]
        }
    );
    // The same identity written another way is no change.
    let same = merged_spec(
        &old,
        &request(json!({"mac_address": "02:00:00:00:00:01".to_uppercase()})),
    );
    assert_eq!(classify(&old, &same.unwrap()), Changes::default());
    for key in ["hostname", "mac_address", "machine_id"] {
        let error = merged_spec(&old, &request(json!({ key: null })))
            .unwrap_err()
            .to_string();
        assert!(error.contains("not removed"), "{key}: {error}");
    }
    assert!(merged_spec(&old, &request(json!({"hostname": "unity.lan"}))).is_err());
    assert!(merged_spec(&old, &request(json!({"mac_address": "ff:ff:ff:ff:ff:ff"}))).is_err());
    // A record saved before identities were kept has none until it gets a new container.
    let legacy = spec();
    let renamed = merged_spec(&legacy, &request(json!({"name": "Other"}))).unwrap();
    assert_eq!(classify(&legacy, &renamed).live, ["name"]);
    assert_eq!(renamed.hostname, None);
}

#[test]
fn every_setting_that_recreates_the_container_is_named_in_the_dashboards_warning() {
    let old = ContainerSpec {
        cpus: Some(2.0),
        memory_mb: Some(2048),
        hostname: Some("thoric-e".into()),
        mac_address: Some("02:00:00:00:00:01".into()),
        machine_id: Some("8aed3e38ba435c50deadb0896ace189b".into()),
        ..spec()
    };
    let new = ContainerSpec {
        resolution: "1280x720".into(),
        gpu: true,
        audio: true,
        microphone: true,
        sudo: false,
        gamepad: true,
        hostname: Some("unity-build".into()),
        mac_address: Some("02:00:00:00:00:02".into()),
        machine_id: Some("0123456789abcdef0123456789abcdef".into()),
        cpus: None,
        memory_mb: None,
        shm_mb: 1024,
        mounts: vec![Mount {
            volume: "work".into(),
            target: "/workspace".into(),
            readonly: false,
        }],
        ..old.clone()
    };
    let restart = classify(&old, &new).restart;
    assert_eq!(restart.len(), 13, "{restart:?}");
    let catalog = include_str!("../../web/src/locales/en.ts");
    for name in restart {
        assert!(
            catalog.contains(&format!("\"restart.field.{name}\":")),
            "core/web/src/locales/en.ts has no restart.field.{name}"
        );
    }
}

#[test]
fn reordered_mounts_are_not_a_change() {
    let a = json!({"volume": "alpha", "target": "/a"});
    let b = json!({"volume": "beta", "target": "/b"});
    let old = merged_spec(&spec(), &request(json!({"mounts": [a, b]}))).unwrap();
    let reordered = merged_spec(&old, &request(json!({"mounts": [b, a]}))).unwrap();
    assert_eq!(classify(&old, &reordered), Changes::default());
    let readonly = merged_spec(
        &old,
        &request(json!({"mounts": [a, {"volume": "beta", "target": "/b", "readonly": true}]})),
    )
    .unwrap();
    assert_eq!(classify(&old, &readonly).restart, ["mounts"]);
}

#[test]
fn requests_cannot_change_the_image_or_reach_unknown_or_invalid_settings() {
    let current = spec();
    let same_image = merged_spec(
        &current,
        &request(json!({"image": current.image, "sudo": false})),
    );
    assert!(!same_image.unwrap().sudo);
    let error = merged_spec(&current, &request(json!({"image": "thoric/custom:other"})))
        .unwrap_err()
        .to_string();
    assert!(error.contains("cannot change"), "{error}");
    let error = merged_spec(&current, &request(json!({"privileged": true})))
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("Unknown container setting: privileged"),
        "{error}"
    );
    // The result is validated as a whole, as creation does.
    let error = merged_spec(&current, &request(json!({"microphone": true})))
        .unwrap_err()
        .to_string();
    assert!(error.contains("Enable audio"), "{error}");
    assert!(merged_spec(&current, &request(json!({"resolution": "10x10"}))).is_err());
    assert!(merged_spec(&current, &request(json!({"shm_mb": "large"}))).is_err());
    assert!(
        merged_spec(
            &current,
            &request(json!({"mounts": [{"volume": "work", "target": "/etc"}]}))
        )
        .is_err()
    );
}

#[test]
fn limits_change_with_a_swap_limit_that_keeps_dockers_default() {
    let old = spec();
    let new = ContainerSpec {
        cpus: Some(2.5),
        memory_mb: Some(3072),
        ..spec()
    };
    assert_eq!(
        update_args(&old, &new).unwrap(),
        [
            "update",
            "--cpus",
            "2.5",
            "--memory",
            "3072m",
            "--memory-swap",
            "6144m"
        ]
    );
    // Only the limits that changed are sent; nothing to send is no command at all.
    let renamed = ContainerSpec {
        name: "Other".into(),
        ..new.clone()
    };
    assert_eq!(update_args(&new, &renamed), None);
}

#[test]
fn volume_settings_leave_the_compose_volumes_alone() {
    let app = crate::tests::app(false);
    let compose = Mount {
        volume: "unity-installs".into(),
        target: "/opt/unity".into(),
        readonly: false,
    };
    let setup: crate::compose::Setup = serde_json::from_value(json!({
        "status": "completed", "image": "img", "parameters": {"UNITY_HOME": "unity-installs"},
        "secrets": [], "mounts": [compose]
    }))
    .unwrap();
    let env = Container {
        id: "e".into(),
        container: "thoric-e".into(),
        private_network: "thoric-e-private".into(),
        egress_network: "thoric-e-egress".into(),
        source_image_id: "sha256:source".into(),
        created_at: 0,
        spec: spec(),
        last_error: None,
        desktop: None,
        working_image: None,
        setup: Some(setup),
    };
    let mounts = |mounts: Value| {
        app.settings_preview(&env, &request(json!({"mounts": mounts})))
            .map(|(_, changes)| changes)
            .map_err(|e| e.to_string())
    };
    let projects = json!([{"volume": "projects", "target": "/home/desktop/Projects"}]);
    assert_eq!(mounts(projects).unwrap().restart, ["mounts"]);
    // Another volume at the compose's path, or the compose's volume at another path.
    assert!(
        mounts(json!([{"volume": "projects", "target": "/opt/unity"}]))
            .unwrap_err()
            .contains("/opt/unity")
    );
    assert!(
        mounts(json!([{"volume": "unity-installs", "target": "/srv/unity"}]))
            .unwrap_err()
            .contains("unity-installs")
    );
}

#[cfg(unix)]
#[tokio::test]
async fn a_settings_change_cannot_mount_the_managers_own_volume() {
    let mut app = crate::tests::app(false);
    let calls = crate::tests::manager_docker(&mut app);
    let env = Container {
        id: "edit".into(),
        container: "thoric-edit".into(),
        private_network: "thoric-edit-private".into(),
        egress_network: "thoric-edit-egress".into(),
        source_image_id: "sha256:source".into(),
        created_at: 0,
        spec: spec(),
        last_error: None,
        desktop: None,
        working_image: None,
        setup: None,
    };
    app.db.put(CONTAINERS, "edit", &env).unwrap();
    let mounts = json!({"mounts": [{"volume": "thoric-data", "target": "/workspace"}]});
    let error = app
        .update_container("edit", request(mounts), "settings-job")
        .await
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "thoric-data is the manager's own volume and cannot be mounted in a desktop"
    );
    // Refused before the desktop is stopped or the settings are saved.
    assert_eq!(
        std::fs::read_to_string(&calls).unwrap(),
        "container inspect thoric-manager\n"
    );
    let saved: Container = app.db.get(CONTAINERS, "edit").unwrap();
    assert_eq!(saved.spec, env.spec);
    std::fs::remove_dir_all(calls.parent().unwrap()).unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn a_recreated_container_cannot_keep_a_compose_mount_of_the_managers_own_volume() {
    let mut app = crate::tests::app(false);
    let calls = crate::tests::manager_docker(&mut app);
    // A compose volume parameter that named the data volume before creation refused it.
    let setup: crate::compose::Setup = serde_json::from_value(json!({
        "status": "completed", "image": "img", "parameters": {"DATA": "/opt/data"},
        "secrets": [],
        "mounts": [{"volume": "thoric-data", "target": "/opt/data", "readonly": false}],
    }))
    .unwrap();
    let env = Container {
        id: "composed".into(),
        container: "thoric-composed".into(),
        private_network: "thoric-composed-private".into(),
        egress_network: "thoric-composed-egress".into(),
        source_image_id: "sha256:source".into(),
        created_at: 0,
        spec: spec(),
        last_error: None,
        desktop: None,
        working_image: None,
        setup: Some(setup),
    };
    app.db.put(CONTAINERS, "composed", &env).unwrap();
    // Shared memory is fixed at creation, so the change recreates the container.
    let error = app
        .update_container("composed", request(json!({"shm_mb": 1024})), "settings-job")
        .await
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "thoric-data is the manager's own volume and cannot be mounted in a desktop"
    );
    assert_eq!(
        std::fs::read_to_string(&calls).unwrap(),
        "container inspect thoric-manager\n"
    );
    let saved: Container = app.db.get(CONTAINERS, "composed").unwrap();
    assert_eq!(saved.spec, env.spec);
    std::fs::remove_dir_all(calls.parent().unwrap()).unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn a_settings_change_keeps_the_volumes_a_container_mounts_and_filters_new_ones() {
    let mut app = crate::tests::app(false);
    let calls = crate::tests::manager_docker(&mut app);
    std::sync::Arc::get_mut(&mut app)
        .unwrap()
        .config
        .volume_filter = crate::config::volume_filter(None).unwrap();
    let job = crate::model::Job {
        id: "settings-job".into(),
        kind: "settings".into(),
        resource: None,
        status: "running".into(),
        created_at: 0,
        finished_at: None,
        log: String::new(),
        result: None,
        progress: None,
        stages: Vec::new(),
        name: None,
    };
    app.db.put("job", "settings-job", &job).unwrap();
    let container = |id: &str, mounts: Value| {
        let mut spec = spec();
        spec.mounts = serde_json::from_value(mounts).unwrap();
        Container {
            id: id.into(),
            container: format!("thoric-{id}"),
            private_network: format!("thoric-{id}-private"),
            egress_network: format!("thoric-{id}-egress"),
            source_image_id: "sha256:source".into(),
            created_at: 0,
            spec,
            last_error: None,
            desktop: None,
            working_image: None,
            setup: None,
        }
    };
    // `outside` carries no label: a container that does not mount it cannot start to.
    let fresh = container("fresh", json!([]));
    app.db.put(CONTAINERS, "fresh", &fresh).unwrap();
    let outside = json!({"mounts": [{"volume": "outside", "target": "/workspace"}]});
    let error = app
        .update_container("fresh", request(outside), "settings-job")
        .await
        .unwrap_err();
    assert_eq!(error.to_string(), "Volume not found: outside");
    let saved: Container = app.db.get(CONTAINERS, "fresh").unwrap();
    assert_eq!(saved.spec, fresh.spec);
    // One that mounted it before the filter keeps it, also at another path.
    let kept = container(
        "kept",
        json!([{"volume": "outside", "target": "/workspace", "readonly": false}]),
    );
    app.db.put(CONTAINERS, "kept", &kept).unwrap();
    let moved = json!({"mounts": [{"volume": "outside", "target": "/srv/outside"}]});
    app.update_container("kept", request(moved), "settings-job")
        .await
        .unwrap();
    let saved: Container = app.db.get(CONTAINERS, "kept").unwrap();
    assert_eq!(saved.spec.mounts[0].target, "/srv/outside");
    std::fs::remove_dir_all(calls.parent().unwrap()).unwrap();
}

#[test]
fn proxied_ports_change_in_place_within_their_limits() {
    let old = spec();
    assert!(old.ports.is_empty());
    let new = merged_spec(&old, &request(json!({"ports": [8090, 6400]}))).unwrap();
    assert_eq!(new.ports, [8090, 6400]);
    assert_eq!(
        classify(&old, &new),
        Changes {
            live: vec!["ports"],
            restart: vec![]
        }
    );
    let many = (1..=33).collect::<Vec<u16>>();
    for ports in [json!([0]), json!([8090, 8090]), json!([70000]), json!(many)] {
        assert!(
            merged_spec(&old, &request(json!({ "ports": ports }))).is_err(),
            "{ports}"
        );
    }
}

/// A manager whose Docker CLI is a shell script in a new directory, which it enters before
/// `commands` run with the CLI's arguments. The container `e` exists, stopped, and the manager
/// container is `thoric-manager`.
#[cfg(unix)]
fn fake_docker(commands: &str) -> (std::sync::Arc<App>, std::path::PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!(
        "thoric-reconfigure-{}",
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let docker = dir.join("docker");
    std::fs::write(
        &docker,
        format!(
            "#!/bin/sh\ncd '{}'\necho \"$*\" >> calls\n{commands}",
            dir.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&docker, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut app = crate::tests::app(false);
    let config = &mut std::sync::Arc::get_mut(&mut app).unwrap().config;
    config.docker_bin = docker.display().to_string();
    config.data = dir.join("data");
    config.manager = Some("thoric-manager".into());
    let env = Container {
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
    app.db.put(CONTAINERS, "e", &env).unwrap();
    (app, dir)
}

#[cfg(unix)]
const STOPPED: &str = r#"[{"Id":"c0ffee","Config":{"Image":"sha256:desktop","Labels":{"thoric.managed":"true","thoric.environment":"e"}},"State":{"Running":false}}]"#;

#[cfg(unix)]
#[tokio::test]
async fn the_record_names_the_commit_before_the_new_container_is_created() {
    // Creating the new container fails; whether the old one can be renamed back is scripted.
    let (app, dir) = fake_docker(
        r#"case "$*" in
  'container inspect thoric-e') cat stopped.json; exit ;;
  'container inspect '*) exit 1 ;;
  'image inspect '*) echo '[{"Id":"sha256:work","Config":{}}]'; exit ;;
  'rename c0ffee thoric-e') exit $(cat rename-status) ;;
  create*) exit 1 ;;
esac
"#,
    );
    std::fs::write(dir.join("stopped.json"), STOPPED).unwrap();
    // The stages are reported on the job, so the job record exists as a launched one would.
    let job = Job {
        id: "change".into(),
        kind: "settings".into(),
        resource: Some("container:e".into()),
        status: "running".into(),
        created_at: 1,
        finished_at: None,
        log: String::new(),
        result: None,
        progress: None,
        stages: Vec::new(),
        name: None,
    };
    app.db.put("job", "change", &job).unwrap();
    let change = |rename_status: u8| {
        std::fs::write(dir.join("rename-status"), rename_status.to_string()).unwrap();
        let app = app.clone();
        async move {
            app.update_container("e", request(json!({"resolution": "1280x720"})), "change")
                .await
                .unwrap_err()
        }
    };

    // The old container could not be renamed back, so the record keeps naming the commit the
    // files are in, with the new settings: a retry creates the container from it.
    let error = change(1).await;
    let calls = std::fs::read_to_string(dir.join("calls")).unwrap();
    let saved: Container = app.db.get(CONTAINERS, "e").unwrap();
    assert_eq!(
        saved.working_image,
        Some(WorkingImage {
            image_id: "sha256:work".into(),
            reference: "thoric/container:change".into()
        }),
        "{error:#}
{calls}"
    );
    assert_eq!(saved.spec.resolution, "1280x720");

    // Once the old container is back under its name, the record is what it was.
    let mut before: Container = app.db.get(CONTAINERS, "e").unwrap();
    before.working_image = None;
    before.spec = spec();
    app.db.put(CONTAINERS, "e", &before).unwrap();
    change(0).await;
    let restored: Container = app.db.get(CONTAINERS, "e").unwrap();
    assert_eq!(restored.working_image, None);
    assert_eq!(restored.spec, spec());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn a_retry_never_replaces_a_container_set_aside_without_its_commit_on_record() {
    // The container is gone from under its name, and the one set aside is the only copy.
    let (app, dir) = fake_docker(
        r#"case "$*" in
  'container inspect thoric-manager') echo '[{"Id":"manager","Config":{"Labels":{"thoric.manager":"true"}}}]'; exit ;;
  'container inspect thoric-e-previous') cat stopped.json; exit ;;
  'container inspect '*) exit 1 ;;
esac
"#,
    );
    std::fs::write(dir.join("stopped.json"), STOPPED).unwrap();
    let error = app.retry_container("e", "retry").await.unwrap_err();
    assert_eq!(
        error.to_string(),
        format!("thoric-e-previous {PREVIOUS_LEFT}")
    );
    let calls = std::fs::read_to_string(dir.join("calls")).unwrap();
    assert!(!calls.contains("create"), "{calls}");
    assert!(!calls.contains(" rm "), "{calls}");
    // With the commit on record, the retry goes on to create the container from it.
    let mut env: Container = app.db.get(CONTAINERS, "e").unwrap();
    env.working_image = Some(WorkingImage {
        image_id: "sha256:work".into(),
        reference: "thoric/container:change".into(),
    });
    app.db.put(CONTAINERS, "e", &env).unwrap();
    std::fs::write(dir.join("calls"), "").unwrap();
    let _ = app.retry_container("e", "retry").await;
    let calls = std::fs::read_to_string(dir.join("calls")).unwrap();
    assert!(calls.contains("image inspect sha256:work"), "{calls}");
    std::fs::remove_dir_all(&dir).unwrap();
}
