//! Structured desks: agents the building drives over a JSON-RPC protocol on
//! stdio instead of a terminal, so their messages, tool calls and approval
//! requests arrive typed. Codex speaks its app-server protocol (`codex.rs`);
//! opencode and Cursor speak the Agent Client Protocol (`acp.rs`).
//!
//! A driver owns the child process. It reports everything through a `Sink`
//! and takes the human's prompts and approval answers through `Driver`.

mod acp;
mod codex;
pub mod desks;
pub mod difftext;

use crate::model::{SessionState, ToolKind};
use crate::setup::Launch;
use serde::Serialize;
use std::path::Path;
use std::sync::Arc;

/// How far a tool call has got.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ToolStatus {
    Pending,
    Running,
    Done,
    Failed,
}

/// One file's change inside a tool call.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileDiff {
    pub path: String,
    /// Diff lines as plain text: `+` added, `-` removed, a space for context, `@@` for a hunk header.
    pub text: String,
}

/// One line of a structured desk's transcript. A later entry with the same
/// `id` replaces the earlier one (streamed text grows, a tool call finishes).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Entry {
    /// What the human sent.
    User { id: String, text: String },
    /// What the agent said (Markdown as the agent wrote it).
    Agent { id: String, text: String },
    /// The agent's reasoning summary, when the protocol shares one.
    Thought { id: String, text: String },
    /// A tool call: `title` names it (a command, a file), `detail` holds what it
    /// printed and `diffs` the files it changed.
    Tool {
        id: String,
        title: String,
        status: ToolStatus,
        detail: String,
        diffs: Vec<FileDiff>,
    },
    /// Something the building or the protocol wants the human to know (an error, a plan).
    Notice { id: String, text: String },
}

impl Entry {
    #[must_use]
    pub fn id(&self) -> &str {
        match self {
            Self::User { id, .. }
            | Self::Agent { id, .. }
            | Self::Thought { id, .. }
            | Self::Tool { id, .. }
            | Self::Notice { id, .. } => id,
        }
    }
}

/// The most characters of the text of a user, agent, thought or notice entry.
pub const TEXT_MAX: usize = 40_000;
/// The most characters of a tool entry's output.
pub const OUTPUT_MAX: usize = 20_000;
/// The most characters of one file's diff.
pub const DIFF_MAX: usize = 20_000;
/// The most files of one tool entry.
pub const DIFFS_MAX: usize = 20;
/// The most characters of a tool entry's title, and of a diff's path.
pub const TITLE_MAX: usize = 300;
/// The most entries of a backlog.
pub const BACKLOG_MAX: usize = 300;
/// The most characters of text in a backlog.
pub const BACKLOG_CHARS: usize = 600_000;
/// The line that ends a text cut at its cap.
const CUT_MARKER: &str = "… cut here: the rest is not kept";

/// `text` cut to `max` characters and the marker line, if it is longer.
fn cut(text: String, max: usize) -> String {
    // Bytes first: text no longer than the cap in bytes cannot be over it in characters.
    if text.len() <= max {
        return text;
    }
    match text.char_indices().nth(max) {
        Some((end, _)) => format!("{}\n{CUT_MARKER}", &text[..end]),
        None => text,
    }
}

/// `entry` cut to the caps, so no desk keeps or sends more than they allow.
#[must_use]
pub fn capped(entry: Entry) -> Entry {
    match entry {
        Entry::User { id, text } => Entry::User {
            id,
            text: cut(text, TEXT_MAX),
        },
        Entry::Agent { id, text } => Entry::Agent {
            id,
            text: cut(text, TEXT_MAX),
        },
        Entry::Thought { id, text } => Entry::Thought {
            id,
            text: cut(text, TEXT_MAX),
        },
        Entry::Notice { id, text } => Entry::Notice {
            id,
            text: cut(text, TEXT_MAX),
        },
        Entry::Tool {
            id,
            title,
            status,
            detail,
            mut diffs,
        } => {
            diffs.truncate(DIFFS_MAX);
            Entry::Tool {
                id,
                title: cut(title, TITLE_MAX),
                status,
                detail: cut(detail, OUTPUT_MAX),
                diffs: diffs
                    .into_iter()
                    .map(|d| FileDiff {
                        path: cut(d.path, TITLE_MAX),
                        text: cut(d.text, DIFF_MAX),
                    })
                    .collect(),
            }
        }
    }
}

