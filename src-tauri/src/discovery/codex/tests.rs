use super::*;
use std::io::Cursor;

fn parse(lines: &[&str]) -> Transcript {
    parse_transcript(Cursor::new(lines.join("\n")))
}

const META: &str = r#"{"timestamp":"2026-09-29T10:00:00.000Z","type":"session_meta","payload":{"id":"s1","session_id":"root1","cwd":"/code/app","source":"cli"}}"#;
const TURN_CTX: &str = r#"{"timestamp":"2026-09-29T10:00:01.000Z","type":"turn_context","payload":{"cwd":"/code/app","model":"gpt-5-codex","effort":"high"}}"#;
const USER: &str = r#"{"timestamp":"2026-09-29T10:00:02.000Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"fix the parser"}]}}"#;

#[test]
fn reads_cwd_model_effort_and_reply() {
    let started = r#"{"timestamp":"2026-09-29T10:00:03.000Z","type":"event_msg","payload":{"type":"task_started","turn_id":"t1"}}"#;
    let reasoning = r#"{"timestamp":"2026-09-29T10:00:04.000Z","type":"response_item","payload":{"type":"reasoning","summary":[]}}"#;
    let reply = r#"{"timestamp":"2026-09-29T10:00:05.000Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Done. Tests pass."}]}}"#;
    let complete = r#"{"timestamp":"2026-09-29T10:00:06.000Z","type":"event_msg","payload":{"type":"task_complete","turn_id":"t1","last_agent_message":"Done."}}"#;
    let tokens = r#"{"timestamp":"2026-09-29T10:00:06.000Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":40,"cached_input_tokens":10,"output_tokens":2,"total_tokens":42}}}}"#;
    let t = parse(&[
        META, TURN_CTX, USER, started, reasoning, reply, complete, tokens,
    ]);
    assert_eq!(t.cwd.as_deref(), Some("/code/app"));
    assert_eq!(t.model.as_deref(), Some("gpt-5-codex"));
    assert_eq!(t.effort.as_deref(), Some("high"));
    assert_eq!(t.root_session_id.as_deref(), Some("root1"));
    assert_eq!(t.reply_to_human, "Done. Tests pass.");
    assert_eq!(t.spend.tokens, 32);
    assert!(!t.turn_open);
    assert!(!t.api_error);
    assert_eq!(t.last_activity.as_deref(), Some("2026-09-29T10:00:06.000Z"));
    assert!(!t.human_spoke_last);
    assert_eq!(t.kin, children::Kin::Desk);
}

#[test]
fn a_trailing_prose_question_scores_as_a_soft_block() {
    let reply = r#"{"timestamp":"2026-09-29T10:00:05.000Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Fixed it.\n\nShould I also bump the version?"}]}}"#;
    let t = parse(&[META, USER, reply]);
    let s = signals(&t, true);
    assert_eq!(
        crate::discovery::rules::derive_state(&s),
        crate::model::SessionState::WaitingHuman
    );
    assert_eq!(
        super::super::rules::score_soft_block(&t.reply_to_human).question,
        "Should I also bump the version?"
    );
}

#[test]
fn the_last_reply_survives_the_human_speaking_again() {
    let reply = r#"{"timestamp":"2026-09-29T10:00:05.000Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Parser fixed. Should I proceed?"}]}}"#;
    let next_human = r#"{"timestamp":"2026-09-29T10:01:00.000Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"yes"}]}}"#;
    let t = parse(&[META, USER, reply, next_human]);
    assert_eq!(t.reply_to_human, "");
    assert_eq!(t.last_reply.as_deref(), Some("Parser fixed."));
}

#[test]
fn subagent_thread_is_flagged_and_dropped() {
    let sub_meta = r#"{"timestamp":"2026-09-29T10:00:00.000Z","type":"session_meta","payload":{"id":"s2","session_id":"root1","cwd":"/code/app","source":{"subagent":{"thread_spawn":{"parent_thread_id":"root1"}}}}}"#;
    let t = parse(&[sub_meta]);
    assert_eq!(
        t.kin,
        children::Kin::Worker {
            parent: "root1".to_string()
        }
    );
}

