//! Pins down what bounds a desktop: the pids limit `DESKTOP_PIDS_LIMIT` sets and the `docker update`
//! that gives it to a desktop with another one, and log options that rotate only under Docker's
//! json-file driver, which another driver would refuse.
use super::*;
use crate::model::{Container, ContainerSpec};
use serde_json::json;
use std::sync::Arc;

fn desktop() -> Container {
    let spec: ContainerSpec = serde_json::from_value(
        json!({"name": "Unity", "image": "ghcr.io/co3moz/thoric/ubuntu-xfce:24.04"}),
    )
    .unwrap();
    Container {
        id: "limits".into(),
        container: "thoric-limits".into(),
        private_network: "thoric-limits-private".into(),
        egress_network: "thoric-limits-egress".into(),
        source_image_id: "sha256:source".into(),
        created_at: 0,
        spec,
        last_error: None,
        desktop: None,
        working_image: None,
        setup: None,
    }
}

/// The `docker create` arguments of a desktop on an installation with this driver and limit.
fn created(log_driver: Option<&str>, pids_limit: u64) -> String {
    let mut app = crate::tests::app(false);
    let config = &mut Arc::get_mut(&mut app).unwrap().config;
    config.log_driver = log_driver.map(str::to_owned);
    config.pids_limit = pids_limit;
    app.container_args(&desktop(), None).join(" ")
}

#[test]
fn a_desktop_is_created_with_the_pids_limit_unless_it_is_none() {
    let args = created(Some("json-file"), DEFAULT_PIDS_LIMIT);
    assert!(args.contains(" --pids-limit 16384 "), "{args}");
    let args = created(Some("json-file"), 2048);
    assert!(args.contains(" --pids-limit 2048 "), "{args}");
    let args = created(Some("json-file"), 0);
    assert!(!args.contains("--pids-limit"), "{args}");
}

#[test]
fn only_json_file_logs_get_a_size_and_a_number_of_files() {
    let args = created(Some("json-file"), DEFAULT_PIDS_LIMIT);
    assert!(
        args.contains(" --log-opt max-size=10m --log-opt max-file=3 "),
        "{args}"
    );
    // local rotates by itself, journald and syslog refuse max-size, and a driver nobody named may
    // be either.
    for driver in [Some("local"), Some("journald"), Some("syslog"), None] {
        let args = created(driver, DEFAULT_PIDS_LIMIT);
        assert!(!args.contains("--log-opt"), "{driver:?}: {args}");
        assert!(!args.contains("--log-driver"), "{driver:?}: {args}");
    }
}

#[test]
fn the_logging_driver_is_the_one_docker_info_names() {
    let info = json!({"LoggingDriver": "json-file", "Runtimes": {"runc": {}}});
    assert_eq!(log_driver(Some(&info)).as_deref(), Some("json-file"));
    assert_eq!(log_driver(Some(&json!({"LoggingDriver": ""}))), None);
    assert_eq!(log_driver(Some(&json!({}))), None);
    assert_eq!(log_driver(None), None);
}

#[test]
fn the_setting_is_none_or_a_limit_a_desktop_can_start_with() {
    assert_eq!(pids_limit(None).unwrap(), 16384);
    assert_eq!(pids_limit(Some("0")).unwrap(), 0);
    assert_eq!(pids_limit(Some("1024")).unwrap(), 1024);
    assert_eq!(pids_limit(Some("4194304")).unwrap(), 4_194_304);
    for refused in ["", "1023", "100", "4194305", "-1", "16k", " 16384", "none"] {
        let error = pids_limit(Some(refused)).unwrap_err();
        assert!(
            format!("{error:#}").contains("DESKTOP_PIDS_LIMIT must be 0 for no limit"),
            "{refused:?}: {error:#}"
        );
    }
}

#[test]
fn a_container_is_updated_only_when_its_limit_differs() {
    let inspected = |limit: Value| json!({"Id": "abc", "HostConfig": {"PidsLimit": limit}});
    let update = |limit: &str| {
        Some(
            ["update", "--pids-limit", limit, "abc"]
                .map(str::to_owned)
                .to_vec(),
        )
    };
    // A desktop created before the limit.
    assert_eq!(pids_update(&inspected(Value::Null), 16384), update("16384"));
    assert_eq!(pids_update(&inspected(json!(16384)), 16384), None);
    assert_eq!(pids_update(&inspected(json!(4096)), 16384), update("16384"));
    // No limit at all, as older Docker releases also write it.
    for none in [Value::Null, json!(0), json!(-1)] {
        assert_eq!(pids_update(&inspected(none), 0), None);
    }
    assert_eq!(pids_update(&inspected(json!(16384)), 0), update("-1"));
    // Without its ID there is nothing to update.
    assert_eq!(pids_update(&json!({"HostConfig": {}}), 16384), None);
}

/// A Docker CLI that answers `docker info` and logs every call.
#[cfg(unix)]
fn fake_docker(info: &str) -> (Arc<App>, std::path::PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("thoric-limits-{}", uuid::Uuid::new_v4().simple()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("info"), info).unwrap();
    let docker = dir.join("docker");
    let script = "#!/bin/sh\ncd 'DIR'\necho \"$*\" >> calls\n[ \"$1\" = info ] && cat info\ntrue\n";
    std::fs::write(&docker, script.replace("DIR", &dir.display().to_string())).unwrap();
    std::fs::set_permissions(&docker, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut app = crate::tests::app(false);
    Arc::get_mut(&mut app).unwrap().config.docker_bin = docker.display().to_string();
    (app, dir)
}

#[cfg(unix)]
#[tokio::test]
async fn startup_reads_the_driver_from_docker_info() {
    let (app, dir) = fake_docker(r#"{"LoggingDriver":"local","Runtimes":{"runc":{}}}"#);
    let info = docker_info(&app.config.docker_bin).await;
    assert_eq!(log_driver(info.as_ref()).as_deref(), Some("local"));
    let calls = std::fs::read_to_string(dir.join("calls")).unwrap();
    assert_eq!(calls, "info --format {{json .}}\n");
    // A daemon that does not answer with JSON names no driver, and desktops get no options.
    std::fs::write(dir.join("info"), "Cannot connect to the Docker daemon").unwrap();
    assert_eq!(docker_info(&app.config.docker_bin).await, None);
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn a_running_desktop_gets_the_limit_in_place() {
    let (app, dir) = fake_docker("{}");
    let calls = || std::fs::read_to_string(dir.join("calls")).unwrap_or_default();
    let info = |limit: Value| json!({"Id": "abc", "HostConfig": {"PidsLimit": limit}});
    app.keep_pids_limit(&info(json!(16384))).await.unwrap();
    assert_eq!(calls(), "");
    app.keep_pids_limit(&info(Value::Null)).await.unwrap();
    assert_eq!(calls(), "update --pids-limit 16384 abc\n");
    std::fs::remove_dir_all(dir).unwrap();
}
