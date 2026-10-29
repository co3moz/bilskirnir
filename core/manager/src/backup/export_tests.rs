//! Pins down a backup as it streams out: its entries in order, each part the exact bytes Docker or
//! the volume helper wrote, in chunks with a length and digest at its end, a commit of a
//! container's files made for the backup and removed after it, the manager's own volume never
//! read, and no secret value unless it was asked for.
use super::*;
use crate::backup::{
    format::{Item, Reader},
    tests::{SECRET, calls, container, endpoint, fake_docker, image, job, pinned},
};

/// The installation the tests back up: a container with its files and a volume, a saved image,
/// its compose with a secret value, and its endpoint. The image is larger than one chunk.
#[cfg(unix)]
fn installation() -> (Arc<App>, std::path::PathBuf) {
    let (app, dir) = fake_docker(
        r#"  'image ls -a --no-trunc --format {{.ID}}') echo sha256:image-i1 ;;
  'ps -a --format {{.Names}}') echo thoric-c1 ;;
  'container inspect thoric-c1') cat c1.json ;;
  'volume ls --format {{.Name}}') printf 'projects\nthoric-data\n' ;;
  'volume ls --format {{json .}}') printf '{"Name":"projects"}\n{"Name":"thoric-data"}\n' ;;
  'volume ls --filter label=thoric.managed=true --format {{.Name}}') printf 'projects\nthoric-data\n' ;;
  'volume inspect projects') echo '[{"Name":"projects","Labels":{"thoric.managed":"true"}}]' ;;
  'save sha256:image-i1') head -c 5000000 /dev/zero | tr '\0' 'i' ;;
  'commit '*) echo sha256:commit-c1 ;;
  'save sha256:commit-c1') printf 'files of c1' ;;
  'image rm sha256:commit-c1') echo Deleted ;;
  'run '*) printf 'volume tar of projects' ;;"#,
    );
    std::fs::write(
        dir.join("c1.json"),
        serde_json::json!([{
            "Id": "cid1",
            "Config": {"Hostname": "host-c1", "Labels": {"thoric.managed": "true", "thoric.environment": "c1"}},
            "State": {"Running": true},
            "NetworkSettings": {"Networks": {}},
        }])
        .to_string(),
    )
    .unwrap();
    app.db
        .put(
            CONTAINERS,
            "c1",
            &container("c1", "Desk", "thoric/custom:i1", &["projects"]),
        )
        .unwrap();
    app.db
        .put("image", "i1", &image("i1", "Tools", "custom"))
        .unwrap();
    app.db.put(PINNED, "c1", &pinned()).unwrap();
    app.db
        .put(ENDPOINTS, "desk", &endpoint("desk", &["c1"]))
        .unwrap();
    job(&app, "backup-job", "backup");
    (app, dir)
}

/// Run a backup and return what the browser received.
#[cfg(unix)]
async fn download(app: &App, included: Included) -> anyhow::Result<Vec<u8>> {
    let (body, mut chunks) = mpsc::channel::<std::io::Result<Bytes>>(4);
    let received = tokio::spawn(async move {
        let mut out = Vec::new();
        while let Some(chunk) = chunks.recv().await {
            out.extend_from_slice(&chunk.unwrap());
        }
        out
    });
    let result = app
        .back_up(
            "backup-job",
            included,
            1_700_000_000,
            &body,
            auth::Session::Cookie(None),
        )
        .await;
    drop(body);
    let archive = received.await.unwrap();
    result.map(|_| archive)
}

#[cfg(unix)]
/// The entries of a backup, in order.
fn entries(archive: &[u8]) -> Vec<(String, Vec<u8>)> {
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
    entries
}

