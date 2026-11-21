//! The permission requests Codex's hooks saw begin, and which of them nothing has settled
//! (spec 5.3). A `PermissionRequest` proves a request began, not that a person is needed
//! (F20), so a request stays open until a hook proves its tool call ran (`PostToolUse`), the
//! root turn ended (`Stop`) or the session moved to another turn. Time never retires one.
//! Hook input is untrusted: every field is read leniently and a file of the wrong shape is
//! read as far as it goes.

use super::signals::loose;
use crate::delivery::text;
use crate::setup::HookEvent;
use serde::Deserialize;
use serde_json::value::RawValue;
use std::collections::BTreeMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::time::SystemTime;

/// The most open requests and remembered tool calls kept for a session.
const KEPT: usize = 16;
/// The longest tool name kept.
const TOOL_MAX: usize = 60;
/// The longest command kept.
const DETAIL_MAX: usize = 300;

/// One permission request a Codex hook saw begin (F19, F20).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asked {
    /// Unique while the app runs. The row's id is built from it.
    pub seq: u64,
    /// The turn it belongs to.
    pub turn_id: Option<String>,
    /// The tool call it is about, when a `PreToolUse` of the same tool and input named one.
    pub call_id: Option<String>,
    /// The sub-agent that asked, if one did. Stage M2b reads it.
    pub agent_id: Option<String>,
    /// The tool's name, cleaned, at most 60 chars.
    pub tool: String,
    /// The command it asks to run, cleaned onto one line of at most 300 chars. Empty when
    /// the input names none.
    pub detail: String,
    pub at: SystemTime,
    /// A hash of the tool input's command (or its JSON text when it names none), to find
    /// the same call's `PostToolUse`.
    input: u64,
}

/// A command as a string or as a list of words.
#[derive(Deserialize)]
#[serde(untagged)]
enum Command {
    Line(String),
    Words(Vec<String>),
}

/// The part of a tool's input that names the command.
#[derive(Deserialize)]
struct ToolInput {
    #[serde(default, deserialize_with = "loose")]
    command: Option<Command>,
}

/// The fields of a hook's input this reads.
#[derive(Deserialize)]
struct Hooked {
    session_id: String,
    #[serde(default, deserialize_with = "loose")]
    turn_id: Option<String>,
    #[serde(default, deserialize_with = "loose")]
    agent_id: Option<String>,
    #[serde(default, deserialize_with = "loose")]
    tool_name: Option<String>,
    #[serde(default, deserialize_with = "loose")]
    tool_use_id: Option<String>,
    #[serde(default)]
    tool_input: Option<Box<RawValue>>,
}

/// `raw` on one line, cut to `max` characters. None when nothing is left.
fn short(raw: &str, max: usize) -> Option<String> {
    let line = text::line(raw, usize::MAX).ok()?;
    Some(line.chars().take(max).collect())
}

/// One hook file taken in and not yet applied.
struct Event {
    kind: HookEvent,
    at: SystemTime,
    session: String,
    turn: Option<String>,
    agent: Option<String>,
    tool: String,
    call: Option<String>,
    input: u64,
    detail: String,
}

impl Event {
    fn read(kind: HookEvent, body: &str, at: SystemTime) -> Option<Self> {
        let hooked: Hooked = serde_json::from_str(body).ok()?;
        let raw = hooked.tool_input.as_deref().map(RawValue::get);
        // Codex adds a `description` to a PermissionRequest's input that Pre/PostToolUse
        // lack, so a call is matched on its command, and on the whole input only when the
        // input names none.
        let command = raw
            .and_then(|r| serde_json::from_str::<ToolInput>(r).ok())
            .and_then(|i| i.command)
            .map(|c| match c {
                Command::Line(line) => line,
                Command::Words(words) => words.join(" "),
            });
        let mut hasher = DefaultHasher::new();
        match command.as_deref() {
            Some(c) => c.hash(&mut hasher),
            None => raw.hash(&mut hasher),
        }
        let detail = command
            .and_then(|c| short(&c, DETAIL_MAX))
            .unwrap_or_default();
        Some(Self {
            kind,
            at,
            session: hooked.session_id,
            turn: hooked.turn_id,
            agent: hooked.agent_id,
            tool: hooked
                .tool_name
                .and_then(|t| short(&t, TOOL_MAX))
                .unwrap_or_default(),
            call: hooked.tool_use_id,
            input: if raw.is_some() { hasher.finish() } else { 0 },
            detail,
        })
    }

    /// Where the event falls in a tool call's life, to order files that carry one time.
    fn place(&self) -> u8 {
        match self.kind {
            HookEvent::UserPromptSubmit => 0,
            HookEvent::PreToolUse => 1,
            HookEvent::PermissionRequest => 2,
            HookEvent::PostToolUse => 3,
            _ => 4,
        }
    }
}

