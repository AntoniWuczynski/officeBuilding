use super::*;
use crate::routing::config::ConnectionKind;
use crate::routing::fixtures::{conn, credentials, ctx, settings};
use crate::routing::observe::{Breadth, Exhaustion};

/// A plan as (entry id, target, moves).
type Planned = Result<(String, Target, bool), RouteError>;
type Fitted = Result<Target, Skip>;
type Opened = Result<Vec<&'static str>, OpenError>;

const NOW: u64 = 10_000;
const CLAUDE: [&str; 4] = [
    "claude-fable-5-2",
    "claude-opus-5-5",
    "claude-sonnet-5-5",
    "claude-haiku-5-5",
];
const OPENAI: [&str; 4] = [
    "gpt-6-astra",
    "gpt-5.6-sol",
    "gpt-5.6-terra",
    "gpt-5.6-luna",
];
const OPUS: &str = "claude-opus-5-5";
const HAIKU: &str = "claude-haiku-5-5";

fn route(context: &str) -> SessionRoute {
    SessionRoute::open(&settings(), &ctx(context)).expect("fixture context opens")
}

fn index(route: &SessionRoute, id: &str) -> usize {
    route
        .entries
        .iter()
        .position(|e| e.connection == conn(id))
        .expect("entry in the chain")
}

fn entry<'a>(route: &'a mut SessionRoute, id: &str) -> &'a mut Entry {
    let i = index(route, id);
    &mut route.entries[i]
}

fn req(model: &str, effort: Option<&str>, capabilities: &[Capability]) -> Requirements {
    Requirements::new(model, effort, capabilities.iter().copied().collect()).expect("valid request")
}

fn ask(model: &str) -> Requirements {
    req(model, None, &[])
}

fn no_target(tier: TierId) -> Skip {
    Skip::NoTarget { tier }
}

fn effort_unmapped(tier: TierId, effort: &str) -> Skip {
    Skip::EffortUnmapped {
        tier,
        effort: effort.to_string(),
    }
}

fn unmapped(model: &str) -> Skip {
    Skip::UnmappedModel {
        model: model.to_string(),
    }
}

fn unsupported(capability: Capability) -> Skip {
    Skip::Unsupported { capability }
}

fn fresh() -> Attempt {
    Attempt {
        visited: Vec::new(),
        retries: 0,
        started_at: NOW,
    }
}

fn rewrite(model: &str, effort: Option<&str>) -> Target {
    Target::Rewrite {
        model: model.to_string(),
        effort: effort.map(str::to_string),
    }
}

fn whole(reset_at: Option<u64>) -> Exhaustion {
    Exhaustion {
        breadth: Breadth::Whole,
        reset_at,
        window: None,
    }
}

/// What the records say about one connection.
#[derive(Debug, Clone, Copy)]
enum State {
    Open,
    Shut,
    Armed,
    Probing,
}

const STATES: [State; 4] = [State::Open, State::Shut, State::Armed, State::Probing];

fn put(a: &mut Availability, id: &str, state: State) {
    let id = conn(id);
    if matches!(state, State::Open) {
        return;
    }
    a.mark(&id, Bucket::Whole, &whole(None), NOW);
    if matches!(state, State::Shut) {
        return;
    }
    a.arm(&id, NOW);
    if let State::Probing = state {
        let Gate::Probe(tickets) = a.gate(&id, &Bucket::Whole, NOW) else {
            panic!("an armed record is checkable");
        };
        a.take(&tickets, 7, NOW);
    }
}

fn shut(ids: &[&str]) -> Availability {
    let mut a = Availability::default();
    for id in ids {
        put(&mut a, id, State::Shut);
    }
    a
}

/// Everything a call to `plan_attempt` reads, with the fixture's defaults.
struct Case {
    route: SessionRoute,
    active: Option<usize>,
    needs: Needs,
    availability: Availability,
    attempt: Attempt,
    credentials: BTreeSet<ConnectionId>,
}

