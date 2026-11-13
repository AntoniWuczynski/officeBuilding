use super::*;
use crate::routing::availability::{Bucket, ExhaustionRecord};
use crate::routing::capabilities::Requirements;
use crate::routing::config::TierId;
use crate::routing::fixtures::{conn, ctx, secret_of, settings, temp_path};
use crate::routing::observe::{Breadth, Exhaustion, Observation};
use crate::routing::policy::Skip;
use crate::routing::state::Verdict;
use crate::routing::upstream::{FakeUpstream, SendError};
use crate::secrets::MemorySecrets;
use serde_json::json;
use std::fmt::Write as _;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;

const T: u64 = 1_700_000_100;
const OPUS: &str = "claude-opus-5-5";

/// Name, method, path, headers, the status expected.
type DispatchRow<'a> = (&'a str, &'a str, String, Vec<(&'a str, &'a str)>, u16);
/// Name, resumed, connections that answered, the active one, the reasoning expected.
type ReasoningRow<'a> = (
    &'a str,
    bool,
    BTreeSet<ConnectionId>,
    Option<(&'a ConnectionId, ConnectionKind)>,
    Reasoning,
);
/// Name, paid use on, desk known, target, the generation or the error expected.
type SwitchRow = (
    &'static str,
    bool,
    bool,
    &'static str,
    Result<u64, &'static str>,
);

struct Rig {
    router: Arc<Router>,
    keys: Arc<Keys>,
    fake: Arc<FakeUpstream>,
    path: PathBuf,
}

impl Rig {
    fn new(name: &str) -> Self {
        Self::on(temp_path("router", name))
    }

    /// A router on an existing records file.
    fn on(path: PathBuf) -> Self {
        let keys = Arc::new(Keys::new(Arc::new(MemorySecrets::new())));
        let fake = Arc::new(FakeUpstream::new(|_| Err(SendError::NotSent)));
        let upstream: Arc<dyn Upstream> = Arc::<FakeUpstream>::clone(&fake);
        let router = Router::new(
            ExhaustionFile::new(path.clone()),
            Arc::clone(&keys),
            upstream,
            Bases::default(),
        );
        Self {
            router,
            keys,
            fake,
            path,
        }
    }

    fn key(&self, id: &str, kind: ConnectionKind) {
        self.keys
            .set(kind, &conn(id), Some(format!("test-{id}-key")), || true)
            .expect("key saved");
    }

    fn open_with(&self, s: &RoutingSettings, context: &str) -> Opened {
        self.router
            .open(s, &ctx(context), false)
            .expect("opens")
            .expect("routed")
    }

    fn open(&self, context: &str) -> Opened {
        self.open_with(&settings(), context)
    }

    fn port(&self) -> u16 {
        lock(&self.router.port).expect("listening")
    }

    fn records(&self) -> Vec<crate::routing::availability::ExhaustionRecord> {
        ExhaustionFile::new(self.path.clone()).load().records
    }

    fn cleanup(&self) {
        let _ = fs::remove_dir_all(self.path.parent().expect("dir"));
    }
}

fn ask(model: &str) -> Requirements {
    Requirements::new(model, Some("high"), BTreeSet::new()).expect("valid request")
}

fn exhausted(reset_at: Option<u64>) -> Observation {
    Observation::Exhausted(Exhaustion {
        breadth: Breadth::Request,
        reset_at,
        window: Some("five_hour".to_owned()),
    })
}

/// Admit one request on `id`, and settle it with `seen`.
fn serve_one(state: &mut RouteState, id: &RouteId, seen: &Observation, now: u64) -> Verdict {
    let credentials = BTreeSet::new();
    let lease = state
        .begin(id, ask(OPUS), &credentials, now)
        .expect("a lease");
    state.settle(lease, seen, false, &credentials, now)
}

