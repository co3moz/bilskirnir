//! Pins down how a script build's configuration is kept: the configuration route hands back the
//! exact request that built the image, image lists and job results leave the script out, and
//! records saved before the configuration existed still load. Also that the jobs which make a
//! container or an image carry its name in the job list before its record exists.
use super::*;

fn record(id: &str, kind: &str, build: Option<BuildConfig>) -> ImageRecord {
    ImageRecord {
        id: id.into(),
        name: format!("{kind} image"),
        image_id: format!("sha256:{id}"),
        reference: format!("thoric/{kind}:{id}"),
        parent_image_id: Some("sha256:base".into()),
        source_container: None,
        kind: kind.into(),
        created_at: 1,
        description: String::new(),
        desktop: None,
        build,
        compose: None,
        icon: None,
        template_ids: Vec::new(),
    }
}

fn unity_tools() -> BuildConfig {
    BuildConfig {
        base_image: "ghcr.io/co3moz/thoric/ubuntu-xfce:24.04".into(),
        internet: false,
        script: "set -eu\napt-get install -y git\n".into(),
        parts: Vec::new(),
        mcp: Vec::new(),
        inherit_compose: true,
    }
}

#[tokio::test]
async fn configuration_is_the_request_that_built_the_image() {
    let app = crate::tests::app(false);
    let built = ImageRecord {
        description: "Git for Unity projects".into(),
        icon: Some(crate::icons::sample(256, 256)),
        template_ids: vec!["unity".into()],
        ..record("a", "build", Some(unity_tools()))
    };
    app.db.put("image", "a", &built).unwrap();
    let Json(config) = image_config(State(app.clone()), Path("a".into()))
        .await
        .unwrap();
    let expected = BuildImage {
        name: "build image".into(),
        description: "Git for Unity projects".into(),
        base_image: unity_tools().base_image,
        internet: false,
        script: unity_tools().script,
        parts: Vec::new(),
        mcp: Vec::new(),
        compose: None,
        inherit_compose: true,
        icon: Some(crate::icons::sample(256, 256)),
        // One template is named as before templates could be merged, so older releases read it.
        template_id: Some("unity".into()),
        template_ids: Vec::new(),
    };
    assert_eq!(config, expected);
    // What the interface copies is a body the build route accepts as it is.
    let exported = serde_json::to_string(&config).unwrap();
    assert_eq!(
        serde_json::from_str::<BuildImage>(&exported).unwrap(),
        expected
    );
    // A build that left its base's compose out exports that, so building it again does too.
    let without = record(
        "b",
        "build",
        Some(BuildConfig {
            inherit_compose: false,
            ..unity_tools()
        }),
    );
    app.db.put("image", "b", &without).unwrap();
    let Json(config) = image_config(State(app), Path("b".into())).await.unwrap();
    assert!(!config.inherit_compose);
}

#[test]
fn a_build_request_checks_its_description_icon_and_template() {
    let request = |extra: Value| {
        let mut body = json!({"name": "Unity", "base_image": "ubuntu:24.04", "script": "true"});
        body.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        serde_json::from_value::<BuildImage>(body)
    };
    let plain = request(json!({})).unwrap();
    assert!(plain.validate().is_ok());
    // A build keeps its base's compose unless it says otherwise, as requests made before it
    // could say so get.
    assert!(plain.inherit_compose);
    // An exported configuration names only what the image has.
    let exported = serde_json::to_value(&plain).unwrap();
    for key in [
        "description",
        "icon",
        "template_id",
        "compose",
        "inherit_compose",
    ] {
        assert!(exported.get(key).is_none(), "{key}");
    }
    let without = request(json!({"inherit_compose": false})).unwrap();
    assert_eq!(
        serde_json::to_value(&without).unwrap()["inherit_compose"],
        json!(false)
    );
    let full = request(json!({
        "description": "Unity Hub and an editor",
        "icon": crate::icons::sample(256, 256),
        "template_id": "unity-6",
    }))
    .unwrap();
    assert!(full.validate().is_ok());
    for refused in [
        json!({"description": "x".repeat(4097)}),
        json!({"icon": crate::icons::sample(300, 300)}),
        json!({"template_id": "Unity"}),
        json!({"template_id": "-unity"}),
        json!({"template_id": "a".repeat(65)}),
    ] {
        let body = request(refused.clone()).unwrap();
        assert!(body.validate().is_err(), "{refused}");
    }
    assert!(request(json!({"icon": "data:image/gif;base64,R0lGODlh"})).is_err());
}

