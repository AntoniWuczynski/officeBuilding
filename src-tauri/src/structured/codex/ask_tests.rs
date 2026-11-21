//! Forms, permission scopes and command kinds as the human sees them. Every
//! request below was composed from the bundle `codex app-server
//! generate-json-schema` writes for codex-cli 0.161.0
//! (`ToolRequestUserInputParams.json`, `PermissionsRequestApprovalParams.json`,
//! `CommandExecutionRequestApprovalParams.json`, `McpServerElicitationRequestParams.json`)
//! and, for `humanCommand`, the same schema at rust-v0.170.0. None is replayed.

use super::mapper::{Mapper, Out};
use super::tests::answered;
use crate::structured::{Approval, Event, Field, FieldKind, Values};

/// A fresh mapper (working in `/work/app`) fed one request, and the approval it opened.
fn opened(line: &str) -> (Mapper, Approval) {
    let mut m = Mapper::new("/work/app", None, None, None);
    let approval = m.line(line).into_iter().find_map(|o| match o {
        Out::Event(Event::Approval(a)) => Some(a),
        _ => None,
    });
    let Some(approval) = approval else {
        panic!("no approval for {line}");
    };
    (m, approval)
}

fn sent(out: &[Out]) -> Vec<&str> {
    out.iter()
        .filter_map(|o| match o {
            Out::Send(s) => Some(s.as_str()),
            Out::Event(_) => None,
        })
        .collect()
}

fn labels(a: &Approval) -> Vec<(&str, bool)> {
    a.choices
        .iter()
        .map(|c| (c.label.as_str(), c.allow))
        .collect()
}

fn field(id: &str, label: &str, detail: &str, kind: FieldKind) -> Field {
    Field {
        id: id.to_string(),
        label: label.to_string(),
        detail: detail.to_string(),
        kind,
        required: true,
    }
}

const FORM: &str = r#"{"method":"item/tool/requestUserInput","id":3,"params":{"threadId":"00000000-0000-0000-0000-000000000001","turnId":"00000000-0000-0000-0000-000000000002","itemId":"item-1","isBlocking":true,"autoResolutionMs":null,"questions":[{"id":"q-one","header":"HEADER-ONE","question":"QUESTION-ONE","isOther":false,"isSecret":false,"options":[{"label":"OPTION-A","description":"MEANS-A"},{"label":"OPTION-B","description":""}]},{"id":"q-two","header":"","question":"QUESTION-TWO","options":null},{"id":"q-three","header":"HEADER-THREE","question":"QUESTION-THREE","isSecret":true,"options":null}]}}"#;

#[test]
fn a_form_keeps_each_question_in_its_own_field() {
    let (_, a) = opened(FORM);
    assert_eq!(
        (a.title.as_str(), a.detail.as_str(), a.background),
        ("Codex has 3 questions", "", false)
    );
    assert_eq!(
        a.fields,
        [
            field(
                "q-one",
                "HEADER-ONE",
                "QUESTION-ONE\nOPTION-A: MEANS-A",
                FieldKind::Pick {
                    options: vec!["OPTION-A".to_string(), "OPTION-B".to_string()],
                    other: false,
                    secret: false,
                },
            ),
            field("q-two", "QUESTION-TWO", "", FieldKind::Text),
            field(
                "q-three",
                "HEADER-THREE",
                "QUESTION-THREE",
                FieldKind::Secret
            ),
        ]
    );
    assert_eq!(labels(&a), [("Send", true)]);
}

