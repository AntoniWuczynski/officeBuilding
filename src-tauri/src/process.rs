//! Running other programs. Always an argument vector, never a shell string built from data.

use std::ffi::OsString;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// How often `launch` looks whether the program has exited.
const POLL: Duration = Duration::from_millis(50);

/// How long `capture` lets a program run before it is killed.
const CAPTURE_LIMIT: Duration = Duration::from_secs(5);

/// How long `capture` waits for the pipes to end once the program has exited.
const PIPE_GRACE: Duration = Duration::from_millis(500);

/// One program run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    pub program: PathBuf,
    pub args: Vec<OsString>,
}

impl Call {
    pub fn new(
        program: impl Into<PathBuf>,
        args: impl IntoIterator<Item = impl Into<OsString>>,
    ) -> Self {
        Self {
            program: program.into(),
            args: args.into_iter().map(Into::into).collect(),
        }
    }
}

/// What a finished program left behind.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Captured {
    /// None when a signal ended it.
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

pub trait Runner: Send + Sync {
    /// Run to the end and keep what it printed. For programs that finish on their own.
    /// Err: it could not be started.
    fn capture(&self, call: &Call) -> Result<Captured, String>;

    /// Start a program that may leave an app running, with no pipes attached, and wait up
    /// to `within` for it to exit. `Ok(Some(code))`: it exited. `Ok(None)`: still running,
    /// left alone. Err: it could not be started, or a signal ended it.
    fn launch(&self, call: &Call, within: Duration) -> Result<Option<i32>, String>;
}

/// The real thing: `std::process::Command`.
pub struct SystemRunner;

fn not_started(call: &Call, e: &std::io::Error) -> String {
    format!("{} could not be started: {e}", call.program.display())
}

/// Wait for `child` on a short-lived thread, so it is reaped whenever it exits. Never kills it.
fn reap_later(mut child: Child) {
    std::thread::spawn(move || {
        // Only reaping: the exit status of a program left running is of no interest.
        let _ = child.wait();
    });
}

/// A command for `call` that does not pass on the launcher's identity (agent tokens, tmux).
fn command(call: &Call) -> Command {
    let mut cmd = Command::new(&call.program);
    cmd.args(&call.args).stdin(Stdio::null());
    for key in crate::pty::LAUNCHER_IDENTITY {
        cmd.env_remove(key);
    }
    cmd
}

/// Read a pipe to its end on its own thread, so a full pipe never stalls the program.
fn drain(pipe: Option<impl Read + Send + 'static>) -> mpsc::Receiver<String> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            // A pipe that breaks keeps what was read so far.
            let _ = pipe.read_to_end(&mut bytes);
        }
        // The receiver may have given up already.
        let _ = tx.send(String::from_utf8_lossy(&bytes).into_owned());
    });
    rx
}

/// What a pipe held, waiting briefly: a job the program left running may keep it open.
fn drained(rx: &mpsc::Receiver<String>) -> String {
    rx.recv_timeout(PIPE_GRACE).unwrap_or_default()
}

impl Runner for SystemRunner {
    fn capture(&self, call: &Call) -> Result<Captured, String> {
        let mut child = command(call)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| not_started(call, &e))?;
        let stdout = drain(child.stdout.take());
        let stderr = drain(child.stderr.take());
        let deadline = Instant::now() + CAPTURE_LIMIT;
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    return Ok(Captured {
                        code: status.code(),
                        stdout: drained(&stdout),
                        stderr: drained(&stderr),
                    });
                }
                Ok(None) if Instant::now() < deadline => std::thread::sleep(POLL),
                // Out of time, or not watchable: stop it and keep nothing. A background job
                // it started may hold the pipes open, so the readers are left to end alone.
                Ok(None) | Err(_) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Ok(Captured::default());
                }
            }
        }
    }

    fn launch(&self, call: &Call, within: Duration) -> Result<Option<i32>, String> {
        let mut child = command(call)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| not_started(call, &e))?;
        let deadline = Instant::now() + within;
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    return status.code().map(Some).ok_or_else(|| {
                        format!("{} was stopped by a signal", call.program.display())
                    });
                }
                Ok(None) => {}
                Err(e) => {
                    reap_later(child);
                    return Err(format!(
                        "{} could not be watched: {e}",
                        call.program.display()
                    ));
                }
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                reap_later(child);
                return Ok(None);
            }
            std::thread::sleep(left.min(POLL));
        }
    }
}

#[cfg(test)]
type Reply = Box<dyn Fn(&Call) -> Result<Captured, String> + Send + Sync>;

/// A runner for tests: records every call and answers from a closure. No program runs.
#[cfg(test)]
pub struct FakeRunner {
    calls: std::sync::Mutex<Vec<Call>>,
    reply: Reply,
}

#[cfg(test)]
impl FakeRunner {
    pub fn new(reply: impl Fn(&Call) -> Result<Captured, String> + Send + Sync + 'static) -> Self {
        Self {
            calls: std::sync::Mutex::new(Vec::new()),
            reply: Box::new(reply),
        }
    }

