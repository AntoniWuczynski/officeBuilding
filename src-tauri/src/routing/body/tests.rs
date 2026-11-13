use super::*;
use Capability::{Documents, Images, ServerTools, Tools};

fn caps(list: &[Capability]) -> BTreeSet<Capability> {
    list.iter().copied().collect()
}

fn text_of(bytes: &[u8]) -> &str {
    std::str::from_utf8(bytes).expect("utf-8 output")
}

/// (name, body, model, effort, capabilities).
type ReadRow = (
    &'static str,
    &'static str,
    &'static str,
    Option<&'static str>,
    &'static [Capability],
);

#[test]
fn read_finds_the_model_the_effort_and_the_capabilities() {
    let rows: [ReadRow; 9] = [
        (
            "model and effort",
            r#"{"model":"claude-opus-5-5","output_config":{"effort":"high"},"messages":[]}"#,
            "claude-opus-5-5",
            Some("high"),
            &[],
        ),
        (
            "no effort in an output_config",
            r#"{"model":"claude-opus-5-5","output_config":{"format":{"type":"json_schema"}}}"#,
            "claude-opus-5-5",
            None,
            &[],
        ),
        (
            "no tools, plain string content",
            r#"{"model":"m","tools":[],"messages":[{"role":"user","content":"hello"}]}"#,
            "m",
            None,
            &[],
        ),
        (
            "a custom tool and one with no type",
            r#"{"model":"m","tools":[{"name":"a","type":"custom","input_schema":{}},{"name":"b","input_schema":{}}]}"#,
            "m",
            None,
            &[Tools],
        ),
        (
            "a tool with another type",
            r#"{"model":"m","tools":[{"type":"web_search_20250305","name":"web_search"}]}"#,
            "m",
            None,
            &[ServerTools],
        ),
        (
            "an image block in a message",
            r#"{"model":"m","messages":[{"role":"user","content":[{"type":"text","text":"see"},{"type":"image","source":{"type":"base64","media_type":"image/png","data":"AAAA"}}]}]}"#,
            "m",
            None,
            &[Images],
        ),
        (
            "a document block inside a tool result",
            r#"{"model":"m","messages":[{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_invented_01","content":[{"type":"document","source":{"type":"text","media_type":"text/plain","data":"x"}}]}]}]}"#,
            "m",
            None,
            &[Documents],
        ),
        (
            "all together",
            r#"{"model":"m","output_config":{"effort":"low"},"tools":[{"name":"a"},{"type":"web_fetch_20250910","name":"web_fetch"}],"messages":[{"role":"user","content":[{"type":"image","source":{}},{"type":"tool_result","tool_use_id":"toolu_invented_02","content":[{"type":"document","source":{}}]}]}]}"#,
            "m",
            Some("low"),
            &[Tools, Images, Documents, ServerTools],
        ),
        (
            "an escaped model name",
            r#"{"model":"claude-opus-5-5A"}"#,
            "claude-opus-5-5A",
            None,
            &[],
        ),
    ];
    for (name, body, model, effort, found) in rows {
        let facts = read(body.as_bytes());
        let expected = Facts {
            model: model.to_owned(),
            effort: effort.map(str::to_owned),
            capabilities: caps(found),
        };
        assert_eq!(facts, Ok(expected), "{name}");
    }
}