impl Case {
    fn new(context: &str, active: Option<&str>) -> Self {
        let route = route(context);
        let active = active.map(|id| index(&route, id));
        Self {
            route,
            active,
            needs: Needs::default(),
            availability: Availability::default(),
            attempt: fresh(),
            credentials: credentials(),
        }
    }

    fn plan(&self, request: &Requirements) -> Result<AttemptPlan, RouteError> {
        let session = SessionView {
            active: self.active,
            needs: &self.needs,
        };
        plan_attempt(
            &self.route,
            &session,
            &self.availability,
            request,
            &self.attempt,
            &self.credentials,
            NOW,
        )
    }

    /// The plan's entry id, target and `moves`, for a compact comparison.
    fn planned(&self, request: &Requirements) -> Planned {
        self.plan(request).map(|p| {
            (
                self.route.entries[p.entry].connection.as_str().to_string(),
                p.target,
                p.moves,
            )
        })
    }
}

fn blocked(reasons: &[(&str, Skip)]) -> RouteError {
    RouteError::Blocked {
        reasons: reasons
            .iter()
            .map(|(id, skip)| (conn(id), skip.clone()))
            .collect(),
    }
}

fn ids(route: &SessionRoute) -> Vec<&str> {
    route
        .entries
        .iter()
        .map(|e| e.connection.as_str())
        .collect()
}

#[test]
fn open_copies_the_contexts_own_chain() {
    type Edit = fn(&mut RoutingSettings);
    #[rustfmt::skip]
    let rows: [(&str, Edit, &str, Opened); 6] = [
        ("work", |_| {}, "work", Ok(vec!["sub", "api", "router", "oai"])),
        ("home", |_| {}, "home", Ok(vec!["home-api"])),
        ("switch off", |s| s.enabled = false, "work", Err(OpenError::Off)),
        ("unknown context", |_| {}, "nowhere", Err(OpenError::UnknownContext)),
        ("empty chain", |s| s.contexts[1].chain.clear(), "home", Err(OpenError::EmptyChain)),
        ("missing, repeated and foreign entries are skipped", |s| { s.contexts[0].chain = ["ghost", "sub", "api", "sub", "home-api"].map(conn).to_vec(); }, "work", Ok(vec!["sub", "api"])),
    ];
    for (row, edit, context, expected) in rows {
        let mut s = settings();
        edit(&mut s);
        let got = SessionRoute::open(&s, &ctx(context));
        assert_eq!(got.as_ref().map(ids).map_err(|e| *e), expected, "{row}");
        if let Ok(route) = got {
            assert_eq!(route.tiers, s.tiers, "{row}: tiers copied");
            for e in &route.entries {
                let c = s.connection(&e.connection).expect("copied from settings");
                assert_eq!(c.context, ctx(context), "{row}: {e:?} is its own context's");
                assert_eq!(
                    (
                        &e.name,
                        e.kind,
                        e.paid,
                        e.experimental,
                        e.removed,
                        &e.targets
                    ),
                    (&c.name, c.kind, c.paid, c.experimental, false, &c.targets),
                    "{row}: {e:?} is a copy"
                );
            }
        }
    }
}

