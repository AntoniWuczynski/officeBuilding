//! The translated route through the router's own listener, with a `FakeUpstream` behind it.
//! The `OpenAI` replies here are written from `OpenAI`'s published documentation (U1 to U5),
//! each is cited where it is written, and none was captured from a real account. Every id is
//! invented, and every model name comes from the fixture settings (`@MODEL@` stands for it).

use super::tests::{Got, OPUS, Rig, Who, body_of, canned, oai_text, ok, who};
use crate::routing::answers::{self, Answer};
use crate::routing::availability::Bucket;
use crate::routing::config::ConnectionKind;
use crate::routing::config::TierId;
use crate::routing::evidence::Said;
use crate::routing::fixtures::conn;
use crate::routing::live::Reasoning;
use crate::routing::observe::Observation;
use crate::routing::policy::Skip;
use crate::routing::status::{ProblemKind, RouteStatus};
use crate::routing::translate::Carry;
use crate::routing::upstream::{self, Canned, Sent, Step};
use serde_json::Value;

/// U2, `response.created`.
pub(super) const CREATED: &str = r#"{"type":"response.created","response":{"id":"resp_inv0201","object":"response","status":"in_progress","model":"@MODEL@","output":[],"usage":null},"sequence_number":0}"#;

/// U2: a message item, its `output_text` part and the first text delta.
pub(super) const TEXT_OPEN: [&str; 3] = [
    r#"{"type":"response.output_item.added","output_index":0,"item":{"id":"msg_inv0201","type":"message","status":"in_progress","role":"assistant","content":[]},"sequence_number":1}"#,
    r#"{"type":"response.content_part.added","item_id":"msg_inv0201","output_index":0,"content_index":0,"part":{"type":"output_text","text":"","annotations":[]},"sequence_number":2}"#,
    r#"{"type":"response.output_text.delta","item_id":"msg_inv0201","output_index":0,"content_index":0,"delta":"first words","sequence_number":3}"#,
];

/// U2, `response.completed`.
const COMPLETED: &str = r#"{"type":"response.completed","response":{"id":"resp_inv0201","status":"completed","error":null,"incomplete_details":null,"model":"@MODEL@","usage":{"input_tokens":40,"output_tokens":12}},"sequence_number":20}"#;

/// U3's streaming sample: one call whose arguments arrive in fragments.
const CALL_FRAGMENTS: [&str; 6] = [
    r#"{"type":"response.output_item.added","output_index":0,"item":{"type":"function_call","id":"fc_inv0201","call_id":"call_inv0201","name":"Read","arguments":""},"sequence_number":1}"#,
    r#"{"type":"response.function_call_arguments.delta","item_id":"fc_inv0201","output_index":0,"delta":"{\"pa","sequence_number":2}"#,
    r#"{"type":"response.function_call_arguments.delta","item_id":"fc_inv0201","output_index":0,"delta":"th\":","sequence_number":3}"#,
    r#"{"type":"response.function_call_arguments.delta","item_id":"fc_inv0201","output_index":0,"delta":"\"a.rs\"}","sequence_number":4}"#,
    r#"{"type":"response.function_call_arguments.done","item_id":"fc_inv0201","output_index":0,"arguments":"{\"path\":\"a.rs\"}","sequence_number":5}"#,
    r#"{"type":"response.output_item.done","output_index":0,"item":{"type":"function_call","id":"fc_inv0201","call_id":"call_inv0201","name":"Read","arguments":"{\"path\":\"a.rs\"}"},"sequence_number":6}"#,
];

/// U2's item events and U3's call items: two calls in one reply.
const TWO_CALLS: [&str; 6] = [
    r#"{"type":"response.output_item.added","output_index":0,"item":{"type":"function_call","id":"fc_inv0202","call_id":"call_inv0202","name":"Read","arguments":""},"sequence_number":1}"#,
    r#"{"type":"response.function_call_arguments.delta","item_id":"fc_inv0202","output_index":0,"delta":"{\"path\":\"a.rs\"}","sequence_number":2}"#,
    r#"{"type":"response.output_item.done","output_index":0,"item":{"type":"function_call","id":"fc_inv0202","call_id":"call_inv0202","name":"Read","arguments":"{\"path\":\"a.rs\"}"},"sequence_number":3}"#,
    r#"{"type":"response.output_item.added","output_index":1,"item":{"type":"function_call","id":"fc_inv0203","call_id":"call_inv0203","name":"Read","arguments":""},"sequence_number":4}"#,
    r#"{"type":"response.function_call_arguments.delta","item_id":"fc_inv0203","output_index":1,"delta":"{\"path\":\"b.rs\"}","sequence_number":5}"#,
    r#"{"type":"response.output_item.done","output_index":1,"item":{"type":"function_call","id":"fc_inv0203","call_id":"call_inv0203","name":"Read","arguments":"{\"path\":\"b.rs\"}"},"sequence_number":6}"#,
];