/// The characters of text an entry holds.
fn weight(entry: &Entry) -> usize {
    let count = |t: &str| t.chars().count();
    match entry {
        Entry::User { text, .. }
        | Entry::Agent { text, .. }
        | Entry::Thought { text, .. }
        | Entry::Notice { text, .. } => count(text),
        Entry::Tool {
            title,
            detail,
            diffs,
            ..
        } => {
            count(title)
                + count(detail)
                + diffs
                    .iter()
                    .map(|d| count(&d.path) + count(&d.text))
                    .sum::<usize>()
        }
    }
}

/// The newest entries that fit `BACKLOG_MAX` and `BACKLOG_CHARS` (always at least
/// one), oldest first, and whether older ones were left out.
#[must_use]
pub fn window(entries: &[Entry]) -> (Vec<Entry>, bool) {
    let mut used = 0;
    let mut kept = 0;
    for entry in entries.iter().rev().take(BACKLOG_MAX) {
        used += weight(entry);
        if kept > 0 && used > BACKLOG_CHARS {
            break;
        }
        kept += 1;
    }
    (
        entries[entries.len() - kept..].to_vec(),
        kept < entries.len(),
    )
}

/// One answer the human can give to an approval request.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Choice {
    pub id: String,
    pub label: String,
    /// Whether this answer lets the agent go ahead.
    pub allow: bool,
}

/// What a field of an approval takes.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum FieldKind {
    Text,
    /// Text the panel hides as it is typed (a password, a token).
    Secret,
    Number,
    /// `true` or `false`.
    Boolean,
    /// One of `options`, or with `other` any text.
    Pick {
        options: Vec<String>,
        other: bool,
    },
}

/// Something the human fills in before answering: a question's answer, a form field.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    pub id: String,
    pub label: String,
    pub kind: FieldKind,
    pub required: bool,
}

/// The agent asks before doing something (run a command, change files, use a
/// tool), or asks a question. A choice that lets it go ahead (`allow`) sends
/// the `fields`' values with it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Approval {
    pub id: String,
    pub title: String,
    pub detail: String,
    pub fields: Vec<Field>,
    pub choices: Vec<Choice>,
}

/// The human's values for an approval's fields, by field id, as typed.
pub type Values = std::collections::BTreeMap<String, String>;

/// What a driver reports, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// Add an entry, or replace the one with the same id.
    Entry(Entry),
    /// The desk's state: thinking while a turn runs, idle when it ends, waiting
    /// while an approval is open.
    State(SessionState),
    /// The tool's own id for this conversation (Codex thread, ACP session), as
    /// its session files name it.
    SessionId(String),
    /// An approval request; the turn waits until `Driver::answer`.
    Approval(Approval),
    /// An approval was settled (answered, or withdrawn by the agent).
    Resolved { approval_id: String },
    /// The agent process is gone. `message` says why, when it is not a clean exit.
    Exited {
        code: Option<i32>,
        message: Option<String>,
    },
}

pub type Sink = Arc<dyn Fn(Event) + Send + Sync>;

/// Pushed to the webview after every change to a structured desk (`desks::Update`).
pub const EVENT: &str = "office://structured";

/// What to start: the tool, its folder, model and effort as the hire picked
/// them (`None`: the tool's default), the setup it is launched with, the first
/// task (empty: wait for the human), and the tool's own id of a conversation to
/// reopen instead of starting a new one.
pub struct Spawn<'a> {
    pub tool: ToolKind,
    pub cwd: &'a Path,
    pub model: Option<&'a str>,
    pub effort: Option<&'a str>,
    pub launch: &'a Launch,
    pub task: &'a str,
    pub resume: Option<&'a str>,
}

