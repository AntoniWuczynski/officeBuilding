//! The chat panel's composer and Stop. A Human's messages go through the delivery path
//! (`office/deliver.rs`), in a queue of their own: this file only queues them.

use super::super::deliver::DeskNow;
use super::super::{Inner, Office, now_iso};
use crate::delivery::gate::{self, Hold, Route};
use crate::delivery::outbox::Item;
use crate::delivery::text;
use crate::manager::wire::QueuedNote;
use crate::model::{SessionState, ToolKind};
use std::time::{Duration, SystemTime};

/// The most one message from a chat panel may hold, in characters.
pub(in crate::office) const MESSAGE_MAX: usize = 16_000;
/// Two Escapes this close together clear a draft or open Claude Code's rewind menu.
const STOP_GAP: Duration = Duration::from_secs(2);
const ESCAPE: &str = "\x1b";

/// Whether `at` was less than `STOP_GAP` ago. A clock that went back counts as recent.
fn within_gap(at: SystemTime) -> bool {
    SystemTime::now()
        .duration_since(at)
        .map_or(true, |since| since < STOP_GAP)
}

impl Office {
    /// Queue A Human's message for a desk and hand it over once the desk can take it. It
    /// overrides the manager's notes waiting for that desk.
    pub fn chat_send(&self, session_id: &str, raw: &str) -> Result<(), String> {
        let text = text::clean(raw, MESSAGE_MAX)?;
        {
            let mut guard = self.lock();
            let inner = &mut *guard;
            let desk = self
                .desk_now(inner, session_id)
                .ok_or("that desk has left, so there is nobody to send it to")?;
            if desk.desk.route == Route::None(Hold::HandStarted) {
                return Err(
                    "this session was started outside the building, so nothing can be sent to it from here"
                        .to_string(),
                );
            }
            let id = inner.store.next_id("q");
            inner.mail.own.push(Item {
                id,
                floor: desk.project_id,
                from: String::new(),
                to: desk.session_id,
                intro: String::new(),
                text,
                queued_at: now_iso(),
            })?;
            self.override_notes(inner, &desk.key);
        }
        self.deliver_due();
        Ok(())
    }

    /// Take one of A Human's own messages out of the queue before it is handed over.
    pub fn withdraw_message(&self, message_id: &str) -> Result<(), String> {
        self.lock()
            .mail
            .own
            .take(message_id)
            .map(|_| ())
            .ok_or_else(|| "that message is no longer waiting".to_string())
    }

    /// Interrupt the turn a desk is running: the provider's own interrupt for a structured
    /// desk, one Escape for Claude Code in an app terminal. It never ends anything.
    pub fn chat_interrupt(&self, session_id: &str) -> Result<(), String> {
        if self
            .structured
            .sessions()
            .iter()
            .any(|s| s.id == session_id)
        {
            return self.structured.cancel(session_id);
        }
        let terminal = {
            let mut guard = self.lock();
            let inner = &mut *guard;
            inner.feed.collect(SystemTime::now());
            let Some((desk, terminal)) = self
                .desk_now(inner, session_id)
                .and_then(|d| d.terminal.clone().map(|t| (d, t)))
                .filter(|(d, _)| Self::is_claude(inner, d))
            else {
                return Err(
                    "only a Claude Code desk in one of the building's terminals can be stopped from its chat"
                        .to_string(),
                );
            };
            // Claude Code's docs: "On a permission prompt, `Esc` declines the action"
            // (https://code.claude.com/docs/en/interactive-mode).
            if gate::waiting_on_you(&desk.desk) {
                return Err(
                    "it is waiting for your answer, and Escape would decline it: answer it in the terminal"
                        .to_string(),
                );
            }
            let working = matches!(
                desk.desk.state,
                SessionState::Running | SessionState::Thinking
            );
            let recent = inner
                .mail
                .stopped
                .get(&terminal)
                .is_some_and(|at| within_gap(*at))
                || desk.desk.typed_ago.is_some_and(|ago| ago < STOP_GAP);
            if !working || recent {
                return Ok(());
            }
            inner
                .mail
                .stopped
                .insert(terminal.clone(), SystemTime::now());
            terminal
        };
        self.terminals.write(&terminal, ESCAPE)
    }

    fn is_claude(inner: &Inner, desk: &DeskNow) -> bool {
        inner
            .found
            .sessions
            .iter()
            .any(|s| s.id == desk.session_id && s.tool == ToolKind::ClaudeCode)
    }

    /// A Human's own messages still waiting, oldest first, each with why it waits.
    pub(in crate::office) fn queued_messages(&self, inner: &Inner) -> Vec<QueuedNote> {
        inner
            .mail
            .own
            .items()
            .iter()
            .filter_map(|item| {
                let desk = self.desk_now(inner, &item.to)?;
                Some(QueuedNote {
                    id: item.id.clone(),
                    to_session_id: desk.session_id.clone(),
                    text: item.text.clone(),
                    queued_at: item.queued_at.clone(),
                    // One the next pass would hand over is still on its way.
                    hold: Self::own_hold(&desk).unwrap_or(Hold::Busy),
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
