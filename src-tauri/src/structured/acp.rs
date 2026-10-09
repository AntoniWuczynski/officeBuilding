//! The Agent Client Protocol (ACP) driver for opencode (`opencode acp`) and
//! Cursor (`cursor-agent acp`): newline-delimited JSON-RPC 2.0 on the agent's
//! stdio, read and written on plain threads (the `agent-client-protocol`
//! crate needs an async runtime, so the few messages used here are typed by hand).
//!
//! Sources, read for this file:
//! * <https://agentclientprotocol.com/protocol/initialization>,
//!   `/session-setup`, `/prompt-turn`, `/tool-calls`, `/session-config-options`
//!   and `/v1/authentication`; field names and enums from
//!   `schema/v1/schema.json` and `meta.json` in
//!   <https://github.com/agentclientprotocol/agent-client-protocol> (schema 1.24.1).
//! * The unstable `session/set_model` and `models` field, removed in
//!   agent-client-protocol#1325 but still what Cursor offers (its
//!   `session/new` sends `models`, no `configOptions`).
//! * <https://opencode.ai/docs/acp/> and <https://cursor.com/docs/cli/acp>.
//!
//! The flow: `initialize` (protocol 1, no `fs` or `terminal`: the agent uses its
//! own tools), `session/new` in the desk's folder with no extra MCP servers,
//! the model through a `model` config option or `session/set_model`, then one
//! `session/prompt` per turn. `authenticate` is never sent: Cursor's
//! `cursor_login` starts a browser sign-in, which is the tool's own business,
//! so a `-32000` answer becomes a notice asking the human to sign in with the tool.

mod cursor;
mod transcript;
mod wire;

#[cfg(test)]
mod tests;

use super::{Driver, Event, Sink, Spawn, Values};
use crate::model::{SessionState, ToolKind};
use crate::{pty, tools};
use serde::Serialize;
use serde_json::value::RawValue;
use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, mpsc};
use std::thread;
use std::time::{Duration, Instant};
use transcript::Transcript;
use wire::{
    InitializeResult, ModelChoices, NewSessionResult, PromptResult, RpcError, SelectValues,
};

/// The ACP version the building speaks.
const PROTOCOL_VERSION: u16 = 1;
/// How long `initialize`, `session/new` and a model switch may take.
const SETUP_TIMEOUT: Duration = Duration::from_secs(90);
/// How long a process that closed its stdout gets to exit before it is killed.
const EXIT_GRACE: Duration = Duration::from_secs(2);
/// Lines of stderr kept to explain an unexpected exit.
const STDERR_TAIL: usize = 12;

type Reply = Result<Option<Box<RawValue>>, RpcError>;

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

struct Desk {
    transcript: Transcript,
    session: Option<String>,
}

struct Shared {
    tool: &'static str,
    sink: Sink,
    stdin: Mutex<Option<ChildStdin>>,
    child: Mutex<Child>,
    next_id: AtomicU64,
    waiting: Mutex<HashMap<u64, mpsc::Sender<Reply>>>,
    desk: Mutex<Desk>,
    stderr: Mutex<VecDeque<String>>,
    /// Why the building ended the process itself (`None` when the human stopped it).
    reason: Mutex<Option<String>>,
    stopping: AtomicBool,
    exited: AtomicBool,
}

impl Shared {
    fn send(&self, line: &str) -> Result<(), String> {
        let mut stdin = lock(&self.stdin);
        let pipe = stdin
            .as_mut()
            .ok_or_else(|| "the agent has stopped".to_string())?;
        pipe.write_all(line.as_bytes())
            .and_then(|()| pipe.write_all(b"\n"))
            .and_then(|()| pipe.flush())
            .map_err(|e| format!("could not reach the agent: {e}"))
    }

