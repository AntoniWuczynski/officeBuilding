//! The office's side of delivery: which desk a note is for, whether it can take it now,
//! and the hand-over. The only code that types a note into a terminal or prompts a
//! structured desk with one. The office lock is never held while it does either.

use super::{Inner, Office, now_iso};
use crate::delivery::gate::{self, Hold, Route, Verdict};
use crate::delivery::outbox::{Item, Outbox};
use crate::delivery::text;
use crate::manager::wire::{LogEvent, NoteState};
use crate::model::{AgentMessage, ControlMode, Session, SessionState, ToolKind};
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant, SystemTime};

/// How long after the text before Enter is sent, so a terminal takes them as two events.
const SUBMIT_PAUSE: Duration = Duration::from_millis(120);

/// Notes on their way to desks, and what the building knows about each desk's keyboard.
#[derive(Default)]
pub(super) struct Mail {
    pub(super) outbox: Outbox,
    /// A Human's own messages from a chat panel. They carry no introduction, a paused
    /// manager does not hold them, and they go first.
    pub(super) own: Outbox,
    /// When A Human's Stop last sent Escape to an app terminal, by terminal id.
    pub(super) stopped: BTreeMap<String, SystemTime>,
    /// When A Human last typed in each app terminal, by terminal id.
    typed: BTreeMap<String, Instant>,
    /// App terminals whose input box holds typing that was never sent.
    drafts: BTreeSet<String>,
    /// Desks handed a note that has not been picked up, by `DeskNow::key`, with when.
    awaiting: BTreeMap<String, SystemTime>,
    /// Something the snapshot shows changed outside a hand-over (an override): the next
    /// `deliver_due` reports it.
    dirty: bool,
}

/// A desk as a note would find it now.
pub(super) struct DeskNow {
    /// Its terminal's id, or its session id when it has no terminal of the app's.
    pub(super) key: String,
    /// The terminal the app runs it in, if any.
    pub(super) terminal: Option<String>,
    /// The session at the desk now (a `/clear` gives a terminal a new one).
    pub(super) session_id: String,
    pub(super) project_id: String,
    pub(super) title: String,
    pub(super) desk: gate::Desk,
    /// When it last did anything the building saw: the feed's newest event, or its last activity.
    pub(super) last_event: Option<SystemTime>,
    /// A Human's Stop interrupted it and its own files say idle, with no event since: the
    /// Stop hook does not fire on an interrupt. Only A Human's own messages read it.
    pub(super) stopped: bool,
}

/// A note to queue. `intro` and `text` are already clean.
pub(super) struct NewNote {
    pub floor: String,
    pub from: String,
    pub to: String,
    pub intro: String,
    pub text: String,
}

/// A note taken out of the outbox to be handed over, with where it goes.
struct HandOver {
    item: Item,
    key: String,
    terminal: Option<String>,
    session_id: String,
    bracketed: bool,
    /// One of A Human's own messages, not a manager's note.
    own: bool,
}

fn parsed(at: &str) -> Option<SystemTime> {
    chrono::DateTime::parse_from_rfc3339(at)
        .ok()
        .map(SystemTime::from)
}

/// The app terminal a session runs in or last ran in. Shells are not sessions.
fn terminal_of(inner: &Inner, session_id: &str) -> Option<String> {
    inner
        .owned
        .iter()
        .find(|(_, s)| s.as_str() == session_id)
        .map(|(t, _)| t)
        .or_else(|| inner.ran_in.get(session_id))
        .cloned()
}

