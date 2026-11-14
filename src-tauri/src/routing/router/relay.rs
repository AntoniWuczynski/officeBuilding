//! One request from admission to the last byte: `impl serve::Handler for Router` (section 8).
//! Runs on the connection's own thread.
//!
//! The commit boundary: a 2xx is committed when its first body bytes are written. From then on
//! Stage A answers every failure with `Cut`, and the reply ends without its terminator. The
//! clean terminator is written only after a clean upstream end of a 2xx, or after a held refusal
//! was passed on whole (`send_body`, its one call site for both). A translated route's attempt is
//! `translated.rs`: its commit is the first translated byte (F1), and its refusals are answered in
//! Anthropic's envelope, never passed on as the provider's bytes (F2).

mod translated;

use super::{Routed, Router, now};
use crate::manager::auth::same;
use crate::routing::answers::{self, Answer};
use crate::routing::body::{self, Edit, Facts};
use crate::routing::capabilities::Requirements;
use crate::routing::config::{ConnectionId, ConnectionKind};
use crate::routing::evidence;
use crate::routing::observe::Observation;
use crate::routing::policy::{RouteError, Skip, Target};
use crate::routing::serve::Handler;
use crate::routing::state::{Lease, LocalFault, RouteId, Verdict};
use crate::routing::translate::sse::read_some;
use crate::routing::upstream::{self, LOCAL_HEADER, Reply, SendError};
use crate::routing::wire::{self, Header, Method, Request};
use crate::sync::lock;
use std::borrow::Cow;
use std::collections::BTreeSet;
use std::io::Read;
use std::net::TcpStream;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// The longest route id `admit` looks up.
const ID_MAX: usize = 64;
/// Compared with when the route is unknown, so an unknown route costs what a known one does.
const DUMMY: &[u8; 64] = &[b'0'; 64];
/// One read of a reply body, and so one chunk to the client at most.
const CHUNK: usize = 16 * 1024;
/// What is read of a refusal's body for evidence (E19).
const ERROR_MAX: u64 = 64 * 1024;

impl Handler for Router {
    fn admits(&self, head: &Request) -> bool {
        is_hello(head) || self.admit(head).is_some()
    }

    fn handle(&self, request: Request, stream: &mut TcpStream) {
        // E3: the warming probe carries no credentials, and its answer is the same for everyone.
        if is_hello(&request) {
            let _unsent = wire::write_local(stream, 200, &[], b"");
            return;
        }
        // Admission before the path, so a caller without the secret learns nothing about paths.
        let Some((id, routed, rest)) = self.admit(&request) else {
            let _unsent = answers::unknown_desk().write(stream);
            return;
        };
        // T5: match on the path, not the query.
        if request.method != Method::Post || rest != "/v1/messages" {
            let _unsent = answers::other_path().write(stream);
            return;
        }
        self.messages(&id, &routed, &request, stream);
    }
}

fn is_hello(request: &Request) -> bool {
    request.method == Method::Head && request.path.ends_with("/api/hello")
}

/// Split `/s/<id>/<rest>` into the id and `/<rest>`.
fn split(path: &str) -> Option<(&str, &str)> {
    let after = path.strip_prefix("/s/")?;
    let slash = after.find('/')?;
    let (id, rest) = after.split_at(slash);
    let valid = (1..=ID_MAX).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    valid.then_some((id, rest))
}

/// A lease that is settled exactly once: by `settle`, or as `Cancelled` when the guard is
/// dropped still holding one, so a thread that unwinds or returns early still gives back its
/// lease and any probe ticket.
struct Open<'a> {
    router: &'a Router,
    lease: Option<Lease>,
    credentials: BTreeSet<ConnectionId>,
}

impl Open<'_> {
    /// Settle the lease held. An `Again` verdict's new lease is held in its place.
    fn settle(&mut self, seen: &Observation, relayed: bool) -> Verdict {
        let Some(lease) = self.lease.take() else {
            return Verdict::Done;
        };
        let now = now();
        let credentials = &self.credentials;
        let verdict = self
            .router
            .with_state(|s| s.settle(lease, seen, relayed, credentials, now));
        if let Verdict::Again { lease, .. } = &verdict {
            self.lease = Some(lease.clone());
        }
        verdict
    }
}

impl Drop for Open<'_> {
    fn drop(&mut self) {
        self.settle(&Observation::Cancelled, false);
    }
}

