//! Following one append-only file by offset: open without following a link,
//! check it is a regular file, resume when the same file only grew, else start
//! again. The chat view's transcript reader and Codex's rollout reader share it.

use super::{feed_lines, grew_in_place, tail_of};
use std::fs;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::Path;

/// The most of a file a fresh read takes from its end, when the caller asks for a tail.
pub(in crate::discovery) const OPEN_TAIL: u64 = 8 * 1024 * 1024;

/// What `Follow::read` hands its caller, in order.
pub(in crate::discovery) enum Fed<'a> {
    /// The file is read anew (the first read, or it was replaced or cut short): forget what
    /// was held. `skipped`: the read begins past the head of the file.
    Fresh { skipped: bool },
    /// One complete line and the byte offset it starts at.
    Line { at: u64, text: &'a str },
}

/// Where the last read of the file stopped, and which file it was.
struct Seen {
    offset: u64,
    tail: Vec<u8>,
    dev: u64,
    ino: u64,
}

/// Follows one append-only file by offset.
#[derive(Default)]
pub(in crate::discovery) struct Follow {
    seen: Option<Seen>,
}

impl Follow {
    /// Feed `on` what was appended to `path` since the last call. `tail`: a fresh read takes
    /// only the last `tail` bytes, from the first line break in them (None: the whole file).
    /// None: `path` cannot be opened as a regular file. Some(reset): true when a file that
    /// was read before is read anew.
    pub(in crate::discovery) fn read(
        &mut self,
        path: &Path,
        tail: Option<u64>,
        mut on: impl FnMut(Fed<'_>),
    ) -> Option<bool> {
        let opened = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)
            .and_then(|file| file.metadata().map(|meta| (file, meta)));
        let (mut file, meta) = opened.ok().filter(|(_, meta)| meta.is_file())?;
        let (len, dev, ino) = (meta.len(), meta.dev(), meta.ino());
        let resume = self
            .seen
            .as_ref()
            .filter(|s| s.dev == dev && s.ino == ino)
            .filter(|s| grew_in_place(&mut file, len, s.offset, &s.tail))
            .map(|s| s.offset);
        let reset = resume.is_none() && self.seen.is_some();
        let start = resume.or_else(|| {
            self.seen = None;
            let from = tail
                .and_then(|t| len.checked_sub(t))
                .filter(|&from| from > 0);
            on(Fed::Fresh {
                skipped: from.is_some(),
            });
            match from {
                None => Some(0),
                Some(from) => first_break(&mut file, from),
            }
        });
        let Some(start) = start.filter(|&s| file.seek(SeekFrom::Start(s)).is_ok()) else {
            return Some(reset);
        };
        let offset = feed_lines(BufReader::new(&mut file), start, true, |at, text| {
            on(Fed::Line { at, text });
        });
        self.seen = Some(Seen {
            offset,
            tail: tail_of(&mut file, offset).unwrap_or_default(),
            dev,
            ino,
        });
        Some(reset)
    }
}

/// Just past the first line break at or after `from`.
fn first_break(file: &mut fs::File, from: u64) -> Option<u64> {
    file.seek(SeekFrom::Start(from)).ok()?;
    let mut skipped = Vec::new();
    let n = BufReader::new(file).read_until(b'\n', &mut skipped).ok()?;
    Some(from + u64::try_from(n).ok()?)
}
