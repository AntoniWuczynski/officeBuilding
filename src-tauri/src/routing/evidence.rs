//! What a provider's refusal proves: a reply that is not 2xx, of one connection kind, or a
//! failure inside a translated route's stream, turned into Stage A's `Observation`. Pure. Only a
//! shape a vendor documents (or, for the Claude subscription, the one the probe replayed) counts
//! as exhaustion. A shape with no row here is ambiguous and falls back to the status alone, which
//! never proves exhaustion. `Said` is the one place a provider's own words reach a sentence (F19).

use super::body::Object;
use super::config::ConnectionKind;
use super::observe::{Breadth, Exhaustion, Observation};
use super::translate::reply::Failure;
use super::wire::{Header, only};
use crate::delivery::text;
use crate::http;
use serde::Deserialize;

/// The subscription's own limit headers (brief and T2: how Claude Code itself reads a used-up
/// plan limit). The vendor does not document their grammar, so only these three are read.
const UNIFIED_STATUS: &str = "anthropic-ratelimit-unified-status";
const UNIFIED_RESET: &str = "anthropic-ratelimit-unified-reset";
const UNIFIED_CLAIM: &str = "anthropic-ratelimit-unified-representative-claim";

/// T9, <https://platform.claude.com/docs/en/api/rate-limits> ("Reaching your spend cap"):
/// "`error.details.error_code` is `enforced_spend_limit_reached`".
const SPEND_CAP: &str = "enforced_spend_limit_reached";
/// T9, same page ("Ramp limits"): "`error.details.error_code` is `ramp_limit_exceeded`".
const RAMP_LIMIT: &str = "ramp_limit_exceeded";
/// T9, same page ("Setting your own spend limit"): "requests return HTTP 400 with error type
/// `invalid_request_error`. The message starts with `You have reached your specified API usage
/// limits`, or `You have reached your specified workspace API usage limits` for a workspace limit".
const OWN_LIMIT: &str = "You have reached your specified API usage limits";
const OWN_WORKSPACE_LIMIT: &str = "You have reached your specified workspace API usage limits";

/// U4, <https://developers.openai.com/api/docs/guides/error-codes>: "429 - Credit balance
/// exhausted", "Organization spend limit reached", "Project spend limit reached", "Organization
/// usage limit reached". "Retrying billing, spend, or quota errors won't restore API access."
const OPENAI_USED_UP: [&str; 4] = [
    "credit_balance_exhausted",
    "organization_spend_limit_exceeded",
    "project_spend_limit_exceeded",
    "organization_usage_limit_exceeded",
];
/// U4: "429 - Slow down", type `rate_limit_error`, code `slow_down`.
const OPENAI_SLOW_DOWN: &str = "slow_down";
/// U2, <https://developers.openai.com/api/reference/resources/responses/streaming-events>:
/// `response.error.code` lists `rate_limit_exceeded` and `server_error`.
const OPENAI_RATE_LIMITED: &str = "rate_limit_exceeded";
const OPENAI_SERVER_ERROR: &str = "server_error";
/// U4: "503 - Model temporarily overloaded", type `service_unavailable_error`, code
/// `server_is_overloaded`.
const OPENAI_OVERLOADED: &str = "server_is_overloaded";

/// The longest provider code `Said` keeps, and the most of a provider message it keeps (F19).
const SAID_CODE_MAX: usize = 64;
const SAID_MESSAGE_MAX: usize = 300;

/// What a reply that is not 2xx proves. `body` is what the relay read of it (at most 64 KiB).
/// Only documented shapes count as exhaustion. Everything else moves nothing.
#[must_use]
pub fn observe(kind: ConnectionKind, status: u16, headers: &[Header], body: &[u8]) -> Observation {
    let retry_after = retry_after(headers);
    let proven = match kind {
        ConnectionKind::ClaudeSubscription => subscription(status, headers),
        ConnectionKind::AnthropicApiKey => anthropic_key(status, body, retry_after),
        ConnectionKind::OpenRouter => openrouter(status, body, retry_after),
        ConnectionKind::OpenAiApiKey => openai_key(status, body, retry_after),
        // Never leased in this build, and its codes are not built (section 12).
        ConnectionKind::ChatGptSignIn => None,
    };
    proven.unwrap_or_else(|| by_status(status, retry_after))
}

/// The base table, for every kind. Status alone never proves exhaustion.
fn by_status(status: u16, retry_after: Option<u64>) -> Observation {
    match status {
        401 => Observation::Unauthorised,
        402 | 403 => Observation::Forbidden,
        429 => Observation::Throttled { retry_after },
        408 | 500..=599 => Observation::Faulted { retry_after },
        400..=499 => Observation::Rejected,
        _ => Observation::Faulted { retry_after: None },
    }
}

