use super::{
    Answer, blocked, busy, cannot_carry, interrupted, no_key, other_path, provider_refused,
    redirect, refused, skip_sentence, stream_stopped, transport, unknown_desk, unreachable,
    unreadable,
};
use crate::routing::capabilities::Capability;
use crate::routing::config::{ConnectionKind, TierId};
use crate::routing::evidence::Said;
use crate::routing::observe::{Breadth, Exhaustion, Observation};
use crate::routing::policy::Skip;
use crate::routing::translate::Carry;
use crate::routing::wire::Refusal;
use serde::Deserialize;

#[derive(Deserialize, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Envelope {
    #[serde(rename = "type")]
    kind: String,
    error: Inner,
}

#[derive(Deserialize, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Inner {
    #[serde(rename = "type")]
    kind: String,
    message: String,
}

fn parsed(answer: &Answer) -> Envelope {
    serde_json::from_slice(&answer.body()).unwrap()
}

const CALM: &str = "Calm Sub";

/// Every constructor, with what it must give.
#[expect(
    clippy::too_many_lines,
    reason = "one row per constructor, with its whole sentence"
)]
fn every() -> Vec<(&'static str, Answer, u16, &'static str, String)> {
    let exhausted = Skip::Exhausted;
    let tried = Skip::Tried;
    let invalid = "invalid_request_error";
    let mut rows = vec![
        (
            "unknown_desk",
            unknown_desk(),
            400,
            invalid,
            "Office Building did not recognise this desk, so nothing was sent. Start the desk again from the building. If your shell profile sets ANTHROPIC_CUSTOM_HEADERS, it replaces the header the building relies on.".to_owned(),
        ),
        (
            "unreadable",
            unreadable(),
            400,
            invalid,
            "Office Building could not read this request as a Messages request, so nothing was sent.".to_owned(),
        ),
        (
            "other_path",
            other_path(),
            404,
            "not_found_error",
            "Office Building's router carries the Messages call only. This request was answered here and was not sent on.".to_owned(),
        ),
        (
            "refused",
            refused(CALM, ConnectionKind::ClaudeSubscription, &exhausted),
            400,
            invalid,
            "Office Building: Calm Sub cannot take this request, because its allowance is used up. Nothing was sent, and the desk stays on Calm Sub.".to_owned(),
        ),
        (
            "blocked",
            blocked(&[
                (CALM, ConnectionKind::ClaudeSubscription, &exhausted),
                ("Work key", ConnectionKind::AnthropicApiKey, &tried),
            ]),
            400,
            invalid,
            "Office Building: no connection in this desk's order can take this request, so nothing was sent. Calm Sub: its allowance is used up. Work key: this request already tried it. Open the desk in Office Building to retry a connection or switch to another.".to_owned(),
        ),
        (
            "blocked with no entries",
            blocked(&[]),
            400,
            invalid,
            "Office Building: no connection in this desk's order can take this request, so nothing was sent. Open the desk in Office Building to retry a connection or switch to another.".to_owned(),
        ),
        (
            "unreachable",
            unreachable(CALM),
            502,
            "api_error",
            "Office Building could not reach Calm Sub. Nothing was sent.".to_owned(),
        ),
        (
            "no_key",
            no_key(CALM),
            502,
            "api_error",
            "Office Building could not read the API key of Calm Sub from the Keychain, so nothing was sent.".to_owned(),
        ),
        (
            "interrupted",
            interrupted(CALM),
            502,
            "api_error",
            "The connection to Calm Sub broke after the request was sent and before any reply came back. Office Building did not send it again.".to_owned(),
        ),
        (
            "redirect",
            redirect(CALM),
            502,
            "api_error",
            "Calm Sub answered with a redirect. Office Building does not follow one and does not pass one on.".to_owned(),
        ),
        (
            "busy",
            busy(),
            503,
            "overloaded_error",
            "Office Building's router is carrying as many requests as it allows at once. Try again in a moment.".to_owned(),
        ),
    ];
    let transports = [
        (
            Refusal::Malformed,
            400,
            invalid,
            "Office Building's router could not read this request.",
        ),
        (
            Refusal::HeadTooLarge,
            431,
            invalid,
            "This request's headers are larger than Office Building's router carries.",
        ),
        (
            Refusal::Version,
            505,
            invalid,
            "Office Building's router speaks HTTP/1.1 only.",
        ),
        (
            Refusal::Origin,
            403,
            "permission_error",
            "Office Building's router answers the building's own desks only.",
        ),
        (
            Refusal::Host,
            403,
            "permission_error",
            "Office Building's router answers the building's own desks only.",
        ),
        (
            Refusal::LengthRequired,
            411,
            invalid,
            "Office Building's router needs a Content-Length.",
        ),
        (
            Refusal::Encoding,
            415,
            invalid,
            "Office Building's router does not take an encoded request body.",
        ),
        (
            Refusal::TooLarge,
            413,
            "request_too_large",
            "This request is larger than 64 MiB, the most Office Building's router carries.",
        ),
    ];
    for (refusal, status, kind, message) in transports {
        rows.push((
            "transport",
            transport(refusal).unwrap(),
            status,
            kind,
            message.to_owned(),
        ));
    }
    for (carry, reason) in carries() {
        rows.push((
            "cannot_carry",
            cannot_carry("oai", &carry),
            400,
            invalid,
            format!(
                "Office Building: oai cannot take this request, because {reason}. Nothing was sent, and the desk stays on oai."
            ),
        ));
    }
    rows.push((
        "stream_stopped",
        stream_stopped("oai", &Said::default()),
        502,
        "api_error",
        "The reply from oai stopped before its end. Office Building did not ask again.".to_owned(),
    ));
    rows.push((
        "stream_stopped with a code",
        stream_stopped("oai", &said(Some("server_error"), Some("not shown"))),
        502,
        "api_error",
        "The reply from oai stopped before its end (server_error). Office Building did not ask again.".to_owned(),
    ));
    rows
}