#[test]
fn a_merged_build_has_parts_instead_of_a_script() {
    let part = |id: &str| json!({"id": id, "name": id, "script": "true"});
    let request = |extra: Value| {
        let mut body = json!({"name": "Unity and Chrome", "base_image": "ubuntu:24.04"});
        body.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        serde_json::from_value::<BuildImage>(body)
            .unwrap()
            .validate()
    };
    request(json!({
        "parts": [part("unity-dev"), part("chrome")],
        "template_ids": ["unity-dev", "chrome"]
    }))
    .unwrap();
    for refused in [
        json!({}),
        json!({"script": "true", "parts": [part("chrome")]}),
        json!({"parts": [part("chrome"), part("chrome")]}),
        json!({"parts": [part("Chrome")]}),
        json!({"parts": [{"id": "chrome", "name": "", "script": "true"}]}),
        json!({"parts": [{"id": "chrome", "name": "Chrome", "script": " \n"}]}),
        json!({"parts": (0..9).map(|i| part(&format!("p{i}"))).collect::<Vec<_>>()}),
        json!({"script": "true", "template_ids": ["Chrome"]}),
    ] {
        assert!(request(refused.clone()).is_err(), "{refused}");
    }
}

#[tokio::test]
async fn a_merged_build_exports_its_parts_and_lists_leave_every_script_out() {
    let app = crate::tests::app(false);
    let part = |id: &str, name: &str| BuildPart {
        id: id.into(),
        name: name.into(),
        script: format!("echo {id}\n"),
    };
    let merged = BuildConfig {
        script: String::new(),
        parts: vec![
            part("unity-dev", "Unity development"),
            part("chrome", "Google Chrome"),
        ],
        ..unity_tools()
    };
    let compose = serde_json::from_value(json!({"steps": [
        {"id": "unity-dev", "script": "echo setup unity"},
        {"id": "vscode", "script": "echo setup code"}
    ]}))
    .unwrap();
    let built = ImageRecord {
        template_ids: vec!["unity-dev".into(), "chrome".into()],
        compose: Some(compose),
        ..record("m", "build", Some(merged.clone()))
    };
    app.db.put("image", "m", &built).unwrap();
    let Json(config) = image_config(State(app.clone()), Path("m".into()))
        .await
        .unwrap();
    assert_eq!(config.parts, merged.parts);
    assert_eq!(config.template_ids, ["unity-dev", "chrome"]);
    let exported = serde_json::to_value(&config).unwrap();
    assert!(exported.get("script").is_none() && exported.get("template_id").is_none());
    config.validate().unwrap();
    let Json(listed) = images(State(app)).await.unwrap();
    let listed = listed
        .as_array()
        .unwrap()
        .iter()
        .find(|image| image["id"] == "m")
        .unwrap();
    assert!(!listed.to_string().contains("echo "), "{listed}");
    assert_eq!(
        listed["build"]["parts"][1],
        json!({"id": "chrome", "name": "Google Chrome"})
    );
    assert_eq!(listed["template_ids"], json!(["unity-dev", "chrome"]));
}

#[test]
fn records_from_before_merged_templates_name_their_template_alone() {
    let mut older = json!(record("a", "build", Some(unity_tools())));
    older["template_id"] = json!("unity");
    let read: ImageRecord = serde_json::from_value(older).unwrap();
    assert_eq!(read.template_ids, ["unity"]);
    // A build of one script is written as it was before parts.
    let written = serde_json::to_value(unity_tools()).unwrap();
    assert!(written.get("parts").is_none() && written["script"].is_string());
}

#[tokio::test]
async fn images_without_a_recorded_build_have_no_configuration() {
    let app = crate::tests::app(false);
    app.db
        .put("image", "s", &record("s", "snapshot", None))
        .unwrap();
    for id in ["s", "unknown"] {
        let error = image_config(State(app.clone()), Path(id.into()))
            .await
            .unwrap_err();
        assert_eq!(error.0, StatusCode::NOT_FOUND, "{id}");
    }
}

#[tokio::test]
async fn image_list_leaves_out_build_scripts() {
    let app = crate::tests::app(false);
    app.db
        .put("image", "a", &record("a", "build", Some(unity_tools())))
        .unwrap();
    // A record saved before builds kept their configuration has no `build` key at all.
    let mut legacy = json!(record("b", "build", None));
    legacy.as_object_mut().unwrap().remove("build");
    app.db.put("image", "b", &legacy).unwrap();
    let Json(listed) = images(State(app)).await.unwrap();
    let listed = listed.as_array().unwrap();
    let built = listed.iter().find(|image| image["id"] == "a").unwrap();
    assert_eq!(
        built["build"],
        json!({"base_image": unity_tools().base_image, "internet": false})
    );
    let older = listed.iter().find(|image| image["id"] == "b").unwrap();
    assert!(older.get("build").is_none());
}

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
    let app = crate::tests::app(false);
    let mut image = record("unity", "snapshot", None);
    image.compose = Some(
        serde_json::from_value(json!({
            "parameters": [{"name": "UNITY_EMAIL", "required": true}],
            "script": "true"
        }))
        .unwrap(),
    );
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
    let app = crate::tests::app(false);
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
    let mut app = crate::tests::app(false);
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
    let mut app = crate::tests::app(false);
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
    let mut app = crate::tests::app(false);
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
