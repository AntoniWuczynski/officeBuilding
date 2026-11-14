use super::*;
use crate::routing::capabilities::Capability;
use crate::routing::config::TierId;
use crate::routing::fixtures::{conn, credentials, ctx, settings};
use crate::routing::observe::Exhaustion;
use serde_json::json;

const T0: u64 = 100_000;
const OPUS: &str = "claude-opus-5-5";
const HAIKU: &str = "claude-haiku-5-5";

type Edit = fn(&mut RoutingSettings);
/// A switch row: name, edit before open, edit after, credential taken away, target, result.
type SwitchRow = (
    &'static str,
    Edit,
    Edit,
    Option<&'static str>,
    &'static str,
    Result<(), RouteError>,
);

fn rid(id: &str) -> RouteId {
    RouteId::new(id.to_string())
}

fn req(model: &str, effort: Option<&str>, capabilities: &[Capability]) -> Requirements {
    Requirements::new(model, effort, capabilities.iter().copied().collect()).expect("valid request")
}

fn ask(model: &str) -> Requirements {
    req(model, None, &[])
}

fn spent(breadth: Breadth, reset_at: Option<u64>, window: Option<&str>) -> Observation {
    Observation::Exhausted(Exhaustion {
        breadth,
        reset_at,
        window: window.map(str::to_string),
    })
}

/// A whole connection used up, with no reset time.
fn whole() -> Observation {
    spent(Breadth::Whole, None, None)
}

/// The refused request's own bucket used up, with no reset time.
fn bucket_spent() -> Observation {
    spent(Breadth::Request, None, None)
}

fn record(id: &str, bucket: Bucket, reset_at: Option<u64>) -> ExhaustionRecord {
    ExhaustionRecord {
        connection: conn(id),
        bucket,
        observed_at: T0 - 100,
        reset_at,
        window: None,
        last_probe_at: None,
    }
}

fn whole_record(id: &str) -> ExhaustionRecord {
    record(id, Bucket::Whole, None)
}

fn tier(tier: TierId) -> Bucket {
    Bucket::Tier { tier }
}

fn blocked(reasons: &[(&str, Skip)]) -> RouteError {
    RouteError::Blocked {
        reasons: reasons
            .iter()
            .map(|(id, skip)| (conn(id), skip.clone()))
            .collect(),
    }
}

fn refused(id: &str, reason: Skip) -> RouteError {
    RouteError::Refused {
        connection: conn(id),
        reason,
    }
}

fn on(lease: &Lease) -> &str {
    lease.connection.as_str()
}

fn again(verdict: Verdict) -> Lease {
    match verdict {
        Verdict::Again { lease, .. } => lease,
        other => panic!("expected Again, got {other:?}"),
    }
}

/// A verdict without the lease's details, for tables.
#[derive(Debug, PartialEq, Eq)]
enum Shape {
    Done,
    Again(String, u64),
    Relay,
    Local(LocalFault),
    Cut,
}

fn again_on(id: &str, wait_ms: u64) -> Shape {
    Shape::Again(id.to_string(), wait_ms)
}

fn shape(verdict: &Verdict) -> Shape {
    match verdict {
        Verdict::Done => Shape::Done,
        Verdict::Again { lease, wait_ms } => Shape::Again(on(lease).to_string(), *wait_ms),
        Verdict::Relay => Shape::Relay,
        Verdict::Local(fault) => Shape::Local(*fault),
        Verdict::Cut => Shape::Cut,
    }
}

/// A `RouteState` on the fixture, counting every lease cut: each one is an upstream call.
struct Rig {
    state: RouteState,
    credentials: BTreeSet<ConnectionId>,
    cut: usize,
}

impl Rig {
    fn new(records: Vec<ExhaustionRecord>) -> Self {
        Self {
            state: RouteState::new(records),
            credentials: credentials(),
            cut: 0,
        }
    }

    fn open(&mut self, id: &str, context: &str) -> RouteId {
        self.open_in(id, &settings(), context)
    }

    fn open_in(&mut self, id: &str, s: &RoutingSettings, context: &str) -> RouteId {
        let route = SessionRoute::open(s, &ctx(context)).expect("fixture context opens");
        self.state.open(rid(id), route);
        rid(id)
    }

    fn begin(
        &mut self,
        id: &RouteId,
        request: Requirements,
        now: u64,
    ) -> Result<Lease, RouteError> {
        let got = self.state.begin(id, request, &self.credentials, now);
        if got.is_ok() {
            self.cut += 1;
        }
        got
    }

    fn ask(&mut self, id: &RouteId, now: u64) -> Result<Lease, RouteError> {
        self.begin(id, ask(OPUS), now)
    }

