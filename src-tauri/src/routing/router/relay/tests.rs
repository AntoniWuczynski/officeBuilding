//! The Messages relay over a real loopback connection to the router's own listener, with a
//! `FakeUpstream` behind it. No network, no Keychain, no agent session.

use super::super::{Opened, Router, now};
use crate::routing::answers;
use crate::routing::availability::{Bucket, ExhaustionRecord};
use crate::routing::config::{ConnectionKind, TierId};
use crate::routing::fixtures::{conn, ctx, settings, temp_path};
use crate::routing::keys::Keys;
use crate::routing::live::{DeskRoute, Reasoning};
use crate::routing::records::{ExhaustionFile, Saved};
use crate::routing::status::{ProblemKind, SwitchReason};
use crate::routing::upstream::{
    self, Bases, Canned, FakeUpstream, OPENROUTER, SendError, Sent, Step, Upstream,
};
use crate::secrets::MemorySecrets;
use crate::settings::RoutingSettings;
use crate::sync::lock;
use std::fmt::Write as _;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub(super) const OPUS: &str = "claude-opus-5-5";
pub(super) const LOGIN: &str = "Bearer test-login";
pub(super) const ANTHROPIC_KEY: &str = "test-anthropic-key";
pub(super) const OPENROUTER_KEY: &str = "test-openrouter-key";

/// What a fake call went to, told by its content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Who {
    Sub,
    Api,
    Router,
}

pub(super) fn who(sent: &Sent) -> Who {
    if sent.url.starts_with(OPENROUTER) {
        Who::Router
    } else if sent.headers.iter().any(|h| h.name == "x-api-key") {
        Who::Api
    } else {
        Who::Sub
    }
}

pub(super) fn has(sent: &Sent, marker: &str) -> bool {
    String::from_utf8_lossy(&sent.body).contains(marker)
}

pub(super) fn canned(status: u16, headers: Vec<(&'static str, String)>, body: &str) -> Canned {
    Canned {
        status,
        headers,
        body: vec![Step::Bytes(body.as_bytes().to_vec())],
    }
}

pub(super) fn ok() -> Canned {
    canned(
        200,
        vec![("content-type", "text/event-stream".to_owned())],
        "event: message_stop\n\n",
    )
}

/// The subscription's used-up plan limit, as the probe replayed it (T2).
pub(super) fn rejected() -> Canned {
    canned(
        429,
        vec![
            ("anthropic-ratelimit-unified-status", "rejected".to_owned()),
            (
                "anthropic-ratelimit-unified-reset",
                (now() + 3600).to_string(),
            ),
            (
                "anthropic-ratelimit-unified-representative-claim",
                "five_hour".to_owned(),
            ),
        ],
        r#"{"type":"error","error":{"type":"rate_limit_error","message":"limit"}}"#,
    )
}

/// T9's tier spend cap.
pub(super) fn spend_cap() -> Canned {
    canned(
        429,
        vec![],
        r#"{"type":"error","error":{"type":"rate_limit_error","message":"cap","details":{"error_code":"enforced_spend_limit_reached"}},"request_id":"req_test"}"#,
    )
}

/// T17's used-up key limit.
pub(super) fn key_limit() -> Canned {
    canned(
        402,
        vec![("x-row", "last".to_owned())],
        r#"{"error":{"code":402,"message":"limit","metadata":{"limit_source":"openrouter_key_limit"}}}"#,
    )
}

/// Every connection of the chain exhausted in turn.
pub(super) fn all_exhausted(sent: &Sent) -> Canned {
    match who(sent) {
        Who::Sub => rejected(),
        Who::Api => spend_cap(),
        Who::Router => key_limit(),
    }
}

/// Gates the fake holds a body on. Dropping the senders lets every held read go on.
#[derive(Clone, Default)]
pub(super) struct Gates(Arc<Mutex<Vec<Sender<()>>>>);

impl Gates {
    pub(super) fn step(&self) -> Step {
        let (tx, rx) = mpsc::channel();
        lock(&self.0).push(tx);
        Step::Hold(rx)
    }

    pub(super) fn open(&self) {
        lock(&self.0).clear();
    }
}

pub(super) fn wait_for(what: &str, done: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        thread::sleep(Duration::from_millis(10));
    }
}

