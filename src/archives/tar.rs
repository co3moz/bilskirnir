//! A `docker save` archive, rewritten in passing so `docker load` cannot apply names of its own.
//!
//! The archive streams through block by block. Layer and configuration blobs pass unchanged;
//! `manifest.json` and `index.json` are rebuilt without the fields that name images, the legacy
//! `repositories` file is dropped and Bilskirnir's own `bilskirnir.json` is taken out. The two
//! rebuilt files are held back until the whole archive has been read and checked, and go out with
//! the closing blocks, so a stream that stops early or fails a check never gives Docker an image
//! it could register.
//!
//! Docker reads the archive with Go's `archive/tar`, so names, sizes and extension headers are
//! interpreted here as that reader interprets them: an entry this filter passes on as a layer
//! must not be a manifest to Docker.
use anyhow::{Context, Result, bail};
use serde_json::{Map, Value, json};
use std::collections::HashMap;

pub(crate) const BLOCK: usize = 512;
/// Bilskirnir's description of an exported image, the first entry of its archives.
pub(crate) const METADATA: &str = "bilskirnir.json";
/// The files that tell Docker what an archive holds. A save never nests them in a directory.
const SPECIAL: [&str; 5] = [
    "manifest.json",
    "index.json",
    "repositories",
    "oci-layout",
    METADATA,
];
/// `manifest.json` and `index.json` are a few kilobytes in every Docker archive.
const DESCRIPTOR_LIMIT: u64 = 4 << 20;
/// The metadata of a script build carries its build script and its compose's setup script, up
/// to a megabyte each before escaping.
const METADATA_LIMIT: u64 = 16 << 20;
/// Go refuses longer extension headers.
const EXTENSION_LIMIT: u64 = 1 << 20;
/// Image configurations are JSON blobs of a few kilobytes, kept to check the image's labels
/// before Docker has the whole archive.
const CONFIG_LIMIT: u64 = 4 << 20;
const CONFIGS_TOTAL: usize = 32 << 20;

/// What the archive described about itself, once it has been read to its end.
pub(crate) struct Contents {
    /// The raw `bilskirnir.json`, when the archive has one.
    pub metadata: Option<Vec<u8>>,
}

#[derive(Clone, Copy, PartialEq)]
enum Format {
    V7,
    Ustar,
    Star,
    Gnu,
}

#[derive(Clone, Copy)]
enum Kept {
    Pax,
    LongName,
    LongLink,
    Manifest,
    Index,
    Metadata,
}

enum Sink {
    Forward,
    /// Forwarded and kept, in case it is the image configuration.
    Tee(Vec<u8>, String),
    Keep(Kept, Vec<u8>),
    Discard,
}

enum State {
    Header,
    Data {
        remaining: u64,
        padding: u64,
        sink: Sink,
    },
    End,
}

/// Extension headers wait for the entry they describe, and go out only if that entry does.
#[derive(Default)]
struct Extension {
    raw: Vec<u8>,
    pax_path: Option<Vec<u8>>,
    pax_size: Option<u64>,
    long_name: Option<Vec<u8>>,
}

pub(crate) struct Filter {
    output: Vec<u8>,
    block: Vec<u8>,
    state: State,
    extension: Extension,
    read: u64,
    limit: u64,
    manifest: Option<Vec<u8>>,
    index: Option<Vec<u8>>,
    metadata: Option<Vec<u8>>,
    configs: HashMap<String, Vec<u8>>,
    configs_size: usize,
}

impl Filter {
    /// `limit` bounds the unpacked archive, so a small compressed upload cannot fill the disk.
    pub(crate) fn new(limit: u64) -> Self {
        Self {
            output: Vec::new(),
            block: Vec::with_capacity(BLOCK),
            state: State::Header,
            extension: Extension::default(),
            read: 0,
            limit,
            manifest: None,
            index: None,
            metadata: None,
            configs: HashMap::new(),
            configs_size: 0,
        }
    }

