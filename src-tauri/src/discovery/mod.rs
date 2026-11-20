//! Session discovery: reads each tool's own files and assembles the floors,
//! desks and waiting questions. Claude Code and Codex today.

pub mod claude_code;
pub mod codex;
pub mod pets;
pub mod pricing;
pub mod rules;

use crate::model::{
    AgentMessage, AnswerVia, ControlMode, EndedSession, Helper, Pet, Project, Question,
    QuestionOption, Session, SessionState, Spend, ToolKind,
};
use rules::Signals;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

/// What discovery currently sees.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Discovered {
    /// Absolute root folder of each floor, by project id.
    pub roots: BTreeMap<String, PathBuf>,
    pub projects: Vec<Project>,
    pub sessions: Vec<Session>,
    pub questions: Vec<Question>,
    /// Sessions that ended recently, newest first.
    pub ended: Vec<EndedSession>,
    /// The folder each ended session ran in, to resume it there.
    pub resume_dirs: BTreeMap<String, PathBuf>,
    /// The top of the checkout each live or signed-out session works in now, by session id,
    /// and each helper's by [`helper_key`]: a worktree's own folder, or its repo's root.
    /// Kept here for the backend only. Never sent to the webview.
    pub checkouts: BTreeMap<String, PathBuf>,
    /// What other sessions and sub-agents sent the sessions found, oldest first.
    pub peer_messages: Vec<AgentMessage>,
    /// The skills each live session called since the human last spoke to it, by session id.
    /// Kept here for the backend only: the snapshot decides which of them count as a review.
    pub skills: BTreeMap<String, Vec<String>>,
}

/// FNV-1a, 32-bit: the same stable hash the frontend uses for looks.
#[must_use]
pub fn fnv1a(s: &str) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for b in s.bytes() {
        h ^= u32::from(b);
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

/// Where a working directory sits: its floor's root, and whether it is inside a
/// linked git worktree of that floor's repo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub root: PathBuf,
    pub worktree: bool,
    /// The top of the checkout the directory is in: the worktree's own folder, else `root`.
    pub checkout: PathBuf,
}

/// The floor a working directory belongs to: its git root, or itself. A linked
/// worktree belongs to its main checkout's floor.
#[must_use]
pub fn locate(cwd: &Path) -> Location {
    let mut dir = Some(cwd);
    while let Some(d) = dir {
        let git = d.join(".git");
        if git.is_file()
            && let Some(root) = main_checkout(d, &git)
        {
            return Location {
                root,
                worktree: true,
                checkout: d.to_path_buf(),
            };
        }
        if git.exists() {
            return Location {
                root: d.to_path_buf(),
                worktree: false,
                checkout: d.to_path_buf(),
            };
        }
        dir = d.parent();
    }
    Location {
        root: cwd.to_path_buf(),
        worktree: false,
        checkout: cwd.to_path_buf(),
    }
}

/// When a message was sent, for ordering; one that does not parse sorts first.
#[must_use]
pub(crate) fn sent_at(at: &str) -> Option<chrono::DateTime<chrono::FixedOffset>> {
    chrono::DateTime::parse_from_rfc3339(at).ok()
}

/// How long a session's `last_reply` may be.
pub(crate) const REPLY_CHARS: usize = 140;
/// How long an ended session stays on its floor's sign-out sheet.
pub(crate) const KEEP_FINISHED: std::time::Duration = std::time::Duration::from_hours(12);

