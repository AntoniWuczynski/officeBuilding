//! The office: live discovery plus the app's own records, behind one lock.
//! Every Tauri command goes through here, so there is one source of truth.

use crate::connectors::hub::Hub;
use crate::discovery::{self, Discovered, Discovery};
use crate::feed::Feed;
use crate::focus;
use crate::model::{
    AgentMessage, AnswerVia, ControlMode, DeskView, EndedSession, HireFailed, HireRequest,
    OfficeSnapshot, Project, Question, QuestionAnswer, Session, SessionState, SharedBoard, Shell,
    Spend, TerminalBacklog, TerminalExit, ToolKind,
};
use crate::project_files::{self, ProjectFiles};
use crate::pty::{self, Terminals};
use crate::qa;
use crate::settings::SettingsStore;
use crate::setup::{Launch, Library};
use crate::store::{Records, Store};
use crate::structured::{self, desks::Desks};
use launch::{
    CLAUDE_SUBCOMMANDS, DESK_ACTIVE, Desk, Launched, chosen_model, desk_session, desk_session_id,
    hireable, is_discovered, launch_command,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

struct Inner {
    discovery: Discovery,
    /// Events from the hooks of agents the app launched.
    feed: Feed,
    found: Discovered,
    /// Where each moved session was discovered, by session id: what a move back restores.
    moved_from: BTreeMap<String, String>,
    /// Decisions and TODOs from each floor's own files, by project id.
    files: BTreeMap<String, ProjectFiles>,
    /// The checklist files `boards.shared_files` names, in that order.
    boards: Vec<SharedBoard>,
    store: Store,
    /// The session each app-owned terminal is running, by terminal id. It
    /// outlives the agent so its last output stays readable.
    owned: BTreeMap<String, String>,
    /// Every session a terminal has run, by session id, so a panel opened on an
    /// earlier session (before a `/clear`) keeps reaching the same terminal.
    ran_in: BTreeMap<String, String>,
    /// The terminal running `claude attach` for each background session, by session id.
    attached: BTreeMap<String, String>,
    /// Hired terminals no session has been bound to yet, with the task they
    /// were hired for: if one exits first, the hire failed after all.
    hired: BTreeMap<String, String>,
    /// Terminals running a tool discovery cannot see: each is its own desk while
    /// it runs, by terminal id.
    desks: BTreeMap<String, Desk>,
    /// Plain shells the human opened, by terminal id, until they exit.
    shells: BTreeMap<String, Shell>,
    /// The terminal of each running QA reviewer, with its floor's project id. Only
    /// `seat_reviewer` adds to it and only `dismiss_reviewer` removes from it.
    reviewers: BTreeMap<String, String>,
    /// Notes on their way to desks, and A Human's keystrokes.
    mail: deliver::Mail,
    /// Each floor's running manager, by project id.
    seats: BTreeMap<String, manager::Seat>,
    /// Which terminals were hired with the chat view.
    chat: chat::State,
    /// Used-up subscriptions, handovers and what each desk has to say about them.
    handover: handover::State,
}

pub struct Office {
    home: PathBuf,
    inner: Mutex<Inner>,
    terminals: Terminals,
    setup: Library,
    /// Agents driven over their tool's protocol instead of a terminal.
    structured: Arc<Desks>,
    /// The setup floor's project id: the library folder is its floor.
    setup_project_id: String,
    /// The connectors' cache of each linked floor's items.
    connectors: Arc<Hub>,
    /// The app's settings, if this office was given them.
    settings: Option<Arc<SettingsStore>>,
    /// The pull request watch behind each floor's QA desk, if this office was given one.
    qa: Option<Arc<qa::watch::Watch>>,
    /// What managers need (the token registry, the socket, the helper), or why there is none.
    manager: Result<manager::ManagerHost, String>,
    /// Held across a manager's hire, from its checks to its record, so hires are one at a time.
    hiring: Mutex<()>,
    /// The Claude Code conversations a panel follows. Taken before the office lock, never under it.
    chats: Mutex<chat::follow::Followers>,
    /// The last ticket handed to a `keep_chat` call, so a stale desk switch is ignored.
    chat_tickets: AtomicU64,
    /// The provider router, if this office was given one.
    routing: Option<Arc<crate::routing::router::Router>>,
    /// What handing desks over needs, if this office was given it.
    handover_host: Option<HandoverHost>,
    /// The transcript readers the handover follows. Taken before the office lock, never under it.
    limits: Mutex<handover::Limits>,
}

/// Pushed when a hired agent exits before it ever reached a desk.
pub const HIRE_FAILED_EVENT: &str = "office://hire-failed";

/// How far up a live session's process tree to look for a terminal we started
/// (a launcher can sit in between; Codex and Claude Code themselves do not).
const MAX_ANCESTRY: usize = 4;

/// How long a hired agent has to keep running, once the login shell has handed
/// over to it, before the hire counts as started.
const LAUNCH_GRACE: Duration = Duration::from_millis(1500);
/// The longest a login shell may take to hand over (a slow profile). Past it the
/// hire counts as started, and a later exit is reported by [`Office::hire_failed`].
const LAUNCH_LIMIT: Duration = Duration::from_secs(15);
const LAUNCH_POLL: Duration = Duration::from_millis(100);
/// How long a demolished floor's agents have to exit after SIGTERM.
const STOP_WAIT: Duration = Duration::from_secs(5);
/// Process names of shells: while the started process still runs one, the
/// login shell has not handed over to the agent yet.
const SHELL_NAMES: &[&str] = &["zsh", "bash", "sh", "fish", "dash", "ksh", "tcsh", "csh"];

/// The app-owned terminal a process runs in, and how many generations below
/// the agent we started there the process is (0: it is that agent).
fn owning_terminal(
    pid: i32,
    children: &BTreeMap<i32, String>,
    parent_of: impl Fn(i32) -> Option<i32>,
) -> Option<(String, usize)> {
    let mut current = pid;
    for depth in 0..=MAX_ANCESTRY {
        if let Some(id) = children.get(&current) {
            return Some((id.clone(), depth));
        }
        match parent_of(current) {
            Some(parent) if parent > 1 => current = parent,
            _ => return None,
        }
    }
    None
}

/// How long a move outlasts its session's absence: the sign-out sheet's window
/// (`keep_finished` in the discovery sources, which do not share a constant).
const MOVE_KEPT_UNSEEN: Duration = Duration::from_hours(12);

/// Forget the moves whose session has been off the books too long, and note which are off it now.
/// A move never confirmed (no `move_seen` entry) goes at its first miss. True when anything changed.
fn prune_moves(
    records: &mut Records,
    on_books: impl Fn(&str) -> bool,
    now: chrono::DateTime<chrono::Utc>,
) -> bool {
    let before = (records.moved_sessions.len(), records.move_seen.clone());
    let stamp = now.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let Records {
        moved_sessions,
        move_seen,
        ..
    } = records;
    moved_sessions.retain(|id, _| {
        if on_books(id) {
            move_seen.insert(id.clone(), None);
            return true;
        }
        match move_seen.get(id).cloned() {
            None => false,
            Some(None) => {
                move_seen.insert(id.clone(), Some(stamp.clone()));
                true
            }
            Some(Some(since)) => chrono::DateTime::parse_from_rfc3339(&since).map_or_else(
                |_| {
                    move_seen.insert(id.clone(), Some(stamp.clone()));
                    true
                },
                |t| {
                    (now - t.to_utc())
                        .to_std()
                        .is_ok_and(|gone| gone <= MOVE_KEPT_UNSEEN)
                },
            ),
        }
    });
    move_seen.retain(|id, _| moved_sessions.contains_key(id));
    before != (moved_sessions.len(), move_seen.clone())
}

/// Put each session the human moved on the floor they chose, wherever it signs out too, and
/// say where each one was found. A move to a floor that is not here is ignored.
fn apply_moves(
    found: &mut Discovered,
    moves: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let mut from = BTreeMap::new();
    for (id, to) in moves {
        if !found.projects.iter().any(|p| p.id == *to) {
            continue;
        }
        let live = found
            .sessions
            .iter_mut()
            .find(|s| s.id == *id)
            .map(|s| std::mem::replace(&mut s.project_id, to.clone()));
        let signed_out = found
            .ended
            .iter_mut()
            .find(|e| e.id == *id)
            .map(|e| std::mem::replace(&mut e.project_id, to.clone()));
        let Some(was) = live.clone().or(signed_out) else {
            continue;
        };
        if was == *to {
            continue;
        }
        from.insert(id.clone(), was.clone());
        if live.is_none() {
            continue;
        }
        for p in &mut found.projects {
            if p.id == was {
                p.session_ids.retain(|s| s != id);
            } else if p.id == *to {
                p.session_ids.push(id.clone());
            }
        }
        for q in found.questions.iter_mut().filter(|q| q.session_id == *id) {
            q.project_id.clone_from(to);
        }
    }
    from
}

/// Bind live sessions to the terminals we started them in, then give every
/// bound session full control. Returns the sessions now live in some other
/// terminal.
///
/// A terminal goes to the live session closest to the agent we started (a
/// session the agent itself launched does not take it), then to the one most
/// recently active (a new session id in the same process, as after `/clear`).
/// A session that is live anywhere else is never ours, whatever it ran in
/// before. A session whose pid cannot be read right now keeps what it had.
fn claim_owned(
    sessions: &mut [Session],
    owned: &mut BTreeMap<String, String>,
    children: &BTreeMap<i32, String>,
    pid_of: impl Fn(&str) -> Option<i32>,
    parent_of: impl Fn(i32) -> Option<i32>,
) -> Vec<String> {
    let mut best: BTreeMap<String, (usize, &Session)> = BTreeMap::new();
    let mut elsewhere: Vec<String> = Vec::new();
    for s in sessions
        .iter()
        .filter(|s| s.control == ControlMode::RaiseWindow)
    {
        // Neither ours now nor bound before: nothing to decide, so skip the lookup.
        if children.is_empty() && !owned.values().any(|id| id == &s.id) {
            continue;
        }
        let Some(pid) = pid_of(&s.id) else { continue };
        let Some((terminal, depth)) = owning_terminal(pid, children, &parent_of) else {
            elsewhere.push(s.id.clone());
            continue;
        };
        let better = best.get(&terminal).is_none_or(|(d, b)| {
            depth < *d || (depth == *d && s.last_activity_at > b.last_activity_at)
        });
        if better {
            best.insert(terminal, (depth, s));
        }
    }
    for (terminal, (_, s)) in best {
        // A resumed session leaves the exited terminal it ran in before.
        owned.retain(|t, id| id != &s.id || t == &terminal);
        owned.insert(terminal, s.id.clone());
    }
    owned.retain(|_, s| !elsewhere.contains(s));
    for s in sessions.iter_mut() {
        if owned.values().any(|id| id == &s.id) {
            s.control = ControlMode::Full;
        }
    }
    elsewhere
}

/// A finished session a terminal ran earlier (before a `/clear`) opens that
/// terminal too, rather than claiming it was started outside the app.
fn reopen_earlier_sessions(sessions: &mut [Session], ran_in: &BTreeMap<String, String>) {
    for s in sessions
        .iter_mut()
        .filter(|s| s.control == ControlMode::ReadOnly && ran_in.contains_key(&s.id))
    {
        s.control = ControlMode::Full;
    }
}

/// The last `n` non-blank lines a terminal printed, as plain text.
fn last_lines(terminals: &Terminals, terminal: &str, n: usize) -> Result<Vec<String>, String> {
    let output = pty::plain_text(&terminals.backlog(terminal)?.data);
    let said: Vec<String> = output
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect();
    Ok(said[said.len().saturating_sub(n)..].to_vec())
}

fn launch_failure(
    terminals: &Terminals,
    terminal: &str,
    code: Option<u32>,
) -> Result<String, String> {
    let tail = last_lines(terminals, terminal, 4)?.join(" ");
    let code = code.map_or_else(String::new, |c| format!(" (exit code {c})"));
    Ok(if tail.is_empty() {
        format!("the agent stopped as soon as it started{code}")
    } else {
        format!("the agent stopped as soon as it started{code}: {tail}")
    })
}

/// Start `command` in a new terminal in `root`. A launch that fails (the tool
/// not on the PATH, a broken shell profile, a rejected option) exits before
/// any desk could show its terminal, so that becomes the error here. The agent
/// has to outlast [`LAUNCH_GRACE`] from the moment the login shell hands over
/// to it, however long the shell's profile takes (up to [`LAUNCH_LIMIT`]).
/// `env` is set in the terminal's environment, never on its command line.
fn start_agent_with(
    terminals: &Terminals,
    root: &Path,
    command: &str,
    env: &[(&str, &str)],
) -> Result<String, String> {
    let terminal = terminals.spawn_shell_with(root, command, env)?;
    let pid = terminals.pid(&terminal)?;
    let started = Instant::now();
    let mut handed_over: Option<Instant> = None;
    loop {
        if let Some(code) = terminals.wait_exit(&terminal, LAUNCH_POLL)? {
            return Err(launch_failure(terminals, &terminal, code)?);
        }
        let now = Instant::now();
        match handed_over {
            Some(at) if now.duration_since(at) >= LAUNCH_GRACE => return Ok(terminal),
            None if focus::process_name(pid)
                .is_some_and(|name| !SHELL_NAMES.contains(&name.as_str())) =>
            {
                handed_over = Some(now);
            }
            None if now.duration_since(started) >= LAUNCH_LIMIT => return Ok(terminal),
            Some(_) | None => {}
        }
    }
}

#[cfg(test)]
fn start_agent(terminals: &Terminals, root: &Path, command: &str) -> Result<String, String> {
    start_agent_with(terminals, root, command, &[])
}

fn attach_command(job: &str, shell: &str) -> String {
    format!("claude attach {}", pty::quote(shell, job))
}

/// The tool's own resume command for an ended session, on the model and effort it ran with.
/// `setup`'s environment goes before the binary, its options after it.
fn resume_command(ended: &EndedSession, shell: &str, setup: &Launch) -> Result<String, String> {
    let quote = |s: &str| pty::quote(shell, s);
    let (env, opts) = (setup.prefix(&quote), setup.args(&quote));
    let claude = match ended.tool {
        ToolKind::ClaudeCode => true,
        ToolKind::Codex => false,
        _ => return Err("only Claude Code and Codex sessions can be resumed so far".to_string()),
    };
    let mut words = if claude {
        vec![format!("{env}claude{opts}"), "--resume".to_string()]
    } else {
        vec![
            format!("{env}codex{opts}"),
            launch::CODEX_NO_UPDATE_CHECK.to_string(),
            "resume".to_string(),
        ]
    };
    words.push(quote(&ended.id));
    if let Some(model) = &ended.model {
        words.push(if claude { "--model" } else { "-m" }.to_string());
        words.push(quote(model));
    }
    if let Some(effort) = &ended.effort {
        if claude {
            words.extend(["--effort".to_string(), quote(effort)]);
        } else {
            words.extend([
                "-c".to_string(),
                quote(&format!("model_reasoning_effort=\"{effort}\"")),
            ]);
        }
    }
    Ok(words.join(" "))
}

/// What A Human chose for `question`: an option's label or their own words.
fn answer_text(question: &Question, answer: &QuestionAnswer) -> Result<String, String> {
    match answer {
        QuestionAnswer::Option { option_id } => question
            .options
            .iter()
            .find(|o| &o.id == option_id)
            .map(|o| o.label.clone())
            .ok_or_else(|| format!("unknown option {option_id}")),
        QuestionAnswer::Other { text } if !text.trim().is_empty() => Ok(text.trim().to_string()),
        QuestionAnswer::Other { .. } => Err("empty free-text answer".to_string()),
    }
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

impl Office {
    pub fn new(
        home: PathBuf,
        store: Store,
        terminals: Terminals,
        setup: Library,
        feed: Feed,
        structured: Arc<Desks>,
    ) -> Self {
        let discovery = Discovery::new(home.clone());
        Self {
            setup_project_id: discovery::project_id(&discovery::project_root(setup.dir())),
            setup,
            structured,
            home,
            inner: Mutex::new(Inner {
                discovery,
                feed,
                found: Discovered::default(),
                moved_from: BTreeMap::new(),
                files: BTreeMap::new(),
                boards: Vec::new(),
                store,
                owned: BTreeMap::new(),
                ran_in: BTreeMap::new(),
                attached: BTreeMap::new(),
                hired: BTreeMap::new(),
                desks: BTreeMap::new(),
                shells: BTreeMap::new(),
                reviewers: BTreeMap::new(),
                mail: deliver::Mail::default(),
                seats: BTreeMap::new(),
                chat: chat::State::default(),
                handover: handover::State::default(),
            }),
            terminals,
            connectors: Arc::new(Hub::new(Vec::new(), Box::new(|| {}))),
            settings: None,
            qa: None,
            manager: Err("the manager's socket is not open".to_string()),
            hiring: Mutex::new(()),
            chats: Mutex::default(),
            chat_tickets: AtomicU64::new(0),
            routing: None,
            handover_host: None,
            limits: Mutex::default(),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        // A panic while holding the lock leaves plain data behind; keep serving it.
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Re-scan the tools' files. Returns whether anything visible changed.
    pub fn refresh(&self) -> bool {
        let mut guard = self.lock();
        let inner = &mut *guard;
        let mut found = inner.discovery.scan(SystemTime::now());
        let pinned = inner.store.records.pinned_floors.clone();
        // An empty pinned floor can be a move's target, and a sunk floor's drop must see where sessions now sit.
        add_pinned_floors(&mut found, &pinned, &self.home);
        inner.moved_from = apply_moves(&mut found, &inner.store.records.moved_sessions);
        let desk_ids: Vec<String> = inner.desks.keys().map(|t| desk_session_id(t)).collect();
        let on_books = |id: &str| {
            desk_ids.iter().any(|d| d == id)
                || found.sessions.iter().any(|s| s.id == id)
                || found.ended.iter().any(|e| e.id == id)
        };
        // A failed save only leaves the change to be made again by the next scan.
        if prune_moves(&mut inner.store.records, on_books, chrono::Utc::now()) {
            let _ = inner.store.save();
        }
        drop_demolished(&mut found, &inner.store.records.demolished_floors);
        if let Some(data) = inner.store.path().parent() {
            reviewers::hide_ended_reviewers(&mut found, &qa::worktrees_dir(data));
        }
        let running = self.terminals.running_children();
        inner
            .desks
            .retain(|terminal, _| running.values().any(|id| id == terminal));
        let floors: Vec<String> = pinned
            .into_iter()
            .chain(
                inner
                    .desks
                    .values()
                    .map(|d| d.root.to_string_lossy().into_owned()),
            )
            .chain(
                self.structured
                    .roots()
                    .into_iter()
                    .map(|r| r.to_string_lossy().into_owned()),
            )
            .collect();
        add_pinned_floors(&mut found, &floors, &self.home);
        for (terminal, desk) in &inner.desks {
            let session = desk_session(
                terminal,
                desk,
                self.terminals.idle_for(terminal).unwrap_or(DESK_ACTIVE),
            );
            if let Some(p) = found.projects.iter_mut().find(|p| p.id == desk.project_id) {
                p.session_ids.push(session.id.clone());
            }
            found
                .checkouts
                .insert(session.id.clone(), desk.root.clone());
            found.sessions.push(session);
        }
        // Desks exist only now; the moves of discovered sessions are already applied.
        inner
            .moved_from
            .extend(apply_moves(&mut found, &inner.store.records.moved_sessions));
        let discovery = &inner.discovery;
        let elsewhere = claim_owned(
            &mut found.sessions,
            &mut inner.owned,
            &running,
            |id| discovery.pid_of(id),
            focus::parent_pid,
        );
        for (terminal, session) in &inner.owned {
            inner.ran_in.insert(session.clone(), terminal.clone());
        }
        inner
            .ran_in
            .retain(|session, _| !elsewhere.contains(session));
        inner
            .hired
            .retain(|terminal, _| !inner.owned.contains_key(terminal));
        reopen_earlier_sessions(&mut found.sessions, &inner.ran_in);
        inner.feed.collect(SystemTime::now());
        inner.feed.apply(&mut found.sessions);
        let shared = self.settings.as_ref().map(|s| s.get().boards.shared_files);
        let boards: Vec<SharedBoard> = shared
            .iter()
            .flatten()
            .map(|path| project_files::read_board(path))
            .collect();
        let canonical: Vec<PathBuf> = boards
            .iter()
            .filter_map(|b| Path::new(&b.path).canonicalize().ok())
            .collect();
        let files: BTreeMap<String, ProjectFiles> = found
            .roots
            .iter()
            .map(|(id, root)| {
                let mut files = project_files::read(root, id);
                project_files::leave_to_boards(&mut files, root, &canonical);
                (id.clone(), files)
            })
            .collect();
        let changed = found != inner.found || files != inner.files || boards != inner.boards;
        inner.found = found;
        inner.files = files;
        inner.boards = boards;
        changed
    }

    pub fn snapshot(&self) -> OfficeSnapshot {
        let inner = self.lock();
        let r = &inner.store.records;
        let mut questions = inner.found.questions.clone();
        questions.extend(inner.files.values().flat_map(|f| f.open.iter().cloned()));
        questions.extend(r.calls.iter().cloned());
        let mut decisions = r.decisions.clone();
        decisions.extend(
            inner
                .files
                .values()
                .flat_map(|f| f.decided.iter().flatten().cloned()),
        );
        let todos = self.all_todos(&inner);
        let (projects, sessions, ended) = with_structured(&inner.found, &self.structured);
        OfficeSnapshot {
            projects,
            sessions,
            ended,
            questions,
            decisions,
            todos,
            messages: crate::store::recent_messages(&r.messages, &inner.found.peer_messages),
            setup_project_id: self.setup_project_id.clone(),
            shells: inner.shells.values().cloned().collect(),
            connections: self.connections(&inner),
            qa: self.qa_floors(&inner),
            managers: self.manager_floors(&inner),
            chat_desks: Self::chat_desks(&inner),
            queued: self.queued_messages(&inner),
            handover: inner.handover.view.clone(),
            shared_boards: inner.boards.clone(),
        }
    }

    /// `spawn_session`, with the terminal it started (None for a structured desk).
    pub(super) fn spawn_in_terminal(
        &self,
        project_id: &str,
        req: &HireRequest,
    ) -> Result<(Session, Option<String>), String> {
        // The absolute root, not the `~/…` display path.
        let root = self
            .lock()
            .found
            .roots
            .get(project_id)
            .cloned()
            .ok_or_else(|| format!("unknown project: {project_id}"))?;
        let tool = hireable(self.hire_options(), req.tool)?;
        let (model, _) = chosen_model(&tool, Some(&req.model), Some(&req.effort))?;
        let task = req.title.trim();
        if req.view == DeskView::Chat && structured::supports(req.tool) {
            return self
                .spawn_structured(project_id, &root, req, model, task)
                .map(|session| (session, None));
        }
        if req.tool == ToolKind::Codex && task.is_empty() {
            // Codex writes nothing discovery can see until its first turn.
            return Err(
                "Codex only shows up in the building once it starts work, so give it a task"
                    .to_string(),
            );
        }
        if req.tool == ToolKind::ClaudeCode && CLAUDE_SUBCOMMANDS.contains(&task) {
            return Err(format!(
                "“{task}” is also a claude command, so describe the task in a few more words"
            ));
        }
        let shell = pty::login_shell();
        let setup = self
            .setup
            .launch(req.tool, &self.home, Some(&self.lock().feed))?;
        let command = launch_command(req, model, task, &shell, &setup)?;
        let (command, routed) = self.routed(project_id, req.tool, false, command, &shell)?;
        let title = if task.is_empty() { "New task" } else { task };
        let launched = if is_discovered(req.tool) {
            Launched::Discovered {
                title: title.to_string(),
            }
        } else {
            Launched::Desk(Desk {
                tool: req.tool,
                project_id: project_id.to_string(),
                root: root.clone(),
                title: title.to_string(),
                model: model.id.clone(),
                effort: req.effort.clone(),
            })
        };
        let terminal = self.launch_routed(&root, &command, launched, routed)?;
        // A discovered session's real id arrives with discovery; this is only the receipt.
        let session = Session {
            id: if is_discovered(req.tool) {
                format!("hired-{}", chrono::Utc::now().timestamp_millis())
            } else {
                desk_session_id(&terminal)
            },
            tool: req.tool,
            project_id: project_id.to_string(),
            title: title.to_string(),
            state: SessionState::Thinking,
            control: ControlMode::Full,
            model: Some(model.id.clone()),
            effort: Some(req.effort.clone()),
            last_activity_at: now_iso(),
            pending_question_ids: Vec::new(),
            spend: Spend::default(),
            helpers: Vec::new(),
            pets: Vec::new(),
            worktree: false,
            branch: None,
            last_reply: None,
        };
        Ok((session, Some(terminal)))
    }

    /// Resume an ended session in an in-app terminal, in the folder it ran in.
    pub fn resume_session(&self, session_id: &str) -> Result<(), String> {
        if let Some(resumed) = self.resume_structured(session_id) {
            return resumed;
        }
        let (ended, dir) = {
            let inner = self.lock();
            let ended = inner.found.ended.iter().find(|e| e.id == session_id);
            match (ended, inner.found.resume_dirs.get(session_id)) {
                (Some(e), Some(d)) => (e.clone(), d.clone()),
                _ => {
                    return Err(format!(
                        "{session_id} has not signed out, so there is nothing to resume"
                    ));
                }
            }
        };
        let shell = pty::login_shell();
        let setup = self
            .setup
            .launch(ended.tool, &self.home, Some(&self.lock().feed))?;
        let command = resume_command(&ended, &shell, &setup)?;
        let (command, routed) =
            self.routed(&ended.project_id, ended.tool, true, command, &shell)?;
        let tool = ended.tool;
        self.launch_routed(
            &dir,
            &command,
            Launched::Discovered { title: ended.title },
            routed,
        )
        .map(|terminal| self.note_view(tool, &terminal))
    }

    /// Open a background session in a docked terminal running `claude attach <job>`.
    /// Ok(false) when it is not a background session, so its own window is raised instead.
    pub fn attach_session(&self, session_id: &str) -> Result<bool, String> {
        let (dir, pid) = {
            let inner = self.lock();
            let session = inner
                .found
                .sessions
                .iter()
                .find(|s| s.id == session_id)
                .ok_or_else(|| format!("unknown session: {session_id}"))?;
            let dir = inner.found.roots.get(&session.project_id).unwrap_or(&self.home);
            (dir.clone(), inner.discovery.pid_of(session_id))
        };
        let Some(job) = pid.and_then(|p| focus::background_job_of(&self.home, p)) else {
            return Ok(false);
        };
        let command = attach_command(&job, &pty::login_shell());
        self.attach_in_terminal(session_id, &dir, &command)?;
        Ok(true)
    }

    /// Run `command` in a docked terminal for `session_id`, unless one is still running.
    /// Not a hire: its exit is not a failure to report, and it is no desk of its own.
    fn attach_in_terminal(&self, session_id: &str, dir: &Path, command: &str) -> Result<(), String> {
        let live = |t: &String| self.terminals.wait_exit(t, Duration::ZERO) == Ok(None);
        if self.lock().attached.get(session_id).is_some_and(live) {
            return Ok(());
        }
        let terminal = start_agent_with(&self.terminals, dir, command, &[])?;
        let mut inner = self.lock();
        // A second click may have attached while this one was starting: keep the first.
        if inner.attached.get(session_id).is_some_and(live) {
            drop(inner);
            self.hang_up(&terminal);
            self.terminals.release(&terminal);
            return Ok(());
        }
        if let Some(old) = inner.attached.insert(session_id.to_string(), terminal) {
            self.terminals.release(&old);
        }
        Ok(())
    }

    /// Hang up and forget the attach terminals of the sessions `ids`, as they go.
    fn hang_up_attached(&self, ids: &[String]) {
        let gone: Vec<String> = {
            let mut inner = self.lock();
            ids.iter()
                .filter_map(|id| inner.attached.remove(id))
                .collect()
        };
        for terminal in gone {
            self.hang_up(&terminal);
            self.terminals.release(&terminal);
        }
    }

    /// The process behind a session: discovery's, or the terminal we ran it in.
    fn agent_of(&self, inner: &Inner, s: &Session) -> Result<(String, i32), String> {
        let pid = inner.discovery.pid_of(&s.id).or_else(|| {
            inner
                .ran_in
                .get(&s.id)
                .and_then(|t| self.terminals.pid(t).ok())
        });
        let pid = pid
            .ok_or_else(|| format!("{} cannot be stopped: its process is unknown", s.title))?;
        Ok((s.title.clone(), pid))
    }

    /// Run `command` in a new in-app terminal in `dir`. An agent that stops at
    /// once is an error; one that stops later, before reaching a desk, is
    /// reported by `hire_failed`.
    fn launch(&self, dir: &Path, command: &str, launched: Launched) -> Result<String, String> {
        self.launch_with(dir, command, launched, &[])
    }

    /// `launch`, with `env` in the terminal's environment and never on its command line.
    fn launch_with(
        &self,
        dir: &Path,
        command: &str,
        launched: Launched,
        env: &[(&str, &str)],
    ) -> Result<String, String> {
        let terminal = start_agent_with(&self.terminals, dir, command, env)?;
        // Under the lock `hire_failed` takes: an exit either lands here or finds the entry.
        let mut inner = self.lock();
        if let Some(code) = self.terminals.wait_exit(&terminal, Duration::ZERO)? {
            return Err(launch_failure(&self.terminals, &terminal, code)?);
        }
        match launched {
            Launched::Discovered { title } => {
                inner.hired.insert(terminal.clone(), title);
            }
            Launched::Desk(desk) => {
                let session = desk_session_id(&terminal);
                inner.owned.insert(terminal.clone(), session.clone());
                inner.ran_in.insert(session, terminal.clone());
                inner.desks.insert(terminal.clone(), desk);
            }
        }
        Ok(terminal)
    }

    /// Pull a floor down: stop its running agents cleanly, then hide it until a
    /// new session starts there. An agent that does not stop keeps it standing.
    pub fn demolish_floor(&self, project_id: &str) -> Result<(), String> {
        let (root, agents) = {
            let inner = self.lock();
            let root = inner
                .found
                .roots
                .get(project_id)
                .cloned()
                .ok_or_else(|| format!("{project_id} is not a floor of this building"))?;
            let mut agents = Vec::new();
            for s in inner
                .found
                .sessions
                .iter()
                .filter(|s| s.project_id == project_id)
            {
                agents.push(self.agent_of(&inner, s)?);
            }
            (root, agents)
        };
        stop_agents(&agents, STOP_WAIT).map_err(|e| format!("{e}, so the floor is still standing"))?;
        self.hang_up_shells(project_id);
        let on_floor: Vec<String> = {
            let inner = self.lock();
            inner
                .found
                .sessions
                .iter()
                .filter(|s| s.project_id == project_id)
                .map(|s| s.id.clone())
                .collect()
        };
        self.hang_up_attached(&on_floor);
        {
            let mut guard = self.lock();
            let inner = &mut *guard;
            let key = root.to_string_lossy().into_owned();
            let found = &inner.found;
            inner.desks.retain(|terminal, d| {
                let floor = found
                    .sessions
                    .iter()
                    .find(|s| s.id == desk_session_id(terminal))
                    .map_or(&d.project_id, |s| &s.project_id);
                floor != project_id
            });
            inner.store.records.pinned_floors.retain(|k| *k != key);
            let at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
            inner.store.records.demolished_floors.insert(key, at);
            inner.store.save()?;
        }
        self.refresh();
        Ok(())
    }

    /// Stop one session, as a floor's agents stop when it goes: its process, then its attach terminal.
    pub fn exile_session(&self, session_id: &str) -> Result<(), String> {
        // An in-app desk is the app's own driver, not a process discovery found.
        if self.structured.sessions().iter().any(|s| s.id == session_id) {
            self.structured.stop(session_id)?;
            self.refresh();
            return Ok(());
        }
        let agent = {
            let inner = self.lock();
            let s = inner
                .found
                .sessions
                .iter()
                .find(|s| s.id == session_id)
                .ok_or_else(|| format!("unknown session: {session_id}"))?;
            self.agent_of(&inner, s)?
        };
        stop_agents(&[agent], STOP_WAIT)?;
        self.hang_up_attached(&[session_id.to_string()]);
        self.lock()
            .desks
            .retain(|terminal, _| desk_session_id(terminal) != session_id);
        self.refresh();
        Ok(())
    }

    /// Put a session on another floor, or back on the one it was found on (`None`, or that floor).
    /// The move outlives the session only as long as the session stays on the building's books.
    pub fn move_session(&self, session_id: &str, project_id: Option<&str>) -> Result<(), String> {
        {
            let mut inner = self.lock();
            let sits = |p: &str| inner.found.projects.iter().any(|x| x.id == p);
            let now = inner
                .found
                .sessions
                .iter()
                .find(|s| s.id == session_id)
                .map(|s| &s.project_id)
                .or_else(|| {
                    inner
                        .found
                        .ended
                        .iter()
                        .find(|e| e.id == session_id)
                        .map(|e| &e.project_id)
                })
                .ok_or_else(|| format!("{session_id} is not a session of this building"))?;
            let home = inner.moved_from.get(session_id).unwrap_or(now);
            let target = match project_id {
                Some(to) if to != home => {
                    if to == self.setup_project_id {
                        return Err("the Setup floor holds the library, not agents".to_string());
                    }
                    if !sits(to) {
                        return Err(format!("{to} is not a floor of this building"));
                    }
                    Some(to.to_string())
                }
                _ => None,
            };
            let home_sunk = !sits(home);
            let moves = &mut inner.store.records.moved_sessions;
            if target.is_none() && home_sunk && moves.contains_key(session_id) {
                return Err(
                    "the island it came from was sunk, so the agent has nowhere to go back to"
                        .to_string(),
                );
            }
            match target {
                Some(to) => {
                    moves.insert(session_id.to_string(), to);
                    inner
                        .store
                        .records
                        .move_seen
                        .insert(session_id.to_string(), None);
                }
                None => {
                    moves.remove(session_id);
                }
            }
            inner.store.save()?;
        }
        self.refresh();
        Ok(())
    }

    /// Add a floor for a repo folder the human picked. Idempotent: a folder that
    /// already has a floor returns that floor.
    pub fn add_floor(&self, path: &str) -> Result<Project, String> {
        let expanded = match path.trim().strip_prefix('~') {
            Some(rest) => self.home.join(rest.trim_start_matches('/')),
            None => PathBuf::from(path.trim()),
        };
        if !expanded.is_absolute() {
            return Err("give the full path to the folder, for example ~/code/app".to_string());
        }
        if !expanded.is_dir() {
            return Err(format!(
                "{} is not a folder on this Mac",
                expanded.display()
            ));
        }
        let root = discovery::project_root(&expanded);
        let key = root.to_string_lossy().into_owned();
        {
            let mut inner = self.lock();
            let rebuilt = inner.store.records.demolished_floors.remove(&key).is_some();
            if !inner.store.records.pinned_floors.contains(&key) {
                inner.store.records.pinned_floors.push(key);
                inner.store.save()?;
            } else if rebuilt {
                inner.store.save()?;
            }
        }
        self.refresh();
        let id = discovery::project_id(&root);
        self.lock()
            .found
            .projects
            .iter()
            .find(|p| p.id == id)
            .cloned()
            .ok_or_else(|| "the floor could not be added".to_string())
    }

    /// Ask macOS for a folder with its own picker. None when the human cancels.
    pub fn pick_folder(&self) -> Result<Option<String>, String> {
        focus::choose_folder("Choose the repo folder for the new floor")
    }

    pub fn answer_question(
        &self,
        question_id: &str,
        answer: &QuestionAnswer,
    ) -> Result<(), String> {
        if let Some(settled) = self.answer_store_call(question_id, answer) {
            return settled;
        }
        {
            let inner = self.lock();
            let question = inner
                .found
                .questions
                .iter()
                .chain(inner.files.values().flat_map(|f| f.open.iter()))
                .find(|q| q.id == question_id)
                .ok_or_else(|| format!("unknown question: {question_id}"))?;
            match question.answer_via {
                AnswerVia::Terminal => {
                    return Err(
                        "this agent was started outside the app; answer it in its own terminal"
                            .to_string(),
                    );
                }
                // App-owned terminals (PTY) arrive with the in-app terminal work.
                AnswerVia::App => {
                    return Err("answering in the app is not wired to a terminal yet".to_string());
                }
                AnswerVia::Store => return Err("that call is kept by the building".to_string()),
                AnswerVia::File => {
                    let text = answer_text(question, answer)?;
                    let (file, line) =
                        project_files::parse_item_id(question_id).ok_or("malformed question id")?;
                    let root = inner
                        .found
                        .roots
                        .get(&question.project_id)
                        .ok_or("that floor is no longer in the building")?;
                    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
                    project_files::decide(root, file, line, &text, &today)?;
                }
            }
        }
        self.refresh();
        Ok(())
    }

    /// Record a note between two agents (delivery into the recipient arrives with the MCP work).
    pub fn send_agent_message(&self, from: &str, to: &str, text: &str) -> Result<(), String> {
        let mut inner = self.lock();
        for id in [from, to] {
            if !inner.found.sessions.iter().any(|s| s.id == id) {
                return Err(format!("unknown session: {id}"));
            }
        }
        if from == to {
            return Err("an agent cannot message itself".to_string());
        }
        if text.trim().is_empty() {
            return Err("empty agent message".to_string());
        }
        let id = inner.store.next_id("m");
        inner.store.push_message(AgentMessage {
            id,
            from_session_id: from.to_string(),
            to_session_id: to.to_string(),
            text: text.trim().to_string(),
            sent_at: now_iso(),
        });
        inner.store.save()
    }

    /// A hired agent's terminal has exited: if no session was ever bound to it,
    /// the hire failed after it was reported as started, so say so.
    pub fn hire_failed(&self, exit: &TerminalExit) -> Option<HireFailed> {
        let title = self.lock().hired.remove(&exit.terminal_id)?;
        let last_lines = last_lines(&self.terminals, &exit.terminal_id, 4).unwrap_or_default();
        Some(HireFailed {
            title,
            exit_code: exit.exit_code,
            last_lines,
        })
    }

    /// The app-owned terminal a session runs in.
    fn terminal_of(&self, session_id: &str) -> Result<String, String> {
        let inner = self.lock();
        if inner.shells.contains_key(session_id) {
            return Ok(session_id.to_string());
        }
        inner
            .owned
            .iter()
            .find(|(_, s)| s.as_str() == session_id)
            .map(|(t, _)| t)
            .or_else(|| inner.ran_in.get(session_id))
            .or_else(|| inner.attached.get(session_id))
            .cloned()
            .ok_or_else(|| {
                format!("{session_id} was not started in the app, so it has no terminal here")
            })
    }

    pub fn terminal_backlog(&self, session_id: &str) -> Result<TerminalBacklog, String> {
        self.terminals.backlog(&self.terminal_of(session_id)?)
    }

    pub fn terminal_write(&self, session_id: &str, data: &str) -> Result<(), String> {
        self.human_typed(session_id, data);
        self.terminals.write(&self.terminal_of(session_id)?, data)
    }

    pub fn terminal_resize(&self, session_id: &str, cols: u16, rows: u16) -> Result<(), String> {
        self.terminals
            .resize(&self.terminal_of(session_id)?, cols, rows)
    }

    pub fn focus_session(&self, session_id: &str) -> Result<(), String> {
        let pid = {
            let inner = self.lock();
            let session = inner
                .found
                .sessions
                .iter()
                .find(|s| s.id == session_id)
                .ok_or_else(|| format!("unknown session: {session_id}"))?;
            if session.control != ControlMode::RaiseWindow {
                return Err(format!(
                    "{} is not running in a terminal window",
                    session.title
                ));
            }
            inner
                .discovery
                .pid_of(session_id)
                .ok_or("its process has exited")?
        };
        focus::focus_pid(pid)
    }
}

/// Ask each agent to exit (SIGTERM) and wait up to `within` for all of them.
fn stop_agents(agents: &[(String, i32)], within: Duration) -> Result<(), String> {
    for (_, pid) in agents {
        discovery::claude_code::terminate(*pid);
    }
    let deadline = Instant::now() + within;
    loop {
        let left: Vec<&str> = agents
            .iter()
            .filter(|(_, pid)| discovery::claude_code::pid_alive(*pid))
            .map(|(title, _)| title.as_str())
            .collect();
        if left.is_empty() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "{} did not stop",
                left.join(", ")
            ));
        }
        std::thread::sleep(LAUNCH_POLL);
    }
}

mod chat;
mod connectors;
mod deliver;
mod desk_states;
mod floor_list;
mod folders;
#[cfg(test)]
mod guard;
mod handover;
pub use handover::{HandoverHost, start_due_handovers};
mod launch;
mod manager;
mod manager_calls;
mod manager_hire;
mod manager_send;
mod manager_tools;
pub use manager::ManagerHost;
mod qa_desk;
mod reviewers;
mod routed;
mod setup;
#[cfg(test)]
mod shared_tests;
mod shells;
mod staff;
mod structured_desks;
mod todos;
use floor_list::{add_pinned_floors, drop_demolished};
use structured_desks::with_structured;
#[cfg(test)]
mod tests;

#[cfg(test)]
mod attach_tests {
    use super::tests::office;
    use super::*;

    #[test]
    fn a_background_job_is_attached_to_in_the_docked_terminal_and_reused() {
        let (office, dir) = office("attach");
        assert_eq!(attach_command("9c4967e5", "/bin/zsh"), "claude attach '9c4967e5'");
        assert!(office.terminal_of("s-bg").is_err());
        office.attach_in_terminal("s-bg", &dir, "sleep 20").expect("attach");
        let first = office.terminal_of("s-bg").expect("the session reaches the attach terminal");
        office.attach_in_terminal("s-bg", &dir, "sleep 20").expect("attach again");
        assert_eq!(office.terminal_of("s-bg"), Ok(first));
        assert!(office.lock().hired.is_empty(), "an attach is not a hire that can fail");
        std::fs::remove_dir_all(&dir).expect("cleanup");
    }

    #[test]
    fn a_session_without_a_background_job_is_left_to_its_window() {
        let (office, dir) = office("attach-none");
        assert_eq!(office.attach_session("ghost"), Err("unknown session: ghost".to_string()));
        std::fs::remove_dir_all(&dir).expect("cleanup");
    }
}

#[cfg(test)]
mod exile_tests {
    use super::tests::floor_office;
    use super::*;

    fn desk_on(office: &Office, dir: &Path, floor: &Project, title: &str) -> String {
        let desk = Desk {
            tool: ToolKind::Antigravity,
            project_id: floor.id.clone(),
            root: dir.join("home/code/app"),
            title: title.to_string(),
            model: "m".to_string(),
            effort: "high".to_string(),
        };
        office
            .launch(&desk.root.clone(), "/bin/cat", Launched::Desk(desk))
            .expect("launch")
    }

    #[test]
    fn exiling_an_unknown_session_is_an_error() {
        let (office, dir, _) = floor_office("exile-unknown");
        assert_eq!(
            office.exile_session("ghost"),
            Err("unknown session: ghost".to_string())
        );
        std::fs::remove_dir_all(&dir).expect("cleanup");
    }

    #[test]
    fn exiling_a_session_stops_only_it_and_hangs_up_only_its_attach_terminal() {
        let (office, dir, floor) = floor_office("exile-one");
        let out = desk_on(&office, &dir, &floor, "leaves");
        let stays = desk_on(&office, &dir, &floor, "stays");
        let (out_id, stays_id) = (desk_session_id(&out), desk_session_id(&stays));
        office.refresh();
        office.attach_in_terminal(&out_id, &dir, "sleep 30").expect("attach");
        office.attach_in_terminal(&stays_id, &dir, "sleep 30").expect("attach");
        let kept = office.lock().attached.get(&stays_id).cloned().expect("attached");
        let kept_pid = office.terminals.pid(&kept).expect("pid");
        office.exile_session(&out_id).expect("exile");
        assert!(office.terminals.wait_exit(&out, Duration::from_secs(10)).expect("wait").is_some());
        assert_eq!(office.terminals.wait_exit(&stays, Duration::ZERO), Ok(None));
        assert!(!office.lock().attached.contains_key(&out_id));
        assert!(office.lock().attached.contains_key(&stays_id));
        assert!(crate::discovery::claude_code::pid_alive(kept_pid));
        let left: Vec<String> = office.snapshot().sessions.into_iter().map(|s| s.id).collect();
        assert_eq!(left, vec![stays_id]);
        office.demolish_floor(&floor.id).expect("cleanup floor");
        std::fs::remove_dir_all(&dir).expect("cleanup");
    }

    #[test]
    fn exiling_an_in_app_desk_stops_its_driver() {
        struct Spy(Arc<std::sync::atomic::AtomicBool>);
        impl structured::Driver for Spy {
            fn prompt(&self, _: &str) -> Result<(), String> { Ok(()) }
            fn answer(&self, _: &str, _: &str, _: &structured::Values) -> Result<(), String> { Ok(()) }
            fn cancel(&self) -> Result<(), String> { Ok(()) }
            fn stop(&self) { self.0.store(true, std::sync::atomic::Ordering::SeqCst); }
        }
        let (office, dir, floor) = floor_office("exile-structured");
        let stopped = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let info = structured::desks::DeskInfo {
            tool: ToolKind::Codex,
            project_id: floor.id.clone(),
            root: dir.join("home/code/app"),
            title: "in app".to_string(),
            model: "default".to_string(),
            effort: "default".to_string(),
        };
        office.structured.seat("structured-1", info, SessionState::Thinking, Box::new(Spy(stopped.clone())));
        office.refresh();
        office.exile_session("structured-1").expect("exile");
        assert!(stopped.load(std::sync::atomic::Ordering::SeqCst));
        std::fs::remove_dir_all(&dir).expect("cleanup");
    }

    #[test]
    fn a_session_that_will_not_stop_is_named_without_talk_of_a_floor() {
        let mut child = std::process::Command::new("sh")
            .args(["-c", "trap '' TERM; while :; do sleep 1; done"])
            .spawn()
            .expect("spawn");
        std::thread::sleep(Duration::from_millis(200));
        let pid = i32::try_from(child.id()).expect("pid");
        let err = stop_agents(&[("stubborn".to_string(), pid)], Duration::from_millis(300));
        child.kill().expect("kill");
        child.wait().expect("reap");
        assert_eq!(err, Err("stubborn did not stop".to_string()));
    }
}
