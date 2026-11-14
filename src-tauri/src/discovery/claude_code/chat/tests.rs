use super::map::{KEPT, Mapper};
use super::*;
use crate::structured::{FileDiff, OUTPUT_MAX, ToolStatus};
use std::io::{Cursor, Write};

// The shapes below come from the documented Agent SDK message types and Messages
// API blocks, and from the fields the existing transcript reader already reads
// (sections 2.4 and 2.5 of the issue #6 spec). They are written for Claude Code
// 2.1.322. The transcript format is not a contract and may change on any release.

const PROMPT: &str = r#"{"type":"user","message":{"role":"user","content":"fix the parser"}}"#;
const THINK: &str = r#"{"type":"assistant","message":{"id":"m1","content":[{"type":"thinking","thinking":"The parser drops the last token."}]}}"#;
const SAY: &str = r#"{"type":"assistant","message":{"id":"m1","content":[{"type":"text","text":"Running the tests first."}]}}"#;
const BASH: &str = r#"{"type":"assistant","message":{"id":"m2","content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"pnpm test\n  --run","description":"Run the tests"}}]}}"#;
const BASH_DONE: &str = r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"2 tests failed"}]}}"#;
const EDIT: &str = r#"{"type":"assistant","message":{"id":"m3","content":[{"type":"tool_use","id":"t2","name":"Edit","input":{"file_path":"/code/app/src/parser.ts","old_string":"a\nold\nz","new_string":"a\nnew\nz"}}]}}"#;
const EDIT_DONE: &str = r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t2","content":"The file was updated."}]}}"#;
const DONE: &str = r#"{"type":"assistant","message":{"id":"m4","content":[{"type":"text","text":"Fixed: the last token is kept."}]}}"#;

const BASE: [&str; 8] = [PROMPT, THINK, SAY, BASH, BASH_DONE, EDIT, EDIT_DONE, DONE];

fn text(lines: &[&str]) -> String {
    lines.iter().flat_map(|l| [*l, "\n"]).collect()
}

/// What the mapper keeps after the lines, as one read of a whole file would.
fn map(lines: &[&str]) -> Vec<Entry> {
    let mut mapper = Mapper::default();
    feed_lines(Cursor::new(text(lines)), 0, false, |at, l| {
        mapper.line(at, l);
    });
    mapper.kept()
}

fn tool_use(id: &str, name: &str, input: &str) -> String {
    format!(
        r#"{{"type":"assistant","message":{{"content":[{{"type":"tool_use","id":"{id}","name":"{name}","input":{input}}}]}}}}"#
    )
}

fn tool_result(id: &str, content: &str, is_error: bool) -> String {
    format!(
        r#"{{"type":"user","message":{{"content":[{{"type":"tool_result","tool_use_id":"{id}","content":{content},"is_error":{is_error}}}]}}}}"#
    )
}

/// The kind of each entry, with its text or title.
fn shapes(entries: &[Entry]) -> Vec<(&'static str, String)> {
    entries
        .iter()
        .map(|e| match e {
            Entry::User { text, .. } => ("user", text.clone()),
            Entry::Agent { text, .. } => ("agent", text.clone()),
            Entry::Thought { text, .. } => ("thought", text.clone()),
            Entry::Notice { text, .. } => ("notice", text.clone()),
            Entry::Tool { title, .. } => ("tool", title.clone()),
        })
        .collect()
}

fn the_tool(entries: &[Entry], id: &str) -> (String, ToolStatus, String, Vec<FileDiff>) {
    match entries.iter().find(|e| e.id() == id) {
        Some(Entry::Tool {
            title,
            status,
            detail,
            diffs,
            ..
        }) => (title.clone(), *status, detail.clone(), diffs.clone()),
        other => panic!("no tool {id}: {other:?}"),
    }
}

#[test]
fn the_base_conversation_maps_in_order() {
    let entries = map(&BASE);
    assert_eq!(
        shapes(&entries),
        vec![
            ("user", "fix the parser".to_string()),
            ("thought", "The parser drops the last token.".to_string()),
            ("agent", "Running the tests first.".to_string()),
            ("tool", "pnpm test --run".to_string()),
            ("tool", "Edit /code/app/src/parser.ts".to_string()),
            ("agent", "Fixed: the last token is kept.".to_string()),
        ]
    );
    let (_, status, detail, diffs) = the_tool(&entries, "tool-t1");
    assert_eq!(
        (status, detail.as_str()),
        (ToolStatus::Done, "2 tests failed")
    );
    assert!(diffs.is_empty());
    let (_, status, _, diffs) = the_tool(&entries, "tool-t2");
    assert_eq!(status, ToolStatus::Done);
    assert_eq!(
        diffs,
        vec![FileDiff {
            path: "/code/app/src/parser.ts".to_string(),
            text: "-old\n+new".to_string(),
        }]
    );
}

