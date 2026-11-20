//! `send_to_desk`, the Manager panel's withdraw, and what every acting tool shares: the floor's
//! log and the refusals A Human sees there. A note only ever leaves through the outbox.

use super::deliver::NewNote;
use super::manager_tools::{floor_desk, json};
use super::{Inner, Office, now_iso};
use crate::delivery::gate::{Hold, Route};
use crate::delivery::text::{self, MESSAGE_MAX};
use crate::manager::tools::SendToDesk;
use crate::manager::wire::{LOG_KEPT, LOG_TEXT_MAX, LogEntry, LogEvent, NoteState};
use crate::model::ToolKind;
use crate::project_files::truncate;
use serde::Serialize;

/// Typed in front of every note from a manager.
pub(super) const NOTE_INTRO: &str = "Note from this floor's manager (an agent): ";

/// A helper that outlived its manager. Not logged: there is no manager to log for.
pub(super) const NO_SEAT: &str = "your floor has no manager seat";

const TOOL: &str = "send_to_desk";

const ON_ITS_WAY: &str = "The building is handing it over now. list_desks shows what became of it.";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Sent {
    /// "typed", "sent", "on-its-way" or "waiting".
    outcome: &'static str,
    note_id: String,
    /// The desk's session id now.
    desk: String,
    /// Set only when `outcome` is "waiting".
    waiting_for: Option<Hold>,
    /// One sentence: what happened, or why it waits and what ends the wait.
    detail: &'static str,
}

/// Why a note waits for a desk of `tool`, in one sentence for the manager.
pub(super) fn hold_sentence(hold: Hold, tool: ToolKind) -> &'static str {
    match hold {
        Hold::Busy => "It is working. The note is typed when it next goes idle.",
        Hold::WaitingOnYou => {
            "It has a permission prompt or a question open that only the human can answer. The note is typed after the human has answered."
        }
        Hold::Stuck => {
            "It stopped on an error. The note waits until the human gets it going again."
        }
        Hold::Finished => "Its agent has ended, so the note cannot be typed.",
        Hold::YouTyped => {
            "The human typed in its terminal in the last minute or left a line there unsent. The note waits until they press Enter or Ctrl-C there."
        }
        Hold::AwaitingTurn => {
            "Your last note to it was typed and it has not started on it yet. This one follows it."
        }
        Hold::HandStarted => {
            "It was started outside the building, so nothing is typed into it. The human sees the note at the desk."
        }
        Hold::NoIdleSignal => match tool {
            ToolKind::Codex => {
                "Codex has not run the building's hooks, so the building cannot tell when it is idle. The human has to type /hooks in its terminal and trust them once."
            }
            ToolKind::ClaudeCode => {
                "Claude Code has not run the building's hooks yet, so the building cannot tell when it is idle. If that stays, the human should check that hooks are not switched off."
            }
            _ => {
                "Its tool does not tell the building when it is idle, so nothing is typed into it."
            }
        },
        Hold::Paused => "You are paused, so nothing is typed until the human resumes you.",
    }
}

/// `text` cut to what a log entry keeps.
fn kept(text: &str) -> String {
    truncate(text, LOG_TEXT_MAX)
}

/// Add an entry to a floor's log, cutting its text to `LOG_TEXT_MAX` and the log to `LOG_KEPT`.
pub(super) fn log_event(inner: &mut Inner, floor: &str, event: LogEvent) {
    let Ok(root) = Office::root_key(inner, floor) else {
        return;
    };
    let event = match event {
        LogEvent::Note {
            note_id,
            to_session_id,
            to_title,
            text,
            state,
        } => LogEvent::Note {
            note_id,
            to_session_id,
            to_title: kept(&to_title),
            text: kept(&text),
            state,
        },
        LogEvent::Hire { tool, task } => LogEvent::Hire {
            tool,
            task: kept(&task),
        },
        LogEvent::Escalation { title } => LogEvent::Escalation {
            title: kept(&title),
        },
        LogEvent::Refused { tool, reason } => LogEvent::Refused {
            tool,
            reason: kept(&reason),
        },
        LogEvent::End {
            session_id,
            title,
            reason,
            how,
        } => LogEvent::End {
            session_id,
            title: kept(&title),
            reason: kept(&reason),
            how,
        },
    };
    let id = inner.store.next_id("g");
    let log = &mut inner.store.records.managers.entry(root).or_default().log;
    log.push(LogEntry {
        id,
        at: now_iso(),
        event,
    });
    let excess = log.len().saturating_sub(LOG_KEPT);
    log.drain(..excess);
}

