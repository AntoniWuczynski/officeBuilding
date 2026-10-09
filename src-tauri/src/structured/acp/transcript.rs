//! What the agent's messages mean for the desk: transcript entries, the desk's
//! state and open approvals. Kept apart from the process so recorded lines can
//! be fed straight in by the tests.

use super::cursor;
use super::wire::{
    self, CancelRequest, ContentBlock, Id, PermissionKind, PermissionOutcome, PermissionRequest,
    PermissionResult, Plan, PlanStatus, RpcError, SessionNotification, SessionUpdate, StopReason,
    ToolCallContent, ToolCallFields, ToolCallStatus,
};
use crate::model::SessionState;
use crate::structured::{Approval, Choice, Entry, Event, ToolStatus, Values};
use serde_json::value::RawValue;
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Voice {
    User,
    Agent,
    Thought,
}

/// The message being streamed: chunks join it while they keep its voice and
/// message id and nothing else has been added in between.
struct Stream {
    voice: Voice,
    message_id: Option<String>,
    entry_id: String,
    text: String,
}

struct Tool {
    title: String,
    status: ToolStatus,
    detail: String,
}

struct Open {
    approval_id: String,
    rpc: Id,
    kind: OpenKind,
}

/// What an open approval answers: a permission request (by its option ids),
/// or one of Cursor's blocking requests.
enum OpenKind {
    Permission(Vec<String>),
    Cursor(cursor::Pending),
}

/// The reply for an approval the human did not get to answer.
fn cancelled(o: &Open) -> Result<String, String> {
    match &o.kind {
        OpenKind::Permission(_) => wire::result(
            &o.rpc,
            PermissionResult {
                outcome: PermissionOutcome::Cancelled,
            },
        ),
        OpenKind::Cursor(pending) => {
            raw_result(&o.rpc, cursor::reply(pending, "cancel", &Values::new()))
        }
    }
}

/// A reply whose `result` is already written as JSON.
fn raw_result(rpc: &Id, result: Result<String, String>) -> Result<String, String> {
    let raw = RawValue::from_string(result?)
        .map_err(|e| format!("the answer could not be written: {e}"))?;
    wire::result(rpc, raw)
}

/// Events for the desk, and the reply line to send back, if one is due now.
pub(super) type Handled = (Vec<Event>, Option<Result<String, String>>);

/// A permission answer ready to go back to the agent.
pub(super) struct Answer {
    pub reply: String,
    pub events: Vec<Event>,
}

#[derive(Default)]
pub(super) struct Transcript {
    next: u64,
    stream: Option<Stream>,
    tools: HashMap<String, Tool>,
    plan_id: Option<String>,
    open: Vec<Open>,
    turn: bool,
    /// `session/load` is replaying an old conversation: the human's own
    /// messages come back as chunks then, and only then are they shown (a live
    /// turn's message is shown when it is sent).
    pub(super) replaying: bool,
}

impl Transcript {
    fn fresh_id(&mut self, prefix: &str) -> String {
        self.next += 1;
        format!("{prefix}-{}", self.next)
    }

    pub(super) fn turn_running(&self) -> bool {
        self.turn
    }

    /// The human's message starts a turn.
    pub(super) fn user(&mut self, text: &str) -> Vec<Event> {
        self.stream = None;
        self.turn = true;
        let id = self.fresh_id("user");
        vec![
            Event::Entry(Entry::User {
                id,
                text: text.to_string(),
            }),
            Event::State(SessionState::Thinking),
        ]
    }

    pub(super) fn notice(&mut self, text: String) -> Event {
        self.stream = None;
        let id = self.fresh_id("notice");
        Event::Entry(Entry::Notice { id, text })
    }

