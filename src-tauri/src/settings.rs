//! The app's settings: typed groups kept in `settings.json` beside `store.json`.
//! The webview is the only writer; Rust readers take the plain copy from `get`.

use crate::editor::Editor;
use crate::store::write_json_atomic;
use crate::sync::lock;
use serde::{Deserialize, Serialize};
use std::fs;
use std::ops::RangeInclusive;
use std::path::PathBuf;
use std::sync::Mutex;

mod agents;
pub use agents::AgentSettings;
mod routing;
pub use routing::RoutingSettings;

pub const COOLDOWN_MAX_SECS: u32 = 600;
pub const AGENT_LABEL_MAX: usize = 80;
pub const QA_POLL_MINUTES: RangeInclusive<u32> = 1..=60;
pub const QA_MAX_CONCURRENT: RangeInclusive<u32> = 1..=4;
pub const QA_REVIEWS_PER_DAY: RangeInclusive<u32> = 1..=20;
pub const QA_NAME_MAX: usize = 100;
pub const MANAGER_MAX_DESKS: RangeInclusive<u32> = 1..=10;
pub const MANAGER_SPEND_CEILING_USD: RangeInclusive<u32> = 1..=1000;
const DEFAULT_AGENT_LABEL: &str = "agent";

/// `raw` trimmed, if it is 1 to `AGENT_LABEL_MAX` characters.
#[must_use]
pub fn agent_label(raw: &str) -> Option<String> {
    let raw = raw.trim();
    (1..=AGENT_LABEL_MAX)
        .contains(&raw.chars().count())
        .then(|| raw.to_string())
}

/// Everything in Settings. One field per group; a missing group or field loads as its default.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub notifications: NotificationSettings,
    pub editor: EditorSettings,
    pub connectors: ConnectorSettings,
    pub qa: QaSettings,
    pub manager: ManagerSettings,
    pub agents: AgentSettings,
    #[serde(deserialize_with = "routing::lenient")]
    pub routing: RoutingSettings,
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

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EditorSettings {
    /// The editor "Open in" uses. None: the first one found.
    pub preferred: Option<Editor>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ConnectorSettings {
    /// Linear issues carrying this label go on the whiteboard (TODO), whoever holds them.
    pub linear_agent_label: String,
}

impl Default for ConnectorSettings {
    fn default() -> Self {
        Self {
            linear_agent_label: DEFAULT_AGENT_LABEL.to_string(),
        }
    }
}

/// The tools that can be the QA reviewer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReviewerTool {
    #[default]
    ClaudeCode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each switch is its own wire field the Settings panel toggles"
)]
pub struct QaSettings {
    /// Whether a floor A Human has never switched has QA on.
    pub default_on: bool,
    /// Minutes between checks of GitHub: 1 to 60.
    pub poll_minutes: u32,
    /// Review each new head commit. False: only a pull request's first.
    pub every_push: bool,
    pub skip_drafts: bool,
    /// Post a short summary when nothing was found. False: post nothing.
    pub post_when_clean: bool,
    /// Reviews running at once across the building: 1 to 4.
    pub max_concurrent: u32,
    /// Reviews of one pull request in 24 hours: 1 to 20.
    pub reviews_per_day: u32,
    pub tool: ReviewerTool,
    /// None: the tool's default model.
    pub model: Option<String>,
    /// None: the model's default effort.
    pub effort: Option<String>,
}

impl Default for QaSettings {
    fn default() -> Self {
        Self {
            default_on: false,
            poll_minutes: 5,
            every_push: true,
            skip_drafts: true,
            post_when_clean: true,
            max_concurrent: 1,
            reviews_per_day: 3,
            tool: ReviewerTool::ClaudeCode,
            model: None,
            effort: None,
        }
    }
}

/// `raw` trimmed, if it is empty (None) or a name of at most `QA_NAME_MAX` characters
/// with no control character. Err: it is not a valid name.
fn qa_name(raw: &str) -> Result<Option<String>, ()> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(None);
    }
    let valid = raw.chars().count() <= QA_NAME_MAX && !raw.chars().any(char::is_control);
    if valid {
        Ok(Some(raw.to_string()))
    } else {
        Err(())
    }
}

