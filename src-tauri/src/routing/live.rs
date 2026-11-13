//! What a route command returns: a routed desk's route as the interface may show it
//! (`DeskRoute` in types.ts). No request content, no secret.

use super::config::{ConnectionId, ConnectionKind};
use super::status::RouteStatus;
use serde::Serialize;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeskRoute {
    /// Stage A's status, unchanged.
    pub status: RouteStatus,
    /// What became of hidden reasoning when the desk changed connection.
    pub reasoning: Reasoning,
    /// Messages requests of this desk the router has taken in. Zero means nothing has arrived.
    pub requests: u64,
    /// Why the saved exhaustion records could not be read or written, if so.
    pub records_problem: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Reasoning {
    /// Nothing has been dropped: one connection has answered this desk.
    Intact,
    /// It moved, and its requests go to Anthropic whole: Anthropic leaves out what it cannot read.
    VendorMayDrop,
    /// The router removes hidden reasoning from every request it sends for this desk. On a
    /// translated route none is sent and none is kept from a reply, whether or not the desk
    /// ever changed connection.
    Removed,
}

impl Reasoning {
    /// `spoke`: the connections whose reply bytes have reached the desk. `active`: the session's
    /// active connection and its kind.
    #[must_use]
    pub fn of(
        resumed: bool,
        spoke: &BTreeSet<ConnectionId>,
        active: Option<(&ConnectionId, ConnectionKind)>,
    ) -> Self {
        match active {
            Some((_, kind)) if kind.translated() => Self::Removed,
            Some((connection, ConnectionKind::OpenRouter))
                if resumed || spoke.iter().any(|c| c != connection) =>
            {
                Self::Removed
            }
            _ if spoke.len() > 1 => Self::VendorMayDrop,
            _ => Self::Intact,
        }
    }
}