/// A single `retry-after` header in whole seconds. A date or anything else is None.
fn retry_after(headers: &[Header]) -> Option<u64> {
    let value = std::str::from_utf8(only(headers, "retry-after")?).ok()?;
    http::retry_after(Some(value)).map(|wait| wait.as_secs())
}

fn whole(reset_at: Option<u64>) -> Observation {
    Observation::Exhausted(Exhaustion {
        breadth: Breadth::Whole,
        reset_at,
        window: None,
    })
}

fn header_text<'h>(headers: &'h [Header], name: &str) -> Option<&'h str> {
    std::str::from_utf8(only(headers, name)?).ok()
}

/// Headers only. The body is never looked at.
fn subscription(status: u16, headers: &[Header]) -> Option<Observation> {
    if status != 429 || only(headers, UNIFIED_STATUS)?.trim_ascii() != b"rejected" {
        return None;
    }
    Some(Observation::Exhausted(Exhaustion {
        breadth: Breadth::Request,
        reset_at: header_text(headers, UNIFIED_RESET).and_then(|v| v.trim().parse::<u64>().ok()),
        window: header_text(headers, UNIFIED_CLAIM).map(str::to_owned),
    }))
}

#[derive(Deserialize)]
struct AnthropicReply {
    error: Object<AnthropicError>,
}

#[derive(Deserialize)]
struct AnthropicError {
    #[serde(rename = "type")]
    kind: String,
    message: String,
    #[serde(default)]
    details: Option<Object<AnthropicDetails>>,
}

#[derive(Deserialize)]
struct AnthropicDetails {
    #[serde(default)]
    error_code: Option<String>,
}

fn anthropic_key(status: u16, body: &[u8], retry_after: Option<u64>) -> Option<Observation> {
    let Object(AnthropicReply {
        error: Object(error),
    }) = serde_json::from_slice(body).ok()?;
    let code = error.details.and_then(|Object(details)| details.error_code);
    match (status, code.as_deref()) {
        // E21: the spend-cap reply has no `retry-after`, and its date is only in the text.
        (429, Some(SPEND_CAP)) => Some(whole(None)),
        (429, Some(RAMP_LIMIT)) => Some(Observation::Throttled { retry_after }),
        (400, _)
            if error.kind == "invalid_request_error"
                && (error.message.starts_with(OWN_LIMIT)
                    || error.message.starts_with(OWN_WORKSPACE_LIMIT)) =>
        {
            Some(whole(None))
        }
        _ => None,
    }
}

/// T17, <https://openrouter.ai/docs/api_reference/limits>: only `error.metadata` is read. No page
/// says `limit_source` sits in the top-level `metadata` of the Anthropic-format envelope (T14).
#[derive(Deserialize)]
struct OpenRouterReply {
    error: Object<OpenRouterError>,
}

#[derive(Deserialize)]
struct OpenRouterError {
    #[serde(default)]
    metadata: Option<Object<OpenRouterMetadata>>,
}

#[derive(Deserialize)]
struct OpenRouterMetadata {
    #[serde(default)]
    limit_source: Option<String>,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    resets_at: Option<String>,
}

fn openrouter(status: u16, body: &[u8], retry_after: Option<u64>) -> Option<Observation> {
    let Object(OpenRouterReply {
        error: Object(error),
    }) = serde_json::from_slice(body).ok()?;
    let Object(metadata) = error.metadata?;
    match (status, metadata.limit_source.as_deref()?) {
        // "`openrouter_key_limit` means the API key's credit limit is exhausted".
        (402, "openrouter_key_limit") => Some(whole(None)),
        // A budget refusal: `resets_at` is "the ISO 8601 UTC instant the window rolls over, or `null`".
        (
            403,
            "openrouter_guardrail_budget" | "openrouter_workspace_budget" | "openrouter_key_limit",
        ) => Some(whole(metadata.resets_at.as_deref().and_then(unix_seconds))),
        // "This is transient [...] The response includes a `Retry-After` header".
        (402, "openrouter_in_flight_budget") => Some(Observation::Throttled { retry_after }),
        // "this single request's estimated cost is larger than your whole budget".
        (402, "openrouter_credits")
            if metadata.reason.as_deref() == Some("weight_exceeds_budget") =>
        {
            Some(Observation::Rejected)
        }
        // Any other 402, `openrouter_credits` included, is the base table's `Forbidden` (E22).
        _ => None,
    }
}

fn unix_seconds(instant: &str) -> Option<u64> {
    let parsed = chrono::DateTime::parse_from_rfc3339(instant).ok()?;
    u64::try_from(parsed.timestamp()).ok()
}

