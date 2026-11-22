//! Codex (`OpenAI`'s `codex` CLI) discovery. One on-disk source, written by
//! Codex itself:
//!
//! * `~/.codex/sessions/YYYY/MM/DD/rollout-<ts>-<uuid>.jsonl`: the transcript,
//!   one JSON record per line (`{"timestamp","ordinal","type","payload"}`).
//!   `session_meta.source` is the plain string `"cli"`/`"vscode"` for a real
//!   top-level session; for a sub-agent thread spawned by Codex's multi-agent
//!   mode it is an object (`{"subagent":{"thread_spawn":{...}}}`) and the
//!   parent is named there, at the top level, or both (`children.rs`). We drop those as
//!   desks, the same way Claude Code's sidechains are dropped: they describe
//!   helpers, not the session itself. Their spend is added to the top-level
//!   thread that spawned them. Verified against real rollouts on this Mac
//!   (2026-09-29), including a repo that runs review swarms through Codex's
//!   subagent feature.
//! * `~/.codex/session_index.jsonl`: `{"id","thread_name","updated_at"}`,
//!   append-only. `id` is the *root* session id (`session_meta.session_id`,
//!   stable across `codex resume`), not the per-rollout `session_meta.id`
//!   (which is the uuid embedded in the filename). We look titles up by the
//!   root id and keep the last (most recent) line per id.
//!
//! ## Liveness
//!
//! Codex has no equivalent of Claude Code's `~/.claude/sessions/<pid>.json`
//! registry, so liveness is inferred from `lsof`. Verified on this Mac
//! (2026-09-29) against two real `codex` processes:
//!
//! * A `codex` CLI running interactively in a terminal held its *own*
//!   top-level rollout open plus three more rollouts that turned out to be sub-agent threads it had spawned — the ones
//!   `session_meta.source` flags as sub-agent. Filtered to rollouts already
//!   known to be top-level (from the parse pass), exactly one open rollout
//!   remained: its own. A single `lsof -p <pid> -Ffn` call lists every open file,
//!   so this needs one `lsof` call per process, not one per candidate file.
//! * The `ChatGPT` desktop app's embedded `codex` binary (`comm` also just
//!   "codex" — matched by `ps` the same as the CLI) held zero rollout files
//!   open and had cwd `/`. Documented limitation: desktop-originated Codex
//!   threads are therefore never live, only ever shown finished, since their
//!   process holds no open file to tie them to a rollout.
//!
//! So: a rollout is live when a running `codex` process holds *that exact
//! file* open, filtered to top-level (non-sub-agent) rollouts under
//! `~/.codex/sessions/**/rollout-*.jsonl` — proof, not a guess.
//!
//! ### No rollout yet (2026-10-08)
//!
//! Checked against a real Codex 0.154.0 in a pseudo-terminal: the rollout is
//! created lazily, only once the first turn starts (about 10 s after the first
//! message). A `codex` sitting at its prompt, or blocked on a pre-turn question
//! such as "Do you trust the contents of this directory?", holds no rollout, so
//! it had no desk. Such a process now gets a placeholder desk
//! (`codex-new-<pid>`, "New Codex session", never resumable) until its rollout
//! appears and it becomes that session. Two kinds of `codex` process are not
//! sessions and never get one: cwd `/` (the desktop app's), and any whose
//! arguments name the `app-server` or `mcp-server` subcommand (IDE extensions
//! and the desktop app run those from a workspace). Nor is any `codex` started,
//! at any depth, by another `codex`: a session's browser tool runs `node_repl`,
//! which starts `codex sandbox` helpers in the session's folder (seen
//! 2026-10-09). A process holding more than one top-level rollout stays
//! ambiguous: not live, no placeholder.

use super::collect_jsonl;
use super::pricing::{self, CodexUsage};
use super::rules::{LastBlock, Signals};
use crate::model::{Helper, Spend};
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

pub mod children;
mod process;
pub mod rollout;
pub mod turns;
pub(super) use process::cwd_of;
use process::{list_codex_pids, probe, resolve_liveness_by_open_files, rolloutless_sessions};

/// A `token_count` event's `info`: cumulative totals and the latest request's usage.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct TokenCountInfo {
    total_token_usage: CodexUsage,
    last_token_usage: CodexUsage,
}