/// A Messages body: Opus at high effort, `marker` as the one user message.
pub(super) fn body_of(marker: &str) -> String {
    format!(
        r#"{{"model":"{OPUS}","max_tokens":64,"messages":[{{"role":"user","content":"{marker}"}}],"output_config":{{"effort":"high"}}}}"#
    )
}

/// What a test sets up besides the script.
pub(super) struct Setup {
    pub settings: RoutingSettings,
    pub records: Vec<ExhaustionRecord>,
    pub resumed: bool,
}

impl Setup {
    /// The fixture settings with the `work` context's order replaced.
    pub(super) fn chain(chain: &[&str]) -> Self {
        let mut s = settings();
        s.contexts[0].chain = chain.iter().map(|id| conn(id)).collect();
        Self {
            settings: s,
            records: Vec::new(),
            resumed: false,
        }
    }
}

pub(super) struct Rig {
    pub router: Arc<Router>,
    pub keys: Arc<Keys>,
    pub fake: Arc<FakeUpstream>,
    pub path: PathBuf,
    pub opened: Opened,
    pub settings: RoutingSettings,
}

impl Rig {
    pub(super) fn new(
        name: &str,
        chain: &[&str],
        script: impl Fn(&Sent) -> Result<Canned, SendError> + Send + Sync + 'static,
    ) -> Self {
        Self::with(name, Setup::chain(chain), script)
    }

    /// A router with the three keys saved and one session open on the `work` context.
    pub(super) fn with(
        name: &str,
        setup: Setup,
        script: impl Fn(&Sent) -> Result<Canned, SendError> + Send + Sync + 'static,
    ) -> Self {
        let path = temp_path("relay", name);
        if !setup.records.is_empty() {
            ExhaustionFile::new(path.clone())
                .save(&Saved {
                    revision: 1,
                    records: setup.records,
                })
                .expect("records written");
        }
        let keys = Arc::new(Keys::new(Arc::new(MemorySecrets::new())));
        for (id, kind, key) in [
            ("api", ConnectionKind::AnthropicApiKey, ANTHROPIC_KEY),
            ("router", ConnectionKind::OpenRouter, OPENROUTER_KEY),
            ("oai", ConnectionKind::OpenAiApiKey, "test-openai-key"),
        ] {
            keys.set(kind, &conn(id), Some(key.to_owned()), || true)
                .expect("key saved");
        }
        let fake = Arc::new(FakeUpstream::new(script));
        let upstream: Arc<dyn Upstream> = Arc::<FakeUpstream>::clone(&fake);
        let router = Router::new(
            ExhaustionFile::new(path.clone()),
            Arc::clone(&keys),
            upstream,
            Bases::default(),
        );
        let opened = router
            .open(&setup.settings, &ctx("work"), setup.resumed)
            .expect("opens")
            .expect("routed");
        Self {
            router,
            keys,
            fake,
            path,
            opened,
            settings: setup.settings,
        }
    }

    pub(super) fn port(&self) -> u16 {
        lock(&self.router.port).expect("listening")
    }

    pub(super) fn ask(&self, body: &str) -> Got {
        self.ask_with(body, &[])
    }

    pub(super) fn ask_with(&self, body: &str, extra: &[(&str, &str)]) -> Got {
        send(self.port(), &self.opened, body, extra)
    }

    /// Start a request on its own thread.
    pub(super) fn ask_later(&self, body: &str) -> thread::JoinHandle<Got> {
        let (port, opened, body) = (self.port(), self.opened.clone(), body.to_owned());
        thread::spawn(move || send(port, &opened, &body, &[]))
    }

