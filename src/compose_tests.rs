//! Pins down what a compose accepts, how a new container's values are resolved from it, and
//! that a secret value reaches only the setup runner: never the container record, an image
//! list or a route.
use super::*;

fn parameter(name: &str) -> Parameter {
    Parameter {
        name: name.into(),
        ..Default::default()
    }
}

fn unity() -> Compose {
    Compose {
        parameters: vec![
            Parameter {
                label: "Unity email".into(),
                required: true,
                ..parameter("UNITY_EMAIL")
            },
            Parameter {
                label: "Unity password".into(),
                required: true,
                secret: true,
                ..parameter("UNITY_PASSWORD")
            },
            Parameter {
                default: "personal".into(),
                ..parameter("UNITY_LICENSE")
            },
            parameter("UNITY_SERIAL"),
        ],
        script: "set -eu\nprintf '%s' \"$UNITY_EMAIL\" > ~/unity-email\r\n".into(),
        root: false,
        timeout_seconds: 600,
    }
}

fn given(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect()
}

fn image(id: &str, compose: Option<Compose>) -> ImageRecord {
    ImageRecord {
        id: id.into(),
        name: id.into(),
        image_id: format!("sha256:{id}"),
        reference: format!("bilskirnir/snapshot:{id}"),
        parent_image_id: None,
        source_container: None,
        kind: "snapshot".into(),
        created_at: 1,
        description: String::new(),
        desktop: None,
        build: None,
        compose,
    }
}

#[test]
fn a_compose_needs_a_script_and_safe_parameter_names() {
    unity().validate().unwrap();
    let with = |change: &dyn Fn(&mut Compose)| {
        let mut compose = unity();
        change(&mut compose);
        compose.validate()
    };
    assert!(with(&|t| t.script = " \n".into()).is_err());
    assert!(with(&|t| t.script = "echo \0".into()).is_err());
    assert!(with(&|t| t.timeout_seconds = 0).is_err());
    assert!(with(&|t| t.timeout_seconds = 3501).is_err());
    assert!(
        with(&|t| t.parameters = (0..33).map(|i| parameter(&format!("P{i}"))).collect()).is_err()
    );
    assert!(with(&|t| t.parameters.push(parameter("UNITY_EMAIL"))).is_err());
    assert!(with(&|t| t.parameters[0].label = "two\nlines".into()).is_err());
    assert!(with(&|t| t.parameters[0].default = "x".repeat(16_385)).is_err());
    // A secret default would sit in the image record, which every image list returns.
    assert!(with(&|t| t.parameters[1].default = "hunter2".into()).is_err());
    for name in [
        "",
        "unity_email",
        "1ST",
        "_HIDDEN",
        "WITH-DASH",
        "PATH",
        "HOME",
        "DISPLAY",
        "LD_PRELOAD",
        "BASH_ENV",
        "XDG_RUNTIME_DIR",
        "PULSE_SERVER",
        "DESKTOP_GPU",
        "PYTHONPATH",
    ] {
        assert!(
            with(&|t| t.parameters[0].name = name.into()).is_err(),
            "{name:?} was accepted"
        );
    }
    assert!(with(&|t| t.parameters[0].name = format!("A{}", "B".repeat(64))).is_err());
    assert!(with(&|t| t.parameters[0].name = "STEAM_2FA_CODE".into()).is_ok());
    // A misspelled field is an error, not a parameter that is silently optional.
    assert!(
        serde_json::from_value::<Compose>(serde_json::json!({
            "script": "true", "parameters": [{"name": "A", "requried": true}]
        }))
        .is_err()
    );
}

