//! Pins down where an archive ends as GNU tar reads it: only at its end-of-archive block, never at
//! a member boundary or in data that happens to be zeros, with the sizes that PAX headers, base-256
//! fields and old GNU sparse headers give.
use super::*;

/// A header block of `typeflag` whose size field holds `size`; tar's checksum is not looked at.
fn header(typeflag: u8, size: u64) -> Vec<u8> {
    let mut block = vec![0; BLOCK];
    block[..4].copy_from_slice(b"file");
    block[124..136].copy_from_slice(format!("{size:011o}\0").as_bytes());
    block[156] = typeflag;
    block
}

/// A member with its data padded to whole blocks.
fn member(typeflag: u8, data: &[u8]) -> Vec<u8> {
    let mut out = header(typeflag, data.len() as u64);
    out.extend_from_slice(data);
    out.resize(out.len() + padding(data.len() as u64) as usize, 0);
    out
}

/// The closing blocks and record padding GNU tar writes.
fn closing() -> Vec<u8> {
    vec![0; 2 * BLOCK + 3 * BLOCK]
}

/// Whether the archive ends, fed in pieces of `size` bytes.
fn reached(archive: &[u8], size: usize) -> bool {
    let mut ending = Ending::new();
    for piece in archive.chunks(size) {
        ending.push(piece);
    }
    ending.reached()
}

fn split_every_way(archive: &[u8]) -> bool {
    let answers: Vec<bool> = [1, 7, 511, 512, 513, 4096, 1 << 20]
        .into_iter()
        .map(|size| reached(archive, size))
        .collect();
    assert!(answers.iter().all(|answer| *answer == answers[0]));
    answers[0]
}

#[test]
fn an_archive_ends_only_at_its_end_of_archive_block() {
    let mut archive = member(b'0', &[7; 700]);
    archive.extend(member(b'5', b""));
    archive.extend(member(b'0', b"two"));
    let boundary = archive.len();
    archive.extend(closing());
    assert!(split_every_way(&archive));
    // GNU tar unpacks these without a word; the upload is not complete.
    assert!(!split_every_way(&archive[..boundary]));
    assert!(!split_every_way(&archive[..BLOCK + 700]));
    assert!(!split_every_way(&archive[..BLOCK + 1024]));
    assert!(!split_every_way(&[]));
    // What follows the end-of-archive block is not read.
    let mut trailing = archive.clone();
    trailing.extend([1; 100]);
    assert!(split_every_way(&trailing));
}

#[test]
fn data_of_zeros_is_no_end_of_archive_block() {
    let mut archive = member(b'0', &[0; 4096]);
    let boundary = archive.len();
    archive.extend(closing());
    assert!(!split_every_way(&archive[..boundary]));
    assert!(!split_every_way(&archive[..BLOCK + 1024]));
    assert!(split_every_way(&archive));
}

#[test]
fn a_pax_size_applies_to_the_member_after_it() {
    // An 8 GiB file's size does not fit the octal field, so the PAX header carries it; here the
    // header says nothing at all and the data follows the PAX size.
    let record = b"13 size=1500\n";
    let mut archive = member(b'x', record);
    archive.extend(header(b'0', 0));
    archive.extend([0; 1536]);
    let boundary = archive.len();
    archive.extend(closing());
    assert!(split_every_way(&archive));
    assert!(!split_every_way(&archive[..boundary]));
    // The size is used once; a global header gives none.
    let mut global = member(b'g', b"13 size=9999\n");
    global.extend(member(b'x', record));
    global.extend(header(b'0', 0));
    global.extend([1; 1536]);
    global.extend(member(b'0', b"next"));
    global.extend(closing());
    assert!(split_every_way(&global));
}

#[test]
fn sizes_are_read_as_gnu_tar_reads_them() {
    // Base-256, as GNU tar writes sizes above the octal field's 8 GiB.
    let mut big = header(b'0', 0);
    big[124..136].copy_from_slice(&[0x80, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x04, 0]);
    big.extend([1; 1024]);
    big.extend(closing());
    assert!(split_every_way(&big));
    // A hard link carries no data whatever its size field says.
    let mut link = header(b'1', 4096);
    link.extend(closing());
    assert!(split_every_way(&link));
    // An unreadable size is refused by tar too; the archive is not taken as complete.
    let mut damaged = header(b'0', 0);
    damaged[124..136].copy_from_slice(b"99999999999\0");
    damaged.extend(closing());
    assert!(!split_every_way(&damaged));
}

#[test]
fn old_gnu_sparse_headers_come_before_the_data() {
    let mut sparse = header(b'S', 600);
    sparse[482] = 1;
    let mut extension = vec![1; BLOCK];
    extension[504] = 1;
    let mut last = vec![1; BLOCK];
    last[504] = 0;
    let mut archive = sparse;
    archive.extend(extension);
    archive.extend(last);
    archive.extend([0; 1024]);
    let boundary = archive.len();
    archive.extend(closing());
    assert!(split_every_way(&archive));
    assert!(!split_every_way(&archive[..boundary]));
}

#[test]
fn a_pax_size_record_is_read_among_others() {
    assert_eq!(pax_size(b"20 path=a/long/name\n13 size=1500\n"), Some(1500));
    assert_eq!(pax_size(b"20 path=a/long/name\n"), None);
    assert_eq!(pax_size(b"99 size=1\n"), None);
    assert_eq!(pax_size(b"garbage"), None);
}