#[test]
fn narrow_only_takes_away() {
    type Edit = fn(&mut RoutingSettings);
    type Expect = fn(&mut SessionRoute);
    #[rustfmt::skip]
    let rows: [(&str, Edit, Edit, Expect); 9] = [
        ("connection removed", |_| {}, |s| s.connections.retain(|c| c.id != conn("api")), |r| entry(r, "api").removed = true),
        ("kind changed", |_| {}, |s| s.connections[1].kind = ConnectionKind::OpenRouter, |r| entry(r, "api").removed = true),
        ("context changed", |_| {}, |s| s.connections[1].context = ctx("home"), |r| entry(r, "api").removed = true),
        ("paid withdrawn", |_| {}, |s| s.connections[1].paid = false, |r| entry(r, "api").paid = false),
        ("experimental withdrawn", |_| {}, |s| s.connections[3].experimental = false, |r| entry(r, "oai").experimental = false),
        ("paid granted later is not gained", |s| s.connections[1].paid = false, |s| s.connections[1].paid = true, |_| {}),
        ("switch off changes nothing", |_| {}, |s| s.enabled = false, |_| {}),
        ("reordered chain changes nothing", |_| {}, |s| s.contexts[0].chain.reverse(), |_| {}),
        ("renamed and retargeted changes nothing", |_| {}, |s| { s.connections[1].name = "Renamed".into(); s.connections[1].targets.opus = None; }, |_| {}),
    ];
    for (row, before, after, expect) in rows {
        let mut s = settings();
        before(&mut s);
        let mut route = SessionRoute::open(&s, &ctx("work")).expect("opens");
        let mut expected = route.clone();
        expect(&mut expected);
        after(&mut s);
        route.narrow(&s);
        assert_eq!(route, expected, "{row}");
    }

    let mut route = route("work");
    let mut s = settings();
    s.connections.retain(|c| c.id != conn("api"));
    route.narrow(&s);
    route.narrow(&settings());
    assert!(
        entry(&mut route, "api").removed,
        "a removed entry never comes back"
    );
}

#[test]
fn class_of_is_the_tier_or_the_model() {
    let route = route("work");
    assert_eq!(
        route.class_of("gpt-5.6-sol"),
        Bucket::Tier { tier: TierId::Opus }
    );
    assert_eq!(
        route.class_of("some-other-model"),
        Bucket::Model {
            model: "some-other-model".into()
        }
    );
}

#[test]
fn the_four_tiers_map_in_both_directions() {
    for (i, tier) in TierId::ALL.into_iter().enumerate() {
        let rows = [("oai", CLAUDE[i], OPENAI[i]), ("api", OPENAI[i], CLAUDE[i])];
        for (on, asked, sent) in rows {
            let got = Case::new("work", Some(on)).planned(&ask(asked));
            assert_eq!(
                got,
                Ok((on.to_string(), rewrite(sent, None), false)),
                "{tier:?}: {asked} on {on}"
            );
        }
    }
}

#[test]
fn openrouter_sends_its_explicit_name_and_never_another_tiers() {
    for (i, tier) in TierId::ALL.into_iter().enumerate() {
        let case = Case::new("work", Some("router"));
        let sent = format!("anthropic/{}", CLAUDE[i]);
        assert_eq!(
            case.planned(&ask(OPENAI[i])),
            Ok(("router".into(), rewrite(&sent, None), false)),
            "{tier:?}"
        );

        let mut case = Case::new("work", Some("router"));
        *entry(&mut case.route, "router").targets.get_mut(tier) = None;
        assert_eq!(
            case.plan(&ask(CLAUDE[i])),
            Err(RouteError::Refused {
                connection: conn("router"),
                reason: Skip::NoTarget { tier }
            }),
            "{tier:?} with its target removed"
        );
    }
}

#[test]
fn the_subscription_sends_everything_as_sent() {
    let case = Case::new("work", Some("sub"));
    let requests = [
        req(OPUS, Some("max"), &[]),
        req("gpt-6-astra", None, &[Capability::Images]),
        req("a-model-in-no-tier", None, &[Capability::ServerTools]),
    ];
    for request in requests {
        assert_eq!(
            case.planned(&request),
            Ok(("sub".into(), Target::AsSent, false)),
            "{request:?}"
        );
    }
}