#[test]
fn the_reply_to_a_form_is_one_message_keyed_by_the_original_ids() {
    let (mut m, _) = opened(FORM);
    let values: Values = [
        ("q-one", "OPTION-B"),
        ("q-two", "ANSWER-TWO"),
        ("q-three", "ANSWER-THREE"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect();
    let out = answered(&mut m, "3", "0", &values).unwrap();
    assert_eq!(
        sent(&out),
        [
            r#"{"id":3,"result":{"answers":{"q-one":{"answers":["OPTION-B"]},"q-three":{"answers":["ANSWER-THREE"]},"q-two":{"answers":["ANSWER-TWO"]}}}}"#
        ]
    );
}

#[test]
fn a_lone_question_with_options_may_take_other_text_and_be_masked() {
    let (_, a) = opened(
        r#"{"method":"item/tool/requestUserInput","id":4,"params":{"threadId":"t","turnId":"u","itemId":"i","isBlocking":true,"questions":[{"id":"q","header":"","question":"QUESTION-ONE","isOther":true,"isSecret":true,"options":[{"label":"OPTION-A","description":""}]}]}}"#,
    );
    assert_eq!(a.title, "QUESTION-ONE");
    assert_eq!(
        a.fields,
        [field(
            "q",
            "Your answer",
            "",
            FieldKind::Pick {
                options: vec!["OPTION-A".to_string()],
                other: true,
                secret: true,
            },
        )]
    );
}

#[test]
fn a_question_is_background_only_when_it_says_it_does_not_block() {
    for (blocking, background) in [
        (r#""isBlocking":true,"#, false),
        (r#""isBlocking":false,"#, true),
        ("", false),
    ] {
        let (_, a) = opened(&format!(
            r#"{{"method":"item/tool/requestUserInput","id":5,"params":{{"threadId":"t","turnId":"u","itemId":"i",{blocking}"questions":[{{"id":"q","header":"","question":"QUESTION-ONE","options":null}}]}}}}"#
        ));
        assert_eq!(a.background, background, "{blocking}");
    }
}

/// The `permissions` of a request asking for every path kind and access of F6.
/// Network comes first so the grant can be compared byte for byte.
const EVERY_SCOPE: &str = r#"{"network":{"enabled":true},"fileSystem":{"entries":[{"access":"read","path":{"type":"path","path":"/work/app/src"}},{"access":"write","path":{"type":"glob_pattern","pattern":"/work/app/**/*.tmp"}},{"access":"deny","path":{"type":"special","value":{"kind":"root"}}},{"access":"read","path":{"type":"special","value":{"kind":"minimal"}}},{"access":"write","path":{"type":"special","value":{"kind":"tmpdir"}}},{"access":"write","path":{"type":"special","value":{"kind":"slash_tmp"}}},{"access":"read","path":{"type":"special","value":{"kind":"project_roots"}}},{"access":"write","path":{"type":"special","value":{"kind":"project_roots","subpath":"docs"}}},{"access":"read","path":{"type":"special","value":{"kind":"unknown","path":"/opt/tool-x","subpath":"cache"}}},{"access":"read","path":{"type":"special","value":{"kind":"unknown","path":"/opt/tool-y","subpath":null}}}],"globScanMaxDepth":3,"read":["/work/shared"],"write":[]}}"#;

fn permission_request(permissions: &str) -> String {
    format!(
        r#"{{"method":"item/permissions/requestApproval","id":7,"params":{{"threadId":"t","turnId":"u","itemId":"p","environmentId":null,"cwd":"/work/other","startedAtMs":1,"reason":"REASON-X","permissions":{permissions}}}}}"#
    )
}

#[test]
fn a_permission_request_lists_every_part_and_echoes_it_when_granted() {
    let (mut m, a) = opened(&permission_request(EVERY_SCOPE));
    assert_eq!(
        a.detail.lines().collect::<Vec<_>>(),
        [
            "REASON-X",
            "in /work/other",
            "Network access",
            "Read: /work/app/src",
            "Write: every path matching /work/app/**/*.tmp",
            "No access: the whole disk",
            "Read: the system's minimal paths",
            "Write: the temporary folder",
            "Write: /tmp",
            "Read: the project folders",
            "Write: docs in the project folders",
            "Read: /opt/tool-x (cache)",
            "Read: /opt/tool-y",
            "Read: /work/shared",
            "Patterns are matched up to 3 folders deep",
        ]
    );
    assert_eq!(
        labels(&a),
        [
            ("Allow for this turn", true),
            ("Allow for this session", true),
            ("Decline", false),
        ]
    );
    let out = answered(&mut m, "7", "1", &Values::new()).unwrap();
    assert_eq!(
        sent(&out),
        [format!(
            r#"{{"id":7,"result":{{"permissions":{EVERY_SCOPE},"scope":"session"}}}}"#
        )]
    );
}

#[test]
fn a_permission_request_that_cannot_all_be_read_can_only_be_declined() {
    for permissions in [
        // A path of a type no version names yet.
        r#"{"fileSystem":{"entries":[{"access":"read","path":{"type":"volume","name":"VOLUME-X"}}]}}"#,
        // A special path of a kind no version names yet.
        r#"{"fileSystem":{"entries":[{"access":"read","path":{"type":"special","value":{"kind":"everything"}}}]}}"#,
        // An access no version names yet.
        r#"{"fileSystem":{"entries":[{"access":"execute","path":{"type":"path","path":"/work/app"}}]}}"#,
        // A file system part that is not an object.
        r#"{"fileSystem":"/work/app"}"#,
        // A network part that is not an object.
        r#"{"network":true}"#,
        // A field the building does not know: a grant would echo what it never showed.
        r#"{"fileSystem":{"read":["/work/app"],"recursive":true}}"#,
    ] {
        let (mut m, a) = opened(&permission_request(permissions));
        assert_eq!(
            a.detail.lines().last(),
            Some(
                "The building could not read everything this request asks for, so it can only be declined here."
            ),
            "{permissions}"
        );
        assert_eq!(labels(&a), [("Decline", false)], "{permissions}");
        let out = answered(&mut m, "7", "0", &Values::new()).unwrap();
        assert_eq!(
            sent(&out),
            [r#"{"id":7,"result":{"permissions":{},"scope":"turn"}}"#],
            "{permissions}"
        );
    }
}

#[test]
fn a_command_of_an_unknown_kind_is_offered_only_accept_decline_and_cancel() {
    for (kind, extra) in [
        (
            "humanCommand",
            r#","availableDecisions":["accept","acceptForSession",{"acceptWithExecpolicyAmendment":{"execpolicy_amendment":["tool-x"]}},"decline","cancel"]"#,
        ),
        (
            "someLaterKind",
            r#","proposedExecpolicyAmendment":["tool-x"]"#,
        ),
    ] {
        let (_, a) = opened(&format!(
            r#"{{"method":"item/commandExecution/requestApproval","id":8,"params":{{"threadId":"t","turnId":"u","itemId":"i","startedAtMs":1,"kind":"{kind}","command":"tool-x --flag","cwd":"/work/app"{extra}}}}}"#
        ));
        assert_eq!(
            a.title, "Codex asks for your decision on a command",
            "{kind}"
        );
        assert_eq!(
            labels(&a),
            [
                ("Allow once", true),
                ("Decline", false),
                ("Decline and stop the turn", false),
            ],
            "{kind}"
        );
    }
}

#[test]
fn an_elicitation_form_whose_required_is_null_parses() {
    let (_, a) = opened(
        r#"{"method":"mcpServer/elicitation/request","id":9,"params":{"threadId":"t","turnId":null,"serverName":"server-x","mode":"form","_meta":null,"message":"MESSAGE-X","requestedSchema":{"type":"object","properties":{"field-a":{"type":"string"}},"required":null}}}"#,
    );
    assert_eq!(a.fields.len(), 1);
    assert!(!a.fields[0].required);
}
