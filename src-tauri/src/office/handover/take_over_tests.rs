//! The take-over's office tests, with fakes only: `/bin/cat` in a terminal standing in for
//! each session, a `FakeRunner` that runs nothing, and transcripts under a temp home.

use super::fixture::*;
use super::run::{Prepared, receiver_command, with_leave};
use super::take_over::TakeOverPlan;
use super::*;
use crate::discovery::claude_code::pid_alive;
use crate::handover::brief;
use crate::handover::start::{refused_ids, take_over_prompt};
use crate::handover::view::{TakeOver, TakeOverStart};
use crate::model::{EffortOption, ModelOption, SessionState, ToolKind, ToolOptions};
use crate::office::manager::Seat;
use crate::office::tests::floor_office;
use crate::setup::Launch;
use crate::structured;

/// The automatic switch off and nothing chosen for either direction.
const NOTHING: &str = r#"{"automatic":false}"#;

/// A Human's own words in a transcript, as Claude Code writes them.
const PROMPT: &str =
    r#"{"type":"user","message":{"role":"user","content":"Please fix the importer"}}"#;

const NO_CONVERSATION: &str =
    "the building cannot read that session's conversation, so there is nothing to take over";

fn chosen(tool: Tool) -> TakeOverStart {
    TakeOverStart {
        tool,
        model: "m".to_string(),
        effort: "high".to_string(),
    }
}

fn plan(fx: &Fixture, id: &str, to: Tool) -> Result<TakeOverPlan, String> {
    fx.office.take_over_plan(&fx.office.lock(), id, &chosen(to))
}

/// A seat for a manager in `terminal` on the fixture's floor.
fn seat(terminal: String) -> Seat {
    Seat {
        terminal,
        started_at: String::new(),
        last_call_at: None,
        sent_to: BTreeSet::new(),
        ending: BTreeSet::new(),
    }
}

struct Quiet;

