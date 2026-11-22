use super::claude_code::{Found, LiveEntry, parse_transcript};
use super::*;
use std::io::Cursor;

fn found(id: &str, cwd: &str, lines: &[&str], live: bool) -> Found {
    Found {
        session_id: id.to_string(),
        transcript: parse_transcript(Cursor::new(lines.join("\n"))),
        live: live.then(|| LiveEntry {
            pid: 1,
            session_id: id.to_string(),
            cwd: cwd.to_string(),
            busy: false,
            waiting: false,
            name: None,
            name_source: None,
        }),
        helpers: Vec::new(),
        helper_places: BTreeMap::new(),
    }
}

/// Every folder is its own floor, outside any worktree.
fn own_floor(cwd: &Path) -> Location {
    Location {
        root: cwd.to_path_buf(),
        worktree: false,
        checkout: cwd.to_path_buf(),
    }
}

fn claude_inputs(founds: Vec<Found>) -> Vec<SessionInput> {
    founds
        .into_iter()
        .filter_map(|f| from_claude(f, &[]))
        .collect()
}

const ASK: &str = r#"{"type":"assistant","cwd":"/home/me/code/app","timestamp":"2026-09-29T10:01:00Z","message":{"id":"m","content":[{"type":"tool_use","id":"toolu_1","name":"AskUserQuestion","input":{"questions":[{"question":"Which?","options":[{"label":"A"},{"label":"B"}]}]}}]}}"#;
const PROSE: &str = r#"{"type":"assistant","cwd":"/home/me/code/app","timestamp":"2026-09-29T10:00:00Z","message":{"id":"m","content":[{"type":"text","text":"Fixed it. Should I also bump the version?"}]}}"#;
const DONE: &str = r#"{"type":"assistant","cwd":"/home/me/code/other","timestamp":"2026-09-29T09:00:00Z","message":{"id":"m","content":[{"type":"text","text":"All done."}]}}"#;

#[test]
fn an_ended_session_in_a_folder_that_is_gone_is_dropped() {
    let here = std::env::temp_dir().to_string_lossy().into_owned();
    let line = |cwd: &str| {
        format!(
            r#"{{"type":"assistant","cwd":"{cwd}","timestamp":"2026-09-29T09:00:00Z","message":{{"id":"m","content":[{{"type":"text","text":"Done."}}]}}}}"#
        )
    };
    let (kept, gone) = (line(&here), line("/nowhere/deleted-checkout"));
    let inputs = claude_inputs(vec![
        found("a", &here, &[&kept], false),
        found("b", "/nowhere/deleted-checkout", &[&gone], false),
        found("c", "/nowhere/deleted-checkout", &[&gone], true),
    ]);
    let ids: Vec<&str> = inputs
        .iter()
        .filter(|i| still_there(i))
        .map(|i| i.session_id.as_str())
        .collect();
    assert_eq!(
        ids,
        vec!["a", "c"],
        "a live agent keeps its desk even there"
    );
}

#[test]
fn groups_sessions_into_floors_and_queues_questions() {
    let home = Path::new("/home/me");
    let d = assemble(
        home,
        claude_inputs(vec![
            found("s1", "/home/me/code/app", &[ASK], true),
            found("s2", "/home/me/code/app", &[PROSE], true),
            found("s3", "/home/me/code/other", &[DONE], false),
        ]),
        own_floor,
        &BTreeMap::new(),
    );
    assert_eq!(d.projects.len(), 2);
    let app = d
        .projects
        .iter()
        .find(|p| p.name == "app")
        .expect("app floor");
    assert_eq!(app.path, "~/code/app");
    assert_eq!(app.session_ids, vec!["s1".to_string(), "s2".to_string()]);

    // An ended session leaves the office, but its recent floor stays.
    assert!(d.sessions.iter().all(|s| s.id != "s3"));
    let other = d
        .projects
        .iter()
        .find(|p| p.name == "other")
        .expect("other floor");
    assert!(other.session_ids.is_empty());

    assert_eq!(d.questions.len(), 2);
    let formal = &d.questions[0];
    assert_eq!(formal.id, "s1:toolu_1");
    assert_eq!(
        formal
            .options
            .iter()
            .map(|o| o.label.as_str())
            .collect::<Vec<_>>(),
        vec!["A", "B"]
    );
    assert_eq!(formal.answer_via, AnswerVia::Terminal);
    let prose = &d.questions[1];
    assert_eq!(prose.prompt, "Fixed it. Should I also bump the version?");
    assert!(prose.options.is_empty());
    let s1 = d.sessions.iter().find(|s| s.id == "s1").expect("s1");
    assert_eq!(s1.pending_question_ids, vec!["s1:toolu_1".to_string()]);
    assert_eq!(s1.control, ControlMode::RaiseWindow);
}

