use super::*;
use crate::model::{Helper, SessionState};
use std::io::{Cursor, Write};

fn parse(lines: &[&str]) -> Transcript {
    parse_transcript(Cursor::new(lines.join("\n")))
}

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ob-claude-code-{}-{}", tag, std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp dir");
    dir
}

const USER: &str = r#"{"type":"user","cwd":"/code/app","timestamp":"2026-09-29T10:00:00Z","message":{"role":"user","content":"fix the parser"}}"#;

#[test]
fn a_session_spends_what_its_sub_agents_spend() {
    let home = temp_dir("subagents");
    let project = home.join(".claude/projects/-code-app");
    let helpers = project.join("s1/subagents");
    fs::create_dir_all(helpers.join("workflows/w1")).expect("dirs");
    let reply = |id: &str, sidechain: bool| {
        format!(
            r#"{{"type":"assistant","isSidechain":{sidechain},"cwd":"/code/app","timestamp":"t","message":{{"id":"{id}","model":"claude-haiku-4-5","usage":{{"input_tokens":1000000}},"content":[{{"type":"text","text":"ok"}}]}}}}"#
        )
    };
    fs::write(
        project.join("s1.jsonl"),
        format!("{USER}\n{}\n", reply("m1", false)),
    )
    .expect("write");
    fs::write(
        helpers.join("agent-a.jsonl"),
        format!("{}\n", reply("m2", true)),
    )
    .expect("write");
    fs::write(
        helpers.join("workflows/w1/agent-b.jsonl"),
        format!("{}\n", reply("m3", true)),
    )
    .expect("write");

    let found = ClaudeSource::new(home.clone()).scan(SystemTime::now());
    assert_eq!(
        found.len(),
        1,
        "sub-agent files are not sessions of their own"
    );
    // Three messages of 1M input tokens on Haiku 4.5 at $1 per MTok.
    assert_eq!(found[0].transcript.spend.tokens, 3_000_000);
    assert!((found[0].transcript.spend.usd - 3.0).abs() < 1e-9);
    // The session's own state still ignores the helpers' records.
    assert_eq!(found[0].transcript.reply_to_human, "ok");
    fs::remove_dir_all(&home).expect("cleanup");
}

#[test]
fn a_sub_agent_is_finished_once_it_signs_off() {
    let ask = r#"{"type":"assistant","timestamp":"t","message":{"id":"m1","stop_reason":"tool_use","content":[{"type":"tool_use","id":"b1","name":"Bash","input":{}}]}}"#;
    let ran = r#"{"type":"user","timestamp":"t","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"b1","content":"ok"}]}}"#;
    let done = r#"{"type":"assistant","timestamp":"t","message":{"id":"m2","stop_reason":"end_turn","content":[{"type":"text","text":"Done."}]}}"#;
    let report = r#"{"type":"assistant","timestamp":"t","message":{"id":"m3","stop_reason":"tool_use","content":[{"type":"tool_use","id":"s1","name":"StructuredOutput","input":{}}]}}"#;
    let reported = r#"{"type":"user","timestamp":"t","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"s1","content":"ok"}]}}"#;
    assert!(!parse(&[ask]).finished);
    assert!(!parse(&[ask, ran]).finished);
    assert!(parse(&[ask, ran, done]).finished);
    // A workflow agent signs off by returning its structured result.
    assert!(!parse(&[ask, ran, report]).finished);
    assert!(parse(&[ask, ran, report, reported]).finished);
    // Messaged again after signing off, it is back at work.
    assert!(!parse(&[ask, ran, done, USER]).finished);
}

