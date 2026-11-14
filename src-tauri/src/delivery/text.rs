//! What may be typed into a desk's terminal, and what counts as A Human typing.

use crate::pty::plain_text;

/// The most a note may hold, in characters.
pub const MESSAGE_MAX: usize = 4000;
/// What submits a typed note.
pub const SUBMIT: &str = "\r";

const PASTE_START: &str = "\x1b[200~";
const PASTE_END: &str = "\x1b[201~";

/// Zero-width and bidirectional controls: they hide or reorder what a reader sees.
fn is_invisible(c: char) -> bool {
    matches!(
        c,
        '\u{200b}'..='\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{2069}'
            | '\u{feff}'
    )
}

/// `raw` made safe to type into a terminal, show in a panel and keep in a file.
/// Err: it is empty once cleaned, or longer than `max` characters.
pub fn clean(raw: &str, max: usize) -> Result<String, String> {
    let plain: String = plain_text(raw)
        .chars()
        .filter_map(|c| match c {
            '\t' => Some(' '),
            '\u{2028}' | '\u{2029}' => Some('\n'),
            c if is_invisible(c) => None,
            c => Some(c),
        })
        .collect();
    let mut lines: Vec<&str> = Vec::new();
    for line in plain.split('\n').map(str::trim_end) {
        // Two blank lines in a row would make three line breaks.
        if !(line.is_empty() && lines.last().is_some_and(|l| l.is_empty())) {
            lines.push(line);
        }
    }
    let text = lines.join("\n").trim().to_string();
    if text.is_empty() {
        return Err("there is nothing to send".to_string());
    }
    if text.chars().count() > max {
        return Err(format!("that is longer than {max} characters"));
    }
    Ok(text)
}

/// `clean`, then on one line: each run of line breaks and spaces becomes one space.
pub fn line(raw: &str, max: usize) -> Result<String, String> {
    Ok(clean(raw, max)?
        .split([' ', '\n'])
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" "))
}

/// The bytes that put `text` in a terminal's input box, without submitting it.
#[must_use]
pub fn typed(text: &str, bracketed_paste: bool) -> String {
    if bracketed_paste {
        format!("{PASTE_START}{text}{PASTE_END}")
    } else {
        text.replace('\n', " ")
    }
}

/// Whether `data`, sent by a terminal panel, holds anything A Human did (text, Enter, an
/// arrow, Esc or a control key) and not only a report the terminal sends by itself.
#[must_use]
pub fn is_typing(data: &str) -> bool {
    let mut rest = data;
    while !rest.is_empty() {
        match terminal_report(rest) {
            Some(len) => rest = &rest[len..],
            None => return true,
        }
    }
    false
}

/// The length of the report `data` starts with: a focus, mouse, device-attribute,
/// status, cursor-position, mode or window report, or an OSC or DCS reply.
fn terminal_report(data: &str) -> Option<usize> {
    if let Some(body) = data
        .strip_prefix("\x1b]")
        .or_else(|| data.strip_prefix("\x1bP"))
    {
        let end = body.find(['\x07', '\x1b'])?;
        let terminator = if body[end..].starts_with('\x07') {
            1
        } else if body[end..].starts_with("\x1b\\") {
            2
        } else {
            return None;
        };
        return Some(2 + end + terminator);
    }
    let body = data.strip_prefix("\x1b[")?;
    let end = body.find(|c: char| ('\x40'..='\x7e').contains(&c))?;
    let (params, last) = (&body[..end], body.as_bytes()[end]);
    let numbers = |p: &str| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit() || c == ';');
    let report = match last {
        b'I' | b'O' => params.is_empty(),
        b'M' | b'm' => params.starts_with('<'),
        b'c' => params.starts_with(['?', '>']),
        b'R' | b'n' | b't' => numbers(params.strip_prefix('?').unwrap_or(params)),
        b'y' => params
            .strip_suffix('$')
            .is_some_and(|p| numbers(p.strip_prefix('?').unwrap_or(p))),
        _ => false,
    };
    report.then_some(2 + end + 1)
}

/// Whether the input box still holds something unsent after `data`: it has typing after
/// its last Enter or Ctrl-C, or no Enter at all.
#[must_use]
pub fn leaves_draft(data: &str) -> bool {
    match data.rfind(['\r', '\n', '\x03']) {
        Some(i) => is_typing(&data[i + 1..]),
        None => is_typing(data),
    }
}