fn is_hex(text: &str, len: usize) -> bool {
    text.len() == len
        && text
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

#[test]
fn an_unrouted_launch_opens_nothing_and_starts_no_listener() {
    type Edit = fn(&mut RoutingSettings);
    let rows: [(&str, Edit, &str); 3] = [
        ("switch off", |s| s.enabled = false, "work"),
        ("unknown context", |_| (), "elsewhere"),
        ("empty order", |s| s.contexts[0].chain.clear(), "work"),
    ];
    for (name, edit, context) in rows {
        let rig = Rig::new(&format!("unrouted-{}", name.replace(' ', "-")));
        let mut s = settings();
        edit(&mut s);
        assert_eq!(
            rig.router.open(&s, &ctx(context), false),
            Ok(None),
            "{name}"
        );
        assert!(
            lock(&rig.router.port).is_none(),
            "{name}: a listener started"
        );
        assert!(lock(&rig.router.sessions).is_empty(), "{name}");
        rig.cleanup();
    }
}

#[test]
fn a_routed_open_gives_an_address_and_a_secret_header() {
    let rig = Rig::new("routed-open");
    let first = rig.open("work");
    let second = rig.open("work");
    let port = rig.port();
    for opened in [&first, &second] {
        let prefix = format!("http://127.0.0.1:{port}/s/");
        let id = opened.base_url.strip_prefix(&prefix).expect("base address");
        assert_eq!(id, opened.id.as_str());
        assert!(is_hex(id, 32), "route id {id}");
        let (name, secret) = opened.header.expose().split_once(": ").expect("header");
        assert_eq!(name, "x-office-building-route");
        assert!(is_hex(secret, 64), "secret shape");
    }
    assert_ne!(first.id, second.id);
    assert_ne!(secret_of(&first), secret_of(&second));
    rig.cleanup();
}

#[test]
fn a_bound_terminal_finds_its_route_until_it_exits() {
    let rig = Rig::new("bind");
    let opened = rig.open("work");
    let other = rig.open("work");
    rig.router.bind(&opened.id, "term-1");
    assert_eq!(rig.router.route_of("term-1"), Some(opened.id.clone()));
    assert_eq!(rig.router.route_of("term-2"), None);
    assert!(rig.router.view(&opened.id).is_some());

    rig.router.terminal_exited("term-1");
    assert_eq!(rig.router.route_of("term-1"), None);
    assert_eq!(rig.router.view(&opened.id), None);
    assert!(
        lock(&rig.router.state)
            .status(&opened.id, &BTreeSet::new(), T)
            .is_none()
    );

    rig.router.close(&other.id);
    rig.router.close(&other.id);
    rig.router.terminal_exited("term-1");
    assert!(lock(&rig.router.sessions).is_empty());
    rig.cleanup();
}

/// One request over a real connection to the router's own listener: the status and the whole reply.
fn send(port: u16, method: &str, path: &str, headers: &[(&str, &str)]) -> (u16, Vec<u8>) {
    let lines = headers
        .iter()
        .fold(String::new(), |mut lines, (name, value)| {
            let _ = write!(lines, "{name}: {value}\r\n");
            lines
        });
    let body = if method == "POST" { "{}" } else { "" };
    let text = format!(
        "{method} {path} HTTP/1.1\r\nhost: 127.0.0.1:{port}\r\n{lines}content-length: {}\r\n\r\n{body}",
        body.len()
    );
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream.write_all(text.as_bytes()).expect("write");
    let mut reply = Vec::new();
    stream.read_to_end(&mut reply).expect("read");
    let status = std::str::from_utf8(reply.get(9..12).expect("status"))
        .expect("text")
        .parse()
        .expect("number");
    (status, reply)
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "one table over every way a request can be turned away"
)]
fn dispatch_answers_without_calling_upstream() {
    let rig = Rig::new("dispatch");
    let a = rig.open("work");
    let b = rig.open("work");
    let port = rig.port();
    let (a_id, b_id) = (a.id.as_str(), b.id.as_str());
    let a_secret = secret_of(&a);
    let wrong = "f".repeat(64);
    let unknown_id = "0123456789abcdef0123456789abcdef";
    let local = upstream::LOCAL_HEADER;
    let unknown: Vec<DispatchRow> = vec![
        (
            "an unknown route",
            "POST",
            format!("/s/{unknown_id}/v1/messages"),
            vec![(local, a_secret.as_str())],
            400,
        ),
        (
            "a missing secret",
            "POST",
            format!("/s/{a_id}/v1/messages"),
            vec![],
            400,
        ),
        (
            "a wrong secret",
            "POST",
            format!("/s/{a_id}/v1/messages"),
            vec![(local, wrong.as_str())],
            400,
        ),
        (
            "the secret twice",
            "POST",
            format!("/s/{a_id}/v1/messages"),
            vec![(local, a_secret.as_str()), (local, a_secret.as_str())],
            400,
        ),
        (
            "A's secret on B's path",
            "POST",
            format!("/s/{b_id}/v1/messages"),
            vec![(local, a_secret.as_str())],
            400,
        ),
        (
            "an id that is not lower-case hex",
            "POST",
            format!("/s/{}/v1/messages", a_id.to_uppercase()),
            vec![(local, a_secret.as_str())],
            400,
        ),
        (
            "no route prefix",
            "POST",
            "/v1/messages".to_owned(),
            vec![(local, a_secret.as_str())],
            400,
        ),
        (
            "a wrong secret on another path",
            "GET",
            format!("/s/{a_id}/v1/models"),
            vec![(local, wrong.as_str())],
            400,
        ),
    ];
    let other: Vec<DispatchRow> = vec![
        (
            "a model list",
            "GET",
            format!("/s/{a_id}/v1/models"),
            vec![(local, a_secret.as_str())],
            404,
        ),
        (
            "a token count",
            "POST",
            format!("/s/{a_id}/v1/messages/count_tokens"),
            vec![(local, a_secret.as_str())],
            404,
        ),
        (
            "the old completion call",
            "POST",
            format!("/s/{a_id}/v1/complete"),
            vec![(local, a_secret.as_str())],
            404,
        ),
        (
            "a messages path read with GET",
            "GET",
            format!("/s/{a_id}/v1/messages"),
            vec![(local, a_secret.as_str())],
            404,
        ),
    ];

    let (status, hello) = send(port, "HEAD", &format!("/s/{a_id}/api/hello"), &[]);
    assert_eq!(status, 200, "hello");
    assert!(hello.ends_with(b"\r\n\r\n"), "hello has no body");
    assert!(
        String::from_utf8_lossy(&hello).contains("content-length: 0\r\n"),
        "hello is empty"
    );

    let mut first: Option<Vec<u8>> = None;
    let desk = answers::unknown_desk().body();
    for (name, method, path, headers, expected) in &unknown {
        let (status, reply) = send(port, method, path, headers);
        assert_eq!(status, *expected, "{name}");
        assert!(reply.ends_with(&desk), "{name}: the unknown-desk body");
        match &first {
            None => first = Some(reply),
            Some(first) => assert_eq!(&reply, first, "{name}: not byte for byte the same"),
        }
    }
    let not_found = answers::other_path().body();
    for (name, method, path, headers, expected) in &other {
        let (status, reply) = send(port, method, path, headers);
        assert_eq!(status, *expected, "{name}");
        assert!(reply.ends_with(&not_found), "{name}: the other-path body");
    }
    assert!(rig.fake.sent().is_empty(), "an upstream was called");
    rig.cleanup();
}