/// A running structured agent.
pub trait Driver: Send + Sync {
    /// Send the human's message as the next turn.
    fn prompt(&self, text: &str) -> Result<(), String>;
    /// Answer an open approval with one of its choices, and the fields' values.
    fn answer(&self, approval_id: &str, choice_id: &str, values: &Values) -> Result<(), String>;
    /// Stop the turn that is running, if any.
    fn cancel(&self) -> Result<(), String>;
    /// End the agent process.
    fn stop(&self);
}

/// Where an open request is answered.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ApprovalMode {
    /// In the panel, with the provider's own choices.
    Inline,
    /// In the desk's terminal: the panel shows a card that flips to it.
    Terminal,
    /// Where the session runs, outside the building.
    None,
}

/// What a conversation's panel may offer. The only way providers differ in the interface.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each flag is its own wire field the panel reads"
)]
pub struct Capabilities {
    /// A Human can send it messages from the panel.
    pub compose: bool,
    /// Stop interrupts the running turn.
    pub stop: bool,
    /// The panel can end the agent.
    pub end: bool,
    pub approvals: ApprovalMode,
    /// The desk has a terminal of the app's the panel can flip to.
    pub terminal: bool,
}

impl Capabilities {
    pub const STRUCTURED: Self = Self {
        compose: true,
        stop: true,
        end: true,
        approvals: ApprovalMode::Inline,
        terminal: false,
    };
    pub const CLAUDE_TERMINAL: Self = Self {
        compose: true,
        stop: true,
        end: false,
        approvals: ApprovalMode::Terminal,
        terminal: true,
    };
    pub const READ_ONLY: Self = Self {
        compose: false,
        stop: false,
        end: false,
        approvals: ApprovalMode::None,
        terminal: false,
    };
}

/// What a structured desk of `tool` can do: each driver's row of the capability matrix.
#[must_use]
pub fn capabilities(tool: ToolKind) -> Capabilities {
    if supports(tool) {
        Capabilities::STRUCTURED
    } else {
        Capabilities::READ_ONLY
    }
}

/// Whether `tool` can run as a structured desk.
#[must_use]
pub fn supports(tool: ToolKind) -> bool {
    matches!(
        tool,
        ToolKind::Codex | ToolKind::Opencode | ToolKind::Cursor
    )
}