/// U2's reasoning events (a summary and reasoning text) and U7's encrypted content, then text.
const REASONING: [&str; 8] = [
    r#"{"type":"response.output_item.added","output_index":0,"item":{"id":"rs_inv0201","type":"reasoning","summary":[]},"sequence_number":1}"#,
    r#"{"type":"response.reasoning_summary_text.delta","item_id":"rs_inv0201","output_index":0,"summary_index":0,"delta":"hidden-summary","sequence_number":2}"#,
    r#"{"type":"response.content_part.added","item_id":"rs_inv0201","output_index":0,"content_index":0,"part":{"type":"reasoning_text","text":""},"sequence_number":3}"#,
    r#"{"type":"response.reasoning_text.delta","item_id":"rs_inv0201","output_index":0,"content_index":0,"delta":"hidden-reasoning","sequence_number":4}"#,
    r#"{"type":"response.output_item.done","output_index":0,"item":{"id":"rs_inv0201","type":"reasoning","summary":[{"type":"summary_text","text":"hidden-summary"}],"encrypted_content":"aGlkZGVuLWJsb2I="},"sequence_number":5}"#,
    r#"{"type":"response.output_item.added","output_index":1,"item":{"id":"msg_inv0202","type":"message","status":"in_progress","role":"assistant","content":[]},"sequence_number":6}"#,
    r#"{"type":"response.content_part.added","item_id":"msg_inv0202","output_index":1,"content_index":0,"part":{"type":"output_text","text":"","annotations":[]},"sequence_number":7}"#,
    r#"{"type":"response.output_text.delta","item_id":"msg_inv0202","output_index":1,"content_index":0,"delta":"Visible answer","sequence_number":8}"#,
];

/// U4, U5: the `error` event with a used-up code. The message is invented.
pub(super) const USED_UP_EVENT: &str = r#"{"type":"error","code":"credit_balance_exhausted","message":"No credits left.","param":null,"sequence_number":4}"#;

/// U2's `response.failed` example.
pub(super) const FAILED_SERVER: &str = r#"{"type":"response.failed","response":{"id":"resp_inv0201","status":"failed","error":{"code":"server_error","message":"The model failed to generate a response."},"incomplete_details":null,"model":"@MODEL@","usage":null},"sequence_number":4}"#;

/// `response.failed` with a code U2's list does not hold.
const FAILED_UNKNOWN: &str = r#"{"type":"response.failed","response":{"id":"resp_inv0201","status":"failed","error":{"code":"made_up_failure","message":"Something odd."},"incomplete_details":null,"model":"@MODEL@","usage":null},"sequence_number":4}"#;

/// `response.failed` carrying a used-up code, which U2's closed list for `response.error.code`
/// does not hold.
const FAILED_USED_UP: &str = r#"{"type":"response.failed","response":{"id":"resp_inv0201","status":"failed","error":{"code":"credit_balance_exhausted","message":"Invented message."},"incomplete_details":null,"model":"@MODEL@","usage":null},"sequence_number":4}"#;

/// A text delta without the `content_index` U2 gives every one: it cannot be read.
const UNREADABLE: &str = r#"{"type":"response.output_text.delta","item_id":"msg_inv0201","output_index":0,"delta":"x","sequence_number":4}"#;

/// The model and the pair of `high` the fixture maps for `oai`'s Opus tier.
fn opus_target() -> (String, String) {
    let target = crate::routing::fixtures::oai_target();
    let effort = target.efforts.get("high").expect("high is mapped").clone();
    (target.model, effort)
}

/// A 200 Responses stream (U1: `data: <json>`, blank line), one read per event, then `tail`.
pub(super) fn oai_stream(events: &[&str], tail: Vec<Step>) -> Canned {
    let model = opus_target().0;
    let events = events.iter().map(|event| {
        Step::Bytes(format!("data: {}\n\n", event.replace("@MODEL@", &model)).into_bytes())
    });
    Canned {
        status: 200,
        headers: vec![("content-type", "text/event-stream".to_owned())],
        body: events.chain(tail).collect(),
    }
}

