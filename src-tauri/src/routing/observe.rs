//! What a transport reports about one upstream attempt. Types only: turning a provider's
//! reply into an `Observation` is Stage B's and Stage C's work. No content, no credential.
//!
//! There is deliberately no variant for "the usage display reads zero" or "the reset time
//! has passed": neither is evidence (D7), so neither can be reported.

/// How far a provider's refusal reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Breadth {
    /// Every request on the connection (an API credit balance, a spend cap, a key's budget, an app grant's limit).
    Whole,
    /// Only requests like the one refused (a subscription window, which can be per model family).
    Request,
}

/// A provider's own, documented or probed, statement that the allowance is used up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exhaustion {
    pub breadth: Breadth,
    /// When the provider says it comes back (Unix seconds). None: it did not say.
    pub reset_at: Option<u64>,
    /// The window the provider named, as it wrote it. Display only, never parsed.
    pub window: Option<String>,
}

/// What became of one upstream attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Observation {
    /// The reply ran to its clean end.
    Completed,
    /// Confirmed: the allowance is used up.
    Exhausted(Exhaustion),
    /// A limit that is not proven to be the allowance: a 429 without the evidence, a ramp limit,
    /// a budget held by requests in flight. `retry_after`: seconds, when the provider gave them.
    Throttled { retry_after: Option<u64> },
    /// The provider said it is overloaded or failed (5xx, 529).
    Faulted { retry_after: Option<u64> },
    /// The credential was refused (401).
    Unauthorised,
    /// The account may not do this (403, a policy, eligibility, a billing set-up problem).
    Forbidden,
    /// The provider refused this request's shape, size or capability (400, 413 and the like).
    Rejected,
    /// The request never left: no connection was made. Nothing can have run.
    NotSent,
    /// The connection broke after the request was sent. Whether it ran is unknown.
    Dropped,
    /// The client went away.
    Cancelled,
}