    fn lease(&mut self, id: &RouteId, now: u64) -> Lease {
        self.ask(id, now).expect("a lease")
    }

    fn settle(&mut self, lease: Lease, seen: &Observation, relayed: bool, now: u64) -> Verdict {
        let verdict = self
            .state
            .settle(lease, seen, relayed, &self.credentials, now);
        if matches!(verdict, Verdict::Again { .. }) {
            self.cut += 1;
        }
        verdict
    }

    fn switch(&mut self, id: &RouteId, to: &str, now: u64) -> Result<(), RouteError> {
        self.state.switch(id, &conn(to), &self.credentials, now)
    }

    fn status(&self, id: &RouteId) -> RouteStatus {
        self.state
            .status(id, &self.credentials, T0)
            .expect("an open session")
    }

    /// The session's active connection and generation.
    fn at(&self, id: &RouteId) -> (Option<String>, u64) {
        let status = self.status(id);
        (
            status.active.map(|c| c.as_str().to_string()),
            status.generation,
        )
    }

    fn records(&self) -> Vec<ExhaustionRecord> {
        self.state.availability.records()
    }

    /// Run one Opus request through the work chain, exhausting each entry it lands on
    /// (the subscription by its request's bucket, the API kinds whole). The connections it
    /// was sent to, and the last verdict.
    fn exhaust_work(&mut self, id: &RouteId) -> (Vec<String>, Verdict) {
        let mut lease = self.lease(id, T0);
        let mut sent = Vec::new();
        for _ in 0..10 {
            sent.push(on(&lease).to_string());
            let seen = if lease.kind.passes_through() {
                bucket_spent()
            } else {
                whole()
            };
            match self.settle(lease, &seen, false, T0) {
                Verdict::Again { lease: next, .. } => lease = next,
                last => return (sent, last),
            }
        }
        panic!("a chain of four entries ran more than ten attempts");
    }
}

fn every_work_entry(reason: &Skip) -> RouteError {
    blocked(&["sub", "api", "router", "oai"].map(|id| (id, reason.clone())))
}

// 1
#[test]
fn the_first_entry_exhausted_moves_the_session_on() {
    let mut rig = Rig::new(Vec::new());
    let id = rig.open("a", "work");
    let lease = rig.lease(&id, T0);
    assert_eq!((on(&lease), lease.generation), ("sub", 1));
    assert_eq!(
        rig.status(&id).last_switch,
        None,
        "a first choice is not a switch"
    );

    let next = again(rig.settle(lease, &bucket_spent(), false, T0 + 1));
    assert_eq!((on(&next), next.generation), ("api", 2));
    assert_eq!(rig.at(&id), (Some("api".into()), 2));
    assert_eq!(
        rig.status(&id).last_switch,
        Some(Switch {
            from: conn("sub"),
            to: conn("api"),
            reason: SwitchReason::Exhausted,
            at: T0 + 1,
        })
    );
    assert_eq!(rig.cut, 2);
}

// 2
#[test]
fn every_entry_exhausted_in_turn_ends_blocked() {
    let mut rig = Rig::new(Vec::new());
    let id = rig.open("a", "work");
    let (sent, last) = rig.exhaust_work(&id);
    assert_eq!(sent, ["sub", "api", "router", "oai"]);
    assert_eq!(last, Verdict::Relay, "the last refusal is the client's");
    assert_eq!(
        rig.at(&id),
        (Some("oai".into()), 4),
        "one move per exhaustion"
    );
    assert_eq!(
        rig.ask(&id, T0 + 1),
        Err(every_work_entry(&Skip::Exhausted))
    );
    assert_eq!(rig.status(&id).blocked_since, Some(T0));
}

// 3
#[test]
fn a_blocked_chain_calls_nothing_for_any_session() {
    let mut rig = Rig::new(Vec::new());
    let first = rig.open("a", "work");
    let second = rig.open("b", "work");
    rig.exhaust_work(&first);
    let third = rig.open("c", "work");
    let calls = rig.cut;
    for id in [&first, &second, &third] {
        for i in 0..50 {
            let got = rig.begin(id, ask(OPUS), T0 + i);
            assert_eq!(got, Err(every_work_entry(&Skip::Exhausted)), "{id:?} #{i}");
        }
    }
    assert_eq!(rig.cut, calls, "no upstream call after the block");
}

// 4
#[test]
fn a_restart_from_the_saved_records_stays_blocked() {
    let mut rig = Rig::new(Vec::new());
    let id = rig.open("a", "work");
    rig.exhaust_work(&id);
    let saved = rig.state.unsaved().expect("records to save");

    let mut restarted = Rig::new(saved.records);
    let id = restarted.open("a", "work");
    assert_eq!(
        restarted.ask(&id, T0 + 60),
        Err(every_work_entry(&Skip::Exhausted))
    );
    assert_eq!(restarted.cut, 0);
}

