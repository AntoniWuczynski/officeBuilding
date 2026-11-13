// These replies are written from OpenAI's published documentation (U1 "Create a model
// response", U2 "Responses streaming events", U3 "Function calling"), each is cited, and none
// was captured from a real account. Every id is invented. `@MODEL@` stands for the model the
// fixture settings map, so no model name is written here.

use super::*;
use crate::routing::fixtures;
use serde_json::Value;

/// The model A Human mapped for the fixture's `OpenAI` connection.
fn model() -> String {
    fixtures::oai_target().model
}

// U1, the streaming example of "Create a model response" (its omitted intermediate events
// filled with a second delta). Shared by every fixture below as the first event.
const CREATED: &str = r#"{"type":"response.created","response":{"id":"resp_inv0001","object":"response","created_at":1741290958,"status":"in_progress","error":null,"incomplete_details":null,"model":"@MODEL@","output":[],"parallel_tool_calls":true,"store":false,"tool_choice":"auto","tools":[],"usage":null},"sequence_number":0}"#;

/// U1's streaming example: plain text, with its usage.
const PLAIN: &[&str] = &[
    CREATED,
    r#"{"type":"response.in_progress","response":{"id":"resp_inv0001","object":"response","status":"in_progress","model":"@MODEL@","output":[],"usage":null},"sequence_number":1}"#,
    r#"{"type":"response.output_item.added","output_index":0,"item":{"id":"msg_inv0001","type":"message","status":"in_progress","role":"assistant","content":[]},"sequence_number":2}"#,
    r#"{"type":"response.content_part.added","item_id":"msg_inv0001","output_index":0,"content_index":0,"part":{"type":"output_text","text":"","annotations":[],"logprobs":[]},"sequence_number":3}"#,
    r#"{"type":"response.output_text.delta","item_id":"msg_inv0001","output_index":0,"content_index":0,"delta":"Hi","sequence_number":4,"logprobs":[]}"#,
    r#"{"type":"response.output_text.delta","item_id":"msg_inv0001","output_index":0,"content_index":0,"delta":" there! How can I assist you today?","sequence_number":5,"logprobs":[]}"#,
    r#"{"type":"response.output_text.done","item_id":"msg_inv0001","output_index":0,"content_index":0,"text":"Hi there! How can I assist you today?","sequence_number":6,"logprobs":[]}"#,
    r#"{"type":"response.content_part.done","item_id":"msg_inv0001","output_index":0,"content_index":0,"part":{"type":"output_text","text":"Hi there! How can I assist you today?","annotations":[],"logprobs":[]},"sequence_number":7}"#,
    r#"{"type":"response.output_item.done","output_index":0,"item":{"id":"msg_inv0001","type":"message","status":"completed","role":"assistant","content":[{"type":"output_text","text":"Hi there! How can I assist you today?","annotations":[],"logprobs":[]}]},"sequence_number":8}"#,
    r#"{"type":"response.completed","response":{"id":"resp_inv0001","object":"response","status":"completed","error":null,"incomplete_details":null,"model":"@MODEL@","output":[{"id":"msg_inv0001","type":"message","status":"completed","role":"assistant","content":[{"type":"output_text","text":"Hi there! How can I assist you today?","annotations":[],"logprobs":[]}]}],"usage":{"input_tokens":37,"output_tokens":11,"output_tokens_details":{"reasoning_tokens":0},"total_tokens":48,"input_tokens_details":{"cached_tokens":0,"cache_write_tokens":0}}},"sequence_number":9}"#,
];

