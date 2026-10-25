//! How a backup is laid out on the wire: a gzip-compressed USTAR archive of regular files with
//! short names, which `tar` lists and unpacks like any other.
//!
//! Its first entry is the manifest, the second the records, and the last says the backup ended.
//! Between them come the parts, each a `docker save` stream or a volume's tar in entries of at
//! most [`CHUNK`] bytes, `parts/NNNN/NNNNNNNN`, closed by `parts/NNNN/end` with the part's length
//! and digest. tar needs an entry's size before its data, and neither Docker nor tar says how long
//! its stream is until it ends, so the chunks let a backup stream without a copy on the manager's
//! disk: one chunk is held in memory at a time. Joined in order, a part's chunks are the stream as
//! it was made, so `cat parts/0003/0* > image.tar` gives an archive `docker load` reads.
use crate::archives::tar::{BLOCK, header, padding, size};
use anyhow::{Context, Result, bail};
use flate2::{Compression, write::GzEncoder, write::MultiGzDecoder};
use std::io::Write;

/// The manifest, what a backup holds: the first entry, so it can be read without the rest.
pub(crate) const MANIFEST: &str = "bilskirnir-backup.json";
/// The records, the second entry.
pub(crate) const RECORDS: &str = "records.json";
/// The last entry, written once every part is complete.
pub(crate) const END: &str = "bilskirnir-backup-end.json";
/// The most a part's chunk holds.
pub(crate) const CHUNK: usize = 4 << 20;
pub(crate) const MANIFEST_LIMIT: u64 = 4 << 20;
/// Records carry build and setup scripts of up to a megabyte each.
pub(crate) const RECORDS_LIMIT: u64 = 64 << 20;
/// A part's end and the backup's end are a few lines of JSON.
pub(crate) const SMALL_LIMIT: u64 = 64 << 10;
/// Parts are numbered in four digits.
pub(crate) const PART_LIMIT: usize = 9999;
/// Compressed input decoded per step. Deflate expands at most about a thousand times, so one step
/// adds at most a few dozen megabytes before its output is handed on.
pub(crate) const GZIP_STEP: usize = 16 * 1024;

pub(crate) fn chunk_name(part: usize, chunk: u64) -> String {
    format!("parts/{part:04}/{chunk:08}")
}

pub(crate) fn part_end_name(part: usize) -> String {
    format!("parts/{part:04}/end")
}

/// What an entry is, by its name.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Name {
    Manifest,
    Records,
    Chunk { part: usize, chunk: u64 },
    PartEnd { part: usize },
    End,
}

/// Read an entry's name; a name no backup writes is refused.
pub(crate) fn parse_name(name: &str) -> Result<Name> {
    let digits = |text: &str, width: usize| {
        (text.len() == width && text.bytes().all(|b| b.is_ascii_digit()))
            .then(|| text.parse::<u64>().ok())
            .flatten()
    };
    Ok(match name {
        MANIFEST => Name::Manifest,
        RECORDS => Name::Records,
        END => Name::End,
        _ => {
            let rest = name.strip_prefix("parts/");
            let (part, entry) = rest
                .and_then(|rest| rest.split_once('/'))
                .with_context(|| format!("The backup has an unexpected entry {name:?}"))?;
            let part = digits(part, 4)
                .with_context(|| format!("The backup has an unexpected entry {name:?}"))?
                as usize;
            match entry {
                "end" => Name::PartEnd { part },
                chunk => Name::Chunk {
                    part,
                    chunk: digits(chunk, 8)
                        .with_context(|| format!("The backup has an unexpected entry {name:?}"))?,
                },
            }
        }
    })
}

/// The compressed archive as it is written. Each call returns the bytes ready to send.
pub(crate) struct Writer {
    encoder: GzEncoder<Vec<u8>>,
    mtime: u64,
}

impl Writer {
    /// The fastest level: most of a backup is image layers and files that are compressed already
    /// or not worth a slower transfer, as an image export judges too.
    pub(crate) fn new(mtime: u64) -> Self {
        Self {
            encoder: GzEncoder::new(Vec::new(), Compression::fast()),
            mtime,
        }
    }

