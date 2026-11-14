//! Adding a call to a repo's human decisions file. The building never creates such a file
//! (OB-002) and never writes through a link: it only appends to one that is already there.

use super::{HUMAN_DECISION_FILES, find_file, is_status};
use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

/// What every call a manager adds is numbered after.
const CALL_PREFIX: &str = "## MGR-";

/// The floor's human decisions file, when the repo has one.
#[must_use]
pub fn decisions_file(root: &Path) -> Option<String> {
    find_file(root, HUMAN_DECISION_FILES)
}

/// One more than the highest `MGR-<n>` heading in a decisions file's text. 1 when there is none.
#[must_use]
pub fn next_call_id(text: &str) -> u32 {
    text.lines()
        .filter_map(|line| line.strip_prefix(CALL_PREFIX))
        .filter_map(|rest| {
            let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
            digits.parse::<u32>().ok()
        })
        .max()
        .map_or(1, |n| n.saturating_add(1))
}

/// Whether `title`, as a call's heading, would read as a settled call (`DECIDED …`, `DONE`).
#[must_use]
pub fn reads_settled(title: &str) -> bool {
    is_status(title.trim())
}

/// A call's section, as `parse_human_decisions` reads an open one.
#[must_use]
pub fn call_section(
    id: u32,
    title: &str,
    question: &str,
    options: &[String],
    date: &str,
) -> String {
    let mut section = format!(
        "{CALL_PREFIX}{id} — {title}\n\nAsked by the floor's manager on {date}.\n\n{question}\n"
    );
    if !options.is_empty() {
        let lines: Vec<String> = options.iter().map(|o| format!("- **{o}**")).collect();
        section = format!("{section}\n{}", lines.join("\n"));
    }
    section.trim_end().to_string()
}

/// Add `block` to the end of `root/file`. The file must exist and must not be a link.
pub fn append(root: &Path, file: &str, block: &str) -> Result<(), String> {
    let mut opened = OpenOptions::new()
        .append(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(root.join(file))
        .map_err(|e| {
            if e.raw_os_error() == Some(libc::ELOOP) {
                format!("{file} is a link, so the building will not write through it")
            } else {
                format!("could not open {file}: {e}")
            }
        })?;
    opened
        .write_all(format!("\n\n{block}\n").as_bytes())
        .map_err(|e| format!("could not write {file}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn temp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ob-append-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn next_call_id_counts_up_from_the_highest_mgr_heading() {
        assert_eq!(next_call_id(""), 1);
        assert_eq!(next_call_id("# Human decisions\n\n## D-4 — Other\n"), 1);
        let text =
            "## MGR-2 — a\n\n## MGR-7 — DECIDED 2026-11-12: b\n## MGR-3 — c\nMGR-90 in prose\n";
        assert_eq!(next_call_id(text), 8);
    }

    #[test]
    fn a_title_that_starts_with_a_status_reads_settled() {
        assert!(reads_settled("DECIDED yes"));
        assert!(reads_settled("done criteria"));
        assert!(!reads_settled("Which database?"));
        assert!(!reads_settled("Ship DECIDED"));
    }

    #[test]
    fn call_section_is_exactly_the_documented_text() {
        let options = vec![
            "Staging database".to_string(),
            "Local file first".to_string(),
        ];
        assert_eq!(
            call_section(
                3,
                "Which database should the importer use?",
                "Should the importer write to the staging database or to a local file first?",
                &options,
                "2026-11-12"
            ),
            "## MGR-3 — Which database should the importer use?\n\nAsked by the floor's manager on 2026-11-12.\n\nShould the importer write to the staging database or to a local file first?\n\n- **Staging database**\n- **Local file first**"
        );
        assert_eq!(
            call_section(1, "Ship it?", "Ready?", &[], "2026-11-12"),
            "## MGR-1 — Ship it?\n\nAsked by the floor's manager on 2026-11-12.\n\nReady?"
        );
    }

    #[test]
    fn append_adds_to_an_existing_file_and_never_creates_or_follows_one() {
        let dir = temp("write");
        fs::write(dir.join("HUMAN_DECISIONS.md"), "# Human decisions\n").unwrap();
        assert_eq!(decisions_file(&dir).as_deref(), Some("HUMAN_DECISIONS.md"));
        append(&dir, "HUMAN_DECISIONS.md", "## MGR-1 — Go?").unwrap();
        assert_eq!(
            fs::read_to_string(dir.join("HUMAN_DECISIONS.md")).unwrap(),
            "# Human decisions\n\n\n## MGR-1 — Go?\n"
        );

        assert!(append(&dir, "FOUNDER_DECISIONS.md", "x").is_err());
        assert!(!dir.join("FOUNDER_DECISIONS.md").exists(), "never created");

        let target = dir.join("elsewhere.md");
        fs::write(&target, "keep\n").unwrap();
        fs::remove_file(dir.join("HUMAN_DECISIONS.md")).unwrap();
        std::os::unix::fs::symlink(&target, dir.join("HUMAN_DECISIONS.md")).unwrap();
        assert_eq!(
            append(&dir, "HUMAN_DECISIONS.md", "## MGR-2 — Go?"),
            Err(
                "HUMAN_DECISIONS.md is a link, so the building will not write through it"
                    .to_string()
            )
        );
        assert_eq!(fs::read_to_string(&target).unwrap(), "keep\n");
        fs::remove_dir_all(&dir).unwrap();
    }
}