    pub(super) fn view(&self) -> DeskRoute {
        self.router.view(&self.opened.id).expect("routed")
    }

    pub(super) fn sent(&self) -> Vec<Sent> {
        self.fake.sent()
    }

    pub(super) fn calls(&self, to: Who) -> usize {
        self.sent().iter().filter(|s| who(s) == to).count()
    }

    pub(super) fn records(&self) -> Vec<ExhaustionRecord> {
        ExhaustionFile::new(self.path.clone()).load().records
    }

    pub(super) fn cleanup(&self) {
        let _ = fs::remove_dir_all(self.path.parent().expect("dir"));
    }
}

/// One reply as the client read it.
#[derive(Debug)]
pub(super) struct Got {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// A chunked body ended with its clean terminator (always true for a sized one).
    pub ended: bool,
}

impl Got {
    pub(super) fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }
}

/// The headers Claude Code sends (T2), with the made-up login.
pub(super) fn client_headers(opened: &Opened) -> Vec<(String, String)> {
    let (name, secret) = opened
        .header
        .expose()
        .split_once(": ")
        .expect("name: value");
    [
        ("authorization", LOGIN),
        ("anthropic-version", "2023-06-01"),
        ("anthropic-beta", "oauth-2025-04-20"),
        ("content-type", "application/json"),
        ("x-claude-code-session-id", "session-1"),
        ("accept-encoding", "gzip, br"),
        (name, secret),
    ]
    .iter()
    .map(|(n, v)| ((*n).to_owned(), (*v).to_owned()))
    .collect()
}

/// One Messages request on a fresh connection, read to the router's close.
pub(super) fn send(port: u16, opened: &Opened, body: &str, extra: &[(&str, &str)]) -> Got {
    let mut stream = connect(port, opened, body, extra);
    let mut reply = Vec::new();
    stream.read_to_end(&mut reply).expect("read");
    parse(&reply)
}

/// The request written, the reply not read.
pub(super) fn connect(port: u16, opened: &Opened, body: &str, extra: &[(&str, &str)]) -> TcpStream {
    let path = opened
        .base_url
        .split_once(&format!("127.0.0.1:{port}"))
        .map(|(_, path)| path.to_owned())
        .expect("base path");
    let mut lines = String::new();
    let extra = extra
        .iter()
        .map(|(n, v)| ((*n).to_owned(), (*v).to_owned()));
    for (name, value) in client_headers(opened).into_iter().chain(extra) {
        let _ = write!(lines, "{name}: {value}\r\n");
    }
    let text = format!(
        "POST {path}/v1/messages?beta=true HTTP/1.1\r\nhost: 127.0.0.1:{port}\r\n{lines}content-length: {}\r\n\r\n{body}",
        body.len()
    );
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .expect("timeout");
    stream.write_all(text.as_bytes()).expect("write");
    stream
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

pub(super) fn parse(reply: &[u8]) -> Got {
    let split = find(reply, b"\r\n\r\n").expect("a head");
    let head = std::str::from_utf8(&reply[..split]).expect("text head");
    let mut lines = head.split("\r\n");
    let status = lines.next().expect("status line")[9..12]
        .parse()
        .expect("status");
    let headers: Vec<(String, String)> = lines
        .map(|line| {
            let (n, v) = line.split_once(": ").expect("header");
            (n.to_ascii_lowercase(), v.to_owned())
        })
        .collect();
    let rest = &reply[split + 4..];
    let chunked = headers
        .iter()
        .any(|(n, v)| n == "transfer-encoding" && v == "chunked");
    if !chunked {
        return Got {
            status,
            headers,
            body: rest.to_vec(),
            ended: true,
        };
    }
    let (mut body, mut at, mut ended) = (Vec::new(), rest, false);
    while let Some(eol) = find(at, b"\r\n") {
        let Ok(size) = usize::from_str_radix(std::str::from_utf8(&at[..eol]).unwrap_or("x"), 16)
        else {
            break;
        };
        at = &at[eol + 2..];
        if size == 0 {
            ended = at == b"\r\n";
            break;
        }
        if at.len() < size + 2 {
            body.extend_from_slice(&at[..size.min(at.len())]);
            break;
        }
        body.extend_from_slice(&at[..size]);
        at = &at[size + 2..];
    }
    Got {
        status,
        headers,
        body,
        ended,
    }
}

fn record(reset_at: Option<u64>) -> ExhaustionRecord {
    ExhaustionRecord {
        connection: conn("sub"),
        bucket: Bucket::Tier { tier: TierId::Opus },
        observed_at: now() - 7200,
        reset_at,
        window: None,
        last_probe_at: None,
    }
}

/// Name, the order, the connection under test, its ambiguous refusal.
type AmbiguousRow = (&'static str, [&'static str; 2], Who, fn() -> Canned);

