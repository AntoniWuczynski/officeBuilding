//! Codex as a structured desk, over its app-server protocol: `codex app-server`
//! speaking JSON-RPC (without the `"jsonrpc"` field), one message per line on
//! stdio. Sources: the bundle `codex app-server generate-ts` and
//! `generate-json-schema` write (Codex 0.161.0), the protocol page at
//! <https://learn.chatgpt.com/docs/app-server>, and live runs recorded in
//! `CODEX_NOTES.md`.
//!
//! The flow: `initialize`, the `initialized` notification, `thread/start` in
//! the desk's folder (its thread id names the rollout file under
//! `~/.codex/sessions`), then `turn/start` per message, or `turn/steer` while a
//! turn runs. `turn/interrupt` stops a turn. Codex's approval requests arrive
//! as server requests and wait for our response.
//!
//! Threads, no async runtime: one reads stdout and feeds the mapper, one keeps
//! the tail of stderr for the exit message. Writes share a mutex, and the
//! mapper's lock is held while its output is delivered, so events reach the
//! sink in the order they happened.

mod ask;
mod mapper;
#[cfg(test)]
mod tests;
mod wire;

use super::{Driver, Event, Sink, Spawn, Values};
use crate::model::ToolKind;
use crate::{pty, tools};
use mapper::{Mapper, Out};
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::Duration;

/// Stderr lines kept for the message when Codex stops on its own.
const STDERR_TAIL: usize = 5;

struct Shared {
    mapper: Mutex<Mapper>,
    stdin: Mutex<Option<ChildStdin>>,
    child: Mutex<Child>,
    stderr: Mutex<VecDeque<String>>,
    sink: Sink,
    /// The building ended it, so its exit needs no explanation.
    stopped: AtomicBool,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    // A panic while holding a lock leaves plain data behind; keep serving it.
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Shared {
    fn run(&self, f: impl FnOnce(&mut Mapper) -> Vec<Out>) -> Result<(), String> {
        let mut mapper = lock(&self.mapper);
        let out = f(&mut mapper);
        self.deliver(out)
    }

    /// Send lines and emit events in order. Every item is delivered even when
    /// a write fails; the first failure is returned.
    fn deliver(&self, out: Vec<Out>) -> Result<(), String> {
        let mut result = Ok(());
        for item in out {
            match item {
                Out::Event(event) => (self.sink)(event),
                Out::Send(line) => {
                    if let Err(e) = self.write(&line) {
                        result = result.and(Err(e));
                    }
                }
            }
        }
        result
    }

    fn write(&self, line: &str) -> Result<(), String> {
        let mut stdin = lock(&self.stdin);
        let pipe = stdin.as_mut().ok_or("Codex is not running")?;
        pipe.write_all(line.as_bytes())
            .and_then(|()| pipe.write_all(b"\n"))
            .and_then(|()| pipe.flush())
            .map_err(|e| format!("Codex is not taking input: {e}"))
    }

    /// After stdout closes: wait for the process without holding the child's
    /// lock for long, so `stop` can still kill one that lingers.
    fn wait_exit(&self) -> Event {
        let status = loop {
            match lock(&self.child).try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) => {}
                Err(e) => break Err(e),
            }
            thread::sleep(Duration::from_millis(50));
        };
        let code = status
            .as_ref()
            .ok()
            .and_then(std::process::ExitStatus::code);
        let message = if self.stopped.load(Ordering::SeqCst) || code == Some(0) {
            None
        } else {
            let tail: Vec<String> = lock(&self.stderr).iter().cloned().collect();
            Some(match (status, tail.is_empty()) {
                (Err(e), _) => format!("Codex stopped and its exit could not be read: {e}"),
                (Ok(_), true) => "Codex stopped unexpectedly.".to_string(),
                (Ok(_), false) => format!("Codex stopped: {}", tail.join("\n")),
            })
        };
        Event::Exited { code, message }
    }
}

struct CodexDriver {
    shared: Arc<Shared>,
}

impl Driver for CodexDriver {
    fn prompt(&self, text: &str) -> Result<(), String> {
        self.shared.run(|m| m.prompt(text))
    }

    fn answer(&self, approval_id: &str, choice_id: &str, values: &Values) -> Result<(), String> {
        let mut mapper = lock(&self.shared.mapper);
        let out = mapper.answer(approval_id, choice_id, values)?;
        self.shared.deliver(out)
    }

    fn cancel(&self) -> Result<(), String> {
        self.shared.run(Mapper::cancel)
    }

    fn stop(&self) {
        self.shared.stopped.store(true, Ordering::SeqCst);
        lock(&self.shared.stdin).take();
        // Already gone is fine: the reader reports the exit either way.
        let _ = lock(&self.shared.child).kill();
    }
}

impl Drop for CodexDriver {
    fn drop(&mut self) {
        self.stop();
    }
}

/// The login shell command that becomes `codex app-server`, with the setup's
/// environment and options. Through the login shell, as the terminal desks
/// are, so the app finds `codex` on the user's PATH.
fn command(spawn: &Spawn<'_>, shell: &str) -> Result<String, String> {
    let quote = |s: &str| pty::quote(shell, s);
    let binary = tools::binary(ToolKind::Codex).ok_or("Codex has no command to run")?;
    Ok(format!(
        "exec {}{binary} app-server -c {}{}",
        spawn.launch.prefix(&quote),
        quote("check_for_update_on_startup=false"),
        spawn.launch.args(&quote),
    ))
}

pub(super) fn start(spawn: &Spawn<'_>, sink: Sink) -> Result<Box<dyn Driver>, String> {
    let shell = pty::login_shell();
    let mut child = Command::new(&shell)
        .args(["-l", "-c", &command(spawn, &shell)?])
        .current_dir(spawn.cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Codex could not start: {e}"))?;
    let (Some(stdin), Some(stdout), Some(stderr)) =
        (child.stdin.take(), child.stdout.take(), child.stderr.take())
    else {
        let _ = child.kill();
        return Err("Codex started without its pipes".to_string());
    };
    let mapper = Mapper::new(
        &spawn.cwd.to_string_lossy(),
        spawn.model,
        spawn.effort,
        spawn.resume,
    );
    let shared = Arc::new(Shared {
        mapper: Mutex::new(mapper),
        stdin: Mutex::new(Some(stdin)),
        child: Mutex::new(child),
        stderr: Mutex::new(VecDeque::new()),
        sink,
        stopped: AtomicBool::new(false),
    });

    let tail = Arc::clone(&shared);
    thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            let line = pty::plain_text(&line);
            let mut kept = lock(&tail.stderr);
            if kept.len() == STDERR_TAIL {
                kept.pop_front();
            }
            kept.push_back(line);
        }
    });

    let reader = Arc::clone(&shared);
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            // A write failing here means Codex is going away; its exit is reported below.
            let _ = reader.run(|m| m.line(&line));
        }
        lock(&reader.stdin).take();
        let exited = reader.wait_exit();
        (reader.sink)(exited);
    });

    let task = spawn.task;
    if let Err(e) = shared.run(|m| m.begin(task)) {
        lock(&shared.child).kill().ok();
        return Err(e);
    }
    Ok(Box::new(CodexDriver { shared }))
}
