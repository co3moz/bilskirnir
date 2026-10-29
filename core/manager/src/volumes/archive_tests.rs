//! Pins down how a volume archive travels: an upload reaches tar as a plain archive whatever its
//! compression, never beyond the unpacked limit, and an import is refused before anything is
//! created when its request cannot work.
use super::*;
use crate::test_support::app;
use axum::http::Request;
use flate2::{Compression, write::GzEncoder};
use tower::ServiceExt;

fn gzip(data: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap()
}

/// What tar receives when the upload arrives in pieces of `size` bytes.
fn unpack(upload: &[u8], size: usize, limit: u64) -> anyhow::Result<Vec<u8>> {
    let mut unpacker = Unpacker::new(limit);
    let mut plain = Vec::new();
    for piece in upload.chunks(size) {
        plain.extend(unpacker.push(piece)?);
    }
    plain.extend(unpacker.finish()?.0);
    Ok(plain)
}

#[test]
fn tar_receives_a_plain_archive_however_the_upload_is_split() {
    let tar: Vec<u8> = (0..20_000u32).flat_map(|n| n.to_le_bytes()).collect();
    for size in [1, 2, 3, 512, GZIP_STEP, 1 << 20] {
        assert_eq!(
            unpack(&tar, size, u64::MAX).unwrap(),
            tar,
            "plain in {size}"
        );
        assert_eq!(
            unpack(&gzip(&tar), size, u64::MAX).unwrap(),
            tar,
            "gzip in {size}"
        );
    }
    // Two gzip members, as concatenated archives are, unpack as one.
    let mut joined = gzip(&tar[..1000]);
    joined.extend(gzip(&tar[1000..]));
    assert_eq!(unpack(&joined, 7, u64::MAX).unwrap(), tar);
}

#[test]
fn the_unpacked_size_is_held_to_the_limit() {
    // A highly compressed upload is refused by what it unpacks to, not by its own size.
    let bomb = gzip(&vec![0; 4 << 20]);
    assert!(bomb.len() < 64 * 1024);
    let error = unpack(&bomb, GZIP_STEP, 1 << 20).unwrap_err();
    assert!(
        error.to_string().contains("IMAGE_ARCHIVE_MAX_MB"),
        "{error}"
    );
    assert!(unpack(&bomb, GZIP_STEP, 4 << 20).is_ok());
    assert!(unpack(&[0; 100], 10, 99).is_err());
}

#[test]
fn a_broken_upload_says_so() {
    let mut cut = gzip(&[7; 100_000]);
    cut.truncate(cut.len() / 2);
    assert!(unpack(&cut, 1024, u64::MAX).is_err());
    let mut garbage = vec![0x1f, 0x8b];
    garbage.extend([0xff; 64]);
    assert!(unpack(&garbage, 8, u64::MAX).is_err());
    // A single byte is no archive; tar receives it and says so.
    assert_eq!(unpack(&[1], 1, u64::MAX).unwrap(), [1]);
}

#[test]
fn an_archive_is_read_by_root_on_a_read_only_mount() {
    let volume = Volume {
        name: "project".into(),
        image: "sha256:manager".into(),
    };
    let command = ["tar", "--create", "--file", "-"].map(str::to_owned);
    let args = helper_args(&volume, Access::Archive, "h", false, &command);
    let after = |flag: &str| {
        let at = args.iter().position(|arg| arg == flag).unwrap();
        args[at + 1].clone()
    };
    assert_eq!(after("--user"), "0:0");
    assert_eq!(after("--cap-drop"), "ALL");
    assert_eq!(after("--network"), "none");
    assert!(after("--mount").ends_with(",readonly"));
    // Reading every file is the one thing root keeps.
    let added: Vec<&String> = args
        .windows(2)
        .filter(|pair| pair[0] == "--cap-add")
        .map(|pair| &pair[1])
        .collect();
    assert_eq!(added, ["DAC_READ_SEARCH"]);
    assert_eq!(after("--entrypoint"), "tar");
}

#[tokio::test]
async fn an_import_is_refused_before_anything_is_created() {
    let app = app(false);
    for (query, content_type, expected) in [
        // Larger than the 4 MiB the test installation accepts.
        (
            "name=project&size=99999999",
            "application/octet-stream",
            StatusCode::PAYLOAD_TOO_LARGE,
        ),
        (
            "name=project&size=10",
            "application/json",
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
        ),
        (
            "name=-bad&size=10",
            "application/octet-stream",
            StatusCode::BAD_REQUEST,
        ),
        // A streamed archive has no size; the rest of the request is checked all the same.
        (
            "name=-bad",
            "application/octet-stream",
            StatusCode::BAD_REQUEST,
        ),
        (
            "name=project",
            "text/plain",
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
        ),
        (
            "name=a&size=10",
            "application/octet-stream",
            StatusCode::BAD_REQUEST,
        ),
    ] {
        let response = crate::router(app.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/volumes/import?{query}"))
                    .header("host", "localhost:8080")
                    .header("content-type", content_type)
                    .header("x-thoric-request", "1")
                    .body(Body::from(vec![0; 10]))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), expected, "{query}");
    }
    assert!(app.db.list::<crate::model::Job>("job").unwrap().is_empty());
}

