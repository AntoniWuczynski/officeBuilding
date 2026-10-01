//! In-app terminals: each hired agent runs in its own pseudo-terminal.
//!
//! A reader thread decodes the agent's output and streams it to the webview;
//! the last [`SCROLLBACK`] bytes are kept so a panel that opens (or reopens)
//! later can replay them. Output travels as UTF-8 text rather than base64: it
//! is valid JSON as is, a third smaller on the wire, and xterm.js writes
//! strings directly. Multi-byte characters split across reads are carried over
//! to the next read, and invalid bytes become U+FFFD.
//!
//! Keystrokes go through a writer thread per terminal, so typing into an agent
//! that is not reading never blocks the caller.
//!
//! Terminals live as long as the app: closing a panel only stops listening.

use crate::model::{TerminalBacklog, TerminalExit, TerminalOutput};
use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, mpsc};
use std::time::{Duration, Instant};

pub const OUTPUT_EVENT: &str = "office://terminal-output";
pub const EXIT_EVENT: &str = "office://terminal-exit";
/// How much output each terminal keeps for replay.
pub const SCROLLBACK: usize = 256 * 1024;
/// After the agent exits, how long to wait for its last output before announcing the exit.
const DRAIN: Duration = Duration::from_millis(500);
const INITIAL_SIZE: PtySize = PtySize {
    rows: 24,
    cols: 100,
    pixel_width: 0,
    pixel_height: 0,
};
/// Login shells known to take `-l -i -c <command>`. tcsh and csh take `-l`
/// only on its own, so a launch through them goes through zsh instead.
const LOGIN_SHELLS: &[&str] = &["zsh", "bash", "sh", "fish"];
const FALLBACK_SHELL: &str = "/bin/zsh";
/// DEC private modes a replay has to restore once the output that set them
/// has been trimmed: cursor keys, autowrap, cursor, alternate screen, mouse
/// reporting, focus events, bracketed paste.
const TRACKED_MODES: &[u16] = &[
    1, 7, 25, 47, 1000, 1002, 1003, 1004, 1005, 1006, 1015, 1047, 1049, 2004,
];
/// Tracked modes that a fresh terminal starts with switched on.
const MODES_ON_BY_DEFAULT: &[u16] = &[7, 25];
/// Variables that tie a process to the agent session or the terminal the app
/// was started from; a hired agent must not inherit them. The Claude Code ones
/// are what a Claude Code session sets for its children (read from `env` under
/// Claude Code 2.1.285); the rest name the launching terminal or multiplexer.
/// Claude Code's own configuration (`CLAUDE_CODE_USE_BEDROCK` and the like) is kept.
const LAUNCHER_IDENTITY: &[&str] = &[
    "CLAUDECODE",
    "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_EXECPATH",
    "CLAUDE_CODE_MESSAGING_SOCKET",
    "CLAUDE_CODE_MESSAGING_TOKEN",
    "CLAUDE_CODE_SESSION_ATTENDED",
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_EFFORT",
    "CLAUDE_PID",
    "AI_AGENT",
    "TERM_PROGRAM",
    "TERM_PROGRAM_VERSION",
    "TERM_SESSION_ID",
    "TMUX",
    "TMUX_PANE",
    "STY",
    "WINDOW",
    "ITERM_SESSION_ID",
    "ITERM_PROFILE",
    "LC_TERMINAL",
    "LC_TERMINAL_VERSION",
    "KITTY_WINDOW_ID",
    "KITTY_PID",
    "KITTY_LISTEN_ON",
    "WEZTERM_PANE",
    "WEZTERM_UNIX_SOCKET",
    "ALACRITTY_WINDOW_ID",
    "ALACRITTY_SOCKET",
    "WINDOWID",
];

/// Tracked terminal modes as they stood at some point of the output.
#[derive(Debug, Default, Clone, PartialEq)]
struct Modes {
    private: BTreeMap<u16, bool>,
    /// Application keypad (`ESC =`) or numeric (`ESC >`), once either was seen.
    keypad_application: Option<bool>,
}

/// What a terminal tells the webview.
#[derive(Debug, Clone, PartialEq)]
pub enum TerminalEvent {
    Output(TerminalOutput),
    Exit(TerminalExit),
}

