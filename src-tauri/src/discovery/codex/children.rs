//! What a Codex rollout says about the thread it belongs to and about the workers that
//! thread spawned (M2b spec, sections 4.1 and 4.2). Every record is untrusted: each field
//! is read leniently into a typed struct, and anything unexpected is no evidence. The
//! shapes are `OpenAI`'s, read at `rust-v0.161.0` and `rust-v0.170.0`:
//! `codex-rs/protocol/src/protocol.rs` (`SessionMeta`, `SessionSource`, `SubAgentSource`,
//! `SubAgentActivityEvent`, `AgentStatus`) and `codex-rs/protocol/src/items.rs`
//! (`SubAgentActivityItem`, `CollabAgentToolCallItem`).

use crate::delivery::text::visible;
use crate::discovery::claude_code::chat::{Loose, lenient};
use crate::discovery::rules::LastBlock;
use crate::discovery::{HELPER_SILENCE, KEEP_FINISHED, Location, helper_in, sent_at};
use crate::model::{Helper, SessionState};
use serde::Deserialize;
use serde::de::IgnoredAny;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::time::{Duration, SystemTime};

/// The most characters of a worker's name that are kept.
const NAME_MAX: usize = 40;
/// The most workers one rollout's records are kept for.
const TOLD_MAX: usize = 512;

/// What a rollout's first `session_meta` says the thread is.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Kin {
    /// A session of its own: it gets a desk.
    #[default]
    Desk,
    /// A worker another thread spawned: a helper of that thread's root.
    Worker { parent: String },
    /// A thread Codex runs for itself (a review, a compaction, memory, a guardian), or one
    /// whose origin the building does not know. Never a desk and never a helper.
    Internal { parent: Option<String> },
}

impl Kin {
    /// The thread its spend rolls up to.
    #[must_use]
    pub fn parent(&self) -> Option<&str> {
        match self {
            Self::Desk => None,
            Self::Worker { parent } => Some(parent),
            Self::Internal { parent } => parent.as_deref(),
        }
    }
}

/// The first `session_meta` of a rollout, reduced.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Meta {
    pub kin: Kin,
    /// What the worker is called: its nickname, else its role, else its agent path's last part.
    pub called: Option<String>,
}

/// String fields of an object, any other value read as absent.
type Fields = HashMap<String, Loose<String>>;

/// `session_meta.source` (G1, G2). Only `{"subagent": {"thread_spawn": {…}}}` is looked into.
#[derive(Deserialize)]
#[serde(untagged)]
enum Source {
    /// A plain word or null.
    Plain(
        #[expect(
            dead_code,
            reason = "matched for its shape only: any word is a session's own"
        )]
        Option<String>,
    ),
    /// An object: its `subagent`, when that is an object, and that one's `thread_spawn`.
    Tagged(HashMap<String, Loose<HashMap<String, Loose<Fields>>>>),
    /// Any other shape.
    Other(IgnoredAny),
}

impl Default for Source {
    fn default() -> Self {
        Self::Plain(None)
    }
}

/// The few `session_meta` fields that say what a thread is (G3).
#[derive(Deserialize, Default)]
struct MetaSeen {
    #[serde(default)]
    source: Source,
    #[serde(default, deserialize_with = "lenient")]
    parent_thread_id: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    thread_source: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    agent_nickname: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    agent_role: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    agent_type: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    agent_path: Option<String>,
}

/// A worker's name, shown and never interpreted: no invisible or control characters,
/// trimmed and cut to `NAME_MAX` characters. None when nothing is left.
fn cleaned(raw: &str) -> Option<String> {
    let kept: String = visible(raw).chars().filter(|c| !c.is_control()).collect();
    let name: String = kept.trim().chars().take(NAME_MAX).collect();
    let name = name.trim_end().to_string();
    (!name.is_empty()).then_some(name)
}