/// The first sentence of a reply, at most `max` chars. None when it says nothing.
/// Blank lines and fenced code come first and are skipped; a heading's `#`s or a list marker
/// (`-`, `*`, `1.`) is not part of the sentence. A `.` `!` or `?` ends it only before whitespace,
/// so `v1.2` and `foo.rs` stay whole, and a `.` after a lone letter or a number (`e.g.`, `1.`) does not end it.
#[must_use]
pub(crate) fn first_sentence(text: &str, max: usize) -> Option<String> {
    let mut in_fence = false;
    let line = text.lines().map(str::trim).find(|l| {
        if l.starts_with("```") {
            in_fence = !in_fence;
            return false;
        }
        !in_fence && !l.is_empty()
    })?;
    let line = unmarked(line);
    let mut end = line.len();
    let mut chars = line.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        let spaced = chars.peek().is_none_or(|(_, n)| n.is_whitespace());
        if matches!(c, '.' | '!' | '?') && spaced && !(c == '.' && abbreviation(&line[..i])) {
            end = i + c.len_utf8();
            break;
        }
    }
    let sentence = line[..end].trim();
    (!sentence.is_empty()).then(|| crate::project_files::truncate(sentence, max))
}

/// The line without a markdown heading's `#`s or a list marker.
fn unmarked(line: &str) -> &str {
    let line = line.trim_start_matches('#').trim_start();
    if let Some(rest) = line.strip_prefix(['-', '*']).filter(|r| r.starts_with(' ')) {
        return rest.trim_start();
    }
    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    match line[digits..].strip_prefix(". ") {
        Some(rest) if digits > 0 => rest.trim_start(),
        _ => line,
    }
}

/// Whether the word ending `before` is one letter or a run of digits, so a `.` after it is `e.g.` or `1.`.
fn abbreviation(before: &str) -> bool {
    let word: Vec<char> = before
        .chars()
        .rev()
        .take_while(|c| c.is_alphanumeric())
        .collect();
    match word.as_slice() {
        [] => false,
        [_] => true,
        w => w.iter().all(char::is_ascii_digit),
    }
}

/// The key of a helper's entry in `Discovered::checkouts`. Session ids hold no `/`.
#[must_use]
pub fn helper_key(session_id: &str, helper_id: &str) -> String {
    format!("{session_id}/{helper_id}")
}

/// The branch checked out at the top of a checkout, read from its HEAD file. None when
/// HEAD is detached, unreadable, or the reftable placeholder.
#[must_use]
pub fn branch_of(checkout: &Path) -> Option<String> {
    let git = checkout.join(".git");
    let git_dir = if git.is_file() {
        linked_git_dir(checkout, &git)?
    } else {
        git
    };
    let head = fs::read_to_string(git_dir.join("HEAD")).ok()?;
    let branch = head.trim().strip_prefix("ref: refs/heads/")?;
    (!branch.is_empty() && branch != ".invalid").then(|| branch.to_string())
}

/// The git dir a worktree's `.git` file points at.
fn linked_git_dir(dir: &Path, git: &Path) -> Option<PathBuf> {
    let pointer = fs::read_to_string(git).ok()?;
    Some(dir.join(pointer.strip_prefix("gitdir:")?.trim()))
}

#[must_use]
pub fn project_root(cwd: &Path) -> PathBuf {
    locate(cwd).root
}

/// The main checkout of the worktree at `dir`, whose `.git` file reads
/// `gitdir: <main>/.git/worktrees/<name>`. Only a worktree's git dir has a
/// `commondir`, so a submodule (also a `.git` file) stays its own floor, as does
/// a worktree of a bare repo, which has no main checkout.
fn main_checkout(dir: &Path, git: &Path) -> Option<PathBuf> {
    let git_dir = linked_git_dir(dir, git)?;
    let common = fs::read_to_string(git_dir.join("commondir")).ok()?;
    let common = normalise(&git_dir.join(common.trim()));
    if common.file_name()? != ".git" {
        return None;
    }
    common.parent().map(Path::to_path_buf)
}

