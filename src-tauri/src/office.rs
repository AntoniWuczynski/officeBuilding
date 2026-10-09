//! The office: live discovery plus the app's own records, behind one lock.
//! Every Tauri command goes through here, so there is one source of truth.

use crate::discovery::{self, Discovered, Discovery};
use crate::feed::Feed;
use crate::focus;
use crate::model::{
    AgentMessage, AnswerVia, ControlMode, EndedSession, HireFailed, HireRequest, OfficeSnapshot,
    Party, Project, QuestionAnswer, Session, SessionState, Spend, TerminalBacklog, TerminalExit,
    TodoItem, ToolKind, ToolOptions,
};
use crate::options;
use crate::project_files::{self, ProjectFiles};
use crate::pty::{self, Terminals};
use crate::setup::{Launch, Library};
use crate::store::Store;
use crate::structured::{self, desks::Desks};
use launch::{
    DESK_ACTIVE, Desk, Launched, desk_session, desk_session_id, hireable, is_discovered,
    launch_command,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

struct Inner {
    discovery: Discovery,
    /// Events from the hooks of agents the app launched.
    feed: Feed,
    found: Discovered,
    /// Decisions and TODOs from each floor's own files, by project id.
    files: BTreeMap<String, ProjectFiles>,
    store: Store,
    /// The session each app-owned terminal is running, by terminal id. It
    /// outlives the agent so its last output stays readable.
    owned: BTreeMap<String, String>,
    /// Every session a terminal has run, by session id, so a panel opened on an
    /// earlier session (before a `/clear`) keeps reaching the same terminal.
    ran_in: BTreeMap<String, String>,
    /// Hired terminals no session has been bound to yet, with the task they
    /// were hired for: if one exits first, the hire failed after all.
    hired: BTreeMap<String, String>,
    /// Terminals running a tool discovery cannot see: each is its own desk while
    /// it runs, by terminal id.
    desks: BTreeMap<String, Desk>,
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
/// `claude` subcommands (from `claude --help`, 2.1.285, plus commander's own
/// `help`). A task that is exactly one of these runs the subcommand even after
/// `--`; codex takes such a task as its prompt.
const CLAUDE_SUBCOMMANDS: &[&str] = &[
    "agents",
    "attach",
    "auth",
    "auto-mode",
    "doctor",
    "gateway",
    "help",
    "import",
    "install",
    "logs",
    "mcp",
    "plugin",
    "plugins",
    "project",
    "respawn",
    "rm",
    "setup-token",
    "stop",
    "kill",
    "ultrareview",
    "update",
    "upgrade",
];

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
fn start_agent(terminals: &Terminals, root: &Path, command: &str) -> Result<String, String> {
    let terminal = terminals.spawn_shell(root, command)?;
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
                files: BTreeMap::new(),
                store,
                owned: BTreeMap::new(),
                ran_in: BTreeMap::new(),
                hired: BTreeMap::new(),
                desks: BTreeMap::new(),
            }),
            terminals,
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
        drop_demolished(&mut found, &inner.store.records.demolished_floors);
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
            found.sessions.push(session);
        }
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
        let files: BTreeMap<String, ProjectFiles> = found
            .roots
            .iter()
            .map(|(id, root)| (id.clone(), project_files::read(root, id)))
            .collect();
        let changed = found != inner.found || files != inner.files;
        inner.found = found;
        inner.files = files;
        changed
    }

    /// A floor's TODOs for one assignee: its file when it has one, else the app's store.
    fn todos_for(inner: &Inner, project_id: &str, assignee: Party) -> Vec<TodoItem> {
        let from_file = inner.files.get(project_id).and_then(|f| match assignee {
            Party::Human => f.human_todos.clone(),
            Party::Ai => f.agent_todos.clone(),
        });
        from_file.unwrap_or_else(|| {
            inner
                .store
                .records
                .todos
                .iter()
                .filter(|t| t.project_id == project_id && t.assignee == assignee)
                .cloned()
                .collect()
        })
    }

    pub fn snapshot(&self) -> OfficeSnapshot {
        let inner = self.lock();
        let r = &inner.store.records;
        let mut questions = inner.found.questions.clone();
        questions.extend(inner.files.values().flat_map(|f| f.open.iter().cloned()));
        let mut decisions = r.decisions.clone();
        decisions.extend(
            inner
                .files
                .values()
                .flat_map(|f| f.decided.iter().flatten().cloned()),
        );
        let todos = inner
            .found
            .projects
            .iter()
            .flat_map(|p| {
                let mut all = Self::todos_for(&inner, &p.id, Party::Human);
                all.extend(Self::todos_for(&inner, &p.id, Party::Ai));
                all
            })
            .collect();
        let (projects, sessions, ended) = with_structured(&inner.found, &self.structured);
        OfficeSnapshot {
            projects,
            sessions,
            ended,
            questions,
            decisions,
            todos,
            messages: r.messages.clone(),
            setup_project_id: self.setup_project_id.clone(),
        }
    }

    pub fn hire_options(&self) -> Vec<ToolOptions> {
        let mut all = options::hire_options(&self.home);
        for o in &mut all {
            o.structured = o.available && structured::supports(o.tool);
        }
        all
    }

    /// Start an agent in an in-app terminal, in the floor's folder. It appears
    /// as a desk as soon as the tool writes its session files.
    pub fn spawn_session(&self, project_id: &str, req: &HireRequest) -> Result<Session, String> {
        // The absolute root, not the `~/…` display path.
        let root = self
            .lock()
            .found
            .roots
            .get(project_id)
            .cloned()
            .ok_or_else(|| format!("unknown project: {project_id}"))?;
        let tool = hireable(self.hire_options(), req.tool)?;
        let model = tool
            .models
            .iter()
            .find(|m| m.id == req.model)
            .ok_or_else(|| format!("{} is not a model the app can start", req.model))?;
        if !model.efforts.iter().any(|e| e.id == req.effort) {
            return Err(format!(
                "{} does not support {} effort",
                model.label, req.effort
            ));
        }
        let task = req.title.trim();
        if req.structured {
            return self.spawn_structured(project_id, &root, req, model, task);
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
        let terminal = self.launch(&root, &command, launched)?;
        // A discovered session's real id arrives with discovery; this is only the receipt.
        Ok(Session {
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
        })
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
        self.launch(&dir, &command, Launched::Discovered { title: ended.title })
            .map(drop)
    }

    /// Run `command` in a new in-app terminal in `dir`. An agent that stops at
    /// once is an error; one that stops later, before reaching a desk, is
    /// reported by `hire_failed`.
    fn launch(&self, dir: &Path, command: &str, launched: Launched) -> Result<String, String> {
        let terminal = start_agent(&self.terminals, dir, command)?;
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
                let pid = inner.discovery.pid_of(&s.id).or_else(|| {
                    inner
                        .ran_in
                        .get(&s.id)
                        .and_then(|t| self.terminals.pid(t).ok())
                });
                let pid = pid.ok_or_else(|| {
                    format!("{} cannot be stopped: its process is unknown", s.title)
                })?;
                agents.push((s.title.clone(), pid));
            }
            (root, agents)
        };
        stop_agents(&agents, STOP_WAIT)?;
        {
            let mut inner = self.lock();
            let key = root.to_string_lossy().into_owned();
            inner.desks.retain(|_, d| d.project_id != project_id);
            inner.store.records.pinned_floors.retain(|k| *k != key);
            let at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
            inner.store.records.demolished_floors.insert(key, at);
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
                AnswerVia::File => {
                    let text = match answer {
                        QuestionAnswer::Option { option_id } => question
                            .options
                            .iter()
                            .find(|o| &o.id == option_id)
                            .map(|o| o.label.clone())
                            .ok_or_else(|| format!("unknown option {option_id}"))?,
                        QuestionAnswer::Other { text } if !text.trim().is_empty() => {
                            text.trim().to_string()
                        }
                        QuestionAnswer::Other { .. } => {
                            return Err("empty free-text answer".to_string());
                        }
                    };
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

    pub fn tick_todo(&self, todo_id: &str) -> Result<(), String> {
        let from_file = {
            let inner = self.lock();
            inner
                .files
                .values()
                .flat_map(|f| f.human_todos.iter().chain(f.agent_todos.iter()).flatten())
                .find(|t| t.id == todo_id)
                .map(|t| {
                    (
                        t.project_id.clone(),
                        project_files::parse_item_id(todo_id)
                            .map(|(file, line)| (file.to_string(), line)),
                    )
                })
        };
        if let Some((project_id, location)) = from_file {
            let (file, line) = location.ok_or("malformed todo id")?;
            let root = self
                .lock()
                .found
                .roots
                .get(&project_id)
                .cloned()
                .ok_or("that floor is no longer in the building")?;
            project_files::tick(&root, &file, line)?;
            self.refresh();
            return Ok(());
        }
        let mut inner = self.lock();
        let todo = inner
            .store
            .records
            .todos
            .iter_mut()
            .find(|t| t.id == todo_id)
            .ok_or_else(|| format!("unknown todo: {todo_id}"))?;
        todo.done = true;
        inner.store.save()
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
        inner
            .owned
            .iter()
            .find(|(_, s)| s.as_str() == session_id)
            .map(|(t, _)| t)
            .or_else(|| inner.ran_in.get(session_id))
            .cloned()
            .ok_or_else(|| {
                format!("{session_id} was not started in the app, so it has no terminal here")
            })
    }

    pub fn terminal_backlog(&self, session_id: &str) -> Result<TerminalBacklog, String> {
        self.terminals.backlog(&self.terminal_of(session_id)?)
    }

    pub fn terminal_write(&self, session_id: &str, data: &str) -> Result<(), String> {
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
                "{} did not stop, so the floor is still standing",
                left.join(", ")
            ));
        }
        std::thread::sleep(LAUNCH_POLL);
    }
}