/// The last `/`-separated part of an agent path, cleaned.
#[must_use]
pub fn path_name(path: &str) -> Option<String> {
    path.rsplit('/').next().and_then(cleaned)
}

/// What a rollout's first `session_meta` payload says the thread is (section 4.1).
#[must_use]
pub fn read_meta(payload: &Value) -> Meta {
    let seen = if payload.is_object() {
        MetaSeen::deserialize(payload).unwrap_or_default()
    } else {
        MetaSeen::default()
    };
    let field = |fields: Option<&Fields>, key: &str| {
        fields
            .and_then(|f| f.get(key))
            .and_then(|Loose(v)| v.clone())
    };
    let (kin, spawn) = match seen.source {
        Source::Tagged(mut tagged) => {
            let spawn = tagged
                .remove("subagent")
                .and_then(|Loose(sub)| sub)
                .and_then(|mut sub| sub.remove("thread_spawn"))
                .and_then(|Loose(spawn)| spawn);
            let kin = match &spawn {
                Some(fields) => match field(Some(fields), "parent_thread_id")
                    .or_else(|| seen.parent_thread_id.clone())
                {
                    Some(parent) => Kin::Worker { parent },
                    None => Kin::Internal { parent: None },
                },
                None => Kin::Internal {
                    parent: seen.parent_thread_id.clone(),
                },
            };
            (kin, spawn)
        }
        Source::Other(_) => (
            Kin::Internal {
                parent: seen.parent_thread_id.clone(),
            },
            None,
        ),
        Source::Plain(_) => {
            let kin = match seen.parent_thread_id.clone() {
                Some(parent)
                    if matches!(
                        seen.thread_source.as_deref(),
                        Some("guardian_review" | "memory_consolidation")
                    ) =>
                {
                    Kin::Internal {
                        parent: Some(parent),
                    }
                }
                Some(parent) => Kin::Worker { parent },
                None => Kin::Desk,
            };
            (kin, None)
        }
    };
    let spawn = spawn.as_ref();
    let called = [
        field(spawn, "agent_nickname").and_then(|n| cleaned(&n)),
        seen.agent_nickname.as_deref().and_then(cleaned),
        field(spawn, "agent_role").and_then(|n| cleaned(&n)),
        field(spawn, "agent_type").and_then(|n| cleaned(&n)),
        seen.agent_role.as_deref().and_then(cleaned),
        seen.agent_type.as_deref().and_then(cleaned),
        field(spawn, "agent_path").and_then(|p| path_name(&p)),
        seen.agent_path.as_deref().and_then(path_name),
    ]
    .into_iter()
    .flatten()
    .next();
    Meta { kin, called }
}