#[test]
fn the_registry_name_is_the_session_name_and_leaves_the_title_alone() {
    let titled = r#"{"type":"ai-title","aiTitle":"Fix the parser"}"#;
    let mut f = found("s", "/home/me/code/app", &[titled, PROSE], true);
    f.live.as_mut().expect("live").name = Some("alpha-fix-2-X".to_string());
    let d = assemble(
        Path::new("/home/me"),
        claude_inputs(vec![f]),
        own_floor,
        &BTreeMap::new(),
    );
    assert_eq!(d.sessions[0].session_name.as_deref(), Some("alpha-fix-2-X"));
    assert_eq!(d.sessions[0].title, "Fix the parser");
}

#[test]
fn an_auto_named_registry_entry_carries_its_name_source() {
    let entry = claude_code::parse_registry_entry(
        r#"{"pid":7,"sessionId":"s","cwd":"/home/me/code/app","name":"beta-web","nameSource":"auto"}"#,
    )
    .expect("entry");
    let mut f = found("s", "/home/me/code/app", &[PROSE], true);
    f.live = Some(entry);
    let d = assemble(
        Path::new("/home/me"),
        claude_inputs(vec![f]),
        own_floor,
        &BTreeMap::new(),
    );
    assert_eq!(
        d.sessions[0].session_name_source,
        Some(crate::model::NameSource::Auto)
    );
}

fn skills_after(lines: &[&str]) -> Vec<String> {
    parse_transcript(Cursor::new(lines.join("\n"))).skills_since_prompt
}

const SKILL: &str = r#"{"type":"assistant","message":{"id":"m1","content":[{"type":"tool_use","id":"t1","name":"Skill","input":{"skill":"review"}}]}}"#;
const OTHER_SKILL: &str = r#"{"type":"assistant","message":{"id":"m2","content":[{"type":"tool_use","id":"t2","name":"Skill","input":{"skill":"deploy"}}]}}"#;
const HUMAN: &str =
    r#"{"type":"user","origin":{"kind":"human"},"message":{"role":"user","content":"and now?"}}"#;
const META: &str = r#"{"type":"user","isMeta":true,"message":{"role":"user","content":"caveat"}}"#;
const PEER: &str = r#"{"type":"user","isMeta":true,"origin":{"kind":"peer","from":"uds:/tmp/a.sock"},"message":{"role":"user","content":"hello"}}"#;

#[test]
fn skills_called_stay_until_the_human_speaks_again() {
    assert_eq!(skills_after(&[SKILL, OTHER_SKILL]), ["review", "deploy"]);
    assert_eq!(skills_after(&[SKILL, SKILL]), ["review"]);
    assert_eq!(skills_after(&[SKILL, META, PEER]), ["review"]);
    let notified = r#"{"type":"user","origin":{"kind":"task-notification"},"message":{"role":"user","content":"a task ended"}}"#;
    assert_eq!(skills_after(&[SKILL, notified]), ["review"]);
    assert!(skills_after(&[SKILL, HUMAN]).is_empty());
    assert_eq!(skills_after(&[SKILL, HUMAN, OTHER_SKILL]), ["deploy"]);
}

