//! Codex's messages in, the building's events and Codex's next messages out.
//! No process here: the driver feeds it lines and delivers what it returns, so
//! the tests can replay recorded sessions.

use super::ask::{self, Ask, Fill, FormKind, lines};
use super::wire::{Answer, FormValue};
use super::wire::{
    Capabilities, ClientInfo, CommandDecision, DeltaNote, ElicitAction, ErrorNote, ErrorReply,
    FileApprovalParams, FileUpdateChange, Incoming, InitializeParams, ItemNote, ItemStatus,
    McpContent, McpProgressNote, McpResult, Notification, PatchKind, PatchUpdatedNote, PlanNote,
    Reply, ReplyBody, Request, RequestId, ResolvedNote, ReviewDecision, RpcError, StepStatus,
    SummaryDeltaNote, SummaryPartNote, TextInput, ThreadItem, ThreadResumeParams,
    ThreadResumeResult, ThreadStartParams, ThreadStartResult, TurnInterruptParams, TurnNote,
    TurnStartParams, TurnStartResult, TurnStatus, TurnSteerParams, UserInput, WarningNote,
};
use crate::model::SessionState;
use crate::structured::{Approval, Choice, Entry, Event, ToolStatus, Values};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::collections::{HashMap, VecDeque};

/// JSON-RPC's "method not found", for server requests the building does not serve.
const METHOD_NOT_FOUND: i64 = -32601;
/// JSON-RPC's "invalid params", for a request whose params did not parse.
const INVALID_PARAMS: i64 = -32602;

/// One thing for the driver to do, in order.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Out {
    Event(Event),
    /// A line for Codex's stdin (without the newline).
    Send(String),
}

/// A message the human sent, by the id its User entry carries.
#[derive(Debug, Clone)]
struct Prompt {
    client_id: String,
    text: String,
}

/// Our requests still waiting for Codex's response.
enum Call {
    Initialize,
    ThreadStart,
    ThreadResume,
    TurnStart(Prompt),
    Steer(Prompt),
    Interrupt,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TurnState {
    Idle,
    /// `turn/start` sent, its id not known yet.
    Starting,
    Active(String),
}

/// What a tool entry shows, kept so deltas and approvals can update it.
struct ToolView {
    title: String,
    status: ToolStatus,
    detail: String,
}

/// An approval the human has not answered: the request to answer and, per
/// choice (by index), the reply it sends.
struct Open {
    request: RequestId,
    item: Option<String>,
    replies: Vec<ReplyBody>,
    fill: Option<Fill>,
    fields: Vec<crate::structured::Field>,
}

pub(super) struct Mapper {
    cwd: String,
    model: Option<String>,
    /// A thread to reopen instead of starting one.
    resume: Option<String>,
    effort: Option<String>,
    next_request: i64,
    calls: HashMap<i64, Call>,
    thread: Option<String>,
    turn: TurnState,
    /// Prompts waiting for the thread, or for the turn they will steer.
    queued: VecDeque<Prompt>,
    next_user: u64,
    next_notice: u64,
    /// Whether this turn's failure was already shown by an `error` notification.
    turn_error_shown: bool,
    texts: HashMap<String, String>,
    thoughts: HashMap<String, Vec<String>>,
    tools: HashMap<String, ToolView>,
    approvals: HashMap<String, Open>,
}

impl Mapper {
    pub(super) fn new(
        cwd: &str,
        model: Option<&str>,
        effort: Option<&str>,
        resume: Option<&str>,
    ) -> Self {
        Self {
            cwd: cwd.to_string(),
            model: model.map(str::to_string),
            resume: resume.map(str::to_string),
            effort: effort.map(str::to_string),
            next_request: 0,
            calls: HashMap::new(),
            thread: None,
            turn: TurnState::Idle,
            queued: VecDeque::new(),
            next_user: 0,
            next_notice: 0,
            turn_error_shown: false,
            texts: HashMap::new(),
            thoughts: HashMap::new(),
            tools: HashMap::new(),
            approvals: HashMap::new(),
        }
    }

    /// The handshake, and the first task when there is one.
    pub(super) fn begin(&mut self, task: &str) -> Vec<Out> {
        let mut out = vec![self.request(
            "initialize",
            &InitializeParams {
                client_info: ClientInfo {
                    name: "office_building",
                    title: "Office Building",
                    version: env!("CARGO_PKG_VERSION"),
                },
                capabilities: Capabilities {
                    experimental_api: false,
                    request_attestation: false,
                },
            },
            Call::Initialize,
        )];
        if !task.is_empty() {
            out.extend(self.prompt(task));
        }
        out
    }

