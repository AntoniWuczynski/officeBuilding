use super::*;

const DECISIONS: &str = "# FOUNDER DECISIONS\n\nIntro.\n\n## IMP-006 — Which proposals go ahead? — DECIDED 2026-09-06\n\nRecommendation, in order:\n\n1. **Wikilink candidates** — small.\n2. **Evidence hints** — compounds.\n\n## IMP-028 — Where does ingestion run? — DEFAULT APPLIED 2026-09-08 (option 1), veto open\n\nBody.\n\n## Decisions taken 2026-09-04\n\nSummary.\n\n## IMP-030 — Is `andrewikuto` Andrew Basley, the other Andrew, or neither?\n\n**Context.** The last unmerged pair.\n\n**Options.** (1) It is Andrew Basley — approve a merge. (2) It is the other Andrew — merge into `people/andrew`. (3) Different people. (4) Leave it flagged.\n\n**Recommendation.** One word from you closes AUD-015.\n";

const CHECKLIST_DECISIONS: &str = "# FOUNDER_DECISIONS — choices only the founder can make\n\nIntro line.\n\n## Conversation quality (CQ-1) — raised 2026-09-29\n\nGroup intro.\n\n- [ ] **CQ-1 D1. How many rounds does a turn get?** (TODO.md CQ-1) Some context. Options: (a) keep 8 and rely on that,\n  which should halve the continues, (b) 16 rounds for a browser turn and 8 otherwise, (c) (b) plus one automatic\n  continue. Each continue resends the history. Recommendation: **(b)**. Blocks: nothing.\n- [x] **Voice-note backend: local or hosted?**\n  **Ruled 2026-09-25:** **A hosted speech API**, not local.\n  Old option text.\n\n## Blocking work\n\n- [ ] **Keep the snapshots of a failed turn, or drop them\n  at once?** Recommendation: drop them.\n";

#[test]
fn checklist_decisions_are_calls_and_rulings() {
    let (open, decided) = parse_human_decisions(CHECKLIST_DECISIONS, "p", "FOUNDER_DECISIONS.md");
    assert_eq!(
        open.iter().map(|q| q.prompt.as_str()).collect::<Vec<_>>(),
        vec![
            "CQ-1 D1. How many rounds does a turn get?",
            "Keep the snapshots of a failed turn, or drop them at once?"
        ]
    );
    let first = &open[0];
    assert_eq!(first.id, "p|FOUNDER_DECISIONS.md|8");
    assert_eq!(first.answer_via, AnswerVia::File);
    assert_eq!(
        first
            .options
            .iter()
            .map(|o| o.label.as_str())
            .collect::<Vec<_>>(),
        vec![
            "keep 8 and rely on that, which should halve the continues",
            "16 rounds for a browser turn and 8 otherwise",
            "(b) plus one automatic continue"
        ]
    );
    // No date in the item itself: the group heading's date stands in.
    assert_eq!(first.asked_at, "2026-09-29T00:00:00Z");
    assert!(first.context.contains("Recommendation: **(b)**"));
    assert!(open[1].options.is_empty());

    assert_eq!(decided.len(), 1);
    assert_eq!(decided[0].title, "Voice-note backend: local or hosted?");
    assert_eq!(
        decided[0].detail,
        "Ruled 2026-09-25: A hosted speech API, not local. Old option text."
    );
    assert_eq!(decided[0].decided_at, "2026-09-25T00:00:00Z");
    assert_eq!(decided[0].by, Party::Human);
}