pub type Sink = Arc<dyn Fn(TerminalEvent) + Send + Sync>;

struct State {
    scrollback: String,
    /// Chunks emitted so far: lets a panel drop live events its backlog already holds.
    seq: u64,
    size: PtySize,
    /// `None` while the agent runs; then its exit code, if one could be read.
    #[expect(
        clippy::option_option,
        reason = "running, exited with a code, and exited without one are all distinct"
    )]
    exit: Option<Option<u32>>,
    /// Tracked modes as they stood where the kept scrollback begins.
    modes_at_start: Modes,
}

struct Terminal {
    pid: i32,
    master: Mutex<Box<dyn MasterPty + Send>>,
    /// Feeds the writer thread; `None` once the agent has exited.
    input: Mutex<Option<mpsc::Sender<Vec<u8>>>>,
    state: Mutex<State>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    // A panic while holding a lock leaves plain data behind; keep serving it.
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

fn is_login_shell(shell: &OsStr) -> bool {
    Path::new(shell)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| LOGIN_SHELLS.contains(&name))
}

/// The shell a hired agent starts through: the user's own when it is one known
/// to take `-l -i -c`, else zsh.
#[must_use]
pub fn login_shell() -> String {
    std::env::var("SHELL")
        .ok()
        .filter(|shell| is_login_shell(OsStr::new(shell)))
        .unwrap_or_else(|| FALLBACK_SHELL.to_string())
}

/// Single-quote `s` as one word for `shell`. fish also treats `\\` and `\'` as
/// escapes inside single quotes; POSIX shells take everything literally but `'`.
#[must_use]
pub fn quote(shell: &str, s: &str) -> String {
    if Path::new(shell)
        .file_name()
        .is_some_and(|name| name == "fish")
    {
        format!("'{}'", s.replace('\\', r"\\").replace('\'', r"\'"))
    } else {
        format!("'{}'", s.replace('\'', r"'\''"))
    }
}

/// A login-shell launch (`<shell> -l -i -c …`) through a shell not known to
/// take those flags goes through zsh instead.
fn use_known_login_shell(cmd: &mut CommandBuilder) {
    let argv = cmd.get_argv_mut();
    let login =
        argv.iter()
            .skip(1)
            .take(3)
            .map(|a| a.to_str())
            .eq([Some("-l"), Some("-i"), Some("-c")]);
    if login
        && !argv.first().is_some_and(|shell| is_login_shell(shell))
        && let Some(shell) = argv.first_mut()
    {
        *shell = FALLBACK_SHELL.into();
    }
}

/// Every terminal the app has started, by terminal id.
pub struct Terminals {
    sink: Sink,
    next: AtomicU64,
    all: Mutex<BTreeMap<String, Arc<Terminal>>>,
}

impl Terminals {
    pub fn new(sink: Sink) -> Self {
        Self {
            sink,
            next: AtomicU64::new(1),
            all: Mutex::new(BTreeMap::new()),
        }
    }

    /// Run `command` through the user's login shell (so it finds the tools on
    /// their usual PATH), as Terminal.app would. `exec` keeps the agent's pid.
    /// Quote its words for [`login_shell`].
    pub fn spawn_shell(&self, cwd: &Path, command: &str) -> Result<String, String> {
        let mut cmd = CommandBuilder::new(login_shell());
        cmd.args(["-l", "-i", "-c", &format!("exec {command}")]);
        self.spawn(cmd, cwd)
    }

