//! Pins down how an image's space is explained: Docker's history becomes readable instructions in
//! build order, steps are matched to the layers they wrote, a step belongs to the image it came
//! from even when a rebuild changed the history above identical layers, own sizes add up to every
//! layer counted once, and each image's layers are read from Docker only once.
use super::*;
use axum::{body::Body, http::Request};
use tower::ServiceExt;

/// One line of `docker image history --no-trunc --human=false --format '{{json .}}'`.
fn line(created_by: &str, size: u64, created_at: &str, comment: &str) -> String {
    json!({
        "Comment": comment,
        "CreatedAt": created_at,
        "CreatedBy": created_by,
        "CreatedSince": created_at,
        "ID": "<missing>",
        "Size": size.to_string(),
    })
    .to_string()
}

/// A history in Docker's order, the newest step first.
fn history(lines: &[String]) -> String {
    lines.iter().rev().cloned().collect::<Vec<_>>().join("\n")
}

const UBUNTU: &str = "2026-10-02T11:10:54+03:00";
const DESKTOP: &str = "2026-10-17T19:00:47+03:00";
const BUILD: &str = "2026-10-18T22:26:37+03:00";

/// Ubuntu's base image, recorded by the classic builder, then the desktop stages BuildKit
/// recorded, the oldest step first.
fn desktop_lines(desktop: &str) -> Vec<String> {
    vec![
        line("/bin/sh -c #(nop)  ARG RELEASE", 0, UBUNTU, ""),
        line(
            "/bin/sh -c #(nop) ADD file:7121eb4a in / ",
            87_633_920,
            UBUNTU,
            "",
        ),
        line("/bin/sh -c #(nop)  CMD [\"/bin/bash\"]", 0, UBUNTU, ""),
        line("ARG DEBIAN_FRONTEND=noninteractive", 0, desktop, BUILDKIT),
        line(
            "RUN |2 DEBIAN_FRONTEND=noninteractive VIRTUALGL_VERSION=3.1.4 /bin/sh -c apt-get update && apt-get install -y dbus # buildkit",
            721_317_888,
            desktop,
            BUILDKIT,
        ),
        line("LABEL bilskirnir.desktop=true", 0, desktop, BUILDKIT),
        line(
            "RUN |2 DEBIAN_FRONTEND=noninteractive VIRTUALGL_VERSION=3.1.4 /bin/sh -c apt-get install -y xfce4 # buildkit",
            88_707_072,
            desktop,
            BUILDKIT,
        ),
    ]
}

/// The steps a script build adds to its base.
fn build_lines(script: u64) -> Vec<String> {
    vec![
        line("USER root", 0, BUILD, BUILDKIT),
        line(
            "COPY setup.sh /tmp/bilskirnir-setup.sh # buildkit",
            12_288,
            BUILD,
            BUILDKIT,
        ),
        line(
            "RUN /bin/sh -c bash /tmp/bilskirnir-setup.sh && rm /tmp/bilskirnir-setup.sh # buildkit",
            script,
            BUILD,
            BUILDKIT,
        ),
        line("LABEL bilskirnir.managed=true", 0, BUILD, BUILDKIT),
    ]
}

fn image(lines: &[String], layers: &[&str]) -> Image {
    let steps = parse_history(&history(lines)).unwrap();
    let size = steps.iter().map(|step| step.size).sum();
    Image::new(
        size,
        layers.iter().map(|layer| layer.to_string()).collect(),
        steps,
    )
}

fn listed(id: &str, catalog: bool, created_at: u64, image: Image) -> Listed {
    Listed {
        id: id.into(),
        image_id: format!("sha256:{id}"),
        catalog,
        created_at,
        image: Arc::new(image),
    }
}