impl Office {
    /// The desk `session_id` names. None when no such session is at a desk now.
    pub(super) fn desk_now(&self, inner: &Inner, session_id: &str) -> Option<DeskNow> {
        let awaiting = |key: &str| inner.mail.awaiting.contains_key(key);
        if let Some(s) = self
            .structured
            .sessions()
            .into_iter()
            .find(|s| s.id == session_id)
        {
            return Some(DeskNow {
                desk: gate::Desk {
                    route: Route::Structured,
                    state: s.state,
                    prompt_open: s.state == SessionState::WaitingHuman,
                    typed_ago: None,
                    draft_open: false,
                    awaiting_turn: awaiting(&s.id),
                },
                last_event: parsed(&s.last_activity_at),
                ..at_desk(&s)
            });
        }
        let found = |id: &str| inner.found.sessions.iter().find(|s| s.id == id);
        if let Some(t) = terminal_of(inner, session_id)
            .filter(|t| self.terminals.wait_exit(t, Duration::ZERO) == Ok(None))
        {
            let s = found(inner.owned.get(&t)?)?;
            let seen = inner.feed.seen(&s.id);
            let hooked = matches!(s.tool, ToolKind::ClaudeCode | ToolKind::Codex);
            let feed_state = seen.map(|(state, _)| state);
            let state = match (s.state, feed_state) {
                (SessionState::Idle, Some(SessionState::Idle)) => SessionState::Idle,
                (SessionState::Idle, _) => SessionState::Running,
                (other, _) => other,
            };
            let stopped = s.state == SessionState::Idle
                && inner
                    .mail
                    .stopped
                    .get(&t)
                    .is_some_and(|at| seen.is_none_or(|(_, e)| *at > e));
            return Some(DeskNow {
                key: t.clone(),
                desk: gate::Desk {
                    route: if hooked && seen.is_some() {
                        Route::Terminal
                    } else {
                        Route::None(Hold::NoIdleSignal)
                    },
                    state,
                    prompt_open: !s.pending_question_ids.is_empty()
                        || s.state == SessionState::WaitingHuman
                        || feed_state == Some(SessionState::WaitingHuman),
                    typed_ago: inner.mail.typed.get(&t).map(Instant::elapsed),
                    draft_open: inner.mail.drafts.contains(&t),
                    awaiting_turn: awaiting(&t),
                },
                last_event: seen.map(|(_, at)| at),
                stopped,
                terminal: Some(t),
                ..at_desk(s)
            });
        }
        let s = found(session_id)
            .filter(|s| matches!(s.control, ControlMode::RaiseWindow | ControlMode::ReadOnly))?;
        Some(DeskNow {
            desk: gate::Desk {
                route: Route::None(Hold::HandStarted),
                state: s.state,
                prompt_open: !s.pending_question_ids.is_empty()
                    || s.state == SessionState::WaitingHuman,
                typed_ago: None,
                draft_open: false,
                awaiting_turn: awaiting(&s.id),
            },
            last_event: parsed(&s.last_activity_at),
            ..at_desk(s)
        })
    }

    /// The desk whose `DeskNow::key` is `key`.
    fn desk_by_key(&self, inner: &Inner, key: &str) -> Option<DeskNow> {
        let session = inner.owned.get(key).map_or(key, String::as_str);
        self.desk_now(inner, session).filter(|d| d.key == key)
    }

    /// Why a note of `floor` for `desk` waits now. None: it would be handed over.
    pub(super) fn hold_for(inner: &Inner, floor: &str, desk: &DeskNow) -> Option<Hold> {
        let paused = Self::root_key(inner, floor)
            .ok()
            .and_then(|root| inner.store.records.managers.get(&root))
            .is_some_and(|record| record.paused);
        if paused {
            return Some(Hold::Paused);
        }
        match gate::verdict(&desk.desk) {
            Verdict::Hold(hold) => Some(hold),
            Verdict::Deliver(_) => None,
        }
    }

    /// Why one of A Human's own messages for `desk` waits now. None: it would be handed over.
    /// The gate as for a note, except that A Human's own typing does not hold it and a desk
    /// A Human stopped counts as idle.
    pub(super) fn own_hold(desk: &DeskNow) -> Option<Hold> {
        let mut gated = gate::Desk {
            typed_ago: None,
            ..desk.desk
        };
        if desk.stopped && matches!(gated.state, SessionState::Running | SessionState::Thinking) {
            gated.state = SessionState::Idle;
        }
        match gate::verdict(&gated) {
            Verdict::Hold(hold) => Some(hold),
            Verdict::Deliver(_) => None,
        }
    }

    /// Queue a note and return its id (`n-<seq>` from `Store::next_id`).
    pub(super) fn post_note(inner: &mut Inner, note: NewNote) -> Result<String, String> {
        let id = inner.store.next_id("n");
        inner.mail.outbox.push(Item {
            id: id.clone(),
            floor: note.floor,
            from: note.from,
            to: note.to,
            intro: note.intro,
            text: note.text,
            queued_at: now_iso(),
        })?;
        Ok(id)
    }