// 5
#[test]
fn a_paid_entry_is_reached_only_with_its_opt_in() {
    #[rustfmt::skip]
    let rows = [
        ("paid off", false, Shape::Relay),
        ("paid on", true, again_on("api", 0)),
    ];
    for (row, paid, expected) in rows {
        let mut s = settings();
        s.contexts[0].chain = vec![conn("sub"), conn("api")];
        s.connections[1].paid = paid;
        let mut rig = Rig::new(Vec::new());
        let id = rig.open_in("a", &s, "work");
        let lease = rig.lease(&id, T0);
        let verdict = rig.settle(lease, &bucket_spent(), false, T0);
        assert_eq!(shape(&verdict), expected, "{row}");
        if !paid {
            assert_eq!(
                rig.ask(&id, T0 + 1),
                Err(blocked(&[
                    ("sub", Skip::Exhausted),
                    ("api", Skip::PaidNotAllowed)
                ])),
                "{row}"
            );
            assert_eq!(rig.cut, 1, "{row}: no lease ever names api");
        }
    }
}

// 6
#[test]
fn simultaneous_failures_on_one_generation_move_the_session_once() {
    let mut rig = Rig::new(Vec::new());
    let id = rig.open("a", "work");
    let parent = rig.lease(&id, T0);
    let child = rig.begin(&id, ask(HAIKU), T0).expect("a lease");
    assert_eq!((parent.generation, child.generation), (1, 1));

    let first = again(rig.settle(parent, &bucket_spent(), false, T0 + 1));
    let second = again(rig.settle(child, &bucket_spent(), false, T0 + 1));
    assert_eq!((on(&first), first.generation), ("api", 2));
    assert_eq!((on(&second), second.generation), ("api", 2));
    assert_eq!(rig.at(&id), (Some("api".into()), 2), "moved exactly once");
    assert_eq!(rig.records().len(), 2, "both refusals are evidence");
}

// 7
#[test]
fn a_late_success_clears_nothing_and_moves_nothing() {
    let mut rig = Rig::new(Vec::new());
    let id = rig.open("a", "work");
    let early = rig.lease(&id, T0);
    let failing = rig.lease(&id, T0);
    again(rig.settle(failing, &bucket_spent(), false, T0 + 1));
    let records = rig.records();

    let verdict = rig.settle(early, &Observation::Completed, true, T0 + 2);
    assert_eq!(verdict, Verdict::Done);
    assert_eq!(rig.records(), records, "the record stays");
    assert_eq!(rig.at(&id), (Some("api".into()), 2));
}

// 8
#[test]
fn a_stale_generation_cannot_move_the_session() {
    let mut rig = Rig::new(Vec::new());
    let id = rig.open("a", "work");
    let old = rig.lease(&id, T0);
    rig.switch(&id, "api", T0 + 1).expect("switch");
    rig.switch(&id, "sub", T0 + 2).expect("switch back");
    assert_eq!(rig.at(&id), (Some("sub".into()), 3));

    let verdict = rig.settle(old, &bucket_spent(), false, T0 + 3);
    assert_eq!(verdict, Verdict::Relay);
    assert_eq!(rig.records().len(), 1, "the refusal is recorded");
    assert_eq!(rig.at(&id), (Some("sub".into()), 3), "generation unchanged");
    assert_eq!(rig.cut, 1);
}

// 9
#[test]
fn what_is_not_exhaustion_moves_nothing() {
    use ProblemKind as P;
    #[rustfmt::skip]
    let rows = [
        (Observation::Throttled { retry_after: None }, again_on("sub", RETRY_WAIT_MS), Some(P::Throttled)),
        (Observation::Faulted { retry_after: None }, again_on("sub", RETRY_WAIT_MS), Some(P::Faulted)),
        (Observation::Unauthorised, Shape::Relay, Some(P::Unauthorised)),
        (Observation::Forbidden, Shape::Relay, Some(P::Forbidden)),
        (Observation::Rejected, Shape::Relay, Some(P::Rejected)),
        (Observation::NotSent, again_on("sub", RETRY_WAIT_MS), Some(P::Unreachable)),
        (Observation::Dropped, Shape::Local(LocalFault::Interrupted), Some(P::Interrupted)),
        (Observation::Cancelled, Shape::Done, None),
    ];
    for (seen, expected, problem) in rows {
        let mut rig = Rig::new(Vec::new());
        let id = rig.open("a", "work");
        let lease = rig.lease(&id, T0);
        assert_eq!(rig.state.unsaved(), None, "{seen:?}: nothing before");
        let verdict = rig.settle(lease, &seen, false, T0 + 1);
        assert_eq!(shape(&verdict), expected, "{seen:?}");
        assert_eq!(rig.state.unsaved(), None, "{seen:?}: no record");
        assert_eq!(rig.at(&id), (Some("sub".into()), 1), "{seen:?}");
        let noted = rig.status(&id).entries[0].problem;
        assert_eq!(
            noted,
            problem.map(|kind| Problem { kind, at: T0 + 1 }),
            "{seen:?}"
        );
    }
}

