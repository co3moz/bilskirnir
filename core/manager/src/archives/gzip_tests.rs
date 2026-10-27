//! An export must come back through an import unchanged apart from the filter's rewrites, from a
//! `.tar.gz` or a plain `.tar`, without a compressed upload expanding all at once in memory.
use super::super::tar::{BLOCK, METADATA, entry};
use super::*;
use serde_json::json;

const CONFIG: &str =
    "blobs/sha256/2222222222222222222222222222222222222222222222222222222222222222";
const LAYER: &str = "blobs/sha256/1111111111111111111111111111111111111111111111111111111111111111";

/// A `docker save` archive as Docker streams it, with a layer of `layer` bytes.
fn saved(layer: &[u8], labelled: bool) -> Vec<u8> {
    let labels = if labelled {
        json!({"thoric.desktop": "true"})
    } else {
        json!({})
    };
    let config = serde_json::to_vec(&json!({"config": {"Labels": labels}})).unwrap();
    let manifest = serde_json::to_vec(
        &json!([{"Config": CONFIG, "RepoTags": ["victim:1"], "Layers": [LAYER]}]),
    )
    .unwrap();
    let mut out = [
        entry(LAYER, layer, 0),
        entry(CONFIG, &config, 0),
        entry("manifest.json", &manifest, 0),
    ]
    .concat();
    out.extend([0; 2 * BLOCK]);
    out
}

/// What the export writes: the metadata entry, then the saved archive, compressed.
fn exported(saved: &[u8]) -> Vec<u8> {
    let (mut compressor, mut out) =
        Compressor::new(&entry(METADATA, br#"{"format":1,"name":"Saved"}"#, 0)).unwrap();
    for chunk in saved.chunks(100_000) {
        out.extend(compressor.push(chunk).unwrap());
    }
    out.extend(compressor.finish().unwrap());
    out
}

/// Feeds an upload as the import does and returns what Docker receives and the largest step.
fn import(upload: &[u8], chunk: usize, budget: usize) -> Result<(Vec<u8>, Contents, usize)> {
    let mut unpacker = Unpacker::new(1 << 30);
    let (mut out, mut largest) = (Vec::new(), 0);
    for mut pending in upload.chunks(chunk) {
        while !pending.is_empty() {
            let (used, output) = unpacker.push(pending, budget)?;
            largest = largest.max(output.len());
            out.extend(output);
            pending = &pending[used..];
        }
    }
    let (last, contents) = unpacker.finish()?;
    out.extend(last);
    Ok((out, contents, largest))
}

fn filtered(saved: &[u8]) -> Vec<u8> {
    let mut filter = Filter::new(1 << 30);
    filter.push(saved).unwrap();
    let mut out = filter.take_output();
    out.extend(filter.finish().unwrap().0);
    out
}

#[test]
fn an_export_imports_as_the_filtered_save() {
    let saved = saved(&[5; 70_000], true);
    let upload = exported(&saved);
    assert_eq!(&upload[..2], [0x1f, 0x8b]);
    for chunk in [1, 1000, upload.len()] {
        let (docker, contents, _) = import(&upload, chunk, 1 << 20).unwrap();
        assert_eq!(docker, filtered(&saved), "chunk {chunk}");
        assert_eq!(
            contents.metadata.as_deref(),
            Some(&br#"{"format":1,"name":"Saved"}"#[..])
        );
    }
}

#[test]
fn a_plain_docker_save_tar_is_accepted() {
    let saved = saved(&[5; 10_000], true);
    let (docker, contents, _) = import(&saved, 4096, 1 << 20).unwrap();
    assert_eq!(docker, filtered(&saved));
    assert!(contents.metadata.is_none());
}

#[test]
fn a_highly_compressed_upload_is_unpacked_within_the_budget() {
    let saved = saved(&vec![0; 16 << 20], true);
    let upload = exported(&saved);
    assert!(upload.len() < 1 << 20, "zeros compress well");
    let budget = 1 << 20;
    let (docker, _, largest) = import(&upload, upload.len(), budget).unwrap();
    assert_eq!(docker, filtered(&saved));
    assert!(largest < budget + PLAIN_STEP, "largest step {largest}");
}

#[test]
fn damaged_empty_or_refused_uploads_say_why() {
    let upload = exported(&saved(&[5; 10_000], true));
    let message =
        |result: Result<(Vec<u8>, Contents, usize)>| format!("{:#}", result.err().unwrap());
    assert!(message(import(&upload[..upload.len() - 4], 1000, 1 << 20)).contains("gzip data"));
    assert!(message(import(b"", 1, 1 << 20)).contains("empty"));
    assert!(message(import(&[b'x'; 2000], 5, 1 << 20)).contains("not a tar archive"));
    // The filter's own refusal is not blamed on gzip.
    let refused = message(import(&exported(&saved(&[5; 100], false)), 1000, 1 << 20));
    assert!(refused.contains("not contain a Thoric desktop image"));
    assert!(!refused.contains("gzip"));
}