/// What a request of the worker `id` says about who asked (D11).
#[must_use]
pub fn name_of(called: Option<&str>, id: &str) -> String {
    called.map_or_else(
        || format!("helper {}", id.chars().take(8).collect::<String>()),
        str::to_string,
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Says {
    Working,
    Failed,
    Over,
}

/// The newest thing one rollout records about one worker its thread tree started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Told {
    /// The worker's thread id.
    pub thread: String,
    pub says: Says,
    /// The timestamp of the record that said so, as written.
    pub at: String,
    /// The last part of its agent path, cleaned as in 4.1, when a record gave one.
    pub called: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
}

/// An `AgentStatus` (G9): a plain word, or an object whose one key is its name.
#[derive(Deserialize)]
#[serde(untagged)]
enum Status {
    Word(String),
    Keyed(HashMap<String, Loose<String>>),
}

impl Status {
    fn says(&self) -> Option<Says> {
        let name = match self {
            Self::Word(word) => word.as_str(),
            Self::Keyed(keyed) if keyed.len() == 1 => keyed.keys().next()?.as_str(),
            Self::Keyed(_) => return None,
        };
        match name {
            "pending_init" | "running" => Some(Says::Working),
            "errored" => Some(Says::Failed),
            "completed" | "interrupted" | "shutdown" | "not_found" => Some(Says::Over),
            _ => None,
        }
    }
}

/// The fields of a `sub_agent_activity` payload, a `SubAgentActivity` item or a
/// `CollabAgentToolCall` item (G6, G9) that are read.
#[derive(Deserialize, Default)]
struct Activity {
    #[serde(rename = "type", default, deserialize_with = "lenient")]
    item_type: String,
    #[serde(default, deserialize_with = "lenient")]
    kind: String,
    #[serde(default, deserialize_with = "lenient")]
    agent_thread_id: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    agent_path: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    model: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    reasoning_effort: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    tool: String,
    #[serde(default, deserialize_with = "lenient")]
    status: String,
    #[serde(default, deserialize_with = "lenient")]
    receiver_thread_ids: Vec<Loose<String>>,
    #[serde(default, deserialize_with = "lenient")]
    agents_states: BTreeMap<String, Loose<Status>>,
}

/// `value` read as an `Activity` when it is an object, else nothing.
fn activity(value: Option<&Value>) -> Option<Activity> {
    value
        .filter(|v| v.is_object())
        .and_then(|v| Activity::deserialize(v).ok())
}

/// One state change for one worker.
struct Said<'a> {
    thread: &'a str,
    says: Says,
    path: Option<&'a str>,
    model: Option<&'a str>,
    effort: Option<&'a str>,
}

/// Reduces a rollout's activity and collaboration records to one `Told` per worker.
#[derive(Default)]
pub struct Tellings {
    told: Vec<Told>,
}

impl Tellings {
    /// One `event_msg` payload, with its record's timestamp. Anything that is not one of
    /// the three records of G6 and G9 changes nothing.
    pub fn read(&mut self, payload: &Value, at: &str) {
        let Some(kind) = payload.get("type").and_then(Value::as_str) else {
            return;
        };
        let seen = match kind {
            "sub_agent_activity" => activity(Some(payload)),
            "item_completed" => activity(payload.get("item")),
            _ => None,
        };
        let Some(seen) = seen else {
            return;
        };
        match (kind, seen.item_type.as_str()) {
            ("sub_agent_activity", _) | ("item_completed", "SubAgentActivity") => {
                self.activity(&seen, at);
            }
            ("item_completed", "CollabAgentToolCall") => self.call(&seen, at),
            _ => {}
        }
    }

    /// A `sub_agent_activity` record or a `SubAgentActivity` item (G6).
    fn activity(&mut self, seen: &Activity, at: &str) {
        let Some(thread) = seen.agent_thread_id.as_deref() else {
            return;
        };
        let says = match seen.kind.as_str() {
            "started" => Says::Working,
            "completed" | "interrupted" => Says::Over,
            "interacted" => {
                if let Some(told) = self.told.iter_mut().find(|t| t.thread == thread)
                    && told.called.is_none()
                {
                    told.called = seen.agent_path.as_deref().and_then(path_name);
                }
                return;
            }
            _ => return,
        };
        self.say(
            &Said {
                thread,
                says,
                path: seen.agent_path.as_deref(),
                model: seen.model.as_deref(),
                effort: seen.reasoning_effort.as_deref(),
            },
            at,
        );
    }

    /// A `CollabAgentToolCall` item (G9).
    fn call(&mut self, seen: &Activity, at: &str) {
        let receivers: Vec<&str> = seen
            .receiver_thread_ids
            .iter()
            .filter_map(|Loose(id)| id.as_deref())
            .collect();
        let mut changes: Vec<(&str, Says)> = seen
            .agents_states
            .iter()
            .filter_map(|(thread, Loose(status))| Some((thread.as_str(), status.as_ref()?.says()?)))
            .collect();
        let by_tool = match seen.tool.as_str() {
            "spawn_agent" | "resume_agent" => Some(Says::Working),
            "close_agent" => Some(Says::Over),
            _ => None,
        };
        if seen.status == "completed"
            && let Some(says) = by_tool
        {
            changes.extend(
                receivers
                    .iter()
                    .filter(|r| !seen.agents_states.contains_key(**r))
                    .map(|r| (*r, says)),
            );
        }
        for (thread, says) in changes {
            // Only a spawn's receivers run on the call's model and effort.
            let spawned = seen.tool == "spawn_agent" && receivers.contains(&thread);
            let said = Said {
                thread,
                says,
                path: None,
                model: seen.model.as_deref().filter(|_| spawned),
                effort: seen.reasoning_effort.as_deref().filter(|_| spawned),
            };
            self.say(&said, at);
        }
    }

