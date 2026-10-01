#![expect(clippy::unwrap_used, reason = "test helpers outside #[test] functions")]
//! Adversarial tests for OB-005 (in-app PTY terminals).

use office_building_lib::model::TerminalOutput;
use office_building_lib::pty::{TerminalEvent, Terminals, decode_utf8, trim_front};
use portable_pty::CommandBuilder;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

type Events = Arc<Mutex<Vec<TerminalEvent>>>;

fn terminals() -> (Terminals, Events) {
    let events: Events = Arc::new(Mutex::new(Vec::new()));
    let seen = events.clone();
    (
        Terminals::new(Arc::new(move |e| seen.lock().unwrap().push(e))),
        events,
    )
}

fn command(program: &str, args: &[&str]) -> CommandBuilder {
    let mut cmd = CommandBuilder::new(program);
    cmd.args(args);
    cmd
}

fn wait_exit(t: &Terminals, id: &str) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while t.backlog(id).is_ok_and(|b| b.running) {
        assert!(Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(20));
    }
}

const SAMPLES: &[&str] = &[
    "Zażółć gęślą jaźń",
    "🐙👩‍💻🇵🇱",
    "漢字かなカナ한국어",
    "a€b\u{10FFFF}c",
    "e\u{301}",
];

#[test]
fn utf8_split_at_every_boundary_regression() {
    for s in SAMPLES {
        let bytes = s.as_bytes();
        for cut in 0..=bytes.len() {
            let mut carry = bytes[..cut].to_vec();
            let mut out = decode_utf8(&mut carry);
            carry.extend_from_slice(&bytes[cut..]);
            out.push_str(&decode_utf8(&mut carry));
            assert_eq!(&out, s, "split at {cut}");
            assert!(carry.is_empty());
        }
        // Byte by byte.
        let mut carry = Vec::new();
        let mut out = String::new();
        for b in bytes {
            carry.push(*b);
            out.push_str(&decode_utf8(&mut carry));
        }
        assert_eq!(&out, s);
    }
    // Invalid input never stalls and never loses the valid tail.
    let mut bad = vec![0xE2, 0x82, b'a', 0xC0, 0xAF, b'b', 0xF5, b'c'];
    assert_eq!(decode_utf8(&mut bad), "\u{FFFD}a\u{FFFD}\u{FFFD}b\u{FFFD}c");
    assert!(bad.is_empty());
}

#[test]
fn trim_front_never_splits_a_character_regression() {
    let s: String = "ż🐙漢".repeat(50);
    for cap in 0..s.len() + 2 {
        let mut t = s.clone();
        trim_front(&mut t, cap);
        assert!(t.len() <= cap);
        assert!(s.ends_with(&t));
    }
}

#[test]
fn split_character_across_pty_writes_regression() {
    let (t, events) = terminals();
    // "ż" is C5 BC; the two bytes arrive in separate reads.
    let id = t
        .spawn(
            command(
                "/bin/sh",
                &["-c", "printf '\\305'; sleep 0.3; printf '\\274 🐙\\n'"],
            ),
            Path::new("/"),
        )
        .unwrap();
    wait_exit(&t, &id);
    let b = t.backlog(&id).unwrap();
    assert!(b.data.contains("ż 🐙"), "got {:?}", b.data);
    assert!(!b.data.contains('\u{FFFD}'));
    let seqs: Vec<u64> = events
        .lock()
        .unwrap()
        .iter()
        .filter_map(|e| match e {
            TerminalEvent::Output(TerminalOutput { seq, .. }) => Some(*seq),
            TerminalEvent::Exit(_) => None,
        })
        .collect();
    assert_eq!(
        seqs,
        (1..=u64::try_from(seqs.len()).unwrap()).collect::<Vec<_>>()
    );
}

/// FAILS: portable-pty 0.9 silently replaces a cwd that is not a directory with
/// $HOME (cmdbuilder.rs `as_command`: `.filter(|dir| is_dir()).unwrap_or(home)`),
/// so hiring onto a floor whose folder was moved/renamed starts the agent in the
/// home folder instead of failing. The old `cd <dir> && ...` failed visibly.
#[test]
fn verifier_missing_floor_folder_does_not_start_agent_in_home() {
    let (t, _) = terminals();
    let result = t.spawn(
        command("/bin/sh", &["-c", "pwd"]),
        Path::new("/nonexistent/ob005-floor"),
    );
    let Ok(id) = result else { return };
    wait_exit(&t, &id);
    let data = t.backlog(&id).unwrap().data;
    let home = std::env::var("HOME").unwrap();
    assert!(
        !data.contains(&home),
        "agent started in {data:?} instead of failing"
    );
}

/// FAILS: `spawn_shell` always passes `-l -i -c`; tcsh/csh (a valid macOS login
/// shell, /bin/tcsh) rejects `-l` unless it is the only flag, so the agent never
/// starts and the error goes to a terminal nobody can open. Mirrors `spawn_shell`'s argv exactly.
#[test]
fn verifier_tcsh_login_shell_runs_the_command() {
    let (t, _) = terminals();
    let id = t
        .spawn(
            command("/bin/tcsh", &["-l", "-i", "-c", "exec /bin/echo hired"]),
            Path::new("/"),
        )
        .unwrap();
    wait_exit(&t, &id);
    let b = t.backlog(&id).unwrap();
    assert!(
        b.data.contains("hired"),
        "exit {:?}, output {:?}",
        b.exit_code,
        b.data
    );
}