#[test]
fn a_new_prompt_leaves_the_last_reply_on_the_desk_and_on_the_sign_out_sheet() {
    let said = r#"{"type":"assistant","cwd":"/home/me/code/app","timestamp":"2026-09-29T10:00:00Z","message":{"id":"m","content":[{"type":"text","text":"Shipped it. Next is tests."}]}}"#;
    let asked = r#"{"type":"user","origin":{"kind":"human"},"cwd":"/home/me/code/app","timestamp":"2026-09-29T10:05:00Z","message":{"role":"user","content":"and the docs?"}}"#;
    let d = assemble(
        Path::new("/home/me"),
        claude_inputs(vec![
            found("live", "/home/me/code/app", &[said, asked], true),
            found("gone", "/home/me/code/app", &[said, asked], false),
        ]),
        own_floor,
        &BTreeMap::new(),
    );
    assert_eq!(d.sessions[0].last_reply.as_deref(), Some("Shipped it."));
    assert_eq!(d.ended[0].last_reply.as_deref(), Some("Shipped it."));
}

#[test]
fn a_reply_is_cut_to_its_first_sentence_within_the_limit() {
    assert_eq!(first_sentence("Done. More.", 140).as_deref(), Some("Done."));
    assert_eq!(
        first_sentence("v1.2 is out! Go.", 140).as_deref(),
        Some("v1.2 is out!")
    );
    assert_eq!(
        first_sentence("a.rs fixed\nsecond line", 140).as_deref(),
        Some("a.rs fixed")
    );
    assert_eq!(first_sentence("  \n ", 140), None);
    assert_eq!(
        first_sentence("\n\n  Done. More.", 140).as_deref(),
        Some("Done.")
    );
    let long = first_sentence(&"x".repeat(500), 140).expect("long");
    assert_eq!(long.chars().count(), 140);
}

#[test]
fn a_reply_skips_a_code_fence_and_drops_its_markdown_marker() {
    let first = |t: &str| first_sentence(t, 140);
    assert_eq!(
        first("```rust\nlet x = 1.\n```\nIt works. Next.").as_deref(),
        Some("It works.")
    );
    assert_eq!(first("```\nunclosed code. here"), None);
    assert_eq!(first("## Summary\nAll good.").as_deref(), Some("Summary"));
    assert_eq!(
        first("# Plan: ship it. Then rest").as_deref(),
        Some("Plan: ship it.")
    );
    for line in [
        "- Fix it. Then",
        "* Fix it. Then",
        "1. Fix it. Then",
        "12. Fix it. Then",
    ] {
        assert_eq!(first(line).as_deref(), Some("Fix it."), "{line}");
    }
    assert_eq!(first("-5 is low. Ok").as_deref(), Some("-5 is low."));
    assert_eq!(
        first("**bold** start. Ok").as_deref(),
        Some("**bold** start.")
    );
}

#[test]
fn a_dot_after_one_letter_or_a_number_does_not_end_a_sentence() {
    let first = |t: &str| first_sentence(t, 140);
    assert_eq!(
        first("1. Fix the parser. Then x").as_deref(),
        Some("Fix the parser.")
    );
    assert_eq!(
        first("Use e.g. a map. Done").as_deref(),
        Some("Use e.g. a map.")
    );
    assert_eq!(
        first("That is, i.e. the map. Done").as_deref(),
        Some("That is, i.e. the map.")
    );
    assert_eq!(
        first("Fixed in step 2. Done").as_deref(),
        Some("Fixed in step 2. Done")
    );
    assert_eq!(first("It ran. Fine.").as_deref(), Some("It ran."));
    assert_eq!(
        first("It ends with e.g.").as_deref(),
        Some("It ends with e.g.")
    );
}