#[test]
fn efforts_map_by_pair_or_pass_on_the_same_api() {
    let mut route = route("work");
    if let Some(target) = entry(&mut route, "api").targets.opus.as_mut() {
        target.efforts.insert("max".into(), "xhigh".into());
    }
    #[rustfmt::skip]
    let rows: [(&str, &str, Option<&str>, Fitted); 6] = [
        ("kept on api with no pair", "api", Some("high"), Ok(rewrite(OPUS, Some("high")))),
        ("replaced by a pair on api", "api", Some("max"), Ok(rewrite(OPUS, Some("xhigh")))),
        ("none asked on api", "api", None, Ok(rewrite(OPUS, None))),
        ("none asked on oai", "oai", None, Ok(rewrite("gpt-5.6-sol", None))),
        ("xhigh on oai by its pair", "oai", Some("xhigh"), Ok(rewrite("gpt-5.6-sol", Some("high")))),
        ("max on oai has no pair", "oai", Some("max"), Err(effort_unmapped(TierId::Opus, "max"))),
    ];
    for (row, on, effort, expected) in rows {
        let request = req(OPUS, effort, &[]);
        let class = route.class_of(&request.model);
        let e = &route.entries[index(&route, on)];
        assert_eq!(fit(e, &class, &request, &credentials()), expected, "{row}");
    }
}

#[test]
fn fit_reports_the_first_check_that_fails() {
    type Edit = fn(&mut Entry);
    let none: Edit = |_| {};
    let document = [Capability::Documents];
    #[rustfmt::skip]
    let rows: [(&str, &str, Edit, bool, Requirements, Fitted); 15] = [
        ("fits", "oai", none, true, req(OPUS, None, &[Capability::Tools]), Ok(rewrite("gpt-5.6-sol", None))),
        ("removed", "api", |e| e.removed = true, true, ask(OPUS), Err(Skip::Removed)),
        ("removed before not paid", "api", |e| { e.removed = true; e.paid = false; }, true, ask(OPUS), Err(Skip::Removed)),
        ("not paid", "api", |e| e.paid = false, true, ask(OPUS), Err(Skip::PaidNotAllowed)),
        ("not paid before not experimental", "oai", |e| { e.paid = false; e.experimental = false; }, true, ask(OPUS), Err(Skip::PaidNotAllowed)),
        ("not experimental", "oai", |e| e.experimental = false, true, ask(OPUS), Err(Skip::ExperimentalNotAllowed)),
        ("not experimental before no credential", "oai", |e| e.experimental = false, false, ask(OPUS), Err(Skip::ExperimentalNotAllowed)),
        ("no credential", "api", none, false, ask(OPUS), Err(Skip::NoCredential)),
        ("no credential before unmapped model", "api", none, false, ask("in-no-tier"), Err(Skip::NoCredential)),
        ("unmapped model", "api", none, true, ask("in-no-tier"), Err(unmapped("in-no-tier"))),
        ("unmapped model before unsupported", "oai", none, true, req("in-no-tier", None, &document), Err(unmapped("in-no-tier"))),
        ("no target", "api", |e| e.targets.haiku = None, true, ask(HAIKU), Err(no_target(TierId::Haiku))),
        ("no target before unsupported", "oai", |e| e.targets.haiku = None, true, req(HAIKU, None, &document), Err(no_target(TierId::Haiku))),
        ("effort unmapped before unsupported", "oai", none, true, req(OPUS, Some("max"), &document), Err(effort_unmapped(TierId::Opus, "max"))),
        ("unsupported", "oai", none, true, req(OPUS, None, &[Capability::Tools, Capability::Images, Capability::Documents]), Err(unsupported(Capability::Documents))),
    ];
    for (row, on, edit, credentialed, request, expected) in rows {
        let mut route = route("work");
        edit(entry(&mut route, on));
        let class = route.class_of(&request.model);
        let creds = if credentialed {
            credentials()
        } else {
            BTreeSet::new()
        };
        let e = &route.entries[index(&route, on)];
        assert_eq!(fit(e, &class, &request, &creds), expected, "{row}");
        assert_eq!(
            standing(e, &creds).is_ok(),
            !matches!(
                expected,
                Err(Skip::Removed
                    | Skip::PaidNotAllowed
                    | Skip::ExperimentalNotAllowed
                    | Skip::NoCredential)
            ),
            "{row}: standing is checks 1 to 4"
        );
    }
}