    /// The human's message: a new turn, or more input for the running one.
    pub(super) fn prompt(&mut self, text: &str) -> Vec<Out> {
        self.next_user += 1;
        let prompt = Prompt {
            client_id: format!("user-{}", self.next_user),
            text: text.to_string(),
        };
        let mut out = vec![Out::Event(Event::Entry(Entry::User {
            id: prompt.client_id.clone(),
            text: prompt.text.clone(),
        }))];
        match (&self.thread, &self.turn) {
            (Some(_), TurnState::Idle) => out.extend(self.start_turn(prompt)),
            (Some(_), TurnState::Active(turn)) => {
                let turn = turn.clone();
                out.push(self.steer(&prompt, &turn));
            }
            _ => self.queued.push_back(prompt),
        }
        out.push(self.state());
        out
    }

    /// Stop the running turn. A turn whose id has not arrived yet cannot be named, so it runs on.
    pub(super) fn cancel(&mut self) -> Vec<Out> {
        let (Some(thread), TurnState::Active(turn)) = (self.thread.clone(), self.turn.clone())
        else {
            return Vec::new();
        };
        vec![self.request(
            "turn/interrupt",
            &TurnInterruptParams {
                thread_id: &thread,
                turn_id: &turn,
            },
            Call::Interrupt,
        )]
    }

    pub(super) fn answer(
        &mut self,
        approval_id: &str,
        choice_id: &str,
        values: &Values,
    ) -> Result<Vec<Out>, String> {
        let open = self
            .approvals
            .get(approval_id)
            .ok_or("that approval is no longer open")?;
        let index: usize = choice_id
            .parse()
            .map_err(|_| format!("{choice_id} is not one of this approval's choices"))?;
        let reply = open
            .replies
            .get(index)
            .ok_or_else(|| format!("{choice_id} is not one of this approval's choices"))?;
        let reply = &filled(reply, open.fill.as_ref(), &open.fields, values)?;
        let allow = !matches!(
            reply,
            ReplyBody::Decision {
                decision: CommandDecision::Decline | CommandDecision::Cancel,
            } | ReplyBody::Review {
                decision: ReviewDecision::Denied { .. } | ReviewDecision::Abort,
            } | ReplyBody::Elicit {
                action: ElicitAction::Decline | ElicitAction::Cancel,
                ..
            }
        );
        let mut out = vec![line(&Reply {
            id: &open.request,
            result: reply,
        })];
        let item = open.item.clone();
        self.approvals.remove(approval_id);
        out.push(Out::Event(Event::Resolved {
            approval_id: approval_id.to_string(),
        }));
        if allow && let Some(item) = item {
            out.extend(self.set_tool_status(&item, ToolStatus::Running));
        }
        out.push(self.state());
        Ok(out)
    }

    /// One line from Codex's stdout. Anything that is not a protocol message
    /// (a login shell's greeting) is skipped.
    pub(super) fn line(&mut self, line: &str) -> Vec<Out> {
        let Ok(msg) = serde_json::from_str::<Incoming>(line) else {
            return Vec::new();
        };
        let params = msg.params.as_deref().map_or("null", |p| p.get());
        match (msg.id, msg.method) {
            (Some(id), Some(method)) => self.server_request(&id, &method, params),
            (None, Some(method)) => self.notification(&method, params),
            (Some(RequestId::Number(id)), None) => match self.calls.remove(&id) {
                Some(call) => match msg.error {
                    Some(error) => self.failed(call, &error.message),
                    None => self.responded(call, msg.result.as_deref().map_or("null", |r| r.get())),
                },
                None => Vec::new(),
            },
            _ => Vec::new(),
        }
    }

    // ---- Our requests and their responses ------------------------------

    fn request<P: Serialize>(&mut self, method: &str, params: &P, call: Call) -> Out {
        self.next_request += 1;
        self.calls.insert(self.next_request, call);
        line(&Request {
            id: self.next_request,
            method,
            params,
        })
    }