/// U3's streaming sample (one call in fragments), wrapped in `response.created` and
/// `response.completed` (U2).
const TOOL_FRAGMENTS: &[&str] = &[
    CREATED,
    r#"{"type":"response.output_item.added","response_id":"resp_inv0001","output_index":0,"item":{"type":"function_call","id":"fc_inv0001","call_id":"call_inv0001","name":"get_weather","arguments":""}}"#,
    r#"{"type":"response.function_call_arguments.delta","response_id":"resp_inv0001","item_id":"fc_inv0001","output_index":0,"delta":"{\""}"#,
    r#"{"type":"response.function_call_arguments.delta","response_id":"resp_inv0001","item_id":"fc_inv0001","output_index":0,"delta":"location"}"#,
    r#"{"type":"response.function_call_arguments.delta","response_id":"resp_inv0001","item_id":"fc_inv0001","output_index":0,"delta":"\":\""}"#,
    r#"{"type":"response.function_call_arguments.delta","response_id":"resp_inv0001","item_id":"fc_inv0001","output_index":0,"delta":"Paris"}"#,
    r#"{"type":"response.function_call_arguments.delta","response_id":"resp_inv0001","item_id":"fc_inv0001","output_index":0,"delta":","}"#,
    r#"{"type":"response.function_call_arguments.delta","response_id":"resp_inv0001","item_id":"fc_inv0001","output_index":0,"delta":" France"}"#,
    r#"{"type":"response.function_call_arguments.delta","response_id":"resp_inv0001","item_id":"fc_inv0001","output_index":0,"delta":"\"}"}"#,
    r#"{"type":"response.function_call_arguments.done","response_id":"resp_inv0001","item_id":"fc_inv0001","output_index":0,"arguments":"{\"location\":\"Paris, France\"}"}"#,
    r#"{"type":"response.output_item.done","response_id":"resp_inv0001","output_index":0,"item":{"type":"function_call","id":"fc_inv0001","call_id":"call_inv0001","name":"get_weather","arguments":"{\"location\":\"Paris, France\"}"}}"#,
    r#"{"type":"response.completed","response":{"id":"resp_inv0001","status":"completed","error":null,"incomplete_details":null,"model":"@MODEL@","usage":{"input_tokens":60,"output_tokens":20,"input_tokens_details":{"cached_tokens":0,"cache_write_tokens":0}}},"sequence_number":12}"#,
];

/// Text, then two calls one after the other (U2's item events, U3's call items).
const TEXT_THEN_TWO_CALLS: &[&str] = &[
    CREATED,
    r#"{"type":"response.output_item.added","output_index":0,"item":{"id":"msg_inv0002","type":"message","status":"in_progress","role":"assistant","content":[]},"sequence_number":1}"#,
    r#"{"type":"response.content_part.added","item_id":"msg_inv0002","output_index":0,"content_index":0,"part":{"type":"output_text","text":"","annotations":[]},"sequence_number":2}"#,
    r#"{"type":"response.output_text.delta","item_id":"msg_inv0002","output_index":0,"content_index":0,"delta":"Reading both.","sequence_number":3}"#,
    r#"{"type":"response.content_part.done","item_id":"msg_inv0002","output_index":0,"content_index":0,"part":{"type":"output_text","text":"Reading both.","annotations":[]},"sequence_number":4}"#,
    r#"{"type":"response.output_item.done","output_index":0,"item":{"id":"msg_inv0002","type":"message","status":"completed","role":"assistant","content":[{"type":"output_text","text":"Reading both.","annotations":[]}]},"sequence_number":5}"#,
    r#"{"type":"response.output_item.added","output_index":1,"item":{"type":"function_call","id":"fc_inv0002","call_id":"call_inv0002","name":"read","arguments":""},"sequence_number":6}"#,
    r#"{"type":"response.function_call_arguments.delta","item_id":"fc_inv0002","output_index":1,"delta":"{\"path\":","sequence_number":7}"#,
    r#"{"type":"response.function_call_arguments.delta","item_id":"fc_inv0002","output_index":1,"delta":"\"a.rs\"}","sequence_number":8}"#,
    r#"{"type":"response.output_item.done","output_index":1,"item":{"type":"function_call","id":"fc_inv0002","call_id":"call_inv0002","name":"read","arguments":"{\"path\":\"a.rs\"}"},"sequence_number":9}"#,
    r#"{"type":"response.output_item.added","output_index":2,"item":{"type":"function_call","id":"fc_inv0003","call_id":"call_inv0003","name":"read","arguments":""},"sequence_number":10}"#,
    r#"{"type":"response.function_call_arguments.delta","item_id":"fc_inv0003","output_index":2,"delta":"{\"path\":\"b.rs\"}","sequence_number":11}"#,
    r#"{"type":"response.output_item.done","output_index":2,"item":{"type":"function_call","id":"fc_inv0003","call_id":"call_inv0003","name":"read","arguments":"{\"path\":\"b.rs\"}"},"sequence_number":12}"#,
    r#"{"type":"response.completed","response":{"id":"resp_inv0001","status":"completed","model":"@MODEL@","usage":{"input_tokens":80,"output_tokens":30}},"sequence_number":13}"#,
];

