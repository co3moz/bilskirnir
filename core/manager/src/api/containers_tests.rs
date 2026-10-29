//! Pins down what a create request carries and gets: its compose values beside the settings, the
//! installation's defaults for the features and the proxy it leaves out, a refusal before any job
//! when a required value is missing, and the name of what a creation job makes, which the job list
//! shows before the record exists. Also that one container's route answers as the list does, and
//! that a settings change is checked before any job.
use super::*;
use crate::api::build;
use crate::test_support::{app, spec};
use crate::{model, router};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use std::sync::Arc;
use tower::ServiceExt;

#[test]
fn a_create_request_carries_compose_values_beside_the_settings() {
    let body = serde_json::from_value::<CreateContainer>(json!({
        "name": "Unity", "image": "sha256:a", "gpu": true,
        "parameters": {"UNITY_EMAIL": "dev@example.com"}
    }))
    .unwrap();
    assert!(body.spec.gpu && body.spec.internet && body.spec.shm_mb == 512);
    assert_eq!(body.parameters["UNITY_EMAIL"], "dev@example.com");
    // Requests without parameters, the MCP tool's included, still parse.
    let plain = serde_json::from_value::<CreateContainer>(
        json!({"name": "Plain", "image": "sha256:a", "cpus": 1.5}),
    )
    .unwrap();
    assert!(plain.parameters.is_empty() && plain.spec.cpus == Some(1.5));
    assert!(
        serde_json::from_value::<CreateContainer>(
            json!({"name": "Bad", "image": "sha256:a", "parameters": {"A": 1}})
        )
        .is_err()
    );
}

#[tokio::test]
async fn a_create_request_missing_a_required_value_is_refused_before_a_job() {
    let app = crate::test_support::app(false);
    let image: ImageRecord = serde_json::from_value(json!({
        "id": "unity", "name": "snapshot image", "image_id": "sha256:unity",
        "reference": "thoric/snapshot:unity", "kind": "snapshot", "created_at": 1,
        "description": "",
        "compose": {
            "parameters": [{"name": "UNITY_EMAIL", "required": true}],
            "script": "true"
        }
    }))
    .unwrap();
    app.db.put("image", "unity", &image).unwrap();
    let request =
        serde_json::from_value(json!({"name": "Unity", "image": "sha256:unity"})).unwrap();
    let refused = create(State(app.clone()), Json(request)).await.unwrap_err();
    assert!(refused.1.contains("UNITY_EMAIL"), "{}", refused.1);
    assert!(app.db.list::<Job>("job").unwrap().is_empty());
}

