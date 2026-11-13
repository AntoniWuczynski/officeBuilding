//! What the interface may show about a routed session. Serialise-only, no request content.
//! Stage B adds the command and the TypeScript mirror: the JSON shape is fixed here.

use super::availability::Bucket;
use super::config::{ConnectionId, ConnectionKind, ContextId};
use super::policy::Skip;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteStatus {
    pub context: ContextId,
    pub generation: u64,
    pub active: Option<ConnectionId>,
    /// Since when no entry could be tried (Unix seconds). None: it is not blocked.
    pub blocked_since: Option<u64>,
    pub last_switch: Option<Switch>,
    pub last_served: Option<Served>,
    pub entries: Vec<EntryStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Switch {
    pub from: ConnectionId,
    pub to: ConnectionId,
    pub reason: SwitchReason,
    pub at: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SwitchReason {
    Exhausted,
    Manual,
}

/// The last request admitted: what was asked for and what it was sent as.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Served {
    pub connection: ConnectionId,
    pub requested_model: String,
    pub requested_effort: Option<String>,
    /// None: sent as requested.
    pub model: Option<String>,
    /// The effort sent.
    pub effort: Option<String>,
    pub at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryStatus {
    pub connection: ConnectionId,
    pub name: String,
    pub kind: ConnectionKind,
    pub state: EntryState,
    pub exhausted: Vec<ExhaustedScope>,
    pub problem: Option<Problem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum EntryState {
    Active,
    Ready,
    Unavailable { reason: Skip },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExhaustedScope {
    pub bucket: Bucket,
    pub window: Option<String>,
    pub observed_at: u64,
    pub reset_at: Option<u64>,
    /// A request is checking it now.
    pub checking: bool,
    /// A Human asked for a retry that has not run yet.
    pub retry_armed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    pub kind: ProblemKind,
    pub at: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProblemKind {
    Throttled,
    Faulted,
    Unauthorised,
    Forbidden,
    Rejected,
    Unreachable,
    Interrupted,
}
