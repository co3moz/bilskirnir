//! Pins down which settings change in place and which recreate the container, how a partial
//! request becomes a full set of settings, and that a recreated container gets every setting as
//! a creation flag, from the commit that carries its files.
use super::*;

fn spec() -> EnvironmentSpec {
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
    let old = EnvironmentSpec {
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
        error.contains("Unknown environment setting: privileged"),
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
    let new = EnvironmentSpec {
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
    let renamed = EnvironmentSpec {
        name: "Other".into(),
        ..new.clone()
    };
    assert_eq!(update_args(&new, &renamed), None);
}