#[tokio::test]
async fn creation_jobs_carry_the_name_of_what_they_make() {
    // The test manager has no Docker CLI and no manager container, so both jobs fail quickly.
    let app = crate::test_support::app(false);
    let request = json!({"name": "Agent desktop", "image": "example/desktop:1"});
    let created = create(
        State(app.clone()),
        Json(serde_json::from_value(request).unwrap()),
    )
    .await
    .unwrap();
    let request = json!({"name": "Tools", "base_image": "example/desktop:1", "script": "true"});
    let built = build(
        State(app.clone()),
        Json(serde_json::from_value(request).unwrap()),
    )
    .await
    .unwrap();
    let other = launch(app.clone(), "start", "container:one".into(), |_, _| async {
        Ok(json!({}))
    })
    .unwrap();
    let ids = [&created, &built, &other].map(|started| started.0["job_id"].clone());
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while ids.iter().any(|id| {
            let job: Job = app.db.job(id.as_str().unwrap()).unwrap();
            job.finished_at.is_none()
        }) {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    // The list the pages poll, as they read it, keeps the name once the job has ended too.
    let listed = serde_json::to_value(app.db.jobs_summary().unwrap()).unwrap();
    let job = |id: &Value| {
        listed
            .as_array()
            .unwrap()
            .iter()
            .find(|job| job["id"] == *id)
            .unwrap()
            .clone()
    };
    assert_eq!(job(&ids[0])["name"], "Agent desktop");
    assert_eq!(job(&ids[0])["status"], "failed");
    assert_eq!(job(&ids[1])["name"], "Tools");
    assert!(
        job(&ids[2]).get("name").is_none(),
        "other jobs name nothing"
    );
}

#[test]
fn a_create_request_gets_the_installations_defaults_for_the_features_it_leaves_out() {
    let mut app = crate::test_support::app(false);
    let config = &mut Arc::get_mut(&mut app).unwrap().config;
    let request = |body: Value| creation_request(config, body).unwrap().spec;
    let features = |spec: ContainerSpec| {
        [
            spec.gpu,
            spec.internet,
            spec.sudo,
            spec.audio,
            spec.microphone,
            spec.gamepad,
            spec.vpn,
        ]
    };
    let plain = json!({"name": "Plain", "image": "sha256:a"});
    // The defaults as they were: GPU access where the installation has it, internet and sudo.
    assert_eq!(
        features(request(plain.clone())),
        [true, true, true, false, false, false, false]
    );
    config.defaults = crate::config::Features {
        gpu: false,
        internet: false,
        sudo: false,
        audio: true,
        microphone: true,
        gamepad: true,
        vpn: true,
    };
    let config = &*config;
    let request = |body: Value| creation_request(config, body).unwrap().spec;
    assert_eq!(
        features(request(plain.clone())),
        [false, false, false, true, true, true, true]
    );
    // What a request asks for is kept, and the microphone follows the request's audio.
    let asked = json!({"name": "Asked", "image": "sha256:a", "gpu": true, "audio": false});
    assert_eq!(
        features(request(asked)),
        [true, false, false, false, false, true, true]
    );
    // A malformed request is refused as one.
    let error = creation_request(
        config,
        json!({"name": "Bad", "image": "sha256:a", "gpu": "yes"}),
    )
    .err()
    .unwrap();
    assert_eq!(error.0, StatusCode::BAD_REQUEST);
}

#[test]
fn a_switched_off_feature_is_never_a_default_and_stored_records_keep_theirs() {
    let mut app = crate::test_support::app(false);
    let config = &mut Arc::get_mut(&mut app).unwrap().config;
    config.defaults = crate::config::Features {
        gpu: true,
        internet: true,
        sudo: true,
        audio: true,
        microphone: true,
        gamepad: true,
        vpn: true,
    };
    config.wsl_gpu = false;
    config.cuda = false;
    config.internet = false;
    config.sudo = false;
    config.gamepad = false;
    config.vpn = false;
    config.microphone = false;
    let spec = creation_request(config, json!({"name": "Off", "image": "sha256:a"}))
        .unwrap()
        .spec;
    assert!(
        !spec.gpu && !spec.internet && !spec.sudo && !spec.microphone && !spec.gamepad && !spec.vpn
    );
    assert!(spec.audio);
    // A record saved before the features existed reads as it always did, whatever the
    // installation's defaults: the serde defaults are not the configured ones.
    let stored: ContainerSpec =
        serde_json::from_value(json!({"name": "Old", "image": "sha256:a"})).unwrap();
    assert!(
        !stored.gpu
            && stored.internet
            && stored.sudo
            && !stored.audio
            && !stored.gamepad
            && !stored.vpn
    );
}

#[test]
fn a_new_container_follows_the_installations_proxy_unless_it_asks_otherwise() {
    let mut app = crate::test_support::app(false);
    let config = &mut Arc::get_mut(&mut app).unwrap().config;
    let plain = json!({"name": "Plain", "image": "sha256:a"});
    let proxy = |config: &crate::config::Config, body: Value| {
        creation_request(config, body).unwrap().spec.proxy
    };
    assert_eq!(proxy(config, plain.clone()), None);
    config.proxy = crate::outbound::installation(
        &|key: &str| (key == "PROXY_URL").then(|| "http://proxy.example:3128".into()),
        &mut Vec::new(),
    )
    .unwrap();
    assert_eq!(
        proxy(config, plain),
        Some(crate::outbound::ProxyChoice::Installation)
    );
    let none = json!({"name": "Direct", "image": "sha256:a", "proxy": null});
    assert_eq!(proxy(config, none), None);
    // The password goes beside the settings, never into them.
    let own = json!({"name": "Own", "image": "sha256:a", "proxy_password": "pw",
        "proxy": {"mode": "custom", "url": "http://own.example:1"}});
    let request = creation_request(config, own).unwrap();
    assert_eq!(request.proxy_password.as_deref(), Some("pw"));
    assert!(
        !serde_json::to_string(&request.spec)
            .unwrap()
            .contains("\"pw\"")
    );
}

#[tokio::test]
async fn single_container_api_returns_the_same_state_as_the_list() {
    let app = app(false);
    let env = model::Container {
        id: "test-id".into(),
        container: "test-container".into(),
        private_network: "test-private".into(),
        egress_network: "test-egress".into(),
        source_image_id: "sha256:test".into(),
        created_at: 0,
        spec: spec(),
        last_error: None,
        desktop: None,
        working_image: None,
        setup: None,
    };
    app.db.put(model::CONTAINERS, &env.id, &env).unwrap();
    let response = router(app)
        .oneshot(
            Request::builder()
                .uri("/api/containers/test-id")
                .header("host", "localhost")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 65536)
            .await
            .unwrap(),
    )
    .unwrap();
    // The record's own fields and Docker's view of it sit side by side.
    assert_eq!(body["id"], "test-id");
    assert_eq!(body["spec"]["name"], spec().name);
    assert_eq!(body["state"], "unavailable");
}

