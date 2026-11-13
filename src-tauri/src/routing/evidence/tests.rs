use super::*;
use ConnectionKind::{AnthropicApiKey, ClaudeSubscription, OpenRouter};

const RELAYED: [ConnectionKind; 3] = [ClaudeSubscription, AnthropicApiKey, OpenRouter];

// Anthropic, https://platform.claude.com/docs/en/api/rate-limits ("Reaching your spend cap"),
// the body as printed there with an invented request id.
const SPEND_CAP_BODY: &str = r#"{
  "type": "error",
  "error": {
    "type": "rate_limit_error",
    "message": "You have reached your API usage limits: your organization has crossed its monthly API usage threshold, set based on your organization's API tier. You will regain access on 2026-09-01 at 00:00 UTC.",
    "details": { "error_code": "enforced_spend_limit_reached" }
  },
  "request_id": "req_invented_0001"
}"#;

// Anthropic, same page ("Ramp limits"): built from the documented members, `rate_limit_error`
// with `error.details.error_code` `ramp_limit_exceeded`. The message is invented.
const RAMP_BODY: &str = r#"{"type":"error","error":{"type":"rate_limit_error","message":"Your request rate rose faster than your ramp limit allows.","details":{"error_code":"ramp_limit_exceeded"}},"request_id":"req_invented_0002"}"#;

// Anthropic, https://platform.claude.com/docs/en/api/errors : the envelope with
// `rate_limit_error` for a 429. The message is invented.
const RATE_LIMIT_BODY: &str = r#"{"type":"error","error":{"type":"rate_limit_error","message":"Number of requests has exceeded your per-minute rate limit."},"request_id":"req_invented_0003"}"#;

// Anthropic, rate-limits page ("Setting your own spend limit"): a 400 `invalid_request_error`
// whose message starts with one of the two documented openings. The rest is invented.
const OWN_LIMIT_BODY: &str = r#"{"type":"error","error":{"type":"invalid_request_error","message":"You have reached your specified API usage limits. You will regain access on 2026-12-01 at 00:00 UTC."},"request_id":"req_invented_0004"}"#;
const OWN_WORKSPACE_LIMIT_BODY: &str = r#"{"type":"error","error":{"type":"invalid_request_error","message":"You have reached your specified workspace API usage limits. You will regain access on 2026-12-01 at 00:00 UTC."},"request_id":"req_invented_0005"}"#;

// Anthropic, errors page: an ordinary 400 `invalid_request_error`, as printed there.
const OTHER_400_BODY: &str = r#"{"type":"error","error":{"type":"invalid_request_error","message":"This model does not support assistant message prefill. The conversation must end with a user message."}}"#;

// The spend-limit opening under another `error.type`: not the documented shape.
const OPENING_UNDER_API_ERROR: &str = r#"{"type":"error","error":{"type":"api_error","message":"You have reached your specified API usage limits."},"request_id":"req_invented_0006"}"#;

const NOT_JSON: &str = "<html><body>upstream error</body></html>";

// OpenRouter, https://openrouter.ai/docs/api_reference/limits ("In-flight spending budget"),
// the 402 body as printed there.
const OR_IN_FLIGHT: &str = r#"{
  "error": {
    "code": 402,
    "message": "This request would exceed your available credits given your current in-flight requests. Retry after in-flight requests settle, or add credits.",
    "metadata": {
      "reason": "in_flight_budget_exhausted",
      "limit_source": "openrouter_in_flight_budget",
      "remedy_hint": "Retry after your in-flight requests settle (see the Retry-After header). Adding credits at https://openrouter.ai/settings/credits raises your in-flight budget, up to a capped ceiling."
    }
  }
}"#;

// OpenRouter, same page ("Handling 403 budget errors"), the 403 body as printed there.
const OR_GUARDRAIL_BUDGET: &str = r#"{
  "error": {
    "code": 403,
    "message": "Org member budget limit exceeded (weekly limit). Contact your org admin. $52.00 of $50.00 used; resets 2026-10-12T00:00:00.000Z.",
    "metadata": {
      "limit_source": "openrouter_guardrail_budget",
      "remedy_hint": "Wait for the budget window to reset (see metadata.resets_at), or ask your org admin to raise the guardrail budget.",
      "scope": "member",
      "limit_usd": 50,
      "spend_usd": 52,
      "resets_at": "2026-10-12T00:00:00.000Z"
    }
  }
}"#;
/// 2026-10-12T00:00:00Z.
const OR_RESET: u64 = 1_791_763_200;

