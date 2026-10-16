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