/// Every `Carry`, with its reason.
fn carries() -> Vec<(Carry, &'static str)> {
    vec![
        (
            Carry::NotStreamed,
            "this route answers as a stream only, and the request did not ask for one",
        ),
        (
            Carry::Prewarm,
            "it only asks Anthropic to warm its prompt cache (max_tokens 0), which this route has no counterpart for",
        ),
        (
            Carry::ContextManagement,
            "this route cannot carry context management (context_management: Extra inputs are not permitted)",
        ),
        (
            Carry::StopSequences,
            "this route cannot carry stop sequences",
        ),
        (
            Carry::Member {
                name: "temperature".to_owned(),
            },
            "this route does not carry the request setting temperature",
        ),
        (
            Carry::OutputSetting,
            "this route carries no output setting but the effort",
        ),
        (
            Carry::Block {
                kind: "document".to_owned(),
            },
            "this route cannot carry a document block",
        ),
        (
            Carry::ImageSource,
            "this route carries an image only as base64 data or a web address",
        ),
        (
            Carry::Tool,
            "this route carries plain function tools only: no strict schema, no deferred loading and no tool the provider runs",
        ),
        (
            Carry::ToolChoice,
            "this route does not know that tool choice",
        ),
        (
            Carry::CallId,
            "a tool call in the conversation has an id OpenAI does not take (1 to 64 characters)",
        ),
        (
            Carry::Unknown,
            "it holds something the translation does not know, and nothing is sent that cannot be carried whole",
        ),
    ]
}

fn said(code: Option<&str>, message: Option<&str>) -> Said {
    Said {
        code: code.map(str::to_owned),
        message: message.map(str::to_owned),
    }
}

#[test]
fn the_context_management_refusal_has_the_words_claude_code_looks_for() {
    let answer = cannot_carry("oai", &Carry::ContextManagement);
    assert_eq!(answer.status, 400);
    assert!(
        answer
            .message
            .contains("context_management: Extra inputs are not permitted")
    );
}