    /// `session/prompt` came back (or could not be sent): the turn is over.
    pub(super) fn turn_ended(&mut self, reply: Result<StopReason, String>) -> Vec<Event> {
        self.turn = false;
        let mut events: Vec<Event> = self
            .open
            .drain(..)
            .map(|o| Event::Resolved {
                approval_id: o.approval_id,
            })
            .collect();
        let note = match reply {
            Ok(StopReason::EndTurn | StopReason::Cancelled) => None,
            Ok(StopReason::MaxTokens) => {
                Some("The agent stopped: it reached its token limit.".to_string())
            }
            Ok(StopReason::MaxTurnRequests) => {
                Some("The agent stopped: it reached its limit of steps for one turn.".to_string())
            }
            Ok(StopReason::Refusal) => Some("The agent refused to go on.".to_string()),
            Err(text) => Some(text),
        };
        events.extend(note.map(|n| self.notice(n)));
        events.push(Event::State(SessionState::Idle));
        events
    }

    /// An agent→client notification. Returns the events and, for a withdrawn
    /// request, the reply that closes it.
    pub(super) fn on_notification(&mut self, method: &str, params: Option<&RawValue>) -> Handled {
        match method {
            "session/update" => (
                wire::body::<SessionNotification>(params)
                    .and_then(|n| n.update())
                    .map(|u| self.update(u))
                    .unwrap_or_default(),
                None,
            ),
            "$/cancel_request" => wire::body::<CancelRequest>(params)
                .map(|c| self.withdraw(&c.request_id))
                .unwrap_or_default(),
            _ => match params.and_then(|p| cursor::note(method, p.get())) {
                Some(text) => (vec![self.notice(text)], None),
                None => (Vec::new(), None),
            },
        }
    }

    /// An agent→client request. Returns the events and, unless the human has
    /// to answer first, the reply line.
    pub(super) fn on_request(
        &mut self,
        id: Id,
        method: &str,
        params: Option<&RawValue>,
    ) -> Handled {
        if method == "session/request_permission"
            && let Some(req) = wire::body::<PermissionRequest>(params)
        {
            return (self.permission(id, req), None);
        }
        if let Some(ask) = params.and_then(|p| cursor::ask(method, p.get())) {
            return (self.cursor_ask(id, ask), None);
        }
        let reply = wire::error(
            &id,
            wire::METHOD_NOT_FOUND,
            &format!("Office Building does not provide {method}"),
        );
        (Vec::new(), Some(reply))
    }

    fn update(&mut self, update: SessionUpdate) -> Vec<Event> {
        match update {
            SessionUpdate::UserMessageChunk(c) if self.replaying => {
                self.chunk(Voice::User, c.message_id, &c.content)
            }
            SessionUpdate::AgentMessageChunk(c) => {
                self.chunk(Voice::Agent, c.message_id, &c.content)
            }
            SessionUpdate::AgentThoughtChunk(c) => {
                self.chunk(Voice::Thought, c.message_id, &c.content)
            }
            SessionUpdate::ToolCall(t) => vec![self.tool(&t)],
            SessionUpdate::Plan(p) => vec![self.plan(&p)],
            SessionUpdate::UserMessageChunk(_) | SessionUpdate::Other => Vec::new(),
        }
    }

    fn chunk(
        &mut self,
        voice: Voice,
        message_id: Option<String>,
        content: &ContentBlock,
    ) -> Vec<Event> {
        let Some(piece) = block_text(content) else {
            return Vec::new();
        };
        let joins = self
            .stream
            .as_ref()
            .is_some_and(|s| s.voice == voice && s.message_id == message_id);
        if !joins {
            let entry_id = self.fresh_id(match voice {
                Voice::User => "user",
                Voice::Agent => "agent",
                Voice::Thought => "thought",
            });
            self.stream = Some(Stream {
                voice,
                message_id,
                entry_id,
                text: String::new(),
            });
        }
        let Some(s) = self.stream.as_mut() else {
            return Vec::new();
        };
        s.text.push_str(&piece);
        let (id, text) = (s.entry_id.clone(), s.text.clone());
        vec![Event::Entry(match voice {
            Voice::User => Entry::User { id, text },
            Voice::Agent => Entry::Agent { id, text },
            Voice::Thought => Entry::Thought { id, text },
        })]
    }

