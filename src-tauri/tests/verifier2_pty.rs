#![expect(clippy::unwrap_used, reason = "test helpers outside #[test] functions")]
//! Round-2 adversarial tests for OB-005 (in-app PTY terminals).

use office_building_lib::pty::{Terminals, trim_front};
use portable_pty::CommandBuilder;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn terminals() -> Terminals {
    Terminals::new(Arc::new(|_| {}))
}

fn command(program: &str, args: &[&str]) -> CommandBuilder {
    let mut cmd = CommandBuilder::new(program);
    cmd.args(args);
    cmd
}

fn run(t: &Terminals, cmd: CommandBuilder) -> String {
    let id = t.spawn(cmd, Path::new("/")).unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    while t.backlog(&id).unwrap().running {
        assert!(Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(20));
    }
    t.backlog(&id).unwrap().data
}

fn flood_after(prefix: &str) -> CommandBuilder {
    let script = format!(
        "printf '{prefix}'; i=0; while [ $i -lt 9000 ]; do echo \"line $i of a long agent run\"; i=$((i+1)); done"
    );
    command("/bin/sh", &["-c", &script])
}

/// FAILS: `trim_front` only looks 32 bytes back for an ESC, so a longer control
/// sequence straddling the cut is split: its tail is kept and replays as literal
/// text, and the modes set in it are lost. Refutes "a trim never cuts a control sequence".
#[test]
fn verifier2_trim_never_cuts_a_long_csi() {
    // 38 bytes: every mouse mode plus bracketed paste in one switch.
    let seq = "\x1b[?1000;1002;1003;1005;1006;1015;2004h";
    let mut s = format!("{}{seq}{}", "a".repeat(100), "b".repeat(100));
    // Cut 36 bytes into the sequence.
    let cap = s.len() - (100 + 36);
    trim_front(&mut s, cap);
    assert!(
        s.starts_with('b') || s.starts_with('\x1b'),
        "replay starts {:?}",
        &s[..8]
    );
}

/// FAILS: an OSC string (hyperlink, window title) is a control sequence too, but
/// the trim only protects CSI, so the replay starts with the tail of a URL.
#[test]
fn verifier2_trim_never_cuts_an_osc_hyperlink() {
    let link = "\x1b]8;;file:///Users/me/code/app/src/main.rs\x1b\\main.rs\x1b]8;;\x1b\\";
    let mut s = format!("{}{link}{}", "a".repeat(100), "b".repeat(100));
    let cap = s.len() - 100 - 20;
    trim_front(&mut s, cap);
    assert!(
        !s.starts_with("/code") && !s.contains("app/src/main.rs\x1b\\main"),
        "replay starts {:?}",
        &s[..40]
    );
}

/// FAILS: application cursor keys (DECCKM, `ESC[?1h`) is not a tracked mode. After
/// a trim, a reopened panel sends normal-mode arrow keys to a program that asked
/// for application mode (vim, less, many TUIs).
#[test]
fn verifier2_application_cursor_mode_survives_a_trim() {
    let data = run(&terminals(), flood_after("\\033[?1h"));
    assert!(
        data.contains("\x1b[?1h"),
        "DECCKM lost from a {}-byte replay",
        data.len()
    );
}

/// FAILS: the C1 form of CSI (U+009B, which xterm.js honours) is not scanned, so an
/// alternate-screen switch written that way is lost after a trim.
#[test]
fn verifier2_c1_csi_mode_survives_a_trim() {
    let data = run(&terminals(), flood_after("\\302\\233?1049h"));
    assert!(
        data.contains("?1049h"),
        "C1 alternate-screen switch lost from a {}-byte replay",
        data.len()
    );
}

/// FAILS: env stripping leaves the launching terminal's identity in place. Run from
/// tmux (how this user starts dev servers), a hired agent inherits `TMUX/TMUX_PANE`, so
/// any `tmux` command it runs targets the user's own pane. It also inherits
/// `ITERM_SESSION_ID/TERM_PROGRAM` (it thinks it is in iTerm/Terminal.app) and `CLAUDE_PID`.
#[test]
fn verifier2_launcher_terminal_identity_is_not_inherited() {
    let mut cmd = command("/usr/bin/env", &[]);
    for (k, v) in [
        ("TMUX", "/private/tmp/tmux-501/default,1234,0"),
        ("TMUX_PANE", "%3"),
        ("ITERM_SESSION_ID", "w0t0p0:ABC"),
        ("TERM_PROGRAM", "Apple_Terminal"),
        ("CLAUDE_PID", "59191"),
    ] {
        cmd.env(k, v);
    }
    let data = run(&terminals(), cmd);
    let leaked: Vec<&str> = [
        "TMUX=",
        "TMUX_PANE=",
        "ITERM_SESSION_ID=",
        "TERM_PROGRAM=",
        "CLAUDE_PID=",
    ]
    .into_iter()
    .filter(|k| data.lines().any(|l| l.starts_with(k)))
    .collect();
    assert!(leaked.is_empty(), "inherited: {leaked:?}");
}

/// FAILS: the `CLAUDE_CODE_*` prefix also strips Claude Code's own documented
/// configuration (Bedrock/Vertex routing, OAuth token, output limits). A value the
/// user set outside their shell profile (launchctl setenv, the launching shell) is
/// silently dropped, so the hired agent authenticates or routes differently.
#[test]
fn verifier2_agent_configuration_vars_are_kept() {
    let mut cmd = command("/usr/bin/env", &[]);
    for (k, v) in [
        ("CLAUDE_CODE_USE_BEDROCK", "1"),
        ("CLAUDE_CODE_OAUTH_TOKEN", "tok"),
        ("CLAUDE_CODE_MAX_OUTPUT_TOKENS", "8000"),
    ] {
        cmd.env(k, v);
    }
    let data = run(&terminals(), cmd);
    let lost: Vec<&str> = [
        "CLAUDE_CODE_USE_BEDROCK=1",
        "CLAUDE_CODE_OAUTH_TOKEN=tok",
        "CLAUDE_CODE_MAX_OUTPUT_TOKENS=8000",
    ]
    .into_iter()
    .filter(|kv| !data.lines().any(|l| l.trim_end() == *kv))
    .collect();
    assert!(lost.is_empty(), "stripped: {lost:?}");
}