#[test]
fn a_childs_request_is_planned_for_its_own_tier() {
    let mut case = Case::new("work", Some("oai"));
    let fable = ask("claude-fable-5-2");
    case.needs.note(&case.route.class_of(&fable.model), &fable);
    assert_eq!(
        case.planned(&ask(HAIKU)),
        Ok(("oai".into(), rewrite("gpt-5.6-luna", None), false))
    );
    assert_eq!(
        case.planned(&fable),
        Ok(("oai".into(), rewrite("gpt-6-astra", None), false))
    );
}

#[test]
fn a_healthy_active_entry_is_the_answer() {
    for (row, active) in [
        ("on api", "api"),
        ("on router", "router"),
        ("on oai", "oai"),
    ] {
        let case = Case::new("work", Some(active));
        let got = case.plan(&ask(OPUS)).map(|p| (p.entry, p.tickets, p.moves));
        assert_eq!(
            got,
            Ok((index(&case.route, active), Vec::new(), false)),
            "{row}: no switch back to sub"
        );
    }
}

#[test]
fn a_request_the_active_entry_cannot_serve_is_refused() {
    type Edit = fn(&mut Case);
    let none: Edit = |_| {};
    #[rustfmt::skip]
    let rows: [(&str, Edit, Requirements, Skip); 6] = [
        ("a document", none, req(OPUS, None, &[Capability::Documents]), unsupported(Capability::Documents)),
        ("an unmapped effort", none, req(OPUS, Some("max"), &[]), effort_unmapped(TierId::Opus, "max")),
        ("a model in no tier", none, ask("in-no-tier"), unmapped("in-no-tier")),
        ("a tier with no target", |c| entry(&mut c.route, "oai").targets.sonnet = None, ask("claude-sonnet-5-5"), no_target(TierId::Sonnet)),
        ("the key removed", |c| { c.credentials.remove(&conn("oai")); }, ask(OPUS), Skip::NoCredential),
        ("paid consent narrowed away", |c| { let mut s = settings(); s.connections[3].paid = false; c.route.narrow(&s); }, ask(OPUS), Skip::PaidNotAllowed),
    ];
    for (row, edit, request, reason) in rows {
        let mut case = Case::new("work", Some("oai"));
        edit(&mut case);
        assert_eq!(
            case.plan(&request),
            Err(RouteError::Refused {
                connection: conn("oai"),
                reason
            }),
            "{row}"
        );
    }
}

#[test]
fn only_exhaustion_moves_a_session_and_skips_keep_their_reasons() {
    type Edit = fn(&mut Case);
    use Skip::{Exhausted, ExperimentalNotAllowed, NoCredential, PaidNotAllowed};
    let not_paid: Edit = |c| entry(&mut c.route, "api").paid = false;
    let no_key: Edit = |c| {
        c.credentials.remove(&conn("api"));
    };
    #[rustfmt::skip]
    let rows: [(&str, Edit, &[&str], Planned); 6] = [
        ("the next entry", |_| {}, &["sub"], Ok(("api".into(), rewrite(OPUS, None), true))),
        ("api not paid", not_paid, &["sub"], Ok(("router".into(), rewrite("anthropic/claude-opus-5-5", None), true))),
        ("api not paid, the rest shut", not_paid, &["sub", "router", "oai"], Err(blocked(&[("sub", Exhausted), ("api", PaidNotAllowed), ("router", Exhausted), ("oai", Exhausted)]))),
        ("api has no key", no_key, &["sub"], Ok(("router".into(), rewrite("anthropic/claude-opus-5-5", None), true))),
        ("api has no key, the rest shut", no_key, &["sub", "router", "oai"], Err(blocked(&[("sub", Exhausted), ("api", NoCredential), ("router", Exhausted), ("oai", Exhausted)]))),
        ("oai not allowed, the rest shut", |c| entry(&mut c.route, "oai").experimental = false, &["sub", "api", "router"], Err(blocked(&[("sub", Exhausted), ("api", Exhausted), ("router", Exhausted), ("oai", ExperimentalNotAllowed)]))),
    ];
    for (row, edit, shut_ids, expected) in rows {
        let mut case = Case::new("work", Some("sub"));
        edit(&mut case);
        case.availability = shut(shut_ids);
        assert_eq!(case.planned(&ask(OPUS)), expected, "{row}");
    }
}