    /// Fold a tool call or update into what is known of it.
    fn tool(&mut self, t: &ToolCallFields) -> Event {
        self.stream = None;
        let tool = self
            .tools
            .entry(t.tool_call_id.clone())
            .or_insert_with(|| Tool {
                title: "Tool call".to_string(),
                status: ToolStatus::Pending,
                detail: String::new(),
            });
        if let Some(title) = &t.title {
            tool.title.clone_from(title);
        }
        if let Some(status) = t.status {
            tool.status = tool_status(status);
        }
        if let Some(content) = &t.content {
            tool.detail = tool_detail(content);
        }
        tool_entry(&t.tool_call_id, tool)
    }

    fn plan(&mut self, plan: &Plan) -> Event {
        self.stream = None;
        if self.plan_id.is_none() {
            self.plan_id = Some(self.fresh_id("plan"));
        }
        let id = self.plan_id.clone().unwrap_or_default();
        let mut text = String::from("Plan:");
        for e in &plan.entries {
            let mark = match e.status {
                PlanStatus::Completed => "- [x] ",
                PlanStatus::InProgress => "- [ ] (in progress) ",
                PlanStatus::Pending => "- [ ] ",
            };
            text.push('\n');
            text.push_str(mark);
            text.push_str(&e.content);
        }
        Event::Entry(Entry::Notice { id, text })
    }

    fn permission(&mut self, rpc: Id, req: PermissionRequest) -> Vec<Event> {
        let tool = self.tool(&req.tool_call);
        let (title, mut detail) = match &tool {
            Event::Entry(Entry::Tool { title, detail, .. }) => (title.clone(), detail.clone()),
            _ => (String::new(), String::new()),
        };
        if detail.is_empty()
            && let Some(raw) = &req.tool_call.raw_input
        {
            detail = raw.get().to_string();
        }
        let approval_id = self.fresh_id("approval");
        let choices = req
            .options
            .iter()
            .map(|o| Choice {
                id: o.option_id.clone(),
                label: o.name.clone(),
                allow: matches!(
                    o.kind,
                    PermissionKind::AllowOnce | PermissionKind::AllowAlways
                ),
            })
            .collect();
        self.open.push(Open {
            approval_id: approval_id.clone(),
            rpc,
            kind: OpenKind::Permission(req.options.into_iter().map(|o| o.option_id).collect()),
        });
        vec![
            tool,
            Event::Approval(Approval {
                id: approval_id,
                title,
                detail,
                fields: Vec::new(),
                choices,
            }),
            Event::State(SessionState::WaitingHuman),
        ]
    }

    /// One of Cursor's blocking requests, waiting on the human.
    fn cursor_ask(&mut self, rpc: Id, ask: cursor::Ask) -> Vec<Event> {
        self.stream = None;
        let approval_id = self.fresh_id("approval");
        self.open.push(Open {
            approval_id: approval_id.clone(),
            rpc,
            kind: OpenKind::Cursor(ask.pending),
        });
        vec![
            Event::Approval(Approval {
                id: approval_id,
                title: ask.title,
                detail: ask.detail,
                fields: ask.fields,
                choices: ask.choices,
            }),
            Event::State(SessionState::WaitingHuman),
        ]
    }

    pub(super) fn answer(
        &mut self,
        approval_id: &str,
        choice_id: &str,
        values: &Values,
    ) -> Result<Answer, String> {
        let at = self
            .open
            .iter()
            .position(|o| o.approval_id == approval_id)
            .ok_or_else(|| "that approval is no longer open".to_string())?;
        let reply = match &self.open[at].kind {
            OpenKind::Permission(options) => {
                if !options.iter().any(|o| o == choice_id) {
                    return Err("that is not one of the approval's answers".to_string());
                }
                wire::result(
                    &self.open[at].rpc,
                    PermissionResult {
                        outcome: PermissionOutcome::Selected {
                            option_id: choice_id,
                        },
                    },
                )?
            }
            OpenKind::Cursor(pending) => raw_result(
                &self.open[at].rpc,
                cursor::reply(pending, choice_id, values),
            )?,
        };
        let open = self.open.remove(at);
        let mut events = vec![Event::Resolved {
            approval_id: open.approval_id,
        }];
        events.extend(self.state_after_approval());
        Ok(Answer { reply, events })
    }

