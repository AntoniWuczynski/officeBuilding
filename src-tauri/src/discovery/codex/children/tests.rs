use super::*;
use serde_json::json;

const P: &str = "0199c0de-0000-4000-8000-000000000001";
const Q: &str = "0199c0de-0000-4000-8000-000000000002";
const A: &str = "0199c0de-0000-4000-8000-00000000000a";
const B: &str = "0199c0de-0000-4000-8000-00000000000b";
const C: &str = "0199c0de-0000-4000-8000-00000000000c";

fn worker(parent: &str) -> Kin {
    Kin::Worker {
        parent: parent.to_string(),
    }
}

fn internal(parent: Option<&str>) -> Kin {
    Kin::Internal {
        parent: parent.map(str::to_string),
    }
}

/// The kin table of spec 10.1, over the rows of 4.1 (G1 to G3, G5).
#[test]
fn the_first_session_meta_says_what_a_thread_is() {
    let cases: Vec<(&str, Value, Kin)> = vec![
        ("plain cli", json!({"source": "cli"}), Kin::Desk),
        (
            "a user's fork",
            json!({"source": "cli", "forked_from_id": P}),
            Kin::Desk,
        ),
        ("no source at all", json!({"cwd": "/code/app"}), Kin::Desk),
        ("null source", json!({"source": null}), Kin::Desk),
        (
            "0.161.0's nested link only",
            json!({"source": {"subagent": {"thread_spawn": {"parent_thread_id": P, "depth": 1}}}}),
            worker(P),
        ),
        (
            "0.170.0's both links (G5)",
            json!({"source": {"subagent": {"thread_spawn": {"parent_thread_id": P, "depth": 1}}}, "parent_thread_id": P}),
            worker(P),
        ),
        (
            "thread_spawn without its own link, top-level Q",
            json!({"source": {"subagent": {"thread_spawn": {"depth": 1}}}, "parent_thread_id": Q}),
            worker(Q),
        ),
        (
            "thread_spawn with a nested link that is not a string, top-level Q",
            json!({"source": {"subagent": {"thread_spawn": {"parent_thread_id": 7}}}, "parent_thread_id": Q}),
            worker(Q),
        ),
        (
            "thread_spawn with no link at all",
            json!({"source": {"subagent": {"thread_spawn": {"depth": 1}}}}),
            internal(None),
        ),
        (
            "review",
            json!({"source": {"subagent": "review"}}),
            internal(None),
        ),
        (
            "compact",
            json!({"source": {"subagent": "compact"}}),
            internal(None),
        ),
        (
            "memory consolidation",
            json!({"source": {"subagent": "memory_consolidation"}}),
            internal(None),
        ),
        (
            "a guardian review thread",
            json!({"source": {"subagent": {"other": "guardian"}}, "parent_thread_id": P}),
            internal(Some(P)),
        ),
        (
            "internal guardian",
            json!({"source": {"internal": "guardian"}}),
            internal(None),
        ),
        ("custom", json!({"source": {"custom": "x"}}), internal(None)),
        ("a number", json!({"source": 42}), internal(None)),
        ("an array", json!({"source": ["cli"]}), internal(None)),
        (
            "cli with a top-level link",
            json!({"source": "cli", "parent_thread_id": P}),
            worker(P),
        ),
        (
            "vscode, top-level link, thread_source subagent",
            json!({"source": "vscode", "parent_thread_id": P, "thread_source": "subagent"}),
            worker(P),
        ),
        (
            "cli, top-level link, thread_source guardian_review",
            json!({"source": "cli", "parent_thread_id": P, "thread_source": "guardian_review"}),
            internal(Some(P)),
        ),
        (
            "cli, top-level link, thread_source memory_consolidation",
            json!({"source": "cli", "parent_thread_id": P, "thread_source": "memory_consolidation"}),
            internal(Some(P)),
        ),
        ("a payload that is an array", json!(["cli"]), Kin::Desk),
    ];
    for (case, payload, want) in cases {
        assert_eq!(read_meta(&payload).kin, want, "{case}");
    }
}

/// G4: a worker's own `session_meta` comes first, a copy of its parent's later. The first decides.
#[test]
fn the_first_record_decides_through_the_parse() {
    let own = format!(
        r#"{{"timestamp":"t","type":"session_meta","payload":{{"id":"{Q}","session_id":"{P}","cwd":"/code/app","source":{{"subagent":{{"thread_spawn":{{"parent_thread_id":"{P}","depth":1,"agent_nickname":"Ada"}}}}}}}}}}"#
    );
    let copy = format!(
        r#"{{"timestamp":"t","type":"session_meta","payload":{{"id":"{P}","session_id":"{P}","cwd":"/code/app","source":"cli"}}}}"#
    );
    let t = crate::discovery::codex::parse_transcript(std::io::Cursor::new([own, copy].join("\n")));
    assert_eq!(t.kin, worker(P));
    assert_eq!(t.called.as_deref(), Some("Ada"));
}