    /// Start `cmd` in a new pseudo-terminal in `cwd`. Returns the terminal id.
    pub fn spawn(&self, mut cmd: CommandBuilder, cwd: &Path) -> Result<String, String> {
        // portable-pty silently falls back to the home folder for a missing cwd.
        if !cwd.is_dir() {
            return Err(format!(
                "{} is not a folder on this Mac any more",
                cwd.display()
            ));
        }
        use_known_login_shell(&mut cmd);
        cmd.cwd(cwd);
        cmd.env("TERM", "xterm-256color");
        if cmd.get_env("LANG").is_none() {
            cmd.env("LANG", "en_US.UTF-8");
        }
        for key in LAUNCHER_IDENTITY {
            cmd.env_remove(key);
        }
        let pair = native_pty_system()
            .openpty(INITIAL_SIZE)
            .map_err(|e| format!("could not open a terminal: {e}"))?;
        let mut child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| format!("could not start the agent: {e}"))?;
        // The reader only sees end-of-file once no one else holds the agent's side open.
        drop(pair.slave);
        let pid = child
            .process_id()
            .and_then(|p| i32::try_from(p).ok())
            .ok_or("the agent has no process id")?;
        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| format!("could not read the terminal: {e}"))?;
        let mut writer = pair
            .master
            .take_writer()
            .map_err(|e| format!("could not write to the terminal: {e}"))?;
        let id = format!("t-{}", self.next.fetch_add(1, Ordering::Relaxed));
        let (input, typed) = mpsc::channel::<Vec<u8>>();
        let terminal = Arc::new(Terminal {
            pid,
            master: Mutex::new(pair.master),
            input: Mutex::new(Some(input)),
            state: Mutex::new(State {
                scrollback: String::new(),
                seq: 0,
                size: INITIAL_SIZE,
                exit: None,
                modes_at_start: Modes::default(),
            }),
        });
        lock(&self.all).insert(id.clone(), terminal.clone());

        std::thread::Builder::new()
            .name(format!("pty-write-{id}"))
            .spawn(move || {
                // Ends when the agent exits (the sender is dropped) or its side is gone.
                for bytes in typed {
                    if writer
                        .write_all(&bytes)
                        .and_then(|()| writer.flush())
                        .is_err()
                    {
                        return;
                    }
                }
            })
            .map_err(|e| format!("could not start the terminal writer: {e}"))?;
        let (drained_tx, drained_rx) = mpsc::channel::<()>();
        let (t, sink, tid) = (terminal.clone(), self.sink.clone(), id.clone());
        std::thread::Builder::new()
            .name(format!("pty-read-{id}"))
            .spawn(move || {
                pump(reader, &t, &sink, &tid);
                // The waiter may have given up on us already.
                let _ = drained_tx.send(());
            })
            .map_err(|e| format!("could not start the terminal reader: {e}"))?;
        let (sink, tid) = (self.sink.clone(), id.clone());
        std::thread::Builder::new()
            .name(format!("pty-wait-{id}"))
            .spawn(move || {
                let code = child.wait().ok().map(|s| s.exit_code());
                // A grandchild can keep the terminal open; do not wait on it forever.
                let _ = drained_rx.recv_timeout(DRAIN);
                *lock(&terminal.input) = None;
                lock(&terminal.state).exit = Some(code);
                sink(TerminalEvent::Exit(TerminalExit {
                    terminal_id: tid,
                    exit_code: code,
                }));
            })
            .map_err(|e| format!("could not start the terminal waiter: {e}"))?;
        Ok(id)
    }

    fn get(&self, id: &str) -> Result<Arc<Terminal>, String> {
        lock(&self.all)
            .get(id)
            .cloned()
            .ok_or_else(|| format!("unknown terminal: {id}"))
    }

    /// Terminal ids by the pid of their still-running agent.
    pub fn running_children(&self) -> BTreeMap<i32, String> {
        lock(&self.all)
            .iter()
            .filter(|(_, t)| lock(&t.state).exit.is_none())
            .map(|(id, t)| (t.pid, id.clone()))
            .collect()
    }

    /// The pid of the process started in a terminal.
    pub fn pid(&self, id: &str) -> Result<i32, String> {
        Ok(self.get(id)?.pid)
    }

    /// Wait up to `within` for the agent to exit. `Some(exit code)` if it did.
    pub fn wait_exit(&self, id: &str, within: Duration) -> Result<Option<Option<u32>>, String> {
        let t = self.get(id)?;
        let deadline = Instant::now() + within;
        loop {
            if let Some(code) = lock(&t.state).exit {
                return Ok(Some(code));
            }
            if Instant::now() >= deadline {
                return Ok(None);
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    /// Everything a panel needs to catch up before live events.
    pub fn backlog(&self, id: &str) -> Result<TerminalBacklog, String> {
        let t = self.get(id)?;
        let s = lock(&t.state);
        Ok(TerminalBacklog {
            terminal_id: id.to_string(),
            seq: s.seq,
            data: format!("{}{}", mode_prefix(&s.modes_at_start), s.scrollback),
            running: s.exit.is_none(),
            exit_code: s.exit.flatten(),
            cols: s.size.cols,
            rows: s.size.rows,
        })
    }

    /// Keystrokes from the panel, as the terminal would send them. Queued for
    /// the writer thread, so this returns at once even if the agent is busy.
    pub fn write(&self, id: &str, data: &str) -> Result<(), String> {
        let t = self.get(id)?;
        let input = lock(&t.input);
        let exited = || "the agent has exited".to_string();
        input
            .as_ref()
            .ok_or_else(exited)?
            .send(data.as_bytes().to_vec())
            .map_err(|_| exited())
    }

    pub fn resize(&self, id: &str, cols: u16, rows: u16) -> Result<(), String> {
        if cols == 0 || rows == 0 {
            return Err("a terminal needs at least one row and one column".to_string());
        }
        let t = self.get(id)?;
        let size = PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        };
        lock(&t.master)
            .resize(size)
            .map_err(|e| format!("could not resize the terminal: {e}"))?;
        lock(&t.state).size = size;
        Ok(())
    }
}

/// Read until the agent's side closes, keeping scrollback and emitting each chunk.
fn pump(mut reader: Box<dyn Read + Send>, t: &Terminal, sink: &Sink, id: &str) {
    let mut buf = [0u8; 8192];
    let mut carry: Vec<u8> = Vec::new();
    loop {
        let n = match reader.read(&mut buf) {
            // macOS reports a closed terminal as an error (EIO) rather than end-of-file.
            Ok(0) | Err(_) => return,
            Ok(n) => n,
        };
        carry.extend_from_slice(&buf[..n]);
        let text = decode_utf8(&mut carry);
        if text.is_empty() {
            continue;
        }
        let mut s = lock(&t.state);
        s.scrollback.push_str(&text);
        let dropped = trim_front(&mut s.scrollback, SCROLLBACK);
        track_modes(&dropped, &mut s.modes_at_start);
        s.seq += 1;
        // Emitting under the lock keeps `seq` and the backlog consistent for a panel attaching now.
        sink(TerminalEvent::Output(TerminalOutput {
            terminal_id: id.to_string(),
            seq: s.seq,
            data: text,
        }));
    }
}

/// Decode as much of `bytes` as possible, leaving an incomplete trailing character in it.
pub fn decode_utf8(bytes: &mut Vec<u8>) -> String {
    let mut out = String::new();
    let mut rest: &[u8] = bytes;
    loop {
        match std::str::from_utf8(rest) {
            Ok(s) => {
                out.push_str(s);
                rest = &[];
                break;
            }
            Err(e) => {
                let (valid, after) = rest.split_at(e.valid_up_to());
                out.push_str(std::str::from_utf8(valid).unwrap_or_default());
                if let Some(bad) = e.error_len() {
                    out.push(char::REPLACEMENT_CHARACTER);
                    rest = &after[bad..];
                } else {
                    // Cut off mid-character: wait for the next read.
                    rest = after;
                    break;
                }
            }
        }
    }
    let keep = rest.to_vec();
    *bytes = keep;
    out
}

/// Characters that start a control sequence: ESC, and the 8-bit CSI and OSC.
fn is_introducer(c: char) -> bool {
    matches!(c, '\x1b' | '\u{9b}' | '\u{9d}')
}

/// Where the control sequence starting at byte `at` of `s` ends (exclusive),
/// or `None` while it is unterminated. Covers CSI (`ESC [ … final`), the
/// strings (OSC `ESC ] … BEL` or `… ESC \`, DCS, APC, PM, SOS) and short
/// escapes (`ESC ( B`, `ESC =`).
fn sequence_end(s: &str, at: usize) -> Option<usize> {
    let rest = s.get(at..)?;
    let mut chars = rest.char_indices();
    let (_, intro) = chars.next()?;
    let end_at = |i: usize, c: char| Some(at + i + c.len_utf8());
    let kind = match intro {
        '\u{9b}' => '[',
        '\u{9d}' => ']',
        _ => {
            let (i, c) = chars.next()?;
            if !matches!(c, '[' | ']' | 'P' | '_' | '^' | 'X') {
                if !('\x20'..='\x2f').contains(&c) {
                    return end_at(i, c);
                }
                // Intermediates, then one final character.
                return chars
                    .find(|(_, c)| !('\x20'..='\x2f').contains(c))
                    .and_then(|(i, c)| end_at(i, c));
            }
            c
        }
    };
    if kind == '[' {
        for (i, c) in chars {
            if ('\x40'..='\x7e').contains(&c) {
                return end_at(i, c);
            }
            if !('\x20'..='\x3f').contains(&c) {
                // Aborted by a character that cannot be part of it.
                return Some(at + i);
            }
        }
        return None;
    }
    let mut after_esc = false;
    for (i, c) in chars {
        if c == '\x07' || c == '\u{9c}' || (after_esc && c == '\\') {
            return end_at(i, c);
        }
        after_esc = c == '\x1b';
    }
    None
}

/// Drop the oldest text so at most `cap` bytes remain, cutting on a character
/// boundary and never inside a control sequence of any length: a sequence the
/// cut would split is dropped whole. Returns what was dropped.
///
/// A sequence still unterminated at the cut is over 256 KiB long by then (the
/// cut sits that far from the end); it is dropped with everything after it,
/// so a runaway string empties the scrollback rather than stopping the trim.
pub fn trim_front(s: &mut String, cap: usize) -> String {
    if s.len() <= cap {
        return String::new();
    }
    let mut cut = s.len() - cap;
    while !s.is_char_boundary(cut) {
        cut += 1;
    }
    if let Some(start) = s.get(..cut).and_then(|head| head.rfind(is_introducer)) {
        cut = cut.max(sequence_end(s, start).unwrap_or(s.len()));
    }
    s.drain(..cut).collect()
}

/// Record the tracked mode switches in `text`: `CSI ? n h` / `CSI ? n l` (CSI
/// as `ESC [` or U+009B) and the keypad's `ESC =` / `ESC >`.
fn track_modes(text: &str, modes: &mut Modes) {
    let mut rest = text;
    while let Some(at) = rest.find(['\x1b', '\u{9b}']) {
        let after = &rest[at..];
        let Some(params) = after
            .strip_prefix("\x1b[?")
            .or_else(|| after.strip_prefix("\u{9b}?"))
        else {
            let tail = after.get(1..).unwrap_or_default();
            match tail.chars().next() {
                Some('=') if after.starts_with('\x1b') => modes.keypad_application = Some(true),
                Some('>') if after.starts_with('\x1b') => modes.keypad_application = Some(false),
                _ => {}
            }
            rest = after.trim_start_matches(['\x1b', '\u{9b}']);
            continue;
        };
        let end = params
            .find(|c: char| !(c.is_ascii_digit() || c == ';'))
            .unwrap_or(params.len());
        let (list, tail) = params.split_at(end);
        let on = match tail.chars().next() {
            Some('h') => Some(true),
            Some('l') => Some(false),
            _ => None,
        };
        if let Some(on) = on {
            for mode in list
                .split(';')
                .filter_map(|p| p.parse::<u16>().ok())
                .filter(|m| TRACKED_MODES.contains(m))
            {
                modes.private.insert(mode, on);
            }
        }
        rest = tail;
    }
}

/// The switches that put a fresh terminal into `modes` (only those away from
/// a fresh terminal's defaults).
fn mode_prefix(modes: &Modes) -> String {
    let mut out = String::new();
    for (mode, &on) in modes
        .private
        .iter()
        .filter(|(mode, on)| **on != MODES_ON_BY_DEFAULT.contains(mode))
    {
        out.push_str("\x1b[?");
        out.push_str(&mode.to_string());
        out.push(if on { 'h' } else { 'l' });
    }
    if modes.keypad_application == Some(true) {
        out.push_str("\x1b=");
    }
    out
}

/// Terminal output as plain text: escape sequences and carriage returns removed.
#[must_use]
pub fn plain_text(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\x1b' => match chars.next() {
                // CSI: parameters up to a final byte.
                Some('[') => {
                    for n in chars.by_ref() {
                        if ('\x40'..='\x7e').contains(&n) {
                            break;
                        }
                    }
                }
                // OSC: up to BEL or ESC \.
                Some(']') => {
                    while let Some(n) = chars.next() {
                        if n == '\x07' || (n == '\x1b' && chars.next_if_eq(&'\\').is_some()) {
                            break;
                        }
                    }
                }
                _ => {}
            },
            '\r' => {}
            c if c == '\n' || c == '\t' || !c.is_control() => out.push(c),
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn collecting() -> (Sink, Arc<Mutex<Vec<TerminalEvent>>>) {
        let events = Arc::new(Mutex::new(Vec::new()));
        let seen = events.clone();
        (Arc::new(move |e| lock(&seen).push(e)), events)
    }

    fn wait_for(what: &str, mut ok: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !ok() {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn command(program: &str, args: &[&str]) -> CommandBuilder {
        let mut cmd = CommandBuilder::new(program);
        cmd.args(args);
        cmd
    }

    #[test]
    fn decodes_utf8_across_reads() {
        let euro = "€".as_bytes();
        let mut bytes = vec![b'a', euro[0], euro[1]];
        assert_eq!(decode_utf8(&mut bytes), "a");
        assert_eq!(bytes, vec![euro[0], euro[1]]);
        bytes.extend_from_slice(&[euro[2], b'b']);
        assert_eq!(decode_utf8(&mut bytes), "€b");
        assert!(bytes.is_empty());
        let mut bad = vec![b'x', 0xff, b'y'];
        assert_eq!(decode_utf8(&mut bad), "x\u{fffd}y");
        assert!(bad.is_empty());
    }

    #[test]
    fn trims_scrollback_on_a_character_boundary() {
        let mut s = "ab€cd".to_string();
        trim_front(&mut s, 4);
        assert_eq!(s, "cd");
        let mut short = "abc".to_string();
        trim_front(&mut short, 4);
        assert_eq!(short, "abc");
    }

    #[test]
    fn streams_output_then_reports_the_exit() {
        let (sink, events) = collecting();
        let terminals = Terminals::new(sink);
        let id = terminals
            .spawn(
                command("/bin/sh", &["-c", "printf 'hello €\\n'; exit 3"]),
                Path::new("/"),
            )
            .expect("spawn");
        wait_for("exit", || terminals.backlog(&id).is_ok_and(|b| !b.running));
        let backlog = terminals.backlog(&id).expect("backlog");
        assert!(backlog.data.contains("hello €"), "got {:?}", backlog.data);
        assert_eq!(backlog.exit_code, Some(3));
        assert!(terminals.running_children().is_empty());
        let events = lock(&events).clone();
        let output: String = events
            .iter()
            .filter_map(|e| match e {
                TerminalEvent::Output(o) => Some(o.data.clone()),
                TerminalEvent::Exit(_) => None,
            })
            .collect();
        assert_eq!(output, backlog.data);
        assert_eq!(
            events.last(),
            Some(&TerminalEvent::Exit(TerminalExit {
                terminal_id: id.clone(),
                exit_code: Some(3)
            }))
        );
        assert_eq!(
            terminals.write(&id, "x").expect_err("exited"),
            "the agent has exited"
        );
    }

    #[test]
    fn runs_in_the_given_folder_with_a_colour_terminal() {
        let (sink, _) = collecting();
        let terminals = Terminals::new(sink);
        let id = terminals
            .spawn(
                command("/bin/sh", &["-c", "pwd; echo $TERM"]),
                Path::new("/usr"),
            )
            .expect("spawn");
        wait_for("exit", || terminals.backlog(&id).is_ok_and(|b| !b.running));
        let data = terminals.backlog(&id).expect("backlog").data;
        assert!(
            data.contains("/usr\r\n") && data.contains("xterm-256color"),
            "got {data:?}"
        );
    }

    #[test]
    fn takes_input_and_resizes() {
        let (sink, _) = collecting();
        let terminals = Terminals::new(sink);
        let id = terminals
            .spawn(command("/bin/cat", &[]), Path::new("/"))
            .expect("spawn");
        let pid = *terminals.running_children().keys().next().expect("running");
        assert!(pid > 0);
        terminals.write(&id, "ping\r").expect("write");
        wait_for("echo", || {
            terminals
                .backlog(&id)
                .is_ok_and(|b| b.data.matches("ping").count() >= 2)
        });
        terminals.resize(&id, 120, 40).expect("resize");
        let b = terminals.backlog(&id).expect("backlog");
        assert_eq!((b.cols, b.rows), (120, 40));
        assert!(b.running && b.seq > 0);
        assert!(terminals.resize(&id, 0, 40).is_err());
        assert!(terminals.write("t-404", "x").is_err());
        // End-of-transmission on an empty line ends cat.
        terminals.write(&id, "\u{4}").expect("eot");
        wait_for("exit", || terminals.backlog(&id).is_ok_and(|b| !b.running));
        assert_eq!(terminals.backlog(&id).expect("backlog").exit_code, Some(0));
    }

    #[test]
    fn keeps_only_the_newest_output() {
        let (sink, _) = collecting();
        let terminals = Terminals::new(sink);
        let script = "i=0; while [ $i -lt 10000 ]; do echo \"line $i of the scrollback test\"; i=$((i+1)); done";
        let id = terminals
            .spawn(command("/bin/sh", &["-c", script]), Path::new("/"))
            .expect("spawn");
        wait_for("exit", || terminals.backlog(&id).is_ok_and(|b| !b.running));
        let data = terminals.backlog(&id).expect("backlog").data;
        assert!(
            data.len() <= SCROLLBACK && data.len() > SCROLLBACK - 64,
            "kept {} bytes",
            data.len()
        );
        assert!(data.contains("line 9999 of"));
        assert!(!data.contains("line 0 of"));
    }

    #[test]
    fn a_login_shell_runs_the_command_in_place() {
        let (sink, _) = collecting();
        let terminals = Terminals::new(sink);
        let id = terminals
            .spawn_shell(Path::new("/"), "/bin/echo hired")
            .expect("spawn");
        wait_for("exit", || terminals.backlog(&id).is_ok_and(|b| !b.running));
        assert!(
            terminals
                .backlog(&id)
                .expect("backlog")
                .data
                .contains("hired")
        );
    }

    #[test]
    fn only_known_login_shells_run_as_login_shells() {
        let mut tcsh = command("/bin/tcsh", &["-l", "-i", "-c", "exec x"]);
        use_known_login_shell(&mut tcsh);
        assert_eq!(tcsh.get_argv()[0], "/bin/zsh");
        let mut bash = command("/opt/homebrew/bin/bash", &["-l", "-i", "-c", "exec x"]);
        use_known_login_shell(&mut bash);
        assert_eq!(bash.get_argv()[0], "/opt/homebrew/bin/bash");
        // Anything that is not a login-shell launch is left alone.
        let mut plain = command("/bin/tcsh", &["-c", "x"]);
        use_known_login_shell(&mut plain);
        assert_eq!(plain.get_argv()[0], "/bin/tcsh");
    }

    #[test]
    fn a_hired_agent_does_not_inherit_the_launchers_session() {
        let (sink, _) = collecting();
        let terminals = Terminals::new(sink);
        let mut cmd = command("/usr/bin/env", &[]);
        cmd.env("CLAUDECODE", "1");
        cmd.env("CLAUDE_CODE_SESSION_ID", "abc");
        cmd.env("TERM_SESSION_ID", "w0t0p0");
        cmd.env_remove("LANG");
        cmd.env("KEEP_ME", "yes");
        let id = terminals.spawn(cmd, Path::new("/")).expect("spawn");
        wait_for("exit", || terminals.backlog(&id).is_ok_and(|b| !b.running));
        let env = terminals.backlog(&id).expect("backlog").data;
        assert!(
            env.contains("KEEP_ME=yes") && env.contains("LANG=en_US.UTF-8"),
            "got {env:?}"
        );
        assert!(
            !env.contains("CLAUDECODE")
                && !env.contains("CLAUDE_CODE_")
                && !env.contains("TERM_SESSION_ID"),
            "got {env:?}"
        );
    }

    #[test]
    fn refuses_a_missing_folder() {
        let (sink, _) = collecting();
        let err = Terminals::new(sink)
            .spawn(command("/bin/pwd", &[]), Path::new("/nonexistent/floor"))
            .expect_err("missing");
        assert_eq!(
            err,
            "/nonexistent/floor is not a folder on this Mac any more"
        );
    }

    #[test]
    fn waits_briefly_for_an_early_exit() {
        let (sink, _) = collecting();
        let terminals = Terminals::new(sink);
        let quick = terminals
            .spawn(command("/bin/sh", &["-c", "exit 127"]), Path::new("/"))
            .expect("spawn");
        assert_eq!(
            terminals.wait_exit(&quick, Duration::from_secs(5)),
            Ok(Some(Some(127)))
        );
        let slow = terminals
            .spawn(command("/bin/cat", &[]), Path::new("/"))
            .expect("spawn");
        assert_eq!(
            terminals.wait_exit(&slow, Duration::from_millis(100)),
            Ok(None)
        );
        terminals.write(&slow, "\u{4}").expect("eot");
        assert!(terminals.wait_exit("t-404", Duration::ZERO).is_err());
    }

    #[test]
    fn a_replay_restores_the_modes_trimmed_away() {
        let mut modes = Modes::default();
        track_modes(
            "\x1b[?1049h\x1b[?1000;1006h\x1b[?25l\x1b[?2004h text \x1b[?2004l\x1b[?7h\x1b[?1h\x1b=",
            &mut modes,
        );
        assert_eq!(
            mode_prefix(&modes),
            "\x1b[?1h\x1b[?25l\x1b[?1000h\x1b[?1006h\x1b[?1049h\x1b="
        );
        track_modes(
            "\x1b[?1049l\x1b[?25h\x1b>\u{9b}?1004h\x1b[?7l\x1b(B",
            &mut modes,
        );
        assert_eq!(
            mode_prefix(&modes),
            "\x1b[?1h\x1b[?7l\x1b[?1000h\x1b[?1004h\x1b[?1006h"
        );
    }

    #[test]
    fn a_trim_drops_any_control_sequence_it_would_split_whole() {
        // (text, cap): the cut lands inside an OSC hyperlink ended by ST, a title
        // ended by BEL, an 8-bit CSI, and a charset escape.
        for (text, kept) in [
            (
                "ab\x1b]8;;https://example.com/a/very/long/path\x1b\\link",
                "link",
            ),
            ("ab\x1b]0;window title\x07rest", "rest"),
            ("ab\u{9b}?1049hrest", "rest"),
            ("ab\x1b(Brest", "rest"),
        ] {
            let mut s = text.to_string();
            trim_front(&mut s, kept.len() + 2);
            assert_eq!(s, kept, "trimming {text:?}");
        }
        // A sequence still open at the cut goes, with everything after it.
        let mut open = "ab\x1b]52;c;aGVsbG8".to_string();
        assert_eq!(trim_front(&mut open, 4), "ab\x1b]52;c;aGVsbG8");
        assert!(open.is_empty());
    }

    #[test]
    fn quotes_for_posix_shells_and_fish() {
        assert_eq!(quote("/bin/zsh", r"it's a \ test"), r"'it'\''s a \ test'");
        assert_eq!(
            quote("/opt/homebrew/bin/fish", r"it's a \ test\"),
            r"'it\'s a \\ test\\'"
        );
    }

    #[test]
    fn trimming_never_cuts_a_control_sequence_in_half() {
        let mut s = "ab\x1b[?1049hcd".to_string();
        // A plain cut at 4 bytes from the end would keep "049hcd".
        let dropped = trim_front(&mut s, 6);
        assert_eq!((dropped.as_str(), s.as_str()), ("ab\x1b[?1049h", "cd"));
    }

    #[test]
    fn plain_text_drops_escapes() {
        let raw = "\x1b]0;title\x07\x1b[1;31mzsh:\x1b[0m command not found: claude\r\n\x1b]2;x\x1b\\done\x1b(";
        assert_eq!(plain_text(raw), "zsh: command not found: claude\ndone");
    }
}
