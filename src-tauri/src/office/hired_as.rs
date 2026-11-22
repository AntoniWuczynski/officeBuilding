//! What a Claude Code or Codex desk was hired as in the Hire sheet: the model, the effort and
//! ultracode. The tools' own records fill a session's model and effort only after its first
//! reply, and nothing tells the app a session's ultracode state, so the hire is kept by terminal.

use std::collections::BTreeMap;

use super::launch::{DEFAULT_EFFORT, DEFAULT_MODEL, is_discovered};
use super::{Inner, Office};
use crate::model::{EndedSession, HireRequest, Session, ToolKind};

/// What A Human chose in the Hire sheet for a Claude Code or Codex terminal.
pub(super) struct HiredAs {
    model: Option<String>,
    effort: Option<String>,
    ultracode: bool,
}

/// By terminal id.
#[derive(Default)]
pub(super) struct State {
    pub(super) by_terminal: BTreeMap<String, HiredAs>,
}

impl State {
    /// Keep the hires of terminals still running or still holding a desk that ran in them.
    pub(super) fn keep_needed(
        &mut self,
        running: &BTreeMap<i32, String>,
        ran_in: &BTreeMap<String, String>,
    ) {
        self.by_terminal.retain(|terminal, _| {
            running.values().any(|id| id == terminal) || ran_in.values().any(|t| t == terminal)
        });
    }
}

fn chosen(value: &str, default: &str) -> Option<String> {
    (value != default).then(|| value.to_string())
}

impl Office {
    /// Record a hire of a discovered tool (`model` is the id the hire resolved to).
    pub(super) fn note_hire(&self, req: &HireRequest, model: &str, terminal: &str) {
        if !is_discovered(req.tool) {
            return;
        }
        let hired = HiredAs {
            model: chosen(model, DEFAULT_MODEL),
            effort: chosen(&req.effort, DEFAULT_EFFORT),
            ultracode: req.tool == ToolKind::ClaudeCode && req.ultracode,
        };
        self.lock()
            .hired_as
            .by_terminal
            .insert(terminal.to_string(), hired);
    }

    /// Fill a desk's model and effort from its hire where the tool has not given its own.
    fn fill_one(inner: &Inner, id: &str, model: &mut Option<String>, effort: &mut Option<String>) {
        let hired = inner
            .ran_in
            .get(id)
            .and_then(|terminal| inner.hired_as.by_terminal.get(terminal));
        if let Some(hired) = hired {
            if model.is_none() {
                model.clone_from(&hired.model);
            }
            if effort.is_none() {
                effort.clone_from(&hired.effort);
            }
        }
    }

    /// Give a session the hire's model and effort where the tool has not given its own.
    pub(super) fn fill_hired(inner: &Inner, sessions: &mut [Session]) {
        for s in sessions {
            Self::fill_one(inner, &s.id, &mut s.model, &mut s.effort);
        }
    }

    /// As `fill_hired`, for the desks that have ended, so the sign-out sheet shows the desk's look.
    pub(super) fn fill_hired_ended(inner: &Inner, ended: &mut [EndedSession]) {
        for e in ended {
            Self::fill_one(inner, &e.id, &mut e.model, &mut e.effort);
        }
    }