#[test]
fn values_come_from_the_request_then_the_default() {
    let values = unity()
        .resolve(&given(&[
            ("UNITY_EMAIL", "dev@example.com"),
            ("UNITY_PASSWORD", "hunter2"),
        ]))
        .unwrap();
    assert_eq!(
        values,
        given(&[
            ("UNITY_EMAIL", "dev@example.com"),
            ("UNITY_LICENSE", "personal"),
            ("UNITY_PASSWORD", "hunter2"),
            // Every parameter reaches the script, so `set -u` does not trip on an optional one.
            ("UNITY_SERIAL", ""),
        ])
    );
    // A value cleared on purpose stays empty instead of falling back to the default.
    let cleared = unity()
        .resolve(&given(&[
            ("UNITY_EMAIL", "dev@example.com"),
            ("UNITY_PASSWORD", "hunter2"),
            ("UNITY_LICENSE", ""),
        ]))
        .unwrap();
    assert_eq!(cleared["UNITY_LICENSE"], "");
    let error = unity()
        .resolve(&given(&[("UNITY_EMAIL", "dev@example.com")]))
        .unwrap_err();
    assert_eq!(error.to_string(), "Enter a value for Unity password");
    assert!(
        unity()
            .resolve(&given(&[
                ("UNITY_EMAIL", "a"),
                ("UNITY_PASSWORD", "b"),
                ("UNITY_TYPO", "c"),
            ]))
            .unwrap_err()
            .to_string()
            .contains("UNITY_TYPO")
    );
    assert!(
        unity()
            .resolve(&given(&[("UNITY_EMAIL", "a"), ("UNITY_PASSWORD", "nul\0")]))
            .is_err()
    );
}

#[test]
fn the_container_shows_ordinary_values_and_only_the_names_of_secrets() {
    let values = unity()
        .resolve(&given(&[
            ("UNITY_EMAIL", "dev@example.com"),
            ("UNITY_PASSWORD", "hunter2"),
        ]))
        .unwrap();
    let pinned = Pinned {
        compose: unity(),
        values,
    };
    let setup = pinned.setup("img");
    assert_eq!(setup.status, "pending");
    assert_eq!(setup.secrets, ["UNITY_PASSWORD"]);
    assert!(!setup.parameters.contains_key("UNITY_PASSWORD"));
    assert_eq!(setup.parameters["UNITY_EMAIL"], "dev@example.com");
    let env = Container {
        setup: Some(setup),
        ..container()
    };
    assert!(!serde_json::to_string(&env).unwrap().contains("hunter2"));
    // The runner gets every value and the names to mask, with the script's line endings fixed.
    let request = pinned.request();
    assert_eq!(request["parameters"]["UNITY_PASSWORD"], "hunter2");
    assert_eq!(request["secrets"], serde_json::json!(["UNITY_PASSWORD"]));
    assert!(!request["script"].as_str().unwrap().contains('\r'));
}

/// A compose that installs Unity into a volume, with a cache that may stay in the container.
fn installs() -> Compose {
    let volume = |name: &str, path: &str| Parameter {
        kind: parameters::Kind::Volume,
        path: path.into(),
        ..parameter(name)
    };
    Compose {
        parameters: vec![
            Parameter {
                label: "Unity installs".into(),
                default: "unity-installs".into(),
                ..volume("UNITY_HOME", "/opt/unity")
            },
            volume("UNITY_CACHE", "/home/desktop/.cache/unity"),
            parameter("UNITY_EDITOR"),
        ],
        script: "mkdir -p \"$UNITY_HOME/$UNITY_EDITOR\"\n".into(),
        root: false,
        timeout_seconds: 600,
    }
}

fn mount(volume: &str, target: &str) -> Mount {
    Mount {
        volume: volume.into(),
        target: target.into(),
        readonly: false,
    }
}

#[test]
fn volume_parameters_need_paths_and_variables_of_their_own() {
    installs().validate().unwrap();
    let with = |change: &dyn Fn(&mut Compose)| {
        let mut compose = installs();
        change(&mut compose);
        compose.validate().map_err(|e| e.to_string())
    };
    assert!(
        with(&|c| c.parameters[1].path = "/opt/unity".into())
            .unwrap_err()
            .contains("/opt/unity is used twice")
    );
    // The volume's own variable would overwrite a parameter of that name.
    assert!(
        with(&|c| c.parameters[2].name = "UNITY_HOME_VOLUME".into())
            .unwrap_err()
            .contains("carries the volume of UNITY_HOME")
    );
    // Nested folders are separate mounts, as volume settings can be.
    with(&|c| c.parameters[1].path = "/opt/unity/cache".into()).unwrap();
}