    /// Records a state change: `at` moves on, and a name, model and effort fill in once.
    fn say(&mut self, said: &Said, at: &str) {
        let told = if let Some(index) = self.told.iter().position(|t| t.thread == said.thread) {
            &mut self.told[index]
        } else {
            if self.told.len() == TOLD_MAX {
                self.told.remove(0);
            }
            self.told.push(Told {
                thread: said.thread.to_string(),
                says: said.says,
                at: String::new(),
                called: None,
                model: None,
                effort: None,
            });
            let last = self.told.len() - 1;
            &mut self.told[last]
        };
        told.says = said.says;
        at.clone_into(&mut told.at);
        if told.called.is_none() {
            told.called = said.path.and_then(path_name);
        }
        if told.model.is_none() {
            told.model = said.model.map(str::to_string);
        }
        if told.effort.is_none() {
            told.effort = said.effort.map(str::to_string);
        }
    }

    /// One entry per worker, in the order each was first named.
    #[must_use]
    pub fn finish(self) -> Vec<Told> {
        self.told
    }
}

/// How long a worker that was over before the building ever drew it is shown as done, so it
/// appears and then leaves: a little longer than one tick of the watch loop (5 s).
pub const JUST_LEFT: Duration = Duration::from_secs(8);

/// What the building keeps about a worker beside its `Helper`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kid {
    /// Where it works now. None when its rollout was not read or names no folder.
    pub place: Option<Location>,
    /// What a request of its says about who asked (`name_of`).
    pub name: String,
    /// What its own rollout proves about its requests. Empty when it has no rollout.
    pub evidence: super::turns::Evidence,
    /// Its newest evidence says it is over or failed, however old. Drawn or not.
    pub gone: bool,
}

/// A root's workers: the ones to draw, in pod order, and what is kept about every worker
/// the disk knows, drawn or not, by thread id.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Team {
    pub helpers: Vec<Helper>,
    pub kids: BTreeMap<String, Kid>,
}

/// A top-level thread the scan read.
pub struct Root<'a> {
    pub id: &'a str,
    /// A `codex` process holds its rollout open, or a running chat desk owns it.
    pub live: bool,
    pub transcript: &'a super::Transcript,
}

/// One worker's rollout as the scan read it.
pub struct Seen<'a> {
    pub id: &'a str,
    pub parent: &'a str,
    pub transcript: &'a super::Transcript,
    /// The rollout file's last write.
    pub written: SystemTime,
}

/// One worker attached to a root: its immediate parent, and its rollout when it has one.
struct Member<'a> {
    root: &'a str,
    parent: &'a str,
    seen: Option<&'a Seen<'a>>,
}

/// The root a worker's chain of parents ends at. None when it ends anywhere else, or loops.
fn root_of<'a>(
    worker: &'a Seen<'a>,
    by_id: &HashMap<&str, &'a Seen<'a>>,
    roots: &HashSet<&str>,
) -> Option<&'a str> {
    let mut at = worker.parent;
    for _ in 0..by_id.len() {
        match by_id.get(at) {
            Some(up) => at = up.parent,
            None => break,
        }
    }
    (!by_id.contains_key(at) && roots.contains(at)).then_some(at)
}

