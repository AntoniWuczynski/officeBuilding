//! The handover brief: what the receiving tool is told about the desk it takes over, and the
//! owner-only file it is kept in. Hidden reasoning never appears, tool output is cut short and
//! file changes are not repeated.

use super::{Task, TaskKind, Tool, Window, Work, head, tidy, tidy_line};
use crate::structured::{Entry, ToolStatus};
use chrono::{DateTime, SecondsFormat};
use std::fs::{self, DirBuilder, OpenOptions, Permissions};
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

/// Characters of rendered conversation, counted from the end.
pub const CONVERSATION_MAX: usize = 200_000;
/// Characters of one tool result.
pub const RESULT_MAX: usize = 1_500;
/// Characters of one tool call's line.
pub const TOOL_LINE_MAX: usize = 200;
/// Characters of one notice.
pub const NOTICE_MAX: usize = 500;
/// Briefs kept on disk (G12).
pub const KEEP: usize = 10;

/// Between two entries of the conversation.
const GAP: &str = "\n\n";
const EARLIER: &str = "(Earlier parts of the conversation are left out.)";

/// What the brief says about the handover itself.
pub struct Facts<'a> {
    /// "h-12".
    pub id: &'a str,
    pub from: Tool,
    pub to: Tool,
    pub by_human: bool,
    pub at: i64,
    pub window: Option<Window>,
    pub reset_at: Option<i64>,
    pub title: &'a str,
    pub folder: &'a Path,
    pub floor: &'a str,
    pub branch: Option<&'a str>,
    pub worktree: bool,
    /// What was running when the old session was ended.
    pub work: &'a Work,
    /// `git::summary`'s text.
    pub git: &'a str,
    /// The reader did not keep the start of the conversation.
    pub earlier: bool,
}

/// A Unix time as RFC 3339 in UTC.
fn time(at: i64) -> String {
    DateTime::from_timestamp(at, 0).map_or_else(
        || format!("{at} (Unix seconds)"),
        |t| t.to_rfc3339_opts(SecondsFormat::Secs, true),
    )
}

/// One untrusted line of the header, or "not known" when nothing is left of it.
fn known(raw: &str) -> String {
    let text = tidy_line(raw);
    if text.is_empty() {
        "not known".to_owned()
    } else {
        text
    }
}

fn task(task: &Task) -> String {
    let kind = match task.kind {
        TaskKind::Workflow => "a workflow",
        TaskKind::Subagent => "a background subagent",
        TaskKind::Shell => "a background shell",
        TaskKind::Scheduled => "a scheduled wake-up",
        TaskKind::Other => "a background task",
    };
    match task.name.as_deref().map(tidy_line) {
        Some(name) if !name.is_empty() => format!("{kind} \"{name}\""),
        _ => kind.to_owned(),
    }
}

fn ended(work: &Work) -> String {
    match work {
        Work::Clear => "nothing the building knows of".to_owned(),
        // A running list with nothing named in it says no more than an unknown one.
        Work::Running { tasks } if !tasks.is_empty() => {
            tasks.iter().map(task).collect::<Vec<_>>().join(", ")
        }
        Work::Running { .. } | Work::Unknown => {
            "the building could not tell what was running".to_owned()
        }
    }
}