/// Why nothing left the Mac.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Why {
    /// The key could not be read for the call.
    NoKey,
    /// No relay for the kind.
    NoRelay,
    /// The body could not be edited.
    Unreadable,
    /// No connection was made.
    Connect,
}

/// What one attempt came to (section 8.3).
enum Outcome {
    /// A reply the client has not seen any of: its status, its headers, what was read of its
    /// body, and the rest of the body if the cap was reached first.
    Held {
        status: u16,
        headers: Vec<Header>,
        read: Vec<u8>,
        rest: Option<Box<dyn Read + Send>>,
    },
    /// A 2xx relayed to its clean end.
    Relayed,
    /// Reply bytes reached the client, then the upstream broke.
    Broken,
    /// Reply bytes were being written and the client went away.
    ClientLeft,
    /// Nothing left the Mac.
    Unsent(Why),
    /// It broke after the request may have left, with nothing relayed.
    Lost,
    /// The request was refused here, after its lease was cut. Nothing was sent (F3).
    Refused(Answer),
    /// A translated route's reply could not be passed on, and none of it reached the client.
    Failed { seen: Observation, answer: Answer },
    /// Translated bytes reached the client, then the provider reported a failure.
    Stopped(Observation),
}

impl Outcome {
    /// What the outcome proves, and whether reply bytes reached the client.
    fn proves(&self, kind: ConnectionKind) -> (Observation, bool) {
        match self {
            Self::Held {
                status,
                headers,
                read,
                ..
            } => (evidence::observe(kind, *status, headers, read), false),
            Self::Relayed => (Observation::Completed, true),
            Self::Broken => (Observation::Dropped, true),
            Self::ClientLeft => (Observation::Cancelled, true),
            Self::Unsent(_) => (Observation::NotSent, false),
            Self::Lost => (Observation::Dropped, false),
            Self::Refused(_) => (Observation::Cancelled, false),
            Self::Failed { seen, .. } => (seen.clone(), false),
            Self::Stopped(seen) => (seen.clone(), true),
        }
    }
}

/// Where a body copy stopped.
enum Broke {
    Upstream,
    Client,
}

/// `first` as a chunk, then `rest` read by read, one chunk per read, then the clean terminator.
/// A break on either side returns before the terminator.
fn send_body(
    stream: &mut TcpStream,
    first: &[u8],
    rest: Option<&mut dyn Read>,
) -> Result<(), Broke> {
    wire::write_chunk(stream, first).map_err(|_| Broke::Client)?;
    if let Some(rest) = rest {
        let mut buf = vec![0_u8; CHUNK];
        loop {
            match read_some(rest, &mut buf) {
                Ok(0) => break,
                Ok(read) => wire::write_chunk(stream, &buf[..read]).map_err(|_| Broke::Client)?,
                Err(_) => return Err(Broke::Upstream),
            }
        }
    }
    wire::write_end(stream).map_err(|_| Broke::Client)
}

/// Pass a held reply on as it came (section 8.4). A `rest` that breaks ends the reply without
/// its terminator.
fn pass_on(
    stream: &mut TcpStream,
    status: u16,
    headers: &[Header],
    read: &[u8],
    rest: Option<Box<dyn Read + Send>>,
) {
    if wire::write_head(stream, status, &upstream::returned(headers)).is_err() {
        return;
    }
    let mut rest = rest;
    let rest = rest.as_mut().map(|rest| -> &mut dyn Read { rest.as_mut() });
    let _ended = send_body(stream, read, rest);
}

/// Read a refusal's body up to `ERROR_MAX`, without writing anything to the client.
fn hold(reply: Reply) -> Outcome {
    let Reply {
        status,
        headers,
        mut body,
    } = reply;
    let mut read = Vec::new();
    if (&mut body).take(ERROR_MAX).read_to_end(&mut read).is_err() {
        return Outcome::Lost;
    }
    let full = u64::try_from(read.len()).is_ok_and(|len| len >= ERROR_MAX);
    Outcome::Held {
        status,
        headers,
        read,
        rest: full.then_some(body),
    }
}

