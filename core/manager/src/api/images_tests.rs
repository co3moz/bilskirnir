//! Pins down how a script build's configuration is kept: the configuration route hands back the
//! exact request that built the image, image lists and job results leave the script out, and
//! records saved before the configuration existed still load. Also that only a catalog image is
//! built or pulled.
use super::*;
use crate::test_support::app;
use crate::{model, router};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;

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
    let app = crate::test_support::app(false);
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
    let app = crate::test_support::app(false);
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
    let app = crate::test_support::app(false);
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
    let app = crate::test_support::app(false);
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

#[tokio::test]
async fn only_catalog_images_are_built_or_pulled() {
    let app = app(false);
    // A free-form reference or a saved image never reaches a registry or the bundled context.
    for action in ["build", "pull"] {
        let response = router(app.clone())
            .oneshot(
                Request::builder()
                    .header("host", "localhost:8080")
                    .method("POST")
                    .uri(format!("/api/images/not-in-catalog/{action}"))
                    .header("x-thoric-request", "1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{action}");
    }
    assert!(app.db.list::<model::Job>("job").unwrap().is_empty());
}
