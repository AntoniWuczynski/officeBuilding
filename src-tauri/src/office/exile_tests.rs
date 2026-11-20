use super::exile::stop_agents;
use super::tests::{floor_office, office, session};
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
    office
        .attach_in_terminal(&out_id, &dir, "sleep 30")
        .expect("attach");
    office
        .attach_in_terminal(&stays_id, &dir, "sleep 30")
        .expect("attach");
    let kept = office
        .lock()
        .attached
        .get(&stays_id)
        .cloned()
        .expect("attached");
    let kept_pid = office.terminals.pid(&kept).expect("pid");
    office.exile_session(&out_id).expect("exile");
    assert!(
        office
            .terminals
            .wait_exit(&out, Duration::from_secs(10))
            .expect("wait")
            .is_some()
    );
    assert_eq!(office.terminals.wait_exit(&stays, Duration::ZERO), Ok(None));
    assert!(!office.lock().attached.contains_key(&out_id));
    assert!(office.lock().attached.contains_key(&stays_id));
    assert!(crate::discovery::claude_code::pid_alive(kept_pid));
    let left: Vec<String> = office
        .snapshot()
        .sessions
        .into_iter()
        .map(|s| s.id)
        .collect();
    assert_eq!(left, vec![stays_id]);
    office.demolish_floor(&floor.id).expect("cleanup floor");
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn exiling_an_in_app_desk_stops_its_driver() {
    struct Spy(Arc<std::sync::atomic::AtomicBool>);
    impl structured::Driver for Spy {
        fn prompt(&self, _: &str) -> Result<(), String> {
            Ok(())
        }
        fn answer(&self, _: &str, _: &str, _: &structured::Values) -> Result<(), String> {
            Ok(())
        }
        fn cancel(&self) -> Result<(), String> {
            Ok(())
        }
        fn stop(&self) {
            self.0.store(true, std::sync::atomic::Ordering::SeqCst);
        }
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
    office.structured.seat(
        "structured-1",
        info,
        SessionState::Thinking,
        Box::new(Spy(stopped.clone())),
    );
    office.refresh();
    office.exile_session("structured-1").expect("exile");
    assert!(stopped.load(std::sync::atomic::Ordering::SeqCst));
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn a_desk_being_handed_over_is_neither_exiled_nor_sunk() {
    let (office, dir, floor) = floor_office("exile-handover");
    let terminal = desk_on(&office, &dir, &floor, "moving");
    let id = desk_session_id(&terminal);
    office.refresh();
    office.lock().handover.underway.insert(id.clone());
    assert_eq!(
        office.exile_session(&id),
        Err("this desk is being handed over".to_string())
    );
    assert_eq!(
        office.demolish_floor(&floor.id),
        Err("moving is being handed over, so the floor is still standing".to_string())
    );
    assert_eq!(
        office.terminals.wait_exit(&terminal, Duration::ZERO),
        Ok(None)
    );
    office.lock().handover.underway.remove(&id);
    office.demolish_floor(&floor.id).expect("cleanup floor");
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
        last_reply: None,
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
