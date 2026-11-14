// The requests below are written from Anthropic's published pages (U10 "Create a Message", U12
// "Per-message effort"), with invented ids. None was captured from a real session.

use super::*;

/// A body around `rest` (members after the three every carried request has).
fn wrap(rest: &str) -> String {
    format!(r#"{{"model":"m","max_tokens":16,"stream":true{rest}}}"#)
}

/// A body whose `messages` is `list`.
fn with_messages(list: &str) -> String {
    wrap(&format!(r#","messages":{list}"#))
}

fn member(name: &str) -> Carry {
    Carry::Member {
        name: name.to_owned(),
    }
}

fn block(kind: &str) -> Carry {
    Carry::Block {
        kind: kind.to_owned(),
    }
}

fn piece_text(piece: &Piece<'_>) -> String {
    match piece {
        Piece::Text(text) => format!("text:{text}"),
        Piece::Image(Image::Base64 { media_type, data }) => format!("b64:{media_type}:{data}"),
        Piece::Image(Image::Url(url)) => format!("url:{url}"),
        Piece::Call { id, name, input } => format!("call:{id}:{name}:{}", input.get()),
        Piece::Result { id, body, failed } => {
            let state = if *failed { "failed" } else { "ok" };
            let body = match body {
                Returned::Text(text) => format!("'{text}'"),
                Returned::Parts(parts) => format!("[{}]", joined(parts)),
            };
            format!("result:{id}:{state}:{body}")
        }
    }
}

fn joined(pieces: &[Piece<'_>]) -> String {
    pieces.iter().map(piece_text).collect::<Vec<_>>().join(",")
}

/// The turns, one word each, so a table can say what it expects.
fn sketch(asked: &Asked<'_>) -> String {
    asked
        .turns
        .iter()
        .map(|turn| match turn {
            Turn::User(pieces) => format!("user({})", joined(pieces)),
            Turn::Assistant(pieces) => format!("assistant({})", joined(pieces)),
            Turn::System(texts) => format!("system({})", texts.join("|")),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// (name, body, whether it reads or why it is refused).
type Row = (&'static str, String, Result<(), Carry>);

fn check<N: std::fmt::Display>(rows: impl IntoIterator<Item = (N, String, Result<(), Carry>)>) {
    for (name, body, expected) in rows {
        assert_eq!(read(body.as_bytes()).map(|_| ()), expected, "{name}");
    }
}

#[test]
fn the_top_level_is_carried_left_out_or_refused_by_name() {
    check(top_level_rows());
}

fn top_level_rows() -> Vec<Row> {
    vec![
        ("smallest", wrap(""), Ok(())),
        (
            "left out on purpose",
            wrap(
                r#","metadata":{"user_id":"u-1"},"thinking":{"type":"adaptive"},"cache_control":{"type":"ephemeral"}"#,
            ),
            Ok(()),
        ),
        (
            "context_management",
            wrap(r#","context_management":{"edits":[]}"#),
            Err(Carry::ContextManagement),
        ),
        (
            "context_management null",
            wrap(r#","context_management":null"#),
            Ok(()),
        ),
        (
            "stop_sequences with an entry",
            wrap(r#","stop_sequences":["END"]"#),
            Err(Carry::StopSequences),
        ),
        (
            "stop_sequences empty",
            wrap(r#","stop_sequences":[]"#),
            Ok(()),
        ),
        (
            "stream false",
            r#"{"model":"m","max_tokens":16,"stream":false}"#.to_owned(),
            Err(Carry::NotStreamed),
        ),
        (
            "stream missing",
            r#"{"model":"m","max_tokens":16}"#.to_owned(),
            Err(Carry::NotStreamed),
        ),
        (
            "max_tokens 0",
            r#"{"model":"m","max_tokens":0,"stream":true}"#.to_owned(),
            Err(Carry::Prewarm),
        ),
        (
            "max_tokens missing",
            r#"{"model":"m","stream":true}"#.to_owned(),
            Err(Carry::Unknown),
        ),
        (
            "max_tokens a string",
            r#"{"model":"m","max_tokens":"16","stream":true}"#.to_owned(),
            Err(Carry::Unknown),
        ),
        (
            "output_config with format",
            wrap(r#","output_config":{"effort":"high","format":{"type":"json_schema"}}"#),
            Err(Carry::OutputSetting),
        ),
        (
            "a repeated member",
            wrap(r#","stream":true"#),
            Err(Carry::Unknown),
        ),
        (
            "an array",
            r#"["m",16,true]"#.to_owned(),
            Err(Carry::Unknown),
        ),
        (
            "the first of two refusals: a member",
            r#"{"temperature":1,"stop_sequences":["x"],"model":"m","max_tokens":16,"stream":true}"#
                .to_owned(),
            Err(member("temperature")),
        ),
        (
            "the first of two refusals: stop_sequences",
            r#"{"stop_sequences":["x"],"temperature":1,"model":"m","max_tokens":16,"stream":true}"#
                .to_owned(),
            Err(Carry::StopSequences),
        ),
    ]
}

#[test]
fn a_member_nobody_carries_is_refused_by_name() {
    let long = "a".repeat(50);
    let rows: [(&str, &str, Result<(), Carry>); 8] = [
        ("temperature", "0.5", Err(member("temperature"))),
        ("service_tier", r#""auto""#, Err(member("service_tier"))),
        ("speed", r#""fast""#, Err(member("speed"))),
        ("container", r#""c-1""#, Err(member("container"))),
        ("inference_geo", r#""us""#, Err(member("inference_geo"))),
        ("diagnostics", "{}", Err(member("diagnostics"))),
        ("a b", "1", Err(Carry::Unknown)),
        (&long, "1", Err(Carry::Unknown)),
    ];
    check(
        rows.into_iter().map(|(name, value, expected)| {
            (name, wrap(&format!(r#","{name}":{value}"#)), expected)
        }),
    );
}

#[test]
fn members_left_out_leave_no_trace() {
    let plain = wrap("");
    let busy = wrap(
        r#","metadata":{"user_id":"u-secret"},"thinking":{"type":"enabled","budget_tokens":1024},"cache_control":{"type":"ephemeral"}"#,
    );
    let plain = format!("{:?}", read(plain.as_bytes()));
    let busy = format!("{:?}", read(busy.as_bytes()));
    assert_eq!(plain, busy);
    assert!(!busy.contains("u-secret"));
}

#[test]
fn the_top_level_is_read_into_the_model() {
    let body = wrap(
        r#","system":[{"type":"text","text":"one"},{"type":"text","text":"two","cache_control":{"type":"ephemeral"}}],"messages":[{"role":"user","content":"hi"}]"#,
    );
    let asked = read(body.as_bytes()).expect("reads");
    assert_eq!(asked.system, ["one", "two"]);
    assert_eq!(asked.max_tokens, 16);
    assert_eq!(sketch(&asked), "user(text:hi)");
    let rows: [(&str, &str, &[&str]); 3] = [
        ("a string", r#","system":"be brief""#, &["be brief"]),
        ("null", r#","system":null"#, &[]),
        ("an escaped string", r#","system":"a\"b""#, &["a\"b"]),
    ];
    for (name, rest, expected) in rows {
        let body = wrap(rest);
        let asked = read(body.as_bytes()).expect(name);
        assert_eq!(asked.system, expected, "{name}");
    }
}

#[test]
fn turns_are_read_by_role_and_block() {
    let rows: [(&str, &str, Result<&str, Carry>); 15] = [
        (
            "a user string",
            r#"[{"role":"user","content":"hi"}]"#,
            Ok("user(text:hi)"),
        ),
        (
            "user text and image blocks",
            r#"[{"role":"user","content":[{"type":"text","text":"look","cache_control":{"type":"ephemeral"}},{"type":"image","source":{"type":"url","url":"https://example.com/a.png"}}]}]"#,
            Ok("user(text:look,url:https://example.com/a.png)"),
        ),
        (
            "an assistant string",
            r#"[{"role":"assistant","content":"hi"}]"#,
            Ok("assistant(text:hi)"),
        ),
        (
            "assistant text and tool_use",
            r#"[{"role":"assistant","content":[{"type":"text","text":"ok"},{"type":"tool_use","id":"toolu_1","name":"Read","input":{"path":"a"}}]}]"#,
            Ok(r#"assistant(text:ok,call:toolu_1:Read:{"path":"a"})"#),
        ),
        (
            "a tool_result with a string",
            r#"[{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"done"}]}]"#,
            Ok("user(result:toolu_1:ok:'done')"),
        ),
        (
            "a tool_result with nothing",
            r#"[{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1"}]}]"#,
            Ok("user(result:toolu_1:ok:'')"),
        ),
        (
            "a tool_result with a list of text and image",
            r#"[{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1","content":[{"type":"text","text":"a"},{"type":"image","source":{"type":"base64","media_type":"image/png","data":"AAAA"}}]}]}]"#,
            Ok("user(result:toolu_1:ok:[text:a,b64:image/png:AAAA])"),
        ),
        (
            "a tool_result with is_error",
            r#"[{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"boom","is_error":true}]}]"#,
            Ok("user(result:toolu_1:failed:'boom')"),
        ),
        (
            "a system entry with a string",
            r#"[{"role":"system","content":"note"}]"#,
            Ok("system(note)"),
        ),
        (
            "a system entry with text blocks",
            r#"[{"role":"system","content":[{"type":"text","text":"a"},{"type":"text","text":"b"}]}]"#,
            Ok("system(a|b)"),
        ),
        (
            "a system entry with empty content",
            r#"[{"role":"system","content":[]},{"role":"user","content":"hi"}]"#,
            Ok("user(text:hi)"),
        ),
        (
            "a role tool",
            r#"[{"role":"tool","content":"x"}]"#,
            Err(Carry::Unknown),
        ),
        (
            "an output_config on a user turn",
            r#"[{"role":"user","content":"x","output_config":{"effort":"low"}}]"#,
            Err(Carry::Unknown),
        ),
        (
            "a member nobody knows on a turn",
            r#"[{"role":"user","content":"x","name":"y"}]"#,
            Err(Carry::Unknown),
        ),
        (
            "a turn as an array",
            r#"[["user","x"]]"#,
            Err(Carry::Unknown),
        ),
    ];
    for (name, list, expected) in rows {
        let body = with_messages(list);
        let read = read(body.as_bytes());
        assert_eq!(
            read.as_ref().map_err(Carry::clone).map(sketch),
            expected.map(str::to_owned),
            "{name}"
        );
    }
}

#[test]
fn hidden_reasoning_is_dropped_and_never_kept() {
    let body = with_messages(
        r#"[{"role":"user","content":"go"},{"role":"assistant","content":[{"type":"thinking","thinking":"SECRET-THOUGHT","signature":"SIG-123"},{"type":"text","text":"visible"},{"type":"redacted_thinking","data":"REDACTED-DATA"},{"type":"tool_use","id":"toolu_1","name":"Read","input":{}}]},{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"x"}]},{"role":"assistant","content":[{"type":"thinking","thinking":"SECRET-THOUGHT-2","signature":"SIG-456"}]}]"#,
    );
    let asked = read(body.as_bytes()).expect("reads");
    assert_eq!(
        sketch(&asked),
        "user(text:go) assistant(text:visible,call:toolu_1:Read:{}) user(result:toolu_1:ok:'x')"
    );
    let debug = format!("{asked:?}");
    for secret in ["SECRET-THOUGHT", "SIG-123", "SIG-456", "REDACTED-DATA"] {
        assert!(!debug.contains(secret), "{secret}");
    }
}

/// A user turn holding `blocks`.
fn user_blocks(blocks: &str) -> String {
    with_messages(&format!(r#"[{{"role":"user","content":[{blocks}]}}]"#))
}

/// An assistant turn holding `blocks`.
fn assistant_blocks(blocks: &str) -> String {
    with_messages(&format!(r#"[{{"role":"assistant","content":[{blocks}]}}]"#))
}

fn image_of(source: &str) -> String {
    user_blocks(&format!(r#"{{"type":"image","source":{source}}}"#))
}

fn base64_of(media_type: &str) -> String {
    image_of(&format!(
        r#"{{"type":"base64","media_type":"{media_type}","data":"AAAA"}}"#
    ))
}

#[test]
fn blocks_this_route_cannot_carry_refuse_the_request() {
    check(block_rows());
}

fn block_rows() -> Vec<Row> {
    vec![
        (
            "document",
            user_blocks(
                r#"{"type":"document","source":{"type":"text","media_type":"text/plain","data":"x"}}"#,
            ),
            Err(block("document")),
        ),
        (
            "server_tool_use",
            assistant_blocks(
                r#"{"type":"server_tool_use","id":"srvtoolu_1","name":"web_search","input":{}}"#,
            ),
            Err(block("server_tool_use")),
        ),
        (
            "web_search_tool_result",
            user_blocks(
                r#"{"type":"web_search_tool_result","tool_use_id":"srvtoolu_1","content":[]}"#,
            ),
            Err(block("web_search_tool_result")),
        ),
        (
            "search_result",
            user_blocks(r#"{"type":"search_result","source":"s","title":"t","content":[]}"#),
            Err(block("search_result")),
        ),
        (
            "container_upload",
            user_blocks(r#"{"type":"container_upload","file_id":"file_1"}"#),
            Err(block("container_upload")),
        ),
        (
            "a tool_reference inside a tool result",
            user_blocks(
                r#"{"type":"tool_result","tool_use_id":"toolu_1","content":[{"type":"tool_reference","tool_name":"Read"}]}"#,
            ),
            Err(block("tool_reference")),
        ),
        (
            "an invented type",
            user_blocks(r#"{"type":"hologram"}"#),
            Err(block("hologram")),
        ),
        (
            "a type with a quote",
            user_blocks(r#"{"type":"a\"b"}"#),
            Err(Carry::Unknown),
        ),
        (
            "thinking in a user turn",
            user_blocks(r#"{"type":"thinking","thinking":"x","signature":"s"}"#),
            Err(block("thinking")),
        ),
        (
            "text with a citation",
            user_blocks(
                r#"{"type":"text","text":"x","citations":[{"type":"char_location","cited_text":"x"}]}"#,
            ),
            Err(block("citations")),
        ),
        (
            "text with citations null",
            user_blocks(r#"{"type":"text","text":"x","citations":null}"#),
            Ok(()),
        ),
        (
            "a member nobody knows on a text block",
            user_blocks(r#"{"type":"text","text":"x","colour":"red"}"#),
            Err(Carry::Unknown),
        ),
        (
            "a tool_use with caller direct",
            assistant_blocks(
                r#"{"type":"tool_use","id":"toolu_1","name":"Read","input":{},"caller":{"type":"direct"}}"#,
            ),
            Ok(()),
        ),
        (
            "a tool_use with another caller",
            assistant_blocks(
                r#"{"type":"tool_use","id":"toolu_1","name":"Read","input":{},"caller":{"type":"code_execution_20250825","tool_id":"srvtoolu_1"}}"#,
            ),
            Err(block("tool_use")),
        ),
    ]
}

#[test]
fn images_are_base64_of_a_documented_type_or_a_web_address() {
    check(vec![
        ("an image/jpeg", base64_of("image/jpeg"), Ok(())),
        ("an image/png", base64_of("image/png"), Ok(())),
        ("an image/gif", base64_of("image/gif"), Ok(())),
        ("an image/webp", base64_of("image/webp"), Ok(())),
        (
            "an image/tiff",
            base64_of("image/tiff"),
            Err(Carry::ImageSource),
        ),
        (
            "an image by web address",
            image_of(r#"{"type":"url","url":"https://example.com/a.png"}"#),
            Ok(()),
        ),
        (
            "an image by file id",
            image_of(r#"{"type":"file","file_id":"file_1"}"#),
            Err(Carry::ImageSource),
        ),
    ]);
}

#[test]
fn call_ids_are_one_to_sixty_four_characters() {
    let call = |id: &str, input: &str| {
        assistant_blocks(&format!(
            r#"{{"type":"tool_use","id":"{id}","name":"Read","input":{input}}}"#
        ))
    };
    let result = |id: &str| {
        user_blocks(&format!(
            r#"{{"type":"tool_result","tool_use_id":"{id}","content":"x"}}"#
        ))
    };
    let (max, over) = ("a".repeat(64), "a".repeat(65));
    check(vec![
        ("tool_use id of 64", call(&max, "{}"), Ok(())),
        ("tool_use id of 65", call(&over, "{}"), Err(Carry::CallId)),
        ("tool_use id empty", call("", "{}"), Err(Carry::CallId)),
        ("tool_use_id of 64", result(&max), Ok(())),
        ("tool_use_id of 65", result(&over), Err(Carry::CallId)),
        ("tool_use_id empty", result(""), Err(Carry::CallId)),
        (
            "tool_use input a list",
            call("toolu_1", "[1]"),
            Err(Carry::Unknown),
        ),
    ]);
}

#[test]
fn only_plain_function_tools_are_carried() {
    let rows: [(&str, &str, Result<&str, Carry>); 11] = [
        (
            "a plain tool",
            r#"[{"name":"Read","description":"reads","input_schema":{"type":"object"}}]"#,
            Ok(r#"Read|reads|{"type":"object"}"#),
        ),
        (
            "type custom",
            r#"[{"type":"custom","name":"Read","input_schema":{"type":"object"}}]"#,
            Ok(r#"Read|-|{"type":"object"}"#),
        ),
        (
            "cache_control and eager_input_streaming",
            r#"[{"name":"Read","input_schema":{},"cache_control":{"type":"ephemeral"},"eager_input_streaming":true}]"#,
            Ok("Read|-|{}"),
        ),
        (
            "strict true",
            r#"[{"name":"Read","input_schema":{},"strict":true}]"#,
            Err(Carry::Tool),
        ),
        (
            "strict false",
            r#"[{"name":"Read","input_schema":{},"strict":false}]"#,
            Ok("Read|-|{}"),
        ),
        (
            "defer_loading true",
            r#"[{"name":"Read","input_schema":{},"defer_loading":true}]"#,
            Err(Carry::Tool),
        ),
        (
            "another type",
            r#"[{"type":"web_search_20250305","name":"web_search","max_uses":5}]"#,
            Err(Carry::Tool),
        ),
        (
            "a member nobody knows",
            r#"[{"name":"Read","input_schema":{},"colour":"red"}]"#,
            Err(Carry::Tool),
        ),
        (
            "a schema with odd spacing",
            r#"[{"name":"Read","input_schema":{ "type" :  "object" ,"properties":{ } }}]"#,
            Ok(r#"Read|-|{ "type" :  "object" ,"properties":{ } }"#),
        ),
        ("null", "null", Ok("")),
        ("empty", "[]", Ok("")),
    ];
    for (name, tools, expected) in rows {
        let body = wrap(&format!(r#","tools":{tools}"#));
        let read = read(body.as_bytes());
        let got = read.as_ref().map_err(Carry::clone).map(|asked| {
            asked
                .tools
                .iter()
                .map(|tool| {
                    format!(
                        "{}|{}|{}",
                        tool.name,
                        tool.description.as_deref().unwrap_or("-"),
                        tool.schema.get()
                    )
                })
                .collect::<Vec<_>>()
                .join(" ")
        });
        assert_eq!(got, expected.map(str::to_owned), "{name}");
    }
}

/// (name, `tool_choice`, the choice in a word and whether parallel calls stay allowed).
type ChoiceRow = (
    &'static str,
    Option<&'static str>,
    Result<(&'static str, bool), Carry>,
);

#[test]
fn tool_choice_is_one_of_four_kinds() {
    let rows: [ChoiceRow; 9] = [
        ("auto", Some(r#"{"type":"auto"}"#), Ok(("auto", true))),
        ("any", Some(r#"{"type":"any"}"#), Ok(("any", true))),
        ("none", Some(r#"{"type":"none"}"#), Ok(("none", true))),
        (
            "tool",
            Some(r#"{"type":"tool","name":"Read"}"#),
            Ok(("tool:Read", true)),
        ),
        (
            "disable_parallel_tool_use",
            Some(r#"{"type":"any","disable_parallel_tool_use":true}"#),
            Ok(("any", false)),
        ),
        (
            "an unknown type",
            Some(r#"{"type":"some"}"#),
            Err(Carry::ToolChoice),
        ),
        (
            "tool without a name",
            Some(r#"{"type":"tool"}"#),
            Err(Carry::ToolChoice),
        ),
        ("null", Some("null"), Ok(("-", true))),
        ("absent", None, Ok(("-", true))),
    ];
    for (name, choice, expected) in rows {
        let body = wrap(&choice.map_or(String::new(), |c| format!(r#","tool_choice":{c}"#)));
        let read = read(body.as_bytes());
        let got = read.as_ref().map_err(Carry::clone).map(|asked| {
            let kind = match &asked.choice {
                None => "-".to_owned(),
                Some(Choice::Auto) => "auto".to_owned(),
                Some(Choice::Any) => "any".to_owned(),
                Some(Choice::None) => "none".to_owned(),
                Some(Choice::Tool(tool)) => format!("tool:{tool}"),
            };
            (kind, asked.parallel)
        });
        assert_eq!(
            got,
            expected.map(|(kind, parallel)| (kind.to_owned(), parallel)),
            "{name}"
        );
    }
}

const USER: &str = r#"{"role":"user","content":"next"}"#;
const ASSISTANT: &str = r#"{"role":"assistant","content":"ok"}"#;
const CALL: &str = r#"{"role":"assistant","content":[{"type":"tool_use","id":"toolu_1","name":"Read","input":{}}]}"#;
const RESULT: &str =
    r#"{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"x"}]}"#;
const LOW: &str = r#"{"role":"system","content":[],"output_config":{"effort":"low"}}"#;
const MEDIUM: &str = r#"{"role":"system","content":[],"output_config":{"effort":"medium"}}"#;

/// (name, the top-level `output_config`, the turns, the effort in force).
type EffortRow = (
    &'static str,
    Option<&'static str>,
    &'static [&'static str],
    Result<Option<&'static str>, Carry>,
);

#[test]
fn the_effort_in_force_follows_the_per_message_rule() {
    let high = Some(r#"{"effort":"high"}"#);
    let rows: [EffortRow; 10] = [
        (
            "the page's own example",
            high,
            &[USER, ASSISTANT, LOW, USER],
            Ok(Some("low")),
        ),
        (
            "an entry straight after new input",
            high,
            &[USER, LOW],
            Ok(Some("low")),
        ),
        (
            "an entry after only tool results waits",
            high,
            &[USER, CALL, RESULT, LOW],
            Ok(Some("high")),
        ),
        (
            "and applies from the next new input",
            high,
            &[USER, CALL, RESULT, LOW, CALL, RESULT, USER],
            Ok(Some("low")),
        ),
        (
            "two entries, the later wins",
            high,
            &[USER, ASSISTANT, LOW, MEDIUM, USER],
            Ok(Some("medium")),
        ),
        ("no entry", high, &[USER, ASSISTANT, USER], Ok(Some("high"))),
        ("nothing at all", None, &[USER], Ok(None)),
        (
            "an entry holds for the turns after it",
            high,
            &[USER, ASSISTANT, LOW, USER, ASSISTANT, USER],
            Ok(Some("low")),
        ),
        (
            "a top-level effort with a space",
            Some(r#"{"effort":"very high"}"#),
            &[USER],
            Err(Carry::Unknown),
        ),
        (
            "an entry's effort with a space",
            high,
            &[
                USER,
                r#"{"role":"system","content":[],"output_config":{"effort":"very high"}}"#,
            ],
            Err(Carry::Unknown),
        ),
    ];
    for (name, config, turns, expected) in rows {
        // The top level comes after `messages`, so the walk does not depend on body order.
        let config = config.map_or(String::new(), |c| format!(r#","output_config":{c}"#));
        let body = wrap(&format!(r#","messages":[{}]{config}"#, turns.join(",")));
        let read = read(body.as_bytes());
        assert_eq!(
            read.map(|asked| asked.effort),
            expected.map(|effort| effort.map(str::to_owned)),
            "{name}"
        );
    }
}

#[test]
fn a_system_entry_with_text_and_an_effort_gives_both() {
    let body = wrap(&format!(
        r#","output_config":{{"effort":"high"}},"messages":[{USER},{ASSISTANT},{{"role":"system","content":"remember","output_config":{{"effort":"low"}}}},{USER}]"#
    ));
    let asked = read(body.as_bytes()).expect("reads");
    assert_eq!(
        sketch(&asked),
        "user(text:next) assistant(text:ok) system(remember) user(text:next)"
    );
    assert_eq!(asked.effort.as_deref(), Some("low"));
}

#[test]
fn label_lets_only_plain_names_into_a_sentence() {
    let (max, over) = ("a".repeat(40), "a".repeat(41));
    let rows: [(&str, &str, bool); 8] = [
        ("plain", "temperature", true),
        ("with a hyphen", "inference-geo", true),
        ("40 characters", &max, true),
        ("empty", "", false),
        ("41 characters", &over, false),
        ("a space", "a b", false),
        ("a quote", "a\"b", false),
        ("a non-ASCII letter", "caf\u{e9}", false),
    ];
    for (name, raw, fits) in rows {
        let expected = fits.then(|| raw.to_owned());
        assert_eq!(label(raw), expected, "{name}");
    }
}
