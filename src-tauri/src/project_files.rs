//! Decisions and TODOs read from each repo's own files, following the
//! conventions already in use:
//!
//! * `FOUNDER_DECISIONS.md` / `HUMAN_DECISIONS.md`: one `## ID — Title` section per
//!   call. No status on the heading means it is open; `— DECIDED <date>: …`,
//!   `CONFIRMED`, `VETOED`, `DEFAULT APPLIED` and similar mark it settled.
//!   Under a heading that only groups calls, each top-level checklist item is a
//!   call instead: `- [ ] **Title** …` is open, and `- [x]` is ruled, usually
//!   with a `**Ruled <date>:** …` line. A `### ID · Title · …` entry under such
//!   a heading is a call too: open, unless the heading says the group is
//!   answered or decided, or the entry carries a status. Hyphenated names
//!   (`FOUNDER-DECISIONS.md`, `FOUNDER-TODO.md`) are read the same way.
//! * `DECISIONS.md`: a log of calls agents made on their own (all settled).
//! * `FOUNDER_TODO.md` / `HUMAN_TODO.md`: the human's checklist.
//! * `TODO.md`: the agents' checklist.
//!
//! Checklist items are top-level `- [ ]`, `- [x]` or `- [~]` (partly done) lines;
//! indented lines under an item continue it. Answering a decision or ticking an
//! item in the app writes back into the same file.

use crate::model::{AnswerVia, Decision, Party, Question, QuestionOption, TodoItem};
use std::fs;
use std::path::Path;

pub const HUMAN_DECISION_FILES: &[&str] = &[
    "FOUNDER_DECISIONS.md",
    "HUMAN_DECISIONS.md",
    "FOUNDER-DECISIONS.md",
    "HUMAN-DECISIONS.md",
];
pub const AGENT_DECISION_FILES: &[&str] = &["DECISIONS.md"];
pub const HUMAN_TODO_FILES: &[&str] = &[
    "FOUNDER_TODO.md",
    "HUMAN_TODO.md",
    "FOUNDER-TODO.md",
    "HUMAN-TODO.md",
];
pub const AGENT_TODO_FILES: &[&str] = &["TODO.md"];

const SETTLED: &[&str] = &[
    "DECIDED",
    "PART DECIDED",
    "CONFIRMED",
    "VETOED",
    "DECLINED",
    "DEFAULT APPLIED",
    "DONE",
    "SUPERSEDED",
    "WITHDRAWN",
];

/// What one project's files say. `None` means the file does not exist, so the
/// app falls back to its own records for that list.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProjectFiles {
    pub open: Vec<Question>,
    pub decided: Option<Vec<Decision>>,
    pub human_todos: Option<Vec<TodoItem>>,
    pub agent_todos: Option<Vec<TodoItem>>,
}

/// Ids carry where an item lives: `<project>|<file>|<line>` (line is 0-based).
#[must_use]
pub fn item_id(project_id: &str, file: &str, line: usize) -> String {
    format!("{project_id}|{file}|{line}")
}

/// Split an item id back into (file, line).
#[must_use]
pub fn parse_item_id(id: &str) -> Option<(&str, usize)> {
    let mut parts = id.rsplitn(3, '|');
    let line = parts.next()?.parse().ok()?;
    let file = parts.next()?;
    parts.next()?;
    Some((file, line))
}

/// The first of `names` that exists in `root` (exact name, then case-insensitive).
fn find_file(root: &Path, names: &[&str]) -> Option<String> {
    let entries: Vec<String> = fs::read_dir(root)
        .ok()?
        .filter_map(Result::ok)
        .filter(|e| e.path().is_file())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    names.iter().find_map(|want| {
        entries
            .iter()
            .find(|have| have.as_str() == *want)
            .or_else(|| entries.iter().find(|have| have.eq_ignore_ascii_case(want)))
            .cloned()
    })
}

