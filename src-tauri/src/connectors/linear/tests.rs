use super::wire::ViewerData;
use super::*;
use crate::http::{FakeHttp, Sent};
use crate::model::LinearProject;
use crate::secrets::MemorySecrets;
use crate::settings::{ConnectorSettings, SettingsPatch};
use serde_json::{Value, json};
use std::path::PathBuf;

const KEY: &str = "test-key";
const TEAM: &str = "0b0e0c1a-0000-4000-8000-000000000001";
const ISSUE: &str = "0b0e0c1a-0000-4000-8000-000000000101";

const VIEWER_JSON: &str = include_str!("fixtures/viewer.json");
const TEAMS_JSON: &str = include_str!("fixtures/teams.json");
const PAGE1_JSON: &str = include_str!("fixtures/issues_page1.json");
const PAGE2_JSON: &str = include_str!("fixtures/issues_page2.json");
const STATES_JSON: &str = include_str!("fixtures/states.json");
const TICK_OK_JSON: &str = include_str!("fixtures/tick_ok.json");
const TICK_REFUSED_JSON: &str = include_str!("fixtures/tick_refused.json");
const RATELIMITED_JSON: &str = include_str!("fixtures/error_ratelimited.json");
const AUTH_JSON: &str = include_str!("fixtures/error_auth.json");
const INVALID_JSON: &str = include_str!("fixtures/error_invalid.json");

type Answer = Result<Reply, HttpError>;
type Workspace = fn(&Sent) -> Answer;

#[expect(
    clippy::unnecessary_wraps,
    reason = "the fakes answer with a Result, and this builds the replies they play"
)]
fn reply(status: u16, body: &str) -> Answer {
    Ok(Reply {
        status,
        retry_after: None,
        body: body.to_string(),
    })
}

fn ok(body: &str) -> Answer {
    reply(200, body)
}

fn body(sent: &Sent) -> Value {
    serde_json::from_str(&sent.body).expect("the body is JSON")
}

/// The operation's name: "Viewer", "Issues", "Tick", …
fn op(sent: &Sent) -> String {
    let query = body(sent)["query"].as_str().expect("a query").to_string();
    query
        .split_whitespace()
        .nth(1)
        .expect("a name")
        .split('(')
        .next()
        .expect("a name")
        .to_string()
}

fn ops(http: &FakeHttp) -> Vec<String> {
    http.sent().iter().map(op).collect()
}

/// A workspace that answers every query from the fixtures.
fn workspace(sent: &Sent) -> Answer {
    match op(sent).as_str() {
        "Viewer" => ok(VIEWER_JSON),
        "Teams" => ok(TEAMS_JSON),
        "Issues" if body(sent)["variables"]["after"].is_null() => ok(PAGE1_JSON),
        "Issues" => ok(PAGE2_JSON),
        "CompletedStates" => ok(STATES_JSON),
        "Tick" => ok(TICK_OK_JSON),
        other => panic!("unexpected operation {other}"),
    }
}

struct Rig {
    linear: Linear,
    http: Arc<FakeHttp>,
    secrets: Arc<MemorySecrets>,
    settings: Arc<SettingsStore>,
    dir: PathBuf,
}

