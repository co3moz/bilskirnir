//! Pins down what a template accepts, how a new environment's values are resolved from it, and
//! that a secret value reaches only the setup runner: never the environment record, an image
//! list or a route.
use super::*;

fn parameter(name: &str) -> Parameter {
    Parameter {
        name: name.into(),
        ..Default::default()
    }
}

fn unity() -> Template {
    Template {
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

fn image(id: &str, template: Option<Template>) -> ImageRecord {
    ImageRecord {
        id: id.into(),
        name: id.into(),
        image_id: format!("sha256:{id}"),
        reference: format!("bilskirnir/snapshot:{id}"),
        parent_image_id: None,
        source_environment: None,
        kind: "snapshot".into(),
        created_at: 1,
        description: String::new(),
        desktop: None,
        build: None,
        template,
    }
}

#[test]
fn a_template_needs_a_script_and_safe_parameter_names() {
    unity().validate().unwrap();
    let with = |change: &dyn Fn(&mut Template)| {
        let mut template = unity();
        change(&mut template);
        template.validate()
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
        serde_json::from_value::<Template>(serde_json::json!({
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
fn the_environment_shows_ordinary_values_and_only_the_names_of_secrets() {
    let values = unity()
        .resolve(&given(&[
            ("UNITY_EMAIL", "dev@example.com"),
            ("UNITY_PASSWORD", "hunter2"),
        ]))
        .unwrap();
    let pinned = Pinned {
        template: unity(),
        values,
    };
    let setup = pinned.setup("img");
    assert_eq!(setup.status, "pending");
    assert_eq!(setup.secrets, ["UNITY_PASSWORD"]);
    assert!(!setup.parameters.contains_key("UNITY_PASSWORD"));
    assert_eq!(setup.parameters["UNITY_EMAIL"], "dev@example.com");
    let env = Environment {
        setup: Some(setup),
        ..environment()
    };
    assert!(!serde_json::to_string(&env).unwrap().contains("hunter2"));
    // The runner gets every value and the names to mask, with the script's line endings fixed.
    let request = pinned.request();
    assert_eq!(request["parameters"]["UNITY_PASSWORD"], "hunter2");
    assert_eq!(request["secrets"], serde_json::json!(["UNITY_PASSWORD"]));
    assert!(!request["script"].as_str().unwrap().contains('\r'));
}

#[test]
fn the_template_comes_from_a_record_of_the_requested_image() {
    let records = [
        image("plain", None),
        image("unity", Some(unity())),
        image("other", Some(unity())),
    ];
    let by_id = template_image(&records, "sha256:unity", "sha256:unity").unwrap();
    assert_eq!(by_id.id, "unity");
    // A reference resolves through Docker to the record's image ID.
    let by_reference = template_image(&records, "bilskirnir/snapshot:unity", "sha256:unity");
    assert_eq!(by_reference.unwrap().id, "unity");
    let resolved = template_image(&records, "some-tag:latest", "sha256:other");
    assert_eq!(resolved.unwrap().id, "other");
    assert!(template_image(&records, "sha256:plain", "sha256:plain").is_none());
    assert!(template_image(&records, "ghcr.io/x:1", "sha256:profile").is_none());
}

#[tokio::test]
async fn a_new_environment_pins_the_template_and_its_values() {
    let app = crate::tests::app(false);
    app.db
        .put("image", "unity", &image("unity", Some(unity())))
        .unwrap();
    app.db.put("image", "plain", &image("plain", None)).unwrap();
    let (record, pinned) = app
        .pin_template(
            "sha256:unity",
            "sha256:unity",
            &given(&[("UNITY_EMAIL", "a@b"), ("UNITY_PASSWORD", "pw")]),
        )
        .unwrap()
        .unwrap();
    assert_eq!(record, "unity");
    assert_eq!(pinned.values["UNITY_PASSWORD"], "pw");
    assert!(
        app.pin_template("sha256:plain", "sha256:plain", &BTreeMap::new())
            .unwrap()
            .is_none()
    );
    // Values for an image without a template are a mistake worth reporting.
    assert!(
        app.pin_template("sha256:plain", "sha256:plain", &given(&[("A", "b")]))
            .is_err()
    );
    app.save_pinned("env", &pinned).unwrap();
    assert_eq!(app.pinned_template("env"), Some(unity()));
    app.forget_pinned("env").unwrap();
    assert_eq!(app.pinned_template("env"), None);
}

#[tokio::test]
async fn template_routes_keep_the_script_out_of_image_lists() {
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
    assert!(summary["template"].get("script").is_none());
    assert_eq!(
        summary["template"]["parameters"][1]["name"],
        "UNITY_PASSWORD"
    );
    let Json(listed) = crate::api::images(State(app.clone())).await.unwrap();
    assert!(listed[0]["template"].get("script").is_none());
    let Json(template) = get(State(app.clone()), Path("unity".into())).await.unwrap();
    assert_eq!(template, unity());
    let Json(summary) = remove(State(app.clone()), Path("unity".into()))
        .await
        .unwrap();
    assert!(summary.get("template").is_none());
    assert!(get(State(app), Path("absent".into())).await.is_err());
}

#[test]
fn a_restart_before_setup_finished_leaves_it_failed_for_retry() {
    let app = crate::tests::app(false);
    let pinned = Pinned {
        template: unity(),
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
        let env = Environment {
            id: id.to_string(),
            last_error: error.map(str::to_owned),
            setup: Some(setup),
            ..environment()
        };
        app.db.put("environment", &id.to_string(), &env).unwrap();
    }
    mark_interrupted(&app.db).unwrap();
    let after = |id: usize| {
        app.db
            .get::<Environment>("environment", &id.to_string())
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

fn environment() -> Environment {
    Environment {
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
        template: unity(),
        values: BTreeMap::new(),
    };
    for key in pinned.request().as_object().unwrap().keys() {
        assert!(
            RUNNER.contains(&format!("\"{key}\"")),
            "setup_runner.py does not read {key}"
        );
    }
}