/// Start a structured agent. Events reach `sink` on the driver's own threads.
pub fn start(spawn: &Spawn<'_>, sink: Sink) -> Result<Box<dyn Driver>, String> {
    match spawn.tool {
        ToolKind::Codex => codex::start(spawn, sink),
        ToolKind::Opencode | ToolKind::Cursor => acp::start(spawn, sink),
        _ => Err("that tool can only run in a terminal".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MARKER: &str = "\n… cut here: the rest is not kept";

    fn agent(id: &str, text: String) -> Entry {
        Entry::Agent {
            id: id.to_string(),
            text,
        }
    }

    fn tool(detail: String, diffs: Vec<FileDiff>) -> Entry {
        Entry::Tool {
            id: "t".to_string(),
            title: "title".to_string(),
            status: ToolStatus::Done,
            detail,
            diffs,
        }
    }

    fn diff(path: String, text: String) -> FileDiff {
        FileDiff { path, text }
    }

    #[test]
    fn the_capability_matrix_has_a_row_per_tool_and_three_constants() {
        for tool in [ToolKind::Codex, ToolKind::Opencode, ToolKind::Cursor] {
            assert_eq!(capabilities(tool), Capabilities::STRUCTURED);
        }
        for tool in [
            ToolKind::ClaudeCode,
            ToolKind::Antigravity,
            ToolKind::Unknown,
        ] {
            assert_eq!(capabilities(tool), Capabilities::READ_ONLY);
        }
        let flags = |c: Capabilities| (c.compose, c.stop, c.end, c.approvals, c.terminal);
        assert_eq!(
            flags(Capabilities::STRUCTURED),
            (true, true, true, ApprovalMode::Inline, false)
        );
        assert_eq!(
            flags(Capabilities::CLAUDE_TERMINAL),
            (true, true, false, ApprovalMode::Terminal, true)
        );
        assert_eq!(
            flags(Capabilities::READ_ONLY),
            (false, false, false, ApprovalMode::None, false)
        );
    }

    #[test]
    fn capped_leaves_short_entries_alone() {
        let entry = tool("out".to_string(), vec![diff("a".into(), "+b".into())]);
        assert_eq!(capped(entry.clone()), entry);
        let said = agent("m", "é".repeat(TEXT_MAX));
        assert_eq!(capped(said.clone()), said);
    }

    #[test]
    fn capped_cuts_text_detail_diff_and_path_at_their_caps() {
        let Entry::Agent { text, .. } = capped(agent("m", "a".repeat(50_000))) else {
            panic!("not an agent entry");
        };
        assert_eq!(text, format!("{}{MARKER}", "a".repeat(TEXT_MAX)));
        let long = |n: usize| "z".repeat(n + 1);
        let cut = |n: usize| format!("{}{MARKER}", "z".repeat(n));
        let Entry::Tool { detail, diffs, .. } = capped(tool(
            long(OUTPUT_MAX),
            vec![diff(long(TITLE_MAX), long(DIFF_MAX))],
        )) else {
            panic!("not a tool entry");
        };
        assert_eq!(detail, cut(OUTPUT_MAX));
        assert_eq!(diffs, vec![diff(cut(TITLE_MAX), cut(DIFF_MAX))]);
    }

    #[test]
    fn capped_keeps_at_most_the_most_files_and_cuts_a_title() {
        let many = (0..DIFFS_MAX + 3)
            .map(|i| diff(format!("f{i}"), String::new()))
            .collect();
        let Entry::Tool { diffs, .. } = capped(tool(String::new(), many)) else {
            panic!("not a tool entry");
        };
        assert_eq!(diffs.len(), DIFFS_MAX);
        let Entry::Tool { title, .. } = capped(Entry::Tool {
            id: "t".into(),
            title: "t".repeat(TITLE_MAX + 1),
            status: ToolStatus::Done,
            detail: String::new(),
            diffs: Vec::new(),
        }) else {
            panic!("not a tool entry");
        };
        assert_eq!(title, format!("{}{MARKER}", "t".repeat(TITLE_MAX)));
    }

    fn numbered(n: usize) -> Vec<Entry> {
        (0..n).map(|i| agent(&i.to_string(), "x".into())).collect()
    }

    #[test]
    fn window_returns_a_short_list_whole() {
        let all = numbered(5);
        assert_eq!(window(&all), (all, false));
        assert_eq!(window(&[]), (Vec::new(), false));
    }

    #[test]
    fn window_keeps_the_newest_entries_up_to_the_entry_cap() {
        let all = numbered(BACKLOG_MAX + 1);
        let (kept, earlier) = window(&all);
        assert!(earlier);
        assert_eq!(kept.len(), BACKLOG_MAX);
        assert_eq!(kept.first(), all.get(1));
        assert_eq!(kept.last(), all.last());
    }

    #[test]
    fn window_stops_at_the_character_budget_and_always_keeps_the_newest() {
        let third = BACKLOG_CHARS / 3 + 1;
        let all = vec![
            agent("a", "x".repeat(third)),
            agent("b", "x".repeat(third)),
            agent("c", "x".repeat(third)),
        ];
        let (kept, earlier) = window(&all);
        assert_eq!(kept, all[1..]);
        assert!(earlier);
        let huge = vec![
            agent("a", "x".into()),
            agent("b", "x".repeat(BACKLOG_CHARS + 1)),
        ];
        let (kept, earlier) = window(&huge);
        assert_eq!(kept, huge[1..]);
        assert!(earlier);
    }
}
