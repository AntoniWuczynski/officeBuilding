//! The workers a chat desk's thread tree started, as the app-server reports them
//! (build spec M2b, section 6.2). A thread becomes a worker only when a
//! collaboration item or an activity item names it (D10). Its turns then say
//! when it works. A failed worker is left out: its error light comes from the disk (D12).

use super::wire::{ActivityKind, AgentState, AgentStatus, CollabStatus, CollabTool, ThreadItem};
use crate::discovery::codex::children::{name_of, path_name};
use crate::model::{Helper, SessionState};
use crate::structured::ToolStatus;
use std::collections::BTreeMap;

/// One worker the driver was told of.
struct Worker {
    thread: String,
    working: bool,
    /// The last part of its agent path, cleaned.
    called: Option<String>,
    model: Option<String>,
    effort: Option<String>,
}

/// The workers this desk's thread tree started, in the order first named, and the list
/// the desk was last told.
#[derive(Default)]
pub(super) struct Children {
    workers: Vec<Worker>,
    sent: Vec<Helper>,
}

impl Children {
    fn known(&mut self, thread: &str) -> &mut Worker {
        let at = self
            .workers
            .iter()
            .position(|w| w.thread == thread)
            .unwrap_or_else(|| {
                self.workers.push(Worker {
                    thread: thread.to_string(),
                    working: false,
                    called: None,
                    model: None,
                    effort: None,
                });
                self.workers.len() - 1
            });
        &mut self.workers[at]
    }

    /// A thread item of the desk or of one of its workers. A collaboration call returns
    /// its conversation line (id, title, status). Any other item changes nothing.
    pub(super) fn item(&mut self, item: &ThreadItem) -> Option<(String, String, ToolStatus)> {
        match item {
            ThreadItem::CollabAgentToolCall {
                id,
                tool,
                status,
                receiver_thread_ids,
                agents_states,
                model,
                reasoning_effort,
            } => {
                self.collab(
                    *tool,
                    *status,
                    receiver_thread_ids,
                    agents_states,
                    model.as_deref(),
                    reasoning_effort.as_deref(),
                );
                let (title, status) = call_line(*tool, *status);
                Some((id.clone(), title, status))
            }
            ThreadItem::SubAgentActivity {
                agent_thread_id,
                agent_path,
                kind,
                model,
                reasoning_effort,
            } => {
                self.activity(
                    agent_thread_id,
                    agent_path,
                    *kind,
                    model.as_deref(),
                    reasoning_effort.as_deref(),
                );
                None
            }
            _ => None,
        }
    }

    /// A `collabAgentToolCall` item, started or completed.
    fn collab(
        &mut self,
        tool: CollabTool,
        status: CollabStatus,
        receivers: &[String],
        agents: &BTreeMap<String, AgentState>,
        model: Option<&str>,
        effort: Option<&str>,
    ) {
        for (thread, state) in agents {
            let working = match state.status {
                AgentStatus::PendingInit | AgentStatus::Running => Some(true),
                AgentStatus::Interrupted
                | AgentStatus::Completed
                | AgentStatus::Errored
                | AgentStatus::Shutdown
                | AgentStatus::NotFound => Some(false),
                AgentStatus::Other => None,
            };
            let worker = self.known(thread);
            if let Some(working) = working {
                worker.working = working;
            }
        }
        // G17: a completed spawn or resume is over, its worker goes on.
        let working = match (tool, status) {
            (CollabTool::SpawnAgent | CollabTool::ResumeAgent, CollabStatus::Completed) => true,
            (CollabTool::CloseAgent, CollabStatus::Completed) => false,
            _ => return,
        };
        for thread in receivers {
            let named = agents.contains_key(thread);
            let worker = self.known(thread);
            if !named {
                worker.working = working;
            }
            if tool == CollabTool::SpawnAgent {
                worker.model = model.map(str::to_string).or(worker.model.take());
                worker.effort = effort.map(str::to_string).or(worker.effort.take());
            }
        }
    }

    /// A `subAgentActivity` item.
    fn activity(
        &mut self,
        thread: &str,
        path: &str,
        kind: ActivityKind,
        model: Option<&str>,
        effort: Option<&str>,
    ) {
        let worker = self.known(thread);
        worker.called = path_name(path).or(worker.called.take());
        match kind {
            ActivityKind::Started => {
                worker.working = true;
                worker.model = model.map(str::to_string).or(worker.model.take());
                worker.effort = effort.map(str::to_string).or(worker.effort.take());
            }
            ActivityKind::Interrupted | ActivityKind::Completed => worker.working = false,
            // The worker's own turn says when it works again.
            ActivityKind::Interacted | ActivityKind::Other => {}
        }
    }

    /// A turn of another thread began or ended. A thread no item named is not a worker (D10).
    pub(super) fn turn(&mut self, thread: &str, began: bool) {
        if let Some(worker) = self.workers.iter_mut().find(|w| w.thread == thread) {
            worker.working = began;
        }
    }

    /// Nothing replayed from a stored conversation still runs.
    pub(super) fn settle(&mut self) {
        for worker in &mut self.workers {
            worker.working = false;
        }
    }

    /// The workers to draw now. One with a request open (`asking`) shows as waiting.
    fn helpers(&self, asking: impl Fn(&str) -> bool) -> Vec<Helper> {
        self.workers
            .iter()
            .filter_map(|w| {
                let state = if asking(&w.thread) {
                    SessionState::WaitingHuman
                } else if w.working {
                    SessionState::Running
                } else {
                    return None;
                };
                Some(Helper {
                    id: w.thread.clone(),
                    state,
                    worktree: false,
                    branch: None,
                    model: w.model.clone(),
                    effort: w.effort.clone(),
                })
            })
            .collect()
    }

    /// The workers to draw, when they differ from the list the desk was last told.
    pub(super) fn fresh(&mut self, asking: impl Fn(&str) -> bool) -> Option<Vec<Helper>> {
        let now = self.helpers(asking);
        (now != self.sent).then(|| {
            self.sent.clone_from(&now);
            now
        })
    }

    /// What a request of `thread` says about who asked (D11).
    pub(super) fn name(&self, thread: &str) -> String {
        let called = self
            .workers
            .iter()
            .find(|w| w.thread == thread)
            .and_then(|w| w.called.as_deref());
        name_of(called, thread)
    }
}

/// The conversation's line for a collaboration call of the desk's own thread.
fn call_line(tool: CollabTool, status: CollabStatus) -> (String, ToolStatus) {
    let title = match tool {
        CollabTool::SpawnAgent => "Start a helper",
        CollabTool::SendInput | CollabTool::SendMessage | CollabTool::FollowupTask => {
            "Message a helper"
        }
        CollabTool::ResumeAgent => "Resume a helper",
        CollabTool::Wait => "Wait for helpers",
        CollabTool::CloseAgent => "Dismiss a helper",
        CollabTool::InterruptAgent => "Interrupt a helper",
        CollabTool::ListAgents => "List the helpers",
        CollabTool::Other => "Work with helpers",
    };
    let status = match status {
        CollabStatus::InProgress | CollabStatus::Other => ToolStatus::Running,
        CollabStatus::Completed => ToolStatus::Done,
        CollabStatus::Failed | CollabStatus::Interrupted => ToolStatus::Failed,
    };
    (title.to_string(), status)
}