#[test]
fn an_ambiguous_refusal_moves_nothing() {
    let rows: [AmbiguousRow; 9] = [
        (
            "subscription 429 with no unified header",
            ["sub", "api"],
            Who::Sub,
            || row(429, vec![]),
        ),
        ("subscription 429 allowed", ["sub", "api"], Who::Sub, || {
            row(
                429,
                vec![("anthropic-ratelimit-unified-status", "allowed".to_owned())],
            )
        }),
        ("529", ["sub", "api"], Who::Sub, || row(529, vec![])),
        ("500", ["sub", "api"], Who::Sub, || row(500, vec![])),
        ("401", ["sub", "api"], Who::Sub, || row(401, vec![])),
        ("403", ["sub", "api"], Who::Sub, || row(403, vec![])),
        ("400", ["sub", "api"], Who::Sub, || row(400, vec![])),
        ("plain 429 on the key", ["api", "router"], Who::Api, || {
            row(429, vec![])
        }),
        ("openrouter credits", ["router", "api"], Who::Router, || {
            canned(
                402,
                vec![("x-row", "upstream".to_owned())],
                r#"{"error":{"code":402,"message":"m","metadata":{"limit_source":"openrouter_credits"}}}"#,
            )
        }),
    ];
    for (i, (name, chain, under_test, refusal)) in rows.into_iter().enumerate() {
        let rig = Rig::new(&format!("ambiguous-{i}"), &chain, move |sent| {
            Ok(if has(sent, "next") { ok() } else { refusal() })
        });
        let expected = refusal();
        let got = rig.ask(&body_of("first"));
        assert_eq!(got.status, expected.status, "{name}");
        assert_eq!(got.header("x-row"), Some("upstream"), "{name}");
        let Step::Bytes(bytes) = &expected.body[0] else {
            panic!("{name}: bytes")
        };
        assert_eq!(&got.body, bytes, "{name}");
        let view = rig.view();
        let active = rig.settings.contexts[0].chain[0].clone();
        assert_eq!(view.status.active.as_ref(), Some(&active), "{name}");
        assert_eq!(view.status.generation, 1, "{name}");
        assert!(rig.records().is_empty(), "{name}: a record");
        let calls = rig.sent();
        assert!(
            (1..=2).contains(&calls.len()),
            "{name}: {} calls",
            calls.len()
        );
        assert!(calls.iter().all(|s| who(s) == under_test), "{name}");
        assert_eq!(rig.ask(&body_of("next")).status, 200, "{name}");
        let last = rig.sent().pop().expect("a call");
        assert_eq!(who(&last), under_test, "{name}: the next request moved");
        rig.cleanup();
    }
}

