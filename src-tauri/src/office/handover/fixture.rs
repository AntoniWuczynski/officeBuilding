//! The fixture of the handover's office tests: an office on a floor with a handover host of
//! fakes, `/bin/cat` desks in terminals of their own, and the records each tool writes.

use super::*;
use crate::discovery::claude_code::terminate;
pub(super) use crate::discovery::codex::rollout::fixtures::{ABORTED, CLEAN, ERROR_EVENT};
use crate::discovery::codex::rollout::fixtures::{GOAL, ID as FIXTURE_ID, LIMIT, META};
use crate::handover::view::OfferWhy;
use crate::handover::work::Signal;
use crate::handover::{Task, Window};
use crate::model::{Project, Session, SessionState, Spend, ToolKind};
use crate::office::tests::floor_office;
use crate::process::FakeRunner;
use crate::settings::{SettingsPatch, SettingsStore};
use std::io::Write;
use std::sync::Mutex;
use std::time::{Instant, SystemTime};

/// The automatic switch on, and Codex starting on its own configured default.
pub(super) const ON: &str = r#"{"automatic":true,"claudeToCodex":{"kind":"tool-default"}}"#;
/// The automatic switch off, and a choice made.
pub(super) const OFF: &str = r#"{"automatic":false,"claudeToCodex":{"kind":"tool-default"}}"#;

/// The entry Claude Code 2.1.323 wrote when a local listener replayed a used-up allowance to it
/// on 2026-11-13. A replay, not Anthropic's own reply, and the transcript format is
/// undocumented. The reset is an hour ahead of the test's clock: the fixture's own number is in
/// the past by now, and a limit whose reset has passed is not recorded.
pub(super) fn limit_line(resets_at: i64) -> String {
    format!(
        r#"{{"type":"assistant","isSidechain":false,"timestamp":"2026-11-13T10:53:07.124Z","message":{{"id":"m1","type":"message","role":"assistant","model":"<synthetic>","stop_reason":"stop_sequence","content":[{{"type":"text","text":"You've hit your 5-hour limit · resets 12:53pm (UTC)"}}]}},"apiError":"usage_limit_reached","apiErrorParams":{{"rate_limit_info":{{"status":"rejected","resetsAt":{resets_at},"rateLimitType":"five_hour","isUsingOverage":false}}}},"quotaLimits":{{"status":"rejected","resetsAt":{resets_at},"unifiedRateLimitFallbackAvailable":false,"rateLimitType":"five_hour","isUsingOverage":false}},"error":"rate_limit","isApiErrorMessage":true,"apiErrorStatus":429}}"#
    )
}

/// The entry Claude Code 2.1.323 wrote when a local listener replayed a throttle to it on
/// 2026-11-13. A replay, not Anthropic's own reply, and the transcript format is undocumented.
pub(super) const THROTTLE: &str = r#"{"type":"assistant","isSidechain":false,"timestamp":"2026-11-13T11:17:26.534Z","message":{"id":"m2","type":"message","role":"assistant","model":"<synthetic>","stop_reason":"stop_sequence","content":[{"type":"text","text":"API Error: Server is temporarily limiting requests (not your usage limit)"}]},"apiErrorIsTransient":true,"error":"rate_limit","isApiErrorMessage":true,"apiErrorStatus":429}"#;

/// Composed from the key names of entries seen on one machine, not from a replay.
pub(super) const AUTHENTICATION: &str = r#"{"type":"assistant","isSidechain":false,"timestamp":"2026-11-13T11:17:26.534Z","message":{"id":"m3","type":"message","role":"assistant","model":"<synthetic>","stop_reason":"stop_sequence","content":[{"type":"text","text":"Please run /login"}]},"error":"authentication_failed","isApiErrorMessage":true,"apiErrorStatus":401}"#;

/// Composed from the key names of entries seen on one machine, not from a replay.
pub(super) const SERVER: &str = r#"{"type":"assistant","isSidechain":false,"timestamp":"2026-11-13T11:17:26.534Z","message":{"id":"m4","type":"message","role":"assistant","model":"<synthetic>","stop_reason":"stop_sequence","content":[{"type":"text","text":"API Error: 500"}]},"error":"server_error","isApiErrorMessage":true,"apiErrorStatus":500}"#;

/// The model answering again.
pub(super) const REPLY: &str = r#"{"type":"assistant","isSidechain":false,"message":{"id":"m5","type":"message","role":"assistant","model":"claude-opus-4-1","content":[{"type":"text","text":"Back at it."}]}}"#;

pub(super) const TITLE: &str = "Fix the importer";

pub(super) struct Fixture {
    pub(super) office: Office,
    pub(super) dir: PathBuf,
    pub(super) floor: Project,
    pub(super) settings: Arc<SettingsStore>,
    pub(super) stops: Arc<Mutex<Vec<i32>>>,
}