    /// The sessions in terminals hired with ultracode on, by session id.
    pub(super) fn ultracode_desks(inner: &Inner) -> Vec<String> {
        inner
            .owned
            .iter()
            .filter(|(terminal, _)| {
                inner
                    .hired_as
                    .by_terminal
                    .get(*terminal)
                    .is_some_and(|h| h.ultracode)
            })
            .map(|(_, session)| session.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ControlMode, DeskView, OfficeSnapshot};
    use crate::office::tests::{office, session};

    fn req(tool: ToolKind, model: &str, effort: &str, ultracode: bool) -> HireRequest {
        HireRequest {
            tool,
            title: "t".to_string(),
            model: model.to_string(),
            effort: effort.to_string(),
            ultracode,
            view: DeskView::Terminal,
        }
    }

    fn hired(office: &Office, terminal: &str) -> Option<(Option<String>, Option<String>, bool)> {
        office
            .lock()
            .hired_as
            .by_terminal
            .get(terminal)
            .map(|h| (h.model.clone(), h.effort.clone(), h.ultracode))
    }

    #[test]
    fn records_what_was_chosen() {
        let (office, _dir) = office("hired-record");
        let claude = req(ToolKind::ClaudeCode, "claude-opus-4-8", "high", true);
        office.note_hire(&claude, "claude-opus-4-8", "t1");
        assert_eq!(
            hired(&office, "t1"),
            Some((
                Some("claude-opus-4-8".to_string()),
                Some("high".to_string()),
                true
            ))
        );
        let codex = req(ToolKind::Codex, "default", "default", true);
        office.note_hire(&codex, "default", "t2");
        assert_eq!(hired(&office, "t2"), Some((None, None, false)));
        for tool in [ToolKind::Opencode, ToolKind::Cursor, ToolKind::Antigravity] {
            office.note_hire(&req(tool, "m", "high", true), "m", "t3");
        }
        assert_eq!(hired(&office, "t3"), None);
    }

    #[test]
    fn lists_ultracode_desks_and_follows_the_terminal() {
        let (office, _dir) = office("hired-ultra");
        office.note_hire(
            &req(ToolKind::ClaudeCode, "default", "default", true),
            "default",
            "a",
        );
        office.note_hire(
            &req(ToolKind::ClaudeCode, "default", "default", false),
            "default",
            "b",
        );
        let mut inner = office.lock();
        inner.owned.insert("a".to_string(), "s-1".to_string());
        inner.owned.insert("b".to_string(), "s-2".to_string());
        inner.owned.insert("c".to_string(), "s-3".to_string());
        assert_eq!(Office::ultracode_desks(&inner), vec!["s-1".to_string()]);
        inner.owned.insert("a".to_string(), "s-9".to_string());
        assert_eq!(Office::ultracode_desks(&inner), vec!["s-9".to_string()]);
    }

    #[test]
    fn fills_only_what_the_tool_left_empty() {
        let (office, _dir) = office("hired-fill");
        office.note_hire(
            &req(ToolKind::ClaudeCode, "claude-sonnet-5-5", "low", false),
            "claude-sonnet-5-5",
            "a",
        );
        let mut inner = office.lock();
        inner.ran_in.insert("s-1".to_string(), "a".to_string());
        inner.ran_in.insert("s-2".to_string(), "a".to_string());
        let mut sessions = vec![
            session("s-1", ControlMode::Full),
            session("s-2", ControlMode::Full),
            session("s-3", ControlMode::Full),
        ];
        sessions[0].model = None;
        sessions[0].effort = None;
        sessions[1].model = Some("claude-haiku-5-5".to_string());
        sessions[1].effort = None;
        sessions[2].model = None;
        sessions[2].effort = None;
        Office::fill_hired(&inner, &mut sessions);
        assert_eq!(sessions[0].model.as_deref(), Some("claude-sonnet-5-5"));
        assert_eq!(sessions[0].effort.as_deref(), Some("low"));
        assert_eq!(sessions[1].model.as_deref(), Some("claude-haiku-5-5"));
        assert_eq!(sessions[1].effort.as_deref(), Some("low"));
        assert_eq!(
            (sessions[2].model.clone(), sessions[2].effort.clone()),
            (None, None)
        );
    }

    #[test]
    fn fills_an_ended_desk() {
        let (office, _dir) = office("hired-ended");
        office.note_hire(
            &req(ToolKind::ClaudeCode, "claude-opus-5-5", "high", false),
            "claude-opus-5-5",
            "a",
        );
        let mut inner = office.lock();
        inner.ran_in.insert("e-1".to_string(), "a".to_string());
        let mut ended = vec![EndedSession {
            id: "e-1".to_string(),
            tool: ToolKind::ClaudeCode,
            project_id: "p".to_string(),
            title: "t".to_string(),
            ended_at: String::new(),
            model: None,
            effort: None,
            last_reply: None,
        }];
        Office::fill_hired_ended(&inner, &mut ended);
        assert_eq!(ended[0].model.as_deref(), Some("claude-opus-5-5"));
        assert_eq!(ended[0].effort.as_deref(), Some("high"));
    }

    #[test]
    fn keeps_only_hires_still_needed() {
        let (office, _dir) = office("hired-prune");
        for terminal in ["live", "ended", "gone"] {
            office.note_hire(
                &req(ToolKind::ClaudeCode, "default", "high", false),
                "default",
                terminal,
            );
        }
        let running = BTreeMap::from([(1, "live".to_string())]);
        let ran_in = BTreeMap::from([("e-1".to_string(), "ended".to_string())]);
        office.lock().hired_as.keep_needed(&running, &ran_in);
        assert!(hired(&office, "live").is_some());
        assert!(hired(&office, "ended").is_some());
        assert_eq!(hired(&office, "gone"), None);
    }

    #[test]
    fn an_empty_snapshot_has_an_empty_ultracode_list() {
        let value = serde_json::to_value(OfficeSnapshot::default()).expect("json");
        assert_eq!(value["ultracode"], serde_json::json!([]));
    }
}
