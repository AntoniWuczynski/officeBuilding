use super::*;
use crate::routing::fixtures::conn;
use crate::routing::observe::Breadth;
use serde_json::json;

const NOW: u64 = 1_000;

fn opus() -> Bucket {
    Bucket::Tier { tier: TierId::Opus }
}

fn sonnet() -> Bucket {
    Bucket::Tier {
        tier: TierId::Sonnet,
    }
}

fn model(name: &str) -> Bucket {
    Bucket::Model {
        model: name.to_string(),
    }
}

fn seen(reset_at: Option<u64>, window: Option<&str>) -> Exhaustion {
    Exhaustion {
        breadth: Breadth::Request,
        reset_at,
        window: window.map(str::to_string),
    }
}

/// Mark `bucket` on `id` at `NOW` with `reset_at`.
fn mark(a: &mut Availability, id: &str, bucket: Bucket, reset_at: Option<u64>) {
    a.mark(&conn(id), bucket, &seen(reset_at, None), NOW);
}

/// The tickets `gate` hands out, or a failure naming the row.
fn tickets(a: &Availability, id: &str, class: &Bucket, now: u64, row: &str) -> Vec<Ticket> {
    match a.gate(&conn(id), class, now) {
        Gate::Probe(tickets) => tickets,
        other => panic!("{row}: expected a probe, got {other:?}"),
    }
}

/// A gate as a label and a ticket count, so a table can name it.
fn label(gate: &Gate) -> (&'static str, usize) {
    match gate {
        Gate::Open => ("open", 0),
        Gate::Probe(tickets) => ("probe", tickets.len()),
        Gate::Probing => ("probing", 0),
        Gate::Shut => ("shut", 0),
    }
}

type State = (bool, Option<u64>, bool);

/// `(armed, last_probe_at, being checked)` of the one record of `id` in `bucket`.
fn state(a: &Availability, id: &str, bucket: &Bucket) -> State {
    let (record, probing, armed) = a
        .of(&conn(id), NOW)
        .into_iter()
        .find(|(record, _, _)| record.bucket == *bucket)
        .expect("record");
    (armed, record.last_probe_at, probing)
}

#[test]
fn covers() {
    let rows: [(&str, Bucket, &str, Bucket, bool); 9] = [
        ("whole covers a tier", Bucket::Whole, "sub", opus(), true),
        (
            "whole covers a model",
            Bucket::Whole,
            "sub",
            model("x"),
            true,
        ),
        (
            "whole covers whole",
            Bucket::Whole,
            "sub",
            Bucket::Whole,
            true,
        ),
        ("a tier covers itself", opus(), "sub", opus(), true),
        ("a tier not another", opus(), "sub", sonnet(), false),
        ("a tier not a model", opus(), "sub", model("x"), false),
        ("a model covers itself", model("x"), "sub", model("x"), true),
        ("a model not another", model("x"), "sub", model("y"), false),
        ("another connection", Bucket::Whole, "api", opus(), false),
    ];
    for (row, bucket, asked_on, class, covered) in rows {
        let mut a = Availability::default();
        mark(&mut a, "sub", bucket, None);
        let gate = a.gate(&conn(asked_on), &class, NOW);
        assert_eq!(gate != Gate::Open, covered, "{row}: {gate:?}");
    }
}

