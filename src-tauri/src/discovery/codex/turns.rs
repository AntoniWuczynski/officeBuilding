//! A rollout reduced to the turn it is in: which calls are open, what the turn's reply is and
//! which questions Codex asked with its `request_user_input` tool and has no answer to yet.
//! The record shapes are the ones the Codex source writes (turn events with their aliases,
//! message phases, function calls and their outputs); every field is read leniently.

use crate::delivery::text;
use crate::discovery::rules::LastBlock;
use serde::Deserialize;

/// The prompt of a question whose arguments could not be read.
const FALLBACK_PROMPT: &str = "Codex asked a question in its terminal";
const PROMPT_CHARS: usize = 400;
const OPTION_CHARS: usize = 120;
const MAX_OPTIONS: usize = 6;
const MAX_ENDED_TURNS: usize = 8;
const MAX_DONE_CALLS: usize = 64;
const ASK_TOOL: &str = "request_user_input";

/// A question Codex asked with its `request_user_input` tool and has no answer to yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ask {
    pub call_id: String,
    /// The first question as asked, at most 400 chars.
    pub prompt: String,
    /// The first question's option labels: at most 6, each at most 120 chars.
    pub options: Vec<String>,
    /// How many questions the call asks.
    pub questions: usize,
    /// The record's timestamp, or empty.
    pub asked_at: String,
}

/// What a rollout proves about requests, for the reconcile. Backend only.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Evidence {
    /// The turn the rollout is in, or was last in.
    pub turn_id: Option<String>,
    /// The turns it shows ended, oldest first, at most 8.
    pub ended_turns: Vec<String>,
    /// The tool calls of the current turn that have their output, oldest first, at most 64.
    pub done_calls: Vec<String>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct AskArguments {
    questions: Vec<AskQuestion>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct AskQuestion {
    question: String,
    options: Vec<AskOption>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct AskOption {
    label: String,
}

impl Ask {
    fn read(call_id: &str, arguments: &str, asked_at: &str) -> Self {
        let parsed = serde_json::from_str::<AskArguments>(arguments).unwrap_or_default();
        let (prompt, options, questions) = match parsed.questions.first() {
            Some(first) if !first.question.trim().is_empty() => (
                text::visible(first.question.trim())
                    .chars()
                    .take(PROMPT_CHARS)
                    .collect(),
                first
                    .options
                    .iter()
                    .map(|o| {
                        text::visible(&o.label)
                            .chars()
                            .take(OPTION_CHARS)
                            .collect::<String>()
                    })
                    .filter(|l| !l.is_empty())
                    .take(MAX_OPTIONS)
                    .collect(),
                parsed.questions.len(),
            ),
            _ => (FALLBACK_PROMPT.to_string(), Vec::new(), 0),
        };
        Self {
            call_id: call_id.to_string(),
            prompt,
            options,
            questions,
            asked_at: asked_at.to_string(),
        }
    }
}

struct OpenCall {
    id: String,
    ask: Option<Ask>,
}

/// The turn a rollout is in and what belongs to it.
pub(super) struct Turns {
    turn: Option<String>,
    open: bool,
    calls: Vec<OpenCall>,
    done: Vec<String>,
    ended: Vec<String>,
    error: bool,
    human: bool,
    block: LastBlock,
    reply: String,
}

impl Default for Turns {
    fn default() -> Self {
        Self {
            turn: None,
            open: false,
            calls: Vec::new(),
            done: Vec::new(),
            ended: Vec::new(),
            error: false,
            human: false,
            block: LastBlock::Nothing,
            reply: String::new(),
        }
    }
}

impl Turns {
    /// `task_started` or `turn_started`.
    pub(super) fn started(&mut self, turn_id: Option<&str>) {
        if turn_id.is_some_and(|id| self.ended.iter().any(|e| e == id)) {
            return;
        }
        self.turn = turn_id.map(str::to_string);
        self.open = true;
        self.calls.clear();
        self.done.clear();
        self.error = false;
    }

    /// `task_complete` or `turn_complete`.
    pub(super) fn ended(&mut self, turn_id: Option<&str>, error: bool) {
        let late = match (turn_id, self.turn.as_deref()) {
            (Some(id), Some(current)) => id != current,
            _ => false,
        };
        if let Some(id) = turn_id {
            if !self.ended.iter().any(|e| e == id) {
                self.ended.push(id.to_string());
                if self.ended.len() > MAX_ENDED_TURNS {
                    self.ended.remove(0);
                }
            }
            if self.turn.is_none() {
                self.turn = Some(id.to_string());
            }
        }
        if late {
            return;
        }
        self.open = false;
        self.calls.clear();
        self.error = error;
    }

    /// `turn_aborted`: the same as an end with no error.
    pub(super) fn aborted(&mut self, turn_id: Option<&str>) {
        self.ended(turn_id, false);
    }

    /// A message the human wrote, in any of the three encodings.
    pub(super) fn human(&mut self) {
        self.human = true;
        self.reply.clear();
    }

    /// An assistant message. Commentary is mid-turn text and is never the reply.
    pub(super) fn assistant(&mut self, text: &str, commentary: bool) {
        self.human = false;
        self.block = LastBlock::Text;
        if !commentary {
            self.reply = text.to_string();
        }
    }

    pub(super) fn reasoning(&mut self) {
        self.block = LastBlock::Thinking;
    }

    /// A `function_call` or `custom_tool_call`. A call with no id still counts as the last block.
    pub(super) fn call(
        &mut self,
        call_id: Option<&str>,
        name: &str,
        arguments: &str,
        timestamp: &str,
    ) {
        self.block = LastBlock::ToolUse;
        if let Some(id) = call_id {
            let ask = (name == ASK_TOOL).then(|| Ask::read(id, arguments, timestamp));
            self.calls.push(OpenCall {
                id: id.to_string(),
                ask,
            });
        }
    }

    /// A `function_call_output` or `custom_tool_call_output`.
    pub(super) fn output(&mut self, call_id: &str) {
        self.calls.retain(|c| c.id != call_id);
        self.done.push(call_id.to_string());
        if self.done.len() > MAX_DONE_CALLS {
            self.done.remove(0);
        }
    }

    pub(super) fn turn_open(&self) -> bool {
        self.open
    }

    pub(super) fn open_tools(&self) -> usize {
        self.calls.len()
    }

    pub(super) fn api_error(&self) -> bool {
        self.error
    }

    pub(super) fn human_spoke_last(&self) -> bool {
        self.human
    }

    pub(super) fn last_block(&self) -> LastBlock {
        self.block
    }

    pub(super) fn reply(&self) -> &str {
        &self.reply
    }

    pub(super) fn asks(&self) -> Vec<Ask> {
        self.calls.iter().filter_map(|c| c.ask.clone()).collect()
    }

    pub(super) fn evidence(&self) -> Evidence {
        Evidence {
            turn_id: self.turn.clone(),
            ended_turns: self.ended.clone(),
            done_calls: self.done.clone(),
        }
    }
}

#[cfg(test)]
mod tests;
