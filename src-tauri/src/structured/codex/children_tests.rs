//! The workers of a chat desk (build spec M2b, table 10.4). Every line is composed
//! from the bundle `codex app-server generate-json-schema` writes for codex-cli
//! 0.161.0: `v2/ItemStartedNotification.json` (`CollabAgentToolCallThreadItem`, G12,
//! and `SubAgentActivityThreadItem`, G13, with 0.170.0's `model` and `reasoningEffort`
//! from rust-v0.170.0 `v2/item.rs`), `v2/TurnStartedNotification.json`,
//! `v2/TurnCompletedNotification.json`, `CommandExecutionRequestApprovalParams.json`,
//! `v2/AgentMessageDeltaNotification.json`, `v2/TurnPlanUpdatedNotification.json`,
//! `v2/ErrorNotification.json` and `v2/ThreadResumeResponse.json`. None is replayed.

use super::mapper::{Mapper, Out};
use super::tests::running_on;
use crate::model::SessionState::{self, Error, Idle, Running, Thinking, WaitingHuman};
use crate::structured::{Entry, Event, Values};

/// The desk's thread, its turn, two workers, a nested worker and a thread nothing named.
const T: &str = "0199c0de-0000-4000-8000-000000000001";
const X: &str = "0199c0de-0000-4000-8000-0000000000a1";
const A: &str = "0199c0de-0000-4000-8000-000000000002";
const B: &str = "0199c0de-0000-4000-8000-000000000003";
const N: &str = "0199c0de-0000-4000-8000-000000000004";
const G: &str = "0199c0de-0000-4000-8000-000000000005";

enum Step {
    Line(String),
    /// Answer the request with this approval id with its first choice.
    Answer(&'static str),
}

/// One helper as drawn: its id, state, model and effort.
type Drawn = (String, SessionState, Option<String>, Option<String>);

/// What one step put out.
#[derive(Debug, Default, PartialEq, Eq)]
struct Seen {
    /// Each `Event::Helpers`, in order.
    helpers: Vec<Vec<Drawn>>,
    /// Each entry: a tool as its title and status, anything else as its kind.
    entries: Vec<String>,
    /// Each approval's `asked_by`.
    asked_by: Vec<Option<String>>,
    /// The last state reported.
    state: Option<SessionState>,
}

impl Seen {
    fn at(state: SessionState) -> Self {
        Self {
            state: Some(state),
            ..Self::default()
        }
    }

    fn drew(mut self, list: &[Drawn]) -> Self {
        self.helpers.push(list.to_vec());
        self
    }

    fn entry(mut self, entry: &str) -> Self {
        self.entries.push(entry.to_string());
        self
    }