fn clean(text: &str) -> String {
    text.replace("**", "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let cut: String = text.chars().take(max - 1).collect();
    format!("{}…", cut.trim_end())
}

/// Top-level checklist items. `[x]` is done; `[ ]` and `[~]` are open.
pub fn parse_checklist(text: &str, project_id: &str, file: &str, assignee: Party) -> Vec<TodoItem> {
    let lines: Vec<&str> = text.lines().collect();
    let mut items = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let (done, rest) = if let Some(r) = line.strip_prefix("- [ ]") {
            (false, r)
        } else if let Some(r) = line.strip_prefix("- [~]") {
            (false, r)
        } else if let Some(r) = line
            .strip_prefix("- [x]")
            .or_else(|| line.strip_prefix("- [X]"))
        {
            (true, r)
        } else {
            continue;
        };
        let mut body = rest.trim().to_string();
        for next in lines.iter().skip(i + 1) {
            let continues = next.starts_with(char::is_whitespace)
                && !next.trim().is_empty()
                && !next.trim_start().starts_with("- ");
            if !continues {
                break;
            }
            body.push(' ');
            body.push_str(next.trim());
        }
        items.push(TodoItem {
            id: item_id(project_id, file, i),
            project_id: project_id.to_string(),
            text: truncate(&clean(&body), 300),
            done,
            assignee,
            source_file: Some(file.to_string()),
        });
    }
    items
}

/// A `## heading` split into (id, title, status).
/// An id such as `IMP-030` or `F-10`.
fn is_id(p: &str) -> bool {
    let mut halves = p.splitn(2, '-');
    let (Some(a), Some(b)) = (halves.next(), halves.next()) else {
        return false;
    };
    !a.is_empty()
        && a.chars().all(|c| c.is_ascii_uppercase())
        && !b.is_empty()
        && b.chars().all(|c| c.is_ascii_digit())
}

fn is_status(part: &str) -> bool {
    let up = part.to_uppercase();
    SETTLED.iter().any(|s| up.starts_with(s))
}

/// `F-10 · Title · **urgency: …**`: (id, title, status). Parts after the
/// title are metadata; a status is a `— DECIDED …` part, as `decide` writes.
fn split_entry_heading(heading: &str) -> (Option<String>, String, Option<String>) {
    if !heading.contains(" · ") {
        return split_heading(heading);
    }
    let (main, status) = match heading.split_once(" — ") {
        Some((main, tail)) if is_status(tail.trim()) => (main, Some(tail.trim().to_string())),
        _ => (heading, None),
    };
    let parts: Vec<&str> = main.split(" · ").map(str::trim).collect();
    let (id, title) = match parts.as_slice() {
        [first, title, ..] if is_id(first) => (Some((*first).to_string()), *title),
        [title, ..] => (None, *title),
        [] => (None, ""),
    };
    (id, clean(title), status)
}

fn split_heading(heading: &str) -> (Option<String>, String, Option<String>) {
    let parts: Vec<&str> = heading.split(" — ").map(str::trim).collect();
    let (id, rest) = match parts.split_first() {
        Some((first, rest)) if is_id(first) && !rest.is_empty() => {
            (Some((*first).to_string()), rest.to_vec())
        }
        _ => (None, parts.clone()),
    };
    let status_at = rest.iter().position(|p| is_status(p));
    match status_at {
        Some(i) => (id, rest[..i].join(" — "), Some(rest[i..].join(" — "))),
        None => (id, rest.join(" — "), None),
    }
}

/// `2026-09-06` somewhere in `text`, as an ISO timestamp.
fn find_date(text: &str) -> String {
    let b = text.as_bytes();
    for i in 0..b.len().saturating_sub(9) {
        let w = &b[i..i + 10];
        let digits = |r: std::ops::Range<usize>| w[r].iter().all(u8::is_ascii_digit);
        if digits(0..4) && w[4] == b'-' && digits(5..7) && w[7] == b'-' && digits(8..10) {
            return format!("{}T00:00:00Z", &text[i..i + 10]);
        }
    }
    String::new()
}