#[test]
fn a_volume_parameter_mounts_its_volume_and_the_script_reads_its_path() {
    let values = installs()
        .resolve(&given(&[("UNITY_EDITOR", "6000.0.23f1")]))
        .unwrap();
    // The default volume is chosen unless the request clears it; the cache has none.
    assert_eq!(values["UNITY_HOME"], "unity-installs");
    assert_eq!(values["UNITY_CACHE"], "");
    let pinned = Pinned {
        compose: installs(),
        values,
    };
    assert_eq!(pinned.mounts(), [mount("unity-installs", "/opt/unity")]);
    assert_eq!(pinned.paths(), ["/opt/unity", "/home/desktop/.cache/unity"]);
    // The container record shows the volume chosen and where it is mounted.
    let setup = pinned.setup("img");
    assert_eq!(setup.parameters["UNITY_HOME"], "unity-installs");
    assert_eq!(setup.mounts, pinned.mounts());
    let request = pinned.request();
    assert_eq!(
        request["parameters"],
        json!({
            "UNITY_HOME": "/opt/unity", "UNITY_HOME_VOLUME": "unity-installs",
            "UNITY_CACHE": "/home/desktop/.cache/unity", "UNITY_CACHE_VOLUME": "",
            "UNITY_EDITOR": "6000.0.23f1"
        })
    );
    let cleared = installs()
        .resolve(&given(&[
            ("UNITY_HOME", ""),
            ("UNITY_CACHE", "unity-cache"),
        ]))
        .unwrap();
    assert_eq!(
        installs().mounts(&cleared),
        [mount("unity-cache", "/home/desktop/.cache/unity")]
    );
    assert!(
        installs()
            .resolve(&given(&[("UNITY_HOME", "not a volume")]))
            .is_err()
    );
}

#[test]
fn a_compose_volume_has_its_path_and_its_volume_to_itself() {
    let compose = [mount("unity-installs", "/opt/unity")];
    check_mounts(&[mount("projects", "/home/desktop/Projects")], &compose).unwrap();
    check_mounts(&[mount("cache", "/opt/unity/cache")], &compose).unwrap();
    let refused =
        |own: &[Mount], compose: &[Mount]| check_mounts(own, compose).unwrap_err().to_string();
    assert!(
        refused(&[mount("projects", "/opt/unity")], &compose)
            .contains("remove the volume setting with that path")
    );
    assert!(
        refused(&[mount("unity-installs", "/srv/unity")], &compose)
            .contains("mounted at both /srv/unity and /opt/unity")
    );
    assert!(
        refused(
            &[],
            &[compose[0].clone(), mount("unity-installs", "/srv/cache")]
        )
        .contains("mounted at both /opt/unity and /srv/cache")
    );
    // A parameter left without a volume leaves its path to the volume settings.
    check_mounts(&[mount("projects", "/opt/unity")], &[]).unwrap();
}

#[test]
fn every_new_container_mounts_the_volumes_its_compose_was_given() {
    let app = crate::tests::app(false);
    let pinned = Pinned {
        compose: installs(),
        values: installs().resolve(&BTreeMap::new()).unwrap(),
    };
    let mut env = Container {
        setup: Some(pinned.setup("img")),
        ..container()
    };
    env.spec.mounts = vec![mount("projects", "/home/desktop/Projects")];
    assert_eq!(
        env.mounts(),
        [
            mount("projects", "/home/desktop/Projects"),
            mount("unity-installs", "/opt/unity")
        ]
    );
    let args = app.container_args(&env).join(" ");
    for flag in [
        "--mount type=volume,src=projects,dst=/home/desktop/Projects",
        "--mount type=volume,src=unity-installs,dst=/opt/unity ",
    ] {
        assert!(args.contains(flag), "{flag} missing from {args}");
    }
    // A reset or a retry runs the setup again on the same record, mounts and all.
    setup_again(&mut env);
    assert_eq!(env.compose_mounts(), pinned.mounts());
    // The settings never list them, so a change of volume settings cannot drop them.
    let changed =
        crate::reconfigure::merged_spec(&env.spec, json!({"mounts": []}).as_object().unwrap())
            .unwrap();
    env.spec = changed;
    assert_eq!(env.mounts(), pinned.mounts());
    // Records saved before volume parameters read as having none.
    let mut old = serde_json::to_value(pinned.setup("img")).unwrap();
    old.as_object_mut().unwrap().remove("mounts");
    let old: Setup = serde_json::from_value(old).unwrap();
    assert!(old.mounts.is_empty());
}