/// A plain tar of one file, closed as tar closes it.
fn archive() -> Vec<u8> {
    let mut tar = crate::archives::tar::entry("notes.txt", b"hello", 0);
    tar.extend([0; 2 * 512]);
    tar
}

fn pieces(data: &[u8]) -> Vec<std::result::Result<Bytes, axum::Error>> {
    data.chunks(700)
        .map(|piece| Ok(Bytes::copy_from_slice(piece)))
        .collect()
}

/// What `deliver` makes of an upload arriving in these pieces, and what tar received.
async fn feed(
    pieces: Vec<std::result::Result<Bytes, axum::Error>>,
    size: Option<u64>,
    limit: u64,
) -> (anyhow::Result<u64>, Vec<u8>) {
    let mut tar = Vec::new();
    let stream = futures_util::stream::iter(pieces);
    let result = deliver(
        stream,
        &mut tar,
        size,
        limit,
        std::future::pending(),
        |_| Ok(()),
    )
    .await;
    (result, tar)
}

#[tokio::test]
async fn an_upload_of_unknown_size_is_complete_only_with_its_closing_blocks() {
    let archive = archive();
    let (received, tar) = feed(pieces(&archive), None, 1 << 20).await;
    assert_eq!(received.unwrap(), archive.len() as u64);
    assert_eq!(tar, archive);
    assert!(feed(pieces(&gzip(&archive)), None, 1 << 20).await.0.is_ok());
    // Cut at the member boundary, which GNU tar would unpack without a word.
    let cut = &archive[..1024];
    let error = feed(pieces(cut), None, 1 << 20).await.0.unwrap_err();
    assert!(error.to_string().contains("end-of-archive"), "{error}");
    // A sender's tar that stopped early while its gzip closed the stream properly.
    assert!(feed(pieces(&gzip(cut)), None, 1 << 20).await.0.is_err());
    // A declared size vouches for the archive, as before.
    assert!(feed(pieces(cut), Some(1024), 1 << 20).await.0.is_ok());
}

#[tokio::test]
async fn a_declared_size_is_exact() {
    let archive = archive();
    let size = archive.len() as u64;
    assert!(feed(pieces(&archive), Some(size), 1 << 20).await.0.is_ok());
    let error = feed(pieces(&archive), Some(size - 1), 1 << 20)
        .await
        .0
        .unwrap_err();
    assert!(error.to_string().contains("declared size"), "{error}");
    let error = feed(pieces(&archive), Some(size + 1), 1 << 20)
        .await
        .0
        .unwrap_err();
    assert!(error.to_string().contains("ended after"), "{error}");
}

#[tokio::test]
async fn an_upload_without_a_size_is_held_to_the_limit() {
    let archive = archive();
    let error = feed(pieces(&archive), None, 1000).await.0.unwrap_err();
    assert!(
        error.to_string().contains("IMAGE_ARCHIVE_MAX_MB"),
        "{error}"
    );
    // The route's body limit stops a chunked body with an error of its own, named the same way.
    let limited = http_body_util::Limited::new(Body::from(archive), 1000);
    let mut tar = Vec::new();
    let error = deliver(
        Body::new(limited).into_data_stream(),
        &mut tar,
        None,
        1 << 20,
        std::future::pending(),
        |_| Ok(()),
    )
    .await
    .unwrap_err();
    assert!(
        error.to_string().contains("IMAGE_ARCHIVE_MAX_MB"),
        "{error}"
    );
}

#[tokio::test]
async fn an_interrupted_upload_fails_whether_or_not_it_has_a_size() {
    let archive = archive();
    for size in [None, Some(archive.len() as u64)] {
        // As hyper reports a chunked upload whose connection closed before its last chunk.
        let mut parts = pieces(&archive[..1024]);
        parts.push(Err(axum::Error::new(std::io::Error::other(
            "connection closed before message completed",
        ))));
        parts.extend(pieces(&archive[1024..]));
        let error = feed(parts, size, 1 << 20).await.0.unwrap_err();
        assert!(error.to_string().contains("interrupted"), "{error}");
    }
}

#[tokio::test]
async fn tar_that_stops_reading_is_told_apart() {
    let (mut tar, reader) = tokio::io::duplex(64);
    drop(reader);
    let stream = futures_util::stream::iter(pieces(&archive()));
    let error = deliver(
        stream,
        &mut tar,
        None,
        1 << 20,
        std::future::pending(),
        |_| Ok(()),
    )
    .await
    .unwrap_err();
    assert!(error.is::<Stopped>(), "{error}");
}

