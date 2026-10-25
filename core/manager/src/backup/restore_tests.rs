//! Pins down a restore from end to end against a fake Docker: a backup another installation made
//! is checked before anything is created, its images load through the import filter and get
//! Bilskirnir's own tags, its volume is unpacked into a new volume, its records are saved with the
//! IDs Docker gave the images here, and its container is created from its files with its machine
//! identity and left stopped. A name already in use refuses it, and a damaged part takes back
//! everything the restore had created.
use super::*;
use crate::{
    backup::{
        format::{END, Item, Reader, Writer},
        manifest::{Included, PartEnd},
        tests::{SECRET, calls, container, endpoint, fake_docker, image, job, pinned, saved_image},
    },
    model::WorkingImage,
};
use axum::body::Bytes;

/// A backup of an installation with a container, its files and volume, a saved image it starts
/// from, its compose and its endpoint, made by the backup job against a fake Docker.
#[cfg(unix)]
async fn backup() -> Vec<u8> {
    let (app, dir) = fake_docker(
        r#"  'image ls -a --no-trunc --format {{.ID}}') echo sha256:image-i1 ;;
  'ps -a --format {{.Names}}') echo bilskirnir-c1 ;;
  'container inspect bilskirnir-c1') cat c1.json ;;
  'volume ls --format {{.Name}}') echo projects ;;
  'volume ls --format {{json .}}') echo '{"Name":"projects"}' ;;
  'volume ls --filter label=bilskirnir.managed=true --format {{.Name}}') echo projects ;;
  'volume inspect projects') echo '[{"Name":"projects","Labels":{"bilskirnir.managed":"true"}}]' ;;
  'save sha256:image-i1') cat image.tar ;;
  'commit '*) echo sha256:commit-c1 ;;
  'save sha256:commit-c1') cat files.tar ;;
  'image rm sha256:commit-c1') ;;
  'run '*) printf 'volume tar' ;;"#,
    );
    let desktop = json!({"bilskirnir.desktop": "true"});
    std::fs::write(
        dir.join("image.tar"),
        saved_image(desktop.clone(), "evil/name:latest", b"the image's layer"),
    )
    .unwrap();
    std::fs::write(
        dir.join("files.tar"),
        saved_image(desktop, "other/name:1", b"the desk's files"),
    )
    .unwrap();
    std::fs::write(
        dir.join("c1.json"),
        json!([{
            "Id": "cid1",
            "Config": {"Hostname": "host-c1", "Labels": {"bilskirnir.managed": "true", "bilskirnir.environment": "c1"}},
            "NetworkSettings": {"Networks": {}},
        }])
        .to_string(),
    )
    .unwrap();
    let mut desk = container("c1", "Desk", "bilskirnir/build:i1", &["projects"]);
    desk.source_image_id = "sha256:image-i1".into();
    app.db.put(CONTAINERS, "c1", &desk).unwrap();
    app.db
        .put("image", "i1", &image("i1", "Tools", "build"))
        .unwrap();
    app.db.put(PINNED, "c1", &pinned()).unwrap();
    app.db
        .put(ENDPOINTS, "desk", &endpoint("desk", &["c1"]))
        .unwrap();
    job(&app, "backup-job", "backup");
    let (body, mut chunks) = tokio::sync::mpsc::channel::<std::io::Result<Bytes>>(4);
    let received = tokio::spawn(async move {
        let mut out = Vec::new();
        while let Some(chunk) = chunks.recv().await {
            out.extend_from_slice(&chunk.unwrap());
        }
        out
    });
    let included = Included {
        images: true,
        volumes: true,
        files: true,
        secrets: false,
    };
    app.back_up(
        "backup-job",
        included,
        1,
        &body,
        auth::Session::Cookie(None),
    )
    .await
    .unwrap();
    drop(body);
    std::fs::remove_dir_all(dir).unwrap();
    received.await.unwrap()
}

/// A fresh installation's fake Docker, which keeps what the restore makes in files.
#[cfg(unix)]
fn fresh() -> (Arc<App>, std::path::PathBuf) {
    let (app, dir) = fake_docker(
        r#"  'ps -a --format {{.Names}}') cat names 2>/dev/null; true ;;
  'network ls --format {{.Name}}') ;;
  'volume ls --format {{.Name}}') for v in vol-*; do [ -e "$v" ] && echo "${v#vol-}"; done; true ;;
  'image ls --format {{.Repository}}:{{.Tag}}') cat tags 2>/dev/null; true ;;
  'load --quiet') n=$(( $(cat count 2>/dev/null || echo 0) + 1 )); echo $n > count; cat > load-$n; echo "Loaded image ID: sha256:loaded-$n" ;;
  'image inspect sha256:loaded-'*) echo "[{\"Id\":\"$3\",\"Config\":{\"Labels\":{\"bilskirnir.desktop\":\"true\"}},\"RepoTags\":[]}]" ;;
  'image tag '*) echo "$4" >> tags ;;
  'image rm '*) echo "$3" >> removed ;;
  'volume inspect projects') [ -f vol-projects ] && echo '[{"Name":"projects","Labels":{"bilskirnir.managed":"true"}}]' || exit 1 ;;
  'volume create '*) eval "last=\${$#}"; touch "vol-$last"; echo "$last" ;;
  'volume rm '*) rm -f "vol-$3" ;;
  'run '*) echo ready; cat > volume-data ;;
  'network inspect net-'*) echo "[{\"Id\":\"$3\"}]" ;;
  'network create '*) eval "last=\${$#}"; echo "net-$last" ;;
  'network connect '*) ;;
  'create '*) echo created ;;
  'cp - bilskirnir-c1:/') cat > /dev/null ;;"#,
    );
    job(&app, "restore-job", "restore");
    (app, dir)
}

