//! The office side of the handover, with fakes only: a `FakeRunner` that runs nothing,
//! `/bin/cat` in a terminal standing in for Claude Code, a stop closure that records the pids
//! it is given, and transcripts written under a temp home. No pass here calls `refresh` or
//! `hire_options`.

use super::fixture::*;
use super::run::receiver_command;
use super::*;
use crate::discovery::claude_code::pid_alive;
use crate::handover::start::first_prompt;
use crate::handover::view::OfferWhy;
use crate::handover::{Task, TaskKind, Window};
use crate::model::{EffortOption, EndedSession, ModelOption, ToolKind, ToolOptions};
use crate::office::manager::Seat;
use crate::setup::Launch;
use std::time::Instant;

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
        project_id: fx.floor.id.clone(),
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
        ("office/handover/desk.rs", include_str!("desk.rs")),
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
        let got = receiver_command(
            Tool::Codex,
            &start,
            &codex_options(),
            prompt,
            "/bin/zsh",
            &setup,
        );
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
    assert_eq!(refused("s-one"), Err(choose(Tool::Codex)), "nothing chosen");
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
