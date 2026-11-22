//! Row 4 of issue #12's walkthrough on the disk (M2b spec 10.2): which workers a root's desk
//! draws, as what, and for how long. Synthetic rollouts only, under a temp home, in the
//! shapes of G1, G2 (`session_meta`), G6 and G7 (activity records) and G3 (`turn_context`).

use crate::discovery::codex::Found;
use crate::discovery::{Discovery, helper_key};
use crate::model::SessionState::{self, Done, Error, Running, Thinking};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// The scans' clock: 2026-09-29T10:00:00Z.
fn now() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_hours(497_410)
}

const ROOT: u32 = 1;

fn uuid(n: u32) -> String {
    format!("0199c0de-0000-4000-8000-{n:012}")
}

fn secs(n: u64) -> Duration {
    Duration::from_secs(n)
}

/// A record's timestamp, `ago` before `now()`.
fn ts(ago: Duration) -> String {
    chrono::DateTime::<chrono::Utc>::from(now() - ago).to_rfc3339()
}

struct Home {
    dir: PathBuf,
    /// The main checkout every root works in.
    repo: PathBuf,
    /// A linked worktree of `repo` on the branch `b-branch`.
    tree: PathBuf,
}

impl Home {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("ob-codex-team-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let repo = dir.join("app");
        let tree = dir.join("tree-b");
        let git_dir = repo.join(".git/worktrees/b");
        fs::create_dir_all(&git_dir).expect("git dir");
        fs::create_dir_all(&tree).expect("tree");
        fs::create_dir_all(dir.join(".codex/sessions/2026/09/29")).expect("sessions");
        fs::write(git_dir.join("commondir"), "../..\n").expect("write");
        fs::write(git_dir.join("HEAD"), "ref: refs/heads/b-branch\n").expect("write");
        fs::write(
            tree.join(".git"),
            format!("gitdir: {}\n", git_dir.display()),
        )
        .expect("write");
        Self { dir, repo, tree }
    }

    /// Writes the rollout of thread `n`, last written `age` before `at`.
    fn write(&self, n: u32, lines: &[String], at: SystemTime, age: Duration) {
        let path = self.dir.join(format!(
            ".codex/sessions/2026/09/29/rollout-2026-09-29T10-00-00-{}.jsonl",
            uuid(n)
        ));
        fs::write(&path, format!("{}\n", lines.join("\n"))).expect("write");
        fs::File::options()
            .write(true)
            .open(&path)
            .and_then(|f| f.set_modified(at - age))
            .expect("mtime");
    }
}

/// G1: a session of its own.
fn root_meta(h: &Home) -> String {
    let (id, cwd) = (uuid(ROOT), h.repo.display());
    format!(
        r#"{{"timestamp":"t","type":"session_meta","payload":{{"id":"{id}","session_id":"{id}","cwd":"{cwd}","source":"cli"}}}}"#
    )
}

/// G2: a worker in 0.161.0's nested encoding, working in `cwd`.
fn worker(n: u32, parent: u32, cwd: &Path) -> String {
    let (id, parent, cwd) = (uuid(n), uuid(parent), cwd.display());
    format!(
        r#"{{"timestamp":"t","type":"session_meta","payload":{{"id":"{id}","cwd":"{cwd}","source":{{"subagent":{{"thread_spawn":{{"parent_thread_id":"{parent}","depth":1,"agent_path":"/root/scout-{n}"}}}}}}}}}}"#
    )
}

