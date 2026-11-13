//! Upstreams: where each connection kind is sent and which headers cross in each direction.
//!
//! The host comes from the lease's kind through `url`, and the headers from `outbound`, and from
//! nowhere else. The client's `authorization` reaches one host only: Anthropic's, for the Claude
//! subscription.

use super::config::ConnectionKind;
use super::wire::Header;
use crate::secrets::Secret;
use std::fmt;
use std::io::{ErrorKind, Read};
use std::time::Duration;

pub const ANTHROPIC: &str = "https://api.anthropic.com";
/// T13: the Anthropic-format base of `OpenRouter`. T14: Messages is `POST /messages` under `/api/v1`.
pub const OPENROUTER: &str = "https://openrouter.ai/api";
/// The header that carries the route secret between a desk and the router. It never leaves the Mac.
pub const LOCAL_HEADER: &str = "x-office-building-route";

/// Hop-by-hop headers: they belong to one connection and never cross the router.
const HOP: &[&str] = &[
    "connection",
    "keep-alive",
    "proxy-connection",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
];
/// Headers of the loopback connection. The HTTP client writes its own `host` and `content-length`.
const LOCAL: &[&str] = &["host", "content-length", "expect", LOCAL_HEADER];
const CREDENTIALS: &[&str] = &["authorization", "x-api-key", "cookie"];
/// What a key connection asks for, so its error body can be read as evidence (E7).
const IDENTITY: &str = "identity";

/// The two hosts. `Default` is the two constants. Only tests build another.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bases {
    pub anthropic: String,
    pub openrouter: String,
}

impl Default for Bases {
    fn default() -> Self {
        Self {
            anthropic: ANTHROPIC.to_owned(),
            openrouter: OPENROUTER.to_owned(),
        }
    }
}

/// Whether this stage has a relay for `kind`. Taking a kind out here takes it out of `url` and
/// `outbound` too.
#[must_use]
pub const fn relays(kind: ConnectionKind) -> bool {
    match kind {
        ConnectionKind::ClaudeSubscription
        | ConnectionKind::AnthropicApiKey
        | ConnectionKind::OpenRouter => true,
        ConnectionKind::OpenAiApiKey | ConnectionKind::ChatGptSignIn => false,
    }
}

/// The address a Messages call of `kind` is posted to: `<base>/v1/messages`, plus `?<query>` as
/// the client sent it.
#[must_use]
pub fn url(bases: &Bases, kind: ConnectionKind, query: Option<&str>) -> Option<String> {
    if !relays(kind) {
        return None;
    }
    let base = match kind {
        ConnectionKind::ClaudeSubscription | ConnectionKind::AnthropicApiKey => &bases.anthropic,
        ConnectionKind::OpenRouter => &bases.openrouter,
        ConnectionKind::OpenAiApiKey | ConnectionKind::ChatGptSignIn => return None,
    };
    Some(match query {
        Some(query) => format!("{base}/v1/messages?{query}"),
        None => format!("{base}/v1/messages"),
    })
}

/// The headers that go upstream for a connection of `kind`. None: no relay for that kind, or a
/// key kind with no key. `key` is ignored for the subscription.
#[must_use]
pub fn outbound(
    kind: ConnectionKind,
    client: &[Header],
    key: Option<&Secret>,
) -> Option<Vec<Header>> {
    if !relays(kind) {
        return None;
    }
    let kept = |keep: &dyn Fn(&str) -> bool| -> Vec<Header> {
        client
            .iter()
            .filter(|header| keep(&header.name))
            .cloned()
            .collect()
    };
    let crossing = |name: &str| !HOP.contains(&name) && !LOCAL.contains(&name);
    match kind {
        // Pass-through (T3, T4): the login and every other header go on untouched, nothing is
        // added. Of the credentials only the login crosses: a desk on the subscription sends no
        // key or cookie, and one that did would have it stopped here.
        ConnectionKind::ClaudeSubscription => Some(kept(&|name| {
            crossing(name) && (name == "authorization" || !CREDENTIALS.contains(&name))
        })),
        // E8, E9: everything but the client's credentials, then the key as `x-api-key` (T8).
        ConnectionKind::AnthropicApiKey => {
            let key = key?;
            let mut headers = kept(&|name| {
                crossing(name) && !CREDENTIALS.contains(&name) && name != "accept-encoding"
            });
            headers.push(header("x-api-key", key.expose().as_bytes()));
            headers.push(header("accept-encoding", IDENTITY.as_bytes()));
            Some(headers)
        }
        // E9: another vendor gets an allow-list, then the key as a bearer token (T13).
        ConnectionKind::OpenRouter => {
            let key = key?;
            let mut headers = kept(&|name| {
                matches!(name, "content-type" | "accept" | "user-agent" | "x-app")
                    || ["anthropic-", "x-claude-code-", "x-stainless-"]
                        .iter()
                        .any(|prefix| name.starts_with(prefix))
            });
            headers.push(header(
                "authorization",
                format!("Bearer {}", key.expose()).as_bytes(),
            ));
            headers.push(header("accept-encoding", IDENTITY.as_bytes()));
            Some(headers)
        }
        ConnectionKind::OpenAiApiKey | ConnectionKind::ChatGptSignIn => None,
    }
}