#[test]
fn running_sub_agents_are_the_sessions_helpers() {
    let home = temp_dir("helpers");
    let project = home.join(".claude/projects/-code-app");
    let helpers = project.join("s1/subagents");
    fs::create_dir_all(helpers.join("workflows/w1")).expect("dirs");
    let record = |stop: &str, block: &str| {
        format!(
            r#"{{"type":"assistant","isSidechain":true,"timestamp":"t","message":{{"id":"m","stop_reason":{stop},"content":[{block}]}}}}"#
        )
    };
    fs::write(project.join("s1.jsonl"), format!("{USER}\n")).expect("write");
    let tool = r#"{"type":"tool_use","id":"b1","name":"Bash","input":{}}"#;
    fs::write(
        helpers.join("agent-b.jsonl"),
        record("\"tool_use\"", tool) + "\n",
    )
    .expect("write");
    fs::write(
        helpers.join("agent-done.jsonl"),
        record("\"end_turn\"", r#"{"type":"text","text":"ok"}"#) + "\n",
    )
    .expect("write");
    fs::write(
        helpers.join("workflows/w1/agent-a.jsonl"),
        record("null", r#"{"type":"thinking","thinking":""}"#) + "\n",
    )
    .expect("write");

    let now = SystemTime::now();
    let mut source = ClaudeSource::new(home.clone());
    let found = source.scan(now);
    assert_eq!(
        found[0].helpers,
        vec![
            Helper {
                id: "agent-a".into(),
                state: SessionState::Thinking
            },
            Helper {
                id: "agent-b".into(),
                state: SessionState::Running
            },
        ]
    );
    // An agent that stopped writing long ago was killed, not left working.
    let later = now + Duration::from_mins(11);
    assert!(source.scan(later)[0].helpers.is_empty());
    fs::remove_dir_all(&home).expect("cleanup");
}

#[test]
fn reads_title_model_effort_and_tokens_once_per_message() {
    let t = parse(&[
        r#"{"type":"ai-title","aiTitle":"Fix the parser","sessionId":"s"}"#,
        USER,
        r#"{"type":"assistant","cwd":"/code/app","timestamp":"2026-09-29T10:00:05Z","effort":"high","message":{"id":"m1","model":"claude-opus-5-5","usage":{"input_tokens":10,"cache_creation_input_tokens":5,"cache_read_input_tokens":900,"output_tokens":20},"content":[{"type":"thinking","thinking":"..."}]}}"#,
        r#"{"type":"assistant","cwd":"/code/app","timestamp":"2026-09-29T10:00:06Z","effort":"high","message":{"id":"m1","model":"claude-opus-5-5","usage":{"input_tokens":10,"cache_creation_input_tokens":5,"cache_read_input_tokens":900,"output_tokens":20},"content":[{"type":"text","text":"Done. Tests pass."}]}}"#,
    ]);
    assert_eq!(t.title.as_deref(), Some("Fix the parser"));
    assert_eq!(t.model.as_deref(), Some("claude-opus-5-5"));
    assert_eq!(t.effort.as_deref(), Some("high"));
    assert_eq!(t.cwd.as_deref(), Some("/code/app"));
    assert_eq!(t.spend.tokens, 35);
    // claude-opus-5-5: 10 in x $4 + 5 written x $5 + 900 read x $0.20 + 20 out x $20, per MTok.
    assert!(
        (t.spend.usd - (40.0 + 25.0 + 180.0 + 400.0) / 1_000_000.0).abs() < 1e-12,
        "{}",
        t.spend.usd
    );
    assert_eq!(t.last_block, LastBlock::Text);
    assert_eq!(t.reply_to_human, "Done. Tests pass.");
    assert_eq!(t.last_activity.as_deref(), Some("2026-09-29T10:00:06Z"));
    assert!(!t.human_spoke_last);
}

#[test]
fn open_ask_user_question_is_pending_until_answered() {
    let ask = r#"{"type":"assistant","timestamp":"2026-09-29T10:01:00Z","message":{"id":"m2","content":[{"type":"tool_use","id":"toolu_1","name":"AskUserQuestion","input":{"questions":[{"question":"Which backend?","header":"Backend","multiSelect":false,"options":[{"label":"Rust","description":"own"},{"label":"CCC","description":"theirs"}]}]}}]}}"#;
    let pending = parse(&[USER, ask]);
    let q = pending.question.expect("pending question");
    assert_eq!(q.prompt, "Which backend?");
    assert_eq!(q.options, vec!["Rust".to_string(), "CCC".to_string()]);
    assert_eq!(q.asked_at, "2026-09-29T10:01:00Z");
    assert_eq!(pending.open_tools, 1);

    let answered = parse(&[
        USER,
        ask,
        r#"{"type":"user","timestamp":"2026-09-29T10:02:00Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"Rust"}]}}"#,
    ]);
    assert!(answered.question.is_none());
    assert_eq!(answered.open_tools, 0);
    // A tool result is not the human speaking.
    assert!(!answered.human_spoke_last);
}

#[test]
fn skips_sidechains_meta_and_garbage() {
    let t = parse(&[
        USER,
        r#"{"type":"assistant","isSidechain":true,"message":{"id":"x","content":[{"type":"tool_use","id":"t9","name":"Bash","input":{}}]}}"#,
        r#"{"type":"user","isMeta":true,"message":{"role":"user","content":"<meta>"}}"#,
        "not json",
        r#"{"type":"assistant","message":{"id":"m3","model":"<synthetic>","content":[{"type":"text","text":"ok"}]},"isApiErrorMessage":true}"#,
    ]);
    assert_eq!(t.open_tools, 0);
    assert!(t.api_error);
    assert_eq!(t.model, None);
    assert!(!t.human_spoke_last);
}

#[test]
fn a_system_turn_does_not_bury_the_question_asked_to_the_human() {
    let asked = r#"{"type":"assistant","timestamp":"2026-09-29T19:52:18Z","message":{"id":"a1","content":[{"type":"text","text":"Pushed. Should the TODO panels read each repo's TODO.md?"}]}}"#;
    let notification = r#"{"type":"user","origin":{"kind":"task-notification"},"promptSource":"system","timestamp":"2026-09-29T19:59:40Z","message":{"role":"user","content":"<task-notification>monitor expired</task-notification>"}}"#;
    let aside = r#"{"type":"assistant","timestamp":"2026-09-29T19:59:47Z","message":{"id":"a2","content":[{"type":"text","text":"That monitor expired; nothing to do."}]}}"#;
    let t = parse(&[
        r#"{"type":"user","origin":{"kind":"human"},"message":{"role":"user","content":"push it"}}"#,
        asked,
        notification,
        aside,
    ]);
    assert_eq!(
        t.reply_to_human,
        "Pushed. Should the TODO panels read each repo's TODO.md?"
    );
    assert!(!t.human_spoke_last);

    let human = r#"{"type":"user","origin":{"kind":"human"},"message":{"role":"user","content":"yes, read TODO.md"}}"#;
    let replied = parse(&[asked, notification, aside, human]);
    assert!(replied.human_spoke_last);
    assert_eq!(replied.reply_to_human, "");
}

#[test]
fn registry_entries_parse_and_filter_by_liveness() {
    let e = parse_registry_entry(
        r#"{"pid":42,"sessionId":"abc","cwd":"/code/app","status":"busy","name":"fix"}"#,
    )
    .expect("entry");
    assert_eq!(
        e,
        LiveEntry {
            pid: 42,
            session_id: "abc".into(),
            cwd: "/code/app".into(),
            busy: true,
            waiting: false,
            name: Some("fix".into())
        }
    );
    let waiting = parse_registry_entry(
        r#"{"pid":42,"sessionId":"abc","cwd":"/code/app","status":"waiting","waitingFor":"dialog open"}"#,
    )
    .expect("waiting entry");
    assert!(!waiting.busy);
    assert!(waiting.waiting);
    assert!(parse_registry_entry(r#"{"pid":"x"}"#).is_none());

    let dir = std::env::temp_dir().join(format!("ob-registry-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("temp dir");
    fs::write(
        dir.join("1.json"),
        r#"{"pid":1,"sessionId":"a","cwd":"/a","status":"idle"}"#,
    )
    .expect("write");
    fs::write(
        dir.join("2.json"),
        r#"{"pid":2,"sessionId":"b","cwd":"/b","status":"idle"}"#,
    )
    .expect("write");
    fs::write(dir.join("x.key"), "ignored").expect("write");
    let live = read_registry(&dir, |pid| pid == 2);
    fs::remove_dir_all(&dir).expect("cleanup");
    assert_eq!(
        live.iter()
            .map(|e| e.session_id.as_str())
            .collect::<Vec<_>>(),
        vec!["b"]
    );
}

#[test]
fn this_process_is_alive_and_a_bogus_pid_is_not() {
    assert!(pid_alive(i32::try_from(std::process::id()).unwrap()));
    assert!(!pid_alive(-1));
}

fn full_text(lines: &[&str]) -> String {
    let mut s = String::new();
    for l in lines {
        s.push_str(l);
        s.push('\n');
    }
    s
}

#[test]
fn incremental_parse_matches_full_parse_at_every_split_point() {
    let ask = r#"{"type":"assistant","timestamp":"2026-09-29T10:01:00Z","message":{"id":"m2","content":[{"type":"tool_use","id":"toolu_1","name":"AskUserQuestion","input":{"questions":[{"question":"Which backend?","header":"Backend","multiSelect":false,"options":[{"label":"Rust","description":"own"},{"label":"CCC","description":"theirs"}]}]}}]}}"#;
    let answer = r#"{"type":"user","timestamp":"2026-09-29T10:02:00Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"Rust"}]}}"#;
    let asked = r#"{"type":"assistant","timestamp":"2026-09-29T19:52:18Z","message":{"id":"a1","content":[{"type":"text","text":"Pushed. Should the TODO panels read each repo's TODO.md?"}]}}"#;
    let notification = r#"{"type":"user","origin":{"kind":"task-notification"},"promptSource":"system","timestamp":"2026-09-29T19:59:40Z","message":{"role":"user","content":"<task-notification>monitor expired</task-notification>"}}"#;
    let aside = r#"{"type":"assistant","timestamp":"2026-09-29T19:59:47Z","message":{"id":"a2","content":[{"type":"text","text":"That monitor expired; nothing to do."}]}}"#;
    let human = r#"{"type":"user","origin":{"kind":"human"},"message":{"role":"user","content":"yes, read TODO.md"}}"#;

    let fixtures: Vec<Vec<&str>> = vec![
        vec![
            r#"{"type":"ai-title","aiTitle":"Fix the parser","sessionId":"s"}"#,
            USER,
            r#"{"type":"assistant","cwd":"/code/app","timestamp":"2026-09-29T10:00:05Z","effort":"high","message":{"id":"m1","model":"claude-opus-5-5","usage":{"input_tokens":10,"cache_creation_input_tokens":5,"cache_read_input_tokens":900,"output_tokens":20},"content":[{"type":"thinking","thinking":"..."}]}}"#,
            r#"{"type":"assistant","cwd":"/code/app","timestamp":"2026-09-29T10:00:06Z","effort":"high","message":{"id":"m1","model":"claude-opus-5-5","usage":{"input_tokens":10,"cache_creation_input_tokens":5,"cache_read_input_tokens":900,"output_tokens":20},"content":[{"type":"text","text":"Done. Tests pass."}]}}"#,
        ],
        vec![USER, ask, answer],
        vec![
            USER,
            r#"{"type":"assistant","isSidechain":true,"message":{"id":"x","content":[{"type":"tool_use","id":"t9","name":"Bash","input":{}}]}}"#,
            r#"{"type":"user","isMeta":true,"message":{"role":"user","content":"<meta>"}}"#,
            "not json",
            r#"{"type":"assistant","message":{"id":"m3","model":"<synthetic>","content":[{"type":"text","text":"ok"}]},"isApiErrorMessage":true}"#,
        ],
        vec![asked, notification, aside, human],
    ];

    for fixture in &fixtures {
        let text = full_text(fixture);
        let bytes = text.as_bytes();

        let mut full_state = ParserState::new();
        full_state.feed_lines(Cursor::new(bytes.to_vec()), 0, false);
        let full = full_state.finish();

        let mut cut = 0u64;
        let mut cuts = vec![0u64];
        for l in fixture {
            cut += u64::try_from(l.len()).unwrap() + 1;
            cuts.push(cut);
        }

        for &c in &cuts {
            let mut state = ParserState::new();
            state.feed_lines(
                Cursor::new(bytes[..usize::try_from(c).unwrap()].to_vec()),
                0,
                true,
            );
            state.feed_lines(
                Cursor::new(bytes[usize::try_from(c).unwrap()..].to_vec()),
                c,
                true,
            );
            assert_eq!(state.finish(), full, "split at byte {c} for {fixture:?}");
        }
    }
}

#[test]
fn partial_trailing_line_is_deferred_then_consumed_on_next_scan() {
    let dir = temp_dir("partial");
    let path = dir.join("s.jsonl");
    fs::write(&path, format!("{USER}\n")).expect("write");

    let mut src = ClaudeSource::new(dir.clone());
    let t1 = src.transcript(&path).expect("first parse");
    assert_eq!(t1.cwd.as_deref(), Some("/code/app"));
    assert_eq!(t1.reply_to_human, "");

    let full_line = r#"{"type":"assistant","timestamp":"2026-09-29T10:01:00Z","message":{"id":"m2","content":[{"type":"text","text":"hello"}]}}"#;
    let (first_half, second_half) = full_line.split_at(full_line.len() / 2);

    // Write a trailing partial line (no newline yet): must not be consumed.
    {
        let mut f = fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .expect("open");
        f.write_all(first_half.as_bytes()).expect("write");
    }
    let t2 = src.transcript(&path).expect("second parse");
    assert_eq!(
        t2, t1,
        "a partial trailing line must not change the transcript"
    );

    // Complete the line: it should now be picked up.
    {
        let mut f = fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .expect("open");
        f.write_all(second_half.as_bytes()).expect("write");
        f.write_all(b"\n").expect("write");
    }
    let t3 = src.transcript(&path).expect("third parse");
    assert_eq!(t3.last_block, LastBlock::Text);
    assert_eq!(t3.reply_to_human, "hello");

    fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn growth_reads_only_the_new_bytes() {
    let dir = temp_dir("growth");
    let path = dir.join("s.jsonl");
    // Long enough that the front of the line (where `cwd` lives) sits
    // well outside the last `TAIL_WINDOW` bytes before the offset, so
    // corrupting it exercises the "not re-read" claim rather than the
    // tail-window staleness check (see `shrunk_or_replaced_file_...`
    // and `same_inode_truncate_and_rewrite_longer_is_reparsed` for that).
    let padding = "x".repeat(6000);
    let long_user = format!(
        r#"{{"type":"user","cwd":"/code/app","timestamp":"t0","message":{{"role":"user","content":"{padding}"}}}}"#
    );
    fs::write(&path, format!("{long_user}\n")).expect("write");
    assert!(
        u64::try_from(long_user.len()).unwrap() > TAIL_WINDOW + 1000,
        "fixture must be well past the tail window"
    );

    let mut src = ClaudeSource::new(dir.clone());
    let t1 = src.transcript(&path).expect("first parse");
    assert_eq!(t1.cwd.as_deref(), Some("/code/app"));
    let offset1 = src.cache.get(&path).expect("cached").offset;
    assert_eq!(offset1, u64::try_from(long_user.len() + 1).unwrap());

    // Corrupt the front of the already-consumed prefix in place (same
    // length, so this does not change the file's size or move the tail
    // window) and append a new, valid line. If the incremental path is
    // honest about only reading from `offset1` onward, this corruption —
    // which would flip `cwd` if it were re-parsed — is never seen, and
    // the cached `cwd` survives untouched.
    let corrupted_line = long_user.replace("/code/app", "/EVIL/xxx");
    assert_eq!(corrupted_line.len(), long_user.len());
    {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .expect("open");
        f.write_all(corrupted_line.as_bytes())
            .expect("corrupt prefix");
    }
    let ask = r#"{"type":"assistant","timestamp":"2026-09-29T10:01:00Z","message":{"id":"m2","content":[{"type":"tool_use","id":"toolu_1","name":"AskUserQuestion","input":{"questions":[{"question":"Which backend?","header":"Backend","multiSelect":false,"options":[{"label":"Rust","description":"own"}]}]}}]}}"#;
    {
        let mut f = fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .expect("open");
        f.write_all(format!("{ask}\n").as_bytes()).expect("append");
    }

    let t2 = src.transcript(&path).expect("second parse");
    assert_eq!(
        t2.cwd.as_deref(),
        Some("/code/app"),
        "the corrupted prefix must not have been re-read"
    );
    assert!(t2.question.is_some());
    let offset2 = src.cache.get(&path).expect("cached").offset;
    assert_eq!(offset2, offset1 + u64::try_from(ask.len()).unwrap() + 1);

    fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn shrunk_or_replaced_file_triggers_a_full_reparse() {
    let dir = temp_dir("shrink");
    let path = dir.join("s.jsonl");
    let ask = r#"{"type":"assistant","timestamp":"2026-09-29T10:01:00Z","message":{"id":"m2","content":[{"type":"tool_use","id":"toolu_1","name":"AskUserQuestion","input":{"questions":[{"question":"Which backend?","header":"Backend","multiSelect":false,"options":[{"label":"Rust","description":"own"}]}]}}]}}"#;
    fs::write(&path, format!("{USER}\n{ask}\n")).expect("write");

    let mut src = ClaudeSource::new(dir.clone());
    let t1 = src.transcript(&path).expect("first parse");
    assert!(t1.question.is_some());

    // Shrink: truncate back to just the first line.
    fs::write(&path, format!("{USER}\n")).expect("write");
    let t2 = src.transcript(&path).expect("second parse");
    assert!(
        t2.question.is_none(),
        "a shrunk file must be fully re-parsed, not read from a stale offset"
    );

    // Replace: a new file at the same path (a different inode), longer
    // than the previous offset, with different content throughout.
    let other = r#"{"type":"user","cwd":"/other/dir","timestamp":"2026-09-29T11:00:00Z","message":{"role":"user","content":"different session entirely, unrelated to the file this path used to name"}}"#;
    fs::remove_file(&path).expect("remove");
    fs::write(&path, format!("{other}\n")).expect("write");
    let t3 = src.transcript(&path).expect("third parse");
    assert_eq!(
        t3.cwd.as_deref(),
        Some("/other/dir"),
        "a replaced file (different inode) must be fully re-parsed"
    );

    fs::remove_dir_all(&dir).expect("cleanup");
}
