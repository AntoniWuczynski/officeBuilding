//! The Tauri-boundary data model: the Rust mirror of `src/types.ts` and the
//! request types of `src/api/DiscoveryApi.ts`. Everything serialises in the
//! shape the frontend expects (camelCase fields, kebab-case enum values).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum ToolKind {
    ClaudeCode,
    Codex,
    Opencode,
    Antigravity,
    Cursor,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SessionState {
    Error,
    WaitingHuman,
    Running,
    Thinking,
    Idle,
    Done,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ControlMode {
    Full,
    RaiseWindow,
    ReadOnly,
}

/// Dollars at API list prices. `unpriced_tokens` (part of `tokens`) came from
/// models with no public price, so their cost is missing from `usd`.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Spend {
    pub usd: f64,
    pub tokens: u64,
    pub unpriced_tokens: u64,
}

impl std::ops::Add for Spend {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        Self {
            usd: self.usd + other.usd,
            tokens: self.tokens + other.tokens,
            unpriced_tokens: self.unpriced_tokens + other.unpriced_tokens,
        }
    }
}

impl std::iter::Sum for Spend {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Self::default(), |a, b| a + b)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    pub tool: ToolKind,
    pub project_id: String,
    pub title: String,
    pub state: SessionState,
    pub control: ControlMode,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub last_activity_at: String,
    pub pending_question_ids: Vec<String>,
    pub spend: Spend,
    /// Sub-agents (plain or in a workflow) still working for this session.
    pub helpers: Vec<Helper>,
}

/// A session whose agent has left: listed on its floor's sign-out sheet to resume.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EndedSession {
    pub id: String,
    pub tool: ToolKind,
    pub project_id: String,
    pub title: String,
    /// Its last activity.
    pub ended_at: String,
    pub model: Option<String>,
    pub effort: Option<String>,
}

/// A sub-agent at work for a session: it gets a tiny desk beside its session's.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Helper {
    pub id: String,
    pub state: SessionState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub path: String,
    pub session_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QuestionOption {
    pub id: String,
    pub label: String,
}

/// Where the human answers a question (see `Question.answerVia` in types.ts).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AnswerVia {
    App,
    Terminal,
    /// An open call in the repo's decisions file: the answer is written back there.
    File,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Question {
    pub id: String,
    pub project_id: String,
    /// The asking session; empty for calls that come from a repo file.
    pub session_id: String,
    pub prompt: String,
    pub options: Vec<QuestionOption>,
    pub allow_other: bool,
    pub answer_via: AnswerVia,
    pub priority: i32,
    pub asked_at: String,
    /// Background for the call (the file section's text), empty for agent questions.
    pub context: String,
    /// The repo file the call lives in, when it comes from one.
    pub source_file: Option<String>,
}

/// Who decided something, or who is expected to tick a TODO off.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Party {
    Ai,
    Human,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Decision {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub detail: String,
    pub decided_at: String,
    pub by: Party,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TodoItem {
    pub id: String,
    pub project_id: String,
    pub text: String,
    pub done: bool,
    pub assignee: Party,
    /// The repo file the item lives in; None for the app's own records.
    pub source_file: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentMessage {
    pub id: String,
    pub from_session_id: String,
    pub to_session_id: String,
    pub text: String,
    pub sent_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OfficeSnapshot {
    pub projects: Vec<Project>,
    pub sessions: Vec<Session>,
    /// Sessions that ended recently, newest first: each floor's sign-out sheet.
    pub ended: Vec<EndedSession>,
    pub questions: Vec<Question>,
    pub decisions: Vec<Decision>,
    pub todos: Vec<TodoItem>,
    pub messages: Vec<AgentMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EffortOption {
    pub id: String,
    pub label: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelOption {
    pub id: String,
    pub label: String,
    pub efforts: Vec<EffortOption>,
    pub default_effort: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolOptions {
    pub tool: ToolKind,
    pub models: Vec<ModelOption>,
    pub default_model: String,
}

/// Everything needed to start a new agent session (`HireRequest` in DiscoveryApi.ts).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HireRequest {
    pub tool: ToolKind,
    pub title: String,
    pub model: String,
    pub effort: String,
}

/// A human's answer to a queued question (`QuestionAnswer` in DiscoveryApi.ts).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum QuestionAnswer {
    Option {
        #[serde(rename = "optionId")]
        option_id: String,
    },
    Other {
        text: String,
    },
}

/// A chunk of an in-app terminal's output (`office://terminal-output`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TerminalOutput {
    pub terminal_id: String,
    /// Counts chunks from 1, so a panel can skip the ones its backlog already holds.
    pub seq: u64,
    pub data: String,
}

/// An in-app terminal's agent has exited (`office://terminal-exit`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TerminalExit {
    pub terminal_id: String,
    /// None when the exit status could not be read.
    pub exit_code: Option<u32>,
}

/// A hired agent exited before it ever reached a desk (`office://hire-failed`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HireFailed {
    /// The task it was hired for.
    pub title: String,
    pub exit_code: Option<u32>,
    /// The last non-blank lines it printed, as plain text.
    pub last_lines: Vec<String>,
}

/// What a panel replays when it opens: the kept output up to chunk `seq`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TerminalBacklog {
    pub terminal_id: String,
    pub seq: u64,
    pub data: String,
    pub running: bool,
    pub exit_code: Option<u32>,
    pub cols: u16,
    pub rows: u16,
}
