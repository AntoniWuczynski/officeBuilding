//! The router's own answers: status, Anthropic's error envelope and every sentence. A blocked
//! chain is a 400 `invalid_request_error`, never a 429 or 529: Claude Code stops at once on a 400
//! and shows the sentence, and retries a 429 or 529 for minutes (T2). On the translated route a
//! message may also hold a provider's own code and one-line message, cleaned by `Said` (F19), and
//! nothing else from outside.

use super::capabilities::Capability;
use super::config::{ConnectionKind, TierId};
use super::evidence::Said;
use super::observe::Observation;
use super::policy::Skip;
use super::translate::Carry;
use super::upstream;
use super::wire::{self, Refusal};
use serde::Serialize;
use std::io;
use std::net::TcpStream;

const INVALID: &str = "invalid_request_error";
const API: &str = "api_error";

/// One local answer. The message holds no request content: only fixed words, connection names
/// from Settings, model or effort names that passed `Requirements::new`, member or block-type
/// names that passed `translate::label`, and a provider's code and message cleaned by `Said`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    pub status: u16,
    pub kind: &'static str,
    pub message: String,
    pub retry_after: Option<u64>,
}

#[derive(Serialize)]
struct Envelope<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    error: Inner<'a>,
}

#[derive(Serialize)]
struct Inner<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    message: &'a str,
}

impl Answer {
    fn new(status: u16, kind: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            kind,
            message: message.into(),
            retry_after: None,
        }
    }

    /// `{"type":"error","error":{"type":"<kind>","message":"<message>"}}`.
    #[must_use]
    pub fn body(&self) -> Vec<u8> {
        let envelope = Envelope {
            kind: "error",
            error: Inner {
                kind: self.kind,
                message: &self.message,
            },
        };
        // Two structs of strings always serialise: the empty fallback is never taken.
        serde_json::to_vec(&envelope).unwrap_or_default()
    }

    /// `wire::write_local`, with `retry-after` when it is set.
    pub fn write(&self, stream: &mut TcpStream) -> io::Result<()> {
        let retry_after = self.retry_after.map(|secs| secs.to_string());
        let extra: Vec<(&str, &str)> = retry_after
            .iter()
            .map(|secs| ("retry-after", secs.as_str()))
            .collect();
        wire::write_local(stream, self.status, &extra, &self.body())
    }
}

#[must_use]
pub fn unknown_desk() -> Answer {
    Answer::new(
        400,
        INVALID,
        "Office Building did not recognise this desk, so nothing was sent. Start the desk again from the building. If your shell profile sets ANTHROPIC_CUSTOM_HEADERS, it replaces the header the building relies on.",
    )
}

#[must_use]
pub fn unreadable() -> Answer {
    Answer::new(
        400,
        INVALID,
        "Office Building could not read this request as a Messages request, so nothing was sent.",
    )
}

#[must_use]
pub fn other_path() -> Answer {
    Answer::new(
        404,
        "not_found_error",
        "Office Building's router carries the Messages call only. This request was answered here and was not sent on.",
    )
}

#[must_use]
pub fn refused(name: &str, kind: ConnectionKind, skip: &Skip) -> Answer {
    cannot_take(name, &skip_sentence(kind, skip))
}

/// A request the translated route cannot carry, refused after its lease was cut (F3).
#[must_use]
pub fn cannot_carry(name: &str, carry: &Carry) -> Answer {
    let reason = match carry {
        Carry::NotStreamed => {
            "this route answers as a stream only, and the request did not ask for one".to_owned()
        }
        Carry::Prewarm => "it only asks Anthropic to warm its prompt cache (max_tokens 0), which this route has no counterpart for".to_owned(),
        Carry::ContextManagement => "this route cannot carry context management (context_management: Extra inputs are not permitted)".to_owned(),
        Carry::StopSequences => "this route cannot carry stop sequences".to_owned(),
        Carry::Member { name } => format!("this route does not carry the request setting {name}"),
        Carry::OutputSetting => "this route carries no output setting but the effort".to_owned(),
        Carry::Block { kind } => format!("this route cannot carry a {kind} block"),
        Carry::ImageSource => {
            "this route carries an image only as base64 data or a web address".to_owned()
        }
        Carry::Tool => "this route carries plain function tools only: no strict schema, no deferred loading and no tool the provider runs".to_owned(),
        Carry::ToolChoice => "this route does not know that tool choice".to_owned(),
        Carry::CallId => "a tool call in the conversation has an id OpenAI does not take (1 to 64 characters)".to_owned(),
        Carry::Unknown => "it holds something the translation does not know, and nothing is sent that cannot be carried whole".to_owned(),
    };
    cannot_take(name, &reason)
}

