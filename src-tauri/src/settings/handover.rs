//! The Handover group of Settings: whether a used-up desk is handed over by itself,
//! and what the receiving tool starts with.

use super::qa_name;
use crate::handover::Tool;
use serde::{Deserialize, Serialize};

/// What the receiving tool starts with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum StartWith {
    /// The tool's own configured default: no model and no effort is passed.
    ToolDefault,
    Chosen {
        model: String,
        effort: String,
    },
}

/// The Handover group of Settings (`HandoverSettings` in types.ts).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HandoverSettings {
    /// Hand a desk over without asking when its own tool records a used-up subscription.
    pub automatic: bool,
    /// What Codex starts with when a Claude Code desk is handed to it. None: not chosen.
    pub claude_to_codex: Option<StartWith>,
    /// The other direction. Stage H1 refuses anything but None.
    pub codex_to_claude: Option<StartWith>,
}

/// For `Settings::handover`: a group that does not parse loads as the default, so the other
/// groups survive.
pub(super) fn lenient<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<HandoverSettings, D::Error> {
    // `Value` is only the step between the file and the typed group. It goes no further.
    let raw = serde_json::Value::deserialize(d)?;
    Ok(serde_json::from_value(raw).unwrap_or_default())
}

/// Whether a settings file holds a handover group that loads as the default instead.
pub(super) fn dropped(text: &str) -> bool {
    #[derive(Deserialize)]
    struct Probe {
        handover: Option<serde_json::Value>,
    }
    let Ok(Probe {
        handover: Some(raw),
    }) = serde_json::from_str(text)
    else {
        return false;
    };
    serde_json::from_value::<HandoverSettings>(raw).is_err()
}

impl StartWith {
    /// The same choice with a model and an effort trimmed, or why one is not valid.
    fn checked(self) -> Result<Self, String> {
        let Self::Chosen { model, effort } = self else {
            return Ok(self);
        };
        let name = |raw: &str, what: &str| match qa_name(raw) {
            Ok(Some(n)) => Ok(n),
            Ok(None) => Err("choose a model and an effort for Codex".to_string()),
            Err(()) => Err(format!("that {what} name is not valid")),
        };
        Ok(Self::Chosen {
            model: name(&model, "model")?,
            effort: name(&effort, "effort")?,
        })
    }
}

impl HandoverSettings {
    /// What the tool a desk of `from` is handed to starts with, if A Human has chosen.
    #[must_use]
    pub fn start_with(&self, from: Tool) -> Option<&StartWith> {
        match from {
            Tool::ClaudeCode => self.claude_to_codex.as_ref(),
            Tool::Codex => self.codex_to_claude.as_ref(),
        }
    }

    pub(super) fn checked(mut self) -> Result<Self, String> {
        if self.codex_to_claude.is_some() {
            return Err("handing a Codex desk to Claude Code is not built yet".to_string());
        }
        self.claude_to_codex = self.claude_to_codex.map(StartWith::checked).transpose()?;
        Ok(self)
    }