    /// The human cancelled the turn: every open approval is answered
    /// `cancelled` (the spec's MUST) and unfinished tool calls are marked failed
    /// (its SHOULD). Returns the reply lines and the events.
    pub(super) fn cancel(&mut self) -> (Vec<Result<String, String>>, Vec<Event>) {
        let mut replies = Vec::new();
        let mut events = Vec::new();
        for o in self.open.drain(..) {
            replies.push(cancelled(&o));
            events.push(Event::Resolved {
                approval_id: o.approval_id,
            });
        }
        for (id, tool) in &mut self.tools {
            if matches!(tool.status, ToolStatus::Pending | ToolStatus::Running) {
                tool.status = ToolStatus::Failed;
                events.push(tool_entry(id, tool));
            }
        }
        if self.turn {
            events.push(Event::State(SessionState::Thinking));
        }
        (replies, events)
    }

    /// The agent withdrew a request (`$/cancel_request`); it is answered with
    /// the cancelled error, as JSON-RPC asks of the side that received it.
    fn withdraw(&mut self, rpc: &Id) -> Handled {
        let Some(at) = self.open.iter().position(|o| &o.rpc == rpc) else {
            return (Vec::new(), None);
        };
        let open = self.open.remove(at);
        let mut events = vec![Event::Resolved {
            approval_id: open.approval_id,
        }];
        events.extend(self.state_after_approval());
        let reply = wire::error(&open.rpc, wire::REQUEST_CANCELLED, "withdrawn by the agent");
        (events, Some(reply))
    }

    fn state_after_approval(&self) -> Option<Event> {
        if !self.open.is_empty() {
            Some(Event::State(SessionState::WaitingHuman))
        } else if self.turn {
            Some(Event::State(SessionState::Thinking))
        } else {
            None
        }
    }
}

/// An error from the agent, phrased for the human. Sign-in is the tool's own
/// business: the building never handles credentials.
pub(super) fn error_text(e: &RpcError) -> String {
    if e.code == wire::AUTH_REQUIRED {
        format!(
            "The agent needs you to sign in. Sign in with the tool itself in a terminal, then hire it again. ({})",
            e.text()
        )
    } else {
        format!("The agent reported an error: {}", e.text())
    }
}

fn tool_entry(id: &str, tool: &Tool) -> Event {
    Event::Entry(Entry::Tool {
        id: format!("tool-{id}"),
        title: tool.title.clone(),
        status: tool.status,
        detail: tool.detail.clone(),
    })
}

fn tool_status(s: ToolCallStatus) -> ToolStatus {
    match s {
        ToolCallStatus::Pending => ToolStatus::Pending,
        ToolCallStatus::InProgress => ToolStatus::Running,
        ToolCallStatus::Completed => ToolStatus::Done,
        ToolCallStatus::Failed => ToolStatus::Failed,
    }
}

fn block_text(b: &ContentBlock) -> Option<String> {
    match b {
        ContentBlock::Text { text } => Some(text.clone()),
        ContentBlock::ResourceLink { name, uri } => Some(format!("[{name}]({uri})")),
        ContentBlock::Other => None,
    }
}

/// A tool call's content as text: output as written, diffs as `-`/`+` lines.
fn tool_detail(content: &[ToolCallContent]) -> String {
    content
        .iter()
        .filter_map(|c| match c {
            ToolCallContent::Content { content } => block_text(content),
            ToolCallContent::Diff {
                path,
                old_text,
                new_text,
            } => Some(diff(path, old_text.as_deref().unwrap_or(""), new_text)),
            ToolCallContent::Other => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The changed middle of a file: lines shared at the start and end are left out.
fn diff(path: &str, old: &str, new: &str) -> String {
    let old: Vec<&str> = old.lines().collect();
    let new: Vec<&str> = new.lines().collect();
    let head = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let tail = old[head..]
        .iter()
        .rev()
        .zip(new[head..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let mut out = format!("--- {path}\n+++ {path}");
    for line in &old[head..old.len() - tail] {
        out.push_str("\n-");
        out.push_str(line);
    }
    for line in &new[head..new.len() - tail] {
        out.push_str("\n+");
        out.push_str(line);
    }
    out
}
