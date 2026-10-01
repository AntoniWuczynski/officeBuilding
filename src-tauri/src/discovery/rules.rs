//! Session state rules.
//!
//! The precedence (a dead process is finished; waiting on a human beats working;
//! working beats idle) and the soft-block scorer that spots an agent which
//! stopped to ask something in plain prose are ported from Claude Command Center
//! v5.14.0 (`_session_state_label`, `_detect_soft_block`, `_score_soft_block` in
//! server.py), MIT licence, Copyright (c) 2026 Amir Fish. See
//! `THIRD_PARTY_NOTICES.md`. The `error` and `thinking` split is ours: CCC has no
//! error state and no thinking/working distinction.

use crate::model::SessionState;

/// What the newest assistant turn ended on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LastBlock {
    Thinking,
    Text,
    ToolUse,
    Nothing,
}

/// Everything the state rules look at, gathered from a tool's own files.
#[derive(Debug, Clone, PartialEq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent facts the rules combine, not a state machine"
)]
pub struct Signals {
    /// The agent's process is running.
    pub live: bool,
    /// The tool itself reports a turn in progress.
    pub busy: bool,
    /// A formal question (e.g. `AskUserQuestion`) is waiting for an answer.
    pub question_waiting: bool,
    /// A tool call has no result yet.
    pub pending_tool: bool,
    /// The human has spoken since the agent's last reply: the agent owes the next move.
    pub human_spoke_last: bool,
    /// The newest assistant turn was an API error.
    pub api_error: bool,
    pub last_block: LastBlock,
    /// The agent's latest reply to the human (not to system notifications).
    pub reply_to_human: String,
}

/// The single state every view binds to.
#[must_use]
pub fn derive_state(s: &Signals) -> SessionState {
    if !s.live {
        return SessionState::Done;
    }
    if s.question_waiting || soft_block(s).is_some() {
        return SessionState::WaitingHuman;
    }
    if s.busy || s.pending_tool {
        return if s.last_block == LastBlock::Thinking {
            SessionState::Thinking
        } else {
            SessionState::Running
        };
    }
    if s.api_error {
        return SessionState::Error;
    }
    SessionState::Idle
}