    /// The bytes Docker may have so far.
    pub(crate) fn take_output(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.output)
    }

    /// Read more of the archive.
    pub(crate) fn push(&mut self, mut input: &[u8]) -> Result<()> {
        self.read += input.len() as u64;
        if self.read > self.limit {
            bail!(
                "The unpacked archive is larger than the {} MiB import limit (IMAGE_ARCHIVE_MAX_MB)",
                self.limit >> 20
            );
        }
        while !input.is_empty() {
            match std::mem::replace(&mut self.state, State::Header) {
                // Go stops at the first zero block and so does Docker; anything after it is not read.
                State::End => {
                    self.state = State::End;
                    return Ok(());
                }
                State::Header => {
                    let take = (BLOCK - self.block.len()).min(input.len());
                    self.block.extend_from_slice(&input[..take]);
                    input = &input[take..];
                    if self.block.len() == BLOCK {
                        let block: [u8; BLOCK] = std::mem::take(&mut self.block)
                            .try_into()
                            .expect("a full block");
                        self.header(&block)?;
                    }
                }
                State::Data {
                    mut remaining,
                    mut padding,
                    mut sink,
                } => {
                    if remaining > 0 {
                        let take = remaining.min(input.len() as u64) as usize;
                        let chunk = &input[..take];
                        match &mut sink {
                            Sink::Forward => self.output.extend_from_slice(chunk),
                            Sink::Tee(kept, _) => {
                                self.output.extend_from_slice(chunk);
                                // Every configuration is a JSON object; a layer is let go at once.
                                if kept.is_empty() && chunk[0] != b'{' {
                                    sink = Sink::Forward;
                                } else {
                                    kept.extend_from_slice(chunk);
                                }
                            }
                            Sink::Keep(_, kept) => kept.extend_from_slice(chunk),
                            Sink::Discard => {}
                        }
                        remaining -= take as u64;
                        input = &input[take..];
                    } else {
                        let take = padding.min(input.len() as u64) as usize;
                        if matches!(sink, Sink::Forward | Sink::Tee(..)) {
                            self.output.extend_from_slice(&input[..take]);
                        }
                        padding -= take as u64;
                        input = &input[take..];
                    }
                    if remaining == 0 && padding == 0 {
                        self.complete(sink)?;
                    } else {
                        self.state = State::Data {
                            remaining,
                            padding,
                            sink,
                        };
                    }
                }
            }
        }
        Ok(())
    }

    /// Check the archive as a whole and return its last blocks for Docker, with what it described.
    pub(crate) fn finish(mut self) -> Result<(Vec<u8>, Contents)> {
        // Go also accepts an archive that ends at an entry boundary without its zero blocks.
        let at_boundary = matches!(self.state, State::Header) && self.block.is_empty();
        if !(matches!(self.state, State::End) || at_boundary) || !self.extension.raw.is_empty() {
            bail!("The archive ends in the middle of an entry; the file is incomplete");
        }
        let manifest = self
            .manifest
            .take()
            .context("The archive has no manifest.json; create it with docker save")?;
        let (manifest, config) = rebuild_manifest(&manifest)?;
        let config = self.configs.get(&config).with_context(|| {
            format!("The archive's image configuration {config} is missing or larger than 4 MiB")
        })?;
        check_desktop(config)?;
        let mut tail = self.take_output();
        if let Some(index) = self.index.take() {
            tail.extend(entry("index.json", &rebuild_index(&index)?, 0));
        }
        tail.extend(entry("manifest.json", &manifest, 0));
        tail.extend([0; 2 * BLOCK]);
        Ok((
            tail,
            Contents {
                metadata: self.metadata,
            },
        ))
    }

    fn begin(&mut self, size: u64, sink: Sink) -> Result<()> {
        if size == 0 {
            return self.complete(sink);
        }
        self.state = State::Data {
            remaining: size,
            padding: padding(size),
            sink,
        };
        Ok(())
    }

    fn header(&mut self, block: &[u8; BLOCK]) -> Result<()> {
        if block.iter().all(|byte| *byte == 0) {
            self.extension = Extension::default();
            self.state = State::End;
            return Ok(());
        }
        let format = format(block)?;
        let typeflag = block[156];
        let size = size(&block[124..136]).context("The archive has an invalid entry size")?;
        match typeflag {
            b'x' | b'L' | b'K' => {
                if size > EXTENSION_LIMIT {
                    bail!("The archive has an oversized extension header");
                }
                self.extension.raw.extend_from_slice(block);
                let kept = match typeflag {
                    b'x' => Kept::Pax,
                    b'L' => Kept::LongName,
                    _ => Kept::LongLink,
                };
                self.begin(size, Sink::Keep(kept, Vec::new()))
            }
            // Go returns a global header as an entry of its own and forgets what came before it.
            b'g' => {
                let extension = std::mem::take(&mut self.extension);
                self.output.extend(extension.raw);
                self.output.extend_from_slice(block);
                self.begin(size, Sink::Forward)
            }
            b'S' => bail!("The archive contains a sparse file, which docker save never writes"),
            _ => self.entry(block, format, typeflag, size),
        }
    }

    fn entry(
        &mut self,
        block: &[u8; BLOCK],
        format: Format,
        typeflag: u8,
        size: u64,
    ) -> Result<()> {
        let extension = std::mem::take(&mut self.extension);
        // Go's precedence: the GNU long name, then the PAX path, then the header's own name.
        let mut name = header_name(block, format);
        if let Some(path) = extension.pax_path.filter(|path| !path.is_empty()) {
            name = path;
        }
        if let Some(long) = extension.long_name.filter(|long| !long.is_empty()) {
            name = long;
        }
        let typeflag = match typeflag {
            0 if name.ends_with(b"/") => b'5',
            0 => b'0',
            other => other,
        };
        let size = if header_only(typeflag) {
            0
        } else {
            extension.pax_size.unwrap_or(size)
        };
        let path = clean(&name)?;
        let regular = typeflag == b'0';
        match special(&path)? {
            Some(special) if !regular => bail!("The archive's {special} is not a regular file"),
            Some(special @ ("manifest.json" | "index.json" | METADATA)) => {
                let limit = if special == METADATA {
                    METADATA_LIMIT
                } else {
                    DESCRIPTOR_LIMIT
                };
                if size > limit {
                    bail!("The archive's {special} is larger than {} MiB", limit >> 20);
                }
                let kept = match special {
                    "manifest.json" => Kept::Manifest,
                    "index.json" => Kept::Index,
                    _ => Kept::Metadata,
                };
                self.begin(size, Sink::Keep(kept, Vec::new()))
            }
            Some("repositories") => self.begin(size, Sink::Discard),
            _ => {
                self.output.extend(extension.raw);
                self.output.extend_from_slice(block);
                let sink = if regular
                    && size <= CONFIG_LIMIT
                    && self.configs_size + size as usize <= CONFIGS_TOTAL
                {
                    Sink::Tee(Vec::new(), String::from_utf8_lossy(&path).into_owned())
                } else {
                    Sink::Forward
                };
                self.begin(size, sink)
            }
        }
    }

    fn complete(&mut self, sink: Sink) -> Result<()> {
        self.state = State::Header;
        let (kept, data) = match sink {
            Sink::Forward | Sink::Discard => return Ok(()),
            Sink::Tee(data, path) => {
                if !data.is_empty() {
                    self.configs_size += data.len();
                    self.configs.insert(path, data);
                }
                return Ok(());
            }
            Sink::Keep(kept, data) => (kept, data),
        };
        if matches!(kept, Kept::Pax | Kept::LongName | Kept::LongLink) {
            self.extension.raw.extend_from_slice(&data);
            self.extension.raw.resize(
                self.extension.raw.len() + padding(data.len() as u64) as usize,
                0,
            );
        }
        match kept {
            // Each PAX header replaces the one before it, as in Go.
            Kept::Pax => (self.extension.pax_path, self.extension.pax_size) = pax(&data)?,
            Kept::LongName => self.extension.long_name = Some(until_nul(&data).to_vec()),
            Kept::LongLink => {}
            Kept::Manifest => self.manifest = Some(data),
            Kept::Index => self.index = Some(data),
            Kept::Metadata => self.metadata = Some(data),
        }
        Ok(())
    }
}