#[tokio::test]
async fn an_extract_is_refused_before_anything_is_created() {
    let app = app(false);
    for (query, content_type, expected) in [
        (
            "path=/project&size=99999999",
            "application/octet-stream",
            StatusCode::PAYLOAD_TOO_LARGE,
        ),
        (
            "path=/project",
            "text/plain",
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
        ),
        (
            "path=project",
            "application/octet-stream",
            StatusCode::BAD_REQUEST,
        ),
        (
            "path=/a/../b",
            "application/octet-stream",
            StatusCode::BAD_REQUEST,
        ),
        (
            "path=/a//b",
            "application/octet-stream",
            StatusCode::BAD_REQUEST,
        ),
        (
            "size=10",
            "application/octet-stream",
            StatusCode::BAD_REQUEST,
        ),
    ] {
        let response = crate::router(app.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/volumes/project/extract?{query}"))
                    .header("host", "localhost:8080")
                    .header("content-type", content_type)
                    .header("x-thoric-request", "1")
                    .body(Body::from(vec![0; 10]))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), expected, "{query}");
    }
    assert!(app.db.list::<crate::model::Job>("job").unwrap().is_empty());
}

#[tokio::test]
async fn a_refusal_before_the_upload_keeps_its_status() {
    let app = app(false);
    // A folder that exists answers 409, as the volume file routes do, not a generic 400.
    let (accepted, was_accepted) = oneshot::channel();
    let (_done, outcome) = oneshot::channel();
    let logged = refuse(
        accepted,
        Error(StatusCode::CONFLICT, "/project already exists".into()),
    );
    assert_eq!(logged.to_string(), "/project already exists");
    let refused = answer(
        &app,
        "job".into(),
        was_accepted,
        outcome,
        auth::Session::Link,
    )
    .await
    .unwrap_err();
    assert_eq!(
        (refused.0, refused.1.as_str()),
        (StatusCode::CONFLICT, "/project already exists")
    );
    // A job that ended without a word answers with its error.
    let (accepted, was_accepted) = oneshot::channel::<Result<()>>();
    let (done, outcome) = oneshot::channel();
    drop(accepted);
    done.send(Err("tar: This does not look like a tar archive".into()))
        .unwrap();
    let refused = answer(
        &app,
        "job".into(),
        was_accepted,
        outcome,
        auth::Session::Link,
    )
    .await
    .unwrap_err();
    assert_eq!(refused.0, StatusCode::BAD_REQUEST);
    assert!(refused.1.contains("tar archive"), "{}", refused.1);
}

/// What a helper that is abandoned right after its tar ended meets on Docker 28: `--rm` is
/// already removing the container, so `docker rm --force` refuses at once while the volume is
/// still in use. The import removes its volume only after `abandon`, so it waits for the removal.
#[cfg(unix)]
#[tokio::test]
async fn an_abandoned_helper_is_gone_before_its_volume_is_removed() {
    let dir =
        std::env::temp_dir().join(format!("thoric-abandon-{}", uuid::Uuid::new_v4().simple()));
    std::fs::create_dir_all(&dir).unwrap();
    // The container is there until the third removal attempt, which finds it gone.
    let script = r#"#!/bin/sh
cd '@DIR@'
echo "$*" >> calls
removals=$(cat removals 2>/dev/null || echo 0)
case "$*" in
  'rm --force thoric-volume-helper')
    echo $((removals + 1)) > removals
    [ "$removals" -lt 2 ] && echo 'Error response from daemon: removal of container thoric-volume-helper is already in progress' >&2
    exit 1 ;;
  'container inspect thoric-volume-helper') [ "$removals" -lt 3 ] && echo '[{"Name": "/thoric-volume-helper"}]' && exit 0; exit 1 ;;
  'ps -a --format {{.Names}}') [ "$removals" -lt 3 ] && echo thoric-volume-helper; exit 0 ;;
esac
exit 1
"#
    .replace("@DIR@", &dir.display().to_string());
    let docker = dir.join("docker");
    crate::test_support::write_script(&docker, &script);
    let mut app = app(false);
    Arc::get_mut(&mut app).unwrap().config.docker_bin = docker.display().to_string();
    let cli = tokio::process::Command::new("sleep")
        .arg("60")
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    Helper {
        child: cli,
        name: "thoric-volume-helper".into(),
    }
    .abandon(&app)
    .await;
    let calls = std::fs::read_to_string(dir.join("calls")).unwrap();
    assert_eq!(
        calls.lines().collect::<Vec<_>>(),
        [
            "rm --force thoric-volume-helper",
            "container inspect thoric-volume-helper",
            "rm --force thoric-volume-helper",
            "container inspect thoric-volume-helper",
            "rm --force thoric-volume-helper",
            "container inspect thoric-volume-helper",
            "ps -a --format {{.Names}}",
        ]
    );
    std::fs::remove_dir_all(&dir).unwrap();
}
