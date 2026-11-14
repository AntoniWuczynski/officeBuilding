//! The QA desk: a reviewer on each floor that reads new pull requests and posts its findings.

pub mod brief;
pub mod diff;
pub mod findings;
pub mod github;
pub mod record;
pub mod review;
pub mod watch;
pub mod worktree;

use std::path::{Path, PathBuf};

/// Where the throwaway worktrees of reviews live: one folder per floor.
#[must_use]
pub fn worktrees_dir(data: &Path) -> PathBuf {
    data.join("qa").join("worktrees")
}

/// Where each review keeps its diff and the reviewer's report.
#[must_use]
pub fn runs_dir(data: &Path) -> PathBuf {
    data.join("qa").join("runs")
}

/// The first line of `text` that is not blank, trimmed.
fn first_line(text: &str) -> Option<&str> {
    text.lines().map(str::trim).find(|line| !line.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_sit_under_the_data_folder() {
        let data = Path::new("/data");
        assert_eq!(worktrees_dir(data), Path::new("/data/qa/worktrees"));
        assert_eq!(runs_dir(data), Path::new("/data/qa/runs"));
    }
}
