//! A Claude Code session's conversation, read from its transcript for the chat
//! view. The file is found from the session id alone, read by offset as it
//! grows, and mapped to `structured::Entry` (`map.rs`). The transcript format
//! is internal to Claude Code and changes between versions
//! (<https://code.claude.com/docs/en/sessions>), so every line is untrusted.

pub mod limit;
mod map;

#[cfg(test)]
use super::feed_lines;
use super::follow::{Fed, Follow, OPEN_TAIL};
use crate::structured::Entry;
use limit::Reading;
use map::Mapper;
pub(in crate::discovery) use map::{KEPT, Loose, lenient, one_line};
use std::fs;
use std::path::{Path, PathBuf};
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

/// Follows one session's transcript.
pub struct ChatReader {
    home: PathBuf,
    session_id: String,
    path: Option<PathBuf>,
    follow: Follow,
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
            follow: Follow::default(),
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
        let mapper = &mut self.mapper;
        let Some(reset) = self.follow.read(path, Some(OPEN_TAIL), |fed| match fed {
            Fed::Fresh { skipped } => {
                *mapper = Mapper::default();
                if skipped {
                    mapper.mark_earlier();
                }
            }
            Fed::Line { at, text } => mapper.line(at, text),
        }) else {
            self.path = None;
            return Read::default();
        };
        Read {
            entries: self.mapper.take_touched(),
            reset,
        }
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

    /// What stands at the end of the main chain read so far: the proof of a
    /// used-up subscription, another API error, or nothing.
    #[must_use]
    pub fn limit(&self) -> &Reading {
        self.mapper.limit()
    }
}

#[cfg(test)]
mod tests;