fn event(kind: &str, extra: &str) -> String {
    format!(r#"{{"timestamp":"t","type":"event_msg","payload":{{"type":"{kind}"{extra}}}}}"#)
}

fn started(turn: &str) -> String {
    event("task_started", &format!(r#","turn_id":"{turn}""#))
}

fn completed(turn: &str, error: bool) -> String {
    let error = if error {
        r#","error":{"message":"x"}"#
    } else {
        ""
    };
    event("task_complete", &format!(r#","turn_id":"{turn}"{error}"#))
}

fn aborted(turn: &str) -> String {
    event("turn_aborted", &format!(r#","turn_id":"{turn}""#))
}

fn reasoning() -> String {
    r#"{"timestamp":"t","type":"response_item","payload":{"type":"reasoning","summary":[]}}"#
        .to_string()
}

/// G3: a turn's model and effort.
fn context(model: &str, effort: &str) -> String {
    format!(
        r#"{{"timestamp":"t","type":"turn_context","payload":{{"model":"{model}","effort":"{effort}"}}}}"#
    )
}

/// G6 and G7, a legacy rollout: `sub_agent_activity` for every kind but `completed`, which is
/// an `item_completed` with a `SubAgentActivity` item. `extra` is spliced into either.
fn activity(n: u32, kind: &str, ago: Duration, extra: &str) -> String {
    let (at, thread) = (ts(ago), uuid(n));
    let fields = format!(
        r#""kind":"{kind}","agent_thread_id":"{thread}","agent_path":"/root/scout-{n}"{extra}"#
    );
    let payload = if kind == "completed" {
        format!(
            r#"{{"type":"item_completed","thread_id":"x","turn_id":"t","item":{{"type":"SubAgentActivity","id":"i",{fields}}}}}"#
        )
    } else {
        format!(r#"{{"type":"sub_agent_activity","event_id":"e","occurred_at_ms":0,{fields}}}"#)
    };
    format!(r#"{{"timestamp":"{at}","type":"event_msg","payload":{payload}}}"#)
}

/// One scan of a case: the rollouts written before it and what the root's desk draws.
struct Step {
    /// Thread, lines and how long before the scan the file was written.
    rollouts: Vec<(u32, Vec<String>, Duration)>,
    /// The scan runs this long after `now()`.
    later: Duration,
    /// Whether a chat desk owns the root, which makes it live.
    owned: bool,
    expect: Vec<(u32, SessionState)>,
    check: fn(&Home, &Discovery, &[Found]),
}

fn step(rollouts: Vec<(u32, Vec<String>, Duration)>, expect: Vec<(u32, SessionState)>) -> Step {
    Step {
        rollouts,
        later: Duration::ZERO,
        owned: true,
        expect,
        check: |_, _, _| {},
    }
}

fn root_of(found: &[Found]) -> &Found {
    found
        .iter()
        .find(|f| f.session_id == uuid(ROOT))
        .expect("the root is found")
}

fn siblings_and_a_nested_worker(h: &Home) -> Vec<Step> {
    let open = |n: u32, parent: u32, cwd: &Path| vec![worker(n, parent, cwd), started("t1")];
    let mut first = step(
        vec![
            (ROOT, vec![root_meta(h)], secs(1)),
            (2, open(2, ROOT, &h.repo), secs(1)),
            (3, open(3, ROOT, &h.tree), secs(1)),
            (4, open(4, 2, &h.repo), secs(1)),
        ],
        vec![(2, Running), (4, Running), (3, Running)],
    );
    first.check = |h, _, found| {
        assert_eq!(found.len(), 1, "case 1: only the root is a desk");
        let root = root_of(found);
        let b = &root.helpers[2];
        assert!(b.worktree, "case 1: B works in a worktree");
        assert_eq!(b.branch.as_deref(), Some("b-branch"), "case 1");
        assert_eq!(root.kids.len(), 3, "case 1");
        let place = root.kids[&uuid(3)]
            .place
            .as_ref()
            .expect("case 1: B's place");
        assert_eq!(place.checkout, h.tree, "case 1");
    };
    let done = vec![
        worker(2, ROOT, &h.repo),
        started("t1"),
        completed("t1", false),
    ];
    let second = step(vec![(2, done, secs(1))], vec![(4, Running), (3, Running)]);
    vec![first, second]
}

fn aborted_then_resumed(h: &Home) -> Vec<Step> {
    let stopped = vec![worker(2, ROOT, &h.repo), started("t1"), aborted("t1")];
    let mut resumed = stopped.clone();
    resumed.push(started("t2"));
    vec![
        step(
            vec![(ROOT, vec![root_meta(h)], secs(1)), (2, stopped, secs(30))],
            vec![],
        ),
        step(vec![(2, resumed, secs(1))], vec![(2, Running)]),
    ]
}

fn failed(h: &Home) -> Vec<Step> {
    let lines = vec![
        worker(2, ROOT, &h.repo),
        started("t1"),
        completed("t1", true),
    ];
    vec![
        step(
            vec![
                (ROOT, vec![root_meta(h)], secs(1)),
                (2, lines.clone(), secs(1)),
            ],
            vec![(2, Error)],
        ),
        step(vec![(2, lines, Duration::from_mins(11))], vec![]),
    ]
}

fn thinking(h: &Home) -> Vec<Step> {
    let lines = vec![worker(2, ROOT, &h.repo), started("t1"), reasoning()];
    vec![step(
        vec![(ROOT, vec![root_meta(h)], secs(1)), (2, lines, secs(1))],
        vec![(2, Thinking)],
    )]
}

fn over_before_it_was_drawn(h: &Home) -> Vec<Step> {
    let lines = vec![
        worker(2, ROOT, &h.repo),
        started("t1"),
        completed("t1", false),
    ];
    let mut later = step(vec![], vec![]);
    later.later = secs(9);
    vec![
        step(
            vec![(ROOT, vec![root_meta(h)], secs(1)), (2, lines, secs(3))],
            vec![(2, Done)],
        ),
        later,
    ]
}

fn known_only_from_the_root(h: &Home) -> Vec<Step> {
    let root = vec![
        root_meta(h),
        activity(2, "started", secs(2), ""),
        activity(2, "completed", secs(2), ""),
    ];
    let mut first = step(vec![(ROOT, root, secs(1))], vec![(2, Done)]);
    first.check = |_, _, found| {
        let kid = &root_of(found).kids[&uuid(2)];
        assert_eq!(kid.name, "scout-2", "case 8: named from its path");
        assert_eq!(kid.place, None, "case 8: no rollout, no place");
    };
    let mut later = step(vec![], vec![]);
    later.later = secs(9);
    vec![first, later]
}

fn started_record_only(h: &Home) -> Vec<Step> {
    let record = |ago| vec![root_meta(h), activity(2, "started", ago, "")];
    vec![
        step(
            vec![(ROOT, record(Duration::from_mins(1)), secs(1))],
            vec![(2, Running)],
        ),
        step(
            // A new mtime, or the cache keeps the old parse.
            vec![(ROOT, record(Duration::from_mins(11)), secs(2))],
            vec![],
        ),
    ]
}

fn the_newer_evidence_decides(h: &Home) -> Vec<Step> {
    let open = vec![worker(2, ROOT, &h.repo), started("t1")];
    let root = |ago| vec![root_meta(h), activity(2, "completed", ago, "")];
    vec![
        step(
            vec![
                (ROOT, root(Duration::from_mins(1)), secs(1)),
                (2, open, Duration::from_mins(2)),
            ],
            vec![],
        ),
        step(
            vec![(ROOT, root(Duration::from_mins(3)), secs(2))],
            vec![(2, Running)],
        ),
    ]
}

fn a_guardian_is_busy_but_never_drawn(h: &Home) -> Vec<Step> {
    let (id, parent) = (uuid(2), uuid(ROOT));
    let guardian = vec![
        format!(
            r#"{{"timestamp":"t","type":"session_meta","payload":{{"id":"{id}","cwd":"/x","parent_thread_id":"{parent}","source":{{"subagent":{{"other":"guardian"}}}}}}}}"#
        ),
        context("gpt-6-luna", "low"),
        event(
            "token_count",
            r#","info":{"total_token_usage":{"input_tokens":40,"output_tokens":2}}"#,
        ),
        started("t1"),
    ];
    let mut only = step(
        vec![(ROOT, vec![root_meta(h)], secs(1)), (2, guardian, secs(1))],
        vec![],
    );
    only.check = |_, d, found| {
        assert_eq!(found.len(), 1, "case 11: no desk for the guardian");
        assert_eq!(d.codex_subagents_busy(&uuid(ROOT)), 1, "case 11 (D6)");
        assert!(
            root_of(found).transcript.spend.tokens > 0,
            "case 11: its spend is the root's"
        );
    };
    vec![only]
}

fn broken_parent_links(h: &Home) -> Vec<Step> {
    let open = |n: u32, parent: u32| vec![worker(n, parent, &h.repo), started("t1")];
    let mut only = step(
        vec![
            (ROOT, vec![root_meta(h)], secs(1)),
            (2, open(2, 99), secs(1)),
            (3, open(3, 4), secs(1)),
            (4, open(4, 3), secs(1)),
            (5, open(5, 5), secs(1)),
        ],
        vec![],
    );
    only.check = |_, _, found| {
        assert_eq!(found.len(), 1, "case 12: no worker is a desk");
        assert!(root_of(found).kids.is_empty(), "case 12: and none is a kid");
    };
    vec![only]
}

fn a_root_that_is_not_live(h: &Home) -> Vec<Step> {
    let open = vec![worker(2, ROOT, &h.repo), started("t1")];
    let mut only = step(
        vec![(ROOT, vec![root_meta(h)], secs(1)), (2, open, secs(1))],
        vec![],
    );
    only.owned = false;
    only.check = |_, d, _| assert_eq!(d.codex_subagents_busy(&uuid(ROOT)), 1, "case 13");
    vec![only]
}

fn model_and_effort(h: &Home) -> Vec<Step> {
    let spawn = r#","model":"gpt-test-b","reasoning_effort":"medium""#;
    let root = vec![
        root_meta(h),
        activity(3, "started", Duration::from_mins(1), spawn),
    ];
    let own = vec![
        worker(2, ROOT, &h.repo),
        context("gpt-test-a", "low"),
        started("t1"),
    ];
    let mut only = step(
        vec![
            (ROOT, root, secs(1)),
            (2, own, secs(1)),
            (3, vec![worker(3, ROOT, &h.repo), started("t1")], secs(1)),
            (4, vec![worker(4, ROOT, &h.repo), started("t1")], secs(1)),
        ],
        vec![(2, Running), (3, Running), (4, Running)],
    );
    only.check = |_, _, found| {
        let got: Vec<(Option<&str>, Option<&str>)> = root_of(found)
            .helpers
            .iter()
            .map(|h| (h.model.as_deref(), h.effort.as_deref()))
            .collect();
        let want = vec![
            (Some("gpt-test-a"), Some("low")),
            (Some("gpt-test-b"), Some("medium")),
            (None, None),
        ];
        assert_eq!(got, want, "case 14 (D16)");
    };
    vec![only]
}

fn an_old_open_turn(h: &Home) -> Vec<Step> {
    let open = vec![worker(2, ROOT, &h.repo), started("t1")];
    vec![step(
        vec![
            (ROOT, vec![root_meta(h)], secs(1)),
            (2, open, Duration::from_hours(13)),
        ],
        vec![],
    )]
}

fn an_owned_root_written_long_ago(h: &Home) -> Vec<Step> {
    let mut only = step(
        vec![(ROOT, vec![root_meta(h)], Duration::from_hours(13))],
        vec![],
    );
    only.check = |_, _, found| assert!(root_of(found).owned, "case 16: found, and live");
    vec![only]
}

/// Case 17: the parent's newer "running" record never ages out a worker whose own turn is
/// still open, and case 18: a worker that is over stays proven over once it is not drawn.
fn a_blocked_worker_stays_and_a_finished_one_is_proven_over(h: &Home) -> Vec<Step> {
    let root = vec![
        root_meta(h),
        activity(2, "started", Duration::from_mins(1), ""),
        activity(3, "completed", Duration::from_hours(13), ""),
    ];
    let open = vec![worker(2, ROOT, &h.repo), started("t1")];
    let over = vec![
        worker(3, ROOT, &h.repo),
        started("t1"),
        completed("t1", false),
    ];
    let mut only = step(
        vec![
            (ROOT, root, secs(1)),
            (2, open, Duration::from_mins(20)),
            (3, over, Duration::from_hours(13)),
        ],
        vec![(2, Running)],
    );
    only.check = |_, _, found| {
        let kids = &root_of(found).kids;
        assert!(!kids[&uuid(2)].gone, "case 17: blocked, not over");
        assert!(kids[&uuid(3)].gone, "case 18: over, however old");
    };
    vec![only]
}

type Case = (&'static str, fn(&Home) -> Vec<Step>);

#[test]
fn row_four_on_the_disk() {
    let cases: [Case; 15] = [
        (
            "1-2 siblings and a nested worker, then one ends",
            siblings_and_a_nested_worker,
        ),
        ("3-4 aborted, then resumed", aborted_then_resumed),
        ("5 failed, then too old", failed),
        ("6 thinking", thinking),
        ("7 over before it was drawn", over_before_it_was_drawn),
        (
            "8 known only from the root's records",
            known_only_from_the_root,
        ),
        ("9 a started record only", started_record_only),
        ("10 the newer evidence decides", the_newer_evidence_decides),
        ("11 a guardian", a_guardian_is_busy_but_never_drawn),
        ("12 broken parent links", broken_parent_links),
        ("13 a root that is not live", a_root_that_is_not_live),
        ("14 model and effort", model_and_effort),
        ("15 an open turn 13 hours old", an_old_open_turn),
        (
            "16 an owned root written long ago",
            an_owned_root_written_long_ago,
        ),
        (
            "17-18 a blocked worker and a finished one",
            a_blocked_worker_stays_and_a_finished_one_is_proven_over,
        ),
    ];
    for (i, (name, steps)) in cases.into_iter().enumerate() {
        let h = Home::new(&i.to_string());
        let mut discovery = Discovery::new(h.dir.clone());
        discovery.ignore_running_codex();
        for (s, step) in steps(&h).into_iter().enumerate() {
            let at = now() + step.later;
            for (n, lines, age) in &step.rollouts {
                h.write(*n, lines, at, *age);
            }
            let owned = if step.owned {
                vec![uuid(ROOT)]
            } else {
                Vec::new()
            };
            let found = discovery.codex.scan_with(at, Vec::new(), &owned);
            let drawn: Vec<(String, SessionState)> = root_of(&found)
                .helpers
                .iter()
                .map(|h| (h.id.clone(), h.state))
                .collect();
            let want: Vec<(String, SessionState)> = step
                .expect
                .iter()
                .map(|(n, state)| (uuid(*n), *state))
                .collect();
            assert_eq!(drawn, want, "{name}, scan {s}");
            (step.check)(&h, &discovery, &found);
        }
        fs::remove_dir_all(&h.dir).expect("cleanup");
    }
}

// Through `Discovery::scan` and `assemble`: a live Codex desk carries its team, and the
// worktree worker's checkout is kept under its helper key for the editor action.
#[test]
fn a_codex_desk_carries_its_workers_through_assemble() {
    let h = Home::new("assemble");
    h.write(ROOT, &[root_meta(&h)], now(), secs(1));
    h.write(
        3,
        &[worker(3, ROOT, &h.tree), started("t1")],
        now(),
        secs(1),
    );
    let mut discovery = Discovery::new(h.dir.clone());
    discovery.ignore_running_codex();
    let found = discovery.scan(now(), &[uuid(ROOT)]);
    let desk = found
        .sessions
        .iter()
        .find(|s| s.id == uuid(ROOT))
        .expect("the root's desk");
    let ids: Vec<&str> = desk.helpers.iter().map(|h| h.id.as_str()).collect();
    assert_eq!(ids, vec![uuid(3).as_str()]);
    assert!(desk.helpers[0].worktree);
    assert_eq!(
        found.checkouts.get(&helper_key(&uuid(ROOT), &uuid(3))),
        Some(&h.tree)
    );
    fs::remove_dir_all(&h.dir).expect("cleanup");
}
