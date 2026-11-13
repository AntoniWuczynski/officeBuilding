//! Linear: a floor's open issues over GraphQL, with a personal API key from the Keychain.

mod wire;

#[cfg(test)]
mod tests;

use super::{Connector, ConnectorError, Item, Listing};
use crate::http::{HttpClient, HttpError, Reply, Request};
use crate::model::{ConnectorKind, LinearKeyStatus, LinearTeam, Link, Party, TodoSource};
use crate::project_files::truncate;
use crate::secrets::{Secret, SecretStore};
use crate::settings::SettingsStore;
use crate::sync::lock;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use wire::{
    Body, Data, Errors, IssueFilter, IssuesData, IssuesVars, NoVars, StatesData, StatesVars,
    TeamsData, TickData, TickVars, ViewerData,
};

pub const ENDPOINT: &str = "https://api.linear.app/graphql";
/// The Keychain account the key is saved under.
pub const KEY_ACCOUNT: &str = "linear-api-key";
pub const POLL_EVERY: Duration = Duration::from_secs(60);
/// 200 issues a floor, at the 50 a page written into `ISSUES`.
const MAX_PAGES: usize = 4;
/// Like a file item.
const TEXT_MAX: usize = 300;
const KEY_MAX: usize = 200;
/// Of Linear's own error message.
const WORDS_MAX: usize = 200;

const NO_KEY: &str = "no Linear API key is saved";
const KEY_REFUSED: &str = "Linear did not accept the saved key";

const VIEWER: &str = "query Viewer { viewer { id name } }";
const TEAMS: &str = r#"query Teams {
  teams(first: 50) {
    nodes {
      id key name
      projects(first: 50, filter: { status: { type: { nin: ["completed", "canceled"] } } }) { nodes { id name } }
    }
  }
}"#;
const ISSUES: &str = "query Issues($filter: IssueFilter!, $after: String) {
  issues(first: 50, after: $after, filter: $filter) {
    nodes { id identifier title url assignee { isMe } labels(first: 10) { nodes { name } } }
    pageInfo { hasNextPage endCursor }
  }
}";
const COMPLETED_STATES: &str = r#"query CompletedStates($teamId: String!) {
  team(id: $teamId) { states(first: 50, filter: { type: { eq: "completed" } }) { nodes { id position } } }
}"#;
const TICK: &str = "mutation Tick($id: String!, $stateId: String!) {
  issueUpdate(id: $id, input: { stateId: $stateId }) { success issue { id state { type } } }
}";

pub struct Linear {
    http: Arc<dyn HttpClient>,
    secrets: Arc<dyn SecretStore>,
    settings: Arc<SettingsStore>,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    /// None: the Keychain has not been asked this launch.
    key: Option<Key>,
    /// Each team's completed status with the lowest position, by team id.
    completed: BTreeMap<String, String>,
}

enum Key {
    Missing,
    Saved {
        secret: Secret,
        account: Option<String>,
    },
    /// Linear refused it. Kept so Re-check can try it once more.
    Rejected(Secret),
    /// The Keychain would not hand it over: why.
    Unavailable(String),
}

impl Linear {
    #[must_use]
    pub fn new(
        http: Arc<dyn HttpClient>,
        secrets: Arc<dyn SecretStore>,
        settings: Arc<SettingsStore>,
    ) -> Self {
        Self {
            http,
            secrets,
            settings,
            state: Mutex::new(State::default()),
        }
    }

    /// Run `f` on the key, reading the Keychain first if it has not been read this launch.
    /// The lock is held across that read on purpose: two threads must not both ask, because
    /// each ask can raise a system prompt.
    fn with_key<R>(&self, f: impl FnOnce(&Key) -> R) -> R {
        let mut state = lock(&self.state);
        let key = state
            .key
            .get_or_insert_with(|| match self.secrets.get(KEY_ACCOUNT) {
                Ok(Some(secret)) => Key::Saved {
                    secret,
                    account: None,
                },
                Ok(None) => Key::Missing,
                Err(why) => Key::Unavailable(why),
            });
        f(key)
    }

    fn key(&self) -> Result<Secret, ConnectorError> {
        self.with_key(|key| match key {
            Key::Saved { secret, .. } => Ok(secret.clone()),
            Key::Missing => Err(ConnectorError::NoKey(NO_KEY.to_string())),
            Key::Rejected(_) => Err(ConnectorError::NoKey(KEY_REFUSED.to_string())),
            Key::Unavailable(why) => Err(ConnectorError::NoKey(why.clone())),
        })
    }

    /// What is known about the key. Reads the Keychain if it has not been read this launch.
    #[must_use]
    pub fn key_status(&self) -> LinearKeyStatus {
        self.with_key(|key| match key {
            Key::Missing => LinearKeyStatus::Missing,
            Key::Saved { account, .. } => LinearKeyStatus::Saved {
                account: account.clone(),
            },
            Key::Rejected(_) => LinearKeyStatus::Rejected,
            Key::Unavailable(reason) => LinearKeyStatus::Unavailable {
                reason: reason.clone(),
            },
        })
    }