// OpenRouter, same page: the other two documented budget sources, with `resets_at` null for a
// lifetime workspace budget ("or `null` for a lifetime workspace budget"). Messages invented.
const OR_WORKSPACE_BUDGET_NULL: &str = r#"{"error":{"code":403,"message":"Workspace budget exceeded.","metadata":{"limit_source":"openrouter_workspace_budget","resets_at":null}}}"#;
const OR_KEY_LIMIT_403: &str = r#"{"error":{"code":403,"message":"Key limit exceeded.","metadata":{"limit_source":"openrouter_key_limit","resets_at":"2026-10-12T00:00:00Z"}}}"#;
const OR_BUDGET_BAD_DATE: &str = r#"{"error":{"code":403,"message":"Budget exceeded.","metadata":{"limit_source":"openrouter_guardrail_budget","resets_at":"next monday"}}}"#;
const OR_403_NO_SOURCE: &str =
    r#"{"error":{"code":403,"message":"Forbidden.","metadata":{"scope":"member"}}}"#;

// OpenRouter, same page ("Handling 402 errors"): a 402 with each other `limit_source`.
const OR_KEY_LIMIT_402: &str = r#"{"error":{"code":402,"message":"Key credit limit reached.","metadata":{"limit_source":"openrouter_key_limit"}}}"#;
const OR_CREDITS: &str = r#"{"error":{"code":402,"message":"Insufficient credits.","metadata":{"limit_source":"openrouter_credits"}}}"#;
const OR_CREDITS_WEIGHT: &str = r#"{"error":{"code":402,"message":"This request costs more than your whole budget.","metadata":{"reason":"weight_exceeds_budget","limit_source":"openrouter_credits"}}}"#;
const OR_402_GUARDRAIL: &str = r#"{"error":{"code":402,"message":"Budget.","metadata":{"limit_source":"openrouter_guardrail_budget"}}}"#;
const OR_402_NO_METADATA: &str = r#"{"error":{"code":402,"message":"Payment required."}}"#;

// OpenRouter, https://openrouter.ai/docs/api/api-reference/anthropic-messages/create-a-message :
// the Anthropic-format 429 example as printed there, with an invented id.
const OR_ANTHROPIC_429: &str = r#"{"error":{"message":"Rate limit exceeded","type":"rate_limit_error"},"request_id":"gen-invented0001","type":"error"}"#;

// The same envelope's optional top-level `metadata` holding `limit_source`: no page says it
// sits there, so it is not read.
const OR_TOP_LEVEL_SOURCE: &str = r#"{"type":"error","error":{"type":"rate_limit_error","message":"Key limit reached."},"metadata":{"limit_source":"openrouter_key_limit"},"request_id":"gen-invented0002"}"#;

// The documented members in a shape the vendor does not send: an `error` that is an array.
const ERROR_AS_ARRAY: &str =
    r#"{"error":["invalid_request_error","You have reached your specified API usage limits"]}"#;

type Pairs = &'static [(&'static str, &'static str)];

/// (name, status, headers, body, expected).
type Row = (&'static str, u16, Pairs, &'static str, Observation);

const WAIT_30: Pairs = &[("retry-after", "30")];

const fn throttled(retry_after: Option<u64>) -> Observation {
    Observation::Throttled { retry_after }
}

const fn faulted(retry_after: Option<u64>) -> Observation {
    Observation::Faulted { retry_after }
}

const fn whole_with(reset_at: Option<u64>) -> Observation {
    Observation::Exhausted(Exhaustion {
        breadth: Breadth::Whole,
        reset_at,
        window: None,
    })
}

fn request(reset_at: Option<u64>, window: Option<&str>) -> Observation {
    Observation::Exhausted(Exhaustion {
        breadth: Breadth::Request,
        reset_at,
        window: window.map(str::to_owned),
    })
}

fn headers(pairs: Pairs) -> Vec<Header> {
    pairs
        .iter()
        .map(|(name, value)| Header {
            name: (*name).to_owned(),
            value: value.as_bytes().to_vec(),
        })
        .collect()
}

fn check(kind: ConnectionKind, rows: &[Row]) {
    for (name, status, pairs, body, expected) in rows {
        let seen = observe(kind, *status, &headers(pairs), body.as_bytes());
        assert_eq!(&seen, expected, "{kind:?}: {name}");
    }
}