/// Two calls open at once whose argument deltas alternate (U2: each delta names its
/// `output_index`).
const ALTERNATING: &[&str] = &[
    CREATED,
    r#"{"type":"response.output_item.added","output_index":0,"item":{"type":"function_call","id":"fc_inv0004","call_id":"call_inv0004","name":"read","arguments":""},"sequence_number":1}"#,
    r#"{"type":"response.output_item.added","output_index":1,"item":{"type":"function_call","id":"fc_inv0005","call_id":"call_inv0005","name":"grep","arguments":""},"sequence_number":2}"#,
    r#"{"type":"response.function_call_arguments.delta","item_id":"fc_inv0004","output_index":0,"delta":"{\"path\":","sequence_number":3}"#,
    r#"{"type":"response.function_call_arguments.delta","item_id":"fc_inv0005","output_index":1,"delta":"{\"pattern\":","sequence_number":4}"#,
    r#"{"type":"response.function_call_arguments.delta","item_id":"fc_inv0004","output_index":0,"delta":"\"a.rs\"}","sequence_number":5}"#,
    r#"{"type":"response.function_call_arguments.delta","item_id":"fc_inv0005","output_index":1,"delta":"\"fn main\"}","sequence_number":6}"#,
    r#"{"type":"response.output_item.done","output_index":0,"item":{"type":"function_call","id":"fc_inv0004","call_id":"call_inv0004","name":"read","arguments":"{\"path\":\"a.rs\"}"},"sequence_number":7}"#,
    r#"{"type":"response.output_item.done","output_index":1,"item":{"type":"function_call","id":"fc_inv0005","call_id":"call_inv0005","name":"grep","arguments":"{\"pattern\":\"fn main\"}"},"sequence_number":8}"#,
    r#"{"type":"response.completed","response":{"id":"resp_inv0001","status":"completed","model":"@MODEL@","usage":null},"sequence_number":9}"#,
];

/// A call that arrives whole, only in `response.output_item.done` (U3's done item).
const WHOLE_CALL: &[&str] = &[
    CREATED,
    r#"{"type":"response.output_item.done","output_index":0,"item":{"type":"function_call","id":"fc_inv0006","call_id":"call_inv0006","name":"get_weather","arguments":"{\"location\":\"Paris, France\"}"},"sequence_number":1}"#,
    r#"{"type":"response.completed","response":{"id":"resp_inv0001","status":"completed","model":"@MODEL@","usage":null},"sequence_number":2}"#,
];

/// The same, with empty arguments.
const WHOLE_CALL_EMPTY: &[&str] = &[
    CREATED,
    r#"{"type":"response.output_item.done","output_index":0,"item":{"type":"function_call","id":"fc_inv0007","call_id":"call_inv0007","name":"list","arguments":""},"sequence_number":1}"#,
    r#"{"type":"response.completed","response":{"id":"resp_inv0001","status":"completed","model":"@MODEL@","usage":null},"sequence_number":2}"#,
];

/// A reasoning item with its summary and reasoning text before the visible text (U2's
/// `response.reasoning_summary_*` and `response.reasoning_text.*` events).
const REASONING: &[&str] = &[
    CREATED,
    r#"{"type":"response.output_item.added","output_index":0,"item":{"id":"rs_inv0001","type":"reasoning","summary":[]},"sequence_number":1}"#,
    r#"{"type":"response.reasoning_summary_part.added","item_id":"rs_inv0001","output_index":0,"summary_index":0,"part":{"type":"summary_text","text":""},"sequence_number":2}"#,
    r#"{"type":"response.reasoning_summary_text.delta","item_id":"rs_inv0001","output_index":0,"summary_index":0,"delta":"**Weighing the greeting**","sequence_number":3}"#,
    r#"{"type":"response.reasoning_summary_text.delta","item_id":"rs_inv0001","output_index":0,"summary_index":0,"delta":" and how to answer it.","sequence_number":4}"#,
    r#"{"type":"response.reasoning_summary_text.done","item_id":"rs_inv0001","output_index":0,"summary_index":0,"text":"**Weighing the greeting** and how to answer it.","sequence_number":5}"#,
    r#"{"type":"response.reasoning_summary_part.done","item_id":"rs_inv0001","output_index":0,"summary_index":0,"part":{"type":"summary_text","text":"**Weighing the greeting** and how to answer it."},"sequence_number":6}"#,
    r#"{"type":"response.content_part.added","item_id":"rs_inv0001","output_index":0,"content_index":0,"part":{"type":"reasoning_text","text":""},"sequence_number":7}"#,
    r#"{"type":"response.reasoning_text.delta","item_id":"rs_inv0001","output_index":0,"content_index":0,"delta":"thinking it over","sequence_number":8}"#,
    r#"{"type":"response.content_part.done","item_id":"rs_inv0001","output_index":0,"content_index":0,"part":{"type":"reasoning_text","text":"thinking it over"},"sequence_number":9}"#,
    r#"{"type":"response.output_item.done","output_index":0,"item":{"id":"rs_inv0001","type":"reasoning","summary":[{"type":"summary_text","text":"**Weighing the greeting** and how to answer it."}],"encrypted_content":"ZW5jcnlwdGVk"},"sequence_number":10}"#,
    r#"{"type":"response.output_item.added","output_index":1,"item":{"id":"msg_inv0003","type":"message","status":"in_progress","role":"assistant","content":[]},"sequence_number":11}"#,
    r#"{"type":"response.content_part.added","item_id":"msg_inv0003","output_index":1,"content_index":0,"part":{"type":"output_text","text":"","annotations":[]},"sequence_number":12}"#,
    r#"{"type":"response.output_text.delta","item_id":"msg_inv0003","output_index":1,"content_index":0,"delta":"Hello!","sequence_number":13}"#,
    r#"{"type":"response.content_part.done","item_id":"msg_inv0003","output_index":1,"content_index":0,"part":{"type":"output_text","text":"Hello!","annotations":[]},"sequence_number":14}"#,
    r#"{"type":"response.output_item.done","output_index":1,"item":{"id":"msg_inv0003","type":"message","status":"completed","role":"assistant","content":[{"type":"output_text","text":"Hello!","annotations":[]}]},"sequence_number":15}"#,
    r#"{"type":"response.completed","response":{"id":"resp_inv0001","status":"completed","model":"@MODEL@","usage":{"input_tokens":9,"output_tokens":40,"output_tokens_details":{"reasoning_tokens":35}}},"sequence_number":16}"#,
];