/// A range in words: "between 1 and 60".
fn between(range: &RangeInclusive<u32>) -> String {
    format!("between {} and {}", range.start(), range.end())
}

/// `model` and `effort` trimmed (empty becomes None), or why one is not valid.
pub(crate) fn checked_names(
    model: Option<String>,
    effort: Option<String>,
) -> Result<(Option<String>, Option<String>), String> {
    let check = |n: Option<String>, what: &str| {
        n.as_deref()
            .map(qa_name)
            .transpose()
            .map(Option::flatten)
            .map_err(|()| format!("that {what} name is not valid"))
    };
    Ok((check(model, "model")?, check(effort, "effort")?))
}

/// A name from a file, or None when it is not valid.
fn clamped_name(name: Option<String>) -> Option<String> {
    name.and_then(|n| qa_name(&n).ok().flatten())
}

fn clamp(v: u32, r: &RangeInclusive<u32>) -> u32 {
    v.clamp(*r.start(), *r.end())
}

impl QaSettings {
    /// The same settings with each name trimmed (empty becomes None), or why one is not valid.
    fn checked(mut self) -> Result<Self, String> {
        (self.model, self.effort) = checked_names(self.model, self.effort)?;
        if !QA_POLL_MINUTES.contains(&self.poll_minutes) {
            return Err(format!(
                "the check interval must be {} minutes",
                between(&QA_POLL_MINUTES)
            ));
        }
        if !QA_MAX_CONCURRENT.contains(&self.max_concurrent) {
            return Err(format!(
                "reviews at once must be {}",
                between(&QA_MAX_CONCURRENT)
            ));
        }
        if !QA_REVIEWS_PER_DAY.contains(&self.reviews_per_day) {
            return Err(format!(
                "reviews a day must be {}",
                between(&QA_REVIEWS_PER_DAY)
            ));
        }
        Ok(self)
    }

    /// Clamp the numbers into range and drop a name that is not valid, as a file may hold any.
    fn clamped(mut self) -> Self {
        self.poll_minutes = clamp(self.poll_minutes, &QA_POLL_MINUTES);
        self.max_concurrent = clamp(self.max_concurrent, &QA_MAX_CONCURRENT);
        self.reviews_per_day = clamp(self.reviews_per_day, &QA_REVIEWS_PER_DAY);
        self.model = clamped_name(self.model);
        self.effort = clamped_name(self.effort);
        self
    }
}

/// The tools that can be a floor's manager.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ManagerTool {
    #[default]
    ClaudeCode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ManagerSettings {
    pub tool: ManagerTool,
    /// None: the tool's default model.
    pub model: Option<String>,
    /// None: the model's default effort.
    pub effort: Option<String>,
    /// Desks a manager may have hired at once: 1 to 10.
    pub max_desks: u32,
    /// Ceiling on the summed spend of the desks a manager hired, in whole US dollars: 1 to 1000.
    pub spend_ceiling_usd: u32,
}

impl Default for ManagerSettings {
    fn default() -> Self {
        Self {
            tool: ManagerTool::ClaudeCode,
            model: None,
            effort: None,
            max_desks: 3,
            spend_ceiling_usd: 20,
        }
    }
}

impl ManagerSettings {
    /// The same settings with each name trimmed (empty becomes None), or why one is not valid.
    fn checked(mut self) -> Result<Self, String> {
        (self.model, self.effort) = checked_names(self.model, self.effort)?;
        if !MANAGER_MAX_DESKS.contains(&self.max_desks) {
            return Err(format!(
                "desks at once must be {}",
                between(&MANAGER_MAX_DESKS)
            ));
        }
        if !MANAGER_SPEND_CEILING_USD.contains(&self.spend_ceiling_usd) {
            return Err(format!(
                "the spend ceiling must be {} dollars",
                between(&MANAGER_SPEND_CEILING_USD)
            ));
        }
        Ok(self)
    }