/// Refuse a call: log it as `Refused` and hand the reason back as the tool's error.
pub(super) fn refuse(inner: &mut Inner, floor: &str, tool: &str, reason: String) -> String {
    log_event(
        inner,
        floor,
        LogEvent::Refused {
            tool: tool.to_string(),
            reason: reason.clone(),
        },
    );
    // The refusal stands either way; the log is caught up at the next save.
    let _ = inner.store.save();
    reason
}

/// What every tool that acts checks first: the floor has a manager, and it is not paused.
pub(super) fn may_act(
    inner: &mut Inner,
    floor: &str,
    tool: &str,
    paused: &str,
) -> Result<(), String> {
    if !inner.seats.contains_key(floor) {
        return Err(NO_SEAT.to_string());
    }
    if Office::manager_record(inner, floor).paused {
        return Err(refuse(inner, floor, tool, paused.to_string()));
    }
    Ok(())
}

/// What a floor's log says became of a note.
fn note_state(inner: &Inner, floor: &str, note_id: &str) -> Option<NoteState> {
    let root = Office::root_key(inner, floor).ok()?;
    inner
        .store
        .records
        .managers
        .get(&root)?
        .log
        .iter()
        .find_map(|entry| match &entry.event {
            LogEvent::Note {
                note_id: id, state, ..
            } if id == note_id => Some(*state),
            LogEvent::Note { .. }
            | LogEvent::Hire { .. }
            | LogEvent::Escalation { .. }
            | LogEvent::Refused { .. }
            | LogEvent::End { .. } => None,
        })
}

