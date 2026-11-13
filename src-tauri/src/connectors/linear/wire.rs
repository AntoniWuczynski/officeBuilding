//! Linear's GraphQL shapes: what is sent and what comes back. Linear owns the answers, so
//! unknown fields are skipped rather than refused.

use crate::model::LinearProject;
use serde::{Deserialize, Serialize};

/// A GraphQL request body. The query is a constant: every value travels in `variables`.
#[derive(Serialize)]
pub struct Body<'a, V: Serialize> {
    pub query: &'static str,
    pub variables: &'a V,
}

/// Variables for a query that takes none: serialised as `{}`.
#[derive(Serialize)]
pub struct NoVars {}

#[derive(Serialize)]
pub struct IssuesVars<'a> {
    pub filter: &'a IssueFilter,
    pub after: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatesVars<'a> {
    pub team_id: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TickVars<'a> {
    pub id: &'a str,
    pub state_id: &'a str,
}

/// Every clause must hold. Explicit, so nothing depends on how Linear joins siblings.
#[derive(Debug, PartialEq, Serialize)]
pub struct IssueFilter {
    and: Vec<Clause>,
}

/// One member of a filter, written as `{ "<field>": … }`.
#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
enum Clause {
    Team(IdIs),
    State(StateIs),
    Project(IdIs),
    Labels(NameIs),
    Assignee(AssigneeIs),
    Or(Vec<Clause>),
}

#[derive(Debug, PartialEq, Serialize)]
struct Equals<T> {
    eq: T,
}

#[derive(Debug, PartialEq, Serialize)]
struct IdIs {
    id: Equals<String>,
}

#[derive(Debug, PartialEq, Serialize)]
struct StateIs {
    #[serde(rename = "type")]
    kind: OneOf,
}

#[derive(Debug, PartialEq, Serialize)]
struct OneOf {
    #[serde(rename = "in")]
    values: [&'static str; 2],
}

#[derive(Debug, PartialEq, Serialize)]
struct NameIs {
    name: EqIgnoreCase,
}

#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct EqIgnoreCase {
    eq_ignore_case: String,
}

#[derive(Debug, PartialEq, Serialize)]
enum AssigneeIs {
    #[serde(rename = "isMe")]
    IsMe(Equals<bool>),
    #[serde(rename = "null")]
    Null(bool),
}

impl IssueFilter {
    /// Open issues of `team_id`, narrowed by an optional project and label, that are the key
    /// owner's, nobody's, or carry `agent_label`.
    #[must_use]
    pub fn new(
        team_id: &str,
        project_id: Option<&str>,
        label: Option<&str>,
        agent_label: &str,
    ) -> Self {
        let id = |value: &str| IdIs {
            id: Equals {
                eq: value.to_string(),
            },
        };
        let named = |value: &str| NameIs {
            name: EqIgnoreCase {
                eq_ignore_case: value.to_string(),
            },
        };
        let mut and = vec![
            Clause::Team(id(team_id)),
            Clause::State(StateIs {
                kind: OneOf {
                    values: ["unstarted", "started"],
                },
            }),
        ];
        and.extend(project_id.map(|p| Clause::Project(id(p))));
        and.extend(label.map(|l| Clause::Labels(named(l))));
        and.push(Clause::Or(vec![
            Clause::Assignee(AssigneeIs::IsMe(Equals { eq: true })),
            Clause::Assignee(AssigneeIs::Null(true)),
            Clause::Labels(named(agent_label)),
        ]));
        Self { and }
    }
}

/// Only the `errors` array of an answer.
#[derive(Deserialize)]
pub struct Errors {
    #[serde(default)]
    pub errors: Vec<GraphError>,
}

#[derive(Deserialize)]
pub struct GraphError {
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub extensions: Option<Extensions>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Extensions {
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
    #[serde(default)]
    pub user_presentable_message: Option<String>,
}

#[derive(Deserialize)]
pub struct Data<T> {
    pub data: T,
}

#[derive(Deserialize)]
pub struct Nodes<T> {
    pub nodes: Vec<T>,
}

#[derive(Deserialize)]
pub struct ViewerData {
    pub viewer: Viewer,
}

#[derive(Deserialize)]
pub struct Viewer {
    pub name: String,
}

#[derive(Deserialize)]
pub struct TeamsData {
    pub teams: Nodes<Team>,
}

#[derive(Deserialize)]
pub struct Team {
    pub id: String,
    pub key: String,
    pub name: String,
    pub projects: Nodes<LinearProject>,
}

#[derive(Deserialize)]
pub struct IssuesData {
    pub issues: IssuePage,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssuePage {
    pub nodes: Vec<Issue>,
    pub page_info: PageInfo,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageInfo {
    pub has_next_page: bool,
    pub end_cursor: Option<String>,
}

#[derive(Deserialize)]
pub struct Issue {
    pub id: String,
    pub identifier: String,
    pub title: String,
    pub url: String,
    pub assignee: Option<Assignee>,
    pub labels: Nodes<Label>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Assignee {
    pub is_me: bool,
}

#[derive(Deserialize)]
pub struct Label {
    pub name: String,
}

#[derive(Deserialize)]
pub struct StatesData {
    pub team: StatesTeam,
}

#[derive(Deserialize)]
pub struct StatesTeam {
    pub states: Nodes<WorkflowState>,
}

#[derive(Deserialize)]
pub struct WorkflowState {
    pub id: String,
    pub position: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TickData {
    pub issue_update: TickPayload,
}

#[derive(Deserialize)]
pub struct TickPayload {
    pub success: bool,
    pub issue: Option<TickIssue>,
}

#[derive(Deserialize)]
pub struct TickIssue {
    pub state: TickState,
}

#[derive(Deserialize)]
pub struct TickState {
    #[serde(rename = "type")]
    pub kind: String,
}