    fn start_turn(&mut self, prompt: Prompt) -> Vec<Out> {
        let Some(thread) = self.thread.clone() else {
            self.queued.push_front(prompt);
            return Vec::new();
        };
        self.turn = TurnState::Starting;
        self.turn_error_shown = false;
        let effort = self.effort.clone();
        let params = TurnStartParams {
            thread_id: &thread,
            input: [TextInput {
                text: &prompt.text,
                text_elements: [],
            }],
            client_user_message_id: &prompt.client_id,
            effort: effort.as_deref(),
        };
        vec![self.request("turn/start", &params, Call::TurnStart(prompt.clone()))]
    }

    fn steer(&mut self, prompt: &Prompt, turn: &str) -> Out {
        let thread = self.thread.clone().unwrap_or_default();
        let params = TurnSteerParams {
            thread_id: &thread,
            input: [TextInput {
                text: &prompt.text,
                text_elements: [],
            }],
            client_user_message_id: &prompt.client_id,
            expected_turn_id: turn,
        };
        self.request("turn/steer", &params, Call::Steer(prompt.clone()))
    }

    fn responded(&mut self, call: Call, result: &str) -> Vec<Out> {
        match call {
            Call::Initialize => {
                let cwd = self.cwd.clone();
                let model = self.model.clone();
                let open = match self.resume.clone() {
                    Some(thread_id) => self.request(
                        "thread/resume",
                        &ThreadResumeParams {
                            thread_id: &thread_id,
                            cwd: &cwd,
                            model: model.as_deref(),
                        },
                        Call::ThreadResume,
                    ),
                    None => self.request(
                        "thread/start",
                        &ThreadStartParams {
                            cwd: &cwd,
                            model: model.as_deref(),
                        },
                        Call::ThreadStart,
                    ),
                };
                vec![
                    line(&Notification {
                        method: "initialized",
                    }),
                    open,
                ]
            }
            Call::ThreadResume => match serde_json::from_str::<ThreadResumeResult>(result) {
                Ok(r) => {
                    self.thread = Some(r.thread.id.clone());
                    let mut out = vec![Out::Event(Event::SessionId(r.thread.id))];
                    // The conversation so far, as finished items.
                    for item in r.thread.turns.into_iter().flat_map(|t| t.items) {
                        out.extend(self.item(item));
                    }
                    if let Some(first) = self.queued.pop_front() {
                        out.extend(self.start_turn(first));
                    }
                    out.push(self.state());
                    out
                }
                Err(e) => self.failed(Call::ThreadResume, &e.to_string()),
            },
            Call::ThreadStart => match serde_json::from_str::<ThreadStartResult>(result) {
                Ok(r) => {
                    self.thread = Some(r.thread.id.clone());
                    let mut out = vec![Out::Event(Event::SessionId(r.thread.id))];
                    if let Some(first) = self.queued.pop_front() {
                        out.extend(self.start_turn(first));
                    }
                    out.push(self.state());
                    out
                }
                Err(e) => self.failed(Call::ThreadStart, &e.to_string()),
            },
            Call::TurnStart(prompt) => match serde_json::from_str::<TurnStartResult>(result) {
                Ok(r) => self.turn_began(&r.turn.id),
                Err(e) => self.failed(Call::TurnStart(prompt), &e.to_string()),
            },
            Call::Steer(_) | Call::Interrupt => Vec::new(),
        }
    }

    fn failed(&mut self, call: Call, message: &str) -> Vec<Out> {
        match call {
            Call::ThreadResume => vec![
                self.notice(format!(
                    "Codex could not reopen the conversation: {message}"
                )),
                Out::Event(Event::State(SessionState::Error)),
            ],
            Call::Initialize | Call::ThreadStart => vec![
                self.notice(format!("Codex did not start a conversation: {message}")),
                Out::Event(Event::State(SessionState::Error)),
            ],
            Call::TurnStart(_) => {
                self.turn = TurnState::Idle;
                vec![
                    self.notice(format!("Codex did not take that message: {message}")),
                    self.state(),
                ]
            }
            // The turn ended as the message went out: send it as the next turn.
            Call::Steer(prompt) if self.turn == TurnState::Idle => self.start_turn(prompt),
            Call::Steer(_) => {
                vec![self.notice(format!("Codex did not take that message: {message}"))]
            }
            Call::Interrupt => vec![self.notice(format!("Codex did not stop the turn: {message}"))],
        }
    }

