//! The words of an alert and what to do with them. Pure: no I/O.

use super::{Alert, AlertKind};
use crate::settings::NotificationSettings;

/// The longest title shown, counted in characters.
const TITLE_MAX_CHARS: usize = 80;
const APP_TITLE: &str = "Office Building";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub title: String,
    pub body: String,
}

/// What to do for one batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub sound: bool,
    pub bounce: bool,
    pub notice: Option<Notice>,
}

fn short_title(title: &str) -> String {
    if title.is_empty() {
        return "An agent".to_owned();
    }
    if title.chars().count() <= TITLE_MAX_CHARS {
        return title.to_owned();
    }
    let mut cut: String = title.chars().take(TITLE_MAX_CHARS - 1).collect();
    cut.push('…');
    cut
}

fn single_body(alert: &Alert) -> String {
    let what = match alert.kind {
        AlertKind::NeedsYou => "Needs you",
        AlertKind::Stuck => "Stuck",
        AlertKind::Finished => "Finished its turn",
    };
    if alert.floor.is_empty() {
        format!("{what}.")
    } else {
        format!("{what} on the {} floor.", alert.floor)
    }
}

fn count_part(kind: AlertKind, n: usize) -> String {
    match (kind, n) {
        (AlertKind::NeedsYou, 1) => "1 agent needs you".to_owned(),
        (AlertKind::NeedsYou, _) => format!("{n} agents need you"),
        (AlertKind::Stuck, 1) => "1 agent is stuck".to_owned(),
        (AlertKind::Stuck, _) => format!("{n} agents are stuck"),
        (AlertKind::Finished, 1) => "1 agent finished its turn".to_owned(),
        (AlertKind::Finished, _) => format!("{n} agents finished their turn"),
    }
}

/// The text for one batch. The batch is not empty.
#[must_use]
pub fn notice(batch: &[Alert]) -> Notice {
    if let [alert] = batch {
        return Notice {
            title: short_title(&alert.title),
            body: single_body(alert),
        };
    }
    let parts: Vec<String> = [AlertKind::NeedsYou, AlertKind::Stuck, AlertKind::Finished]
        .into_iter()
        .filter_map(|kind| {
            let n = batch.iter().filter(|a| a.kind == kind).count();
            (n > 0).then(|| count_part(kind, n))
        })
        .collect();
    Notice {
        title: APP_TITLE.to_owned(),
        body: format!("{}.", parts.join(", ")),
    }
}

/// The text of the Settings panel's test alert.
#[must_use]
pub fn test_notice() -> Notice {
    Notice {
        title: APP_TITLE.to_owned(),
        body: "This is a test alert.".to_owned(),
    }
}

/// Which delivery methods to use for one batch.
#[must_use]
pub fn plan(notice: Notice, settings: &NotificationSettings, focused: bool) -> Plan {
    Plan {
        sound: settings.sound,
        bounce: settings.dock_bounce && !focused,
        notice: (settings.system_notification && !focused).then_some(notice),
    }
}