/// The model's refusal (U2's `response.content_part.added` with a `refusal` part and
/// `response.refusal.delta`).
const REFUSAL: &[&str] = &[
    CREATED,
    r#"{"type":"response.output_item.added","output_index":0,"item":{"id":"msg_inv0004","type":"message","status":"in_progress","role":"assistant","content":[]},"sequence_number":1}"#,
    r#"{"type":"response.content_part.added","item_id":"msg_inv0004","output_index":0,"content_index":0,"part":{"type":"refusal","refusal":""},"sequence_number":2}"#,
    r#"{"type":"response.refusal.delta","item_id":"msg_inv0004","output_index":0,"content_index":0,"delta":"refusal text so far","sequence_number":3}"#,
    r#"{"type":"response.refusal.done","item_id":"msg_inv0004","output_index":0,"content_index":0,"refusal":"refusal text so far","sequence_number":4}"#,
    r#"{"type":"response.content_part.done","item_id":"msg_inv0004","output_index":0,"content_index":0,"part":{"type":"refusal","refusal":"refusal text so far"},"sequence_number":5}"#,
    r#"{"type":"response.output_item.done","output_index":0,"item":{"id":"msg_inv0004","type":"message","status":"completed","role":"assistant","content":[{"type":"refusal","refusal":"refusal text so far"}]},"sequence_number":6}"#,
    r#"{"type":"response.completed","response":{"id":"resp_inv0001","status":"completed","model":"@MODEL@","usage":null},"sequence_number":7}"#,
];

/// Text whose block is still open when the reply ends incomplete (U2's `response.incomplete`
/// example). `@REASON@` is the `incomplete_details.reason`.
const INCOMPLETE: &[&str] = &[
    CREATED,
    r#"{"type":"response.output_item.added","output_index":0,"item":{"id":"msg_inv0005","type":"message","status":"in_progress","role":"assistant","content":[]},"sequence_number":1}"#,
    r#"{"type":"response.content_part.added","item_id":"msg_inv0005","output_index":0,"content_index":0,"part":{"type":"output_text","text":"","annotations":[]},"sequence_number":2}"#,
    r#"{"type":"response.output_text.delta","item_id":"msg_inv0005","output_index":0,"content_index":0,"delta":"In a shimmering","sequence_number":3}"#,
    r#"{"type":"response.incomplete","response":{"id":"resp_inv0001","object":"response","status":"incomplete","error":null,"incomplete_details":{"reason":"@REASON@"},"model":"@MODEL@","usage":null},"sequence_number":4}"#,
];

/// U2's `response.failed` example.
const FAILED: &[&str] = &[
    CREATED,
    r#"{"type":"response.failed","response":{"id":"resp_inv0001","object":"response","status":"failed","error":{"code":"server_error","message":"The model failed to generate a response."},"incomplete_details":null,"model":"@MODEL@","output":[],"usage":null},"sequence_number":1}"#,
];

/// U2's `error` event.
const ERROR: &[&str] = &[
    CREATED,
    r#"{"type":"error","code":"ERR_SOMETHING","message":"Something went wrong","param":null,"sequence_number":1}"#,
];

/// A completed reply with `@USAGE@` as its `usage` and nothing before it but `response.created`.
const EMPTY: &[&str] = &[
    CREATED,
    r#"{"type":"response.completed","response":{"id":"resp_inv0001","status":"completed","model":"@MODEL@","output":[],"usage":@USAGE@},"sequence_number":1}"#,
];