    /// Save `key` after Linear accepts it, or remove the saved key with `None`.
    pub fn set_key(&self, key: Option<String>) -> Result<LinearKeyStatus, String> {
        let Some(key) = key else {
            self.secrets.delete(KEY_ACCOUNT)?;
            let mut state = lock(&self.state);
            state.key = Some(Key::Missing);
            state.completed.clear();
            return Ok(LinearKeyStatus::Missing);
        };
        let key = key.trim();
        if !(1..=KEY_MAX).contains(&key.chars().count())
            || !key.chars().all(|c| c.is_ascii_graphic())
        {
            return Err("that does not look like a Linear API key".to_string());
        }
        let secret = Secret::new(key.to_string());
        // A trial: a refusal here says nothing about the saved key, so the state is untouched.
        let viewer: ViewerData = self.post(&secret, VIEWER, &NoVars {}).map_err(|e| match e {
            ConnectorError::NoKey(_) => "Linear did not accept that key".to_string(),
            ConnectorError::Unreachable(why) => {
                format!("the key could not be checked, so it was not saved: {why}")
            }
            ConnectorError::RateLimited(_) => "Linear asked the app to slow down, so the key was not saved. Try again in a minute".to_string(),
            ConnectorError::Refused(why) => why,
        })?;
        self.secrets.set(KEY_ACCOUNT, &secret)?;
        let account = Some(viewer.viewer.name);
        let mut state = lock(&self.state);
        state.key = Some(Key::Saved {
            secret,
            account: account.clone(),
        });
        // A new key may belong to another workspace.
        state.completed.clear();
        Ok(LinearKeyStatus::Saved { account })
    }

    /// The teams the key can see, each with its open projects.
    pub fn teams(&self) -> Result<Vec<LinearTeam>, String> {
        let words = |e: ConnectorError| e.words(ConnectorKind::Linear);
        let key = self.key().map_err(words)?;
        let data: TeamsData = self.ask(&key, TEAMS, &NoVars {}).map_err(words)?;
        Ok(data
            .teams
            .nodes
            .into_iter()
            .map(|t| LinearTeam {
                id: t.id,
                key: t.key,
                name: t.name,
                projects: t.projects.nodes,
            })
            .collect())
    }

    /// One request with the saved key. A refusal of the key is remembered, so no further
    /// request is made until the key changes or Re-check is pressed.
    fn ask<V: Serialize, T: DeserializeOwned>(
        &self,
        key: &Secret,
        query: &'static str,
        variables: &V,
    ) -> Result<T, ConnectorError> {
        let answer = self.post(key, query, variables);
        if matches!(answer, Err(ConnectorError::NoKey(_))) {
            let mut state = lock(&self.state);
            if matches!(&state.key, Some(Key::Saved { secret, .. }) if secret == key) {
                state.key = Some(Key::Rejected(key.clone()));
            }
        }
        answer
    }

    /// One request with `key`, touching no state.
    fn post<V: Serialize, T: DeserializeOwned>(
        &self,
        key: &Secret,
        query: &'static str,
        variables: &V,
    ) -> Result<T, ConnectorError> {
        let body = serde_json::to_string(&Body { query, variables }).map_err(|_| {
            ConnectorError::Refused("the app could not write its question to Linear".to_string())
        })?;
        read(self.http.post_json(&Request {
            url: ENDPOINT,
            authorization: Some(key),
            body: &body,
        }))
    }

    /// The id of `team_id`'s completed status with the lowest position, cached per team.
    fn completed_state(&self, key: &Secret, team_id: &str) -> Result<String, ConnectorError> {
        let cached = lock(&self.state).completed.get(team_id).cloned();
        if let Some(id) = cached {
            return Ok(id);
        }
        let data: StatesData = self.ask(key, COMPLETED_STATES, &StatesVars { team_id })?;
        let id = data
            .team
            .states
            .nodes
            .into_iter()
            .min_by(|a, b| a.position.total_cmp(&b.position))
            .map(|s| s.id)
            .ok_or_else(|| {
                ConnectorError::Refused("this Linear team has no completed status".to_string())
            })?;
        lock(&self.state)
            .completed
            .insert(team_id.to_string(), id.clone());
        Ok(id)
    }
}

