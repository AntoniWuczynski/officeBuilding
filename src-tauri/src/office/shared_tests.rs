//! What reaches the snapshot from outside the office's own records: peer messages found in
//! transcripts and the shared boards of Settings.

use super::tests::{floor_office, office};
use super::*;
use crate::model::{Party, TodoSource};
use crate::settings::{BoardSettings, SettingsPatch};
use std::fs;
use std::io::Write;

fn boards_patch(paths: &[&Path]) -> SettingsPatch {
    SettingsPatch {
        boards: Some(BoardSettings {
            shared_files: paths
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect(),
        }),
        ..SettingsPatch::default()
    }
}

const BOARD: &str = "# Actions\n- [ ] first | proj-a | 2026-11-16 | note\n- [x] second |  | 2026-11-16\n- [ ] third\n";

fn with_board(tag: &str) -> (Office, PathBuf, PathBuf, Arc<SettingsStore>) {
    let (office, dir, _floor) = floor_office(tag);
    let board = dir.join("board.md");
    fs::write(&board, BOARD).expect("board");
    let store = Arc::new(SettingsStore::open(dir.join("settings.json")));
    (office.with_settings(Arc::clone(&store)), dir, board, store)
}

fn peer_line(n: usize, body: &str) -> String {
    let me = std::process::id();
    format!(
        r#"{{"type":"user","isMeta":true,"timestamp":"2026-11-16T10:{n:02}:00Z","cwd":"/code/app","message":{{"role":"user","content":"Another Claude session sent a message:\n{body}"}},"origin":{{"kind":"peer","from":"uds:/tmp/cc-socks/{me}.sock","verifiedPeerPid":{me},"msg_id":"m-{n}","name":"sky","fromMode":"prompting","body":"{body}","verifiedLocal":true}},"userType":"external"}}"#
    )
}