#[test]
fn gate() {
    type Setup = fn(&mut Availability);
    let rows: [(&str, Setup, Bucket, (&str, usize)); 10] = [
        ("no record", |_| {}, opus(), ("open", 0)),
        (
            "another bucket only",
            |a| mark(a, "sub", sonnet(), None),
            opus(),
            ("open", 0),
        ),
        (
            "no reset time",
            |a| mark(a, "sub", opus(), None),
            opus(),
            ("shut", 0),
        ),
        (
            "a reset to come",
            |a| mark(a, "sub", opus(), Some(NOW + 1)),
            opus(),
            ("shut", 0),
        ),
        (
            "a reset passed",
            |a| mark(a, "sub", opus(), Some(NOW - 1)),
            opus(),
            ("probe", 1),
        ),
        (
            "armed with no reset",
            |a| {
                mark(a, "sub", opus(), None);
                a.arm(&conn("sub"), NOW);
            },
            opus(),
            ("probe", 1),
        ),
        (
            "whole checkable, tier not",
            |a| {
                mark(a, "sub", Bucket::Whole, Some(NOW));
                mark(a, "sub", opus(), None);
            },
            opus(),
            ("shut", 0),
        ),
        (
            "whole and tier both checkable",
            |a| {
                mark(a, "sub", Bucket::Whole, Some(NOW));
                mark(a, "sub", opus(), Some(NOW - 10));
            },
            opus(),
            ("probe", 2),
        ),
        (
            "the tier is being checked",
            |a| {
                mark(a, "sub", opus(), Some(NOW));
                let t = tickets(a, "sub", &opus(), NOW, "setup");
                a.take(&t, 1, NOW);
            },
            opus(),
            ("probing", 0),
        ),
        (
            "whole being checked, tier not checkable",
            |a| {
                mark(a, "sub", Bucket::Whole, Some(NOW));
                let t = tickets(a, "sub", &sonnet(), NOW, "setup");
                a.take(&t, 1, NOW);
                mark(a, "sub", opus(), None);
            },
            opus(),
            ("probing", 0),
        ),
    ];
    for (row, setup, class, expected) in rows {
        let mut a = Availability::default();
        setup(&mut a);
        assert_eq!(label(&a.gate(&conn("sub"), &class, NOW)), expected, "{row}");
    }
}

#[test]
fn probe_tickets_carry_each_record_and_its_epoch() {
    let mut a = Availability::default();
    mark(&mut a, "sub", Bucket::Whole, Some(NOW));
    mark(&mut a, "sub", opus(), Some(NOW));
    let t = tickets(&a, "sub", &opus(), NOW, "two");
    assert_eq!(
        t.iter().map(|t| t.bucket.clone()).collect::<Vec<_>>(),
        vec![Bucket::Whole, opus()]
    );
    assert_eq!(t.iter().map(|t| t.epoch).collect::<Vec<_>>(), vec![1, 2]);
}