impl Drop for Rig {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn rig(
    name: &str,
    secrets: MemorySecrets,
    answer: impl Fn(&Sent) -> Answer + Send + Sync + 'static,
) -> Rig {
    let dir = std::env::temp_dir().join(format!("ob-linear-{}-{name}", std::process::id()));
    let http = Arc::new(FakeHttp::new(answer));
    let secrets = Arc::new(secrets);
    let settings = Arc::new(SettingsStore::open(dir.join("settings.json")));
    let linear = Linear::new(http.clone(), secrets.clone(), settings.clone());
    Rig {
        linear,
        http,
        secrets,
        settings,
        dir,
    }
}

fn keyed() -> MemorySecrets {
    MemorySecrets::new().with(KEY_ACCOUNT, KEY)
}

fn link(project: Option<&str>, label: Option<&str>) -> Link {
    Link::Linear {
        team_id: TEAM.to_string(),
        team_key: "ENG".to_string(),
        team_name: "Engineering".to_string(),
        project: project.map(|id| LinearProject {
            id: id.to_string(),
            name: "Checkout".to_string(),
        }),
        label: label.map(str::to_string),
    }
}

fn plain() -> Link {
    link(None, None)
}

fn no_key(why: &str) -> ConnectorError {
    ConnectorError::NoKey(why.to_string())
}

fn refused(why: &str) -> ConnectorError {
    ConnectorError::Refused(why.to_string())
}

#[test]
fn a_request_goes_to_linear_with_the_bare_key_and_a_graphql_body() {
    let rig = rig("request", keyed(), workspace);
    rig.linear.list(&plain()).expect("listed");
    let sent = rig.http.sent();
    assert_eq!(sent[0].url, ENDPOINT);
    assert_eq!(sent[0].authorization.as_deref(), Some(KEY));
    let body = body(&sent[0]);
    assert!(
        body["query"]
            .as_str()
            .expect("query")
            .starts_with("query Issues(")
    );
    assert!(body["variables"].is_object());
}

#[test]
fn the_filter_holds_the_project_the_label_and_the_agent_label_from_settings() {
    let rig = rig("filter-full", keyed(), workspace);
    rig.settings
        .update(SettingsPatch {
            connectors: Some(ConnectorSettings {
                linear_agent_label: "robot".to_string(),
            }),
            ..SettingsPatch::default()
        })
        .expect("saved");
    rig.linear
        .list(&link(Some("P"), Some("bug")))
        .expect("listed");
    let want = json!({ "and": [
        { "team": { "id": { "eq": TEAM } } },
        { "state": { "type": { "in": ["unstarted", "started"] } } },
        { "project": { "id": { "eq": "P" } } },
        { "labels": { "name": { "eqIgnoreCase": "bug" } } },
        { "or": [
            { "assignee": { "isMe": { "eq": true } } },
            { "assignee": { "null": true } },
            { "labels": { "name": { "eqIgnoreCase": "robot" } } }
        ] }
    ] });
    assert_eq!(body(&rig.http.sent()[0])["variables"]["filter"], want);
}

#[test]
fn the_filter_leaves_out_a_missing_project_and_label() {
    let rig = rig("filter-plain", keyed(), workspace);
    rig.linear.list(&plain()).expect("listed");
    let want = json!({ "and": [
        { "team": { "id": { "eq": TEAM } } },
        { "state": { "type": { "in": ["unstarted", "started"] } } },
        { "or": [
            { "assignee": { "isMe": { "eq": true } } },
            { "assignee": { "null": true } },
            { "labels": { "name": { "eqIgnoreCase": "agent" } } }
        ] }
    ] });
    assert_eq!(body(&rig.http.sent()[0])["variables"]["filter"], want);
}

#[test]
fn board_splits_by_label_then_assignee() {
    let none: Vec<String> = Vec::new();
    let agent = vec!["Bug".to_string(), "AGENT".to_string()];
    assert_eq!(board(None, &agent, "agent"), Some(Party::Ai));
    assert_eq!(board(Some(true), &agent, "Agent"), Some(Party::Ai));
    assert_eq!(board(Some(false), &agent, "agent"), Some(Party::Ai));
    assert_eq!(board(Some(true), &none, "agent"), Some(Party::Human));
    assert_eq!(board(None, &none, "agent"), Some(Party::Ai));
    assert_eq!(board(Some(false), &none, "agent"), None);
    assert_eq!(board(Some(false), &["Bug".to_string()], "agent"), None);
}

#[test]
fn a_listing_sorts_issues_onto_boards_in_linears_order() {
    let rig = rig("listing", keyed(), workspace);
    let listing = rig.linear.list(&plain()).expect("listed");
    let got: Vec<(String, Party, TodoSource)> = listing
        .items
        .iter()
        .map(|i| (i.external_id.clone(), i.assignee, i.source.clone()))
        .collect();
    let item = |n: &str, party: Party, slug: &str| {
        (
            format!("0b0e0c1a-0000-4000-8000-000000000{n}"),
            party,
            TodoSource::Linear {
                identifier: format!("ENG-{n}"),
                url: format!("https://linear.app/example/issue/ENG-{n}/{slug}"),
            },
        )
    };
    assert_eq!(
        got,
        vec![
            item("101", Party::Human, "check-the-refund-email-copy"),
            item("102", Party::Ai, "fix-the-orders-ingest-dedupe"),
            item("103", Party::Ai, "add-retries-to-the-tile-fetcher"),
            item("105", Party::Ai, "write-the-checkout-smoke-test"),
            item("106", Party::Ai, "rotate-the-tile-api-key"),
        ]
    );
    assert_eq!(listing.items[1].text, "Fix the orders ingest dedupe");
    assert!(!listing.truncated);
}

#[test]
fn the_second_page_is_asked_with_the_first_pages_cursor() {
    let rig = rig("cursor", keyed(), workspace);
    rig.linear.list(&plain()).expect("listed");
    let sent = rig.http.sent();
    assert_eq!(sent.len(), 2);
    assert!(body(&sent[0])["variables"]["after"].is_null());
    assert_eq!(body(&sent[1])["variables"]["after"], "cursor-page-1");
}

#[test]
fn a_floor_with_endless_pages_stops_after_four_and_is_truncated() {
    let rig = rig("endless", keyed(), |_| ok(PAGE1_JSON));
    let listing = rig.linear.list(&plain()).expect("listed");
    assert_eq!(rig.http.sent().len(), 4);
    assert_eq!(listing.items.len(), 16);
    assert!(listing.truncated);
}

#[test]
fn a_next_page_without_a_cursor_is_truncated() {
    let page = PAGE1_JSON.replace("\"cursor-page-1\"", "null");
    let rig = rig("no-cursor", keyed(), move |_| ok(&page));
    let listing = rig.linear.list(&plain()).expect("listed");
    assert_eq!(rig.http.sent().len(), 1);
    assert!(listing.truncated);
}

#[test]
fn a_long_title_is_cut_to_300_characters() {
    let long = "x".repeat(400);
    let page = json!({ "data": { "issues": {
        "nodes": [{ "id": ISSUE, "identifier": "ENG-101", "title": long,
                    "url": "https://linear.app/example/issue/ENG-101/x",
                    "assignee": null, "labels": { "nodes": [] } }],
        "pageInfo": { "hasNextPage": false, "endCursor": null } } } })
    .to_string();
    let rig = rig("long-title", keyed(), move |_| ok(&page));
    let listing = rig.linear.list(&plain()).expect("listed");
    assert_eq!(listing.items[0].text.chars().count(), 300);
}

#[test]
fn read_maps_each_failure_in_order() {
    let get = |answer: Answer| read::<ViewerData>(answer).map(|v| v.viewer.name);
    let limited = Ok(Reply {
        status: 429,
        retry_after: Some(Duration::from_secs(30)),
        body: String::new(),
    });
    assert_eq!(
        get(limited),
        Err(ConnectorError::RateLimited(Some(Duration::from_secs(30))))
    );
    assert_eq!(
        get(ok(RATELIMITED_JSON)),
        Err(ConnectorError::RateLimited(None))
    );
    assert_eq!(get(reply(401, "")), Err(no_key(KEY_REFUSED)));
    assert_eq!(get(ok(AUTH_JSON)), Err(no_key(KEY_REFUSED)));
    assert_eq!(
        get(reply(400, INVALID_JSON)),
        Err(refused("Linear said: Team not found."))
    );
    assert_eq!(
        get(reply(503, "")),
        Err(ConnectorError::Unreachable(
            "Linear had a problem (HTTP 503)".to_string()
        ))
    );
    assert_eq!(
        get(Err(HttpError::Offline)),
        Err(ConnectorError::Unreachable(
            "Linear could not be asked: it could not be reached".to_string()
        ))
    );
    assert_eq!(
        get(ok(r#"{ "data": { "somethingElse": 1 } }"#)),
        Err(refused("Linear sent an answer the app could not read"))
    );
    assert_eq!(
        get(reply(404, "not json")),
        Err(refused("Linear answered with HTTP 404"))
    );
    assert_eq!(get(ok(VIEWER_JSON)), Ok("Demo User".to_string()));
}

#[test]
fn read_falls_back_to_the_message_and_cuts_it() {
    let long = json!({ "errors": [{ "message": "y".repeat(500) }] }).to_string();
    let Err(ConnectorError::Refused(words)) = read::<ViewerData>(ok(&long)) else {
        panic!("refused");
    };
    assert_eq!(
        words.chars().count(),
        "Linear said: ".chars().count() + WORDS_MAX
    );
}

#[test]
fn no_error_holds_the_key() {
    let answers: [Workspace; 8] = [
        |_| reply(401, ""),
        |_| ok(AUTH_JSON),
        |_| ok(RATELIMITED_JSON),
        |_| reply(400, INVALID_JSON),
        |_| reply(503, ""),
        |_| Err(HttpError::Offline),
        |_| ok("{}"),
        |_| reply(404, ""),
    ];
    for (n, answer) in answers.into_iter().enumerate() {
        let rig = rig(&format!("hygiene-{n}"), keyed(), answer);
        let error = rig.linear.list(&plain()).expect_err("fails");
        let trial = rig
            .linear
            .set_key(Some("other-test-key".to_string()))
            .expect_err("fails");
        let teams = rig.linear.teams().expect_err("fails");
        for text in [
            error.words(ConnectorKind::Linear),
            format!("{error:?}"),
            trial,
            teams,
        ] {
            assert!(!text.contains(KEY), "case {n}");
        }
    }
}

#[test]
fn a_refused_saved_key_stops_requests_until_recheck() {
    let rig = rig("refused-key", keyed(), |_| reply(401, ""));
    assert_eq!(rig.linear.list(&plain()), Err(no_key(KEY_REFUSED)));
    assert_eq!(rig.http.sent().len(), 1);
    assert_eq!(rig.linear.list(&plain()), Err(no_key(KEY_REFUSED)));
    assert_eq!(rig.linear.tick(&plain(), ISSUE), Err(no_key(KEY_REFUSED)));
    assert_eq!(rig.http.sent().len(), 1);
    assert_eq!(rig.linear.key_status(), LinearKeyStatus::Rejected);
    rig.linear.recheck();
    assert_eq!(
        rig.linear.key_status(),
        LinearKeyStatus::Saved { account: None }
    );
    assert_eq!(rig.linear.list(&plain()), Err(no_key(KEY_REFUSED)));
    assert_eq!(rig.http.sent().len(), 2);
    assert_eq!(rig.secrets.reads(), 1);
}

#[test]
fn no_saved_key_sends_nothing_and_reads_the_keychain_once() {
    let rig = rig("no-key", MemorySecrets::new(), workspace);
    assert_eq!(rig.linear.list(&plain()), Err(no_key(NO_KEY)));
    assert_eq!(rig.linear.tick(&plain(), ISSUE), Err(no_key(NO_KEY)));
    assert_eq!(rig.linear.key_status(), LinearKeyStatus::Missing);
    assert!(rig.http.sent().is_empty());
    assert_eq!(rig.secrets.reads(), 1);
}

#[test]
fn a_keychain_refusal_is_remembered_until_recheck() {
    let rig = rig("keychain-refused", keyed(), workspace);
    rig.secrets.fail_with("the Keychain is locked");
    let locked = no_key("the Keychain is locked");
    assert_eq!(rig.linear.list(&plain()), Err(locked.clone()));
    assert_eq!(rig.linear.list(&plain()), Err(locked));
    assert_eq!(rig.secrets.reads(), 1);
    assert_eq!(
        rig.linear.key_status(),
        LinearKeyStatus::Unavailable {
            reason: "the Keychain is locked".to_string()
        }
    );
    rig.linear.recheck();
    assert!(rig.linear.list(&plain()).is_err());
    assert_eq!(rig.secrets.reads(), 2);
    assert!(rig.http.sent().is_empty());
}

#[test]
fn set_key_checks_a_good_key_with_viewer_and_saves_it() {
    let rig = rig("set-good", MemorySecrets::new(), workspace);
    let status = rig.linear.set_key(Some("  new-test-key ".to_string()));
    assert_eq!(
        status,
        Ok(LinearKeyStatus::Saved {
            account: Some("Demo User".to_string())
        })
    );
    let sent = rig.http.sent();
    assert_eq!(ops(&rig.http), ["Viewer"]);
    assert_eq!(sent[0].authorization.as_deref(), Some("new-test-key"));
    assert_eq!(
        rig.secrets.get(KEY_ACCOUNT),
        Ok(Some(Secret::new("new-test-key".to_string())))
    );
    assert_eq!(
        rig.linear.key_status(),
        LinearKeyStatus::Saved {
            account: Some("Demo User".to_string())
        }
    );
    rig.linear.list(&plain()).expect("listed");
    assert_eq!(
        rig.http.sent()[1].authorization.as_deref(),
        Some("new-test-key")
    );
}

#[test]
fn set_key_saves_nothing_when_linear_refuses_or_cannot_be_asked() {
    let cases: [(Workspace, &str); 3] = [
        (|_| reply(401, ""), "Linear did not accept that key"),
        (
            |_| Err(HttpError::Offline),
            "the key could not be checked, so it was not saved: Linear could not be asked: it could not be reached",
        ),
        (
            |_| reply(429, ""),
            "Linear asked the app to slow down, so the key was not saved. Try again in a minute",
        ),
    ];
    for (n, (answer, want)) in cases.into_iter().enumerate() {
        let rig = rig(&format!("set-refused-{n}"), keyed(), answer);
        assert_eq!(
            rig.linear.set_key(Some("new-test-key".to_string())),
            Err(want.to_string())
        );
        assert_eq!(
            rig.secrets.get(KEY_ACCOUNT),
            Ok(Some(Secret::new(KEY.to_string())))
        );
        // A refused trial key leaves the saved one as it was.
        assert_eq!(
            rig.linear.key_status(),
            LinearKeyStatus::Saved { account: None }
        );
    }
}

#[test]
fn set_key_refuses_a_malformed_key_without_asking() {
    let rig = rig("set-malformed", MemorySecrets::new(), workspace);
    for key in ["   ", "two words", &"k".repeat(201)] {
        assert_eq!(
            rig.linear.set_key(Some(key.to_string())),
            Err("that does not look like a Linear API key".to_string())
        );
    }
    assert!(rig.http.sent().is_empty());
    assert_eq!(rig.secrets.get(KEY_ACCOUNT), Ok(None));
}

#[test]
fn set_key_reports_a_keychain_write_failure_and_keeps_the_state() {
    let rig = rig("set-write-fails", keyed(), workspace);
    assert_eq!(
        rig.linear.key_status(),
        LinearKeyStatus::Saved { account: None }
    );
    rig.secrets.fail_with("the Keychain did not save the key");
    assert_eq!(
        rig.linear.set_key(Some("new-test-key".to_string())),
        Err("the Keychain did not save the key".to_string())
    );
    assert_eq!(
        rig.linear.key_status(),
        LinearKeyStatus::Saved { account: None }
    );
    rig.linear.list(&plain()).expect("listed");
    assert_eq!(rig.http.sent()[1].authorization.as_deref(), Some(KEY));
}

#[test]
fn set_key_none_removes_the_key() {
    let rig = rig("set-none", keyed(), workspace);
    assert_eq!(rig.linear.set_key(None), Ok(LinearKeyStatus::Missing));
    assert_eq!(rig.secrets.get(KEY_ACCOUNT), Ok(None));
    assert_eq!(rig.linear.key_status(), LinearKeyStatus::Missing);
    assert_eq!(rig.linear.list(&plain()), Err(no_key(NO_KEY)));
    assert!(rig.http.sent().is_empty());
}

#[test]
fn a_tick_uses_the_lowest_completed_status_and_caches_it() {
    let rig = rig("tick", keyed(), workspace);
    assert_eq!(rig.linear.tick(&plain(), ISSUE), Ok(()));
    let sent = rig.http.sent();
    assert_eq!(ops(&rig.http), ["CompletedStates", "Tick"]);
    assert_eq!(body(&sent[0])["variables"], json!({ "teamId": TEAM }));
    assert_eq!(
        body(&sent[1])["variables"],
        json!({ "id": ISSUE, "stateId": "0b0e0c1a-0000-4000-8000-0000000000c1" })
    );
    assert_eq!(rig.linear.tick(&plain(), ISSUE), Ok(()));
    assert_eq!(ops(&rig.http), ["CompletedStates", "Tick", "Tick"]);
}

#[test]
fn a_refused_tick_says_why_and_asks_for_the_status_again() {
    let rig = rig("tick-refused", keyed(), |sent| match op(sent).as_str() {
        "CompletedStates" => ok(STATES_JSON),
        _ => ok(TICK_REFUSED_JSON),
    });
    let want = Err(refused(
        "Linear said: The status does not belong to the issue's team.",
    ));
    assert_eq!(rig.linear.tick(&plain(), ISSUE), want);
    assert_eq!(rig.linear.tick(&plain(), ISSUE), want);
    assert_eq!(
        ops(&rig.http),
        ["CompletedStates", "Tick", "CompletedStates", "Tick"]
    );
}

#[test]
fn a_tick_that_did_not_complete_the_issue_is_refused() {
    for (n, data) in [
        r#"{ "data": { "issueUpdate": { "success": false, "issue": null } } }"#,
        r#"{ "data": { "issueUpdate": { "success": true, "issue": { "id": "x", "state": { "type": "started" } } } } }"#,
    ]
    .into_iter()
    .enumerate()
    {
        let rig = rig(&format!("tick-undone-{n}"), keyed(), move |sent| {
            match op(sent).as_str() {
                "CompletedStates" => ok(STATES_JSON),
                _ => ok(data),
            }
        });
        assert_eq!(
            rig.linear.tick(&plain(), ISSUE),
            Err(refused("Linear did not mark the issue as done"))
        );
    }
}

#[test]
fn a_team_with_no_completed_status_cannot_be_ticked() {
    let empty = r#"{ "data": { "team": { "states": { "nodes": [] } } } }"#;
    let rig = rig("tick-no-status", keyed(), move |_| ok(empty));
    assert_eq!(
        rig.linear.tick(&plain(), ISSUE),
        Err(refused("this Linear team has no completed status"))
    );
    assert_eq!(ops(&rig.http), ["CompletedStates"]);
}

#[test]
fn teams_lists_each_team_with_its_open_projects() {
    let rig = rig("teams", keyed(), workspace);
    let teams = rig.linear.teams().expect("teams");
    let project = |id: &str, name: &str| LinearProject {
        id: format!("0b0e0c1a-0000-4000-8000-0000000000{id}"),
        name: name.to_string(),
    };
    assert_eq!(
        teams,
        vec![
            LinearTeam {
                id: TEAM.to_string(),
                key: "ENG".to_string(),
                name: "Engineering".to_string(),
                projects: vec![project("aa", "Checkout"), project("ab", "Orders ingest")],
            },
            LinearTeam {
                id: "0b0e0c1a-0000-4000-8000-000000000002".to_string(),
                key: "OPS".to_string(),
                name: "Operations".to_string(),
                projects: Vec::new(),
            },
        ]
    );
    assert_eq!(ops(&rig.http), ["Teams"]);
}

#[test]
fn teams_fails_without_a_key() {
    let rig = rig("teams-no-key", MemorySecrets::new(), workspace);
    assert_eq!(rig.linear.teams(), Err(NO_KEY.to_string()));
    assert!(rig.http.sent().is_empty());
}

#[test]
fn it_is_the_linear_connector_polled_every_minute() {
    let rig = rig("identity", MemorySecrets::new(), workspace);
    assert_eq!(rig.linear.kind(), ConnectorKind::Linear);
    assert_eq!(rig.linear.poll_interval(), Duration::from_secs(60));
}