#[test]
fn history_reads_as_instructions_in_build_order() {
    let steps = parse_history(&history(&desktop_lines(DESKTOP))).unwrap();
    let read: Vec<(&str, StepKind, u64)> = steps
        .iter()
        .map(|step| (step.instruction.as_str(), step.kind, step.size))
        .collect();
    assert_eq!(
        read,
        [
            ("ARG RELEASE", StepKind::Config, 0),
            ("ADD file:7121eb4a in /", StepKind::Add, 87_633_920),
            ("CMD [\"/bin/bash\"]", StepKind::Config, 0),
            ("ARG DEBIAN_FRONTEND=noninteractive", StepKind::Config, 0),
            (
                "RUN apt-get update && apt-get install -y dbus",
                StepKind::Run,
                721_317_888
            ),
            ("LABEL bilskirnir.desktop=true", StepKind::Config, 0),
            ("RUN apt-get install -y xfce4", StepKind::Run, 88_707_072),
        ]
    );
    // BuildKit's comment on every step carries nothing, and is left out.
    assert!(steps.iter().all(|step| step.comment.is_empty()));
    assert_eq!(steps[1].created, Some(1_790_928_654));
    assert!(parse_history(&line("RUN x", 1, BUILD, "")).is_ok());
    assert!(
        parse_history(&json!({"CreatedBy": "RUN x", "Size": "1.2GB"}).to_string()).is_err(),
        "a human-readable size is refused, not read as zero"
    );
}

#[test]
fn every_builder_form_is_described() {
    for (created_by, comment, kind, instruction) in [
        (
            "COPY setup.sh /tmp/x # buildkit",
            BUILDKIT,
            StepKind::Copy,
            "COPY setup.sh /tmp/x",
        ),
        ("WORKDIR /app", BUILDKIT, StepKind::Config, "WORKDIR /app"),
        (
            "/bin/sh -c apt-get install -y git",
            "",
            StepKind::Run,
            "RUN apt-get install -y git",
        ),
        (
            "|1 VERSION=2 /bin/sh -c make install",
            "",
            StepKind::Run,
            "RUN make install",
        ),
        (
            "/bin/sh -c #(nop) COPY dir:abc in /srv ",
            "",
            StepKind::Copy,
            "COPY dir:abc in /srv",
        ),
        // `docker commit` records the container's command, or nothing.
        ("", "Bilskirnir snapshot of Work", StepKind::Commit, ""),
        (
            "/usr/bin/tini -- desktop-entrypoint",
            "",
            StepKind::Commit,
            "/usr/bin/tini -- desktop-entrypoint",
        ),
    ] {
        assert_eq!(
            describe(created_by, comment),
            (kind, instruction.to_owned()),
            "{created_by}"
        );
    }
    let long = format!("RUN /bin/sh -c echo {} # buildkit", "é".repeat(3000));
    let steps = parse_history(&line(&long, 1, BUILD, BUILDKIT)).unwrap();
    assert!(steps[0].instruction.len() <= INSTRUCTION_LIMIT + '…'.len_utf8());
    assert!(steps[0].instruction.ends_with('…'));
}

#[test]
fn rfc_3339_times_become_unix_seconds() {
    for (text, seconds) in [
        ("1970-01-01T00:00:00Z", Some(0)),
        ("2026-10-18T22:26:37+03:00", Some(1_792_351_597)),
        ("2026-10-18T19:26:37Z", Some(1_792_351_597)),
        ("2026-10-18T19:26:37.123456789Z", Some(1_792_351_597)),
        ("2026-10-18T14:26:37-05:00", Some(1_792_351_597)),
        ("2024-02-29T00:00:00Z", Some(1_709_164_800)),
        // Docker's zero time, for an image that records none.
        ("0001-01-01T00:00:00Z", None),
        ("2026-13-01T00:00:00Z", None),
        ("2026-10-18T22:26:37", None),
        ("2026-10-18T22:26:37+3:00", None),
        ("2 weeks ago", None),
        ("", None),
    ] {
        assert_eq!(unix_time(text), seconds, "{text}");
    }
}

#[test]
fn inspect_output_gives_the_size_and_the_layers() {
    assert_eq!(
        parse_inspect("2630695169 [\"sha256:a\",\"sha256:b\"]\n").unwrap(),
        (2_630_695_169, vec!["sha256:a".into(), "sha256:b".into()])
    );
    assert_eq!(parse_inspect("12 null").unwrap(), (12, vec![]));
    assert!(parse_inspect("unexpected").is_err());
}