    /// Clamp the numbers into range and drop a name that is not valid, as a file may hold any.
    fn clamped(mut self) -> Self {
        self.max_desks = clamp(self.max_desks, &MANAGER_MAX_DESKS);
        self.spend_ceiling_usd = clamp(self.spend_ceiling_usd, &MANAGER_SPEND_CEILING_USD);
        self.model = clamped_name(self.model);
        self.effort = clamped_name(self.effort);
        self
    }
}

/// A change from the Settings panel: each group present replaces that group whole.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SettingsPatch {
    #[serde(default)]
    pub notifications: Option<NotificationSettings>,
    #[serde(default)]
    pub editor: Option<EditorSettings>,
    #[serde(default)]
    pub connectors: Option<ConnectorSettings>,
    #[serde(default)]
    pub qa: Option<QaSettings>,
    #[serde(default)]
    pub manager: Option<ManagerSettings>,
    #[serde(default)]
    pub agents: Option<AgentSettings>,
    #[serde(default)]
    pub routing: Option<RoutingSettings>,
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
        if let Some(editor) = patch.editor {
            self.editor = editor;
        }
        if let Some(mut connectors) = patch.connectors {
            connectors.linear_agent_label = agent_label(&connectors.linear_agent_label)
                .ok_or_else(|| {
                    format!("the agent label must be 1 to {AGENT_LABEL_MAX} characters")
                })?;
            self.connectors = connectors;
        }
        if let Some(qa) = patch.qa {
            self.qa = qa.checked()?;
        }
        if let Some(manager) = patch.manager {
            self.manager = manager.checked()?;
        }
        if let Some(agents) = patch.agents {
            self.agents = agents;
        }
        if let Some(routing) = patch.routing {
            self.routing = routing.checked(&self.routing)?;
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
            Ok(text) => {
                if routing::dropped(&text) {
                    // The routing group loads as the default, and the next save would write
                    // that over it. Keep what was there. Best effort.
                    let _ = fs::copy(&path, path.with_file_name("settings.routing.invalid.json"));
                }
                serde_json::from_str(&text).unwrap_or_else(|_| aside())
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Settings::default(),
            Err(_) => aside(),
        };
        settings.notifications.cooldown_secs =
            settings.notifications.cooldown_secs.min(COOLDOWN_MAX_SECS);
        settings.connectors.linear_agent_label =
            agent_label(&settings.connectors.linear_agent_label)
                .unwrap_or_else(|| DEFAULT_AGENT_LABEL.to_string());
        settings.qa = settings.qa.clamped();
        settings.manager = settings.manager.clamped();
        settings.routing = settings.routing.clamped();
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
            ..SettingsPatch::default()
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
            ..SettingsPatch::default()
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
            "cooldownSecs": 10 }, "editor": { "preferred": null },
            "connectors": { "linearAgentLabel": "agent" },
            "qa": { "defaultOn": false, "pollMinutes": 5, "everyPush": true, "skipDrafts": true,
            "postWhenClean": true, "maxConcurrent": 1, "reviewsPerDay": 3,
            "tool": "claude-code", "model": null, "effort": null },
            "manager": { "tool": "claude-code", "model": null, "effort": null,
            "maxDesks": 3, "spendCeilingUsd": 20 },
            "agents": { "view": "terminal" },
            "routing": { "enabled": false, "contexts": [], "connections": [], "tiers": { "fable": [], "opus": [], "sonnet": [], "haiku": [] } } }"#,
        )
        .expect("json");
        assert_eq!(
            serde_json::to_value(Settings::default()).expect("value"),
            want
        );
    }

    #[test]
    fn a_file_without_the_editor_group_loads_with_no_preference() {
        let path = with_file("no-editor", r#"{"notifications":{}}"#);
        assert_eq!(
            SettingsStore::open(path.clone()).get().editor.preferred,
            None
        );
        cleanup(&path);
    }

    #[test]
    fn a_saved_preferred_editor_loads() {
        let path = with_file("zed", r#"{"editor":{"preferred":"zed"}}"#);
        let got = SettingsStore::open(path.clone()).get();
        assert_eq!(got.editor.preferred, Some(Editor::Zed));
        assert_eq!(got.notifications, NotificationSettings::default());
        cleanup(&path);
    }

    #[test]
    fn an_editor_patch_saves_and_reloads() {
        let path = temp_path("editor-patch");
        let store = SettingsStore::open(path.clone());
        let change: SettingsPatch =
            serde_json::from_str(r#"{"editor":{"preferred":"devin-desktop"}}"#).expect("patch");
        let saved = store.update(change).expect("update");
        assert_eq!(saved.editor.preferred, Some(Editor::DevinDesktop));
        assert_eq!(saved.notifications, NotificationSettings::default());
        assert_eq!(SettingsStore::open(path.clone()).get(), saved);
        let cleared: SettingsPatch =
            serde_json::from_str(r#"{"editor":{"preferred":null}}"#).expect("patch");
        assert_eq!(
            store.update(cleared).expect("update").editor.preferred,
            None
        );
        cleanup(&path);
    }

    #[test]
    fn a_patch_naming_an_unknown_editor_is_rejected() {
        assert!(
            serde_json::from_str::<SettingsPatch>(r#"{"editor":{"preferred":"notepad"}}"#).is_err()
        );
    }

    fn label_patch(label: &str) -> SettingsPatch {
        SettingsPatch {
            connectors: Some(ConnectorSettings {
                linear_agent_label: label.to_string(),
            }),
            ..SettingsPatch::default()
        }
    }

    #[test]
    fn a_file_without_the_connectors_group_loads_the_default_label() {
        let path = with_file("no-connectors", r#"{"editor":{}}"#);
        let got = SettingsStore::open(path.clone()).get();
        assert_eq!(got.connectors.linear_agent_label, "agent");
        cleanup(&path);
    }

    #[test]
    fn a_label_is_trimmed_saved_and_reloaded() {
        let path = temp_path("label");
        let store = SettingsStore::open(path.clone());
        let saved = store.update(label_patch("  robot  ")).expect("update");
        assert_eq!(saved.connectors.linear_agent_label, "robot");
        assert_eq!(SettingsStore::open(path.clone()).get(), saved);
        cleanup(&path);
    }

    #[test]
    fn an_empty_or_overlong_label_is_refused_and_changes_nothing() {
        let path = temp_path("label-refuse");
        let store = SettingsStore::open(path.clone());
        store.update(label_patch("robot")).expect("update");
        let before = fs::read_to_string(&path).expect("file");
        for bad in ["", "   ", &"x".repeat(81)] {
            let err = store.update(label_patch(bad)).expect_err("refused");
            assert_eq!(err, "the agent label must be 1 to 80 characters");
        }
        assert_eq!(store.get().connectors.linear_agent_label, "robot");
        assert_eq!(fs::read_to_string(&path).expect("file"), before);
        assert!(store.update(label_patch(&"x".repeat(80))).is_ok());
        cleanup(&path);
    }

    #[test]
    fn a_file_with_a_bad_label_loads_the_default() {
        for label in ["", "  ", &"x".repeat(81)] {
            let text = format!(r#"{{"connectors":{{"linearAgentLabel":"{label}"}}}}"#);
            let path = with_file("label-bad", &text);
            let got = SettingsStore::open(path.clone()).get();
            assert_eq!(got.connectors.linear_agent_label, "agent");
            cleanup(&path);
        }
    }

    type Change = fn(&mut QaSettings);

    fn qa_patch(change: impl FnOnce(&mut QaSettings)) -> SettingsPatch {
        let mut qa = QaSettings::default();
        change(&mut qa);
        SettingsPatch {
            qa: Some(qa),
            ..SettingsPatch::default()
        }
    }

    #[test]
    fn a_file_without_the_qa_group_loads_the_defaults() {
        let path = with_file("no-qa", r#"{"editor":{}}"#);
        assert_eq!(
            SettingsStore::open(path.clone()).get().qa,
            QaSettings::default()
        );
        cleanup(&path);
    }

    #[test]
    fn out_of_range_qa_numbers_are_refused_and_change_nothing() {
        let path = temp_path("qa-refuse");
        let store = SettingsStore::open(path.clone());
        store
            .update(qa_patch(|q| q.default_on = true))
            .expect("update");
        let before = fs::read_to_string(&path).expect("file");
        let bad: [(Change, &str); 6] = [
            (
                |q| q.poll_minutes = 0,
                "the check interval must be between 1 and 60 minutes",
            ),
            (
                |q| q.poll_minutes = 61,
                "the check interval must be between 1 and 60 minutes",
            ),
            (
                |q| q.max_concurrent = 0,
                "reviews at once must be between 1 and 4",
            ),
            (
                |q| q.max_concurrent = 5,
                "reviews at once must be between 1 and 4",
            ),
            (
                |q| q.reviews_per_day = 0,
                "reviews a day must be between 1 and 20",
            ),
            (
                |q| q.reviews_per_day = 21,
                "reviews a day must be between 1 and 20",
            ),
        ];
        for (change, message) in bad {
            assert_eq!(
                store.update(qa_patch(change)).expect_err("refused"),
                message
            );
        }
        assert!(store.get().qa.default_on);
        assert_eq!(fs::read_to_string(&path).expect("file"), before);
        cleanup(&path);
    }

    #[test]
    fn the_ends_of_the_qa_ranges_are_accepted() {
        let path = temp_path("qa-ends");
        let store = SettingsStore::open(path.clone());
        for (poll, at_once, per_day) in [(1, 1, 1), (60, 4, 20)] {
            let saved = store
                .update(qa_patch(|q| {
                    q.poll_minutes = poll;
                    q.max_concurrent = at_once;
                    q.reviews_per_day = per_day;
                }))
                .expect("ends");
            assert_eq!(
                (
                    saved.qa.poll_minutes,
                    saved.qa.max_concurrent,
                    saved.qa.reviews_per_day
                ),
                (poll, at_once, per_day)
            );
        }
        cleanup(&path);
    }

    #[test]
    fn qa_names_are_trimmed_emptied_or_refused() {
        let path = temp_path("qa-names");
        let store = SettingsStore::open(path.clone());
        let saved = store
            .update(qa_patch(|q| {
                q.model = Some("  opus  ".into());
                q.effort = Some(String::new());
            }))
            .expect("update");
        assert_eq!(saved.qa.model.as_deref(), Some("opus"));
        assert_eq!(saved.qa.effort, None);
        let long = "x".repeat(101);
        let err = store
            .update(qa_patch(|q| q.model = Some(long.clone())))
            .expect_err("long");
        assert_eq!(err, "that model name is not valid");
        let err = store
            .update(qa_patch(|q| q.effort = Some(long.clone())))
            .expect_err("long");
        assert_eq!(err, "that effort name is not valid");
        let err = store
            .update(qa_patch(|q| q.model = Some("o\npus".into())))
            .expect_err("control");
        assert_eq!(err, "that model name is not valid");
        assert!(
            store
                .update(qa_patch(|q| q.model = Some("x".repeat(100))))
                .is_ok()
        );
        assert_eq!(SettingsStore::open(path.clone()).get(), store.get());
        cleanup(&path);
    }

    #[test]
    fn a_file_with_out_of_range_qa_values_loads_clamped() {
        let path = with_file(
            "qa-clamp",
            r#"{"qa":{"pollMinutes":0,"maxConcurrent":99,"reviewsPerDay":500,"model":"  ","effort":"bad\u0007"}}"#,
        );
        let qa = SettingsStore::open(path.clone()).get().qa;
        assert_eq!(
            (qa.poll_minutes, qa.max_concurrent, qa.reviews_per_day),
            (1, 4, 20)
        );
        assert_eq!((qa.model, qa.effort), (None, None));
        cleanup(&path);
    }

    #[test]
    fn a_patch_naming_an_unknown_reviewer_tool_does_not_deserialise() {
        assert!(serde_json::from_str::<SettingsPatch>(r#"{"qa":{"tool":"codex"}}"#).is_err());
        assert!(serde_json::from_str::<SettingsPatch>(r#"{"qa":{"tool":"claude-code"}}"#).is_ok());
    }

    type ManagerChange = fn(&mut ManagerSettings);

    fn manager_patch(change: impl FnOnce(&mut ManagerSettings)) -> SettingsPatch {
        let mut manager = ManagerSettings::default();
        change(&mut manager);
        SettingsPatch {
            manager: Some(manager),
            ..SettingsPatch::default()
        }
    }

    #[test]
    fn a_file_from_before_the_manager_group_loads_its_defaults() {
        let path = with_file("no-manager", r#"{"qa":{"defaultOn":true}}"#);
        let got = SettingsStore::open(path.clone()).get();
        assert_eq!(got.manager, ManagerSettings::default());
        assert_eq!(
            (
                got.manager.tool,
                got.manager.max_desks,
                got.manager.spend_ceiling_usd
            ),
            (ManagerTool::ClaudeCode, 3, 20)
        );
        assert!(got.qa.default_on);
        cleanup(&path);
    }

    #[test]
    fn a_partial_manager_group_fills_in_defaults() {
        let path = with_file("manager-partial", r#"{"manager":{"maxDesks":5}}"#);
        let got = SettingsStore::open(path.clone()).get().manager;
        assert_eq!(
            got,
            ManagerSettings {
                max_desks: 5,
                ..ManagerSettings::default()
            }
        );
        cleanup(&path);
    }

    #[test]
    fn out_of_range_manager_values_are_refused_and_change_nothing() {
        let path = temp_path("manager-refuse");
        let store = SettingsStore::open(path.clone());
        store
            .update(manager_patch(|m| m.max_desks = 4))
            .expect("update");
        let before = fs::read_to_string(&path).expect("file");
        let bad: [(ManagerChange, &str); 6] = [
            (
                |m| m.max_desks = 0,
                "desks at once must be between 1 and 10",
            ),
            (
                |m| m.max_desks = 11,
                "desks at once must be between 1 and 10",
            ),
            (
                |m| m.spend_ceiling_usd = 0,
                "the spend ceiling must be between 1 and 1000 dollars",
            ),
            (
                |m| m.spend_ceiling_usd = 1001,
                "the spend ceiling must be between 1 and 1000 dollars",
            ),
            (
                |m| m.model = Some("o\npus".into()),
                "that model name is not valid",
            ),
            (
                |m| m.effort = Some("x".repeat(101)),
                "that effort name is not valid",
            ),
        ];
        for (change, message) in bad {
            assert_eq!(
                store.update(manager_patch(change)).expect_err("refused"),
                message
            );
        }
        assert_eq!(store.get().manager.max_desks, 4);
        assert_eq!(fs::read_to_string(&path).expect("file"), before);
        let saved = store
            .update(manager_patch(|m| {
                m.max_desks = 10;
                m.spend_ceiling_usd = 1000;
                m.model = Some(" opus ".into());
                m.effort = Some(String::new());
            }))
            .expect("ends");
        assert_eq!(
            (
                saved.manager.max_desks,
                saved.manager.spend_ceiling_usd,
                saved.manager.model.as_deref(),
                saved.manager.effort.as_deref()
            ),
            (10, 1000, Some("opus"), None)
        );
        assert_eq!(SettingsStore::open(path.clone()).get(), saved);
        cleanup(&path);
    }

    #[test]
    fn a_file_with_out_of_range_manager_values_loads_clamped() {
        let path = with_file(
            "manager-clamp",
            r#"{"manager":{"maxDesks":0,"spendCeilingUsd":5000,"model":" ","effort":"bad\u0007"}}"#,
        );
        let got = SettingsStore::open(path.clone()).get().manager;
        assert_eq!((got.max_desks, got.spend_ceiling_usd), (1, 1000));
        assert_eq!((got.model, got.effort), (None, None));
        cleanup(&path);
    }

    #[test]
    fn a_patch_naming_an_unknown_manager_tool_does_not_deserialise() {
        assert!(serde_json::from_str::<SettingsPatch>(r#"{"manager":{"tool":"codex"}}"#).is_err());
        assert!(
            serde_json::from_str::<SettingsPatch>(r#"{"manager":{"tool":"claude-code"}}"#).is_ok()
        );
    }
}
