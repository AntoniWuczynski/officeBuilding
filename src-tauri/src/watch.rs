//! Keeps the office live: file events on the tools' session stores trigger a
//! re-scan (debounced), and a slow tick catches processes that exit without
//! touching a file. Changes are pushed to the webview as `office://snapshot`.

use crate::office::Office;
use notify::{RecursiveMode, Watcher};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

pub const SNAPSHOT_EVENT: &str = "office://snapshot";
const DEBOUNCE: Duration = Duration::from_millis(400);
const TICK: Duration = Duration::from_secs(5);

pub fn start(app: AppHandle, office: Arc<Office>, watched: &[PathBuf]) -> Result<(), String> {
    let (sender, events) = mpsc::channel();
    let mut notifier = notify::recommended_watcher(sender)
        .map_err(|e| format!("file watcher failed to start: {e}"))?;
    for path in watched.iter().filter(|p| p.exists()) {
        notifier
            .watch(path, RecursiveMode::Recursive)
            .map_err(|e| format!("cannot watch {}: {e}", path.display()))?;
    }
    std::thread::Builder::new()
        .name("office-watch".into())
        .spawn(move || {
            // Owning the notifier here keeps it alive for the life of the thread.
            let _notifier = notifier;
            loop {
                match events.recv_timeout(TICK) {
                    Ok(_) => while events.recv_timeout(DEBOUNCE).is_ok() {},
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => return,
                }
                if office.refresh() {
                    // A closed webview just means nobody is listening right now.
                    let _ = app.emit(SNAPSHOT_EVENT, office.snapshot());
                }
            }
        })
        .map_err(|e| format!("could not start the watch thread: {e}"))?;
    Ok(())
}
