//! The tar filter must read every name, size and extension header as Go's reader in Docker does,
//! so nothing that names an image reaches `docker load`, and must hold back the files Docker
//! registers an image from until the archive has been checked.
use super::*;

const LAYER: &str = "blobs/sha256/1111111111111111111111111111111111111111111111111111111111111111";
const CONFIG: &str =
    "blobs/sha256/2222222222222222222222222222222222222222222222222222222222222222";
const DIGEST: &str = "sha256:3333333333333333333333333333333333333333333333333333333333333333";

fn checksum(block: &mut [u8; BLOCK]) {
    block[148..156].fill(b' ');
    let sum: u32 = block.iter().map(|byte| u32::from(*byte)).sum();
    block[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
}

fn header(name: &str, typeflag: u8, size: u64) -> [u8; BLOCK] {
    let mut block = [0u8; BLOCK];
    block[..name.len()].copy_from_slice(name.as_bytes());
    octal(&mut block[100..108], 0o644);
    octal(&mut block[124..136], size);
    octal(&mut block[136..148], 0);
    block[156] = typeflag;
    block[257..263].copy_from_slice(b"ustar\0");
    block[263..265].copy_from_slice(b"00");
    checksum(&mut block);
    block
}

fn with_data(block: [u8; BLOCK], data: &[u8]) -> Vec<u8> {
    let mut out = block.to_vec();
    out.extend_from_slice(data);
    out.resize(out.len() + padding(data.len() as u64) as usize, 0);
    out
}

fn file(name: &str, data: &[u8]) -> Vec<u8> {
    with_data(header(name, b'0', data.len() as u64), data)
}

fn pax(records: &[(&str, &str)]) -> Vec<u8> {
    let mut data = String::new();
    for (key, value) in records {
        let body = format!(" {key}={value}\n");
        let mut length = body.len() + 1;
        while length.to_string().len() + body.len() != length {
            length = length.to_string().len() + body.len();
        }
        data.push_str(&format!("{length}{body}"));
    }
    with_data(
        header("PaxHeaders/x", b'x', data.len() as u64),
        data.as_bytes(),
    )
}

fn config(labelled: bool) -> Vec<u8> {
    let labels = if labelled {
        json!({"thoric.desktop": "true"})
    } else {
        json!({})
    };
    serde_json::to_vec(
        &json!({"architecture": "amd64", "os": "linux", "config": {"Labels": labels}}),
    )
    .unwrap()
}

fn layer() -> Vec<u8> {
    (0..3000u32).map(|i| (i % 251) as u8).collect()
}

fn manifest(config: &str) -> Vec<u8> {
    serde_json::to_vec(
        &json!([{"Config": config, "RepoTags": ["victim:latest"], "Layers": [LAYER]}]),
    )
    .unwrap()
}

fn index() -> Vec<u8> {
    serde_json::to_vec(&json!({"schemaVersion": 2, "manifests": [{
        "mediaType": "application/vnd.oci.image.manifest.v1+json",
        "digest": DIGEST,
        "size": 400,
        "annotations": {
            "io.containerd.image.name": "docker.io/library/victim:latest",
            "org.opencontainers.image.ref.name": "latest"
        }
    }]}))
    .unwrap()
}

fn close(parts: Vec<Vec<u8>>) -> Vec<u8> {
    let mut out = parts.concat();
    out.extend([0; 2 * BLOCK]);
    out
}

/// Laid out as Docker 25 saves an image, with Thoric's metadata in front.
fn archive(config_data: &[u8]) -> Vec<u8> {
    close(vec![
        file(METADATA, br#"{"format":1,"name":"Saved"}"#),
        with_data(header("blobs/", b'5', 0), b""),
        file(LAYER, &layer()),
        file(CONFIG, config_data),
        file("index.json", &index()),
        file("manifest.json", &manifest(CONFIG)),
        file("oci-layout", br#"{"imageLayoutVersion":"1.0.0"}"#),
        file("repositories", br#"{"victim":{"latest":"1111"}}"#),
    ])
}

fn run(archive: &[u8], step: usize) -> Result<(Vec<u8>, Contents)> {
    let mut filter = Filter::new(1 << 30);
    let mut out = Vec::new();
    for chunk in archive.chunks(step) {
        filter.push(chunk)?;
        out.extend(filter.take_output());
    }
    let (tail, contents) = filter.finish()?;
    out.extend(tail);
    Ok((out, contents))
}

/// The headers in a tar stream with each one's data, extension headers included as they are.
fn entries(tar: &[u8]) -> Vec<(String, u8, Vec<u8>)> {
    let mut out = Vec::new();
    let mut at = 0;
    let mut pax_size = None;
    while at + BLOCK <= tar.len() {
        let block = &tar[at..at + BLOCK];
        if block.iter().all(|byte| *byte == 0) {
            break;
        }
        let name = String::from_utf8_lossy(until_nul(&block[..100])).into_owned();
        let typeflag = block[156];
        let stored = size(&block[124..136]).unwrap();
        let length = if header_only(typeflag) || (typeflag == 0 && name.ends_with('/')) {
            0
        } else if typeflag == b'x' {
            stored
        } else {
            pax_size.take().unwrap_or(stored)
        } as usize;
        at += BLOCK;
        let data = tar[at..at + length].to_vec();
        if typeflag == b'x' {
            pax_size = super::pax(&data).unwrap().1;
        }
        out.push((name, typeflag, data));
        at += length + padding(length as u64) as usize;
    }
    out
}

fn names(tar: &[u8]) -> Vec<String> {
    entries(tar).into_iter().map(|(name, ..)| name).collect()
}

fn error(archive: &[u8]) -> String {
    match run(archive, 1000) {
        Ok(_) => panic!("the archive was accepted"),
        Err(error) => format!("{error:#}"),
    }
}

#[test]
fn layers_pass_and_naming_files_are_rebuilt_at_the_end() {
    let (out, contents) = run(&archive(&config(true)), 4096).unwrap();
    assert_eq!(
        names(&out),
        [
            "blobs/",
            LAYER,
            CONFIG,
            "oci-layout",
            "index.json",
            "manifest.json"
        ]
    );
    let entries = entries(&out);
    assert_eq!(entries[1].2, layer(), "layer bytes are untouched");
    let manifest: Value = serde_json::from_slice(&entries[5].2).unwrap();
    assert_eq!(
        manifest,
        json!([{"Config": CONFIG, "RepoTags": [], "Layers": [LAYER]}])
    );
    let index: Value = serde_json::from_slice(&entries[4].2).unwrap();
    assert_eq!(index["manifests"][0]["digest"], DIGEST);
    assert!(index["manifests"][0].get("annotations").is_none());
    assert_eq!(
        contents.metadata.as_deref(),
        Some(&br#"{"format":1,"name":"Saved"}"#[..])
    );
    assert!(out.ends_with(&[0; 2 * BLOCK]));
}

#[test]
fn chunk_boundaries_do_not_change_the_result() {
    let archive = archive(&config(true));
    let (whole, _) = run(&archive, archive.len()).unwrap();
    for step in [1, 7, 511, 513, 1500] {
        assert_eq!(run(&archive, step).unwrap().0, whole, "step {step}");
    }
}

#[test]
fn nothing_that_registers_an_image_leaves_before_the_checks() {
    let archive = archive(&config(true));
    let mut filter = Filter::new(1 << 30);
    filter.push(&archive[..archive.len() - 2 * BLOCK]).unwrap();
    let early = filter.take_output();
    assert!(!names(&early).iter().any(|name| name.ends_with(".json")));
}

#[test]
fn rebuilt_manifest_drops_case_variants_go_would_still_read() {
    let original = json!([{
        "Config": CONFIG,
        "RepoTags": ["a:1"],
        "repotags": ["b:2"],
        "REPOTAGS": ["c:3"],
        "Layers": [LAYER],
        "Extra": true
    }]);
    let (rebuilt, config) = rebuild_manifest(&serde_json::to_vec(&original).unwrap()).unwrap();
    assert_eq!(config, CONFIG);
    let rebuilt: Value = serde_json::from_slice(&rebuilt).unwrap();
    assert_eq!(
        rebuilt,
        json!([{"Config": CONFIG, "RepoTags": [], "Layers": [LAYER]}])
    );
}

#[test]
fn a_desktop_image_made_for_a_newer_thoric_stops_before_docker_loads_it() {
    let labels = json!({
        "thoric.desktop": "true",
        "thoric.desktop-level": (crate::catalog::DESKTOP_LEVEL + 1).to_string(),
    });
    let newer = serde_json::to_vec(
        &json!({"architecture": "amd64", "os": "linux", "config": {"Labels": labels}}),
    )
    .unwrap();
    assert!(error(&archive(&newer)).contains("made for a newer Thoric"));
}

#[test]
fn only_a_desktop_image_and_only_one_is_accepted() {
    assert!(error(&archive(&config(false))).contains("not contain a Thoric desktop image"));
    let two = serde_json::to_vec(&json!([
        {"Config": CONFIG, "Layers": [LAYER]},
        {"Config": CONFIG, "Layers": [LAYER]}
    ]))
    .unwrap();
    let archive = close(vec![
        file(CONFIG, &config(true)),
        file("manifest.json", &two),
    ]);
    assert!(error(&archive).contains("exactly one image"));
    let mut index: Value = serde_json::from_slice(&index()).unwrap();
    let mut other = index["manifests"][0].clone();
    other["digest"] = json!(DIGEST.replace('3', "4"));
    index["manifests"].as_array_mut().unwrap().push(other);
    let archive = close(vec![
        file(CONFIG, &config(true)),
        file("index.json", &serde_json::to_vec(&index).unwrap()),
        file("manifest.json", &manifest(CONFIG)),
    ]);
    assert!(error(&archive).contains("exactly one image"));
}

#[test]
fn an_archive_without_manifest_or_configuration_is_refused() {
    let archive = close(vec![file(CONFIG, &config(true))]);
    assert!(error(&archive).contains("no manifest.json"));
    let archive = close(vec![file("manifest.json", &manifest(CONFIG))]);
    assert!(error(&archive).contains("image configuration"));
}

#[test]
fn extension_headers_rename_entries_as_in_go() {
    // A PAX path, then a GNU long name, each turning an innocent header into the manifest.
    let mut pax_named = pax(&[("path", "manifest.json")]);
    pax_named.extend(file("innocent", &manifest(CONFIG)));
    let mut long_named = with_data(header("././@LongLink", b'L', 14), b"manifest.json\0");
    long_named.extend(file("innocent", &manifest(CONFIG)));
    for renamed in [pax_named, long_named] {
        let archive = close(vec![file(CONFIG, &config(true)), renamed]);
        let (out, _) = run(&archive, 100).unwrap();
        assert_eq!(names(&out), [CONFIG, "manifest.json"]);
        let manifest: Value = serde_json::from_slice(&entries(&out)[1].2).unwrap();
        assert_eq!(manifest[0]["RepoTags"], json!([]));
    }
    // A PAX header for an entry that passes goes out with it, and only once.
    let mut described = pax(&[("path", LAYER)]);
    described.extend(file("short", &layer()));
    let archive = close(vec![
        described,
        file(CONFIG, &config(true)),
        file("manifest.json", &manifest(CONFIG)),
    ]);
    let (out, _) = run(&archive, 300).unwrap();
    assert_eq!(
        names(&out),
        ["PaxHeaders/x", "short", CONFIG, "manifest.json"]
    );
}

#[test]
fn pax_size_and_header_only_types_decide_where_the_next_header_starts() {
    // A symbolic link's size field is ignored, so its "data" is really the next header.
    let mut link = header("link", b'2', BLOCK as u64);
    link[157..163].copy_from_slice(b"target");
    checksum(&mut link);
    // A directory written as an old-style file with a trailing slash has no data either.
    let directory = header("old-dir/", 0, BLOCK as u64);
    let mut sized = pax(&[("size", "600")]);
    sized.extend(with_data(header("big", b'0', 0), &[7; 600]));
    let archive = close(vec![
        link.to_vec(),
        directory.to_vec(),
        sized,
        file(CONFIG, &config(true)),
        file("manifest.json", &manifest(CONFIG)),
    ]);
    let (out, _) = run(&archive, 64).unwrap();
    assert_eq!(
        names(&out),
        [
            "link",
            "old-dir/",
            "PaxHeaders/x",
            "big",
            CONFIG,
            "manifest.json"
        ]
    );
}

#[test]
fn name_prefixes_follow_the_header_format() {
    // USTAR joins the prefix, so the cleaned path is the manifest.
    let mut ustar = header("../manifest.json", b'0', 0);
    ustar[345..348].copy_from_slice(b"dir");
    // GNU headers keep times there instead; valid times mean no prefix at all.
    let mut gnu = header("manifest.json", b'0', 0);
    gnu[257..265].copy_from_slice(b"ustar  \0");
    gnu[345..356].copy_from_slice(b"00000000000");
    gnu[357..368].copy_from_slice(b"00000000000");
    for mut block in [ustar, gnu] {
        let data = manifest(CONFIG);
        octal(&mut block[124..136], data.len() as u64);
        checksum(&mut block);
        let archive = close(vec![file(CONFIG, &config(true)), with_data(block, &data)]);
        let (out, _) = run(&archive, 512).unwrap();
        assert_eq!(names(&out), [CONFIG, "manifest.json"]);
    }
}

#[test]
fn naming_files_must_be_regular_and_at_the_top() {
    let mut link = header("manifest.json", b'2', 0);
    link[157..166].copy_from_slice(b"evil.json");
    checksum(&mut link);
    assert!(error(&close(vec![link.to_vec()])).contains("not a regular file"));
    assert!(error(&close(vec![file("x/manifest.json", b"[]")])).contains("nested manifest.json"));
    assert!(error(&close(vec![file("../layer", b"x")])).contains("outside its root"));
    // A leading slash or dot still lands at the top, so it is still the manifest.
    for name in [
        "/manifest.json",
        "./manifest.json",
        "blobs/../manifest.json",
    ] {
        let archive = close(vec![
            file(CONFIG, &config(true)),
            file(name, &manifest(CONFIG)),
        ]);
        assert_eq!(
            names(&run(&archive, 512).unwrap().0),
            [CONFIG, "manifest.json"]
        );
    }
}

#[test]
fn base256_sizes_are_read() {
    let data = layer();
    let mut block = header(LAYER, b'0', 0);
    block[124..136].fill(0);
    block[124] = 0x80;
    block[134..136].copy_from_slice(&(data.len() as u16).to_be_bytes());
    checksum(&mut block);
    let archive = close(vec![
        with_data(block, &data),
        file(CONFIG, &config(true)),
        file("manifest.json", &manifest(CONFIG)),
    ]);
    let (out, _) = run(&archive, 999).unwrap();
    assert_eq!(entries(&out)[0].2, data);
}

#[test]
fn damaged_unusual_or_oversized_archives_are_refused() {
    let mut damaged = archive(&config(true));
    damaged[10] ^= 1;
    assert!(error(&damaged).contains("not a tar archive"));
    let archive = archive(&config(true));
    let cut = &archive[..BLOCK + 100];
    let mut filter = Filter::new(1 << 30);
    filter.push(cut).unwrap();
    let message = format!("{:#}", filter.finish().err().unwrap());
    assert!(message.contains("middle of an entry"));
    assert!(error(&close(vec![with_data(header("sparse", b'S', 0), b"")])).contains("sparse"));
    assert!(error(&close(vec![pax(&[("GNU.sparse.major", "1")])])).contains("sparse"));
    let mut filter = Filter::new(2048);
    assert!(filter.push(&archive).is_err());
}

#[test]
fn metadata_may_carry_a_build_script_but_a_manifest_stays_small() {
    let script = json!({"format": 1, "name": "Built", "build": {"script": "x".repeat(5 << 20)}});
    let archive = close(vec![
        file(METADATA, &serde_json::to_vec(&script).unwrap()),
        file(CONFIG, &config(true)),
        file("manifest.json", &manifest(CONFIG)),
    ]);
    assert!(run(&archive, 1 << 20).unwrap().1.metadata.unwrap().len() > 5 << 20);
    let mut large = manifest(CONFIG);
    large.resize(5 << 20, b' ');
    let archive = close(vec![
        file(CONFIG, &config(true)),
        file("manifest.json", &large),
    ]);
    assert!(error(&archive).contains("manifest.json is larger than 4 MiB"));
}

#[test]
fn the_archive_ends_at_its_first_zero_block() {
    let mut archive = archive(&config(true));
    archive.extend(file("after-the-end", b"ignored"));
    let (out, _) = run(&archive, 2048).unwrap();
    assert!(!names(&out).contains(&"after-the-end".to_owned()));
}

#[test]
fn written_entries_are_valid_ustar() {
    let written = entry(METADATA, b"{}", 1_700_000_000);
    let block: [u8; BLOCK] = written[..BLOCK].try_into().unwrap();
    assert!(matches!(format(&block).unwrap(), Format::Ustar));
    assert_eq!(header_name(&block, Format::Ustar), METADATA.as_bytes());
    assert_eq!(size(&block[124..136]).unwrap(), 2);
    assert_eq!(written.len(), 2 * BLOCK);
}