    /// As a file may hold anything: a choice that is not valid, or a direction not built, is dropped.
    pub(super) fn clamped(mut self) -> Self {
        self.claude_to_codex = self.claude_to_codex.and_then(|c| c.checked().ok());
        self.codex_to_claude = None;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Settings, SettingsPatch, SettingsStore};
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn path(name: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!(
                "ob-handover-settings-{}-{name}",
                std::process::id()
            ))
            .join("settings.json")
    }

    fn store_with(name: &str, text: Option<&str>) -> (SettingsStore, PathBuf) {
        let p = path(name);
        fs::create_dir_all(p.parent().expect("dir")).expect("mkdir");
        if let Some(text) = text {
            fs::write(&p, text).expect("write");
        }
        (SettingsStore::open(p.clone()), p)
    }

    fn cleanup(p: &std::path::Path) {
        fs::remove_dir_all(p.parent().expect("dir")).expect("cleanup");
    }

    fn patch(handover: HandoverSettings) -> SettingsPatch {
        SettingsPatch {
            handover: Some(handover),
            ..SettingsPatch::default()
        }
    }

    fn chosen(model: &str, effort: &str) -> StartWith {
        StartWith::Chosen {
            model: model.into(),
            effort: effort.into(),
        }
    }

    fn refused(h: HandoverSettings) -> String {
        Settings::default().apply(patch(h)).expect_err("refused")
    }

    #[test]
    fn a_file_without_the_group_loads_off_and_unset() {
        let (store, p) = store_with("none", Some(r#"{"editor":{"preferred":null}}"#));
        assert_eq!(store.get().handover, HandoverSettings::default());
        assert!(!store.get().handover.automatic);
        cleanup(&p);
    }

    #[test]
    fn a_chosen_start_is_trimmed_saved_and_reloaded() {
        let (store, p) = store_with("chosen", None);
        let text = r#"{"handover":{"automatic":true,"claudeToCodex":{"kind":"chosen","model":" gpt-x ","effort":"high"},"codexToClaude":null}}"#;
        let patch: SettingsPatch = serde_json::from_str(text).expect("patch");
        let saved = store.update(patch).expect("update");
        let want = HandoverSettings {
            automatic: true,
            claude_to_codex: Some(chosen("gpt-x", "high")),
            codex_to_claude: None,
        };
        assert_eq!(saved.handover, want);
        assert_eq!(SettingsStore::open(p.clone()).get().handover, want);
        cleanup(&p);
    }

    #[test]
    fn the_tool_default_saves() {
        let (store, p) = store_with("default", None);
        let h = HandoverSettings {
            claude_to_codex: Some(StartWith::ToolDefault),
            ..HandoverSettings::default()
        };
        store.update(patch(h.clone())).expect("update");
        assert_eq!(SettingsStore::open(p.clone()).get().handover, h);
        cleanup(&p);
    }

    #[test]
    fn bad_names_are_refused_with_their_sentences() {
        let with = |m: &str, e: &str| HandoverSettings {
            claude_to_codex: Some(chosen(m, e)),
            ..HandoverSettings::default()
        };
        assert_eq!(
            refused(with("  ", "high")),
            "choose a model and an effort for Codex"
        );
        assert_eq!(
            refused(with("gpt", "")),
            "choose a model and an effort for Codex"
        );
        assert_eq!(
            refused(with("g\u{7}pt", "high")),
            "that model name is not valid"
        );
        assert_eq!(
            refused(with("gpt", &"e".repeat(101))),
            "that effort name is not valid"
        );
        assert!(
            Settings::default()
                .apply(patch(with(&"m".repeat(100), "high")))
                .is_ok()
        );
    }

    #[test]
    fn codex_to_claude_is_refused() {
        let h = HandoverSettings {
            codex_to_claude: Some(StartWith::ToolDefault),
            ..HandoverSettings::default()
        };
        assert_eq!(
            refused(h),
            "handing a Codex desk to Claude Code is not built yet"
        );
    }

    #[test]
    fn a_file_with_an_invalid_choice_or_the_other_direction_loads_both_as_none() {
        let text = r#"{"handover":{"automatic":true,"claudeToCodex":{"kind":"chosen","model":"","effort":"high"},"codexToClaude":{"kind":"tool-default"}}}"#;
        let (store, p) = store_with("invalid", Some(text));
        let h = store.get().handover;
        assert!(h.automatic);
        assert_eq!((h.claude_to_codex, h.codex_to_claude), (None, None));
        cleanup(&p);
    }

    #[test]
    fn a_patch_with_an_unknown_kind_does_not_deserialise() {
        let text = r#"{"handover":{"automatic":true,"claudeToCodex":{"kind":"whatever"}}}"#;
        assert!(serde_json::from_str::<SettingsPatch>(text).is_err());
    }

    #[test]
    fn start_with_follows_the_direction() {
        let h = HandoverSettings {
            claude_to_codex: Some(StartWith::ToolDefault),
            ..HandoverSettings::default()
        };
        assert_eq!(
            h.start_with(Tool::ClaudeCode),
            Some(&StartWith::ToolDefault)
        );
        assert_eq!(h.start_with(Tool::Codex), None);
    }
}