/// What a fixture came to: every frame as (event name, data), the bytes, the end, and the
/// bytes of the last step.
struct Run {
    frames: Vec<(String, Value)>,
    bytes: String,
    end: Option<End>,
    last: Vec<u8>,
}

impl Run {
    fn names(&self) -> Vec<&str> {
        self.frames.iter().map(|(name, _)| name.as_str()).collect()
    }

    fn of(&self, name: &str) -> Vec<&Value> {
        self.frames
            .iter()
            .filter(|(n, _)| n == name)
            .map(|(_, data)| data)
            .collect()
    }

    fn stop_reason(&self) -> &Value {
        &self.of("message_delta")[0]["delta"]["stop_reason"]
    }

    fn usage(&self) -> &Value {
        &self.of("message_delta")[0]["usage"]
    }
}

fn events(fixture: &[&str], swaps: &[(&str, &str)]) -> Vec<String> {
    fixture
        .iter()
        .map(|event| {
            swaps
                .iter()
                .fold(event.replace("@MODEL@", &model()), |e, (from, to)| {
                    e.replace(from, to)
                })
        })
        .collect()
}

/// Feeds the events to a fresh `Reply` until a step ends it.
fn feed(events: &[String]) -> Run {
    let mut reply = Reply::new();
    let mut out = Vec::new();
    let mut run = Run {
        frames: Vec::new(),
        bytes: String::new(),
        end: None,
        last: Vec::new(),
    };
    for event in events {
        let step = reply.take(event.as_bytes());
        out.extend_from_slice(&step.out);
        run.last = step.out;
        if step.end.is_some() {
            run.end = step.end;
            break;
        }
    }
    run.bytes = String::from_utf8(out).expect("utf-8");
    for frame in run.bytes.split_terminator("\n\n") {
        let (event, data) = frame
            .strip_prefix("event: ")
            .and_then(|rest| rest.split_once("\ndata: "))
            .unwrap_or_else(|| panic!("a frame is an event line and a data line: {frame:?}"));
        let data: Value = serde_json::from_str(data).expect("each frame's data is JSON");
        run.frames.push((event.to_owned(), data));
    }
    run
}

fn run(fixture: &[&str]) -> Run {
    feed(&events(fixture, &[]))
}

/// The `partial_json` fragments of the block at `index`, joined.
fn joined_args(run: &Run, index: u64) -> String {
    run.of("content_block_delta")
        .iter()
        .filter(|d| d["index"] == index)
        .map(|d| d["delta"]["partial_json"].as_str().expect("an input delta"))
        .collect()
}

#[test]
fn plain_text_is_the_documented_flow_byte_for_byte() {
    let run = run(PLAIN);
    assert_eq!(run.end, Some(End::Clean));
    let model = model();
    let expected = [
        format!(
            r#"{{"type":"message_start","message":{{"id":"resp_inv0001","type":"message","role":"assistant","content":[],"model":"{model}","stop_reason":null,"stop_sequence":null,"usage":{{"input_tokens":0,"output_tokens":0}}}}}}"#
        ),
        r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#.to_owned(),
        r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hi"}}"#.to_owned(),
        r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" there! How can I assist you today?"}}"#.to_owned(),
        r#"{"type":"content_block_stop","index":0}"#.to_owned(),
        r#"{"type":"message_delta","delta":{"stop_reason":"end_turn","stop_sequence":null},"usage":{"input_tokens":37,"cache_creation_input_tokens":0,"cache_read_input_tokens":0,"output_tokens":11}}"#.to_owned(),
        r#"{"type":"message_stop"}"#.to_owned(),
    ];
    let expected: String = expected
        .iter()
        .map(|data| {
            let name = data
                .strip_prefix(r#"{"type":""#)
                .and_then(|rest| rest.split_once('"'))
                .map(|(name, _)| name)
                .expect("type first");
            String::from_utf8(sse::frame(name, data.as_bytes())).expect("utf-8")
        })
        .collect();
    assert_eq!(run.bytes, expected);
}

#[test]
fn message_start_carries_openai_id_and_model() {
    let run = run(PLAIN);
    let start = run.of("message_start");
    let message = &start[0]["message"];
    assert_eq!(message["id"], "resp_inv0001");
    assert_eq!(message["model"], model().as_str());
    assert_eq!(message["content"], Value::Array(Vec::new()));
    assert_eq!(message["stop_reason"], Value::Null);
    assert_eq!(message["stop_sequence"], Value::Null);
}