/// Options in a decision section: bold-led list items, or inline "(1) … (2) …".
#[must_use]
pub fn parse_options(body: &str) -> Vec<String> {
    let mut options = Vec::new();
    for line in body.lines() {
        let t = line.trim_start();
        let rest = t
            .strip_prefix("- ")
            .or_else(|| t.strip_prefix("* "))
            .or_else(|| {
                let digits = t.chars().take_while(char::is_ascii_digit).count();
                (digits > 0)
                    .then(|| &t[digits..])
                    .and_then(|r| r.strip_prefix(". ").or_else(|| r.strip_prefix(") ")))
            });
        if let Some(label) = rest
            .and_then(|r| r.strip_prefix("**"))
            .and_then(|r| r.split_once("**"))
            .map(|(l, _)| l)
        {
            options.push(truncate(label.trim().trim_end_matches(['.', ':']), 140));
        }
    }
    if !options.is_empty() {
        return options;
    }
    // Table rows led by a short label: `| A | **Pull it** | trade-off |`.
    for line in body.lines() {
        let Some(row) = line
            .trim()
            .strip_prefix('|')
            .and_then(|r| r.strip_suffix('|'))
        else {
            continue;
        };
        let cells: Vec<&str> = row.split('|').map(str::trim).collect();
        if let [label, option, ..] = cells.as_slice()
            && !label.is_empty()
            && label.len() <= 2
            && label.chars().all(|c| c.is_ascii_alphanumeric())
            && !option.is_empty()
        {
            options.push(truncate(&clean(option), 140));
        }
    }
    if !options.is_empty() {
        return options;
    }
    // Inline options only count after the word "Option": lettered parts of the
    // question itself ("keep or drop (a) … and (b) …") are not choices.
    let flat = clean(body);
    let Some(at) = flat.find("Option") else {
        return Vec::new();
    };
    let flat = &flat[at..];
    let numbered = inline_options(flat, |n| format!("({})", n + 1));
    if numbered.len() >= 2 {
        return numbered;
    }
    for first in *b"aA" {
        let lettered = inline_options(flat, |n| {
            let letter = u8::try_from(n).map_or('?', |n| char::from(first.saturating_add(n)));
            format!("({letter})")
        });
        if lettered.len() >= 2 {
            return lettered;
        }
    }
    Vec::new()
}

/// Inline options marked `marker(0)`, `marker(1)`, …: each runs to the next
/// marker, and the last to "Recommendation" or the end of its first sentence.
fn inline_options(flat: &str, marker: impl Fn(usize) -> String) -> Vec<String> {
    let mut options = Vec::new();
    let mut rest = flat;
    for n in 0..26 {
        let Some(start) = rest.find(&marker(n)) else {
            break;
        };
        let after = &rest[start + marker(n).len()..];
        let next = after.find(&marker(n + 1));
        let end = next
            .or_else(|| after.find("Recommendation"))
            .unwrap_or(after.len());
        let mut label = &after[..end];
        if next.is_none()
            && let Some(stop) = label.find(". ")
        {
            label = &label[..stop];
        }
        let label = label.trim().trim_end_matches([',', ';', '.']).trim();
        let label = label
            .strip_suffix(" or")
            .unwrap_or(label)
            .trim_end_matches(',');
        if !label.is_empty() {
            options.push(truncate(label, 140));
        }
        rest = after;
    }
    options
}

struct Section<'a> {
    line: usize,
    heading: &'a str,
    body: String,
}

fn sections(text: &str) -> Vec<Section<'_>> {
    let mut out: Vec<Section<'_>> = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if let Some(h) = line.strip_prefix("## ") {
            out.push(Section {
                line: i,
                heading: h.trim(),
                body: String::new(),
            });
        } else if let Some(last) = out.last_mut() {
            last.body.push_str(line);
            last.body.push('\n');
        }
    }
    out
}