fn header(facts: &Facts<'_>) -> String {
    let from = facts.from.label();
    let limit = match facts.window {
        Some(Window::FiveHour) => "the five-hour limit",
        Some(Window::SevenDay) => "the weekly limit",
        None => "which limit is not known",
    };
    let reset = facts.reset_at.map_or_else(
        || "The reset time is not known.".to_owned(),
        |at| format!("It resets at {}.", time(at)),
    );
    let asked = if facts.by_human {
        "A Human"
    } else {
        "the building itself"
    };
    let branch = facts.branch.map_or_else(|| "not known".to_owned(), known);
    let worktree = if facts.worktree {
        ", in a git worktree"
    } else {
        ""
    };
    format!(
        "# Handover brief {id}\n\n\
         This desk was handed over from {from} to {to} by Office Building.\n\n\
         - When: {when}\n\
         - Asked by: {asked}\n\
         - Why: {from}'s subscription allowance is used up ({limit}). {reset}\n\
         - The desk: {title}\n\
         - Folder: {folder}\n\
         - Floor: {floor}\n\
         - Branch: {branch}{worktree}\n\
         - Ended with the old session: {ended}\n\n\
         ## State of the folder\n\n\
         {git}\n\n\
         ## Conversation so far\n\n\
         Oldest first. Hidden reasoning is left out, tool output is cut short and file changes \
         are not repeated here: the files in the folder are the truth.\n",
        id = facts.id,
        to = facts.to.label(),
        when = time(facts.at),
        title = known(facts.title),
        folder = known(&facts.folder.display().to_string()),
        floor = known(facts.floor),
        ended = ended(facts.work),
        git = facts.git.trim(),
    )
}

/// `text` cut to `max` characters on one line.
fn cut_line(raw: &str, max: usize) -> String {
    head(&tidy_line(raw), max).0.trim_end().to_owned()
}

fn status(status: ToolStatus) -> &'static str {
    match status {
        ToolStatus::Pending => "pending",
        ToolStatus::Running => "running",
        ToolStatus::Done => "done",
        ToolStatus::Failed => "failed",
    }
}

/// One entry as the brief writes it. None: nothing of it is shown.
fn entry(entry: &Entry, from: Tool) -> Option<String> {
    let said = |who: &str, raw: &str| {
        let text = tidy(raw);
        (!text.is_empty()).then(|| format!("{who}:\n{text}"))
    };
    match entry {
        Entry::User { text, .. } => said("A Human", text),
        Entry::Agent { text, .. } => said(from.label(), text),
        // Hidden reasoning never leaves the building, in any form.
        Entry::Thought { .. } => None,
        // The diffs are left out on purpose: the files in the folder are the truth.
        Entry::Tool {
            title,
            status: state,
            detail,
            ..
        } => {
            let title = cut_line(title, TOOL_LINE_MAX);
            let output = tidy(detail);
            if title.is_empty() && output.is_empty() {
                return None;
            }
            let mut text = format!("Tool: {title} ({})", status(*state));
            if !output.is_empty() {
                let (kept, more) = head(&output, RESULT_MAX);
                text.push('\n');
                text.push_str(kept.trim_end());
                if more > 0 {
                    text = format!("{text}\n… cut: {more} more characters");
                }
            }
            Some(text)
        }
        Entry::Notice { text, .. } => {
            let text = cut_line(text, NOTICE_MAX);
            (!text.is_empty()).then(|| format!("Notice: {text}"))
        }
    }
}

/// The brief's text: the facts, then as much of the conversation as fits, counted from the end.
#[must_use]
pub fn render(facts: &Facts<'_>, entries: &[Entry]) -> String {
    let shown: Vec<String> = entries
        .iter()
        .filter_map(|e| entry(e, facts.from))
        .collect();
    let mut taken: Vec<&str> = Vec::new();
    let mut size = 0;
    for text in shown.iter().rev() {
        let cost = text.chars().count() + GAP.len();
        // The newest is always taken, whatever its size.
        if !taken.is_empty() && size + cost > CONVERSATION_MAX {
            break;
        }
        size += cost;
        taken.push(text);
    }
    taken.reverse();
    let mut out = header(facts);
    if facts.earlier || taken.len() < shown.len() {
        out.push_str(EARLIER);
        out.push('\n');
    }
    if !taken.is_empty() {
        out.push('\n');
        out.push_str(&taken.join(GAP));
        out.push('\n');
    }
    out
}

/// The number `n` of a brief named `h-<n>.md`.
fn number(name: &str) -> Option<u64> {
    name.strip_prefix("h-")?.strip_suffix(".md")?.parse().ok()
}