impl std::io::Write for Filter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.push(buf)
            .map_err(|error| std::io::Error::other(Rejected(format!("{error:#}"))))?;
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// A refusal from the filter, kept apart from gzip's own errors on the way through the decoder.
#[derive(Debug)]
pub(crate) struct Rejected(pub String);
impl std::fmt::Display for Rejected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Rejected {}

/// One regular file as a USTAR entry, padded to whole blocks. Names must fit the header.
pub(crate) fn entry(name: &str, data: &[u8], mtime: u64) -> Vec<u8> {
    assert!(name.len() <= 100, "entry name too long");
    let mut header = [0u8; BLOCK];
    header[..name.len()].copy_from_slice(name.as_bytes());
    octal(&mut header[100..108], 0o644);
    octal(&mut header[108..116], 0);
    octal(&mut header[116..124], 0);
    octal(&mut header[124..136], data.len() as u64);
    octal(&mut header[136..148], mtime);
    header[156] = b'0';
    header[257..263].copy_from_slice(b"ustar\0");
    header[263..265].copy_from_slice(b"00");
    header[148..156].fill(b' ');
    let sum: u32 = header.iter().map(|byte| u32::from(*byte)).sum();
    header[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
    let mut out = header.to_vec();
    out.extend_from_slice(data);
    out.resize(out.len() + padding(data.len() as u64) as usize, 0);
    out
}

fn octal(field: &mut [u8], value: u64) {
    let digits = format!("{value:0width$o}", width = field.len() - 1);
    field[..digits.len()].copy_from_slice(digits.as_bytes());
    field[digits.len()] = 0;
}

pub(crate) fn padding(size: u64) -> u64 {
    (BLOCK as u64 - size % BLOCK as u64) % BLOCK as u64
}

/// Entries whose size Go ignores: nothing follows their header.
fn header_only(typeflag: u8) -> bool {
    matches!(typeflag, b'1'..=b'6')
}

fn until_nul(bytes: &[u8]) -> &[u8] {
    bytes
        .iter()
        .position(|byte| *byte == 0)
        .map_or(bytes, |end| &bytes[..end])
}

/// An entry's data length; Go refuses a negative one.
pub(crate) fn size(field: &[u8]) -> Result<u64> {
    u64::try_from(numeric(field)?).context("negative size")
}

/// Go's `parseNumeric`: base-256 when the high bit is set, otherwise octal.
fn numeric(field: &[u8]) -> Result<i64> {
    if field[0] & 0x80 == 0 {
        return octal_value(field);
    }
    let invert = if field[0] & 0x40 != 0 { 0xff } else { 0 };
    let mut value: u64 = 0;
    for (i, byte) in field.iter().enumerate() {
        let mut byte = byte ^ invert;
        if i == 0 {
            byte &= 0x7f;
        }
        if value >> 56 != 0 {
            bail!("numeric field overflows");
        }
        value = value << 8 | u64::from(byte);
    }
    if value >> 63 != 0 {
        bail!("numeric field overflows");
    }
    let value = value as i64;
    Ok(if invert == 0xff { !value } else { value })
}

/// Go's `parseOctal`: the digits between leading and trailing spaces or NULs, up to a NUL.
fn octal_value(field: &[u8]) -> Result<i64> {
    let start = field
        .iter()
        .position(|byte| *byte != b' ' && *byte != 0)
        .unwrap_or(field.len());
    let end = field
        .iter()
        .rposition(|byte| *byte != b' ' && *byte != 0)
        .map_or(start, |end| end + 1);
    let digits = until_nul(&field[start..end]);
    if digits.is_empty() {
        return Ok(0);
    }
    if !digits.iter().all(|byte| (b'0'..=b'7').contains(byte)) {
        bail!("not an octal number");
    }
    let digits = std::str::from_utf8(digits).expect("octal digits are ASCII");
    Ok(u64::from_str_radix(digits, 8)? as i64)
}

/// Go's format detection, which decides where a header's name prefix is read from.
fn format(block: &[u8; BLOCK]) -> Result<Format> {
    let stored = octal_value(&block[148..156]).ok();
    let (mut unsigned, mut signed) = (0i64, 0i64);
    for (i, byte) in block.iter().enumerate() {
        let byte = if (148..156).contains(&i) { b' ' } else { *byte };
        unsigned += i64::from(byte);
        signed += i64::from(byte as i8);
    }
    if !stored.is_some_and(|stored| stored == unsigned || stored == signed) {
        bail!("The upload is not a tar archive, or the archive is damaged");
    }
    let (magic, version, trailer) = (&block[257..263], &block[263..265], &block[508..512]);
    Ok(if magic == b"ustar\0" && trailer == b"tar\0" {
        Format::Star
    } else if magic == b"ustar\0" {
        Format::Ustar
    } else if magic == b"ustar " && version == b" \0" {
        Format::Gnu
    } else {
        Format::V7
    })
}

fn header_name(block: &[u8; BLOCK], format: Format) -> Vec<u8> {
    let name = until_nul(&block[..100]);
    let prefix: &[u8] = match format {
        Format::Ustar => until_nul(&block[345..500]),
        Format::Star => until_nul(&block[345..476]),
        // Go reads the USTAR prefix of a GNU header whose access and change times do not parse,
        // for archives written by Go releases before 1.8.
        Format::Gnu => {
            let unreadable = |field: &[u8]| field[0] != 0 && numeric(field).is_err();
            let prefix = until_nul(&block[345..500]);
            if (unreadable(&block[345..357]) || unreadable(&block[357..369])) && prefix.is_ascii() {
                prefix
            } else {
                b""
            }
        }
        Format::V7 => b"",
    };
    if prefix.is_empty() {
        name.to_vec()
    } else {
        [prefix, b"/", name].concat()
    }
}

/// Where Docker writes an entry, relative to the archive root: leading slashes and `.` go and
/// `..` is resolved. A name that leaves the root is refused, as Docker refuses it.
fn clean(name: &[u8]) -> Result<Vec<u8>> {
    let rooted = name.starts_with(b"/");
    let mut parts: Vec<&[u8]> = Vec::new();
    for part in name.split(|byte| *byte == b'/') {
        match part {
            b"" | b"." => {}
            b".." => {
                if parts.pop().is_none() && !rooted {
                    bail!("The archive has an entry outside its root");
                }
            }
            part => parts.push(part),
        }
    }
    Ok(parts.join(&b'/'))
}

/// A top-level file Docker reads for names, or an error for one anywhere else: through a
/// symbolic link, a nested copy could still land at the top.
fn special(path: &[u8]) -> Result<Option<&'static str>> {
    if let Some(special) = SPECIAL.iter().find(|special| special.as_bytes() == path) {
        return Ok(Some(special));
    }
    if let Some(special) = path
        .split(|byte| *byte == b'/')
        .find_map(|part| SPECIAL.iter().find(|special| special.as_bytes() == part))
    {
        bail!("The archive has an unexpected nested {special}");
    }
    Ok(None)
}