#[test]
fn the_compose_comes_from_a_record_of_the_requested_image() {
    let records = [
        image("plain", None),
        image("unity", Some(unity())),
        image("other", Some(unity())),
    ];
    let by_id = compose_image(&records, "sha256:unity", "sha256:unity").unwrap();
    assert_eq!(by_id.id, "unity");
    // A reference resolves through Docker to the record's image ID.
    let by_reference = compose_image(&records, "bilskirnir/snapshot:unity", "sha256:unity");
    assert_eq!(by_reference.unwrap().id, "unity");
    let resolved = compose_image(&records, "some-tag:latest", "sha256:other");
    assert_eq!(resolved.unwrap().id, "other");
    assert!(compose_image(&records, "sha256:plain", "sha256:plain").is_none());
    assert!(compose_image(&records, "ghcr.io/x:1", "sha256:catalog").is_none());
}

#[test]
fn data_saved_while_composes_were_templates_keeps_its_compose() {
    // Databases, exported configurations and archives outlive the rename; each still names it
    // `template`.
    let compose = json!({"parameters": [{"name": "UNITY_EMAIL"}], "script": "true"});
    let mut record = serde_json::to_value(image("old", None)).unwrap();
    record["template"] = compose.clone();
    let record: ImageRecord = serde_json::from_value(record).unwrap();
    assert_eq!(record.compose.unwrap().parameters[0].name, "UNITY_EMAIL");
    let pinned: Pinned =
        serde_json::from_value(json!({"template": compose, "values": {"UNITY_EMAIL": "a"}}))
            .unwrap();
    assert_eq!(pinned.compose.script, "true");
    let build: crate::api::BuildImage = serde_json::from_value(
        json!({"name": "Unity", "base_image": "sha256:a", "script": "true", "template": compose}),
    )
    .unwrap();
    assert!(build.compose.is_some());
    let archive: crate::archives::ArchiveInfo =
        serde_json::from_value(json!({"format": 1, "name": "Unity", "template": compose})).unwrap();
    assert!(archive.compose.is_some());
    // What is written from now on uses the new name.
    let written = serde_json::to_value(&pinned).unwrap();
    assert!(written.get("compose").is_some() && written.get("template").is_none());
}

#[tokio::test]
async fn a_new_container_pins_the_compose_and_its_values() {
    let app = crate::tests::app(false);
    app.db
        .put("image", "unity", &image("unity", Some(unity())))
        .unwrap();
    app.db.put("image", "plain", &image("plain", None)).unwrap();
    let (record, pinned) = app
        .pin_compose(
            "sha256:unity",
            "sha256:unity",
            &given(&[("UNITY_EMAIL", "a@b"), ("UNITY_PASSWORD", "pw")]),
        )
        .unwrap()
        .unwrap();
    assert_eq!(record, "unity");
    assert_eq!(pinned.values["UNITY_PASSWORD"], "pw");
    assert!(
        app.pin_compose("sha256:plain", "sha256:plain", &BTreeMap::new())
            .unwrap()
            .is_none()
    );
    // Values for an image without a compose are a mistake worth reporting.
    assert!(
        app.pin_compose("sha256:plain", "sha256:plain", &given(&[("A", "b")]))
            .is_err()
    );
    app.save_pinned("env", &pinned).unwrap();
    assert_eq!(app.pinned_compose("env"), Some(unity()));
    app.forget_pinned("env").unwrap();
    assert_eq!(app.pinned_compose("env"), None);
}