#[test]
fn a_tool_runs_until_its_result_and_fails_on_an_error_result_in_place() {
    let running = map(&[PROMPT, BASH]);
    assert_eq!(the_tool(&running, "tool-t1").1, ToolStatus::Running);
    let failed = tool_result("t1", r#""exit 1""#, true);
    let entries = map(&[PROMPT, BASH, &failed]);
    assert_eq!(entries.len(), 2, "the result replaces the call");
    let (_, status, detail, _) = the_tool(&entries, "tool-t1");
    assert_eq!((status, detail.as_str()), (ToolStatus::Failed, "exit 1"));
}

#[test]
fn a_result_of_blocks_joins_its_texts_marks_images_and_drops_escapes() {
    let result = tool_result(
        "t1",
        r#"[{"type":"text","text":"\u001b[31mred\u001b[0m line"},{"type":"image","source":{"type":"base64","data":"AAAA"}},{"type":"text","text":"after"}]"#,
        false,
    );
    let (_, _, detail, _) = the_tool(&map(&[BASH, &result]), "tool-t1");
    assert_eq!(detail, "red line\n[image]\nafter");
}

#[test]
fn titles_and_diffs_follow_the_tool() {
    let write = tool_use(
        "w",
        "Write",
        r#"{"file_path":"/code/app/notes.md","content":"one\ntwo"}"#,
    );
    let read = tool_use("r", "Read", r#"{"file_path":"/code/app/src/parser.ts"}"#);
    let grep = tool_use("g", "Grep", r#"{"pattern":"the-pattern","path":"/code"}"#);
    let unnamed = tool_use("n", "", "{}");
    let entries = map(&[&write, &read, &grep, &unnamed]);
    let (title, _, _, diffs) = the_tool(&entries, "tool-w");
    assert_eq!(title, "Write /code/app/notes.md");
    assert_eq!(diffs.len(), 1);
    assert_eq!(diffs[0].text, "+one\n+two");
    let (title, _, _, diffs) = the_tool(&entries, "tool-r");
    assert_eq!(title, "Read /code/app/src/parser.ts");
    assert!(diffs.is_empty());
    assert_eq!(the_tool(&entries, "tool-g").0, "Grep the-pattern");
    assert_eq!(the_tool(&entries, "tool-n").0, "Tool call");
}

#[test]
fn a_prompt_not_from_a_human_is_a_short_notice() {
    let long = "n".repeat(800);
    let task = format!(
        r#"{{"type":"user","origin":{{"kind":"task-notification"}},"message":{{"content":"{long}"}}}}"#
    );
    let human = r#"{"type":"user","origin":{"kind":"human"},"message":{"content":"go on"}}"#;
    let entries = map(&[&task, human, PROMPT]);
    assert_eq!(
        shapes(&entries),
        vec![
            ("notice", "n".repeat(500)),
            ("user", "go on".to_string()),
            ("user", "fix the parser".to_string()),
        ]
    );
}

#[test]
fn a_user_line_with_a_result_and_text_gives_both() {
    let both = r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"ok"},{"type":"text","text":"and now the docs"}]}}"#;
    let entries = map(&[BASH, both]);
    assert_eq!(
        shapes(&entries),
        vec![
            ("tool", "pnpm test --run".to_string()),
            ("user", "and now the docs".to_string()),
        ]
    );
    assert_eq!(the_tool(&entries, "tool-t1").1, ToolStatus::Done);
}

#[test]
fn an_api_error_reply_is_a_notice() {
    let error = r#"{"type":"assistant","isApiErrorMessage":true,"message":{"content":[{"type":"text","text":"API Error: overloaded"}]}}"#;
    assert_eq!(
        shapes(&map(&[error])),
        vec![("notice", "API Error: overloaded".to_string())]
    );
}