/// Keep each demolished floor down until work starts there again. Its earlier
/// sessions never return to the desks, but a rebuilt floor's sign-out sheet
/// keeps everyone who left, the agents its demolition stopped included.
fn drop_demolished(found: &mut Discovered, demolished: &BTreeMap<String, String>) {
    let instant = |s: &str| chrono::DateTime::parse_from_rfc3339(s).ok();
    for (key, at) in demolished {
        let Some(at) = instant(at) else { continue };
        let id = discovery::project_id(Path::new(key));
        let since = |when: &str| instant(when).is_some_and(|t| t > at);
        let rebuilt = found
            .sessions
            .iter()
            .any(|s| s.project_id == id && since(&s.last_activity_at))
            || found
                .ended
                .iter()
                .any(|e| e.project_id == id && since(&e.ended_at));
        found
            .sessions
            .retain(|s| s.project_id != id || since(&s.last_activity_at));
        if rebuilt {
            let kept: Vec<String> = found
                .sessions
                .iter()
                .filter(|s| s.project_id == id)
                .map(|s| s.id.clone())
                .collect();
            if let Some(p) = found.projects.iter_mut().find(|p| p.id == id) {
                p.session_ids.retain(|sid| kept.contains(sid));
            }
        } else {
            found.ended.retain(|e| e.project_id != id);
            found.projects.retain(|p| p.id != id);
            found.roots.remove(&id);
        }
    }
    let sessions = &found.sessions;
    found
        .questions
        .retain(|q| q.session_id.is_empty() || sessions.iter().any(|s| s.id == q.session_id));
    let ended = &found.ended;
    found
        .resume_dirs
        .retain(|sid, _| ended.iter().any(|e| &e.id == sid));
}

/// Floors the human pinned that no agent is working on yet: added with no desks.
fn add_pinned_floors(found: &mut Discovered, pinned: &[String], home: &Path) {
    for key in pinned {
        let root = PathBuf::from(key);
        let id = discovery::project_id(&root);
        if found.roots.contains_key(&id) || !root.is_dir() {
            continue;
        }
        found.projects.push(Project {
            id: id.clone(),
            name: root
                .file_name()
                .map_or_else(|| key.clone(), |n| n.to_string_lossy().into_owned()),
            path: discovery::display_path(&root, home),
            session_ids: Vec::new(),
        });
        found.roots.insert(id, root);
    }
}

mod launch;
mod setup;
mod structured_desks;
use structured_desks::with_structured;
#[cfg(test)]
mod tests;