    /// Remove the waiting notes `gone` is true for and set each one's log entry to `state`.
    pub(super) fn clear_notes(
        inner: &mut Inner,
        gone: impl Fn(&Item) -> bool,
        state: NoteState,
    ) -> usize {
        let taken = inner.mail.outbox.take_all(gone);
        for item in &taken {
            Self::log_note(inner, &item.floor, &item.id, state);
        }
        taken.len()
    }

    /// Set a note's state in its floor's log, when the log has an entry for it. A floor
    /// that has come down is looked for in every record, since note ids are unique.
    pub(super) fn log_note(inner: &mut Inner, floor: &str, note_id: &str, state: NoteState) {
        let root = Self::root_key(inner, floor).ok();
        let entries = inner
            .store
            .records
            .managers
            .iter_mut()
            .filter(|(key, _)| root.as_ref().is_none_or(|r| r == *key))
            .flat_map(|(_, record)| record.log.iter_mut());
        for entry in entries {
            if let LogEvent::Note {
                note_id: id,
                state: old,
                ..
            } = &mut entry.event
                && id == note_id
            {
                *old = state;
            }
        }
    }

    /// Override every note waiting for the desk whose key is `key`: A Human sent it a message.
    pub(super) fn override_notes(&self, inner: &mut Inner, key: &str) {
        let ids: Vec<String> = inner
            .mail
            .outbox
            .items()
            .iter()
            .filter(|i| self.desk_now(inner, &i.to).is_some_and(|d| d.key == key))
            .map(|i| i.id.clone())
            .collect();
        if Self::clear_notes(inner, |i| ids.contains(&i.id), NoteState::Overridden) > 0 {
            inner.mail.dirty = true;
            // The override stands in memory either way; the log is caught up at the next save.
            let _ = inner.store.save();
        }
    }

    /// A Human sent `data` to a session's terminal.
    pub(super) fn human_typed(&self, session_id: &str, data: &str) {
        if !text::is_typing(data) {
            return;
        }
        let mut guard = self.lock();
        let inner = &mut *guard;
        let Some(terminal) = terminal_of(inner, session_id) else {
            return;
        };
        inner.mail.typed.insert(terminal.clone(), Instant::now());
        if text::leaves_draft(data) {
            inner.mail.drafts.insert(terminal.clone());
        } else {
            inner.mail.drafts.remove(&terminal);
        }
        // Enter while the desk waits on a prompt answers the prompt and overrides nothing.
        let answering = self
            .desk_now(inner, session_id)
            .is_some_and(|d| gate::waiting_on_you(&d.desk));
        if text::has_enter(data) && !answering {
            self.override_notes(inner, &terminal);
        }
    }

    /// Whether one of A Human's own messages is waiting for the desk whose key is `key`.
    fn own_waiting(&self, inner: &Inner, key: &str) -> bool {
        inner
            .mail
            .own
            .items()
            .iter()
            .any(|i| self.desk_now(inner, &i.to).is_some_and(|d| d.key == key))
    }