impl structured::Driver for Quiet {
    fn prompt(&self, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn answer(&self, _: &str, _: &str, _: &structured::Values) -> Result<(), String> {
        Ok(())
    }
    fn cancel(&self) -> Result<(), String> {
        Ok(())
    }
    fn stop(&self) {}
}

fn briefs(fx: &Fixture) -> Vec<PathBuf> {
    std::fs::read_dir(&fx.host().briefs)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .collect()
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "one reach table over every kind of session"
)]
fn the_reach_of_a_take_over() {
    let fx = fixture("take-over-reach", NOTHING);
    let codex = thread(1);
    fx.desk("s-claude", ToolKind::ClaudeCode);
    fx.desk(&codex, ToolKind::Codex);
    fx.add("s-window", ToolKind::ClaudeCode, None);
    fx.add("s-read", ToolKind::Codex, None);
    fx.add("s-other", ToolKind::Opencode, None);
    let manager = fx.desk("s-manager", ToolKind::ClaudeCode);
    let reviewer = fx.desk("s-reviewer", ToolKind::ClaudeCode);
    fx.desk("s-underway", ToolKind::ClaudeCode);
    let checkout = fx.root().join("worktree");
    std::fs::create_dir_all(&checkout).expect("checkout");
    let root = {
        let mut inner = fx.office.lock();
        for s in &mut inner.found.sessions {
            s.control = match s.id.as_str() {
                "s-window" => ControlMode::RaiseWindow,
                "s-read" => ControlMode::ReadOnly,
                _ => s.control,
            };
        }
        inner
            .found
            .sessions
            .push(session("s-nowhere", ToolKind::ClaudeCode, "p-gone"));
        inner
            .found
            .checkouts
            .insert(codex.clone(), checkout.clone());
        inner.reviewers.insert(reviewer, fx.floor.id.clone());
        inner.seats.insert(fx.floor.id.clone(), seat(manager));
        inner.handover.underway.insert("s-underway".to_string());
        inner.found.roots.get(&fx.floor.id).cloned().expect("root")
    };
    let info = structured::desks::DeskInfo {
        tool: ToolKind::Codex,
        project_id: fx.floor.id.clone(),
        root: root.clone(),
        title: "in app".to_string(),
        model: "default".to_string(),
        effort: "default".to_string(),
    };
    fx.office
        .structured
        .seat("structured-1", info, SessionState::Idle, Box::new(Quiet));

    let planned = [
        ("s-claude", Tool::ClaudeCode, &root),
        (codex.as_str(), Tool::Codex, &checkout),
        ("s-window", Tool::ClaudeCode, &root),
        ("s-read", Tool::Codex, &root),
    ];
    for (id, from, folder) in planned {
        let got = plan(&fx, id, Tool::Codex).expect(id);
        assert_eq!(
            (got.session_id.as_str(), got.from, got.to, &got.folder),
            (id, from, Tool::Codex, folder),
            "{id}"
        );
    }

    let unwritable = "that session's id cannot be written into a prompt";
    let refused: Vec<(String, &str)> = refused_ids()
        .into_iter()
        .map(|id| (id, unwritable))
        .chain(
            [
                ("s-unknown", "that session is no longer at a desk"),
                (
                    "structured-1",
                    "a desk in a tool's chat view cannot be taken over yet",
                ),
                (
                    "s-other",
                    "only a Claude Code or Codex session can be taken over",
                ),
                (
                    "s-manager",
                    "a floor's manager or QA reviewer is not taken over",
                ),
                (
                    "s-reviewer",
                    "a floor's manager or QA reviewer is not taken over",
                ),
                ("s-underway", "this desk is being handed over"),
                ("s-nowhere", FOLDER_GONE),
            ]
            .map(|(id, why)| (id.to_string(), why)),
        )
        .collect();
    for (id, why) in &refused {
        for to in [Tool::ClaudeCode, Tool::Codex] {
            assert_eq!(
                plan(&fx, id, to).map(|p| p.session_id),
                Err((*why).to_string()),
                "{id:?}"
            );
            assert_eq!(
                fx.office.take_over_desk(id, &chosen(to)),
                Err((*why).to_string()),
                "{id:?}"
            );
        }
    }
    assert!(briefs(&fx).is_empty());
    assert!(fx.stops().is_empty());
    assert!(fx.office.lock().handover.take_overs.is_empty());
    fx.done();

    let (bare, dir, _) = floor_office("take-over-no-host");
    assert_eq!(
        bare.take_over_desk("s-claude", &chosen(Tool::Codex)),
        Err(NOT_SET_UP.to_string())
    );
    std::fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn only_a_conversation_the_building_can_read_is_taken_over() {
    let fx = fixture("take-over-talk", NOTHING);
    for id in ["s-silent", "claude-new-123", "s-talks"] {
        fx.add(id, ToolKind::ClaudeCode, None);
    }
    fx.transcript("s-talks", &[PROMPT, REPLY]);
    for id in ["s-silent", "claude-new-123"] {
        let planned = plan(&fx, id, Tool::Codex).expect(id);
        assert_eq!(
            fx.office.conversation_to_take_over(&planned).map(|t| t.0),
            Err(NO_CONVERSATION.to_string()),
            "{id}"
        );
        assert_eq!(
            fx.office.take_over_desk(id, &chosen(Tool::Codex)),
            Err(NO_CONVERSATION.to_string())
        );
    }
    let planned = plan(&fx, "s-talks", Tool::Codex).expect("planned");
    let (entries, earlier) = fx.office.conversation_to_take_over(&planned).expect("read");
    assert_eq!(entries.len(), 2);
    assert!(!earlier);
    assert!(briefs(&fx).is_empty());
    fx.done();
}

fn options(tool: ToolKind, model: &str) -> ToolOptions {
    let efforts = ["low", "high"]
        .map(|e| EffortOption {
            id: e.to_string(),
            label: e.to_string(),
            description: String::new(),
        })
        .to_vec();
    let listed = ModelOption {
        id: model.to_string(),
        label: model.to_string(),
        efforts,
        default_effort: "high".to_string(),
    };
    ToolOptions::new(tool, true, vec![listed], model.to_string(), None)
}

/// `prompt` as `pty::quote` writes it inside single quotes.
fn quoted(prompt: &str) -> String {
    prompt.replace('\'', r"'\''")
}

/// The tool chosen, its model and effort, the brief's path and the command wanted.
type LaunchRow<'a> = (Tool, &'a str, &'a str, &'a str, Result<String, String>);