/// Go's `parsePAX`: the path and size records, with the same validation.
fn pax(data: &[u8]) -> Result<(Option<Vec<u8>>, Option<u64>)> {
    const INVALID: &str = "The archive has an invalid PAX header";
    let (mut path, mut size) = (None, None);
    let mut rest = data;
    while !rest.is_empty() {
        let space = rest
            .iter()
            .position(|byte| *byte == b' ')
            .context(INVALID)?;
        let length: usize = std::str::from_utf8(&rest[..space])
            .ok()
            .and_then(|digits| digits.parse().ok())
            .context(INVALID)?;
        if length < 5 || length > rest.len() || length <= space + 1 || rest[length - 1] != b'\n' {
            bail!(INVALID);
        }
        let record = &rest[space + 1..length - 1];
        let equals = record
            .iter()
            .position(|byte| *byte == b'=')
            .context(INVALID)?;
        let (key, value) = (&record[..equals], &record[equals + 1..]);
        let nul = |bytes: &[u8]| bytes.contains(&0);
        let valid = !key.is_empty()
            && match key {
                b"path" | b"linkpath" | b"uname" | b"gname" => !nul(value),
                _ => !nul(key),
            };
        if !valid {
            bail!(INVALID);
        }
        if key.starts_with(b"GNU.sparse.") {
            bail!("The archive contains a sparse file, which docker save never writes");
        }
        match key {
            b"path" => path = Some(value.to_vec()),
            b"size" => size = Some(value.to_vec()),
            _ => {}
        }
        rest = &rest[length..];
    }
    // An empty value keeps the header's own field, as in Go.
    let size = match size.filter(|size| !size.is_empty()) {
        Some(size) => Some(
            std::str::from_utf8(&size)
                .ok()
                .and_then(|size| size.parse::<i64>().ok())
                .and_then(|size| u64::try_from(size).ok())
                .context(INVALID)?,
        ),
        None => None,
    };
    Ok((path, size))
}