/// What one rollout says, reduced to what the office shows.
#[derive(Debug, Clone, PartialEq)]
pub struct Transcript {
    pub cwd: Option<String>,
    /// The root session id (`session_meta.session_id`), for the title index.
    pub root_session_id: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub last_activity: Option<String>,
    /// Each `token_count` event's growth in `total_token_usage`, priced at the model then in use.
    pub spend: Spend,
    pub human_spoke_last: bool,
    pub last_block: LastBlock,
    pub reply_to_human: String,
    /// First sentence of the latest assistant message; a new user message does not clear it.
    pub last_reply: Option<String>,
    /// The newest `task_complete` carried an `error`.
    pub api_error: bool,
    pub open_tools: usize,
    /// `task_started` seen with no matching `task_complete` yet.
    pub turn_open: bool,
    /// What the first `session_meta` says the thread is.
    pub kin: children::Kin,
    /// What that record calls it, for a worker.
    pub called: Option<String>,
    /// What this rollout records about the workers its thread tree started.
    pub told: Vec<children::Told>,
    /// The `request_user_input` calls still without an answer (section 5.1).
    pub asks: Vec<turns::Ask>,
    /// What the rollout proves about the current turn's requests.
    pub evidence: turns::Evidence,
}

impl Transcript {
    /// A rollout that says nothing yet.
    fn blank() -> Self {
        Self {
            cwd: None,
            root_session_id: None,
            model: None,
            effort: None,
            last_activity: None,
            spend: Spend::default(),
            human_spoke_last: false,
            last_block: LastBlock::Nothing,
            reply_to_human: String::new(),
            last_reply: None,
            api_error: false,
            open_tools: 0,
            turn_open: false,
            kin: children::Kin::Desk,
            called: None,
            told: Vec::new(),
            asks: Vec::new(),
            evidence: turns::Evidence::default(),
        }
    }
}

fn message_text(payload: &Value) -> String {
    payload
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|c| c.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("")
}

