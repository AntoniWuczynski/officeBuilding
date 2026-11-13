//! The credential and secret tables of the Messages relay: the login reaches Anthropic only, a
//! key reaches only its own vendor, no secret surfaces, and money and experiments need their
//! opt-in. Each scenario drives the router's own listener with a `FakeUpstream` behind it.

use super::tests::{
    ANTHROPIC_KEY, Gates, Got, LOGIN, OPENROUTER_KEY, Rig, Setup, Who, all_exhausted, body_of,
    canned, client_headers, has, ok, rejected, row, send, wait_for, who,
};
use crate::routing::answers;
use crate::routing::body;
use crate::routing::config::{ConnectionKind, TierTarget};
use crate::routing::fixtures::{conn, ctx};
use crate::routing::policy::Skip;
use crate::routing::records::ExhaustionFile;
use crate::routing::router::Router;
use crate::routing::upstream::{
    self, ANTHROPIC, Bases, Canned, FakeUpstream, OPENROUTER, SendError, Sent, Step, Upstream,
};
use crate::routing::wire::{Header, Method, Request};
use crate::sync::lock;
use std::collections::BTreeMap;
use std::fs;
use std::sync::Arc;

type Script = fn(&Sent) -> Result<Canned, SendError>;

/// Name, the order, the script.
const SCENARIOS: [(&str, &[&str], Script); 8] = [
    (
        "a move from the subscription to the key",
        &["sub", "api"],
        |s| Ok(moves(s)),
    ),
    ("a move to OpenRouter", &["sub", "router"], |s| Ok(moves(s))),
    ("a redirect", &["sub", "api"], |_| {
        Ok(canned(
            307,
            vec![("location", "https://elsewhere.test/v1/messages".to_owned())],
            "",
        ))
    }),
    ("not sent", &["sub", "api"], |_| Err(SendError::NotSent)),
    ("dropped", &["sub", "api"], |_| Err(SendError::Dropped)),
    ("a 401", &["sub", "api"], |_| Ok(row(401, vec![]))),
    ("a 500", &["sub", "api"], |_| Ok(row(500, vec![]))),
    ("a blocked chain", &["sub", "api", "router", "oai"], |s| {
        Ok(all_exhausted(s))
    }),
];

/// The subscription is used up and every other connection answers.
fn moves(sent: &Sent) -> Canned {
    if who(sent) == Who::Sub {
        rejected()
    } else {
        ok()
    }
}

/// One scenario run: two requests, and the upstream calls the first one made.
struct Run {
    rig: Rig,
    replies: Vec<Got>,
    first_calls: usize,
}

fn run(prefix: &str) -> Vec<(&'static str, Run)> {
    SCENARIOS
        .iter()
        .enumerate()
        .map(|(i, (name, chain, script))| {
            let rig = Rig::new(&format!("{prefix}-{i}"), chain, *script);
            let first = rig.ask(&body_of("one"));
            let first_calls = rig.sent().len();
            let replies = vec![first, rig.ask(&body_of("two"))];
            let run = Run {
                rig,
                replies,
                first_calls,
            };
            (*name, run)
        })
        .collect()
}

fn holds(value: &[u8], text: &str) -> bool {
    value.windows(text.len()).any(|w| w == text.as_bytes())
}

#[test]
fn the_login_reaches_no_host_but_anthropics() {
    for (name, run) in run("login") {
        for sent in run.rig.sent() {
            if sent.headers.iter().any(|h| holds(&h.value, LOGIN)) {
                assert!(sent.url.starts_with(ANTHROPIC), "{name}: {}", sent.url);
            }
        }
        if name == "a redirect" {
            let got = &run.replies[0];
            assert_eq!(got.status, 502, "{name}");
            assert_eq!(got.body, answers::redirect("sub").body(), "{name}");
            assert!(got.header("location").is_none(), "{name}");
            assert_eq!(run.first_calls, 1, "{name}: a redirect is not asked again");
        }
        run.rig.cleanup();
    }
}