const BASE_ROWS: &[Row] = &[
    ("401", 401, &[], "", Observation::Unauthorised),
    ("402", 402, &[], "", Observation::Forbidden),
    ("403", 403, &[], "", Observation::Forbidden),
    ("429", 429, &[], "", throttled(None)),
    ("429 with a wait", 429, WAIT_30, "", throttled(Some(30))),
    ("408", 408, &[], "", faulted(None)),
    ("500", 500, &[], "", faulted(None)),
    ("503 with a wait", 503, WAIT_30, "", faulted(Some(30))),
    ("529", 529, &[], "", faulted(None)),
    ("599", 599, &[], "", faulted(None)),
    (
        "301, a wait is not carried",
        301,
        WAIT_30,
        "",
        faulted(None),
    ),
    ("399", 399, &[], "", faulted(None)),
    ("400", 400, &[], "", Observation::Rejected),
    ("404", 404, &[], "", Observation::Rejected),
    ("413", 413, &[], "", Observation::Rejected),
    ("418", 418, &[], "", Observation::Rejected),
    ("200", 200, &[], "", faulted(None)),
    ("100", 100, WAIT_30, "", faulted(None)),
    ("600", 600, &[], "", faulted(None)),
];

const REJECTED: Pairs = &[(UNIFIED_STATUS, "rejected")];

const ANTHROPIC_KEY_ROWS: &[Row] = &[
    (
        "the tier spend cap",
        429,
        &[],
        SPEND_CAP_BODY,
        whole_with(None),
    ),
    (
        "the spend cap ignores a retry-after",
        429,
        WAIT_30,
        SPEND_CAP_BODY,
        whole_with(None),
    ),
    ("a ramp limit", 429, WAIT_30, RAMP_BODY, throttled(Some(30))),
    (
        "a plain rate limit",
        429,
        &[],
        RATE_LIMIT_BODY,
        throttled(None),
    ),
    (
        "the customer's own API usage limit",
        400,
        &[],
        OWN_LIMIT_BODY,
        whole_with(None),
    ),
    (
        "the customer's own workspace limit",
        400,
        &[],
        OWN_WORKSPACE_LIMIT_BODY,
        whole_with(None),
    ),
    (
        "a 429 carrying the subscription header",
        429,
        REJECTED,
        RATE_LIMIT_BODY,
        throttled(None),
    ),
    (
        "a 400 with another message",
        400,
        &[],
        OTHER_400_BODY,
        Observation::Rejected,
    ),
    (
        "the opening under another error type",
        400,
        &[],
        OPENING_UNDER_API_ERROR,
        Observation::Rejected,
    ),
    (
        "the own-limit body on a 429",
        429,
        &[],
        OWN_LIMIT_BODY,
        throttled(None),
    ),
    (
        "the spend-cap body on a 400",
        400,
        &[],
        SPEND_CAP_BODY,
        Observation::Rejected,
    ),
    (
        "a body that is not JSON, 429",
        429,
        &[],
        NOT_JSON,
        throttled(None),
    ),
    (
        "a body that is not JSON, 401",
        401,
        &[],
        NOT_JSON,
        Observation::Unauthorised,
    ),
    (
        "an error that is an array",
        400,
        &[],
        ERROR_AS_ARRAY,
        Observation::Rejected,
    ),
];

