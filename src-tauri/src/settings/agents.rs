//! The Agents group of Settings: how new desks open.

use crate::model::DeskView;
use serde::{Deserialize, Serialize};

/// How new desks open (`AgentSettings` in types.ts).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AgentSettings {
    pub view: DeskView,
}

#[cfg(test)]
mod tests {
    use crate::model::DeskView;
    use crate::settings::{Settings, SettingsPatch, SettingsStore};
    use std::fs;
    use std::path::PathBuf;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!("ob-settings-agents-{}-{name}", std::process::id()))
            .join("settings.json")
    }

    fn cleanup(path: &std::path::Path) {
        fs::remove_dir_all(path.parent().expect("dir")).expect("cleanup");
    }

    #[test]
    fn a_file_without_the_agents_group_loads_with_the_terminal() {
        let path = temp_path("none");
        fs::create_dir_all(path.parent().expect("dir")).expect("mkdir");
        fs::write(&path, r#"{"notifications":{}}"#).expect("write");
        assert_eq!(
            SettingsStore::open(path.clone()).get().agents.view,
            DeskView::Terminal
        );
        cleanup(&path);
    }

    #[test]
    fn a_patch_with_the_chat_view_saves_and_reloads() {
        let path = temp_path("chat");
        let store = SettingsStore::open(path.clone());
        let change: SettingsPatch =
            serde_json::from_str(r#"{"agents":{"view":"chat"}}"#).expect("change");
        let saved = store.update(change).expect("update");
        assert_eq!(saved.agents.view, DeskView::Chat);
        assert_eq!(
            SettingsStore::open(path.clone()).get().agents.view,
            DeskView::Chat
        );
        cleanup(&path);
    }

    #[test]
    fn a_patch_naming_an_unknown_view_does_not_deserialise() {
        assert!(serde_json::from_str::<SettingsPatch>(r#"{"agents":{"view":"tiles"}}"#).is_err());
        assert_eq!(Settings::default().agents.view, DeskView::Terminal);
    }
}
