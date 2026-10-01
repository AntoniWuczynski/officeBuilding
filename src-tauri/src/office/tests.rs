use super::*;

fn office(tag: &str) -> (Office, PathBuf) {
    let dir = std::env::temp_dir().join(format!("ob-office-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(dir.join("home")).expect("mkdir");
    let store = Store::open(dir.join("store.json")).expect("store");
    (
        Office::new(
            dir.join("home"),
            store,
            Terminals::new(std::sync::Arc::new(|_| {})),
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
    let office = Office::new(dir, store, Terminals::new(std::sync::Arc::new(|_| {})));
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
