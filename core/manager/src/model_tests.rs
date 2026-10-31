//! Pins down what a container's settings may name: resources and mounts that cannot break the
//! desktop's own folders, image references that cannot pass an option to Docker, records saved
//! by older releases that still read, and a log limit the dashboard's copy follows.
use super::*;
use crate::test_support::spec;

#[test]
fn resource_and_mount_validation() {
    let mut s = spec();
    assert!(s.validate().is_ok());
    s.cpus = Some(f64::INFINITY);
    assert!(s.validate().is_err());
    s.cpus = None;
    s.resolution = "0x1080".into();
    assert!(s.validate().is_err());
    s.resolution = "1920x1080".into();
    s.mounts.push(Mount {
        volume: "/var/run/docker.sock".into(),
        target: "/shared".into(),
        readonly: false,
    });
    assert!(s.validate().is_err());
    s.mounts[0].volume = "project-data".into();
    s.mounts[0].target = "/proc".into();
    assert!(s.validate().is_err());
    s.mounts[0].target = "/home/desktop/../root".into();
    assert!(s.validate().is_err());
    s.mounts[0].target = "/workspace/project".into();
    assert!(s.validate().is_ok());
    s.mounts.push(s.mounts[0].clone());
    assert!(s.validate().is_err());
}

#[test]
fn mount_aliases_and_readonly_home_cannot_break_desktop_infrastructure() {
    let mut s = spec();
    s.mounts.push(Mount {
        volume: "project-data".into(),
        target: String::new(),
        readonly: false,
    });
    for target in [
        "//etc",
        "/./etc",
        "/workspace/../etc",
        "/workspace//data",
        "/lib64",
        "/var/run",
        "/var",
        "/home",
        "/opt/VirtualGL",
        "/etc/",
    ] {
        s.mounts[0].target = target.into();
        assert!(s.validate().is_err(), "{target}");
    }
    s.mounts[0].target = "/opt/game".into();
    assert!(s.validate().is_ok());
    s.mounts[0].target = "/home/desktop".into();
    s.mounts[0].readonly = true;
    assert!(s.validate().is_err());
    s.mounts[0].readonly = false;
    assert!(s.validate().is_ok());
    s.name = "\u{00c7}".repeat(100);
    assert!(s.validate().is_ok());
}

#[test]
fn old_container_records_remain_readable_without_desktop_metadata() {
    let value = serde_json::json!({"id":"old","container":"thoric-old","private_network":"private","egress_network":"egress","source_image_id":"sha256:old","created_at":1,"spec":{"name":"Old XFCE","image":"ghcr.io/co3moz/thoric/ubuntu-xfce:24.04"}});
    let env: Container = serde_json::from_value(value).unwrap();
    assert!(env.desktop.is_none());
    assert_eq!(env.source_image_id, "sha256:old");
}

#[test]
fn records_saved_before_containers_were_renamed_keep_their_container() {
    // An installation keeps its database across the upgrade that renamed environments.
    let image: ImageRecord = serde_json::from_value(serde_json::json!({"id":"snap","name":"Snapshot","image_id":"sha256:snap","reference":"thoric/snapshot:snap","parent_image_id":"sha256:base","source_environment":"env","kind":"snapshot","created_at":1,"description":""})).unwrap();
    assert_eq!(image.source_container.as_deref(), Some("env"));
    assert!(serde_json::to_value(&image).unwrap()["source_container"] == "env");
    let application: Application = serde_json::from_value(serde_json::json!({"id":"app","environment_id":"env","name":"xlogo","command":"xlogo","args":[],"cwd":"/home/desktop","gpu":false,"created_at":1})).unwrap();
    assert_eq!(application.container_id, "env");
    let terminal: Terminal = serde_json::from_value(serde_json::json!({"id":"t","environment_id":"env","root":false,"cwd":"/home/desktop","created_at":1})).unwrap();
    assert_eq!(terminal.container_id, "env");
    let recording: Recording = serde_json::from_value(
        serde_json::json!({"id":"r","environment_id":"env","name":"run","fps":2,"created_at":1}),
    )
    .unwrap();
    assert_eq!(recording.container_id, "env");
    // Container records stay under the kind they were saved with.
    assert_eq!(CONTAINERS, "environment");
}

#[test]
fn command_option_injection_is_rejected() {
    for bad in [
        "--privileged",
        "ubuntu; echo secret",
        "a\nb",
        "a,b",
        "",
        "$(id)",
    ] {
        assert!(image_ref(bad).is_err());
    }
    for valid in [
        "ubuntu:24.04",
        "registry.local:5000/me/desktop:latest",
        "sha256:abcdef",
        "repo@sha256:abcdef",
    ] {
        assert!(image_ref(valid).is_ok());
    }
}

#[test]
fn the_job_drawer_keeps_as_much_of_a_log_as_the_manager_does() {
    // core/web/src/lib/job-log.ts joins the parts of a log it reads and trims them to its own
    // copy of LOG_LIMIT, so a changed limit here must reach it.
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("core/web/src/lib/job-log.ts");
    let source = std::fs::read_to_string(path).unwrap();
    let copy = source
        .lines()
        .find_map(|line| line.strip_prefix("export const LOG_LIMIT = "))
        .expect("core/web/src/lib/job-log.ts must export LOG_LIMIT");
    assert_eq!(
        copy.trim().replace('_', "").parse::<usize>().unwrap(),
        LOG_LIMIT,
        "LOG_LIMIT in core/web/src/lib/job-log.ts must equal the manager's"
    );
}
