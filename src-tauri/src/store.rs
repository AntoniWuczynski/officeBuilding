//! The app's own records: decisions, TODOs and agent-to-agent notes. Discovery
//! owns sessions and questions; this owns what only the office knows. Saved as
//! one JSON file in the app's data directory.

use crate::model::{AgentMessage, Decision, Link, TodoItem};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Keep the visible message history short; the UI only animates new ones.
const MESSAGE_HISTORY: usize = 20;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Records {
    #[serde(default)]
    pub decisions: Vec<Decision>,
    #[serde(default)]
    pub todos: Vec<TodoItem>,
    #[serde(default)]
    pub messages: Vec<AgentMessage>,
    /// Floors the human added by hand (absolute repo roots), kept even with no sessions.
    #[serde(default)]
    pub pinned_floors: Vec<String>,
    /// Floors the human demolished (absolute repo roots), with when: what they held
    /// before then stays hidden, and a session started there since brings them back.
    #[serde(default)]
    pub demolished_floors: BTreeMap<String, String>,
    /// What feeds each floor from outside (absolute repo roots, like `pinned_floors`).
    #[serde(default)]
    pub connectors: BTreeMap<String, Vec<Link>>,
    #[serde(default)]
    pub seq: u64,
}

pub struct Store {
    path: PathBuf,
    pub records: Records,
}

impl Store {
    /// Load the store, or start empty if it does not exist yet. A file that
    /// exists but cannot be read is an error: never overwrite someone's data.
    pub fn open(path: PathBuf) -> Result<Self, String> {
        let records = match fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text)
                .map_err(|e| format!("{} is not a valid store: {e}", path.display()))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Records::default(),
            Err(e) => return Err(format!("could not read {}: {e}", path.display())),
        };
        Ok(Self { path, records })
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn next_id(&mut self, prefix: &str) -> String {
        self.records.seq += 1;
        format!("{prefix}-{}", self.records.seq)
    }

    pub fn push_message(&mut self, message: AgentMessage) {
        self.records.messages.push(message);
        let excess = self.records.messages.len().saturating_sub(MESSAGE_HISTORY);
        self.records.messages.drain(..excess);
    }

    /// Write atomically: a temp file renamed over the old one.
    pub fn save(&self) -> Result<(), String> {
        write_json_atomic(&self.path, &self.records)
    }
}

/// Write `value` as pretty JSON to `path` atomically: a temp file renamed over the old one.
pub fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
    }
    let tmp = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    fs::write(&tmp, text).map_err(|e| format!("could not write {}: {e}", tmp.display()))?;
    fs::rename(&tmp, path).map_err(|e| format!("could not replace {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Party;
    use crate::model::TodoSource;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!("ob-store-{}-{name}", std::process::id()))
            .join("store.json")
    }

    #[test]
    fn round_trips_and_trims_messages() {
        let path = temp_path("rt");
        let mut store = Store::open(path.clone()).expect("open empty");
        assert_eq!(store.records, Records::default());
        let id = store.next_id("t");
        store.records.todos.push(TodoItem {
            id: id.clone(),
            project_id: "p".into(),
            text: "x".into(),
            done: false,
            assignee: Party::Human,
            source: TodoSource::App,
        });
        for i in 0..25 {
            store.push_message(AgentMessage {
                id: format!("m{i}"),
                from_session_id: "a".into(),
                to_session_id: "b".into(),
                text: "hi".into(),
                sent_at: String::new(),
            });
        }
        store.save().expect("save");
        let again = Store::open(path.clone()).expect("reopen");
        assert_eq!(again.records.todos[0].id, "t-1");
        assert_eq!(again.records.messages.len(), 20);
        assert_eq!(again.records.messages[0].id, "m5");
        fs::remove_dir_all(path.parent().expect("dir")).expect("cleanup");
    }

    #[test]
    fn refuses_to_clobber_a_corrupt_file() {
        let path = temp_path("bad");
        fs::create_dir_all(path.parent().expect("dir")).expect("mkdir");
        fs::write(&path, "{not json").expect("write");
        assert!(Store::open(path.clone()).is_err());
        fs::remove_dir_all(path.parent().expect("dir")).expect("cleanup");
    }

    #[test]
    fn an_old_store_loads_and_saves_back_equal() {
        let path = temp_path("old");
        fs::create_dir_all(path.parent().expect("dir")).expect("mkdir");
        fs::write(
            &path,
            r#"{"todos":[{"id":"t-1","projectId":"p","text":"x","done":false,"assignee":"human","sourceFile":null}],"pinnedFloors":[],"seq":1}"#,
        )
        .expect("write");
        let store = Store::open(path.clone()).expect("open");
        assert_eq!(store.records.todos[0].source, TodoSource::App);
        assert!(store.records.connectors.is_empty());
        store.save().expect("save");
        assert_eq!(
            Store::open(path.clone()).expect("reopen").records,
            store.records
        );
        fs::remove_dir_all(path.parent().expect("dir")).expect("cleanup");
    }

    #[test]
    fn links_round_trip_through_a_save() {
        let path = temp_path("links");
        let mut store = Store::open(path.clone()).expect("open");
        let link = Link::Linear {
            team_id: "t".into(),
            team_key: "ENG".into(),
            team_name: "Engineering".into(),
            project: None,
            label: Some("bug".into()),
        };
        store
            .records
            .connectors
            .insert("/r".into(), vec![link.clone()]);
        store.save().expect("save");
        let again = Store::open(path.clone()).expect("reopen");
        assert_eq!(again.records.connectors["/r"], vec![link]);
        fs::remove_dir_all(path.parent().expect("dir")).expect("cleanup");
    }
}
