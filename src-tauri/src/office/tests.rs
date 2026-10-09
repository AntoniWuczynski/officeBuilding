use super::*;
use crate::model::{EffortOption, ModelOption};

fn office(tag: &str) -> (Office, PathBuf) {
    let dir = std::env::temp_dir().join(format!("ob-office-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(dir.join("home")).expect("mkdir");
    let store = Store::open(dir.join("store.json")).expect("store");
    (
        Office::new(
            dir.join("home"),
            store,
            Terminals::new(std::sync::Arc::new(|_| {})),
            Library::new(dir.join("Setup"), dir.join("setup-build")),
            Feed::new(dir.join("feed")),
            crate::structured::desks::Desks::new(std::sync::Arc::new(|_| {})),
        ),
        dir,
    )
}

#[test]
fn a_pinned_floor_appears_with_no_desks_and_survives_a_rescan() {
    let (office, dir) = office("pin");
    let repo = dir.join("home/code/newapp");
    std::fs::create_dir_all(repo.join(".git")).expect("mkdir");
    std::fs::create_dir_all(repo.join("src")).expect("mkdir");
    // Picking a subfolder pins its git root.
    let p = office
        .add_floor(&repo.join("src").to_string_lossy())
        .expect("add");
    assert_eq!(p.name, "newapp");
    assert_eq!(p.path, "~/code/newapp");
    assert!(p.session_ids.is_empty());
    office.refresh();
    assert!(office.snapshot().projects.iter().any(|x| x.id == p.id));
    // Adding it again is not an error and does not duplicate it.
    let again = office.add_floor("~/code/newapp").expect("add again");
    assert_eq!(again.id, p.id);
    assert_eq!(
        office
            .snapshot()
            .projects
            .iter()
            .filter(|x| x.id == p.id)
            .count(),
        1
    );
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn rejects_relative_and_missing_folders() {
    let (office, dir) = office("bad");
    assert!(
        office
            .add_floor("code/app")
            .unwrap_err()
            .contains("full path")
    );
    assert!(
        office
            .add_floor("/definitely/not/here")
            .unwrap_err()
            .contains("not a folder")
    );
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

fn session(id: &str, control: ControlMode) -> Session {
    Session {
        id: id.to_string(),
        tool: ToolKind::ClaudeCode,
        project_id: "p".to_string(),
        title: id.to_string(),
        state: SessionState::Running,
        control,
        model: None,
        effort: None,
        last_activity_at: String::new(),
        pending_question_ids: Vec::new(),
        spend: Spend::default(),
        helpers: Vec::new(),
        worktree: false,
    }
}

fn pids<'a>(pairs: &'a [(&'a str, i32)]) -> impl Fn(&str) -> Option<i32> + 'a {
    move |id| pairs.iter().find(|(s, _)| *s == id).map(|(_, p)| *p)
}

/// 300 was started by a launcher (200) that we spawned; 500 is unrelated.
fn parents(pid: i32) -> Option<i32> {
    match pid {
        300 => Some(200),
        200 | 500 => Some(1),
        _ => None,
    }
}

#[test]
fn finds_the_terminal_by_pid_or_ancestor() {
    let children = BTreeMap::from([(100, "t-1".to_string()), (200, "t-2".to_string())]);
    assert_eq!(
        owning_terminal(100, &children, parents),
        Some(("t-1".to_string(), 0))
    );
    assert_eq!(
        owning_terminal(300, &children, parents),
        Some(("t-2".to_string(), 1))
    );
    assert_eq!(owning_terminal(500, &children, parents), None);
    // A deep tree gives up rather than walking forever.
    assert_eq!(owning_terminal(1000, &children, |p| Some(p + 1)), None);
}

#[test]
fn sessions_in_our_terminals_get_full_control() {
    let children = BTreeMap::from([(100, "t-1".to_string())]);
    let mut owned = BTreeMap::new();
    let mut sessions = vec![
        session("mine", ControlMode::RaiseWindow),
        session("theirs", ControlMode::RaiseWindow),
        session("old", ControlMode::ReadOnly),
    ];
    claim_owned(
        &mut sessions,
        &mut owned,
        &children,
        pids(&[("mine", 100), ("theirs", 500)]),
        parents,
    );
    let controls: Vec<ControlMode> = sessions.iter().map(|s| s.control).collect();
    assert_eq!(
        controls,
        vec![
            ControlMode::Full,
            ControlMode::RaiseWindow,
            ControlMode::ReadOnly
        ]
    );
    assert_eq!(
        owned,
        BTreeMap::from([("t-1".to_string(), "mine".to_string())])
    );
}

#[test]
fn a_resumed_session_moves_to_its_new_terminal() {
    // The session first ran in t-3, which has exited; resuming it started t-5.
    let mut owned = BTreeMap::from([("t-3".to_string(), "mine".to_string())]);
    let children = BTreeMap::from([(100, "t-5".to_string())]);
    let mut sessions = vec![session("mine", ControlMode::RaiseWindow)];
    claim_owned(
        &mut sessions,
        &mut owned,
        &children,
        pids(&[("mine", 100)]),
        parents,
    );
    assert_eq!(
        owned,
        BTreeMap::from([("t-5".to_string(), "mine".to_string())])
    );
    assert_eq!(sessions[0].control, ControlMode::Full);
}

#[test]
fn a_finished_terminal_keeps_its_last_session_and_follows_a_new_one() {
    let mut owned = BTreeMap::from([("t-1".to_string(), "mine".to_string())]);
    // The agent exited: discovery reports it read-only and no terminal is running.
    let mut sessions = vec![session("mine", ControlMode::ReadOnly)];
    claim_owned(
        &mut sessions,
        &mut owned,
        &BTreeMap::new(),
        |_| None,
        parents,
    );
    assert_eq!(sessions[0].control, ControlMode::Full);

    // Same process, fresh session id: the terminal moves on to it.
    let children = BTreeMap::from([(100, "t-1".to_string())]);
    let mut sessions = vec![
        session("mine", ControlMode::ReadOnly),
        session("next", ControlMode::RaiseWindow),
    ];
    claim_owned(
        &mut sessions,
        &mut owned,
        &children,
        pids(&[("next", 100)]),
        parents,
    );
    assert_eq!(sessions[0].control, ControlMode::ReadOnly);
    assert_eq!(sessions[1].control, ControlMode::Full);
}

#[test]
fn a_hired_terminal_is_addressable_by_its_session() {
    // Nothing is read or saved here: the store starts empty for a missing file.
    let dir = std::env::temp_dir().join("ob-office-test-missing");
    let store = Store::open(dir.join("store.json")).expect("store");
    let setup = Library::new(dir.join("Setup"), dir.join("setup-build"));
    let office = Office::new(
        dir,
        store,
        Terminals::new(std::sync::Arc::new(|_| {})),
        setup,
        Feed::new(std::env::temp_dir().join("ob-office-test-missing-feed")),
        crate::structured::desks::Desks::new(std::sync::Arc::new(|_| {})),
    );
    let terminal = office
        .terminals
        .spawn(
            portable_pty::CommandBuilder::new("/bin/cat"),
            Path::new("/"),
        )
        .expect("spawn");
    assert!(office.terminal_backlog("s-1").is_err());
    office.lock().owned.insert(terminal, "s-1".to_string());
    office.terminal_resize("s-1", 90, 30).expect("resize");
    office.terminal_write("s-1", "\u{4}").expect("write");
    let backlog = office.terminal_backlog("s-1").expect("backlog");
    assert_eq!((backlog.cols, backlog.rows), (90, 30));
}

/// FAILS: a second live session whose process descends from our agent (e.g. an
/// interactive `claude` the agent itself launched) steals the terminal from the
/// agent we actually spawned: the binding goes to whichever session discovery
/// lists last, not the direct child and not the newest. The real agent drops to
/// raise-window (no window to raise) and its terminal becomes unreachable.
#[test]
fn verifier_a_nested_session_does_not_steal_the_agents_terminal() {
    let children = BTreeMap::from([(100, "t-1".to_string())]);
    let mut owned = BTreeMap::new();
    let mut sessions = vec![
        session("agent", ControlMode::RaiseWindow),
        session("nested", ControlMode::RaiseWindow),
    ];
    let parent_of = |p: i32| match p {
        300 => Some(100),
        100 => Some(50),
        _ => None,
    };
    claim_owned(
        &mut sessions,
        &mut owned,
        &children,
        pids(&[("agent", 100), ("nested", 300)]),
        parent_of,
    );
    assert_eq!(owned.get("t-1").map(String::as_str), Some("agent"));
    assert_eq!(sessions[0].control, ControlMode::Full);
}

/// FAILS: after our terminal's agent exits, its binding is kept forever and
/// forces `Full` even when the same session is later resumed live in another
/// terminal (`claude --resume <id>` in Terminal.app). Clicking the desk then
/// opens the dead in-app terminal (input refused) instead of raising the
/// window the session now lives in.
#[test]
fn verifier_a_session_resumed_elsewhere_is_not_forced_into_the_dead_terminal() {
    let mut owned = BTreeMap::from([("t-1".to_string(), "mine".to_string())]);
    let mut sessions = vec![session("mine", ControlMode::RaiseWindow)];
    // t-1 has exited (no running children); "mine" is now pid 777 under Terminal.app.
    claim_owned(
        &mut sessions,
        &mut owned,
        &BTreeMap::new(),
        pids(&[("mine", 777)]),
        |_| Some(1),
    );
    assert_eq!(sessions[0].control, ControlMode::RaiseWindow);
}

/// A hired Codex agent: `exec codex` keeps the pid of the process we started
/// (checked on this Mac, codex 2026-09: the PTY child pid is the `codex`
/// process holding its rollout open, so `CodexSource::pid_of` reports it).
/// Until its first turn opens a rollout there is no session to bind.
#[test]
fn a_hired_codex_agent_binds_once_its_rollout_is_open() {
    let children = BTreeMap::from([(4242, "t-1".to_string())]);
    let mut owned = BTreeMap::new();
    let mut before: Vec<Session> = Vec::new();
    claim_owned(&mut before, &mut owned, &children, |_| None, |_| None);
    assert!(owned.is_empty());

    let mut codex = session("019a-codex", ControlMode::RaiseWindow);
    codex.tool = ToolKind::Codex;
    let mut sessions = vec![codex];
    claim_owned(
        &mut sessions,
        &mut owned,
        &children,
        pids(&[("019a-codex", 4242)]),
        |_| None,
    );
    assert_eq!(sessions[0].control, ControlMode::Full);
    assert_eq!(owned.get("t-1").map(String::as_str), Some("019a-codex"));
}

/// A hired Codex sits on a pre-turn question with no rollout: discovery shows a
/// placeholder desk for its process, the terminal binds to it with full control,
/// and when the rollout appears the terminal moves to the real session id.
#[test]
fn a_hired_codex_without_a_rollout_gets_a_placeholder_desk_that_hands_over_to_the_real_session() {
    let children = BTreeMap::from([(4242, "t-1".to_string())]);
    let mut owned = BTreeMap::new();
    let codex = |id: &str| {
        let mut s = session(id, ControlMode::RaiseWindow);
        s.tool = ToolKind::Codex;
        s
    };

    let mut pending = vec![codex("codex-new-4242")];
    claim_owned(
        &mut pending,
        &mut owned,
        &children,
        pids(&[("codex-new-4242", 4242)]),
        |_| None,
    );
    assert_eq!(pending[0].control, ControlMode::Full);
    assert_eq!(owned.get("t-1").map(String::as_str), Some("codex-new-4242"));

    let mut real = vec![codex("019a-codex")];
    claim_owned(
        &mut real,
        &mut owned,
        &children,
        pids(&[("019a-codex", 4242)]),
        |_| None,
    );
    assert_eq!(real[0].control, ControlMode::Full);
    assert_eq!(owned.get("t-1").map(String::as_str), Some("019a-codex"));
}

#[test]
fn between_equally_close_sessions_the_most_recent_takes_the_terminal() {
    let children = BTreeMap::from([(100, "t-1".to_string())]);
    let mut owned = BTreeMap::new();
    let mut newer = session("newer", ControlMode::RaiseWindow);
    newer.last_activity_at = "2026-09-29T10:05:00Z".to_string();
    let mut older = session("older", ControlMode::RaiseWindow);
    older.last_activity_at = "2026-09-29T10:00:00Z".to_string();
    let mut sessions = vec![newer, older];
    let parent_of = |p: i32| matches!(p, 301 | 302).then_some(100);
    claim_owned(
        &mut sessions,
        &mut owned,
        &children,
        pids(&[("newer", 301), ("older", 302)]),
        parent_of,
    );
    assert_eq!(owned.get("t-1").map(String::as_str), Some("newer"));
    assert_eq!(sessions[1].control, ControlMode::RaiseWindow);
}

#[test]
fn a_panel_on_the_session_before_a_clear_still_reaches_the_terminal() {
    let (office, dir) = office("alias");
    let terminal = office
        .terminals
        .spawn(
            portable_pty::CommandBuilder::new("/bin/cat"),
            Path::new("/"),
        )
        .expect("spawn");
    {
        let mut inner = office.lock();
        inner
            .owned
            .insert(terminal.clone(), "after-clear".to_string());
        inner
            .ran_in
            .insert("before-clear".to_string(), terminal.clone());
    }
    assert_eq!(office.terminal_of("before-clear"), Ok(terminal.clone()));
    assert_eq!(office.terminal_of("after-clear"), Ok(terminal));
    office
        .terminal_write("before-clear", "\u{4}")
        .expect("write");
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn a_launch_that_fails_at_once_is_an_error_with_what_it_said() {
    let terminals = Terminals::new(std::sync::Arc::new(|_| {}));
    let err = start_agent(&terminals, Path::new("/"), "no-such-agent-ob005 --model x")
        .expect_err("fails");
    assert!(
        err.starts_with("the agent stopped as soon as it started (exit code 127): "),
        "got {err:?}"
    );
    assert!(err.contains("no-such-agent-ob005"), "got {err:?}");
    let running = start_agent(&terminals, Path::new("/"), "/bin/cat").expect("still running");
    terminals.write(&running, "\u{4}").expect("eot");
}

#[test]
fn hiring_onto_a_floor_whose_folder_is_gone_fails_clearly() {
    let (office, dir) = office("gone");
    let repo = dir.join("home/code/tilde~app");
    std::fs::create_dir_all(&repo).expect("mkdir");
    let floor = office.add_floor(&repo.to_string_lossy()).expect("add");
    std::fs::remove_dir_all(&repo).expect("rm");
    let req = HireRequest {
        tool: ToolKind::ClaudeCode,
        title: "x".to_string(),
        model: "claude-opus-5-5".to_string(),
        effort: "high".to_string(),
        ultracode: false,
        structured: false,
    };
    let err = office.spawn_session(&floor.id, &req).expect_err("gone");
    assert_eq!(
        err,
        format!("{} is not a folder on this Mac any more", repo.display())
    );
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

fn quiet_terminals() -> Terminals {
    Terminals::new(std::sync::Arc::new(|_| {}))
}

/// FAILS: the 1.5 s launch window is a fixed guess. A login shell whose profile
/// takes longer (nvm/conda/pyenv init) and then cannot find the agent exits after
/// the window, so the hire is reported as a success and the error lands in a
/// terminal no desk will ever open (round-1 F2 again).
#[test]
fn verifier2_a_launch_that_fails_after_a_slow_profile_fails_the_hire() {
    let t = quiet_terminals();
    let result = start_agent(
        &t,
        Path::new("/"),
        "/bin/sh -c 'sleep 2; echo \"zsh: command not found: claude\"; exit 127'",
    );
    assert!(result.is_err(), "hire reported as started: {result:?}");
}

/// FAILS: one refresh where `pid_of` misses our own live session (Claude's registry
/// is read a second time, apart from the scan that marked it live) counts it as
/// "live elsewhere": its binding and its `ran_in` entry are dropped, so the open
/// panel's writes fail with "was not started in the app" until a later refresh.
#[test]
fn verifier2_a_missed_pid_lookup_does_not_unbind_our_own_session() {
    let children = BTreeMap::from([(100, "t-1".to_string())]);
    let mut owned = BTreeMap::from([("t-1".to_string(), "mine".to_string())]);
    let mut sessions = vec![session("mine", ControlMode::RaiseWindow)];
    claim_owned(&mut sessions, &mut owned, &children, |_| None, |_| None);
    assert_eq!(owned.get("t-1").map(String::as_str), Some("mine"));
    assert_eq!(sessions[0].control, ControlMode::Full);
}

#[test]
fn a_hired_agent_that_exits_before_reaching_a_desk_is_reported_once() {
    let (office, dir) = office("failed");
    let script = portable_pty::CommandBuilder::from_argv(
        [
            "/bin/sh",
            "-c",
            "echo 'zsh: command not found: claude'; exit 127",
        ]
        .into_iter()
        .map(Into::into)
        .collect(),
    );
    let terminal = office
        .terminals
        .spawn(script, Path::new("/"))
        .expect("spawn");
    office
        .lock()
        .hired
        .insert(terminal.clone(), "fix the tests".to_string());
    let code = office
        .terminals
        .wait_exit(&terminal, Duration::from_secs(10))
        .expect("wait")
        .expect("exited");
    let exit = TerminalExit {
        terminal_id: terminal,
        exit_code: code,
    };
    assert_eq!(
        office.hire_failed(&exit),
        Some(HireFailed {
            title: "fix the tests".to_string(),
            exit_code: Some(127),
            last_lines: vec!["zsh: command not found: claude".to_string()]
        })
    );
    assert_eq!(office.hire_failed(&exit), None);
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn a_task_named_like_a_claude_command_is_refused() {
    let (office, dir) = office("subcommand");
    let repo = dir.join("home/code/app");
    std::fs::create_dir_all(&repo).expect("mkdir");
    let floor = office.add_floor(&repo.to_string_lossy()).expect("add");
    let req = HireRequest {
        tool: ToolKind::ClaudeCode,
        title: " doctor ".to_string(),
        model: "claude-opus-5-5".to_string(),
        effort: "high".to_string(),
        ultracode: false,
        structured: false,
    };
    assert_eq!(
        office.spawn_session(&floor.id, &req),
        Err(
            "“doctor” is also a claude command, so describe the task in a few more words"
                .to_string()
        )
    );
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn the_desk_of_a_session_before_a_clear_opens_its_terminal() {
    let ran_in = BTreeMap::from([("before-clear".to_string(), "t-1".to_string())]);
    let mut sessions = vec![
        session("before-clear", ControlMode::ReadOnly),
        session("other", ControlMode::ReadOnly),
    ];
    reopen_earlier_sessions(&mut sessions, &ran_in);
    assert_eq!(
        sessions.iter().map(|s| s.control).collect::<Vec<_>>(),
        vec![ControlMode::Full, ControlMode::ReadOnly]
    );
}

#[test]
fn resuming_runs_the_tools_own_resume_with_the_same_model_and_effort() {
    let ended = |tool, model: Option<&str>, effort: Option<&str>| EndedSession {
        id: "abc".to_string(),
        tool,
        project_id: "p".to_string(),
        title: "t".to_string(),
        ended_at: String::new(),
        model: model.map(str::to_string),
        effort: effort.map(str::to_string),
    };
    let zsh = "/bin/zsh";
    assert_eq!(
        resume_command(
            &ended(ToolKind::ClaudeCode, Some("claude-opus-5-5"), Some("high")),
            zsh,
            &Launch::default()
        ),
        Ok("claude --resume 'abc' --model 'claude-opus-5-5' --effort 'high'".to_string())
    );
    assert_eq!(
        resume_command(
            &ended(ToolKind::ClaudeCode, None, None),
            zsh,
            &Launch::default()
        ),
        Ok("claude --resume 'abc'".to_string())
    );
    assert_eq!(
        resume_command(
            &ended(ToolKind::Codex, Some("gpt-6-astra"), Some("xhigh")),
            zsh,
            &Launch::default()
        ),
        Ok("codex -c check_for_update_on_startup=false resume 'abc' -m 'gpt-6-astra' -c 'model_reasoning_effort=\"xhigh\"'".to_string())
    );
    assert!(
        resume_command(
            &ended(ToolKind::Cursor, None, None),
            zsh,
            &Launch::default()
        )
        .is_err()
    );
}

#[test]
fn only_an_ended_session_can_be_resumed() {
    let (office, dir) = office("resume-unknown");
    assert_eq!(
        office.resume_session("nope"),
        Err("nope has not signed out, so there is nothing to resume".to_string())
    );
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn a_demolished_floor_is_unpinned_and_hidden_until_it_is_added_again() {
    let (office, dir) = office("demolish");
    let repo = dir.join("home/code/oldapp");
    std::fs::create_dir_all(repo.join(".git")).expect("mkdir");
    let p = office.add_floor("~/code/oldapp").expect("add");
    office.demolish_floor(&p.id).expect("demolish");
    assert!(!office.snapshot().projects.iter().any(|x| x.id == p.id));
    office.refresh();
    assert!(
        !office.snapshot().projects.iter().any(|x| x.id == p.id),
        "it stays down"
    );
    assert!(
        office.demolish_floor(&p.id).is_err(),
        "there is nothing left to demolish"
    );
    office.add_floor("~/code/oldapp").expect("rebuild");
    office.refresh();
    assert!(
        office.snapshot().projects.iter().any(|x| x.id == p.id),
        "adding it rebuilds it"
    );
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn a_demolished_floor_stays_down_until_new_work_and_then_keeps_its_sign_out_sheet() {
    let root = "/code/app";
    let id = discovery::project_id(Path::new(root));
    let at = |id: &str, when: &str| Session {
        project_id: discovery::project_id(Path::new(root)),
        last_activity_at: when.to_string(),
        ..session(id, ControlMode::RaiseWindow)
    };
    let ended = |sid: &str, when: &str| EndedSession {
        id: sid.to_string(),
        tool: ToolKind::ClaudeCode,
        project_id: id.clone(),
        title: sid.to_string(),
        ended_at: when.to_string(),
        model: None,
        effort: None,
    };
    let floor = |session_ids: Vec<String>| Discovered {
        roots: BTreeMap::from([(id.clone(), PathBuf::from(root))]),
        projects: vec![Project {
            id: id.clone(),
            name: "app".to_string(),
            path: root.to_string(),
            session_ids,
        }],
        ..Discovered::default()
    };
    let demolished = BTreeMap::from([(root.to_string(), "2026-10-06T10:00:00.000Z".to_string())]);

    let mut before = floor(vec!["old".to_string()]);
    before.sessions = vec![at("old", "2026-10-06T09:59:59.900Z")];
    before.ended = vec![ended("gone", "2026-10-06T09:00:00Z")];
    before.resume_dirs = BTreeMap::from([("gone".to_string(), PathBuf::from(root))]);
    drop_demolished(&mut before, &demolished);
    assert!(
        before.projects.is_empty() && before.roots.is_empty(),
        "nothing newer: the floor stays down"
    );
    assert!(before.sessions.is_empty() && before.ended.is_empty() && before.resume_dirs.is_empty());

    let mut after = floor(vec!["old".to_string(), "new".to_string()]);
    after.sessions = vec![
        at("old", "2026-10-06T09:59:59Z"),
        at("new", "2026-10-06T10:00:00.001Z"),
    ];
    after.ended = vec![ended("stopped", "2026-10-06T09:59:58Z")];
    after.resume_dirs = BTreeMap::from([("stopped".to_string(), PathBuf::from(root))]);
    drop_demolished(&mut after, &demolished);
    assert_eq!(
        after.projects[0].session_ids,
        vec!["new".to_string()],
        "only the newer work takes a desk"
    );
    assert_eq!(after.sessions.len(), 1);
    let sheet: Vec<&str> = after.ended.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(
        sheet,
        vec!["stopped"],
        "a rebuilt floor's sheet keeps who left before"
    );
    assert!(
        after.resume_dirs.contains_key("stopped"),
        "and they can still be resumed"
    );
}

#[test]
fn stopping_agents_waits_for_them_and_reports_one_that_will_not_stop() {
    let spawn = |script: &str| {
        let mut child = std::process::Command::new("sh")
            .args(["-c", script])
            .spawn()
            .expect("spawn");
        let pid = i32::try_from(child.id()).expect("pid");
        // Reap it, as the app reaps its terminals, so an exit is not left a zombie.
        std::thread::spawn(move || child.wait());
        pid
    };
    let polite = spawn("exec sleep 30");
    stop_agents(&[("polite".to_string(), polite)], Duration::from_secs(5)).expect("stops");
    assert!(!discovery::claude_code::pid_alive(polite));

    let stubborn = spawn("trap '' TERM; exec sleep 30");
    std::thread::sleep(Duration::from_millis(200));
    let err = stop_agents(
        &[("stubborn".to_string(), stubborn)],
        Duration::from_millis(300),
    )
    .expect_err("ignores SIGTERM");
    assert!(err.contains("stubborn did not stop"), "{err}");
    // SAFETY: SIGKILL to the test's own child.
    #[expect(unsafe_code, reason = "cleaning up a test child that ignores SIGTERM")]
    unsafe {
        libc::kill(stubborn, libc::SIGKILL)
    };
}

const ZSH: &str = "/bin/zsh";

fn model(id: &str, efforts: &[&str]) -> ModelOption {
    ModelOption {
        id: id.to_string(),
        label: id.to_string(),
        efforts: efforts
            .iter()
            .map(|e| EffortOption {
                id: e.to_lowercase(),
                label: (*e).to_string(),
                description: String::new(),
            })
            .collect(),
        default_effort: efforts[0].to_lowercase(),
    }
}

fn hire(tool: ToolKind, model: &str, effort: &str, ultracode: bool) -> HireRequest {
    HireRequest {
        tool,
        title: String::new(),
        model: model.to_string(),
        effort: effort.to_string(),
        ultracode,
        structured: false,
    }
}

fn command(req: &HireRequest, efforts: &[&str], task: &str) -> String {
    launch_command(
        req,
        &model(&req.model, efforts),
        task,
        ZSH,
        &Launch::default(),
    )
    .expect("command")
}

#[test]
fn claude_code_gets_the_full_model_id_effort_and_the_ultracode_setting() {
    let on = hire(ToolKind::ClaudeCode, "claude-opus-5-5", "high", true);
    assert_eq!(
        command(&on, &["high"], "fix the tests"),
        r#"claude --model 'claude-opus-5-5' --effort 'high' --settings '{"ultracode":true}' -- 'fix the tests'"#
    );
    let off = hire(ToolKind::ClaudeCode, "claude-opus-5-5", "high", false);
    assert_eq!(
        command(&off, &["high"], "-x it's"),
        r#"claude --model 'claude-opus-5-5' --effort 'high' --settings '{"ultracode":false}' -- '-x it'\''s'"#
    );
}

#[test]
fn claude_code_omits_a_default_effort_and_an_empty_task() {
    let req = hire(ToolKind::ClaudeCode, "claude-sonnet-5", "default", false);
    assert_eq!(
        command(&req, &["default"], ""),
        r#"claude --model 'claude-sonnet-5' --settings '{"ultracode":false}'"#
    );
}

#[test]
fn codex_launches_as_before_and_ignores_ultracode() {
    let req = hire(ToolKind::Codex, "gpt-5.5", "low", true);
    assert_eq!(
        command(&req, &["low"], "go"),
        r#"codex -c check_for_update_on_startup=false -m 'gpt-5.5' -c 'model_reasoning_effort="low"' -- 'go'"#
    );
}

#[test]
fn codex_omits_the_model_and_effort_flags_when_they_are_default() {
    let req = hire(ToolKind::Codex, "default", "default", false);
    assert_eq!(
        command(&req, &["default"], "go"),
        "codex -c check_for_update_on_startup=false -- 'go'"
    );
    assert_eq!(
        command(&req, &["default"], ""),
        "codex -c check_for_update_on_startup=false"
    );
    let model_only = hire(ToolKind::Codex, "gpt-5.5", "default", false);
    assert_eq!(
        command(&model_only, &["default"], "go"),
        "codex -c check_for_update_on_startup=false -m 'gpt-5.5' -- 'go'"
    );
    let effort_only = hire(ToolKind::Codex, "default", "low", false);
    assert_eq!(
        command(&effort_only, &["low"], ""),
        r#"codex -c check_for_update_on_startup=false -c 'model_reasoning_effort="low"'"#
    );
}

#[test]
fn antigravity_gets_the_model_and_effort_as_one_quoted_name() {
    let req = hire(ToolKind::Antigravity, "Gemini 3.6 Flash", "high", true);
    assert_eq!(
        command(&req, &["High"], "refactor it's"),
        r"agy --model 'Gemini 3.6 Flash (High)' -i='refactor it'\''s'"
    );
    assert_eq!(
        command(&req, &["High"], "--help"),
        r"agy --model 'Gemini 3.6 Flash (High)' -i='--help'"
    );
    assert_eq!(
        command(&req, &["High"], ""),
        r"agy --model 'Gemini 3.6 Flash (High)'"
    );
    let plain = hire(ToolKind::Antigravity, "Some Model", "default", false);
    assert_eq!(
        command(&plain, &["Default"], "go"),
        r"agy --model 'Some Model' -i='go'"
    );
}

#[test]
fn opencode_and_cursor_start_with_only_the_model_and_no_prompt() {
    let open = hire(ToolKind::Opencode, "anthropic/claude", "default", false);
    assert_eq!(
        command(&open, &["Default"], "a task"),
        "opencode --model 'anthropic/claude'"
    );
    let cursor = hire(ToolKind::Cursor, "gpt-5", "default", false);
    assert_eq!(
        command(&cursor, &["Default"], "a task"),
        "cursor-agent --model 'gpt-5'"
    );
    let bare = hire(ToolKind::Cursor, "default", "default", false);
    assert_eq!(command(&bare, &["Default"], "a task"), "cursor-agent");
}

fn floor_office(tag: &str) -> (Office, PathBuf, Project) {
    let (office, dir) = office(tag);
    let repo = dir.join("home/code/app");
    std::fs::create_dir_all(&repo).expect("mkdir");
    let floor = office.add_floor(&repo.to_string_lossy()).expect("add");
    (office, dir, floor)
}

/// Run `cat` in a terminal-backed desk on `floor` (it exits on end-of-transmission).
fn launch_desk(office: &Office, dir: &Path, floor: &Project) -> String {
    let desk = Desk {
        tool: ToolKind::Antigravity,
        project_id: floor.id.clone(),
        root: dir.join("home/code/app"),
        title: "write docs".to_string(),
        model: "Gemini 3.6 Flash".to_string(),
        effort: "high".to_string(),
    };
    office
        .launch(&desk.root.clone(), "/bin/cat", Launched::Desk(desk))
        .expect("launch")
}

fn desks_of(office: &Office) -> Vec<Session> {
    office.refresh();
    office.snapshot().sessions
}

#[test]
fn a_tool_without_discovery_has_a_desk_while_its_terminal_runs() {
    let (office, dir, floor) = floor_office("desk");
    let terminal = launch_desk(&office, &dir, &floor);
    let id = desk_session_id(&terminal);
    let sessions = desks_of(&office);
    assert_eq!(sessions.len(), 1);
    let desk = &sessions[0];
    assert_eq!(desk.id, id);
    assert_eq!(
        (desk.tool, desk.title.as_str(), desk.state, desk.control),
        (
            ToolKind::Antigravity,
            "write docs",
            SessionState::Running,
            ControlMode::Full
        )
    );
    assert_eq!(desk.model.as_deref(), Some("Gemini 3.6 Flash"));
    assert_eq!(desk.effort.as_deref(), Some("high"));
    assert!(!desk.worktree && desk.helpers.is_empty());
    assert_eq!(desk.project_id, floor.id);
    let snapshot = office.snapshot();
    let listed = snapshot.projects.iter().find(|p| p.id == floor.id);
    assert_eq!(
        listed.map(|p| p.session_ids.clone()),
        Some(vec![id.clone()])
    );
    // The panel attaches by the desk's id, and a refresh keeps the desk.
    assert_eq!(
        office.terminal_backlog(&id).expect("backlog").terminal_id,
        terminal
    );
    assert_eq!(desks_of(&office).len(), 1);
    office.terminal_write(&id, "\u{4}").expect("eot");
    office
        .terminals
        .wait_exit(&terminal, Duration::from_secs(10))
        .expect("wait")
        .expect("exited");
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn a_desk_leaves_when_its_terminal_exits_and_is_not_a_failed_hire() {
    let (office, dir, floor) = floor_office("desk-exit");
    let terminal = launch_desk(&office, &dir, &floor);
    let id = desk_session_id(&terminal);
    office.terminal_write(&id, "\u{4}").expect("eot");
    let code = office
        .terminals
        .wait_exit(&terminal, Duration::from_secs(10))
        .expect("wait")
        .expect("exited");
    assert!(desks_of(&office).is_empty());
    assert!(office.snapshot().ended.is_empty());
    let exit = TerminalExit {
        terminal_id: terminal,
        exit_code: code,
    };
    assert_eq!(office.hire_failed(&exit), None);
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn demolishing_a_floor_stops_its_terminal_desks() {
    let (office, dir, floor) = floor_office("desk-demolish");
    let terminal = launch_desk(&office, &dir, &floor);
    assert_eq!(desks_of(&office).len(), 1);
    office.demolish_floor(&floor.id).expect("demolish");
    assert!(desks_of(&office).is_empty());
    assert!(office.snapshot().projects.is_empty());
    assert!(
        office
            .terminals
            .wait_exit(&terminal, Duration::from_secs(10))
            .expect("wait")
            .is_some()
    );
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn a_desk_whose_terminal_is_quiet_is_idle() {
    let desk = Desk {
        tool: ToolKind::Cursor,
        project_id: "p".to_string(),
        root: PathBuf::from("/"),
        title: "t".to_string(),
        model: "default".to_string(),
        effort: "default".to_string(),
    };
    let state = |secs| desk_session("t-1", &desk, Duration::from_secs(secs)).state;
    assert_eq!(state(9), SessionState::Running);
    assert_eq!(state(11), SessionState::Idle);
}

#[test]
fn a_tool_that_is_not_installed_cannot_be_hired() {
    let listed = |tool, available| ToolOptions {
        tool,
        available,
        models: Vec::new(),
        default_model: String::new(),
        ultracode: None,
        structured: false,
    };
    let options = vec![
        listed(ToolKind::Codex, true),
        listed(ToolKind::Antigravity, false),
    ];
    assert!(hireable(options.clone(), ToolKind::Codex).is_ok());
    assert_eq!(
        hireable(options.clone(), ToolKind::Antigravity).map(|_| ()),
        Err("Antigravity is not installed on this Mac".to_string())
    );
    assert!(hireable(options, ToolKind::Cursor).is_err());
}