    fn turn_began(&mut self, id: &str) -> Vec<Out> {
        if matches!(&self.turn, TurnState::Active(t) if t == id) {
            return Vec::new();
        }
        self.turn = TurnState::Active(id.to_string());
        let mut out = Vec::new();
        while let Some(prompt) = self.queued.pop_front() {
            out.push(self.steer(&prompt, id));
        }
        out.push(self.state());
        out
    }

    // ---- Notifications --------------------------------------------------

    fn notification(&mut self, method: &str, params: &str) -> Vec<Out> {
        match method {
            "turn/started" => parse(params, |n: TurnNote| self.turn_began(&n.turn.id)),
            "turn/completed" => parse(params, |n: TurnNote| self.turn_ended(&n)),
            "item/started" | "item/completed" => parse(params, |n: ItemNote| self.item(n.item)),
            "item/agentMessage/delta" => parse(params, |n: DeltaNote| {
                let text = self.texts.entry(n.item_id.clone()).or_default();
                text.push_str(&n.delta);
                vec![Out::Event(Event::Entry(Entry::Agent {
                    id: n.item_id,
                    text: text.clone(),
                }))]
            }),
            "item/reasoning/summaryPartAdded" => parse(params, |n: SummaryPartNote| {
                summary_part(self.thoughts.entry(n.item_id).or_default(), n.summary_index);
                Vec::new()
            }),
            "item/reasoning/summaryTextDelta" => parse(params, |n: SummaryDeltaNote| {
                let parts = self.thoughts.entry(n.item_id.clone()).or_default();
                summary_part(parts, n.summary_index).push_str(&n.delta);
                thought(n.item_id, parts)
            }),
            "item/commandExecution/outputDelta" => parse(params, |n: DeltaNote| {
                self.tool_update(&n.item_id, |t| t.detail.push_str(&n.delta))
            }),
            "item/fileChange/patchUpdated" => parse(params, |n: PatchUpdatedNote| {
                let (title, detail) = patch_view(&n.changes);
                self.tool_update(&n.item_id, |t| {
                    t.title = title;
                    t.detail = detail;
                })
            }),
            "item/mcpToolCall/progress" => parse(params, |n: McpProgressNote| {
                self.tool_update(&n.item_id, |t| t.detail = n.message)
            }),
            "turn/plan/updated" => parse(params, |n: PlanNote| {
                vec![Out::Event(Event::Entry(Entry::Notice {
                    id: format!("plan-{}", n.turn_id),
                    text: plan_text(&n),
                }))]
            }),
            "serverRequest/resolved" => parse(params, |n: ResolvedNote| {
                let id = n.request_id.to_string();
                if self.approvals.remove(&id).is_none() {
                    return Vec::new();
                }
                vec![
                    Out::Event(Event::Resolved { approval_id: id }),
                    self.state(),
                ]
            }),
            "error" => parse(params, |n: ErrorNote| {
                let mut text = error_text(&n.error.message, n.error.additional_details.as_deref());
                if n.will_retry {
                    text.push_str(" (Codex is retrying)");
                } else {
                    self.turn_error_shown = true;
                }
                vec![self.notice(text)]
            }),
            "warning" => parse(params, |n: WarningNote| vec![self.notice(n.message)]),
            _ => Vec::new(),
        }
    }

    fn turn_ended(&mut self, n: &TurnNote) -> Vec<Out> {
        self.turn = TurnState::Idle;
        let mut out = Vec::new();
        match n.turn.status {
            TurnStatus::Failed => {
                if !self.turn_error_shown {
                    let text = n.turn.error.as_ref().map_or_else(
                        || "The turn failed.".to_string(),
                        |e| error_text(&e.message, e.additional_details.as_deref()),
                    );
                    out.push(self.notice(text));
                }
                out.push(Out::Event(Event::State(SessionState::Error)));
            }
            TurnStatus::Interrupted => {
                out.push(Out::Event(Event::Entry(Entry::Notice {
                    id: format!("stopped-{}", n.turn.id),
                    text: "Stopped.".to_string(),
                })));
                out.push(self.state());
            }
            TurnStatus::Completed | TurnStatus::InProgress => out.push(self.state()),
        }
        if let Some(next) = self.queued.pop_front() {
            out.extend(self.start_turn(next));
            out.push(self.state());
        }
        out
    }