const OPENROUTER_ROWS: &[Row] = &[
    (
        "402 key limit",
        402,
        &[],
        OR_KEY_LIMIT_402,
        whole_with(None),
    ),
    (
        "403 guardrail budget with its reset",
        403,
        &[],
        OR_GUARDRAIL_BUDGET,
        whole_with(Some(OR_RESET)),
    ),
    (
        "403 workspace budget with resets_at null",
        403,
        &[],
        OR_WORKSPACE_BUDGET_NULL,
        whole_with(None),
    ),
    (
        "403 key limit",
        403,
        &[],
        OR_KEY_LIMIT_403,
        whole_with(Some(OR_RESET)),
    ),
    (
        "403 resets_at that is not a date",
        403,
        &[],
        OR_BUDGET_BAD_DATE,
        whole_with(None),
    ),
    (
        "402 in-flight budget",
        402,
        WAIT_30,
        OR_IN_FLIGHT,
        throttled(Some(30)),
    ),
    (
        "402 one request too large",
        402,
        &[],
        OR_CREDITS_WEIGHT,
        Observation::Rejected,
    ),
    (
        "402 credits is not a zero balance",
        402,
        &[],
        OR_CREDITS,
        Observation::Forbidden,
    ),
    (
        "402 with a budget source",
        402,
        &[],
        OR_402_GUARDRAIL,
        Observation::Forbidden,
    ),
    (
        "402 with no metadata",
        402,
        &[],
        OR_402_NO_METADATA,
        Observation::Forbidden,
    ),
    (
        "403 with no limit_source",
        403,
        &[],
        OR_403_NO_SOURCE,
        Observation::Forbidden,
    ),
    (
        "403 with the in-flight source",
        403,
        &[],
        OR_IN_FLIGHT,
        Observation::Forbidden,
    ),
    (
        "limit_source under a top-level metadata",
        429,
        &[],
        OR_TOP_LEVEL_SOURCE,
        throttled(None),
    ),
    (
        "limit_source under a top-level metadata, 402",
        402,
        &[],
        OR_TOP_LEVEL_SOURCE,
        Observation::Forbidden,
    ),
    (
        "the Anthropic-format 429",
        429,
        &[],
        OR_ANTHROPIC_429,
        throttled(None),
    ),
    (
        "a 503 with a wait of 7",
        503,
        &[("retry-after", "7")],
        "",
        faulted(Some(7)),
    ),
];

#[test]
fn the_base_table_reads_the_status_alone() {
    for kind in RELAYED {
        check(kind, BASE_ROWS);
    }
}

#[test]
fn status_alone_never_proves_exhaustion() {
    for kind in RELAYED {
        for status in 100..=599 {
            let seen = observe(kind, status, &[], b"");
            assert!(
                !matches!(seen, Observation::Exhausted(_)),
                "{kind:?} {status} gave {seen:?}"
            );
        }
    }
}

#[test]
fn the_subscription_reads_its_limit_headers_only() {
    const FULL: Pairs = &[
        (UNIFIED_STATUS, "rejected"),
        (UNIFIED_RESET, "1790000000"),
        (UNIFIED_CLAIM, "five_hour"),
    ];
    let rows: Vec<Row> = vec![
        (
            "rejected with a reset and a claim",
            429,
            FULL,
            "",
            request(Some(1_790_000_000), Some("five_hour")),
        ),
        (
            "rejected with spaces around it",
            429,
            &[(UNIFIED_STATUS, " rejected ")],
            "",
            request(None, None),
        ),
        (
            "rejected and no reset",
            429,
            &[(UNIFIED_STATUS, "rejected"), (UNIFIED_CLAIM, "seven_day")],
            "",
            request(None, Some("seven_day")),
        ),
        (
            "a reset that is not a number",
            429,
            &[(UNIFIED_STATUS, "rejected"), (UNIFIED_RESET, "soon")],
            "",
            request(None, None),
        ),
        (
            "allowed",
            429,
            &[(UNIFIED_STATUS, "allowed")],
            "",
            throttled(None),
        ),
        (
            "allowed_warning",
            429,
            &[(UNIFIED_STATUS, "allowed_warning")],
            "",
            throttled(None),
        ),
        (
            "REJECTED in capitals",
            429,
            &[(UNIFIED_STATUS, "REJECTED")],
            "",
            throttled(None),
        ),
        (
            "the header twice",
            429,
            &[(UNIFIED_STATUS, "rejected"), (UNIFIED_STATUS, "rejected")],
            "",
            throttled(None),
        ),
        ("no such header", 429, &[], "", throttled(None)),
        (
            "rejected on a 503 is not a 429",
            503,
            FULL,
            "",
            faulted(None),
        ),
        (
            "the spend-cap body is not read",
            429,
            &[],
            SPEND_CAP_BODY,
            throttled(None),
        ),
        (
            "a retry-after in seconds is carried",
            429,
            WAIT_30,
            "",
            throttled(Some(30)),
        ),
        (
            "a retry-after date is not",
            429,
            &[("retry-after", "Wed, 21 Oct")],
            "",
            throttled(None),
        ),
    ];
    check(ClaudeSubscription, &rows);
}

#[test]
fn an_anthropic_key_reads_the_documented_error_body() {
    check(AnthropicApiKey, ANTHROPIC_KEY_ROWS);
}

#[test]
fn openrouter_reads_error_metadata_only() {
    check(OpenRouter, OPENROUTER_ROWS);
}
