//! One attempt on a translated route (section 9.3): the desk's Messages request read strictly,
//! rebuilt as a Responses request, sent once to `OpenAI` with the connection's key, and the reply
//! stream turned back into Anthropic's through `pump`. The provider's own bytes never reach the
//! client: a refusal is put into Anthropic's envelope (F2), and nothing is written before the
//! first translated bytes (F1). The clean terminator is written only by the sink's `close`.

use super::super::{Link, Routed, Router};
use super::{Outcome, Why, hold};
use crate::routing::answers;
use crate::routing::body::Facts;
use crate::routing::config::{ConnectionId, ConnectionKind, model_name};
use crate::routing::evidence::{self, Said};
use crate::routing::policy::{Skip, Target};
use crate::routing::state::Lease;
use crate::routing::translate::messages::{self, Asked};
use crate::routing::translate::pump::{Ended, Sink, pump};
use crate::routing::translate::{responses, sse};
use crate::routing::upstream::{self, Reply, SendError};
use crate::routing::wire::{self, Header, Request};
use crate::sync::lock;
use std::io;
use std::net::TcpStream;

/// The client's socket as `pump` writes to it.
struct Client<'a> {
    stream: &'a mut TcpStream,
    head: Vec<Header>,
    routed: &'a Routed,
    connection: &'a ConnectionId,
}

impl Sink for Client<'_> {
    fn open(&mut self) -> io::Result<()> {
        wire::write_head(self.stream, 200, &self.head)?;
        lock(&self.routed.live)
            .spoke
            .insert(self.connection.clone());
        Ok(())
    }

    fn send(&mut self, bytes: &[u8]) -> io::Result<()> {
        wire::write_chunk(self.stream, bytes)
    }

    fn close(&mut self) -> io::Result<()> {
        wire::write_end(self.stream)
    }
}

impl Router {
    /// One upstream call at most. The first step that fails is the outcome.
    pub(super) fn attempt_translated(
        &self,
        routed: &Routed,
        request: &Request,
        facts: &Facts,
        lease: &Lease,
        stream: &mut TcpStream,
    ) -> Outcome {
        let kind = lease.kind;
        let (Some(link), Target::Rewrite { model, .. }) =
            (routed.link(&lease.connection), &lease.target)
        else {
            return Outcome::Unsent(Why::NoRelay);
        };
        // Section 10.1: the client's query is not passed on.
        let Some(url) = upstream::url(&self.bases, kind, request.query.as_deref()) else {
            return Outcome::Unsent(Why::NoRelay);
        };
        let asked = match messages::read(&request.body) {
            Ok(asked) => asked,
            Err(carry) => return Outcome::Refused(answers::cannot_carry(&link.name, &carry)),
        };
        let effort = match effort(routed, link, facts, &asked) {
            Ok(effort) => effort,
            Err(outcome) => return outcome,
        };
        let body = match responses::build(&asked, model, effort) {
            Ok(body) => body,
            Err(carry) => return Outcome::Refused(answers::cannot_carry(&link.name, &carry)),
        };
        // The key lives for this one call.
        let Ok(Some(key)) = self.keys.get(kind, &lease.connection) else {
            return Outcome::Unsent(Why::NoKey);
        };
        let Some(headers) = upstream::outbound(kind, &request.headers, Some(&key)) else {
            return Outcome::Unsent(Why::NoRelay);
        };
        drop(key);
        let reply = match self.upstream.post(&url, &headers, &body) {
            Ok(reply) => reply,
            Err(SendError::NotSent) => return Outcome::Unsent(Why::Connect),
            Err(SendError::Dropped) => return Outcome::Lost,
        };
        if (200..300).contains(&reply.status) {
            return relay(routed, link, kind, reply, stream);
        }
        // A 3xx stays `Held`, so Stage B's redirect rule answers it (E12).
        match hold(reply) {
            Outcome::Held {
                status,
                headers,
                read,
                ..
            } if !(300..400).contains(&status) => {
                let seen = evidence::observe(kind, status, &headers, &read);
                let said = evidence::said(kind, &read);
                let answer = answers::provider_refused(&link.name, Some(status), &seen, &said);
                Outcome::Failed { seen, answer }
            }
            other => other,
        }
    }
}

/// The effort to send (F12): the one in force for this reply, mapped by the connection's own
/// table for the request's tier. Err: the outcome, a refusal when the effort has no pair.
fn effort<'l>(
    routed: &Routed,
    link: &'l Link,
    facts: &Facts,
    asked: &Asked<'_>,
) -> Result<Option<&'l str>, Outcome> {
    let Some(name) = asked.effort.as_deref() else {
        return Ok(None);
    };
    let tier = model_name(&facts.model).and_then(|model| routed.tiers.tier_of(&model));
    let Some((tier, target)) =
        tier.and_then(|tier| link.targets.get(tier).as_ref().map(|t| (tier, t)))
    else {
        return Err(Outcome::Unsent(Why::NoRelay));
    };
    target
        .efforts
        .get(name)
        .map(|e| Some(e.as_str()))
        .ok_or_else(|| {
            let skip = Skip::EffortUnmapped {
                tier,
                effort: name.to_owned(),
            };
            Outcome::Refused(answers::refused(&link.name, link.kind, &skip))
        })
}

/// A 2xx: pumped to the client, nothing written before the first translated bytes.
fn relay(
    routed: &Routed,
    link: &Link,
    kind: ConnectionKind,
    reply: Reply,
    stream: &mut TcpStream,
) -> Outcome {
    let Reply { headers, body, .. } = reply;
    let mut client = Client {
        stream,
        head: upstream::translated(&headers),
        routed,
        connection: &link.connection,
    };
    match pump(body, &mut client) {
        Ended::Clean => Outcome::Relayed,
        Ended::Failed {
            failure,
            committed: false,
        } => {
            let seen = evidence::stream(kind, &failure);
            let answer = answers::provider_refused(&link.name, None, &seen, &Said::of(&failure));
            Outcome::Failed { seen, answer }
        }
        Ended::Failed {
            failure,
            committed: true,
        } => {
            stopped(client.stream, &link.name, &Said::of(&failure));
            Outcome::Stopped(evidence::stream(kind, &failure))
        }
        Ended::Broke { committed: false } => Outcome::Lost,
        Ended::Broke { committed: true } => {
            stopped(client.stream, &link.name, &Said::default());
            Outcome::Broken
        }
        Ended::ClientLeft => Outcome::ClientLeft,
    }
}

/// One best-effort `error` event after translated bytes went out. No terminator follows, so the
/// client never takes the reply for a whole one.
fn stopped(stream: &mut TcpStream, name: &str, said: &Said) {
    let event = sse::frame("error", &answers::stream_stopped(name, said).body());
    let _unsent = wire::write_chunk(stream, &event);
}
