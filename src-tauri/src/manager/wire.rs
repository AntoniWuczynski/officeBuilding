//! The manager's types on the wire (`ManagerFloor` and friends in types.ts) and in `store.json`.

use crate::delivery::gate::Hold;
use crate::model::ToolKind;
use serde::{Deserialize, Serialize};

pub const LOG_KEPT: usize = 200;
pub const LOG_SHOWN: usize = 50;
pub const LOG_TEXT_MAX: usize = 1000;

/// A note waiting to reach a desk (`QueuedNote` in types.ts).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueuedNote {
    pub id: String,
    pub to_session_id: String,
    pub text: String,
    pub queued_at: String,
    pub hold: Hold,
}

/// What became of a note (`NoteState` in types.ts).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NoteState {
    Queued,
    Delivered,
    /// A Human withdrew it in the Manager panel.
    Withdrawn,
    /// A Human sent the desk a message first.
    Overridden,
    /// The desk or the manager left before it could be handed over.
    Dropped,
    /// The hand-over itself failed.
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum LogEvent {
    /// A note to a desk. Its `state` is updated in place as the note moves on.
    Note {
        note_id: String,
        to_session_id: String,
        to_title: String,
        text: String,
        state: NoteState,
    },
    Hire {
        tool: ToolKind,
        task: String,
    },
    Escalation {
        title: String,
    },
    /// A call the building turned down. `tool` is the tool's name, `reason` is in words.
    Refused {
        tool: String,
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub id: String,
    pub at: String,
    pub event: LogEvent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum ManagerStatus {
    /// Nobody is hired: the chair is empty.
    Absent,
    /// `session_id`: its session, once discovery has bound it to its terminal.
    /// `last_call_at`: when it last used one of the building's tools. None: not yet.
    Hired {
        session_id: Option<String>,
        started_at: String,
        last_call_at: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagerCaps {
    pub max_desks: u32,
    /// Desks it hired that are still running.
    pub hired_now: u32,
    pub spend_ceiling_usd: u32,
    /// Summed spend of every desk it hired since it started.
    pub spent_usd: f64,
    /// Hired desks with tokens no price is known for.
    pub unpriced_desks: u32,
}

/// A floor's manager (`ManagerFloor` in types.ts).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagerFloor {
    pub project_id: String,
    pub status: ManagerStatus,
    pub paused: bool,
    pub may_hire: bool,
    /// A call it escalated is still open.
    pub escalated: bool,
    /// Desks it hired or has sent a note to, by session id.
    pub managed: Vec<String>,
    pub caps: ManagerCaps,
    /// Oldest first.
    pub queue: Vec<QueuedNote>,
    /// Newest first, at most `LOG_SHOWN`.
    pub log: Vec<LogEntry>,
}

/// What `store.json` keeps for a floor, by absolute repo root like `pinned_floors`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ManagerRecord {
    pub paused: bool,
    pub may_hire: bool,
    /// Oldest first, at most `LOG_KEPT`. Text in it is cut to `LOG_TEXT_MAX` characters.
    pub log: Vec<LogEntry>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    const DESK: &str = "0f1e2d3c-0000-4000-8000-000000000002";
    const NOTE: &str = "Take the first three items of TODO.md.";

    fn value<T: Serialize>(v: &T) -> Value {
        serde_json::to_value(v).expect("value")
    }

    #[test]
    fn the_example_entry_matches_the_wire_json_and_reads_back() {
        let floor = ManagerFloor {
            project_id: "p-1a2b3c4d".into(),
            status: ManagerStatus::Hired {
                session_id: Some("0f1e2d3c-0000-4000-8000-000000000001".into()),
                started_at: "2026-11-12T10:00:00Z".into(),
                last_call_at: Some("2026-11-12T10:04:10Z".into()),
            },
            paused: false,
            may_hire: true,
            escalated: false,
            managed: vec![DESK.into()],
            caps: ManagerCaps {
                max_desks: 3,
                hired_now: 1,
                spend_ceiling_usd: 20,
                spent_usd: 1.25,
                unpriced_desks: 0,
            },
            queue: vec![QueuedNote {
                id: "n-41".into(),
                to_session_id: DESK.into(),
                text: NOTE.into(),
                queued_at: "2026-11-12T10:04:10Z".into(),
                hold: Hold::Busy,
            }],
            log: vec![LogEntry {
                id: "g-42".into(),
                at: "2026-11-12T10:04:10Z".into(),
                event: LogEvent::Note {
                    note_id: "n-41".into(),
                    to_session_id: DESK.into(),
                    to_title: "audit checkout".into(),
                    text: NOTE.into(),
                    state: NoteState::Queued,
                },
            }],
        };
        let want = json!({ "projectId": "p-1a2b3c4d",
          "status": { "kind": "hired", "sessionId": "0f1e2d3c-0000-4000-8000-000000000001", "startedAt": "2026-11-12T10:00:00Z", "lastCallAt": "2026-11-12T10:04:10Z" },
          "paused": false, "mayHire": true, "escalated": false,
          "managed": ["0f1e2d3c-0000-4000-8000-000000000002"],
          "caps": { "maxDesks": 3, "hiredNow": 1, "spendCeilingUsd": 20, "spentUsd": 1.25, "unpricedDesks": 0 },
          "queue": [{ "id": "n-41", "toSessionId": "0f1e2d3c-0000-4000-8000-000000000002", "text": "Take the first three items of TODO.md.", "queuedAt": "2026-11-12T10:04:10Z", "hold": "busy" }],
          "log": [{ "id": "g-42", "at": "2026-11-12T10:04:10Z",
                    "event": { "kind": "note", "noteId": "n-41", "toSessionId": "0f1e2d3c-0000-4000-8000-000000000002", "toTitle": "audit checkout", "text": "Take the first three items of TODO.md.", "state": "queued" } }] });
        assert_eq!(value(&floor), want);
        assert_eq!(
            serde_json::from_value::<ManagerFloor>(want).expect("read"),
            floor
        );
    }

    #[test]
    fn a_floor_with_no_manager_matches_the_wire_json() {
        let floor = ManagerFloor {
            project_id: "p".into(),
            status: ManagerStatus::Absent,
            paused: false,
            may_hire: false,
            escalated: false,
            managed: Vec::new(),
            caps: ManagerCaps {
                max_desks: 3,
                hired_now: 0,
                spend_ceiling_usd: 20,
                spent_usd: 0.0,
                unpriced_desks: 0,
            },
            queue: Vec::new(),
            log: Vec::new(),
        };
        let want = json!({ "projectId": "p", "status": { "kind": "absent" }, "paused": false,
            "mayHire": false, "escalated": false, "managed": [],
            "caps": { "maxDesks": 3, "hiredNow": 0, "spendCeilingUsd": 20, "spentUsd": 0.0, "unpricedDesks": 0 },
            "queue": [], "log": [] });
        assert_eq!(value(&floor), want);
    }

    #[test]
    fn the_other_log_events_match_the_wire_json() {
        let cases = [
            (
                LogEvent::Hire {
                    tool: ToolKind::Codex,
                    task: "t".into(),
                },
                json!({ "kind": "hire", "tool": "codex", "task": "t" }),
            ),
            (
                LogEvent::Escalation { title: "x".into() },
                json!({ "kind": "escalation", "title": "x" }),
            ),
            (
                LogEvent::Refused {
                    tool: "send_to_desk".into(),
                    reason: "paused".into(),
                },
                json!({ "kind": "refused", "tool": "send_to_desk", "reason": "paused" }),
            ),
        ];
        for (event, want) in cases {
            assert_eq!(value(&event), want);
            assert_eq!(
                serde_json::from_value::<LogEvent>(want).expect("read"),
                event
            );
        }
    }

    #[test]
    fn holds_and_note_states_have_their_wire_names() {
        let holds = [
            (Hold::Busy, "busy"),
            (Hold::WaitingOnYou, "waiting-on-you"),
            (Hold::Stuck, "stuck"),
            (Hold::Finished, "finished"),
            (Hold::YouTyped, "you-typed"),
            (Hold::AwaitingTurn, "awaiting-turn"),
            (Hold::HandStarted, "hand-started"),
            (Hold::NoIdleSignal, "no-idle-signal"),
            (Hold::Paused, "paused"),
        ];
        for (hold, name) in holds {
            assert_eq!(value(&hold), json!(name));
            assert_eq!(
                serde_json::from_value::<Hold>(json!(name)).expect("read"),
                hold
            );
        }
        let states = [
            (NoteState::Queued, "queued"),
            (NoteState::Delivered, "delivered"),
            (NoteState::Withdrawn, "withdrawn"),
            (NoteState::Overridden, "overridden"),
            (NoteState::Dropped, "dropped"),
            (NoteState::Failed, "failed"),
        ];
        for (state, name) in states {
            assert_eq!(value(&state), json!(name));
            assert_eq!(
                serde_json::from_value::<NoteState>(json!(name)).expect("read"),
                state
            );
        }
    }

    #[test]
    fn a_record_loads_from_an_empty_object() {
        assert_eq!(
            serde_json::from_str::<ManagerRecord>("{}").expect("read"),
            ManagerRecord::default()
        );
    }
}
