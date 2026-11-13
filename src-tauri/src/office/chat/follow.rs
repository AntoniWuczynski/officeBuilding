//! Following a Claude Code conversation for the chat view: its transcript, read
//! by offset as it grows, and what the hook feed and discovery say about it.
//! The followers live under their own lock (`Office::chats`), taken before the
//! office lock and never under it, and no file is read while the office lock is held.

use super::super::{Inner, Office};
use crate::delivery::gate;
use crate::discovery::claude_code::chat::ChatReader;
use crate::model::{ControlMode, SessionState, ToolKind};
use crate::structured::desks::{Backlog, Update};
use crate::structured::{self, Approval, Capabilities, Entry};
use crate::sync::lock;
use std::collections::BTreeMap;
use std::sync::atomic::Ordering;

const NO_CONVERSATION: &str = "that desk has no conversation to show";
const ENDED: &str = "This session has ended.";
const RESTARTED: &str =
    "A new conversation was started in this terminal. Open the desk again to follow it.";
/// The id of the card that says the desk waits on A Human in its terminal.
const PROMPT_ID: &str = "terminal-prompt";
const PROMPT_TITLE: &str = "Claude Code is waiting for your answer";

/// The Claude Code conversations a panel is open on, by session id.
#[derive(Default)]
pub(in crate::office) struct Followers {
    open: BTreeMap<String, Follower>,
    /// The ticket of the last `keep_chat` applied.
    kept: u64,
}

struct Follower {
    reader: ChatReader,
    /// What the last update or backlog said, so a pass with nothing new sends nothing.
    said: Option<Facts>,
}

/// What a conversation says about itself beside its entries. `capabilities` goes out with the
/// backlog only, so it is left out when two `Facts` are compared for "did anything change".
#[derive(Clone)]
struct Facts {
    state: SessionState,
    approval: Option<Approval>,
    ended: Option<String>,
    capabilities: Capabilities,
}

impl Facts {
    fn ended(text: &str) -> Self {
        Self {
            state: SessionState::Done,
            approval: None,
            ended: Some(text.to_string()),
            capabilities: Capabilities::READ_ONLY,
        }
    }

    fn same(&self, other: &Self) -> bool {
        self.state == other.state && self.approval == other.approval && self.ended == other.ended
    }

    fn update(&self, session_id: &str, entry: Option<Entry>, reset: bool) -> Update {
        Update {
            session_id: session_id.to_string(),
            entry,
            state: self.state,
            approval: self.approval.clone(),
            ended: self.ended.clone(),
            reset,
            desk_changed: false,
        }
    }
}

impl Office {
    /// What `session_id`'s conversation says about itself now. `followed`: a panel
    /// already follows it, so a session that has gone is an ending, not an error.
    fn facts(&self, inner: &Inner, session_id: &str, followed: bool) -> Result<Facts, String> {
        let Some(s) = inner.found.sessions.iter().find(|s| s.id == session_id) else {
            if !followed && !inner.ran_in.contains_key(session_id) {
                return Err(NO_CONVERSATION.to_string());
            }
            let restarted = inner
                .ran_in
                .get(session_id)
                .and_then(|terminal| inner.owned.get(terminal))
                .is_some_and(|now| now != session_id);
            return Ok(Facts::ended(if restarted { RESTARTED } else { ENDED }));
        };
        if s.tool != ToolKind::ClaudeCode {
            return Err(NO_CONVERSATION.to_string());
        }
        let desk = self.desk_now(inner, session_id);
        let here = s.control == ControlMode::Full
            && desk
                .as_ref()
                .is_some_and(|d| d.terminal.is_some() && d.session_id == session_id);
        let waiting = desk.as_ref().is_some_and(|d| gate::waiting_on_you(&d.desk))
            || s.state == SessionState::WaitingHuman
            || !s.pending_question_ids.is_empty();
        let approval = waiting.then(|| {
            let question = inner
                .found
                .questions
                .iter()
                .find(|q| q.session_id == session_id && s.pending_question_ids.contains(&q.id))
                .map(|q| q.prompt.as_str());
            Approval {
                id: PROMPT_ID.to_string(),
                title: PROMPT_TITLE.to_string(),
                detail: question
                    .or_else(|| inner.feed.asked(session_id))
                    .unwrap_or_default()
                    .to_string(),
                fields: Vec::new(),
                choices: Vec::new(),
            }
        });
        Ok(Facts {
            state: s.state,
            approval,
            ended: None,
            capabilities: if here {
                Capabilities::CLAUDE_TERMINAL
            } else {
                Capabilities::READ_ONLY
            },
        })
    }

    /// Everything a chat panel needs when it opens: a structured desk's own backlog,
    /// or a Claude Code session's transcript so far.
    pub fn chat_backlog(&self, session_id: &str) -> Result<Backlog, String> {
        if let Ok(backlog) = self.structured.backlog(session_id) {
            return Ok(backlog);
        }
        let mut chats = lock(&self.chats);
        let known = chats.open.contains_key(session_id);
        let facts = self.facts(&self.lock(), session_id, known)?;
        let follower = chats
            .open
            .entry(session_id.to_string())
            .or_insert_with(|| Follower {
                reader: ChatReader::new(&self.home, session_id),
                said: None,
            });
        follower.reader.read();
        let (transcript, cut) = structured::window(&follower.reader.kept());
        let earlier = cut || follower.reader.earlier();
        follower.said = Some(facts.clone());
        Ok(Backlog {
            transcript,
            state: facts.state,
            approval: facts.approval,
            ended: facts.ended,
            capabilities: facts.capabilities,
            earlier,
        })
    }

    /// What changed in the followed conversations since the last call: one update per
    /// new or changed entry, or one with no entry when only the state, the card or the
    /// ending changed.
    pub fn chat_updates(&self) -> Vec<Update> {
        let mut chats = lock(&self.chats);
        let mut updates = Vec::new();
        for (id, follower) in &mut chats.open {
            let read = follower.reader.read();
            let facts = self
                .facts(&self.lock(), id, true)
                .unwrap_or_else(|_| Facts::ended(ENDED));
            let changed = follower.said.as_ref().is_none_or(|said| !said.same(&facts));
            if read.entries.is_empty() && (changed || read.reset) {
                updates.push(facts.update(id, None, read.reset));
            }
            let mut reset = read.reset;
            for entry in read.entries {
                updates.push(facts.update(id, Some(entry), reset));
                reset = false;
            }
            follower.said = Some(facts);
        }
        updates
    }

    /// A ticket for `keep_chat`, taken when the open desk changes and before the call
    /// waits on the followers' lock, so the switches apply in the order they were made.
    pub fn chat_ticket(&self) -> u64 {
        self.chat_tickets.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// Follow only the conversation of the desk whose panel is open, if any. A ticket older
    /// than the last one applied is a stale switch and changes nothing.
    pub fn keep_chat(&self, ticket: u64, open: Option<&str>) {
        let mut chats = lock(&self.chats);
        if ticket < chats.kept {
            return;
        }
        chats.kept = ticket;
        chats.open.retain(|id, _| Some(id.as_str()) == open);
    }
}

#[cfg(test)]
mod tests;
