pub(super) use super::fixtures::{ABORTED, CLEAN, ERROR_EVENT, GOAL, ID, LIMIT, META};
use super::*;
use std::fs::{self, File};
use std::io::Write;
use std::time::{Duration, SystemTime};

// The payload codex-cli 0.161.0 wrote when a local listener replayed a used-up allowance
// to it through a custom provider on 2026-11-13 (probe rounds six and seven). A replay,
// not a reply from the ChatGPT backend. The line's `timestamp` is composed.
const TOKENS_EMPTY: &str = r#"{"timestamp":"2026-11-13T12:43:16.000Z","type":"event_msg","payload":{"type":"token_count","info":null,"rate_limits":{"limit_id":null,"limit_name":null,"primary":null,"secondary":null,"credits":null,"individual_limit":null,"spend_control_reached":null,"plan_type":null,"rate_limit_reached_type":null}}}"#;

// Composed from the key names of rollouts on one machine and from the Codex 0.161.0
// source, not from a replay.
const STARTED: &str = r#"{"timestamp":"2026-11-13T12:43:16.000Z","type":"event_msg","payload":{"type":"task_started","turn_id":"turn-1","root_turn_id":"turn-1","started_at":1794573796,"model_context_window":258400,"collaboration_mode_kind":"default"}}"#;

/// When `LIMIT` was written.
const AT: i64 = 1_794_573_796;
const HOUR: i64 = 3600;
const DAY: i64 = 24 * HOUR;

/// The proof as `LIMIT` alone gives it.
const USED_UP: Reading = Reading::UsedUp {
    window: None,
    resets_at: None,
    at: Some(AT),
};

/// `line` with `from` replaced by `to` (it must be there once).
fn with(line: &str, from: &str, to: &str) -> String {
    assert_eq!(line.matches(from).count(), 1, "{from}");
    line.replacen(from, to, 1)
}

fn limit_with(from: &str, to: &str) -> String {
    with(LIMIT, from, to)
}

/// `META` on a custom provider.
fn meta_custom() -> String {
    with(
        META,
        r#""model_provider":"openai""#,
        r#""model_provider":"obprobe""#,
    )
}

/// `META` of a sub-agent's thread.
fn meta_subagent() -> String {
    with(
        META,
        r#""source":"cli""#,
        r#""source":{"subagent":{"other":"guardian"}},"parent_thread_id":"0199c0de-0000-4000-8000-000000000002""#,
    )
}