/// The highest brief number in `dir`, 0 when there is none.
#[must_use]
pub fn highest(dir: &Path) -> u64 {
    fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| number(e.file_name().to_str()?))
        .max()
        .unwrap_or(0)
}

/// Remove all but the `KEEP` newest briefs in `dir`, by number, never `just_written`.
/// Best effort.
fn keep_newest(dir: &Path, just_written: &Path) {
    let Ok(read) = fs::read_dir(dir) else {
        return;
    };
    let mut briefs: Vec<(u64, PathBuf)> = read
        .flatten()
        .filter_map(|e| Some((number(e.file_name().to_str()?)?, e.path())))
        .collect();
    briefs.sort_by_key(|(n, _)| std::cmp::Reverse(*n));
    for (_, path) in briefs.into_iter().skip(KEEP) {
        if path != just_written {
            remove(&path);
        }
    }
}

/// Write `text` as `<dir>/<id>.md`, readable by the owner only. Err: the reason in words.
pub fn write(dir: &Path, id: &str, text: &str, desk_folder: &Path) -> Result<PathBuf, String> {
    DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
        .map_err(|e| format!("the briefs folder could not be made: {e}"))?;
    let meta = fs::symlink_metadata(dir)
        .map_err(|e| format!("the briefs folder could not be read: {e}"))?;
    if !meta.file_type().is_dir() {
        return Err("the briefs folder is a link, so no brief is written there".to_owned());
    }
    fs::set_permissions(dir, Permissions::from_mode(0o700))
        .map_err(|e| format!("the briefs folder could not be made private: {e}"))?;
    let canonical =
        fs::canonicalize(dir).map_err(|e| format!("the briefs folder could not be read: {e}"))?;
    let desk = fs::canonicalize(desk_folder)
        .map_err(|e| format!("the desk's folder could not be read: {e}"))?;
    if canonical.starts_with(&desk) {
        return Err(
            "the building's data folder is inside this desk's folder, so no brief is written there"
                .to_owned(),
        );
    }
    let path = dir.join(format!("{id}.md"));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&path)
        .map_err(|e| format!("the brief {id}.md could not be written: {e}"))?;
    file.set_permissions(Permissions::from_mode(0o600))
        .and_then(|()| file.write_all(text.as_bytes()))
        .map_err(|e| format!("the brief {id}.md could not be written: {e}"))?;
    keep_newest(dir, &path);
    Ok(path)
}