    fn call<P: Serialize>(&self, method: &str, params: P) -> Result<mpsc::Receiver<Reply>, String> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let line = wire::request(id, method, params)?;
        let (tx, rx) = mpsc::channel();
        lock(&self.waiting).insert(id, tx);
        if let Err(e) = self.send(&line) {
            lock(&self.waiting).remove(&id);
            return Err(e);
        }
        Ok(rx)
    }

    /// A request answered within `SETUP_TIMEOUT`; an error answer is phrased for the human.
    fn call_wait<P: Serialize>(
        &self,
        method: &str,
        params: P,
    ) -> Result<Option<Box<RawValue>>, String> {
        let rx = self.call(method, params)?;
        match rx.recv_timeout(SETUP_TIMEOUT) {
            Ok(reply) => reply.map_err(|e| transcript::error_text(&e)),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                Err(format!("{} did not answer {method} in time", self.tool))
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                Err(format!("{} stopped while starting", self.tool))
            }
        }
    }

    /// Change the desk and report the events in order: they are sent while the
    /// desk is locked, so two threads never interleave their events.
    fn with_desk<R>(&self, f: impl FnOnce(&mut Desk) -> (Vec<Event>, R)) -> R {
        let mut desk = lock(&self.desk);
        let (events, out) = f(&mut desk);
        for e in events {
            (self.sink)(e);
        }
        out
    }

    fn notice(&self, text: String) {
        self.with_desk(|d| (vec![d.transcript.notice(text)], ()));
    }

    /// End the process because something went wrong; `text` explains the exit.
    fn fail(&self, text: String) {
        *lock(&self.reason) = Some(text);
        self.kill();
    }

    fn kill(&self) {
        lock(&self.stdin).take();
        let status = {
            let mut child = lock(&self.child);
            // Killing a process that already exited fails harmlessly.
            let _ = child.kill();
            child.wait().ok()
        };
        self.exited_with(status.and_then(|s| s.code()));
    }

    /// Report the exit, once.
    fn exited_with(&self, code: Option<i32>) {
        if self.exited.swap(true, Ordering::SeqCst) {
            return;
        }
        let message = lock(&self.reason).take().or_else(|| {
            if self.stopping.load(Ordering::SeqCst) || code == Some(0) {
                return None;
            }
            let tail: Vec<String> = lock(&self.stderr).iter().cloned().collect();
            Some(if tail.is_empty() {
                format!("{} stopped unexpectedly", self.tool)
            } else {
                format!("{} stopped unexpectedly:\n{}", self.tool, tail.join("\n"))
            })
        });
        (self.sink)(Event::Exited { code, message });
    }

    fn read(&self, stdout: impl BufRead) {
        for line in stdout.lines() {
            let Ok(line) = line else { break };
            match wire::parse(&line) {
                Some(wire::Inbound::Response { id, reply }) => {
                    if let Some(tx) = lock(&self.waiting).remove(&id) {
                        // The caller may have given up waiting; nothing to do then.
                        let _ = tx.send(reply);
                    }
                }
                Some(wire::Inbound::Request { id, method, params }) => {
                    let reply =
                        self.with_desk(|d| d.transcript.on_request(id, &method, params.as_deref()));
                    self.reply(reply);
                }
                Some(wire::Inbound::Notification { method, params }) => {
                    let reply = self
                        .with_desk(|d| d.transcript.on_notification(&method, params.as_deref()));
                    self.reply(reply);
                }
                None => {}
            }
        }
        lock(&self.waiting).clear();
        self.reap();
    }

    fn reply(&self, reply: Option<Result<String, String>>) {
        let sent = reply.map(|line| line.and_then(|l| self.send(&l)));
        if let Some(Err(e)) = sent {
            self.notice(e);
        }
    }

    /// stdout closed: wait briefly for the process to exit, else end it.
    fn reap(&self) {
        let until = Instant::now() + EXIT_GRACE;
        loop {
            let status = lock(&self.child).try_wait();
            match status {
                Ok(Some(status)) => return self.exited_with(status.code()),
                Ok(None) if Instant::now() < until => thread::sleep(Duration::from_millis(50)),
                _ => return self.kill(),
            }
        }
    }

    /// `initialize`, `session/new` (or `session/load` to reopen `resume`), the
    /// model, then the first task.
    fn set_up(
        self: &Arc<Self>,
        cwd: &str,
        model: Option<&str>,
        task: &str,
        resume: Option<&str>,
    ) -> Result<(), String> {
        let init = self.call_wait(
            "initialize",
            wire::InitializeParams {
                protocol_version: PROTOCOL_VERSION,
                client_capabilities: wire::ClientCapabilities {
                    fs: wire::FileSystemCapabilities {
                        read_text_file: false,
                        write_text_file: false,
                    },
                    terminal: false,
                },
                client_info: wire::Implementation {
                    name: "office-building",
                    title: "Office Building",
                    version: env!("CARGO_PKG_VERSION"),
                },
            },
        )?;
        let init: InitializeResult = wire::body(init.as_deref()).ok_or_else(|| {
            format!(
                "{} answered initialize with something unreadable",
                self.tool
            )
        })?;
        if init.protocol_version != PROTOCOL_VERSION {
            return Err(format!(
                "{} speaks ACP version {}, and the building speaks version {PROTOCOL_VERSION}",
                self.tool, init.protocol_version
            ));
        }
        let (session, id) = if let Some(id) = resume {
            if !init.agent_capabilities.load_session {
                return Err(format!(
                    "{} cannot reopen an earlier conversation",
                    self.tool
                ));
            }
            // The agent replays the conversation as updates before it answers.
            self.with_desk(|d| {
                d.transcript.replaying = true;
                (Vec::new(), ())
            });
            let loaded = self.call_wait(
                "session/load",
                wire::LoadSessionParams {
                    session_id: id,
                    cwd,
                    mcp_servers: [],
                },
            );
            self.with_desk(|d| {
                d.transcript.replaying = false;
                (Vec::new(), ())
            });
            (
                loaded.map_err(|e| sign_in_hint(e, &init.auth_methods))?,
                id.to_string(),
            )
        } else {
            let session = self
                .call_wait(
                    "session/new",
                    wire::NewSessionParams {
                        cwd,
                        mcp_servers: [],
                    },
                )
                .map_err(|e| sign_in_hint(e, &init.auth_methods))?;
            let id = wire::body::<NewSessionResult>(session.as_deref())
                .ok_or_else(|| format!("{} started a session the building cannot read", self.tool))?
                .session_id;
            (session, id)
        };
        self.with_desk(|d| {
            d.session = Some(id.clone());
            (vec![Event::SessionId(id.clone())], ())
        });
        if let Some(model) = model {
            let choices = wire::body::<ModelChoices>(session.as_deref()).unwrap_or_default();
            self.choose_model(&id, &choices, model);
        }
        if task.is_empty() {
            // Nothing to do until the human writes: the desk waits, idle.
            self.with_desk(|_| (vec![Event::State(SessionState::Idle)], ()));
        } else {
            // A failed send already ended the turn with a notice.
            let _ = self.prompt(task);
        }
        Ok(())
    }

    fn choose_model(&self, session: &str, choices: &ModelChoices, wanted: &str) {
        let switched = match model_call(choices, wanted) {
            Some(ModelCall::ConfigOption { config_id, value }) => self.call_wait(
                "session/set_config_option",
                wire::SetConfigOptionParams {
                    session_id: session,
                    config_id: &config_id,
                    value: &value,
                },
            ),
            Some(ModelCall::SetModel { model_id }) => self.call_wait(
                "session/set_model",
                wire::SetModelParams {
                    session_id: session,
                    model_id: &model_id,
                },
            ),
            None => Err(format!("{} does not offer {wanted} here", self.tool)),
        };
        if let Err(e) = switched {
            self.notice(format!(
                "Could not switch to the model {wanted}, so the tool's default is used: {e}"
            ));
        }
    }

    fn prompt(self: &Arc<Self>, text: &str) -> Result<(), String> {
        let session = self.with_desk(|d| {
            let Some(session) = d.session.clone() else {
                return (
                    Vec::new(),
                    Err("the agent is still starting; try again in a moment".to_string()),
                );
            };
            if d.transcript.turn_running() {
                return (
                    Vec::new(),
                    Err(
                        "the agent is still on the last message; wait for it or stop it first"
                            .to_string(),
                    ),
                );
            }
            (d.transcript.user(text), Ok(session))
        })?;
        let sent = self.call(
            "session/prompt",
            wire::PromptParams {
                session_id: &session,
                prompt: [wire::TextBlock { kind: "text", text }],
            },
        );
        let rx = match sent {
            Ok(rx) => rx,
            Err(e) => {
                self.with_desk(|d| (d.transcript.turn_ended(Err(e.clone())), ()));
                return Err(e);
            }
        };
        let shared = Arc::clone(self);
        run_thread("acp-turn", move || {
            // A dropped sender means the agent is gone; its exit is reported instead.
            if let Ok(reply) = rx.recv() {
                let ended = turn_result(reply);
                shared.with_desk(|d| (d.transcript.turn_ended(ended), ()));
            }
        })?;
        Ok(())
    }
}

