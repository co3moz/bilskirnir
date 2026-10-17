//! The archive routes refuse what they can before any Docker work, report a Docker failure
//! through their job and the transfer itself, and name downloads and imported records sensibly.
use super::*;
use crate::model::Job;
use axum::http::Request;
use tower::ServiceExt;

fn import_request(query: &str, content_type: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(format!("/api/images/import?{query}"))
        .header("host", "localhost:8080")
        .header("x-wsld-request", "1")
        .header("content-type", content_type)
        .body(Body::from(vec![0u8; 16]))
        .unwrap()
}

fn get(uri: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .header("host", "localhost:8080")
        .body(Body::empty())
        .unwrap()
}

async fn error_message(response: Response) -> String {
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    serde_json::from_slice::<Value>(&bytes).unwrap()["error"]
        .as_str()
        .unwrap()
        .to_owned()
}

async fn finished(app: &App, id: &str) -> Job {
    for _ in 0..200 {
        let job: Job = app.db.get("job", id).unwrap();
        if job.status != "queued" && job.status != "running" {
            return job;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("job {id} did not finish");
}

#[tokio::test]
async fn imports_are_checked_before_any_job_or_docker_work() {
    let app = crate::tests::app(false);
    let long_name = format!("size=16&name={}", "x".repeat(101));
    // An archive streamed without a size is checked the same way.
    let streamed = format!("name={}", "x".repeat(101));
    for (query, content_type, expected) in [
        ("", "application/gzip", StatusCode::UNSUPPORTED_MEDIA_TYPE),
        (
            streamed.as_str(),
            "application/octet-stream",
            StatusCode::BAD_REQUEST,
        ),
        (
            "size=5000000",
            "application/octet-stream",
            StatusCode::PAYLOAD_TOO_LARGE,
        ),
        (
            "size=16",
            "application/gzip",
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
        ),
        (
            long_name.as_str(),
            "application/octet-stream",
            StatusCode::BAD_REQUEST,
        ),
    ] {
        let response = crate::router(app.clone())
            .oneshot(import_request(query, content_type))
            .await
            .unwrap();
        assert_eq!(response.status(), expected, "{query}");
    }
    let _running = [
        app.archives.clone().try_acquire_owned().unwrap(),
        app.archives.clone().try_acquire_owned().unwrap(),
    ];
    let response = crate::router(app.clone())
        .oneshot(import_request("size=16", "application/octet-stream"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(app.db.list::<Job>("job").unwrap().is_empty());
}

#[tokio::test]
async fn a_refused_import_answers_with_its_jobs_error() {
    // The test manager has no Docker CLI, so the job fails before it reads the archive.
    let app = crate::tests::app(false);
    let response = crate::router(app.clone())
        .oneshot(import_request(
            "size=16&name=Saved",
            "application/octet-stream",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(
        error_message(response)
            .await
            .contains("Could not start Docker CLI")
    );
    let jobs = app.db.list::<Job>("job").unwrap();
    assert_eq!(jobs.len(), 1);
    let job = finished(&app, &jobs[0].id).await;
    assert_eq!(
        (job.kind.as_str(), job.status.as_str()),
        ("image-import", "failed")
    );
    assert_eq!(app.archives.available_permits(), 2);
}

#[tokio::test]
async fn a_failed_export_breaks_the_download_and_fails_its_job() {
    let app = crate::tests::app(false);
    let record = ImageRecord {
        id: "saved".into(),
        name: "Geliştirme masası".into(),
        image_id: format!("sha256:{}", "a".repeat(64)),
        reference: "bilskirnir/snapshot:saved".into(),
        parent_image_id: None,
        source_container: None,
        kind: "snapshot".into(),
        created_at: 1,
        description: String::new(),
        desktop: None,
        build: None,
        compose: None,
        icon: None,
        template_id: None,
    };
    app.db.put("image", &record.id, &record).unwrap();
    let response = crate::router(app.clone())
        .oneshot(get("/api/images/saved/archive"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let headers = response.headers();
    assert_eq!(headers[header::CONTENT_TYPE], "application/gzip");
    assert_eq!(
        headers[header::CONTENT_DISPOSITION],
        "attachment; filename=\"geli-tirme-masas.tar.gz\"; filename*=UTF-8''Geli%C5%9Ftirme%20masas%C4%B1.tar.gz"
    );
    let job_id = headers["x-job-id"].to_str().unwrap().to_owned();
    assert!(
        axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .is_err(),
        "a failed export must not look like a complete file"
    );
    let job = finished(&app, &job_id).await;
    assert_eq!(
        (job.kind.as_str(), job.status.as_str()),
        ("image-export", "failed")
    );
    assert!(job.log.contains("no longer available in Docker"));
    assert_eq!(job.resource.as_deref(), Some("image:saved"));
    assert_eq!(app.archives.available_permits(), 2);

    let missing = crate::router(app)
        .oneshot(get("/api/images/unknown/archive"))
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
}

#[test]
fn download_names_are_safe_and_keep_the_image_name() {
    assert_eq!(
        file_names("Unity tools / v2"),
        (
            "unity-tools-v2.tar.gz".into(),
            "Unity tools - v2.tar.gz".into()
        )
    );
    assert_eq!(
        file_names("  ***  "),
        ("image.tar.gz".into(), "---.tar.gz".into())
    );
    assert_eq!(
        file_names(""),
        ("image.tar.gz".into(), "image.tar.gz".into())
    );
}

#[test]
fn the_loaded_image_comes_from_docker_load_output() {
    let id = format!("sha256:{}", "b".repeat(64));
    assert_eq!(
        loaded_image(&format!("Loaded image ID: {id}\n")).unwrap(),
        id
    );
    assert_eq!(
        loaded_image(&format!("Loaded image ID: {id}\nLoaded image ID: {id}\n")).unwrap(),
        id
    );
    assert_eq!(
        loaded_image("Loaded image: bilskirnir/x:1\n").unwrap(),
        "bilskirnir/x:1"
    );
    assert!(loaded_image("").is_err());
    assert!(
        loaded_image(&format!(
            "Loaded image ID: {id}\nLoaded image ID: sha256:c\n"
        ))
        .is_err()
    );
}

#[test]
fn imported_records_keep_the_archive_name_unless_one_is_given() {
    let info = ArchiveInfo::read(Some(
        br#"{"format":1,"name":" Saved desk ","description":"Tools","future":true}"#,
    ))
    .unwrap();
    let id = format!("sha256:{}", "c".repeat(64));
    assert_eq!(record_name(None, Some(&info), &id), "Saved desk");
    assert_eq!(record_name(Some("  "), Some(&info), &id), "Saved desk");
    assert_eq!(record_name(Some("Mine"), Some(&info), &id), "Mine");
    assert_eq!(record_name(None, None, &id), "Imported image cccccccccccc");
    assert_eq!(
        record_name(Some(&"é".repeat(150)), None, &id)
            .chars()
            .count(),
        100
    );
    assert!(ArchiveInfo::read(Some(br#"{"format":2,"name":"Later"}"#)).is_none());
    assert!(ArchiveInfo::read(Some(b"not json")).is_none());
    assert_eq!(clip("ağaç", 2), "a");
    assert!(image_digest(&id) && !image_digest("sha256:short"));
}

#[test]
fn a_script_build_keeps_its_configuration_through_the_archive() {
    let build = BuildConfig {
        base_image: "ghcr.io/co3moz/bilskirnir/ubuntu-xfce:24.04".into(),
        internet: false,
        script: "apt-get install -y git\n".into(),
    };
    let record = ImageRecord {
        id: "built".into(),
        name: "Tools".into(),
        image_id: format!("sha256:{}", "d".repeat(64)),
        reference: "bilskirnir/custom:built".into(),
        parent_image_id: None,
        source_container: None,
        kind: "build".into(),
        created_at: 1,
        description: String::new(),
        desktop: None,
        build: Some(build.clone()),
        compose: None,
        icon: None,
        template_id: None,
    };
    let written = serde_json::to_vec(&ArchiveInfo::of(&record)).unwrap();
    assert_eq!(
        ArchiveInfo::read(Some(&written)).unwrap().build,
        Some(build)
    );
    let mut refused: Value = serde_json::from_slice(&written).unwrap();
    refused["build"]["base_image"] = json!("-not a reference");
    let refused = serde_json::to_vec(&refused).unwrap();
    let info = ArchiveInfo::read(Some(&refused)).unwrap();
    assert!(info.build.is_none() && info.name == "Tools");
}

#[test]
fn a_compose_travels_with_the_archive_and_an_invalid_one_is_dropped() {
    let compose = Compose {
        // A choice carries the most fields a parameter has.
        parameters: vec![
            serde_json::from_value(json!({
                "name": "UNITY_EDITOR", "label": "Editor", "type": "choice", "display": "radio",
                "options": [{"value": "6000.0.23f1", "label": "Unity 6"}, {"value": "2022.3.50f1"}],
                "allow_custom": true, "default": "6000.0.23f1"
            }))
            .unwrap(),
        ],
        script: "printf '%s' \"$UNITY_EDITOR\" > ~/unity-editor\n".into(),
        root: false,
        timeout_seconds: 600,
    };
    let record = ImageRecord {
        id: "snap".into(),
        name: "Unity project".into(),
        image_id: format!("sha256:{}", "e".repeat(64)),
        reference: "bilskirnir/snapshot:snap".into(),
        parent_image_id: None,
        source_container: None,
        kind: "snapshot".into(),
        created_at: 1,
        description: String::new(),
        desktop: None,
        build: None,
        compose: Some(compose.clone()),
        icon: None,
        template_id: None,
    };
    let written = serde_json::to_vec(&ArchiveInfo::of(&record)).unwrap();
    assert_eq!(
        ArchiveInfo::read(Some(&written)).unwrap().compose,
        Some(compose)
    );
    // A compose the compose route would refuse is not kept, but the rest of the metadata is.
    let mut refused: Value = serde_json::from_slice(&written).unwrap();
    refused["compose"]["parameters"][0]["name"] = json!("PATH");
    let info = ArchiveInfo::read(Some(&serde_json::to_vec(&refused).unwrap())).unwrap();
    assert!(info.compose.is_none() && info.name == "Unity project");
}

#[test]
fn an_icon_and_template_travel_with_the_archive_and_unusable_ones_are_dropped() {
    let record = ImageRecord {
        id: "unity".into(),
        name: "Unity 6".into(),
        image_id: format!("sha256:{}", "f".repeat(64)),
        reference: "bilskirnir/custom:unity".into(),
        parent_image_id: None,
        source_container: None,
        kind: "build".into(),
        created_at: 1,
        description: String::new(),
        desktop: None,
        build: None,
        compose: None,
        icon: Some(crate::icons::sample(256, 256)),
        template_id: Some("unity".into()),
    };
    let written = serde_json::to_vec(&ArchiveInfo::of(&record)).unwrap();
    let info = ArchiveInfo::read(Some(&written)).unwrap();
    assert_eq!(info.icon, record.icon);
    assert_eq!(info.template_id.as_deref(), Some("unity"));
    // A picture or an id this release cannot use costs the archive nothing else.
    let mut refused: Value = serde_json::from_slice(&written).unwrap();
    refused["icon"] = json!("data:image/webp;base64,UklGRg==");
    refused["template_id"] = json!("Unity 6");
    let info = ArchiveInfo::read(Some(&serde_json::to_vec(&refused).unwrap())).unwrap();
    assert!(info.icon.is_none() && info.template_id.is_none());
    assert_eq!(info.name, "Unity 6");
}