/// Text, then the failing `tail` events and steps: a stream that fails after its first byte.
pub(super) fn oai_text_then(tail: &[&str], steps: Vec<Step>) -> Canned {
    oai_stream(&[&[CREATED][..], &TEXT_OPEN, tail].concat(), steps)
}

/// U4: an error reply names `error.message`, `error.type`, `error.param` and `error.code`.
pub(super) fn oai_error(
    status: u16,
    headers: Vec<(&'static str, String)>,
    code: Option<&str>,
    message: &str,
) -> Canned {
    let body = format!(
        r#"{{"error":{{"message":{},"type":"invalid_request_error","param":null,"code":{}}}}}"#,
        serde_json::to_string(message).expect("a string"),
        serde_json::to_string(&code).expect("a string or null"),
    );
    canned(status, headers, &body)
}

pub(super) fn holds(value: &[u8], text: &str) -> bool {
    value.windows(text.len()).any(|w| w == text.as_bytes())
}

/// A Messages body at high effort, with `messages` and `extra` members.
pub(super) fn request(messages: &str, extra: &str) -> String {
    format!(
        r#"{{"model":"{OPUS}","max_tokens":64,"messages":{messages},"output_config":{{"effort":"high"}},"stream":true{extra}}}"#
    )
}

/// A plain body with `context_management`, which the route cannot carry (F6).
pub(super) fn managed(marker: &str) -> String {
    request(
        &format!(r#"[{{"role":"user","content":"{marker}"}}]"#),
        r#","context_management":{"edits":[]}"#,
    )
}

/// The client's stream as (event name, data).
fn frames(got: &Got) -> Vec<(String, Value)> {
    String::from_utf8_lossy(&got.body)
        .split("\n\n")
        .filter(|frame| !frame.is_empty())
        .map(|frame| {
            let mut name = String::new();
            let mut data = Value::Null;
            for line in frame.lines() {
                if let Some(event) = line.strip_prefix("event: ") {
                    event.clone_into(&mut name);
                } else if let Some(json) = line.strip_prefix("data: ") {
                    data = serde_json::from_str(json).expect("json data");
                }
            }
            (name, data)
        })
        .collect()
}

/// Every tool block the client was given: (index, id, name, its joined `partial_json`).
fn tool_blocks(got: &Got) -> Vec<(u64, String, String, String)> {
    let frames = frames(got);
    frames
        .iter()
        .filter(|(name, data)| {
            name == "content_block_start" && data["content_block"]["type"] == "tool_use"
        })
        .map(|(_, start)| {
            let index = start["index"].as_u64().expect("an index");
            let joined: String = frames
                .iter()
                .filter(|(name, data)| {
                    name == "content_block_delta"
                        && data["index"].as_u64() == Some(index)
                        && data["delta"]["type"] == "input_json_delta"
                })
                .map(|(_, data)| data["delta"]["partial_json"].as_str().expect("text"))
                .collect();
            let block = &start["content_block"];
            let text = |key: &str| block[key].as_str().expect("text").to_owned();
            (index, text("id"), text("name"), joined)
        })
        .collect()
}

fn stop_reason(got: &Got) -> Value {
    frames(got)
        .into_iter()
        .find(|(name, _)| name == "message_delta")
        .map(|(_, data)| data["delta"]["stop_reason"].clone())
        .expect("a message_delta")
}

fn upstream_body(sent: &Sent) -> Value {
    serde_json::from_slice(&sent.body).expect("json")
}

/// The one call to `OpenAI` whose body holds `marker`.
fn call_with(rig: &Rig, marker: &str) -> Sent {
    let calls: Vec<Sent> = rig
        .sent()
        .into_iter()
        .filter(|s| who(s) == Who::OpenAi && holds(&s.body, marker))
        .collect();
    assert_eq!(calls.len(), 1, "{marker}: one call");
    calls.into_iter().next().expect("one call")
}

#[test]
fn a_text_reply_round_trips_through_openai() {
    let rig = Rig::new("oai-text", &["oai"], |_| Ok(oai_text("Hello there")));
    let got = rig.ask(&body_of("one"));
    assert_eq!(got.status, 200);
    assert_eq!(got.header("content-type"), Some("text/event-stream"));
    assert_eq!(got.header("x-request-id"), Some("req_inv1"));
    assert!(got.ended, "a whole reply ends with its terminator");
    let text = String::from_utf8_lossy(&got.body);
    assert!(text.starts_with("event: message_start\n"), "{text}");
    assert!(text.ends_with("event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n"));
    assert!(text.contains("Hello there"));

    let sent = rig.sent();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].url, format!("{}/v1/responses", upstream::OPENAI));
    assert_eq!(who(&sent[0]), Who::OpenAi);
    let body = upstream_body(&sent[0]);
    let (model, effort) = opus_target();
    assert_eq!(body["model"], Value::String(model.clone()));
    assert_eq!(body["reasoning"]["effort"], Value::String(effort));
    assert_eq!(body["store"], Value::Bool(false));
    assert_eq!(body["stream"], Value::Bool(true));
    assert!(
        !text.contains(OPUS),
        "the client is told the model that served"
    );
    assert!(text.contains(&model));

    let view = rig.view();
    assert_eq!(view.status.active, Some(conn("oai")));
    assert_eq!(view.reasoning, Reasoning::Removed);
    let served = view.status.last_served.expect("served");
    assert_eq!(served.model, Some(model));
    rig.cleanup();
}

