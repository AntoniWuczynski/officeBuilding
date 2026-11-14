//! Which lines of a pull request's diff GitHub accepts an inline comment on.

use std::collections::{BTreeMap, BTreeSet};

/// The lines of each changed file's new version that sit inside the diff: where GitHub
/// accepts an inline comment on the RIGHT side.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChangedLines(BTreeMap<String, BTreeSet<u32>>);

/// A hunk being read: the lines still owed on each side, and the next new line number.
struct Hunk {
    old_left: u32,
    new_left: u32,
    next: u32,
}

impl ChangedLines {
    /// Read git's unified diff (what `gh pr diff` prints).
    #[must_use]
    pub fn parse(diff: &str) -> Self {
        let mut lines = Self::default();
        let mut path: Option<&str> = None;
        let mut hunk: Option<Hunk> = None;
        for line in diff.lines() {
            if line.starts_with("diff --git ") {
                path = None;
                hunk = None;
            } else if let Some(open) = hunk.as_mut() {
                lines.hunk_line(path, open, line);
                if open.old_left == 0 && open.new_left == 0 {
                    hunk = None;
                }
            } else if let Some(rest) = line.strip_prefix("+++ ") {
                path = rest
                    .strip_prefix("b/")
                    .map(|p| p.split('\t').next().unwrap_or(p));
            } else if line.starts_with("@@ ") {
                hunk = open_hunk(line);
            }
        }
        lines
    }

    fn hunk_line(&mut self, path: Option<&str>, hunk: &mut Hunk, line: &str) {
        let (old, new) = match line.chars().next() {
            Some('+') => (false, true),
            Some('-') => (true, false),
            Some('\\') => return,
            _ => (true, true),
        };
        if new {
            if let Some(path) = path {
                self.0.entry(path.to_owned()).or_default().insert(hunk.next);
            }
            hunk.next += 1;
            hunk.new_left = hunk.new_left.saturating_sub(1);
        }
        if old {
            hunk.old_left = hunk.old_left.saturating_sub(1);
        }
    }

    #[must_use]
    pub fn contains(&self, path: &str, line: u32) -> bool {
        self.0.get(path).is_some_and(|set| set.contains(&line))
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// `@@ -a[,b] +c[,d] @@`: the hunk it opens, if the header reads and holds any lines.
fn open_hunk(header: &str) -> Option<Hunk> {
    let mut parts = header.strip_prefix("@@ ")?.split_whitespace();
    let (_, old_left) = range(parts.next()?.strip_prefix('-')?)?;
    let (next, new_left) = range(parts.next()?.strip_prefix('+')?)?;
    (old_left > 0 || new_left > 0).then_some(Hunk {
        old_left,
        new_left,
        next,
    })
}

/// `start[,count]`, the count being 1 when absent.
fn range(text: &str) -> Option<(u32, u32)> {
    match text.split_once(',') {
        Some((start, count)) => Some((start.parse().ok()?, count.parse().ok()?)),
        None => Some((text.parse().ok()?, 1)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIFF: &str = include_str!("fixtures/pull-request.diff");

    fn set(lines: &ChangedLines, path: &str) -> Vec<u32> {
        lines
            .0
            .get(path)
            .map(|s| s.iter().copied().collect())
            .unwrap_or_default()
    }

    #[test]
    fn an_empty_diff_gives_an_empty_set() {
        assert!(ChangedLines::parse("").is_empty());
    }

    #[test]
    fn a_modified_file_keeps_context_and_added_lines_of_both_hunks() {
        let lines = ChangedLines::parse(DIFF);
        assert_eq!(
            set(&lines, "src/upload.rs"),
            [10, 11, 12, 13, 14, 15, 41, 42, 43, 44]
        );
        assert!(lines.contains("src/upload.rs", 13));
        assert!(!lines.contains("src/upload.rs", 16));
        assert!(!lines.contains("src/upload.rs", 40));
        assert!(!lines.contains("src/other.rs", 13));
    }

    #[test]
    fn a_hunk_ending_without_a_newline_is_counted() {
        let lines = ChangedLines::parse(DIFF);
        assert_eq!(set(&lines, "notes.txt"), [1, 2]);
    }

    #[test]
    fn a_header_without_counts_means_one_line() {
        let lines = ChangedLines::parse(DIFF);
        assert_eq!(set(&lines, "VERSION"), [1]);
    }

    #[test]
    fn a_new_file_is_all_added() {
        let lines = ChangedLines::parse(DIFF);
        assert_eq!(set(&lines, "src/retry.rs"), [1, 2, 3]);
    }

    #[test]
    fn a_renamed_file_is_known_by_its_new_path() {
        let lines = ChangedLines::parse(DIFF);
        assert_eq!(set(&lines, "src/labels.rs"), [1, 2, 3]);
        assert!(set(&lines, "src/names.rs").is_empty());
    }

    #[test]
    fn deleted_binary_and_quoted_files_have_no_lines() {
        let lines = ChangedLines::parse(DIFF);
        for path in lines.0.keys() {
            assert!(
                !path.contains("old.rs") && !path.contains("logo") && !path.contains("menu"),
                "{path}"
            );
        }
    }

    #[test]
    fn an_added_line_that_looks_like_a_file_header_starts_no_file() {
        let lines = ChangedLines::parse(DIFF);
        assert!(!lines.0.contains_key("not-a-file.txt"));
        assert_eq!(set(&lines, "src/tricky.txt"), [1, 2, 3]);
    }

    #[test]
    fn exactly_the_changed_files_are_known() {
        let lines = ChangedLines::parse(DIFF);
        let paths: Vec<&str> = lines.0.keys().map(String::as_str).collect();
        assert_eq!(
            paths,
            [
                "VERSION",
                "notes.txt",
                "src/labels.rs",
                "src/retry.rs",
                "src/tricky.txt",
                "src/upload.rs"
            ]
        );
    }

    #[test]
    fn an_empty_line_inside_a_hunk_is_context() {
        let diff = "diff --git a/a b/a\n--- a/a\n+++ b/a\n@@ -1,3 +1,3 @@\n x\n\n-y\n+z\n";
        assert_eq!(set(&ChangedLines::parse(diff), "a"), [1, 2, 3]);
    }

    #[test]
    fn a_path_is_cut_at_a_tab_and_a_bad_header_opens_nothing() {
        let diff = "diff --git a/a b/a\n+++ b/a b.txt\t\n@@ nonsense @@\n+x\n@@ -1 +1 @@\n+y\n";
        let lines = ChangedLines::parse(diff);
        assert_eq!(set(&lines, "a b.txt"), [1]);
        assert!(!lines.contains("a b.txt", 2));
    }
}
