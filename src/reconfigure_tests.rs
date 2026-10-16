//! Pins down which settings change in place and which recreate the container, how a partial
//! request becomes a full set of settings, that a recreated container gets every setting as a
//! creation flag, from the commit that carries its files, and that a change mounting the
//! manager's own volume is refused before the desktop stops.
use super::*;

fn spec() -> ContainerSpec {
    serde_json::from_value(
        json!({"name": "Work", "image": "ghcr.io/co3moz/bilskirnir/ubuntu-xfce:24.04"}),
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
    let error = merged_spec(
        &current,
        &request(json!({"image": "bilskirnir/custom:other"})),
    )
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
        container: "wsld-e".into(),
        private_network: "wsld-e-private".into(),
        egress_network: "wsld-e-egress".into(),
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
        container: "wsld-edit".into(),
        private_network: "wsld-edit-private".into(),
        egress_network: "wsld-edit-egress".into(),
        source_image_id: "sha256:source".into(),
        created_at: 0,
        spec: spec(),
        last_error: None,
        desktop: None,
        working_image: None,
        setup: None,
    };
    app.db.put(CONTAINERS, "edit", &env).unwrap();
    let mounts = json!({"mounts": [{"volume": "bilskirnir-data", "target": "/workspace"}]});
    let error = app
        .update_container("edit", request(mounts), "settings-job")
        .await
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "bilskirnir-data is the manager's own volume and cannot be mounted in a desktop"
    );
    // Refused before the desktop is stopped or the settings are saved.
    assert_eq!(
        std::fs::read_to_string(&calls).unwrap(),
        "container inspect bilskirnir-manager\n"
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
        "mounts": [{"volume": "bilskirnir-data", "target": "/opt/data", "readonly": false}],
    }))
    .unwrap();
    let env = Container {
        id: "composed".into(),
        container: "wsld-composed".into(),
        private_network: "wsld-composed-private".into(),
        egress_network: "wsld-composed-egress".into(),
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
        "bilskirnir-data is the manager's own volume and cannot be mounted in a desktop"
    );
    assert_eq!(
        std::fs::read_to_string(&calls).unwrap(),
        "container inspect bilskirnir-manager\n"
    );
    let saved: Container = app.db.get(CONTAINERS, "composed").unwrap();
    assert_eq!(saved.spec, env.spec);
    std::fs::remove_dir_all(calls.parent().unwrap()).unwrap();
}
