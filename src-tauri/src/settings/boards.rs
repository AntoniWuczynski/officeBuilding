//! The Boards group of Settings: checklist files the whole building shares.

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BoardSettings {
    /// Absolute paths of checklist files, shown as boards in this order.
    pub shared_files: Vec<String>,
}

impl BoardSettings {
    /// The same files with each path trimmed and repeats dropped, or why one is not allowed.
    /// Only a file that exists now is taken: a file that later goes away just shows no items.
    pub(super) fn checked(mut self) -> Result<Self, String> {
        let mut kept: Vec<String> = Vec::new();
        for raw in self.shared_files {
            let path = raw.trim();
            if !Path::new(path).is_absolute() {
                return Err(format!("{path:?} is not an absolute path"));
            }
            // The item ids that tick a line hold the path between pipes.
            if path.contains('|') {
                return Err(format!("{path} has a | in it"));
            }
            if !Path::new(path).is_file() {
                return Err(format!("{path} is not a file"));
            }
            if !kept.iter().any(|k| k == path) {
                kept.push(path.to_string());
            }
        }
        self.shared_files = kept;
        Ok(self)
    }
}