    fn item(&mut self, item: ThreadItem) -> Vec<Out> {
        let entry = match item {
            ThreadItem::UserMessage {
                id,
                client_id,
                content,
            } => Entry::User {
                id: client_id.unwrap_or(id),
                text: user_text(content),
            },
            ThreadItem::AgentMessage { id, text } => {
                // `item/started` carries no text yet; the deltas or the completed item fill it.
                if text.is_empty() {
                    return Vec::new();
                }
                self.texts.remove(&id);
                Entry::Agent { id, text }
            }
            ThreadItem::Reasoning { id, summary } => {
                self.thoughts.remove(&id);
                let text = summary.join("\n\n");
                if text.is_empty() {
                    return Vec::new();
                }
                Entry::Thought { id, text }
            }
            ThreadItem::CommandExecution {
                id,
                command,
                status,
                command_actions,
                aggregated_output,
                exit_code,
            } => {
                let streamed = self.tools.get(&id).map(|t| t.detail.clone());
                let mut detail = aggregated_output.or(streamed).unwrap_or_default();
                if let Some(code) = exit_code.filter(|c| *c != 0) {
                    if !detail.is_empty() && !detail.ends_with('\n') {
                        detail.push('\n');
                    }
                    detail.push_str("Exit code ");
                    detail.push_str(&code.to_string());
                }
                let title = ask::command_line(command_actions, command);
                return self.tool(&id, title, tool_status(status), detail);
            }
            ThreadItem::FileChange {
                id,
                changes,
                status,
            } => {
                let (title, detail) = patch_view(&changes);
                return self.tool(&id, title, tool_status(status), detail);
            }
            ThreadItem::McpToolCall {
                id,
                server,
                tool,
                status,
                result,
                error,
            } => {
                let detail = match (error, result) {
                    (Some(e), _) => e.message,
                    (None, Some(r)) => mcp_text(r),
                    (None, None) => self
                        .tools
                        .get(&id)
                        .map(|t| t.detail.clone())
                        .unwrap_or_default(),
                };
                return self.tool(
                    &id,
                    format!("{server}: {tool}"),
                    tool_status(status),
                    detail,
                );
            }
            ThreadItem::DynamicToolCall { id, tool, status } => {
                return self.tool(&id, tool, tool_status(status), String::new());
            }
            ThreadItem::WebSearch { id, query } => {
                let title = if query.is_empty() {
                    "Search the web".to_string()
                } else {
                    format!("Search the web: {query}")
                };
                return self.tool(&id, title, ToolStatus::Done, String::new());
            }
            ThreadItem::Plan { id, text } => Entry::Notice { id, text },
            ThreadItem::ContextCompaction { id } => Entry::Notice {
                id,
                text: "Codex compacted the conversation to fit its context window.".to_string(),
            },
            ThreadItem::Other => return Vec::new(),
        };
        vec![Out::Event(Event::Entry(entry))]
    }

    fn tool(&mut self, id: &str, title: String, status: ToolStatus, detail: String) -> Vec<Out> {
        // An approval waiting on this item keeps it pending until answered.
        let status = if status == ToolStatus::Running
            && self
                .approvals
                .values()
                .any(|a| a.item.as_deref() == Some(id))
        {
            ToolStatus::Pending
        } else {
            status
        };
        self.tools.insert(
            id.to_string(),
            ToolView {
                title,
                status,
                detail,
            },
        );
        self.tool_entry(id)
    }

    fn tool_update(&mut self, id: &str, f: impl FnOnce(&mut ToolView)) -> Vec<Out> {
        match self.tools.get_mut(id) {
            Some(view) => {
                f(view);
                self.tool_entry(id)
            }
            None => Vec::new(),
        }
    }

    fn set_tool_status(&mut self, id: &str, status: ToolStatus) -> Vec<Out> {
        self.tool_update(id, |t| t.status = status)
    }

    fn tool_entry(&self, id: &str) -> Vec<Out> {
        self.tools
            .get(id)
            .map(|t| {
                Out::Event(Event::Entry(Entry::Tool {
                    id: id.to_string(),
                    title: t.title.clone(),
                    status: t.status,
                    detail: t.detail.clone(),
                }))
            })
            .into_iter()
            .collect()
    }

    // ---- Requests from Codex --------------------------------------------