    fn asked(mut self, by: &str) -> Self {
        self.asked_by.push(Some(by.to_string()));
        self
    }
}

fn h(id: &str, state: SessionState) -> Drawn {
    (id.to_string(), state, None, None)
}

fn hm(id: &str, state: SessionState, model: &str, effort: &str) -> Drawn {
    let (model, effort) = (Some(model.to_string()), Some(effort.to_string()));
    (id.to_string(), state, model, effort)
}

fn take(out: Vec<Out>) -> Seen {
    let mut seen = Seen::default();
    for o in out {
        match o {
            Out::Event(Event::Helpers(list)) => seen.helpers.push(
                list.into_iter()
                    .map(|x| (x.id, x.state, x.model, x.effort))
                    .collect(),
            ),
            Out::Event(Event::Entry(e)) => seen.entries.push(match e {
                Entry::Tool { title, status, .. } => format!("tool {title} {status:?}"),
                Entry::Notice { .. } => "notice".to_string(),
                Entry::Agent { .. } => "agent".to_string(),
                Entry::User { .. } => "user".to_string(),
                Entry::Thought { .. } => "thought".to_string(),
            }),
            Out::Event(Event::Approval(a)) => seen.asked_by.push(a.asked_by),
            Out::Event(Event::State(s)) => seen.state = Some(s),
            Out::Event(_) | Out::Send(_) => {}
        }
    }
    seen
}

fn step(m: &mut Mapper, step: &Step) -> Seen {
    match step {
        Step::Line(line) => take(m.line(line)),
        Step::Answer(id) => take(m.answer(id, "0", &Values::new(), |_| Ok(())).0),
    }
}

/// A desk on thread `T` with turn `X` running.
fn desk() -> Mapper {
    running_on(T, X, "TASK-ONE")
}

/// A desk reopening thread `T`, past `initialize`.
fn reopening() -> Mapper {
    let mut m = Mapper::new("/w", None, None, Some(T));
    m.begin("");
    m.line(r#"{"id":1,"result":{}}"#);
    m
}

/// A named case: the desk it starts on, then each step and what it puts out.
type Case = (&'static str, fn() -> Mapper, Vec<(Step, Seen)>);

/// Walk each case on a fresh desk and check what every step put out.
fn walk(cases: Vec<Case>) {
    for (name, start, steps) in cases {
        let mut m = start();
        for (i, (s, want)) in steps.iter().enumerate() {
            assert_eq!(&step(&mut m, s), want, "case {name}, step {i}");
        }
    }
}

fn note(method: &str, thread: &str, body: &str) -> Step {
    Step::Line(format!(
        r#"{{"method":"{method}","params":{{"threadId":"{thread}",{body}}}}}"#
    ))
}

fn item(phase: &str, thread: &str, item: &str) -> Step {
    let body = format!(r#""turnId":"turn-x","item":{item}"#);
    note(&format!("item/{phase}"), thread, &body)
}

/// A collaboration call of `T`: `agents` is the raw `agentsStates` object, `extra` more fields.
fn collab(tool: &str, status: &str, receivers: &[&str], agents: &str, extra: &str) -> String {
    let receivers = receivers
        .iter()
        .map(|r| format!(r#""{r}""#))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        r#"{{"type":"collabAgentToolCall","id":"call-{tool}","tool":"{tool}","status":"{status}","senderThreadId":"{T}","receiverThreadIds":[{receivers}],"agentsStates":{agents},"prompt":null{extra}}}"#
    )
}

/// `T` spawned `thread`, and the call is over.
fn spawn(thread: &str) -> Step {
    item(
        "completed",
        T,
        &collab("spawnAgent", "completed", &[thread], "{}", ""),
    )
}

/// One worker named in `agentsStates` with `status`.
fn state_of(thread: &str, status: &str) -> String {
    format!(r#"{{"{thread}":{{"status":"{status}","message":null}}}}"#)
}

fn activity(phase: &str, thread: &str, kind: &str, extra: &str) -> Step {
    let act = format!(
        r#"{{"type":"subAgentActivity","id":"act-1","kind":"{kind}","agentThreadId":"{thread}","agentPath":"/root/worker_one"{extra}}}"#
    );
    item(phase, T, &act)
}

fn turn(method: &str, thread: &str, id: &str, status: &str) -> Step {
    let body = format!(r#""turn":{{"id":"{id}","items":[],"status":"{status}","error":null}}"#);
    note(&format!("turn/{method}"), thread, &body)
}

fn command(id: &str, thread: &str) -> Step {
    Step::Line(format!(
        r#"{{"id":{id},"method":"item/commandExecution/requestApproval","params":{{"threadId":"{thread}","turnId":"turn-w","itemId":"item-c","startedAtMs":1,"command":"tool-x --flag","cwd":"/w"}}}}"#
    ))
}

const SPAWNED: &str = "tool Start a helper Done";
const WAITED: &str = "tool Wait for helpers Done";

/// A completed `wait` of `T` naming `agents`.
fn wait(agents: &str) -> Step {
    item(
        "completed",
        T,
        &collab("wait", "completed", &[], agents, ""),
    )
}

/// `spawn(A)` and what it puts out on a fresh desk.
fn spawned_a() -> (Step, Seen) {
    (
        spawn(A),
        Seen::at(Thinking).entry(SPAWNED).drew(&[h(A, Running)]),
    )
}

/// Cases 1 to 4: a spawn makes a worker, and its own turns say when it works.
#[test]
fn a_spawn_makes_a_worker_and_its_turns_say_when_it_works() {
    walk(vec![
        (
            "1 and 2 a spawn, in progress then over",
            desk,
            vec![
                (
                    item(
                        "started",
                        T,
                        &collab("spawnAgent", "inProgress", &[], "{}", ""),
                    ),
                    Seen::at(Running).entry("tool Start a helper Running"),
                ),
                (
                    item(
                        "completed",
                        T,
                        &collab(
                            "spawnAgent",
                            "completed",
                            &[A],
                            "{}",
                            r#","model":"model-test","reasoningEffort":"high""#,
                        ),
                    ),
                    Seen::at(Thinking)
                        .entry(SPAWNED)
                        .drew(&[hm(A, Running, "model-test", "high")]),
                ),
            ],
        ),
        (
            "3 and 4 a worker's turn ends, and more work brings it back",
            desk,
            vec![
                spawned_a(),
                (
                    turn("started", A, "turn-a1", "inProgress"),
                    Seen::at(Thinking),
                ),
                (
                    turn("completed", A, "turn-a1", "completed"),
                    Seen::at(Thinking).drew(&[]),
                ),
                (
                    turn("started", A, "turn-a2", "inProgress"),
                    Seen::at(Thinking).drew(&[h(A, Running)]),
                ),
            ],
        ),
    ]);
}

/// Cases 5 to 8: what ends a worker's work, and what does not.
#[test]
fn a_workers_turn_or_a_call_says_its_work_is_over() {
    let mut cases: Vec<Case> = vec![
        (
            "6 a wait names one done and one running",
            desk,
            vec![
                spawned_a(),
                (
                    spawn(B),
                    Seen::at(Thinking)
                        .entry(SPAWNED)
                        .drew(&[h(A, Running), h(B, Running)]),
                ),
                (
                    wait(&format!(
                        r#"{{"{A}":{{"status":"completed"}},"{B}":{{"status":"running"}}}}"#
                    )),
                    Seen::at(Thinking).entry(WAITED).drew(&[h(B, Running)]),
                ),
            ],
        ),
        (
            "8 an unknown status changes nothing",
            desk,
            vec![
                spawned_a(),
                (
                    wait(&state_of(A, "somethingNew")),
                    Seen::at(Thinking).entry(WAITED),
                ),
            ],
        ),
    ];
    for status in ["interrupted", "failed"] {
        cases.push((
            "5 a worker's turn ends otherwise",
            desk,
            vec![
                spawned_a(),
                (
                    turn("started", A, "turn-a1", "inProgress"),
                    Seen::at(Thinking),
                ),
                (
                    turn("completed", A, "turn-a1", status),
                    Seen::at(Thinking).drew(&[]),
                ),
            ],
        ));
    }
    for status in ["errored", "shutdown", "notFound"] {
        cases.push((
            "7 a worker that is over",
            desk,
            vec![
                spawned_a(),
                (
                    wait(&state_of(A, status)),
                    Seen::at(Thinking).entry(WAITED).drew(&[]),
                ),
            ],
        ));
    }
    cases.push((
        "7 a worker dismissed",
        desk,
        vec![
            spawned_a(),
            (
                item(
                    "completed",
                    T,
                    &collab("closeAgent", "completed", &[A], "{}", ""),
                ),
                Seen::at(Thinking)
                    .entry("tool Dismiss a helper Done")
                    .drew(&[]),
            ),
        ],
    ));
    walk(cases);
}

/// Cases 9 to 11: activity items.
#[test]
fn activity_items_say_who_works() {
    walk(vec![
        (
            "9 a start sent as started and completed",
            desk,
            vec![
                (
                    activity("started", A, "started", ""),
                    Seen::default().drew(&[h(A, Running)]),
                ),
                (activity("completed", A, "started", ""), Seen::default()),
            ],
        ),
        (
            "10 a start with 0.170.0's model and effort",
            desk,
            vec![(
                activity(
                    "started",
                    A,
                    "started",
                    r#","model":"model-test","reasoningEffort":"low""#,
                ),
                Seen::default().drew(&[hm(A, Running, "model-test", "low")]),
            )],
        ),
        (
            "11 completed, then a message alone",
            desk,
            vec![
                (
                    activity("started", A, "started", ""),
                    Seen::default().drew(&[h(A, Running)]),
                ),
                (
                    activity("completed", A, "completed", ""),
                    Seen::default().drew(&[]),
                ),
                (activity("completed", A, "interacted", ""), Seen::default()),
            ],
        ),
    ]);
}

/// Cases 12 to 15: a worker's request names it, and a thread nothing named is nobody.
#[test]
fn a_request_names_the_worker_that_asked() {
    walk(vec![
        (
            "12 and 13 a named worker asks, then is answered",
            desk,
            vec![
                (
                    activity("started", A, "started", ""),
                    Seen::default().drew(&[h(A, Running)]),
                ),
                (
                    command("7", A),
                    Seen::at(WaitingHuman)
                        .asked("worker_one")
                        .drew(&[h(A, WaitingHuman)]),
                ),
                (Step::Answer("7"), Seen::at(Thinking).drew(&[h(A, Running)])),
            ],
        ),
        (
            "14 a thread nothing named asks",
            desk,
            vec![(
                command("8", G),
                Seen::at(WaitingHuman).asked("helper 0199c0de"),
            )],
        ),
        (
            "15 a turn of a thread nothing named",
            desk,
            vec![(
                turn("started", G, "turn-g1", "inProgress"),
                Seen::at(Thinking),
            )],
        ),
    ]);
}

/// Cases 16 to 19: a worker's conversation is not the desk's (D9), a resume runs
/// nothing, and a broken item is no event.
#[test]
fn a_workers_conversation_is_not_the_desks() {
    let agent = r#"{"type":"agentMessage","id":"item-am","text":"TEXT"}"#;
    let exec = r#"{"type":"commandExecution","id":"item-c","command":"tool-x","cwd":"/w","status":"inProgress","commandActions":[],"aggregatedOutput":null,"exitCode":null}"#;
    let no_id = r#"{"type":"collabAgentToolCall","tool":"spawnAgent","status":"inProgress","senderThreadId":"x","receiverThreadIds":[],"agentsStates":{}}"#;
    let bad_states = r#"{"type":"collabAgentToolCall","id":"call-x","tool":"wait","status":"completed","senderThreadId":"x","receiverThreadIds":[],"agentsStates":"STATES"}"#;
    let resumed = format!(
        r#"{{"id":2,"result":{{"thread":{{"id":"{T}","turns":[{{"id":"{X}","status":"completed","items":[{}]}}]}}}}}}"#,
        collab("spawnAgent", "completed", &[A], "{}", "")
    );
    walk(vec![
        (
            "16 a worker's items, deltas, plans and errors",
            desk,
            vec![
                (item("started", A, agent), Seen::default()),
                (item("completed", A, agent), Seen::default()),
                (item("started", A, exec), Seen::default()),
                (
                    note(
                        "item/agentMessage/delta",
                        A,
                        r#""turnId":"turn-a1","itemId":"item-am","delta":"TEXT""#,
                    ),
                    Seen::default(),
                ),
                (
                    note(
                        "turn/plan/updated",
                        A,
                        r#""turnId":"turn-a1","explanation":null,"plan":[{"step":"STEP","status":"pending"}]"#,
                    ),
                    Seen::default(),
                ),
                (
                    note(
                        "error",
                        A,
                        r#""turnId":"turn-a1","willRetry":false,"error":{"message":"FAILED","additionalDetails":null}"#,
                    ),
                    Seen::default(),
                ),
                (
                    turn("started", G, "turn-g1", "inProgress"),
                    Seen::at(Thinking),
                ),
                (
                    turn("completed", T, X, "failed"),
                    Seen::at(Error).entry("notice"),
                ),
            ],
        ),
        (
            "17 a worker spawns a nested worker",
            desk,
            vec![
                spawned_a(),
                (
                    item(
                        "completed",
                        A,
                        &collab("spawnAgent", "completed", &[N], "{}", ""),
                    ),
                    Seen::default().drew(&[h(A, Running), h(N, Running)]),
                ),
            ],
        ),
        (
            "18 a resume replays a finished spawn",
            reopening,
            vec![(Step::Line(resumed), Seen::at(Idle).entry(SPAWNED))],
        ),
        (
            "19 an item with no id, or with states that are text",
            desk,
            vec![
                (item("started", T, no_id), Seen::default()),
                (item("completed", T, bad_states), Seen::default()),
                (item("completed", A, no_id), Seen::default()),
                (item("completed", A, bad_states), Seen::default()),
            ],
        ),
    ]);
}