fn run_thread(name: &str, run: impl FnOnce() + Send + 'static) -> Result<(), String> {
    thread::Builder::new()
        .name(name.to_string())
        .spawn(run)
        .map(drop)
        .map_err(|e| format!("could not start a thread for the agent: {e}"))
}

/// How the turn ended, from `session/prompt`'s reply.
fn turn_result(reply: Reply) -> Result<wire::StopReason, String> {
    match reply {
        Ok(raw) => wire::body::<PromptResult>(raw.as_deref())
            .map(|r| r.stop_reason)
            .ok_or_else(|| "the agent ended the turn with an unreadable reply".to_string()),
        Err(e) => Err(transcript::error_text(&e)),
    }
}

/// Name the agent's own sign-in methods when it asks for a sign-in.
fn sign_in_hint(error: String, methods: &[wire::AuthMethod]) -> String {
    let hints: Vec<String> = methods
        .iter()
        .map(|m| match &m.description {
            Some(d) => format!("{}: {d}", m.name),
            None => m.name.clone(),
        })
        .collect();
    if hints.is_empty() || !error.starts_with("The agent needs you to sign in") {
        error
    } else {
        format!("{error} {}", hints.join(" "))
    }
}

/// How a model is picked in this session.
#[derive(Debug, PartialEq, Eq)]
enum ModelCall {
    ConfigOption { config_id: String, value: String },
    SetModel { model_id: String },
}