    /// One regular file. `flush` sends everything so far, so a download begins at once.
    pub(crate) fn entry(
        &mut self,
        name: &str,
        data: &[u8],
        flush: bool,
    ) -> std::io::Result<Vec<u8>> {
        assert!(name.len() <= 100, "entry name too long");
        self.encoder
            .write_all(&header(name, b'0', 0o644, 0, data.len() as u64, self.mtime))?;
        self.encoder.write_all(data)?;
        self.encoder
            .write_all(&vec![0; padding(data.len() as u64) as usize])?;
        if flush {
            self.encoder.flush()?;
        }
        Ok(std::mem::take(self.encoder.get_mut()))
    }

    /// The end-of-archive blocks and the gzip trailer.
    pub(crate) fn finish(mut self) -> std::io::Result<Vec<u8>> {
        self.encoder.write_all(&[0; 2 * BLOCK])?;
        self.encoder.finish()
    }
}

/// What the reader found next in the archive.
#[derive(Debug, PartialEq)]
pub(crate) enum Item {
    /// An entry begins, with its name and length.
    Begin { name: String, size: u64 },
    /// More of the current entry's data.
    Data(Vec<u8>),
    /// The current entry is complete.
    Finish,
}

enum State {
    Header,
    Data { remaining: u64, padding: u64 },
    End,
}

enum Mode {
    /// Waiting for the two bytes that tell gzip from a plain tar.
    Sniffing(Vec<u8>),
    Plain,
    Gzip(Box<MultiGzDecoder<Vec<u8>>>),
}

/// Reads a backup as it arrives: unpacks gzip, counts the unpacked bytes against `limit`, and
/// accepts only the entries a backup writes, regular files with short names.
pub(crate) struct Reader {
    mode: Mode,
    state: State,
    block: Vec<u8>,
    unpacked: u64,
    limit: u64,
}

impl Reader {
    pub(crate) fn new(limit: u64) -> Self {
        Self {
            mode: Mode::Sniffing(Vec::new()),
            state: State::Header,
            block: Vec::with_capacity(BLOCK),
            unpacked: 0,
            limit,
        }
    }

    /// Whether the archive's end-of-archive block has been read.
    pub(crate) fn ended(&self) -> bool {
        matches!(self.state, State::End)
    }

    /// Read more of the upload; what it holds is added to `items`.
    pub(crate) fn push(&mut self, mut input: &[u8], items: &mut Vec<Item>) -> Result<()> {
        if let Mode::Sniffing(head) = &mut self.mode {
            let take = (2 - head.len()).min(input.len());
            head.extend_from_slice(&input[..take]);
            input = &input[take..];
            if head.len() < 2 {
                return Ok(());
            }
            let head = std::mem::take(head);
            self.mode = if head == [0x1f, 0x8b] {
                Mode::Gzip(Box::new(MultiGzDecoder::new(Vec::new())))
            } else {
                Mode::Plain
            };
            self.feed(&head, items)?;
        }
        for step in input.chunks(GZIP_STEP) {
            self.feed(step, items)?;
        }
        Ok(())
    }

    /// The upload has ended: the rest gzip held back, and a check that the archive is whole.
    pub(crate) fn finish(mut self, items: &mut Vec<Item>) -> Result<()> {
        match std::mem::replace(&mut self.mode, Mode::Plain) {
            Mode::Sniffing(head) => {
                if !head.is_empty() {
                    bail!("The upload is not a Bilskirnir backup");
                }
            }
            Mode::Plain => {}
            Mode::Gzip(mut decoder) => {
                decoder
                    .try_finish()
                    .map_err(|_| anyhow::anyhow!("The backup's gzip data ended early"))?;
                let rest = std::mem::take(decoder.get_mut());
                self.read(&rest, items)?;
            }
        }
        if !self.ended() {
            bail!("The backup ended before its last entry; the file is incomplete");
        }
        Ok(())
    }

