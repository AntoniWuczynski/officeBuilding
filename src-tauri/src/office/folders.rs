//! The folder behind a place in the building: a desk's checkout, a helper's, a floor's.
//! The webview names the place and never sends a path.

use super::Office;
use crate::discovery::helper_key;
use crate::model::Place;
use std::path::PathBuf;

impl Office {
    /// The folder `place` stands for, checked to exist now.
    pub fn folder_of(&self, place: &Place) -> Result<PathBuf, String> {
        let structured_root = match place {
            Place::Session { session_id } => self.structured.root_of(session_id),
            _ => None,
        };
        let (folder, floor_root) = {
            let inner = self.lock();
            let found = &inner.found;
            let root_of = |project_id: &str| found.roots.get(project_id).cloned();
            let unknown = |session_id: &str| format!("unknown session: {session_id}");
            match place {
                Place::Floor { project_id } => {
                    let root = root_of(project_id)
                        .ok_or_else(|| format!("{project_id} is not a floor of this building"))?;
                    (root.clone(), Some(root))
                }
                Place::Session { session_id } => {
                    if let Some(root) = structured_root {
                        (root.clone(), Some(root))
                    } else {
                        let project_id = found
                            .sessions
                            .iter()
                            .find(|s| &s.id == session_id)
                            .map(|s| &s.project_id)
                            .or_else(|| {
                                found
                                    .ended
                                    .iter()
                                    .find(|s| &s.id == session_id)
                                    .map(|s| &s.project_id)
                            })
                            .ok_or_else(|| unknown(session_id))?;
                        let folder = found
                            .checkouts
                            .get(session_id)
                            .cloned()
                            .ok_or_else(|| unknown(session_id))?;
                        (folder, root_of(project_id))
                    }
                }
                Place::Helper {
                    session_id,
                    helper_id,
                } => {
                    let session = found
                        .sessions
                        .iter()
                        .find(|s| &s.id == session_id)
                        .ok_or_else(|| unknown(session_id))?;
                    if !session.helpers.iter().any(|h| &h.id == helper_id) {
                        return Err(format!("unknown helper: {helper_id}"));
                    }
                    let folder = found
                        .checkouts
                        .get(&helper_key(session_id, helper_id))
                        .cloned()
                        .ok_or_else(|| "that helper's folder is not known yet".to_string())?;
                    (folder, root_of(&session.project_id))
                }
            }
        };
        if folder.is_dir() {
            Ok(folder)
        } else if floor_root.is_some_and(|root| root != folder) {
            Err("that worktree is gone".to_string())
        } else {
            Err("that folder is gone".to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::floor_office;
    use super::*;
    use crate::discovery::Discovered;
    use crate::model::{ControlMode, EndedSession, Helper, Session, SessionState, Spend, ToolKind};
    use std::fs;

    fn session(id: &str, project_id: &str, helpers: Vec<Helper>) -> Session {
        Session {
            id: id.to_string(),
            tool: ToolKind::ClaudeCode,
            project_id: project_id.to_string(),
            title: "t".to_string(),
            state: SessionState::Idle,
            control: ControlMode::RaiseWindow,
            model: None,
            effort: None,
            last_activity_at: String::new(),
            pending_question_ids: Vec::new(),
            spend: Spend::default(),
            helpers,
            pets: Vec::new(),
            worktree: false,
            branch: None,
        }
    }

    fn helper(id: &str) -> Helper {
        Helper {
            id: id.to_string(),
            state: SessionState::Running,
            worktree: true,
            branch: None,
        }
    }

    fn ended(id: &str, project_id: &str) -> EndedSession {
        EndedSession {
            id: id.to_string(),
            tool: ToolKind::ClaudeCode,
            project_id: project_id.to_string(),
            title: "t".to_string(),
            ended_at: String::new(),
            model: None,
            effort: None,
        }
    }

    fn sess(id: &str) -> Place {
        Place::Session {
            session_id: id.to_string(),
        }
    }

    /// An office with a floor, a live session `s1` with helpers `h1` (known folder) and
    /// `h2` (no folder), an ended session `e1`, and a stray `checkouts` entry `ghost`.
    fn seeded(tag: &str) -> (Office, PathBuf, String, PathBuf) {
        let (office, dir, floor) = floor_office(tag);
        let root = dir.join("home/code/app");
        let tree = dir.join("tree");
        let other = dir.join("other");
        for d in [&tree, &other] {
            fs::create_dir_all(d).expect("mkdir");
        }
        {
            let mut inner = office.lock();
            let found: &mut Discovered = &mut inner.found;
            found.sessions = vec![session("s1", &floor.id, vec![helper("h1"), helper("h2")])];
            found.ended = vec![ended("e1", &floor.id)];
            found.checkouts.insert("s1".to_string(), tree.clone());
            found.checkouts.insert("e1".to_string(), root.clone());
            found.checkouts.insert("ghost".to_string(), root);
            found
                .checkouts
                .insert(helper_key("s1", "h1"), other.clone());
        }
        (office, dir, floor.id, other)
    }

    #[test]
    fn a_floor_resolves_to_its_root_and_an_unknown_floor_is_refused() {
        let (office, dir, floor, _) = seeded("folders-floor");
        assert_eq!(
            office.folder_of(&Place::Floor { project_id: floor }),
            Ok(dir.join("home/code/app"))
        );
        assert_eq!(
            office.folder_of(&Place::Floor {
                project_id: "p-nope".to_string()
            }),
            Err("p-nope is not a floor of this building".to_string())
        );
        fs::remove_dir_all(dir).expect("cleanup");
    }

    #[test]
    fn live_and_ended_sessions_resolve_through_checkouts() {
        let (office, dir, _, _) = seeded("folders-sessions");
        assert_eq!(office.folder_of(&sess("s1")), Ok(dir.join("tree")));
        assert_eq!(office.folder_of(&sess("e1")), Ok(dir.join("home/code/app")));
        fs::remove_dir_all(dir).expect("cleanup");
    }

    #[test]
    fn unknown_sessions_are_refused_even_with_a_stray_checkout() {
        let (office, dir, _, _) = seeded("folders-unknown");
        for id in ["nope", "ghost"] {
            assert_eq!(
                office.folder_of(&sess(id)),
                Err(format!("unknown session: {id}"))
            );
        }
        fs::remove_dir_all(dir).expect("cleanup");
    }

    #[test]
    fn helpers_resolve_only_when_listed_and_located() {
        let (office, dir, _, other) = seeded("folders-helpers");
        let helper_at = |helper_id: &str| Place::Helper {
            session_id: "s1".to_string(),
            helper_id: helper_id.to_string(),
        };
        assert_eq!(office.folder_of(&helper_at("h1")), Ok(other));
        assert_eq!(
            office.folder_of(&helper_at("h2")),
            Err("that helper's folder is not known yet".to_string())
        );
        assert_eq!(
            office.folder_of(&helper_at("h3")),
            Err("unknown helper: h3".to_string())
        );
        assert_eq!(
            office.folder_of(&Place::Helper {
                session_id: "zz".to_string(),
                helper_id: "h1".to_string()
            }),
            Err("unknown session: zz".to_string())
        );
        fs::remove_dir_all(dir).expect("cleanup");
    }

    #[test]
    fn a_terminal_backed_desk_resolves_to_its_floor_root() {
        use super::super::launch::{Desk, Launched, desk_session_id};
        let (office, dir, floor) = floor_office("folders-desk");
        let desk = Desk {
            tool: ToolKind::Antigravity,
            project_id: floor.id.clone(),
            root: dir.join("home/code/app"),
            title: "write docs".to_string(),
            model: "m".to_string(),
            effort: "high".to_string(),
        };
        let terminal = office
            .launch(&desk.root.clone(), "/bin/cat", Launched::Desk(desk))
            .expect("launch");
        office.refresh();
        assert_eq!(
            office.folder_of(&sess(&desk_session_id(&terminal))),
            Ok(dir.join("home/code/app"))
        );
        fs::remove_dir_all(dir).expect("cleanup");
    }

    #[test]
    fn a_removed_folder_says_what_is_gone() {
        let (office, dir, floor, _) = seeded("folders-gone");
        fs::remove_dir_all(dir.join("tree")).expect("remove");
        assert_eq!(
            office.folder_of(&sess("s1")),
            Err("that worktree is gone".to_string())
        );
        fs::remove_dir_all(dir.join("home/code/app")).expect("remove");
        assert_eq!(
            office.folder_of(&Place::Floor { project_id: floor }),
            Err("that folder is gone".to_string())
        );
        fs::remove_dir_all(dir).expect("cleanup");
    }

    #[test]
    fn a_place_parses_from_its_three_shapes_and_nothing_else() {
        let parse = |json: &str| serde_json::from_str::<Place>(json);
        assert_eq!(
            parse(r#"{"kind":"session","sessionId":"a"}"#).expect("session"),
            sess("a")
        );
        assert_eq!(
            parse(r#"{"kind":"helper","sessionId":"a","helperId":"h"}"#).expect("helper"),
            Place::Helper {
                session_id: "a".to_string(),
                helper_id: "h".to_string()
            }
        );
        assert_eq!(
            parse(r#"{"kind":"floor","projectId":"p"}"#).expect("floor"),
            Place::Floor {
                project_id: "p".to_string()
            }
        );
        for bad in [
            r#"{"kind":"session","sessionId":"a","path":"/etc"}"#,
            r#"{"kind":"folder","sessionId":"a"}"#,
            r#"{"kind":"helper","sessionId":"a"}"#,
            r#""session""#,
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }
}