/// Remove a brief. Best effort.
pub fn remove(path: &Path) {
    // A brief that is already gone, or cannot be removed, is left as it is.
    let _ = fs::remove_file(path);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structured::FileDiff;
    use std::os::unix::fs::symlink;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("ob-handover-brief-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch");
        dir
    }

    const CLEAR: Work = Work::Clear;

    fn facts(work: &Work) -> Facts<'_> {
        Facts {
            id: "h-12",
            from: Tool::ClaudeCode,
            to: Tool::Codex,
            by_human: false,
            at: 1_794_567_190,
            window: Some(Window::FiveHour),
            reset_at: Some(1_794_574_385),
            title: "Fix the login form",
            folder: Path::new("/work/widgets"),
            floor: "widgets",
            branch: Some("main"),
            worktree: false,
            work,
            git: "$ git status --porcelain=v1 --branch\n## main",
            earlier: false,
        }
    }

    fn user(id: &str, text: &str) -> Entry {
        Entry::User {
            id: id.into(),
            text: text.into(),
        }
    }

    fn tool(title: &str, detail: &str, diffs: Vec<FileDiff>) -> Entry {
        Entry::Tool {
            id: "t".into(),
            title: title.into(),
            status: ToolStatus::Done,
            detail: detail.into(),
            diffs,
        }
    }

    fn conversation(text: &str) -> &str {
        text.split_once("are not repeated here: the files in the folder are the truth.\n")
            .expect("conversation heading")
            .1
    }

    #[test]
    fn the_brief_reads_as_written() {
        let entries = [
            user("u", "Please fix it"),
            Entry::Agent {
                id: "a".into(),
                text: "Looking now".into(),
            },
            tool("Bash: ls", "", Vec::new()),
            Entry::Notice {
                id: "n".into(),
                text: "Context compacted".into(),
            },
        ];
        let got = render(&facts(&CLEAR), &entries);
        assert_eq!(
            got,
            "# Handover brief h-12\n\n\
             This desk was handed over from Claude Code to Codex by Office Building.\n\n\
             - When: 2026-11-13T10:53:10Z\n\
             - Asked by: the building itself\n\
             - Why: Claude Code's subscription allowance is used up (the five-hour limit). \
             It resets at 2026-11-13T12:53:05Z.\n\
             - The desk: Fix the login form\n\
             - Folder: /work/widgets\n\
             - Floor: widgets\n\
             - Branch: main\n\
             - Ended with the old session: nothing the building knows of\n\n\
             ## State of the folder\n\n\
             $ git status --porcelain=v1 --branch\n## main\n\n\
             ## Conversation so far\n\n\
             Oldest first. Hidden reasoning is left out, tool output is cut short and file \
             changes are not repeated here: the files in the folder are the truth.\n\n\
             A Human:\nPlease fix it\n\n\
             Claude Code:\nLooking now\n\n\
             Tool: Bash: ls (done)\n\n\
             Notice: Context compacted\n"
        );
    }

    #[test]
    fn hidden_reasoning_appears_nowhere() {
        let entries = [
            user("u", "hi"),
            Entry::Thought {
                id: "th".into(),
                text: "secret plan zebra".into(),
            },
        ];
        let got = render(&facts(&CLEAR), &entries);
        assert!(!got.contains("zebra"), "{got}");
    }

    #[test]
    fn the_cap_counts_from_the_end() {
        let entries: Vec<Entry> = (0..300)
            .map(|n| {
                user(
                    &n.to_string(),
                    &format!("message-{n:03} {}", "x".repeat(1000)),
                )
            })
            .collect();
        let got = render(&facts(&CLEAR), &entries);
        assert!(got.contains("message-299"), "the newest is there");
        assert!(!got.contains("message-000"), "the oldest is not");
        assert!(got.contains(EARLIER));
        let part = conversation(&got)
            .split_once(&format!("{EARLIER}\n"))
            .expect("earlier line")
            .1;
        assert!(part.trim().chars().count() <= CONVERSATION_MAX);
    }

    #[test]
    fn the_newest_entry_is_taken_even_when_it_alone_is_over_the_cap() {
        let entries = [
            user("old", "first-of-all"),
            user("new", &"y".repeat(CONVERSATION_MAX + 10)),
        ];
        let got = render(&facts(&CLEAR), &entries);
        assert!(got.contains(&"y".repeat(CONVERSATION_MAX)));
        assert!(!got.contains("first-of-all"));
        assert!(got.contains(EARLIER));
    }

    #[test]
    fn earlier_alone_gives_the_line() {
        let entries = [user("u", "hi")];
        assert!(!render(&facts(&CLEAR), &entries).contains(EARLIER));
        let mut with = facts(&CLEAR);
        with.earlier = true;
        assert!(render(&with, &entries).contains(EARLIER));
    }

    #[test]
    fn a_long_tool_result_is_cut() {
        let got = render(
            &facts(&CLEAR),
            &[tool("Bash: build", &"z".repeat(5000), Vec::new())],
        );
        assert!(got.contains(&format!(
            "Tool: Bash: build (done)\n{}\n… cut: 3500 more characters",
            "z".repeat(RESULT_MAX)
        )));
        assert!(!got.contains(&"z".repeat(RESULT_MAX + 1)));
    }

    #[test]
    fn a_tool_title_is_one_short_line_and_diffs_are_not_written() {
        let title = format!("Edit:\nsrc/a.rs\r\n{}", "w".repeat(400));
        let diffs = vec![FileDiff {
            path: "src/a.rs".into(),
            text: "+added giraffe line".into(),
        }];
        let got = render(&facts(&CLEAR), &[tool(&title, "", diffs)]);
        let line = got
            .lines()
            .find(|l| l.starts_with("Tool: "))
            .expect("tool line");
        assert_eq!(
            line,
            format!(
                "Tool: Edit: src/a.rs {} (done)",
                "w".repeat(TOOL_LINE_MAX - 15)
            )
        );
        assert!(!got.contains("giraffe"), "{got}");
    }

    #[test]
    fn escape_sequences_and_control_characters_are_gone() {
        let entries = [
            user(
                "u",
                "red \x1b[31mtext\x1b[0m\x07 and \x1b]0;title\x07 done\u{202e}",
            ),
            tool("Bash: \x1b[1mls\x1b[0m", "out\x08put\x1b[2J", Vec::new()),
        ];
        let mut with = facts(&CLEAR);
        with.title = "Fix \x1b[31mit\x1b[0m";
        let got = render(&with, &entries);
        assert!(!got.contains('\x1b'), "{got:?}");
        assert!(!got.chars().any(|c| c.is_control() && c != '\n'), "{got:?}");
        assert!(!got.contains('\u{202e}'));
        assert!(got.contains("red text and  done") || got.contains("red text and done"));
        assert!(got.contains("- The desk: Fix it\n"));
        assert!(got.contains("Tool: Bash: ls (done)\noutput"));
    }

    #[test]
    fn empty_entries_are_left_out() {
        let entries = [
            user("u", "\x1b[31m\x1b[0m"),
            Entry::Notice {
                id: "n".into(),
                text: " \n ".into(),
            },
            tool("", "", Vec::new()),
        ];
        let got = render(&facts(&CLEAR), &entries);
        assert!(
            got.ends_with("the files in the folder are the truth.\n"),
            "{got}"
        );
    }

    #[test]
    fn the_header_rows() {
        let running = Work::Running {
            tasks: vec![
                Task {
                    kind: TaskKind::Workflow,
                    name: Some("release".into()),
                },
                Task {
                    kind: TaskKind::Shell,
                    name: None,
                },
            ],
        };
        let unknown = Work::Unknown;
        let row = |f: &Facts<'_>, start: &str| -> String {
            render(f, &[])
                .lines()
                .find(|l| l.starts_with(start))
                .expect(start)
                .to_owned()
        };
        let mut by_human = facts(&CLEAR);
        by_human.by_human = true;
        assert_eq!(
            row(&facts(&CLEAR), "- Asked by"),
            "- Asked by: the building itself"
        );
        assert_eq!(row(&by_human, "- Asked by"), "- Asked by: A Human");

        let mut weekly = facts(&CLEAR);
        weekly.window = Some(Window::SevenDay);
        weekly.reset_at = None;
        assert_eq!(
            row(&weekly, "- Why"),
            "- Why: Claude Code's subscription allowance is used up (the weekly limit). \
             The reset time is not known."
        );
        let mut neither = facts(&CLEAR);
        neither.window = None;
        assert_eq!(
            row(&neither, "- Why"),
            "- Why: Claude Code's subscription allowance is used up (which limit is not known). \
             It resets at 2026-11-13T12:53:05Z."
        );

        let ended = "- Ended with the old session";
        assert_eq!(
            row(&facts(&CLEAR), ended),
            format!("{ended}: nothing the building knows of")
        );
        assert_eq!(
            row(&facts(&unknown), ended),
            format!("{ended}: the building could not tell what was running")
        );
        assert_eq!(
            row(&facts(&running), ended),
            format!("{ended}: a workflow \"release\", a background shell")
        );

        let mut tree = facts(&CLEAR);
        tree.branch = None;
        tree.worktree = true;
        assert_eq!(
            row(&tree, "- Branch"),
            "- Branch: not known, in a git worktree"
        );
    }

    #[test]
    fn a_brief_is_owner_only_and_outside_the_desk() {
        let root = scratch("modes");
        let desk = root.join("desk");
        fs::create_dir_all(&desk).expect("desk");
        let dir = root.join("data").join("handover").join("briefs");
        let path = write(&dir, "h-1", "the brief", &desk).expect("write");
        assert_eq!(path, dir.join("h-1.md"));
        assert!(!path.starts_with(&desk));
        assert_eq!(fs::read_to_string(&path).expect("read"), "the brief");
        let mode = |p: &Path| fs::metadata(p).expect("meta").permissions().mode() & 0o777;
        assert_eq!(mode(&path), 0o600);
        assert_eq!(mode(&dir), 0o700);
        assert_eq!(
            write(&dir, "h-1", "again", &desk),
            Err(format!(
                "the brief h-1.md could not be written: {}",
                std::io::Error::from_raw_os_error(libc::EEXIST)
            ))
        );
        assert_eq!(fs::read_to_string(&path).expect("read"), "the brief");
        remove(&path);
        assert!(!path.exists());
        remove(&path);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn a_folder_inside_the_desk_is_refused() {
        let root = scratch("inside");
        let dir = root.join("data").join("briefs");
        assert_eq!(
            write(&dir, "h-1", "x", &root),
            Err(
                "the building's data folder is inside this desk's folder, so no brief is \
                 written there"
                    .to_owned()
            )
        );
        assert!(!dir.join("h-1.md").exists());
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn a_linked_folder_is_refused() {
        let root = scratch("link");
        let desk = root.join("desk");
        let real = root.join("real");
        fs::create_dir_all(&desk).expect("desk");
        fs::create_dir_all(&real).expect("real");
        let dir = root.join("briefs");
        symlink(&real, &dir).expect("link");
        assert_eq!(
            write(&dir, "h-1", "x", &desk),
            Err("the briefs folder is a link, so no brief is written there".to_owned())
        );
        assert!(!real.join("h-1.md").exists());
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn a_linked_file_is_not_written_through() {
        let root = scratch("file-link");
        let desk = root.join("desk");
        fs::create_dir_all(&desk).expect("desk");
        let dir = root.join("briefs");
        fs::create_dir_all(&dir).expect("dir");
        let target = root.join("target");
        fs::write(&target, "keep").expect("target");
        symlink(&target, dir.join("h-1.md")).expect("link");
        assert!(write(&dir, "h-1", "x", &desk).is_err());
        assert_eq!(fs::read_to_string(&target).expect("target"), "keep");
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn the_ten_newest_by_number_are_kept() {
        let root = scratch("keep");
        let desk = root.join("desk");
        fs::create_dir_all(&desk).expect("desk");
        let dir = root.join("briefs");
        for n in 1..=12 {
            write(&dir, &format!("h-{n}"), "x", &desk).expect("write");
        }
        let mut left: Vec<String> = fs::read_dir(&dir)
            .expect("read")
            .flatten()
            .filter_map(|e| e.file_name().to_str().map(str::to_owned))
            .collect();
        left.sort_by_key(|name| number(name));
        let want: Vec<String> = (3..=12).map(|n| format!("h-{n}.md")).collect();
        assert_eq!(left, want);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn the_brief_just_written_survives_the_cleanup_and_ids_start_above_the_disk() {
        let dir = scratch("collide");
        let (briefs, desk) = (dir.join("briefs"), dir.join("desk"));
        fs::create_dir_all(&desk).expect("desk");
        for n in 40..=50 {
            write(&briefs, &format!("h-{n}"), "x", &desk).expect("old");
        }
        assert_eq!(highest(&briefs), 50);
        let path = write(&briefs, "h-1", "new", &desk).expect("written");
        assert!(path.exists());
    }
}
