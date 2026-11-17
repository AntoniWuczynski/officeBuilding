//! How sessions group into projects: the rules the human keeps in
//! `~/.claude/office-building/projects.json`, read again only when the file changes.

use crate::store::write_json_atomic;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProjectRule {
    pub id: String,
    pub name: String,
    pub prefixes: Vec<String>,
    pub contains: Vec<String>,
    pub repos: Vec<String>,
    /// Keys the human added to a rule; kept so a save never drops them.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// What the file says, after dropping the entries that cannot be used.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RulesView {
    pub rules: Vec<ProjectRule>,
    /// The harness rule's id, only when a kept rule has it.
    pub harness_id: Option<String>,
    pub review_skills: Vec<String>,
    pub error: Option<String>,
}

/// What a front-end choice is about: a repo folder, or a session by its full name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TargetKind {
    Repo,
    Session,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Target {
    pub kind: TargetKind,
    pub key: String,
}

/// The project chosen: a known one by `id`, or any by `name`.
#[derive(Debug, Clone, Deserialize)]
pub struct Choice {
    pub id: Option<String>,
    pub name: String,
}

#[derive(Debug, Default)]
pub struct ProjectRules {
    seen: Option<(Option<SystemTime>, RulesView)>,
    /// The last file that parsed, kept while a later edit leaves it unreadable.
    last_good: Option<RulesView>,
    /// What `changed` last reported, to tell the next refresh whether the rules moved.
    announced: Option<RulesView>,
    /// How many times the file was read and parsed: a test seam for the mtime check.
    pub reads: u32,
}

fn path_of(home: &Path) -> PathBuf {
    home.join(".claude/office-building/projects.json")
}

fn slug(text: &str) -> String {
    let dashed: String = text
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_lowercase().to_string()
            } else {
                "-".to_string()
            }
        })
        .collect();
    dashed
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn parse(text: &str) -> Result<RulesView, String> {
    let Ok(Value::Object(file)) = serde_json::from_str::<Value>(text) else {
        return Err("projects.json is not a JSON object".to_string());
    };
    let mut view = RulesView::default();
    let mut dropped = 0;
    let mut ids = HashSet::new();
    for entry in file
        .get("rules")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let rule = serde_json::from_value::<ProjectRule>(entry.clone()).ok();
        let Some(mut rule) = rule.filter(|r| !r.name.trim().is_empty()) else {
            dropped += 1;
            continue;
        };
        if rule.id.trim().is_empty() {
            rule.id = slug(&rule.name);
        }
        if rule.id.is_empty() || !ids.insert(rule.id.clone()) {
            dropped += 1;
            continue;
        }
        view.rules.push(rule);
    }
    if dropped > 0 {
        view.error = Some(format!(
            "{dropped} rule(s) in projects.json have no name or repeat an id and were left out"
        ));
    }
    view.harness_id = file
        .get("harnessId")
        .and_then(Value::as_str)
        .filter(|id| view.rules.iter().any(|r| r.id == *id))
        .map(str::to_string);
    view.review_skills = file
        .get("reviewSkills")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|s| s.as_str().map(str::to_string))
        .collect();
    Ok(view)
}

impl ProjectRules {
    /// The rules as the file has them now. A missing file is no rules and no error.
    pub fn view(&mut self, home: &Path) -> &RulesView {
        let path = path_of(home);
        let modified = fs::metadata(&path).and_then(|m| m.modified()).ok();
        let stale = self.seen.as_ref().is_none_or(|(at, _)| *at != modified);
        if stale {
            let parsed = match fs::read_to_string(&path) {
                Ok(text) => parse(&text),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(RulesView::default()),
                Err(e) => Err(format!("could not read projects.json: {e}")),
            };
            let view = match parsed {
                Ok(view) => {
                    self.last_good = Some(view.clone());
                    view
                }
                Err(error) => RulesView {
                    error: Some(error),
                    ..self.last_good.clone().unwrap_or_default()
                },
            };
            self.reads += 1;
            self.seen = Some((modified, view));
        }
        &self.seen.get_or_insert_with(Default::default).1
    }

    /// Whether the rules differ from what the last call reported.
    pub fn changed(&mut self, home: &Path) -> bool {
        let now = self.view(home).clone();
        self.announced
            .replace(now.clone())
            .is_none_or(|was| was != now)
    }

