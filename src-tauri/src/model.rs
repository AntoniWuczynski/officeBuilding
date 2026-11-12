//! The Tauri-boundary data model: the Rust mirror of `src/types.ts` and the
//! request types of `src/api/DiscoveryApi.ts`. Everything serialises in the
//! shape the frontend expects (camelCase fields, kebab-case enum values).

use crate::qa::record::{Review, Verdict};
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
    /// Driven by the app over the tool's protocol: it opens a conversation panel.
    Structured,
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
    /// What it left running: shells, monitors and other programs, each beside its desk.
    pub pets: Vec<Pet>,
    /// It works in a linked git worktree of its floor's repo.
    pub worktree: bool,
    /// The branch of the worktree it works in. None outside a worktree or on a detached HEAD.
    pub branch: Option<String>,
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
    /// It works in a linked git worktree (an isolated sub-agent's own checkout).
    pub worktree: bool,
    /// The branch of the worktree it works in. None outside a worktree or on a detached HEAD.
    pub branch: Option<String>,
}

/// Something in the building that has a folder. The webview names it and the backend finds
/// the folder: a path never crosses the boundary.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Place {
    /// A desk, or a line on the sign-out sheet.
    Session { session_id: String },
    /// A sub-agent's tiny desk.
    Helper {
        session_id: String,
        helper_id: String,
    },
    /// A floor: its main checkout.
    Floor { project_id: String },
}

/// A program a session left running (see `discovery::pets`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Pet {
    pub id: String,
    pub kind: PetKind,
    /// The command, or a monitor's description, shortened.
    pub label: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PetKind {
    /// A shell running a background or long command: drawn as a seashell.
    Shell,
    /// Claude Code's `Monitor` tool watching a command.
    Monitor,
    /// A browser the agent drives.
    Browser,
    /// Any other program it started.
    Process,
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
    /// Where the item lives. A store written before this field existed has none: the app's own.
    #[serde(default)]
    pub source: TodoSource,
}

/// Where a TODO item lives (`TodoSource` in types.ts).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum TodoSource {
    /// The app's own records.
    #[default]
    App,
    /// A checklist line in one of the repo's own files.
    File { file: String },
    /// An issue in Linear.
    Linear { identifier: String, url: String },
}

/// An outside tool a floor's TODOs can come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ConnectorKind {
    Linear,
}

impl ConnectorKind {
    pub const ALL: [Self; 1] = [Self::Linear];

    /// The middle segment of its items' ids.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Linear => "linear",
        }
    }

    /// Its name in a sentence.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Linear => "Linear",
        }
    }
}

/// A project in Linear (not a floor).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinearProject {
    pub id: String,
    pub name: String,
}

/// What feeds a floor from a connector. One variant per connector, and a floor holds at
/// most one link of each kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum Link {
    Linear {
        team_id: String,
        /// "ENG": shown, never sent to Linear.
        team_key: String,
        team_name: String,
        /// Only issues in this Linear project. None: any project.
        project: Option<LinearProject>,
        /// Only issues carrying this label. None: any label.
        label: Option<String>,
    },
}

impl Link {
    #[must_use]
    pub const fn kind(&self) -> ConnectorKind {
        match self {
            Self::Linear { .. } => ConnectorKind::Linear,
        }
    }
}

/// A floor's link and how its last check went (`Connection` in types.ts).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    pub project_id: String,
    pub link: Link,
    /// When its items were last fetched (ISO 8601). None until a check has worked.
    pub fetched_at: Option<String>,
    /// Why the last check failed, in words. None when it worked, or none has run yet.
    pub error: Option<String>,
    /// It cannot be checked until a key is saved, or allowed, in Settings.
    pub needs_key: bool,
    /// More issues match than are shown.
    pub truncated: bool,
}

/// What the building knows about the Linear key. Never the key itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum LinearKeyStatus {
    /// No key is saved.
    Missing,
    /// A key is in the Keychain. `account`: whose it is, when Linear has said so this launch.
    Saved { account: Option<String> },
    /// Linear refused the saved key.
    Rejected,
    /// The Keychain would not hand the key over.
    Unavailable { reason: String },
}