#[test]
fn unknown_and_malformed_lines_are_skipped_and_later_lines_still_map() {
    let huge = format!(
        r#"{{"type":"user","message":{{"content":"{}"}}}}"#,
        "h".repeat(3 * 1024 * 1024)
    );
    let string_input = tool_use("s", "Bash", r#""not an object""#);
    let array_content = tool_use(
        "a",
        "Write",
        r#"{"file_path":"/code/app/a.json","content":[1,2]}"#,
    );
    let entries = map(&[
        "",
        "not json",
        "[]",
        r#""text""#,
        r#"{"type":"summary","summary":"a summary"}"#,
        r#"{"type":"assistant","isSidechain":true,"message":{"content":[{"type":"text","text":"helper"}]}}"#,
        r#"{"type":"user","isMeta":true,"message":{"content":"<meta>"}}"#,
        r#"{"type":"assistant","message":{"content":[{"type":"server_tool_use","id":"x"},{"type":"text","text":"kept"}]}}"#,
        r#"{"type":"assistant","message":{"content":[{"type":"text"}]}}"#,
        &string_input,
        &array_content,
        &huge,
        PROMPT,
    ]);
    assert_eq!(
        shapes(&entries),
        vec![
            ("agent", "kept".to_string()),
            ("tool", "Bash".to_string()),
            ("tool", "Write /code/app/a.json".to_string()),
            ("user", "fix the parser".to_string()),
        ]
    );
}

#[test]
fn an_entry_over_a_cap_arrives_cut() {
    let result = tool_result("t1", &format!("\"{}\"", "o".repeat(50_000)), false);
    let (_, _, detail, _) = the_tool(&map(&[BASH, &result]), "tool-t1");
    assert_eq!(
        detail,
        format!(
            "{}\n… cut here: the rest is not kept",
            "o".repeat(OUTPUT_MAX)
        )
    );
}

/// A temp home with one project folder, removed when dropped.
struct Home(PathBuf);

impl Home {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("ob-chat-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join(".claude/projects/-code-app")).expect("dirs");
        Self(dir)
    }

    fn file(&self, id: &str) -> PathBuf {
        self.0
            .join(format!(".claude/projects/-code-app/{id}.jsonl"))
    }

    fn append(&self, id: &str, bytes: &str) {
        let mut f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.file(id))
            .expect("open");
        f.write_all(bytes.as_bytes()).expect("append");
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn more_than_kept_entries_drops_the_oldest_and_says_so() {
    let home = Home::new("kept");
    let lines: Vec<String> = (0..=KEPT)
        .map(|i| format!(r#"{{"type":"user","message":{{"content":"prompt {i}"}}}}"#))
        .collect();
    home.append(
        "s1",
        &text(&lines.iter().map(String::as_str).collect::<Vec<_>>()),
    );
    let mut reader = ChatReader::new(&home.0, "s1");
    reader.read();
    let kept = reader.kept();
    assert_eq!(kept.len(), KEPT);
    assert_eq!(shapes(&kept[..1]), vec![("user", "prompt 1".to_string())]);
    assert!(reader.earlier());
}

#[test]
fn reading_in_two_halves_matches_one_read_at_every_line_split() {
    let whole = text(&BASE);
    let one = Home::new("split-one");
    one.append("s1", &whole);
    let mut reader = ChatReader::new(&one.0, "s1");
    reader.read();
    let full = reader.kept();
    for cut in 0..=BASE.len() {
        let home = Home::new(&format!("split-{cut}"));
        let mut reader = ChatReader::new(&home.0, "s1");
        home.append("s1", &text(&BASE[..cut]));
        let first = reader.read();
        home.append("s1", &text(&BASE[cut..]));
        let second = reader.read();
        assert!(!first.reset && !second.reset);
        assert_eq!(reader.kept(), full, "split before line {cut}");
    }
}

#[test]
fn a_last_line_without_a_break_waits_for_it() {
    let home = Home::new("partial");
    home.append("s1", &format!("{PROMPT}\n{SAY}"));
    let mut reader = ChatReader::new(&home.0, "s1");
    assert_eq!(shapes(&reader.read().entries).len(), 1);
    home.append("s1", "\n");
    assert_eq!(
        shapes(&reader.read().entries),
        vec![("agent", "Running the tests first.".to_string())]
    );
}

#[test]
fn appending_returns_only_new_and_changed_entries() {
    let home = Home::new("append");
    home.append("s1", &text(&[PROMPT, BASH]));
    let mut reader = ChatReader::new(&home.0, "s1");
    assert_eq!(reader.read().entries.len(), 2);
    home.append("s1", &text(&[BASH_DONE, DONE]));
    let read = reader.read();
    assert!(!read.reset);
    assert_eq!(
        shapes(&read.entries),
        vec![
            ("tool", "pnpm test --run".to_string()),
            ("agent", "Fixed: the last token is kept.".to_string()),
        ]
    );
    assert_eq!(reader.read(), Read::default());
}

#[test]
fn a_shrunk_replaced_or_rewritten_file_is_read_again_as_a_reset() {
    let home = Home::new("reset");
    let path = home.file("s1");
    home.append("s1", &text(&[PROMPT, SAY, DONE]));
    let mut reader = ChatReader::new(&home.0, "s1");
    reader.read();

    fs::write(&path, text(&[SAY])).expect("shrink");
    let read = reader.read();
    assert!(read.reset, "shrunk");
    assert_eq!(shapes(&read.entries), shapes(&map(&[SAY])));

    let other = home.0.join("other.jsonl");
    fs::write(&other, text(&[DONE, PROMPT])).expect("write");
    fs::rename(&other, &path).expect("replace");
    let read = reader.read();
    assert!(read.reset, "replaced");
    assert_eq!(reader.kept(), map(&[DONE, PROMPT]));

    // Same inode, earlier bytes changed and the file grew.
    let rewritten = text(&[THINK, PROMPT, SAY]);
    let mut f = fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .expect("open");
    f.write_all(rewritten.as_bytes()).expect("rewrite");
    drop(f);
    let read = reader.read();
    assert!(read.reset, "rewritten in place");
    assert_eq!(reader.kept(), map(&[THINK, PROMPT, SAY]));
}

#[test]
fn a_long_file_is_read_from_its_last_window_only() {
    let home = Home::new("tail");
    let filler = |n: usize| format!("{{\"type\":\"summary\",\"pad\":\"{}\"}}\n", "f".repeat(n));
    // The head ends inside a line whose rest, alone, would be a valid prompt.
    let fragment = r#"{"type":"user","message":{"content":"cut-off fragment"}}"#;
    let head = format!("{}{}xx", filler(1000).repeat(1100), filler(10));
    let end = text(&[PROMPT, SAY, DONE]);
    // The window must start exactly at the fragment: size the filler after it.
    let window = usize::try_from(OPEN_TAIL).expect("small");
    let short = window - fragment.len() - 1 - end.len();
    let mut middle = filler(1000).repeat(short / filler(1000).len() - 1);
    middle.push_str(&filler(short - middle.len() - filler(0).len()));
    assert_eq!(middle.len(), short);
    home.append("s1", &format!("{head}{fragment}\n{middle}{end}"));

    let mut reader = ChatReader::new(&home.0, "s1");
    let read = reader.read();
    assert_eq!(shapes(&read.entries), shapes(&map(&[PROMPT, SAY, DONE])));
    assert!(reader.earlier());
}

#[test]
fn a_transcript_that_appears_later_is_read_without_a_reset() {
    let home = Home::new("later");
    let mut reader = ChatReader::new(&home.0, "s1");
    assert_eq!(reader.read(), Read::default());
    home.append("s1", &text(&[PROMPT]));
    let read = reader.read();
    assert!(!read.reset);
    assert_eq!(read.entries.len(), 1);
}

#[test]
fn of_two_project_folders_the_file_modified_last_is_read() {
    let home = Home::new("two");
    let older = home.0.join(".claude/projects/-code-old/s1.jsonl");
    fs::create_dir_all(older.parent().expect("parent")).expect("dirs");
    fs::write(&older, text(&[SAY])).expect("write");
    home.append("s1", &text(&[DONE]));
    let day_ago = std::time::SystemTime::now() - std::time::Duration::from_hours(24);
    fs::File::options()
        .write(true)
        .open(&older)
        .expect("open")
        .set_modified(day_ago)
        .expect("touch");
    assert_eq!(find(&home.0, "s1"), Some(home.file("s1")));
    let mut reader = ChatReader::new(&home.0, "s1");
    assert_eq!(shapes(&reader.read().entries), shapes(&map(&[DONE])));
}

#[test]
fn ids_links_and_folders_that_leave_the_projects_folder_are_refused() {
    use std::os::unix::fs::symlink;
    let home = Home::new("refused");
    home.append("ok", &text(&[PROMPT]));
    assert!(find(&home.0, "ok").is_some());
    let long = "a".repeat(65);
    for id in ["a/b", "..", "a.b", "", long.as_str()] {
        assert_eq!(find(&home.0, id), None, "{id:?}");
        assert_eq!(ChatReader::new(&home.0, id).read(), Read::default());
    }

    let outside = home.0.join("outside");
    fs::create_dir_all(&outside).expect("dirs");
    fs::write(outside.join("linked.jsonl"), text(&[PROMPT])).expect("write");
    symlink(outside.join("linked.jsonl"), home.file("linked")).expect("link");
    fs::write(outside.join("infolder.jsonl"), text(&[PROMPT])).expect("write");
    symlink(&outside, home.0.join(".claude/projects/-linked-folder")).expect("link");
    fs::create_dir_all(home.file("dir")).expect("dir");
    for id in ["linked", "infolder", "dir"] {
        assert_eq!(find(&home.0, id), None, "{id}");
        assert_eq!(ChatReader::new(&home.0, id).read(), Read::default());
    }
}