#[test]
fn a_peer_message_written_after_the_first_scan_reaches_the_snapshot() {
    let (office, dir) = office("peer-office");
    let me = std::process::id();
    let sessions = dir.join("home/.claude/sessions");
    fs::create_dir_all(&sessions).expect("sessions");
    fs::write(
        sessions.join(format!("{me}.json")),
        format!(r#"{{"pid":{me},"sessionId":"sender","cwd":"/code/app","status":"idle"}}"#),
    )
    .expect("registry");
    let project = dir.join("home/.claude/projects/-code-app");
    fs::create_dir_all(&project).expect("project");
    let cwd = dir.join("home/code/app");
    fs::create_dir_all(&cwd).expect("cwd");
    let path = project.join("rx.jsonl");
    let first = format!(
        r#"{{"type":"user","origin":{{"kind":"human"}},"cwd":"{}","timestamp":"2026-11-16T09:00:00Z","message":{{"role":"user","content":"hi"}}}}"#,
        cwd.display()
    );
    fs::write(&path, format!("{first}\n")).expect("transcript");
    office.refresh();
    assert!(office.snapshot().messages.is_empty());

    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .expect("open");
    writeln!(file, "{}", peer_line(1, "ping")).expect("append");
    assert!(office.refresh(), "a new peer line is a visible change");
    let messages = office.snapshot().messages;
    assert_eq!(messages.len(), 1);
    assert_eq!(
        (
            messages[0].from_session_id.as_str(),
            messages[0].to_session_id.as_str(),
            messages[0].text.as_str()
        ),
        ("sender", "rx", "ping")
    );

    let long = "y".repeat(300);
    for n in 2..27 {
        writeln!(file, "{}", peer_line(n, &long)).expect("append");
    }
    office.refresh();
    let messages = office.snapshot().messages;
    assert_eq!(messages.len(), 20);
    assert!(messages.iter().all(|m| m.text.chars().count() <= 140));
    assert_eq!(messages.last().map(|m| m.id.as_str()), Some("m-26"));
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn a_shared_board_lists_its_lines_with_their_project_tags() {
    let (office, dir, board, store) = with_board("board-lists");
    assert!(office.snapshot().shared_boards.is_empty());
    store.update(boards_patch(&[&board])).expect("allowed");
    assert!(office.refresh(), "a new board is a visible change");
    let boards = office.snapshot().shared_boards;
    assert_eq!(boards.len(), 1);
    let id = format!("board:{}", board.display());
    assert_eq!(
        (boards[0].id.as_str(), boards[0].name.as_str()),
        (id.as_str(), "board.md")
    );
    let items = &boards[0].items;
    assert_eq!(
        items
            .iter()
            .map(|i| i.project_tag.as_deref())
            .collect::<Vec<_>>(),
        [Some("proj-a"), None, None]
    );
    assert!(
        items
            .iter()
            .all(|i| i.project_id == id && i.assignee == Party::Human)
    );
    assert_eq!(items[0].text, "first | proj-a | 2026-11-16 | note");
    assert_eq!(
        items[0].source,
        TodoSource::File {
            file: board.to_string_lossy().into_owned()
        }
    );

    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(&board)
        .expect("open");
    writeln!(file, "- [ ] fourth | proj-b").expect("append");
    assert!(office.refresh(), "a changed board file is a visible change");
    let items = &office.snapshot().shared_boards[0].items;
    assert_eq!(
        items.last().map(|i| i.text.as_str()),
        Some("fourth | proj-b")
    );
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn only_an_absolute_path_to_a_file_that_exists_can_be_a_board() {
    let (_office, dir, board, store) = with_board("board-refuse");
    for bad in ["Cargo.toml", "~/board.md", "/nowhere/board.md"] {
        let patch = SettingsPatch {
            boards: Some(BoardSettings {
                shared_files: vec![bad.to_string()],
            }),
            ..SettingsPatch::default()
        };
        assert!(store.update(patch).is_err(), "{bad} must be refused");
    }
    assert!(
        store.update(boards_patch(&[&dir])).is_err(),
        "a folder is no board"
    );
    assert_eq!(store.get().boards, BoardSettings::default());
    store
        .update(boards_patch(&[&board, &board]))
        .expect("allowed");
    assert_eq!(
        store.get().boards.shared_files.len(),
        1,
        "a repeat is dropped"
    );
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn a_floor_human_todo_that_is_a_board_lists_its_lines_once() {
    let (office, dir, floor) = floor_office("board-double");
    let own = dir.join("home/code/app/MY-ACTIONS.md");
    fs::write(&own, BOARD).expect("file");
    let link = dir.join("link.md");
    std::os::unix::fs::symlink(&own, &link).expect("symlink");
    let store = Arc::new(SettingsStore::open(dir.join("settings.json")));
    let office = office.with_settings(Arc::clone(&store));
    office.refresh();
    let on_floor = |o: &Office| {
        o.snapshot()
            .todos
            .iter()
            .filter(|t| t.project_id == floor.id)
            .count()
    };
    assert_eq!(on_floor(&office), 3, "without a board the floor lists them");

    // The board names the same file by another path: canonical paths decide.
    store.update(boards_patch(&[&link])).expect("allowed");
    office.refresh();
    assert_eq!(on_floor(&office), 0);
    assert_eq!(office.snapshot().shared_boards[0].items.len(), 3);

    store.update(boards_patch(&[])).expect("allowed");
    office.refresh();
    assert_eq!(
        on_floor(&office),
        3,
        "leaving the boards gives the file back"
    );
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn ticking_a_board_item_rewrites_that_line_and_nothing_else() {
    let (office, dir, board, store) = with_board("board-tick");
    store.update(boards_patch(&[&board])).expect("allowed");
    office.refresh();
    let id = office.snapshot().shared_boards[0].items[2].id.clone();
    office.tick_todo(&id).expect("tick");
    assert_eq!(
        fs::read_to_string(&board).expect("read"),
        BOARD.replace("- [ ] third", "- [x] third")
    );
    assert!(office.snapshot().shared_boards[0].items[2].done);
    assert!(
        office.tick_todo(&id).is_err(),
        "a ticked line is not ticked twice"
    );
    assert!(
        office.tick_todo("board:/etc/hosts|/etc/hosts|0").is_err(),
        "only a board's own lines"
    );
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn a_new_prompt_keeps_the_last_reply_on_a_desk_and_on_its_sign_out_line() {
    let (office, dir) = office("last-reply");
    let me = std::process::id();
    fs::create_dir_all(dir.join("home/.claude/sessions")).expect("sessions");
    fs::write(
        dir.join(format!("home/.claude/sessions/{me}.json")),
        format!(r#"{{"pid":{me},"sessionId":"live","cwd":"/code/app","status":"idle"}}"#),
    )
    .expect("registry");
    let project = dir.join("home/.claude/projects/-code-app");
    fs::create_dir_all(&project).expect("project");
    let cwd = dir.join("home/code/app");
    fs::create_dir_all(&cwd).expect("cwd");
    let at = |t: &str| format!(r#""cwd":"{}","timestamp":"2026-11-16T{t}Z""#, cwd.display());
    let transcript = format!(
        concat!(
            r#"{{"type":"user","origin":{{"kind":"human"}},{a},"message":{{"role":"user","content":"go"}}}}"#,
            "\n",
            r#"{{"type":"assistant",{b},"message":{{"id":"a1","content":[{{"type":"text","text":"Shipped it. More soon."}}]}}}}"#,
            "\n",
            r#"{{"type":"user","origin":{{"kind":"human"}},{c},"message":{{"role":"user","content":"and docs?"}}}}"#,
            "\n",
        ),
        a = at("09:00:00"),
        b = at("09:01:00"),
        c = at("09:02:00"),
    );
    for id in ["live", "old"] {
        fs::write(project.join(format!("{id}.jsonl")), &transcript).expect("transcript");
    }
    office.refresh();
    let snapshot = office.snapshot();
    let desk = snapshot
        .sessions
        .iter()
        .find(|s| s.id == "live")
        .expect("desk");
    let signed_out = snapshot
        .ended
        .iter()
        .find(|e| e.id == "old")
        .expect("ended");
    assert_eq!(desk.last_reply.as_deref(), Some("Shipped it."));
    assert_eq!(signed_out.last_reply.as_deref(), Some("Shipped it."));
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn a_board_whose_file_goes_missing_stays_with_no_items() {
    let (office, dir, board, store) = with_board("board-missing");
    store.update(boards_patch(&[&board])).expect("allowed");
    office.refresh();
    assert_eq!(office.snapshot().shared_boards[0].items.len(), 3);
    fs::remove_file(&board).expect("delete");
    assert!(office.refresh(), "the emptied board is a visible change");
    let boards = office.snapshot().shared_boards;
    assert_eq!(boards.len(), 1, "the board is kept");
    assert_eq!(boards[0].id, format!("board:{}", board.display()));
    assert_eq!(
        (boards[0].path.as_str(), boards[0].name.as_str()),
        (board.to_str().expect("utf8"), "board.md")
    );
    assert!(boards[0].items.is_empty());
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn only_a_shared_board_line_carries_a_project_tag() {
    let (office, dir, floor) = floor_office("board-tag");
    let line = "- [ ] handle a | b in the parser\n";
    fs::write(dir.join("home/code/app/TODO.md"), line).expect("floor file");
    let board = dir.join("board.md");
    fs::write(&board, line).expect("board");
    let store = Arc::new(SettingsStore::open(dir.join("settings.json")));
    let office = office.with_settings(Arc::clone(&store));
    store.update(boards_patch(&[&board])).expect("allowed");
    office.refresh();
    let snapshot = office.snapshot();
    let on_floor: Vec<_> = snapshot
        .todos
        .iter()
        .filter(|t| t.project_id == floor.id)
        .collect();
    assert_eq!(on_floor.len(), 1);
    assert_eq!(on_floor[0].project_tag, None);
    assert_eq!(
        snapshot.shared_boards[0].items[0].project_tag.as_deref(),
        Some("b in the parser")
    );
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn a_board_that_is_a_floors_agent_or_human_file_lists_each_line_once() {
    let (office, dir, floor) = floor_office("board-double-both");
    let root = dir.join("home/code/app");
    let mut boards = Vec::new();
    for (name, link) in [("TODO.md", "agent.md"), ("HUMAN_TODO.md", "human.md")] {
        fs::write(root.join(name), BOARD).expect("file");
        let link = dir.join(link);
        std::os::unix::fs::symlink(root.join(name), &link).expect("symlink");
        boards.push(link);
    }
    let store = Arc::new(SettingsStore::open(dir.join("settings.json")));
    let office = office.with_settings(Arc::clone(&store));
    let paths: Vec<&Path> = boards.iter().map(PathBuf::as_path).collect();
    store.update(boards_patch(&paths)).expect("allowed");
    office.refresh();
    let snapshot = office.snapshot();
    assert_eq!(
        snapshot
            .todos
            .iter()
            .filter(|t| t.project_id == floor.id)
            .count(),
        0
    );
    let listed: usize = snapshot.shared_boards.iter().map(|b| b.items.len()).sum();
    assert_eq!(listed, 6, "three lines per file, each shown once");
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn two_paths_to_one_file_make_one_board() {
    let (_office, dir, board, store) = with_board("board-same-file");
    let link = dir.join("link.md");
    std::os::unix::fs::symlink(&board, &link).expect("symlink");
    store
        .update(boards_patch(&[&board, &link]))
        .expect("allowed");
    assert_eq!(store.get().boards.shared_files.len(), 1);
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn a_resumed_transcript_repeating_a_msg_id_adds_no_second_message() {
    let (office, dir) = office("peer-resume");
    let me = std::process::id();
    let sessions = dir.join("home/.claude/sessions");
    fs::create_dir_all(&sessions).expect("sessions");
    fs::write(
        sessions.join(format!("{me}.json")),
        format!(r#"{{"pid":{me},"sessionId":"sender","cwd":"/code/app","status":"idle"}}"#),
    )
    .expect("registry");
    let project = dir.join("home/.claude/projects/-code-app");
    fs::create_dir_all(&project).expect("project");
    fs::create_dir_all(dir.join("home/code/app")).expect("cwd");
    for name in ["first", "resumed"] {
        fs::write(
            project.join(format!("{name}.jsonl")),
            format!("{}\n", peer_line(1, "ping")),
        )
        .expect("transcript");
    }
    office.refresh();
    let ids: Vec<String> = office
        .snapshot()
        .messages
        .into_iter()
        .map(|m| m.id)
        .collect();
    assert_eq!(ids, ["m-1"]);
    fs::remove_dir_all(dir).expect("cleanup");
}
