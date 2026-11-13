use std::time::{Duration, Instant};

use super::*;
use crate::model::SessionState::{self, Done, Error, Idle, Running, Thinking, WaitingHuman};

fn desk(id: &str, state: SessionState, hooked: bool) -> DeskState {
    DeskState {
        id: id.to_owned(),
        title: format!("Desk {id}"),
        floor: "demo".to_owned(),
        state,
        hooked,
    }
}

fn look(settings: &NotificationSettings) -> Look<'_> {
    Look {
        settings,
        focused: false,
        open_desk: None,
    }
}

struct Clock(Instant);

impl Clock {
    fn new() -> Self {
        Self(Instant::now())
    }
    fn at(&self, secs: u64) -> Instant {
        self.0 + Duration::from_secs(secs)
    }
}

/// The kind and desk of each alert in a batch, in batch order.
fn got(batch: Vec<Alert>) -> Vec<(AlertKind, String)> {
    batch.into_iter().map(|a| (a.kind, a.desk_id)).collect()
}

fn one(kind: AlertKind, id: &str) -> Vec<(AlertKind, String)> {
    vec![(kind, id.to_owned())]
}

#[test]
fn first_sight_is_silent_in_every_state() {
    let settings = NotificationSettings::default();
    let clock = Clock::new();
    for state in [Error, WaitingHuman, Running, Thinking, Idle, Done] {
        for hooked in [true, false] {
            let mut tracker = Tracker::default();
            let scan = [desk("a", state, hooked)];
            assert!(
                tracker
                    .observe(&scan, &look(&settings), clock.at(0))
                    .is_empty()
            );
            assert_eq!(tracker.next_wake(clock.at(0), Duration::ZERO), None);
        }
    }
}

#[test]
fn a_hooked_desk_finishing_alerts_on_that_scan() {
    let settings = NotificationSettings::default();
    let clock = Clock::new();
    let mut tracker = Tracker::default();
    let _ = tracker.observe(&[desk("a", Running, true)], &look(&settings), clock.at(0));
    let batch = tracker.observe(&[desk("a", Idle, true)], &look(&settings), clock.at(1));
    assert_eq!(
        batch,
        vec![Alert {
            kind: AlertKind::Finished,
            desk_id: "a".to_owned(),
            title: "Desk a".to_owned(),
            floor: "demo".to_owned(),
        }]
    );
}

#[test]
fn a_sampled_desk_alerts_only_after_the_hold() {
    let settings = NotificationSettings::default();
    let clock = Clock::new();
    let mut tracker = Tracker::default();
    let l = look(&settings);
    let _ = tracker.observe(&[desk("a", Running, false)], &l, clock.at(0));
    let idle = [desk("a", Idle, false)];
    assert!(tracker.observe(&idle, &l, clock.at(10)).is_empty());
    assert!(tracker.observe(&idle, &l, clock.at(15)).is_empty());
    assert_eq!(
        got(tracker.observe(&idle, &l, clock.at(16))),
        one(AlertKind::Finished, "a")
    );
    assert_eq!(SAMPLED_HOLD, Duration::from_secs(6));
}

#[test]
fn a_flap_never_alerts_and_the_next_stop_needs_the_full_hold() {
    let settings = NotificationSettings::default();
    let clock = Clock::new();
    let mut tracker = Tracker::default();
    let l = look(&settings);
    let running = [desk("a", Running, false)];
    let idle = [desk("a", Idle, false)];
    let _ = tracker.observe(&running, &l, clock.at(0));
    assert!(tracker.observe(&idle, &l, clock.at(1)).is_empty());
    assert!(tracker.observe(&running, &l, clock.at(3)).is_empty());
    assert_eq!(tracker.next_wake(clock.at(3), Duration::ZERO), None);
    assert!(tracker.observe(&idle, &l, clock.at(4)).is_empty());
    // Seven seconds after the first idle, but only three after this one.
    assert!(tracker.observe(&idle, &l, clock.at(7)).is_empty());
    assert!(tracker.observe(&idle, &l, clock.at(9)).is_empty());
    assert_eq!(
        got(tracker.observe(&idle, &l, clock.at(10))),
        one(AlertKind::Finished, "a")
    );
}

