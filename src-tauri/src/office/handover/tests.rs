//! The office side of the handover, with fakes only: a `FakeRunner` that runs nothing,
//! `/bin/cat` in a terminal standing in for Claude Code, a stop closure that records the pids
//! it is given, and transcripts written under a temp home. No pass here calls `refresh` or
//! `hire_options`.

use super::run::codex_command;
use super::*;
use crate::discovery::claude_code::{pid_alive, terminate};
use crate::handover::start::first_prompt;
use crate::handover::view::OfferWhy;
use crate::handover::work::Signal;
use crate::handover::{Task, TaskKind, Window};
use crate::model::{
    EffortOption, EndedSession, ModelOption, Project, Session, SessionState, Spend, ToolKind,
    ToolOptions,
};
use crate::office::manager::Seat;
use crate::office::tests::floor_office;
use crate::process::FakeRunner;
use crate::settings::{SettingsPatch, SettingsStore};
use crate::setup::Launch;
use std::io::Write;
use std::sync::Mutex;
use std::time::{Instant, SystemTime};

/// The automatic switch on, and Codex starting on its own configured default.
const ON: &str = r#"{"automatic":true,"claudeToCodex":{"kind":"tool-default"}}"#;
/// The automatic switch off, and a choice made.
const OFF: &str = r#"{"automatic":false,"claudeToCodex":{"kind":"tool-default"}}"#;

/// The entry Claude Code 2.1.323 wrote when a local listener replayed a used-up allowance to it
/// on 2026-11-13. A replay, not Anthropic's own reply, and the transcript format is
/// undocumented. The reset is an hour ahead of the test's clock: the fixture's own number is in
/// the past by now, and a limit whose reset has passed is not recorded.
fn limit_line(resets_at: i64) -> String {
    format!(
        r#"{{"type":"assistant","isSidechain":false,"timestamp":"2026-11-13T10:53:07.124Z","message":{{"id":"m1","type":"message","role":"assistant","model":"<synthetic>","stop_reason":"stop_sequence","content":[{{"type":"text","text":"You've hit your 5-hour limit · resets 12:53pm (UTC)"}}]}},"apiError":"usage_limit_reached","apiErrorParams":{{"rate_limit_info":{{"status":"rejected","resetsAt":{resets_at},"rateLimitType":"five_hour","isUsingOverage":false}}}},"quotaLimits":{{"status":"rejected","resetsAt":{resets_at},"unifiedRateLimitFallbackAvailable":false,"rateLimitType":"five_hour","isUsingOverage":false}},"error":"rate_limit","isApiErrorMessage":true,"apiErrorStatus":429}}"#
    )
}

/// The entry Claude Code 2.1.323 wrote when a local listener replayed a throttle to it on
/// 2026-11-13. A replay, not Anthropic's own reply, and the transcript format is undocumented.
const THROTTLE: &str = r#"{"type":"assistant","isSidechain":false,"timestamp":"2026-11-13T11:17:26.534Z","message":{"id":"m2","type":"message","role":"assistant","model":"<synthetic>","stop_reason":"stop_sequence","content":[{"type":"text","text":"API Error: Server is temporarily limiting requests (not your usage limit)"}]},"apiErrorIsTransient":true,"error":"rate_limit","isApiErrorMessage":true,"apiErrorStatus":429}"#;

/// Composed from the key names of entries seen on one machine, not from a replay.
const AUTHENTICATION: &str = r#"{"type":"assistant","isSidechain":false,"timestamp":"2026-11-13T11:17:26.534Z","message":{"id":"m3","type":"message","role":"assistant","model":"<synthetic>","stop_reason":"stop_sequence","content":[{"type":"text","text":"Please run /login"}]},"error":"authentication_failed","isApiErrorMessage":true,"apiErrorStatus":401}"#;

/// Composed from the key names of entries seen on one machine, not from a replay.
const SERVER: &str = r#"{"type":"assistant","isSidechain":false,"timestamp":"2026-11-13T11:17:26.534Z","message":{"id":"m4","type":"message","role":"assistant","model":"<synthetic>","stop_reason":"stop_sequence","content":[{"type":"text","text":"API Error: 500"}]},"error":"server_error","isApiErrorMessage":true,"apiErrorStatus":500}"#;