/// Open calls (questions for the human) and settled ones from a founder/human decisions file.
#[must_use]
pub fn parse_human_decisions(
    text: &str,
    project_id: &str,
    file: &str,
) -> (Vec<Question>, Vec<Decision>) {
    let mut open = Vec::new();
    let mut decided = Vec::new();
    for s in sections(text) {
        // Summary and group sections ("Decisions taken 2026-09-04") are neither a call nor a record.
        if !is_call(s.heading) {
            continue;
        }
        let (id, title, status) = split_heading(s.heading);
        let key = item_id(project_id, file, s.line);
        let label = match &id {
            Some(id) => format!("{id}: {title}"),
            None => title.clone(),
        };
        match status {
            None => open.push(Question {
                id: key,
                project_id: project_id.to_string(),
                session_id: String::new(),
                prompt: label,
                options: parse_options(&s.body)
                    .into_iter()
                    .enumerate()
                    .map(|(i, l)| QuestionOption {
                        id: format!("opt-{i}"),
                        label: l,
                    })
                    .collect(),
                allow_other: true,
                answer_via: AnswerVia::File,
                priority: 0,
                asked_at: find_date(&s.body),
                context: truncate(s.body.trim(), 2000),
                source_file: Some(file.to_string()),
            }),
            Some(status) => decided.push(Decision {
                id: key,
                project_id: project_id.to_string(),
                title: label,
                detail: truncate(&clean(&status), 300),
                decided_at: find_date(&status),
                by: if status.to_uppercase().starts_with("DEFAULT APPLIED") {
                    Party::Ai
                } else {
                    Party::Human
                },
            }),
        }
    }
    let (items_open, items_decided) = checklist_decisions(text, project_id, file);
    open.extend(items_open);
    decided.extend(items_decided);
    (open, decided)
}

/// A `## heading` that is itself a call or a record, rather than a group of them.
fn is_call(heading: &str) -> bool {
    let (id, title, status) = split_heading(heading);
    id.is_some() || title.ends_with('?') || status.is_some()
}

/// A checklist item's text from its first line through its indented
/// continuation lines, and the line just past it.
fn item_body(lines: &[&str], start: usize, first: &str) -> (String, usize) {
    let mut body = first.trim().to_string();
    let mut end = start + 1;
    while let Some(next) = lines.get(end) {
        if next.trim().is_empty() || !next.starts_with(char::is_whitespace) {
            break;
        }
        body.push('\n');
        body.push_str(next.trim());
        end += 1;
    }
    (body, end)
}

/// The item's leading `**bold**` title, else its first 140 characters.
fn item_title(body: &str) -> String {
    let flat = body.split_whitespace().collect::<Vec<_>>().join(" ");
    flat.strip_prefix("**")
        .and_then(|r| r.split_once("**"))
        .map_or_else(
            || truncate(&clean(&flat), 140),
            |(t, _)| t.trim().to_string(),
        )
}

/// Top-level checklist items under group headings: `[ ]` and `[~]` are open
/// calls, `[x]` ruled ones.
fn checklist_decisions(text: &str, project_id: &str, file: &str) -> (Vec<Question>, Vec<Decision>) {
    let lines: Vec<&str> = text.lines().collect();
    let mut open = Vec::new();
    let mut decided = Vec::new();
    let mut group = "";
    let mut in_call = false;
    let mut in_entry = false;
    for (i, line) in lines.iter().enumerate() {
        if let Some(h) = line.strip_prefix("## ") {
            group = h;
            in_call = is_call(h.trim());
            in_entry = false;
            continue;
        }
        if in_call {
            continue;
        }
        if let Some(h) = line.strip_prefix("### ") {
            let (id, title, status) = split_entry_heading(h.trim());
            in_entry = id.is_some() || title.ends_with('?') || status.is_some();
            if in_entry {
                let end = lines[i + 1..]
                    .iter()
                    .position(|l| l.starts_with("## ") || l.starts_with("### "))
                    .map_or(lines.len(), |n| i + 1 + n);
                let body = lines[i + 1..end].join("\n");
                let label = match &id {
                    Some(id) => format!("{id}: {title}"),
                    None => title,
                };
                let answered = [
                    "ANSWERED", "DECIDED", "SETTLED", "CLOSED", "RESOLVED", "RULED",
                ]
                .iter()
                .any(|w| group.trim().to_uppercase().starts_with(w));
                let entry = Entry {
                    line: i,
                    label,
                    body: &body,
                    group,
                };
                match status {
                    Some(status) => decided.push(entry.decided(project_id, file, &status)),
                    None if answered => decided.push(entry.decided(project_id, file, &body)),
                    None => open.push(entry.question(project_id, file)),
                }
            }
            continue;
        }
        if in_entry {
            continue;
        }
        let (ruled, first) = if let Some(r) = line
            .strip_prefix("- [ ]")
            .or_else(|| line.strip_prefix("- [~]"))
        {
            (false, r)
        } else if let Some(r) = line
            .strip_prefix("- [x]")
            .or_else(|| line.strip_prefix("- [X]"))
        {
            (true, r)
        } else {
            continue;
        };
        let (body, _) = item_body(&lines, i, first);
        let entry = Entry {
            line: i,
            label: item_title(&body),
            body: &body,
            group,
        };
        if ruled {
            let ruling = body.find("Ruled").map_or(body.as_str(), |at| &body[at..]);
            decided.push(entry.decided(project_id, file, ruling));
        } else {
            open.push(entry.question(project_id, file));
        }
    }
    (open, decided)
}