#[test]
fn steps_that_wrote_bytes_are_the_layers_and_an_empty_commit_counts_when_it_must() {
    let built = image(
        &[desktop_lines(DESKTOP), build_lines(1_000)].concat(),
        &["base", "deps", "xfce", "copy", "script"],
    );
    let layers: Vec<&str> = built
        .steps
        .iter()
        .filter(|step| step.layer)
        .map(|step| step.instruction.as_str())
        .collect();
    assert_eq!(layers.len(), 5);
    assert!(layers[0].starts_with("ADD"));
    assert_eq!(built.steps.last().unwrap().chain, 5);
    assert_eq!(
        built.layers_size(),
        87_633_920 + 721_317_888 + 88_707_072 + 12_288 + 1_000
    );

    // A snapshot of a container that changed nothing still wrote a layer, of no bytes.
    let mut lines = desktop_lines(DESKTOP);
    lines.push(line("", 0, BUILD, "Bilskirnir snapshot of Idle"));
    let snapshot = image(&lines, &["base", "deps", "xfce", "empty"]);
    let commit = snapshot.steps.last().unwrap();
    assert!(commit.layer);
    assert_eq!(commit.chain, 4);

    // When neither count matches, only the steps with bytes are layers.
    let mut odd = desktop_lines(DESKTOP);
    odd.push(line("RUN /bin/sh -c true # buildkit", 0, BUILD, BUILDKIT));
    odd.push(line("", 0, BUILD, ""));
    let guessed = image(&odd, &["base", "deps", "xfce", "one-of-them"]);
    assert_eq!(guessed.steps.iter().filter(|step| step.layer).count(), 3);
}

/// The catalog image as Docker has it now, and two script builds made on an earlier build of it
/// whose base layer is the same and whose desktop layers are not.
fn rebuilt_catalog() -> Vec<Listed> {
    vec![
        listed(
            "ubuntu-xfce",
            true,
            0,
            image(
                &desktop_lines("2026-10-19T00:25:36+03:00"),
                &["base", "deps", "xfce"],
            ),
        ),
        listed(
            "first",
            false,
            10,
            image(
                &[desktop_lines(DESKTOP), build_lines(1_000)].concat(),
                &["base", "deps-old", "xfce-old", "copy-1", "script-1"],
            ),
        ),
        listed(
            "second",
            false,
            20,
            image(
                &[desktop_lines(DESKTOP), build_lines(2_000)].concat(),
                &["base", "deps-old", "xfce-old", "copy-2", "script-2"],
            ),
        ),
    ]
}

#[test]
fn a_step_belongs_to_the_image_it_came_from_by_its_layers() {
    let images = rebuilt_catalog();
    let second = origins(&images, 2);
    let from = |instruction: &str| {
        let index = images[2]
            .image
            .steps
            .iter()
            .position(|step| step.instruction.starts_with(instruction))
            .unwrap();
        second[index].from.map(|other| images[other].id.as_str())
    };
    // The base layer is the catalog image's although the rebuild changed the history above it.
    assert_eq!(from("ADD"), Some("ubuntu-xfce"));
    assert_eq!(from("CMD"), Some("ubuntu-xfce"));
    // The earlier desktop layers are only in the two builds, and the older one brought them.
    assert_eq!(from("RUN apt-get update"), Some("first"));
    assert_eq!(from("RUN apt-get install -y xfce4"), Some("first"));
    // The setup script's layers are its own, and so is the label on top of them.
    assert_eq!(from("COPY"), None);
    assert_eq!(from("RUN bash"), None);
    assert_eq!(from("LABEL bilskirnir.managed"), None);
    let add = images[2]
        .image
        .steps
        .iter()
        .position(|step| step.instruction.starts_with("ADD"))
        .unwrap();
    assert_eq!(second[add].shared_with, [0, 1]);

    // The older build keeps the desktop layers it shares with the newer one.
    let first = origins(&images, 1);
    assert_eq!(
        own_size(&images[1].image, &first),
        721_317_888 + 88_707_072 + 12_288 + 1_000
    );
    assert_eq!(own_size(&images[2].image, &second), 12_288 + 2_000);

    // Own sizes count every distinct layer once.
    let total: u64 = (0..images.len())
        .map(|index| own_size(&images[index].image, &origins(&images, index)))
        .sum();
    assert_eq!(
        total,
        87_633_920 + 2 * (721_317_888 + 88_707_072) + 2 * 12_288 + 1_000 + 2_000
    );
}