#[test]
fn a_replaced_pending_state_settles_on_its_own_hold() {
    let settings = NotificationSettings::default();
    let clock = Clock::new();
    let mut tracker = Tracker::default();
    let l = look(&settings);
    let waiting = [desk("a", WaitingHuman, false)];
    let _ = tracker.observe(&[desk("a", Running, false)], &l, clock.at(0));
    assert!(
        tracker
            .observe(&[desk("a", Idle, false)], &l, clock.at(10))
            .is_empty()
    );
    assert!(tracker.observe(&waiting, &l, clock.at(13)).is_empty());
    assert!(tracker.observe(&waiting, &l, clock.at(18)).is_empty());
    assert_eq!(
        got(tracker.observe(&waiting, &l, clock.at(19))),
        one(AlertKind::NeedsYou, "a")
    );
}

#[test]
fn classify_follows_the_table() {
    let settings = NotificationSettings::default();
    let clock = Clock::new();
    let cases = [
        (Running, WaitingHuman, Some(AlertKind::NeedsYou)),
        (Idle, WaitingHuman, Some(AlertKind::NeedsYou)),
        (Running, Error, Some(AlertKind::Stuck)),
        (Idle, Error, Some(AlertKind::Stuck)),
        (Running, Idle, Some(AlertKind::Finished)),
        (Thinking, Idle, Some(AlertKind::Finished)),
        (WaitingHuman, Idle, None),
        (Error, Idle, None),
        (Done, Idle, None),
        (Running, Done, None),
        (WaitingHuman, Done, None),
        (Error, Done, None),
        (Idle, Running, None),
        (Idle, Thinking, None),
        (Running, Thinking, None),
    ];
    for (from, to, expected) in cases {
        let mut tracker = Tracker::default();
        let _ = tracker.observe(&[desk("a", from, true)], &look(&settings), clock.at(0));
        let batch = tracker.observe(&[desk("a", to, true)], &look(&settings), clock.at(1));
        let kinds: Vec<AlertKind> = batch.into_iter().map(|a| a.kind).collect();
        assert_eq!(
            kinds,
            expected.into_iter().collect::<Vec<_>>(),
            "{from:?} to {to:?}"
        );
    }
}

#[test]
fn an_event_switched_off_queues_nothing() {
    let settings = NotificationSettings {
        finished: false,
        ..NotificationSettings::default()
    };
    let clock = Clock::new();
    let mut tracker = Tracker::default();
    let l = look(&settings);
    let _ = tracker.observe(&[desk("a", Running, true)], &l, clock.at(0));
    assert!(
        tracker
            .observe(&[desk("a", Idle, true)], &l, clock.at(1))
            .is_empty()
    );
    assert_eq!(
        tracker.next_wake(clock.at(1), Duration::from_secs(10)),
        None
    );
}

#[test]
fn the_open_desk_is_quiet_only_while_the_window_is_focused() {
    let settings = NotificationSettings::default();
    let clock = Clock::new();
    let run = |focused: bool, open: &str| {
        let mut tracker = Tracker::default();
        let l = Look {
            settings: &settings,
            focused,
            open_desk: Some(open),
        };
        let _ = tracker.observe(&[desk("a", Running, true)], &l, clock.at(0));
        got(tracker.observe(&[desk("a", Idle, true)], &l, clock.at(1)))
    };
    assert!(run(true, "a").is_empty());
    assert_eq!(run(false, "a"), one(AlertKind::Finished, "a"));
    assert_eq!(run(true, "b"), one(AlertKind::Finished, "a"));
}