    fn feed(&mut self, mut input: &[u8], items: &mut Vec<Item>) -> Result<()> {
        while !input.is_empty() {
            let output = match &mut self.mode {
                Mode::Plain => return self.read(input, items),
                Mode::Gzip(decoder) => {
                    let used = decoder
                        .write(input)
                        .map_err(|_| anyhow::anyhow!("The backup's gzip data could not be read"))?;
                    if used == 0 {
                        bail!("The backup's gzip data could not be read");
                    }
                    input = &input[used..];
                    std::mem::take(decoder.get_mut())
                }
                Mode::Sniffing(_) => unreachable!("the format is known before data is fed"),
            };
            self.read(&output, items)?;
        }
        Ok(())
    }

    fn read(&mut self, mut input: &[u8], items: &mut Vec<Item>) -> Result<()> {
        self.unpacked += input.len() as u64;
        if self.unpacked > self.limit {
            bail!("The unpacked backup is larger than the import limit (IMAGE_ARCHIVE_MAX_MB)");
        }
        while !input.is_empty() {
            match &mut self.state {
                // Anything after the end-of-archive block is padding no reader looks at.
                State::End => return Ok(()),
                State::Header => {
                    let take = (BLOCK - self.block.len()).min(input.len());
                    self.block.extend_from_slice(&input[..take]);
                    input = &input[take..];
                    if self.block.len() == BLOCK {
                        let block = std::mem::take(&mut self.block);
                        self.header(&block, items)?;
                    }
                }
                State::Data { remaining, padding } => {
                    if *remaining > 0 {
                        let take = (*remaining).min(input.len() as u64) as usize;
                        match items.last_mut() {
                            Some(Item::Data(data)) => data.extend_from_slice(&input[..take]),
                            _ => items.push(Item::Data(input[..take].to_vec())),
                        }
                        *remaining -= take as u64;
                        input = &input[take..];
                    } else {
                        let take = (*padding).min(input.len() as u64);
                        *padding -= take;
                        input = &input[take as usize..];
                    }
                    if let State::Data {
                        remaining: 0,
                        padding: 0,
                    } = self.state
                    {
                        items.push(Item::Finish);
                        self.state = State::Header;
                    }
                }
            }
        }
        Ok(())
    }

    fn header(&mut self, block: &[u8], items: &mut Vec<Item>) -> Result<()> {
        if block.iter().all(|byte| *byte == 0) {
            self.state = State::End;
            return Ok(());
        }
        let (name, size) = entry_header(block)?;
        items.push(Item::Begin { name, size });
        self.state = if size == 0 {
            items.push(Item::Finish);
            State::Header
        } else {
            State::Data {
                remaining: size,
                padding: padding(size),
            }
        };
        Ok(())
    }
}

/// The name and size of an entry as a backup writes it: a USTAR header of a regular file whose
/// short name needs no prefix or extension header.
fn entry_header(block: &[u8]) -> Result<(String, u64)> {
    const DAMAGED: &str = "The upload is not a Bilskirnir backup, or the backup is damaged";
    let stored = std::str::from_utf8(&block[148..156])
        .ok()
        .map(|field| field.trim_matches([' ', '\0']))
        .and_then(|digits| u32::from_str_radix(digits, 8).ok())
        .context(DAMAGED)?;
    let sum: u32 = block
        .iter()
        .enumerate()
        .map(|(i, byte)| {
            if (148..156).contains(&i) {
                32
            } else {
                u32::from(*byte)
            }
        })
        .sum();
    if stored != sum || &block[257..263] != b"ustar\0" {
        bail!(DAMAGED);
    }
    if !matches!(block[156], b'0' | 0) || block[345..500].iter().any(|byte| *byte != 0) {
        bail!("The backup holds an entry that is not a plain file");
    }
    let end = block[..100]
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(100);
    let name = std::str::from_utf8(&block[..end])
        .ok()
        .filter(|name| !name.is_empty() && name.bytes().all(|b| b.is_ascii_graphic()))
        .context(DAMAGED)?;
    let size = size(&block[124..136]).context(DAMAGED)?;
    Ok((name.to_owned(), size))
}

#[cfg(test)]
#[path = "format_tests.rs"]
mod tests;