#[test]
fn ended_sessions_go_on_their_floors_sign_out_sheet_newest_first() {
    let later = r#"{"type":"assistant","cwd":"/home/me/code/app/web","timestamp":"2026-09-29T11:00:00Z","message":{"id":"m","model":"claude-opus-5-5","content":[{"type":"text","text":"Bye."}]}}"#;
    let scripted = r#"{"type":"assistant","entrypoint":"sdk-cli","cwd":"/home/me/code/app","timestamp":"2026-09-29T12:00:00Z","message":{"id":"m","content":[{"type":"text","text":"ok"}]}}"#;
    let d = assemble(
        Path::new("/home/me"),
        claude_inputs(vec![
            found("s1", "/home/me/code/app", &[ASK], true),
            found("s3", "/home/me/code/other", &[DONE], false),
            found("s4", "/home/me/code/app/web", &[later], false),
            found("s5", "/home/me/code/app", &[scripted], false),
        ]),
        |cwd| {
            if cwd.starts_with("/home/me/code/app") {
                own_floor(Path::new("/home/me/code/app"))
            } else {
                own_floor(cwd)
            }
        },
        &BTreeMap::new(),
    );
    let ended: Vec<&str> = d.ended.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(ended, vec!["s4", "s3"], "scripted runs are not listed");
    let s4 = &d.ended[0];
    assert_eq!(s4.project_id, project_id(Path::new("/home/me/code/app")));
    assert_eq!(s4.ended_at, "2026-09-29T11:00:00Z");
    assert_eq!(s4.model.as_deref(), Some("claude-opus-5-5"));
    // Resumed in the folder it ran in, not just its floor's root.
    assert_eq!(
        d.resume_dirs.get("s4"),
        Some(&PathBuf::from("/home/me/code/app/web"))
    );
    assert!(
        !d.resume_dirs.contains_key("s1"),
        "a live session is not ended"
    );
}

#[test]
fn a_question_only_the_registry_knows_about_still_waits() {
    // Claude Code writes an open AskUserQuestion to the transcript only once it is answered.
    let mut asking = found("s1", "/home/me/code/app", &[DONE], true);
    if let Some(live) = asking.live.as_mut() {
        live.waiting = true;
    }
    let d = assemble(
        Path::new("/home/me"),
        claude_inputs(vec![asking]),
        own_floor,
        &BTreeMap::new(),
    );
    let s1 = d.sessions.iter().find(|s| s.id == "s1").expect("s1");
    assert_eq!(s1.state, SessionState::WaitingHuman);
    assert_eq!(s1.pending_question_ids, vec!["s1:waiting".to_string()]);
    let q = &d.questions[0];
    assert_eq!(q.prompt, "Waiting for you in its terminal");
    assert!(q.options.is_empty());
    assert_eq!(q.answer_via, AnswerVia::Terminal);
}

#[test]
fn a_codex_placeholder_is_an_idle_desk_that_is_never_offered_for_resuming() {
    let placeholder = super::codex::Found {
        session_id: "codex-new-7".to_string(),
        transcript: super::codex::parse_transcript(Cursor::new(
            r#"{"timestamp":"t","type":"session_meta","payload":{"id":"x","session_id":"x","cwd":"/home/me/code/app","source":"cli"}}"#,
        )),
        title: Some("New Codex session".to_string()),
        live_pid: Some(7),
        resumable: false,
        owned: false,
        helpers: Vec::new(),
        kids: BTreeMap::new(),
    };
    let input = from_codex(placeholder, &[]).expect("a codex session");
    assert!(!input.resumable);
    let d = assemble(
        Path::new("/home/me"),
        vec![input],
        own_floor,
        &BTreeMap::new(),
    );
    assert_eq!(d.sessions[0].title, "New Codex session");
    assert_eq!(d.sessions[0].state, SessionState::Idle);
}

