//! Alerts when an agent stops: the pure tracker that decides what is due, its text and its
//! delivery plan, and the handle that delivers them.

use crate::model::SessionState;
use crate::settings::NotificationSettings;

mod deliver;
mod notice;
mod tracker;

pub use deliver::Alerts;
pub use notice::{Notice, Plan, notice, plan, test_notice};
pub use tracker::{SAMPLED_HOLD, Tracker};

/// What an alert is about. The order is the order in a merged alert's text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AlertKind {
    NeedsYou,
    Stuck,
    Finished,
}

/// One desk as the tracker sees it on one scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeskState {
    /// The session id.
    pub id: String,
    pub title: String,
    /// The project's name, empty if unknown.
    pub floor: String,
    pub state: SessionState,
    /// Its state is reported as it happens (a hook feed or a structured protocol), not sampled.
    pub hooked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alert {
    pub kind: AlertKind,
    pub desk_id: String,
    pub title: String,
    pub floor: String,
}

/// What the tracker needs to know about the human right now.
pub struct Look<'a> {
    pub settings: &'a NotificationSettings,
    pub focused: bool,
    pub open_desk: Option<&'a str>,
}

#[cfg(test)]
mod tests;