#[test]
fn credentials_list_saved_keys_of_relayed_kinds_only() {
    let rig = Rig::new("credentials");
    let mut s = settings();
    let mut sign_in = s.connections[3].clone();
    sign_in.id = conn("siwc");
    sign_in.name = "siwc".to_owned();
    sign_in.kind = ConnectionKind::ChatGptSignIn;
    s.connections.push(sign_in);
    s.contexts[0].chain.push(conn("siwc"));
    let opened = rig.open_with(&s, "work");
    let routed = rig.router.routed(&opened.id).expect("routed");
    assert!(
        rig.router.credentials(&routed.chain).is_empty(),
        "no keys yet"
    );
    rig.key("api", ConnectionKind::AnthropicApiKey);
    rig.key("oai", ConnectionKind::OpenAiApiKey);
    rig.key("siwc", ConnectionKind::ChatGptSignIn);
    assert_eq!(
        rig.router.credentials(&routed.chain),
        BTreeSet::from([conn("api"), conn("oai")]),
        "the subscription, a missing key and Sign in with ChatGPT are never listed"
    );
    rig.key("router", ConnectionKind::OpenRouter);
    assert_eq!(
        rig.router.credentials(&routed.chain),
        BTreeSet::from([conn("api"), conn("router"), conn("oai")])
    );
    rig.cleanup();
}

#[test]
fn a_fresh_view_has_no_active_connection_and_nothing_arrived() {
    let rig = Rig::new("fresh-view");
    let opened = rig.open("work");
    let view = rig.router.view(&opened.id).expect("a view");
    assert_eq!(view.status.active, None);
    assert_eq!(view.status.generation, 0);
    assert_eq!(view.requests, 0);
    assert_eq!(view.reasoning, Reasoning::Intact);
    assert_eq!(view.records_problem, None);
    assert_eq!(
        rig.router.view(&RouteId::new("abc".to_owned())),
        None,
        "unknown"
    );
    rig.cleanup();
}