    fn server_request(&mut self, id: &RequestId, method: &str, params: &str) -> Vec<Out> {
        let asked = match method {
            "item/commandExecution/requestApproval" => {
                typed(params).map(|p| Some(ask::command_approval(p, &self.cwd)))
            }
            "item/fileChange/requestApproval" => typed(params).map(|p: FileApprovalParams| {
                let files = self.tools.get(&p.item_id).map(|t| t.detail.clone());
                Some(ask::file_approval(p, files))
            }),
            "item/permissions/requestApproval" => {
                typed(params).map(|p| Some(ask::permissions_approval(p)))
            }
            "item/tool/requestUserInput" => typed(params).map(ask::user_input),
            "mcpServer/elicitation/request" => ask::elicitation(params).map(Some),
            "execCommandApproval" => typed(params).map(|p| Some(ask::legacy_exec(p))),
            "applyPatchApproval" => typed(params).map(|p| Some(ask::legacy_patch(p))),
            _ => {
                return vec![line(&ErrorReply {
                    id,
                    error: RpcError {
                        code: METHOD_NOT_FOUND,
                        message: format!("Office Building does not handle {method}"),
                    },
                })];
            }
        };
        match asked {
            Ok(Some(ask)) => self.open(id, ask),
            Ok(None) => vec![line(&ErrorReply {
                id,
                error: RpcError {
                    code: METHOD_NOT_FOUND,
                    message: "Office Building cannot show this request".to_string(),
                },
            })],
            Err(e) => vec![
                self.notice(format!(
                    "Codex sent a {method} request the building could not read: {e}"
                )),
                line(&ErrorReply {
                    id,
                    error: RpcError {
                        code: INVALID_PARAMS,
                        message: e,
                    },
                }),
            ],
        }
    }

    fn open(&mut self, request: &RequestId, ask: Ask) -> Vec<Out> {
        let approval_id = request.to_string();
        let (choices, replies): (Vec<Choice>, Vec<ReplyBody>) = ask
            .choices
            .into_iter()
            .enumerate()
            .map(|(i, (label, allow, reply))| {
                (
                    Choice {
                        id: i.to_string(),
                        label,
                        allow,
                    },
                    reply,
                )
            })
            .unzip();
        self.approvals.insert(
            approval_id.clone(),
            Open {
                request: request.clone(),
                item: ask.item.clone(),
                replies,
                fill: ask.fill,
                fields: ask.fields.clone(),
            },
        );
        let mut out = vec![Out::Event(Event::Approval(Approval {
            id: approval_id,
            title: ask.title,
            detail: ask.detail,
            fields: ask.fields,
            choices,
        }))];
        if let Some(item) = ask.item {
            out.extend(self.set_tool_status(&item, ToolStatus::Pending));
        }
        out.push(self.state());
        out
    }

    // ---- Small pieces ---------------------------------------------------

    fn state(&self) -> Out {
        Out::Event(Event::State(if !self.approvals.is_empty() {
            SessionState::WaitingHuman
        } else if self.turn == TurnState::Idle && self.queued.is_empty() {
            SessionState::Idle
        } else {
            SessionState::Thinking
        }))
    }

