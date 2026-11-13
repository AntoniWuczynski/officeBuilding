//! One pass over the desks: what each has to say about a handover, and which to hand
//! over now. Pure: `now` and every other input is passed in.

use super::decide::{Decision, Facts, decide};
use super::record::{Book, Done, Outcome, UsedUp};
use super::view::{DeskHandover, DeskState, HandoverView, UsedUpView};
use super::work::WorkLog;
use super::{Tool, Work};
use crate::settings::HandoverSettings;
use std::collections::BTreeSet;

pub struct DeskFacts<'a> {
    pub session_id: &'a str,
    pub tool: Tool,
    pub ours: bool,
}

pub struct Survey {
    pub view: HandoverView,
    /// Desks to hand over now, without asking.
    pub hand_over: Vec<String>,
}

pub struct Now<'a> {
    pub now: i64,
    pub settings: &'a HandoverSettings,
    /// Desks whose automatic decision is due in this pass.
    pub fresh: &'a BTreeSet<String>,
    /// Desks whose own proof was read less than the settle time ago: nothing is shown for them yet.
    pub pending: &'a BTreeSet<String>,
    /// Desks being handed over right now.
    pub underway: &'a BTreeSet<String>,
    /// The sessions on the sign-out sheets.
    pub ended: &'a [&'a str],
}

fn used_up_view(record: &UsedUp) -> UsedUpView {
    UsedUpView {
        tool: record.tool,
        window: record.window,
        reset_at: record.reset_at,
        seen_at: record.seen_at,
    }
}

/// The newest handover that started `session_id`'s successor.
fn arrival<'a>(book: &'a Book, session_id: &str) -> Option<&'a Done> {
    book.done()
        .iter()
        .filter(|d| matches!(&d.outcome, Outcome::Started { session: Some(s) } if s == session_id))
        .max_by_key(|d| d.at)
}

fn desk_state(
    book: &Book,
    works: &WorkLog,
    desk: &DeskFacts<'_>,
    at: &Now<'_>,
) -> (Option<DeskState>, bool) {
    let id = desk.session_id;
    let to = desk.tool.other();
    if at.underway.contains(id) {
        return (Some(DeskState::Underway { to }), false);
    }
    let record = book
        .own(id, at.now)
        .or_else(|| book.live(desk.tool, at.now));
    let work = match desk.tool {
        Tool::ClaudeCode => works.of(id),
        Tool::Codex => Work::Unknown,
    };
    let receiver = book.live(to, at.now);
    let facts = Facts {
        ours: desk.ours,
        used_up: record.is_some(),
        own: book.own(id, at.now).is_some(),
        fresh: at.fresh.contains(id),
        automatic: at.settings.automatic,
        chosen: at.settings.start_with(desk.tool).is_some(),
        receiver_used_up: receiver.is_some(),
        before: record.and_then(|r| book.before(id, r)),
        work: &work,
    };
    let (window, reset_at) = record.map_or((None, None), |r| (r.window, r.reset_at));
    match decide(&facts) {
        Decision::HandOver => (Some(DeskState::Underway { to }), true),
        Decision::Offer(why) => (
            Some(DeskState::Offered {
                to,
                why,
                work,
                window,
                reset_at,
            }),
            false,
        ),
        Decision::Stay(why) => (
            Some(DeskState::Blocked {
                to,
                why,
                window,
                reset_at,
                other_reset_at: receiver.and_then(|r| r.reset_at),
            }),
            false,
        ),
        Decision::Nothing => (
            arrival(book, id).map(|d| DeskState::Arrived {
                from: d.from,
                at: d.at,
                window: d.window,
                reset_at: d.reset_at,
            }),
            false,
        ),
    }
}

/// The sign-out line of a session a handover ended, if it has one.
fn sign_out(book: &Book, session_id: &str) -> Option<DeskState> {
    let d = book
        .done()
        .iter()
        .filter(|d| d.from_session == session_id)
        .max_by_key(|d| d.at)?;
    match &d.outcome {
        Outcome::Started { .. } => Some(DeskState::Left { to: d.to, at: d.at }),
        Outcome::Failed {
            reason,
            ended: true,
        } => Some(DeskState::Failed {
            to: d.to,
            at: d.at,
            reason: reason.clone(),
        }),
        Outcome::Failed { ended: false, .. } => None,
    }
}

