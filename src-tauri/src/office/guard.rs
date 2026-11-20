//! Tests only: the manager's files never name a way to stop an agent, answer for A Human or
//! type into a terminal (section 3.5 of the issue #4 spec). It lives outside those files
//! because it has to spell the names.

/// Matched as plain substrings, so not even a comment in a manager file may hold one.
const FORBIDDEN: &[&str] = &[
    "dismiss_reviewer",
    "structured_stop",
    "structured_cancel",
    "structured_answer",
    "structured.stop",
    "structured.cancel",
    "structured.answer",
    "answer_question",
    "end_shell",
    "demolish_floor",
    "terminate",
    "stop_agents",
    "exile_session",
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
    "hang_up",
    "end_desk_through",
    "DeskEnder",
    "force_own_child",
    "SIGKILL",
    "take_over",
    "TakeOver",
];

/// The one way out of the manager's files that ends a desk.
const ENDING: &str = "end_floor_desk";

/// What the file that ends a desk may not name either: it ends one process and nothing else.
const NOT_IN_THE_ENDING: &[&str] = &[
    "exile_session",
    "demolish_floor",
    "end_shell",
    "hang_up",
    "dismiss_reviewer",
    "structured_stop",
    "structured.stop",
    "structured.cancel",
    "structured.answer",
    "structured.prompt",
    "killpg",
    "stop_agents",
    "hang_up_shells",
    "as kill",
    "Command::new",
    "terminal_write",
    "terminals.write",
    "answer_question",
    "resume_session",
    "chat_send",
    "chat_interrupt",
    "hand_over_desk",
    "carry_out_handover",
    "take_over",
    "terminals.release",
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
    assert!(!tools.contains("take_over"));
    assert!(!tools.contains("takeover"));
}

/// The manager's files reach the ending of a desk through one call, in the tool dispatch,
/// and through nothing else.
#[test]
fn the_managers_files_end_a_desk_through_one_call_only() {
    let uses: Vec<(&str, usize)> = MANAGER_FILES
        .iter()
        .map(|(file, text)| (*file, text.matches(ENDING).count()))
        .collect();
    assert_eq!(
        uses,
        [
            ("office/manager.rs", 0),
            ("office/manager_tools.rs", 1),
            ("office/manager_send.rs", 0),
            ("office/manager_hire.rs", 0),
            ("office/manager_calls.rs", 0),
        ]
    );
}

/// The file that ends a desk signals one process, once, and names no wider way to stop one.
#[test]
fn the_ending_file_kills_once_and_names_nothing_wider() {
    let ending = include_str!("exile/end_desk.rs");
    for name in NOT_IN_THE_ENDING {
        assert!(!ending.contains(name), "exile/end_desk.rs names {name}");
    }
    assert_eq!(ending.matches("libc::kill(").count(), 1);
}

#[test]
fn the_guard_sees_a_forbidden_name_in_a_comment_and_a_method_call() {
    let found = |text: &str| FORBIDDEN.iter().any(|name| text.contains(name));
    assert!(found("// then self.terminals.write(t, x)"));
    assert!(found("office.end_shell(&id)"));
    assert!(found("office.exile_session(&id)"));
    assert!(found("office.take_over_desk(&id, &start)"));
    assert!(found("self.hang_up(&terminal)"));
    assert!(!found("self.terminals.backlog(t)"));
}
