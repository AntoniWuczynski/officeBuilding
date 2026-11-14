//! What a hook's input says about a session's background work (spec 5.1). The
//! fields are documented on <https://code.claude.com/docs/en/hooks>, in the input
//! sections of `Stop`, `SubagentStop`, `SessionStart`, `PreToolUse` and
//! `PostToolUse` (read 2026-11-13). Hook input is untrusted, so every field is
//! read leniently: one of the wrong shape reads as absent and never drops the event.

use crate::delivery::text;
use crate::handover::work::Signal;
use crate::handover::{Task, TaskKind};
use crate::setup::HookEvent;
use serde::de::IgnoredAny;
use serde::{Deserialize, Deserializer};
use std::time::SystemTime;

/// The longest task name kept.
const NAME_MAX: usize = 80;

/// Tools that start work which outlives the turn, as this machine's Claude Code 2.1.323
/// lists them. `Bash` and `Agent` count only when their input asks for the background.
const STARTERS: &[(&str, TaskKind)] = &[
    ("Workflow", TaskKind::Workflow),
    ("Monitor", TaskKind::Other),
    ("CronCreate", TaskKind::Scheduled),
    ("ScheduleWakeup", TaskKind::Scheduled),
];

/// A value of `T`, or anything else.
#[derive(Deserialize)]
#[serde(untagged)]
enum Loose<T> {
    Is(T),
    Not(IgnoredAny),
}

/// `T` when the value has its shape, None when it has any other.
fn loose<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Option<T>, D::Error> {
    Ok(match Loose::<T>::deserialize(d)? {
        Loose::Is(value) => Some(value),
        Loose::Not(_) => None,
    })
}

/// The fields of a hook's input the work log reads.
#[derive(Deserialize)]
struct Hooked {
    session_id: String,
    /// `SessionStart`: `startup`, `resume`, `clear`, `compact` or `fork`.
    #[serde(default, deserialize_with = "loose")]
    source: Option<String>,
    #[serde(default, deserialize_with = "loose")]
    tool_name: Option<String>,
    #[serde(default, deserialize_with = "loose")]
    tool_input: Option<ToolInput>,
    #[serde(default, deserialize_with = "loose")]
    tool_response: Option<ToolResponse>,
    /// `Stop`, `SubagentStop`: the tasks in flight.
    #[serde(default, deserialize_with = "loose")]
    background_tasks: Option<Vec<Listed>>,
    /// `Stop`, `SubagentStop`: the scheduled wake-ups. Only counted.
    #[serde(default, deserialize_with = "loose")]
    session_crons: Option<Vec<IgnoredAny>>,
}

#[derive(Deserialize)]
struct ToolInput {
    #[serde(default, deserialize_with = "loose")]
    run_in_background: Option<bool>,
}

#[derive(Deserialize)]
struct ToolResponse {
    #[serde(default, deserialize_with = "loose")]
    status: Option<String>,
}

/// One entry of `background_tasks`.
#[derive(Deserialize)]
struct Listed {
    #[serde(rename = "type")]
    kind: String,
    /// The workflow's name, for a `workflow` task.
    #[serde(default, deserialize_with = "loose")]
    name: Option<String>,
}

impl Listed {
    fn task(self) -> Task {
        let kind = match self.kind.as_str() {
            "workflow" => TaskKind::Workflow,
            "subagent" => TaskKind::Subagent,
            "shell" => TaskKind::Shell,
            _ => TaskKind::Other,
        };
        Task {
            kind,
            name: self.name.and_then(|n| text::line(&n, NAME_MAX).ok()),
        }
    }
}

/// What the hook input `text` of `event`, dropped at `at`, says about background work.
pub(super) fn read(event: HookEvent, text: &str, at: SystemTime) -> Option<Signal> {
    let hooked: Hooked = serde_json::from_str(text).ok()?;
    let session_id = hooked.session_id;
    let tool = hooked.tool_name.as_deref();
    let kind = match event {
        HookEvent::SessionStart => {
            return (hooked.source.as_deref() == Some("startup")).then(|| Signal::Listed {
                session_id,
                at,
                tasks: Some(Vec::new()),
            });
        }
        HookEvent::Stop | HookEvent::SubagentStop => {
            let tasks = hooked
                .background_tasks
                .zip(hooked.session_crons)
                .map(|(listed, crons)| {
                    let scheduled = crons.iter().map(|_| Task {
                        kind: TaskKind::Scheduled,
                        name: None,
                    });
                    listed
                        .into_iter()
                        .map(Listed::task)
                        .chain(scheduled)
                        .collect()
                });
            return Some(Signal::Listed {
                session_id,
                at,
                tasks,
            });
        }
        // A refused request ends a turn without a `Stop`, so what the turn started is taken
        // from its tool calls. Only a call that asks for work outliving the turn counts: a
        // foreground command or an unknown tool does not, or no desk that ran a command
        // would ever be handed over by itself.
        HookEvent::PreToolUse => {
            let background = hooked.tool_input.and_then(|i| i.run_in_background) == Some(true);
            match tool {
                Some("Bash") if background => Some(TaskKind::Shell),
                Some("Agent") if background => Some(TaskKind::Subagent),
                Some(name) => STARTERS
                    .iter()
                    .find(|(starter, _)| *starter == name)
                    .map(|(_, kind)| *kind),
                None => None,
            }
        }
        HookEvent::PostToolUse => (tool == Some("Agent")
            && hooked.tool_response.and_then(|r| r.status).as_deref() == Some("async_launched"))
        .then_some(TaskKind::Subagent),
        HookEvent::UserPromptSubmit
        | HookEvent::Notification
        | HookEvent::PermissionRequest
        | HookEvent::StopFailure => None,
    }?;
    Some(Signal::Started {
        session_id,
        at,
        task: Task { kind, name: None },
    })
}

/// When a signal happened.
pub(super) fn at(signal: &Signal) -> SystemTime {
    match signal {
        Signal::Listed { at, .. } | Signal::Started { at, .. } => *at,
    }
}