#[test]
fn codex_sessions_join_the_same_floor_with_tool_kind_and_prose_questions() {
    let meta = r#"{"timestamp":"2026-09-29T10:00:00.000Z","type":"session_meta","payload":{"id":"c1","session_id":"root1","cwd":"/home/me/code/app","source":"cli"}}"#;
    let user = r#"{"timestamp":"2026-09-29T10:00:01.000Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"fix it"}]}}"#;
    let reply = r#"{"timestamp":"2026-09-29T10:00:02.000Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Fixed it. Should I also bump the version?"}]}}"#;
    let codex_transcript =
        super::codex::parse_transcript(Cursor::new([meta, user, reply].join("\n")));
    let codex_found = super::codex::Found {
        session_id: "c1".to_string(),
        transcript: codex_transcript,
        title: Some("Bump check".to_string()),
        live_pid: Some(7),
        resumable: true,
        owned: false,
        helpers: Vec::new(),
        kids: BTreeMap::new(),
    };
    let idle: &str = r#"{"type":"assistant","cwd":"/home/me/code/app","timestamp":"2026-09-29T09:00:00Z","message":{"id":"m","content":[{"type":"text","text":"All good."}]}}"#;
    let home = Path::new("/home/me");
    let mut inputs = claude_inputs(vec![found("s1", "/home/me/code/app", &[idle], true)]);
    inputs.extend(from_codex(codex_found, &[]));
    let d = assemble(home, inputs, own_floor, &BTreeMap::new());

    assert_eq!(d.projects.len(), 1);
    let app = &d.projects[0];
    assert_eq!(app.session_ids, vec!["s1".to_string(), "c1".to_string()]);

    let c1 = d.sessions.iter().find(|s| s.id == "c1").expect("c1");
    assert_eq!(c1.tool, ToolKind::Codex);
    assert_eq!(c1.title, "Bump check");
    assert_eq!(c1.state, SessionState::WaitingHuman);
    assert_eq!(c1.control, ControlMode::RaiseWindow);

    let prose = d
        .questions
        .iter()
        .find(|q| q.session_id == "c1")
        .expect("codex question");
    assert_eq!(prose.prompt, "Fixed it. Should I also bump the version?");
    assert!(prose.options.is_empty());
    assert_eq!(prose.answer_via, AnswerVia::Terminal);
}

#[test]
fn a_worktree_session_sits_on_its_main_checkouts_floor() {
    let main = std::env::temp_dir().join(format!("ob-worktree-{}", std::process::id()));
    let git_dir = main.join(".git/worktrees/agent-1");
    let tree = main.join(".claude/worktrees/agent-1");
    let submodule = main.join("vendor/lib");
    for dir in [
        &git_dir,
        &tree.join("server"),
        &main.join("web"),
        &main.join(".git/modules/lib"),
        &submodule,
    ] {
        fs::create_dir_all(dir).expect("mkdir");
    }
    fs::write(git_dir.join("commondir"), "../..\n").expect("commondir");
    fs::write(
        tree.join(".git"),
        format!("gitdir: {}\n", git_dir.display()),
    )
    .expect("pointer");
    fs::write(submodule.join(".git"), "gitdir: ../../.git/modules/lib\n").expect("submodule");

    let at = |dir: &Path| {
        let cwd = dir.to_string_lossy().into_owned();
        let line = format!(
            r#"{{"type":"assistant","cwd":"{cwd}","timestamp":"2026-09-29T10:00:00Z","message":{{"id":"m","content":[{{"type":"text","text":"ok"}}]}}}}"#
        );
        (cwd, line)
    };
    let (in_main, main_line) = at(&main.join("web"));
    let (in_tree, tree_line) = at(&tree.join("server"));
    let d = assemble(
        Path::new("/home/me"),
        claude_inputs(vec![
            found("s1", &in_main, &[&main_line], true),
            found("s2", &in_tree, &[&tree_line], true),
            // Started in the main checkout, then `cd`ed into the worktree.
            found("s3", &in_main, &[&main_line, &tree_line], true),
        ]),
        locate,
        &BTreeMap::new(),
    );
    assert_eq!(
        d.projects.len(),
        1,
        "the worktree is not a floor of its own"
    );
    assert_eq!(
        d.projects[0].session_ids,
        vec!["s1".to_string(), "s2".to_string(), "s3".to_string()]
    );
    let worktree = |id: &str| {
        d.sessions
            .iter()
            .find(|s| s.id == id)
            .expect("session")
            .worktree
    };
    assert!(!worktree("s1"));
    assert!(worktree("s2"));
    assert!(worktree("s3"));

    // A submodule's `.git` file points at a git dir with no commondir: its own floor.
    assert_eq!(
        locate(&submodule),
        Location {
            root: submodule.clone(),
            worktree: false,
            checkout: submodule.clone(),
        }
    );
    fs::remove_dir_all(&main).expect("cleanup");
}