/// The age of `time` at `now`. A time after `now` is as fresh as can be.
fn age(now: SystemTime, time: SystemTime) -> Duration {
    now.duration_since(time).unwrap_or(Duration::ZERO)
}

/// What the newest evidence about one worker says, when it was written, and whether the
/// worker's own rollout decides it (section 4.4). A parent's "running" record never
/// outranks a rollout whose turn is still open.
fn newest(
    seen: Option<&Seen<'_>>,
    word: Option<(&Told, SystemTime)>,
) -> Option<(Says, SystemTime, bool)> {
    let own = seen.map(|s| {
        let t = s.transcript;
        let says = if t.turn_open {
            Says::Working
        } else if t.api_error {
            Says::Failed
        } else {
            Says::Over
        };
        (says, s.written)
    });
    match (own, word) {
        (Some(own), Some((told, at)))
            if at > own.1 && !(own.0 == Says::Working && told.says == Says::Working) =>
        {
            Some((told.says, at, false))
        }
        (Some((says, written)), _) => Some((says, written, true)),
        (None, Some((told, at))) => Some((told.says, at, false)),
        (None, None) => None,
    }
}

/// What the newest evidence about one worker says, and how old it is (section 4.4).
fn state_of(
    seen: Option<&Seen<'_>>,
    word: Option<(&Told, SystemTime)>,
    now: SystemTime,
    drawn_before: bool,
) -> Option<SessionState> {
    let (says, at, own_decides) = newest(seen, word)?;
    let age = age(now, at);
    match says {
        Says::Working if own_decides => {
            (age <= KEEP_FINISHED).then(|| match seen.map(|s| s.transcript.last_block) {
                Some(LastBlock::Thinking) => SessionState::Thinking,
                _ => SessionState::Running,
            })
        }
        Says::Working => (age <= HELPER_SILENCE).then_some(SessionState::Running),
        Says::Failed => (age <= HELPER_SILENCE).then_some(SessionState::Error),
        Says::Over => (!drawn_before && age <= JUST_LEFT).then_some(SessionState::Done),
    }
}

/// Every worker that belongs to a root, by id (section 4.5, steps 1 and 2).
fn members<'a>(roots: &[&'a Root<'a>], workers: &[&'a Seen<'a>]) -> BTreeMap<&'a str, Member<'a>> {
    let root_ids: HashSet<&str> = roots.iter().map(|r| r.id).collect();
    let by_id: HashMap<&str, &Seen<'_>> = workers.iter().map(|w| (w.id, *w)).collect();
    let mut members: BTreeMap<&str, Member<'_>> = BTreeMap::new();
    for &worker in workers {
        if let Some(root) = root_of(worker, &by_id, &root_ids) {
            let member = Member {
                root,
                parent: worker.parent,
                seen: Some(worker),
            };
            members.insert(worker.id, member);
        }
    }
    // What each read rollout of the tree says, roots before workers and each in id order.
    let mut readers: Vec<(&str, &str, &[Told])> = roots
        .iter()
        .map(|r| (r.id, r.id, r.transcript.told.as_slice()))
        .collect();
    readers.extend(
        members
            .iter()
            .filter_map(|(id, m)| Some((*id, m.root, m.seen?.transcript.told.as_slice()))),
    );
    // A worker named only in its tree's records has no rollout: it is that rollout's thread's.
    for (thread, root, tolds) in readers {
        for told in tolds {
            let id = told.thread.as_str();
            if id != thread && !root_ids.contains(id) && !by_id.contains_key(id) {
                members.entry(id).or_insert(Member {
                    root,
                    parent: thread,
                    seen: None,
                });
            }
        }
    }
    members
}

