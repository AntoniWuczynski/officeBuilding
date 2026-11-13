use super::{
    Answer, blocked, busy, interrupted, no_key, other_path, redirect, refused, skip_sentence,
    transport, unknown_desk, unreachable, unreadable,
};
use crate::routing::capabilities::Capability;
use crate::routing::config::{ConnectionKind, TierId};
use crate::routing::policy::Skip;
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
    rows
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
            "this build cannot use that kind of connection yet",
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
    ];
    for (skip, for_relayed, for_openai) in rows {
        for kind in relayed {
            assert_eq!(skip_sentence(kind, &skip), for_relayed, "{skip:?} {kind:?}");
        }
        assert_eq!(
            skip_sentence(ConnectionKind::OpenAiApiKey, &skip),
            for_openai,
            "{skip:?} openai"
        );
    }
}