#[tokio::test]
async fn container_settings_are_checked_before_any_job_or_docker_work() {
    let mut app = app(false);
    {
        let config = &mut Arc::get_mut(&mut app).unwrap().config;
        config.audio = false;
        config.microphone = false;
    }
    let mut existing = spec();
    existing.gpu = true;
    let env = model::Container {
        id: "edit-id".into(),
        container: "thoric-edit-id".into(),
        private_network: "thoric-edit-id-private".into(),
        egress_network: "thoric-edit-id-egress".into(),
        source_image_id: "sha256:source".into(),
        created_at: 0,
        spec: existing,
        last_error: None,
        desktop: None,
        working_image: None,
        setup: None,
    };
    app.db.put(model::CONTAINERS, &env.id, &env).unwrap();
    let post = |id: &str, body: serde_json::Value| {
        let app = app.clone();
        let request = Request::builder()
            .header("host", "localhost:8080")
            .header("x-thoric-request", "1")
            .header("content-type", "application/json")
            .method("POST")
            .uri(format!("/api/containers/{id}/settings"))
            .body(Body::from(body.to_string()))
            .unwrap();
        async move {
            let response = router(app).oneshot(request).await.unwrap();
            let status = response.status();
            let bytes = axum::body::to_bytes(response.into_body(), 65536)
                .await
                .unwrap();
            (
                status,
                serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),
            )
        }
    };
    let (status, body) = post(
        "edit-id",
        serde_json::json!({"name": "Renamed", "cpus": 2, "resolution": "1280x720", "dry_run": true}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["live"], serde_json::json!(["name", "cpus"]));
    assert_eq!(body["restart"], serde_json::json!(["resolution"]));
    // A feature the installation switched off cannot be turned on, but a desktop that has one
    // (GPU here, created before the switch) can still change its other settings.
    for (request, expected) in [
        (
            serde_json::json!({"audio": true, "dry_run": true}),
            "Audio is disabled",
        ),
        (
            serde_json::json!({"image": "thoric/custom:other"}),
            "cannot change",
        ),
        (serde_json::json!({"dry_run": "yes"}), "dry_run"),
        (
            serde_json::json!({"volume": "x"}),
            "Unknown container setting",
        ),
    ] {
        let (status, body) = post("edit-id", request).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert!(body["error"].as_str().unwrap().contains(expected), "{body}");
    }
    let (status, _) = post("missing-id", serde_json::json!({"dry_run": true})).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(app.db.list::<model::Job>("job").unwrap().is_empty());
    let saved: model::Container = app.db.get(model::CONTAINERS, "edit-id").unwrap();
    assert_eq!(saved.spec.name, "Unity");
}