/// The frame `refused` and `cannot_carry` share.
fn cannot_take(name: &str, reason: &str) -> Answer {
    Answer::new(
        400,
        INVALID,
        format!(
            "Office Building: {name} cannot take this request, because {reason}. Nothing was sent, and the desk stays on {name}."
        ),
    )
}

/// A translated route's refusal, in Anthropic's envelope, because the client cannot read the
/// provider's own (F2). `status`: the provider's HTTP status, None for a failure inside a stream.
/// No 401 (it would send A Human to the Claude login) and no 529.
#[must_use]
pub fn provider_refused(
    name: &str,
    status: Option<u16>,
    seen: &Observation,
    said: &Said,
) -> Answer {
    let code = wrapped(said.code.as_deref(), " (", ")");
    let listed = wrapped(said.code.as_deref(), ", ", "");
    let http = status.map_or_else(
        || "in its reply".to_owned(),
        |status| format!("HTTP {status}"),
    );
    let message = wrapped(said.message.as_deref(), ": ", "");
    match seen {
        Observation::Exhausted(_) => Answer::new(
            400,
            INVALID,
            format!(
                "Office Building: {name} reports that its allowance is used up{code}. Nothing was passed on. Open the desk in Office Building to retry a connection or switch to another."
            ),
        ),
        Observation::Throttled { retry_after } => Answer {
            retry_after: *retry_after,
            ..Answer::new(
                429,
                "rate_limit_error",
                format!(
                    "{name} is limiting requests for now{code}. Office Building passed nothing on."
                ),
            )
        },
        Observation::Faulted { retry_after } => Answer {
            retry_after: *retry_after,
            ..Answer::new(
                // No 529: Claude Code retries it for minutes (T2), and OpenAI documents none.
                status
                    .filter(|s| (500..=599).contains(s) && *s != 529)
                    .unwrap_or(500),
                API,
                format!("{name} failed or is overloaded{code}. Office Building passed nothing on."),
            )
        },
        Observation::Unauthorised => Answer::new(
            400,
            INVALID,
            format!(
                "Office Building: {name} refused the API key saved for it (HTTP 401). Check the key in Settings, under Routing. Nothing was passed on."
            ),
        ),
        Observation::Forbidden => Answer::new(
            400,
            INVALID,
            format!(
                "Office Building: {name} says the account may not do this ({http}{listed}){message}. Nothing was passed on."
            ),
        ),
        Observation::Rejected => Answer::new(
            400,
            INVALID,
            format!(
                "Office Building: {name} refused this request ({http}{listed}){message}. Nothing was passed on, and the desk stays on {name}."
            ),
        ),
        // `Dropped`, and the rest, which cannot reach here.
        Observation::Dropped
        | Observation::Completed
        | Observation::NotSent
        | Observation::Cancelled => Answer::new(
            502,
            API,
            format!(
                "{name} sent a reply Office Building could not read as a Responses stream, so nothing was passed on. It was not asked again."
            ),
        ),
    }
}

/// A translated reply that stopped after its first bytes reached the client. Only `body()` is
/// used, as the data of the stream's `error` event.
#[must_use]
pub fn stream_stopped(name: &str, said: &Said) -> Answer {
    let code = wrapped(said.code.as_deref(), " (", ")");
    Answer::new(
        502,
        API,
        format!(
            "The reply from {name} stopped before its end{code}. Office Building did not ask again."
        ),
    )
}

/// One `<name>: <reason>.` per entry, in order.
#[must_use]
pub fn blocked(entries: &[(&str, ConnectionKind, &Skip)]) -> Answer {
    let mut parts = vec![
        "Office Building: no connection in this desk's order can take this request, so nothing was sent.".to_owned(),
    ];
    parts.extend(
        entries
            .iter()
            .map(|(name, kind, skip)| format!("{name}: {}.", skip_sentence(*kind, skip))),
    );
    parts.push(
        "Open the desk in Office Building to retry a connection or switch to another.".to_owned(),
    );
    Answer::new(400, INVALID, parts.join(" "))
}