impl Router {
    /// The session a request belongs to, by the id in its path and its secret, compared in
    /// constant time. An unknown route, a missing header, a header sent twice and a wrong secret
    /// all give None, so the caller answers them alike. The secret is looked up by the id,
    /// never searched for.
    fn admit(&self, request: &Request) -> Option<(RouteId, Arc<Routed>, String)> {
        let (id, rest) = split(&request.path)?;
        let id = RouteId::new(id.to_owned());
        let routed = self.routed(&id);
        let presented = request.only(LOCAL_HEADER);
        let expected: &[u8] = routed
            .as_ref()
            .map_or(DUMMY, |routed| routed.secret.expose().as_bytes());
        let matched = same(expected, presented.unwrap_or(DUMMY));
        match (routed, presented) {
            (Some(routed), Some(_)) if matched => Some((id, routed, rest.to_owned())),
            _ => None,
        }
    }

    /// The Messages call (section 8.2). An error from `begin` means no upstream call.
    fn messages(&self, id: &RouteId, routed: &Routed, request: &Request, stream: &mut TcpStream) {
        let Ok(facts) = body::read(&request.body) else {
            let _unsent = answers::unreadable().write(stream);
            return;
        };
        let Some(requirements) = Requirements::new(
            &facts.model,
            facts.effort.as_deref(),
            facts.capabilities.clone(),
        ) else {
            let _unsent = answers::unreadable().write(stream);
            return;
        };
        // The Keychain may block: no lock is held.
        let credentials = self.credentials(&routed.chain);
        lock(&routed.live).requests += 1;
        let now = now();
        let begun = self.with_state(|s| s.begin(id, requirements, &credentials, now));
        let lease = match begun {
            Ok(lease) => lease,
            Err(error) => {
                let _unsent = refusal(routed, &error).write(stream);
                return;
            }
        };
        let mut open = Open {
            router: self,
            lease: Some(lease),
            credentials,
        };
        loop {
            let Some(lease) = open.lease.as_ref() else {
                return;
            };
            let (connection, kind) = (lease.connection.clone(), lease.kind);
            // E17: a request nobody waits for is not sent, nor sent again.
            if wire::client_gone(stream) {
                open.settle(&Observation::Cancelled, false);
                return;
            }
            let outcome = self.attempt(routed, request, &facts, lease, stream);
            let (seen, relayed) = outcome.proves(kind);
            // E12: a redirect is never followed, never passed on and never asked again.
            let redirect = matches!(
                outcome,
                Outcome::Held {
                    status: 300..=399,
                    ..
                }
            );
            let verdict = open.settle(&seen, relayed);
            // F3: settled as `Cancelled`, so the verdict is `Done`, and the refusal is the answer.
            if let Outcome::Refused(answer) = outcome {
                let _unsent = answer.write(stream);
                return;
            }
            let name = routed
                .link(&connection)
                .map_or(connection.as_str(), |link| link.name.as_str());
            let answer = match verdict {
                Verdict::Done | Verdict::Cut => return,
                // The guard settles a new lease of `Again` as cancelled on the way out.
                _ if redirect => answers::redirect(name),
                Verdict::Again { wait_ms, .. } => {
                    thread::sleep(Duration::from_millis(wait_ms));
                    continue;
                }
                Verdict::Relay => match outcome {
                    Outcome::Held {
                        status,
                        headers,
                        read,
                        rest,
                    } => {
                        pass_on(stream, status, &headers, &read, rest);
                        return;
                    }
                    // F2: a translated route's refusal, in Anthropic's envelope.
                    Outcome::Failed { answer, .. } => answer,
                    // Stage A gives `Relay` only for a reply. Never seen, but answered truthfully.
                    _ => answers::unreachable(name),
                },
                Verdict::Local(LocalFault::Unreachable) => match outcome {
                    Outcome::Unsent(Why::NoKey) => answers::no_key(name),
                    _ => answers::unreachable(name),
                },
                Verdict::Local(LocalFault::Interrupted) => match outcome {
                    Outcome::Failed { answer, .. } => answer,
                    _ => answers::interrupted(name),
                },
            };
            let _unsent = answer.write(stream);
            return;
        }
    }

