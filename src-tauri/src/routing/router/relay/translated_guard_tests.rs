//! The hard line of the translated route, through the router's own listener with a
//! `FakeUpstream` behind it: the Claude login and Anthropic's headers never reach `OpenAI`, the
//! `OpenAI` key reaches only `OpenAI`, no secret surfaces, the route needs both opt-ins, and a
//! request it cannot carry reaches no upstream. The `OpenAI` replies here are written from
//! `OpenAI`'s published documentation (U1 to U5, cited in `translated_tests.rs` where they are
//! written), and none was captured from a real account.

use super::tests::{
    ANTHROPIC_KEY, Got, LOGIN, OPENAI_KEY, OPENROUTER_KEY, Rig, Setup, Who, all_exhausted, body_of,
    canned, oai_text, rejected, spend_cap, who,
};
use super::translated_tests::{FAILED_SERVER, holds, managed, oai_error, oai_text_then, request};
use crate::routing::answers;
use crate::routing::config::{ConnectionKind, TierId};
use crate::routing::fixtures::{conn, secret_of};
use crate::routing::policy::Skip;
use crate::routing::translate::Carry;
use crate::routing::upstream::{ANTHROPIC, Canned, OPENAI, SendError, Sent};
use crate::settings::RoutingSettings;
use std::fs;

type Script = fn(&Sent) -> Result<Canned, SendError>;

/// Name, the order, the script, the body of both requests.
type Scenario = (
    &'static str,
    &'static [&'static str],
    Script,
    fn(&str) -> String,
);

const SCENARIOS: [Scenario; 11] = [
    (
        "a desk on oai alone",
        &["oai"],
        |_| Ok(oai_text("hi")),
        body_of,
    ),
    (
        "a move from the subscription to oai",
        &["sub", "oai"],
        |s| {
            Ok(if who(s) == Who::Sub {
                rejected()
            } else {
                oai_text("hi")
            })
        },
        body_of,
    ),
    (
        "a move from the Anthropic key to oai",
        &["api", "oai"],
        |s| {
            Ok(if who(s) == Who::Api {
                spend_cap()
            } else {
                oai_text("hi")
            })
        },
        body_of,
    ),
    (
        "a blocked order",
        &["sub", "api", "router", "oai"],
        |s| Ok(all_exhausted(s)),
        body_of,
    ),
    (
        "OpenAI answers 401",
        &["oai"],
        |_| {
            Ok(oai_error(
                401,
                vec![],
                Some("invalid_api_key"),
                "Wrong key.",
            ))
        },
        body_of,
    ),
    (
        "OpenAI answers 500",
        &["oai"],
        |_| Ok(oai_error(500, vec![], Some("server_error"), "Failed.")),
        body_of,
    ),
    (
        "OpenAI answers 307",
        &["oai"],
        |_| {
            Ok(canned(
                307,
                vec![("location", "https://elsewhere.test/v1/responses".to_owned())],
                "",
            ))
        },
        body_of,
    ),
    (
        "a stream that fails",
        &["oai"],
        |_| Ok(oai_text_then(&[FAILED_SERVER], vec![])),
        body_of,
    ),
    ("not sent", &["oai"], |_| Err(SendError::NotSent), body_of),
    ("dropped", &["oai"], |_| Err(SendError::Dropped), body_of),
    (
        "a request refused here",
        &["oai"],
        |_| Ok(oai_text("hi")),
        managed,
    ),
];

/// One scenario run: two requests.
struct Run {
    name: &'static str,
    rig: Rig,
    replies: Vec<Got>,
}

fn run(prefix: &str) -> Vec<Run> {
    SCENARIOS
        .iter()
        .enumerate()
        .map(|(i, (name, chain, script, body))| {
            let rig = Rig::new(&format!("{prefix}-{i}"), chain, *script);
            let replies = vec![rig.ask(&body("one")), rig.ask(&body("two"))];
            Run { name, rig, replies }
        })
        .collect()
}

fn to_openai(rig: &Rig) -> Vec<Sent> {
    rig.sent()
        .into_iter()
        .filter(|s| s.url.starts_with(OPENAI))
        .collect()
}

// Group 1.
#[test]
fn the_claude_login_and_anthropics_headers_never_reach_openai() {
    let mut reached = 0;
    for Run { name, rig, .. } in run("oai-login") {
        let secret = secret_of(&rig.opened);
        let never = ["test-login", &secret, "oauth-2025-04-20", "session-1"];
        for sent in to_openai(&rig) {
            reached += 1;
            let mut names: Vec<&str> = sent.headers.iter().map(|h| h.name.as_str()).collect();
            names.sort_unstable();
            assert_eq!(
                names,
                ["accept", "accept-encoding", "authorization", "content-type"],
                "{name}"
            );
            for value in sent.headers.iter().map(|h| &h.value).chain([&sent.body]) {
                for text in never {
                    assert!(!holds(value, text), "{name}: {text} reached OpenAI");
                }
            }
        }
        for sent in rig.sent() {
            if sent.headers.iter().any(|h| holds(&h.value, LOGIN)) {
                assert!(sent.url.starts_with(ANTHROPIC), "{name}: {}", sent.url);
            }
        }
        if name == "a request refused here" {
            assert!(rig.sent().is_empty(), "{name}");
        }
        rig.cleanup();
    }
    assert!(reached >= 10, "the scenarios reached OpenAI");
}