/// Restore `archive` and return how the upload was answered with the job's outcome.
#[cfg(unix)]
async fn restore(app: &App, archive: Vec<u8>) -> (Option<Result<()>>, anyhow::Result<Value>) {
    let (accepted, was_accepted) = oneshot::channel();
    let size = archive.len() as u64;
    let outcome = app
        .restore_backup(
            "restore-job",
            Some(size),
            Body::from(archive),
            auth::Session::Cookie(None),
            accepted,
        )
        .await;
    (was_accepted.await.ok(), outcome)
}

#[cfg(unix)]
/// The backup's entries written again, each through `edit`, which may drop one.
fn rewritten(archive: &[u8], edit: impl Fn(&str, Vec<u8>) -> Option<Vec<u8>>) -> Vec<u8> {
    let mut reader = Reader::new(u64::MAX);
    let mut items = Vec::new();
    reader.push(archive, &mut items).unwrap();
    reader.finish(&mut items).unwrap();
    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    for item in items {
        match item {
            Item::Begin { name, .. } => entries.push((name, Vec::new())),
            Item::Data(data) => entries.last_mut().unwrap().1.extend(data),
            Item::Finish => {}
        }
    }
    let mut writer = Writer::new(1);
    let mut out = Vec::new();
    for (name, data) in entries {
        if let Some(data) = edit(&name, data) {
            out.extend(writer.entry(&name, &data, false).unwrap());
        }
    }
    out.extend(writer.finish().unwrap());
    out
}