#[test]
fn a_request_the_route_cannot_carry_reaches_no_upstream() {
    let rig = Rig::new("oai-carry", &["oai"], |_| Ok(oai_text("ok")));
    assert_eq!(rig.ask(&body_of("first")).status, 200);
    let before = rig.view().status;
    let got = rig.ask(&managed("refused"));
    let expected = answers::cannot_carry("oai", &Carry::ContextManagement);
    assert_eq!((got.status, got.body), (400, expected.body()));
    assert_eq!(rig.sent().len(), 1, "no call for a request it cannot carry");
    let after = rig.view().status;
    assert_eq!(
        (after.active, after.generation),
        (before.active, before.generation)
    );
    assert_eq!(rig.ask(&body_of("next")).status, 200);
    assert_eq!(rig.calls(Who::OpenAi), 2);
    rig.cleanup();
}

const TOOLS: &str = r#","tools":[{"name":"Read","description":"Reads a file.","input_schema":{"type":"object","properties":{"path":{"type":"string"}}}}]"#;

// Group 6.
#[test]
fn tool_calls_and_images_round_trip() {
    let rig = Rig::new("oai-round-trips", &["oai"], |sent| {
        let completed =
            |events: &[&str]| oai_stream(&[&[CREATED][..], events, &[COMPLETED]].concat(), vec![]);
        Ok(
            if holds(&sent.body, "call_inv0201") || holds(&sent.body, "look") {
                oai_text("done")
            } else if holds(&sent.body, "both") {
                completed(&TWO_CALLS)
            } else {
                completed(&CALL_FRAGMENTS)
            },
        )
    });

    let asked = r#"{"role":"user","content":"read it"}"#;
    let got = rig.ask(&request(&format!("[{asked}]"), TOOLS));
    assert_eq!((got.status, got.ended), (200, true));
    let blocks = tool_blocks(&got);
    assert_eq!(blocks.len(), 1);
    let (_, id, name, arguments) = blocks[0].clone();
    assert_eq!((id.as_str(), name.as_str()), ("call_inv0201", "Read"));
    assert_eq!(arguments, r#"{"path":"a.rs"}"#, "the fragments join whole");
    assert_eq!(stop_reason(&got), "tool_use");

    let history = format!(
        r#"[{asked},{{"role":"assistant","content":[{{"type":"tool_use","id":"{id}","name":"{name}","input":{arguments}}}]}},{{"role":"user","content":[{{"type":"tool_result","tool_use_id":"{id}","content":"fn main() {{}}"}}]}}]"#
    );
    assert_eq!(rig.ask(&request(&history, TOOLS)).status, 200);
    let input = upstream_body(&call_with(&rig, "call_inv0201"))["input"].clone();
    let item = |kind: &str| {
        input
            .as_array()
            .expect("a list")
            .iter()
            .find(|item| item["type"] == kind)
            .cloned()
            .expect(kind)
    };
    let call = item("function_call");
    assert_eq!(call["call_id"], Value::String(id.clone()));
    assert_eq!(call["name"], Value::String(name));
    assert_eq!(call["arguments"], Value::String(arguments));
    let output = item("function_call_output");
    assert_eq!(output["call_id"], Value::String(id));
    assert_eq!(output["output"], "fn main() {}");

    let got = rig.ask(&request(
        r#"[{"role":"user","content":"read both"}]"#,
        TOOLS,
    ));
    let blocks: Vec<(String, String)> = tool_blocks(&got)
        .into_iter()
        .map(|(_, id, _, arguments)| (id, arguments))
        .collect();
    assert_eq!(
        blocks,
        [
            ("call_inv0202".to_owned(), r#"{"path":"a.rs"}"#.to_owned()),
            ("call_inv0203".to_owned(), r#"{"path":"b.rs"}"#.to_owned()),
        ]
    );
    let indexes: Vec<u64> = tool_blocks(&got).iter().map(|block| block.0).collect();
    assert_ne!(indexes[0], indexes[1], "each call is its own block");
    assert_eq!(stop_reason(&got), "tool_use");

    let image = r#"[{"role":"user","content":[{"type":"text","text":"look"},{"type":"image","source":{"type":"base64","media_type":"image/png","data":"iVBORw0KGgo="}}]}]"#;
    assert_eq!(rig.ask(&request(image, "")).status, 200);
    let sent = call_with(&rig, "look");
    assert!(holds(
        &sent.body,
        r#""image_url":"data:image/png;base64,iVBORw0KGgo=""#
    ));
    assert!(holds(&sent.body, r#""type":"input_image""#));
    rig.cleanup();
}

// Group 7.
#[test]
fn hidden_reasoning_is_dropped_and_reported() {
    let rig = Rig::new("oai-reasoning", &["oai"], |_| {
        Ok(oai_stream(
            &[&[CREATED][..], &REASONING, &[COMPLETED]].concat(),
            vec![],
        ))
    });
    let messages = r#"[{"role":"user","content":"think"},{"role":"assistant","content":[{"type":"thinking","thinking":"secret-thought","signature":"c2lnbmF0dXJl"},{"type":"redacted_thinking","data":"cmVkYWN0ZWQ="},{"type":"text","text":"said"}]},{"role":"user","content":"again"}]"#;
    let thinking = r#","thinking":{"type":"enabled","budget_tokens":1024}"#;
    let got = rig.ask(&request(messages, thinking));
    assert_eq!((got.status, got.ended), (200, true));
    let sent = rig.sent();
    assert_eq!(sent.len(), 1);
    for gone in [
        "secret-thought",
        "c2lnbmF0dXJl",
        "cmVkYWN0ZWQ=",
        "budget_tokens",
        "thinking",
    ] {
        assert!(!holds(&sent[0].body, gone), "{gone} reached OpenAI");
    }
    assert!(holds(&sent[0].body, "said"), "the visible text is carried");
    for gone in [
        "hidden-summary",
        "hidden-reasoning",
        "aGlkZGVuLWJsb2I=",
        "thinking",
    ] {
        assert!(!holds(&got.body, gone), "{gone} reached the client");
    }
    let started: Vec<Value> = frames(&got)
        .into_iter()
        .filter(|(name, _)| name == "content_block_start")
        .map(|(_, data)| data["content_block"]["type"].clone())
        .collect();
    assert_eq!(started, ["text"]);
    assert!(holds(&got.body, "Visible answer"));
    assert_eq!(rig.view().reasoning, Reasoning::Removed);
    rig.cleanup();
}

/// What a refused request comes to on the order `oai`, `api`.
#[derive(Clone, Copy)]
enum Out {
    /// A proven used-up allowance: recorded for the whole connection, one move, `api` serves.
    Moved,
    /// Answered here with this answer, nothing moved, and the entry shows the problem.
    Answered(fn() -> Answer, ProblemKind),
}

/// A refusal before any translated byte: the name, the reply, the `OpenAI` calls it costs.
type Row = (&'static str, fn() -> Canned, usize, Out);

const MSG: &str = "Invented message.";

fn said(code: &str) -> Said {
    Said::new(Some(code), Some(MSG))
}

fn used_up(code: &'static str) -> Canned {
    oai_error(429, vec![], Some(code), MSG)
}

/// Group 8: each documented error, as a reply that is not 2xx (U4, U5, U6).
const NOT_2XX: [Row; 12] = [
    (
        "credit_balance_exhausted",
        || used_up("credit_balance_exhausted"),
        1,
        Out::Moved,
    ),
    (
        "organization_spend_limit_exceeded",
        || used_up("organization_spend_limit_exceeded"),
        1,
        Out::Moved,
    ),
    (
        "project_spend_limit_exceeded",
        || used_up("project_spend_limit_exceeded"),
        1,
        Out::Moved,
    ),
    (
        "organization_usage_limit_exceeded",
        || used_up("organization_usage_limit_exceeded"),
        1,
        Out::Moved,
    ),
    (
        "429 slow_down",
        || oai_error(429, vec![], Some("slow_down"), MSG),
        2,
        Out::Answered(
            || {
                answers::provider_refused(
                    "oai",
                    Some(429),
                    &Observation::Throttled { retry_after: None },
                    &said("slow_down"),
                )
            },
            ProblemKind::Throttled,
        ),
    ),
    (
        "429 with Retry-After 6",
        || {
            oai_error(
                429,
                vec![("retry-after", "6".to_owned())],
                None,
                "Rate limit reached for requests",
            )
        },
        1,
        Out::Answered(
            || {
                let said = Said::new(None, Some("Rate limit reached for requests"));
                answers::provider_refused(
                    "oai",
                    Some(429),
                    &Observation::Throttled {
                        retry_after: Some(6),
                    },
                    &said,
                )
            },
            ProblemKind::Throttled,
        ),
    ),
    (
        "503 server_is_overloaded",
        || oai_error(503, vec![], Some("server_is_overloaded"), MSG),
        2,
        Out::Answered(
            || {
                answers::provider_refused(
                    "oai",
                    Some(503),
                    &Observation::Faulted { retry_after: None },
                    &said("server_is_overloaded"),
                )
            },
            ProblemKind::Faulted,
        ),
    ),
    (
        "401",
        || oai_error(401, vec![], Some("invalid_api_key"), MSG),
        1,
        Out::Answered(
            || {
                answers::provider_refused(
                    "oai",
                    Some(401),
                    &Observation::Unauthorised,
                    &said("invalid_api_key"),
                )
            },
            ProblemKind::Unauthorised,
        ),
    ),
    (
        "403",
        || oai_error(403, vec![], None, MSG),
        1,
        Out::Answered(
            || {
                answers::provider_refused(
                    "oai",
                    Some(403),
                    &Observation::Forbidden,
                    &Said::new(None, Some(MSG)),
                )
            },
            ProblemKind::Forbidden,
        ),
    ),
    (
        "400 with a message on two lines",
        || {
            oai_error(
                400,
                vec![],
                Some("invalid_value"),
                "Bad tool name.\nSecond line.",
            )
        },
        1,
        Out::Answered(
            || {
                let said = Said::new(Some("invalid_value"), Some("Bad tool name. Second line."));
                answers::provider_refused("oai", Some(400), &Observation::Rejected, &said)
            },
            ProblemKind::Rejected,
        ),
    ),
    (
        "429 with a code nobody knows",
        || oai_error(429, vec![], Some("mystery_code"), MSG),
        2,
        Out::Answered(
            || {
                answers::provider_refused(
                    "oai",
                    Some(429),
                    &Observation::Throttled { retry_after: None },
                    &said("mystery_code"),
                )
            },
            ProblemKind::Throttled,
        ),
    ),
    (
        "403 with a used-up code",
        || oai_error(403, vec![], Some("credit_balance_exhausted"), MSG),
        1,
        Out::Answered(
            || {
                answers::provider_refused(
                    "oai",
                    Some(403),
                    &Observation::Forbidden,
                    &said("credit_balance_exhausted"),
                )
            },
            ProblemKind::Forbidden,
        ),
    ),
];

/// Group 9, before the first relayed byte: a stream that fails inside a 200 (U2, U5).
const BEFORE_COMMIT: [Row; 7] = [
    (
        "an error event with a used-up code",
        || oai_stream(&[CREATED, USED_UP_EVENT], vec![]),
        1,
        Out::Moved,
    ),
    (
        "a used-up code after the content part and before any text",
        || {
            oai_stream(
                &[CREATED, TEXT_OPEN[0], TEXT_OPEN[1], USED_UP_EVENT],
                vec![],
            )
        },
        1,
        Out::Moved,
    ),
    (
        "server_error after the content part and before any text",
        || {
            oai_stream(
                &[CREATED, TEXT_OPEN[0], TEXT_OPEN[1], FAILED_SERVER],
                vec![],
            )
        },
        2,
        Out::Answered(
            || {
                let said = Said::new(
                    Some("server_error"),
                    Some("The model failed to generate a response."),
                );
                answers::provider_refused(
                    "oai",
                    None,
                    &Observation::Faulted { retry_after: None },
                    &said,
                )
            },
            ProblemKind::Faulted,
        ),
    ),
    (
        "response.failed with server_error",
        || oai_stream(&[CREATED, FAILED_SERVER], vec![]),
        2,
        Out::Answered(
            || {
                let said = Said::new(
                    Some("server_error"),
                    Some("The model failed to generate a response."),
                );
                answers::provider_refused(
                    "oai",
                    None,
                    &Observation::Faulted { retry_after: None },
                    &said,
                )
            },
            ProblemKind::Faulted,
        ),
    ),
    (
        "response.failed with a code nobody knows",
        || oai_stream(&[CREATED, FAILED_UNKNOWN], vec![]),
        1,
        Out::Answered(
            || {
                let said = Said::new(Some("made_up_failure"), Some("Something odd."));
                answers::provider_refused("oai", None, &Observation::Rejected, &said)
            },
            ProblemKind::Rejected,
        ),
    ),
    (
        "response.failed with a used-up code, which proves nothing (F14)",
        || oai_stream(&[CREATED, FAILED_USED_UP], vec![]),
        1,
        Out::Answered(
            || {
                let said = Said::new(Some("credit_balance_exhausted"), Some(MSG));
                answers::provider_refused("oai", None, &Observation::Rejected, &said)
            },
            ProblemKind::Rejected,
        ),
    ),
    (
        "a body that breaks",
        || oai_stream(&[CREATED], vec![Step::Break]),
        1,
        Out::Answered(|| answers::interrupted("oai"), ProblemKind::Interrupted),
    ),
];

/// One plain request served by `oai`, then the refused one. The status before, and its reply.
fn refuse(name: &str, reply: fn() -> Canned) -> (Rig, RouteStatus, Got) {
    let rig = Rig::new(name, &["oai", "api"], move |sent| {
        Ok(match who(sent) {
            Who::OpenAi if holds(&sent.body, "first") => oai_text("ok"),
            Who::OpenAi => reply(),
            Who::Sub | Who::Api | Who::Router => ok(),
        })
    });
    assert_eq!(rig.ask(&body_of("first")).status, 200, "{name}");
    let before = rig.view().status;
    let got = rig.ask(&body_of("refused"));
    (rig, before, got)
}

fn openai_calls(rig: &Rig, marker: &str) -> usize {
    rig.sent()
        .iter()
        .filter(|s| who(s) == Who::OpenAi && holds(&s.body, marker))
        .count()
}

fn check_refusals(prefix: &str, rows: &[Row]) {
    for (i, (name, reply, calls, out)) in rows.iter().enumerate() {
        let (rig, before, got) = refuse(&format!("{prefix}-{i}"), *reply);
        assert_eq!(openai_calls(&rig, "refused"), *calls, "{name}: calls");
        let status = rig.view().status;
        match out {
            Out::Moved => {
                assert_eq!(
                    (got.status, got.body.as_slice()),
                    (200, &b"event: message_stop\n\n"[..]),
                    "{name}"
                );
                assert_eq!(status.generation, before.generation + 1, "{name}");
                assert_eq!(status.active, Some(conn("api")), "{name}");
                let records = rig.records();
                assert_eq!(records.len(), 1, "{name}");
                assert_eq!(
                    (&records[0].connection, &records[0].bucket),
                    (&conn("oai"), &Bucket::Whole),
                    "{name}"
                );
            }
            Out::Answered(answer, problem) => {
                let answer = answer();
                assert_eq!(
                    (got.status, &got.body),
                    (answer.status, &answer.body()),
                    "{name}"
                );
                let retry_after = answer.retry_after.map(|secs| secs.to_string());
                assert_eq!(got.header("retry-after"), retry_after.as_deref(), "{name}");
                assert!(rig.records().is_empty(), "{name}: a record");
                assert_eq!(
                    (&status.active, status.generation),
                    (&before.active, before.generation),
                    "{name}"
                );
                let entry = status
                    .entries
                    .iter()
                    .find(|e| e.connection == conn("oai"))
                    .expect("oai");
                assert_eq!(
                    entry.problem.as_ref().map(|p| p.kind),
                    Some(*problem),
                    "{name}"
                );
                assert_eq!(rig.calls(Who::Api), 0, "{name}");
                assert!(
                    !holds(&got.body, "\"param\"") && !holds(&got.body, "\\n"),
                    "{name}"
                );
            }
        }
        rig.cleanup();
    }
}

// Group 8.
#[test]
fn each_documented_error_maps_to_its_observation() {
    check_refusals("oai-errors", &NOT_2XX);
}

/// Group 9, after the first relayed byte: the name, what follows the text, and whether it proves
/// a used-up allowance.
type AfterRow = (&'static str, fn() -> Canned, bool);

const AFTER_COMMIT: [AfterRow; 4] = [
    (
        "an error event with a used-up code",
        || oai_text_then(&[USED_UP_EVENT], vec![]),
        true,
    ),
    (
        "response.failed",
        || oai_text_then(&[FAILED_SERVER], vec![]),
        false,
    ),
    ("a break", || oai_text_then(&[], vec![Step::Break]), false),
    (
        "an event that cannot be read",
        || oai_text_then(&[UNREADABLE], vec![]),
        false,
    ),
];

/// No `message_delta`, no `message_stop`, no terminator: nothing the client takes for a whole reply.
fn assert_not_whole(name: &str, got: &Got) {
    let names: Vec<String> = frames(got).into_iter().map(|(name, _)| name).collect();
    assert!(!got.ended, "{name}: a terminator");
    for clean in ["message_delta", "message_stop"] {
        assert!(!names.iter().any(|n| n == clean), "{name}: {clean}");
    }
}

// Group 9.
#[test]
fn a_stream_may_move_the_session_only_before_its_first_relayed_byte() {
    check_refusals("oai-before", &BEFORE_COMMIT);
    for (i, (name, reply, proves)) in AFTER_COMMIT.iter().enumerate() {
        let (rig, before, got) = refuse(&format!("oai-after-{i}"), *reply);
        assert_eq!(got.status, 200, "{name}");
        assert!(holds(&got.body, "first words"), "{name}");
        assert_not_whole(name, &got);
        let last = frames(&got).pop().expect("frames");
        assert_eq!(last.0, "error", "{name}");
        assert_eq!(last.1["error"]["type"], "api_error", "{name}");
        assert_eq!(openai_calls(&rig, "refused"), 1, "{name}");
        assert_eq!(rig.calls(Who::Api), 0, "{name}");
        let status = rig.view().status;
        assert_eq!(
            (&status.active, status.generation),
            (&before.active, before.generation),
            "{name}"
        );
        assert_eq!(!rig.records().is_empty(), *proves, "{name}: the record");
        if *proves {
            assert_eq!(rig.ask(&body_of("next")).status, 200, "{name}");
            let status = rig.view().status;
            assert_eq!(
                (status.active, status.generation),
                (Some(conn("api")), before.generation + 1),
                "{name}"
            );
        }
        rig.cleanup();
    }
}

// Group 10.
#[test]
fn a_failing_reply_never_ends_clean() {
    let failing = NOT_2XX
        .iter()
        .chain(&BEFORE_COMMIT)
        .filter(|row| matches!(row.3, Out::Answered(..)))
        .map(|row| (row.0, row.1))
        .chain(AFTER_COMMIT.iter().map(|row| (row.0, row.1)));
    let mut seen = 0;
    for (i, (name, reply)) in failing.enumerate() {
        let (rig, _, got) = refuse(&format!("oai-clean-{i}"), reply);
        if got.status == 200 {
            assert_not_whole(name, &got);
        }
        seen += 1;
        rig.cleanup();
    }
    assert_eq!(seen, 8 + 5 + 4);
}

// Group 11.
#[test]
fn the_effort_in_force_for_the_reply_is_sent() {
    let rig = Rig::new("oai-effort", &["oai"], |_| Ok(oai_text("ok")));
    let entry = |effort: &str| {
        format!(
            r#"[{{"role":"user","content":"effort {effort}"}},{{"role":"system","content":[],"output_config":{{"effort":"{effort}"}}}}]"#
        )
    };
    for (effort, sent) in [("low", "low"), ("xhigh", "high"), ("medium", "medium")] {
        assert_eq!(
            rig.ask(&request(&entry(effort), "")).status,
            200,
            "{effort}"
        );
        let body = upstream_body(&call_with(&rig, &format!("effort {effort}")));
        assert_eq!(body["reasoning"]["effort"], sent, "{effort}");
    }
    let calls = rig.sent().len();
    let got = rig.ask(&request(&entry("max"), ""));
    let skip = Skip::EffortUnmapped {
        tier: TierId::Opus,
        effort: "max".to_owned(),
    };
    let expected = answers::refused("oai", ConnectionKind::OpenAiApiKey, &skip);
    assert_eq!((got.status, got.body), (400, expected.body()));
    assert_eq!(
        rig.sent().len(),
        calls,
        "no call for an effort with no pair"
    );
    rig.cleanup();
}