/// One call found under a group heading: a checklist item or a `###` entry.
struct Entry<'a> {
    line: usize,
    label: String,
    body: &'a str,
    group: &'a str,
}

impl Entry<'_> {
    fn question(self, project_id: &str, file: &str) -> Question {
        let asked = find_date(self.body);
        Question {
            id: item_id(project_id, file, self.line),
            project_id: project_id.to_string(),
            session_id: String::new(),
            prompt: self.label,
            options: parse_options(self.body)
                .into_iter()
                .enumerate()
                .map(|(i, l)| QuestionOption {
                    id: format!("opt-{i}"),
                    label: l,
                })
                .collect(),
            allow_other: true,
            answer_via: AnswerVia::File,
            priority: 0,
            // No date on the call itself: the group heading's date stands in.
            asked_at: if asked.is_empty() {
                find_date(self.group)
            } else {
                asked
            },
            context: truncate(self.body.trim(), 2000),
            source_file: Some(file.to_string()),
        }
    }

    /// `ruling` is the part of the text that says what was decided.
    fn decided(self, project_id: &str, file: &str, ruling: &str) -> Decision {
        Decision {
            id: item_id(project_id, file, self.line),
            project_id: project_id.to_string(),
            title: self.label,
            detail: truncate(&clean(ruling), 300),
            decided_at: find_date(ruling),
            by: Party::Human,
        }
    }
}

/// Calls agents made on their own (`DECISIONS.md`): every section is settled.
#[must_use]
pub fn parse_agent_decisions(text: &str, project_id: &str, file: &str) -> Vec<Decision> {
    sections(text)
        .into_iter()
        .map(|s| {
            let first_paragraph = s
                .body
                .split("\n\n")
                .map(str::trim)
                .find(|p| !p.is_empty() && *p != "---")
                .unwrap_or_default();
            Decision {
                id: item_id(project_id, file, s.line),
                project_id: project_id.to_string(),
                title: clean(s.heading),
                detail: truncate(&clean(first_paragraph), 300),
                decided_at: find_date(s.heading),
                by: Party::Ai,
            }
        })
        .collect()
}

/// Read every recognised file in a project root.
pub fn read(root: &Path, project_id: &str) -> ProjectFiles {
    let text = |name: &str| fs::read_to_string(root.join(name)).ok();
    let mut files = ProjectFiles::default();
    let mut decided: Option<Vec<Decision>> = None;
    if let Some(name) = find_file(root, HUMAN_DECISION_FILES)
        && let Some(t) = text(&name)
    {
        let (open, settled) = parse_human_decisions(&t, project_id, &name);
        files.open = open;
        decided.get_or_insert_with(Vec::new).extend(settled);
    }
    if let Some(name) = find_file(root, AGENT_DECISION_FILES)
        && let Some(t) = text(&name)
    {
        decided
            .get_or_insert_with(Vec::new)
            .extend(parse_agent_decisions(&t, project_id, &name));
    }
    files.decided = decided;
    files.human_todos = find_file(root, HUMAN_TODO_FILES)
        .and_then(|name| text(&name).map(|t| parse_checklist(&t, project_id, &name, Party::Human)));
    files.agent_todos = find_file(root, AGENT_TODO_FILES)
        .and_then(|name| text(&name).map(|t| parse_checklist(&t, project_id, &name, Party::Ai)));
    files
}