/// A refusal of `status` the evidence rows do not know, with a marker header.
pub(super) fn row(status: u16, headers: Vec<(&'static str, String)>) -> Canned {
    canned(
        status,
        [vec![("x-row", "upstream".to_owned())], headers].concat(),
        r#"{"type":"error","error":{"type":"some_error","message":"m"}}"#,
    )
}

#[test]
fn exhaustion_before_any_relayed_byte_moves_the_session_once() {
    let rig = Rig::new("exhausted-once", &["sub", "api", "router"], |sent| {
        Ok(match who(sent) {
            Who::Sub if has(sent, "first") => ok(),
            Who::Sub => rejected(),
            Who::Api | Who::Router => canned(200, vec![], "from the key"),
        })
    });
    assert_eq!(rig.ask(&body_of("first")).status, 200);
    let before = rig.view().status.generation;
    let got = rig.ask(&body_of("second"));
    assert_eq!(
        (got.status, got.body.as_slice()),
        (200, &b"from the key"[..])
    );
    let status = rig.view().status;
    assert_eq!(status.generation, before + 1);
    assert_eq!(status.active, Some(conn("api")));
    let switch = status.last_switch.expect("a switch");
    assert_eq!(
        (switch.from, switch.to, switch.reason),
        (conn("sub"), conn("api"), SwitchReason::Exhausted)
    );
    let records = rig.records();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].connection, conn("sub"));
    rig.cleanup();
}

#[test]
fn after_a_relayed_byte_nothing_else_is_tried() {
    let rig = Rig::new("committed", &["sub", "api"], |sent| {
        Ok(if has(sent, "next") {
            ok()
        } else {
            Canned {
                status: 200,
                headers: vec![],
                body: vec![Step::Bytes(b"event: one\n".to_vec()), Step::Break],
            }
        })
    });
    let got = rig.ask(&body_of("first"));
    assert_eq!(got.status, 200);
    assert_eq!(got.body, b"event: one\n");
    assert!(!got.ended, "a broken stream must not look complete");
    assert_eq!(rig.sent().len(), 1);
    let status = rig.view().status;
    assert_eq!((status.active, status.generation), (Some(conn("sub")), 1));
    rig.ask(&body_of("next"));
    assert_eq!(rig.calls(Who::Sub), 2);
    assert_eq!(rig.calls(Who::Api), 0);
    rig.cleanup();
}

#[test]
fn a_lost_upstream_is_not_replayed() {
    type Script = fn(&Sent) -> Result<Canned, SendError>;
    let rows: [(&str, Script); 2] = [
        ("dropped on send", |_| Err(SendError::Dropped)),
        ("a 200 whose first read fails", |_| {
            Ok(Canned {
                status: 200,
                headers: vec![],
                body: vec![Step::Break],
            })
        }),
    ];
    for (i, (name, script)) in rows.into_iter().enumerate() {
        let rig = Rig::new(&format!("lost-{i}"), &["sub", "api"], script);
        let got = rig.ask(&body_of("one"));
        assert_eq!(got.status, 502, "{name}");
        assert_eq!(got.body, answers::interrupted("sub").body(), "{name}");
        assert_eq!(rig.sent().len(), 1, "{name}");
        rig.cleanup();
    }
}

#[test]
fn not_sent_is_asked_twice_then_answered_unreachable() {
    let rig = Rig::new("not-sent", &["sub", "api"], |_| Err(SendError::NotSent));
    let got = rig.ask(&body_of("one"));
    assert_eq!(
        (got.status, got.body),
        (502, answers::unreachable("sub").body())
    );
    assert_eq!(rig.calls(Who::Sub), 2);
    assert_eq!(rig.sent().len(), 2);
    rig.cleanup();
}