#[test]
fn a_snapshot_of_a_snapshot_takes_its_parents_layer_from_the_parent() {
    let mut first = desktop_lines(DESKTOP);
    first.push(line("", 5_000, BUILD, "Bilskirnir snapshot of Work"));
    let mut second = first.clone();
    second.push(line("", 7_000, BUILD, "Bilskirnir snapshot of Work"));
    let images = vec![
        listed(
            "ubuntu-xfce",
            true,
            0,
            image(&desktop_lines(DESKTOP), &["base", "deps", "xfce"]),
        ),
        listed(
            "parent",
            false,
            10,
            image(&first, &["base", "deps", "xfce", "work-1"]),
        ),
        listed(
            "child",
            false,
            20,
            image(&second, &["base", "deps", "xfce", "work-1", "work-2"]),
        ),
    ];
    let report = report(&images, 2);
    let steps = report["steps"].as_array().unwrap();
    // Newest first, as `docker history` lists them.
    assert_eq!(steps[0]["size"], 7_000);
    assert_eq!(steps[0]["from"], Value::Null);
    assert_eq!(steps[0]["kind"], "commit");
    assert_eq!(steps[0]["comment"], "Bilskirnir snapshot of Work");
    assert_eq!(steps[1]["from"], "parent");
    assert_eq!(steps[1]["shared_with"], json!(["parent"]));
    assert_eq!(steps.last().unwrap()["from"], "ubuntu-xfce");
    assert_eq!(steps.last().unwrap()["instruction"], "ARG RELEASE");
    assert_eq!(report["own_size"], 7_000);
    assert_eq!(report["image_id"], "sha256:child");
    assert_eq!(
        report["layers_size"],
        87_633_920 + 721_317_888 + 88_707_072 + 5_000 + 7_000
    );
    // The parent's own layer is shared with the child it has, and stays the parent's.
    let parent = origins(&images, 1);
    assert_eq!(parent.last().unwrap().from, None);
    assert_eq!(parent.last().unwrap().shared_with, [2]);
}

#[tokio::test]
async fn without_docker_sizes_are_null_and_layers_are_refused() {
    let app = crate::tests::app(false);
    let list = crate::router(app.clone())
        .oneshot(
            Request::get("/api/images")
                .header("host", "localhost:8080")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list.status(), StatusCode::OK);
    let body = http_body_util::BodyExt::collect(list.into_body())
        .await
        .unwrap()
        .to_bytes();
    let images: Vec<Value> = serde_json::from_slice(&body).unwrap();
    assert!(!images.is_empty());
    for image in &images {
        for key in ["size", "layers_size", "own_size"] {
            assert_eq!(image[key], Value::Null, "{key}");
        }
    }
    for (path, message) in [
        (
            "/api/images/ubuntu-xfce/layers",
            "Docker does not have this image",
        ),
        ("/api/images/nothing/layers", "Record not found"),
    ] {
        let response = crate::router(app.clone())
            .oneshot(
                Request::get(path)
                    .header("host", "localhost:8080")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
        let body = http_body_util::BodyExt::collect(response.into_body())
            .await
            .unwrap()
            .to_bytes();
        assert!(String::from_utf8_lossy(&body).contains(message), "{path}");
    }
    assert!(
        include_str!("../docs/API.md").contains("`/api/images/{id}/layers`"),
        "docs/API.md must document the layers route"
    );
}

/// A Docker CLI stand-in that has the XFCE catalog image and one snapshot made from it, lists
/// the snapshot only while `present` exists, and records its arguments in `calls`.
#[cfg(unix)]
fn layered_docker(app: &mut Arc<App>) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!(
        "bilskirnir-layers-{}",
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let mut snapshot = desktop_lines(DESKTOP);
    snapshot.push(line("", 5_000, BUILD, "Bilskirnir snapshot of Work"));
    std::fs::write(dir.join("xfce.history"), history(&desktop_lines(DESKTOP))).unwrap();
    std::fs::write(dir.join("snap.history"), history(&snapshot)).unwrap();
    std::fs::write(
        dir.join("xfce.json"),
        json!([{
            "Id": "sha256:xfce",
            "Config": {"Labels": {
                "bilskirnir.desktop": "true",
                "bilskirnir.profile": "ubuntu-xfce",
                "bilskirnir.session": "xfce",
                "org.opencontainers.image.title": "Ubuntu 24.04 · Lightweight XFCE",
            }},
        }])
        .to_string(),
    )
    .unwrap();
    std::fs::write(dir.join("present"), "").unwrap();
    let script = r#"#!/bin/sh
cd 'DIR'
echo "$*" >> calls
case "$*" in
  'image ls -a --no-trunc --format {{.ID}}') echo sha256:xfce; [ -e present ] && echo sha256:snap ;;
  'image inspect ghcr.io/co3moz/bilskirnir/ubuntu-xfce:24.04') cat xfce.json ;;
  'image inspect --format {{.Size}} {{json .RootFS.Layers}} sha256:xfce') echo '1200000000 ["sha256:base","sha256:deps","sha256:xfce"]' ;;
  'image inspect --format {{.Size}} {{json .RootFS.Layers}} sha256:snap') echo '1200010000 ["sha256:base","sha256:deps","sha256:xfce","sha256:work"]' ;;
  'image history --no-trunc --human=false --format {{json .}} sha256:xfce') cat xfce.history ;;
  'image history --no-trunc --human=false --format {{json .}} sha256:snap') cat snap.history ;;
  *) exit 1 ;;
