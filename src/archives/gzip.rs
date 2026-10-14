//! The gzip layer around an archive: compression for exports and, for imports, decompression
//! fed to the tar filter in steps small enough that a highly compressed upload cannot fill memory.
use super::tar::{Contents, Filter, Rejected};
use anyhow::{Result, anyhow, bail};
use flate2::{Compression, write::GzEncoder, write::MultiGzDecoder};
use std::io::{self, Write};

/// Compressed input decoded per step. Deflate expands at most about a thousand times, so one
/// step adds at most a few dozen megabytes before its output is handed on.
const GZIP_STEP: usize = 16 * 1024;
const PLAIN_STEP: usize = 256 * 1024;

/// An export: the archive's first entry, then `docker save` output as it arrives.
pub(crate) struct Compressor(GzEncoder<Vec<u8>>);

impl Compressor {
    /// Starts with the metadata entry, flushed so the download begins before Docker has
    /// prepared the image. The fastest level: most of the size is already compressed layers on
    /// the containerd image store, and the rest is not worth a slower export.
    pub(crate) fn new(first: &[u8]) -> io::Result<(Self, Vec<u8>)> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
        encoder.write_all(first)?;
        encoder.flush()?;
        let output = std::mem::take(encoder.get_mut());
        Ok((Self(encoder), output))
    }

    pub(crate) fn push(&mut self, data: &[u8]) -> io::Result<Vec<u8>> {
        self.0.write_all(data)?;
        Ok(std::mem::take(self.0.get_mut()))
    }

    pub(crate) fn finish(self) -> io::Result<Vec<u8>> {
        self.0.finish()
    }
}

enum Mode {
    /// Waiting for the two bytes that tell gzip from a plain `docker save` tar.
    Sniffing(Filter, Vec<u8>),
    Plain(Filter),
    Gzip(MultiGzDecoder<Filter>),
}

/// An import: a `.tar.gz` or plain `.tar` upload, unpacked and filtered for `docker load`.
pub(crate) struct Unpacker(Mode);

impl Unpacker {
    pub(crate) fn new(limit: u64) -> Self {
        Self(Mode::Sniffing(Filter::new(limit), Vec::new()))
    }

    /// Unpack the start of `input`. Returns how much of it was used and the bytes for Docker;
    /// it stops early once `budget` bytes are waiting, and the caller passes the rest again.
    pub(crate) fn push(&mut self, input: &[u8], budget: usize) -> Result<(usize, Vec<u8>)> {
        let mut used = 0;
        if let Mode::Sniffing(_, head) = &mut self.0 {
            used = (2 - head.len()).min(input.len());
            head.extend_from_slice(&input[..used]);
            if head.len() < 2 {
                return Ok((used, Vec::new()));
            }
            self.choose()?;
        }
        let mut output = Vec::new();
        loop {
            output.append(&mut self.filter().take_output());
            if used == input.len() || output.len() >= budget {
                return Ok((used, output));
            }
            let rest = &input[used..];
            used += match &mut self.0 {
                Mode::Plain(filter) => {
                    let step = rest.len().min(PLAIN_STEP);
                    filter.push(&rest[..step])?;
                    step
                }
                Mode::Gzip(decoder) => match decoder.write(&rest[..rest.len().min(GZIP_STEP)]) {
                    Ok(0) => bail!("The archive's gzip data could not be read"),
                    Ok(step) => step,
                    Err(error) => return Err(unpack_error(error)),
                },
                Mode::Sniffing(..) => unreachable!("the format was chosen above"),
            };
        }
    }

    /// The archive's closing bytes for Docker, once the upload is complete and checked.
    pub(crate) fn finish(self) -> Result<(Vec<u8>, Contents)> {
        let filter = match self.0 {
            Mode::Sniffing(mut filter, head) => {
                if head.is_empty() {
                    bail!("The upload is empty");
                }
                filter.push(&head)?;
                filter
            }
            Mode::Plain(filter) => filter,
            Mode::Gzip(decoder) => decoder.finish().map_err(unpack_error)?,
        };
        let mut output = Vec::new();
        let mut filter = filter;
        output.append(&mut filter.take_output());
        let (tail, contents) = filter.finish()?;
        output.extend(tail);
        Ok((output, contents))
    }

    fn choose(&mut self) -> Result<()> {
        let Mode::Sniffing(filter, head) =
            std::mem::replace(&mut self.0, Mode::Plain(Filter::new(0)))
        else {
            return Ok(());
        };
        self.0 = if head == [0x1f, 0x8b] {
            let mut decoder = MultiGzDecoder::new(filter);
            decoder.write_all(&head).map_err(unpack_error)?;
            Mode::Gzip(decoder)
        } else {
            let mut filter = filter;
            filter.push(&head)?;
            Mode::Plain(filter)
        };
        Ok(())
    }

    fn filter(&mut self) -> &mut Filter {
        match &mut self.0 {
            Mode::Sniffing(filter, _) | Mode::Plain(filter) => filter,
            Mode::Gzip(decoder) => decoder.get_mut(),
        }
    }
}

/// The filter's own refusal passes through the decoder unchanged; anything else is gzip's.
fn unpack_error(error: io::Error) -> anyhow::Error {
    match error
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<Rejected>())
    {
        Some(rejected) => anyhow!("{rejected}"),
        None => anyhow!("The archive's gzip data is damaged or incomplete ({error})"),
    }
}

#[cfg(test)]
#[path = "gzip_tests.rs"]
mod tests;
