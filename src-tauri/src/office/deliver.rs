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
    fn override_notes(&self, inner: &mut Inner, key: &str) {
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

    /// A Human sent a structured desk a message.
    pub(super) fn human_prompted(&self, session_id: &str) {
        let mut guard = self.lock();
        self.override_notes(&mut guard, session_id);
    }

    /// Under the lock: tidy what is waiting, then take out the first note whose desk can
    /// take it now. Sets `changed` when a note was dropped or an override is pending.
    fn next_hand_over(&self, changed: &mut bool) -> Option<HandOver> {
        let mut guard = self.lock();
        let inner = &mut *guard;
        let now = SystemTime::now();
        let picked_up: Vec<String> = inner
            .mail
            .awaiting
            .iter()
            .filter(|(key, at)| {
                self.desk_by_key(inner, key).is_none_or(|d| {
                    d.desk.state != SessionState::Idle || d.last_event.is_some_and(|e| e > **at)
                })
            })
            .map(|(key, _)| key.clone())
            .collect();
        for key in picked_up {
            inner.mail.awaiting.remove(&key);
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

        let (id, desk) = inner.mail.outbox.heads().into_iter().find_map(|head| {
            let desk = self.desk_now(inner, &head.to)?;
            Self::hold_for(inner, &head.floor, &desk)
                .is_none()
                .then(|| (head.id.clone(), desk))
        })?;
        let item = inner.mail.outbox.take(&id)?;
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
        })
    }

    /// Hand over every note whose desk can take it now, one at a time. True when anything
    /// the snapshot shows changed.
    pub fn deliver_due(&self) -> bool {
        let mut changed = Self::follow_hire_spend(&mut self.lock());
        while let Some(hand) = self.next_hand_over(&mut changed) {
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
            changed = true;
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
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::super::tests::floor_office;
    use super::*;
    use crate::manager::wire::LogEntry;
    use crate::model::{Project, Spend};
    use crate::structured::desks::DeskInfo;
    use crate::structured::{Driver, Values};
    use portable_pty::CommandBuilder;
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    const INTRO: &str = "Note from the test: ";

    fn wait_for(what: &str, mut done: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !done() {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn session(id: &str, floor: &Project, tool: ToolKind, control: ControlMode) -> Session {
        Session {
            id: id.to_string(),
            tool,
            project_id: floor.id.clone(),
            title: format!("desk {id}"),
            state: SessionState::Idle,
            control,
            model: None,
            effort: None,
            last_activity_at: "2026-11-12T10:00:00Z".to_string(),
            pending_question_ids: Vec::new(),
            spend: Spend::default(),
            helpers: Vec::new(),
            pets: Vec::new(),
            worktree: false,
            branch: None,
        }
    }

    /// A desk running `/bin/cat` in an app terminal, bound to an injected session of `tool`
    /// in `state`. Returns the terminal and the session id.
    pub(in super::super) fn seat(
        office: &Office,
        dir: &std::path::Path,
        floor: &Project,
        tool: ToolKind,
        state: SessionState,
    ) -> (String, String) {
        let terminal = office
            .terminals
            .spawn(CommandBuilder::new("/bin/cat"), &dir.join("home/code/app"))
            .unwrap();
        let id = format!("s-{terminal}");
        let mut inner = office.lock();
        inner.owned.insert(terminal.clone(), id.clone());
        inner.ran_in.insert(id.clone(), terminal.clone());
        let mut s = session(&id, floor, tool, ControlMode::Full);
        s.state = state;
        inner.found.sessions.push(s);
        (terminal, id)
    }

    /// Drop a hook event into the feed's folder as a tool would, and collect it.
    pub(in super::super) fn event(
        office: &Office,
        session_id: &str,
        name: &str,
        notification: Option<&str>,
    ) {
        static SEQ: AtomicUsize = AtomicUsize::new(0);
        // A newer event must carry a later time than a hand-over just before it.
        std::thread::sleep(Duration::from_millis(15));
        let dir = office.lock().feed.dir().to_path_buf();
        fs::create_dir_all(&dir).unwrap();
        let kind =
            notification.map_or_else(String::new, |n| format!(r#","notification_type":"{n}""#));
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        fs::write(
            dir.join(format!("{name}.{n:06}")),
            format!(r#"{{"session_id":"{session_id}"{kind}}}"#),
        )
        .unwrap();
        office.lock().feed.collect(SystemTime::now());
    }

    /// Queue a note as a sender would, with its log entry.
    fn queue(office: &Office, floor: &Project, to: &str, text: &str) -> String {
        let mut guard = office.lock();
        let inner = &mut *guard;
        let note = NewNote {
            floor: floor.id.clone(),
            from: "boss".to_string(),
            to: to.to_string(),
            intro: INTRO.to_string(),
            text: text.to_string(),
        };
        let id = Office::post_note(inner, note).unwrap();
        let root = Office::root_key(inner, &floor.id).unwrap();
        let at = now_iso();
        inner
            .store
            .records
            .managers
            .entry(root)
            .or_default()
            .log
            .push(LogEntry {
                id: format!("g-{id}"),
                at,
                event: LogEvent::Note {
                    note_id: id.clone(),
                    to_session_id: to.to_string(),
                    to_title: "a desk".to_string(),
                    text: text.to_string(),
                    state: NoteState::Queued,
                },
            });
        id
    }

    fn state_of(office: &Office, note: &str) -> Option<NoteState> {
        let inner = office.lock();
        inner
            .store
            .records
            .managers
            .values()
            .flat_map(|r| r.log.iter())
            .find_map(|entry| match &entry.event {
                LogEvent::Note { note_id, state, .. } if note_id == note => Some(*state),
                _ => None,
            })
    }

    fn hold_of(office: &Office, floor: &Project, session_id: &str) -> Option<Hold> {
        let inner = office.lock();
        let desk = office.desk_now(&inner, session_id).unwrap();
        Office::hold_for(&inner, &floor.id, &desk)
    }

    fn waiting(office: &Office) -> Vec<String> {
        office
            .lock()
            .mail
            .outbox
            .items()
            .iter()
            .map(|i| i.id.clone())
            .collect()
    }

    fn messages(office: &Office) -> Vec<AgentMessage> {
        office.lock().store.records.messages.clone()
    }

    pub(in super::super) fn screen(office: &Office, terminal: &str) -> String {
        crate::pty::plain_text(&office.terminals.backlog(terminal).unwrap().data)
    }

    fn set_paused(office: &Office, floor: &Project, paused: bool) {
        let mut guard = office.lock();
        let root = Office::root_key(&guard, &floor.id).unwrap();
        guard.store.records.managers.entry(root).or_default().paused = paused;
    }

    #[test]
    fn an_idle_desk_gets_the_note_typed_stored_and_logged() {
        let (office, dir, floor) = floor_office("deliver-idle");
        let (terminal, id) = seat(
            &office,
            &dir,
            &floor,
            ToolKind::ClaudeCode,
            SessionState::Idle,
        );
        event(&office, &id, "Stop", None);
        let note = queue(&office, &floor, &id, "Take the first item of TODO.md.");
        assert_eq!(hold_of(&office, &floor, &id), None);
        assert!(office.deliver_due());
        // The terminal echoes the typing, and cat prints the line back once Enter submits it.
        wait_for("the note typed and submitted", || {
            screen(&office, &terminal)
                .matches("Note from the test: Take the first item of TODO.md.")
                .count()
                == 2
        });
        let sent = messages(&office);
        assert_eq!(sent.len(), 1);
        assert_eq!(
            (
                sent[0].from_session_id.as_str(),
                sent[0].to_session_id.as_str(),
                sent[0].text.as_str()
            ),
            ("boss", id.as_str(), "Take the first item of TODO.md.")
        );
        assert_eq!(state_of(&office, &note), Some(NoteState::Delivered));
        assert!(waiting(&office).is_empty());
        assert!(!office.deliver_due(), "nothing more to do");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_busy_stuck_or_finished_desk_waits() {
        let (office, dir, floor) = floor_office("deliver-busy");
        for (state, hold) in [
            (SessionState::Running, Hold::Busy),
            (SessionState::Thinking, Hold::Busy),
            (SessionState::Error, Hold::Stuck),
            (SessionState::Done, Hold::Finished),
        ] {
            let (terminal, id) = seat(&office, &dir, &floor, ToolKind::ClaudeCode, state);
            event(&office, &id, "Stop", None);
            let note = queue(&office, &floor, &id, "go");
            assert!(!office.deliver_due(), "{state:?}");
            assert_eq!(hold_of(&office, &floor, &id), Some(hold), "{state:?}");
            assert!(waiting(&office).contains(&note));
            assert_eq!(screen(&office, &terminal), "");
        }
        // The session's files say idle but the feed's newest event says it is working.
        let (_, id) = seat(
            &office,
            &dir,
            &floor,
            ToolKind::ClaudeCode,
            SessionState::Idle,
        );
        event(&office, &id, "PreToolUse", None);
        queue(&office, &floor, &id, "go");
        assert_eq!(hold_of(&office, &floor, &id), Some(Hold::Busy));
        assert!(!office.deliver_due());
        assert!(messages(&office).is_empty());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_desk_waiting_on_a_prompt_or_a_question_is_never_typed_into() {
        let (office, dir, floor) = floor_office("deliver-prompt");
        let (prompted, a) = seat(
            &office,
            &dir,
            &floor,
            ToolKind::ClaudeCode,
            SessionState::Idle,
        );
        event(&office, &a, "Notification", Some("permission_prompt"));
        let (asked, b) = seat(&office, &dir, &floor, ToolKind::Codex, SessionState::Idle);
        event(&office, &b, "Stop", None);
        {
            let mut inner = office.lock();
            let s = inner.found.sessions.iter_mut().find(|s| s.id == b).unwrap();
            s.pending_question_ids.push("q-1".to_string());
        }
        queue(&office, &floor, &a, "go");
        queue(&office, &floor, &b, "go");
        for _ in 0..4 {
            assert!(!office.deliver_due());
            assert_eq!(hold_of(&office, &floor, &a), Some(Hold::WaitingOnYou));
            assert_eq!(hold_of(&office, &floor, &b), Some(Hold::WaitingOnYou));
            std::thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(screen(&office, &prompted), "");
        assert_eq!(screen(&office, &asked), "");
        assert_eq!(waiting(&office).len(), 2);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn desks_with_no_idle_signal_or_started_by_hand_are_not_typed_into() {
        let (office, dir, floor) = floor_office("deliver-route");
        let (quiet, a) = seat(
            &office,
            &dir,
            &floor,
            ToolKind::ClaudeCode,
            SessionState::Idle,
        );
        let (hookless, b) = seat(
            &office,
            &dir,
            &floor,
            ToolKind::Antigravity,
            SessionState::Idle,
        );
        event(&office, &b, "Stop", None);
        let c = "by-hand".to_string();
        office.lock().found.sessions.push(session(
            &c,
            &floor,
            ToolKind::ClaudeCode,
            ControlMode::RaiseWindow,
        ));
        for to in [&a, &b, &c] {
            queue(&office, &floor, to, "go");
        }
        assert!(!office.deliver_due());
        assert_eq!(hold_of(&office, &floor, &a), Some(Hold::NoIdleSignal));
        assert_eq!(hold_of(&office, &floor, &b), Some(Hold::NoIdleSignal));
        assert_eq!(hold_of(&office, &floor, &c), Some(Hold::HandStarted));
        let inner = office.lock();
        let hand = office.desk_now(&inner, &c).unwrap();
        assert_eq!((hand.key.as_str(), hand.terminal), (c.as_str(), None));
        assert_eq!(hand.title, "desk by-hand");
        drop(inner);
        assert_eq!(screen(&office, &quiet), "");
        assert_eq!(screen(&office, &hookless), "");
        assert!(messages(&office).is_empty());
        // Once the feed reports the first desk idle, its note goes.
        event(&office, &a, "Stop", None);
        assert!(office.deliver_due());
        assert_eq!(messages(&office).len(), 1);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_paused_floor_holds_its_notes_and_resuming_hands_them_over() {
        let (office, dir, floor) = floor_office("deliver-pause");
        let (_, id) = seat(
            &office,
            &dir,
            &floor,
            ToolKind::ClaudeCode,
            SessionState::Idle,
        );
        event(&office, &id, "Stop", None);
        set_paused(&office, &floor, true);
        let note = queue(&office, &floor, &id, "go");
        assert!(!office.deliver_due());
        assert_eq!(hold_of(&office, &floor, &id), Some(Hold::Paused));
        set_paused(&office, &floor, false);
        assert!(office.deliver_due());
        assert_eq!(state_of(&office, &note), Some(NoteState::Delivered));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_keystroke_holds_a_terminal_and_a_terminals_own_report_does_not() {
        let (office, dir, floor) = floor_office("deliver-typed");
        let (_, a) = seat(
            &office,
            &dir,
            &floor,
            ToolKind::ClaudeCode,
            SessionState::Idle,
        );
        let (_, b) = seat(
            &office,
            &dir,
            &floor,
            ToolKind::ClaudeCode,
            SessionState::Idle,
        );
        event(&office, &a, "Stop", None);
        event(&office, &b, "Stop", None);
        office.terminal_write(&a, "a").unwrap();
        office.terminal_write(&b, "\x1b[I").unwrap();
        office.terminal_write(&b, "\x1b[<0;10;5M").unwrap();
        let held = queue(&office, &floor, &a, "go");
        let sent = queue(&office, &floor, &b, "go");
        assert_eq!(hold_of(&office, &floor, &a), Some(Hold::YouTyped));
        assert!(office.deliver_due());
        assert_eq!(state_of(&office, &held), Some(NoteState::Queued));
        assert_eq!(state_of(&office, &sent), Some(NoteState::Delivered));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn enter_from_a_human_overrides_the_waiting_notes_but_not_while_answering_a_prompt() {
        let (office, dir, floor) = floor_office("deliver-override");
        let (_, busy) = seat(
            &office,
            &dir,
            &floor,
            ToolKind::ClaudeCode,
            SessionState::Running,
        );
        event(&office, &busy, "PreToolUse", None);
        let first = queue(&office, &floor, &busy, "one");
        let second = queue(&office, &floor, &busy, "two");
        assert!(!office.deliver_due());
        office.terminal_write(&busy, "do this instead").unwrap();
        assert_eq!(waiting(&office).len(), 2, "no Enter yet");
        office.terminal_write(&busy, "\r").unwrap();
        assert!(waiting(&office).is_empty());
        assert_eq!(state_of(&office, &first), Some(NoteState::Overridden));
        assert_eq!(state_of(&office, &second), Some(NoteState::Overridden));
        assert!(office.deliver_due(), "the override shows");
        assert!(!office.deliver_due());

        let (_, asking) = seat(
            &office,
            &dir,
            &floor,
            ToolKind::ClaudeCode,
            SessionState::Idle,
        );
        event(&office, &asking, "Notification", Some("permission_prompt"));
        let kept = queue(&office, &floor, &asking, "three");
        office.terminal_write(&asking, "1\r").unwrap();
        assert_eq!(waiting(&office), vec![kept.clone()]);
        assert_eq!(state_of(&office, &kept), Some(NoteState::Queued));
        assert!(!office.deliver_due());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_next_note_waits_until_the_last_one_is_picked_up() {
        let (office, dir, floor) = floor_office("deliver-turn");
        let (_, id) = seat(&office, &dir, &floor, ToolKind::Codex, SessionState::Idle);
        event(&office, &id, "Stop", None);
        let first = queue(&office, &floor, &id, "one");
        let second = queue(&office, &floor, &id, "two");
        assert!(office.deliver_due());
        assert_eq!(state_of(&office, &first), Some(NoteState::Delivered));
        assert_eq!(hold_of(&office, &floor, &id), Some(Hold::AwaitingTurn));
        assert!(!office.deliver_due());
        assert_eq!(waiting(&office), vec![second.clone()]);
        event(&office, &id, "Stop", None);
        assert!(office.deliver_due());
        assert_eq!(state_of(&office, &second), Some(NoteState::Delivered));
        assert_eq!(messages(&office).len(), 2);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_note_for_a_desk_that_left_is_dropped() {
        let (office, dir, floor) = floor_office("deliver-left");
        let (terminal, id) = seat(
            &office,
            &dir,
            &floor,
            ToolKind::ClaudeCode,
            SessionState::Running,
        );
        let note = queue(&office, &floor, &id, "go");
        assert!(!office.deliver_due());
        office.terminals.write(&terminal, "\u{4}").unwrap();
        office
            .terminals
            .wait_exit(&terminal, Duration::from_secs(10))
            .unwrap()
            .unwrap();
        assert!(office.deliver_due(), "a dropped note shows");
        assert_eq!(state_of(&office, &note), Some(NoteState::Dropped));
        assert!(waiting(&office).is_empty());
        assert!(!office.deliver_due());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn two_passes_at_once_type_a_note_once() {
        let (office, dir, floor) = floor_office("deliver-race");
        let (terminal, id) = seat(
            &office,
            &dir,
            &floor,
            ToolKind::ClaudeCode,
            SessionState::Idle,
        );
        event(&office, &id, "Stop", None);
        queue(&office, &floor, &id, "only once please");
        let handed: Vec<bool> = std::thread::scope(|scope| {
            let passes: Vec<_> = (0..2)
                .map(|_| scope.spawn(|| office.deliver_due()))
                .collect();
            passes.into_iter().map(|p| p.join().unwrap()).collect()
        });
        assert!(handed.contains(&true));
        assert_eq!(messages(&office).len(), 1);
        wait_for("the note on the screen", || {
            screen(&office, &terminal).contains("only once please")
        });
        std::thread::sleep(Duration::from_millis(300));
        // The terminal echoes the line and cat prints it back: twice for one typing.
        assert!(
            screen(&office, &terminal)
                .matches("only once please")
                .count()
                <= 2
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    /// Records the prompts a structured desk is given.
    struct Recorder(Arc<Mutex<Vec<String>>>);

    impl Driver for Recorder {
        fn prompt(&self, text: &str) -> Result<(), String> {
            crate::sync::lock(&self.0).push(text.to_string());
            Ok(())
        }
        fn answer(&self, _: &str, _: &str, _: &Values) -> Result<(), String> {
            Ok(())
        }
        fn cancel(&self) -> Result<(), String> {
            Ok(())
        }
        fn stop(&self) {}
    }

    #[test]
    fn a_structured_desk_gets_its_note_as_a_turn_only_when_idle() {
        let (office, dir, floor) = floor_office("deliver-structured");
        let prompts = Arc::new(Mutex::new(Vec::new()));
        let seat_desk = |state| {
            let info = DeskInfo {
                tool: ToolKind::Codex,
                project_id: floor.id.clone(),
                root: dir.join("home/code/app"),
                title: "structured".to_string(),
                model: "default".to_string(),
                effort: "default".to_string(),
            };
            let driver = Box::new(Recorder(Arc::clone(&prompts)));
            office.structured.seat("structured-9", info, state, driver);
        };
        seat_desk(SessionState::Idle);
        let first = queue(&office, &floor, "structured-9", "one");
        assert!(office.deliver_due());
        assert_eq!(
            *crate::sync::lock(&prompts),
            ["Note from the test: one".to_string()]
        );
        assert_eq!(state_of(&office, &first), Some(NoteState::Delivered));
        assert_eq!(messages(&office)[0].to_session_id, "structured-9");

        seat_desk(SessionState::Thinking);
        let second = queue(&office, &floor, "structured-9", "two");
        assert!(!office.deliver_due());
        assert_eq!(hold_of(&office, &floor, "structured-9"), Some(Hold::Busy));

        office.structured_prompt("structured-9", "mine").unwrap();
        assert_eq!(state_of(&office, &second), Some(NoteState::Overridden));
        assert!(waiting(&office).is_empty());
        assert_eq!(crate::sync::lock(&prompts).len(), 2);
        assert!(office.deliver_due(), "the override shows");
        fs::remove_dir_all(&dir).unwrap();
    }
}