impl Office {
    /// Queue a note for a desk on the manager's floor and hand it over if the desk can take it.
    pub(super) fn send_to_desk(&self, floor: &str, args: &SendToDesk) -> Result<String, String> {
        let (note_id, to, structured) = {
            let mut guard = self.lock();
            let inner = &mut *guard;
            may_act(
                inner,
                floor,
                TOOL,
                "the manager is paused, so nothing is sent",
            )?;
            let desk = match floor_desk(self, inner, floor, &args.desk) {
                Ok(desk) => desk,
                Err(reason) => return Err(refuse(inner, floor, TOOL, reason)),
            };
            let text =
                text::clean(&args.text, MESSAGE_MAX).map_err(|e| refuse(inner, floor, TOOL, e))?;
            let caps = self.manager_caps(inner, floor);
            if Self::hired_sessions(inner, floor).contains(&desk.session_id)
                && caps.spent_usd >= f64::from(caps.spend_ceiling_usd)
            {
                let reason = "the spend ceiling for desks you hired is reached, so nothing more is sent to them";
                return Err(refuse(inner, floor, TOOL, reason.to_string()));
            }
            let structured = desk.desk.route == Route::Structured;
            let note = NewNote {
                floor: floor.to_string(),
                from: Self::manager_session(inner, floor).unwrap_or_default(),
                to: desk.session_id.clone(),
                intro: NOTE_INTRO.to_string(),
                text: text.clone(),
            };
            let note_id =
                Self::post_note(inner, note).map_err(|e| refuse(inner, floor, TOOL, e))?;
            log_event(
                inner,
                floor,
                LogEvent::Note {
                    note_id: note_id.clone(),
                    to_session_id: desk.session_id.clone(),
                    to_title: desk.title,
                    text,
                    state: NoteState::Queued,
                },
            );
            if let Some(seat) = inner.seats.get_mut(floor) {
                seat.sent_to.insert(desk.session_id.clone());
            }
            // The note is queued either way; the log is caught up at the next save.
            let _ = inner.store.save();
            (note_id, desk.session_id, structured)
        };
        self.deliver_due();
        let inner = self.lock();
        let now = self.desk_now(&inner, &to);
        let desk = now.as_ref().map_or(to, |d| d.session_id.clone());
        let (outcome, waiting_for, detail) = if inner.mail.outbox.contains(&note_id) {
            match now.and_then(|d| Self::hold_for(&inner, floor, &d).map(|h| (h, d.tool))) {
                Some((hold, tool)) => ("waiting", Some(hold), hold_sentence(hold, tool)),
                // Nothing holds it, such as one of the human's own messages going first.
                None => ("on-its-way", None, ON_ITS_WAY),
            }
        } else {
            // The refusals are not logged again: the note's own entry holds its state.
            match note_state(&inner, floor, &note_id) {
                Some(NoteState::Delivered) if structured => {
                    ("sent", None, "Given to the desk as its next turn.")
                }
                Some(NoteState::Delivered) => (
                    "typed",
                    None,
                    "Typed into its terminal, and Enter pressed. list_desks shows awaiting-turn until the desk has picked it up.",
                ),
                // The watch pass has taken it and is typing it now.
                Some(NoteState::Queued) => ("on-its-way", None, ON_ITS_WAY),
                Some(NoteState::Failed) => {
                    return Err(
                        "the note could not be typed, because the desk's terminal has gone"
                            .to_string(),
                    );
                }
                Some(NoteState::Dropped) => {
                    return Err(
                        "the desk left before the note could be typed, so it was dropped"
                            .to_string(),
                    );
                }
                Some(NoteState::Overridden) => {
                    return Err(
                        "the human sent that desk a message first, which replaced your note"
                            .to_string(),
                    );
                }
                Some(NoteState::Withdrawn) => return Err("the human withdrew the note".to_string()),
                None => return Err("the note could not be handed over".to_string()),
            }
        };
        json(&Sent {
            outcome,
            note_id,
            desk,
            waiting_for,
            detail,
        })
    }

    /// Withdraw a note the floor's manager has waiting for a desk.
    pub fn withdraw_manager_note(&self, project_id: &str, note_id: &str) -> Result<(), String> {
        let mut guard = self.lock();
        let inner = &mut *guard;
        let ours = inner
            .mail
            .outbox
            .items()
            .iter()
            .any(|item| item.id == note_id && item.floor == project_id);
        if !ours || inner.mail.outbox.take(note_id).is_none() {
            return Err("that note is no longer waiting".to_string());
        }
        Self::log_note(inner, project_id, note_id, NoteState::Withdrawn);
        inner.store.save()
    }
}

#[cfg(test)]
mod tests {
    use super::super::deliver::tests::{event, screen, seat as desk_at};
    use super::super::manager::Hire;
    use super::super::manager::tests::{bind, manager_office, seat, session, wait_for};
    use super::*;
    use crate::manager::tools::{Call, Tools};
    use crate::model::{ControlMode, SessionState};
    use serde::Deserialize;
    use std::fs;

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct TestSent {
        outcome: String,
        note_id: String,
        desk: String,
        waiting_for: Option<Hold>,
        detail: String,
    }

    fn send(office: &Office, floor: &str, desk: &str, text: &str) -> Result<TestSent, String> {
        let call = Call::SendToDesk(SendToDesk {
            desk: desk.to_string(),
            text: text.to_string(),
        });
        office
            .call(floor, call)
            .map(|answer| serde_json::from_str(&answer).unwrap())
    }

    /// The floor's log entries, oldest first.
    fn log(office: &Office, floor: &str) -> Vec<LogEvent> {
        Office::manager_record(&office.lock(), floor)
            .log
            .into_iter()
            .map(|entry| entry.event)
            .collect()
    }

