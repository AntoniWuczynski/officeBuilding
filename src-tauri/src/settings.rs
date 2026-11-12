//! The app's settings: typed groups kept in `settings.json` beside `store.json`.
//! The webview is the only writer; Rust readers take the plain copy from `get`.

use crate::store::write_json_atomic;
use crate::sync::lock;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

pub const COOLDOWN_MAX_SECS: u32 = 600;

/// Everything in Settings. One field per group; a missing group or field loads as its default.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub notifications: NotificationSettings,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each switch is its own wire field the Settings panel toggles"
)]
pub struct NotificationSettings {
    /// Events: an agent finished its turn, needs you, is stuck on an error.
    pub finished: bool,
    pub needs_you: bool,
    pub stuck: bool,
    /// Methods: a soft system sound, a Dock bounce, a system notification.
    pub sound: bool,
    pub dock_bounce: bool,
    pub system_notification: bool,
    /// At most one alert every this many seconds. 0 means no cooldown.
    pub cooldown_secs: u32,
}

impl Default for NotificationSettings {
    fn default() -> Self {
        Self {
            finished: true,
            needs_you: true,
            stuck: true,
            sound: true,
            dock_bounce: true,
            system_notification: true,
            cooldown_secs: 10,
        }
    }
}

/// A change from the Settings panel: each group present replaces that group whole.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SettingsPatch {
    #[serde(default)]
    pub notifications: Option<NotificationSettings>,
}

impl Settings {
    /// Apply a change, or say why it is not allowed. Nothing changes on an error.
    ///
    /// # Errors
    /// When a group in the patch holds a value out of range.
    pub fn apply(&mut self, patch: SettingsPatch) -> Result<(), String> {
        if let Some(notifications) = patch.notifications {
            if notifications.cooldown_secs > COOLDOWN_MAX_SECS {
                return Err(format!(
                    "the cooldown must be between 0 and {COOLDOWN_MAX_SECS} seconds"
                ));
            }
            self.notifications = notifications;
        }
        Ok(())
    }
}

pub struct SettingsStore {
    path: PathBuf,
    current: Mutex<Settings>,
}