/// The upstream's headers as the client gets them: everything except `HOP` and `content-length`,
/// in order.
#[must_use]
pub fn returned(upstream: &[Header]) -> Vec<Header> {
    upstream
        .iter()
        .filter(|header| !HOP.contains(&header.name.as_str()) && header.name != "content-length")
        .cloned()
        .collect()
}

fn header(name: &str, value: &[u8]) -> Header {
    Header {
        name: name.to_owned(),
        value: value.to_vec(),
    }
}

/// An upstream's reply, its body not yet read. `Debug` prints the status and the header names.
pub struct Reply {
    pub status: u16,
    pub headers: Vec<Header>,
    pub body: Box<dyn Read + Send>,
}

impl fmt::Debug for Reply {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Reply")
            .field("status", &self.status)
            .field("headers", &self.headers)
            .finish_non_exhaustive()
    }
}

/// Why no reply came. No detail: an error text could name a URL or echo a header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendError {
    /// No connection was made. Nothing can have run.
    NotSent,
    /// It broke after the request may have left. Whether it ran is unknown.
    Dropped,
}

pub trait Upstream: Send + Sync {
    fn post(&self, url: &str, headers: &[Header], body: &[u8]) -> Result<Reply, SendError>;
}

/// The real upstream: one `ureq` agent for the app's life (T1).
pub struct UreqUpstream {
    agent: ureq::Agent,
}

impl Default for UreqUpstream {
    fn default() -> Self {
        Self::new()
    }
}

impl UreqUpstream {
    #[must_use]
    pub fn new() -> Self {
        Self::with_https_only(true)
    }

    /// The same agent without `https_only`, for a fake server on loopback.
    #[cfg(test)]
    pub(crate) fn plain() -> Self {
        Self::with_https_only(false)
    }

    /// T1: a 4xx or 5xx is a reply, no redirect is followed, no proxy from the environment, and
    /// no global timeout, which would keep running while a stream is read.
    fn with_https_only(https_only: bool) -> Self {
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .https_only(https_only)
            .max_redirects(0)
            .proxy(None)
            .timeout_global(None)
            .timeout_resolve(Some(Duration::from_secs(10)))
            .timeout_connect(Some(Duration::from_secs(15)))
            .timeout_recv_response(Some(Duration::from_secs(600)))
            .timeout_per_read(Some(Duration::from_secs(600)))
            .user_agent("office-building")
            .tls_config(crate::http::tls())
            .build()
            .into();
        Self { agent }
    }
}

impl Upstream for UreqUpstream {
    /// The body is never read, limited or decoded here.
    fn post(&self, url: &str, headers: &[Header], body: &[u8]) -> Result<Reply, SendError> {
        let request = headers
            .iter()
            .fold(self.agent.post(url), |request, header| {
                request.header(header.name.as_str(), header.value.as_slice())
            });
        let response = request.send(body).map_err(|error| send_error(&error))?;
        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .map(|(name, value)| header(name.as_str(), value.as_bytes()))
            .collect();
        Ok(Reply {
            status,
            headers,
            body: Box::new(response.into_body().into_reader()),
        })
    }
}