#[test]
fn answering_a_checklist_call_ticks_it_and_records_the_ruling() {
    let dir = std::env::temp_dir().join(format!("ob-checklist-decide-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("dir");
    fs::write(dir.join("FOUNDER_DECISIONS.md"), CHECKLIST_DECISIONS).expect("write");

    decide(&dir, "FOUNDER_DECISIONS.md", 17, "Drop them.", "2026-09-30").expect("decide");
    let text = fs::read_to_string(dir.join("FOUNDER_DECISIONS.md")).expect("read");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines[17],
        "- [x] **Keep the snapshots of a failed turn, or drop them"
    );
    assert_eq!(lines[18], "  at once?** Recommendation: drop them.");
    assert_eq!(
        lines[19],
        "  **Ruled 2026-09-30:** Drop them. Answered in Office Building."
    );
    let (open, decided) = parse_human_decisions(&text, "p", "FOUNDER_DECISIONS.md");
    assert_eq!(open.len(), 1);
    assert_eq!(decided.len(), 2);

    let again = decide(&dir, "FOUNDER_DECISIONS.md", 17, "Keep them", "2026-09-30");
    assert_eq!(again, Err("that decision is already settled".to_string()));
    fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn founder_decisions_split_open_and_settled() {
    let (open, decided) = parse_human_decisions(DECISIONS, "p", "FOUNDER_DECISIONS.md");
    assert_eq!(open.len(), 1);
    let q = &open[0];
    assert_eq!(
        q.prompt,
        "IMP-030: Is `andrewikuto` Andrew Basley, the other Andrew, or neither?"
    );
    assert_eq!(q.id, "p|FOUNDER_DECISIONS.md|19");
    assert_eq!(q.answer_via, AnswerVia::File);
    assert_eq!(
        q.options
            .iter()
            .map(|o| o.label.as_str())
            .collect::<Vec<_>>(),
        vec![
            "It is Andrew Basley — approve a merge",
            "It is the other Andrew — merge into `people/andrew`",
            "Different people",
            "Leave it flagged"
        ]
    );
    assert!(
        q.context
            .starts_with("**Context.** The last unmerged pair.")
    );
    assert_eq!(decided.len(), 2);
    assert_eq!(decided[0].title, "IMP-006: Which proposals go ahead?");
    assert_eq!(decided[0].decided_at, "2026-09-06T00:00:00Z");
    assert_eq!(decided[0].by, Party::Human);
    assert_eq!(decided[1].by, Party::Ai);
}

#[test]
fn bold_list_options() {
    assert_eq!(
        parse_options(
            "- **Rotate and keep (chosen)** — bounds it.\n- **Age-out** — no.\n1. **Third**: x"
        ),
        vec!["Rotate and keep (chosen)", "Age-out", "Third"]
    );
    assert!(parse_options("No options here.").is_empty());
}

#[test]
fn checklists_keep_continuations_and_partials() {
    let text = "# TODO\n## Open\n- [ ] **Rotate the tokens.** The Mac's values\n      are stale.\n  - nested child\n- [~] IMP-001 — half done\n- [x] shipped\nnot an item\n";
    let items = parse_checklist(text, "p", "TODO.md", Party::Ai);
    assert_eq!(items.len(), 3);
    assert_eq!(
        items[0].text,
        "Rotate the tokens. The Mac's values are stale."
    );
    assert_eq!(items[0].id, "p|TODO.md|2");
    assert!(!items[1].done);
    assert!(items[2].done);
    assert_eq!(parse_item_id(&items[1].id), Some(("TODO.md", 5)));
    assert_eq!(parse_item_id("nope"), None);
}

#[test]
fn agent_decision_log() {
    let d = parse_agent_decisions(
        "# Decisions log\n\nIntro.\n\n---\n\n## #299 — Orders ships as a PWA\n\n**Decision.** PWA first.\n\nMore.\n",
        "p",
        "DECISIONS.md",
    );
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].title, "#299 — Orders ships as a PWA");
    assert_eq!(d[0].detail, "Decision. PWA first.");
    assert_eq!(d[0].decided_at, "");
}

#[test]
fn reads_a_project_and_writes_back() {
    let root = std::env::temp_dir().join(format!("ob-files-{}", std::process::id()));
    fs::create_dir_all(&root).expect("mkdir");
    fs::write(root.join("FOUNDER_DECISIONS.md"), DECISIONS).expect("write");
    fs::write(root.join("todo.md"), "- [ ] one\n- [x] two\n").expect("write");
    let files = read(&root, "p");
    assert_eq!(files.open.len(), 1);
    assert!(files.human_todos.is_none());
    let todos = files.agent_todos.expect("TODO.md found case-insensitively");
    assert_eq!(todos.len(), 2);

    tick(&root, "todo.md", 0).expect("tick");
    assert_eq!(
        fs::read_to_string(root.join("todo.md")).expect("read"),
        "- [x] one\n- [x] two\n"
    );
    assert!(tick(&root, "todo.md", 0).is_err());

    decide(
        &root,
        "FOUNDER_DECISIONS.md",
        19,
        "It is **Andrew Basley**",
        "2026-09-29",
    )
    .expect("decide");
    let after = read(&root, "p");
    assert!(after.open.is_empty());
    let settled = after.decided.expect("decided");
    let last = settled
        .iter()
        .find(|d| d.title.starts_with("IMP-030"))
        .expect("IMP-030 settled");
    assert_eq!(last.detail, "DECIDED 2026-09-29: It is Andrew Basley");
    assert!(
        fs::read_to_string(root.join("FOUNDER_DECISIONS.md"))
            .expect("read")
            .contains("Decided in Office Building on 2026-09-29: It is Andrew Basley.")
    );
    assert!(decide(&root, "FOUNDER_DECISIONS.md", 19, "again", "2026-09-29").is_err());
    fs::remove_dir_all(&root).expect("cleanup");
}