/// An answer, or why there is none, in the order: no reply, rate limited, key refused,
/// Linear's own failure, any other error, then the data.
fn read<T: DeserializeOwned>(reply: Result<Reply, HttpError>) -> Result<T, ConnectorError> {
    let reply = reply
        .map_err(|e| ConnectorError::Unreachable(format!("Linear could not be asked: {e}")))?;
    let errors = serde_json::from_str::<Errors>(&reply.body)
        .map(|e| e.errors)
        .unwrap_or_default();
    let extension = |test: &dyn Fn(&wire::Extensions) -> bool| {
        errors
            .iter()
            .any(|e| e.extensions.as_ref().is_some_and(test))
    };
    let status = reply.status;
    if status == 429
        || extension(&|x| {
            x.code
                .as_deref()
                .is_some_and(|c| c.eq_ignore_ascii_case("RATELIMITED"))
                || x.kind.as_deref() == Some("ratelimited")
        })
    {
        return Err(ConnectorError::RateLimited(reply.retry_after));
    }
    if status == 401
        || extension(&|x| {
            x.kind.as_deref() == Some("authentication error")
                || x.code.as_deref() == Some("AUTHENTICATION_ERROR")
        })
    {
        return Err(ConnectorError::NoKey(KEY_REFUSED.to_string()));
    }
    if status >= 500 {
        return Err(ConnectorError::Unreachable(format!(
            "Linear had a problem (HTTP {status})"
        )));
    }
    if !errors.is_empty() || !(200..=299).contains(&status) {
        let words = errors
            .first()
            .map(|e| {
                e.extensions
                    .as_ref()
                    .and_then(|x| x.user_presentable_message.as_deref())
                    .unwrap_or(&e.message)
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .filter(|w| !w.is_empty());
        return Err(ConnectorError::Refused(match words {
            Some(words) => format!("Linear said: {}", truncate(&words, WORDS_MAX)),
            None => format!("Linear answered with HTTP {status}"),
        }));
    }
    serde_json::from_str::<Data<T>>(&reply.body)
        .map(|d| d.data)
        .map_err(|_| {
            ConnectorError::Refused("Linear sent an answer the app could not read".to_string())
        })
}

/// Whose board an issue goes on. None: not shown.
fn board(is_me: Option<bool>, labels: &[String], agent_label: &str) -> Option<Party> {
    let agent_label = agent_label.to_lowercase();
    if labels.iter().any(|l| l.to_lowercase() == agent_label) {
        return Some(Party::Ai);
    }
    match is_me {
        Some(true) => Some(Party::Human),
        None => Some(Party::Ai),
        Some(false) => None,
    }
}

impl Connector for Linear {
    fn kind(&self) -> ConnectorKind {
        ConnectorKind::Linear
    }

    fn poll_interval(&self) -> Duration {
        POLL_EVERY
    }

    fn list(&self, link: &Link) -> Result<Listing, ConnectorError> {
        let Link::Linear {
            team_id,
            project,
            label,
            ..
        } = link;
        let key = self.key()?;
        let agent_label = self.settings.get().connectors.linear_agent_label;
        let filter = IssueFilter::new(
            team_id,
            project.as_ref().map(|p| p.id.as_str()),
            label.as_deref(),
            &agent_label,
        );
        let mut listing = Listing::default();
        let mut after = None;
        for page in 1..=MAX_PAGES {
            let data: IssuesData = self.ask(
                &key,
                ISSUES,
                &IssuesVars {
                    filter: &filter,
                    after,
                },
            )?;
            listing
                .items
                .extend(data.issues.nodes.into_iter().filter_map(|issue| {
                    let labels: Vec<String> =
                        issue.labels.nodes.into_iter().map(|l| l.name).collect();
                    let assignee = board(issue.assignee.map(|a| a.is_me), &labels, &agent_label)?;
                    Some(Item {
                        external_id: issue.id,
                        text: truncate(issue.title.trim(), TEXT_MAX),
                        assignee,
                        source: TodoSource::Linear {
                            identifier: issue.identifier,
                            url: issue.url,
                        },
                    })
                }));
            let info = data.issues.page_info;
            if !info.has_next_page {
                return Ok(listing);
            }
            match info.end_cursor {
                Some(cursor) if page < MAX_PAGES => after = Some(cursor),
                _ => break,
            }
        }
        listing.truncated = true;
        Ok(listing)
    }

    fn tick(&self, link: &Link, external_id: &str) -> Result<(), ConnectorError> {
        let Link::Linear { team_id, .. } = link;
        let key = self.key()?;
        let state_id = self.completed_state(&key, team_id)?;
        let done = self
            .ask::<_, TickData>(
                &key,
                TICK,
                &TickVars {
                    id: external_id,
                    state_id: &state_id,
                },
            )
            .and_then(|d| {
                let update = d.issue_update;
                if update.success && update.issue.is_some_and(|i| i.state.kind == "completed") {
                    Ok(())
                } else {
                    Err(ConnectorError::Refused(
                        "Linear did not mark the issue as done".to_string(),
                    ))
                }
            });
        if matches!(done, Err(ConnectorError::Refused(_))) {
            // The status may not belong to the issue's team any more: ask again next time.
            lock(&self.state).completed.remove(team_id);
        }
        done
    }

    fn recheck(&self) {
        let mut state = lock(&self.state);
        state.key = match state.key.take() {
            Some(Key::Rejected(secret)) => Some(Key::Saved {
                secret,
                account: None,
            }),
            Some(Key::Unavailable(_)) => None,
            other => other,
        };
    }
}