/// U4 names the members `error.code`, `error.type` and `error.message` but prints no whole body,
/// so only `error.code` and `error.message` are read, and a body that does not fit proves nothing.
#[derive(Deserialize)]
struct OpenAiReply {
    error: Object<OpenAiError>,
}

#[derive(Deserialize)]
struct OpenAiError {
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

fn openai_error(body: &[u8]) -> Option<OpenAiError> {
    let Object(OpenAiReply {
        error: Object(error),
    }) = serde_json::from_slice(body).ok()?;
    Some(error)
}

/// Only a 429 with one of U4's four billing codes proves a used-up allowance. Not rows, on
/// purpose: `error.type` `insufficient_quota` with another code (U4 calls the type "broader" and
/// says to inspect the code), a 429 with no code, and `x-ratelimit-*` headers.
fn openai_key(status: u16, body: &[u8], retry_after: Option<u64>) -> Option<Observation> {
    let error = openai_error(body)?;
    match (status, error.code.as_deref()?) {
        // U6: a spend limit "resets with the next monthly cycle", and the reply gives no time.
        (429, code) if OPENAI_USED_UP.contains(&code) => Some(whole(None)),
        (429, OPENAI_SLOW_DOWN) => Some(Observation::Throttled { retry_after }),
        _ => None,
    }
}

/// What a failure inside a translated route's stream proves. Only `kind`'s own documented codes
/// count. U5: "An error after streaming begins can arrive as a stream event", so only the `error`
/// event can prove a used-up allowance. U2 gives `response.failed` a closed list of codes that
/// holds no billing code, so one there is undocumented and moves nothing (F14).
#[must_use]
pub fn stream(kind: ConnectionKind, failure: &Failure) -> Observation {
    let (code, may_prove) = match failure {
        Failure::Error { code, .. } => (code.as_deref(), kind == ConnectionKind::OpenAiApiKey),
        Failure::Failed { code, .. } => (code.as_deref(), false),
        // F17: a reason this route has no stop reason for.
        Failure::Incomplete { .. } => return Observation::Rejected,
        // The outcome is unknown, so it is never asked again (Stage A, 6.3).
        Failure::Unreadable => return Observation::Dropped,
    };
    match code {
        Some(code) if may_prove && OPENAI_USED_UP.contains(&code) => whole(None),
        Some(OPENAI_SLOW_DOWN | OPENAI_RATE_LIMITED) => {
            Observation::Throttled { retry_after: None }
        }
        None | Some(OPENAI_SERVER_ERROR | OPENAI_OVERLOADED) => {
            Observation::Faulted { retry_after: None }
        }
        // Not retried, and its code is shown to the desk.
        Some(_) => Observation::Rejected,
    }
}

/// A provider's own code and message, cleaned for a sentence (F19). Never stored, never logged.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Said {
    pub code: Option<String>,
    pub message: Option<String>,
}

impl Said {
    /// `code` kept only when it is 1 to 64 characters of ASCII letters, digits, `_`, `-` or `.`.
    /// `message` cut to its first 300 characters, then put on one line. None when nothing is left.
    #[must_use]
    pub fn new(code: Option<&str>, message: Option<&str>) -> Self {
        let code = code
            .and_then(|c| crate::routing::translate::token(c, SAID_CODE_MAX, b"_-."))
            .map(str::to_owned);
        let message = message.and_then(|m| {
            let cut: String = m.chars().take(SAID_MESSAGE_MAX).collect();
            text::line(&cut, SAID_MESSAGE_MAX).ok()
        });
        Self { code, message }
    }

    /// From a stream failure. `Incomplete`'s reason takes the place of the code. `Unreadable`
    /// says nothing.
    #[must_use]
    pub fn of(failure: &Failure) -> Self {
        match failure {
            Failure::Failed { code, message } | Failure::Error { code, message } => {
                Self::new(code.as_deref(), message.as_deref())
            }
            Failure::Incomplete { reason } => Self::new(reason.as_deref(), None),
            Failure::Unreadable => Self::default(),
        }
    }
}

/// From a reply that is not 2xx: `OpenAI`'s `error.code` and `error.message` (U4). Empty for
/// another kind or a body that does not fit.
#[must_use]
pub fn said(kind: ConnectionKind, body: &[u8]) -> Said {
    match kind {
        ConnectionKind::OpenAiApiKey => openai_error(body)
            .map(|e| Said::new(e.code.as_deref(), e.message.as_deref()))
            .unwrap_or_default(),
        ConnectionKind::ClaudeSubscription
        | ConnectionKind::AnthropicApiKey
        | ConnectionKind::OpenRouter
        | ConnectionKind::ChatGptSignIn => Said::default(),
    }
}

#[cfg(test)]
mod tests;
