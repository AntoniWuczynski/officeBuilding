//! The alert handle Tauri manages: it feeds the tracker a scan, then delivers what is due by
//! sound, Dock bounce and system notification. Every decision lives in the pure tracker and
//! plan; this file only does the I/O.
//!
//! Locks: tracker, then Office (inside `desk_states`). Nothing here asks the main thread
//! anything (no `is_focused`): plain commands run there and take the Office lock.

use super::{Look, Plan, Tracker, notice, plan, test_notice};
use crate::office::Office;
use crate::settings::SettingsStore;
use crate::sync::lock;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, UserAttentionType};
use tauri_plugin_notification::NotificationExt;

const SOUND_PLAYER: &str = "/usr/bin/afplay";
const SOUND_FILE: &str = "/System/Library/Sounds/Tink.aiff";
/// Long enough to switch to another app and see the real focus rule at work.
const TEST_DELAY: Duration = Duration::from_secs(3);

pub struct Alerts {
    settings: Arc<SettingsStore>,
    tracker: Mutex<Tracker>,
    /// Follows `WindowEvent::Focused`; starts false.
    focused: AtomicBool,
    /// The desk whose panel the webview has open.
    open_desk: Mutex<Option<String>>,
}

impl Alerts {
    #[must_use]
    pub fn new(settings: Arc<SettingsStore>) -> Self {
        Self {
            settings,
            tracker: Mutex::new(Tracker::default()),
            focused: AtomicBool::new(false),
            open_desk: Mutex::new(None),
        }
    }

    pub fn set_focused(&self, focused: bool) {
        self.focused.store(focused, Ordering::Relaxed);
    }

    pub fn set_open_desk(&self, session_id: Option<String>) {
        *lock(&self.open_desk) = session_id;
    }

    /// Whether the window is focused, and the open desk.
    fn human(&self) -> (bool, Option<String>) {
        (
            self.focused.load(Ordering::Relaxed),
            lock(&self.open_desk).clone(),
        )
    }

    /// Compare the desks with the last look and deliver what is due. Returns how long until
    /// the tracker next needs a look, when something is waiting.
    pub fn check(&self, app: &AppHandle, office: &Office) -> Option<Duration> {
        // Held through the observe, so two threads cannot feed it scans out of order.
        let mut tracker = lock(&self.tracker);
        let desks = office.desk_states();
        let settings = self.settings.get().notifications;
        let (focused, open_desk) = self.human();
        let look = Look {
            settings: &settings,
            focused,
            open_desk: open_desk.as_deref(),
        };
        let now = Instant::now();
        let batch = tracker.observe(&desks, &look, now);
        let wake = tracker.next_wake(now, Duration::from_secs(u64::from(settings.cooldown_secs)));
        drop(tracker);
        if !batch.is_empty() {
            perform(app, &plan(notice(&batch), &settings, focused));
        }
        wake
    }

    /// Deliver a test alert after `TEST_DELAY`, by the methods switched on then.
    pub fn test(self: &Arc<Self>, app: AppHandle) -> Result<(), String> {
        let alerts = Arc::clone(self);
        std::thread::Builder::new()
            .name("alert-test".into())
            .spawn(move || {
                std::thread::sleep(TEST_DELAY);
                let settings = alerts.settings.get().notifications;
                let (focused, _) = alerts.human();
                perform(&app, &plan(test_notice(), &settings, focused));
            })
            .map(drop)
            .map_err(|e| format!("the test alert could not start: {e}"))
    }
}

/// The soft system sound, as an argument vector.
fn sound_command() -> Command {
    let mut command = Command::new(SOUND_PLAYER);
    command.arg(SOUND_FILE);
    command
}

/// Each method is best effort: one that fails must not stop the others.
fn perform(app: &AppHandle, plan: &Plan) {
    if plan.sound
        && let Ok(mut child) = sound_command()
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
    {
        // Reaped so no zombie is left; how it ended changes nothing.
        let _ = std::thread::Builder::new()
            .name("alert-sound".into())
            .spawn(move || {
                let _ = child.wait();
            });
    }
    if plan.bounce
        && let Some(window) = app.get_webview_window("main")
    {
        // A window that cannot bounce the Dock still gets the other methods.
        let _ = window.request_user_attention(Some(UserAttentionType::Informational));
    }
    if let Some(n) = &plan.notice {
        // macOS may be silencing the app; it cannot be told, so there is nothing to do.
        let _ = app
            .notification()
            .builder()
            .title(&n.title)
            .body(&n.body)
            .show();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alerts() -> Alerts {
        let path = std::env::temp_dir().join(format!(
            "ob-alerts-{}-never-written.json",
            std::process::id()
        ));
        Alerts::new(Arc::new(SettingsStore::open(path)))
    }

    #[test]
    fn the_sound_is_afplay_with_one_file_and_no_shell() {
        let command = sound_command();
        assert_eq!(command.get_program(), "/usr/bin/afplay");
        let args: Vec<_> = command.get_args().collect();
        assert_eq!(args, ["/System/Library/Sounds/Tink.aiff"]);
    }

    #[test]
    fn focus_and_the_open_desk_are_read_back() {
        let alerts = alerts();
        assert_eq!(
            alerts.human(),
            (false, None),
            "starts unfocused, nothing open"
        );
        alerts.set_focused(true);
        alerts.set_open_desk(Some("s-1".to_string()));
        assert_eq!(alerts.human(), (true, Some("s-1".to_string())));
        alerts.set_focused(false);
        alerts.set_open_desk(None);
        assert_eq!(alerts.human(), (false, None));
    }
}
