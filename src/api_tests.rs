//! Pins down how a script build's configuration is kept: the configuration route hands back the
//! exact request that built the image, image lists and job results leave the script out, and
//! records saved before the configuration existed still load.
use super::*;

fn record(id: &str, kind: &str, build: Option<BuildConfig>) -> ImageRecord {
    ImageRecord {
        id: id.into(),
        name: format!("{kind} image"),
        image_id: format!("sha256:{id}"),
        reference: format!("bilskirnir/{kind}:{id}"),
        parent_image_id: Some("sha256:base".into()),
        source_environment: None,
        kind: kind.into(),
        created_at: 1,
        description: String::new(),
        desktop: None,
        build,
        template: None,
    }
}

fn unity_tools() -> BuildConfig {
    BuildConfig {
        base_image: "ghcr.io/co3moz/bilskirnir/ubuntu-xfce:24.04".into(),
        internet: false,
        script: "set -eu\napt-get install -y git\n".into(),
    }
}

#[tokio::test]
async fn configuration_is_the_request_that_built_the_image() {
    let app = crate::tests::app(false);
    app.db
        .put("image", "a", &record("a", "build", Some(unity_tools())))
        .unwrap();
    let Json(config) = image_config(State(app), Path("a".into())).await.unwrap();
    let expected = BuildImage {
        name: "build image".into(),
        base_image: unity_tools().base_image,
        internet: false,
        script: unity_tools().script,
        template: None,
    };
    assert_eq!(config, expected);
    // What the interface copies is a body the build route accepts as it is.
    let exported = serde_json::to_string(&config).unwrap();
    assert_eq!(
        serde_json::from_str::<BuildImage>(&exported).unwrap(),
        expected
    );
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
fn a_create_request_carries_template_values_beside_the_settings() {
    let body = serde_json::from_value::<CreateEnvironment>(json!({
        "name": "Unity", "image": "sha256:a", "gpu": true,
        "parameters": {"UNITY_EMAIL": "dev@example.com"}
    }))
    .unwrap();
    assert!(body.spec.gpu && body.spec.internet && body.spec.shm_mb == 512);
    assert_eq!(body.parameters["UNITY_EMAIL"], "dev@example.com");
    // Requests from before templates, the MCP tool's included, still parse.
    let plain = serde_json::from_value::<CreateEnvironment>(
        json!({"name": "Plain", "image": "sha256:a", "cpus": 1.5}),
    )
    .unwrap();
    assert!(plain.parameters.is_empty() && plain.spec.cpus == Some(1.5));
    assert!(
        serde_json::from_value::<CreateEnvironment>(
            json!({"name": "Bad", "image": "sha256:a", "parameters": {"A": 1}})
        )
        .is_err()
    );
}

#[tokio::test]
async fn a_create_request_missing_a_required_value_is_refused_before_a_job() {
    let app = crate::tests::app(false);
    let mut image = record("unity", "snapshot", None);
    image.template = Some(
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