#[test]
fn two_concurrent_refusals_advance_the_session_once() {
    let gates = Gates::default();
    let held = gates.clone();
    let rig = Rig::new("concurrent", &["sub", "api"], move |sent| {
        Ok(match who(sent) {
            Who::Sub => {
                let mut refusal = rejected();
                refusal.body.insert(0, held.step());
                refusal
            }
            Who::Api | Who::Router => ok(),
        })
    });
    let first = rig.ask_later(&body_of("a"));
    let second = rig.ask_later(&body_of("b"));
    wait_for("both subscription calls", || rig.calls(Who::Sub) == 2);
    gates.open();
    for pending in [first, second] {
        assert_eq!(pending.join().expect("reply").status, 200);
    }
    let status = rig.view().status;
    assert_eq!(status.generation, 2, "one opening choice and one move");
    assert_eq!(status.active, Some(conn("api")));
    assert_eq!((rig.calls(Who::Sub), rig.calls(Who::Api)), (2, 2));
    rig.cleanup();
}

/// A conversation with hidden reasoning in an assistant turn, and `marker` in the first message.
pub(super) fn thinking_body(marker: &str) -> String {
    format!(
        r#"{{"model":"{OPUS}","messages":[{{"role":"user","content":"{marker}"}},{{"role":"assistant","content":[{{"type":"thinking","thinking":"t","signature":"c2ln"}},{{"type":"redacted_thinking","data":"ZGF0YQ=="}},{{"type":"tool_use","id":"toolu_1","name":"Read","input":{{"p":1}}}}]}},{{"role":"user","content":[{{"type":"tool_result","tool_use_id":"toolu_1","content":"ok"}}]}}]}}"#
    )
}