// 10
#[test]
fn transient_failures_are_retried_at_most_once_within_the_budget() {
    let throttled = |retry_after| Observation::Throttled { retry_after };
    let unreachable = Shape::Local(LocalFault::Unreachable);
    #[rustfmt::skip]
    let rows = [
        ("throttled twice", 1, vec![(throttled(None), again_on("sub", 500)), (throttled(None), Shape::Relay)]),
        ("retry-after 2", 1, vec![(throttled(Some(2)), again_on("sub", 2_000)), (throttled(Some(2)), Shape::Relay)]),
        ("retry-after 6", 1, vec![(throttled(Some(6)), Shape::Relay)]),
        ("30 seconds late", ATTEMPT_WINDOW_SECS, vec![(throttled(None), Shape::Relay)]),
        ("not sent twice", 1, vec![(Observation::NotSent, again_on("sub", 500)), (Observation::NotSent, unreachable)]),
    ];
    for (row, after, steps) in rows {
        let mut rig = Rig::new(Vec::new());
        let id = rig.open("a", "work");
        let mut lease = Some(rig.lease(&id, T0));
        for (seen, expected) in steps {
            let current = lease.take().expect("a lease to settle");
            let verdict = rig.settle(current, &seen, false, T0 + after);
            assert_eq!(shape(&verdict), expected, "{row}");
            if let Verdict::Again { lease: next, .. } = verdict {
                lease = Some(next);
            }
        }
        assert!(lease.is_none(), "{row}: the request ends");
        assert!(rig.cut <= 2, "{row}: {} calls", rig.cut);
        assert_eq!(rig.at(&id), (Some("sub".into()), 1), "{row}");
    }
}

// 11
#[test]
fn nothing_is_retried_or_moved_once_a_byte_is_relayed() {
    #[rustfmt::skip]
    let rows = [
        (Observation::Completed, Shape::Done),
        (bucket_spent(), Shape::Cut),
        (Observation::Throttled { retry_after: None }, Shape::Cut),
        (Observation::Faulted { retry_after: None }, Shape::Cut),
        (Observation::Unauthorised, Shape::Cut),
        (Observation::Forbidden, Shape::Cut),
        (Observation::Rejected, Shape::Cut),
        (Observation::NotSent, Shape::Cut),
        (Observation::Dropped, Shape::Cut),
        (Observation::Cancelled, Shape::Done),
    ];
    for (seen, expected) in rows {
        let mut rig = Rig::new(Vec::new());
        let id = rig.open("a", "work");
        let lease = rig.lease(&id, T0);
        let verdict = rig.settle(lease, &seen, true, T0 + 1);
        assert_eq!(shape(&verdict), expected, "{seen:?}");
        assert_eq!(rig.cut, 1, "{seen:?}: no further lease");
        assert_eq!(rig.at(&id), (Some("sub".into()), 1), "{seen:?}");
    }

    // Exhausted after relay: the record is written and the next request moves the session.
    let mut rig = Rig::new(Vec::new());
    let id = rig.open("a", "work");
    let lease = rig.lease(&id, T0);
    assert_eq!(rig.settle(lease, &bucket_spent(), true, T0), Verdict::Cut);
    assert_eq!(
        rig.records(),
        [{
            let mut r = record("sub", tier(TierId::Opus), None);
            r.observed_at = T0;
            r
        }]
    );
    let next = rig.lease(&id, T0 + 1);
    assert_eq!((on(&next), next.generation), ("api", 2));

    // A dropped connection with nothing relayed is never replayed.
    let mut rig = Rig::new(Vec::new());
    let id = rig.open("a", "work");
    let lease = rig.lease(&id, T0);
    let verdict = rig.settle(lease, &Observation::Dropped, false, T0);
    assert_eq!(verdict, Verdict::Local(LocalFault::Interrupted));
    assert_eq!(rig.cut, 1);
}