/// Reduce a rollout to its current facts.
#[expect(
    clippy::too_many_lines,
    reason = "one reducer over the rollout's record kinds; splitting it scatters the shared state"
)]
pub fn parse_transcript(reader: impl BufRead) -> Transcript {
    let mut t = Transcript::blank();
    let mut counted = CodexUsage::default();
    let mut turns = turns::Turns::default();
    let mut tellings = children::Tellings::default();
    // A sub-agent or forked rollout's first record is its own `session_meta`; a later record
    // in the same file can be a *copy* of the parent's (source "cli", no subagent wrapper).
    // Identity, cwd and the kin must come from the first one, or a copied parent
    // meta later in the file would relabel a subagent thread as a real top-level session.
    let mut meta_seen = false;

    for line in reader.lines().map_while(Result::ok) {
        let Ok(rec) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let Some(payload) = rec.get("payload") else {
            continue;
        };
        if let Some(at) = rec.get("timestamp").and_then(Value::as_str) {
            t.last_activity = Some(at.to_string());
        }
        let kind = rec.get("type").and_then(Value::as_str).unwrap_or_default();
        match kind {
            "session_meta" if !meta_seen => {
                meta_seen = true;
                if let Some(cwd) = payload.get("cwd").and_then(Value::as_str) {
                    t.cwd = Some(cwd.to_string());
                }
                t.root_session_id = payload
                    .get("session_id")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                let meta = children::read_meta(payload);
                t.kin = meta.kin;
                t.called = meta.called;
            }
            "turn_context" => {
                if let Some(cwd) = payload.get("cwd").and_then(Value::as_str) {
                    t.cwd = Some(cwd.to_string());
                }
                if let Some(model) = payload.get("model").and_then(Value::as_str) {
                    t.model = Some(model.to_string());
                }
                if let Some(effort) = payload.get("effort").and_then(Value::as_str) {
                    t.effort = Some(effort.to_string());
                }
            }
            "event_msg" => match payload.get("type").and_then(Value::as_str) {
                Some("task_started" | "turn_started") => turns.started(turn_id(payload)),
                Some("task_complete" | "turn_complete") => turns.ended(
                    turn_id(payload),
                    payload.get("error").is_some_and(|e| !e.is_null()),
                ),
                // Interrupting a turn writes `turn_aborted`, never a `task_complete` (41/41 real
                // aborts on this Mac). Without this, an interrupted live session shows Running forever.
                Some("turn_aborted") => turns.aborted(turn_id(payload)),
                Some("user_message") => turns.human(),
                Some(kind @ ("item_completed" | "sub_agent_activity")) => {
                    if kind == "item_completed"
                        && payload.pointer("/item/type").and_then(Value::as_str)
                            == Some("UserMessage")
                    {
                        turns.human();
                    }
                    tellings.read(
                        payload,
                        rec.get("timestamp")
                            .and_then(Value::as_str)
                            .unwrap_or_default(),
                    );
                }
                // Totals are cumulative and the same total is often re-sent (rate-limit
                // updates), so only the growth since the last one counted is priced.
                Some("token_count") => {
                    if let Some(info) = payload
                        .get("info")
                        .and_then(|i| TokenCountInfo::deserialize(i).ok())
                    {
                        let total = info.total_token_usage;
                        let long_context = info.last_token_usage.input > 272_000;
                        t.spend = t.spend
                            + pricing::codex_cost(
                                t.model.as_deref(),
                                total.since(counted),
                                long_context,
                            );
                        counted = total;
                    }
                }
                _ => {}
            },
            "response_item" => match payload.get("type").and_then(Value::as_str) {
                Some("message") => {
                    let role = payload
                        .get("role")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let text = message_text(payload);
                    if role == "user" {
                        turns.human();
                    } else if role == "assistant" {
                        turns.assistant(
                            &text,
                            payload.get("phase").and_then(Value::as_str) == Some("commentary"),
                        );
                        if let Some(first) = super::first_sentence(&text, super::REPLY_CHARS) {
                            t.last_reply = Some(first);
                        }
                    }
                }
                Some("reasoning") => turns.reasoning(),
                Some("function_call" | "custom_tool_call") => turns.call(
                    payload.get("call_id").and_then(Value::as_str),
                    payload
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                    payload
                        .get("arguments")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                    rec.get("timestamp")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                ),
                Some("function_call_output" | "custom_tool_call_output") => {
                    if let Some(id) = payload.get("call_id").and_then(Value::as_str) {
                        turns.output(id);
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }
    t.turn_open = turns.turn_open();
    t.open_tools = turns.open_tools();
    t.api_error = turns.api_error();
    t.human_spoke_last = turns.human_spoke_last();
    t.last_block = turns.last_block();
    t.reply_to_human = turns.reply().to_string();
    t.asks = turns.asks();
    t.evidence = turns.evidence();
    t.told = tellings.finish();
    t
}

fn turn_id(payload: &Value) -> Option<&str> {
    payload.get("turn_id").and_then(Value::as_str)
}

/// Turn a transcript plus liveness into the state rules' inputs.
#[must_use]
pub fn signals(t: &Transcript, live: bool) -> Signals {
    Signals {
        live,
        busy: t.turn_open,
        question_waiting: !t.asks.is_empty(),
        pending_tool: t.open_tools > 0,
        human_spoke_last: t.human_spoke_last,
        api_error: t.api_error,
        last_block: t.last_block,
        reply_to_human: t.reply_to_human.clone(),
    }
}

/// One rollout entry from `~/.codex/session_index.jsonl`.
struct IndexEntry {
    id: String,
    thread_name: String,
}

fn parse_index_line(text: &str) -> Option<IndexEntry> {
    let v: Value = serde_json::from_str(text).ok()?;
    Some(IndexEntry {
        id: v.get("id")?.as_str()?.to_string(),
        thread_name: v.get("thread_name")?.as_str()?.to_string(),
    })
}

/// Thread titles by root session id. The file is append-only; a later line
/// for the same id (a rename) overrides an earlier one.
pub fn read_title_index(path: &Path) -> HashMap<String, String> {
    let Ok(text) = fs::read_to_string(path) else {
        return HashMap::new();
    };
    let mut index = HashMap::new();
    for entry in text.lines().filter_map(parse_index_line) {
        index.insert(entry.id, entry.thread_name);
    }
    index
}

/// One Codex session as discovered on disk.
#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    pub session_id: String,
    pub transcript: Transcript,
    pub title: Option<String>,
    pub live_pid: Option<i32>,
    /// Whether it can be offered for resuming once it ends: false for a
    /// placeholder, which has no rollout to resume.
    pub resumable: bool,
    /// A running chat desk owns this thread.
    pub owned: bool,
    /// Its workers to draw, in pod order.
    pub helpers: Vec<Helper>,
    /// What is kept about each of its workers the disk knows, drawn or not, by thread id.
    pub kids: BTreeMap<String, children::Kid>,
}

/// Scans Codex's rollouts, re-reading one only when it changed.
pub struct CodexSource {
    home: PathBuf,
    /// Finished sessions stay visible this long after their last write.
    keep_finished: Duration,
    cache: HashMap<PathBuf, (SystemTime, u64, Transcript)>,
    live_pids: HashMap<String, i32>,
    /// Per top-level thread: its sub-agent threads with a turn open, rebuilt by every scan.
    busy_subagents: HashMap<String, usize>,
    /// The workers drawn thinking, running or failed, so one that is over is not shown as done.
    drawn: BTreeSet<String>,
    /// The running `codex` pids; tests swap in none, so a real session on the Mac stays out.
    pub(crate) list_pids: fn() -> Vec<i32>,
}

impl CodexSource {
    #[must_use]
    pub fn new(home: PathBuf) -> Self {
        Self {
            home,
            keep_finished: super::KEEP_FINISHED,
            cache: HashMap::new(),
            live_pids: HashMap::new(),
            busy_subagents: HashMap::new(),
            drawn: BTreeSet::new(),
            list_pids: list_codex_pids,
        }
    }

    fn transcript(&mut self, path: &Path) -> Option<Transcript> {
        let meta = fs::metadata(path).ok()?;
        let modified = meta.modified().ok()?;
        if let Some((m, len, t)) = self.cache.get(path)
            && *m == modified
            && *len == meta.len()
        {
            return Some(t.clone());
        }
        let parsed = parse_transcript(BufReader::new(fs::File::open(path).ok()?));
        self.cache
            .insert(path.to_path_buf(), (modified, meta.len(), parsed.clone()));
        Some(parsed)
    }

    /// Every live session, plus finished ones written to recently. `owned`: the threads
    /// running chat desks own, live whatever their files say.
    pub fn scan(&mut self, now: SystemTime, owned: &[String]) -> Vec<Found> {
        self.scan_with(now, (self.list_pids)(), owned)
    }

    /// `scan`, with the running `codex` pids supplied instead of queried from the OS.
    #[expect(
        clippy::too_many_lines,
        reason = "one pass from the rollouts to the finds; its stages share the parsed list"
    )]
    pub fn scan_with(&mut self, now: SystemTime, pids: Vec<i32>, owned: &[String]) -> Vec<Found> {
        let codex_home = self.home.join(".codex");
        let title_index = read_title_index(&codex_home.join("session_index.jsonl"));

        let mut paths = Vec::new();
        collect_jsonl(&codex_home.join("sessions"), &mut paths);

        let mut parsed: Vec<(PathBuf, String, SystemTime, Transcript)> = Vec::new();
        let mut parent_of: HashMap<String, String> = HashMap::new();
        // Per sub-agent: its spend, and whether it is busy (a turn open in a recent file).
        let mut subagents: Vec<(String, Spend, bool)> = Vec::new();
        // Per worker: its id, its parent, when its file was written, and its transcript.
        let mut workers: Vec<(String, String, SystemTime, Transcript)> = Vec::new();
        for path in paths {
            let Some(session_id) = path
                .file_stem()
                .and_then(|s| s.to_str())
                .and_then(session_uuid)
            else {
                continue;
            };
            let Ok(mtime) = fs::metadata(&path).and_then(|m| m.modified()) else {
                continue;
            };
            let Some(transcript) = self.transcript(&path) else {
                continue;
            };
            if transcript.kin != children::Kin::Desk {
                if let Some(parent) = transcript.kin.parent() {
                    parent_of.insert(session_id.clone(), parent.to_string());
                    let recent =
                        now.duration_since(mtime).unwrap_or(Duration::ZERO) <= self.keep_finished;
                    subagents.push((
                        session_id.clone(),
                        transcript.spend,
                        transcript.turn_open && recent,
                    ));
                }
                if let children::Kin::Worker { parent } = &transcript.kin {
                    workers.push((session_id, parent.clone(), mtime, transcript));
                }
                continue;
            }
            if transcript.cwd.is_none() {
                continue;
            }
            parsed.push((path, session_id, mtime, transcript));
        }

        // A sub-agent's spend is its top-level thread's: walk up through nested sub-agents.
        let (spend_by_thread, busy) = roll_up(subagents, &parent_of);
        self.busy_subagents = busy;
        for (_, id, _, transcript) in &mut parsed {
            if let Some(extra) = spend_by_thread.get(id) {
                transcript.spend = transcript.spend + *extra;
            }
        }

        let top_level: HashSet<String> = parsed.iter().map(|(_, id, _, _)| id.clone()).collect();
        let sessions_dir = codex_home.join("sessions");
        let process_open: Vec<_> = pids
            .into_iter()
            .map(|pid| probe(pid, &sessions_dir, &top_level))
            .collect();
        self.live_pids = resolve_liveness_by_open_files(&process_open);
        let placeholders: Vec<Found> = rolloutless_sessions(&process_open)
            .into_iter()
            .map(|(pid, cwd)| {
                let session_id = format!("codex-new-{pid}");
                self.live_pids.insert(session_id.clone(), pid);
                Found {
                    session_id,
                    transcript: Transcript {
                        cwd: Some(cwd),
                        ..Transcript::blank()
                    },
                    title: Some("New Codex session".to_string()),
                    live_pid: Some(pid),
                    resumable: false,
                    owned: false,
                    helpers: Vec::new(),
                    kids: BTreeMap::new(),
                }
            })
            .collect();

        let is_owned = |id: &str| owned.iter().any(|o| o == id);
        parsed.retain(|(_, id, mtime, _)| {
            self.live_pids.contains_key(id)
                || is_owned(id)
                || now
                    .duration_since(*mtime)
                    .is_ok_and(|age| age <= self.keep_finished)
        });
        let roots: Vec<children::Root<'_>> = parsed
            .iter()
            .map(|(_, id, _, transcript)| children::Root {
                id,
                live: self.live_pids.contains_key(id) || is_owned(id),
                transcript,
            })
            .collect();
        let seen: Vec<children::Seen<'_>> = workers
            .iter()
            .map(|(id, parent, written, transcript)| children::Seen {
                id,
                parent,
                transcript,
                written: *written,
            })
            .collect();
        let mut teams = children::teams(&roots, &seen, now, &mut self.drawn);
        parsed
            .into_iter()
            .map(|(_, session_id, _, transcript)| {
                let title = transcript
                    .root_session_id
                    .as_ref()
                    .and_then(|id| title_index.get(id))
                    .cloned();
                let live_pid = self.live_pids.get(&session_id).copied();
                let team = teams.remove(&session_id).unwrap_or_default();
                Found {
                    owned: is_owned(&session_id),
                    session_id,
                    transcript,
                    title,
                    live_pid,
                    resumable: true,
                    helpers: team.helpers,
                    kids: team.kids,
                }
            })
            .chain(placeholders)
            .collect()
    }

    /// Pid of a running session, for focusing its terminal.
    #[must_use]
    pub fn pid_of(&self, session_id: &str) -> Option<i32> {
        self.live_pids.get(session_id).copied()
    }
}

/// The uuid a rollout filename ends with, e.g.
/// `rollout-2026-09-29T10-00-00-deadbeef-0000-4000-8000-000000000000` ->
/// `deadbeef-0000-4000-8000-000000000000`. A uuid is 5 `-`-separated groups
/// of hex digits (8-4-4-4-12); we take the last 5 groups of the filename.
fn session_uuid(file_stem: &str) -> Option<String> {
    let groups: Vec<&str> = file_stem.rsplit('-').take(5).collect();
    if groups.len() != 5
        || groups
            .iter()
            .any(|g| !g.chars().all(|c| c.is_ascii_hexdigit()))
    {
        return None;
    }
    let lens = [12, 4, 4, 4, 8];
    if groups.iter().zip(lens).any(|(g, want)| g.len() != want) {
        return None;
    }
    let mut rev: Vec<&str> = groups;
    rev.reverse();
    Some(rev.join("-"))
}

/// Each sub-agent's spend and busy flag, added to the top-level thread that spawned it at any depth.
fn roll_up(
    subagents: Vec<(String, Spend, bool)>,
    parent_of: &HashMap<String, String>,
) -> (HashMap<String, Spend>, HashMap<String, usize>) {
    let mut spend_by_thread: HashMap<String, Spend> = HashMap::new();
    let mut busy_by_thread: HashMap<String, usize> = HashMap::new();
    for (id, spend, busy) in subagents {
        let mut at = &id;
        // Bounded, so a malformed parent cycle cannot hang the scan.
        for _ in 0..parent_of.len() {
            let Some(parent) = parent_of.get(at) else {
                break;
            };
            at = parent;
        }
        let total = spend_by_thread.entry(at.clone()).or_default();
        *total = *total + spend;
        if busy {
            *busy_by_thread.entry(at.clone()).or_default() += 1;
        }
    }
    (spend_by_thread, busy_by_thread)
}

impl super::Discovery {
    /// How many sub-agent threads of the Codex session `session_id` had a turn in flight at the last scan.
    #[must_use]
    pub fn codex_subagents_busy(&self, session_id: &str) -> usize {
        self.codex
            .busy_subagents
            .get(session_id)
            .copied()
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests;