#[test]
fn paths_and_ids() {
    assert_eq!(
        display_path(Path::new("/home/me"), Path::new("/home/me")),
        "~"
    );
    assert_eq!(
        display_path(Path::new("/opt/x"), Path::new("/home/me")),
        "/opt/x"
    );
    assert_eq!(fnv1a(""), 0x811c_9dc5);
    assert_eq!(fnv1a("a"), 0xe40c_292c);
    assert_eq!(project_id(Path::new("/a")), project_id(Path::new("/a")));
    let here = std::env::current_dir().expect("cwd");
    assert!(project_root(&here).join(".git").exists());
}

fn codex_input(id: &str, live: bool, records: &[String]) -> SessionInput {
    let meta = format!(
        r#"{{"timestamp":"2026-11-20T10:00:00.000Z","type":"session_meta","payload":{{"id":"{id}","session_id":"{id}","cwd":"/home/me/code/app","source":"cli"}}}}"#
    );
    let lines: Vec<String> = std::iter::once(meta)
        .chain(records.iter().cloned())
        .collect();
    let found = super::codex::Found {
        session_id: id.to_string(),
        transcript: super::codex::parse_transcript(Cursor::new(lines.join("\n"))),
        title: None,
        live_pid: live.then_some(7),
        resumable: true,
        owned: false,
        helpers: Vec::new(),
        kids: BTreeMap::new(),
    };
    from_codex(found, &[]).expect("a codex session")
}

fn codex_event(kind: &str, extra: &str) -> String {
    format!(
        r#"{{"timestamp":"2026-11-20T10:00:01.000Z","type":"event_msg","payload":{{"type":"{kind}"{extra}}}}}"#
    )
}

fn codex_reply(text: &str) -> String {
    format!(
        r#"{{"timestamp":"2026-11-20T10:00:02.000Z","type":"response_item","payload":{{"type":"message","role":"assistant","phase":"final_answer","content":[{{"type":"output_text","text":"{text}"}}]}}}}"#
    )
}

fn codex_assemble(inputs: Vec<SessionInput>) -> Discovered {
    assemble(Path::new("/home/me"), inputs, own_floor, &BTreeMap::new())
}