#[must_use]
pub fn survey(book: &Book, works: &WorkLog, desks: &[DeskFacts<'_>], at: &Now<'_>) -> Survey {
    let used_up = [Tool::ClaudeCode, Tool::Codex]
        .into_iter()
        .filter_map(|tool| book.live(tool, at.now))
        .map(used_up_view)
        .collect();
    let mut view = HandoverView {
        used_up,
        desks: Vec::new(),
    };
    let mut hand_over = Vec::new();
    for desk in desks.iter().filter(|d| !at.pending.contains(d.session_id)) {
        let (state, now_handed) = desk_state(book, works, desk, at);
        if let Some(state) = state
            && !view.desks.iter().any(|d| d.session_id == desk.session_id)
        {
            view.desks.push(DeskHandover {
                session_id: desk.session_id.to_string(),
                state,
            });
            if now_handed {
                hand_over.push(desk.session_id.to_string());
            }
        }
    }
    for id in at.ended {
        if view.desks.iter().any(|d| d.session_id == *id) {
            continue;
        }
        if let Some(state) = sign_out(book, id) {
            view.desks.push(DeskHandover {
                session_id: (*id).to_string(),
                state,
            });
        }
    }
    Survey { view, hand_over }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handover::view::{BlockWhy, OfferWhy};
    use crate::settings::StartWith;

    const NOW: i64 = 1_000_000;
    const RESET: i64 = NOW + 3600;

    fn record(tool: Tool, session: &str, reset: Option<i64>) -> UsedUp {
        UsedUp {
            tool,
            window: Some(Window::FiveHour),
            reset_at: reset,
            seen_at: NOW - 60,
            session_id: session.into(),
        }
    }

    fn chosen() -> HandoverSettings {
        HandoverSettings {
            automatic: true,
            claude_to_codex: Some(StartWith::ToolDefault),
            codex_to_claude: Some(StartWith::ToolDefault),
        }
    }

    fn set(ids: &[&str]) -> BTreeSet<String> {
        ids.iter().map(|s| (*s).to_string()).collect()
    }

    fn clear_log(ids: &[&str]) -> WorkLog {
        let mut log = WorkLog::default();
        for id in ids {
            log.note(crate::handover::work::Signal::Listed {
                session_id: (*id).into(),
                at: std::time::SystemTime::UNIX_EPOCH,
                tasks: Some(vec![]),
            });
        }
        log
    }

    fn claude(id: &str) -> DeskFacts<'_> {
        DeskFacts {
            session_id: id,
            tool: Tool::ClaudeCode,
            ours: true,
        }
    }

    struct Case {
        settings: HandoverSettings,
        fresh: BTreeSet<String>,
        pending: BTreeSet<String>,
        underway: BTreeSet<String>,
    }

    impl Case {
        fn new() -> Self {
            Self {
                settings: chosen(),
                fresh: set(&["a"]),
                pending: set(&[]),
                underway: set(&[]),
            }
        }

        fn run(
            &self,
            book: &Book,
            log: &WorkLog,
            desks: &[DeskFacts<'_>],
            ended: &[&str],
        ) -> Survey {
            survey(
                book,
                log,
                desks,
                &Now {
                    now: NOW,
                    settings: &self.settings,
                    fresh: &self.fresh,
                    pending: &self.pending,
                    underway: &self.underway,
                    ended,
                },
            )
        }
    }

    fn book_with(records: Vec<UsedUp>) -> Book {
        let mut book = Book::default();
        for r in records {
            book.saw(r);
        }
        book
    }

    fn done(from_session: &str, outcome: Outcome, at: i64) -> Done {
        Done {
            id: "h-1".into(),
            from_session: from_session.into(),
            from: Tool::ClaudeCode,
            to: Tool::Codex,
            at,
            window: Some(Window::FiveHour),
            reset_at: Some(RESET),
            seen_at: NOW - 60,
            outcome,
        }
    }

    use crate::handover::Window;

    fn state<'a>(s: &'a Survey, id: &str) -> Option<&'a DeskState> {
        s.view
            .desks
            .iter()
            .find(|d| d.session_id == id)
            .map(|d| &d.state)
    }

    #[test]
    fn one_used_up_entry_per_tool() {
        let book = book_with(vec![
            record(Tool::ClaudeCode, "a", Some(RESET)),
            record(Tool::ClaudeCode, "b", Some(RESET + 100)),
            record(Tool::Codex, "c", Some(RESET + 5)),
        ]);
        let s = Case::new().run(&book, &WorkLog::default(), &[], &[]);
        let tools: Vec<_> = s
            .view
            .used_up
            .iter()
            .map(|u| (u.tool, u.reset_at))
            .collect();
        assert_eq!(
            tools,
            vec![
                (Tool::ClaudeCode, Some(RESET + 100)),
                (Tool::Codex, Some(RESET + 5))
            ]
        );
        assert!(s.hand_over.is_empty());
    }

    #[test]
    fn each_decision_gives_its_state() {
        let book = book_with(vec![record(Tool::ClaudeCode, "a", Some(RESET))]);
        let log = clear_log(&["a"]);
        let desks = [claude("a")];

        let s = Case::new().run(&book, &log, &desks, &[]);
        assert_eq!(
            state(&s, "a"),
            Some(&DeskState::Underway { to: Tool::Codex })
        );
        assert_eq!(s.hand_over, vec!["a".to_string()]);

        let mut c = Case::new();
        c.settings.automatic = false;
        let s = c.run(&book, &log, &desks, &[]);
        assert_eq!(
            state(&s, "a"),
            Some(&DeskState::Offered {
                to: Tool::Codex,
                why: OfferWhy::AutomaticOff,
                work: Work::Clear,
                window: Some(Window::FiveHour),
                reset_at: Some(RESET),
            })
        );
        assert!(s.hand_over.is_empty());

        let mut c = Case::new();
        c.settings.claude_to_codex = None;
        let s = c.run(&book, &log, &desks, &[]);
        assert_eq!(
            state(&s, "a"),
            Some(&DeskState::Blocked {
                to: Tool::Codex,
                why: BlockWhy::NotChosen,
                window: Some(Window::FiveHour),
                reset_at: Some(RESET),
                other_reset_at: None,
            })
        );
    }

    #[test]
    fn both_used_up_names_the_receivers_reset() {
        let book = book_with(vec![
            record(Tool::ClaudeCode, "a", Some(RESET)),
            record(Tool::Codex, "x", Some(RESET + 900)),
        ]);
        let s = Case::new().run(&book, &clear_log(&["a"]), &[claude("a")], &[]);
        assert_eq!(
            state(&s, "a"),
            Some(&DeskState::Blocked {
                to: Tool::Codex,
                why: BlockWhy::BothUsedUp,
                window: Some(Window::FiveHour),
                reset_at: Some(RESET),
                other_reset_at: Some(RESET + 900),
            })
        );
    }

    #[test]
    fn a_record_seen_elsewhere_is_offered_and_never_automatic() {
        let book = book_with(vec![record(Tool::ClaudeCode, "other", Some(RESET))]);
        let s = Case::new().run(&book, &clear_log(&["a"]), &[claude("a")], &[]);
        assert!(matches!(
            state(&s, "a"),
            Some(DeskState::Offered {
                why: OfferWhy::SeenElsewhere,
                ..
            })
        ));
        assert!(s.hand_over.is_empty());
    }

    #[test]
    fn pending_shows_nothing_and_underway_wins() {
        let book = book_with(vec![record(Tool::ClaudeCode, "a", Some(RESET))]);
        let log = clear_log(&["a"]);
        let desks = [claude("a")];
        let mut c = Case::new();
        c.pending = set(&["a"]);
        let s = c.run(&book, &log, &desks, &[]);
        assert!(s.view.desks.is_empty());
        assert!(s.hand_over.is_empty());

        let mut c = Case::new();
        c.underway = set(&["a"]);
        let s = c.run(&book, &log, &desks, &[]);
        assert_eq!(
            state(&s, "a"),
            Some(&DeskState::Underway { to: Tool::Codex })
        );
        assert!(s.hand_over.is_empty());
    }

    #[test]
    fn arrived_for_the_started_session_and_not_while_it_is_used_up() {
        let mut book = Book::default();
        book.finished(done(
            "a",
            Outcome::Started {
                session: Some("n".into()),
            },
            NOW - 30,
        ));
        let new_desk = DeskFacts {
            session_id: "n",
            tool: Tool::Codex,
            ours: true,
        };
        let s = Case::new().run(&book, &WorkLog::default(), &[new_desk], &[]);
        assert_eq!(
            state(&s, "n"),
            Some(&DeskState::Arrived {
                from: Tool::ClaudeCode,
                at: NOW - 30,
                window: Some(Window::FiveHour),
                reset_at: Some(RESET),
            })
        );

        book.saw(record(Tool::Codex, "n", Some(RESET)));
        let new_desk = DeskFacts {
            session_id: "n",
            tool: Tool::Codex,
            ours: true,
        };
        let s = Case::new().run(&book, &WorkLog::default(), &[new_desk], &[]);
        assert!(matches!(
            state(&s, "n"),
            Some(DeskState::Offered { .. } | DeskState::Blocked { .. })
        ));
    }

    #[test]
    fn sign_out_lines() {
        let mut book = Book::default();
        book.finished(done("a", Outcome::Started { session: None }, NOW - 10));
        book.finished(done(
            "b",
            Outcome::Failed {
                reason: "no terminal".into(),
                ended: true,
            },
            NOW - 10,
        ));
        book.finished(done(
            "c",
            Outcome::Failed {
                reason: "no terminal".into(),
                ended: false,
            },
            NOW - 10,
        ));
        let c = Case::new();
        let s = c.run(&book, &WorkLog::default(), &[], &["a", "b", "c", "d"]);
        assert_eq!(
            state(&s, "a"),
            Some(&DeskState::Left {
                to: Tool::Codex,
                at: NOW - 10
            })
        );
        assert_eq!(
            state(&s, "b"),
            Some(&DeskState::Failed {
                to: Tool::Codex,
                at: NOW - 10,
                reason: "no terminal".into()
            })
        );
        assert_eq!(state(&s, "c"), None);
        assert_eq!(state(&s, "d"), None);
        // Not on a sign-out sheet: no line.
        let s = c.run(&book, &WorkLog::default(), &[], &[]);
        assert!(s.view.desks.is_empty());
    }

    #[test]
    fn one_entry_per_session_desks_first() {
        let mut book = book_with(vec![record(Tool::ClaudeCode, "a", Some(RESET))]);
        book.finished(done(
            "a",
            Outcome::Failed {
                reason: "x".into(),
                ended: true,
            },
            NOW - 5,
        ));
        let mut c = Case::new();
        c.fresh = set(&[]);
        let s = c.run(&book, &clear_log(&["a"]), &[claude("a")], &["a"]);
        assert_eq!(s.view.desks.len(), 1);
        assert!(matches!(
            state(&s, "a"),
            Some(DeskState::Offered {
                why: OfferWhy::Failed { .. },
                ..
            })
        ));
    }

    #[test]
    fn hand_over_holds_only_fresh_desks() {
        let book = book_with(vec![
            record(Tool::ClaudeCode, "a", Some(RESET)),
            record(Tool::ClaudeCode, "b", Some(RESET)),
        ]);
        let log = clear_log(&["a", "b"]);
        let s = Case::new().run(&book, &log, &[claude("a"), claude("b")], &[]);
        assert_eq!(s.hand_over, vec!["a".to_string()]);
        assert!(matches!(
            state(&s, "b"),
            Some(DeskState::Offered {
                why: OfferWhy::NotAtTheTime,
                ..
            })
        ));
    }

    #[test]
    fn a_codex_desk_beside_a_claude_code_record_gets_nothing() {
        let book = book_with(vec![record(Tool::ClaudeCode, "a", Some(RESET))]);
        let codex = DeskFacts {
            session_id: "z",
            tool: Tool::Codex,
            ours: true,
        };
        let s = Case::new().run(&book, &WorkLog::default(), &[codex], &[]);
        assert!(s.view.desks.is_empty());
    }
}