#[test]
fn a_worker_is_called_by_nickname_then_role_then_path() {
    let spawn = |fields: Value| json!({"source": {"subagent": {"thread_spawn": fields}}, "parent_thread_id": P});
    let long = "x".repeat(60);
    let cases: Vec<(&str, Value, Option<String>)> = vec![
        (
            "the nested nickname over the top-level one",
            {
                let mut v = spawn(json!({"agent_nickname": "Nested", "agent_role": "explorer"}));
                v["agent_nickname"] = json!("Top");
                v
            },
            Some("Nested".to_string()),
        ),
        (
            "the top-level nickname over a nested role",
            {
                let mut v = spawn(json!({"agent_role": "explorer"}));
                v["agent_nickname"] = json!("Top");
                v
            },
            Some("Top".to_string()),
        ),
        (
            "a role over the path",
            spawn(json!({"agent_role": "explorer", "agent_path": "/root/worker_one"})),
            Some("explorer".to_string()),
        ),
        (
            "a role read as agent_type",
            spawn(json!({"agent_type": "reviewer", "agent_path": "/root/worker_one"})),
            Some("reviewer".to_string()),
        ),
        (
            "the top-level role as agent_type",
            json!({"source": "cli", "parent_thread_id": P, "agent_type": "reviewer"}),
            Some("reviewer".to_string()),
        ),
        (
            "the path's last part",
            spawn(json!({"agent_path": "/root/team/worker_one"})),
            Some("worker_one".to_string()),
        ),
        (
            "the top-level path",
            json!({"source": "cli", "parent_thread_id": P, "agent_path": "/root/worker_two"}),
            Some("worker_two".to_string()),
        ),
        (
            "control and invisible characters removed",
            spawn(json!({"agent_nickname": "  A\u{1b}[31md\u{200b}a\n "})),
            Some("A[31mda".to_string()),
        ),
        (
            "an empty nickname skipped",
            spawn(json!({"agent_nickname": " \u{7} ", "agent_role": "explorer"})),
            Some("explorer".to_string()),
        ),
        (
            "60 characters cut to 40",
            spawn(json!({"agent_nickname": long})),
            Some("x".repeat(40)),
        ),
        ("nothing", spawn(json!({"depth": 1})), None),
        (
            "a nickname that is a number",
            spawn(json!({"agent_nickname": 3})),
            None,
        ),
    ];
    for (case, payload, want) in cases {
        assert_eq!(read_meta(&payload).called, want, "{case}");
    }
}

#[test]
fn a_request_names_its_worker() {
    assert_eq!(name_of(Some("worker_one"), A), "worker_one");
    assert_eq!(name_of(None, A), "helper 0199c0de");
}

/// G6a: a legacy rollout's activity record.
fn activity(thread: &str, kind: &str) -> Value {
    json!({"type": "sub_agent_activity", "event_id": "e", "occurred_at_ms": 1, "agent_thread_id": thread, "agent_path": "/root/worker_one", "kind": kind})
}

/// G6b: an activity as a completed `SubAgentActivity` item.
fn activity_item(thread: &str, kind: &str) -> Value {
    json!({"type": "item_completed", "thread_id": P, "turn_id": "t1", "item": {"type": "SubAgentActivity", "id": "i", "kind": kind, "agent_thread_id": thread, "agent_path": "/root/worker_one"}})
}

/// G9: a completed `CollabAgentToolCall` item.
fn call(tool: &str, status: &str, receivers: &[&str], agents: &Value) -> Value {
    json!({"type": "item_completed", "thread_id": P, "turn_id": "t1", "item": {"type": "CollabAgentToolCall", "id": "c", "tool": tool, "status": status, "sender_thread_id": P, "receiver_thread_ids": receivers, "model": "gpt-6-luna", "reasoning_effort": "high", "agents_states": agents}})
}

fn told(thread: &str, says: Says, at: &str) -> Told {
    Told {
        thread: thread.to_string(),
        says,
        at: at.to_string(),
        called: Some("worker_one".to_string()),
        model: None,
        effort: None,
    }
}

fn from_call(thread: &str, says: Says, model: bool) -> Told {
    Told {
        thread: thread.to_string(),
        says,
        at: "t1".to_string(),
        called: None,
        model: model.then(|| "gpt-6-luna".to_string()),
        effort: model.then(|| "high".to_string()),
    }
}