/// The model `wanted` among the session's offers. A model matches by its id,
/// its name, or (Cursor) its id without the `[…]` parameters.
fn model_call(choices: &ModelChoices, wanted: &str) -> Option<ModelCall> {
    let matches = |id: &str, name: &str| {
        id == wanted || name == wanted || id.split('[').next() == Some(wanted)
    };
    let from_config = choices.config_options.iter().flatten().find_map(|o| {
        let wire::ConfigOption::Select(select) = o else {
            return None;
        };
        if select.category.as_deref() != Some("model") {
            return None;
        }
        let values: Vec<&wire::SelectValue> = match &select.options {
            SelectValues::Flat(values) => values.iter().collect(),
            SelectValues::Grouped(groups) => groups.iter().flat_map(|g| &g.options).collect(),
        };
        values
            .into_iter()
            .find(|v| matches(&v.value, &v.name))
            .map(|v| ModelCall::ConfigOption {
                config_id: select.id.clone(),
                value: v.value.clone(),
            })
    });
    from_config.or_else(|| {
        choices
            .models
            .as_ref()?
            .available_models
            .iter()
            .find(|m| matches(&m.model_id, &m.name))
            .map(|m| ModelCall::SetModel {
                model_id: m.model_id.clone(),
            })
    })
}

fn tool_name(tool: ToolKind) -> &'static str {
    match tool {
        ToolKind::Cursor => "Cursor",
        ToolKind::Opencode => "opencode",
        _ => "the agent",
    }
}

struct AcpDriver(Arc<Shared>);

impl Driver for AcpDriver {
    fn prompt(&self, text: &str) -> Result<(), String> {
        self.0.prompt(text)
    }

    fn answer(&self, approval_id: &str, choice_id: &str, values: &Values) -> Result<(), String> {
        let shared = &self.0;
        shared.with_desk(
            |d| match d.transcript.answer(approval_id, choice_id, values) {
                Ok(answer) => match shared.send(&answer.reply) {
                    Ok(()) => (answer.events, Ok(())),
                    Err(e) => (Vec::new(), Err(e)),
                },
                Err(e) => (Vec::new(), Err(e)),
            },
        )
    }

