//! Pins down the archive a backup is: an ordinary gzip-compressed tar whose entries the reader
//! gets back exactly, in steps small enough for any upload, and nothing a backup does not write:
//! no links, folders, long names or an archive that stops before its end.
use super::*;
use crate::archives::tar::entry;

/// The archive the writer makes of `entries`.
fn written(entries: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut writer = Writer::new(1_700_000_000);
    let mut out = Vec::new();
    for (index, (name, data)) in entries.iter().enumerate() {
        out.extend(writer.entry(name, data, index == 0).unwrap());
    }
    out.extend(writer.finish().unwrap());
    out
}

/// What the reader finds in `archive`, handed to it `step` bytes at a time, as entries.
fn read(archive: &[u8], step: usize, limit: u64) -> Result<Vec<(String, Vec<u8>)>> {
    let mut reader = Reader::new(limit);
    let mut items = Vec::new();
    for piece in archive.chunks(step) {
        reader.push(piece, &mut items)?;
    }
    reader.finish(&mut items)?;
    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    let mut open = false;
    for item in items {
        match item {
            Item::Begin { name, size } => {
                assert!(!open, "{name} began inside another entry");
                entries.push((name, Vec::with_capacity(size as usize)));
                open = true;
            }
            Item::Data(data) => entries.last_mut().unwrap().1.extend(data),
            Item::Finish => open = false,
        }
    }
    assert!(!open);
    Ok(entries)
}

fn noise(length: usize) -> Vec<u8> {
    let mut seed = 2_463_534_242u32;
    (0..length)
        .map(|_| {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed as u8
        })
        .collect()
}

#[test]
fn what_the_writer_writes_the_reader_reads_back_in_any_steps() {
    let entries = vec![
        (MANIFEST, br#"{"kind":"bilskirnir-backup"}"#.to_vec()),
        (RECORDS, Vec::new()),
        ("parts/0000/00000000", noise(CHUNK)),
        ("parts/0000/00000001", noise(1000)),
        ("parts/0000/end", b"{}".to_vec()),
        (END, b"{\"parts\":1}".to_vec()),
    ];
    let archive = written(&entries);
    let expected = entries
        .iter()
        .map(|(name, data)| (name.to_string(), data.clone()))
        .collect::<Vec<_>>();
    for step in [1, 511, 4096, 1 << 20, archive.len()] {
        assert_eq!(read(&archive, step, u64::MAX).unwrap(), expected, "{step}");
    }
    // The same entries uncompressed read the same.
    let mut plain = Vec::new();
    for (name, data) in &entries {
        plain.extend(entry(name, data, 0));
    }
    plain.extend([0; 1024]);
    assert_eq!(read(&plain, 7000, u64::MAX).unwrap(), expected);
}

#[test]
fn the_unpacked_backup_is_held_to_the_import_limit() {
    let archive = written(&[(MANIFEST, vec![0; 64 << 10])]);
    let error = read(&archive, 4096, 32 << 10).unwrap_err();
    assert!(
        error.to_string().contains("IMAGE_ARCHIVE_MAX_MB"),
        "{error}"
    );
}

#[test]
fn a_backup_cut_short_is_refused() {
    let archive = written(&[(MANIFEST, noise(10_000)), (END, b"{}".to_vec())]);
    // Within the gzip stream, and at an entry boundary of a plain archive without its end.
    assert!(read(&archive[..archive.len() / 2], 4096, u64::MAX).is_err());
    let plain = entry(MANIFEST, b"{}", 0);
    let error = read(&plain, 512, u64::MAX).unwrap_err();
    assert!(error.to_string().contains("incomplete"), "{error}");
    assert!(read(b"", 512, u64::MAX).is_err());
}

#[test]
fn only_plain_files_with_short_names_are_read() {
    let mut link = entry("bilskirnir-backup.json", b"", 0);
    link[156] = b'2';
    let mut damaged = entry(MANIFEST, b"{}", 0);
    damaged[0] = b'X';
    let mut prefixed = entry(MANIFEST, b"{}", 0);
    prefixed[345] = b'a';
    for (archive, why) in [
        (link, "not a plain file"),
        (damaged, "damaged"),
        (prefixed, "not a plain file"),
    ] {
        let mut archive = archive;
        if why == "not a plain file" {
            // Mend the checksum the edit above broke, so only the entry's type is wrong.
            archive[148..156].fill(b' ');
            let sum: u32 = archive[..512].iter().map(|b| u32::from(*b)).sum();
            archive[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
        }
        archive.extend([0; 1024]);
        let error = read(&archive, 512, u64::MAX).unwrap_err().to_string();
        assert!(error.contains(why), "{why}: {error}");
    }
    let error = read(&[b'x'; 600], 64, u64::MAX).unwrap_err();
    assert!(
        error.to_string().contains("not a Bilskirnir backup"),
        "{error}"
    );
}

#[test]
fn entry_names_say_what_each_entry_is() {
    assert_eq!(parse_name(MANIFEST).unwrap(), Name::Manifest);
    assert_eq!(parse_name(RECORDS).unwrap(), Name::Records);
    assert_eq!(parse_name(END).unwrap(), Name::End);
    assert_eq!(
        parse_name(&chunk_name(3, 12)).unwrap(),
        Name::Chunk { part: 3, chunk: 12 }
    );
    assert_eq!(
        parse_name(&part_end_name(9999)).unwrap(),
        Name::PartEnd { part: 9999 }
    );
    for name in [
        "parts/1/00000000",
        "parts/0001/1",
        "parts/0001",
        "../etc/passwd",
        "parts/0001/end/x",
        "manifest.json",
    ] {
        assert!(parse_name(name).is_err(), "{name}");
    }
}