#[test]
fn heading_parts_and_dates() {
    assert_eq!(
        split_heading("Plain title?"),
        (None, "Plain title?".into(), None)
    );
    assert_eq!(
        split_heading("IMP-7 — A — B — VETOED 2026-09-04"),
        (
            Some("IMP-7".into()),
            "A — B".into(),
            Some("VETOED 2026-09-04".into())
        )
    );
    assert_eq!(find_date("x 2026-09-04: y"), "2026-09-04T00:00:00Z");
    assert_eq!(find_date("none"), "");
    assert_eq!(truncate("abcdef", 4), "abc…");
}

const ENTRY_DECISIONS: &str = "# FOUNDER-DECISIONS\n\nIntro.\n\n## Open — commercial and strategic\n\n### F-10 · Pull the raise on the current deck? · **urgency: high, decide this month**\n\n**Decision:** withdraw the deck, or raise on it.\n\n**Options**\n\n| | Option | Trade-off |\n|---|---|---|\n| A | **Pull it, rebuild on measured numbers** | Costs four weeks. |\n| B | Raise now, correct in diligence | Keeps momentum. |\n\n**Recommendation: A.** Measured 2026-08-06.\n\n### F-18 · Both founders are named on a public page · **urgency: high**\n\nBody.\n\n## Answered — kept for the record\n\n### F-05 · Which phone for testing? · **urgency: low**\n\nAnswered: the Pixel 6a, 2026-08-07.\n";

#[test]
fn entry_headings_under_groups_are_calls() {
    let (open, decided) = parse_human_decisions(ENTRY_DECISIONS, "p", "FOUNDER-DECISIONS.md");
    assert_eq!(
        open.iter().map(|q| q.prompt.as_str()).collect::<Vec<_>>(),
        vec![
            "F-10: Pull the raise on the current deck?",
            "F-18: Both founders are named on a public page"
        ]
    );
    assert_eq!(open[0].id, "p|FOUNDER-DECISIONS.md|6");
    assert_eq!(
        open[0]
            .options
            .iter()
            .map(|o| o.label.as_str())
            .collect::<Vec<_>>(),
        vec![
            "Pull it, rebuild on measured numbers",
            "Raise now, correct in diligence"
        ]
    );
    assert_eq!(open[0].asked_at, "2026-08-06T00:00:00Z");
    assert!(open[0].context.contains("**Recommendation: A.**"));
    assert_eq!(decided.len(), 1);
    assert_eq!(decided[0].title, "F-05: Which phone for testing?");
    assert_eq!(decided[0].decided_at, "2026-08-07T00:00:00Z");
}

#[test]
fn answering_an_entry_marks_its_heading_decided() {
    let dir = std::env::temp_dir().join(format!("ob-entry-decide-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("dir");
    fs::write(dir.join("FOUNDER-DECISIONS.md"), ENTRY_DECISIONS).expect("write");
    fs::write(dir.join("FOUNDER-TODO.md"), "- [ ] **F-01** register\n").expect("write");

    let files = read(&dir, "p");
    assert_eq!(files.open.len(), 2, "the hyphenated file name is found");
    assert_eq!(
        files.human_todos.map(|t| t.len()),
        Some(1),
        "and so is the hyphenated TODO"
    );

    decide(&dir, "FOUNDER-DECISIONS.md", 6, "A", "2026-09-30").expect("decide");
    let text = fs::read_to_string(dir.join("FOUNDER-DECISIONS.md")).expect("read");
    assert!(
        text.lines()
            .nth(6)
            .is_some_and(|l| l.starts_with("### F-10 · Pull the raise")
                && l.ends_with("— DECIDED 2026-09-30: A"))
    );
    let (open, decided) = parse_human_decisions(&text, "p", "FOUNDER-DECISIONS.md");
    assert_eq!(open.len(), 1);
    assert_eq!(decided.len(), 2);
    assert_eq!(
        decide(&dir, "FOUNDER-DECISIONS.md", 6, "B", "2026-09-30"),
        Err("that decision is already settled".to_string())
    );
    fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn inline_options_follow_the_word_option() {
    // Lettered parts of the question itself are not options.
    assert!(
        parse_options("**Decision:** keep or drop (a) the stance and (b) the anonymity.")
            .is_empty()
    );
    assert_eq!(
        parse_options(
            "Context.\n**Options:** (A) drop the tier; (B) reprice upward;\n(C) keep it. More prose."
        ),
        vec!["drop the tier", "reprice upward", "keep it"]
    );
}