    fn notice(&mut self, text: String) -> Out {
        self.next_notice += 1;
        Out::Event(Event::Entry(Entry::Notice {
            id: format!("notice-{}", self.next_notice),
            text,
        }))
    }
}

/// A tool title naming the files, and the diffs as the detail.
fn patch_view(changes: &[FileUpdateChange]) -> (String, String) {
    let names: Vec<String> = changes
        .iter()
        .map(|c| match &c.kind {
            PatchKind::Update {
                move_path: Some(to),
            } => format!("{} → {to}", c.path),
            _ => c.path.clone(),
        })
        .collect();
    let verb = match changes {
        [c] if matches!(c.kind, PatchKind::Add) => "Add",
        [c] if matches!(c.kind, PatchKind::Delete) => "Delete",
        _ => "Edit",
    };
    let detail = changes
        .iter()
        .map(|c| format!("{}\n{}", c.path, c.diff))
        .collect::<Vec<_>>()
        .join("\n");
    (format!("{verb} {}", names.join(", ")), detail)
}

fn plan_text(n: &PlanNote) -> String {
    let steps = n.plan.iter().map(|s| {
        let mark = match s.status {
            StepStatus::Completed => "[x]",
            StepStatus::InProgress => "[~]",
            StepStatus::Pending => "[ ]",
        };
        Some(format!("- {mark} {}", s.step))
    });
    lines(std::iter::once(n.explanation.clone()).chain(steps))
}

fn user_text(content: Vec<UserInput>) -> String {
    content
        .into_iter()
        .filter_map(|c| match c {
            UserInput::Text { text } => Some(text),
            UserInput::Other => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn mcp_text(r: McpResult) -> String {
    r.content
        .into_iter()
        .filter_map(|c| match c {
            McpContent::Text { text } => Some(text),
            McpContent::Other => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn tool_status(s: ItemStatus) -> ToolStatus {
    match s {
        ItemStatus::InProgress => ToolStatus::Running,
        ItemStatus::Completed => ToolStatus::Done,
        ItemStatus::Failed | ItemStatus::Declined => ToolStatus::Failed,
    }
}

fn error_text(message: &str, details: Option<&str>) -> String {
    match details.filter(|d| !d.is_empty()) {
        Some(d) => format!("{message}\n{d}"),
        None => message.to_string(),
    }
}

/// Make sure part `index` exists and hand it back.
fn summary_part(parts: &mut Vec<String>, index: usize) -> &mut String {
    if parts.len() <= index {
        parts.resize(index + 1, String::new());
    }
    &mut parts[index]
}

fn thought(id: String, parts: &[String]) -> Vec<Out> {
    let text = parts
        .iter()
        .filter(|p| !p.is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join("\n\n");
    vec![Out::Event(Event::Entry(Entry::Thought { id, text }))]
}

/// A message as one stdin line. Serialising these plain structs cannot fail
/// in practice, but if it did the human hears about it instead of Codex
/// getting half a message.
fn line<T: Serialize>(msg: &T) -> Out {
    match serde_json::to_string(msg) {
        Ok(s) => Out::Send(s),
        Err(e) => Out::Event(Event::Entry(Entry::Notice {
            id: "send-error".to_string(),
            text: format!("The building could not write a message for Codex: {e}"),
        })),
    }
}

/// A notification's params, typed, or nothing when they do not parse.
fn parse<T: DeserializeOwned>(params: &str, f: impl FnOnce(T) -> Vec<Out>) -> Vec<Out> {
    serde_json::from_str(params).map(f).unwrap_or_default()
}

/// A request's params, typed, or why they did not parse.
fn typed<T: DeserializeOwned>(params: &str) -> Result<T, String> {
    serde_json::from_str(params).map_err(|e| e.to_string())
}

/// The reply an answer sends: as prepared, or, for a choice that accepts a
/// question or a form, built from the human's values. A required field left
/// empty is refused before anything is sent.
fn filled(
    reply: &ReplyBody,
    fill: Option<&Fill>,
    fields: &[crate::structured::Field],
    values: &Values,
) -> Result<ReplyBody, String> {
    let accepting = matches!(
        reply,
        ReplyBody::Input { .. }
            | ReplyBody::Elicit {
                action: ElicitAction::Accept,
                ..
            }
    );
    let Some(fill) = fill.filter(|_| accepting) else {
        return Ok(reply.clone());
    };
    let value = |id: &str| values.get(id).map(|v| v.trim()).filter(|v| !v.is_empty());
    if let Some(missing) = fields.iter().find(|f| f.required && value(&f.id).is_none()) {
        return Err(format!("{} needs an answer first", missing.label));
    }
    Ok(match fill {
        Fill::Input(ids) => ReplyBody::Input {
            answers: ids
                .iter()
                .filter_map(|id| {
                    let answer = Answer {
                        answers: vec![value(id)?.to_string()],
                    };
                    Some((id.clone(), answer))
                })
                .collect(),
        },
        Fill::Form(kinds) => {
            let mut content = std::collections::BTreeMap::new();
            for (id, kind) in kinds {
                let Some(v) = value(id) else { continue };
                let typed = match kind {
                    FormKind::Text => FormValue::Text(v.to_string()),
                    FormKind::Boolean => FormValue::Bool(match v {
                        "true" => true,
                        "false" => false,
                        _ => return Err(format!("{id} takes yes or no")),
                    }),
                    FormKind::Integer => FormValue::Number(
                        v.parse::<i64>()
                            .map_err(|_| format!("{id} takes a whole number"))?
                            .into(),
                    ),
                    FormKind::Number => FormValue::Number(
                        v.parse::<f64>()
                            .ok()
                            .and_then(serde_json::Number::from_f64)
                            .ok_or_else(|| format!("{id} takes a number"))?,
                    ),
                };
                content.insert(id.clone(), typed);
            }
            ReplyBody::Elicit {
                action: ElicitAction::Accept,
                content: Some(content),
                meta: None,
            }
        }
    })
}