#[test]
fn an_open_request_user_input_call_is_a_waiting_desk_with_a_formal_row() {
    let arguments = r#"{\"questions\":[{\"question\":\"QUESTION-ONE\",\"options\":[{\"label\":\"OPTION-A\"},{\"label\":\"OPTION-B\"}]},{\"question\":\"QUESTION-TWO\"}]}"#;
    let call = format!(
        r#"{{"timestamp":"2026-11-20T10:00:03.000Z","type":"response_item","payload":{{"type":"function_call","call_id":"c1","name":"request_user_input","arguments":"{arguments}"}}}}"#
    );
    let d = codex_assemble(vec![codex_input(
        "x1",
        true,
        &[codex_event("task_started", r#","turn_id":"t1""#), call],
    )]);
    assert_eq!(d.sessions[0].state, SessionState::WaitingHuman);
    let [q] = d.questions.as_slice() else {
        panic!("one row, got {:?}", d.questions);
    };
    assert_eq!(q.id, "x1:c1");
    assert_eq!(q.prompt, "QUESTION-ONE (and 1 more in its terminal)");
    assert_eq!(q.priority, 0);
    assert_eq!(q.asked_at, "2026-11-20T10:00:03.000Z");
    let labels: Vec<&str> = q.options.iter().map(|o| o.label.as_str()).collect();
    assert_eq!(labels, ["OPTION-A", "OPTION-B"]);
    assert_eq!(
        d.sessions[0].pending_question_ids,
        vec!["x1:c1".to_string()]
    );
}

#[test]
fn a_codex_session_that_finished_cleanly_is_idle_with_no_row() {
    let records = [
        codex_event("task_started", r#","turn_id":"t1""#),
        codex_reply("All done."),
        codex_event("task_complete", r#","turn_id":"t1""#),
    ];
    let d = codex_assemble(vec![codex_input("x1", true, &records)]);
    assert_eq!(d.sessions[0].state, SessionState::Idle);
    assert!(d.questions.is_empty());
}

#[test]
fn a_final_message_ending_on_a_quoted_question_is_idle_with_no_row() {
    let records = [
        codex_event("task_started", r#","turn_id":"t1""#),
        codex_reply(r#"The test asks \"is it done?\""#),
        codex_event("task_complete", r#","turn_id":"t1""#),
    ];
    let d = codex_assemble(vec![codex_input("x1", true, &records)]);
    assert_eq!(d.sessions[0].state, SessionState::Idle);
    assert!(d.questions.is_empty());
}

#[test]
fn evidence_is_kept_for_live_codex_sessions_only() {
    let records = [codex_event("task_started", r#","turn_id":"t1""#)];
    let d = codex_assemble(vec![
        codex_input("live", true, &records),
        codex_input("gone", false, &records),
    ]);
    assert_eq!(d.codex.keys().collect::<Vec<_>>(), ["live"]);
    assert_eq!(d.codex["live"].turn_id.as_deref(), Some("t1"));
}

/// Probes counted by `counting_probe`.
static PROBES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn counting_probe(_: &Path, _: i32) -> crate::focus::Host {
    PROBES.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    crate::focus::Host::Nowhere
}

#[test]
fn host_of_probes_once_while_the_pid_stays_and_again_when_it_changes() {
    use std::sync::atomic::Ordering::SeqCst;
    let home = std::env::temp_dir().join(format!("ob-hostof-{}", std::process::id()));
    let sessions = home.join(".claude/sessions");
    std::fs::create_dir_all(&sessions).expect("dir");
    let register = |pid: u32| {
        let entry = format!(r#"{{"pid":{pid},"sessionId":"s-host","cwd":"/work/app"}}"#);
        std::fs::write(sessions.join("s-host.json"), entry).expect("write");
    };
    let mut discovery = Discovery::new(home.clone());
    discovery.ignore_running_codex();
    discovery.read_host = counting_probe;
    let before = PROBES.load(SeqCst);

    assert_eq!(discovery.host_of("s-host"), None, "no process, no host");
    register(std::process::id());
    assert_eq!(
        discovery.host_of("s-host"),
        Some(crate::focus::Host::Nowhere)
    );
    assert_eq!(
        discovery.host_of("s-host"),
        Some(crate::focus::Host::Nowhere)
    );
    assert_eq!(PROBES.load(SeqCst) - before, 1);

    register(std::os::unix::process::parent_id());
    assert!(discovery.host_of("s-host").is_some());
    assert_eq!(PROBES.load(SeqCst) - before, 2);
    let _ = std::fs::remove_dir_all(&home);
}
