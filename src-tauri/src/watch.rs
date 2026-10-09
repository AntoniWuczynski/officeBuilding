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
/// Pushed when anything the Setup panel shows may have changed.
pub const SETUP_EVENT: &str = "office://setup-changed";
/// Tools rewrite their config in bursts (Claude Code saves `~/.claude.json` often).
const SETUP_DEBOUNCE: Duration = Duration::from_secs(1);
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

/// Watch `paths` (files or folders) and push `office://setup-changed` after a
/// burst of changes to any of them. A file is watched through its folder,
/// since tools save by replacing the file; events for its neighbours are dropped.
pub fn start_setup(app: AppHandle, paths: Vec<PathBuf>) -> Result<(), String> {
    let (sender, events) = mpsc::channel::<notify::Result<notify::Event>>();
    let mut notifier = notify::recommended_watcher(sender)
        .map_err(|e| format!("setup watcher failed to start: {e}"))?;
    for path in &paths {
        let (at, mode) = if path.is_dir() {
            (path.as_path(), RecursiveMode::Recursive)
        } else {
            match path.parent() {
                Some(parent) => (parent, RecursiveMode::NonRecursive),
                None => continue,
            }
        };
        if at.exists() {
            notifier
                .watch(at, mode)
                .map_err(|e| format!("cannot watch {}: {e}", at.display()))?;
        }
    }
    let relevant = move |event: &notify::Result<notify::Event>| {
        event.as_ref().is_ok_and(|e| {
            e.paths
                .iter()
                .any(|changed| paths.iter().any(|p| changed.starts_with(p)))
        })
    };
    std::thread::Builder::new()
        .name("setup-watch".into())
        .spawn(move || {
            let _notifier = notifier;
            while let Ok(event) = events.recv() {
                if !relevant(&event) {
                    continue;
                }
                while events.recv_timeout(SETUP_DEBOUNCE).is_ok() {}
                // A closed webview just means nobody is listening right now.
                let _ = app.emit(SETUP_EVENT, ());
            }
        })
        .map_err(|e| format!("could not start the setup watch thread: {e}"))?;
    Ok(())
}