// 12
#[test]
fn cancellation_undoes_nothing_and_gives_a_check_back() {
    let mut rig = Rig::new(vec![whole_record("api")]);
    let id = rig.open("a", "work");
    let lease = rig.lease(&id, T0);
    assert!(!lease.is_probe());
    assert_eq!(
        rig.settle(lease, &Observation::Cancelled, false, T0),
        Verdict::Done
    );
    assert_eq!(rig.records(), [whole_record("api")]);
    assert_eq!(rig.at(&id), (Some("sub".into()), 1));

    let mut rig = Rig::new(vec![whole_record("sub")]);
    let id = rig.open("a", "work");
    assert!(rig.state.retry(&conn("sub"), T0));
    let probe = rig.lease(&id, T0);
    assert!(probe.is_probe() && on(&probe) == "sub");
    assert_eq!(
        rig.settle(probe, &Observation::Cancelled, false, T0),
        Verdict::Done
    );
    let scope = &rig.status(&id).entries[0].exhausted[0];
    assert!(
        scope.retry_armed && !scope.checking,
        "the retry is still armed"
    );
}

/// A home session whose only connection has a record with a reset time that has passed.
fn due_home() -> (Rig, RouteId) {
    let mut rig = Rig::new(vec![record("home-api", Bucket::Whole, Some(T0 - 10))]);
    let id = rig.open("h", "home");
    (rig, id)
}

// 13
#[test]
fn a_passed_reset_allows_one_coordinated_check() {
    let (mut rig, id) = due_home();
    let probe = rig.lease(&id, T0);
    assert!(probe.is_probe());
    assert_eq!(
        rig.ask(&id, T0),
        Err(blocked(&[("home-api", Skip::Exhausted)]))
    );
    assert_eq!(rig.cut, 1);
    assert_eq!(
        rig.settle(probe, &Observation::Completed, true, T0 + 5),
        Verdict::Done
    );
    assert_eq!(rig.records(), []);
    assert_eq!(rig.at(&id), (Some("home-api".into()), 1));
}

// 14
#[test]
fn stale_reset_data_allows_no_further_check() {
    #[rustfmt::skip]
    let rows = [
        ("the same reset again", T0 - 10, None),
        ("a later reset", T0 + 100, Some(T0 + 100)),
    ];
    for (row, reset, allows_at) in rows {
        let (mut rig, id) = due_home();
        let probe = rig.lease(&id, T0);
        let seen = spent(Breadth::Whole, Some(reset), None);
        assert_eq!(
            rig.settle(probe, &seen, false, T0 + 1),
            Verdict::Relay,
            "{row}"
        );
        for now in [T0 + 2, T0 + 99, T0 + 10_000] {
            if allows_at.is_some_and(|at| now >= at) {
                continue;
            }
            assert!(rig.ask(&id, now).is_err(), "{row}: at {now}");
        }
        assert_eq!(rig.cut, 1, "{row}: nothing by itself");
        if let Some(at) = allows_at {
            assert!(rig.lease(&id, at).is_probe(), "{row}");
            assert!(rig.ask(&id, at).is_err(), "{row}: only one");
            assert_eq!(rig.cut, 2, "{row}");
        }
    }
}

// 15
#[test]
fn clearing_one_record_leaves_unrelated_ones() {
    let sonnet = record("sub", tier(TierId::Sonnet), Some(T0 + 500));
    let home = whole_record("home-api");
    let mut rig = Rig::new(vec![
        record("sub", tier(TierId::Opus), Some(T0 - 10)),
        sonnet.clone(),
        home.clone(),
    ]);
    let id = rig.open("a", "work");
    let probe = rig.lease(&id, T0);
    assert!(probe.is_probe() && on(&probe) == "sub");
    rig.settle(probe, &Observation::Completed, false, T0 + 1);
    assert_eq!(rig.records(), [home, sonnet]);
}

// 16
#[test]
fn an_explicit_retry_lets_exactly_one_check_through() {
    let mut rig = Rig::new(vec![whole_record("home-api")]);
    let id = rig.open("h", "home");
    assert!(rig.ask(&id, T0).is_err());
    assert!(rig.state.retry(&conn("home-api"), T0));
    assert!(rig.lease(&id, T0 + 1).is_probe());
    assert!(rig.ask(&id, T0 + 1).is_err());
    assert_eq!(rig.cut, 1);

    let mut rig = Rig::new(vec![whole_record("sub")]);
    let id = rig.open("a", "work");
    assert_eq!(on(&rig.lease(&id, T0)), "api");
    assert!(rig.state.retry(&conn("sub"), T0));
    for i in 1..4 {
        assert_eq!(on(&rig.lease(&id, T0 + i)), "api", "request {i}");
    }
    assert_eq!(rig.at(&id), (Some("api".into()), 1));
}