#[test]
fn the_desk_route_json_is_pinned_and_holds_no_secret() {
    let rig = Rig::new("pinned");
    let mut s = settings();
    s.contexts[0].chain = vec![conn("sub"), conn("api")];
    rig.key("api", ConnectionKind::AnthropicApiKey);
    let a = rig.open_with(&s, "work");
    let b = rig.open_with(&s, "work");
    let credentials = BTreeSet::from([conn("api")]);
    rig.router.with_state(|state| {
        // Session B leaves a throttle on the api connection, which every session sees.
        state
            .switch(&b.id, &conn("api"), &credentials, T - 10)
            .expect("switch");
        let lease = state
            .begin(&b.id, ask(OPUS), &credentials, T - 10)
            .expect("lease");
        let throttled = Observation::Throttled { retry_after: None };
        let Verdict::Again { lease, .. } =
            state.settle(lease, &throttled, false, &credentials, T - 10)
        else {
            panic!("a throttle is retried once");
        };
        state.settle(lease, &Observation::Cancelled, false, &credentials, T - 10);
        // Session A runs out on the subscription and moves to the key.
        let lease = state
            .begin(&a.id, ask(OPUS), &credentials, T)
            .expect("lease");
        let seen = exhausted(Some(T + 3500));
        let Verdict::Again { lease, .. } = state.settle(lease, &seen, false, &credentials, T)
        else {
            panic!("an exhausted subscription moves on");
        };
        state.settle(lease, &Observation::Cancelled, false, &credentials, T);
    });
    rig.router.close(&b.id);
    {
        let routed = rig.router.routed(&a.id).expect("routed");
        let mut live = lock(&routed.live);
        live.spoke = BTreeSet::from([conn("sub"), conn("api")]);
        live.requests = 7;
    }
    let view = rig.router.view(&a.id).expect("a view");
    let value = serde_json::to_value(&view).expect("serialises");
    assert_eq!(
        value,
        json!({
            "status": {
                "context": "work", "generation": 2, "active": "api", "blockedSince": null,
                "lastSwitch": { "from": "sub", "to": "api", "reason": "exhausted", "at": T },
                "lastServed": { "connection": "api", "requestedModel": OPUS, "requestedEffort": "high",
                                "model": OPUS, "effort": "high", "at": T },
                "entries": [
                    { "connection": "sub", "name": "sub", "kind": "claude-subscription",
                      "state": { "kind": "unavailable", "reason": { "kind": "exhausted" } },
                      "exhausted": [ { "bucket": { "kind": "tier", "tier": "opus" }, "window": "five_hour",
                                       "observedAt": T, "resetAt": T + 3500, "checking": false, "retryArmed": false } ],
                      "problem": null },
                    { "connection": "api", "name": "api", "kind": "anthropic-api-key", "state": { "kind": "active" },
                      "exhausted": [], "problem": { "kind": "throttled", "at": T - 10 } }
                ]
            },
            "reasoning": "vendor-may-drop", "requests": 7, "recordsProblem": null
        })
    );
    let secret = secret_of(&a);
    let text = serde_json::to_string(&view).expect("serialises");
    assert!(!text.contains(&secret), "the JSON holds the secret");
    assert!(
        !format!("{a:?}").contains(&secret),
        "Debug holds the secret"
    );
    assert!(!format!("{view:?}").contains(&secret));
    rig.cleanup();
}