/// `NotSent` only for what can only happen before a request leaves, `Dropped` for everything
/// else. A failed connect and a dropped connection are both `Io`, told apart by kind only (T1).
/// The error is never formatted: some variants echo the URL.
fn send_error(error: &ureq::Error) -> SendError {
    match error {
        ureq::Error::HostNotFound
        | ureq::Error::ConnectionFailed
        | ureq::Error::BadUri(_)
        | ureq::Error::Http(_)
        | ureq::Error::InvalidProxyUrl
        | ureq::Error::Tls(_)
        | ureq::Error::NativeTls(_)
        | ureq::Error::TlsRequired
        | ureq::Error::RequireHttpsOnly(_)
        | ureq::Error::Timeout(ureq::Timeout::Resolve | ureq::Timeout::Connect) => {
            SendError::NotSent
        }
        ureq::Error::Io(io)
            if matches!(
                io.kind(),
                ErrorKind::ConnectionRefused
                    | ErrorKind::AddrNotAvailable
                    | ErrorKind::NetworkUnreachable
                    | ErrorKind::HostUnreachable
            ) =>
        {
            SendError::NotSent
        }
        _ => SendError::Dropped,
    }
}

/// What a `FakeUpstream` was sent.
#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Sent {
    pub url: String,
    pub headers: Vec<Header>,
    pub body: Vec<u8>,
}

/// A scripted reply.
#[cfg(test)]
pub(crate) struct Canned {
    pub status: u16,
    pub headers: Vec<(&'static str, String)>,
    pub body: Vec<Step>,
}

/// One `read` of a scripted body.
#[cfg(test)]
pub(crate) enum Step {
    Bytes(Vec<u8>),
    /// The read fails: the upstream dropped.
    Break,
    /// Block until the test lets go (or drops its sender), then go on with the next step.
    Hold(std::sync::mpsc::Receiver<()>),
}

#[cfg(test)]
type Script = Box<dyn Fn(&Sent) -> Result<Canned, SendError> + Send + Sync>;

/// A scripted upstream. Records what it was sent and answers from a closure. Never opens a socket.
#[cfg(test)]
pub(crate) struct FakeUpstream {
    sent: std::sync::Mutex<Vec<Sent>>,
    answer: Script,
}

#[cfg(test)]
impl FakeUpstream {
    pub(crate) fn new(
        answer: impl Fn(&Sent) -> Result<Canned, SendError> + Send + Sync + 'static,
    ) -> Self {
        Self {
            sent: std::sync::Mutex::new(Vec::new()),
            answer: Box::new(answer),
        }
    }

    /// Every call so far. Match on content, never on position.
    pub(crate) fn sent(&self) -> Vec<Sent> {
        crate::sync::lock(&self.sent).clone()
    }
}

#[cfg(test)]
impl Upstream for FakeUpstream {
    fn post(&self, url: &str, headers: &[Header], body: &[u8]) -> Result<Reply, SendError> {
        let sent = Sent {
            url: url.to_owned(),
            headers: headers.to_vec(),
            body: body.to_vec(),
        };
        crate::sync::lock(&self.sent).push(sent.clone());
        let canned = (self.answer)(&sent)?;
        Ok(Reply {
            status: canned.status,
            headers: canned
                .headers
                .iter()
                .map(|(name, value)| header(name, value.as_bytes()))
                .collect(),
            body: Box::new(Played {
                steps: canned.body.into(),
                left: Vec::new(),
            }),
        })
    }
}

/// Plays a script one step per `read`, and ends cleanly after the last.
#[cfg(test)]
struct Played {
    steps: std::collections::VecDeque<Step>,
    /// What a step longer than the caller's buffer left over.
    left: Vec<u8>,
}

#[cfg(test)]
impl Read for Played {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        while self.left.is_empty() {
            match self.steps.pop_front() {
                None => return Ok(0),
                Some(Step::Bytes(bytes)) => self.left = bytes,
                Some(Step::Break) => {
                    return Err(std::io::Error::new(
                        ErrorKind::ConnectionReset,
                        "the upstream dropped",
                    ));
                }
                Some(Step::Hold(gate)) => {
                    let _ = gate.recv();
                }
            }
        }
        let n = buf.len().min(self.left.len());
        buf[..n].copy_from_slice(&self.left[..n]);
        self.left.drain(..n);
        Ok(n)
    }
}

#[cfg(test)]
mod tests;
