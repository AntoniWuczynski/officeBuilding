//! The Boards group of Settings: checklist files the whole building shares.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BoardSettings {
    /// Absolute paths of checklist files, shown as boards in this order.
    pub shared_files: Vec<String>,
}

impl BoardSettings {
    /// The same files with each path trimmed and repeats dropped, or why one is not allowed.
    /// A path newly added must be a file that exists now; one already in `current` is kept even
    /// if its file is gone, so an unmounted drive never blocks editing the rest of the list.
    pub(super) fn checked(mut self, current: &Self) -> Result<Self, String> {
        let mut kept = Vec::new();
        for raw in self.shared_files {
            let path = raw.trim();
            let file = allowed(path)?;
            if !current.shared_files.iter().any(|c| c == path) && !Path::new(path).is_file() {
                return Err(format!("{path} is not a file"));
            }
            keep(&mut kept, file);
        }
        self.shared_files = kept.into_iter().map(|(path, _)| path).collect();
        Ok(self)
    }

    /// The loaded files that pass `allowed`: a hand-edited settings.json skips `apply`, so each
    /// entry that fails is dropped and the rest stay. A file that is missing at start (an
    /// unmounted drive, an unsynced folder) stays too: dropping it here would lose it on the next save.
    pub(super) fn clamped(mut self) -> Self {
        let mut kept = Vec::new();
        for raw in self.shared_files {
            if let Ok(path) = allowed(raw.trim()) {
                keep(&mut kept, path);
            }
        }
        self.shared_files = kept.into_iter().map(|(path, _)| path).collect();
        self
    }
}

/// The path with its canonical form (the path itself when it cannot be resolved), or why it is
/// not allowed. Existence is not checked here.
fn allowed(path: &str) -> Result<(String, PathBuf), String> {
    if !Path::new(path).is_absolute() {
        return Err(format!("{path:?} is not an absolute path"));
    }
    // The item ids that tick a line hold the path between pipes.
    if path.contains('|') {
        return Err(format!("{path} has a | in it"));
    }
    let canonical = Path::new(path)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(path));
    Ok((path.to_string(), canonical))
}

/// Add the file unless another path to the same file is already there.
fn keep(kept: &mut Vec<(String, PathBuf)>, file: (String, PathBuf)) {
    if !kept.iter().any(|(_, canonical)| *canonical == file.1) {
        kept.push(file);
    }
}