impl SettingsStore {
    /// Load the settings, or start on the defaults. Never fails: a file that does not
    /// parse or read is moved aside to `settings.invalid.json`.
    #[must_use]
    pub fn open(path: PathBuf) -> Self {
        let aside = || {
            // Best effort: the copy keeps what was there, the defaults carry on.
            let _ = fs::rename(&path, path.with_file_name("settings.invalid.json"));
            Settings::default()
        };
        let mut settings = match fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|_| aside()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Settings::default(),
            Err(_) => aside(),
        };
        settings.notifications.cooldown_secs =
            settings.notifications.cooldown_secs.min(COOLDOWN_MAX_SECS);
        Self {
            path,
            current: Mutex::new(settings),
        }
    }

    #[must_use]
    pub fn get(&self) -> Settings {
        lock(&self.current).clone()
    }

    /// Apply a change and save it. On any error the settings in memory and on disk are unchanged.
    ///
    /// # Errors
    /// When the change is not allowed or the file cannot be written.
    pub fn update(&self, patch: SettingsPatch) -> Result<Settings, String> {
        let mut current = lock(&self.current);
        let mut next = current.clone();
        next.apply(patch)?;
        write_json_atomic(&self.path, &next)?;
        *current = next.clone();
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!("ob-settings-{}-{name}", std::process::id()))
            .join("settings.json")
    }

    fn with_file(name: &str, text: &str) -> PathBuf {
        let path = temp_path(name);
        fs::create_dir_all(path.parent().expect("dir")).expect("mkdir");
        fs::write(&path, text).expect("write");
        path
    }

    fn cleanup(path: &std::path::Path) {
        fs::remove_dir_all(path.parent().expect("dir")).expect("cleanup");
    }

    fn cooldown_patch(secs: u32) -> SettingsPatch {
        SettingsPatch {
            notifications: Some(NotificationSettings {
                cooldown_secs: secs,
                ..NotificationSettings::default()
            }),
        }
    }

    #[test]
    fn missing_file_gives_defaults_and_writes_nothing() {
        let path = temp_path("missing");
        let store = SettingsStore::open(path.clone());
        assert_eq!(store.get(), Settings::default());
        assert!(!path.exists());
    }

    #[test]
    fn partial_files_fill_in_defaults() {
        for text in ["{}", r#"{"notifications":{"sound":false}}"#] {
            let path = with_file("partial", text);
            let got = SettingsStore::open(path.clone()).get();
            let want = NotificationSettings {
                sound: !text.contains("false"),
                ..NotificationSettings::default()
            };
            assert_eq!(got.notifications, want, "{text}");
            cleanup(&path);
        }
    }

    #[test]
    fn unknown_groups_and_fields_load() {
        let path = with_file(
            "unknown",
            r#"{"future":{"a":1},"notifications":{"stuck":false,"extra":3}}"#,
        );
        let got = SettingsStore::open(path.clone()).get();
        assert!(!got.notifications.stuck);
        assert!(got.notifications.finished);
        cleanup(&path);
    }

    #[test]
    fn invalid_file_is_moved_aside() {
        let path = with_file("invalid", "{not json");
        let store = SettingsStore::open(path.clone());
        assert_eq!(store.get(), Settings::default());
        assert!(!path.exists());
        let aside = path.with_file_name("settings.invalid.json");
        assert_eq!(fs::read_to_string(aside).expect("aside"), "{not json");
        cleanup(&path);
    }

    #[test]
    fn unreadable_file_is_moved_aside() {
        let path = with_file("unreadable", "");
        fs::write(&path, [0xff, 0xfe]).expect("write");
        let store = SettingsStore::open(path.clone());
        assert_eq!(store.get(), Settings::default());
        assert!(!path.exists());
        assert!(path.with_file_name("settings.invalid.json").exists());
        cleanup(&path);
    }

    #[test]
    fn huge_cooldown_in_file_is_clamped() {
        let path = with_file("clamp", r#"{"notifications":{"cooldownSecs":5000}}"#);
        let got = SettingsStore::open(path.clone()).get();
        assert_eq!(got.notifications.cooldown_secs, 600);
        cleanup(&path);
    }

    #[test]
    fn update_saves_returns_and_reloads() {
        let path = temp_path("update");
        let store = SettingsStore::open(path.clone());
        let change = SettingsPatch {
            notifications: Some(NotificationSettings {
                sound: false,
                cooldown_secs: 30,
                ..NotificationSettings::default()
            }),
        };
        let saved = store.update(change).expect("update");
        assert!(!saved.notifications.sound);
        assert_eq!(store.get(), saved);
        assert_eq!(SettingsStore::open(path.clone()).get(), saved);
        cleanup(&path);
    }

    #[test]
    fn cooldown_over_the_maximum_is_refused_and_changes_nothing() {
        let path = temp_path("refuse");
        let store = SettingsStore::open(path.clone());
        let err = store.update(cooldown_patch(601)).expect_err("refused");
        assert_eq!(err, "the cooldown must be between 0 and 600 seconds");
        assert_eq!(store.get(), Settings::default());
        assert!(!path.exists());
        assert!(store.update(cooldown_patch(600)).is_ok());
        let before = fs::read_to_string(&path).expect("file");
        assert!(store.update(cooldown_patch(601)).is_err());
        assert_eq!(fs::read_to_string(&path).expect("file"), before);
        assert_eq!(store.get().notifications.cooldown_secs, 600);
        cleanup(&path);
    }

    #[test]
    fn empty_patch_changes_nothing() {
        let path = temp_path("empty");
        let store = SettingsStore::open(path.clone());
        assert_eq!(
            store.update(SettingsPatch::default()),
            Ok(Settings::default())
        );
        cleanup(&path);
    }

    #[test]
    fn patch_with_an_unknown_key_is_rejected() {
        assert!(serde_json::from_str::<SettingsPatch>(r#"{"bogus":{}}"#).is_err());
        assert!(serde_json::from_str::<SettingsPatch>("{}").is_ok());
    }

    #[test]
    fn defaults_serialise_to_the_wire_json() {
        let want: serde_json::Value = serde_json::from_str(
            r#"{ "notifications": { "finished": true, "needsYou": true, "stuck": true,
            "sound": true, "dockBounce": true, "systemNotification": true,
            "cooldownSecs": 10 } }"#,
        )
        .expect("json");
        assert_eq!(
            serde_json::to_value(Settings::default()).expect("value"),
            want
        );
    }
}