/// Whether `data` holds Enter.
#[must_use]
pub fn has_enter(data: &str) -> bool {
    data.contains(['\r', '\n'])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_removes_escapes_and_controls() {
        assert_eq!(clean("\x1b[31mred\x1b[0m", 100).as_deref(), Ok("red"));
        assert_eq!(
            clean("\x1b]0;title\x07a\x1b]8;;x\x1b\\b", 100).as_deref(),
            Ok("ab")
        );
        assert_eq!(
            clean("a\x00b\x07c\x7fd\u{85}e\u{9c}f", 100).as_deref(),
            Ok("abcdef")
        );
        assert_eq!(
            clean(
                "a\u{200b}b\u{202e}c\u{2066}d\u{2069}e\u{feff}f\u{2060}g\u{200f}h",
                100
            )
            .as_deref(),
            Ok("abcdefgh")
        );
    }

    #[test]
    fn clean_tidies_lines_and_spaces() {
        assert_eq!(clean("a\r\nb", 100).as_deref(), Ok("a\nb"));
        assert_eq!(clean("a\tb", 100).as_deref(), Ok("a b"));
        assert_eq!(clean("a\u{2028}b\u{2029}c", 100).as_deref(), Ok("a\nb\nc"));
        assert_eq!(clean("a\n\n\nb", 100).as_deref(), Ok("a\n\nb"));
        assert_eq!(clean("a  \n \n\n \nb", 100).as_deref(), Ok("a\n\nb"));
        assert_eq!(clean("  \n a \n  ", 100).as_deref(), Ok("a"));
    }

    #[test]
    fn clean_refuses_nothing_left() {
        for raw in ["", "  \n\t", "\x1b[0m\u{200b}", "\x07"] {
            assert_eq!(
                clean(raw, 100),
                Err("there is nothing to send".to_string()),
                "{raw:?}"
            );
        }
    }

    #[test]
    fn the_cap_counts_characters_not_bytes() {
        assert!(clean(&"é".repeat(MESSAGE_MAX), MESSAGE_MAX).is_ok());
        assert_eq!(
            clean(&"é".repeat(MESSAGE_MAX + 1), MESSAGE_MAX),
            Err("that is longer than 4000 characters".to_string())
        );
    }

    #[test]
    fn a_note_cannot_end_the_paste_early() {
        let text = clean("one\x1b[201~two\n/exit", 100).expect("clean");
        assert!(!text.contains('\x1b'));
        let bytes = typed(&text, true);
        assert_eq!(bytes.matches(PASTE_END).count(), 1);
        assert!(bytes.starts_with(PASTE_START) && bytes.ends_with(PASTE_END));
        assert!(bytes.contains("two\n/exit"));
        assert!(!bytes.contains(SUBMIT));
    }

    #[test]
    fn line_gives_one_line() {
        assert_eq!(line("a \n\n  b\nc  d", 100).as_deref(), Ok("a b c d"));
        assert!(line(" \n", 100).is_err());
    }

    #[test]
    fn typed_without_bracketed_paste_has_no_line_break() {
        let bytes = typed("a\nb\n\nc", false);
        assert_eq!(bytes, "a b  c");
        assert!(!bytes.contains(['\n', '\r']));
    }

    #[test]
    fn typing_is_text_or_enter_not_a_terminal_report() {
        for data in [
            "a",
            "\r",
            "\n",
            "\x1b[Aé",
            "\x1b[A",
            "\x1b",
            "\x1b\x1b",
            "\x03",
            " ",
            "\x1b[200~x\x1b[201~",
            "\x1b]",
            "\x1bPx",
            "\x1b[$y",
            "\x1b[0nx",
        ] {
            assert!(is_typing(data), "{data:?}");
        }
        for data in [
            "\x1b[I",
            "\x1b[O",
            "\x1b[<0;10;5M",
            "\x1b[?1;2c",
            "\x1b[12;40R",
            "\x1b[I\x1b[O",
            "\x1b[0n",
            "\x1b[?2026;2$y",
            "\x1b[4;600;800t",
            "\x1b]11;rgb:1e1e/1e1e/1e1e\x1b\\",
            "\x1b]10;rgb:ffff/ffff/ffff\x07",
            "\x1bP1$r0m\x1b\\",
            "\x1b]11;rgb:0/0/0\x07\x1b[0n\x1b[?1;2c",
        ] {
            assert!(!is_typing(data), "{data:?}");
        }
        assert!(!is_typing(""));
        assert!(has_enter("x\r") && has_enter("\n") && !has_enter("x"));
    }

    #[test]
    fn a_draft_stays_until_enter_or_ctrl_c() {
        assert!(leaves_draft("a") && leaves_draft("\x1b[A") && leaves_draft("\x1b"));
        assert!(!leaves_draft("\r") && !leaves_draft("\x03") && !leaves_draft("ab\r"));
        assert!(leaves_draft("a\rb") && !leaves_draft("\x1b[I"));
        assert!(!leaves_draft("\x1b]11;rgb:0/0/0\x1b\\\x1b[?2026;2$y"));
    }
}