#[test]
fn reasoning_follows_the_connections_that_answered() {
    let (sub, api, router, oai) = (conn("sub"), conn("api"), conn("router"), conn("oai"));
    let set = |ids: &[&ConnectionId]| ids.iter().map(|c| (*c).clone()).collect::<BTreeSet<_>>();
    let rows: [ReasoningRow; 13] = [
        ("fresh", false, set(&[]), None, Reasoning::Intact),
        (
            "one answer, still there",
            false,
            set(&[&sub]),
            Some((&sub, ConnectionKind::ClaudeSubscription)),
            Reasoning::Intact,
        ),
        (
            "resumed on the subscription",
            true,
            set(&[]),
            Some((&sub, ConnectionKind::ClaudeSubscription)),
            Reasoning::Intact,
        ),
        (
            "moved to the key",
            false,
            set(&[&sub, &api]),
            Some((&api, ConnectionKind::AnthropicApiKey)),
            Reasoning::VendorMayDrop,
        ),
        (
            "moved back to the subscription",
            false,
            set(&[&sub, &api]),
            Some((&sub, ConnectionKind::ClaudeSubscription)),
            Reasoning::VendorMayDrop,
        ),
        (
            "openrouter alone",
            false,
            set(&[&router]),
            Some((&router, ConnectionKind::OpenRouter)),
            Reasoning::Intact,
        ),
        (
            "openrouter first, nothing yet",
            false,
            set(&[]),
            Some((&router, ConnectionKind::OpenRouter)),
            Reasoning::Intact,
        ),
        (
            "openrouter on a resumed desk",
            true,
            set(&[]),
            Some((&router, ConnectionKind::OpenRouter)),
            Reasoning::Removed,
        ),
        (
            "openrouter after the subscription",
            false,
            set(&[&sub, &router]),
            Some((&router, ConnectionKind::OpenRouter)),
            Reasoning::Removed,
        ),
        (
            "no active, two answered",
            false,
            set(&[&sub, &api]),
            None,
            Reasoning::VendorMayDrop,
        ),
        (
            "openai on a fresh desk",
            false,
            set(&[]),
            Some((&oai, ConnectionKind::OpenAiApiKey)),
            Reasoning::Removed,
        ),
        (
            "openai after a move",
            false,
            set(&[&sub, &oai]),
            Some((&oai, ConnectionKind::OpenAiApiKey)),
            Reasoning::Removed,
        ),
        (
            "anthropic active again after openai",
            false,
            set(&[&api, &oai]),
            Some((&api, ConnectionKind::AnthropicApiKey)),
            Reasoning::VendorMayDrop,
        ),
    ];
    for (name, resumed, spoke, active, expected) in rows {
        assert_eq!(Reasoning::of(resumed, &spoke, active), expected, "{name}");
    }
}

#[test]
fn retry_arms_a_record_of_this_desks_own_order_only() {
    let rig = Rig::new("retry");
    let opened = rig.open("work");
    rig.router
        .with_state(|state| serve_one(state, &opened.id, &exhausted(None), T));
    let no_record =
        "that connection has no used-up allowance on record, so there is nothing to retry";
    let rows: [(&str, RouteId, &str, Result<(), &str>); 5] = [
        (
            "unknown desk",
            RouteId::new("abc".to_owned()),
            "sub",
            Err(NOT_ROUTED),
        ),
        (
            "outside the order",
            opened.id.clone(),
            "nowhere",
            Err(NOT_IN_ORDER),
        ),
        (
            "another context's connection",
            opened.id.clone(),
            "home-api",
            Err(NOT_IN_ORDER),
        ),
        ("no record", opened.id.clone(), "api", Err(no_record)),
        ("a record", opened.id.clone(), "sub", Ok(())),
    ];
    for (name, id, connection, expected) in rows {
        let got = rig.router.retry(&id, &conn(connection));
        if let Err(text) = expected {
            assert_eq!(got, Err(text.to_owned()), "{name}");
        } else {
            let view = got.expect(name);
            let sub = &view.status.entries[0];
            assert!(sub.exhausted.iter().all(|e| e.retry_armed), "{name}: armed");
            assert!(!sub.exhausted.is_empty(), "{name}");
        }
    }
    rig.cleanup();
}

#[test]
fn switch_moves_to_a_usable_entry_of_the_desks_order() {
    let paid = "api cannot be switched to, because it is billed for each request and may not spend money (Settings, Routing, \"May spend money\")";
    let rows: [SwitchRow; 4] = [
        ("a ready entry", true, true, "api", Ok(2)),
        ("paid use off", false, true, "api", Err(paid)),
        (
            "outside the order",
            true,
            true,
            "home-api",
            Err(NOT_IN_ORDER),
        ),
        ("an unknown desk", true, false, "api", Err(NOT_ROUTED)),
    ];
    for (name, paid, known, target, expected) in rows {
        let rig = Rig::new(&format!("switch-{}", name.replace(' ', "-")));
        rig.key("api", ConnectionKind::AnthropicApiKey);
        let mut s = settings();
        s.connections[1].paid = paid;
        let opened = rig.open_with(&s, "work");
        rig.router
            .with_state(|state| serve_one(state, &opened.id, &Observation::Completed, T));
        let id = if known {
            opened.id.clone()
        } else {
            RouteId::new("abc".to_owned())
        };
        let got = rig.router.switch(&id, &conn(target));
        match expected {
            Ok(generation) => {
                let status = got.expect(name).status;
                assert_eq!(status.generation, generation, "{name}");
                assert_eq!(status.active, Some(conn(target)), "{name}");
                let last = status.last_switch.expect("a switch");
                assert_eq!(
                    last.reason,
                    crate::routing::status::SwitchReason::Manual,
                    "{name}"
                );
            }
            Err(text) => assert_eq!(got, Err(text.to_owned()), "{name}"),
        }
        rig.cleanup();
    }
}