#[test]
fn the_launch_words_of_a_take_over() {
    const ID: &str = "0f5c2a9e-7d1b-4c3a-9e2f-1a2b3c4d5e6f";
    const BRIEF: &str = "/data/handover/briefs/h-14.md";
    const ODD: &str = "/data/it's/h-14.md";
    let opus = "claude-opus-5-5";
    let astra = "gpt-6-astra";
    let claude = |brief: &str, rule: &str| {
        format!(
            r#"claude --allowedTools '{rule}' --model '{opus}' --effort 'high' --settings '{{"ultracode":false}}' -- '{}'"#,
            quoted(&take_over_prompt(ID, Path::new(brief)))
        )
    };
    let codex = format!(
        r#"codex -c check_for_update_on_startup=false -m '{astra}' -c 'model_reasoning_effort="high"' -- '{}'"#,
        quoted(&take_over_prompt(ID, Path::new(BRIEF)))
    );
    let not_listed = "opus'; rm -rf ~ #";
    let rows: Vec<LaunchRow<'_>> = vec![
        (
            Tool::ClaudeCode,
            opus,
            "high",
            BRIEF,
            Ok(claude(BRIEF, "Read(//data/handover/briefs/h-14.md)")),
        ),
        (
            Tool::ClaudeCode,
            opus,
            "high",
            ODD,
            Ok(claude(ODD, r"Read(//data/it'\''s/h-14.md)")),
        ),
        (Tool::Codex, astra, "high", BRIEF, Ok(codex)),
        (
            Tool::ClaudeCode,
            not_listed,
            "high",
            BRIEF,
            Err(format!("{not_listed} is not a model the app can start")),
        ),
        (
            Tool::Codex,
            not_listed,
            "high",
            BRIEF,
            Err(format!("{not_listed} is not a model the app can start")),
        ),
        (
            Tool::ClaudeCode,
            opus,
            "max",
            BRIEF,
            Err(format!("{opus} does not support max effort")),
        ),
        (
            Tool::Codex,
            astra,
            "max",
            BRIEF,
            Err(format!("{astra} does not support max effort")),
        ),
        (
            Tool::Codex,
            astra,
            "hi\"gh",
            BRIEF,
            Err("that effort cannot be passed to Codex".to_string()),
        ),
    ];
    for (to, model, effort, brief, want) in rows {
        let listed = match to {
            Tool::ClaudeCode => options(ToolKind::ClaudeCode, opus),
            Tool::Codex => options(ToolKind::Codex, astra),
        };
        let start = StartWith::Chosen {
            model: model.to_string(),
            effort: effort.to_string(),
        };
        let got = receiver_command(
            to,
            &start,
            &listed,
            &take_over_prompt(ID, Path::new(brief)),
            "/bin/zsh",
            &with_leave(to, Launch::default(), Path::new(brief)),
        );
        assert_eq!(got, want, "{to:?} {model} {effort} {brief}");
        if let Ok(command) = got {
            assert!(!command.contains("/home/code/app"), "{command}");
            assert!(
                command.contains(&format!("-- 'Take over from {ID}. ")),
                "{command}"
            );
        }
    }
    assert!(claude(ODD, "r").contains(r#"read the brief at "/data/it'\''s/h-14.md""#));
}

/// Take `id` over into a new session running `command`, past every check but the launch.
fn beside(fx: &Fixture, id: &str, to: Tool, command: &str) -> (Result<(), String>, PathBuf) {
    let planned = plan(fx, id, to).expect("planned");
    let brief_id = fx.office.brief_id(fx.host());
    let path = brief::write(&fx.host().briefs, &brief_id, "brief", &fx.root()).expect("brief");
    let prepared = Prepared {
        command: command.to_string(),
        brief: path.clone(),
        route: None,
    };
    (fx.office.start_beside(&planned, brief_id, prepared), path)
}

fn listed(fx: &Fixture) -> Vec<TakeOver> {
    fx.office.lock().handover.view.take_overs.clone()
}

#[test]
fn the_old_session_is_untouched_and_the_new_one_is_listed_and_bound() {
    let fx = fixture("take-over-beside", NOTHING);
    let old = fx.desk("s-old", ToolKind::ClaudeCode);
    let pid = fx.office.terminals.pid(&old).expect("pid");
    let screen = || fx.office.terminals.backlog(&old).expect("backlog").data;
    let before = screen();
    let (started, path) = beside(&fx, "s-old", Tool::Codex, "/bin/cat");
    assert_eq!(started, Ok(()));
    assert!(path.exists());
    assert!(fx.stops().is_empty());
    assert!(pid_alive(pid));
    assert_eq!(
        fx.office.terminals.wait_exit(&old, Duration::ZERO),
        Ok(None)
    );
    assert_eq!(screen(), before);
    let new = {
        let inner = fx.office.lock();
        assert_eq!(inner.owned.get(&old).map(String::as_str), Some("s-old"));
        assert!(inner.handover.underway.is_empty());
        assert!(inner.handover.book.done().is_empty());
        assert!(inner.handover.book.own("s-old", now()).is_none());
        assert!(inner.handover.book.live(Tool::ClaudeCode, now()).is_none());
        let new = inner.handover.take_overs[0].1.clone();
        assert_eq!(
            inner.handover.handed_from().get(&new).map(String::as_str),
            Some("s-old")
        );
        new
    };
    assert_ne!(new, old);
    let first = listed(&fx);
    assert_eq!(first.len(), 1);
    assert_eq!(
        (
            &*first[0].from_session,
            first[0].from,
            first[0].to,
            &first[0].to_session
        ),
        ("s-old", Tool::ClaudeCode, Tool::Codex, &None)
    );
    assert_eq!(fx.office.snapshot().handover.take_overs, first);

    {
        let mut inner = fx.office.lock();
        inner.owned.insert(new, "s-new".to_string());
        inner
            .found
            .sessions
            .push(session("s-new", ToolKind::Codex, &fx.floor.id));
    }
    assert!(fx.office.handover_due().changed);
    assert_eq!(listed(&fx)[0].to_session.as_deref(), Some("s-new"));

    let (again, _) = beside(&fx, "s-old", Tool::ClaudeCode, "/bin/cat");
    assert_eq!(again, Ok(()));
    let both = listed(&fx);
    assert_eq!(both.len(), 2);
    assert_eq!(
        (both[0].to, both[0].to_session.as_deref()),
        (Tool::ClaudeCode, None)
    );
    assert_ne!(both[0].id, both[1].id);
    assert_eq!(both[1].to_session.as_deref(), Some("s-new"));
    assert!(fx.stops().is_empty());
    assert!(pid_alive(pid));
    fx.done();
}

#[test]
fn a_used_up_desk_keeps_its_offer_after_a_take_over() {
    let fx = fixture("take-over-offer", OFF);
    fx.desk("s-old", ToolKind::ClaudeCode);
    let reset = now() + 3600;
    fx.transcript("s-old", &[&limit_line(reset)]);
    fx.office.handover_due();
    fx.settle("s-old");
    fx.office.handover_due();
    let offer = fx.state("s-old");
    assert_eq!(
        offer,
        Some(offered(
            crate::handover::view::OfferWhy::AutomaticOff,
            Work::Unknown,
            reset
        ))
    );
    let (started, _) = beside(&fx, "s-old", Tool::Codex, "/bin/cat");
    assert_eq!(started, Ok(()));
    fx.office.handover_due();
    assert_eq!(fx.state("s-old"), offer);
    assert!(fx.office.lock().handover.book.done().is_empty());
    assert!(fx.stops().is_empty());
    fx.done();
}

#[test]
fn a_start_that_fails_removes_the_brief_and_lists_nothing() {
    let fx = fixture("take-over-fails", NOTHING);
    fx.desk("s-old", ToolKind::ClaudeCode);
    let (got, path) = beside(&fx, "s-old", Tool::Codex, "/usr/bin/false");
    let why = got.expect_err("failed");
    assert!(why.starts_with("Codex did not start: "), "{why}");
    assert!(!path.exists());
    assert!(listed(&fx).is_empty());
    assert!(fx.office.lock().handover.take_overs.is_empty());
    assert!(fx.stops().is_empty());
    fx.done();
}

#[test]
fn the_take_over_code_stops_signals_and_types_nothing() {
    let text = include_str!("take_over.rs");
    for word in [
        "host.stop",
        "change_hands",
        "terminate",
        "libc::",
        "SIGTERM",
        "SIGKILL",
        "SIGHUP",
        "terminals.write",
        "structured.prompt",
        "underway.insert",
        "wait_exit",
    ] {
        assert!(!text.contains(word), "take_over.rs holds {word}");
    }
}
