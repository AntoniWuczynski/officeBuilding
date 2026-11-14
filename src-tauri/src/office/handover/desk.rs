//! The one place the office picks a desk's own reader by its tool: Claude Code's transcript
//! or Codex's rollout. Each is read with no office lock held.

use super::Limits;
use crate::discovery::claude_code::chat::ChatReader;
use crate::discovery::codex::rollout::{self, RolloutReader};
use crate::handover::work::Goal;
use crate::handover::{Allowance, Tool};
use crate::structured::Entry;
use std::path::Path;

/// The reader of one desk's own record.
pub(in crate::office) enum Follower {
    Claude(ChatReader),
    Codex(RolloutReader),
}

/// What a Codex desk's rollout says beyond its allowance.
pub(super) struct CodexSeen {
    /// A top-level thread on Codex's built-in provider (K5).
    pub(super) counts: bool,
    pub(super) goal: Goal,
}

impl Follower {
    pub(super) fn new(tool: Tool, home: &Path, session_id: &str) -> Self {
        match tool {
            Tool::ClaudeCode => Self::Claude(ChatReader::new(home, session_id)),
            Tool::Codex => Self::Codex(RolloutReader::new(home, session_id)),
        }
    }

    pub(super) fn tool(&self) -> Tool {
        match self {
            Self::Claude(_) => Tool::ClaudeCode,
            Self::Codex(_) => Tool::Codex,
        }
    }

    /// Read what was appended since the last call.
    pub(super) fn read(&mut self) {
        match self {
            Self::Claude(reader) => {
                reader.read();
            }
            Self::Codex(reader) => reader.read(),
        }
    }

    pub(super) fn allowance(&self) -> Allowance {
        match self {
            Self::Claude(reader) => Allowance::from(reader.limit()),
            Self::Codex(reader) => reader.allowance(),
        }
    }

    /// None for a Claude Code desk, whose work comes from the hook feed.
    pub(super) fn codex(&self) -> Option<CodexSeen> {
        match self {
            Self::Claude(_) => None,
            Self::Codex(reader) => Some(CodexSeen {
                counts: reader.counts(),
                goal: reader.goal().clone(),
            }),
        }
    }
}

/// A desk the building started: its session id, its tool and, for Codex, how many of its
/// sub-agent threads have a turn in flight.
pub(super) type Started = (String, Tool, usize);

/// What one pass read: each desk whose allowance changed, with its tool, and what each Codex
/// desk's rollout says, with its busy sub-agent threads.
pub(super) struct Read {
    pub(super) changed: Vec<(String, Tool, Allowance)>,
    pub(super) codex: Vec<(String, CodexSeen, usize)>,
}

/// Follow each desk in `desks` with its own reader, made on first sight. Readers of desks no
/// longer listed, or listed under another tool, are dropped. Never under the office lock.
pub(super) fn follow(home: &Path, limits: &mut Limits, desks: &[Started]) -> Read {
    limits.retain(|id, (follower, _)| {
        desks
            .iter()
            .any(|(desk, tool, _)| desk == id && *tool == follower.tool())
    });
    let mut read = Read {
        changed: Vec::new(),
        codex: Vec::new(),
    };
    for (id, tool, busy) in desks {
        let (follower, held) = limits
            .entry(id.clone())
            .or_insert_with(|| (Follower::new(*tool, home, id), Allowance::Clear));
        follower.read();
        let says = follower.allowance();
        if says != *held {
            *held = says.clone();
            read.changed.push((id.clone(), *tool, says));
        }
        if let Some(seen) = follower.codex() {
            read.codex.push((id.clone(), seen, *busy));
        }
    }
    read
}

/// The desk's visible conversation for the brief, and whether its start is left out.
pub(super) fn conversation(tool: Tool, home: &Path, session_id: &str) -> (Vec<Entry>, bool) {
    match tool {
        Tool::ClaudeCode => {
            let mut reader = ChatReader::new(home, session_id);
            reader.read();
            (reader.kept(), reader.earlier())
        }
        Tool::Codex => {
            let talk = rollout::conversation(home, session_id);
            (talk.entries, talk.earlier)
        }
    }
}
