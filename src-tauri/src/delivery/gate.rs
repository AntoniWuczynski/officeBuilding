//! Whether a desk can take a note now. Pure: the caller describes the desk.

use crate::model::SessionState;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// How long after A Human's last keystroke in a terminal nothing is typed there.
pub const HUMAN_QUIET: Duration = Duration::from_secs(60);

/// Why a note has not been handed to its desk (`NoteHold` in types.ts).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Hold {
    /// The desk is running or thinking.
    Busy,
    /// It has a question or a permission prompt open.
    WaitingOnYou,
    /// It is stuck on an error.
    Stuck,
    /// Its agent has finished.
    Finished,
    /// A Human typed in its terminal in the last minute, or left an unsent draft.
    YouTyped,
    /// The last note handed to it has not been picked up yet.
    AwaitingTurn,
    /// It was started outside the building: nothing is typed into it.
    HandStarted,
    /// Its tool does not tell the building when it is idle.
    NoIdleSignal,
    /// The sender is paused. The gate never returns it: the sender adds it.
    Paused,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Via {
    Terminal,
    Structured,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// An app-started terminal whose tool reports idle through the hook feed.
    Terminal,
    /// A structured desk: a note is its next turn.
    Structured,
    /// No safe way in: `Hold::HandStarted` or `Hold::NoIdleSignal`.
    None(Hold),
}

/// A desk as a note would find it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Desk {
    pub route: Route,
    /// For a terminal desk, `Idle` only when the hook feed and the session's own files agree.
    pub state: SessionState,
    /// It has a question or a permission prompt open, whatever `state` says.
    pub prompt_open: bool,
    /// Since A Human last typed in its terminal. None: never, or it has no terminal.
    pub typed_ago: Option<Duration>,
    /// What A Human typed in its terminal since the last Enter has not been sent.
    pub draft_open: bool,
    /// A note was handed to it and no later event has been seen.
    pub awaiting_turn: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Deliver(Via),
    Hold(Hold),
}

/// Whether `desk` has a question or a permission prompt open.
#[must_use]
pub fn waiting_on_you(desk: &Desk) -> bool {
    desk.prompt_open || desk.state == SessionState::WaitingHuman
}

/// Whether `desk` can take a note now, or why not. The first rule that matches wins.
#[must_use]
pub fn verdict(desk: &Desk) -> Verdict {
    let via = match desk.route {
        Route::None(hold) => return Verdict::Hold(hold),
        Route::Terminal => Via::Terminal,
        Route::Structured => Via::Structured,
    };
    if waiting_on_you(desk) {
        return Verdict::Hold(Hold::WaitingOnYou);
    }
    match desk.state {
        SessionState::Error => return Verdict::Hold(Hold::Stuck),
        SessionState::Done => return Verdict::Hold(Hold::Finished),
        SessionState::Running | SessionState::Thinking => return Verdict::Hold(Hold::Busy),
        SessionState::Idle | SessionState::WaitingHuman => {}
    }
    if desk.awaiting_turn {
        return Verdict::Hold(Hold::AwaitingTurn);
    }
    if via == Via::Terminal && (desk.draft_open || desk.typed_ago.is_some_and(|d| d < HUMAN_QUIET))
    {
        return Verdict::Hold(Hold::YouTyped);
    }
    Verdict::Deliver(via)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn idle(route: Route) -> Desk {
        Desk {
            route,
            state: SessionState::Idle,
            prompt_open: false,
            typed_ago: None,
            draft_open: false,
            awaiting_turn: false,
        }
    }

    fn held(desk: &Desk) -> Option<Hold> {
        match verdict(desk) {
            Verdict::Hold(hold) => Some(hold),
            Verdict::Deliver(_) => None,
        }
    }

    #[test]
    fn an_idle_desk_takes_the_note_by_its_route() {
        assert_eq!(
            verdict(&idle(Route::Terminal)),
            Verdict::Deliver(Via::Terminal)
        );
        assert_eq!(
            verdict(&idle(Route::Structured)),
            Verdict::Deliver(Via::Structured)
        );
    }

    #[test]
    fn no_route_holds_with_its_reason_whatever_else_holds() {
        for hold in [Hold::HandStarted, Hold::NoIdleSignal] {
            let mut desk = idle(Route::None(hold));
            assert_eq!(held(&desk), Some(hold));
            desk.prompt_open = true;
            desk.state = SessionState::Running;
            assert_eq!(held(&desk), Some(hold));
        }
    }

    #[test]
    fn an_open_prompt_holds_even_when_idle() {
        let mut desk = idle(Route::Terminal);
        desk.prompt_open = true;
        assert_eq!(held(&desk), Some(Hold::WaitingOnYou));
        let mut desk = idle(Route::Structured);
        desk.state = SessionState::WaitingHuman;
        assert_eq!(held(&desk), Some(Hold::WaitingOnYou));
        // It beats a busy state too.
        let mut desk = idle(Route::Terminal);
        desk.state = SessionState::Running;
        desk.prompt_open = true;
        assert_eq!(held(&desk), Some(Hold::WaitingOnYou));
    }

    #[test]
    fn each_state_holds_with_its_reason() {
        for (state, hold) in [
            (SessionState::Error, Hold::Stuck),
            (SessionState::Done, Hold::Finished),
            (SessionState::Running, Hold::Busy),
            (SessionState::Thinking, Hold::Busy),
        ] {
            let mut desk = idle(Route::Terminal);
            desk.state = state;
            desk.awaiting_turn = true;
            assert_eq!(held(&desk), Some(hold), "{state:?}");
        }
    }

    #[test]
    fn awaiting_its_turn_beats_the_keyboard() {
        let mut desk = idle(Route::Terminal);
        desk.awaiting_turn = true;
        desk.typed_ago = Some(Duration::from_secs(1));
        assert_eq!(held(&desk), Some(Hold::AwaitingTurn));
        let mut desk = idle(Route::Structured);
        desk.awaiting_turn = true;
        assert_eq!(held(&desk), Some(Hold::AwaitingTurn));
    }

    #[test]
    fn a_keystroke_in_the_last_minute_holds_a_terminal() {
        let mut desk = idle(Route::Terminal);
        desk.typed_ago = Some(Duration::from_secs(59));
        assert_eq!(held(&desk), Some(Hold::YouTyped));
        desk.typed_ago = Some(HUMAN_QUIET);
        assert_eq!(verdict(&desk), Verdict::Deliver(Via::Terminal));
    }

    #[test]
    fn an_unsent_draft_holds_a_terminal_however_old() {
        let mut desk = idle(Route::Terminal);
        desk.typed_ago = Some(Duration::from_secs(3600));
        desk.draft_open = true;
        assert_eq!(held(&desk), Some(Hold::YouTyped));
        assert_eq!(held(&idle(Route::Structured)), None);
    }

    #[test]
    fn a_structured_desk_ignores_the_keyboard() {
        let mut desk = idle(Route::Structured);
        desk.typed_ago = Some(Duration::ZERO);
        assert_eq!(verdict(&desk), Verdict::Deliver(Via::Structured));
    }
}