#[cfg(unix)]
#[tokio::test]
async fn a_backup_streams_every_part_with_its_length_and_digest() {
    let (app, dir) = installation();
    let included = Included {
        images: true,
        volumes: true,
        files: true,
        secrets: false,
    };
    let archive = download(&app, included).await.unwrap();
    let entries = entries(&archive);
    let names = entries
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        [
            MANIFEST,
            RECORDS,
            "parts/0000/00000000",
            "parts/0000/00000001",
            "parts/0000/end",
            "parts/0001/00000000",
            "parts/0001/end",
            "parts/0002/00000000",
            "parts/0002/end",
            END,
        ]
    );
    let manifest: Manifest = serde_json::from_slice(&entries[0].1).unwrap();
    crate::backup::review::check_manifest(&manifest).unwrap();
    // The manager's data volume is never part of a backup.
    let carried = manifest
        .volumes
        .iter()
        .map(|v| v.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(carried, ["projects"]);
    let records: crate::backup::manifest::Records = serde_json::from_slice(&entries[1].1).unwrap();
    crate::backup::review::check_records(&manifest, &records).unwrap();

    let part = |index: usize| -> (Vec<u8>, PartEnd) {
        let prefix = format!("parts/{index:04}/");
        let data = entries
            .iter()
            .filter(|(name, _)| name.starts_with(&prefix) && !name.ends_with("/end"))
            .flat_map(|(_, data)| data.clone())
            .collect::<Vec<_>>();
        let end = entries
            .iter()
            .find(|(name, _)| *name == format!("{prefix}end"))
            .unwrap();
        (data, serde_json::from_slice(&end.1).unwrap())
    };
    for (index, expected) in [
        (0, vec![b'i'; 5_000_000]),
        (1, b"files of c1".to_vec()),
        (2, b"volume tar of projects".to_vec()),
    ] {
        let (data, end) = part(index);
        assert_eq!(data, expected, "part {index}");
        assert_eq!(end.bytes, expected.len() as u64);
        assert_eq!(end.blake2s, hex(&Blake2s256::digest(&expected)));
    }
    let ending: Ending = serde_json::from_slice(&entries.last().unwrap().1).unwrap();
    assert_eq!(ending.parts, 3);

    let unpacked = entries
        .iter()
        .flat_map(|(_, data)| data.clone())
        .collect::<Vec<_>>();
    assert!(!String::from_utf8_lossy(&unpacked).contains(SECRET));
    let calls = calls(&dir);
    // The commit made for the backup goes again; the volume is read on a read-only mount.
    assert!(
        calls.contains(
            "commit --change LABEL thoric.managed=true --message Thoric backup of Desk cid1"
        ),
        "{calls}"
    );
    assert!(calls.contains("image rm sha256:commit-c1"), "{calls}");
    let helper = calls.lines().find(|call| call.starts_with("run ")).unwrap();
    for flag in [
        "--network none",
        "--cap-add DAC_READ_SEARCH",
        "src=projects,dst=/volume,readonly",
    ] {
        assert!(helper.contains(flag), "{helper}");
    }
    assert!(!calls.contains("src=thoric-data"), "{calls}");
    let job: crate::model::Job = app.db.job("backup-job").unwrap();
    assert!(!job.log.contains(SECRET));
    let stages = job
        .stages
        .iter()
        .map(|s| (s.name.as_str(), s.started_at.is_some()))
        .collect::<Vec<_>>();
    assert_eq!(
        stages,
        [
            ("plan", true),
            ("images", true),
            ("files", true),
            ("volumes", true)
        ]
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn secret_values_leave_only_in_a_backup_that_asks_for_them() {
    let (app, dir) = installation();
    let included = Included {
        secrets: true,
        ..Included::default()
    };
    let archive = download(&app, included).await.unwrap();
    let entries = entries(&archive);
    // Without the optional parts, the records alone.
    let names = entries
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(names, [MANIFEST, RECORDS, END]);
    assert!(String::from_utf8_lossy(&entries[1].1).contains(SECRET));
    assert!(!String::from_utf8_lossy(&entries[0].1).contains(SECRET));
    let job: crate::model::Job = app.db.job("backup-job").unwrap();
    assert!(!job.log.contains(SECRET));
    assert!(
        job.log
            .contains("It includes the values of secret parameters")
    );
    assert!(!calls(&dir).contains("commit"));
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn a_failing_part_fails_the_download_instead_of_ending_it_short() {
    let (app, dir) = installation();
    // Docker cannot save the image.
    std::fs::write(
        dir.join("docker"),
        std::fs::read_to_string(dir.join("docker"))
            .unwrap()
            .replace(
                "head -c 5000000 /dev/zero | tr '\\0' 'i'",
                "printf partial; echo 'no such image' >&2; exit 1",
            ),
    )
    .unwrap();
    let included = Included {
        images: true,
        ..Included::default()
    };
    let error = download(&app, included).await.unwrap_err();
    assert!(error.to_string().contains("no such image"), "{error:#}");
    std::fs::remove_dir_all(dir).unwrap();
}

/// A record this release cannot read is not left out of a backup that would then look whole: the
/// backup fails before its first entry and names the record, without its data.
#[cfg(unix)]
#[tokio::test]
async fn a_record_that_does_not_read_fails_the_backup_instead_of_going_missing() {
    let (app, dir) = installation();
    app.db
        .put(
            CONTAINERS,
            "c2",
            &serde_json::json!({"id": "c2", "spec": SECRET}),
        )
        .unwrap();
    let error = format!(
        "{:#}",
        download(&app, Included::default()).await.unwrap_err()
    );
    assert!(
        error.contains(&format!("{CONTAINERS} record c2")),
        "{error}"
    );
    assert!(!error.contains(SECRET), "{error}");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_parts_digest_is_blake2s_256_as_earlier_backups_wrote_it() {
    // The BLAKE2s-256 example of RFC 7693, so a backup written before checks out on restore.
    assert_eq!(
        hex(&Blake2s256::digest(b"abc")),
        "508c5e8c327c14e2e1a72ba34eeb452f37458b209ed63a294d999b4c86675982"
    );
}