#[test]
fn read_refuses_what_is_not_a_readable_messages_body() {
    let rows = [
        ("not JSON", r#"{"model":"m""#),
        ("an array", r#"["claude-opus-5-5"]"#),
        ("no model", r#"{"messages":[]}"#),
        ("a model that is a number", r#"{"model":5}"#),
        ("two model members", r#"{"model":"a","model":"b"}"#),
        (
            "two messages members",
            r#"{"model":"m","messages":[],"messages":[]}"#,
        ),
        (
            "two provider members",
            r#"{"model":"m","provider":{},"provider":{}}"#,
        ),
        (
            "an output_config that is a string",
            r#"{"model":"m","output_config":"high"}"#,
        ),
        (
            "an output_config that is an array",
            r#"{"model":"m","output_config":["high"]}"#,
        ),
        ("trailing bytes", r#"{"model":"m"} {"#),
    ];
    for (name, body) in rows {
        assert_eq!(read(body.as_bytes()), Err(Unreadable), "{name}");
    }
}

#[test]
fn a_content_the_scan_does_not_know_reports_every_capability() {
    let body = r#"{"model":"m","messages":[{"role":"user","content":{"type":"text"}}]}"#;
    let facts = read(body.as_bytes()).expect("still readable");
    assert_eq!(facts.model, "m");
    assert_eq!(facts.capabilities, caps(&ALL));
}

#[test]
fn rewrite_returns_the_very_bytes_when_nothing_changes() {
    let thinking_free = r#"{"model":"claude-opus-5-5","output_config":{"effort":"high"},"messages":[{"role":"user","content":[{"type":"text","text":"hi"}]}],"provider":{"allow_fallbacks":false,"require_parameters":true}}"#;
    let rows = [
        ("nothing asked", Edit::default()),
        (
            "the same model",
            Edit {
                model: Some("claude-opus-5-5"),
                ..Edit::default()
            },
        ),
        (
            "the same effort",
            Edit {
                effort: Some("high"),
                ..Edit::default()
            },
        ),
        (
            "no thinking to drop",
            Edit {
                drop_thinking: true,
                ..Edit::default()
            },
        ),
        (
            "a provider already pinned",
            Edit {
                pin_provider: true,
                ..Edit::default()
            },
        ),
    ];
    let body = thinking_free.as_bytes();
    for (name, edit) in rows {
        let out = rewrite(body, &edit).expect("readable");
        assert!(
            matches!(out, Cow::Borrowed(bytes) if std::ptr::eq(bytes, body)),
            "{name}"
        );
    }
}

#[test]
fn rewrite_refuses_a_body_it_cannot_read() {
    let edit = Edit {
        model: Some("m2"),
        ..Edit::default()
    };
    let rows = [
        ("not JSON", r#"{"model":"#),
        ("an array", r#"["m"]"#),
        (
            "a repeated top-level member",
            r#"{"model":"m","stream":true,"stream":false}"#,
        ),
        ("no model", r#"{"stream":true}"#),
    ];
    for (name, body) in rows {
        assert_eq!(rewrite(body.as_bytes(), &edit), Err(Unreadable), "{name}");
    }
}

#[test]
fn a_model_edit_keeps_every_other_member_byte_for_byte() {
    let body = concat!(
        "{ \"system\" : [ {\"type\":\"text\",\"text\":\"caf\\u00e9 \u{e9}\"} ] ,\n",
        "  \"model\":\"claude-opus-5-5\",\n",
        "  \"max_tokens\" : 1.0e3,\n",
        "  \"metadata\": {\"user_id\" : \"user_invented_01\", \"nested\": {\"z\":1,\"a\":[1, 2 ,3]}},\n",
        "  \"stream\":true }"
    );
    let edit = Edit {
        model: Some("claude-sonnet-5-5"),
        ..Edit::default()
    };
    let out = rewrite(body.as_bytes(), &edit).expect("readable");
    let before: Members<'_> = serde_json::from_slice(body.as_bytes()).expect("input parses");
    let after: Members<'_> = serde_json::from_slice(&out).expect("output parses");
    let keys = |members: &Members<'_>| -> Vec<String> {
        members.0.iter().map(|(key, _)| key.clone()).collect()
    };
    assert_eq!(keys(&before), keys(&after), "same members, same order");
    for ((key, old), (_, new)) in before.0.iter().zip(after.0.iter()) {
        if key == "model" {
            assert_eq!(new.get(), "\"claude-sonnet-5-5\"");
        } else {
            assert_eq!(old.get(), new.get(), "{key} keeps its bytes");
            assert!(text_of(&out).contains(old.get()), "{key} appears verbatim");
        }
    }
    assert_eq!(read(&out).expect("readable").model, "claude-sonnet-5-5");
}

#[test]
fn an_effort_edit_touches_only_the_effort() {
    let effort = Edit {
        effort: Some("low"),
        ..Edit::default()
    };
    let replaced = rewrite(
        br#"{"model":"m","output_config":{"format":{"type" : "json_schema"},"effort":"high"}}"#,
        &effort,
    )
    .expect("readable");
    assert_eq!(
        text_of(&replaced),
        r#"{"model":"m","output_config":{"format":{"type" : "json_schema"},"effort":"low"}}"#
    );
    let untouched = [
        ("no output_config", r#"{"model":"m"}"#),
        (
            "an output_config with no effort",
            r#"{"model":"m","output_config":{"format":{}}}"#,
        ),
        (
            "a null output_config",
            r#"{"model":"m","output_config":null}"#,
        ),
    ];
    for (name, body) in untouched {
        let out = rewrite(body.as_bytes(), &effort).expect("readable");
        assert!(matches!(out, Cow::Borrowed(_)), "{name}");
    }
}

#[test]
fn drop_thinking_removes_hidden_reasoning_and_keeps_everything_else() {
    let drop = Edit {
        drop_thinking: true,
        ..Edit::default()
    };
    let rows: [(&str, &str, Option<&str>); 6] = [
        (
            "thinking at the start, tool_use kept byte for byte",
            r#"{"model":"m","messages":[{"role":"assistant","content":[{"type":"thinking","thinking":"a","signature":"sig_invented_1"},{"type":"text","text":"hi" },{"type":"tool_use","id":"toolu_invented_1","name":"Read","input":{"path" : "a.txt"}}]}]}"#,
            Some(
                r#"{"model":"m","messages":[{"role":"assistant","content":[{"type":"text","text":"hi" },{"type":"tool_use","id":"toolu_invented_1","name":"Read","input":{"path" : "a.txt"}}]}]}"#,
            ),
        ),
        (
            "redacted thinking in the middle",
            r#"{"model":"m","messages":[{"role":"assistant","content":[{"type":"text","text":"x"},{"type":"redacted_thinking","data":"invented"},{"type":"text","text":"y"}]}]}"#,
            Some(
                r#"{"model":"m","messages":[{"role":"assistant","content":[{"type":"text","text":"x"},{"type":"text","text":"y"}]}]}"#,
            ),
        ),
        (
            "thinking at the end, other messages keep their exact bytes",
            "{\"model\":\"m\",\"messages\":[ {\"role\":\"user\",\"content\":\"plain words\"},\n {\"role\":\"user\",\"content\":[{\"type\":\"tool_result\",\"tool_use_id\":\"toolu_invented_1\",\"content\":\"ok\"}]},\n {\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"z\"},{\"type\":\"thinking\",\"thinking\":\"b\",\"signature\":\"sig_invented_2\"}]},\n {\"role\" : \"user\", \"content\" : [ {\"type\":\"text\",\"text\":\"untouched\"} ]} ]}",
            Some(
                r#"{"model":"m","messages":[{"role":"user","content":"plain words"},{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_invented_1","content":"ok"}]},{"role":"assistant","content":[{"type":"text","text":"z"}]},{"role" : "user", "content" : [ {"type":"text","text":"untouched"} ]}]}"#,
            ),
        ),
        (
            "a message that held only thinking is removed whole",
            r#"{"model":"m","messages":[{"role":"user","content":"q"},{"role":"assistant","content":[{"type":"redacted_thinking","data":"only"},{"type":"thinking","thinking":"c","signature":"sig_invented_3"}]},{"role":"user","content":"r"}]}"#,
            Some(
                r#"{"model":"m","messages":[{"role":"user","content":"q"},{"role":"user","content":"r"}]}"#,
            ),
        ),
        (
            "a messages that is not an array is left alone",
            r#"{"model":"m","messages":{"type":"thinking"}}"#,
            None,
        ),
        (
            "shapes it does not expect are left alone",
            r#"{"model":"m","messages":["text",{"role":"user"},{"role":"user","content":{"type":"thinking"}},{"role":"user","content":[7,{"no_type":1}]}]}"#,
            None,
        ),
    ];
    for (name, body, expected) in rows {
        let out = rewrite(body.as_bytes(), &drop).expect("readable");
        match expected {
            Some(expected) => assert_eq!(text_of(&out), expected, "{name}"),
            None => assert!(matches!(out, Cow::Borrowed(_)), "{name}"),
        }
    }
}

#[test]
fn pin_provider_switches_openrouter_fallback_off() {
    let pin = Edit {
        pin_provider: true,
        ..Edit::default()
    };
    let rows = [
        (
            "appended when absent",
            r#"{"model":"m"}"#,
            r#"{"model":"m","provider":{"allow_fallbacks":false,"require_parameters":true}}"#,
        ),
        (
            "merged into an object, keeping its other members",
            r#"{"model":"m","provider":{"order" : [ "anthropic" ]}}"#,
            r#"{"model":"m","provider":{"order":[ "anthropic" ],"allow_fallbacks":false,"require_parameters":true}}"#,
        ),
        (
            "an existing allow_fallbacks true turned false, in place",
            r#"{"provider":{"allow_fallbacks":true,"order":["a"]},"model":"m"}"#,
            r#"{"provider":{"allow_fallbacks":false,"order":["a"],"require_parameters":true},"model":"m"}"#,
        ),
        (
            "a provider that is a string replaced",
            r#"{"model":"m","provider":"anthropic"}"#,
            r#"{"model":"m","provider":{"allow_fallbacks":false,"require_parameters":true}}"#,
        ),
    ];
    for (name, body, expected) in rows {
        let out = rewrite(body.as_bytes(), &pin).expect("readable");
        assert_eq!(text_of(&out), expected, "{name}");
    }
    assert_eq!(
        rewrite(br#"{"model":"m","provider":{"order":[],"order":[]}}"#, &pin),
        Err(Unreadable),
        "a provider object with a repeated member"
    );
}

#[test]
fn all_four_edits_at_once() {
    let body = r#"{"model":"claude-opus-5-5","max_tokens":32000,"output_config":{"effort":"max"},"messages":[{"role":"user","content":"go"},{"role":"assistant","content":[{"type":"thinking","thinking":"t","signature":"sig_invented_4"},{"type":"text","text":"done"}]}],"provider":{"order":["anthropic"],"allow_fallbacks":true}}"#;
    let edit = Edit {
        model: Some("anthropic/claude-sonnet-5-5"),
        effort: Some("medium"),
        drop_thinking: true,
        pin_provider: true,
    };
    let out = rewrite(body.as_bytes(), &edit).expect("readable");
    assert_eq!(
        text_of(&out),
        r#"{"model":"anthropic/claude-sonnet-5-5","max_tokens":32000,"output_config":{"effort":"medium"},"messages":[{"role":"user","content":"go"},{"role":"assistant","content":[{"type":"text","text":"done"}]}],"provider":{"order":["anthropic"],"allow_fallbacks":false,"require_parameters":true}}"#
    );
    let facts = read(&out).expect("readable");
    assert_eq!(facts.model, "anthropic/claude-sonnet-5-5");
    assert_eq!(facts.effort.as_deref(), Some("medium"));
}
