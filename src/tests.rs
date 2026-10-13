use super::*;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;

fn app(auth: bool) -> Arc<App> {
    Arc::new(App {
        config: config::Config {
            listen: "127.0.0.1:0".into(),
            data: "data".into(),
            web: "web/dist".into(),
            docker_bin: "definitely-not-a-docker-command".into(),
            manager: None,
            base_image: "wsl-desktop/ubuntu:24.04".into(),
            desktop_context: "desktop".into(),
            auth,
            username: "admin".into(),
            password_hash: None,
            secure_cookie: false,
            audio: true,
            microphone: true,
            wsl_gpu: true,
            cuda: true,
            adapter: String::new(),
            allowed_hosts: vec![],
            upload_limit: 1024 * 1024,
        },
        db: db::Db::open(std::path::Path::new(":memory:")).unwrap(),
        sessions: Mutex::new(HashMap::new()),
        login_attempts: Mutex::new(Vec::new()),
        operations: Arc::new(tokio::sync::Mutex::new(())),
        resource_locks: Mutex::new(HashMap::new()),
        session_events: tokio::sync::Notify::new(),
        screenshots: tokio::sync::Semaphore::new(2),
        uploads: tokio::sync::Semaphore::new(2),
    })
}
fn spec() -> model::EnvironmentSpec {
    serde_json::from_value(serde_json::json!({"name":"Unity","image":"wsl-desktop/ubuntu:24.04"}))
        .unwrap()
}
#[test]
fn uploads_only_accept_canonical_paths_in_writable_locations() {
    let mut spec = spec();
    spec.mounts = vec![
        model::Mount {
            volume: "projects".into(),
            target: "/shared".into(),
            readonly: false,
        },
        model::Mount {
            volume: "reference".into(),
            target: "/shared/reference".into(),
            readonly: true,
        },
    ];
    for target in [
        "/home/desktop/Downloads",
        "/home/desktop/Downloads/project",
        "/shared/assets",
    ] {
        assert!(
            uploads::validate_target(&spec, target, "file & notes.txt").is_ok(),
            "{target}"
        );
    }
    for target in [
        "/etc",
        "/sharedish",
        "/shared/reference/sub",
        "/home/desktop/Downloads-fake",
        "/home/desktop/Downloads/../.ssh",
        "/shared//assets",
        "/shared/.",
    ] {
        assert!(
            uploads::validate_target(&spec, target, "file.txt").is_err(),
            "{target}"
        );
    }
    for name in [
        "",
        ".",
        "..",
        "../file",
        "file/name",
        "file\\name",
        "file\0name",
    ] {
        assert!(uploads::validate_target(&spec, "/shared", name).is_err());
    }
    spec.mounts.push(model::Mount {
        volume: "downloads".into(),
        target: "/home/desktop/Downloads".into(),
        readonly: true,
    });
    assert!(uploads::validate_target(&spec, "/home/desktop/Downloads", "file.txt").is_err());
}