#[test]
fn human_speaking_again_clears_the_prior_reply() {
    let reply = r#"{"timestamp":"2026-09-29T10:00:05.000Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Should I proceed?"}]}}"#;
    let next_human = r#"{"timestamp":"2026-09-29T10:01:00.000Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"yes"}]}}"#;
    let t = parse(&[META, USER, reply, next_human]);
    assert!(t.human_spoke_last);
    assert_eq!(t.reply_to_human, "");
}

#[test]
fn title_index_keeps_the_last_line_per_id() {
    let dir = std::env::temp_dir().join(format!("ob-codex-index-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("session_index.jsonl");
    fs::write(
        &path,
        "{\"id\":\"root1\",\"thread_name\":\"Example thread one\",\"updated_at\":\"2000-01-01T00:00:00Z\"}\n\
         {\"id\":\"root1\",\"thread_name\":\"Example thread renamed\",\"updated_at\":\"2000-01-01T00:05:00Z\"}\n\
         not json\n",
    )
    .expect("write");
    let index = read_title_index(&path);
    fs::remove_dir_all(&dir).expect("cleanup");
    assert_eq!(
        index.get("root1").map(String::as_str),
        Some("Example thread renamed")
    );
}

#[test]
fn cache_reuses_a_parsed_transcript_when_the_file_is_unchanged() {
    let dir = std::env::temp_dir().join(format!("ob-codex-cache-{}", std::process::id()));
    fs::create_dir_all(dir.join(".codex/sessions/2026/09/29")).expect("temp dir");
    let path = dir.join(".codex/sessions/2026/09/29/rollout-2026-09-29T10-00-00-deadbeef-0000-4000-8000-000000000000.jsonl");
    fs::write(&path, format!("{META}\n{TURN_CTX}\n{USER}\n")).expect("write");

    let mut source = CodexSource::new(dir.clone());
    let first = source.scan_with(SystemTime::now(), Vec::new(), &[]);
    assert_eq!(first.len(), 1);
    assert_eq!(source.cache.len(), 1);
    let cached_before = source.cache.values().next().cloned();

    let second = source.scan_with(SystemTime::now(), Vec::new(), &[]);
    assert_eq!(second.len(), 1);
    assert_eq!(source.cache.values().next().cloned(), cached_before);

    fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn spend_prices_each_token_count_once_at_the_model_then_in_use() {
    let ctx = |model: &str| {
        format!(
            r#"{{"timestamp":"t","type":"turn_context","payload":{{"cwd":"/code/app","model":"{model}"}}}}"#
        )
    };
    let count = |input: u64, output: u64| {
        format!(
            r#"{{"timestamp":"t","type":"event_msg","payload":{{"type":"token_count","info":{{"total_token_usage":{{"input_tokens":{input},"cached_input_tokens":0,"output_tokens":{output}}},"last_token_usage":{{"input_tokens":1}}}}}}}}"#
        )
    };
    let astra = ctx("gpt-6-astra");
    let luna = ctx("gpt-6-luna");
    let (first, resent, second) = (
        count(1_000_000, 0),
        count(1_000_000, 0),
        count(2_000_000, 1_000_000),
    );
    let t = parse(&[META, &astra, &first, &resent, &luna, &second]);
    // 1M input on astra ($10), then 1M input + 1M output on luna ($0.10 + $0.50).
    assert!((t.spend.usd - 10.6).abs() < 1e-9, "{}", t.spend.usd);
    assert_eq!(t.spend.tokens, 3_000_000);
    assert_eq!(t.spend.unpriced_tokens, 0);
}

#[test]
fn a_thread_spends_what_its_sub_agents_spend_at_any_depth() {
    let dir = std::env::temp_dir().join(format!("ob-codex-subagents-{}", std::process::id()));
    let day = dir.join(".codex/sessions/2026/09/29");
    fs::create_dir_all(&day).expect("temp dir");
    let uuid = |n: u32| format!("deadbeef-0000-4000-8000-{n:012}");
    let meta = |id: &str, parent: Option<&str>| match parent {
        None => format!(
            r#"{{"timestamp":"t","type":"session_meta","payload":{{"id":"{id}","session_id":"{id}","cwd":"/code/app","source":"cli"}}}}"#
        ),
        Some(p) => format!(
            r#"{{"timestamp":"t","type":"session_meta","payload":{{"id":"{id}","session_id":"{id}","parent_thread_id":"{p}","cwd":"/code/app","source":{{"subagent":{{"other":"guardian"}}}}}}}}"#
        ),
    };
    let spent = r#"{"timestamp":"t","type":"turn_context","payload":{"cwd":"/code/app","model":"gpt-6-luna"}}
{"timestamp":"t","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1000000,"output_tokens":0}}}}"#;
    let (top, child, grandchild) = (uuid(1), uuid(2), uuid(3));
    for (id, parent) in [
        (&top, None),
        (&child, Some(top.as_str())),
        (&grandchild, Some(child.as_str())),
    ] {
        fs::write(
            day.join(format!("rollout-2026-09-29T10-00-00-{id}.jsonl")),
            format!("{}\n{spent}\n", meta(id, parent)),
        )
        .expect("write");
    }

    let found = CodexSource::new(dir.clone()).scan_with(SystemTime::now(), Vec::new(), &[]);
    assert_eq!(found.len(), 1);
    // Three threads of 1M input tokens on gpt-6-luna at $0.10 per MTok.
    assert!(
        (found[0].transcript.spend.usd - 0.3).abs() < 1e-9,
        "{}",
        found[0].transcript.spend.usd
    );
    assert_eq!(found[0].transcript.spend.tokens, 3_000_000);
    fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn busy_subagents_are_counted_per_top_level_thread() {
    let dir = std::env::temp_dir().join(format!("ob-codex-busy-{}", std::process::id()));
    let day = dir.join(".codex/sessions/2026/09/29");
    fs::create_dir_all(&day).expect("temp dir");
    let uuid = |n: u32| format!("deadbeef-0000-4000-8000-{n:012}");
    let meta = |id: &str, parent: Option<&str>| match parent {
        None => format!(
            r#"{{"timestamp":"t","type":"session_meta","payload":{{"id":"{id}","session_id":"{id}","cwd":"/code/app","source":"cli"}}}}"#
        ),
        Some(p) => format!(
            r#"{{"timestamp":"t","type":"session_meta","payload":{{"id":"{id}","session_id":"{id}","parent_thread_id":"{p}","cwd":"/code/app","source":{{"subagent":{{"other":"guardian"}}}}}}}}"#
        ),
    };
    let open =
        r#"{"timestamp":"t","type":"event_msg","payload":{"type":"task_started","turn_id":"t1"}}"#;
    let done =
        r#"{"timestamp":"t","type":"event_msg","payload":{"type":"task_complete","turn_id":"t1"}}"#;
    let (top, other, a, b, stale, nested, ahead) = (
        uuid(1),
        uuid(2),
        uuid(3),
        uuid(4),
        uuid(5),
        uuid(6),
        uuid(7),
    );
    let now = SystemTime::now();
    let old = now - Duration::from_hours(13);
    for (id, parent, last, modified) in [
        (&top, None, None, now),
        (&other, None, None, now),
        (&a, Some(top.as_str()), Some(open), now),
        (&b, Some(top.as_str()), Some(done), now),
        (&stale, Some(top.as_str()), Some(open), old),
        (&nested, Some(a.as_str()), Some(open), now),
        // written after the scan's clock was read
        (
            &ahead,
            Some(top.as_str()),
            Some(open),
            now + Duration::from_secs(1),
        ),
    ] {
        let path = day.join(format!("rollout-2026-09-29T10-00-00-{id}.jsonl"));
        let text = [Some(meta(id, parent)), last.map(str::to_string)]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(&path, format!("{text}\n")).expect("write");
        fs::File::options()
            .write(true)
            .open(&path)
            .and_then(|f| f.set_modified(modified))
            .expect("mtime");
    }

    let mut discovery = crate::discovery::Discovery::new(dir.clone());
    discovery.codex.scan_with(now, Vec::new(), &[]);
    assert_eq!(discovery.codex_subagents_busy(&top), 3);
    assert_eq!(discovery.codex_subagents_busy(&other), 0);
    assert_eq!(discovery.codex_subagents_busy("unknown"), 0);
    fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn session_uuid_reads_the_filename_tail() {
    assert_eq!(
        session_uuid("rollout-2026-09-29T10-00-00-deadbeef-0000-4000-8000-000000000000"),
        Some("deadbeef-0000-4000-8000-000000000000".to_string())
    );
    assert_eq!(session_uuid("not-a-uuid"), None);
}

// VERIFIER (failing on purpose): a sub-agent/forked rollout carries its own session_meta first
// and a copy of the parent's (source "cli") later. The first one must decide.
#[test]
fn verifier_subagent_with_copied_parent_meta_stays_a_subagent() {
    let sub_meta = r#"{"timestamp":"2026-09-29T10:00:00.000Z","type":"session_meta","payload":{"id":"s2","session_id":"root1","cwd":"/code/app","source":{"subagent":{"thread_spawn":{"parent_thread_id":"root1"}}}}}"#;
    let parent_meta = r#"{"timestamp":"2026-09-29T10:00:00.500Z","type":"session_meta","payload":{"id":"root1","session_id":"root1","cwd":"/code/app","source":"cli"}}"#;
    let t = parse(&[sub_meta, parent_meta]);
    assert_eq!(
        t.kin,
        children::Kin::Worker {
            parent: "root1".to_string()
        }
    );
}

// A running codex with no rollout (here this very test process, standing in for one, as
// `lsof` and `ps` really are run on it) gets a placeholder desk that is live but not resumable.
#[test]
fn a_running_process_with_no_rollout_gets_a_placeholder_session() {
    let dir = std::env::temp_dir().join(format!("ob-codex-placeholder-{}", std::process::id()));
    fs::create_dir_all(dir.join(".codex/sessions")).expect("temp dir");
    let pid = i32::try_from(std::process::id()).expect("pid fits");

    let found = CodexSource::new(dir.clone()).scan_with(SystemTime::now(), vec![pid], &[]);
    fs::remove_dir_all(&dir).expect("cleanup");

    let [placeholder] = found.as_slice() else {
        panic!("expected one placeholder, got {found:?}");
    };
    assert_eq!(placeholder.session_id, format!("codex-new-{pid}"));
    assert_eq!(placeholder.title.as_deref(), Some("New Codex session"));
    assert_eq!(placeholder.live_pid, Some(pid));
    assert!(!placeholder.resumable);
    assert_eq!(
        placeholder.transcript.cwd.as_deref(),
        std::env::current_dir()
            .ok()
            .and_then(|d| d.to_str().map(str::to_string))
            .as_deref()
    );
}

/// Writes one rollout under `day` with the given lines and modified time.
fn write_rollout(day: &Path, id: &str, lines: &[String], modified: SystemTime) {
    let path = day.join(format!("rollout-2026-09-29T10-00-00-{id}.jsonl"));
    fs::write(&path, format!("{}\n", lines.join("\n"))).expect("write");
    fs::File::options()
        .write(true)
        .open(&path)
        .and_then(|f| f.set_modified(modified))
        .expect("mtime");
}

// M2b spec 10.1: a worker whose only parent link is the nested one (0.161.0's encoding, G5)
// now adds its spend and its busy flag to its root (D6), and a plain source with a top-level
// parent link is a worker, never a desk (D2).
#[test]
fn a_parent_link_in_either_place_rolls_up_and_never_makes_a_desk() {
    let dir = std::env::temp_dir().join(format!("ob-codex-kin-{}", std::process::id()));
    let day = dir.join(".codex/sessions/2026/09/29");
    fs::create_dir_all(&day).expect("temp dir");
    let uuid = |n: u32| format!("0199c0de-0000-4000-8000-{n:012}");
    let (root, nested_only, plain_linked) = (uuid(1), uuid(2), uuid(3));
    let meta = |id: &str, source: &str, top_link: &str| {
        format!(
            r#"{{"timestamp":"t","type":"session_meta","payload":{{"id":"{id}","session_id":"{root}","cwd":"/code/app","source":{source}{top_link}}}}}"#
        )
    };
    let spent = r#"{"timestamp":"t","type":"turn_context","payload":{"cwd":"/code/app","model":"gpt-6-luna"}}
{"timestamp":"t","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1000000,"output_tokens":0}}}}"#
        .to_string();
    let open =
        r#"{"timestamp":"t","type":"event_msg","payload":{"type":"task_started","turn_id":"t1"}}"#
            .to_string();
    let now = SystemTime::now();
    write_rollout(
        &day,
        &root,
        &[meta(&root, r#""cli""#, ""), spent.clone()],
        now,
    );
    write_rollout(
        &day,
        &nested_only,
        &[
            meta(
                &nested_only,
                &format!(
                    r#"{{"subagent":{{"thread_spawn":{{"parent_thread_id":"{root}","depth":1}}}}}}"#
                ),
                "",
            ),
            spent.clone(),
            open.clone(),
        ],
        now,
    );
    write_rollout(
        &day,
        &plain_linked,
        &[
            meta(
                &plain_linked,
                r#""cli""#,
                &format!(r#","parent_thread_id":"{root}""#),
            ),
            spent,
            open,
        ],
        now,
    );

    let mut discovery = crate::discovery::Discovery::new(dir.clone());
    let found = discovery.codex.scan_with(now, Vec::new(), &[]);
    fs::remove_dir_all(&dir).expect("cleanup");

    let ids: Vec<&str> = found.iter().map(|f| f.session_id.as_str()).collect();
    assert_eq!(ids, [root.as_str()], "only the root gets a desk");
    assert_eq!(
        found[0].transcript.spend.tokens, 3_000_000,
        "both workers' spend"
    );
    assert_eq!(
        discovery.codex_subagents_busy(&root),
        2,
        "both workers busy"
    );
}

// M2b spec 10.1: kin, tellings and the turn come out of one parse together.
#[test]
fn a_rollout_gives_its_kin_its_tellings_and_its_turn() {
    let meta = r#"{"timestamp":"2026-09-29T10:00:00.000Z","type":"session_meta","payload":{"id":"0199c0de-0000-4000-8000-000000000001","session_id":"0199c0de-0000-4000-8000-000000000001","cwd":"/code/app","source":"cli"}}"#;
    // G6a, a legacy rollout's `started`.
    let started = r#"{"timestamp":"2026-09-29T10:00:01.000Z","type":"event_msg","payload":{"type":"sub_agent_activity","event_id":"e1","occurred_at_ms":1,"agent_thread_id":"0199c0de-0000-4000-8000-000000000002","agent_path":"/root/worker_one","kind":"started"}}"#;
    let turn = r#"{"timestamp":"2026-09-29T10:00:02.000Z","type":"event_msg","payload":{"type":"task_started","turn_id":"t1"}}"#;
    let t = parse(&[meta, started, turn]);
    assert_eq!(t.kin, children::Kin::Desk);
    assert!(t.turn_open);
    assert_eq!(
        t.told,
        [children::Told {
            thread: "0199c0de-0000-4000-8000-000000000002".to_string(),
            says: children::Says::Working,
            at: "2026-09-29T10:00:01.000Z".to_string(),
            called: Some("worker_one".to_string()),
            model: None,
            effort: None,
        }]
    );
}