    fn refusals(office: &Office, floor: &str) -> Vec<String> {
        log(office, floor)
            .into_iter()
            .filter_map(|event| match event {
                LogEvent::Refused { tool, reason } if tool == TOOL => Some(reason),
                _ => None,
            })
            .collect()
    }

    fn note_states(office: &Office, floor: &str) -> Vec<NoteState> {
        log(office, floor)
            .into_iter()
            .filter_map(|event| match event {
                LogEvent::Note { state, .. } => Some(state),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn an_idle_desk_gets_the_note_typed_after_the_introduction_and_joins_managed() {
        let (office, dir, floor, registry, _) = manager_office("send-idle");
        let (manager, _) = seat(&office, &dir, &floor, &registry, "/bin/cat");
        bind(&office, &manager, "s-manager", &floor.id);
        let (terminal, id) = desk_at(
            &office,
            &dir,
            &floor,
            ToolKind::ClaudeCode,
            SessionState::Idle,
        );
        event(&office, &id, "Stop", None);

        let sent = send(&office, &floor.id, &id, "/exit\r\nthen \x1b[201~ stop").unwrap();
        assert_eq!(
            (sent.outcome.as_str(), sent.waiting_for, sent.desk.as_str()),
            ("typed", None, id.as_str())
        );
        assert!(sent.detail.contains("awaiting-turn"), "{}", sent.detail);
        wait_for("the note typed", || {
            screen(&office, &terminal).contains("stop")
        });
        let shown = screen(&office, &terminal);
        let typed = shown.lines().find(|l| l.contains("/exit")).unwrap();
        assert!(typed.starts_with(NOTE_INTRO), "{typed}");
        let raw = office.terminals.backlog(&terminal).unwrap().data;
        assert!(!raw.contains("\x1b[201~") && !raw.contains("\x1b[200~"));

        let messages = office.lock().store.records.messages.clone();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].from_session_id, "s-manager");
        assert!(!messages[0].text.contains('\x1b') && !messages[0].text.contains('\r'));
        assert!(messages[0].text.starts_with("/exit"));
        assert_eq!(note_states(&office, &floor.id), vec![NoteState::Delivered]);
        let entry = office
            .snapshot()
            .managers
            .into_iter()
            .find(|m| m.project_id == floor.id)
            .unwrap();
        assert!(entry.managed.contains(&id));
        assert!(sent.note_id.starts_with("n-"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_busy_desk_gets_the_note_queued_with_its_hold() {
        let (office, dir, floor, registry, _) = manager_office("send-busy");
        seat(&office, &dir, &floor, &registry, "/bin/cat");
        let (terminal, id) = desk_at(
            &office,
            &dir,
            &floor,
            ToolKind::ClaudeCode,
            SessionState::Running,
        );
        event(&office, &id, "PreToolUse", None);
        let sent = send(&office, &floor.id, &id, "Take TODO.md.").unwrap();
        assert_eq!(
            (sent.outcome.as_str(), sent.waiting_for, sent.desk.as_str()),
            ("waiting", Some(Hold::Busy), id.as_str())
        );
        assert_eq!(sent.detail, hold_sentence(Hold::Busy, ToolKind::ClaudeCode));
        assert!(office.lock().mail.outbox.contains(&sent.note_id));
        assert_eq!(screen(&office, &terminal), "");
        assert_eq!(note_states(&office, &floor.id), vec![NoteState::Queued]);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn every_refused_send_is_logged_with_its_reason() {
        let (office, dir, floor, registry, _) = manager_office("send-refuse");
        // Before any session is injected: adding a floor rescans.
        let other_root = dir.join("home/code/other");
        fs::create_dir_all(&other_root).unwrap();
        let other = office.add_floor(&other_root.to_string_lossy()).unwrap();
        let (_, busy) = desk_at(
            &office,
            &dir,
            &floor,
            ToolKind::ClaudeCode,
            SessionState::Running,
        );
        assert_eq!(
            send(&office, &floor.id, &busy, "go").err(),
            Some(NO_SEAT.to_string())
        );
        assert!(
            log(&office, &floor.id).is_empty(),
            "no manager, nothing logged"
        );

        seat(&office, &dir, &floor, &registry, "/bin/cat");
        office
            .lock()
            .found
            .sessions
            .push(session("s-there", &other.id, ControlMode::RaiseWindow));
        let no_desk = "there is no such desk on your floor";
        assert_eq!(
            send(&office, &floor.id, "s-there", "go").err(),
            Some(no_desk.to_string())
        );
        assert_eq!(
            send(&office, &floor.id, "s-nobody", "go").err(),
            Some(no_desk.to_string())
        );
        let long = "x".repeat(MESSAGE_MAX + 1);
        let too_long = "that is longer than 4000 characters".to_string();
        assert_eq!(
            send(&office, &floor.id, &busy, &long).err(),
            Some(too_long.clone())
        );
        for n in 0..5 {
            assert_eq!(
                send(&office, &floor.id, &busy, &format!("note {n}"))
                    .unwrap()
                    .outcome,
                "waiting"
            );
        }
        let sixth = "that desk already has 5 notes waiting".to_string();
        assert_eq!(
            send(&office, &floor.id, &busy, "note 6").err(),
            Some(sixth.clone())
        );

        assert_eq!(
            refusals(&office, &floor.id),
            vec![no_desk.to_string(), no_desk.to_string(), too_long, sixth]
        );
        assert!(log(&office, &other.id).is_empty());
        assert!(
            office
                .lock()
                .mail
                .outbox
                .items()
                .iter()
                .all(|i| i.to == busy)
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_hired_desk_at_the_ceiling_and_a_paused_manager_are_refused() {
        let (office, dir, floor, registry, _) = manager_office("send-ceiling");
        seat(&office, &dir, &floor, &registry, "/bin/cat");
        let (_, busy) = desk_at(
            &office,
            &dir,
            &floor,
            ToolKind::ClaudeCode,
            SessionState::Running,
        );
        let (hired, hired_id) = desk_at(
            &office,
            &dir,
            &floor,
            ToolKind::ClaudeCode,
            SessionState::Running,
        );
        let hire = Hire {
            terminal: hired,
            usd: 20.0,
            unpriced: false,
        };
        office
            .lock()
            .seats
            .get_mut(&floor.id)
            .unwrap()
            .hires
            .push(hire);
        let ceiling =
            "the spend ceiling for desks you hired is reached, so nothing more is sent to them";
        assert_eq!(
            send(&office, &floor.id, &hired_id, "go").err(),
            Some(ceiling.to_string())
        );
        let other_desk = send(&office, &floor.id, &busy, "a desk it did not hire").unwrap();
        assert_eq!(other_desk.outcome, "waiting");

        office.set_manager_paused(&floor.id, true).unwrap();
        let paused = "the manager is paused, so nothing is sent";
        assert_eq!(
            send(&office, &floor.id, &busy, "go").err(),
            Some(paused.to_string())
        );
        assert_eq!(refusals(&office, &floor.id), [ceiling, paused]);
        assert_eq!(office.lock().mail.outbox.items().len(), 1);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_waiting_note_can_be_withdrawn_from_its_own_floor_only() {
        let (office, dir, floor, registry, _) = manager_office("send-withdraw");
        seat(&office, &dir, &floor, &registry, "/bin/cat");
        let (_, busy) = desk_at(
            &office,
            &dir,
            &floor,
            ToolKind::ClaudeCode,
            SessionState::Running,
        );
        let sent = send(&office, &floor.id, &busy, "Take TODO.md.").unwrap();
        let other_note = {
            let mut inner = office.lock();
            let note = NewNote {
                floor: "p-other".to_string(),
                from: String::new(),
                to: "s-there".to_string(),
                intro: String::new(),
                text: "theirs".to_string(),
            };
            Office::post_note(&mut inner, note).unwrap()
        };
        let gone = Err("that note is no longer waiting".to_string());
        assert_eq!(office.withdraw_manager_note(&floor.id, &other_note), gone);
        assert_eq!(office.withdraw_manager_note(&floor.id, "n-404"), gone);
        office
            .withdraw_manager_note(&floor.id, &sent.note_id)
            .unwrap();
        assert_eq!(office.withdraw_manager_note(&floor.id, &sent.note_id), gone);
        let left: Vec<String> = office
            .lock()
            .mail
            .outbox
            .items()
            .iter()
            .map(|i| i.id.clone())
            .collect();
        assert_eq!(left, vec![other_note]);
        assert_eq!(note_states(&office, &floor.id), vec![NoteState::Withdrawn]);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_hand_started_or_unhooked_desk_answers_waiting_with_the_reason() {
        let (office, dir, floor, registry, _) = manager_office("send-why");
        seat(&office, &dir, &floor, &registry, "/bin/cat");
        office
            .lock()
            .found
            .sessions
            .push(session("s-hand", &floor.id, ControlMode::RaiseWindow));
        let hand = send(&office, &floor.id, "s-hand", "go").unwrap();
        assert_eq!(
            (hand.outcome.as_str(), hand.waiting_for),
            ("waiting", Some(Hold::HandStarted))
        );
        let (_, codex) = desk_at(&office, &dir, &floor, ToolKind::Codex, SessionState::Idle);
        let quiet = send(&office, &floor.id, &codex, "go").unwrap();
        assert_eq!(
            (quiet.outcome.as_str(), quiet.waiting_for),
            ("waiting", Some(Hold::NoIdleSignal))
        );
        assert!(quiet.detail.contains("/hooks"), "{}", quiet.detail);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn every_hold_has_a_sentence_and_no_idle_signal_differs_by_tool() {
        let holds = [
            Hold::Busy,
            Hold::WaitingOnYou,
            Hold::Stuck,
            Hold::Finished,
            Hold::YouTyped,
            Hold::AwaitingTurn,
            Hold::HandStarted,
            Hold::NoIdleSignal,
            Hold::Paused,
        ];
        let tools = [ToolKind::ClaudeCode, ToolKind::Codex, ToolKind::Opencode];
        for hold in holds {
            for tool in tools {
                assert!(!hold_sentence(hold, tool).is_empty(), "{hold:?} {tool:?}");
            }
        }
        let quiet: Vec<&str> = tools
            .iter()
            .map(|t| hold_sentence(Hold::NoIdleSignal, *t))
            .collect();
        assert!(quiet[0] != quiet[1] && quiet[1] != quiet[2] && quiet[0] != quiet[2]);
    }

    #[test]
    fn the_log_keeps_its_newest_entries_and_cuts_their_text() {
        let (office, dir, floor) = super::super::tests::floor_office("send-log");
        let mut inner = office.lock();
        for n in 0..LOG_KEPT + 5 {
            let reason = if n == 0 {
                "y".repeat(LOG_TEXT_MAX + 50)
            } else {
                format!("r{n}")
            };
            log_event(
                &mut inner,
                &floor.id,
                LogEvent::Refused {
                    tool: TOOL.to_string(),
                    reason,
                },
            );
        }
        let kept = Office::manager_record(&inner, &floor.id).log;
        assert_eq!(kept.len(), LOG_KEPT);
        assert!(matches!(&kept[0].event, LogEvent::Refused { reason, .. } if reason == "r5"));
        log_event(
            &mut inner,
            &floor.id,
            LogEvent::Escalation {
                title: "z".repeat(LOG_TEXT_MAX + 1),
            },
        );
        let last = Office::manager_record(&inner, &floor.id).log.pop().unwrap();
        assert!(
            matches!(&last.event, LogEvent::Escalation { title } if title.chars().count() == LOG_TEXT_MAX)
        );
        drop(inner);
        fs::remove_dir_all(&dir).unwrap();
    }
}