/// The model answering again.
const REPLY: &str = r#"{"type":"assistant","isSidechain":false,"message":{"id":"m5","type":"message","role":"assistant","model":"claude-opus-4-1","content":[{"type":"text","text":"Back at it."}]}}"#;

const TITLE: &str = "Fix the importer";

struct Fixture {
    office: Office,
    dir: PathBuf,
    floor: Project,
    settings: Arc<SettingsStore>,
    stops: Arc<Mutex<Vec<i32>>>,
}

/// An office on a floor, with `handover` as its settings group. `terminate`: the stop sends
/// SIGTERM, else it only records. `file`: another office's `handover.json`.
fn build(tag: &str, handover: &str, terminate_on_stop: bool, file: Option<PathBuf>) -> Fixture {
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

fn fixture(tag: &str, handover: &str) -> Fixture {
    build(tag, handover, true, None)
}

fn session(id: &str, tool: ToolKind, project_id: &str) -> Session {
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

fn wait_for(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

impl Fixture {
    fn root(&self) -> PathBuf {
        self.dir.join("home/code/app")
    }

    fn host(&self) -> &HandoverHost {
        self.office.handover_host.as_ref().expect("host")
    }

    /// `/bin/cat` in a terminal of its own, running once the login shell has handed over.
    fn terminal(&self) -> String {
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
    fn desk(&self, id: &str, tool: ToolKind) -> String {
        let terminal = self.terminal();
        self.add(id, tool, Some(&terminal));
        terminal
    }

    /// A session discovery found, in `terminal` if it is given one.
    fn add(&self, id: &str, tool: ToolKind, terminal: Option<&str>) {
        let mut inner = self.office.lock();
        if let Some(terminal) = terminal {
            inner.owned.insert(terminal.to_string(), id.to_string());
        }
        inner.found.sessions.push(session(id, tool, &self.floor.id));
    }

    /// Append `lines` to the session's transcript.
    fn transcript(&self, id: &str, lines: &[&str]) {
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
    fn listed(&self, id: &str, tasks: Vec<Task>) {
        self.office.lock().handover.works.note(Signal::Listed {
            session_id: id.to_string(),
            at: SystemTime::now(),
            tasks: Some(tasks),
        });
    }

    /// Move the desk's proof back past the settle time instead of sleeping through it.
    fn settle(&self, id: &str) {
        self.office
            .lock()
            .handover
            .undecided
            .insert(id.to_string(), now() - SETTLE_SECS);
    }

    fn state(&self, id: &str) -> Option<DeskState> {
        self.office
            .lock()
            .handover
            .view
            .desks
            .iter()
            .find(|d| d.session_id == id)
            .map(|d| d.state.clone())
    }

    fn stops(&self) -> Vec<i32> {
        lock(&self.stops).clone()
    }

    fn plan(&self, id: &str, by_human: bool) -> Plan {
        self.office
            .plan(&self.office.lock(), id, by_human)
            .expect("plan")
    }

    fn choose(&self, handover: HandoverSettings) {
        self.settings
            .update(SettingsPatch {
                handover: Some(handover),
                ..SettingsPatch::default()
            })
            .expect("settings");
    }

    /// End every agent still running and remove the temp folder.
    fn done(self) {
        for terminal in self.office.terminals.running_children().values() {
            let _ = self.office.terminals.write(terminal, "\u{4}");
        }
        std::fs::remove_dir_all(&self.dir).expect("cleanup");
    }
}

/// `change_hands`, with its failure as the reason and whether the old session was ended.
fn change(fx: &Fixture, plan: &Plan, command: &str) -> Result<String, (String, bool)> {
    fx.office
        .change_hands(fx.host(), plan, command)
        .map_err(|f| (f.reason, f.ended))
}

fn offered(why: OfferWhy, work: Work, reset_at: i64) -> DeskState {
    DeskState::Offered {
        to: Tool::Codex,
        why,
        work,
        window: Some(Window::FiveHour),
        reset_at: Some(reset_at),
    }
}

#[test]
fn only_a_running_desk_the_building_started_is_ours() {
    let fx = fixture("ours", OFF);
    let reset = now() + 3600;
    let line = limit_line(reset);
    fx.desk("s-ours", ToolKind::ClaudeCode);
    fx.add("s-elsewhere", ToolKind::ClaudeCode, None);
    let reviewer = fx.desk("s-reviewer", ToolKind::ClaudeCode);
    let manager = fx.desk("s-manager", ToolKind::ClaudeCode);
    let exited = fx.desk("s-exited", ToolKind::ClaudeCode);
    {
        let mut inner = fx.office.lock();
        inner.reviewers.insert(reviewer, fx.floor.id.clone());
        inner.seats.insert(
            fx.floor.id.clone(),
            Seat {
                terminal: manager,
                started_at: String::new(),
                last_call_at: None,
                hires: Vec::new(),
                sent_to: BTreeSet::new(),
            },
        );
    }
    fx.office.terminals.write(&exited, "\u{4}").expect("eot");
    wait_for("the desk to exit", || {
        !matches!(
            fx.office.terminals.wait_exit(&exited, Duration::ZERO),
            Ok(None)
        )
    });
    let others = ["s-elsewhere", "s-reviewer", "s-manager", "s-exited"];
    for id in std::iter::once("s-ours").chain(others) {
        fx.transcript(id, &[&line]);
    }
    fx.office.handover_due();
    fx.settle("s-ours");
    let due = fx.office.handover_due();
    assert!(due.plans.is_empty());
    assert_eq!(
        fx.state("s-ours"),
        Some(offered(OfferWhy::AutomaticOff, Work::Unknown, reset))
    );
    let followed: Vec<String> = lock(&fx.office.limits).keys().cloned().collect();
    assert_eq!(followed, vec!["s-ours".to_string()]);
    for id in others {
        assert_eq!(fx.state(id), None, "{id}");
        assert_eq!(
            fx.office.hand_over_desk(id),
            Err(NO_ENTRY.to_string()),
            "{id}"
        );
        let refused = fx
            .office
            .plan(&fx.office.lock(), id, true)
            .map(|p| p.session_id);
        assert_eq!(refused, Err(NOT_OURS.to_string()), "{id}");
    }
    assert!(fx.stops().is_empty());
    fx.done();
}

#[test]
fn a_quiet_used_up_desk_is_handed_over_once_after_the_settle_time() {
    let fx = fixture("pass", ON);
    let terminal = fx.desk("s-quiet", ToolKind::ClaudeCode);
    fx.listed("s-quiet", Vec::new());
    let reset = now() + 3600;
    fx.transcript("s-quiet", &[&limit_line(reset)]);

    let due = fx.office.handover_due();
    assert!(due.plans.is_empty());
    assert_eq!(fx.state("s-quiet"), None);
    let used_up = fx.office.lock().handover.view.used_up.clone();
    assert_eq!(used_up.len(), 1);
    assert_eq!(
        (used_up[0].tool, used_up[0].window, used_up[0].reset_at),
        (Tool::ClaudeCode, Some(Window::FiveHour), Some(reset))
    );

    fx.settle("s-quiet");
    let due = fx.office.handover_due();
    assert!(due.changed);
    assert_eq!(due.plans.len(), 1);
    let plan = &due.plans[0];
    assert_eq!(
        (
            plan.session_id.as_str(),
            plan.terminal.as_str(),
            plan.by_human,
            plan.title.as_str(),
            plan.floor.as_str(),
        ),
        (
            "s-quiet",
            terminal.as_str(),
            false,
            TITLE,
            fx.floor.name.as_str()
        )
    );
    assert_eq!(plan.folder, fx.root());
    assert_eq!(plan.start, StartWith::ToolDefault);
    assert_eq!(plan.work, Work::Clear);
    assert_eq!(plan.record.reset_at, Some(reset));
    assert!(fx.office.lock().handover.underway.contains("s-quiet"));
    assert_eq!(
        fx.state("s-quiet"),
        Some(DeskState::Underway { to: Tool::Codex })
    );
    assert_eq!(
        fx.office.snapshot().handover,
        fx.office.lock().handover.view
    );

    let due = fx.office.handover_due();
    assert!(due.plans.is_empty());
    assert!(!due.changed);
    assert_eq!(
        fx.office.hand_over_desk("s-quiet"),
        Err("this desk is already being handed over".to_string())
    );
    assert!(fx.stops().is_empty());
    fx.done();
}

#[test]
fn a_throttle_an_authentication_failure_and_a_server_fault_hand_nothing_over() {
    let fx = fixture("api-errors", ON);
    for (n, line) in [THROTTLE, AUTHENTICATION, SERVER].into_iter().enumerate() {
        let id = format!("s-api-{n}");
        fx.desk(&id, ToolKind::ClaudeCode);
        fx.listed(&id, Vec::new());
        fx.transcript(&id, &[line]);
    }
    for _ in 0..2 {
        let due = fx.office.handover_due();
        assert!(due.plans.is_empty());
    }
    let inner = fx.office.lock();
    assert!(inner.handover.book.used_up().is_empty());
    assert!(inner.handover.undecided.is_empty());
    assert_eq!(inner.handover.view, HandoverView::default());
    drop(inner);
    fx.done();
}

#[test]
fn background_work_or_no_listing_waits_for_a_human() {
    let fx = fixture("offers", ON);
    let reset = now() + 3600;
    let line = limit_line(reset);
    let workflow = Task {
        kind: TaskKind::Workflow,
        name: Some("release".to_string()),
    };
    fx.desk("s-workflow", ToolKind::ClaudeCode);
    fx.listed("s-workflow", vec![workflow.clone()]);
    fx.desk("s-unknown", ToolKind::ClaudeCode);
    for id in ["s-workflow", "s-unknown"] {
        fx.transcript(id, &[&line]);
    }
    fx.office.handover_due();
    fx.settle("s-workflow");
    fx.settle("s-unknown");
    let due = fx.office.handover_due();
    assert!(due.plans.is_empty());
    assert_eq!(
        fx.state("s-workflow"),
        Some(offered(
            OfferWhy::WorkRunning,
            Work::Running {
                tasks: vec![workflow]
            },
            reset
        ))
    );
    assert_eq!(
        fx.state("s-unknown"),
        Some(offered(OfferWhy::WorkUnknown, Work::Unknown, reset))
    );
    fx.done();
}

#[test]
fn with_the_switch_off_a_desk_is_offered_and_a_reply_takes_the_record_away() {
    let fx = fixture("switch-off", OFF);
    fx.desk("s-off", ToolKind::ClaudeCode);
    fx.listed("s-off", Vec::new());
    let reset = now() + 3600;
    fx.transcript("s-off", &[&limit_line(reset)]);
    fx.office.handover_due();
    fx.settle("s-off");
    let due = fx.office.handover_due();
    assert!(due.plans.is_empty());
    assert_eq!(
        fx.state("s-off"),
        Some(offered(OfferWhy::AutomaticOff, Work::Clear, reset))
    );
    fx.transcript("s-off", &[REPLY]);
    let due = fx.office.handover_due();
    assert!(due.changed);
    assert_eq!(fx.state("s-off"), None);
    assert!(fx.office.lock().handover.book.used_up().is_empty());
    fx.done();
}

#[test]
fn a_limit_whose_reset_has_passed_is_not_recorded() {
    let fx = fixture("past", ON);
    fx.desk("s-past", ToolKind::ClaudeCode);
    fx.listed("s-past", Vec::new());
    fx.transcript("s-past", &[&limit_line(now() - 60)]);
    let due = fx.office.handover_due();
    assert!(due.plans.is_empty());
    let inner = fx.office.lock();
    assert!(inner.handover.book.used_up().is_empty());
    assert!(inner.handover.undecided.is_empty());
    assert_eq!(inner.handover.view, HandoverView::default());
    drop(inner);
    fx.done();
}

/// A plan for a desk whose record was written by hand.
fn hand_plan(fx: &Fixture, id: &str, terminal: &str) -> Plan {
    let plan = Plan {
        session_id: id.to_string(),
        terminal: terminal.to_string(),
        by_human: true,
        record: UsedUp {
            tool: Tool::ClaudeCode,
            window: Some(Window::FiveHour),
            reset_at: Some(now() + 3600),
            seen_at: now(),
            session_id: id.to_string(),
        },
        start: StartWith::ToolDefault,
        title: TITLE.to_string(),
        folder: fx.root(),
        floor: fx.floor.name.clone(),
        branch: None,
        worktree: false,
        work: Work::Clear,
    };
    // The recheck before the stop reads the record from the book.
    fx.office.lock().handover.book.saw(plan.record.clone());
    plan
}

#[test]
fn the_old_session_is_asked_to_stop_once_and_the_receiver_starts_in_its_folder() {
    let fx = fixture("stop", ON);
    let old = fx.desk("s-old", ToolKind::ClaudeCode);
    let pid = fx.office.terminals.pid(&old).expect("pid");
    let plan = hand_plan(&fx, "s-old", &old);
    let started = change(&fx, &plan, "/bin/sh -c 'pwd -P; exec /bin/cat'").expect("started");
    assert_eq!(fx.stops(), vec![pid]);
    assert!(!pid_alive(pid));
    assert_ne!(started, old);
    let folder = std::fs::canonicalize(fx.root()).expect("canonical");
    let folder = folder.to_string_lossy().into_owned();
    wait_for("the receiver to print its folder", || {
        fx.office
            .terminals
            .backlog(&started)
            .is_ok_and(|b| crate::pty::plain_text(&b.data).contains(&folder))
    });
    assert_eq!(
        fx.office.terminals.wait_exit(&started, Duration::ZERO),
        Ok(None)
    );
    fx.done();
}

#[test]
fn a_session_that_does_not_stop_is_never_forced_and_codex_is_not_started() {
    let fx = build("no-stop", ON, false, None);
    let old = fx.desk("s-stays", ToolKind::ClaudeCode);
    let pid = fx.office.terminals.pid(&old).expect("pid");
    let plan = hand_plan(&fx, "s-stays", &old);
    let before = fx.office.terminals.running_children();
    let asked = Instant::now();
    let got = change(&fx, &plan, "/bin/cat");
    assert!(asked.elapsed() >= Duration::from_millis(300));
    assert_eq!(
        got,
        Err((
            "Claude Code did not stop when asked, so Codex was not started. Type /exit in its terminal, then hand the desk over again".to_string(),
            false
        ))
    );
    assert_eq!(fx.stops(), vec![pid]);
    std::thread::sleep(Duration::from_millis(300));
    assert!(pid_alive(pid));
    assert_eq!(fx.office.terminals.running_children(), before);
    fx.done();
}

#[test]
fn an_old_session_that_already_exited_is_not_asked_to_stop() {
    let fx = fixture("exited", ON);
    let old = fx.desk("s-gone", ToolKind::ClaudeCode);
    fx.office.terminals.write(&old, "\u{4}").expect("eot");
    wait_for("the desk to exit", || {
        !matches!(
            fx.office.terminals.wait_exit(&old, Duration::ZERO),
            Ok(None)
        )
    });
    let plan = hand_plan(&fx, "s-gone", &old);
    let started = change(&fx, &plan, "/bin/cat");
    assert!(started.is_ok());
    assert!(fx.stops().is_empty());
    fx.done();
}

#[test]
fn a_reviewers_terminal_is_refused_and_never_stopped() {
    let fx = fixture("reviewer", ON);
    let old = fx.desk("s-review", ToolKind::ClaudeCode);
    let pid = fx.office.terminals.pid(&old).expect("pid");
    fx.office
        .lock()
        .reviewers
        .insert(old.clone(), fx.floor.id.clone());
    let plan = hand_plan(&fx, "s-review", &old);
    let got = change(&fx, &plan, "/bin/cat");
    assert_eq!(
        got,
        Err((
            "the desk changed while it was being handed over".to_string(),
            false
        ))
    );
    assert!(fx.stops().is_empty());
    assert!(pid_alive(pid));
    fx.done();
}

#[test]
fn a_receiver_that_exits_at_once_is_a_failure_after_the_end() {
    let fx = fixture("false", ON);
    let old = fx.desk("s-false", ToolKind::ClaudeCode);
    let pid = fx.office.terminals.pid(&old).expect("pid");
    let plan = hand_plan(&fx, "s-false", &old);
    let got = change(&fx, &plan, "/usr/bin/false");
    let (reason, ended) = got.expect_err("failed");
    assert!(ended);
    assert!(
        reason.starts_with("Claude Code was ended and Codex did not start: "),
        "{reason}"
    );
    assert!(reason.ends_with(". Resume the Claude Code session from the sign-out sheet"));
    assert_eq!(fx.stops(), vec![pid]);
    fx.done();
}

#[test]
fn the_handover_code_holds_no_force_and_types_nothing() {
    for (file, text) in [
        ("office/handover.rs", include_str!("../handover.rs")),
        ("office/handover/run.rs", include_str!("run.rs")),
    ] {
        for word in ["SIGKILL", "kill(", "SIGHUP", "terminals.write"] {
            assert!(!text.contains(word), "{file} holds {word}");
        }
    }
}

fn codex_options() -> ToolOptions {
    let model = |id: &str, efforts: &[&str]| ModelOption {
        id: id.to_string(),
        label: id.to_string(),
        efforts: efforts
            .iter()
            .map(|e| EffortOption {
                id: (*e).to_string(),
                label: (*e).to_string(),
                description: String::new(),
            })
            .collect(),
        default_effort: "high".to_string(),
    };
    ToolOptions::new(
        ToolKind::Codex,
        true,
        vec![
            model("gpt-6-astra", &["low", "high"]),
            model("odd'$(touch x)", &["high"]),
        ],
        "gpt-6-astra".to_string(),
        None,
    )
}

#[test]
fn codex_starts_as_an_ordinary_hire_with_every_word_quoted() {
    let setup = Launch {
        env: vec![("CODEX_HOME", "/data/setup-build/codex-home".to_string())],
        ..Launch::default()
    };
    let folder = Path::new("/work/code/app");
    let prompt = first_prompt(
        TITLE,
        Tool::ClaudeCode,
        Path::new("/data/handover/briefs/h-1.md"),
    );
    let chosen = |model: &str, effort: &str| StartWith::Chosen {
        model: model.to_string(),
        effort: effort.to_string(),
    };
    let lead =
        "env CODEX_HOME='/data/setup-build/codex-home' codex -c check_for_update_on_startup=false";
    let quoted_prompt = format!("'{prompt}'");
    let rows: Vec<(StartWith, &str, Result<String, String>)> = vec![
        (
            chosen("gpt-6-astra", "high"),
            prompt.as_str(),
            Ok(format!(
                r#"{lead} -m 'gpt-6-astra' -c 'model_reasoning_effort="high"' -- {quoted_prompt}"#
            )),
        ),
        (
            StartWith::ToolDefault,
            prompt.as_str(),
            Ok(format!("{lead} -- {quoted_prompt}")),
        ),
        (
            chosen("odd'$(touch x)", "high"),
            "go",
            Ok(format!(
                r#"{lead} -m 'odd'\''$(touch x)' -c 'model_reasoning_effort="high"' -- 'go'"#
            )),
        ),
        (
            StartWith::ToolDefault,
            "it's \"x\" `y` $(x); z",
            Ok(format!(r#"{lead} -- 'it'\''s "x" `y` $(x); z'"#)),
        ),
        (
            chosen("gpt-7", "high"),
            "go",
            Err("gpt-7 is not a model the app can start".to_string()),
        ),
        (
            chosen("gpt-6-astra", "max"),
            "go",
            Err("gpt-6-astra does not support max effort".to_string()),
        ),
        (
            chosen("gpt-6-astra", "hi\"gh"),
            "go",
            Err("that effort cannot be passed to Codex".to_string()),
        ),
        (
            chosen("gpt-6-astra", "hi\\gh"),
            "go",
            Err("that effort cannot be passed to Codex".to_string()),
        ),
    ];
    for (start, prompt, want) in rows {
        let got = codex_command(&start, &codex_options(), prompt, "/bin/zsh", &setup);
        assert_eq!(got, want, "{start:?}");
        if let Ok(command) = got {
            assert!(!command.contains(&*folder.to_string_lossy()));
        }
    }
}

#[test]
fn a_failure_is_recorded_shown_and_never_retried_by_the_building() {
    let fx = fixture("failed", ON);
    fx.desk("s-fail", ToolKind::ClaudeCode);
    let gone = fx.dir.join("home/code/gone");
    std::fs::create_dir_all(&gone).expect("mkdir");
    fx.office
        .lock()
        .found
        .checkouts
        .insert("s-fail".to_string(), gone.clone());
    fx.listed("s-fail", Vec::new());
    let reset = now() + 3600;
    fx.transcript("s-fail", &[&limit_line(reset)]);
    fx.office.handover_due();
    fx.settle("s-fail");
    let mut due = fx.office.handover_due();
    assert_eq!(due.plans.len(), 1);
    std::fs::remove_dir_all(&gone).expect("remove");
    let plan = due.plans.remove(0);
    assert_eq!(plan.folder, gone);
    assert_eq!(
        fx.office.carry_out_handover(&plan),
        Err(FOLDER_GONE.to_string())
    );
    assert!(fx.stops().is_empty());
    {
        let inner = fx.office.lock();
        let done = inner.handover.book.done();
        assert_eq!(done.len(), 1);
        assert_eq!(
            done[0].outcome,
            Outcome::Failed {
                reason: FOLDER_GONE.to_string(),
                ended: false
            }
        );
        assert_eq!(
            (done[0].from_session.as_str(), done[0].from, done[0].to),
            ("s-fail", Tool::ClaudeCode, Tool::Codex)
        );
        assert!(inner.handover.underway.is_empty());
    }
    let failed = OfferWhy::Failed {
        reason: FOLDER_GONE.to_string(),
    };
    assert_eq!(
        fx.state("s-fail"),
        Some(offered(failed.clone(), Work::Clear, reset))
    );
    let saved = std::fs::read_to_string(&fx.host().file).expect("saved");
    assert!(saved.contains(FOLDER_GONE));
    for _ in 0..2 {
        assert!(fx.office.handover_due().plans.is_empty());
    }
    assert_eq!(
        fx.state("s-fail"),
        Some(offered(failed, Work::Clear, reset))
    );
    // A Human may try again.
    assert_eq!(
        fx.office.hand_over_desk("s-fail"),
        Err(FOLDER_GONE.to_string())
    );
    assert_eq!(fx.office.lock().handover.book.done().len(), 2);
    assert!(fx.stops().is_empty());
    assert!(!fx.host().briefs.exists());
    fx.done();
}

#[test]
fn a_human_handover_is_refused_in_words() {
    let fx = fixture("refusals", ON);
    fx.desk("s-one", ToolKind::ClaudeCode);
    fx.add("s-theirs", ToolKind::ClaudeCode, None);
    let reset = now() + 3600;
    let record = |tool: Tool, id: &str| UsedUp {
        tool,
        window: Some(Window::FiveHour),
        reset_at: Some(reset),
        seen_at: now(),
        session_id: id.to_string(),
    };
    let refused = |id: &str| fx.office.hand_over_desk(id);
    assert_eq!(refused("s-one"), Err(NO_ENTRY.to_string()), "no record");
    fx.office
        .lock()
        .handover
        .book
        .saw(record(Tool::ClaudeCode, "s-theirs"));
    assert_eq!(refused("s-theirs"), Err(NO_ENTRY.to_string()), "not ours");
    fx.office
        .lock()
        .handover
        .book
        .saw(record(Tool::ClaudeCode, "s-one"));

    fx.choose(HandoverSettings::default());
    assert_eq!(refused("s-one"), Err(CHOOSE.to_string()), "nothing chosen");
    let on: HandoverSettings = serde_json::from_str(ON).expect("settings");
    fx.choose(on);

    fx.office
        .lock()
        .handover
        .book
        .saw(record(Tool::Codex, "s-codex"));
    assert_eq!(
        refused("s-one"),
        Err("Codex is used up too, so this desk stays where it is".to_string())
    );
    fx.office.lock().handover.book.cleared("s-codex");

    fx.office
        .lock()
        .handover
        .underway
        .insert("s-one".to_string());
    assert_eq!(
        refused("s-one"),
        Err("this desk is already being handed over".to_string())
    );
    fx.office.lock().handover.underway.remove("s-one");

    let id = fx.office.lock().handover.book.next_id();
    fx.office.lock().handover.book.finished(Done {
        id,
        from_session: "s-one".to_string(),
        from: Tool::ClaudeCode,
        to: Tool::Codex,
        at: now(),
        window: Some(Window::FiveHour),
        reset_at: Some(reset),
        seen_at: now(),
        outcome: Outcome::Started { session: None },
    });
    assert_eq!(
        refused("s-one"),
        Err("this desk was already handed over for this limit".to_string())
    );
    assert!(fx.stops().is_empty());
    fx.done();
}

/// Hand `id` over by hand as far as the fakes allow: the plan, the change and its record.
fn hand_over_with_cat(fx: &Fixture, plan: &Plan) -> String {
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

#[test]
fn the_receiver_gets_its_note_and_the_old_desk_its_sign_out_line() {
    let fx = fixture("note", OFF);
    fx.desk("s-old", ToolKind::ClaudeCode);
    let reset = now() + 3600;
    fx.transcript("s-old", &[&limit_line(reset)]);
    fx.office.handover_due();
    fx.settle("s-old");
    fx.office.handover_due();
    let plan = fx.plan("s-old", true);
    let started = hand_over_with_cat(&fx, &plan);
    assert!(fx.office.lock().handover.underway.is_empty());
    {
        let mut inner = fx.office.lock();
        inner.found.sessions.retain(|s| s.id != "s-old");
        inner.found.ended.push(EndedSession {
            id: "s-old".to_string(),
            tool: ToolKind::ClaudeCode,
            project_id: fx.floor.id.clone(),
            title: TITLE.to_string(),
            ended_at: String::new(),
            model: None,
            effort: None,
        });
        inner.owned.insert(started, "s-new".to_string());
        inner
            .found
            .sessions
            .push(session("s-new", ToolKind::Codex, &fx.floor.id));
    }
    assert!(fx.office.handover_due().changed);
    let at = fx.office.lock().handover.book.done()[0].at;
    assert_eq!(
        fx.state("s-new"),
        Some(DeskState::Arrived {
            from: Tool::ClaudeCode,
            at,
            window: Some(Window::FiveHour),
            reset_at: Some(reset),
        })
    );
    assert_eq!(
        fx.state("s-old"),
        Some(DeskState::Left {
            to: Tool::Codex,
            at
        })
    );
    let saved = std::fs::read_to_string(&fx.host().file).expect("saved");
    assert!(saved.contains(r#""session": "s-new""#), "{saved}");
    assert_eq!(
        fx.office.snapshot().handover,
        fx.office.lock().handover.view
    );
    fx.done();
}

#[test]
fn a_restart_keeps_the_record_and_the_handover() {
    let first = fixture("restart-1", ON);
    first.desk("s-again", ToolKind::ClaudeCode);
    first.listed("s-again", Vec::new());
    let reset = now() + 3600;
    let line = limit_line(reset);
    first.transcript("s-again", &[&line]);
    first.office.handover_due();
    first.settle("s-again");
    let mut due = first.office.handover_due();
    assert_eq!(due.plans.len(), 1);
    let plan = due.plans.remove(0);
    hand_over_with_cat(&first, &plan);

    let file = first.host().file.clone();
    let second = build("restart-2", ON, true, Some(file));
    assert_eq!(second.office.lock().handover.book.used_up().len(), 1);
    second.desk("s-again", ToolKind::ClaudeCode);
    second.listed("s-again", Vec::new());
    second.transcript("s-again", &[&line]);
    let due = second.office.handover_due();
    assert!(due.plans.is_empty());
    assert!(second.office.lock().handover.undecided.is_empty());
    let used_up = second.office.lock().handover.view.used_up.clone();
    assert_eq!(used_up.len(), 1);
    assert_eq!(used_up[0].tool, Tool::ClaudeCode);
    assert_eq!(
        second.state("s-again"),
        Some(DeskState::Blocked {
            to: Tool::Codex,
            why: BlockWhy::AlreadyHandedOver,
            window: Some(Window::FiveHour),
            reset_at: Some(reset),
            other_reset_at: None,
        })
    );
    second.done();
    first.done();

    let lapsed = build("restart-3", ON, true, None);
    let text = format!(
        r#"{{"version":1,"usedUp":[{{"tool":"claude-code","window":"five-hour","resetAt":{},"seenAt":{},"sessionId":"s-old"}}],"done":[],"nextId":1}}"#,
        now() - 60,
        now() - 3600
    );
    std::fs::write(&lapsed.host().file, text).expect("write");
    let file = lapsed.host().file.clone();
    let reloaded = build("restart-4", ON, true, Some(file));
    assert!(reloaded.office.lock().handover.book.used_up().is_empty());
    reloaded.done();
    lapsed.done();
}