#[test]
fn preferences_bite_at_once_and_flush_writes_the_file() {
    let path = temp_path("router", "preferences");
    let gone = ExhaustionRecord {
        connection: conn("router"),
        bucket: Bucket::Whole,
        observed_at: T,
        reset_at: None,
        window: None,
        last_probe_at: None,
    };
    ExhaustionFile::new(path.clone())
        .save(&Saved {
            revision: 1,
            records: vec![gone],
        })
        .expect("seed");
    let rig = Rig::on(path);
    rig.key("home-api", ConnectionKind::AnthropicApiKey);
    let opened = rig.open("home");
    let credentials = BTreeSet::from([conn("home-api")]);
    let lease = rig
        .router
        .with_state(|s| s.begin(&opened.id, ask(OPUS), &credentials, T))
        .expect("a lease while paid use is on");
    rig.router.with_state(|s| {
        s.settle(lease, &Observation::Completed, false, &credentials, T);
    });

    let mut s = settings();
    s.connections.retain(|c| c.id != conn("router"));
    let home = s
        .connections
        .iter_mut()
        .find(|c| c.id == conn("home-api"))
        .expect("home-api");
    home.paid = false;
    rig.router.apply_preferences(&s);
    assert_eq!(
        lock(&rig.router.state).begin(&opened.id, ask(OPUS), &credentials, T + 1),
        Err(RouteError::Refused {
            connection: conn("home-api"),
            reason: Skip::PaidNotAllowed
        })
    );
    assert_eq!(rig.records().len(), 1, "apply_preferences writes nothing");
    rig.router.flush();
    assert!(rig.records().is_empty(), "flush drops the gone record");
    rig.cleanup();
}

#[test]
fn a_settled_exhaustion_is_on_disk_when_with_state_returns() {
    let rig = Rig::new("save-cycle");
    let opened = rig.open("work");
    rig.router
        .with_state(|s| serve_one(s, &opened.id, &exhausted(Some(T + 3600)), T));
    let records = rig.records();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].connection, conn("sub"));
    assert_eq!(records[0].bucket, Bucket::Tier { tier: TierId::Opus });

    let again = Rig::on(rig.path.clone());
    let other = again.open("work");
    let view = again.router.view(&other.id).expect("a view");
    assert_eq!(
        view.status.entries[0].exhausted.len(),
        1,
        "a new router starts with it"
    );
    rig.cleanup();
}

#[test]
fn a_failed_write_is_reported_until_a_good_one() {
    let rig = Rig::new("unwritable");
    let opened = rig.open("work");
    // A folder where the temporary file goes makes the write fail.
    let blocker = rig.path.with_extension("json.tmp");
    fs::create_dir_all(&blocker).expect("blocker");
    rig.router
        .with_state(|s| serve_one(s, &opened.id, &exhausted(None), T));
    assert!(rig.records().is_empty());
    let view = rig.router.view(&opened.id).expect("a view");
    assert!(view.records_problem.is_some(), "the failure is shown");

    fs::remove_dir_all(&blocker).expect("unblock");
    rig.router.flush();
    assert_eq!(rig.records().len(), 1, "offered again and written");
    let view = rig.router.view(&opened.id).expect("a view");
    assert_eq!(view.records_problem, None, "a good write clears it");
    rig.cleanup();
}

#[test]
fn a_damaged_records_file_is_reported() {
    let path = temp_path("router", "damaged");
    fs::write(&path, "{ not json").expect("damage");
    let rig = Rig::on(path);
    let opened = rig.open("work");
    let view = rig.router.view(&opened.id).expect("a view");
    assert!(view.records_problem.is_some());
    rig.cleanup();
}