// 17
#[test]
fn a_recovered_entry_never_pulls_a_healthy_session_back() {
    let mut rig = Rig::new(vec![whole_record("sub")]);
    let healthy = rig.open("a", "work");
    assert_eq!(on(&rig.lease(&healthy, T0)), "api");
    rig.state.retry(&conn("sub"), T0);
    let other = rig.open("b", "work");
    let probe = rig.lease(&other, T0 + 1);
    assert!(probe.is_probe() && on(&probe) == "sub");
    rig.settle(probe, &Observation::Completed, false, T0 + 2);
    assert_eq!(rig.records(), []);
    for i in 3..6 {
        assert_eq!(on(&rig.lease(&healthy, T0 + i)), "api", "request {i}");
    }
    assert_eq!(rig.at(&healthy), (Some("api".into()), 1));
}

// 18
#[test]
fn an_explicit_switch_reaches_only_entries_with_standing() {
    let no_edit: Edit = |_| {};
    #[rustfmt::skip]
    let rows: [SwitchRow; 7] = [
        ("healthy", no_edit, no_edit, None, "api", Ok(())),
        ("unknown session", no_edit, no_edit, None, "api", Err(RouteError::UnknownSession)),
        ("not in chain", no_edit, no_edit, None, "home-api", Err(RouteError::NotInChain)),
        ("paid not allowed", |s| s.connections[1].paid = false, no_edit, None, "api", Err(refused("api", Skip::PaidNotAllowed))),
        ("experimental not allowed", |s| s.connections[3].experimental = false, no_edit, None, "oai", Err(refused("oai", Skip::ExperimentalNotAllowed))),
        ("no credential", no_edit, no_edit, Some("router"), "router", Err(refused("router", Skip::NoCredential))),
        ("removed", no_edit, |s| s.connections.retain(|c| c.id != conn("api")), None, "api", Err(refused("api", Skip::Removed))),
    ];
    for (row, before, after, without, to, expected) in rows {
        let mut s = settings();
        before(&mut s);
        let mut rig = Rig::new(Vec::new());
        let id = rig.open_in("a", &s, "work");
        rig.lease(&id, T0);
        after(&mut s);
        rig.state.apply_preferences(&s);
        if let Some(gone) = without {
            rig.credentials.remove(&conn(gone));
        }
        let target = if row == "unknown session" {
            rid("nobody")
        } else {
            id.clone()
        };
        assert_eq!(rig.switch(&target, to, T0 + 1), expected, "{row}");
        let status = rig.status(&id);
        if expected.is_ok() {
            assert_eq!(rig.at(&id), (Some(to.into()), 2), "{row}");
            let reason = status.last_switch.map(|s| s.reason);
            assert_eq!(reason, Some(SwitchReason::Manual), "{row}");
        } else {
            assert_eq!(rig.at(&id), (Some("sub".into()), 1), "{row}: nothing moved");
        }
    }

    // To an exhausted entry: the next request is the check, a sibling detours, and a failed
    // check moves the session on by the ordinary rule.
    let mut rig = Rig::new(vec![whole_record("api")]);
    let id = rig.open("a", "work");
    rig.lease(&id, T0);
    rig.switch(&id, "api", T0 + 1).expect("switch");
    let probe = rig.lease(&id, T0 + 2);
    assert!(probe.is_probe() && on(&probe) == "api");
    let sibling = rig.lease(&id, T0 + 2);
    assert!(!sibling.is_probe() && on(&sibling) == "sub");
    assert_eq!(
        rig.at(&id),
        (Some("api".into()), 2),
        "the detour does not move it"
    );
    let next = again(rig.settle(probe, &whole(), false, T0 + 3));
    assert_eq!((on(&next), next.generation), ("sub", 3));
    let reason = rig.status(&id).last_switch.map(|s| s.reason);
    assert_eq!(reason, Some(SwitchReason::Exhausted));
}

// 19
#[test]
fn a_request_the_active_entry_cannot_serve_is_refused_without_a_move() {
    let mut rig = Rig::new(Vec::new());
    let id = rig.open("a", "work");
    rig.lease(&id, T0);
    rig.switch(&id, "oai", T0).expect("switch");
    let document = req(OPUS, None, &[Capability::Documents]);
    assert_eq!(
        rig.begin(&id, document, T0 + 1),
        Err(refused(
            "oai",
            Skip::Unsupported {
                capability: Capability::Documents
            }
        ))
    );
    assert_eq!(rig.at(&id), (Some("oai".into()), 2));
    let next = rig.lease(&id, T0 + 2);
    assert_eq!(on(&next), "oai");
    assert_eq!(
        next.target,
        Target::Rewrite {
            model: "gpt-5.6-sol".into(),
            effort: None
        }
    );
}

