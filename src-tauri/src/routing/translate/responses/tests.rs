// The requests below are written from Anthropic's published pages (U10 "Create a Message"), and
// the expected output from OpenAI's (U1 "Create a model response", U3 "Function calling", U8
// "Images and vision"), with invented ids. None was captured from a real session or account.

use super::*;
use crate::routing::fixtures;
use crate::routing::translate::messages;
use serde::Deserialize;

/// The model A Human mapped for the fixture's `OpenAI` connection, so no name is written here.
fn model() -> String {
    fixtures::oai_target().model
}

/// A body around `rest` (members after the three every carried request has).
fn wrap(rest: &str) -> String {
    with_tokens(rest, 64)
}

fn built_with(body: &str, effort: Option<&str>) -> String {
    let asked = messages::read(body.as_bytes()).expect("the reader carries the body");
    let out = build(&asked, &model(), effort).expect("built");
    String::from_utf8(out).expect("utf-8")
}

fn built(body: &str) -> String {
    built_with(body, None)
}

/// The output with its fixed head and tail cut off, so a row can name only what it is about.
fn middle(out: &str) -> String {
    let head = format!(r#"{{"model":"{}","#, model());
    out.strip_prefix(&head)
        .and_then(|rest| rest.strip_suffix(r#","store":false,"stream":true}"#))
        .unwrap_or_else(|| panic!("fixed head and tail: {out}"))
        .to_owned()
}

#[test]
fn a_whole_conversation_in_order() {
    let body = wrap(concat!(
        r#","system":[{"type":"text","text":"rule one"},{"type":"text","text":"rule two","cache_control":{"type":"ephemeral"}}]"#,
        r#","messages":["#,
        r#"{"role":"user","content":"find the bug"},"#,
        r#"{"role":"assistant","content":[{"type":"text","text":"looking"},"#,
        r#"{"type":"tool_use","id":"toolu_a1","name":"read","input":{"path":"a.rs"}},"#,
        r#"{"type":"tool_use","id":"toolu_b2","name":"shot","input":{}}]},"#,
        r#"{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_a1","content":"fn main() {}"},"#,
        r#"{"type":"tool_result","tool_use_id":"toolu_b2","content":[{"type":"text","text":"the screen"},"#,
        r#"{"type":"image","source":{"type":"url","url":"https://example.test/s.png"}}]},"#,
        r#"{"type":"text","text":"and now?"}]},"#,
        r#"{"role":"system","content":"a reminder"},"#,
        r#"{"role":"assistant","content":[{"type":"tool_use","id":"toolu_c3","name":"run","input":{"cmd":"make"}}]},"#,
        r#"{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_c3","content":"exit 2","is_error":true}]}"#,
        r#"]"#,
    ));
    let expected = concat!(
        r#"{"model":"MODEL","input":["#,
        r#"{"type":"message","role":"developer","content":[{"type":"input_text","text":"rule one"},{"type":"input_text","text":"rule two"}]},"#,
        r#"{"type":"message","role":"user","content":[{"type":"input_text","text":"find the bug"}]},"#,
        r#"{"type":"message","role":"assistant","content":"looking"},"#,
        r#"{"type":"function_call","call_id":"toolu_a1","name":"read","arguments":"{\"path\":\"a.rs\"}"},"#,
        r#"{"type":"function_call","call_id":"toolu_b2","name":"shot","arguments":"{}"},"#,
        r#"{"type":"function_call_output","call_id":"toolu_a1","output":"fn main() {}"},"#,
        r#"{"type":"function_call_output","call_id":"toolu_b2","output":[{"type":"input_text","text":"the screen"},"#,
        r#"{"type":"input_image","image_url":"https://example.test/s.png","detail":"auto"}]},"#,
        r#"{"type":"message","role":"user","content":[{"type":"input_text","text":"and now?"}]},"#,
        r#"{"type":"message","role":"developer","content":[{"type":"input_text","text":"a reminder"}]},"#,
        r#"{"type":"function_call","call_id":"toolu_c3","name":"run","arguments":"{\"cmd\":\"make\"}"},"#,
        r#"{"type":"function_call_output","call_id":"toolu_c3","output":"[tool call failed]\nexit 2"}"#,
        r#"],"max_output_tokens":64,"store":false,"stream":true}"#,
    )
    .replace("MODEL", &model());
    assert_eq!(built(&body), expected);
}

#[test]
fn each_row_of_the_mapping() {
    let rows: [(&str, String, &str); 14] = [
        (
            "system as a string: one developer message first",
            wrap(r#","system":"be brief","messages":[{"role":"user","content":"hi"}]"#),
            r#""input":[{"type":"message","role":"developer","content":[{"type":"input_text","text":"be brief"}]},{"type":"message","role":"user","content":[{"type":"input_text","text":"hi"}]}],"max_output_tokens":64"#,
        ),
        (
            "no system: no developer message",
            wrap(r#","messages":[{"role":"user","content":"hi"}]"#),
            r#""input":[{"type":"message","role":"user","content":[{"type":"input_text","text":"hi"}]}],"max_output_tokens":64"#,
        ),
        (
            "a system entry at its position, one part per text",
            wrap(
                r#","messages":[{"role":"user","content":"a"},{"role":"system","content":[{"type":"text","text":"x"},{"type":"text","text":"y"}]},{"role":"user","content":"b"}]"#,
            ),
            r#""input":[{"type":"message","role":"user","content":[{"type":"input_text","text":"a"}]},{"type":"message","role":"developer","content":[{"type":"input_text","text":"x"},{"type":"input_text","text":"y"}]},{"type":"message","role":"user","content":[{"type":"input_text","text":"b"}]}],"max_output_tokens":64"#,
        ),
        (
            "a user run of text and images is one message",
            wrap(
                r#","messages":[{"role":"user","content":[{"type":"text","text":"a"},{"type":"image","source":{"type":"url","url":"https://example.test/i.png"}},{"type":"text","text":"b"}]}]"#,
            ),
            r#""input":[{"type":"message","role":"user","content":[{"type":"input_text","text":"a"},{"type":"input_image","image_url":"https://example.test/i.png","detail":"auto"},{"type":"input_text","text":"b"}]}],"max_output_tokens":64"#,
        ),
        (
            "a result splits the user run",
            wrap(
                r#","messages":[{"role":"user","content":[{"type":"text","text":"a"},{"type":"tool_result","tool_use_id":"t1","content":"r"},{"type":"text","text":"b"}]}]"#,
            ),
            r#""input":[{"type":"message","role":"user","content":[{"type":"input_text","text":"a"}]},{"type":"function_call_output","call_id":"t1","output":"r"},{"type":"message","role":"user","content":[{"type":"input_text","text":"b"}]}],"max_output_tokens":64"#,
        ),
        (
            "each assistant text is one message with a string content",
            wrap(
                r#","messages":[{"role":"user","content":"q"},{"role":"assistant","content":[{"type":"text","text":"one"},{"type":"text","text":"two"}]}]"#,
            ),
            r#""input":[{"type":"message","role":"user","content":[{"type":"input_text","text":"q"}]},{"type":"message","role":"assistant","content":"one"},{"type":"message","role":"assistant","content":"two"}],"max_output_tokens":64"#,
        ),
        (
            "an assistant string",
            wrap(
                r#","messages":[{"role":"user","content":"q"},{"role":"assistant","content":"a"}]"#,
            ),
            r#""input":[{"type":"message","role":"user","content":[{"type":"input_text","text":"q"}]},{"type":"message","role":"assistant","content":"a"}],"max_output_tokens":64"#,
        ),
        (
            "a base64 image is a data URL",
            wrap(
                r#","messages":[{"role":"user","content":[{"type":"image","source":{"type":"base64","media_type":"image/png","data":"iVBORw0KGgo="}}]}]"#,
            ),
            r#""input":[{"type":"message","role":"user","content":[{"type":"input_image","image_url":"data:image/png;base64,iVBORw0KGgo=","detail":"auto"}]}],"max_output_tokens":64"#,
        ),
        (
            "a call: the input's own text, no id, no status",
            wrap(
                r#","messages":[{"role":"user","content":"q"},{"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"f","input":{"a":[1,2]}}]}]"#,
            ),
            r#""input":[{"type":"message","role":"user","content":[{"type":"input_text","text":"q"}]},{"type":"function_call","call_id":"t1","name":"f","arguments":"{\"a\":[1,2]}"}],"max_output_tokens":64"#,
        ),
        (
            "a result with nothing is the empty string",
            wrap(
                r#","messages":[{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1"}]}]"#,
            ),
            r#""input":[{"type":"function_call_output","call_id":"t1","output":""}],"max_output_tokens":64"#,
        ),
        (
            "a result as a list of text and a base64 image",
            wrap(
                r#","messages":[{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":[{"type":"text","text":"x"},{"type":"image","source":{"type":"base64","media_type":"image/gif","data":"R0lG"}}]}]}]"#,
            ),
            r#""input":[{"type":"function_call_output","call_id":"t1","output":[{"type":"input_text","text":"x"},{"type":"input_image","image_url":"data:image/gif;base64,R0lG","detail":"auto"}]}],"max_output_tokens":64"#,
        ),
        (
            "tools: function, description kept, strict false",
            wrap(
                r#","messages":[{"role":"user","content":"q"}],"tools":[{"name":"f","description":"does f","input_schema":{"type":"object"}},{"name":"g","input_schema":{"type":"object","properties":{}}}]"#,
            ),
            r#""input":[{"type":"message","role":"user","content":[{"type":"input_text","text":"q"}]}],"tools":[{"type":"function","name":"f","description":"does f","parameters":{"type":"object"},"strict":false},{"type":"function","name":"g","parameters":{"type":"object","properties":{}},"strict":false}],"max_output_tokens":64"#,
        ),
        (
            "metadata, thinking and cache_control leave no trace",
            wrap(
                r#","metadata":{"user_id":"u-1"},"thinking":{"type":"adaptive"},"cache_control":{"type":"ephemeral"},"messages":[{"role":"user","content":[{"type":"text","text":"q","cache_control":{"type":"ephemeral"}}]}]"#,
            ),
            r#""input":[{"type":"message","role":"user","content":[{"type":"input_text","text":"q"}]}],"max_output_tokens":64"#,
        ),
        (
            "an empty conversation",
            wrap(r#","messages":[]"#),
            r#""input":[],"max_output_tokens":64"#,
        ),
    ];
    for (name, body, expected) in rows {
        assert_eq!(middle(&built(&body)), expected, "{name}");
    }
}

#[test]
fn choice_parallel_reasoning_and_the_token_floor() {
    let q =
        r#","messages":[{"role":"user","content":"q"}],"tools":[{"name":"f","input_schema":{}}]"#;
    let rows: [(&str, String, Option<&str>, &str); 11] = [
        ("no choice", wrap(q), None, r#""max_output_tokens":64"#),
        (
            "auto",
            wrap(&format!(r#"{q},"tool_choice":{{"type":"auto"}}"#)),
            None,
            r#""tool_choice":"auto","max_output_tokens":64"#,
        ),
        (
            "any is required",
            wrap(&format!(r#"{q},"tool_choice":{{"type":"any"}}"#)),
            None,
            r#""tool_choice":"required","max_output_tokens":64"#,
        ),
        (
            "none",
            wrap(&format!(r#"{q},"tool_choice":{{"type":"none"}}"#)),
            None,
            r#""tool_choice":"none","max_output_tokens":64"#,
        ),
        (
            "one tool",
            wrap(&format!(
                r#"{q},"tool_choice":{{"type":"tool","name":"f"}}"#
            )),
            None,
            r#""tool_choice":{"type":"function","name":"f"},"max_output_tokens":64"#,
        ),
        (
            "parallel left on: the member is left off",
            wrap(&format!(
                r#"{q},"tool_choice":{{"type":"auto","disable_parallel_tool_use":false}}"#
            )),
            None,
            r#""tool_choice":"auto","max_output_tokens":64"#,
        ),
        (
            "parallel off",
            wrap(&format!(
                r#"{q},"tool_choice":{{"type":"any","disable_parallel_tool_use":true}}"#
            )),
            None,
            r#""tool_choice":"required","parallel_tool_calls":false,"max_output_tokens":64"#,
        ),
        (
            "an effort",
            wrap(q),
            Some("high"),
            r#""reasoning":{"effort":"high"},"max_output_tokens":64"#,
        ),
        (
            "max_tokens 1 is sent as 16",
            with_tokens(q, 1),
            None,
            r#""max_output_tokens":16"#,
        ),
        (
            "max_tokens 16",
            with_tokens(q, 16),
            None,
            r#""max_output_tokens":16"#,
        ),
        (
            "max_tokens 32000",
            with_tokens(q, 32000),
            None,
            r#""max_output_tokens":32000"#,
        ),
    ];
    let tools = r#""tools":[{"type":"function","name":"f","parameters":{},"strict":false}],"#;
    for (name, body, effort, tail) in rows {
        let out = middle(&built_with(&body, effort));
        let rest = out
            .split_once(tools)
            .map_or_else(|| panic!("{name}: tools in {out}"), |(_, rest)| rest);
        assert_eq!(rest, tail, "{name}");
    }
}

/// A body with `rest` and the given `max_tokens`.
fn with_tokens(rest: &str, max_tokens: u64) -> String {
    format!(r#"{{"model":"m","max_tokens":{max_tokens},"stream":true{rest}}}"#)
}

#[test]
fn a_failed_result_starts_with_the_mark() {
    let rows: [(&str, &str, &str); 2] = [
        (
            "a string output",
            r#""content":"no such file","is_error":true"#,
            r#""output":"[tool call failed]\nno such file""#,
        ),
        (
            "a list output",
            r#""content":[{"type":"text","text":"no such file"}],"is_error":true"#,
            r#""output":[{"type":"input_text","text":"[tool call failed]"},{"type":"input_text","text":"no such file"}]"#,
        ),
    ];
    for (name, result, expected) in rows {
        let body = wrap(&format!(
            r#","messages":[{{"role":"user","content":[{{"type":"tool_result","tool_use_id":"t1",{result}}}]}}]"#
        ));
        let out = built(&body);
        assert!(
            out.contains(&format!(
                r#"{{"type":"function_call_output","call_id":"t1",{expected}}}"#
            )),
            "{name}: {out}"
        );
    }
    assert!(ERROR_MARK.starts_with('[') && !ERROR_MARK.contains('\n'));
}

#[test]
fn arguments_and_schemas_are_the_bytes_that_arrived() {
    let input = r#"{ "path" :  "a.rs",
        "lines": [ 1 ,2 ] }"#;
    let schema = r#"{"type" : "object" ,  "properties":{ "path":{"type":"string"} } }"#;
    let body = wrap(&format!(
        r#","messages":[{{"role":"user","content":"q"}},{{"role":"assistant","content":[{{"type":"tool_use","id":"t1","name":"read","input":{input}}}]}}],"tools":[{{"name":"read","input_schema":{schema}}}]"#
    ));
    let out = built(&body);
    let parsed: Top = serde_json::from_str(&out).expect("strict top level");
    let arguments = parsed
        .input
        .iter()
        .find_map(
            |item| match serde_json::from_str::<StrictItem>(item.get()) {
                Ok(StrictItem::FunctionCall { arguments, .. }) => Some(arguments),
                _ => None,
            },
        )
        .expect("a function call");
    assert_eq!(arguments, input);
    assert_eq!(parsed.tools[0].parameters.get(), schema);
}

#[test]
fn hidden_reasoning_appears_nowhere() {
    let body = wrap(concat!(
        r#","messages":[{"role":"user","content":"q"},{"role":"assistant","content":["#,
        r#"{"type":"thinking","thinking":"SECRET-THOUGHT","signature":"SECRET-SIGNATURE"},"#,
        r#"{"type":"redacted_thinking","data":"SECRET-REDACTED"},"#,
        r#"{"type":"text","text":"visible"}]},{"role":"user","content":"next"}]"#,
    ));
    let out = built(&body);
    for secret in [
        "SECRET-THOUGHT",
        "SECRET-SIGNATURE",
        "SECRET-REDACTED",
        "thinking",
    ] {
        assert!(!out.contains(secret), "{secret} in {out}");
    }
    assert!(out.contains(r#""content":"visible""#), "{out}");
}

// Strict readers that know only the members section 5 writes.

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Top {
    model: String,
    input: Vec<Box<serde_json::value::RawValue>>,
    #[serde(default)]
    tools: Vec<StrictFunction>,
    #[serde(default)]
    tool_choice: Option<StrictChoice>,
    #[serde(default)]
    parallel_tool_calls: Option<bool>,
    #[serde(default)]
    reasoning: Option<StrictReasoning>,
    max_output_tokens: u64,
    store: bool,
    stream: bool,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum StrictItem {
    Message {
        role: String,
        content: StrictContent,
    },
    FunctionCall {
        call_id: String,
        name: String,
        arguments: String,
    },
    FunctionCallOutput {
        call_id: String,
        output: StrictContent,
    },
}

#[derive(Deserialize)]
#[serde(untagged)]
enum StrictContent {
    Text(#[expect(dead_code, reason = "read only to prove the content is a string")] String),
    Parts(Vec<StrictPart>),
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum StrictPart {
    InputText { text: String },
    InputImage { image_url: String, detail: String },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StrictFunction {
    #[serde(rename = "type")]
    kind: String,
    name: String,
    #[serde(default)]
    description: Option<String>,
    parameters: Box<serde_json::value::RawValue>,
    strict: bool,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum StrictChoice {
    Mode(String),
    Function(StrictForced),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StrictForced {
    #[serde(rename = "type")]
    kind: String,
    name: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StrictReasoning {
    effort: String,
}

/// Every output the other tests build, in one list.
fn every_output() -> Vec<String> {
    let bodies = [
        wrap(concat!(
            r#","system":"s","messages":[{"role":"user","content":[{"type":"text","text":"a"},"#,
            r#"{"type":"image","source":{"type":"base64","media_type":"image/webp","data":"UklG"}}]},"#,
            r#"{"role":"assistant","content":[{"type":"text","text":"b"},{"type":"tool_use","id":"t1","name":"f","input":{}}]},"#,
            r#"{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","is_error":true,"content":[{"type":"text","text":"x"}]}]},"#,
            r#"{"role":"system","content":"r"}],"#,
            r#""tools":[{"name":"f","description":"d","input_schema":{}}],"#,
            r#""tool_choice":{"type":"tool","name":"f","disable_parallel_tool_use":true}"#,
        )),
        wrap(r#","messages":[{"role":"user","content":"q"}],"tool_choice":{"type":"auto"}"#),
        wrap(r#","messages":[]"#),
    ];
    let mut outs = Vec::new();
    for body in &bodies {
        outs.push(built(body));
        outs.push(built_with(body, Some("medium")));
    }
    outs
}

#[test]
fn nothing_else_is_written() {
    for out in every_output() {
        let top: Top = serde_json::from_str(&out).unwrap_or_else(|e| panic!("{e}: {out}"));
        assert_eq!(top.model, model());
        assert!(top.max_output_tokens >= MIN_OUTPUT_TOKENS);
        for item in &top.input {
            let item: StrictItem =
                serde_json::from_str(item.get()).unwrap_or_else(|e| panic!("{e}: {out}"));
            match item {
                StrictItem::Message { role, content } => {
                    assert!(["user", "assistant", "developer"].contains(&role.as_str()));
                    check_content(&content);
                }
                StrictItem::FunctionCall {
                    call_id,
                    name,
                    arguments,
                } => assert!(!call_id.is_empty() && !name.is_empty() && !arguments.is_empty()),
                StrictItem::FunctionCallOutput { call_id, output } => {
                    assert!(!call_id.is_empty());
                    check_content(&output);
                }
            }
        }
        for tool in &top.tools {
            assert_eq!(tool.kind, "function");
            assert!(!tool.strict, "{out}");
            assert!(!tool.name.is_empty() && tool.description.as_deref() != Some(""));
            assert!(!tool.parameters.get().is_empty());
        }
        match &top.tool_choice {
            Some(StrictChoice::Mode(mode)) => {
                assert!(["auto", "required", "none"].contains(&mode.as_str()));
            }
            Some(StrictChoice::Function(forced)) => {
                assert_eq!(forced.kind, "function");
                assert!(!forced.name.is_empty());
            }
            None => {}
        }
        assert_ne!(top.parallel_tool_calls, Some(true), "{out}");
        if let Some(reasoning) = &top.reasoning {
            assert_eq!(reasoning.effort, "medium");
        }
    }
}

fn check_content(content: &StrictContent) {
    let StrictContent::Parts(parts) = content else {
        return;
    };
    for part in parts {
        match part {
            StrictPart::InputText { text } => assert!(!text.is_empty()),
            StrictPart::InputImage { image_url, detail } => {
                assert_eq!(detail, "auto");
                assert!(image_url.starts_with("data:image/") || image_url.starts_with("https://"));
            }
        }
    }
}

#[test]
fn store_is_false_and_stream_is_true_on_every_output() {
    for out in every_output() {
        let top: Top = serde_json::from_str(&out).expect("strict top level");
        assert!(!top.store && top.stream, "{out}");
    }
}