/// A `token_count` line carrying `rate_limits`.
fn tokens(rate_limits: &str) -> String {
    with(
        TOKENS_EMPTY,
        r#""rate_limits":{"limit_id":null,"limit_name":null,"primary":null,"secondary":null,"credits":null,"individual_limit":null,"spend_control_reached":null,"plan_type":null,"rate_limit_reached_type":null}"#,
        &format!(r#""rate_limits":{rate_limits}"#),
    )
}

/// A snapshot with a limit name (JSON) and two windows (JSON).
fn limits(name: &str, primary: &str, secondary: &str) -> String {
    format!(
        r#"{{"limit_id":"codex","limit_name":{name},"primary":{primary},"secondary":{secondary},"credits":null,"individual_limit":null,"spend_control_reached":null,"plan_type":null,"rate_limit_reached_type":null}}"#
    )
}

fn window(used: &str, minutes: i64, reset: &str) -> String {
    format!(r#"{{"used_percent":{used},"window_minutes":{minutes},"resets_at":{reset}}}"#)
}

fn reduce(lines: &[&str]) -> Reducer {
    let mut reducer = Reducer::default();
    for line in lines {
        reducer.line(line);
    }
    reducer
}

fn reading(lines: &[&str]) -> Reading {
    reduce(lines).reading
}

/// A reader holding what `lines` leave, with no file behind it.
fn reader_of(lines: &[&str]) -> RolloutReader {
    RolloutReader {
        reducer: reduce(lines),
        ..RolloutReader::new(
            Path::new("/nonexistent"),
            "0199c0de-0000-4000-8000-000000000001",
        )
    }
}

#[test]
fn the_proof_table() {
    let info = |word: &str| {
        limit_with(
            r#""codex_error_info":"usage_limit_exceeded""#,
            &format!(r#""codex_error_info":{word}"#),
        )
    };
    let throttle = info(r#""rate_limit_exceeded""#);
    let overloaded = info(r#""server_overloaded""#);
    let unauthorized = info(r#""unauthorized""#);
    let object = info(r#"{"http_connection_failed":{"http_status_code":502}}"#);
    let no_info = limit_with(r#","codex_error_info":"usage_limit_exceeded""#, "");
    let error_word = |error: &str| {
        let start = LIMIT.find(r#""error":{"#).expect("error");
        let end = LIMIT.find(r#","started_at""#).expect("started_at");
        format!(r#"{}"error":{error}{}"#, &LIMIT[..start], &LIMIT[end..])
    };
    let error_string = error_word(r#""usage_limit_exceeded""#);
    let error_list = error_word(r#"["You've hit your usage limit.","usage_limit_exceeded"]"#);
    let error_null = error_word("null");
    let limit_spelled = limit_with("task_complete", "turn_complete");
    let rows: Vec<(&str, Vec<&str>, Reading)> = vec![
        ("nothing", vec![], Reading::Clear),
        (
            "the replayed turn",
            vec![META, STARTED, TOKENS_EMPTY, LIMIT],
            USED_UP,
        ),
        ("the limit alone", vec![LIMIT], USED_UP),
        ("the limit as turn_complete", vec![&limit_spelled], USED_UP),
        (
            "rate_limit_exceeded",
            vec![&throttle],
            Reading::Other(Why::OtherError),
        ),
        (
            "server_overloaded",
            vec![&overloaded],
            Reading::Other(Why::OtherError),
        ),
        (
            "unauthorized",
            vec![&unauthorized],
            Reading::Other(Why::OtherError),
        ),
        (
            "an error info that is an object",
            vec![&object],
            Reading::Other(Why::OtherError),
        ),
        (
            "no error info",
            vec![&no_info],
            Reading::Other(Why::OtherError),
        ),
        (
            "an error that is a string",
            vec![&error_string],
            Reading::Other(Why::OtherError),
        ),
        (
            "an error that is a list of the right words",
            vec![&error_list],
            Reading::Other(Why::OtherError),
        ),
        ("an error that is null", vec![&error_null], Reading::Clear),
    ];
    for (name, lines, want) in rows {
        assert_eq!(reading(&lines), want, "{name}");
    }
}

#[test]
fn what_takes_the_proof_away_and_what_leaves_it() {
    let clean_spelled = with(CLEAN, "task_complete", "turn_complete");
    let rows: Vec<(&str, Vec<&str>, Reading)> = vec![
        (
            "an interrupted turn",
            vec![ABORTED],
            Reading::Other(Why::Aborted),
        ),
        (
            "an interrupted turn after the limit",
            vec![LIMIT, ABORTED],
            Reading::Other(Why::Aborted),
        ),
        ("an error event alone", vec![ERROR_EVENT], Reading::Clear),
        (
            "an error event after the limit",
            vec![LIMIT, ERROR_EVENT],
            USED_UP,
        ),
        (
            "a clean turn after the limit",
            vec![LIMIT, CLEAN],
            Reading::Clear,
        ),
        (
            "a clean turn_complete after the limit",
            vec![LIMIT, &clean_spelled],
            Reading::Clear,
        ),
        ("a turn started alone", vec![STARTED], Reading::Clear),
        (
            "a turn started after the limit",
            vec![LIMIT, STARTED],
            USED_UP,
        ),
        (
            "a token count after the limit",
            vec![LIMIT, TOKENS_EMPTY],
            USED_UP,
        ),
        ("the limit after a clean turn", vec![CLEAN, LIMIT], USED_UP),
        ("a blank line after the limit", vec![LIMIT, "  "], USED_UP),
    ];
    for (name, lines, want) in rows {
        assert_eq!(reading(&lines), want, "{name}");
    }
}

#[test]
fn an_unreadable_line_proves_nothing() {
    let payload_string = with(
        CLEAN,
        &CLEAN[CLEAN.find(r#""payload":"#).expect("payload")..],
        r#""payload":"task_complete"}"#,
    );
    let payload_list = with(
        LIMIT,
        &LIMIT[LIMIT.find(r#""payload":"#).expect("payload")..],
        r#""payload":["task_complete",{"codex_error_info":"usage_limit_exceeded"}]}"#,
    );
    let unreadable: [(&str, &str); 5] = [
        ("not json", "not json"),
        ("an array", "[1]"),
        ("a payload that is a string", &payload_string),
        ("a payload that is a list", &payload_list),
        ("a cut line", &LIMIT[..LIMIT.len() - 2]),
    ];
    for (name, line) in unreadable {
        let want = Reading::Other(Why::Unreadable);
        assert_eq!(reading(&[line]), want, "{name} after nothing");
        assert_eq!(reading(&[LIMIT, line]), want, "{name} after the limit");
    }
}

#[test]
fn who_counts() {
    let custom = meta_custom();
    let subagent = meta_subagent();
    let source_object = with(META, r#""source":"cli""#, r#""source":{"exec":true}"#);
    let parent_only = with(
        META,
        r#""source":"cli""#,
        r#""source":"cli","parent_thread_id":"0199c0de-0000-4000-8000-000000000002""#,
    );
    let forked = with(
        META,
        r#""source":"cli""#,
        r#""source":"cli","forked_from_id":"0199c0de-0000-4000-8000-000000000002""#,
    );
    let no_source = with(META, r#""source":"cli","#, "");
    let rows: Vec<(&str, Vec<&str>, bool)> = vec![
        ("the built-in provider", vec![META, LIMIT], true),
        ("no source", vec![&no_source, LIMIT], true),
        ("a custom provider", vec![&custom, LIMIT], false),
        (
            "a fork copying its parent's refusal",
            vec![&forked, LIMIT],
            false,
        ),
        ("a sub-agent's thread", vec![&subagent, LIMIT], false),
        ("no session_meta", vec![STARTED, TOKENS_EMPTY, LIMIT], false),
        (
            "a source that is an object",
            vec![&source_object, LIMIT],
            false,
        ),
        ("a parent thread named", vec![&parent_only, LIMIT], false),
        (
            "a second session_meta naming another provider",
            vec![META, &custom, LIMIT],
            true,
        ),
        (
            "a sub-agent carrying a copy of its parent's session_meta",
            vec![&subagent, META, LIMIT],
            false,
        ),
        (
            "a session_meta after another line",
            vec![STARTED, META, LIMIT],
            false,
        ),
        (
            "an unreadable first line",
            vec!["not json", META, LIMIT],
            false,
        ),
        ("nothing read", vec![], false),
    ];
    for (name, lines, counts) in rows {
        let reader = reader_of(&lines);
        assert_eq!(reader.counts(), counts, "{name}");
        let want = if counts {
            Allowance::UsedUp {
                window: None,
                reset_at: None,
                at: Some(AT),
            }
        } else {
            Allowance::Unproven
        };
        assert_eq!(reader.allowance(), want, "{name}");
    }
    assert_eq!(reader_of(&[META]).allowance(), Allowance::Clear);
    assert_eq!(reader_of(&[META, ABORTED]).allowance(), Allowance::Unproven);
}

#[test]
fn the_account_identifiers_go_nowhere() {
    let reducer = reduce(&[META, GOAL, LIMIT]);
    let held = format!("{:?} {:?}", reducer.reading, reducer.goal);
    assert!(!held.contains("invented"), "{held}");
}

#[test]
fn a_named_limit_is_not_the_proof() {
    let full = window("100.0", 300, &(AT + HOUR).to_string());
    let named = |name: &str| tokens(&limits(name, &full, "null"));
    let five_hour = Reading::UsedUp {
        window: Some(Window::FiveHour),
        resets_at: Some(AT + HOUR),
        at: Some(AT),
    };
    let rows: [(&str, &str, Reading); 6] = [
        (
            "a model's name",
            r#""GPT-6 Astra""#,
            Reading::Other(Why::ModelLimit),
        ),
        ("codex", r#""codex""#, five_hour.clone()),
        ("Codex", r#""Codex""#, five_hour.clone()),
        ("gpt-reserve", r#""gpt-reserve""#, five_hour.clone()),
        ("blank", r#""  ""#, five_hour.clone()),
        ("null", "null", five_hour),
    ];
    for (name, word, want) in rows {
        let line = named(word);
        assert_eq!(reading(&[META, STARTED, &line, LIMIT]), want, "{name}");
    }
}

#[test]
fn the_reset_table() {
    let snapshot = |primary: &str, secondary: &str| tokens(&limits("null", primary, secondary));
    let reset = |r: i64| r.to_string();
    let used_up = |window: Option<Window>, resets_at: Option<i64>| Reading::UsedUp {
        window,
        resets_at,
        at: Some(AT),
    };
    let none = used_up(None, None);
    let rows: Vec<(&str, String, Reading)> = vec![
        (
            "primary full",
            snapshot(
                &window("100.0", 300, &reset(AT + HOUR)),
                &window("41.5", 10080, &reset(AT + 5 * DAY)),
            ),
            used_up(Some(Window::FiveHour), Some(AT + HOUR)),
        ),
        (
            "only secondary full",
            snapshot(
                &window("50", 300, &reset(AT + HOUR)),
                &window("100", 10080, &reset(AT + 5 * DAY)),
            ),
            used_up(Some(Window::SevenDay), Some(AT + 5 * DAY)),
        ),
        (
            "both full",
            snapshot(
                &window("100.0", 300, &reset(AT + HOUR)),
                &window("100.0", 10080, &reset(AT + 5 * DAY)),
            ),
            used_up(Some(Window::SevenDay), Some(AT + 5 * DAY)),
        ),
        (
            "both full, the shorter window later",
            snapshot(
                &window("100.0", 300, &reset(AT + 6 * DAY)),
                &window("100.0", 10080, &reset(AT + 5 * DAY)),
            ),
            used_up(Some(Window::FiveHour), Some(AT + 6 * DAY)),
        ),
        (
            "not quite full",
            snapshot(&window("99.9", 300, &reset(AT + HOUR)), "null"),
            none.clone(),
        ),
        (
            "a reset before the record",
            snapshot(&window("100.0", 300, &reset(AT - 1)), "null"),
            none.clone(),
        ),
        (
            "a reset at the record's own time",
            snapshot(&window("100.0", 300, &reset(AT)), "null"),
            none.clone(),
        ),
        (
            "a reset more than eight days on",
            snapshot(
                &window("100.0", 10080, &reset(AT + RESET_HORIZON + 1)),
                "null",
            ),
            none.clone(),
        ),
        (
            "a reset eight days on",
            snapshot(&window("100.0", 10080, &reset(AT + RESET_HORIZON)), "null"),
            used_up(Some(Window::SevenDay), Some(AT + RESET_HORIZON)),
        ),
        (
            "a reset that is not a number",
            snapshot(&window("100.0", 300, r#""soon""#), "null"),
            none.clone(),
        ),
        (
            "a window that is a list",
            snapshot(&format!("[100.0,300,{}]", AT + HOUR), "null"),
            none.clone(),
        ),
        (
            "a window of an hour",
            snapshot(&window("100.0", 60, &reset(AT + HOUR)), "null"),
            used_up(None, Some(AT + HOUR)),
        ),
    ];
    for (name, line, want) in rows {
        assert_eq!(reading(&[META, STARTED, &line, LIMIT]), want, "{name}");
    }
}

#[test]
fn the_reset_comes_from_this_turn_at_the_records_own_time() {
    let full = tokens(&limits(
        "null",
        &window("100.0", 300, &(AT + HOUR).to_string()),
        "null",
    ));
    assert_eq!(
        reading(&[META, &full, STARTED, LIMIT]),
        USED_UP,
        "a snapshot from the turn before"
    );
    let untimed = limit_with(r#","completed_at":1794573796"#, "");
    assert_eq!(
        reading(&[META, STARTED, &full, &untimed]),
        Reading::UsedUp {
            window: Some(Window::FiveHour),
            resets_at: Some(AT + HOUR),
            at: Some(AT),
        },
        "the time from the line's timestamp"
    );
    let unstamped = with(&untimed, r#""timestamp":"2026-11-13T12:43:16.000Z","#, "");
    assert_eq!(
        reading(&[META, STARTED, &full, &unstamped]),
        Reading::UsedUp {
            window: None,
            resets_at: None,
            at: None,
        },
        "no time at all"
    );
}

#[test]
fn the_goal_table() {
    let status = |word: &str| with(GOAL, r#""status":"active""#, &format!(r#""status":{word}"#));
    let complete = status(r#""complete""#);
    let paused = status(r#""paused""#);
    let usage_limited = status(r#""usageLimited""#);
    let budget_limited = status(r#""budgetLimited""#);
    let blocked = status(r#""blocked""#);
    let made_up = status(r#""dreaming""#);
    let number = status("3");
    let no_goal = with(GOAL, &GOAL[GOAL.find(r#","goal":"#).expect("goal")..], "}}");
    let objective_number = with(
        GOAL,
        r#""objective":"Reply ok until told to stop""#,
        r#""objective":7"#,
    );
    let standing = |objective: &str| Goal::Standing {
        objective: objective.to_string(),
    };
    let reply = standing("Reply ok until told to stop");
    let rows: Vec<(&str, Vec<&str>, Goal)> = vec![
        ("no goal line", vec![META, STARTED, LIMIT], Goal::None),
        ("a goal set", vec![META, GOAL], reply.clone()),
        (
            "a goal set before the limit",
            vec![META, GOAL, STARTED, TOKENS_EMPTY, LIMIT],
            reply.clone(),
        ),
        ("complete", vec![GOAL, &complete], Goal::None),
        ("paused", vec![GOAL, &paused], Goal::None),
        ("usageLimited", vec![&usage_limited], reply.clone()),
        ("budgetLimited", vec![&budget_limited], reply.clone()),
        ("blocked", vec![&blocked], reply.clone()),
        ("a made-up word", vec![&made_up], reply.clone()),
        (
            "an objective that is not a string",
            vec![&objective_number],
            standing(""),
        ),
        ("no goal in the line", vec![GOAL, &no_goal], Goal::Unknown),
        ("a status that is a number", vec![&number], Goal::Unknown),
        ("an unreadable line", vec![GOAL, "not json"], Goal::Unknown),
        (
            "an unreadable line, then a goal line",
            vec!["not json", &complete],
            Goal::None,
        ),
        (
            "a status that is a number, then a goal line",
            vec![&number, GOAL],
            reply,
        ),
    ];
    for (name, lines, want) in rows {
        assert_eq!(reduce(&lines).goal, want, "{name}");
    }
}

/// A temp home for one test, empty.
pub(super) fn home(name: &str) -> PathBuf {
    let home = std::env::temp_dir().join(format!("ob-rollout-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&home);
    fs::create_dir_all(&home).expect("home");
    home
}

pub(super) fn rollout_name(time: &str) -> String {
    format!("rollout-{time}-{ID}.jsonl")
}

pub(super) fn day(home: &Path, date: &str) -> PathBuf {
    let dir = home.join(".codex/sessions").join(date);
    fs::create_dir_all(&dir).expect("dirs");
    dir
}

fn append(path: &Path, text: &str) {
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .expect("open");
    file.write_all(text.as_bytes()).expect("write");
}

#[test]
fn finding_the_rollout() {
    let home = home("find");
    let dir = day(&home, "2026/11/13");
    assert_eq!(find(&home, ID), None, "no file");
    let path = dir.join(rollout_name("2026-11-13T12-43-15"));
    append(&path, META);
    assert_eq!(find(&home, ID), Some(path.clone()));
    for id in [
        "codex-new-123",
        "s1",
        "",
        "../0199c0de-0000-4000-8000-000000000001",
    ] {
        assert_eq!(find(&home, id), None, "{id}");
    }

    let older = day(&home, "2026/11/12").join(rollout_name("2026-11-12T09-00-00"));
    append(&older, META);
    let now = SystemTime::now();
    File::options()
        .write(true)
        .open(&older)
        .expect("open")
        .set_modified(now - Duration::from_secs(3600))
        .expect("touch");
    File::options()
        .write(true)
        .open(&path)
        .expect("open")
        .set_modified(now)
        .expect("touch");
    assert_eq!(find(&home, ID), Some(path.clone()), "the newer of two");
    File::options()
        .write(true)
        .open(&older)
        .expect("open")
        .set_modified(now + Duration::from_secs(3600))
        .expect("touch");
    assert_eq!(
        find(&home, ID),
        Some(older),
        "the newer of two, the other way"
    );
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn finding_refuses_links_and_other_names() {
    let home = home("refuse");
    let dir = day(&home, "2026/11/13");
    let outside = home.join("elsewhere.jsonl");
    append(&outside, META);
    std::os::unix::fs::symlink(&outside, dir.join(rollout_name("2026-11-13T12-43-15")))
        .expect("link");
    assert_eq!(find(&home, ID), None, "a symbolic link");
    append(
        &dir.join(format!("other-2026-11-13T12-43-15-{ID}.jsonl")),
        META,
    );
    assert_eq!(find(&home, ID), None, "a name not starting rollout-");
    append(
        &dir.join(format!("rollout-2026-11-13T12-43-15-{ID}.json")),
        META,
    );
    assert_eq!(find(&home, ID), None, "not a .jsonl file");
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn the_reader_follows_a_rollout() {
    let home = home("follow");
    let mut reader = RolloutReader::new(&home, ID);
    reader.read();
    assert_eq!(reader.allowance(), Allowance::Unproven, "no file");
    assert!(!reader.counts());

    let path = day(&home, "2026/11/13").join(rollout_name("2026-11-13T12-43-15"));
    append(
        &path,
        &format!("{META}\n{GOAL}\n{STARTED}\n{TOKENS_EMPTY}\n"),
    );
    reader.read();
    assert!(reader.counts());
    assert_eq!(reader.allowance(), Allowance::Clear);

    append(&path, LIMIT);
    reader.read();
    assert_eq!(
        reader.allowance(),
        Allowance::Clear,
        "a last line with no line break waits"
    );
    append(&path, "\n");
    reader.read();
    let used_up = Allowance::UsedUp {
        window: None,
        reset_at: None,
        at: Some(AT),
    };
    assert_eq!(reader.allowance(), used_up);
    assert_eq!(
        reader.goal(),
        &Goal::Standing {
            objective: "Reply ok until told to stop".into()
        }
    );

    append(&path, &format!("{CLEAN}\n"));
    reader.read();
    assert_eq!(
        reader.allowance(),
        Allowance::Clear,
        "a clean turn appended"
    );

    fs::write(&path, format!("{META}\n{LIMIT}\n")).expect("write");
    reader.read();
    assert_eq!(
        reader.allowance(),
        used_up,
        "a file replaced by a shorter one"
    );
    assert_eq!(
        reader.goal(),
        &Goal::None,
        "read anew, the goal is forgotten"
    );

    fs::remove_file(&path).expect("remove");
    reader.read();
    append(&path, &format!("{}\n{CLEAN}\n", meta_custom()));
    reader.read();
    assert!(!reader.counts(), "a new file is read anew");
    assert_eq!(reader.allowance(), Allowance::Unproven);
    let _ = fs::remove_dir_all(&home);
}