fn thinking_kept(sent: &Sent) -> bool {
    has(sent, r#""type":"thinking""#) && has(sent, r#""type":"redacted_thinking""#)
}

fn thinking_gone(sent: &Sent) -> bool {
    !has(sent, "thinking")
        && has(
            sent,
            r#"{"type":"tool_use","id":"toolu_1","name":"Read","input":{"p":1}}"#,
        )
        && has(
            sent,
            r#"{"type":"tool_result","tool_use_id":"toolu_1","content":"ok"}"#,
        )
}

#[test]
fn openrouter_is_pinned_and_loses_foreign_reasoning_only() {
    let pinned = r#""provider":{"allow_fallbacks":false,"require_parameters":true}"#;
    let rig = Rig::new("or-alone", &["router"], |_| Ok(ok()));
    rig.ask(&thinking_body("one"));
    rig.ask(&thinking_body("two"));
    for sent in rig.sent() {
        assert!(has(&sent, pinned), "the provider is pinned");
        assert!(thinking_kept(&sent), "its own reasoning is kept");
    }
    assert_eq!(rig.view().reasoning, Reasoning::Intact);
    rig.cleanup();

    let script = |sent: &Sent| {
        Ok(match who(sent) {
            Who::Sub if has(sent, "one") => ok(),
            Who::Sub => rejected(),
            Who::Api | Who::Router => ok(),
        })
    };
    let rig = Rig::new("or-after-sub", &["sub", "router"], script);
    rig.ask(&thinking_body("one"));
    rig.ask(&thinking_body("two"));
    let to_router: Vec<Sent> = rig
        .sent()
        .into_iter()
        .filter(|s| who(s) == Who::Router)
        .collect();
    assert_eq!(to_router.len(), 1);
    assert!(thinking_gone(&to_router[0]), "foreign reasoning removed");
    assert_eq!(rig.view().reasoning, Reasoning::Removed);
    rig.cleanup();

    let mut setup = Setup::chain(&["router"]);
    setup.resumed = true;
    let rig = Rig::with("or-resumed", setup, |_| Ok(ok()));
    rig.ask(&thinking_body("one"));
    assert!(
        thinking_gone(&rig.sent()[0]),
        "a resumed desk's history is not its own"
    );
    rig.cleanup();

    let rig = Rig::new("sub-to-key", &["sub", "api"], script);
    rig.ask(&thinking_body("one"));
    let body = thinking_body("two");
    rig.ask(&body);
    let to_api: Vec<Sent> = rig
        .sent()
        .into_iter()
        .filter(|s| who(s) == Who::Api)
        .collect();
    assert_eq!(to_api.len(), 1);
    assert_eq!(to_api[0].body, body.as_bytes(), "Anthropic gets it whole");
    assert_eq!(rig.view().reasoning, Reasoning::VendorMayDrop);
    rig.cleanup();
}

#[test]
fn a_client_that_has_gone_is_not_served() {
    let gates = Gates::default();
    let held = gates.clone();
    let mut setup = Setup::chain(&["sub", "api"]);
    setup.records = vec![record(None)];
    let rig = Rig::with("gone-before-retry", setup, move |_| {
        Ok(Canned {
            status: 529,
            headers: vec![],
            body: vec![held.step()],
        })
    });
    rig.router
        .retry(&rig.opened.id, &conn("sub"))
        .expect("armed");
    let client = connect(rig.port(), &rig.opened, &body_of("one"), &[]);
    wait_for("the check", || rig.sent().len() == 1);
    drop(client);
    gates.open();
    let scope = || rig.view().status.entries[0].exhausted[0].clone();
    wait_for("the lease to be given back", || !scope().checking);
    assert!(scope().retry_armed, "the armed retry is still armed");
    assert_eq!(rig.sent().len(), 1, "no call for a client that has gone");
    rig.cleanup();

    let gates = Gates::default();
    let held = gates.clone();
    let rig = Rig::new("gone-mid-stream", &["sub", "api"], move |sent| {
        Ok(if has(sent, "next") {
            ok()
        } else {
            let more = (0..64).map(|_| Step::Bytes(vec![b'x'; 16 * 1024]));
            Canned {
                status: 200,
                headers: vec![],
                body: [Step::Bytes(b"one".to_vec()), held.step()]
                    .into_iter()
                    .chain(more)
                    .collect(),
            }
        })
    });
    let client = connect(rig.port(), &rig.opened, &body_of("one"), &[]);
    let spoke = || {
        rig.router
            .routed(&rig.opened.id)
            .is_some_and(|routed| !lock(&routed.live).spoke.is_empty())
    };
    wait_for("the first chunk", spoke);
    drop(client);
    gates.open();
    assert_eq!(rig.ask(&body_of("next")).status, 200);
    let status = rig.view().status;
    assert_eq!((status.active, status.generation), (Some(conn("sub")), 1));
    assert!(rig.records().is_empty());
    rig.cleanup();
}

#[test]
fn a_late_reply_finishes_on_the_connection_it_started_on() {
    let gates = Gates::default();
    let held = gates.clone();
    let rig = Rig::new("late", &["sub", "api"], move |sent| {
        Ok(match who(sent) {
            Who::Sub if has(sent, "slow") => Canned {
                status: 200,
                headers: vec![],
                body: vec![
                    Step::Bytes(b"one".to_vec()),
                    held.step(),
                    Step::Bytes(b"two".to_vec()),
                ],
            },
            Who::Sub => rejected(),
            Who::Api | Who::Router => ok(),
        })
    });
    let slow = rig.ask_later(&body_of("slow"));
    wait_for("the slow call", || rig.calls(Who::Sub) == 1);
    assert_eq!(rig.ask(&body_of("fast")).status, 200);
    gates.open();
    let got = slow.join().expect("reply");
    assert_eq!(
        (got.status, got.body.as_slice(), got.ended),
        (200, &b"onetwo"[..], true)
    );
    assert_eq!(rig.records().len(), 1);
    let status = rig.view().status;
    assert_eq!((status.active, status.generation), (Some(conn("api")), 2));
    rig.cleanup();
}

#[test]
fn a_due_record_is_checked_once_and_cleared() {
    let gates = Gates::default();
    let held = gates.clone();
    let mut setup = Setup::chain(&["sub", "api"]);
    setup.records = vec![record(Some(now() - 60))];
    let rig = Rig::with("recovery", setup, move |sent| {
        Ok(match who(sent) {
            Who::Sub => Canned {
                status: 200,
                headers: vec![],
                body: vec![held.step(), Step::Bytes(b"checked".to_vec())],
            },
            Who::Api | Who::Router => ok(),
        })
    });
    let check = rig.ask_later(&body_of("check"));
    wait_for("the check", || rig.calls(Who::Sub) == 1);
    assert_eq!(rig.ask(&body_of("other")).status, 200);
    assert_eq!((rig.calls(Who::Sub), rig.calls(Who::Api)), (1, 1));
    let status = rig.view().status;
    assert_eq!((status.active, status.generation), (Some(conn("sub")), 1));
    assert_eq!(
        rig.records().len(),
        1,
        "the record holds while it is checked"
    );
    gates.open();
    assert_eq!(check.join().expect("reply").body, b"checked");
    assert!(rig.records().is_empty(), "the check cleared the record");
    rig.cleanup();
}

#[test]
fn the_view_counts_requests_and_clears_a_problem_once_served() {
    let rig = Rig::new("view", &["sub", "api"], |sent| {
        Ok(if has(sent, "bad") {
            row(500, vec![])
        } else {
            ok()
        })
    });
    assert_eq!(rig.ask(&body_of("bad")).status, 500);
    let entry = rig.view().status.entries[0].clone();
    assert_eq!(entry.problem.map(|p| p.kind), Some(ProblemKind::Faulted));
    assert_eq!(rig.ask(&body_of("good")).status, 200);
    let view = rig.view();
    assert_eq!(view.requests, 2);
    let served = view.status.last_served.expect("served");
    assert_eq!(served.connection, conn("sub"));
    assert_eq!(view.status.entries[0].problem, None);
    rig.cleanup();
}

/// The real router in front of the real Anthropic API, for a hand-run check by the dispatcher:
/// `OB_ROUTE_FILE=<path> OB_ROUTE_SECS=<n> cargo test --lib live_route -- --ignored --nocapture`
#[test]
#[ignore = "serves real Claude Code requests on this Mac's own login, so only the dispatcher runs it"]
fn live_route_serves_a_claude_subscription_desk() {
    use std::os::unix::fs::OpenOptionsExt as _;
    let file = std::env::var("OB_ROUTE_FILE").expect("OB_ROUTE_FILE names the file to write");
    let secs: u64 = std::env::var("OB_ROUTE_SECS")
        .ok()
        .and_then(|secs| secs.parse().ok())
        .expect("OB_ROUTE_SECS is a whole number of seconds");
    let path = temp_path("relay", "live");
    let keys = Arc::new(Keys::new(Arc::new(MemorySecrets::new())));
    let upstream: Arc<dyn Upstream> = Arc::new(upstream::UreqUpstream::new());
    let router = Router::new(
        ExhaustionFile::new(path.clone()),
        keys,
        upstream,
        Bases::default(),
    );
    let settings = Setup::chain(&["sub"]).settings;
    let opened = router
        .open(&settings, &ctx("work"), false)
        .expect("opens")
        .expect("routed");
    let mut out = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&file)
        .expect("OB_ROUTE_FILE can be created");
    writeln!(
        out,
        "ANTHROPIC_BASE_URL={}\nANTHROPIC_CUSTOM_HEADERS={}",
        opened.base_url,
        opened.header.expose()
    )
    .expect("written");
    drop(out);
    thread::sleep(Duration::from_secs(secs));
    fs::remove_file(&file).expect("removed");
    let view = router.view(&opened.id).expect("routed");
    assert!(view.requests >= 1, "no request reached the router");
    assert_eq!(view.status.active, Some(conn("sub")));
    assert_eq!(view.status.blocked_since, None);
    assert!(view.status.last_served.is_some());
    let entry = &view.status.entries[0];
    assert_eq!(entry.problem, None);
    assert!(entry.exhausted.is_empty());
    assert!(!path.exists(), "an exhaustion record was written");
    println!("requests: {}", view.requests);
    let _ = fs::remove_dir_all(path.parent().expect("dir"));
}