#[test]
fn a_key_reaches_only_its_own_vendor() {
    let mut keyed = 0;
    for (name, run) in run("keys") {
        for sent in run.rig.sent() {
            assert!(!holds(&sent.body, ANTHROPIC_KEY) && !holds(&sent.body, OPENROUTER_KEY));
            for header in &sent.headers {
                if holds(&header.value, ANTHROPIC_KEY) {
                    keyed += 1;
                    assert_eq!(header.name, "x-api-key", "{name}");
                    assert!(sent.url.starts_with(ANTHROPIC), "{name}");
                }
                if holds(&header.value, OPENROUTER_KEY) {
                    keyed += 1;
                    assert_eq!(header.name, "authorization", "{name}");
                    assert!(sent.url.starts_with(OPENROUTER), "{name}");
                }
            }
        }
        run.rig.cleanup();
    }
    assert!(keyed >= 2, "the scenarios sent both keys");
}

#[test]
fn a_secret_never_surfaces() {
    for (name, run) in run("secrets") {
        let rig = &run.rig;
        let (_, route_secret) = rig
            .opened
            .header
            .expose()
            .split_once(": ")
            .expect("name: value");
        let secrets = [route_secret, ANTHROPIC_KEY, OPENROUTER_KEY, "test-login"];
        let headers: Vec<Header> = client_headers(&rig.opened)
            .into_iter()
            .map(|(name, value)| Header {
                name,
                value: value.into_bytes(),
            })
            .collect();
        let request = Request {
            method: Method::Post,
            path: "/s/x/v1/messages".to_owned(),
            query: Some("beta=true".to_owned()),
            headers: headers.clone(),
            body: body_of("one").into_bytes(),
        };
        let mut texts: Vec<Vec<u8>> = run
            .replies
            .iter()
            .flat_map(|got| {
                let values = got.headers.iter().map(|(_, v)| v.clone().into_bytes());
                values.chain([got.body.clone()]).collect::<Vec<_>>()
            })
            .collect();
        texts.push(format!("{request:?}").into_bytes());
        texts.extend(headers.iter().map(|h| format!("{h:?}").into_bytes()));
        texts.push(format!("{:?}", rig.opened).into_bytes());
        texts.push(serde_json::to_vec(&rig.view()).expect("json"));
        let id = &rig.opened.id;
        for connection in ["api", "home-api", "oai", "nowhere"] {
            for result in [
                rig.router.retry(id, &conn(connection)),
                rig.router.switch(id, &conn(connection)),
            ] {
                if let Err(why) = result {
                    texts.push(why.into_bytes());
                }
            }
        }
        if let Ok(file) = fs::read(&rig.path) {
            texts.push(file);
        }
        for text in &texts {
            for secret in secrets {
                assert!(!holds(text, secret), "{name}: a secret surfaced");
            }
        }
        rig.cleanup();
    }
}

#[test]
fn pass_through_touches_nothing() {
    let reply: [&[u8]; 2] = [b"\x1f\x8b\x08\x00first", b"\x00\xffsecond"];
    let rig = Rig::new("pass-through", &["sub", "api"], move |_| {
        Ok(Canned {
            status: 200,
            headers: vec![
                ("content-type", "text/event-stream".to_owned()),
                ("content-encoding", "gzip".to_owned()),
                ("anthropic-ratelimit-unified-status", "allowed".to_owned()),
                ("anthropic-ratelimit-unified-reset", "1700000000".to_owned()),
            ],
            body: reply.iter().map(|b| Step::Bytes(b.to_vec())).collect(),
        })
    });
    let body = r#"{ "model" :"claude-opus-5-5",  "messages":[{"role":"assistant","content":[{"type":"thinking","thinking":"t","signature":"c2ln"},{"type":"text","text":"é"}]}],"max_tokens": 1.0e3 }"#;
    let got = rig.ask_with(body, &[("x-claude-code-agent-id", "agent-1")]);
    let sent = rig.sent();
    assert_eq!(sent.len(), 1);
    let call = &sent[0];
    assert_eq!(call.url, "https://api.anthropic.com/v1/messages?beta=true");
    assert_eq!(call.body, body.as_bytes(), "the body byte for byte");
    let value = |name: &str| {
        call.headers
            .iter()
            .find(|h| h.name == name)
            .map(|h| h.value.clone())
    };
    for (name, expected) in [
        ("authorization", LOGIN),
        ("anthropic-version", "2023-06-01"),
        ("anthropic-beta", "oauth-2025-04-20"),
        ("x-claude-code-session-id", "session-1"),
        ("x-claude-code-agent-id", "agent-1"),
        ("accept-encoding", "gzip, br"),
    ] {
        assert_eq!(value(name), Some(expected.as_bytes().to_vec()), "{name}");
    }
    assert_eq!(value(upstream::LOCAL_HEADER), None);
    assert_eq!(got.status, 200);
    assert!(got.ended);
    assert_eq!(got.body, reply.concat());
    assert_eq!(got.header("content-encoding"), Some("gzip"));
    assert_eq!(
        got.header("anthropic-ratelimit-unified-status"),
        Some("allowed")
    );
    assert_eq!(
        got.header("anthropic-ratelimit-unified-reset"),
        Some("1700000000")
    );
    rig.cleanup();
}