    /// Put a repo or a session's name under a project, read fresh from the file so that two
    /// choices in a row both stay. The project is made when no rule has its id or name.
    pub fn assign(&mut self, home: &Path, target: &Target, choice: &Choice) -> Result<(), String> {
        if target.key.trim().is_empty() {
            return Err("nothing to put under a project".to_string());
        }
        self.seen = None;
        let view = self.view(home);
        if let Some(error) = &view.error {
            return Err(error.clone());
        }
        let mut rules = view.rules.clone();
        let name = choice.name.trim();
        let id = choice.id.as_deref().filter(|id| !id.is_empty());
        let at = rules
            .iter()
            .position(|r| Some(r.id.as_str()) == id)
            .or_else(|| {
                rules
                    .iter()
                    .position(|r| r.name.to_lowercase() == name.to_lowercase())
            });
        let at = if let Some(at) = at {
            at
        } else {
            let made = id.map_or_else(|| slug(name), str::to_string);
            if name.is_empty() || made.is_empty() || rules.iter().any(|r| r.id == made) {
                return Err("that project needs a name no other project has".to_string());
            }
            rules.push(ProjectRule {
                id: made,
                name: name.to_string(),
                ..ProjectRule::default()
            });
            rules.len() - 1
        };
        let key = match target.kind {
            TargetKind::Repo => target.key.clone(),
            TargetKind::Session => target.key.to_lowercase(),
        };
        for (i, rule) in rules.iter_mut().enumerate() {
            let list = match target.kind {
                TargetKind::Repo => &mut rule.repos,
                TargetKind::Session => &mut rule.prefixes,
            };
            list.retain(|k| *k != key);
            if i == at {
                list.push(key.clone());
            }
        }
        self.save(home, &rules)
    }