type DueRow = (&'static str, Option<u64>, Option<u64>, u64, bool);

#[test]
fn due() {
    let rows: [DueRow; 8] = [
        ("no reset time, ever", None, None, u64::MAX, false),
        ("no reset time, probed", None, Some(1), u64::MAX, false),
        ("a reset to come", Some(500), None, 499, false),
        ("the reset is now", Some(500), None, 500, true),
        ("a reset passed, no probe", Some(500), None, 900, true),
        ("a probe at the reset", Some(500), Some(500), 900, false),
        ("a probe after the reset", Some(500), Some(600), 900, false),
        (
            "a newer reset after the probe",
            Some(700),
            Some(600),
            900,
            true,
        ),
    ];
    for (row, reset_at, last_probe_at, now, expected) in rows {
        assert_eq!(super::due(reset_at, last_probe_at, now), expected, "{row}");
    }
}

#[test]
fn the_same_reset_reported_again_allows_no_further_check() {
    let mut a = Availability::default();
    mark(&mut a, "sub", opus(), Some(500));
    let t = tickets(&a, "sub", &opus(), NOW, "first check");
    a.take(&t, 1, NOW);
    // The check is refused with the same reset time.
    a.mark(&conn("sub"), opus(), &seen(Some(500), None), NOW + 1);
    a.spend(&t, 1);
    for later in [NOW + 1, NOW + 10_000, u64::MAX] {
        assert_eq!(a.gate(&conn("sub"), &opus(), later), Gate::Shut, "{later}");
    }
    // A later reset time allows one more.
    a.mark(&conn("sub"), opus(), &seen(Some(NOW + 50), None), NOW + 2);
    assert_eq!(a.gate(&conn("sub"), &opus(), NOW + 49), Gate::Shut);
    assert_eq!(
        label(&a.gate(&conn("sub"), &opus(), NOW + 50)),
        ("probe", 1)
    );
}

#[test]
fn a_reset_already_past_at_load_allows_exactly_one_check() {
    let saved = ExhaustionRecord {
        connection: conn("sub"),
        bucket: opus(),
        observed_at: 10,
        reset_at: Some(20),
        window: None,
        last_probe_at: None,
    };
    let mut a = Availability::load(vec![saved]);
    let t = tickets(&a, "sub", &opus(), NOW, "after load");
    a.take(&t, 7, NOW);
    assert_eq!(a.gate(&conn("sub"), &opus(), NOW), Gate::Probing);
    a.spend(&t, 7);
    assert_eq!(a.gate(&conn("sub"), &opus(), NOW + 1), Gate::Shut);
}

#[test]
fn clear_removes_a_record_only_for_its_lease_and_epoch() {
    let mut a = Availability::default();
    mark(&mut a, "sub", opus(), Some(NOW));
    let t = tickets(&a, "sub", &opus(), NOW, "take");
    a.take(&t, 1, NOW);

    a.clear(&t, 2);
    assert_eq!(a.records().len(), 1, "another lease");
    let mut stale = t.clone();
    stale[0].epoch += 100;
    a.clear(&stale, 1);
    assert_eq!(a.records().len(), 1, "another epoch");
    a.clear(&t, 1);
    assert!(a.records().is_empty(), "its own lease and epoch");

    // Newer evidence during the check renews the record, and the check's success leaves it.
    mark(&mut a, "sub", opus(), Some(NOW));
    let t = tickets(&a, "sub", &opus(), NOW, "take again");
    a.take(&t, 3, NOW);
    mark(&mut a, "sub", opus(), Some(NOW + 60));
    a.clear(&t, 3);
    assert_eq!(a.records().len(), 1, "renewed during the check");
}

#[test]
fn give_back_restores_and_spend_does_not() {
    type Release = fn(&mut Availability, &[Ticket], u64);
    let rows: [(&str, Release, State); 2] = [
        ("give_back", Availability::give_back, (true, Some(5), false)),
        ("spend", Availability::spend, (false, Some(NOW), false)),
    ];
    for (row, release, expected) in rows {
        let mut a = Availability::load(vec![ExhaustionRecord {
            connection: conn("sub"),
            bucket: opus(),
            observed_at: 1,
            reset_at: None,
            window: None,
            last_probe_at: Some(5),
        }]);
        a.arm(&conn("sub"), NOW);
        let t = tickets(&a, "sub", &opus(), NOW, row);
        a.take(&t, 1, NOW);
        assert_eq!(state(&a, "sub", &opus()), (false, Some(NOW), true), "{row}");
        release(&mut a, &t, 2);
        assert_eq!(
            state(&a, "sub", &opus()),
            (false, Some(NOW), true),
            "{row}: another lease changes nothing"
        );
        release(&mut a, &t, 1);
        assert_eq!(state(&a, "sub", &opus()), expected, "{row}");
    }
}

#[test]
fn a_check_older_than_the_lease_reads_as_given_back() {
    let mut a = Availability::default();
    mark(&mut a, "sub", opus(), Some(NOW));
    let first = tickets(&a, "sub", &opus(), NOW, "first");
    a.take(&first, 1, NOW);
    let expiry = NOW + PROBE_LEASE_SECS;
    assert_eq!(a.gate(&conn("sub"), &opus(), expiry - 1), Gate::Probing);
    let second = tickets(&a, "sub", &opus(), expiry, "after the lease");
    a.take(&second, 2, expiry);
    assert_eq!(a.gate(&conn("sub"), &opus(), expiry), Gate::Probing);
    a.clear(&first, 1);
    assert_eq!(a.records().len(), 1, "the stale lease no longer holds it");
    a.give_back(&second, 2);
    // Given back to the state before the first check: still due.
    assert_eq!(state(&a, "sub", &opus()), (false, None, false));
    assert_eq!(label(&a.gate(&conn("sub"), &opus(), expiry)), ("probe", 1));
}

#[test]
fn a_stale_check_keeps_an_armed_retry() {
    let mut a = Availability::default();
    mark(&mut a, "sub", opus(), None);
    a.arm(&conn("sub"), NOW);
    let t = tickets(&a, "sub", &opus(), NOW, "armed");
    a.take(&t, 1, NOW);
    assert_eq!(a.gate(&conn("sub"), &opus(), NOW + 1), Gate::Probing);
    assert_eq!(
        label(&a.gate(&conn("sub"), &opus(), NOW + PROBE_LEASE_SECS)),
        ("probe", 1)
    );
}

#[test]
fn mark_adds_and_renews() {
    let mut a = Availability::default();
    a.mark(
        &conn("sub"),
        opus(),
        &seen(Some(500), Some("seven_day")),
        400,
    );
    assert_eq!(
        a.records(),
        vec![ExhaustionRecord {
            connection: conn("sub"),
            bucket: opus(),
            observed_at: 400,
            reset_at: Some(500),
            window: Some("seven_day".to_string()),
            last_probe_at: None,
        }]
    );
    let t = tickets(&a, "sub", &opus(), 600, "probe");
    a.take(&t, 1, 600);
    a.spend(&t, 1);
    a.arm(&conn("sub"), NOW);
    a.mark(&conn("sub"), opus(), &seen(None, None), 700);
    assert_eq!(
        a.records(),
        vec![ExhaustionRecord {
            connection: conn("sub"),
            bucket: opus(),
            observed_at: 700,
            reset_at: None,
            window: None,
            last_probe_at: Some(600),
        }]
    );
    assert_eq!(state(&a, "sub", &opus()), (false, Some(600), false));
}

#[test]
fn mark_keeps_only_plain_window_names() {
    let forty = "a".repeat(WINDOW_MAX);
    let forty_one = "a".repeat(WINDOW_MAX + 1);
    let rows: [(&str, Option<&str>, Option<&str>); 8] = [
        ("plain", Some("five_hour"), Some("five_hour")),
        (
            "dashes and digits",
            Some("seven-day-2"),
            Some("seven-day-2"),
        ),
        ("forty characters", Some(&forty), Some(&forty)),
        ("too long", Some(&forty_one), None),
        ("a space", Some("five hour"), None),
        ("a slash", Some("five/hour"), None),
        ("empty", Some(""), None),
        ("none", None, None),
    ];
    for (row, given, kept) in rows {
        let mut a = Availability::default();
        a.mark(&conn("sub"), opus(), &seen(None, given), NOW);
        assert_eq!(a.records()[0].window.as_deref(), kept, "{row}");
    }
}

#[test]
fn arm_marks_every_record_of_the_connection_only() {
    let mut a = Availability::default();
    mark(&mut a, "sub", opus(), None);
    mark(&mut a, "sub", sonnet(), None);
    mark(&mut a, "api", Bucket::Whole, None);
    assert!(a.arm(&conn("sub"), NOW));
    assert!(a.of(&conn("sub"), NOW).iter().all(|(_, _, armed)| *armed));
    assert!(a.of(&conn("api"), NOW).iter().all(|(_, _, armed)| !*armed));
    assert!(!a.arm(&conn("router"), NOW));
}

#[test]
fn a_check_in_flight_is_not_saved() {
    let mut a = Availability::default();
    mark(&mut a, "sub", opus(), Some(NOW));
    let t = tickets(&a, "sub", &opus(), NOW, "take");
    a.take(&t, 1, NOW);
    let restarted = Availability::load(a.records());
    assert_eq!(
        label(&restarted.gate(&conn("sub"), &opus(), NOW + PROBE_LEASE_SECS)),
        ("probe", 1)
    );
}

#[test]
fn a_check_whose_lease_ran_out_shows_as_not_checking_and_can_be_armed() {
    let mut a = Availability::default();
    mark(&mut a, "sub", opus(), Some(NOW));
    let t = tickets(&a, "sub", &opus(), NOW, "take");
    a.take(&t, 1, NOW);
    let later = NOW + PROBE_LEASE_SECS;
    assert!(
        a.of(&conn("sub"), later)
            .iter()
            .all(|(_, checking, _)| !*checking)
    );
    assert!(a.arm(&conn("sub"), later));
    assert_eq!(state(&a, "sub", &opus()), (true, None, false));
}

#[test]
fn arm_leaves_a_record_being_checked() {
    let mut a = Availability::default();
    mark(&mut a, "sub", opus(), Some(NOW));
    let t = tickets(&a, "sub", &opus(), NOW, "take");
    a.take(&t, 1, NOW);
    assert!(a.arm(&conn("sub"), NOW));
    assert_eq!(state(&a, "sub", &opus()), (false, Some(NOW), true));
}

#[test]
fn clearing_one_record_leaves_the_others() {
    let mut a = Availability::default();
    mark(&mut a, "sub", opus(), Some(NOW));
    mark(&mut a, "sub", sonnet(), Some(NOW));
    mark(&mut a, "api", Bucket::Whole, Some(NOW));
    let others: Vec<ExhaustionRecord> = a
        .records()
        .into_iter()
        .filter(|r| r.bucket != opus())
        .collect();
    let t = tickets(&a, "sub", &opus(), NOW, "opus");
    a.take(&t, 1, NOW);
    a.clear(&t, 1);
    assert_eq!(a.records(), others);
    assert_eq!(label(&a.gate(&conn("sub"), &sonnet(), NOW)), ("probe", 1));
    assert_eq!(label(&a.gate(&conn("api"), &opus(), NOW)), ("probe", 1));
}

#[test]
fn keep_only_drops_unknown_connections() {
    let mut a = Availability::default();
    mark(&mut a, "sub", opus(), None);
    mark(&mut a, "api", Bucket::Whole, None);
    mark(&mut a, "router", Bucket::Whole, None);
    a.keep_only(&[conn("sub"), conn("router"), conn("oai")].into());
    assert_eq!(
        a.records()
            .iter()
            .map(|r| r.connection.as_str())
            .collect::<Vec<_>>(),
        vec!["router", "sub"]
    );
}

#[test]
fn records_are_ordered_by_connection_then_bucket() {
    let mut a = Availability::default();
    mark(&mut a, "sub", model("x"), None);
    mark(&mut a, "sub", sonnet(), None);
    mark(&mut a, "api", opus(), None);
    mark(&mut a, "sub", Bucket::Whole, None);
    mark(&mut a, "sub", opus(), None);
    let order: Vec<(String, Bucket)> = a
        .records()
        .into_iter()
        .map(|r| (r.connection.as_str().to_string(), r.bucket))
        .collect();
    assert_eq!(
        order,
        vec![
            ("api".to_string(), opus()),
            ("sub".to_string(), Bucket::Whole),
            ("sub".to_string(), opus()),
            ("sub".to_string(), sonnet()),
            ("sub".to_string(), model("x")),
        ]
    );
}

type Step = fn(&mut Availability);

/// A due record on `sub`, Opus.
fn marked(a: &mut Availability) {
    mark(a, "sub", opus(), Some(NOW));
}

/// The first record of a fresh `Availability`, as its ticket.
fn first_ticket() -> Vec<Ticket> {
    vec![Ticket {
        connection: conn("sub"),
        bucket: opus(),
        epoch: 1,
    }]
}

/// A due record on `sub`, Opus, taken by lease 1.
fn taken(a: &mut Availability) {
    marked(a);
    a.take(&first_ticket(), 1, NOW);
}

fn assert_revision(rows: &[(&str, Step, Step)], up: bool) {
    for (row, setup, step) in rows {
        let mut a = Availability::default();
        setup(&mut a);
        let before = a.revision();
        step(&mut a);
        let expected = if up { before + 1 } else { before };
        assert_eq!(a.revision(), expected, "{row}");
    }
}

#[test]
fn revision_goes_up_when_a_saved_field_changes() {
    assert_revision(
        &[
            ("mark", |_| {}, marked),
            ("spend", taken, |a| a.spend(&first_ticket(), 1)),
            ("a clear that removed something", taken, |a| {
                a.clear(&first_ticket(), 1);
            }),
            ("a keep_only that removed something", marked, |a| {
                a.keep_only(&BTreeSet::new());
            }),
        ],
        true,
    );
}

#[test]
fn revision_stays_when_nothing_saved_changes() {
    assert_revision(
        &[
            ("gate", marked, |a| {
                let _ = a.gate(&conn("sub"), &opus(), NOW);
            }),
            ("arm", marked, |a| {
                a.arm(&conn("sub"), NOW);
            }),
            ("take", marked, |a| a.take(&first_ticket(), 1, NOW)),
            ("give_back", taken, |a| a.give_back(&first_ticket(), 1)),
            ("a spend under another lease", taken, |a| {
                a.spend(&first_ticket(), 2);
            }),
            ("a clear that removed nothing", marked, |a| {
                a.clear(&first_ticket(), 1);
            }),
            ("a keep_only that removed nothing", marked, |a| {
                a.keep_only(&[conn("sub")].into());
            }),
            ("a take on a stale epoch", marked, |a| {
                let mut t = first_ticket();
                t[0].epoch += 1;
                a.take(&t, 1, NOW);
            }),
            ("a give_back under another lease", taken, |a| {
                a.give_back(&first_ticket(), 2);
            }),
        ],
        false,
    );
}

#[test]
fn bucket_and_record_json_is_pinned() {
    let rows = [
        ("whole", Bucket::Whole, json!({ "kind": "whole" })),
        ("tier", opus(), json!({ "kind": "tier", "tier": "opus" })),
        (
            "model",
            model("odd-1"),
            json!({ "kind": "model", "model": "odd-1" }),
        ),
    ];
    for (row, bucket, expected) in rows {
        assert_eq!(
            serde_json::to_value(&bucket).expect("json"),
            expected,
            "{row}"
        );
        assert_eq!(
            serde_json::from_value::<Bucket>(expected).expect("read"),
            bucket,
            "{row}"
        );
    }
    let record = ExhaustionRecord {
        connection: conn("sub"),
        bucket: Bucket::Tier {
            tier: TierId::Fable,
        },
        observed_at: 10,
        reset_at: Some(20),
        window: Some("seven_day_opus".to_string()),
        last_probe_at: Some(30),
    };
    let expected = json!({
        "connection": "sub",
        "bucket": { "kind": "tier", "tier": "fable" },
        "observedAt": 10,
        "resetAt": 20,
        "window": "seven_day_opus",
        "lastProbeAt": 30
    });
    assert_eq!(serde_json::to_value(&record).expect("json"), expected);
    let minimal = json!({ "connection": "sub", "bucket": { "kind": "whole" }, "observedAt": 10 });
    assert_eq!(
        serde_json::from_value::<ExhaustionRecord>(minimal).expect("read"),
        ExhaustionRecord {
            connection: conn("sub"),
            bucket: Bucket::Whole,
            observed_at: 10,
            reset_at: None,
            window: None,
            last_probe_at: None,
        }
    );
}