/// FAILS if a write to an agent that is not reading blocks. `terminal_write` is a
/// sync Tauri command (runs on the main thread), so a blocked write freezes the UI.
#[test]
fn verifier_large_write_to_busy_agent_does_not_block() {
    let (t, _) = terminals();
    let t = Arc::new(t);
    let id = t
        .spawn(
            command("/bin/sh", &["-c", "stty raw -echo; sleep 4"]),
            Path::new("/"),
        )
        .unwrap();
    std::thread::sleep(Duration::from_millis(300));
    let (tx, rx) = std::sync::mpsc::channel();
    let (t2, id2) = (t.clone(), id.clone());
    std::thread::spawn(move || {
        let paste = "x".repeat(64 * 1024);
        let _ = tx.send(t2.write(&id2, &paste));
    });
    let answered = rx.recv_timeout(Duration::from_secs(2));
    wait_exit(&t, &id);
    assert!(
        answered.is_ok(),
        "a 64 KiB paste blocked terminal_write for over 2 s while the agent was busy"
    );
}

#[test]
fn shell_quoting_survives_hostile_titles_regression() {
    let (t, _) = terminals();
    let title = "it's \"$HOME\" `id` $(id) \\n !x ; rm -rf / # Zażółć 🐙 漢字";
    let quoted = format!("'{}'", title.replace('\'', r"'\''"));
    let id = t
        .spawn_shell(Path::new("/"), &format!("/usr/bin/printf '[%s]' {quoted}"))
        .unwrap();
    wait_exit(&t, &id);
    let data = t.backlog(&id).unwrap().data;
    assert!(data.contains(&format!("[{title}]")), "got {data:?}");
}

/// FAILS: the scrollback is trimmed by bytes with no memory of terminal modes, so
/// once an agent has printed more than 256 KiB, a panel opened (or reopened) later
/// replays into a fresh xterm that never saw the startup mode switches (here the
/// alternate screen and mouse tracking; bracketed paste is the same). The replayed
/// terminal then renders on the wrong screen and stops reporting mouse/paste framing.
#[test]
fn verifier_replay_after_trim_keeps_terminal_modes() {
    let (t, _) = terminals();
    let script = "printf '\\033[?1049h\\033[?1000h'; i=0; while [ $i -lt 9000 ]; do echo \"line $i of a long agent run\"; i=$((i+1)); done";
    let id = t
        .spawn(command("/bin/sh", &["-c", script]), Path::new("/"))
        .unwrap();
    wait_exit(&t, &id);
    let data = t.backlog(&id).unwrap().data;
    assert!(
        data.contains("\u{1b}[?1049h") && data.contains("\u{1b}[?1000h"),
        "modes lost from a {}-byte replay",
        data.len()
    );
}

#[test]
fn backlog_matches_events_up_to_its_seq_and_exit_is_last_and_once_regression() {
    for _ in 0..5 {
        let (t, events) = terminals();
        let script = "i=0; while [ $i -lt 1500 ]; do echo \"ż🐙漢 $i\"; i=$((i+1)); done; exit 7";
        let id = t
            .spawn(command("/bin/sh", &["-c", script]), Path::new("/"))
            .unwrap();
        let mut checks = 0;
        while t.backlog(&id).unwrap().running {
            let b = t.backlog(&id).unwrap();
            let evs = events.lock().unwrap().clone();
            let upto: String = evs
                .iter()
                .filter_map(|e| match e {
                    TerminalEvent::Output(o) if o.seq <= b.seq => Some(o.data.clone()),
                    _ => None,
                })
                .collect();
            assert_eq!(
                upto, b.data,
                "backlog at seq {} disagrees with events",
                b.seq
            );
            checks += 1;
        }
        assert!(checks > 0);
        let evs = events.lock().unwrap().clone();
        let exits = evs
            .iter()
            .filter(|e| matches!(e, TerminalEvent::Exit(_)))
            .count();
        assert_eq!(exits, 1);
        assert!(matches!(evs.last(), Some(TerminalEvent::Exit(x)) if x.exit_code == Some(7)));
        assert!(!t.backlog(&id).unwrap().data.contains('\u{FFFD}'));
    }
}

#[test]
fn edge_inputs_are_rejected_or_harmless_regression() {
    let (t, _) = terminals();
    let id = t.spawn(command("/bin/cat", &[]), Path::new("/")).unwrap();
    assert!(t.resize(&id, u16::MAX, u16::MAX).is_ok());
    assert!(t.resize(&id, 0, 0).is_err());
    assert!(t.resize("t-999", 80, 24).is_err());
    assert!(t.backlog("").is_err());
    t.write(&id, "Zażółć 🐙\r").unwrap();
    t.write(&id, "\u{4}").unwrap();
    wait_exit(&t, &id);
    assert!(t.write(&id, "late").is_err());
    assert!(t.resize(&id, 80, 24).is_ok());
}

#[test]
fn exited_agent_is_reaped_regression() {
    let (t, _) = terminals();
    let id = t
        .spawn(command("/bin/sh", &["-c", "sleep 0.3"]), Path::new("/"))
        .unwrap();
    let pid = t
        .running_children()
        .into_iter()
        .find(|(_, v)| *v == id)
        .map(|(p, _)| p)
        .expect("running");
    wait_exit(&t, &id);
    let out = std::process::Command::new("ps")
        .args(["-o", "stat=", "-p", &pid.to_string()])
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&out.stdout).trim().is_empty(),
        "pid {pid} still listed"
    );
}