#[must_use]
pub fn unreachable(name: &str) -> Answer {
    Answer::new(
        502,
        API,
        format!("Office Building could not reach {name}. Nothing was sent."),
    )
}

#[must_use]
pub fn no_key(name: &str) -> Answer {
    Answer::new(
        502,
        API,
        format!(
            "Office Building could not read the API key of {name} from the Keychain, so nothing was sent."
        ),
    )
}

#[must_use]
pub fn interrupted(name: &str) -> Answer {
    Answer::new(
        502,
        API,
        format!(
            "The connection to {name} broke after the request was sent and before any reply came back. Office Building did not send it again."
        ),
    )
}

#[must_use]
pub fn redirect(name: &str) -> Answer {
    Answer::new(
        502,
        API,
        format!(
            "{name} answered with a redirect. Office Building does not follow one and does not pass one on."
        ),
    )
}

#[must_use]
pub fn busy() -> Answer {
    Answer {
        retry_after: Some(1),
        ..Answer::new(
            503,
            "overloaded_error",
            "Office Building's router is carrying as many requests as it allows at once. Try again in a moment.",
        )
    }
}

/// The answer to a request turned away on the wire. None for `Gone`: nobody is there to read one.
#[must_use]
pub fn transport(refusal: Refusal) -> Option<Answer> {
    let (status, kind, message) = match refusal {
        Refusal::Malformed => (
            400,
            INVALID,
            "Office Building's router could not read this request.",
        ),
        Refusal::HeadTooLarge => (
            431,
            INVALID,
            "This request's headers are larger than Office Building's router carries.",
        ),
        Refusal::Version => (
            505,
            INVALID,
            "Office Building's router speaks HTTP/1.1 only.",
        ),
        Refusal::Origin | Refusal::Host => (
            403,
            "permission_error",
            "Office Building's router answers the building's own desks only.",
        ),
        Refusal::LengthRequired => (
            411,
            INVALID,
            "Office Building's router needs a Content-Length.",
        ),
        Refusal::Encoding => (
            415,
            INVALID,
            "Office Building's router does not take an encoded request body.",
        ),
        Refusal::TooLarge => (
            413,
            "request_too_large",
            "This request is larger than 64 MiB, the most Office Building's router carries.",
        ),
        Refusal::Unadmitted => return Some(unknown_desk()),
        Refusal::Gone => return None,
    };
    Some(Answer::new(status, kind, message))
}

/// `<before><value><after>`, or nothing.
fn wrapped(value: Option<&str>, before: &str, after: &str) -> String {
    value.map_or_else(String::new, |value| format!("{before}{value}{after}"))
}

/// Why a connection was skipped, as the clause after "because". Also used by `Router::switch`.
#[must_use]
pub fn skip_sentence(kind: ConnectionKind, skip: &Skip) -> String {
    match skip {
        Skip::Removed => "it is no longer in Settings".to_owned(),
        Skip::PaidNotAllowed => "it is billed for each request and may not spend money (Settings, Routing, \"May spend money\")".to_owned(),
        Skip::ExperimentalNotAllowed => "it is an experimental route that is not switched on".to_owned(),
        Skip::NoCredential if upstream::relays(kind) => "no API key is saved for it".to_owned(),
        Skip::NoCredential => {
            "it needs Sign in with ChatGPT, which this build does not have yet".to_owned()
        }
        Skip::UnmappedModel { model } => {
            format!("the model {model} is not listed under any tier in Settings")
        }
        Skip::NoTarget { tier } => format!("it has no model for the {} tier", tier_name(*tier)),
        Skip::EffortUnmapped { tier, effort } => format!(
            "it has no mapping for {effort} effort on the {} tier",
            tier_name(*tier)
        ),
        Skip::Unsupported { capability } => {
            format!("it cannot carry {}", capability_words(*capability))
        }
        Skip::Exhausted => "its allowance is used up".to_owned(),
        Skip::Tried => "this request already tried it".to_owned(),
    }
}

const fn tier_name(tier: TierId) -> &'static str {
    match tier {
        TierId::Fable => "Fable",
        TierId::Opus => "Opus",
        TierId::Sonnet => "Sonnet",
        TierId::Haiku => "Haiku",
    }
}

const fn capability_words(capability: Capability) -> &'static str {
    match capability {
        Capability::Tools => "tools",
        Capability::Images => "images",
        Capability::Documents => "documents",
        Capability::ServerTools => "tools the provider runs",
    }
}

#[cfg(test)]
mod tests;
