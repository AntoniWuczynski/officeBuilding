//! Tests only: the manager's files never name a way to stop an agent, answer for A Human or
//! type into a terminal (section 3.5 of the issue #4 spec). It lives outside those files
//! because it has to spell the names.

/// Matched as plain substrings, so not even a comment in a manager file may hold one.
const FORBIDDEN: &[&str] = &[
    "dismiss_reviewer",
    "structured_stop",
    "structured_cancel",
    "structured_answer",
    "answer_question",
    "end_shell",
    "demolish_floor",
    "terminate",
    "stop_agents",
    "terminal_write",
    "terminals.write",
    "structured.prompt",
    "resume_session",
    "libc::kill",
    "chat_send",
    "chat_interrupt",
    "withdraw_message",
    "mail.own",
    "hand_over_desk",
    "carry_out_handover",
    "handover_due",
    "start_due_handovers",
    "handover::",
];

const MANAGER_FILES: &[(&str, &str)] = &[
    ("office/manager.rs", include_str!("manager.rs")),
    ("office/manager_tools.rs", include_str!("manager_tools.rs")),
    ("office/manager_send.rs", include_str!("manager_send.rs")),
    ("office/manager_hire.rs", include_str!("manager_hire.rs")),
    ("office/manager_calls.rs", include_str!("manager_calls.rs")),
];

#[test]
fn the_managers_files_name_no_way_to_stop_answer_or_type() {
    for (file, text) in MANAGER_FILES {
        for name in FORBIDDEN {
            assert!(!text.contains(name), "{file} names {name}");
        }
    }
}

/// No tool of the manager's catalogue can hand a desk over.
#[test]
fn the_managers_tool_catalogue_names_no_handover() {
    let tools = include_str!("../manager/tools.rs");
    assert!(!tools.contains("handover"));
    assert!(!tools.contains("hand_over"));
}

#[test]
fn the_guard_sees_a_forbidden_name_in_a_comment_and_a_method_call() {
    let found = |text: &str| FORBIDDEN.iter().any(|name| text.contains(name));
    assert!(found("// then self.terminals.write(t, x)"));
    assert!(found("office.end_shell(&id)"));
    assert!(!found("self.terminals.backlog(t)"));
}