    /// Every call so far, in order.
    pub fn calls(&self) -> Vec<Call> {
        crate::sync::lock(&self.calls).clone()
    }

    fn record(&self, call: &Call) -> Result<Captured, String> {
        crate::sync::lock(&self.calls).push(call.clone());
        (self.reply)(call)
    }
}

#[cfg(test)]
impl Runner for FakeRunner {
    fn capture(&self, call: &Call) -> Result<Captured, String> {
        self.record(call)
    }

    /// The reply's `code`: a reply with `code: None` plays "still running".
    fn launch(&self, call: &Call, _within: Duration) -> Result<Option<i32>, String> {
        self.record(call).map(|c| c.code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    const MISSING: &str = "/nonexistent-ob-test/no-such-program";

    #[test]
    fn capture_keeps_an_argument_with_spaces_and_a_quote_whole() {
        let arg = "two words and a \"quote' here";
        let got = SystemRunner
            .capture(&Call::new("/bin/echo", [arg]))
            .expect("echo");
        assert_eq!(got.code, Some(0));
        assert_eq!(got.stdout, format!("{arg}\n"));
        assert_eq!(got.stderr, "");
    }

    #[test]
    fn programs_do_not_see_the_launcher_identity() {
        let call = Call::new("/usr/bin/env", Vec::<OsString>::new());
        let mut cmd = command(&call);
        cmd.env("CLAUDE_CODE_MESSAGING_TOKEN", "secret")
            .env("TMUX", "x");
        for key in crate::pty::LAUNCHER_IDENTITY {
            cmd.env_remove(key);
        }
        let out = cmd.output().expect("env");
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(!text.contains("CLAUDE_CODE_MESSAGING_TOKEN"), "{text}");
        assert!(!text.contains("TMUX="), "{text}");
    }

    #[test]
    fn capture_kills_a_program_that_outlives_the_limit() {
        let start = Instant::now();
        let got = SystemRunner
            .capture(&Call::new("/bin/sh", ["-c", "exec sleep 30"]))
            .expect("sh");
        assert_eq!(got.code, None);
        assert!(start.elapsed() < Duration::from_secs(15));
    }

    #[test]
    fn capture_reports_the_exit_code() {
        let got = SystemRunner
            .capture(&Call::new("/bin/sh", ["-c", "exit 3"]))
            .expect("sh");
        assert_eq!(got.code, Some(3));
    }

    #[test]
    fn a_missing_program_is_an_error_naming_it() {
        let call = Call::new(MISSING, Vec::<OsString>::new());
        let err = SystemRunner.capture(&call).expect_err("capture");
        assert!(
            err.starts_with(&format!("{MISSING} could not be started: ")),
            "{err}"
        );
        let err = SystemRunner
            .launch(&call, Duration::from_millis(100))
            .expect_err("launch");
        assert!(
            err.starts_with(&format!("{MISSING} could not be started: ")),
            "{err}"
        );
    }

    #[test]
    fn launch_reports_the_exit_code_of_a_quick_program() {
        let wait = Duration::from_secs(5);
        let none = Vec::<OsString>::new();
        assert_eq!(
            SystemRunner.launch(&Call::new("/usr/bin/true", none.clone()), wait),
            Ok(Some(0))
        );
        assert_eq!(
            SystemRunner.launch(&Call::new("/usr/bin/false", none), wait),
            Ok(Some(1))
        );
    }

    #[test]
    fn launch_leaves_a_slow_program_running_and_returns_at_the_deadline() {
        let start = Instant::now();
        let got = SystemRunner.launch(&Call::new("/bin/sleep", ["2"]), Duration::from_millis(100));
        assert_eq!(got, Ok(None));
        assert!(
            start.elapsed() < Duration::from_secs(1),
            "{:?}",
            start.elapsed()
        );
    }

    #[test]
    fn fake_runner_records_calls_in_order_and_plays_its_replies() {
        let fake = FakeRunner::new(|call| {
            if call.program.as_path() == Path::new("/a") {
                Ok(Captured {
                    code: Some(0),
                    stdout: "out".into(),
                    stderr: String::new(),
                })
            } else if call.program.as_path() == Path::new("/b") {
                Ok(Captured::default())
            } else {
                Err("refused".into())
            }
        });
        let a = Call::new("/a", ["x"]);
        let b = Call::new("/b", ["y z"]);
        let c = Call::new("/c", Vec::<OsString>::new());
        let wait = Duration::from_secs(1);
        assert_eq!(fake.capture(&a).expect("a").stdout, "out");
        assert_eq!(fake.launch(&a, wait), Ok(Some(0)));
        assert_eq!(fake.launch(&b, wait), Ok(None));
        assert_eq!(fake.capture(&c), Err("refused".into()));
        assert_eq!(fake.launch(&c, wait), Err("refused".into()));
        assert_eq!(fake.calls(), vec![a.clone(), a, b, c.clone(), c]);
    }
}
