//! Whether a plain tar reached its end-of-archive block. GNU tar unpacks an archive that stops at
//! a member boundary as if it were whole, so an upload of unknown size that ended early would
//! pass for a complete one. Following the archive from header to header, as GNU tar reads it,
//! tells the two apart; the members themselves are left to tar.
use crate::archives::tar::{BLOCK, padding, size};

/// A PAX header longer than this is passed over without reading the size it may give.
const PAX_LIMIT: u64 = 64 * 1024;

enum State {
    /// The next block is a header or the end-of-archive block.
    Header,
    /// Old GNU sparse headers follow a member's header before its data.
    Sparse { size: u64 },
    /// A member's data and padding, passed over.
    Skip(u64),
    /// A PAX header's records, kept for the size they give the member after them.
    Pax { remaining: u64, padding: u64 },
    /// GNU tar reads nothing after the end-of-archive block.
    End,
    /// A header whose size cannot be read; tar refuses it with its own error.
    Damaged,
}

pub(crate) struct Ending {
    state: State,
    block: Vec<u8>,
    pax: Vec<u8>,
    /// The size a PAX header gave the member that follows it.
    next_size: Option<u64>,
}

impl Ending {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Header,
            block: Vec::with_capacity(BLOCK),
            pax: Vec::new(),
            next_size: None,
        }
    }

    /// Whether the archive so far ends with its end-of-archive block.
    pub(crate) fn reached(&self) -> bool {
        matches!(self.state, State::End)
    }

    pub(crate) fn push(&mut self, mut input: &[u8]) {
        while !input.is_empty() {
            match &mut self.state {
                State::End | State::Damaged => return,
                State::Skip(remaining) => {
                    let take = (*remaining).min(input.len() as u64);
                    *remaining -= take;
                    input = &input[take as usize..];
                    if *remaining == 0 {
                        self.state = State::Header;
                    }
                }
                State::Pax { remaining, padding } => {
                    if *remaining > 0 {
                        let take = (*remaining).min(input.len() as u64);
                        self.pax.extend_from_slice(&input[..take as usize]);
                        *remaining -= take;
                        input = &input[take as usize..];
                    } else {
                        let take = (*padding).min(input.len() as u64);
                        *padding -= take;
                        input = &input[take as usize..];
                    }
                    if let State::Pax {
                        remaining: 0,
                        padding: 0,
                    } = self.state
                    {
                        self.next_size = pax_size(&std::mem::take(&mut self.pax));
                        self.state = State::Header;
                    }
                }
                State::Header | State::Sparse { .. } => {
                    let take = (BLOCK - self.block.len()).min(input.len());
                    self.block.extend_from_slice(&input[..take]);
                    input = &input[take..];
                    if self.block.len() == BLOCK {
                        let block = std::mem::take(&mut self.block);
                        match self.state {
                            // The last sparse header says so in its own extension flag.
                            State::Sparse { size } if block[504] == 0 => self.skip(size),
                            State::Sparse { .. } => {}
                            _ => self.header(&block),
                        }
                    }
                }
            }
        }
    }

    fn header(&mut self, block: &[u8]) {
        if block.iter().all(|byte| *byte == 0) {
            self.state = State::End;
            return;
        }
        let Ok(stated) = size(&block[124..136]) else {
            self.state = State::Damaged;
            return;
        };
        let typeflag = block[156];
        match typeflag {
            b'x' if stated > 0 && stated <= PAX_LIMIT => {
                self.state = State::Pax {
                    remaining: stated,
                    padding: padding(stated),
                };
            }
            // A global header, a GNU long name or link and an empty or oversized PAX header
            // leave the size of the next member alone.
            b'x' | b'g' | b'L' | b'K' => self.skip(stated),
            _ => {
                let size = self.next_size.take().unwrap_or(stated);
                match typeflag {
                    // GNU tar records no data for a hard link and passes over none for a
                    // directory.
                    b'1' | b'5' => self.skip(0),
                    b'S' if block[482] != 0 => self.state = State::Sparse { size },
                    _ => self.skip(size),
                }
            }
        }
    }

    fn skip(&mut self, size: u64) {
        self.state = match size + padding(size) {
            0 => State::Header,
            bytes => State::Skip(bytes),
        };
    }
}

/// The `size` record of a PAX header, as GNU tar reads it for the member that follows.
fn pax_size(records: &[u8]) -> Option<u64> {
    let mut rest = records;
    let mut size = None;
    while let Some(space) = rest.iter().position(|byte| *byte == b' ') {
        let length: usize = std::str::from_utf8(&rest[..space]).ok()?.parse().ok()?;
        if length <= space + 1 || length > rest.len() {
            break;
        }
        if let Some(value) = rest[space + 1..length - 1].strip_prefix(b"size=") {
            size = std::str::from_utf8(value).ok()?.parse().ok();
        }
        rest = &rest[length..];
    }
    size
}

#[cfg(test)]
#[path = "ending_tests.rs"]
mod tests;