// 20
#[test]
fn changed_preferences_narrow_sessions_and_drop_gone_records() {
    type Check = fn(&mut Rig, &RouteId);
    #[rustfmt::skip]
    let rows: [(&str, Edit, Check); 4] = [
        ("paid withdrawn", |s| s.connections[1].paid = false, |rig, id| {
            assert_eq!(rig.ask(id, T0 + 1), Err(refused("api", Skip::PaidNotAllowed)));
            assert_eq!(rig.at(id), (Some("api".into()), 1));
        }),
        ("connection removed", |s| s.connections.retain(|c| c.id != conn("router")), |rig, id| {
            assert_eq!(rig.records(), [whole_record("sub")]);
            let router = &rig.status(id).entries[2];
            assert_eq!(router.state, EntryState::Unavailable { reason: Skip::Removed });
            assert!(router.exhausted.is_empty());
        }),
        ("chain reordered", |s| s.contexts[0].chain.reverse(), |rig, id| {
            let order: Vec<String> = rig.status(id).entries.iter().map(|e| e.connection.as_str().to_string()).collect();
            assert_eq!(order, ["sub", "api", "router", "oai"]);
            assert_eq!(on(&rig.lease(id, T0 + 1)), "api");
        }),
        ("another connection removed", |s| s.connections.retain(|c| c.id != conn("oai")), |rig, _| {
            assert_eq!(rig.records(), [whole_record("router"), whole_record("sub")]);
        }),
    ];
    for (row, edit, check) in rows {
        let mut rig = Rig::new(vec![whole_record("sub"), whole_record("router")]);
        let id = rig.open("a", "work");
        assert_eq!(on(&rig.lease(&id, T0)), "api", "{row}");
        let mut s = settings();
        edit(&mut s);
        rig.state.apply_preferences(&s);
        check(&mut rig, &id);
    }
}

// 21
#[test]
fn contexts_stay_apart_through_fallback_recovery_and_restart() {
    let work_records = ["api", "oai", "router", "sub"].map(whole_record);
    let mut seeded = work_records.to_vec();
    seeded.push(whole_record("home-api"));
    let mut rig = Rig::new(seeded);
    let work = rig.open("w", "work");
    let home = rig.open("h", "home");
    assert_eq!(rig.ask(&work, T0), Err(every_work_entry(&Skip::Exhausted)));

    assert!(rig.state.retry(&conn("home-api"), T0));
    let probe = rig.lease(&home, T0 + 1);
    assert_eq!(on(&probe), "home-api");
    rig.settle(probe, &Observation::Completed, false, T0 + 2);
    assert_eq!(
        rig.records(),
        work_records.to_vec(),
        "no work record changed"
    );
    assert_eq!(
        rig.ask(&work, T0 + 3),
        Err(every_work_entry(&Skip::Exhausted))
    );

    let saved = rig.state.unsaved().expect("records to save");
    let mut restarted = Rig::new(saved.records);
    let work = restarted.open("w", "work");
    let home = restarted.open("h", "home");
    assert_eq!(
        restarted.ask(&work, T0 + 4),
        Err(every_work_entry(&Skip::Exhausted))
    );
    assert_eq!(on(&restarted.lease(&home, T0 + 4)), "home-api");
    assert_eq!(restarted.cut, 1);
}