/// An office on a floor, with `handover` as its settings group. `terminate`: the stop sends
/// SIGTERM, else it only records. `file`: another office's `handover.json`.
pub(super) fn build(
    tag: &str,
    handover: &str,
    terminate_on_stop: bool,
    file: Option<PathBuf>,
) -> Fixture {
    let (office, dir, floor) = floor_office(&format!("handover-{tag}"));
    let path = dir.join("settings.json");
    std::fs::write(&path, format!(r#"{{"handover":{handover}}}"#)).expect("settings");
    let settings = Arc::new(SettingsStore::open(path));
    let stops = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&stops);
    let host = HandoverHost {
        runner: Arc::new(FakeRunner::new(|_| {
            Err("no program runs in a test".to_string())
        })),
        file: file.unwrap_or_else(|| dir.join("handover.json")),
        briefs: dir.join("data/handover/briefs"),
        stop: Arc::new(move |pid| {
            lock(&seen).push(pid);
            if terminate_on_stop {
                terminate(pid);
            }
        }),
        stop_wait: Duration::from_millis(300),
    };
    let office = office
        .with_settings(Arc::clone(&settings))
        .with_handover(host);
    Fixture {
        office,
        dir,
        floor,
        settings,
        stops,
    }
}

pub(super) fn fixture(tag: &str, handover: &str) -> Fixture {
    build(tag, handover, true, None)
}

pub(super) fn session(id: &str, tool: ToolKind, project_id: &str) -> Session {
    Session {
        id: id.to_string(),
        tool,
        project_id: project_id.to_string(),
        title: TITLE.to_string(),
        state: SessionState::Idle,
        control: ControlMode::Full,
        model: None,
        effort: None,
        last_activity_at: String::new(),
        pending_question_ids: Vec::new(),
        spend: Spend::default(),
        helpers: Vec::new(),
        pets: Vec::new(),
        worktree: false,
        branch: None,
    }
}

pub(super) fn wait_for(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

impl Fixture {
    pub(super) fn root(&self) -> PathBuf {
        self.dir.join("home/code/app")
    }

    pub(super) fn host(&self) -> &HandoverHost {
        self.office.handover_host.as_ref().expect("host")
    }

    /// `/bin/cat` in a terminal of its own, running once the login shell has handed over.
    pub(super) fn terminal(&self) -> String {
        let terminal = self
            .office
            .terminals
            .spawn_shell(&self.root(), "/bin/cat")
            .expect("spawn");
        let pid = self.office.terminals.pid(&terminal).expect("pid");
        wait_for("cat to start", || {
            crate::focus::process_name(pid).as_deref() == Some("cat")
        });
        terminal
    }

    /// A desk of `tool` the building started: its terminal runs `/bin/cat`.
    pub(super) fn desk(&self, id: &str, tool: ToolKind) -> String {
        let terminal = self.terminal();
        self.add(id, tool, Some(&terminal));
        terminal
    }

    /// A session discovery found, in `terminal` if it is given one.
    pub(super) fn add(&self, id: &str, tool: ToolKind, terminal: Option<&str>) {
        let mut inner = self.office.lock();
        if let Some(terminal) = terminal {
            inner.owned.insert(terminal.to_string(), id.to_string());
        }
        inner.found.sessions.push(session(id, tool, &self.floor.id));
    }

    /// Append `lines` to the session's transcript.
    pub(super) fn transcript(&self, id: &str, lines: &[&str]) {
        let folder = self.dir.join("home/.claude/projects/p");
        std::fs::create_dir_all(&folder).expect("mkdir");
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(folder.join(format!("{id}.jsonl")))
            .expect("open");
        for line in lines {
            writeln!(file, "{line}").expect("write");
        }
    }

    /// A hook listed the session's background work.
    pub(super) fn listed(&self, id: &str, tasks: Vec<Task>) {
        self.office.lock().handover.works.note(Signal::Listed {
            session_id: id.to_string(),
            at: SystemTime::now(),
            tasks: Some(tasks),
        });
    }

    /// Move the desk's proof back past the settle time instead of sleeping through it.
    pub(super) fn settle(&self, id: &str) {
        self.office
            .lock()
            .handover
            .undecided
            .insert(id.to_string(), now() - SETTLE_SECS);
    }

    pub(super) fn state(&self, id: &str) -> Option<DeskState> {
        self.office
            .lock()
            .handover
            .view
            .desks
            .iter()
            .find(|d| d.session_id == id)
            .map(|d| d.state.clone())
    }

    pub(super) fn stops(&self) -> Vec<i32> {
        lock(&self.stops).clone()
    }

    pub(super) fn plan(&self, id: &str, by_human: bool) -> Plan {
        self.office
            .plan(&self.office.lock(), id, by_human)
            .expect("plan")
    }

    pub(super) fn choose(&self, handover: HandoverSettings) {
        self.settings
            .update(SettingsPatch {
                handover: Some(handover),
                ..SettingsPatch::default()
            })
            .expect("settings");
    }

    /// End every agent still running and remove the temp folder.
    pub(super) fn done(self) {
        for terminal in self.office.terminals.running_children().values() {
            let _ = self.office.terminals.write(terminal, "\u{4}");
        }
        std::fs::remove_dir_all(&self.dir).expect("cleanup");
    }
}

/// `change_hands`, with its failure as the reason and whether the old session was ended.
pub(super) fn change(fx: &Fixture, plan: &Plan, command: &str) -> Result<String, (String, bool)> {
    fx.office
        .change_hands(fx.host(), plan, command, None)
        .map_err(|f| (f.reason, f.ended))
}

pub(super) fn offered(why: OfferWhy, work: Work, reset_at: i64) -> DeskState {
    DeskState::Offered {
        to: Tool::Codex,
        why,
        work,
        window: Some(Window::FiveHour),
        reset_at: Some(reset_at),
    }
}

/// Hand `id` over by hand as far as the fakes allow: the plan, the change and its record.
pub(super) fn hand_over_with_cat(fx: &Fixture, plan: &Plan) -> String {
    fx.office
        .lock()
        .handover
        .underway
        .insert(plan.session_id.clone());
    let started = change(fx, plan, "/bin/cat").expect("started");
    let id = fx.office.lock().handover.book.next_id();
    fx.office
        .finish(plan, id, Ok(started.clone()))
        .expect("recorded");
    started
}

/// The switch on, and each tool starting on its own configured default.
pub(super) const BOTH: &str = r#"{"automatic":true,"claudeToCodex":{"kind":"tool-default"},"codexToClaude":{"kind":"tool-default"}}"#;
/// The switch off, and both directions chosen.
pub(super) const BOTH_OFF: &str = r#"{"automatic":false,"claudeToCodex":{"kind":"tool-default"},"codexToClaude":{"kind":"tool-default"}}"#;

/// A Codex desk's session id: the thread id, a uuid as in a rollout's name.
pub(super) fn thread(n: u8) -> String {
    format!("0199c0de-0000-4000-8000-0000000000{n:02x}")
}

/// The first line of the rollout of the top-level thread `id` on Codex's built-in provider.
pub(super) fn meta(id: &str) -> String {
    META.replace(FIXTURE_ID, id)
}

/// `meta` for a custom provider.
pub(super) fn custom_meta(id: &str) -> String {
    meta(id).replace(
        r#""model_provider":"openai""#,
        r#""model_provider":"local-listener""#,
    )
}

/// `meta` for a sub-agent's thread, which another thread spawned.
pub(super) fn subagent_meta(id: &str) -> String {
    meta(id).replace(
        r#""source":"cli""#,
        r#""source":{"subagent":"thread_spawn"},"parent_thread_id":"0199c0de-0000-4000-8000-0000000000ff""#,
    )
}

/// Codex's used-up record, written at `completed_at` by the test's own clock: the fixture's
/// own time is hours in the past, and a record older than its hold is not recorded.
pub(super) fn codex_limit(completed_at: i64) -> String {
    LIMIT.replace(
        r#""started_at":1794573796,"completed_at":1794573796"#,
        &format!(r#""started_at":{completed_at},"completed_at":{completed_at}"#),
    )
}

/// `codex_limit` with another `codex_error_info`.
pub(super) fn codex_error(info: &str) -> String {
    codex_limit(now()).replace("usage_limit_exceeded", info)
}

/// The objective `GOAL` sets.
pub(super) const OBJECTIVE: &str = "Reply ok until told to stop";

/// `GOAL` with its status `status`, a JSON value.
pub(super) fn goal(status: &str) -> String {
    GOAL.replace(r#""active""#, status)
}

// Composed from the Codex 0.161.0 source (V4), not from a replay: a limit on one named model.
pub(super) const NAMED: &str = r#"{"timestamp":"2026-11-13T12:43:16.000Z","type":"event_msg","payload":{"type":"token_count","info":null,"rate_limits":{"limit_id":"codex","limit_name":"GPT-6 Astra","primary":null,"secondary":null,"credits":null,"individual_limit":null,"spend_control_reached":null,"plan_type":null,"rate_limit_reached_type":null}}}"#;

impl Fixture {
    /// Append `lines` to the rollout of the Codex thread `id` under the temp home.
    pub(super) fn rollout(&self, id: &str, lines: &[&str]) {
        let folder = self.dir.join("home/.codex/sessions/2026/11/13");
        std::fs::create_dir_all(&folder).expect("mkdir");
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(folder.join(format!("rollout-2026-11-13T12-00-00-{id}.jsonl")))
            .expect("open");
        for line in lines {
            writeln!(file, "{line}").expect("write");
        }
    }

    /// One pass, then another once the proofs it read have settled. The second's plans.
    pub(super) fn settled_pass(&self, ids: &[&str]) -> Vec<Plan> {
        self.office.handover_due();
        for id in ids {
            self.settle(id);
        }
        self.office.handover_due().plans
    }
}