    /// Under the lock: tidy what is waiting, then take out the first message whose desk can
    /// take it now: A Human's own first, then the manager's notes for desks with none of
    /// A Human's waiting. Nothing goes to a desk in `skip`. Sets `changed` when a message
    /// was dropped or an override is pending.
    fn next_hand_over(&self, changed: &mut bool, skip: &[String]) -> Option<HandOver> {
        let mut guard = self.lock();
        let inner = &mut *guard;
        let now = SystemTime::now();
        let picked_up: Vec<String> = inner
            .mail
            .awaiting
            .iter()
            .filter(|(key, at)| {
                self.desk_by_key(inner, key).is_none_or(|d| {
                    (d.desk.state != SessionState::Idle && !d.stopped)
                        || d.last_event.is_some_and(|e| e > **at)
                })
            })
            .map(|(key, _)| key.clone())
            .collect();
        for key in picked_up {
            inner.mail.awaiting.remove(&key);
        }
        let own_left: Vec<String> = inner
            .mail
            .own
            .items()
            .iter()
            .filter(|i| self.desk_now(inner, &i.to).is_none())
            .map(|i| i.id.clone())
            .collect();
        if !inner
            .mail
            .own
            .take_all(|i| own_left.contains(&i.id))
            .is_empty()
        {
            *changed = true;
        }
        let left: Vec<String> = inner
            .mail
            .outbox
            .items()
            .iter()
            .filter(|i| self.desk_now(inner, &i.to).is_none())
            .map(|i| i.id.clone())
            .collect();
        if Self::clear_notes(inner, |i| left.contains(&i.id), NoteState::Dropped) > 0 {
            *changed = true;
        }
        *changed |= std::mem::take(&mut inner.mail.dirty);

        let open = |desk: &DeskNow| !skip.contains(&desk.key);
        let own = inner.mail.own.heads().into_iter().find_map(|head| {
            let desk = self.desk_now(inner, &head.to).filter(open)?;
            Self::own_hold(&desk)
                .is_none()
                .then(|| (head.id.clone(), desk))
        });
        let (item, desk, own) = if let Some((id, desk)) = own {
            (inner.mail.own.take(&id)?, desk, true)
        } else {
            let (id, desk) = inner.mail.outbox.heads().into_iter().find_map(|head| {
                let desk = self.desk_now(inner, &head.to).filter(open)?;
                (!self.own_waiting(inner, &desk.key)
                    && Self::hold_for(inner, &head.floor, &desk).is_none())
                .then(|| (head.id.clone(), desk))
            })?;
            (inner.mail.outbox.take(&id)?, desk, false)
        };
        inner.mail.awaiting.insert(desk.key.clone(), now);
        let bracketed = desk
            .terminal
            .as_ref()
            .is_some_and(|t| self.terminals.bracketed_paste(t).unwrap_or(false));
        Some(HandOver {
            item,
            key: desk.key,
            terminal: desk.terminal,
            session_id: desk.session_id,
            bracketed,
            own,
        })
    }

    /// Hand over every message whose desk can take it now, one at a time. True when anything
    /// the snapshot shows changed.
    pub fn deliver_due(&self) -> bool {
        let mut changed = Self::follow_hire_spend(&mut self.lock());
        // Desks whose hand-over of A Human's message failed in this call: the next call retries.
        let mut skip: Vec<String> = Vec::new();
        while let Some(hand) = self.next_hand_over(&mut changed, &skip) {
            let body = format!("{}{}", hand.item.intro, hand.item.text);
            let handed = match &hand.terminal {
                Some(t) => self
                    .terminals
                    .write(t, &text::typed(&body, hand.bracketed))
                    .and_then(|()| {
                        std::thread::sleep(SUBMIT_PAUSE);
                        self.terminals.write(t, text::SUBMIT)
                    }),
                None => self.structured.prompt(&hand.key, &body),
            };
            let mut guard = self.lock();
            let inner = &mut *guard;
            changed = true;
            if hand.own {
                // A Human's own message is in the transcript: nothing is stored or logged.
                if handed.is_err() {
                    inner.mail.awaiting.remove(&hand.key);
                    inner.mail.own.put_back(hand.item);
                    skip.push(hand.key);
                }
                continue;
            }
            let state = if handed.is_ok() {
                let id = inner.store.next_id("m");
                inner.store.push_message(AgentMessage {
                    id,
                    from_session_id: hand.item.from.clone(),
                    to_session_id: hand.session_id,
                    text: hand.item.text.clone(),
                    sent_at: now_iso(),
                });
                NoteState::Delivered
            } else {
                inner.mail.awaiting.remove(&hand.key);
                NoteState::Failed
            };
            Self::log_note(inner, &hand.item.floor, &hand.item.id, state);
            // The note has gone either way; a failed save is caught up at the next one.
            let _ = inner.store.save();
        }
        changed
    }
}

/// The parts of a desk that come from its session.
fn at_desk(s: &Session) -> DeskNow {
    DeskNow {
        key: s.id.clone(),
        terminal: None,
        session_id: s.id.clone(),
        project_id: s.project_id.clone(),
        title: s.title.clone(),
        desk: gate::Desk {
            route: Route::None(Hold::HandStarted),
            state: s.state,
            prompt_open: false,
            typed_ago: None,
            draft_open: false,
            awaiting_turn: false,
        },
        last_event: None,
        stopped: false,
    }
}

#[cfg(test)]
pub(super) mod tests;
