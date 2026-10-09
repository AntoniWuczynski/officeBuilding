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
    /// A tool call: `title` names it (a command, a file), `detail` holds output or a diff.
    Tool {
        id: String,
        title: String,
        status: ToolStatus,
        detail: String,
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
