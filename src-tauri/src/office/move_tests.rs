use super::exile::stop_agents;
use super::tests::office;
use super::*;

/// A repo `name` under the test home with one live Claude Code session `id`: a real `sleep`
/// registered the way Claude Code registers itself, so discovery and `demolish_floor` see a pid.
pub(super) fn live_claude_session(dir: &Path, name: &str, id: &str) -> i32 {
    let repo = dir.join("home/code").join(name);
    std::fs::create_dir_all(repo.join(".git")).expect("mkdir");
    let mut child = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .expect("spawn");
    let pid = i32::try_from(child.id()).expect("pid");
    std::thread::spawn(move || child.wait());
    let claude = dir.join("home/.claude");
    std::fs::create_dir_all(claude.join("sessions")).expect("mkdir");
    std::fs::create_dir_all(claude.join(format!("projects/-{name}"))).expect("mkdir");
    let cwd = repo.to_string_lossy();
    std::fs::write(
        claude.join(format!("sessions/{pid}.json")),
        format!(r#"{{"pid":{pid},"sessionId":"{id}","cwd":"{cwd}","status":"busy"}}"#),
    )
    .expect("registry");
    std::fs::write(
        claude.join(format!("projects/-{name}/{id}.jsonl")),
        format!(
            r#"{{"type":"user","cwd":"{cwd}","timestamp":"2026-09-29T10:00:00Z","message":{{"role":"user","content":"work"}}}}"#
        ),
    )
    .expect("transcript");
    pid
}

#[test]
fn a_moved_session_belongs_to_its_new_floor_when_either_floor_is_sunk() {
    let (office, dir) = office("moved-sink");
    let pid = live_claude_session(&dir, "alpha", "s1");
    let a = discovery::project_id(&discovery::project_root(&dir.join("home/code/alpha")));
    std::fs::create_dir_all(dir.join("home/code/beta/.git")).expect("mkdir");
    let b = office.add_floor("~/code/beta").expect("add beta").id;
    office.refresh();
    let on = |office: &Office, floor: &str| {
        office
            .snapshot()
            .sessions
            .iter()
            .any(|s| s.id == "s1" && s.project_id == floor)
    };
    assert!(on(&office, &a));

    office.move_session("s1", Some(&b)).expect("move");
    assert!(on(&office, &b), "the snapshot shows it on beta");
    assert_eq!(
        office.lock().store.records.moved_sessions.get("s1"),
        Some(&b)
    );

    office.demolish_floor(&a).expect("sink alpha");
    assert!(
        discovery::claude_code::pid_alive(pid),
        "sinking alpha does not stop it"
    );
    office.refresh();
    assert!(on(&office, &b), "and it is still on beta after a rescan");

    office.demolish_floor(&b).expect("sink beta");
    assert!(
        !discovery::claude_code::pid_alive(pid),
        "sinking beta stops it"
    );
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn moving_a_session_back_to_where_it_was_found_clears_the_stored_move() {
    let (office, dir) = office("moved-back");
    let pid = live_claude_session(&dir, "alpha", "s1");
    std::fs::create_dir_all(dir.join("home/code/beta/.git")).expect("mkdir");
    let a = discovery::project_id(&discovery::project_root(&dir.join("home/code/alpha")));
    let b = office.add_floor("~/code/beta").expect("add beta").id;
    office.refresh();
    let moves = |office: &Office| office.lock().store.records.moved_sessions.clone();
    office.move_session("s1", Some(&b)).expect("move");
    assert_eq!(moves(&office).len(), 1);
    office.move_session("s1", Some(&a)).expect("back");
    assert!(moves(&office).is_empty(), "its own floor clears the entry");
    office.move_session("s1", Some(&b)).expect("again");
    office.move_session("s1", None).expect("none");
    assert!(moves(&office).is_empty(), "so does None");
    assert!(
        office.move_session("s1", Some("no-such-floor")).is_err(),
        "an unknown floor is refused"
    );
    assert!(office.move_session("ghost", Some(&b)).is_err());
    stop_agents(&[("s1".to_string(), pid)], Duration::from_secs(5)).expect("stop");
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn a_stored_move_for_a_session_that_is_gone_is_pruned_on_refresh() {
    let (office, dir) = office("moved-prune");
    let pid = live_claude_session(&dir, "alpha", "s1");
    std::fs::create_dir_all(dir.join("home/code/beta/.git")).expect("mkdir");
    let b = office.add_floor("~/code/beta").expect("add beta").id;
    office.refresh();
    {
        let mut inner = office.lock();
        let moves = &mut inner.store.records.moved_sessions;
        moves.insert("s1".to_string(), b.clone());
        moves.insert("ghost".to_string(), b);
    }
    office.refresh();
    let kept: Vec<String> = office
        .lock()
        .store
        .records
        .moved_sessions
        .keys()
        .cloned()
        .collect();
    assert_eq!(kept, vec!["s1".to_string()], "only the known session stays");
    stop_agents(&[("s1".to_string(), pid)], Duration::from_secs(5)).expect("stop");
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn a_move_survives_a_scan_that_misses_its_session_and_goes_once_it_is_unseen_past_the_window() {
    let (office, dir) = office("moved-gap");
    let pid = live_claude_session(&dir, "alpha", "s1");
    std::fs::create_dir_all(dir.join("home/code/beta/.git")).expect("mkdir");
    let b = office.add_floor("~/code/beta").expect("add beta").id;
    office.refresh();
    office.move_session("s1", Some(&b)).expect("move");
    let moves = |office: &Office| office.lock().store.records.moved_sessions.clone();
    stop_agents(&[("s1".to_string(), pid)], Duration::from_secs(5)).expect("stop");
    std::fs::remove_dir_all(dir.join("home/.claude")).expect("session files vanish");
    office.refresh();
    assert_eq!(moves(&office).get("s1"), Some(&b), "one gap keeps the move");
    let at = |hours: i64| {
        (chrono::Utc::now() - chrono::Duration::hours(hours))
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    };
    let unseen_since = |office: &Office, when: String| {
        office
            .lock()
            .store
            .records
            .move_seen
            .insert("s1".to_string(), Some(when));
    };
    unseen_since(&office, at(11));
    office.refresh();
    assert_eq!(moves(&office).get("s1"), Some(&b), "inside the window");
    unseen_since(&office, at(13));
    office.refresh();
    assert!(moves(&office).is_empty(), "past it the move is dropped");
    assert!(office.lock().store.records.move_seen.is_empty());
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn a_move_whose_session_comes_back_is_seen_again() {
    let (office, dir) = office("moved-back-seen");
    let pid = live_claude_session(&dir, "alpha", "s1");
    std::fs::create_dir_all(dir.join("home/code/beta/.git")).expect("mkdir");
    let b = office.add_floor("~/code/beta").expect("add beta").id;
    office.refresh();
    office.move_session("s1", Some(&b)).expect("move");
    office.lock().store.records.move_seen.insert(
        "s1".to_string(),
        Some("2020-01-01T00:00:00.000Z".to_string()),
    );
    office.refresh();
    assert_eq!(office.lock().store.records.move_seen["s1"], None);
    assert_eq!(office.lock().store.records.moved_sessions.len(), 1);
    stop_agents(&[("s1".to_string(), pid)], Duration::from_secs(5)).expect("stop");
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn a_session_cannot_be_moved_onto_the_setup_floor() {
    let (office, dir) = office("moved-setup");
    let pid = live_claude_session(&dir, "alpha", "s1");
    office.ensure_setup_floor().expect("setup floor");
    office.refresh();
    let setup = office.setup_project_id.clone();
    let err = office
        .move_session("s1", Some(&setup))
        .expect_err("refused");
    assert!(err.contains("Setup"), "{err}");
    assert!(office.lock().store.records.moved_sessions.is_empty());
    stop_agents(&[("s1".to_string(), pid)], Duration::from_secs(5)).expect("stop");
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn clearing_a_move_whose_home_floor_was_sunk_is_refused_and_the_agent_stays_visible() {
    let (office, dir) = office("moved-clear-sunk");
    let pid = live_claude_session(&dir, "alpha", "s1");
    let a = discovery::project_id(&discovery::project_root(&dir.join("home/code/alpha")));
    std::fs::create_dir_all(dir.join("home/code/beta/.git")).expect("mkdir");
    let b = office.add_floor("~/code/beta").expect("add beta").id;
    office.refresh();
    office.move_session("s1", Some(&b)).expect("move");
    office.demolish_floor(&a).expect("sink alpha");
    let visible = |office: &Office| {
        office
            .snapshot()
            .sessions
            .iter()
            .any(|s| s.id == "s1" && s.project_id == b)
    };
    assert!(visible(&office));
    for target in [None, Some(a.as_str())] {
        let err = office.move_session("s1", target).expect_err("refused");
        assert!(err.contains("sunk"), "{err}");
    }
    assert_eq!(
        office.lock().store.records.moved_sessions.get("s1"),
        Some(&b)
    );
    assert!(visible(&office), "still on beta");
    stop_agents(&[("s1".to_string(), pid)], Duration::from_secs(5)).expect("stop");
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn a_move_takes_a_signed_out_session_and_ignores_a_floor_that_is_not_there() {
    let ended = EndedSession {
        id: "gone".to_string(),
        tool: ToolKind::ClaudeCode,
        project_id: "a".to_string(),
        title: "gone".to_string(),
        ended_at: "2026-10-06T09:00:00Z".to_string(),
        model: None,
        effort: None,
        last_reply: None,
    };
    let floor = |id: &str| Project {
        id: id.to_string(),
        name: id.to_string(),
        path: id.to_string(),
        session_ids: Vec::new(),
    };
    let mut found = Discovered {
        projects: vec![floor("a"), floor("b")],
        ended: vec![ended],
        ..Discovered::default()
    };
    let moves = BTreeMap::from([("gone".to_string(), "nowhere".to_string())]);
    assert!(apply_moves(&mut found, &moves).is_empty());
    assert_eq!(found.ended[0].project_id, "a");
    let moves = BTreeMap::from([("gone".to_string(), "b".to_string())]);
    assert_eq!(apply_moves(&mut found, &moves)["gone"], "a");
    assert_eq!(found.ended[0].project_id, "b");
    assert!(found.projects.iter().all(|p| p.session_ids.is_empty()));
}

#[test]
fn a_session_that_takes_over_a_terminal_takes_its_move_once() {
    let map = |pairs: &[(&str, &str)]| -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    };
    let mut records = crate::store::Records {
        moved_sessions: map(&[("old", "b"), ("from", "c")]),
        ..crate::store::Records::default()
    };
    let before = map(&[("t1", "old"), ("t3", "same")]);
    // t1 after a `/clear`, t2 a handover's receiver, t3 unchanged, t4 a fresh desk.
    let owned = map(&[
        ("t1", "new"),
        ("t2", "heir"),
        ("t3", "same"),
        ("t4", "fresh"),
    ]);
    let handed_from = map(&[("t2", "from")]);
    assert!(super::moves::inherit_moves(
        &mut records,
        &before,
        &owned,
        &handed_from
    ));
    assert_eq!(
        records.moved_sessions,
        map(&[("old", "b"), ("from", "c"), ("new", "b"), ("heir", "c")])
    );
    assert_eq!(records.move_seen.get("new"), Some(&None));
    // Moved back by hand afterwards: the next scan, with nothing taken over, leaves it home.
    records.moved_sessions.remove("new");
    assert!(!super::moves::inherit_moves(
        &mut records,
        &owned,
        &owned,
        &handed_from
    ));
    assert!(!records.moved_sessions.contains_key("new"));
}