/// Name, the status, what was seen, what the provider said, and the answer's status, type,
/// message and `retry_after`.
type ProviderRow = (
    &'static str,
    Option<u16>,
    Observation,
    Said,
    u16,
    &'static str,
    &'static str,
    Option<u64>,
);

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "one row per observation and shape, with its whole sentence"
)]
fn provider_refused_table() {
    let used_up = Observation::Exhausted(Exhaustion {
        breadth: Breadth::Whole,
        reset_at: None,
        window: None,
    });
    let invalid = "invalid_request_error";
    let full = || said(Some("some_code"), Some("Model x does not exist."));
    let rows: Vec<ProviderRow> = vec![
        (
            "used up, with a code",
            Some(429),
            used_up.clone(),
            said(Some("credit_balance_exhausted"), Some("not shown")),
            400,
            invalid,
            "Office Building: oai reports that its allowance is used up (credit_balance_exhausted). Nothing was passed on. Open the desk in Office Building to retry a connection or switch to another.",
            None,
        ),
        (
            "used up in a stream, nothing said",
            None,
            used_up,
            Said::default(),
            400,
            invalid,
            "Office Building: oai reports that its allowance is used up. Nothing was passed on. Open the desk in Office Building to retry a connection or switch to another.",
            None,
        ),
        (
            "throttled with a wait",
            Some(429),
            Observation::Throttled {
                retry_after: Some(6),
            },
            said(Some("slow_down"), Some("not shown")),
            429,
            "rate_limit_error",
            "oai is limiting requests for now (slow_down). Office Building passed nothing on.",
            Some(6),
        ),
        (
            "throttled in a stream",
            None,
            Observation::Throttled { retry_after: None },
            Said::default(),
            429,
            "rate_limit_error",
            "oai is limiting requests for now. Office Building passed nothing on.",
            None,
        ),
        (
            "faulted 503",
            Some(503),
            Observation::Faulted {
                retry_after: Some(2),
            },
            said(Some("server_is_overloaded"), None),
            503,
            "api_error",
            "oai failed or is overloaded (server_is_overloaded). Office Building passed nothing on.",
            Some(2),
        ),
        (
            "faulted in a stream",
            None,
            Observation::Faulted { retry_after: None },
            Said::default(),
            500,
            "api_error",
            "oai failed or is overloaded. Office Building passed nothing on.",
            None,
        ),
        (
            "faulted 529 is never passed on as 529",
            Some(529),
            Observation::Faulted { retry_after: None },
            Said::default(),
            500,
            "api_error",
            "oai failed or is overloaded. Office Building passed nothing on.",
            None,
        ),
        (
            "unauthorised",
            Some(401),
            Observation::Unauthorised,
            full(),
            400,
            invalid,
            "Office Building: oai refused the API key saved for it (HTTP 401). Check the key in Settings, under Routing. Nothing was passed on.",
            None,
        ),
        (
            "forbidden with everything",
            Some(403),
            Observation::Forbidden,
            full(),
            400,
            invalid,
            "Office Building: oai says the account may not do this (HTTP 403, some_code): Model x does not exist.. Nothing was passed on.",
            None,
        ),
        (
            "forbidden with nothing",
            Some(403),
            Observation::Forbidden,
            Said::default(),
            400,
            invalid,
            "Office Building: oai says the account may not do this (HTTP 403). Nothing was passed on.",
            None,
        ),
        (
            "rejected with everything",
            Some(400),
            Observation::Rejected,
            full(),
            400,
            invalid,
            "Office Building: oai refused this request (HTTP 400, some_code): Model x does not exist.. Nothing was passed on, and the desk stays on oai.",
            None,
        ),
        (
            "rejected in a stream, a code only",
            None,
            Observation::Rejected,
            said(Some("invalid_prompt"), None),
            400,
            invalid,
            "Office Building: oai refused this request (in its reply, invalid_prompt). Nothing was passed on, and the desk stays on oai.",
            None,
        ),
        (
            "rejected, a message only",
            Some(404),
            Observation::Rejected,
            said(None, Some("No such thing")),
            400,
            invalid,
            "Office Building: oai refused this request (HTTP 404): No such thing. Nothing was passed on, and the desk stays on oai.",
            None,
        ),
    ];
    let dropped = "oai sent a reply Office Building could not read as a Responses stream, so nothing was passed on. It was not asked again.";
    let unreadable = [
        Observation::Dropped,
        Observation::Completed,
        Observation::NotSent,
        Observation::Cancelled,
    ]
    .into_iter()
    .map(|seen| {
        (
            "unreadable",
            None,
            seen,
            full(),
            502,
            "api_error",
            dropped,
            None,
        )
    });
    for (name, status, seen, said, code, kind, message, retry_after) in
        rows.into_iter().chain(unreadable)
    {
        let answer = provider_refused("oai", status, &seen, &said);
        assert_eq!(
            (answer.status, answer.kind, answer.message.as_str()),
            (code, kind, message),
            "{name} {seen:?}"
        );
        assert_eq!(answer.retry_after, retry_after, "{name}");
        assert_eq!(parsed(&answer).error.message, message, "{name}");
        assert_ne!(answer.status, 529, "{name}");
        if answer.status == 429 {
            assert!(
                matches!(seen, Observation::Throttled { .. }),
                "{name}: a 429 only for a provider's own throttle"
            );
        }
    }
}

#[test]
fn a_quote_in_a_name_or_a_provider_message_survives_the_round_trip() {
    let name = "Zoë's \"work\" key";
    let message = r#"Tool "x\y" is not allowed."#;
    let answer = provider_refused(
        name,
        Some(400),
        &Observation::Rejected,
        &said(Some("bad_tool"), Some(message)),
    );
    assert_eq!(
        parsed(&answer).error.message,
        format!(
            "Office Building: {name} refused this request (HTTP 400, bad_tool): {message}. Nothing was passed on, and the desk stays on {name}."
        )
    );
    let answer = cannot_carry(name, &Carry::Tool);
    assert!(parsed(&answer).error.message.contains(name));
}