// Group 2.
#[test]
fn the_openai_key_reaches_only_openai() {
    let mut keyed = 0;
    for Run { name, rig, .. } in run("oai-key") {
        for sent in rig.sent() {
            assert!(!holds(&sent.body, OPENAI_KEY), "{name}: the key in a body");
            let to_openai = sent.url.starts_with(OPENAI);
            for header in &sent.headers {
                if holds(&header.value, OPENAI_KEY) {
                    keyed += 1;
                    assert!(to_openai, "{name}: {}", sent.url);
                    assert_eq!(header.name, "authorization", "{name}");
                    assert_eq!(
                        header.value,
                        format!("Bearer {OPENAI_KEY}").into_bytes(),
                        "{name}"
                    );
                }
            }
            if to_openai {
                for value in sent.headers.iter().map(|h| &h.value).chain([&sent.body]) {
                    for other in [ANTHROPIC_KEY, OPENROUTER_KEY] {
                        assert!(!holds(value, other), "{name}: another key reached OpenAI");
                    }
                }
            }
        }
        rig.cleanup();
    }
    assert!(keyed >= 10, "the scenarios sent the key");
}

// Group 3.
#[test]
fn a_secret_never_surfaces_on_the_openai_route() {
    for Run { name, rig, replies } in run("oai-secrets") {
        let secret = secret_of(&rig.opened);
        let secrets = [
            &secret,
            "test-login",
            ANTHROPIC_KEY,
            OPENROUTER_KEY,
            OPENAI_KEY,
        ];
        let mut texts: Vec<Vec<u8>> = replies
            .iter()
            .flat_map(|got| {
                let values = got.headers.iter().map(|(_, v)| v.clone().into_bytes());
                values.chain([got.body.clone()]).collect::<Vec<_>>()
            })
            .collect();
        texts.push(serde_json::to_vec(&rig.view()).expect("json"));
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

/// The `OpenAI` connection's own settings in the fixture.
fn oai_mut(settings: &mut RoutingSettings) -> &mut crate::routing::config::Connection {
    settings
        .connections
        .iter_mut()
        .find(|c| c.id == conn("oai"))
        .expect("oai")
}

/// The name of the connection under test, and how its settings are changed.
type OptIn = (&'static str, fn(&mut RoutingSettings) -> &'static str, Skip);

// Group 4.
#[test]
fn the_route_is_never_tried_without_both_opt_ins() {
    let rows: [OptIn; 5] = [
        (
            "paid use off",
            |s| {
                oai_mut(s).paid = false;
                "oai"
            },
            Skip::PaidNotAllowed,
        ),
        (
            "the experimental switch off",
            |s| {
                oai_mut(s).experimental = false;
                "oai"
            },
            Skip::ExperimentalNotAllowed,
        ),
        (
            "both off",
            |s| {
                let c = oai_mut(s);
                c.paid = false;
                c.experimental = false;
                "oai"
            },
            Skip::PaidNotAllowed,
        ),
        ("no key saved", |_| "oai", Skip::NoCredential),
        (
            "a Sign in with ChatGPT connection with everything on",
            |s| {
                let mut sign_in = oai_mut(s).clone();
                sign_in.id = conn("siwc");
                sign_in.name = "siwc".to_owned();
                sign_in.kind = ConnectionKind::ChatGptSignIn;
                (sign_in.paid, sign_in.experimental) = (true, true);
                s.connections.push(sign_in);
                "siwc"
            },
            Skip::NoCredential,
        ),
    ];
    for (i, (name, change, skip)) in rows.into_iter().enumerate() {
        let mut setup = Setup::chain(&["sub"]);
        let under_test = change(&mut setup.settings);
        setup.settings.contexts[0].chain.push(conn(under_test));
        let rig = Rig::with(&format!("oai-opt-in-{i}"), setup, |s| {
            Ok(if who(s) == Who::Sub {
                rejected()
            } else {
                oai_text("hi")
            })
        });
        if name == "no key saved" {
            rig.keys
                .set(ConnectionKind::OpenAiApiKey, &conn("oai"), None, || true)
                .expect("key removed");
        }
        assert_eq!(rig.ask(&body_of("one")).status, 429, "{name}");
        let blocked = rig.ask(&body_of("two"));
        let kind = rig
            .settings
            .connections
            .iter()
            .find(|c| c.id == conn(under_test))
            .expect("listed")
            .kind;
        let expected = answers::blocked(&[
            ("sub", ConnectionKind::ClaudeSubscription, &Skip::Exhausted),
            (under_test, kind, &skip),
        ]);
        assert_eq!(
            (blocked.status, blocked.body),
            (400, expected.body()),
            "{name}"
        );
        assert!(to_openai(&rig).is_empty(), "{name}: OpenAI was called");
        assert!(rig.sent().iter().all(|s| who(s) == Who::Sub), "{name}");
        rig.cleanup();
    }

    let rig = Rig::new("oai-withdrawn", &["oai"], |_| Ok(oai_text("hi")));
    assert_eq!(rig.ask(&body_of("one")).status, 200);
    let mut withdrawn = rig.settings.clone();
    oai_mut(&mut withdrawn).experimental = false;
    rig.router.apply_preferences(&withdrawn);
    let got = rig.ask(&body_of("two"));
    let expected = answers::refused(
        "oai",
        ConnectionKind::OpenAiApiKey,
        &Skip::ExperimentalNotAllowed,
    );
    assert_eq!((got.status, got.body), (400, expected.body()));
    assert_eq!(to_openai(&rig).len(), 1, "no call once withdrawn");
    rig.cleanup();
}

// Group 5.
#[test]
fn a_request_the_route_cannot_carry_reaches_no_upstream_at_all() {
    let document = r#"[{"role":"user","content":[{"type":"document","source":{"type":"text","media_type":"text/plain","data":"d"}}]}]"#;
    let server_tool = r#","tools":[{"type":"web_search_20250305","name":"web_search"}]"#;
    let strict_tool =
        r#","tools":[{"name":"Read","input_schema":{"type":"object"},"strict":true}]"#;
    let effort_max = r#"[{"role":"user","content":"x"},{"role":"system","content":[],"output_config":{"effort":"max"}}]"#;
    let unstreamed = format!(
        r#"{{"model":"{}","max_tokens":64,"messages":[{{"role":"user","content":"x"}}]}}"#,
        super::tests::OPUS
    );
    let effort = Skip::EffortUnmapped {
        tier: TierId::Opus,
        effort: "max".to_owned(),
    };
    let kind = ConnectionKind::OpenAiApiKey;
    let rows: [(&str, String, String); 8] = [
        (
            "a document block",
            request(document, ""),
            "it cannot carry documents".to_owned(),
        ),
        (
            "a tool the provider runs",
            request(r#"[{"role":"user","content":"x"}]"#, server_tool),
            "it cannot carry tools the provider runs".to_owned(),
        ),
        (
            "context_management",
            managed("x"),
            message(&answers::cannot_carry("oai", &Carry::ContextManagement)),
        ),
        (
            "a stop sequence",
            request(
                r#"[{"role":"user","content":"x"}]"#,
                r#","stop_sequences":["END"]"#,
            ),
            message(&answers::cannot_carry("oai", &Carry::StopSequences)),
        ),
        (
            "no stream",
            unstreamed,
            message(&answers::cannot_carry("oai", &Carry::NotStreamed)),
        ),
        (
            "temperature",
            request(
                r#"[{"role":"user","content":"x"}]"#,
                r#","temperature":0.5"#,
            ),
            message(&answers::cannot_carry(
                "oai",
                &Carry::Member {
                    name: "temperature".to_owned(),
                },
            )),
        ),
        (
            "a strict tool",
            request(r#"[{"role":"user","content":"x"}]"#, strict_tool),
            message(&answers::cannot_carry("oai", &Carry::Tool)),
        ),
        (
            "a per-message effort with no pair",
            request(effort_max, ""),
            message(&answers::refused("oai", kind, &effort)),
        ),
    ];
    for (i, (name, body, says)) in rows.into_iter().enumerate() {
        let rig = Rig::new(&format!("oai-carry-{i}"), &["oai"], |_| Ok(oai_text("hi")));
        assert_eq!(rig.ask(&body_of("first")).status, 200, "{name}");
        let before = rig.view().status;
        let got = rig.ask(&body);
        assert_eq!(got.status, 400, "{name}");
        let text = String::from_utf8_lossy(&got.body);
        assert!(text.contains(&says), "{name}: {text}");
        assert!(text.contains("invalid_request_error"), "{name}");
        assert_eq!(rig.sent().len(), 1, "{name}: an upstream call");
        let after = rig.view().status;
        assert_eq!(
            (&after.active, after.generation),
            (&before.active, before.generation),
            "{name}"
        );
        assert_eq!(
            rig.ask(&body_of("next")).status,
            200,
            "{name}: the next request"
        );
        assert_eq!(to_openai(&rig).len(), 2, "{name}");
        rig.cleanup();
    }
}

/// The sentence an answer carries, as it reads inside the JSON envelope.
fn message(answer: &answers::Answer) -> String {
    let json = serde_json::to_string(&answer.message).expect("a string");
    json.trim_matches('"').to_owned()
}