fn rewrite_line(
    path: &Path,
    line: usize,
    edit: impl FnOnce(&str) -> Result<String, String>,
) -> Result<(), String> {
    let text =
        fs::read_to_string(path).map_err(|e| format!("could not read {}: {e}", path.display()))?;
    let mut lines: Vec<&str> = text.split('\n').collect();
    let current = *lines
        .get(line)
        .ok_or("the file changed since it was read; try again")?;
    let replaced = edit(current)?;
    lines[line] = &replaced;
    fs::write(path, lines.join("\n"))
        .map_err(|e| format!("could not write {}: {e}", path.display()))
}

/// Tick a checklist item in its file.
pub fn tick(root: &Path, file: &str, line: usize) -> Result<(), String> {
    rewrite_line(&root.join(file), line, |current| {
        for open in ["- [ ]", "- [~]"] {
            if let Some(rest) = current.strip_prefix(open) {
                return Ok(format!("- [x]{rest}"));
            }
        }
        Err(format!("{file} changed since it was read; try again"))
    })
}

/// Record the human's answer to an open call in its file: the heading gets a
/// DECIDED status and the section a line saying what was decided.
pub fn decide(
    root: &Path,
    file: &str,
    line: usize,
    answer: &str,
    date: &str,
) -> Result<(), String> {
    let answer = clean(answer);
    let path = root.join(file);
    let text =
        fs::read_to_string(&path).map_err(|e| format!("could not read {}: {e}", path.display()))?;
    if text
        .split('\n')
        .nth(line)
        .is_some_and(|l| l.starts_with("- ["))
    {
        return decide_item(&path, &text, file, line, &answer, date);
    }
    rewrite_line(&path, line, |current| {
        let (level, heading) = current
            .strip_prefix("### ")
            .map(|h| ("###", h))
            .or_else(|| current.strip_prefix("## ").map(|h| ("##", h)))
            .ok_or_else(|| format!("{file} changed since it was read; try again"))?;
        if split_entry_heading(heading).2.is_some() {
            return Err("that decision is already settled".to_string());
        }
        Ok(format!(
            "{level} {} — DECIDED {date}: {}\n\nDecided in Office Building on {date}: {answer}.",
            heading.trim_end(),
            truncate(&answer, 80)
        ))
    })
}

/// Rule on a checklist call: tick it and add a `**Ruled <date>:**` line after it.
fn decide_item(
    path: &Path,
    text: &str,
    file: &str,
    line: usize,
    answer: &str,
    date: &str,
) -> Result<(), String> {
    let lines: Vec<&str> = text.split('\n').collect();
    let changed = || format!("{file} changed since it was read; try again");
    let current = lines.get(line).ok_or_else(changed)?;
    let Some(rest) = current
        .strip_prefix("- [ ]")
        .or_else(|| current.strip_prefix("- [~]"))
    else {
        return Err(
            if current.starts_with("- [x]") || current.starts_with("- [X]") {
                "that decision is already settled".to_string()
            } else {
                changed()
            },
        );
    };
    let (_, end) = item_body(&lines, line, rest);
    let ticked = format!("- [x]{rest}");
    let ruling = format!(
        "  **Ruled {date}:** {}. Answered in Office Building.",
        answer.trim_end_matches('.')
    );
    let mut out: Vec<&str> = Vec::with_capacity(lines.len() + 1);
    out.extend(&lines[..line]);
    out.push(&ticked);
    out.extend(&lines[line + 1..end]);
    out.push(&ruling);
    out.extend(&lines[end..]);
    fs::write(path, out.join("\n")).map_err(|e| format!("could not write {}: {e}", path.display()))
}

#[cfg(test)]
mod tests;