#[tokio::test]
async fn compose_routes_keep_the_script_out_of_image_lists() {
    let app = crate::tests::app(false);
    app.db.put("image", "unity", &image("unity", None)).unwrap();
    let missing = get(State(app.clone()), Path("unity".into()))
        .await
        .unwrap_err();
    assert_eq!(missing.0, StatusCode::NOT_FOUND);
    let mut invalid = unity();
    invalid.parameters[0].name = "PATH".into();
    assert!(
        set(State(app.clone()), Path("unity".into()), Json(invalid))
            .await
            .is_err()
    );
    let Json(summary) = set(State(app.clone()), Path("unity".into()), Json(unity()))
        .await
        .unwrap();
    assert!(summary["compose"].get("script").is_none());
    assert_eq!(
        summary["compose"]["parameters"][1]["name"],
        "UNITY_PASSWORD"
    );
    let Json(listed) = crate::api::images(State(app.clone())).await.unwrap();
    assert!(listed[0]["compose"].get("script").is_none());
    let Json(compose) = get(State(app.clone()), Path("unity".into())).await.unwrap();
    assert_eq!(compose, unity());
    let Json(summary) = remove(State(app.clone()), Path("unity".into()))
        .await
        .unwrap();
    assert!(summary.get("compose").is_none());
    assert!(get(State(app), Path("absent".into())).await.is_err());
}

#[test]
fn a_restart_before_setup_finished_leaves_it_failed_for_retry() {
    let app = crate::tests::app(false);
    let pinned = Pinned {
        compose: unity(),
        values: given(&[("UNITY_EMAIL", "a"), ("UNITY_PASSWORD", "b")]),
    };
    let cases = [
        ("running", None),
        ("pending", None),
        ("pending", Some("Docker: no such image")),
        ("completed", None),
    ];
    for (id, (status, error)) in cases.iter().enumerate() {
        let mut setup = pinned.setup("unity");
        setup.status = status.to_string();
        let env = Container {
            id: id.to_string(),
            last_error: error.map(str::to_owned),
            setup: Some(setup),
            ..container()
        };
        app.db.put(CONTAINERS, &id.to_string(), &env).unwrap();
    }
    mark_interrupted(&app.db).unwrap();
    let after = |id: usize| {
        app.db
            .get::<Container>(CONTAINERS, &id.to_string())
            .unwrap()
    };
    for id in 0..2 {
        assert_eq!(after(id).setup.unwrap().status, "failed");
        assert!(after(id).last_error.unwrap().contains("retry"));
    }
    // The error that kept the setup from starting stays, and a finished setup is left alone.
    assert_eq!(
        after(2).last_error.as_deref(),
        Some("Docker: no such image")
    );
    assert_eq!(after(3).setup.unwrap().status, "completed");
    assert!(after(3).last_error.is_none());
}

fn container() -> Container {
    Container {
        id: "e".into(),
        container: "wsld-e".into(),
        private_network: "wsld-e-private".into(),
        egress_network: "wsld-e-egress".into(),
        source_image_id: "sha256:unity".into(),
        created_at: 1,
        spec: serde_json::from_value(serde_json::json!({"name": "Unity", "image": "sha256:unity"}))
            .unwrap(),
        last_error: None,
        desktop: None,
        working_image: None,
        setup: None,
    }
}

#[test]
fn the_runner_reads_every_field_the_manager_sends() {
    // The request crosses into Python; a renamed key would silently become an empty default.
    let pinned = Pinned {
        compose: unity(),
        values: BTreeMap::new(),
    };
    for key in pinned.request().as_object().unwrap().keys() {
        assert!(
            RUNNER.contains(&format!("\"{key}\"")),
            "setup_runner.py does not read {key}"
        );
    }
}