/// A Linear team with its open projects, for the connect form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LinearTeam {
    pub id: String,
    pub key: String,
    pub name: String,
    pub projects: Vec<LinearProject>,
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
    /// The setup floor's project id (it may not be standing).
    pub setup_project_id: String,
    /// Plain shells the human opened, listed in each floor's manager's office.
    pub shells: Vec<Shell>,
    /// Each standing floor's links to connectors, with how the last check went.
    pub connections: Vec<Connection>,
    /// One entry per standing floor except the setup floor, in floor order.
    pub qa: Vec<QaFloor>,
}

/// What a floor's QA desk is doing (`QaStatus` in types.ts).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum QaStatus {
    /// Switched off for this floor.
    Off,
    /// On, and GitHub has not been asked yet.
    Waiting,
    /// The last check worked. `open`: A Human's open pull requests there, drafts included.
    Watching {
        repo: String,
        checked_at: String,
        open: u32,
    },
    /// It cannot review: why, in words. No trailing full stop.
    Blocked { reason: String },
}

/// The review a floor's desk is running now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QaRun {
    pub number: u64,
    pub title: String,
    pub started_at: String,
    /// The reviewer's desk, once discovery has bound its session to its terminal.
    pub session_id: Option<String>,
}

/// A floor's QA desk (`QaFloor` in types.ts).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QaFloor {
    pub project_id: String,
    pub enabled: bool,
    pub status: QaStatus,
    pub run: Option<QaRun>,
    /// Newest first, at most 5.
    pub reviews: Vec<Review>,
    /// The stamp on the nameplate: the verdict standing for the pull requests' current heads.
    pub stamp: Option<Verdict>,
}

/// A plain login shell the human opened on a floor (Cmd+N): not an agent, never a desk.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Shell {
    /// Its terminal's id, which the terminal commands take.
    pub id: String,
    pub project_id: String,
    /// Numbered per floor from 1, in the order they were opened.
    pub number: u32,
    pub started_at: String,
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
    /// Installed on this Mac (its CLI is on the login shell's PATH). An
    /// unavailable tool is listed greyed out, with no models.
    pub available: bool,
    pub models: Vec<ModelOption>,
    pub default_model: String,
    /// None when the tool has no ultracode; otherwise its default (Claude Code).
    pub ultracode: Option<bool>,
    /// It can run as a structured desk (no terminal, typed messages and approvals).
    pub structured: bool,
}

/// Everything needed to start a new agent session (`HireRequest` in DiscoveryApi.ts).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HireRequest {
    pub tool: ToolKind,
    pub title: String,
    pub model: String,
    pub effort: String,
    /// Run with ultracode on (Claude Code only, a setting beside the effort).
    #[serde(default)]
    pub ultracode: bool,
    /// Run as a structured desk instead of in a terminal (tools whose options say they can).
    #[serde(default)]
    pub structured: bool,
}

/// One usage-limit window of a tool, as the tool itself last measured it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UsageWindow {
    pub id: String,
    /// e.g. "5-hour session", "Week", "Week · Fable".
    pub label: String,
    /// Share of the window used, 0–100.
    pub percent: f64,
    /// When the window resets (ISO 8601), if the tool says.
    pub resets_at: Option<String>,
}