#[test]
fn a_session_does_not_move_where_it_would_be_stranded() {
    let mut case = Case::new("work", Some("sub"));
    entry(&mut case.route, "api").targets.fable = None;
    let fable = ask("claude-fable-5-2");
    case.needs.note(&case.route.class_of(&fable.model), &fable);
    let haiku = ask(HAIKU);

    case.availability = shut(&["sub"]);
    assert_eq!(
        case.planned(&haiku),
        Ok((
            "router".into(),
            rewrite("anthropic/claude-haiku-5-5", None),
            true
        ))
    );

    case.availability = shut(&["sub", "router", "oai"]);
    assert_eq!(
        case.plan(&haiku),
        Err(blocked(&[
            ("sub", Skip::Exhausted),
            ("api", no_target(TierId::Fable)),
            ("router", Skip::Exhausted),
            ("oai", Skip::Exhausted),
        ]))
    );
}

#[test]
fn everything_shut_is_blocked_and_stays_so() {
    let all = ["sub", "api", "router", "oai"];
    for active in [None, Some("sub"), Some("router")] {
        let mut case = Case::new("work", active);
        case.availability = shut(&all);
        let request = ask(OPUS);
        let expected = blocked(&all.map(|id| (id, Skip::Exhausted)));
        assert_eq!(case.plan(&request), Err(expected.clone()), "{active:?}");
        assert_eq!(case.plan(&request), Err(expected), "{active:?} again");
    }
}

#[test]
fn a_visited_entry_is_tried() {
    let mut case = Case::new("work", Some("sub"));
    case.attempt.visited = vec![0];
    assert_eq!(
        case.planned(&ask(OPUS)),
        Ok(("api".into(), rewrite(OPUS, None), true))
    );

    case.attempt.visited = vec![0, 1];
    case.availability = shut(&["router", "oai"]);
    assert_eq!(
        case.plan(&ask(OPUS)),
        Err(blocked(&[
            ("sub", Skip::Tried),
            ("api", Skip::Tried),
            ("router", Skip::Exhausted),
            ("oai", Skip::Exhausted),
        ]))
    );
}

#[test]
fn an_exhausted_entry_is_only_tried_as_a_probe() {
    let request = ask(OPUS);

    let mut armed = Case::new("work", Some("api"));
    put(&mut armed.availability, "api", State::Armed);
    let plan = armed.plan(&request).expect("a probe");
    assert_eq!((plan.entry, plan.moves), (1, false), "armed active entry");
    assert_eq!(plan.tickets.len(), 1, "armed active entry holds its ticket");

    let mut due = Case::new("work", Some("api"));
    due.availability.mark(
        &conn("sub"),
        Bucket::Whole,
        &whole(Some(NOW - 10)),
        NOW - 100,
    );
    let plan = due.plan(&request).expect("api is healthy");
    assert_eq!((plan.entry, plan.tickets.len()), (1, 0), "no switch back");

    put(&mut due.availability, "api", State::Shut);
    let plan = due.plan(&request).expect("the due record is checked");
    assert_eq!(
        (plan.entry, plan.target, plan.moves),
        (0, Target::AsSent, true),
        "a due record on an earlier entry"
    );
    assert_eq!(plan.tickets.len(), 1);
}

#[test]
fn a_detour_while_the_active_entry_is_checked_does_not_move() {
    let mut case = Case::new("work", Some("api"));
    put(&mut case.availability, "sub", State::Shut);
    put(&mut case.availability, "api", State::Probing);
    entry(&mut case.route, "router").targets.fable = None;
    let fable = ask("claude-fable-5-2");
    case.needs.note(&case.route.class_of(&fable.model), &fable);
    assert_eq!(
        case.planned(&ask(HAIKU)),
        Ok((
            "router".into(),
            rewrite("anthropic/claude-haiku-5-5", None),
            false
        )),
        "fits_session is not applied on a detour"
    );

    case.availability = shut(&["sub", "api"]);
    assert_eq!(
        case.planned(&ask(HAIKU)),
        Ok(("oai".into(), rewrite("gpt-5.6-luna", None), true)),
        "a move skips the entry that would strand the session"
    );
}