    /// One attempt (section 8.3). The host comes only from `upstream::url` and the headers only
    /// from `upstream::outbound`, both by the lease's kind.
    fn attempt(
        &self,
        routed: &Routed,
        request: &Request,
        facts: &Facts,
        lease: &Lease,
        stream: &mut TcpStream,
    ) -> Outcome {
        if lease.kind.translated() {
            return self.attempt_translated(routed, request, facts, lease, stream);
        }
        let kind = lease.kind;
        let Some(url) = upstream::url(&self.bases, kind, request.query.as_deref()) else {
            return Outcome::Unsent(Why::NoRelay);
        };
        // The key lives for this one call.
        let key = if kind.holds_key() {
            match self.keys.get(kind, &lease.connection) {
                Ok(Some(key)) => Some(key),
                Ok(None) | Err(_) => return Outcome::Unsent(Why::NoKey),
            }
        } else {
            None
        };
        let Some(headers) = upstream::outbound(kind, &request.headers, key.as_ref()) else {
            return Outcome::Unsent(Why::NoRelay);
        };
        drop(key);
        let sent: Cow<'_, [u8]> = if kind.passes_through() {
            Cow::Borrowed(&request.body)
        } else {
            let (model, effort) = match &lease.target {
                Target::AsSent => (None, None),
                Target::Rewrite { model, effort } => (
                    Some(model.as_str()).filter(|model| *model != facts.model),
                    effort
                        .as_deref()
                        .filter(|effort| Some(*effort) != facts.effort.as_deref()),
                ),
            };
            let edit = Edit {
                model,
                effort,
                drop_thinking: kind == ConnectionKind::OpenRouter
                    && foreign(routed, &lease.connection),
                // E10.
                pin_provider: kind == ConnectionKind::OpenRouter,
            };
            match body::rewrite(&request.body, &edit) {
                Ok(sent) => sent,
                Err(_) => return Outcome::Unsent(Why::Unreadable),
            }
        };
        let reply = match self.upstream.post(&url, &headers, &sent) {
            Ok(reply) => reply,
            Err(SendError::NotSent) => return Outcome::Unsent(Why::Connect),
            Err(SendError::Dropped) => return Outcome::Lost,
        };
        if (200..300).contains(&reply.status) {
            relay_success(routed, &lease.connection, reply, stream)
        } else {
            hold(reply)
        }
    }
}

/// Section 9: the desk's history may hold another connection's reasoning. `spoke` only grows,
/// so once this holds for a desk it holds for good.
fn foreign(routed: &Routed, connection: &ConnectionId) -> bool {
    routed.resumed || lock(&routed.live).spoke.iter().any(|c| c != connection)
}

/// A 2xx (E5, E6): the first bytes are read before the head is written, so one that dies before
/// any body byte can still be answered locally. Its body is never parsed.
fn relay_success(
    routed: &Routed,
    connection: &ConnectionId,
    reply: Reply,
    stream: &mut TcpStream,
) -> Outcome {
    let Reply {
        status,
        headers,
        mut body,
    } = reply;
    let mut first = vec![0_u8; CHUNK];
    let Ok(read) = read_some(&mut body, &mut first) else {
        return Outcome::Lost;
    };
    if wire::write_head(stream, status, &upstream::returned(&headers)).is_err() {
        return Outcome::ClientLeft;
    }
    lock(&routed.live).spoke.insert(connection.clone());
    let rest: Option<&mut dyn Read> = (read > 0).then_some(&mut body);
    match send_body(stream, &first[..read], rest) {
        Ok(()) => Outcome::Relayed,
        Err(Broke::Upstream) => Outcome::Broken,
        Err(Broke::Client) => Outcome::ClientLeft,
    }
}

/// The local answer to an error from `begin`. Names and kinds come from the session's own
/// order. Every link exists (a session's order only narrows), so an entry without one is left out.
fn refusal(routed: &Routed, error: &RouteError) -> Answer {
    match error {
        RouteError::Refused { connection, reason } => match routed.link(connection) {
            Some(link) => answers::refused(&link.name, link.kind, reason),
            None => answers::blocked(&[]),
        },
        RouteError::Blocked { reasons } => {
            let entries: Vec<(&str, ConnectionKind, &Skip)> = reasons
                .iter()
                .filter_map(|(connection, skip)| {
                    routed
                        .link(connection)
                        .map(|link| (link.name.as_str(), link.kind, skip))
                })
                .collect();
            answers::blocked(&entries)
        }
        RouteError::UnknownSession | RouteError::NotInChain => answers::unknown_desk(),
    }
}

#[cfg(test)]
mod guard_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod translated_guard_tests;
#[cfg(test)]
mod translated_tests;
