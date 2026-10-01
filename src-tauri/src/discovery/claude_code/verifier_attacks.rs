use super::*;
use std::io::{Cursor, Write};

const LINES: &[&str] = &[
    r#"{"type":"ai-title","aiTitle":"Zażółć gęślą jaźń 🚀"}"#,
    r#"{"type":"user","cwd":"/code/ąę","timestamp":"t0","message":{"role":"user","content":"napraw parser 🚀"}}"#,
    "",
    r#"{"type":"assistant","timestamp":"t1","message":{"id":"m1","usage":{"input_tokens":10,"output_tokens":1},"content":[{"type":"thinking","thinking":"…"}]}}"#,
    r#"{"type":"assistant","timestamp":"t2","message":{"id":"m1","usage":{"input_tokens":10,"output_tokens":20},"content":[{"type":"text","text":"Pierwsza część — ✓"}]}}"#,
    r#"{"type":"assistant","timestamp":"t3","message":{"id":"m2","usage":{"input_tokens":3,"output_tokens":4},"content":[{"type":"tool_use","id":"toolu_q","name":"AskUserQuestion","input":{"questions":[{"question":"Który backend? 🤔","header":"B","multiSelect":false,"options":[{"label":"Rust","description":"ż"}]}]}}]}}"#,
    r#"{"type":"user","timestamp":"t4","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_q","content":"Rust"}]}}"#,
    r#"{"type":"user","origin":{"kind":"human"},"timestamp":"t5","message":{"role":"user","content":"dalej ✨"}}"#,
    r#"{"type":"assistant","timestamp":"t6","message":{"id":"m3","usage":{"input_tokens":1,"output_tokens":1},"content":[{"type":"text","text":"Gotowe 🎉"}]}}"#,
];

fn text(crlf: bool) -> Vec<u8> {
    let nl = if crlf { "\r\n" } else { "\n" };
    LINES
        .iter()
        .flat_map(|l| [*l, nl])
        .collect::<String>()
        .into_bytes()
}

fn full(bytes: &[u8]) -> Transcript {
    parse_transcript(Cursor::new(bytes.to_vec()))
}

/// Mimics `ClaudeSource`: first scan sees bytes[..a], second bytes[..b] resumed from the returned offset, third the rest.
#[test]
fn every_byte_split_pair_matches_full_parse() {
    for crlf in [false, true] {
        let bytes = text(crlf);
        let expected = full(&bytes);
        let n = bytes.len();
        for a in 0..=n {
            for b in (a..=n).step_by(7).chain([n]) {
                let mut s = ParserState::new();
                let o1 = s.feed_lines(Cursor::new(bytes[..a].to_vec()), 0, true);
                let o2 = s.feed_lines(
                    Cursor::new(bytes[usize::try_from(o1).unwrap()..b].to_vec()),
                    o1,
                    true,
                );
                let o3 = s.feed_lines(
                    Cursor::new(bytes[usize::try_from(o2).unwrap()..].to_vec()),
                    o2,
                    true,
                );
                assert_eq!(usize::try_from(o3).unwrap(), n, "crlf={crlf} a={a} b={b}");
                assert_eq!(s.finish(), expected, "crlf={crlf} a={a} b={b}");
            }
        }
    }
}

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ob-verify-{}-{}", tag, std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).expect("dir");
    d
}

/// Real files, appended in odd-sized chunks (splitting multibyte chars), scanned after every chunk.
#[test]
fn real_file_chunked_appends_match_full_parse() {
    let d = dir("chunks");
    let path = d.join("s.jsonl");
    let bytes = text(false);
    for chunk in [1usize, 2, 3, 5, 13, 97] {
        fs::write(&path, b"").expect("w");
        let mut src = ClaudeSource::new(d.clone());
        let mut i = 0;
        while i < bytes.len() {
            let j = (i + chunk).min(bytes.len());
            fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .expect("o")
                .write_all(&bytes[i..j])
                .expect("w");
            let got = src.transcript(&path).expect("t");
            let complete = bytes[..j]
                .iter()
                .rposition(|&c| c == b'\n')
                .map_or(0, |p| p + 1);
            assert_eq!(got, full(&bytes[..complete]), "chunk={chunk} j={j}");
            i = j;
        }
    }
    fs::remove_dir_all(&d).expect("cleanup");
}