#[test]
fn the_cooldown_holds_later_alerts_and_merges_them() {
    let settings = NotificationSettings::default();
    let clock = Clock::new();
    let mut tracker = Tracker::default();
    let l = look(&settings);
    let mut scan = [
        desk("a", Running, true),
        desk("b", Running, true),
        desk("c", Running, true),
    ];
    let _ = tracker.observe(&scan, &l, clock.at(0));
    scan[0].state = Idle;
    assert_eq!(
        got(tracker.observe(&scan, &l, clock.at(1))),
        one(AlertKind::Finished, "a")
    );
    scan[1].state = WaitingHuman;
    assert!(tracker.observe(&scan, &l, clock.at(3)).is_empty());
    scan[2].state = Error;
    assert!(tracker.observe(&scan, &l, clock.at(8)).is_empty());
    assert!(tracker.observe(&scan, &l, clock.at(10)).is_empty());
    assert_eq!(
        got(tracker.observe(&scan, &l, clock.at(11))),
        vec![
            (AlertKind::NeedsYou, "b".to_owned()),
            (AlertKind::Stuck, "c".to_owned()),
        ]
    );
}

#[test]
fn a_flush_that_drops_everything_keeps_the_old_cooldown() {
    let settings = NotificationSettings::default();
    let clock = Clock::new();
    let mut tracker = Tracker::default();
    let l = look(&settings);
    let mut scan = [
        desk("a", Running, true),
        desk("b", Running, true),
        desk("c", Running, true),
    ];
    let _ = tracker.observe(&scan, &l, clock.at(0));
    scan[0].state = Idle;
    assert_eq!(tracker.observe(&scan, &l, clock.at(1)).len(), 1);
    scan[1].state = WaitingHuman;
    assert!(tracker.observe(&scan, &l, clock.at(2)).is_empty());
    // b is working again before the queue is flushed: its NeedsYou is stale.
    scan[1].state = Running;
    assert!(tracker.observe(&scan, &l, clock.at(5)).is_empty());
    assert!(tracker.observe(&scan, &l, clock.at(11)).is_empty());
    // Had the empty flush at 11 restarted the cooldown, this would be held.
    scan[2].state = Idle;
    assert_eq!(
        got(tracker.observe(&scan, &l, clock.at(12))),
        one(AlertKind::Finished, "c")
    );
}

#[test]
fn two_queued_alerts_for_one_desk_go_out_as_one() {
    let settings = NotificationSettings::default();
    let clock = Clock::new();
    let mut tracker = Tracker::default();
    let l = look(&settings);
    let mut scan = [desk("a", Running, true), desk("b", Running, true)];
    let _ = tracker.observe(&scan, &l, clock.at(0));
    scan[0].state = Idle;
    assert_eq!(tracker.observe(&scan, &l, clock.at(1)).len(), 1);
    for (secs, state) in [(2, WaitingHuman), (3, Running), (4, WaitingHuman)] {
        scan[1].state = state;
        assert!(tracker.observe(&scan, &l, clock.at(secs)).is_empty());
    }
    assert_eq!(
        got(tracker.observe(&scan, &l, clock.at(11))),
        one(AlertKind::NeedsYou, "b")
    );
}

#[test]
fn a_desk_that_leaves_loses_its_record_and_its_queued_alert() {
    let settings = NotificationSettings::default();
    let clock = Clock::new();
    let mut tracker = Tracker::default();
    let l = look(&settings);
    let a = desk("a", Idle, true);
    let _ = tracker.observe(
        &[desk("a", Running, true), desk("b", Running, true)],
        &l,
        clock.at(0),
    );
    assert_eq!(
        tracker
            .observe(&[a.clone(), desk("b", Running, true)], &l, clock.at(1))
            .len(),
        1
    );
    assert!(
        tracker
            .observe(&[a.clone(), desk("b", Idle, true)], &l, clock.at(2))
            .is_empty()
    );
    assert!(
        tracker
            .observe(std::slice::from_ref(&a), &l, clock.at(3))
            .is_empty()
    );
    let back = [a, desk("b", Idle, true)];
    assert!(tracker.observe(&back, &l, clock.at(4)).is_empty());
    assert!(tracker.observe(&back, &l, clock.at(11)).is_empty());
    assert_eq!(
        tracker.next_wake(clock.at(11), Duration::from_secs(10)),
        None
    );
}

