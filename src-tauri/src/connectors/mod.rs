//! Connectors: outside tools a floor's TODOs can come from. Linear is the first.

#[cfg(test)]
pub mod fake;
pub mod hub;
pub mod linear;

use crate::model::{ConnectorKind, Link, Party, TodoSource};
#[cfg(test)]
use crate::process::URL_MAX;
use crate::process::{Call, browser_call};
use std::time::Duration;

const TEAM_FIELD_MAX: usize = 100;
const PROJECT_NAME_MAX: usize = 200;

/// A standing floor with at least one link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkedFloor {
    pub project_id: String,
    /// The absolute repo root: the key its links are stored under.
    pub root: String,
    pub links: Vec<Link>,
}

/// One open item at a source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// Its stable id at the source (a Linear issue's UUID).
    pub external_id: String,
    pub text: String,
    /// Whose board it goes on.
    pub assignee: Party,
    pub source: TodoSource,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Listing {
    pub items: Vec<Item>,
    /// More match than were fetched.
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectorError {
    /// It cannot ask until a key is saved, allowed or replaced: why, in words.
    NoKey(String),
    /// Told to slow down, for this long if it said.
    RateLimited(Option<Duration>),
    /// Not reached, or it failed on its side. Worth trying again: why, in words.
    Unreachable(String),
    /// It answered and said no: its reason, in words.
    Refused(String),
}

impl ConnectorError {
    /// The sentence shown on a board or in a toast. No trailing full stop.
    #[must_use]
    pub fn words(&self, kind: ConnectorKind) -> String {
        match self {
            Self::NoKey(why) | Self::Unreachable(why) | Self::Refused(why) => why.clone(),
            Self::RateLimited(_) => format!("{} asked the app to slow down", kind.label()),
        }
    }
}

pub trait Connector: Send + Sync {
    fn kind(&self) -> ConnectorKind;
    /// How often each linked floor is asked for its items.
    fn poll_interval(&self) -> Duration;
    /// Every open item `link` asks for. Blocking. Never called under a lock.
    ///
    /// # Errors
    /// When the source cannot be asked or refuses.
    fn list(&self, link: &Link) -> Result<Listing, ConnectorError>;
    /// Mark an item done at its source. Blocking. Never called under a lock.
    ///
    /// # Errors
    /// When the source cannot be reached or refuses.
    fn tick(&self, link: &Link, external_id: &str) -> Result<(), ConnectorError>;
    /// The human asked for a fresh check: forget a remembered refusal.
    fn recheck(&self);
}

/// `<project>|<connector id>|<external id>`.
#[must_use]
pub fn item_id(project_id: &str, kind: ConnectorKind, external_id: &str) -> String {
    format!("{project_id}|{}|{external_id}", kind.id())
}

/// Split a connector item's id into (project id, kind, external id). None for a file item,
/// a store item, or anything malformed.
#[must_use]
pub fn parse_item_id(id: &str) -> Option<(&str, ConnectorKind, &str)> {
    let mut parts = id.rsplitn(3, '|');
    let external = parts.next().filter(|s| !s.is_empty())?;
    let middle = parts.next()?;
    let project = parts.next().filter(|s| !s.is_empty())?;
    let kind = ConnectorKind::ALL.into_iter().find(|k| k.id() == middle)?;
    Some((project, kind, external))
}

/// `value` trimmed, if it is 1 to `max` characters with no control character.
fn bounded(value: &str, max: usize) -> Option<String> {
    let value = value.trim();
    let ok = (1..=max).contains(&value.chars().count()) && !value.chars().any(char::is_control);
    ok.then(|| value.to_string())
}

/// A link from the webview, trimmed and bounded, or why it is not usable.
///
/// # Errors
/// When a field is blank, too long or holds a control character.
pub fn checked(link: Link) -> Result<Link, String> {
    match link {
        Link::Linear {
            team_id,
            team_key,
            team_name,
            project,
            label,
        } => {
            let team = || {
                Some((
                    bounded(&team_id, TEAM_FIELD_MAX)?,
                    bounded(&team_key, TEAM_FIELD_MAX)?,
                    bounded(&team_name, TEAM_FIELD_MAX)?,
                ))
            };
            let (team_id, team_key, team_name) = team().ok_or("choose a Linear team")?;
            let project = project
                .map(|p| {
                    Some(crate::model::LinearProject {
                        id: bounded(&p.id, PROJECT_NAME_MAX)?,
                        name: bounded(&p.name, PROJECT_NAME_MAX)?,
                    })
                })
                .map(|p| p.ok_or("that Linear project is not valid"))
                .transpose()?;
            let label = label
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .map(|l| {
                    crate::settings::agent_label(&l).ok_or_else(|| {
                        format!(
                            "the label must be at most {} characters",
                            crate::settings::AGENT_LABEL_MAX
                        )
                    })
                })
                .transpose()?;
            Ok(Link::Linear {
                team_id,
                team_key,
                team_name,
                project,
                label,
            })
        }
    }
}

/// The program run that opens `url` in the browser, if `url` is a page of `kind`'s service.
pub fn link_call(kind: ConnectorKind, url: &str) -> Result<Call, String> {
    let prefix = match kind {
        ConnectorKind::Linear => "https://linear.app/",
    };
    browser_call(url, prefix).ok_or_else(|| format!("that link does not go to {}", kind.label()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::LinearProject;

    fn linear(team: &str, project: Option<(&str, &str)>, label: Option<&str>) -> Link {
        Link::Linear {
            team_id: team.to_string(),
            team_key: " ENG ".to_string(),
            team_name: "Engineering".to_string(),
            project: project.map(|(id, name)| LinearProject {
                id: id.to_string(),
                name: name.to_string(),
            }),
            label: label.map(str::to_string),
        }
    }

    #[test]
    fn item_ids_round_trip() {
        let id = item_id("p-1", ConnectorKind::Linear, "abc-123");
        assert_eq!(id, "p-1|linear|abc-123");
        assert_eq!(
            parse_item_id(&id),
            Some(("p-1", ConnectorKind::Linear, "abc-123"))
        );
    }

    #[test]
    fn other_ids_are_not_connector_ids() {
        for id in ["p|TODO.md|3", "t-1", "p|linear|", "|linear|x", "p|jira|x"] {
            assert_eq!(parse_item_id(id), None, "{id}");
        }
    }

    #[test]
    fn a_project_id_holding_a_bar_still_parses() {
        assert_eq!(
            parse_item_id("a|b|linear|x"),
            Some(("a|b", ConnectorKind::Linear, "x"))
        );
    }

    #[test]
    fn checked_trims_and_drops_a_blank_label() {
        let got = checked(linear(" t1 ", Some((" pr ", " Checkout ")), Some("  "))).expect("ok");
        assert_eq!(
            got,
            Link::Linear {
                team_id: "t1".to_string(),
                team_key: "ENG".to_string(),
                team_name: "Engineering".to_string(),
                project: Some(LinearProject {
                    id: "pr".to_string(),
                    name: "Checkout".to_string()
                }),
                label: None,
            }
        );
        let labelled = checked(linear("t1", None, Some(" bug "))).expect("ok");
        assert!(matches!(labelled, Link::Linear { label: Some(l), .. } if l == "bug"));
    }

    #[test]
    fn checked_refuses_bad_values() {
        assert_eq!(
            checked(linear("  ", None, None)),
            Err("choose a Linear team".to_string())
        );
        assert_eq!(
            checked(linear("t1", Some(("", "Checkout")), None)),
            Err("that Linear project is not valid".to_string())
        );
        assert_eq!(
            checked(linear("t1", None, Some(&"x".repeat(81)))),
            Err("the label must be at most 80 characters".to_string())
        );
        assert!(checked(linear("t1", None, Some(&"x".repeat(80)))).is_ok());
    }

    #[test]
    fn errors_say_why_in_words() {
        let kind = ConnectorKind::Linear;
        assert_eq!(ConnectorError::NoKey("no key".into()).words(kind), "no key");
        assert_eq!(
            ConnectorError::Unreachable("offline".into()).words(kind),
            "offline"
        );
        assert_eq!(ConnectorError::Refused("no".into()).words(kind), "no");
        assert_eq!(
            ConnectorError::RateLimited(None).words(kind),
            "Linear asked the app to slow down"
        );
    }

    #[test]
    fn a_linear_url_opens_with_open_u() {
        let url = "https://linear.app/example/issue/ENG-1/fix-it";
        assert_eq!(
            link_call(ConnectorKind::Linear, url),
            Ok(Call::new("/usr/bin/open", ["-u", url]))
        );
        let longest = format!("https://linear.app/{}", "x".repeat(URL_MAX - 19));
        assert!(link_call(ConnectorKind::Linear, &longest).is_ok());
    }

    #[test]
    fn other_urls_are_not_opened() {
        let too_long = format!("https://linear.app/{}", "x".repeat(URL_MAX));
        for url in [
            "http://linear.app/example/issue/ENG-1",
            "https://linear.app.example.org/issue/ENG-1",
            "https://linear.app/example/issue/ENG 1",
            "https://linear.app/example/issue/ENG-1\n",
            too_long.as_str(),
        ] {
            assert_eq!(
                link_call(ConnectorKind::Linear, url),
                Err("that link does not go to Linear".to_string()),
                "{url}"
            );
        }
    }
}