// 22
#[test]
fn the_status_of_a_blocked_session_is_pinned() {
    let mut s = settings();
    s.connections[3].paid = false;
    let mut rig = Rig::new(Vec::new());
    rig.credentials.remove(&conn("router"));
    let id = rig.open_in("a", &s, "work");
    let lease = rig
        .begin(&id, req(OPUS, Some("high"), &[]), T0)
        .expect("a lease");
    let five_hour = spent(Breadth::Request, Some(T0 + 3600), Some("five_hour"));
    let next = again(rig.settle(lease, &five_hour, false, T0));
    assert_eq!(rig.settle(next, &whole(), false, T0), Verdict::Relay);
    assert!(rig.ask(&id, T0 + 1).is_err());

    let json = serde_json::to_value(rig.status(&id)).expect("serialises");
    let whole_scope = json!({ "bucket": { "kind": "whole" }, "window": null, "observedAt": T0, "resetAt": null, "checking": false, "retryArmed": false });
    let entry = |id: &str, kind: &str, state, exhausted| json!({ "connection": id, "name": id, "kind": kind, "state": state, "exhausted": exhausted, "problem": null });
    let expected = json!({
        "context": "work",
        "generation": 2,
        "active": "api",
        "blockedSince": T0,
        "lastSwitch": { "from": "sub", "to": "api", "reason": "exhausted", "at": T0 },
        "lastServed": {
            "connection": "api",
            "requestedModel": OPUS,
            "requestedEffort": "high",
            "model": OPUS,
            "effort": "high",
            "at": T0,
        },
        "entries": [
            entry("sub", "claude-subscription", json!({ "kind": "unavailable", "reason": { "kind": "exhausted" } }), json!([{
                "bucket": { "kind": "tier", "tier": "opus" },
                "window": "five_hour",
                "observedAt": T0,
                "resetAt": T0 + 3600,
                "checking": false,
                "retryArmed": false,
            }])),
            entry("api", "anthropic-api-key", json!({ "kind": "active" }), json!([whole_scope])),
            entry("router", "openrouter", json!({ "kind": "unavailable", "reason": { "kind": "no-credential" } }), json!([])),
            entry("oai", "openai-api-key", json!({ "kind": "unavailable", "reason": { "kind": "paid-not-allowed" } }), json!([])),
        ],
    });
    assert_eq!(json, expected);

    // A healthy session on a rewriting entry: Ready, and what was asked for against what was sent.
    let mut rig = Rig::new(Vec::new());
    let id = rig.open("a", "work");
    rig.lease(&id, T0);
    rig.switch(&id, "oai", T0).expect("switch");
    rig.begin(&id, req(OPUS, Some("xhigh"), &[]), T0 + 1)
        .expect("a lease");
    let status = rig.status(&id);
    assert_eq!(
        status.last_served,
        Some(Served {
            connection: conn("oai"),
            requested_model: OPUS.into(),
            requested_effort: Some("xhigh".into()),
            model: Some("gpt-5.6-sol".into()),
            effort: Some("high".into()),
            at: T0 + 1,
        })
    );
    let entry_states: Vec<EntryState> = status.entries.into_iter().map(|e| e.state).collect();
    assert_eq!(
        entry_states,
        [
            EntryState::Ready,
            EntryState::Ready,
            EntryState::Ready,
            EntryState::Active
        ]
    );
}

// 23
#[test]
fn unknown_and_closed_sessions() {
    let mut rig = Rig::new(Vec::new());
    assert_eq!(rig.ask(&rid("nobody"), T0), Err(RouteError::UnknownSession));
    assert_eq!(rig.state.status(&rid("nobody"), &rig.credentials, T0), None);

    // A probe settled after close gives its ticket back: the retry is armed again.
    let mut rig = Rig::new(vec![whole_record("home-api")]);
    let id = rig.open("h", "home");
    rig.state.retry(&conn("home-api"), T0);
    let probe = rig.lease(&id, T0);
    assert!(probe.is_probe());
    rig.state.close(&id);
    assert_eq!(
        rig.settle(probe, &Observation::Completed, false, T0 + 1),
        Verdict::Done
    );
    assert_eq!(rig.records(), [whole_record("home-api")], "not cleared");
    assert!(
        rig.state.availability.of(&conn("home-api"), T0)[0].2,
        "armed again"
    );

    // An exhaustion settled after close still writes its record.
    let mut rig = Rig::new(Vec::new());
    let id = rig.open("a", "work");
    let lease = rig.lease(&id, T0);
    rig.state.close(&id);
    assert_eq!(rig.settle(lease, &whole(), false, T0), Verdict::Done);
    let saved = rig.state.unsaved().expect("the record is saved");
    assert_eq!(saved.records.len(), 1);
    assert_eq!(rig.ask(&id, T0), Err(RouteError::UnknownSession));
}

// 24
#[test]
fn unsaved_hands_out_each_change_until_it_is_written() {
    let mut rig = Rig::new(Vec::new());
    assert_eq!(rig.state.unsaved(), None, "nothing at start");
    let id = rig.open("a", "work");
    let lease = rig.lease(&id, T0);
    rig.settle(lease, &bucket_spent(), true, T0);
    let first = rig.state.unsaved().expect("a change");
    assert_eq!(
        rig.state.unsaved().as_ref(),
        Some(&first),
        "offered again until the write is confirmed"
    );
    rig.state.saved(first.revision);
    assert_eq!(rig.state.unsaved(), None, "straight after");

    let lease = rig.lease(&id, T0 + 1);
    rig.settle(lease, &whole(), true, T0 + 1);
    let second = rig.state.unsaved().expect("another change");
    assert!(second.revision > first.revision);
    assert_eq!(second.records.len(), 2);
    rig.state.saved(second.revision);
    assert_eq!(rig.state.unsaved(), None);
}