/// Folds `.` and `..` out of a path without touching the disk, so the main
/// checkout keeps the spelling its own sessions report.
fn normalise(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

#[must_use]
pub fn project_id(root: &Path) -> String {
    format!("p-{:08x}", fnv1a(&root.to_string_lossy()))
}

/// `/Users/me/code/app` → `~/code/app`.
#[must_use]
pub fn display_path(root: &Path, home: &Path) -> String {
    match root.strip_prefix(home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Ok(rest) => format!("~/{}", rest.to_string_lossy()),
        Err(_) => root.to_string_lossy().into_owned(),
    }
}

/// Runs every source and assembles the office.
pub struct Discovery {
    home: PathBuf,
    claude: claude_code::ClaudeSource,
    codex: codex::CodexSource,
    /// The OS's process listing, for each session's pets; tests swap in none.
    read_processes: fn() -> Vec<pets::ProcessRow>,
    /// The checkouts of the previous scan, to remember a worktree that was then removed.
    checkouts: BTreeMap<String, PathBuf>,
}

impl Discovery {
    #[must_use]
    pub fn new(home: PathBuf) -> Self {
        Self {
            claude: claude_code::ClaudeSource::new(home.clone()),
            codex: codex::CodexSource::new(home.clone()),
            read_processes: pets::read_table,
            checkouts: BTreeMap::new(),
            home,
        }
    }

    pub fn scan(&mut self, now: SystemTime) -> Discovered {
        let table = (self.read_processes)();
        let claude = self.claude.scan(now);
        let mut peer_messages: Vec<AgentMessage> = claude
            .iter()
            .flat_map(|f| f.transcript.peer_messages.iter().cloned())
            .collect();
        peer_messages.sort_by_cached_key(|m| sent_at(&m.sent_at));
        // A resumed transcript repeats its msg_ids; none of 526 real msg_ids repeated across receivers (16 Nov 2026).
        let mut seen = std::collections::HashSet::new();
        peer_messages.retain(|m| seen.insert(m.id.clone()));
        let inputs = claude
            .into_iter()
            .filter_map(|f| from_claude(f, &table))
            .chain(
                self.codex
                    .scan(now)
                    .into_iter()
                    .filter_map(|f| from_codex(f, &table)),
            )
            .filter(still_there)
            .collect();
        let mut found = assemble(&self.home, inputs, locate, &self.checkouts);
        found.peer_messages = peer_messages;
        self.checkouts.clone_from(&found.checkouts);
        found
    }

    /// Leave out the `codex` processes really running, which no test's home owns.
    #[cfg(test)]
    pub fn ignore_running_codex(&mut self) {
        self.codex.list_pids = Vec::new;
        self.read_processes = Vec::new;
    }

    /// Transcripts read from disk so far.
    #[cfg(test)]
    #[must_use]
    pub fn parses(&self) -> u32 {
        self.claude.parses
    }

    /// Pid of a running session, for focusing its terminal window.
    #[must_use]
    pub fn pid_of(&self, session_id: &str) -> Option<i32> {
        self.claude
            .pid_of(session_id)
            .or_else(|| self.codex.pid_of(session_id))
    }
}

/// A formal question a tool asked, tool-agnostic.
struct FormalQuestionInput {
    /// The part of the question's id unique within the session (e.g. the tool-use id).
    id_suffix: String,
    prompt: String,
    options: Vec<String>,
    asked_at: String,
}

/// One discovered session, reduced to what `assemble` needs, independent of
/// which tool found it.
struct SessionInput {
    session_id: String,
    tool: ToolKind,
    /// The folder it started in, which places it on a floor.
    cwd: String,
    /// The folder it works in now: a worktree of its floor's repo earns the badge.
    latest_cwd: String,
    title: Option<String>,
    live: bool,
    model: Option<String>,
    effort: Option<String>,
    last_activity: String,
    spend: Spend,
    signals: Signals,
    /// A formal question (e.g. Claude Code's `AskUserQuestion`), when the tool has one.
    /// Otherwise a waiting state falls back to the plain-prose scorer.
    formal_question: Option<FormalQuestionInput>,
    helpers: Vec<Helper>,
    /// Where each helper works now, by helper id.
    helper_places: BTreeMap<String, Location>,
    pets: Vec<Pet>,
    /// A person started it, so once it ends it can be offered to resume.
    resumable: bool,
    /// First sentence of its latest reply to the human.
    last_reply: Option<String>,
    /// The name the tool's own registry gives it.
    session_name: Option<String>,
    session_name_source: Option<crate::model::NameSource>,
    skills: Vec<String>,
}

fn from_claude(f: claude_code::Found, table: &[pets::ProcessRow]) -> Option<SessionInput> {
    let cwd = f
        .transcript
        .cwd
        .clone()
        .or_else(|| f.live.as_ref().map(|l| l.cwd.clone()))?;
    let latest_cwd = f
        .transcript
        .latest_cwd
        .clone()
        .unwrap_or_else(|| cwd.clone());
    let signals = claude_code::signals(&f.transcript, f.live.as_ref());
    let title = f
        .transcript
        .title
        .clone()
        .or_else(|| f.live.as_ref().and_then(|l| l.name.clone()));
    let last_activity = f.transcript.last_activity.clone().unwrap_or_default();
    let formal_question = match (&f.transcript.question, &f.live) {
        (Some(q), _) => Some(FormalQuestionInput {
            id_suffix: q.tool_use_id.clone(),
            prompt: q.prompt.clone(),
            options: q.options.clone(),
            asked_at: q.asked_at.clone(),
        }),
        // An open question or permission prompt reaches the transcript only once answered.
        (None, Some(live)) if live.waiting => Some(FormalQuestionInput {
            id_suffix: "waiting".to_string(),
            prompt: "Waiting for you in its terminal".to_string(),
            options: Vec::new(),
            asked_at: last_activity.clone(),
        }),
        _ => None,
    };
    Some(SessionInput {
        session_id: f.session_id,
        tool: ToolKind::ClaudeCode,
        cwd,
        latest_cwd,
        title,
        live: f.live.is_some(),
        model: f.transcript.model.clone(),
        effort: f.transcript.effort.clone(),
        last_activity,
        spend: f.transcript.spend,
        signals,
        formal_question,
        pets: f
            .live
            .as_ref()
            .map(|l| pets::pets_of(l.pid, table, &f.transcript.monitors))
            .unwrap_or_default(),
        helpers: f.helpers,
        helper_places: f.helper_places,
        resumable: !f.transcript.scripted,
        last_reply: f.transcript.last_reply,
        session_name: f.live.as_ref().and_then(|l| l.name.clone()),
        session_name_source: f.live.as_ref().and_then(|l| l.name_source),
        skills: f.transcript.skills_since_prompt,
    })
}

fn from_codex(f: codex::Found, table: &[pets::ProcessRow]) -> Option<SessionInput> {
    let cwd = f.transcript.cwd.clone()?;
    let signals = codex::signals(&f.transcript, f.live_pid);
    Some(SessionInput {
        session_id: f.session_id,
        tool: ToolKind::Codex,
        latest_cwd: cwd.clone(),
        cwd,
        title: f.title,
        live: f.live_pid.is_some(),
        model: f.transcript.model.clone(),
        effort: f.transcript.effort.clone(),
        last_activity: f.transcript.last_activity.clone().unwrap_or_default(),
        spend: f.transcript.spend,
        signals,
        // Codex has no formal question/approval event in the rollouts inspected on this Mac
        // (checked 2026-09-29): no `approval`- or `input-request`-shaped event exists. A
        // waiting Codex session always falls back to the plain-prose scorer.
        formal_question: None,
        helpers: Vec::new(),
        helper_places: BTreeMap::new(),
        pets: f
            .live_pid
            .map(|pid| pets::pets_of(pid, table, &[]))
            .unwrap_or_default(),
        resumable: f.resumable,
        last_reply: f.transcript.last_reply,
        session_name: None,
        session_name_source: None,
        skills: Vec::new(),
    })
}

/// What a session with no title of its own is called.
const UNTITLED: &str = "Untitled session";

/// What a waiting session asks the human: its formal question, or the asking
/// line of a plain-prose ask (soft block), which has no options.
fn question_for(input: &SessionInput, root: &Path) -> Question {
    let (suffix, prompt, options, priority, asked_at) = match &input.formal_question {
        Some(q) => (
            q.id_suffix.as_str(),
            q.prompt.clone(),
            q.options
                .iter()
                .enumerate()
                .map(|(i, label)| QuestionOption {
                    id: format!("opt-{i}"),
                    label: label.clone(),
                })
                .collect(),
            0,
            q.asked_at.clone(),
        ),
        None => (
            "prose",
            rules::score_soft_block(&input.signals.reply_to_human).question,
            Vec::new(),
            1,
            input.last_activity.clone(),
        ),
    };
    Question {
        id: format!("{}:{suffix}", input.session_id),
        project_id: project_id(root),
        session_id: input.session_id.clone(),
        prompt,
        options,
        allow_other: true,
        answer_via: AnswerVia::Terminal,
        priority,
        asked_at,
        context: String::new(),
        source_file: None,
    }
}

/// An ended session's line on its floor's sign-out sheet.
fn signed_out(input: SessionInput, root: &Path) -> EndedSession {
    EndedSession {
        id: input.session_id,
        tool: input.tool,
        project_id: project_id(root),
        title: input.title.unwrap_or_else(|| UNTITLED.to_string()),
        ended_at: input.last_activity,
        model: input.model,
        effort: input.effort,
        last_reply: input.last_reply,
    }
}

/// An ended session whose folder is gone (a deleted checkout, a temporary
/// folder) has nowhere to resume and no floor worth keeping. A live one stays:
/// its agent is still at work even if the folder went from under it.
fn still_there(input: &SessionInput) -> bool {
    input.live || Path::new(&input.cwd).is_dir()
}

/// Where a session sits: its floor, the checkout it works in, and its worktree badge.
struct Placement {
    root: PathBuf,
    checkout: PathBuf,
    worktree: bool,
    branch: Option<String>,
}

fn place(
    input: &SessionInput,
    locate: &impl Fn(&Path) -> Location,
    before: &BTreeMap<String, PathBuf>,
) -> Placement {
    let started = locate(Path::new(&input.cwd));
    let now_in = locate(Path::new(&input.latest_cwd));
    let earlier = before.get(&input.session_id);
    // A session launched above a repo (from the plain folder that holds several) sits on the first repo
    // it works in and stays there: a later `cd` into another repo does not move it (A Human, OB-055).
    // One started inside a repo never moves, not into a submodule or a nested clone either.
    // Only into a real checkout: `locate` returns a plain folder as its own root, and a `cd ~/Downloads/x`
    // must not raise a floor called "x".
    let plain_start = !started.root.join(".git").exists();
    let below = |root: &Path| {
        plain_start
            && root != started.root
            && root.starts_with(&started.root)
            && root.join(".git").exists()
    };
    let pinned = earlier
        .map(|was| locate(was).root)
        .filter(|root| below(root));
    let home = pinned.or_else(|| below(&now_in.root).then(|| now_in.root.clone()));
    let root = home.clone().unwrap_or_else(|| started.root.clone());
    let at_home = now_in.root == root;
    let worktree = now_in.worktree && at_home;
    let branch = if worktree {
        branch_of(&now_in.checkout)
    } else {
        None
    };
    // Away from its floor's repo, a pinned session keeps that repo as its checkout, which is also
    // what holds the pin at the next scan.
    let checkout = lost_checkout(earlier, &input.latest_cwd).unwrap_or(if at_home {
        now_in.checkout
    } else if home.is_some() {
        root.clone()
    } else {
        started.checkout
    });
    Placement {
        root,
        checkout,
        worktree,
        branch,
    }
}

/// Each floor as a project, named after its folder.
fn projects_of(floors: BTreeMap<PathBuf, Vec<String>>, home: &Path) -> Vec<Project> {
    floors
        .into_iter()
        .map(|(root, session_ids)| Project {
            id: project_id(&root),
            name: root.file_name().map_or_else(
                || root.to_string_lossy().into_owned(),
                |n| n.to_string_lossy().into_owned(),
            ),
            path: display_path(&root, home),
            session_ids,
        })
        .collect()
}

/// Build floors, desks and questions from every tool's finds. `locate` places a
/// working directory on its floor (git root in production, identity in tests).
fn assemble(
    home: &Path,
    inputs: Vec<SessionInput>,
    locate: impl Fn(&Path) -> Location,
    before: &BTreeMap<String, PathBuf>,
) -> Discovered {
    let mut floors: BTreeMap<PathBuf, Vec<String>> = BTreeMap::new();
    let mut sessions = Vec::new();
    let mut questions = Vec::new();
    let mut ended = Vec::new();
    let mut resume_dirs = BTreeMap::new();
    let mut checkouts = BTreeMap::new();
    let mut skills = BTreeMap::new();

    for input in inputs {
        let Placement {
            root,
            checkout,
            worktree,
            branch,
        } = place(&input, &locate, before);
        // An ended session leaves the office; its floor stays while the session was recent.
        let ids = floors.entry(root.clone()).or_default();
        if !input.live {
            if input.resumable {
                resume_dirs.insert(input.session_id.clone(), PathBuf::from(&input.cwd));
                checkouts.insert(input.session_id.clone(), checkout);
                ended.push(signed_out(input, &root));
            }
            continue;
        }
        ids.push(input.session_id.clone());
        checkouts.insert(input.session_id.clone(), checkout);
        for (helper_id, place) in &input.helper_places {
            if place.root == root {
                checkouts.insert(
                    helper_key(&input.session_id, helper_id),
                    place.checkout.clone(),
                );
            }
        }
        let state = rules::derive_state(&input.signals);

        let mut pending = Vec::new();
        if state == SessionState::WaitingHuman {
            let question = question_for(&input, &root);
            pending.push(question.id.clone());
            questions.push(question);
        }

        let title = input.title.clone().unwrap_or_else(|| UNTITLED.to_string());
        skills.insert(input.session_id.clone(), input.skills);
        sessions.push(Session {
            id: input.session_id,
            tool: input.tool,
            project_id: project_id(&root),
            title,
            state,
            control: ControlMode::RaiseWindow,
            model: input.model,
            effort: input.effort,
            last_activity_at: input.last_activity,
            pending_question_ids: pending,
            spend: input.spend,
            helpers: input.helpers,
            pets: input.pets,
            worktree,
            branch,
            last_reply: input.last_reply,
            session_name: input.session_name,
            session_name_source: input.session_name_source,
            self_reviewed: false,
        });
    }

    let root_by_id: BTreeMap<String, PathBuf> =
        floors.keys().map(|r| (project_id(r), r.clone())).collect();
    let projects = projects_of(floors, home);
    // Most urgent first, then oldest ask first.
    questions.sort_by(|a, b| {
        a.priority
            .cmp(&b.priority)
            .then_with(|| a.asked_at.cmp(&b.asked_at))
    });
    ended.sort_by(|a: &EndedSession, b| b.ended_at.cmp(&a.ended_at));
    Discovered {
        roots: root_by_id,
        projects,
        sessions,
        questions,
        ended,
        resume_dirs,
        checkouts,
        peer_messages: Vec::new(),
        skills,
    }
}

/// The checkout a session was last seen in, when that folder has since been deleted and
/// the session's latest folder is still inside it. Opening it then says it is gone, where
/// a fresh look would quietly land on the main checkout.
fn lost_checkout(before: Option<&PathBuf>, latest_cwd: &str) -> Option<PathBuf> {
    before
        .filter(|was| !was.is_dir() && Path::new(latest_cwd).starts_with(was))
        .cloned()
}

/// Every `*.jsonl` under `dir`, at any depth.
fn collect_jsonl(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            collect_jsonl(&path, out);
        } else if path.extension().is_some_and(|x| x == "jsonl") {
            out.push(path);
        }
    }
}

#[cfg(test)]
mod checkout_tests;

#[cfg(test)]
mod tests;