    /// Replace the rules, keeping every other key of the file as found. Refused while the file
    /// has a problem: saving then would overwrite what the human wrote.
    fn save(&mut self, home: &Path, rules: &[ProjectRule]) -> Result<(), String> {
        if let Some(error) = self.view(home).error.clone() {
            return Err(error);
        }
        let path = path_of(home);
        let mut file = match fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<Value>(&text) {
                Ok(Value::Object(file)) => file,
                _ => return Err("projects.json is not a JSON object".to_string()),
            },
            Err(_) => Map::new(),
        };
        let rules = serde_json::to_value(rules).map_err(|e| e.to_string())?;
        file.insert("rules".to_string(), rules);
        write_json_atomic(&path, &Value::Object(file))?;
        self.seen = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ob-rules-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join(".claude/office-building")).expect("make home");
        dir
    }

    fn write(home: &Path, text: &str) {
        fs::write(path_of(home), text).expect("write rules");
    }

    fn rule(id: &str, name: &str) -> ProjectRule {
        ProjectRule {
            id: id.into(),
            name: name.into(),
            prefixes: vec!["alpha-".into()],
            ..ProjectRule::default()
        }
    }

    #[test]
    fn a_missing_file_is_no_rules_and_no_error() {
        let home = home("missing");
        fs::remove_file(path_of(&home)).ok();
        let mut rules = ProjectRules::default();
        assert_eq!(rules.view(&home), &RulesView::default());
    }

    #[test]
    fn malformed_json_is_no_rules_and_an_error() {
        let home = home("malformed");
        write(&home, "{ not json");
        let mut rules = ProjectRules::default();
        let view = rules.view(&home);
        assert!(view.rules.is_empty());
        assert!(view.error.is_some());
    }

    #[test]
    fn unusable_entries_are_dropped_and_the_rest_kept() {
        let home = home("dropped");
        write(
            &home,
            r#"{"rules":[{"id":"alpha","name":"Alpha"},{"id":"x","name":" "},
            {"id":"alpha","name":"Beta"},{"name":"Omega Lab","prefixes":["om-"]}],
            "harnessId":"x"}"#,
        );
        let mut rules = ProjectRules::default();
        let view = rules.view(&home);
        let ids: Vec<&str> = view.rules.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, ["alpha", "omega-lab"]);
        assert!(view.error.is_some());
        assert_eq!(view.harness_id, None);
    }

    #[test]
    fn a_harness_naming_a_kept_rule_is_kept() {
        let home = home("harness");
        write(
            &home,
            r#"{"rules":[{"id":"omega","name":"Omega"}],"harnessId":"omega"}"#,
        );
        let mut rules = ProjectRules::default();
        assert_eq!(rules.view(&home).harness_id.as_deref(), Some("omega"));
    }

    #[test]
    fn save_is_refused_while_the_file_has_a_problem_and_leaves_its_bytes() {
        let home = home("refused");
        let text = r#"{"rules":[{"id":"a","name":""}]}"#;
        write(&home, text);
        let mut rules = ProjectRules::default();
        assert!(rules.save(&home, &[rule("beta", "Beta")]).is_err());
        assert_eq!(fs::read_to_string(path_of(&home)).expect("read"), text);
    }

    #[test]
    fn save_round_trips_and_keeps_the_other_keys() {
        let home = home("round");
        write(
            &home,
            r#"{"rules":[],"harnessId":"omega","reviewSkills":["review"]}"#,
        );
        let mut rules = ProjectRules::default();
        let saved = [rule("omega", "Omega"), rule("beta", "Beta")];
        rules.save(&home, &saved).expect("save");
        let view = rules.view(&home).clone();
        assert_eq!(view.rules, saved);
        assert_eq!(view.harness_id.as_deref(), Some("omega"));
        assert_eq!(view.review_skills, ["review"]);
        assert_eq!(view.error, None);
    }

    fn choice(id: Option<&str>, name: &str) -> Choice {
        Choice {
            id: id.map(str::to_string),
            name: name.into(),
        }
    }

    fn target(kind: TargetKind, key: &str) -> Target {
        Target {
            kind,
            key: key.into(),
        }
    }

    #[test]
    fn a_save_keeps_keys_the_human_added_to_a_rule() {
        let home = home("extra");
        write(
            &home,
            r#"{"rules":[{"id":"alpha","name":"Alpha","colour":"teal"}]}"#,
        );
        let mut rules = ProjectRules::default();
        rules
            .assign(&home, &target(TargetKind::Repo, "repo-one"), &choice(Some("alpha"), "Alpha"))
            .expect("assign");
        let text = fs::read_to_string(path_of(&home)).expect("file");
        assert!(text.contains(r#""colour": "teal""#) || text.contains(r#""colour":"teal""#), "{text}");
    }

    #[test]
    fn an_empty_key_is_refused_and_the_file_is_left_alone() {
        let home = home("empty-key");
        let before = r#"{"rules":[{"id":"alpha","name":"Alpha"}]}"#;
        write(&home, before);
        let mut rules = ProjectRules::default();
        for kind in [TargetKind::Session, TargetKind::Repo] {
            assert!(rules.assign(&home, &target(kind, "  "), &choice(None, "Alpha")).is_err());
        }
        assert_eq!(fs::read_to_string(path_of(&home)).expect("file"), before);
    }

    #[test]
    fn two_assignments_in_a_row_both_persist() {
        let home = home("assign");
        write(
            &home,
            r#"{"rules":[{"id":"alpha","name":"Alpha","prefixes":["alpha-fix-2-x"]}],"harnessId":"alpha","reviewSkills":["review"]}"#,
        );
        let mut rules = ProjectRules::default();
        let session = target(TargetKind::Session, "Alpha-Fix-2-X");
        rules
            .assign(&home, &session, &choice(None, "beta"))
            .expect("first");
        rules
            .assign(
                &home,
                &target(TargetKind::Repo, "/code/beta-web"),
                &choice(None, "Omega"),
            )
            .expect("second");
        let view = rules.view(&home).clone();
        let by_id = |id: &str| {
            view.rules
                .iter()
                .find(|r| r.id == id)
                .expect("rule")
                .clone()
        };
        assert!(by_id("alpha").prefixes.is_empty());
        assert_eq!(by_id("beta").prefixes, ["alpha-fix-2-x"]);
        assert_eq!(by_id("omega").repos, ["/code/beta-web"]);
        assert_eq!(view.harness_id.as_deref(), Some("alpha"));
        assert_eq!(view.review_skills, ["review"]);
        // A later choice moves the repo off the rule it was on.
        rules
            .assign(
                &home,
                &target(TargetKind::Repo, "/code/beta-web"),
                &choice(Some("alpha"), "Alpha"),
            )
            .expect("move");
        let view = rules.view(&home);
        assert!(
            view.rules
                .iter()
                .find(|r| r.id == "omega")
                .expect("omega")
                .repos
                .is_empty()
        );
    }

    #[test]
    fn an_assignment_is_refused_while_the_file_has_a_problem() {
        let home = home("assign-refused");
        write(&home, "{ not json");
        let mut rules = ProjectRules::default();
        let session = target(TargetKind::Session, "beta-web");
        assert!(
            rules
                .assign(&home, &session, &choice(None, "Beta"))
                .is_err()
        );
        assert_eq!(
            fs::read_to_string(path_of(&home)).expect("read"),
            "{ not json"
        );
    }

    #[test]
    fn a_file_broken_after_a_good_read_keeps_the_last_good_rules() {
        let home = home("keep-good");
        write(&home, r#"{"rules":[{"id":"alpha","name":"Alpha"}]}"#);
        let mut rules = ProjectRules::default();
        assert_eq!(rules.view(&home).rules.len(), 1);
        write(&home, "{ not json");
        let later = SystemTime::now() + std::time::Duration::from_secs(5);
        fs::File::options()
            .write(true)
            .open(path_of(&home))
            .and_then(|f| f.set_modified(later))
            .expect("touch");
        let view = rules.view(&home);
        assert_eq!(view.rules.len(), 1);
        assert!(view.error.is_some());
    }

    #[test]
    fn changed_reports_a_new_file_once() {
        let home = home("changed");
        let mut rules = ProjectRules::default();
        assert!(rules.changed(&home));
        assert!(!rules.changed(&home));
        write(&home, r#"{"rules":[{"id":"alpha","name":"Alpha"}]}"#);
        assert!(rules.changed(&home));
    }

    #[test]
    fn an_unchanged_file_is_not_read_again() {
        let home = home("mtime");
        write(&home, r#"{"rules":[{"id":"a","name":"Alpha"}]}"#);
        let mut rules = ProjectRules::default();
        rules.view(&home);
        rules.view(&home);
        assert_eq!(rules.reads, 1);
        let later = SystemTime::now() + std::time::Duration::from_secs(5);
        fs::File::options()
            .write(true)
            .open(path_of(&home))
            .and_then(|f| f.set_modified(later))
            .expect("touch");
        rules.view(&home);
        assert_eq!(rules.reads, 2);
    }
}