/// One attached worker's helper, when it is drawn, and what is kept about it (section 4.5,
/// steps 4 and 5). `told`: every record about it, in the order the rollouts were read.
fn kid_of(
    id: &str,
    member: &Member<'_>,
    told: &[&Told],
    state: impl FnOnce(Option<(&Told, SystemTime)>) -> Option<SessionState>,
) -> (Option<Helper>, Kid) {
    let latest = told
        .iter()
        .filter_map(|t| Some((*t, sent_at(&t.at)?)))
        .fold(None, |best: Option<(&Told, _)>, (t, at)| match best {
            Some((_, b)) if b >= at => best,
            _ => Some((t, at)),
        })
        .map(|(t, at)| (t, SystemTime::from(at)));
    let gone = newest(member.seen, latest).is_some_and(|(s, ..)| s != Says::Working);
    let state = state(latest);
    let t = member.seen.map(|s| s.transcript);
    let from_told = |f: fn(&Told) -> Option<&String>| told.iter().find_map(|t| f(t)).cloned();
    let (helper, place) = helper_in(
        id,
        state.unwrap_or(SessionState::Done),
        t.and_then(|t| t.cwd.as_deref()),
        t.and_then(|t| t.model.clone())
            .or_else(|| from_told(|t| t.model.as_ref())),
        t.and_then(|t| t.effort.clone())
            .or_else(|| from_told(|t| t.effort.as_ref())),
    );
    let called = t
        .and_then(|t| t.called.clone())
        .or_else(|| from_told(|t| t.called.as_ref()));
    let kid = Kid {
        place,
        name: name_of(called.as_deref(), id),
        evidence: t.map(|t| t.evidence.clone()).unwrap_or_default(),
        gone,
    };
    (state.map(|_| helper), kid)
}

/// Each root's team (section 4.5). `drawn` is the memory of the workers drawn as thinking,
/// running or failed, and is updated to the workers this scan attached to a root.
pub fn teams(
    roots: &[Root<'_>],
    workers: &[Seen<'_>],
    now: SystemTime,
    drawn: &mut BTreeSet<String>,
) -> BTreeMap<String, Team> {
    let mut sorted_roots: Vec<&Root<'_>> = roots.iter().collect();
    sorted_roots.sort_by_key(|r| r.id);
    let mut sorted: Vec<&Seen<'_>> = workers.iter().collect();
    sorted.sort_by_key(|w| w.id);
    let members = members(&sorted_roots, &sorted);
    let mut words: HashMap<&str, Vec<&Told>> = HashMap::new();
    for told in sorted_roots
        .iter()
        .map(|r| r.transcript)
        .chain(sorted.iter().map(|w| w.transcript))
        .flat_map(|t| t.told.iter())
    {
        words.entry(told.thread.as_str()).or_default().push(told);
    }
    let mut under: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for (id, m) in &members {
        under.entry(m.parent).or_default().insert(id);
    }
    let mut out: BTreeMap<String, Team> = BTreeMap::new();
    let mut now_drawn = BTreeSet::new();
    for root in &sorted_roots {
        let mut team = Team::default();
        // Depth first: each worker directly followed by its own, all in id order.
        let mut stack: Vec<&str> = under
            .get(root.id)
            .map(|ids| ids.iter().rev().copied().collect())
            .unwrap_or_default();
        while let Some(id) = stack.pop() {
            if let Some(ids) = under.get(id) {
                stack.extend(ids.iter().rev().copied());
            }
            let Some(member) = members.get(id) else {
                continue;
            };
            let told = words.get(id).map(Vec::as_slice).unwrap_or_default();
            let (helper, kid) = kid_of(id, member, told, |word| {
                let before = drawn.contains(id);
                root.live
                    .then(|| state_of(member.seen, word, now, before))
                    .flatten()
            });
            if let Some(helper) = helper {
                if helper.state != SessionState::Done {
                    now_drawn.insert(id.to_string());
                }
                team.helpers.push(helper);
            }
            team.kids.insert(id.to_string(), kid);
        }
        out.insert(root.id.to_string(), team);
    }
    drawn.retain(|id| members.contains_key(id.as_str()));
    drawn.extend(now_drawn);
    out
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod team_tests;