#[test]
fn every_frame_is_named_by_its_type() {
    let fixtures = [
        ("plain", PLAIN),
        ("fragments", TOOL_FRAGMENTS),
        ("text then calls", TEXT_THEN_TWO_CALLS),
        ("alternating", ALTERNATING),
        ("whole", WHOLE_CALL),
        ("reasoning", REASONING),
        ("refusal", REFUSAL),
    ];
    for (name, fixture) in fixtures {
        let run = run(fixture);
        assert!(!run.frames.is_empty(), "{name}");
        for (event, data) in &run.frames {
            assert_eq!(data["type"], event.as_str(), "{name}");
        }
    }
}

#[test]
fn a_tool_call_in_fragments() {
    let run = run(TOOL_FRAGMENTS);
    assert_eq!(run.end, Some(End::Clean));
    assert_eq!(
        run.names(),
        [
            "message_start",
            "content_block_start",
            "content_block_delta",
            "content_block_delta",
            "content_block_delta",
            "content_block_delta",
            "content_block_delta",
            "content_block_delta",
            "content_block_delta",
            "content_block_stop",
            "message_delta",
            "message_stop",
        ]
    );
    let start = run.of("content_block_start");
    assert_eq!(
        start[0]["content_block"],
        serde_json::json!({"type":"tool_use","id":"call_inv0001","name":"get_weather","input":{}})
    );
    assert_eq!(joined_args(&run, 0), r#"{"location":"Paris, France"}"#);
    assert_eq!(run.stop_reason(), "tool_use");
}

#[test]
fn text_then_two_calls_take_indexes_in_order() {
    let run = run(TEXT_THEN_TWO_CALLS);
    assert_eq!(run.end, Some(End::Clean));
    let indexed: Vec<(&str, u64)> = run
        .frames
        .iter()
        .filter_map(|(name, data)| data["index"].as_u64().map(|i| (name.as_str(), i)))
        .collect();
    assert_eq!(
        indexed,
        [
            ("content_block_start", 0),
            ("content_block_delta", 0),
            ("content_block_stop", 0),
            ("content_block_start", 1),
            ("content_block_delta", 1),
            ("content_block_delta", 1),
            ("content_block_stop", 1),
            ("content_block_start", 2),
            ("content_block_delta", 2),
            ("content_block_stop", 2),
        ]
    );
    let starts = run.of("content_block_start");
    assert_eq!(starts[0]["content_block"]["type"], "text");
    assert_eq!(starts[1]["content_block"]["id"], "call_inv0002");
    assert_eq!(starts[2]["content_block"]["id"], "call_inv0003");
    assert_eq!(joined_args(&run, 1), r#"{"path":"a.rs"}"#);
    assert_eq!(joined_args(&run, 2), r#"{"path":"b.rs"}"#);
    assert_eq!(run.stop_reason(), "tool_use");
}

#[test]
fn alternating_deltas_keep_to_their_own_call() {
    let run = run(ALTERNATING);
    assert_eq!(run.end, Some(End::Clean));
    let deltas: Vec<u64> = run
        .of("content_block_delta")
        .iter()
        .map(|d| d["index"].as_u64().expect("index"))
        .collect();
    assert_eq!(deltas, [0, 1, 0, 1]);
    let starts = run.of("content_block_start");
    assert_eq!(starts[0]["content_block"]["name"], "read");
    assert_eq!(starts[1]["content_block"]["name"], "grep");
    assert_eq!(joined_args(&run, 0), r#"{"path":"a.rs"}"#);
    assert_eq!(joined_args(&run, 1), r#"{"pattern":"fn main"}"#);
    let stops: Vec<&Value> = run
        .of("content_block_stop")
        .into_iter()
        .map(|s| &s["index"])
        .collect();
    assert_eq!(stops, [0, 1]);
}

#[test]
fn a_whole_call_gets_one_delta() {
    let rows = [
        (
            "with arguments",
            WHOLE_CALL,
            r#"{"location":"Paris, France"}"#,
        ),
        ("empty arguments", WHOLE_CALL_EMPTY, "{}"),
    ];
    for (name, fixture, args) in rows {
        let run = run(fixture);
        assert_eq!(run.end, Some(End::Clean), "{name}");
        assert_eq!(
            run.names(),
            [
                "message_start",
                "content_block_start",
                "content_block_delta",
                "content_block_stop",
                "message_delta",
                "message_stop",
            ],
            "{name}"
        );
        assert_eq!(
            run.of("content_block_start")[0]["content_block"]["input"],
            serde_json::json!({}),
            "{name}"
        );
        assert_eq!(joined_args(&run, 0), args, "{name}");
        assert_eq!(run.stop_reason(), "tool_use", "{name}");
    }
}

#[test]
fn hidden_reasoning_leaves_no_trace() {
    let run = run(REASONING);
    assert_eq!(run.end, Some(End::Clean));
    for word in ["thinking", "reasoning", "Weighing", "encrypted", "summary"] {
        assert!(!run.bytes.contains(word), "{word} in {}", run.bytes);
    }
    let start = run.of("content_block_start");
    assert_eq!(start.len(), 1);
    assert_eq!(start[0]["index"], 0);
    assert_eq!(start[0]["content_block"]["type"], "text");
    assert_eq!(run.of("content_block_delta")[0]["delta"]["text"], "Hello!");
    assert_eq!(run.stop_reason(), "end_turn");
}

#[test]
fn a_refusal_is_text() {
    let run = run(REFUSAL);
    assert_eq!(run.end, Some(End::Clean));
    assert_eq!(
        run.of("content_block_start")[0]["content_block"],
        serde_json::json!({"type":"text","text":""})
    );
    assert_eq!(
        run.of("content_block_delta")[0]["delta"],
        serde_json::json!({"type":"text_delta","text":"refusal text so far"})
    );
    assert_eq!(run.stop_reason(), "end_turn");
}

#[test]
fn stop_reasons() {
    let rows: [(&str, Vec<String>, &str); 4] = [
        ("completed without a call", events(PLAIN, &[]), "end_turn"),
        (
            "completed with a call",
            events(TOOL_FRAGMENTS, &[]),
            "tool_use",
        ),
        (
            "incomplete for max_output_tokens",
            events(INCOMPLETE, &[("@REASON@", "max_output_tokens")]),
            "max_tokens",
        ),
        (
            "incomplete for content_filter",
            events(INCOMPLETE, &[("@REASON@", "content_filter")]),
            "refusal",
        ),
    ];
    for (name, events, stop) in rows {
        let run = feed(&events);
        assert_eq!(run.end, Some(End::Clean), "{name}");
        assert_eq!(run.stop_reason(), stop, "{name}");
        assert_eq!(run.names().last().copied(), Some("message_stop"), "{name}");
        assert_eq!(
            run.of("message_delta")[0]["delta"]["stop_sequence"],
            Value::Null,
            "{name}"
        );
    }
}

#[test]
fn a_block_still_open_is_closed_before_the_finish() {
    let run = feed(&events(INCOMPLETE, &[("@REASON@", "max_output_tokens")]));
    assert_eq!(
        run.names(),
        [
            "message_start",
            "content_block_start",
            "content_block_delta",
            "content_block_stop",
            "message_delta",
            "message_stop",
        ]
    );
}

#[test]
fn usage() {
    let rows = [
        (
            "cached and written",
            r#"{"input_tokens":100,"output_tokens":7,"input_tokens_details":{"cached_tokens":30,"cache_write_tokens":5}}"#,
            serde_json::json!({"input_tokens":65,"cache_creation_input_tokens":5,"cache_read_input_tokens":30,"output_tokens":7}),
        ),
        (
            "no details",
            r#"{"input_tokens":12,"output_tokens":3}"#,
            serde_json::json!({"input_tokens":12,"cache_creation_input_tokens":0,"cache_read_input_tokens":0,"output_tokens":3}),
        ),
        (
            "none at all",
            "null",
            serde_json::json!({"output_tokens":0}),
        ),
    ];
    for (name, usage, expected) in &rows {
        let run = feed(&events(EMPTY, &[("@USAGE@", usage)]));
        assert_eq!(run.end, Some(End::Clean), "{name}");
        assert_eq!(run.usage(), expected, "{name}");
    }
    // The three numbers sum to OpenAI's `input_tokens` (U10).
    let run = feed(&events(EMPTY, &[("@USAGE@", rows[0].1)]));
    let sum: u64 = [
        "input_tokens",
        "cache_creation_input_tokens",
        "cache_read_input_tokens",
    ]
    .iter()
    .map(|k| run.usage()[k].as_u64().expect("a number"))
    .sum();
    assert_eq!(sum, 100);
}

#[test]
fn an_empty_completed_reply() {
    let run = feed(&events(EMPTY, &[("@USAGE@", "null")]));
    assert_eq!(run.end, Some(End::Clean));
    assert_eq!(
        run.names(),
        ["message_start", "message_delta", "message_stop"]
    );
}

/// Each row: a name, the events, and the failure the last one ends in.
fn failure_rows() -> Vec<(&'static str, Vec<String>, Failure)> {
    let created = CREATED.replace("@MODEL@", &model());
    let with = |tail: &str| vec![created.clone(), tail.to_owned()];
    let open_text = vec![
        created.clone(),
        r#"{"type":"response.output_text.delta","output_index":0,"content_index":0,"delta":"partly"}"#.to_owned(),
    ];
    vec![
        (
            "response.failed",
            events(FAILED, &[]),
            Failure::Failed {
                code: Some("server_error".to_owned()),
                message: Some("The model failed to generate a response.".to_owned()),
            },
        ),
        (
            "error",
            events(ERROR, &[]),
            Failure::Error {
                code: Some("ERR_SOMETHING".to_owned()),
                message: Some("Something went wrong".to_owned()),
            },
        ),
        (
            "incomplete for max_messages",
            events(INCOMPLETE, &[("@REASON@", "max_messages")]),
            Failure::Incomplete {
                reason: Some("max_messages".to_owned()),
            },
        ),
        ("not JSON", with("not json at all"), Failure::Unreadable),
        (
            "no type",
            with(r#"{"sequence_number":1}"#),
            Failure::Unreadable,
        ),
        (
            "a type that is not text",
            with(r#"{"type":7}"#),
            Failure::Unreadable,
        ),
        (
            "an array",
            with(r#"["response.completed"]"#),
            Failure::Unreadable,
        ),
        (
            "a web_search_call item",
            with(
                r#"{"type":"response.output_item.added","output_index":0,"item":{"id":"ws_inv0001","type":"web_search_call","status":"in_progress"}}"#,
            ),
            Failure::Unreadable,
        ),
        (
            "a web_search_call item done",
            with(
                r#"{"type":"response.output_item.done","output_index":0,"item":{"id":"ws_inv0001","type":"web_search_call","status":"completed"}}"#,
            ),
            Failure::Unreadable,
        ),
        (
            "an argument delta for a call never opened",
            with(
                r#"{"type":"response.function_call_arguments.delta","item_id":"fc_inv0009","output_index":0,"delta":"{}"}"#,
            ),
            Failure::Unreadable,
        ),
        (
            "a text delta before response.created",
            vec![open_text[1].clone()],
            Failure::Unreadable,
        ),
        (
            "a function call with an empty call_id",
            with(
                r#"{"type":"response.output_item.added","output_index":0,"item":{"type":"function_call","id":"fc_inv0010","call_id":"","name":"read","arguments":""}}"#,
            ),
            Failure::Unreadable,
        ),
        (
            "a completed reply that does not fit its shape",
            {
                let mut events = open_text.clone();
                events.push(
                    r#"{"type":"response.completed","response":{"usage":{"input_tokens":"many"}}}"#
                        .to_owned(),
                );
                events
            },
            Failure::Unreadable,
        ),
        (
            "a call still open at response.completed",
            events(TOOL_FRAGMENTS, &[])
                .into_iter()
                .filter(|event| !event.contains(".done\""))
                .collect(),
            Failure::Unreadable,
        ),
    ]
}

#[test]
fn failures_never_finish_the_message() {
    for (name, events, failure) in failure_rows() {
        let run = feed(&events);
        assert_eq!(run.end, Some(End::Failed(failure)), "{name}");
        assert!(
            run.last.is_empty(),
            "{name}: the failing step wrote nothing"
        );
        for frame in ["message_stop", "message_delta"] {
            assert!(run.of(frame).is_empty(), "{name}: {frame}");
        }
    }
}

#[test]
fn unknown_types_and_members_change_nothing() {
    let plain = run(PLAIN).bytes;
    let mut events = events(PLAIN, &[]);
    let unknown =
        r#"{"type":"response.something_new","output_index":0,"item":{"type":"web_search_call"}}"#;
    for at in [9, 5, 1] {
        events.insert(at, unknown.to_owned());
    }
    let events: Vec<String> = events
        .iter()
        .map(|e| e.replacen('{', r#"{"later_member":{"nested":[1,{"type":"x"}]},"#, 1))
        .collect();
    let run = feed(&events);
    assert_eq!(run.end, Some(End::Clean));
    assert_eq!(run.bytes, plain);
}

#[test]
fn empty_deltas_write_nothing() {
    let created = CREATED.replace("@MODEL@", &model());
    for event in [
        r#"{"type":"response.output_text.delta","output_index":0,"content_index":0,"delta":""}"#,
        r#"{"type":"response.refusal.delta","output_index":0,"content_index":0,"delta":""}"#,
        r#"{"type":"response.function_call_arguments.delta","output_index":0,"delta":""}"#,
    ] {
        let mut reply = Reply::new();
        assert_eq!(reply.take(created.as_bytes()), Step::default());
        assert_eq!(reply.take(event.as_bytes()), Step::default(), "{event}");
    }
}
