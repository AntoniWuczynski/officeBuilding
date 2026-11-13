//! Which floors a scan lists: demolished floors kept down, pinned floors added.

use crate::discovery::{self, Discovered};
use crate::model::Project;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Keep each demolished floor down until work starts there again. Its earlier
/// sessions never return to the desks, but a rebuilt floor's sign-out sheet
/// keeps everyone who left, the agents its demolition stopped included.
pub(super) fn drop_demolished(found: &mut Discovered, demolished: &BTreeMap<String, String>) {
    let instant = |s: &str| chrono::DateTime::parse_from_rfc3339(s).ok();
    for (key, at) in demolished {
        let Some(at) = instant(at) else { continue };
        let id = discovery::project_id(Path::new(key));
        let since = |when: &str| instant(when).is_some_and(|t| t > at);
        let rebuilt = found
            .sessions
            .iter()
            .any(|s| s.project_id == id && since(&s.last_activity_at))
            || found
                .ended
                .iter()
                .any(|e| e.project_id == id && since(&e.ended_at));
        found
            .sessions
            .retain(|s| s.project_id != id || since(&s.last_activity_at));
        if rebuilt {
            let kept: Vec<String> = found
                .sessions
                .iter()
                .filter(|s| s.project_id == id)
                .map(|s| s.id.clone())
                .collect();
            if let Some(p) = found.projects.iter_mut().find(|p| p.id == id) {
                p.session_ids.retain(|sid| kept.contains(sid));
            }
        } else {
            found.ended.retain(|e| e.project_id != id);
            found.projects.retain(|p| p.id != id);
            found.roots.remove(&id);
        }
    }
    let sessions = &found.sessions;
    found
        .questions
        .retain(|q| q.session_id.is_empty() || sessions.iter().any(|s| s.id == q.session_id));
    let ended = &found.ended;
    found
        .resume_dirs
        .retain(|sid, _| ended.iter().any(|e| &e.id == sid));
}

/// Floors the human pinned that no agent is working on yet: added with no desks.
pub(super) fn add_pinned_floors(found: &mut Discovered, pinned: &[String], home: &Path) {
    for key in pinned {
        let root = PathBuf::from(key);
        let id = discovery::project_id(&root);
        if found.roots.contains_key(&id) || !root.is_dir() {
            continue;
        }
        found.projects.push(Project {
            id: id.clone(),
            name: root
                .file_name()
                .map_or_else(|| key.clone(), |n| n.to_string_lossy().into_owned()),
            path: discovery::display_path(&root, home),
            session_ids: Vec::new(),
        });
        found.roots.insert(id, root);
    }
}
