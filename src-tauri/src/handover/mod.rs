//! Handing a desk over between the Claude and Codex subscriptions: the types
//! that cross the wire and the pure core they rest on.

use crate::delivery::text::{clean, line};
use crate::model::ToolKind;
use serde::{Deserialize, Serialize};

pub mod brief;
pub mod commands;
pub mod decide;
pub mod git;
pub mod record;
pub mod start;
pub mod survey;
pub mod view;
pub mod work;

/// Untrusted text (from a transcript, a hook or git) made safe to keep and show: escape
/// sequences, control and invisible characters go and blank runs collapse. Empty when
/// nothing is left.
fn tidy(raw: &str) -> String {
    clean(raw, usize::MAX).unwrap_or_default()
}

/// `tidy`, on one line.
fn tidy_line(raw: &str) -> String {
    line(raw, usize::MAX).unwrap_or_default()
}

/// The first `max` characters of `text`, and how many characters come after them.
fn head(text: &str, max: usize) -> (&str, usize) {
    match text.char_indices().nth(max) {
        Some((at, _)) => {
            let (kept, rest) = text.split_at(at);
            (kept, rest.chars().count())
        }
        None => (text, 0),
    }
}

/// The two tools a desk can be handed between.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Tool {
    ClaudeCode,
    Codex,
}

impl Tool {
    /// The handover tool for `kind`, None for every other tool.
    #[must_use]
    pub fn of(kind: ToolKind) -> Option<Self> {
        match kind {
            ToolKind::ClaudeCode => Some(Self::ClaudeCode),
            ToolKind::Codex => Some(Self::Codex),
            ToolKind::Opencode | ToolKind::Antigravity | ToolKind::Cursor | ToolKind::Unknown => {
                None
            }
        }
    }

    #[must_use]
    pub fn kind(self) -> ToolKind {
        match self {
            Self::ClaudeCode => ToolKind::ClaudeCode,
            Self::Codex => ToolKind::Codex,
        }
    }

    /// The tool a desk of this tool is handed to.
    #[must_use]
    pub fn other(self) -> Self {
        match self {
            Self::ClaudeCode => Self::Codex,
            Self::Codex => Self::ClaudeCode,
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::ClaudeCode => "Claude Code",
            Self::Codex => "Codex",
        }
    }

    /// How this tool's used-up allowance is worded after its name:
    /// "subscription allowance is used up" | "usage limit is reached".
    #[must_use]
    pub fn ran_out(self) -> &'static str {
        match self {
            Self::ClaudeCode => "subscription allowance is used up",
            Self::Codex => "usage limit is reached",
        }
    }
}

/// A subscription window the building can name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Window {
    FiveHour,
    SevenDay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TaskKind {
    Workflow,
    Subagent,
    Shell,
    Scheduled,
    Goal,
    Other,
}

/// One piece of background work a desk has in flight.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub kind: TaskKind,
    pub name: Option<String>,
}

/// What a desk's own tool record says about its allowance, at the end of what was read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Allowance {
    /// Nothing that proves anything.
    Unproven,
    /// No refusal stands: none was read, or the tool has answered since.
    Clear,
    /// The tool's own record that its allowance is used up. `at`: when the tool wrote it,
    /// Unix seconds, if the record says.
    UsedUp {
        window: Option<Window>,
        reset_at: Option<i64>,
        at: Option<i64>,
    },
}

/// What the building knows about a desk's background work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Work {
    Clear,
    Running { tasks: Vec<Task> },
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_maps_to_and_from_the_kind() {
        assert_eq!(Tool::of(ToolKind::ClaudeCode), Some(Tool::ClaudeCode));
        assert_eq!(Tool::of(ToolKind::Codex), Some(Tool::Codex));
        assert_eq!(Tool::of(ToolKind::Cursor), None);
        assert_eq!(Tool::of(ToolKind::Unknown), None);
        assert_eq!(Tool::ClaudeCode.kind(), ToolKind::ClaudeCode);
        assert_eq!(Tool::Codex.kind(), ToolKind::Codex);
    }

    #[test]
    fn head_counts_characters_not_bytes() {
        assert_eq!(head("héllo", 2), ("hé", 3));
        assert_eq!(head("abc", 3), ("abc", 0));
        assert_eq!(head("abc", 9), ("abc", 0));
        assert_eq!(tidy("\x1b[31mred\x1b[0m\x07"), "red");
        assert_eq!(tidy_line("a\n\nb"), "a b");
        assert_eq!(tidy("\u{200b}"), "");
    }

    #[test]
    fn tool_other_and_label() {
        assert_eq!(Tool::ClaudeCode.other(), Tool::Codex);
        assert_eq!(Tool::Codex.other(), Tool::ClaudeCode);
        assert_eq!(Tool::ClaudeCode.label(), "Claude Code");
        assert_eq!(Tool::Codex.label(), "Codex");
        assert_eq!(
            Tool::ClaudeCode.ran_out(),
            "subscription allowance is used up"
        );
        assert_eq!(Tool::Codex.ran_out(), "usage limit is reached");
    }

    #[test]
    fn tool_wire_words() {
        assert_eq!(
            serde_json::to_string(&Tool::ClaudeCode).expect("json"),
            r#""claude-code""#
        );
        assert_eq!(
            serde_json::to_string(&Tool::Codex).expect("json"),
            r#""codex""#
        );
        assert_eq!(
            serde_json::to_string(&Window::FiveHour).expect("json"),
            r#""five-hour""#
        );
        assert_eq!(
            serde_json::to_string(&Window::SevenDay).expect("json"),
            r#""seven-day""#
        );
    }

    #[test]
    fn a_goal_is_a_task_kind_on_the_wire() {
        assert_eq!(
            serde_json::to_string(&TaskKind::Goal).expect("json"),
            r#""goal""#
        );
        let running = Work::Running {
            tasks: vec![Task {
                kind: TaskKind::Goal,
                name: Some("Finish the migration and keep tests green".into()),
            }],
        };
        assert_eq!(
            serde_json::to_string(&running).expect("json"),
            r#"{"kind":"running","tasks":[{"kind":"goal","name":"Finish the migration and keep tests green"}]}"#
        );
    }

    #[test]
    fn work_wire_shapes() {
        let json = |w: &Work| serde_json::to_string(w).expect("json");
        assert_eq!(json(&Work::Clear), r#"{"kind":"clear"}"#);
        assert_eq!(json(&Work::Unknown), r#"{"kind":"unknown"}"#);
        let running = Work::Running {
            tasks: vec![
                Task {
                    kind: TaskKind::Workflow,
                    name: Some("release".into()),
                },
                Task {
                    kind: TaskKind::Subagent,
                    name: None,
                },
            ],
        };
        assert_eq!(
            json(&running),
            r#"{"kind":"running","tasks":[{"kind":"workflow","name":"release"},{"kind":"subagent","name":null}]}"#
        );
    }
}