/// Go matches JSON keys to fields without regard to case, so `repotags` would still name an
/// image. The manifest is written anew from the fields Docker needs, under their exact names,
/// and returns with the path of the configuration it names.
fn rebuild_manifest(data: &[u8]) -> Result<(Vec<u8>, String)> {
    let items: Vec<Map<String, Value>> =
        serde_json::from_slice(data).context("The archive's manifest.json is not valid")?;
    let [item] = items.as_slice() else {
        bail!(
            "The archive must contain exactly one image; its manifest.json lists {}",
            items.len()
        );
    };
    let config = item
        .get("Config")
        .and_then(Value::as_str)
        .context("The archive's manifest.json names no image configuration")?;
    let mut rebuilt = Map::new();
    rebuilt.insert("Config".into(), json!(config));
    rebuilt.insert("RepoTags".into(), json!([]));
    for key in ["Layers", "Parent", "LayerSources"] {
        if let Some(value) = item.get(key) {
            rebuilt.insert(key.into(), value.clone());
        }
    }
    let config = String::from_utf8_lossy(&clean(config.as_bytes())?).into_owned();
    Ok((serde_json::to_vec(&[rebuilt])?, config))
}

/// The OCI index names images through descriptor annotations; it is written anew without any.
fn rebuild_index(data: &[u8]) -> Result<Vec<u8>> {
    let index: Map<String, Value> =
        serde_json::from_slice(data).context("The archive's index.json is not valid")?;
    let manifests = index
        .get("manifests")
        .and_then(Value::as_array)
        .context("The archive's index.json lists no images")?;
    let mut digests = std::collections::BTreeSet::new();
    let mut rebuilt = Vec::new();
    for manifest in manifests {
        let digest = manifest
            .get("digest")
            .and_then(Value::as_str)
            .context("The archive's index.json lists an image without a digest")?;
        digests.insert(digest);
        let mut descriptor = Map::new();
        for key in ["mediaType", "digest", "size", "platform"] {
            if let Some(value) = manifest.get(key) {
                descriptor.insert(key.into(), value.clone());
            }
        }
        rebuilt.push(Value::Object(descriptor));
    }
    // A save lists an image once per tag, always with the same digest.
    if digests.len() != 1 {
        bail!(
            "The archive must contain exactly one image; its index.json lists {}",
            digests.len()
        );
    }
    let mut out = Map::new();
    out.insert("schemaVersion".into(), json!(2));
    if let Some(media_type) = index.get("mediaType") {
        out.insert("mediaType".into(), media_type.clone());
    }
    out.insert("manifests".into(), Value::Array(rebuilt));
    Ok(serde_json::to_vec(&out)?)
}

/// An early answer for the common mistake; the manager checks the loaded image again, since
/// Docker's reading of a deliberately ambiguous configuration can differ from this one.
fn check_desktop(config: &[u8]) -> Result<()> {
    let config: Value = serde_json::from_slice(config)
        .context("The archive's image configuration is not valid JSON")?;
    if config["config"]["Labels"]["io.wsl-desktop.desktop"] != "true" {
        bail!("The archive does not contain a Bilskirnir desktop image");
    }
    Ok(())
}

#[cfg(test)]
#[path = "tar_tests.rs"]
mod tests;