#[test]
fn a_zero_cooldown_delivers_on_every_scan() {
    let settings = NotificationSettings {
        cooldown_secs: 0,
        ..NotificationSettings::default()
    };
    let clock = Clock::new();
    let mut tracker = Tracker::default();
    let l = look(&settings);
    let _ = tracker.observe(&[desk("a", Running, true)], &l, clock.at(0));
    for (state, kind) in [
        (Idle, AlertKind::Finished),
        (WaitingHuman, AlertKind::NeedsYou),
        (Error, AlertKind::Stuck),
    ] {
        assert_eq!(
            got(tracker.observe(&[desk("a", state, true)], &l, clock.at(1))),
            one(kind, "a")
        );
    }
}

#[test]
fn next_wake_finds_the_earliest_deadline() {
    let settings = NotificationSettings::default();
    let cooldown = Duration::from_secs(10);
    let clock = Clock::new();
    let mut tracker = Tracker::default();
    let l = look(&settings);
    assert_eq!(tracker.next_wake(clock.at(0), cooldown), None);

    let mut scan = [
        desk("a", Running, true),
        desk("b", Running, true),
        desk("c", Running, false),
    ];
    let _ = tracker.observe(&scan, &l, clock.at(0));
    assert_eq!(tracker.next_wake(clock.at(0), cooldown), None);

    // A pending hold alone: c idles at 2 and matures at 8.
    scan[2].state = Idle;
    let _ = tracker.observe(&scan, &l, clock.at(2));
    assert_eq!(
        tracker.next_wake(clock.at(4), cooldown),
        Some(Duration::from_secs(4))
    );

    // The hold matures at 8 and is delivered at once.
    assert_eq!(tracker.observe(&scan, &l, clock.at(8)).len(), 1);
    assert_eq!(tracker.next_wake(clock.at(8), cooldown), None);

    // Both waiting, the hold first: a is queued until 18, c runs again from 9 (matures at 15).
    scan[0].state = Idle;
    scan[2].state = Running;
    assert!(tracker.observe(&scan, &l, clock.at(9)).is_empty());
    assert_eq!(
        tracker.next_wake(clock.at(9), cooldown),
        Some(Duration::from_secs(6))
    );

    // A queued alert alone: c settles at 16, a still waits for 18.
    assert!(tracker.observe(&scan, &l, clock.at(16)).is_empty());
    assert_eq!(
        tracker.next_wake(clock.at(16), cooldown),
        Some(Duration::from_secs(2))
    );

    // Both waiting, the queue first: c idles again from 16 (matures at 22).
    scan[2].state = Idle;
    assert!(tracker.observe(&scan, &l, clock.at(16)).is_empty());
    assert_eq!(
        tracker.next_wake(clock.at(16), cooldown),
        Some(Duration::from_secs(2))
    );

    // Already due: zero.
    assert_eq!(
        tracker.next_wake(clock.at(30), cooldown),
        Some(Duration::ZERO)
    );
}

fn alert(kind: AlertKind, title: &str, floor: &str) -> Alert {
    Alert {
        kind,
        desk_id: "a".to_owned(),
        title: title.to_owned(),
        floor: floor.to_owned(),
    }
}

fn text(title: &str, body: &str) -> Notice {
    Notice {
        title: title.to_owned(),
        body: body.to_owned(),
    }
}