esac
"#
    .replace("DIR", &dir.display().to_string());
    let docker = dir.join("docker");
    std::fs::write(&docker, script).unwrap();
    std::fs::set_permissions(&docker, std::fs::Permissions::from_mode(0o755)).unwrap();
    Arc::get_mut(app).unwrap().config.docker_bin = docker.display().to_string();
    let record = crate::model::ImageRecord {
        id: "snap".into(),
        name: "Work".into(),
        image_id: "sha256:snap".into(),
        reference: "bilskirnir/snapshot:snap".into(),
        parent_image_id: Some("sha256:xfce".into()),
        source_container: None,
        kind: "snapshot".into(),
        created_at: 10,
        description: String::new(),
        desktop: None,
        build: None,
        compose: None,
        icon: None,
        template_id: None,
    };
    app.db.put("image", "snap", &record).unwrap();
    dir
}

#[cfg(unix)]
async fn get(app: &Arc<App>, path: &str) -> Value {
    let response = crate::router(app.clone())
        .oneshot(
            Request::get(path)
                .header("host", "localhost:8080")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK, "{path}");
    let body = http_body_util::BodyExt::collect(response.into_body())
        .await
        .unwrap()
        .to_bytes();
    serde_json::from_slice(&body).unwrap()
}

#[cfg(unix)]
#[tokio::test]
async fn each_image_is_read_once_and_leaves_the_cache_with_its_image() {
    let mut app = crate::tests::app(false);
    let dir = layered_docker(&mut app);
    let images = get(&app, "/api/images").await;
    let by_id = |images: &Value, id: &str| {
        images
            .as_array()
            .unwrap()
            .iter()
            .find(|image| image["id"] == id)
            .unwrap()
            .clone()
    };
    let xfce = by_id(&images, "ubuntu-xfce");
    assert_eq!(xfce["size"], 1_200_000_000u64);
    assert_eq!(xfce["own_size"], 87_633_920 + 721_317_888 + 88_707_072);
    let snap = by_id(&images, "snap");
    assert_eq!(snap["size"], 1_200_010_000u64);
    assert_eq!(snap["own_size"], 5_000);
    assert_eq!(by_id(&images, "ubuntu-gnome")["size"], Value::Null);

    let layers = get(&app, "/api/images/snap/layers").await;
    assert_eq!(layers["steps"][0]["comment"], "Bilskirnir snapshot of Work");
    assert_eq!(layers["steps"][1]["from"], "ubuntu-xfce");
    get(&app, "/api/images").await;
    let calls = std::fs::read_to_string(dir.join("calls")).unwrap();
    assert_eq!(calls.matches("image history").count(), 2, "{calls}");
    assert_eq!(calls.matches("--format {{.Size}}").count(), 2, "{calls}");

    // Once Docker no longer has the snapshot, it has no sizes and leaves the cache.
    std::fs::remove_file(dir.join("present")).unwrap();
    let images = get(&app, "/api/images").await;
    assert_eq!(by_id(&images, "snap")["size"], Value::Null);
    assert_eq!(
        app.layers.0.lock().unwrap().keys().collect::<Vec<_>>(),
        ["sha256:xfce"]
    );
    std::fs::remove_dir_all(dir).unwrap();
}