#[test]
fn a_session_closed_a_moment_before_reaches_no_upstream() {
    let rig = Rig::new("closed", &["sub"], |_| Ok(ok()));
    rig.router.close(&rig.opened.id);
    let got = rig.ask(&body_of("late"));
    assert_eq!(got.status, 400);
    assert_eq!(got.body, answers::unknown_desk().body());
    assert!(rig.sent().is_empty());
    rig.cleanup();
}

#[test]
fn a_blocked_chain_answers_400_and_stops() {
    let rig = Rig::new("blocked", &["sub", "api", "router", "oai"], |s| {
        Ok(all_exhausted(s))
    });
    let last = rig.ask(&body_of("one"));
    assert_eq!(last.status, 402, "the chain ends on the last real refusal");
    assert_eq!(last.header("x-row"), Some("last"));
    assert_eq!(rig.sent().len(), 3);
    let expected = answers::blocked(&[
        ("sub", ConnectionKind::ClaudeSubscription, &Skip::Exhausted),
        ("api", ConnectionKind::AnthropicApiKey, &Skip::Exhausted),
        ("router", ConnectionKind::OpenRouter, &Skip::Exhausted),
        ("oai", ConnectionKind::OpenAiApiKey, &Skip::NoCredential),
    ]);
    let blocked = rig.ask(&body_of("two"));
    assert_eq!(blocked.status, 400);
    assert_eq!(blocked.body, expected.body());
    assert!(String::from_utf8_lossy(&blocked.body).contains("invalid_request_error"));
    assert!(
        blocked
            .headers
            .iter()
            .all(|(n, _)| !n.starts_with("anthropic-ratelimit") && n != "retry-after")
    );
    for _ in 0..20 {
        assert_eq!(rig.ask(&body_of("more")).status, 400);
    }
    let second = rig
        .router
        .open(&rig.settings, &ctx("work"), false)
        .expect("opens")
        .expect("routed");
    assert_eq!(send(rig.port(), &second, &body_of("x"), &[]).status, 400);
    assert_eq!(rig.sent().len(), 3, "no upstream call once blocked");

    let fresh = Arc::new(FakeUpstream::new(|_| Ok(ok())));
    let upstream: Arc<dyn Upstream> = Arc::<FakeUpstream>::clone(&fresh);
    let again = Router::new(
        ExhaustionFile::new(rig.path.clone()),
        Arc::clone(&rig.keys),
        upstream,
        Bases::default(),
    );
    let opened = again
        .open(&rig.settings, &ctx("work"), false)
        .expect("opens")
        .expect("routed");
    let port = lock(&again.port).expect("listening");
    let got = send(port, &opened, &body_of("restart"), &[]);
    assert_eq!((got.status, got.body), (400, expected.body()));
    assert!(fresh.sent().is_empty(), "a restart reopened nothing");
    rig.cleanup();
}

#[test]
fn a_key_removed_after_admission_is_answered_without_a_call() {
    let gates = Gates::default();
    let held = gates.clone();
    let rig = Rig::new("key-removed", &["sub", "api"], move |sent| {
        Ok(match who(sent) {
            Who::Sub => {
                let mut refusal = rejected();
                refusal.body.insert(0, held.step());
                refusal
            }
            Who::Api | Who::Router => ok(),
        })
    });
    let pending = rig.ask_later(&body_of("one"));
    wait_for("the subscription call", || rig.calls(Who::Sub) == 1);
    rig.keys
        .set(ConnectionKind::AnthropicApiKey, &conn("api"), None, || true)
        .expect("key removed");
    gates.open();
    let got = pending.join().expect("reply");
    assert_eq!((got.status, got.body), (502, answers::no_key("api").body()));
    assert_eq!(rig.calls(Who::Api), 0);
    rig.cleanup();
}