#[test]
fn one_alert_names_its_desk_and_floor() {
    for (kind, body) in [
        (AlertKind::NeedsYou, "Needs you on the demo floor."),
        (AlertKind::Stuck, "Stuck on the demo floor."),
        (AlertKind::Finished, "Finished its turn on the demo floor."),
    ] {
        assert_eq!(
            notice(&[alert(kind, "Fix login", "demo")]),
            text("Fix login", body)
        );
    }
    for (kind, body) in [
        (AlertKind::NeedsYou, "Needs you."),
        (AlertKind::Stuck, "Stuck."),
        (AlertKind::Finished, "Finished its turn."),
    ] {
        assert_eq!(
            notice(&[alert(kind, "Fix login", "")]),
            text("Fix login", body)
        );
    }
    assert_eq!(
        notice(&[alert(AlertKind::Stuck, "", "demo")]),
        text("An agent", "Stuck on the demo floor.")
    );
}

#[test]
fn a_long_title_is_cut_by_characters() {
    let eighty: String = "ż".repeat(80);
    assert_eq!(
        notice(&[alert(AlertKind::Stuck, &eighty, "")]).title,
        eighty
    );
    let long: String = "ż".repeat(81);
    let title = notice(&[alert(AlertKind::Stuck, &long, "")]).title;
    assert_eq!(title.chars().count(), 80);
    assert_eq!(title, format!("{}…", "ż".repeat(79)));
}

#[test]
fn a_merged_alert_counts_each_kind_in_a_fixed_order() {
    let batch = [
        alert(AlertKind::Finished, "x", "demo"),
        alert(AlertKind::NeedsYou, "y", "demo"),
        alert(AlertKind::NeedsYou, "z", ""),
    ];
    assert_eq!(
        notice(&batch),
        text(
            "Office Building",
            "2 agents need you, 1 agent finished its turn."
        )
    );
    let batch = [
        alert(AlertKind::Finished, "x", ""),
        alert(AlertKind::Stuck, "y", ""),
        alert(AlertKind::NeedsYou, "z", ""),
    ];
    assert_eq!(
        notice(&batch).body,
        "1 agent needs you, 1 agent is stuck, 1 agent finished its turn."
    );
    let batch = [
        alert(AlertKind::Stuck, "w", ""),
        alert(AlertKind::Finished, "x", ""),
        alert(AlertKind::Stuck, "y", ""),
        alert(AlertKind::Finished, "z", ""),
    ];
    assert_eq!(
        notice(&batch).body,
        "2 agents are stuck, 2 agents finished their turn."
    );
}

#[test]
fn the_test_notice_is_fixed() {
    assert_eq!(
        test_notice(),
        text("Office Building", "This is a test alert.")
    );
}

#[test]
fn the_plan_follows_focus_and_the_switches() {
    let all = NotificationSettings::default();
    let n = test_notice();
    assert_eq!(
        plan(n.clone(), &all, true),
        Plan {
            sound: true,
            bounce: false,
            notice: None
        }
    );
    assert_eq!(
        plan(n.clone(), &all, false),
        Plan {
            sound: true,
            bounce: true,
            notice: Some(n.clone())
        }
    );
    let no_sound = NotificationSettings {
        sound: false,
        ..all.clone()
    };
    assert!(!plan(n.clone(), &no_sound, false).sound);
    let no_bounce = NotificationSettings {
        dock_bounce: false,
        ..all.clone()
    };
    assert!(!plan(n.clone(), &no_bounce, false).bounce);
    let no_notice = NotificationSettings {
        system_notification: false,
        ..all
    };
    assert_eq!(plan(n, &no_notice, false).notice, None);
}

#[test]
fn a_feed_event_expiring_is_no_change_of_the_desk() {
    let settings = NotificationSettings::default();
    let clock = Clock::new();
    let mut tracker = Tracker::default();
    let l = look(&settings);
    let _ = tracker.observe(&[desk("a", Idle, true)], &l, clock.at(0));
    let sampled = [desk("a", WaitingHuman, false)];
    assert!(tracker.observe(&sampled, &l, clock.at(1)).is_empty());
    assert!(tracker.observe(&sampled, &l, clock.at(20)).is_empty());
    assert_eq!(tracker.next_wake(clock.at(20), Duration::ZERO), None);
}