#[test]
fn a_fresh_session_takes_the_first_entry_with_standing() {
    type Edit = fn(&mut Case);
    let rows: [(&str, Edit, &str); 3] = [
        ("sub", |_| {}, "sub"),
        (
            "sub removed",
            |c| entry(&mut c.route, "sub").removed = true,
            "api",
        ),
        (
            "sub removed, api not paid",
            |c| {
                entry(&mut c.route, "sub").removed = true;
                entry(&mut c.route, "api").paid = false;
            },
            "router",
        ),
    ];
    for (row, edit, expected) in rows {
        let mut case = Case::new("work", None);
        edit(&mut case);
        let got = case.planned(&ask(OPUS)).map(|(id, _, moves)| (id, moves));
        assert_eq!(got, Ok((expected.to_string(), true)), "{row}");
    }
}

#[test]
fn no_plan_ever_spends_money_or_runs_an_experiment_without_consent() {
    let ids = ["sub", "api", "router", "oai"];
    let requests = [
        ask(OPUS),
        ask("in-no-tier"),
        req(HAIKU, Some("xhigh"), &[Capability::Tools]),
    ];
    let mut plans = 0;
    for states in 0..STATES.len().pow(4) {
        for consents in 0..16_u8 {
            let mut case = Case::new("work", None);
            entry(&mut case.route, "api").paid = consents & 1 != 0;
            entry(&mut case.route, "router").paid = consents & 2 != 0;
            entry(&mut case.route, "oai").paid = consents & 4 != 0;
            entry(&mut case.route, "oai").experimental = consents & 8 != 0;
            let mut rest = states;
            for id in ids {
                put(&mut case.availability, id, STATES[rest % STATES.len()]);
                rest /= STATES.len();
            }
            for active in std::iter::once(None).chain((0..ids.len()).map(Some)) {
                case.active = active;
                for request in &requests {
                    if let Ok(plan) = case.plan(request) {
                        plans += 1;
                        let e = &case.route.entries[plan.entry];
                        assert!(
                            !e.kind.metered() || e.paid,
                            "{e:?} planned without paid, states {states}, active {active:?}"
                        );
                        assert!(
                            !e.kind.experimental() || e.experimental,
                            "{e:?} planned without its opt-in, states {states}, active {active:?}"
                        );
                    }
                }
            }
        }
    }
    assert!(plans > 0, "the table planned something");
}

#[test]
fn a_session_only_reaches_its_own_context() {
    let mut case = Case::new("home", Some("home-api"));
    case.availability = shut(&["home-api"]);
    assert_eq!(
        case.plan(&ask(OPUS)),
        Err(blocked(&[("home-api", Skip::Exhausted)]))
    );

    for state in STATES {
        for active in [None, Some(0)] {
            let mut case = Case::new("home", None);
            case.active = active;
            put(&mut case.availability, "home-api", state);
            if let Ok(plan) = case.plan(&ask(OPUS)) {
                assert_eq!(
                    case.route.entries[plan.entry].connection,
                    conn("home-api"),
                    "{state:?} {active:?}"
                );
            }
        }
    }
}

#[test]
fn skips_serialise_with_their_kind() {
    let rows = [
        (Skip::PaidNotAllowed, r#"{"kind":"paid-not-allowed"}"#),
        (
            effort_unmapped(TierId::Opus, "max"),
            r#"{"kind":"effort-unmapped","tier":"opus","effort":"max"}"#,
        ),
        (unmapped("m"), r#"{"kind":"unmapped-model","model":"m"}"#),
        (
            unsupported(Capability::ServerTools),
            r#"{"kind":"unsupported","capability":"server-tools"}"#,
        ),
    ];
    for (skip, json) in rows {
        assert_eq!(serde_json::to_string(&skip).expect("serialise"), json);
    }
}