/// The activity rows of the tellings table of spec 10.1 (G6, G7). Each record's timestamp is `t1`, `t2`, … in order.
#[test]
fn a_rollout_tells_the_newest_thing_about_each_worker() {
    let with_model = |effort: Value| {
        let mut v = activity(A, "started");
        v["model"] = json!("gpt-6-luna");
        v["reasoning_effort"] = effort;
        v
    };
    walk(vec![
        (
            "legacy started",
            vec![activity(A, "started")],
            vec![told(A, Says::Working, "t1")],
        ),
        (
            "legacy started, then a completed item (G7)",
            vec![activity(A, "started"), activity_item(A, "completed")],
            vec![told(A, Says::Over, "t2")],
        ),
        (
            "paginated started then completed",
            vec![activity_item(A, "started"), activity_item(A, "completed")],
            vec![told(A, Says::Over, "t2")],
        ),
        (
            "started, then interacted",
            vec![activity(A, "started"), activity(A, "interacted")],
            vec![told(A, Says::Working, "t1")],
        ),
        ("interacted alone", vec![activity(A, "interacted")], vec![]),
        (
            "started, then interrupted",
            vec![activity(A, "started"), activity(A, "interrupted")],
            vec![told(A, Says::Over, "t2")],
        ),
        (
            "completed, then started again",
            vec![
                activity(A, "started"),
                activity_item(A, "completed"),
                activity(A, "started"),
            ],
            vec![told(A, Says::Working, "t3")],
        ),
        (
            "0.170.0 started with model and effort",
            vec![with_model(json!("high"))],
            vec![Told {
                model: Some("gpt-6-luna".to_string()),
                effort: Some("high".to_string()),
                ..told(A, Says::Working, "t1")
            }],
        ),
        (
            "an effort that is an object",
            vec![with_model(json!({"level": "high"}))],
            vec![Told {
                model: Some("gpt-6-luna".to_string()),
                ..told(A, Says::Working, "t1")
            }],
        ),
        (
            "two workers",
            vec![activity(B, "started"), activity(A, "started")],
            vec![told(B, Says::Working, "t1"), told(A, Says::Working, "t2")],
        ),
    ]);
}

/// The collaboration rows of the tellings table (G9).
#[test]
fn a_collaboration_call_tells_its_workers_states() {
    walk(vec![
        (
            "spawn_agent completed, one receiver, no states",
            vec![call("spawn_agent", "completed", &[A], &json!({}))],
            vec![from_call(A, Says::Working, true)],
        ),
        (
            "spawn_agent naming its receiver in the states",
            vec![call(
                "spawn_agent",
                "completed",
                &[A],
                &json!({A: "pending_init"}),
            )],
            vec![from_call(A, Says::Working, true)],
        ),
        (
            "wait with three states",
            vec![call(
                "wait",
                "completed",
                &[],
                &json!({A: {"completed": null}, B: "running", C: {"errored": "x"}}),
            )],
            vec![
                from_call(A, Says::Over, false),
                from_call(B, Says::Working, false),
                from_call(C, Says::Failed, false),
            ],
        ),
        (
            "close_agent completed",
            vec![call("close_agent", "completed", &[A], &json!({}))],
            vec![from_call(A, Says::Over, false)],
        ),
        (
            "resume_agent completed",
            vec![call("resume_agent", "completed", &[A], &json!({}))],
            vec![from_call(A, Says::Working, false)],
        ),
        (
            "spawn_agent in progress",
            vec![call("spawn_agent", "in_progress", &[], &json!({}))],
            vec![],
        ),
        (
            "an unknown status and a status with two keys",
            vec![call(
                "wait",
                "completed",
                &[],
                &json!({A: "something_new", B: {"completed": null, "errored": "x"}}),
            )],
            vec![],
        ),
        (
            "a user message, an unknown kind, a numeric thread, an array, no type",
            vec![
                json!({"type": "item_completed", "thread_id": P, "turn_id": "t1", "item": {"type": "UserMessage", "id": "u", "content": []}}),
                activity(A, "paused"),
                json!({"type": "sub_agent_activity", "agent_thread_id": 7, "kind": "started"}),
                json!(["sub_agent_activity", A, "started"]),
                json!({"type": "item_completed", "item": ["SubAgentActivity", "i", "started", A]}),
                json!({"agent_thread_id": A, "kind": "started"}),
            ],
            vec![],
        ),
    ]);
}

/// Feeds each case's records in order, stamped `t1`, `t2`, …, and compares the entries.
fn walk(cases: Vec<(&str, Vec<Value>, Vec<Told>)>) {
    for (case, records, want) in cases {
        let mut tellings = Tellings::default();
        for (n, record) in records.iter().enumerate() {
            tellings.read(record, &format!("t{}", n + 1));
        }
        assert_eq!(tellings.finish(), want, "{case}");
    }
}

#[test]
fn at_most_512_workers_are_kept_dropping_the_first_named() {
    let mut tellings = Tellings::default();
    for n in 0..=TOLD_MAX {
        tellings.read(&activity(&format!("w{n}"), "started"), "t");
    }
    let told = tellings.finish();
    assert_eq!(told.len(), TOLD_MAX);
    assert_eq!(told.first().map(|t| t.thread.as_str()), Some("w1"));
}

/// A user message among the records still counts as the human speaking (spec 10.1).
#[test]
fn a_user_message_item_still_counts_as_the_human_speaking() {
    let reply = r#"{"timestamp":"t1","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Shall I go on?"}]}}"#;
    let user = format!(
        r#"{{"timestamp":"t2","type":"event_msg","payload":{{"type":"item_completed","thread_id":"{P}","turn_id":"t1","item":{{"type":"UserMessage","id":"u","content":[]}}}}}}"#
    );
    let t = crate::discovery::codex::parse_transcript(std::io::Cursor::new(
        [reply.to_string(), user].join("\n"),
    ));
    assert!(t.human_spoke_last);
    assert!(t.told.is_empty());
}