/// Divergence from the old reader: a final line without '\n' is never consumed by `ClaudeSource`.
#[test]
fn final_line_without_newline_is_never_consumed() {
    let d = dir("nonl");
    let path = d.join("s.jsonl");
    let mut bytes = text(false);
    bytes.pop();
    fs::write(&path, &bytes).expect("w");
    let mut src = ClaudeSource::new(d.clone());
    let got = src.transcript(&path).expect("t");
    let old = full(&bytes);
    assert_eq!(old.reply_to_human, "Gotowe 🎉");
    assert_eq!(
        got.reply_to_human, "",
        "ClaudeSource defers the unterminated last line"
    );
    fs::remove_dir_all(&d).expect("cleanup");
}

/// Same inode, truncated and rewritten (`O_TRUNC`, e.g. `fs::write` / writeFileSync) to a length >= the old offset.
#[test]
fn same_inode_truncate_and_rewrite_longer_is_reparsed() {
    let d = dir("rewrite");
    let path = d.join("s.jsonl");
    let bytes = text(false);
    fs::write(&path, &bytes).expect("w");
    let mut src = ClaudeSource::new(d.clone());
    assert!(src.transcript(&path).expect("t").title.is_some());
    let ino = fs::metadata(&path).expect("m").ino();
    let other = r#"{"type":"user","cwd":"/other/dir","timestamp":"z","message":{"role":"user","content":"x"}}"#;
    let mut rewritten = String::new();
    while rewritten.len() < bytes.len() + 10 {
        rewritten.push_str(other);
        rewritten.push('\n');
    }
    fs::write(&path, &rewritten).expect("rewrite");
    assert_eq!(fs::metadata(&path).expect("m").ino(), ino, "same inode");
    let got = src.transcript(&path).expect("t");
    assert_eq!(
        got,
        full(rewritten.as_bytes()),
        "stale state survived an in-place rewrite"
    );
    fs::remove_dir_all(&d).expect("cleanup");
}

/// Mirrors Claude Code 2.1.284 `performRemoveByUuid`: open r+, truncate to the start of the
/// tombstoned line, write back what followed it (same inode, file shrinks), then the session
/// keeps appending before the next scan.
#[test]
fn tombstone_removal_then_append_before_next_scan() {
    let d = dir("tombstone");
    let path = d.join("s.jsonl");
    let user = r#"{"type":"user","uuid":"u1","cwd":"/code/app","timestamp":"t0","message":{"role":"user","content":"go"}}"#;
    let orphan = r#"{"type":"assistant","uuid":"x9","timestamp":"t1","message":{"id":"m_orphan","usage":{"input_tokens":500,"output_tokens":500},"content":[{"type":"tool_use","id":"toolu_orphan","name":"Bash","input":{}}]}}"#;
    fs::write(&path, format!("{user}\n{orphan}\n")).expect("w");
    let mut src = ClaudeSource::new(d.clone());
    assert_eq!(src.transcript(&path).expect("t").open_tools, 1);

    let keep = u64::try_from(user.len()).unwrap() + 1;
    {
        let f = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .expect("r+");
        f.set_len(keep).expect("truncate");
    }
    let retry = r#"{"type":"assistant","uuid":"a2","timestamp":"t2","message":{"id":"m_retry","usage":{"input_tokens":7,"output_tokens":3},"content":[{"type":"text","text":"retried answer, long enough to cover the removed line's bytes ....................................................................................................."}]}}"#;
    let human = r#"{"type":"user","uuid":"u3","origin":{"kind":"human"},"timestamp":"t3","message":{"role":"user","content":"thanks"}}"#;
    fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .expect("o")
        .write_all(format!("{retry}\n{human}\n").as_bytes())
        .expect("w");

    let got = src.transcript(&path).expect("t");
    let expected = full(&fs::read(&path).expect("r"));
    assert_eq!(
        got, expected,
        "tombstoned line kept and appended lines skipped"
    );
    fs::remove_dir_all(&d).expect("cleanup");
}

#[test]
fn disappear_then_reappear() {
    let d = dir("gone");
    let path = d.join("s.jsonl");
    let bytes = text(false);
    fs::write(&path, &bytes).expect("w");
    let mut src = ClaudeSource::new(d.clone());
    src.transcript(&path).expect("t");
    fs::remove_file(&path).expect("rm");
    assert!(src.transcript(&path).is_none());
    let other = format!(
        "{}\n",
        r#"{"type":"user","cwd":"/other/dir","timestamp":"z","message":{"role":"user","content":"x"}}"#
    );
    let mut big = String::new();
    while big.len() < bytes.len() + 10 {
        big.push_str(&other);
    }
    fs::write(&path, &big).expect("w");
    assert_eq!(src.transcript(&path).expect("t"), full(big.as_bytes()));
    fs::remove_dir_all(&d).expect("cleanup");
}