/// A plain-prose ask the agent stopped on, if the turn looks like one.
#[must_use]
pub fn soft_block(s: &Signals) -> Option<SoftBlock> {
    // Terminal gate: only a turn that has genuinely ended can be waiting on us.
    if s.pending_tool || s.busy || s.human_spoke_last {
        return None;
    }
    let scored = score_soft_block(&s.reply_to_human);
    if scored.score >= 3 {
        Some(scored)
    } else {
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoftBlock {
    pub score: u32,
    /// The line that carries the ask, for the question queue.
    pub question: String,
}

const PHRASES: &[&str] = &[
    "want me to",
    "shall i",
    "should i",
    "do you want",
    "would you like",
    "for your review",
    "please review",
    "your review",
    "ready for review",
    "review the plan",
    "review this",
    "let me know",
    "lmk",
    "pick one",
    "pick a",
    "choose one",
    "which option",
    "which approach",
    "which one",
    "which direction",
    "which route",
    "your call",
    "up to you",
    "waiting on you",
    "waiting for you",
    "waiting for your",
    "awaiting your",
    "paused for",
    "paused — ",
    "paused -",
    "plan before",
    "before i start",
    "before i write",
    "before i proceed",
    "before i continue",
    "before i implement",
    "before i build",
    "sound good",
    "look right",
    "what do you think",
    "wdyt",
    "thoughts?",
    "go ahead?",
    "proceed?",
    "sign off",
    "confirm",
    "approve",
    "let me know which",
    "let me know if",
    "want me",
    "should we",
    "shall we",
];

const CHOICE_WORDS: &[&str] = &[
    "pick",
    "choose",
    "which",
    "option",
    "options",
    "route",
    "routes",
    "direction",
    "directions",
    "approach",
    "approaches",
    "prefer",
    "preference",
    "decide",
];

/// Last `n` characters of `s` (by char, not byte).
fn tail_chars(s: &str, n: usize) -> &str {
    let count = s.chars().count();
    if count <= n {
        return s;
    }
    let start = s.char_indices().nth(count - n).map_or(0, |(i, _)| i);
    &s[start..]
}

/// A line that lays out one option: "1.", "2)", "-", "*", "•", "a.", "b)" or "option x".
fn is_enumerated(line: &str) -> bool {
    let t = line.trim_start();
    let mut chars = t.chars();
    let (Some(first), second) = (chars.next(), chars.next()) else {
        return false;
    };
    let rest_starts_word = |skip: usize| {
        let rest: String = t.chars().skip(skip).collect();
        rest.starts_with(char::is_whitespace) && rest.trim_start().chars().next().is_some()
    };
    if matches!(first, '-' | '*' | '•') {
        return rest_starts_word(1);
    }
    let lower = first.to_ascii_lowercase();
    if (('1'..='9').contains(&first) || ('a'..='d').contains(&lower))
        && matches!(second, Some('.' | ')'))
    {
        return rest_starts_word(2);
    }
    let low = t.to_lowercase();
    if let Some(after) = low.strip_prefix("option") {
        let mut it = after.chars();
        if it.next().is_some_and(char::is_whitespace) {
            let label: String = it.by_ref().take_while(|c| !c.is_whitespace()).collect();
            return !label.is_empty()
                && label
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphanumeric());
        }
    }
    false
}

fn has_choice_word(low: &str) -> bool {
    low.split(|c: char| !c.is_alphanumeric())
        .any(|w| CHOICE_WORDS.contains(&w))
}

/// Score how strongly a trailing assistant message is waiting on a human (≥ 3 is a hit).
pub fn score_soft_block(text: &str) -> SoftBlock {
    let prose = text.trim();
    if prose.is_empty() {
        return SoftBlock {
            score: 0,
            question: String::new(),
        };
    }
    let tail = tail_chars(prose, 700);
    let low = tail.to_lowercase();
    let mut score = 0;

    let stripped = tail
        .trim_end()
        .trim_end_matches([')', '*', '_', '`', '"', '\'', '>'])
        .trim_end();
    if stripped.ends_with('?') {
        score += 3;
    } else if tail_chars(tail, 400).contains('?') {
        score += 1;
    }

    let matched = PHRASES.iter().filter(|p| low.contains(*p)).count().min(2);
    score += 2 * u32::try_from(matched).unwrap_or(2);

    if tail.lines().any(is_enumerated) && has_choice_word(&low) {
        score += 2;
    }

    let lines: Vec<&str> = prose
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let question = lines
        .iter()
        .rev()
        .find(|l| l.contains('?') || PHRASES.iter().any(|p| l.to_lowercase().contains(p)))
        .or_else(|| lines.last())
        .map_or(String::new(), |l| l.chars().take(400).collect());

    SoftBlock { score, question }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn live() -> Signals {
        Signals {
            live: true,
            busy: false,
            question_waiting: false,
            pending_tool: false,
            human_spoke_last: false,
            api_error: false,
            last_block: LastBlock::Text,
            reply_to_human: "Done. All tests pass.".into(),
        }
    }

    #[test]
    fn dead_is_done_whatever_the_last_turn_said() {
        let s = Signals {
            live: false,
            question_waiting: true,
            ..live()
        };
        assert_eq!(derive_state(&s), SessionState::Done);
    }

    #[test]
    fn formal_question_waits_even_while_its_tool_is_open() {
        let s = Signals {
            question_waiting: true,
            pending_tool: true,
            busy: true,
            ..live()
        };
        assert_eq!(derive_state(&s), SessionState::WaitingHuman);
    }

    #[test]
    fn busy_splits_thinking_from_running() {
        assert_eq!(
            derive_state(&Signals {
                busy: true,
                last_block: LastBlock::Thinking,
                ..live()
            }),
            SessionState::Thinking
        );
        assert_eq!(
            derive_state(&Signals {
                busy: true,
                last_block: LastBlock::ToolUse,
                ..live()
            }),
            SessionState::Running
        );
        assert_eq!(
            derive_state(&Signals {
                pending_tool: true,
                ..live()
            }),
            SessionState::Running
        );
    }

    #[test]
    fn api_error_on_an_ended_turn_is_stuck() {
        assert_eq!(
            derive_state(&Signals {
                api_error: true,
                ..live()
            }),
            SessionState::Error
        );
    }

    #[test]
    fn quiet_live_session_is_idle() {
        assert_eq!(derive_state(&live()), SessionState::Idle);
    }

    #[test]
    fn a_trailing_question_is_a_soft_block() {
        let s = Signals {
            reply_to_human: "I fixed the parser.\n\nShould I also bump the version?".into(),
            ..live()
        };
        assert_eq!(derive_state(&s), SessionState::WaitingHuman);
        assert_eq!(
            soft_block(&s).map(|b| b.question),
            Some("Should I also bump the version?".into())
        );
    }

    #[test]
    fn a_question_the_human_already_answered_is_not_waiting() {
        let s = Signals {
            human_spoke_last: true,
            reply_to_human: "Should I go on?".into(),
            ..live()
        };
        assert_eq!(derive_state(&s), SessionState::Idle);
    }

    #[test]
    fn enumerated_choices_score() {
        let text = "Two routes:\n1. Paid endpoint\n2. Back off\nPick one.";
        let b = score_soft_block(text);
        // "pick one" (+2) and an enumerated choice (+2).
        assert_eq!(b.score, 4);
    }

    #[test]
    fn plain_report_does_not_score() {
        assert!(score_soft_block("Refactored the module and all 94 tests pass.").score < 3);
        assert_eq!(score_soft_block("   ").score, 0);
    }

    #[test]
    fn enumeration_shapes() {
        assert!(is_enumerated("  1. first"));
        assert!(is_enumerated("b) second"));
        assert!(is_enumerated("- bullet"));
        assert!(is_enumerated("Option A: keep it"));
        assert!(!is_enumerated("1.5 seconds"));
        assert!(!is_enumerated("optional work"));
    }

    #[test]
    fn tail_is_char_safe() {
        assert_eq!(tail_chars("żółw", 2), "łw");
        assert_eq!(tail_chars("ab", 5), "ab");
    }
}