    fn cancel(&self) -> Result<(), String> {
        let shared = &self.0;
        shared.with_desk(|d| {
            let Some(session) = d.session.clone() else {
                return (Vec::new(), Ok(()));
            };
            if !d.transcript.turn_running() {
                return (Vec::new(), Ok(()));
            }
            let sent = wire::notification(
                "session/cancel",
                wire::CancelParams {
                    session_id: &session,
                },
            )
            .and_then(|l| shared.send(&l));
            if let Err(e) = sent {
                return (Vec::new(), Err(e));
            }
            let (replies, events) = d.transcript.cancel();
            let sent = replies
                .into_iter()
                .try_for_each(|r| r.and_then(|l| shared.send(&l)));
            (events, sent)
        })
    }

    fn stop(&self) {
        self.0.stopping.store(true, Ordering::SeqCst);
        self.0.kill();
    }
}

pub(super) fn start(spawn: &Spawn<'_>, sink: Sink) -> Result<Box<dyn Driver>, String> {
    let binary = tools::binary(spawn.tool).ok_or_else(|| "that tool has no command".to_string())?;
    if !spawn.cwd.is_dir() {
        return Err(format!("the folder {} does not exist", spawn.cwd.display()));
    }
    let cwd = spawn
        .cwd
        .to_str()
        .ok_or_else(|| {
            format!(
                "the folder {} has a name ACP cannot carry",
                spawn.cwd.display()
            )
        })?
        .to_string();
    let tool = tool_name(spawn.tool);
    // Through the login shell, so the agent finds the PATH a terminal desk gets
    // (`tools::installed` looks it up the same way). No `-i`: an interactive
    // shell could print to stdout, which carries the protocol.
    let shell = pty::login_shell();
    let mut command = format!("exec {binary} acp");
    for opt in &spawn.launch.opts {
        command.push(' ');
        command.push_str(opt.flag);
        command.push(' ');
        command.push_str(&pty::quote(&shell, &opt.value));
    }
    let mut child = Command::new(&shell)
        .args(["-l", "-c", &command])
        .envs(spawn.launch.env.iter().map(|(k, v)| (*k, v)))
        .current_dir(spawn.cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("could not start {tool}: {e}"))?;
    let (Some(stdin), Some(stdout), Some(stderr)) =
        (child.stdin.take(), child.stdout.take(), child.stderr.take())
    else {
        let _ = child.kill();
        return Err(format!("could not connect to {tool}"));
    };
    let shared = Arc::new(Shared {
        tool,
        sink,
        stdin: Mutex::new(Some(stdin)),
        child: Mutex::new(child),
        next_id: AtomicU64::new(1),
        waiting: Mutex::new(HashMap::new()),
        desk: Mutex::new(Desk {
            transcript: Transcript::default(),
            session: None,
        }),
        stderr: Mutex::new(VecDeque::new()),
        reason: Mutex::new(None),
        stopping: AtomicBool::new(false),
        exited: AtomicBool::new(false),
    });
    let s = Arc::clone(&shared);
    let started = run_thread("acp-stdout", move || s.read(BufReader::new(stdout)));
    let s = Arc::clone(&shared);
    let started = started.and_then(|()| {
        run_thread("acp-stderr", move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                let mut tail = lock(&s.stderr);
                tail.push_back(line);
                if tail.len() > STDERR_TAIL {
                    tail.pop_front();
                }
            }
        })
    });
    let s = Arc::clone(&shared);
    let model = spawn.model.map(str::to_string);
    let task = spawn.task.to_string();
    let resume = spawn.resume.map(str::to_string);
    let started = started.and_then(|()| {
        run_thread("acp-setup", move || {
            // A stop during setup is the human's doing, not a failure.
            if let Err(e) = s.set_up(&cwd, model.as_deref(), &task, resume.as_deref())
                && !s.stopping.load(Ordering::SeqCst)
            {
                s.notice(e.clone());
                s.fail(e);
            }
        })
    });
    if let Err(e) = started {
        shared.fail(e.clone());
        return Err(e);
    }
    Ok(Box::new(AcpDriver(shared)))
}