#[test]
fn every_constructor_gives_its_status_type_and_words() {
    for (name, answer, status, kind, message) in every() {
        assert_eq!(answer.status, status, "{name}");
        assert_eq!(answer.kind, kind, "{name}");
        assert_eq!(answer.message, message, "{name}");
        assert_eq!(
            parsed(&answer),
            Envelope {
                kind: "error".to_owned(),
                error: Inner {
                    kind: kind.to_owned(),
                    message,
                },
            },
            "{name}"
        );
    }
}

#[test]
fn an_unadmitted_head_gets_the_unknown_desk_answer() {
    assert_eq!(transport(Refusal::Unadmitted), Some(unknown_desk()));
}

#[test]
fn gone_has_no_answer() {
    assert_eq!(transport(Refusal::Gone), None);
}

#[test]
fn no_answer_is_a_429_or_529_and_only_busy_says_when_to_retry() {
    for (name, answer, ..) in every() {
        assert!(![429, 529].contains(&answer.status), "{name}");
        let expected = (name == "busy").then_some(1);
        assert_eq!(answer.retry_after, expected, "{name}");
    }
}

#[test]
fn an_odd_name_survives_the_round_trip() {
    let name = "Zoë's \"work\" key \\ two";
    let answer = unreachable(name);
    assert_eq!(
        parsed(&answer).error.message,
        format!("Office Building could not reach {name}. Nothing was sent.")
    );
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "one row per skip, with both sentences written out"
)]
fn skip_sentence_table() {
    let model = "claude-invented-9".to_owned();
    // The relayed kinds, then Sign in with ChatGPT, which this build cannot use.
    let rows: Vec<(Skip, &str, &str)> = vec![
        (
            Skip::Removed,
            "it is no longer in Settings",
            "it is no longer in Settings",
        ),
        (
            Skip::PaidNotAllowed,
            "it is billed for each request and may not spend money (Settings, Routing, \"May spend money\")",
            "it is billed for each request and may not spend money (Settings, Routing, \"May spend money\")",
        ),
        (
            Skip::ExperimentalNotAllowed,
            "it is an experimental route that is not switched on",
            "it is an experimental route that is not switched on",
        ),
        (
            Skip::NoCredential,
            "no API key is saved for it",
            "it needs Sign in with ChatGPT, which this build does not have yet",
        ),
        (
            Skip::UnmappedModel { model },
            "the model claude-invented-9 is not listed under any tier in Settings",
            "the model claude-invented-9 is not listed under any tier in Settings",
        ),
        (
            Skip::NoTarget {
                tier: TierId::Fable,
            },
            "it has no model for the Fable tier",
            "it has no model for the Fable tier",
        ),
        (
            Skip::NoTarget {
                tier: TierId::Haiku,
            },
            "it has no model for the Haiku tier",
            "it has no model for the Haiku tier",
        ),
        (
            Skip::EffortUnmapped {
                tier: TierId::Opus,
                effort: "max".to_owned(),
            },
            "it has no mapping for max effort on the Opus tier",
            "it has no mapping for max effort on the Opus tier",
        ),
        (
            Skip::EffortUnmapped {
                tier: TierId::Sonnet,
                effort: "low".to_owned(),
            },
            "it has no mapping for low effort on the Sonnet tier",
            "it has no mapping for low effort on the Sonnet tier",
        ),
        (
            Skip::Unsupported {
                capability: Capability::Tools,
            },
            "it cannot carry tools",
            "it cannot carry tools",
        ),
        (
            Skip::Unsupported {
                capability: Capability::Images,
            },
            "it cannot carry images",
            "it cannot carry images",
        ),
        (
            Skip::Unsupported {
                capability: Capability::Documents,
            },
            "it cannot carry documents",
            "it cannot carry documents",
        ),
        (
            Skip::Unsupported {
                capability: Capability::ServerTools,
            },
            "it cannot carry tools the provider runs",
            "it cannot carry tools the provider runs",
        ),
        (
            Skip::Exhausted,
            "its allowance is used up",
            "its allowance is used up",
        ),
        (
            Skip::Tried,
            "this request already tried it",
            "this request already tried it",
        ),
    ];
    let relayed = [
        ConnectionKind::ClaudeSubscription,
        ConnectionKind::AnthropicApiKey,
        ConnectionKind::OpenRouter,
        ConnectionKind::OpenAiApiKey,
    ];
    for (skip, for_relayed, for_sign_in) in rows {
        for kind in relayed {
            assert_eq!(skip_sentence(kind, &skip), for_relayed, "{skip:?} {kind:?}");
        }
        assert_eq!(
            skip_sentence(ConnectionKind::ChatGptSignIn, &skip),
            for_sign_in,
            "{skip:?} sign-in"
        );
    }
}
