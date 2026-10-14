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
