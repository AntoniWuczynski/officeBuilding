//! A Claude Code session's conversation, read from its transcript for the chat
//! view. The file is found from the session id alone, read by offset as it
//! grows, and mapped to `structured::Entry` (`map.rs`). The transcript format
//! is internal to Claude Code and changes between versions
//! (<https://code.claude.com/docs/en/sessions>), so every line is untrusted.

mod map;

use super::{feed_lines, grew_in_place, tail_of};
use crate::structured::Entry;
use map::Mapper;
use std::fs;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

/// The most of a transcript the first read takes, from its end.
const OPEN_TAIL: u64 = 8 * 1024 * 1024;
/// The longest session id looked for.
const ID_MAX: usize = 64;

/// The transcript of `session_id` under `<home>/.claude/projects`: a regular
/// file (not a link) whose canonical path stays inside that folder. When
/// several project folders hold one, the one modified last.
#[must_use]
pub fn find(home: &Path, session_id: &str) -> Option<PathBuf> {
    let valid = (1..=ID_MAX).contains(&session_id.len())
        && session_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-');
    if !valid {
        return None;
    }
    let projects = home.join(".claude").join("projects");
    let root = fs::canonicalize(&projects).ok()?;
    let file = format!("{session_id}.jsonl");
    fs::read_dir(&projects)
        .ok()?
        .filter_map(Result::ok)
        .map(|dir| dir.path().join(&file))
        .filter_map(|path| {
            let meta = fs::symlink_metadata(&path).ok()?;
            let inside = fs::canonicalize(&path).ok()?.starts_with(&root);
            (meta.is_file() && inside).then_some((meta.modified().ok()?, path))
        })
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, path)| path)
}

/// What a read found.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Read {
    /// Entries that are new or changed, in the order they happened.
    pub entries: Vec<Entry>,
    /// The transcript was read again from the start: forget what was shown before.
    pub reset: bool,
}

/// Where the last read of the file stopped, and which file it was.
struct Seen {
    offset: u64,
    tail: Vec<u8>,
    dev: u64,
    ino: u64,
}

/// Follows one session's transcript.
pub struct ChatReader {
    home: PathBuf,
    session_id: String,
    path: Option<PathBuf>,
    seen: Option<Seen>,
    mapper: Mapper,
}

impl ChatReader {
    /// A reader for `session_id`. It does not touch the disk yet.
    #[must_use]
    pub fn new(home: &Path, session_id: &str) -> Self {
        Self {
            home: home.to_path_buf(),
            session_id: session_id.to_string(),
            path: None,
            seen: None,
            mapper: Mapper::default(),
        }
    }

    /// Read what was appended since the last call. A transcript that is not there (yet) reads as nothing.
    pub fn read(&mut self) -> Read {
        if self.path.is_none() {
            self.path = find(&self.home, &self.session_id);
        }
        let Some(path) = &self.path else {
            return Read::default();
        };
        let opened = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)
            .and_then(|file| file.metadata().map(|meta| (file, meta)));
        let Some((mut file, meta)) = opened.ok().filter(|(_, meta)| meta.is_file()) else {
            self.path = None;
            return Read::default();
        };
        let (len, dev, ino) = (meta.len(), meta.dev(), meta.ino());
        let resume = self
            .seen
            .as_ref()
            .filter(|s| s.dev == dev && s.ino == ino)
            .filter(|s| grew_in_place(&mut file, len, s.offset, &s.tail))
            .map(|s| s.offset);
        let reset = resume.is_none() && self.seen.is_some();
        let start = resume.or_else(|| {
            self.mapper = Mapper::default();
            self.seen = None;
            self.first_offset(&mut file, len)
        });
        let Some(start) = start.filter(|&s| file.seek(SeekFrom::Start(s)).is_ok()) else {
            return Read {
                entries: Vec::new(),
                reset,
            };
        };
        let mapper = &mut self.mapper;
        let offset = feed_lines(BufReader::new(&mut file), start, true, |at, line| {
            mapper.line(at, line);
        });
        self.seen = Some(Seen {
            offset,
            tail: tail_of(&mut file, offset).unwrap_or_default(),
            dev,
            ino,
        });
        Read {
            entries: self.mapper.take_touched(),
            reset,
        }
    }

    /// Where a read from the start begins: 0, or just past the first line
    /// break in the last `OPEN_TAIL` bytes of a longer file.
    fn first_offset(&mut self, file: &mut fs::File, len: u64) -> Option<u64> {
        let Some(from) = len.checked_sub(OPEN_TAIL).filter(|&from| from > 0) else {
            return Some(0);
        };
        self.mapper.mark_earlier();
        file.seek(SeekFrom::Start(from)).ok()?;
        let mut skipped = Vec::new();
        let n = BufReader::new(file).read_until(b'\n', &mut skipped).ok()?;
        Some(from + u64::try_from(n).ok()?)
    }

    /// Everything kept, oldest first.
    #[must_use]
    pub fn kept(&self) -> Vec<Entry> {
        self.mapper.kept()
    }

    /// Older entries exist that are not kept: the file was longer than the first read took, or the cap dropped some.
    #[must_use]
    pub fn earlier(&self) -> bool {
        self.mapper.earlier()
    }
}

#[cfg(test)]
mod tests;