/// The usage limits of one installed tool (`ToolUsage` in types.ts).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolUsage {
    pub tool: ToolKind,
    /// When the tool last measured these numbers (ISO 8601): the "as of" time.
    pub checked_at: Option<String>,
    pub windows: Vec<UsageWindow>,
    /// Why there are no numbers, when there are none.
    pub unavailable: Option<String>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn value<T: Serialize>(v: &T) -> serde_json::Value {
        serde_json::to_value(v).expect("value")
    }

    #[test]
    fn todo_sources_match_the_wire_json_and_read_back() {
        let cases = [
            (TodoSource::App, json!({ "kind": "app" })),
            (
                TodoSource::File {
                    file: "TODO.md".into(),
                },
                json!({ "kind": "file", "file": "TODO.md" }),
            ),
            (
                TodoSource::Linear {
                    identifier: "ENG-101".into(),
                    url: "https://linear.app/example/issue/ENG-101/x".into(),
                },
                json!({ "kind": "linear", "identifier": "ENG-101",
                        "url": "https://linear.app/example/issue/ENG-101/x" }),
            ),
        ];
        for (source, want) in cases {
            assert_eq!(value(&source), want);
            assert_eq!(
                serde_json::from_value::<TodoSource>(want).expect("read"),
                source
            );
        }
    }

    #[test]
    fn a_link_matches_the_wire_json_and_names_its_kind() {
        let link = Link::Linear {
            team_id: "t1".into(),
            team_key: "ENG".into(),
            team_name: "Engineering".into(),
            project: Some(LinearProject {
                id: "pr1".into(),
                name: "Checkout".into(),
            }),
            label: None,
        };
        let want = json!({ "kind": "linear", "teamId": "t1", "teamKey": "ENG",
            "teamName": "Engineering", "project": { "id": "pr1", "name": "Checkout" },
            "label": null });
        assert_eq!(value(&link), want);
        assert_eq!(serde_json::from_value::<Link>(want).expect("read"), link);
        assert_eq!(link.kind(), ConnectorKind::Linear);
    }

    #[test]
    fn a_connector_kind_has_an_id_and_a_label() {
        assert_eq!(value(&ConnectorKind::Linear), json!("linear"));
        assert_eq!(ConnectorKind::Linear.id(), "linear");
        assert_eq!(ConnectorKind::Linear.label(), "Linear");
    }

    #[test]
    fn key_statuses_match_the_wire_json() {
        assert_eq!(
            value(&LinearKeyStatus::Missing),
            json!({ "kind": "missing" })
        );
        assert_eq!(
            value(&LinearKeyStatus::Saved { account: None }),
            json!({ "kind": "saved", "account": null })
        );
        assert_eq!(
            value(&LinearKeyStatus::Rejected),
            json!({ "kind": "rejected" })
        );
        assert_eq!(
            value(&LinearKeyStatus::Unavailable {
                reason: "locked".into()
            }),
            json!({ "kind": "unavailable", "reason": "locked" })
        );
    }

    #[test]
    fn qa_types_match_the_wire_json() {
        use crate::qa::record::ReviewOutcome;
        assert_eq!(value(&QaStatus::Off), json!({ "kind": "off" }));
        assert_eq!(value(&QaStatus::Waiting), json!({ "kind": "waiting" }));
        assert_eq!(
            value(&QaStatus::Watching {
                repo: "example-org/widgets".into(),
                checked_at: "2026-11-12T10:15:00Z".into(),
                open: 2
            }),
            json!({ "kind": "watching", "repo": "example-org/widgets",
                    "checkedAt": "2026-11-12T10:15:00Z", "open": 2 })
        );
        assert_eq!(
            value(&QaStatus::Blocked {
                reason: "gh is not signed in".into()
            }),
            json!({ "kind": "blocked", "reason": "gh is not signed in" })
        );
        let floor = QaFloor {
            project_id: "p-1a2b3c4d".into(),
            enabled: true,
            status: QaStatus::Waiting,
            run: Some(QaRun {
                number: 14,
                title: "Cache the price list".into(),
                started_at: "2026-11-12T10:20:05Z".into(),
                session_id: Some("s-7".into()),
            }),
            reviews: vec![Review {
                repo: "example-org/widgets".into(),
                number: 12,
                title: "Add a retry to the uploader".into(),
                head: "0123456789abcdef0123456789abcdef01234567".into(),
                at: "2026-11-12T10:05:00Z".into(),
                outcome: ReviewOutcome::Posted {
                    verdict: Verdict::Concerns,
                    findings: 2,
                    review_id: 90210,
                },
            }],
            stamp: Some(Verdict::Concerns),
        };
        assert_eq!(
            value(&floor),
            json!({
                "projectId": "p-1a2b3c4d", "enabled": true, "status": { "kind": "waiting" },
                "run": { "number": 14, "title": "Cache the price list",
                         "startedAt": "2026-11-12T10:20:05Z", "sessionId": "s-7" },
                "reviews": [{ "repo": "example-org/widgets", "number": 12,
                    "title": "Add a retry to the uploader",
                    "head": "0123456789abcdef0123456789abcdef01234567",
                    "at": "2026-11-12T10:05:00Z",
                    "outcome": { "kind": "posted", "verdict": "concerns",
                                 "findings": 2, "reviewId": 90210 } }],
                "stamp": "concerns"
            })
        );
    }

    #[test]
    fn a_default_snapshot_has_no_qa_floors() {
        assert_eq!(value(&OfficeSnapshot::default())["qa"], json!([]));
    }

    #[test]
    fn a_default_snapshot_has_no_connections() {
        assert_eq!(value(&OfficeSnapshot::default())["connections"], json!([]));
    }
}
