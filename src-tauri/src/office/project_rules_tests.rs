use super::tests::office;

#[test]
fn a_review_list_set_after_the_parse_flags_the_session_without_a_new_parse() {
    let (office, dir) = office("review");
    let home = dir.join("home");
    let me = std::process::id();
    std::fs::create_dir_all(home.join(".claude/sessions")).expect("sessions");
    std::fs::write(
        home.join(format!(".claude/sessions/{me}.json")),
        format!(
            r#"{{"pid":{me},"sessionId":"s1","cwd":"/code/app","status":"idle","name":"beta-web"}}"#
        ),
    )
    .expect("registry");
    std::fs::create_dir_all(home.join(".claude/projects/-code-app")).expect("project");
    std::fs::write(
        home.join(".claude/projects/-code-app/s1.jsonl"),
        concat!(
            r#"{"type":"assistant","cwd":"/code/app","message":{"id":"m","content":[{"type":"tool_use","id":"t","name":"Skill","input":{"skill":"review"}}]}}"#,
            "\n"
        ),
    )
    .expect("transcript");
    office.refresh();
    let parsed = office.lock().discovery.parses();
    let before = office.snapshot();
    assert_eq!(before.sessions[0].session_name.as_deref(), Some("beta-web"));
    assert!(!before.sessions[0].self_reviewed);
    std::fs::write(
        dir.join("projects.json"),
        r#"{"rules":[],"reviewSkills":["review"]}"#,
    )
    .expect("rules");
    assert!(office.snapshot().sessions[0].self_reviewed);
    office.refresh();
    assert_eq!(office.lock().discovery.parses(), parsed);
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn a_changed_rules_file_makes_a_refresh_report_a_change() {
    let (office, dir) = office("rules-refresh");
    office.refresh();
    assert!(!office.refresh());
    std::fs::write(dir.join("projects.json"), r#"{"rules":[{"name":"Alpha"}]}"#).expect("rules");
    assert!(office.refresh());
    std::fs::remove_dir_all(&dir).expect("cleanup");
}
