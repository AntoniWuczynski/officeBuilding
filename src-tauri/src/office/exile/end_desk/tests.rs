//! `end_desk` with fakes only: `/bin/cat`, `/bin/sh` and `sleep` in app terminals stand in for
//! agents, and a recording ender says which pid each signal went to.

use super::*;
use crate::discovery::claude_code::pid_alive;
use crate::manager::auth::Registry;
use crate::manager::wire::{LOG_SHOWN, LOG_TEXT_MAX, ManagerFloor, NoteState};
use crate::model::{ControlMode, Project, SessionState, Shell, ToolKind};
use crate::office::deliver::tests::{queue, seat as desk_at};
use crate::office::launch::{Desk, desk_session, desk_session_id};
use crate::office::manager::tests::{bind, manager_office, seat, session, wait_for};
use crate::structured;
use portable_pty::CommandBuilder;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::{Arc, Mutex};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TestEnded {
    outcome: EndHow,
    desk: String,
    title: String,
    detail: String,
}

type Signals = Arc<Mutex<Vec<(&'static str, i32)>>>;

/// An ender that records every signal, and sends a real one where it is asked to.
fn recorder(real_stop: bool, real_force: bool, stop_wait: Duration) -> (DeskEnder, Signals) {
    let signals: Signals = Arc::default();
    let (stops, forces) = (Arc::clone(&signals), Arc::clone(&signals));
    let ender = DeskEnder {
        stop: Box::new(move |pid| {
            stops.lock().unwrap().push(("stop", pid));
            if real_stop {
                terminate(pid);
            }
        }),
        force: Box::new(move |pid| {
            forces.lock().unwrap().push(("force", pid));
            !real_force || force_own_child(pid)
        }),
        stop_wait,
        force_wait: FORCE_WAIT,
    };
    (ender, signals)
}

fn end(
    office: &Office,
    floor: &str,
    desk: &str,
    reason: &str,
    ender: &DeskEnder,
) -> Result<TestEnded, String> {
    let args = EndDesk {
        desk: desk.to_string(),
        reason: reason.to_string(),
    };
    office
        .end_desk_through(floor, &args, ender)
        .map(|answer| serde_json::from_str(&answer).unwrap())
}

fn runs(office: &Office, terminal: &str) -> bool {
    office.terminals.wait_exit(terminal, Duration::ZERO) == Ok(None)
}

fn newest_kind(office: &Office, floor: &str) -> Option<&'static str> {
    Office::manager_record(&office.lock(), floor)
        .log
        .last()
        .map(|entry| match entry.event {
            LogEvent::Note { .. } => "note",
            LogEvent::Hire { .. } => "hire",
            LogEvent::Escalation { .. } => "escalation",
            LogEvent::Refused { .. } => "refused",
            LogEvent::End { .. } => "end",
        })
}

fn entry(office: &Office, floor: &str) -> ManagerFloor {
    office
        .snapshot()
        .managers
        .into_iter()
        .find(|m| m.project_id == floor)
        .unwrap()
}

/// Every process the office's terminals still run gets SIGTERM, and the test waits for each.
fn finish(office: &Office, dir: &Path) {
    for (pid, terminal) in office.terminals.running_children() {
        terminate(pid);
        force_own_child(pid);
        let _ = office
            .terminals
            .wait_exit(&terminal, Duration::from_secs(5));
    }
    std::fs::remove_dir_all(dir).unwrap();
}

/// An office with a floor whose manager sits in a `/bin/cat` terminal, a second floor, and a
/// bystander desk on the first.
struct Rig {
    office: Office,
    dir: PathBuf,
    floor: Project,
    other: Project,
    registry: Arc<Registry>,
    manager: String,
    bystander: String,
}

fn rig(tag: &str) -> Rig {
    let (office, dir, floor, registry, _) = manager_office(tag);
    let other_root = dir.join("home/code/other");
    std::fs::create_dir_all(&other_root).unwrap();
    // Adding a floor rescans, so it comes before any session is bound by hand.
    let other = office.add_floor(&other_root.to_string_lossy()).unwrap();
    let (manager, _) = seat(&office, &dir, &floor, &registry, "/bin/cat");
    bind(&office, &manager, "s-manager", &floor.id);
    let (bystander, _) = desk_at(
        &office,
        &dir,
        &floor,
        ToolKind::ClaudeCode,
        SessionState::Running,
    );
    Rig {
        office,
        dir,
        floor,
        other,
        registry,
        manager,
        bystander,
    }
}

