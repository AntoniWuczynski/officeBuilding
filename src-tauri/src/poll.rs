//! A background job on a timer: runs on its own thread, backs off when it fails, waits when
//! told to, and can be woken. Issue #2's connectors use it, and issue #7's pull request watch.

use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

/// How one pass went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Ok,
    /// Something could not be reached: wait longer each time.
    Failed,
    /// Told to wait at least this long. A wake does not cut it short.
    RetryAfter(Duration),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Schedule {
    pub every: Duration,
    /// The longest wait after repeated failures.
    pub max_backoff: Duration,
}

/// The longest a `RetryAfter` is honoured for.
pub const RETRY_AFTER_CAP: Duration = Duration::from_secs(3600);

/// The wait before the next pass. `failures`: passes in a row that failed, this one included.
#[must_use]
pub fn next_wait(schedule: Schedule, failures: u32, outcome: Outcome) -> Duration {
    match outcome {
        Outcome::Ok => schedule.every,
        Outcome::Failed => 1_u32
            .checked_shl(failures)
            .and_then(|factor| schedule.every.checked_mul(factor))
            .map_or(schedule.max_backoff, |wait| wait.min(schedule.max_backoff)),
        Outcome::RetryAfter(wait) => wait.max(schedule.every).min(RETRY_AFTER_CAP),
    }
}

/// The handle of a running poller. Dropping it stops the thread at its next wake-up.
pub struct Poller {
    wake: mpsc::Sender<()>,
}

impl Poller {
    /// Start `job` on a thread named `name`. The first pass runs at once.
    pub fn start(
        name: &str,
        schedule: Schedule,
        mut job: impl FnMut() -> Outcome + Send + 'static,
    ) -> Result<Self, String> {
        let (wake, woken) = mpsc::channel::<()>();
        std::thread::Builder::new()
            .name(name.to_string())
            .spawn(move || {
                let mut failures = 0_u32;
                let mut wait = Duration::ZERO;
                // The last pass was told to wait.
                let mut held = false;
                loop {
                    let deadline = Instant::now() + wait;
                    loop {
                        let left = deadline.saturating_duration_since(Instant::now());
                        match woken.recv_timeout(left) {
                            // Keep waiting out the hold.
                            Ok(()) if held => {}
                            Ok(()) | Err(RecvTimeoutError::Timeout) => break,
                            Err(RecvTimeoutError::Disconnected) => return,
                        }
                    }
                    // One pass for a burst of wakes.
                    while woken.try_recv().is_ok() {}
                    let outcome = job();
                    failures = if outcome == Outcome::Ok {
                        0
                    } else {
                        failures.saturating_add(1)
                    };
                    held = matches!(outcome, Outcome::RetryAfter(_));
                    wait = next_wait(schedule, failures, outcome);
                }
            })
            .map_err(|e| format!("the {name} thread could not start ({e})"))?;
        Ok(Self { wake })
    }

    /// Run a pass now, unless the last pass was told to wait.
    pub fn wake(&self) {
        // A closed channel means the thread is gone: nothing to wake.
        let _ = self.wake.send(());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GENEROUS: Duration = Duration::from_secs(5);

    fn schedule(every_ms: u64, max_ms: u64) -> Schedule {
        Schedule {
            every: Duration::from_millis(every_ms),
            max_backoff: Duration::from_millis(max_ms),
        }
    }

    #[test]
    fn ok_waits_the_interval() {
        assert_eq!(
            next_wait(schedule(60, 300), 0, Outcome::Ok),
            Duration::from_millis(60)
        );
    }

    #[test]
    fn failures_double_the_wait_up_to_the_cap() {
        let s = schedule(60, 300);
        assert_eq!(next_wait(s, 1, Outcome::Failed), Duration::from_millis(120));
        assert_eq!(next_wait(s, 2, Outcome::Failed), Duration::from_millis(240));
        assert_eq!(next_wait(s, 3, Outcome::Failed), Duration::from_millis(300));
        assert_eq!(
            next_wait(s, 31, Outcome::Failed),
            Duration::from_millis(300)
        );
        assert_eq!(
            next_wait(s, u32::MAX, Outcome::Failed),
            Duration::from_millis(300)
        );
        let huge = Schedule {
            every: Duration::MAX,
            max_backoff: Duration::from_secs(1),
        };
        assert_eq!(next_wait(huge, 5, Outcome::Failed), Duration::from_secs(1));
    }

    #[test]
    fn retry_after_is_at_least_the_interval_and_at_most_an_hour() {
        let s = Schedule {
            every: Duration::from_secs(60),
            max_backoff: Duration::from_secs(300),
        };
        let wait = |secs| next_wait(s, 1, Outcome::RetryAfter(Duration::from_secs(secs)));
        assert_eq!(wait(10), Duration::from_secs(60));
        assert_eq!(wait(90), Duration::from_secs(90));
        assert_eq!(wait(7200), RETRY_AFTER_CAP);
    }

    #[test]
    fn first_pass_runs_at_once_and_passes_repeat() {
        let (tx, rx) = mpsc::channel();
        let _poller = Poller::start("test-poll-repeat", schedule(20, 100), move || {
            let _ = tx.send(());
            Outcome::Ok
        })
        .unwrap();
        let start = Instant::now();
        for _ in 0..3 {
            rx.recv_timeout(Duration::from_secs(2)).unwrap();
        }
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn a_wake_runs_a_pass_now() {
        let (tx, rx) = mpsc::channel();
        let poller = Poller::start(
            "test-poll-wake",
            schedule(3_600_000, 3_600_000),
            move || {
                let _ = tx.send(());
                Outcome::Ok
            },
        )
        .unwrap();
        rx.recv_timeout(GENEROUS).unwrap();
        poller.wake();
        rx.recv_timeout(GENEROUS).unwrap();
    }

    #[test]
    fn a_wake_does_not_cut_a_hold_short() {
        let hold = Duration::from_millis(300);
        let (tx, rx) = mpsc::channel();
        let mut first = true;
        let poller = Poller::start("test-poll-hold", schedule(10, 100), move || {
            let _ = tx.send(Instant::now());
            if std::mem::take(&mut first) {
                Outcome::RetryAfter(hold)
            } else {
                Outcome::Ok
            }
        })
        .unwrap();
        let started = rx.recv_timeout(GENEROUS).unwrap();
        poller.wake();
        let next = rx.recv_timeout(GENEROUS).unwrap();
        assert!(
            next.duration_since(started) >= hold,
            "the next pass came {:?} after the first",
            next.duration_since(started)
        );
    }

    #[test]
    fn dropping_the_poller_ends_the_thread() {
        let (tx, rx) = mpsc::channel::<()>();
        let poller = Poller::start(
            "test-poll-drop",
            schedule(3_600_000, 3_600_000),
            move || {
                let _ = tx.send(());
                Outcome::Ok
            },
        )
        .unwrap();
        rx.recv_timeout(GENEROUS).unwrap();
        drop(poller);
        assert_eq!(
            rx.recv_timeout(GENEROUS),
            Err(RecvTimeoutError::Disconnected)
        );
    }
}
