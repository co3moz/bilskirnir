//! Pins down how a volume archive travels: an upload reaches tar as a plain archive whatever its
//! compression, never beyond the unpacked limit, and an import is refused before anything is
//! created when its request cannot work.
use super::*;
use crate::tests::app;
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
    plain.extend(unpacker.finish()?);
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
                    .header("x-wsld-request", "1")
                    .body(Body::from(vec![0; 10]))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), expected, "{query}");
    }
    assert!(app.db.list::<crate::model::Job>("job").unwrap().is_empty());
}