fn cat(office: &Office, dir: &Path) -> String {
    office
        .terminals
        .spawn(CommandBuilder::new("/bin/cat"), &dir.join("home/code/app"))
        .unwrap()
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

#[derive(Clone, Copy, Debug)]
enum Target {
    Cat,
    Launched,
    OtherFloor,
    Outside,
    Structured,
    Manager,
    OtherManager,
    Reviewer,
    HandedOver,
    Exited,
    Shell,
    ClaudeInShell,
    TakenOver,
    BeingEnded,
    Paused,
    NoSeat,
}

/// What a row's target is: the id the manager names and the terminal it runs in.
struct Aimed {
    id: String,
    terminal: Option<String>,
}

/// A terminal desk of a tool discovery cannot see, as `Launched::Desk` records one.
fn launched_desk(r: &Rig) -> (String, String) {
    let terminal = cat(&r.office, &r.dir);
    let launched = Desk {
        tool: ToolKind::Antigravity,
        project_id: r.floor.id.clone(),
        root: r.dir.join("home/code/app"),
        title: "write docs".to_string(),
        model: "m".to_string(),
        effort: "high".to_string(),
    };
    let id = desk_session_id(&terminal);
    let mut inner = r.office.lock();
    inner.owned.insert(terminal.clone(), id.clone());
    inner.ran_in.insert(id.clone(), terminal.clone());
    let s = desk_session(&terminal, &launched, Duration::ZERO);
    inner.found.sessions.push(s);
    inner.desks.insert(terminal.clone(), launched);
    (terminal, id)
}

/// A desk in a tool's chat view on the rig's floor. Its id.
fn structured_desk(r: &Rig) -> String {
    let info = structured::desks::DeskInfo {
        tool: ToolKind::Codex,
        project_id: r.floor.id.clone(),
        root: r.dir.join("home/code/app"),
        title: "in app".to_string(),
        model: "default".to_string(),
        effort: "default".to_string(),
    };
    r.office
        .structured
        .seat("structured-1", info, SessionState::Idle, Box::new(Quiet));
    "structured-1".to_string()
}

/// The human opened `terminal` as a plain shell.
fn hand_open(r: &Rig, terminal: &str) {
    let shell = Shell {
        id: terminal.to_string(),
        project_id: r.floor.id.clone(),
        number: 1,
        started_at: "2026-11-20T10:00:00Z".to_string(),
    };
    r.office.lock().shells.insert(terminal.to_string(), shell);
}

fn aim(r: &Rig, target: Target) -> Aimed {
    let (office, dir) = (&r.office, &r.dir);
    let desk = |floor: &Project| {
        desk_at(
            office,
            dir,
            floor,
            ToolKind::ClaudeCode,
            SessionState::Running,
        )
    };
    let one = |(terminal, id): (String, String)| Aimed {
        id,
        terminal: Some(terminal),
    };
    match target {
        Target::Cat | Target::Paused => one(desk(&r.floor)),
        Target::OtherFloor | Target::NoSeat => one(desk(&r.other)),
        Target::Launched => one(launched_desk(r)),
        Target::Outside => {
            let s = session("s-hand", &r.floor.id, ControlMode::RaiseWindow);
            office.lock().found.sessions.push(s);
            Aimed {
                id: "s-hand".to_string(),
                terminal: None,
            }
        }
        Target::Structured => Aimed {
            id: structured_desk(r),
            terminal: None,
        },
        Target::Manager => Aimed {
            id: "s-manager".to_string(),
            terminal: Some(r.manager.clone()),
        },
        Target::OtherManager => {
            let (theirs, _) = seat(office, dir, &r.other, &r.registry, "/bin/cat");
            bind(office, &theirs, "s-their-manager", &r.other.id);
            one((theirs, "s-their-manager".to_string()))
        }
        Target::Reviewer => {
            let terminal = cat(office, dir);
            bind(office, &terminal, "s-reviewer", &r.floor.id);
            office
                .lock()
                .reviewers
                .insert(terminal.clone(), r.floor.id.clone());
            one((terminal, "s-reviewer".to_string()))
        }
        Target::HandedOver => {
            let aimed = one(desk(&r.floor));
            office.lock().handover.underway.insert(aimed.id.clone());
            aimed
        }
        Target::Exited => {
            let (terminal, id) = desk(&r.floor);
            terminate(office.terminals.pid(&terminal).unwrap());
            wait_for("the desk to exit", || !runs(office, &terminal));
            Aimed { id, terminal: None }
        }
        Target::Shell => {
            let terminal = cat(office, dir);
            hand_open(r, &terminal);
            one((terminal.clone(), terminal))
        }
        Target::ClaudeInShell => {
            let aimed = one(desk(&r.floor));
            let terminal = aimed.terminal.clone().unwrap();
            hand_open(r, &terminal);
            aimed
        }
        Target::TakenOver => {
            let aimed = one(desk(&r.floor));
            office.lock().handover.taking_over.insert(aimed.id.clone());
            aimed
        }
        Target::BeingEnded => {
            let aimed = one(desk(&r.floor));
            if let Some(terminal) = &aimed.terminal {
                let mut inner = office.lock();
                inner
                    .seats
                    .get_mut(&r.floor.id)
                    .unwrap()
                    .ending
                    .insert(terminal.clone());
            }
            aimed
        }
    }
}

enum Expect {
    Ended(EndHow),
    Refused(&'static str),
}

struct Row {
    target: Target,
    reason: String,
    /// The stop sends a real SIGTERM; otherwise the force does, after a 200 ms wait.
    polite: bool,
    expect: Expect,
    /// The kind of the floor's newest log entry afterwards.
    logged: Option<&'static str>,
}

fn row(target: Target, expect: Expect, logged: Option<&'static str>) -> Row {
    Row {
        target,
        reason: "It ignored two notes.".to_string(),
        polite: true,
        expect,
        logged,
    }
}

#[test]
fn end_desk_reaches_exactly_the_desks_the_building_started_on_its_floor() {
    use Expect::{Ended, Refused};
    let refused = Some("refused");
    let rows = [
        row(Target::Cat, Ended(EndHow::Stopped), Some("end")),
        Row {
            polite: false,
            ..row(Target::Cat, Ended(EndHow::Forced), Some("end"))
        },
        row(Target::Launched, Ended(EndHow::Stopped), Some("end")),
        row(Target::OtherFloor, Refused(NO_SUCH_DESK), refused),
        row(Target::Outside, Refused(OUTSIDE), refused),
        row(Target::Structured, Refused(CHAT_VIEW), refused),
        row(Target::Manager, Refused(NO_SUCH_DESK), refused),
        row(Target::OtherManager, Refused(NO_SUCH_DESK), refused),
        row(Target::Reviewer, Refused(NO_SUCH_DESK), refused),
        row(Target::HandedOver, Refused(MOVING), refused),
        row(Target::Exited, Refused(NO_SUCH_DESK), refused),
        row(Target::Shell, Refused(NO_SUCH_DESK), refused),
        row(Target::ClaudeInShell, Refused(OUTSIDE), refused),
        row(Target::TakenOver, Refused(MOVING), refused),
        row(Target::BeingEnded, Refused(ALREADY), refused),
        row(Target::Paused, Refused(PAUSED), refused),
        row(
            Target::NoSeat,
            Refused("your floor has no manager seat"),
            None,
        ),
        Row {
            reason: "  \n ".to_string(),
            ..row(
                Target::Cat,
                Refused("give the reason in one line: there is nothing to send"),
                refused,
            )
        },
        Row {
            reason: "x".repeat(REASON_MAX + 1),
            ..row(
                Target::Cat,
                Refused("give the reason in one line: that is longer than 300 characters"),
                refused,
            )
        },
    ];
    for (n, row) in rows.into_iter().enumerate() {
        check(n + 1, &row);
    }
}

/// One row of the reach table, on an office of its own.
fn check(n: usize, row: &Row) {
    use Expect::{Ended, Refused};
    let what = format!("row {n} ({:?})", row.target);
    let r = rig(&format!("end-reach-{n}"));
    let aimed = aim(&r, row.target);
    let floor = match row.target {
        Target::NoSeat => r.other.id.clone(),
        _ => r.floor.id.clone(),
    };
    if matches!(row.target, Target::Paused) {
        r.office.set_manager_paused(&r.floor.id, true).unwrap();
    }
    let (ender, signals) = if row.polite {
        recorder(true, false, STOP_WAIT)
    } else {
        recorder(false, true, Duration::from_millis(200))
    };
    let pid = aimed
        .terminal
        .as_ref()
        .map(|t| r.office.terminals.pid(t).unwrap());
    let answer = end(&r.office, &floor, &aimed.id, &row.reason, &ender);
    let signalled = signals.lock().unwrap().clone();
    let mut keep = vec![r.manager.clone(), r.bystander.clone()];
    match row.expect {
        Ended(how) => {
            let answer = answer.unwrap_or_else(|e| panic!("{what}: {e}"));
            let pid = pid.unwrap();
            assert_eq!(
                (answer.outcome, answer.desk.as_str()),
                (how, aimed.id.as_str()),
                "{what}"
            );
            let want: Vec<(&str, i32)> = match how {
                EndHow::Stopped => vec![("stop", pid)],
                EndHow::Forced => vec![("stop", pid), ("force", pid)],
            };
            assert_eq!(signalled, want, "{what}");
            assert!(
                !runs(&r.office, aimed.terminal.as_deref().unwrap()),
                "{what}"
            );
            assert!(!answer.title.is_empty() && answer.detail.contains("sign-out sheet"));
        }
        Refused(words) => {
            assert_eq!(answer.err().as_deref(), Some(words), "{what}");
            assert!(signalled.is_empty(), "{what}: {signalled:?}");
            keep.extend(aimed.terminal.clone());
        }
    }
    assert_eq!(newest_kind(&r.office, &floor), row.logged, "{what}");
    for terminal in &keep {
        assert!(runs(&r.office, terminal), "{what}: {terminal} still runs");
    }
    // Only the row that put its desk in `ending` by hand finds it there afterwards.
    let still_ending = aimed
        .terminal
        .as_ref()
        .is_some_and(|t| Office::being_ended(&r.office.lock(), t));
    assert_eq!(
        still_ending,
        matches!(row.target, Target::BeingEnded),
        "{what}"
    );
    finish(&r.office, &r.dir);
}

#[test]
fn a_desk_that_ignores_the_polite_stop_is_forced_and_nothing_else_is_touched() {
    let r = rig("end-real-force");
    let mut cmd = CommandBuilder::new("/bin/sh");
    cmd.args(["-c", "trap '' TERM; exec sleep 30"]);
    let terminal = r
        .office
        .terminals
        .spawn(cmd, &r.dir.join("home/code/app"))
        .unwrap();
    bind(&r.office, &terminal, "s-stubborn", &r.floor.id);
    let pid = r.office.terminals.pid(&terminal).unwrap();
    wait_for("sleep to run", || {
        crate::focus::process_name(pid).as_deref() == Some("sleep")
    });
    let ender = DeskEnder {
        stop_wait: Duration::from_millis(300),
        ..DeskEnder::system()
    };
    let answer = end(
        &r.office,
        &r.floor.id,
        "s-stubborn",
        "It ignored two notes.",
        &ender,
    )
    .unwrap();
    assert_eq!(answer.outcome, EndHow::Forced);
    assert!(answer.detail.starts_with("It did not stop when asked"));
    assert!(!runs(&r.office, &terminal));
    assert!(!pid_alive(pid));
    assert!(runs(&r.office, &r.bystander) && runs(&r.office, &r.manager));
    finish(&r.office, &r.dir);
}

#[test]
fn a_desk_that_heeds_the_polite_stop_is_stopped() {
    let r = rig("end-real-stop");
    let (terminal, id) = desk_at(
        &r.office,
        &r.dir,
        &r.floor,
        ToolKind::ClaudeCode,
        SessionState::Running,
    );
    let answer = end(
        &r.office,
        &r.floor.id,
        &id,
        "Stuck on one file.",
        &DeskEnder::system(),
    )
    .unwrap();
    assert_eq!(answer.outcome, EndHow::Stopped);
    assert!(answer.detail.starts_with("It stopped when asked"));
    assert!(!runs(&r.office, &terminal));
    assert!(runs(&r.office, &r.bystander) && runs(&r.office, &r.manager));
    finish(&r.office, &r.dir);
}

#[test]
fn own_running_child_is_true_only_for_a_running_unreaped_child() {
    let pid = |child: &Child| i32::try_from(child.id()).unwrap();
    let mut running = Command::new("/bin/sleep").arg("30").spawn().unwrap();
    let mut exited = Command::new("/usr/bin/true").spawn().unwrap();
    let parent = i32::try_from(std::os::unix::process::parent_id()).unwrap();
    let rows = [
        ("a running child", pid(&running), true),
        ("pid 1", 1, false),
        ("the test's own parent", parent, false),
        ("0", 0, false),
        ("-1", -1, false),
    ];
    for (what, pid, want) in rows {
        assert_eq!(own_running_child(pid), want, "{what}");
    }
    let gone = pid(&exited);
    wait_for("true to exit", || !own_running_child(gone));
    // Not reaped yet: reaping it now still finds it.
    assert!(
        exited.try_wait().unwrap().is_some(),
        "an exited child, not yet reaped"
    );
    assert!(!own_running_child(gone), "a reaped child");
    running.kill().unwrap();
    running.wait().unwrap();
    assert!(
        !own_running_child(pid(&running)),
        "a killed and reaped child"
    );
}

#[test]
fn an_ended_desk_is_recorded_and_its_waiting_note_dropped() {
    let r = rig("end-after");
    let (office, floor) = (&r.office, &r.floor);
    let (first, first_id) = desk_at(
        office,
        &r.dir,
        floor,
        ToolKind::ClaudeCode,
        SessionState::Running,
    );
    let (second, second_id) = desk_at(
        office,
        &r.dir,
        floor,
        ToolKind::ClaudeCode,
        SessionState::Running,
    );
    let note = queue(office, floor, &first_id, "Take TODO.md.");
    office.deliver_due();
    assert!(
        office.lock().mail.outbox.contains(&note),
        "it waits for the busy desk"
    );

    let system = DeskEnder::system();
    end(office, &floor.id, &first_id, "first", &system).unwrap();
    assert!(!Office::being_ended(&office.lock(), &first));
    // The ending's refresh rescans the temp home, which knows no session: bind it again.
    bind(office, &second, &second_id, &floor.id);
    end(office, &floor.id, &second_id, "second", &system).unwrap();
    office.deliver_due();
    let state = Office::manager_record(&office.lock(), &floor.id)
        .log
        .iter()
        .find_map(|e| match &e.event {
            LogEvent::Note { note_id, state, .. } if *note_id == note => Some(*state),
            _ => None,
        });
    assert_eq!(state, Some(NoteState::Dropped));

    // The whole kept log feeds `ended`, not only the entries shown.
    {
        let mut inner = office.lock();
        for n in 0..=LOG_SHOWN {
            let refused = LogEvent::Refused {
                tool: TOOL.to_string(),
                reason: format!("r{n}"),
            };
            log_event(&mut inner, &floor.id, refused);
        }
    }
    let shown = entry(office, &floor.id);
    assert!(
        shown
            .log
            .iter()
            .all(|e| !matches!(e.event, LogEvent::End { .. }))
    );
    let ended: Vec<(&str, &str, EndHow)> = shown
        .ended
        .iter()
        .map(|e| (e.session_id.as_str(), e.reason.as_str(), e.how))
        .collect();
    assert_eq!(
        ended,
        [
            (second_id.as_str(), "second", EndHow::Stopped),
            (first_id.as_str(), "first", EndHow::Stopped)
        ]
    );
    assert!(shown.ended[0].at >= shown.ended[1].at);

    {
        let mut inner = office.lock();
        let long = LogEvent::End {
            session_id: "s-long".to_string(),
            title: "t".repeat(LOG_TEXT_MAX + 5),
            reason: "y".repeat(LOG_TEXT_MAX + 50),
            how: EndHow::Forced,
        };
        log_event(&mut inner, &floor.id, long);
        let last = Office::manager_record(&inner, &floor.id).log.pop().unwrap();
        assert!(matches!(&last.event, LogEvent::End { title, reason, .. }
            if title.chars().count() == LOG_TEXT_MAX && reason.chars().count() == LOG_TEXT_MAX));
        inner
            .seats
            .get_mut(&floor.id)
            .unwrap()
            .ending
            .insert(r.bystander.clone());
        assert!(Office::being_ended(&inner, &r.bystander));
    }
    finish(office, &r.dir);
}