#[cfg(unix)]
#[tokio::test]
async fn a_backup_comes_back_with_its_files_identity_and_tokens() {
    let archive = backup().await;
    let (app, dir) = fresh();
    let (accepted, outcome) = restore(&app, archive).await;
    let result = outcome.unwrap();
    assert!(matches!(accepted, Some(Ok(()))));
    assert_eq!(result["created"], json!(["c1"]));
    assert_eq!(result["volumes"], json!(["projects"]));
    assert_eq!(result["secrets_left_out"], json!(["c1"]));

    // The image and the files loaded without the names their archives carried, and took
    // Bilskirnir's own tags.
    let image = std::fs::read(dir.join("load-1")).unwrap();
    assert!(String::from_utf8_lossy(&image).contains("the image's layer"));
    assert!(!String::from_utf8_lossy(&image).contains("evil/name"));
    assert!(
        !String::from_utf8_lossy(&std::fs::read(dir.join("load-2")).unwrap())
            .contains("other/name")
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("tags")).unwrap(),
        "bilskirnir/custom:i1\nbilskirnir/container:restored-c1\n"
    );
    assert_eq!(
        std::fs::read(dir.join("volume-data")).unwrap(),
        b"volume tar"
    );

    let restored: ImageRecord = app.db.get("image", "i1").unwrap();
    assert_eq!(
        (restored.image_id.as_str(), restored.reference.as_str()),
        ("sha256:loaded-1", "bilskirnir/custom:i1")
    );
    let desk: Container = app.db.get(CONTAINERS, "c1").unwrap();
    let before = container("c1", "Desk", "bilskirnir/build:i1", &["projects"]);
    assert_eq!(desk.spec.hostname, before.spec.hostname);
    assert_eq!(desk.spec.mac_address, before.spec.mac_address);
    assert_eq!(desk.spec.machine_id, before.spec.machine_id);
    assert_eq!(desk.spec.image, "bilskirnir/custom:i1");
    assert_eq!(desk.source_image_id, "sha256:loaded-1");
    assert_eq!(
        desk.working_image,
        Some(WorkingImage {
            image_id: "sha256:loaded-2".into(),
            reference: "bilskirnir/container:restored-c1".into(),
        })
    );
    assert_eq!(desk.last_error, None);
    let pinned: crate::compose::Pinned = app.db.get(PINNED, "c1").unwrap();
    assert_eq!(
        pinned.values.get("EMAIL").map(String::as_str),
        Some("a@example.com")
    );
    assert!(!pinned.values.contains_key("PASSWORD"));
    let desk_endpoint: Endpoint = app.db.get(ENDPOINTS, "desk").unwrap();
    assert!(desk_endpoint.token_hash.is_some());

    // Created from its files with its identity, and not started.
    let calls = calls(&dir);
    let created = calls
        .lines()
        .find(|call| call.starts_with("create "))
        .unwrap();
    for flag in [
        "--name bilskirnir-c1",
        "--hostname host-c1",
        "--mac-address 02:42:ac:11:00:02",
        "sha256:loaded-2",
    ] {
        assert!(created.contains(flag), "{created}");
    }
    assert!(calls.contains("cp - bilskirnir-c1:/"), "{calls}");
    assert!(
        !calls.lines().any(|call| call.starts_with("start")),
        "{calls}"
    );
    let job: crate::model::Job = app.db.get("job", "restore-job").unwrap();
    assert!(!job.log.contains(SECRET));
    let stages = job
        .stages
        .iter()
        .map(|s| s.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        stages,
        [
            "verify",
            "images",
            "files",
            "volumes",
            "records",
            "containers"
        ]
    );
    assert!(job.stages.iter().all(|stage| stage.started_at.is_some()));
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn a_name_already_in_use_refuses_the_restore_before_anything_is_created() {
    let archive = backup().await;
    let (app, dir) = fresh();
    std::fs::write(dir.join("names"), "bilskirnir-c1\n").unwrap();
    let (accepted, outcome) = restore(&app, archive).await;
    let Some(Err(refused)) = accepted else {
        panic!("{accepted:?}");
    };
    assert_eq!(refused.0, StatusCode::CONFLICT);
    assert!(refused.1.contains("container Desk"), "{}", refused.1);
    assert!(outcome.is_err());
    let calls = calls(&dir);
    assert!(
        !calls.contains("load") && !calls.contains("volume create"),
        "{calls}"
    );
    assert!(app.db.list::<Container>(CONTAINERS).unwrap().is_empty());
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn a_damaged_part_takes_back_what_the_restore_created() {
    let archive = backup().await;
    // The volume's digest no longer matches its data.
    let damaged = rewritten(&archive, |name, data| {
        Some(if name == "parts/0002/end" {
            let mut end: PartEnd = serde_json::from_slice(&data).unwrap();
            end.blake2s = "00".repeat(32);
            serde_json::to_vec(&end).unwrap()
        } else {
            data
        })
    });
    let (app, dir) = fresh();
    let (accepted, outcome) = restore(&app, damaged).await;
    assert!(accepted.is_none());
    let error = outcome.unwrap_err().to_string();
    assert!(error.contains("Part 2 of the backup is damaged"), "{error}");
    assert_eq!(
        std::fs::read_to_string(dir.join("removed")).unwrap(),
        "bilskirnir/container:restored-c1\nbilskirnir/custom:i1\n"
    );
    assert!(!dir.join("vol-projects").exists());
    assert!(app.db.list::<Container>(CONTAINERS).unwrap().is_empty());
    assert!(app.db.list::<ImageRecord>("image").unwrap().is_empty());
    std::fs::remove_dir_all(dir).unwrap();

    // A backup cut before its end is refused the same way.
    let cut = rewritten(&archive, |name, data| (name != END).then_some(data));
    let (app, dir) = fresh();
    let (accepted, outcome) = restore(&app, cut).await;
    assert!(accepted.is_none());
    assert!(outcome.unwrap_err().to_string().contains("incomplete"));
    assert!(!dir.join("vol-projects").exists());
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn a_reset_finds_the_catalog_image_a_restored_container_names() {
    let (app, dir) = fake_docker(
        r#"  'image inspect ghcr.io/co3moz/bilskirnir/ubuntu-xfce:24.04') echo '[{"Id":"sha256:catalog-here"}]' ;;"#,
    );
    job(&app, "reset-job", "reset");
    let mut env = container(
        "c1",
        "Desk",
        "ghcr.io/co3moz/bilskirnir/ubuntu-xfce:24.04",
        &[],
    );
    env.source_image_id = "sha256:catalog-elsewhere".into();
    app.source_image(&mut env, "reset-job").await.unwrap();
    assert_eq!(env.source_image_id, "sha256:catalog-here");
    // Any other image that is missing stays an error, and the record keeps its ID.
    let mut other = container("c2", "Other", "bilskirnir/custom:gone", &[]);
    other.source_image_id = "sha256:gone".into();
    let error = app.source_image(&mut other, "reset-job").await.unwrap_err();
    assert_eq!(error.to_string(), "Docker has no image sha256:gone");
    assert_eq!(other.source_image_id, "sha256:gone");
    std::fs::remove_dir_all(dir).unwrap();
}
