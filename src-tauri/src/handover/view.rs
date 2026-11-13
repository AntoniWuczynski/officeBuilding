//! What the snapshot says about handovers (`HandoverView` in types.ts).

use super::{Tool, Window, Work};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoverView {
    /// Subscriptions recorded as used up and not yet reset: at most one entry per tool.
    pub used_up: Vec<UsedUpView>,
    /// What a desk or a sign-out line has to say, by session id. At most one entry per session.
    pub desks: Vec<DeskHandover>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsedUpView {
    pub tool: Tool,
    pub window: Option<Window>,
    pub reset_at: Option<i64>,
    pub seen_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeskHandover {
    pub session_id: String,
    pub state: DeskState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum DeskState {
    /// Its tool's subscription is used up and A Human can hand it over.
    Offered {
        to: Tool,
        why: OfferWhy,
        work: Work,
        window: Option<Window>,
        reset_at: Option<i64>,
    },
    /// Its tool's subscription is used up and it cannot be handed over now.
    Blocked {
        to: Tool,
        why: BlockWhy,
        window: Option<Window>,
        reset_at: Option<i64>,
        /// When the receiving tool resets, for `BothUsedUp`.
        other_reset_at: Option<i64>,
    },
    /// The building is handing it over now.
    Underway { to: Tool },
    /// It was started by a handover.
    Arrived {
        from: Tool,
        at: i64,
        window: Option<Window>,
        reset_at: Option<i64>,
    },
    /// It was handed over and has signed out.
    Left { to: Tool, at: i64 },
    /// A handover ended it and the other tool did not start.
    Failed { to: Tool, at: i64, reason: String },
}

/// Why a desk waits for A Human instead of being handed over by itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum OfferWhy {
    AutomaticOff,
    WorkRunning,
    WorkUnknown,
    SeenElsewhere,
    NotAtTheTime,
    /// The last handover for this limit did not work and the desk is still here.
    Failed {
        reason: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BlockWhy {
    NotChosen,
    BothUsedUp,
    AlreadyHandedOver,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handover::{Task, TaskKind};

    fn json<T: Serialize>(v: &T) -> String {
        serde_json::to_string(v).expect("json")
    }

    #[test]
    fn the_default_view_is_empty() {
        assert_eq!(
            json(&HandoverView::default()),
            r#"{"usedUp":[],"desks":[]}"#
        );
    }

    #[test]
    fn a_view_with_one_of_each_part() {
        let view = HandoverView {
            used_up: vec![UsedUpView {
                tool: Tool::ClaudeCode,
                window: Some(Window::FiveHour),
                reset_at: Some(1_700_000_000),
                seen_at: 1_699_990_000,
            }],
            desks: vec![DeskHandover {
                session_id: "s1".into(),
                state: DeskState::Underway { to: Tool::Codex },
            }],
        };
        assert_eq!(
            json(&view),
            r#"{"usedUp":[{"tool":"claude-code","window":"five-hour","resetAt":1700000000,"seenAt":1699990000}],"desks":[{"sessionId":"s1","state":{"kind":"underway","to":"codex"}}]}"#
        );
    }

    #[test]
    fn offered_json() {
        let state = DeskState::Offered {
            to: Tool::Codex,
            why: OfferWhy::WorkRunning,
            work: Work::Running {
                tasks: vec![Task {
                    kind: TaskKind::Workflow,
                    name: Some("ship".into()),
                }],
            },
            window: Some(Window::SevenDay),
            reset_at: None,
        };
        assert_eq!(
            json(&state),
            r#"{"kind":"offered","to":"codex","why":{"kind":"work-running"},"work":{"kind":"running","tasks":[{"kind":"workflow","name":"ship"}]},"window":"seven-day","resetAt":null}"#
        );
    }

    #[test]
    fn blocked_json() {
        let state = DeskState::Blocked {
            to: Tool::Codex,
            why: BlockWhy::BothUsedUp,
            window: None,
            reset_at: Some(5),
            other_reset_at: Some(9),
        };
        assert_eq!(
            json(&state),
            r#"{"kind":"blocked","to":"codex","why":"both-used-up","window":null,"resetAt":5,"otherResetAt":9}"#
        );
        assert_eq!(json(&BlockWhy::NotChosen), r#""not-chosen""#);
        assert_eq!(
            json(&BlockWhy::AlreadyHandedOver),
            r#""already-handed-over""#
        );
    }

    #[test]
    fn underway_arrived_left_failed_json() {
        assert_eq!(
            json(&DeskState::Underway { to: Tool::Codex }),
            r#"{"kind":"underway","to":"codex"}"#
        );
        assert_eq!(
            json(&DeskState::Arrived {
                from: Tool::ClaudeCode,
                at: 7,
                window: Some(Window::FiveHour),
                reset_at: Some(8),
            }),
            r#"{"kind":"arrived","from":"claude-code","at":7,"window":"five-hour","resetAt":8}"#
        );
        assert_eq!(
            json(&DeskState::Left {
                to: Tool::Codex,
                at: 7
            }),
            r#"{"kind":"left","to":"codex","at":7}"#
        );
        assert_eq!(
            json(&DeskState::Failed {
                to: Tool::Codex,
                at: 7,
                reason: "no login".into()
            }),
            r#"{"kind":"failed","to":"codex","at":7,"reason":"no login"}"#
        );
    }

    #[test]
    fn offer_why_json() {
        assert_eq!(json(&OfferWhy::AutomaticOff), r#"{"kind":"automatic-off"}"#);
        assert_eq!(json(&OfferWhy::WorkUnknown), r#"{"kind":"work-unknown"}"#);
        assert_eq!(
            json(&OfferWhy::SeenElsewhere),
            r#"{"kind":"seen-elsewhere"}"#
        );
        assert_eq!(
            json(&OfferWhy::NotAtTheTime),
            r#"{"kind":"not-at-the-time"}"#
        );
        assert_eq!(
            json(&OfferWhy::Failed {
                reason: "it broke".into()
            }),
            r#"{"kind":"failed","reason":"it broke"}"#
        );
    }
}