#[tokio::test]
async fn uploads_enforce_authentication_origin_and_size_before_docker() {
    let uri =
        "/api/environments/unknown/files?destination=/home/desktop/Downloads&name=test.txt&size=1";
    for (auth, csrf, size, expected) in [
        (true, true, 1, StatusCode::UNAUTHORIZED),
        (false, false, 1, StatusCode::FORBIDDEN),
        (false, true, 1048577, StatusCode::PAYLOAD_TOO_LARGE),
    ] {
        let uri = uri.replace("size=1", &format!("size={size}"));
        let mut request = Request::builder()
            .method("POST")
            .uri(uri)
            .header("host", "localhost:8080")
            .header("content-type", "application/octet-stream");
        if csrf {
            request = request.header("x-wsld-request", "1");
        }
        let response = router(app(auth))
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), expected);
    }
}
#[test]
fn resource_and_mount_validation() {
    let mut s = spec();
    assert!(s.validate().is_ok());
    s.cpus = Some(f64::INFINITY);
    assert!(s.validate().is_err());
    s.cpus = None;
    s.resolution = "0x1080".into();
    assert!(s.validate().is_err());
    s.resolution = "1920x1080".into();
    s.mounts.push(model::Mount {
        volume: "/var/run/docker.sock".into(),
        target: "/shared".into(),
        readonly: false,
    });
    assert!(s.validate().is_err());
    s.mounts[0].volume = "project-data".into();
    s.mounts[0].target = "/proc".into();
    assert!(s.validate().is_err());
    s.mounts[0].target = "/home/desktop/../root".into();
    assert!(s.validate().is_err());
    s.mounts[0].target = "/workspace/project".into();
    assert!(s.validate().is_ok());
    s.mounts.push(s.mounts[0].clone());
    assert!(s.validate().is_err());
}
#[test]
fn mount_aliases_and_readonly_home_cannot_break_desktop_infrastructure() {
    let mut s = spec();
    s.mounts.push(model::Mount {
        volume: "project-data".into(),
        target: String::new(),
        readonly: false,
    });
    for target in [
        "//etc",
        "/./etc",
        "/workspace/../etc",
        "/workspace//data",
        "/lib64",
        "/var/run",
        "/var",
        "/home",
        "/opt/VirtualGL",
        "/etc/",
    ] {
        s.mounts[0].target = target.into();
        assert!(s.validate().is_err(), "{target}");
    }
    s.mounts[0].target = "/opt/game".into();
    assert!(s.validate().is_ok());
    s.mounts[0].target = "/home/desktop".into();
    s.mounts[0].readonly = true;
    assert!(s.validate().is_err());
    s.mounts[0].readonly = false;
    assert!(s.validate().is_ok());
    s.name = "\u{00c7}".repeat(100);
    assert!(s.validate().is_ok());
}
#[tokio::test]
async fn untrusted_host_is_rejected_even_with_matching_origin_and_no_login() {
    let app = app(false);
    let response = router(app)
        .oneshot(
            Request::builder()
                .uri("/api/session")
                .header("host", "evil.example:8080")
                .header("origin", "http://evil.example:8080")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}
#[test]
fn host_validation_accepts_local_ips_and_explicit_lan_names() {
    let mut app = app(false);
    Arc::get_mut(&mut app)
        .unwrap()
        .config
        .allowed_hosts
        .push("desktop.lan".into());
    for host in [
        "localhost:8080",
        "127.0.0.1:8088",
        "192.168.1.4:8080",
        "[::1]:8080",
        "[fd00::1]:8080",
        "desktop.lan:8443",
    ] {
        let mut h = axum::http::HeaderMap::new();
        h.insert("host", host.parse().unwrap());
        assert!(app.host_allowed(&h), "{host}");
    }
    for host in [
        "example.org",
        "localhost.evil.com",
        "1.1.1.1",
        "user@localhost",
        "0.0.0.0",
    ] {
        let mut h = axum::http::HeaderMap::new();
        h.insert("host", host.parse().unwrap());
        assert!(!app.host_allowed(&h), "{host}");
    }
}
#[tokio::test]
async fn logout_revokes_waiting_media_lease_and_expiry_ends_it() {
    let app = app(true);
    let expiry = Instant::now() + std::time::Duration::from_secs(60);
    app.sessions.lock().unwrap().insert("media".into(), expiry);
    let lease = app.session_revoked(Some("media".into()));
    tokio::pin!(lease);
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(5), &mut lease)
            .await
            .is_err()
    );
    let response = router(app.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/logout")
                .header("host", "localhost:8080")
                .header("cookie", "wsld_session=media")
                .header("x-wsld-request", "1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    tokio::time::timeout(std::time::Duration::from_millis(100), lease)
        .await
        .unwrap();
    app.sessions.lock().unwrap().insert(
        "short".into(),
        Instant::now() + std::time::Duration::from_millis(15),
    );
    tokio::time::timeout(
        std::time::Duration::from_millis(100),
        app.session_revoked(Some("short".into())),
    )
    .await
    .unwrap();
}
#[tokio::test]
async fn unrelated_jobs_progress_while_an_environment_queue_is_blocked() {
    let app = app(false);
    let lock = app.resource_lock("environment:one");
    let guard = lock.lock().await;
    let queued = jobs::launch(
        app.clone(),
        "stop",
        "environment:one".into(),
        |_, _| async { Ok(serde_json::json!({})) },
    )
    .unwrap()
    .0["job_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let free = jobs::launch(
        app.clone(),
        "start",
        "environment:two".into(),
        |_, _| async { Ok(serde_json::json!({})) },
    )
    .unwrap()
    .0["job_id"]
        .as_str()
        .unwrap()
        .to_owned();
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            if app.db.get::<model::Job>("job", &free).unwrap().status == "completed" {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        app.db.get::<model::Job>("job", &queued).unwrap().status,
        "queued"
    );
    drop(guard);
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            if app.db.get::<model::Job>("job", &queued).unwrap().status == "completed" {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
#[test]
fn job_list_is_bounded_and_strips_logs_before_deserialization() {
    let app = app(false);
    for i in 0..120 {
        let id = i.to_string();
        let j = model::Job {
            id: id.clone(),
            kind: "build".into(),
            resource: None,
            status: "completed".into(),
            created_at: 1,
            finished_at: Some(2),
            log: "x".repeat(262144),
            result: None,
        };
        app.db.put("job", &id, &j).unwrap();
    }
    let jobs = app.db.jobs_summary().unwrap();
    assert_eq!(jobs.len(), 100);
    assert_eq!(jobs[0].id, "119");
    assert!(jobs.iter().all(|j| j.log.is_empty()));
}

#[test]
fn profile_catalog_is_unique_and_metadata_is_explicit() {
    let catalog = profiles::catalog();
    assert_eq!(catalog.len(), 2);
    assert_eq!(
        catalog
            .iter()
            .map(|p| &p.id)
            .collect::<std::collections::HashSet<_>>()
            .len(),
        catalog.len()
    );
    assert!(
        catalog
            .iter()
            .any(|p| p.reference == profiles::DEFAULT_IMAGE && p.session == "xfce")
    );
    assert!(
        catalog
            .iter()
            .any(|p| p.id == "ubuntu-gnome" && p.session == "gnome")
    );
    assert!(
        profiles::from_image(
            &serde_json::json!({"Config":{"Labels":{"io.wsl-desktop.desktop":"true"}}})
        )
        .is_none()
    );
    let info = profiles::from_image(&serde_json::json!({"Config":{"Labels":{"io.wsl-desktop.profile":"ubuntu-gnome","io.wsl-desktop.session":"gnome","org.opencontainers.image.title":"Ubuntu 24.04 · GNOME"}}})).unwrap();
    assert_eq!(info.session, "gnome");
}
#[test]
fn old_environment_records_remain_readable_without_profile_metadata() {
    let value = serde_json::json!({"id":"old","container":"wsld-old","private_network":"private","egress_network":"egress","source_image_id":"sha256:old","created_at":1,"spec":{"name":"Old XFCE","image":"wsl-desktop/ubuntu:24.04"}});
    let env: model::Environment = serde_json::from_value(value).unwrap();
    assert!(env.desktop.is_none());
    assert_eq!(env.source_image_id, "sha256:old");
}
#[tokio::test]
async fn unknown_profile_build_is_rejected_without_queuing_docker_work() {
    let app = app(false);
    let response = router(app.clone())
        .oneshot(
            Request::builder()
                .header("host", "localhost:8080")
                .method("POST")
                .uri("/api/profiles/not-a-profile/build")
                .header("x-wsld-request", "1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert!(app.db.list::<model::Job>("job").unwrap().is_empty());
}
#[test]
fn command_option_injection_is_rejected() {
    for bad in [
        "--privileged",
        "ubuntu; echo secret",
        "a\nb",
        "a,b",
        "",
        "$(id)",
    ] {
        assert!(model::image_ref(bad).is_err());
    }
    for valid in [
        "ubuntu:24.04",
        "registry.local:5000/me/desktop:latest",
        "sha256:abcdef",
        "repo@sha256:abcdef",
    ] {
        assert!(model::image_ref(valid).is_ok());
    }
}
#[test]
fn database_round_trip_keeps_snapshot_parent() {
    let db = db::Db::open(std::path::Path::new(":memory:")).unwrap();
    let record = model::ImageRecord {
        id: "a".into(),
        name: "Snapshot".into(),
        image_id: "sha256:new".into(),
        reference: "snapshot:a".into(),
        parent_image_id: Some("sha256:base".into()),
        source_environment: Some("env".into()),
        kind: "snapshot".into(),
        created_at: 1,
        description: String::new(),
        desktop: None,
    };
    db.put("image", "a", &record).unwrap();
    assert_eq!(
        db.get::<model::ImageRecord>("image", "a")
            .unwrap()
            .parent_image_id,
        record.parent_image_id
    );
    assert_eq!(db.list::<model::ImageRecord>("image").unwrap().len(), 1);
    db.remove("image", "a").unwrap();
    assert!(db.list::<model::ImageRecord>("image").unwrap().is_empty());
}
#[test]
fn websocket_origin_requires_exact_host_and_port() {
    let mut h = axum::http::HeaderMap::new();
    h.insert("host", "localhost:8080".parse().unwrap());
    h.insert("origin", "http://localhost:8080".parse().unwrap());
    assert!(auth::origin_matches(&h));
    h.insert("origin", "https://evil.example".parse().unwrap());
    assert!(!auth::origin_matches(&h));
    h.insert("origin", "http://localhost:8081".parse().unwrap());
    assert!(!auth::origin_matches(&h));
    h.remove("origin");
    assert!(!auth::origin_matches(&h));
}
#[tokio::test]
async fn protected_routes_require_login_before_touching_docker() {
    let router = router(app(true));
    for path in [
        "/api/environments",
        "/api/profiles",
        "/api/environments/unknown/screenshot",
        "/api/environments/unknown/vnc",
        "/api/jobs",
        "/api/images",
        "/api/volumes",
    ] {
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .header("host", "localhost:8080")
                    .uri(path)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{path}");
    }
}
#[tokio::test]
async fn login_disabled_does_not_disable_csrf_protection() {
    let router = router(app(false));
    let response = router
        .oneshot(
            Request::builder()
                .header("host", "localhost:8080")
                .method("POST")
                .uri("/api/environments")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}
#[tokio::test]
async fn valid_session_still_rejects_cross_origin_mutation() {
    let app = app(true);
    app.sessions.lock().unwrap().insert(
        "token".into(),
        Instant::now() + std::time::Duration::from_secs(60),
    );
    let response = router(app)
        .oneshot(
            Request::builder()
                .header("host", "localhost:8080")
                .method("POST")
                .uri("/api/logout")
                .header("cookie", "wsld_session=token")
                .header("x-wsld-request", "1")
                .header("host", "localhost:8080")
                .header("origin", "https://evil.example")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}
#[tokio::test]
async fn expired_session_is_not_accepted() {
    let app = app(true);
    app.sessions.lock().unwrap().insert(
        "expired".into(),
        Instant::now() - std::time::Duration::from_secs(1),
    );
    let response = router(app)
        .oneshot(
            Request::builder()
                .header("host", "localhost:8080")
                .uri("/api/jobs")
                .header("cookie", "wsld_session=expired")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