/// A `PreToolUse` the session has not seen finish.
struct Call {
    id: String,
    turn: Option<String>,
    tool: String,
    input: u64,
    agent: Option<String>,
}

#[derive(Default)]
struct Session {
    open: Vec<Asked>,
    calls: Vec<Call>,
}

/// Drop the oldest until at most `KEPT` are left.
fn cap<T>(list: &mut Vec<T>) {
    if list.len() > KEPT {
        list.drain(..list.len() - KEPT);
    }
}

impl Session {
    fn apply(&mut self, ev: Event, seq: &mut u64) {
        if ev.agent.is_none() && ev.turn.is_some() {
            // The root agent moved to another turn: what was open in an earlier one is over.
            self.open.retain(|a| {
                a.agent_id.is_some() || a.turn_id.is_none() || a.turn_id == ev.turn || a.at > ev.at
            });
            self.calls.retain(|c| c.turn.is_none() || c.turn == ev.turn);
        }
        match ev.kind {
            HookEvent::PreToolUse => {
                if let Some(id) = ev.call {
                    self.calls.push(Call {
                        id,
                        turn: ev.turn,
                        tool: ev.tool,
                        input: ev.input,
                        agent: ev.agent,
                    });
                    cap(&mut self.calls);
                }
            }
            HookEvent::PermissionRequest => {
                let call_id = self
                    .calls
                    .iter()
                    .rev()
                    .find(|c| c.tool == ev.tool && c.input == ev.input && c.agent == ev.agent)
                    .map(|c| c.id.clone());
                *seq += 1;
                self.open.push(Asked {
                    seq: *seq,
                    turn_id: ev.turn,
                    call_id,
                    agent_id: ev.agent,
                    tool: ev.tool,
                    detail: ev.detail,
                    at: ev.at,
                    input: ev.input,
                });
                cap(&mut self.open);
            }
            HookEvent::PostToolUse => {
                if let Some(id) = &ev.call {
                    self.calls.retain(|c| &c.id != id);
                }
                self.open.retain(|a| {
                    a.at > ev.at
                        || match &a.call_id {
                            Some(call) => ev.call.as_ref() != Some(call),
                            None => {
                                a.tool != ev.tool || a.input != ev.input || a.agent_id != ev.agent
                            }
                        }
                });
            }
            HookEvent::Stop => {
                self.open.retain(|a| a.at > ev.at);
                self.calls.clear();
            }
            _ => {}
        }
    }
}

/// The permission requests no hook has seen settled, by session id.
#[derive(Default)]
pub struct Requests {
    sessions: BTreeMap<String, Session>,
    taken: Vec<Event>,
    seq: u64,
}

impl Requests {
    /// Take in one hook file. Nothing is applied until `settle`.
    pub(super) fn note(&mut self, event: HookEvent, body: &str, at: SystemTime) {
        if matches!(
            event,
            HookEvent::UserPromptSubmit
                | HookEvent::PreToolUse
                | HookEvent::PermissionRequest
                | HookEvent::PostToolUse
                | HookEvent::Stop
        ) && let Some(ev) = Event::read(event, body, at)
        {
            self.taken.push(ev);
        }
    }

    /// Apply what `note` took in since the last call, in the order it happened.
    pub(super) fn settle(&mut self) {
        let mut taken = std::mem::take(&mut self.taken);
        taken.sort_by_key(|e| (e.at, e.place()));
        for ev in taken {
            let session = self.sessions.entry(ev.session.clone()).or_default();
            session.apply(ev, &mut self.seq);
        }
        self.prune();
    }

    /// The open requests of `session_id`, oldest first.
    #[must_use]
    pub fn open(&self, session_id: &str) -> &[Asked] {
        self.sessions
            .get(session_id)
            .map_or(&[], |s| s.open.as_slice())
    }

    /// Forget the request `seq` of `session_id`: something else proved it is over.
    pub fn retire(&mut self, session_id: &str, seq: u64) {
        if let Some(s) = self.sessions.get_mut(session_id) {
            s.open.retain(|a| a.seq != seq);
        }
        self.prune();
    }

    /// Forget every request `gone` is true for.
    pub fn forget(&mut self, gone: impl Fn(&str, &Asked) -> bool) {
        for (id, s) in &mut self.sessions {
            s.open.retain(|a| !gone(id, a));
        }
        self.prune();
    }

    /// Forget the remembered tool calls of every session `live` is false for: nothing will
    /// finish them.
    pub fn forget_calls(&mut self, live: impl Fn(&str) -> bool) {
        for (id, s) in &mut self.sessions {
            if !live(id) {
                s.calls.clear();
            }
        }
        self.prune();
    }

    fn prune(&mut self) {
        self.sessions
            .retain(|_, s| !s.open.is_empty() || !s.calls.is_empty());
    }
}

#[cfg(test)]
mod tests;