#[test]
fn money_and_experiments_need_their_opt_in() {
    let mut setup = Setup::chain(&["sub", "api", "oai"]);
    setup.settings.connections[1].paid = false;
    let rig = Rig::with("opt-in", setup, |sent| {
        Ok(if who(sent) == Who::Sub {
            rejected()
        } else {
            ok()
        })
    });
    assert_eq!(rig.ask(&body_of("one")).status, 429);
    let blocked = rig.ask(&body_of("two"));
    assert_eq!(blocked.status, 400);
    let expected = answers::blocked(&[
        ("sub", ConnectionKind::ClaudeSubscription, &Skip::Exhausted),
        (
            "api",
            ConnectionKind::AnthropicApiKey,
            &Skip::PaidNotAllowed,
        ),
        ("oai", ConnectionKind::OpenAiApiKey, &Skip::NoCredential),
    ]);
    assert_eq!(blocked.body, expected.body());
    assert!(
        expected
            .message
            .contains("oai: this build cannot use that kind of connection yet")
    );
    assert!(rig.sent().iter().all(|s| who(s) == Who::Sub));
    rig.cleanup();

    let rig = Rig::new("withdrawn", &["api"], |_| Ok(ok()));
    assert_eq!(rig.ask(&body_of("one")).status, 200);
    let mut withdrawn = rig.settings.clone();
    withdrawn.connections[1].paid = false;
    rig.router.apply_preferences(&withdrawn);
    let got = rig.ask(&body_of("two"));
    let expected = answers::refused(
        "api",
        ConnectionKind::AnthropicApiKey,
        &Skip::PaidNotAllowed,
    );
    assert_eq!((got.status, got.body), (400, expected.body()));
    assert_eq!(rig.sent().len(), 1);
    rig.cleanup();
}

#[test]
fn a_key_connection_gets_its_target_and_every_other_byte() {
    let mut setup = Setup::chain(&["api"]);
    let targets = &mut setup.settings.connections[1].targets;
    targets.fable = Some(TierTarget {
        model: "claude-fable-5-2-relay".to_owned(),
        efforts: BTreeMap::from([("high".to_owned(), "max".to_owned())]),
    });
    targets.haiku = Some(TierTarget {
        model: "claude-haiku-5-5-relay".to_owned(),
        efforts: BTreeMap::new(),
    });
    let rig = Rig::with("rewrite", setup, |_| Ok(ok()));
    let kept = r#""messages":[{"role":"user","content":"main  é"}]"#;
    let metadata = r#""metadata":{"user_id":"u", "n":1.0e3}"#;
    let main = format!(
        r#"{{"model":"claude-fable-5-2",{kept},{metadata},"output_config":{{"effort":"high","x":[1, 2]}}}}"#
    );
    let helper = r#"{"model":"claude-haiku-5-5","messages":[{"role":"user","content":"helper"}]}"#;
    assert_eq!(rig.ask(&main).status, 200);
    assert_eq!(
        rig.ask_with(helper, &[("x-claude-code-agent-id", "agent-1")])
            .status,
        200
    );
    let sent = rig.sent();
    let main_call = sent.iter().find(|s| has(s, "main")).expect("main call");
    let helper_call = sent.iter().find(|s| has(s, "helper")).expect("helper call");
    let facts = body::read(&main_call.body).expect("readable");
    assert_eq!(
        (facts.model.as_str(), facts.effort.as_deref()),
        ("claude-fable-5-2-relay", Some("max"))
    );
    let text = String::from_utf8_lossy(&main_call.body);
    for member in [kept, metadata, r#""x":[1, 2]"#] {
        assert!(text.contains(member), "{member} lost its bytes in {text}");
    }
    assert!(
        helper_call
            .headers
            .iter()
            .any(|h| h.name == "x-claude-code-agent-id")
    );
    let facts = body::read(&helper_call.body).expect("readable");
    assert_eq!(facts.model, "claude-haiku-5-5-relay");

    let unknown = r#"{"model":"claude-unknown-9","messages":[]}"#;
    let got = rig.ask(unknown);
    assert_eq!(got.status, 400);
    assert!(String::from_utf8_lossy(&got.body).contains("the model claude-unknown-9"));
    assert_eq!(rig.sent().len(), 2);
    rig.cleanup();

    let rig = Rig::new("rewrite-sub", &["sub"], |_| Ok(ok()));
    assert_eq!(rig.ask(unknown).status, 200);
    assert_eq!(rig.sent()[0].body, unknown.as_bytes());
    rig.cleanup();
}
