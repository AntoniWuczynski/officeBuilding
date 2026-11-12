use super::brief::{DIFF_FILE, HANDOVER_DIR};
use super::*;
use crate::process::{Call, Captured, FakeRunner};
use crate::settings::SettingsPatch;
use chrono::{TimeDelta, TimeZone};
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};

const GH: &str = "/opt/homebrew/bin/gh";
const GIT: &str = "/usr/bin/git";
const WIDGETS: &str = "example-org/widgets";
const SPROCKETS: &str = "example-org/sprockets";
const DIFF: &str = include_str!("../fixtures/pull-request.diff");
const OVERRAN: &str = "the review ran past 20 minutes and was stopped";
const NOT_FINDINGS: &str = "the reviewer's reply was not the findings it was asked for";

fn t0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 12, 10, 0, 0)
        .single()
        .expect("time")
}

fn secs(s: i64) -> DateTime<Utc> {
    t0() + TimeDelta::seconds(s)
}

fn mins(m: i64) -> DateTime<Utc> {
    secs(m * 60)
}

fn sha(c: char) -> String {
    c.to_string().repeat(40)
}

fn ok(stdout: &str) -> Captured {
    Captured {
        code: Some(0),
        stdout: stdout.into(),
        stderr: String::new(),
    }
}

fn exit(code: i32, stderr: &str) -> Captured {
    Captured {
        code: Some(code),
        stdout: String::new(),
        stderr: stderr.into(),
    }
}

fn strings(call: &Call) -> Vec<String> {
    let mut args = call.args.iter().map(|a| a.to_string_lossy().into_owned());
    let mut kept = Vec::new();
    while let Some(arg) = args.next() {
        // git's `-c name=value` pairs are not part of a call's shape.
        if arg == "-c" && call.program == Path::new(GIT) {
            args.next();
        } else {
            kept.push(arg);
        }
    }
    kept
}

/// A pull request as `gh pr list` shows it.
struct Pr {
    number: u64,
    head: String,
    draft: bool,
    labels: Vec<&'static str>,
}

fn pr(number: u64, head: char) -> Pr {
    Pr {
        number,
        head: sha(head),
        draft: false,
        labels: Vec::new(),
    }
}

fn title(number: u64) -> String {
    format!("Polish widget {number}")
}

fn listing(prs: &[Pr]) -> Captured {
    let items: Vec<Value> = prs
        .iter()
        .map(|p| {
            json!({
                "number": p.number,
                "title": title(p.number),
                "headRefOid": p.head,
                "isDraft": p.draft,
                "labels": p.labels.iter().map(|l| json!({ "name": l })).collect::<Vec<_>>(),
            })
        })
        .collect();
    ok(&Value::from(items).to_string())
}

/// The reviewer's final reply: one fenced block of findings, each `(file, line)`.
fn reply(verdict: &str, findings: &[(&str, u32)]) -> String {
    let findings: Vec<Value> = findings
        .iter()
        .map(|(file, line)| {
            json!({
                "file": file, "line": line, "severity": "high",
                "summary": "A 404 is retried forever.",
                "failure_scenario": "Deleting the bucket makes every upload spin."
            })
        })
        .collect();
    let report =
        json!({ "verdict": verdict, "summary": "Retries need a stop.", "findings": findings });
    format!("```json\n{report}\n```")
}

/// What the fake `gh`, `git` and login shell answer.
struct Script {
    probe: String,
    /// By repo root. A root not here has the widgets origin.
    origins: BTreeMap<String, Option<String>>,
    /// By repository. One not here lists nothing.
    lists: BTreeMap<String, Captured>,
    diff: Captured,
    fetch: Captured,
    has_commit: bool,
    add: Captured,
    /// Replies to each post in turn. When empty, a post works.
    posts: VecDeque<Captured>,
    /// What each post's `--input` file held.
    payloads: Vec<String>,
    /// What listing a pull request's reviews answers (tab-separated id, head, body).
    reviews: Captured,
}

impl Default for Script {
    fn default() -> Self {
        Self {
            probe: format!("Last login\n{GH}\n{GIT}\n"),
            origins: BTreeMap::new(),
            lists: BTreeMap::new(),
            diff: ok(DIFF),
            fetch: ok(""),
            has_commit: true,
            add: ok(""),
            posts: VecDeque::new(),
            payloads: Vec::new(),
            reviews: ok(""),
        }
    }
}

fn answer(script: &Mutex<Script>, call: &Call) -> Captured {
    let args = strings(call);
    let arg = |i: usize| args.get(i).map_or("", String::as_str);
    let mut script = lock(script);
    if arg(0) == "-l" {
        return ok(&script.probe);
    }
    if call.program == Path::new(GH) {
        return match (arg(0), arg(1)) {
            ("pr", "list") => script
                .lists
                .get(arg(3))
                .cloned()
                .unwrap_or_else(|| ok("[]")),
            ("pr", "diff") => script.diff.clone(),
            ("api", _) if !args.iter().any(|a| a == "--method") => script.reviews.clone(),
            ("api", _) => {
                let body = fs::read_to_string(arg(5)).expect("payload");
                script.payloads.push(body);
                script
                    .posts
                    .pop_front()
                    .unwrap_or_else(|| ok(r#"{"id": 90210, "node_id": "x"}"#))
            }
            _ => panic!("unexpected gh call {args:?}"),
        };
    }
    assert_eq!(call.program, Path::new(GIT));
    match (arg(2), arg(3)) {
        ("remote", _) => match script.origins.get(arg(1)) {
            None => ok("git@github.com:example-org/widgets.git\n"),
            Some(Some(url)) => ok(url),
            Some(None) => exit(2, "error: No such remote 'origin'"),
        },
        ("fetch", _) => script.fetch.clone(),
        ("cat-file", _) => {
            if script.has_commit {
                ok("")
            } else {
                exit(1, "")
            }
        }
        ("worktree", "add") => script.add.clone(),
        ("worktree", _) => ok(""),
        _ => panic!("unexpected git call {args:?}"),
    }
}

/// A short name for a recorded call: "pr list", "api", "fetch", "worktree add", "probe".
fn name(call: &Call) -> String {
    let args = strings(call);
    if args.first().is_some_and(|a| a == "-l") {
        "probe".into()
    } else if call.program == Path::new(GH) {
        if args[0] == "api" {
            if args.iter().any(|a| a == "--method") {
                "api".into()
            } else {
                "api list".into()
            }
        } else {
            args[..2].join(" ")
        }
    } else if args[2] == "worktree" {
        args[2..4].join(" ")
    } else {
        args[2].clone()
    }
}

/// The only `gh` call that is not `pr list` or `pr diff` is the review post.
fn assert_gh_only_posts_reviews(calls: &[Call]) -> usize {
    let mut posts = 0;
    for call in calls.iter().filter(|c| c.program == Path::new(GH)) {
        let args = strings(call);
        if args[0] == "pr" && matches!(args[1].as_str(), "list" | "diff") {
            continue;
        }
        if args.len() == 5 && args[2] == "--paginate" {
            // Reading the reviews already on a pull request writes nothing.
            continue;
        }
        assert_eq!(args.len(), 6, "{args:?}");
        assert_eq!(args[0], "api", "{args:?}");
        let number = args[1]
            .strip_prefix("repos/")
            .and_then(|rest| rest.strip_suffix("/reviews"))
            .and_then(|rest| rest.rsplit_once("/pulls/"))
            .map(|(repo, n)| (Repo::parse(repo), n.parse::<u64>()));
        assert!(
            matches!(number, Some((Some(_), Ok(_)))),
            "not a review post: {args:?}"
        );
        assert_eq!(args[2..5], ["--method", "POST", "--input"], "{args:?}");
        posts += 1;
    }
    posts
}

#[derive(Default)]
struct FakeBuilding {
    floors: Mutex<Vec<Floor>>,
    hires: Mutex<Vec<(String, brief::Job)>>,
    refuse: Mutex<Option<String>>,
    left: Mutex<BTreeSet<String>>,
    dismissed: Mutex<Vec<String>>,
    recorded: Mutex<Vec<(String, Review)>>,
}

impl Building for FakeBuilding {
    fn qa_on(&self) -> Vec<Floor> {
        lock(&self.floors).clone()
    }

    fn record_review(&self, root: &str, review: Review) -> Result<(), String> {
        lock(&self.recorded).push((root.to_string(), review.clone()));
        if let Some(floor) = lock(&self.floors).iter_mut().find(|f| f.root == root) {
            floor.reviews.push(review);
        }
        Ok(())
    }

    fn hire_reviewer(&self, project_id: &str, job: &brief::Job) -> Result<String, String> {
        if let Some(why) = lock(&self.refuse).clone() {
            return Err(why);
        }
        let mut hires = lock(&self.hires);
        hires.push((project_id.to_string(), job.clone()));
        Ok(format!("t-{}", hires.len()))
    }

    fn reviewer_left(&self, terminal: &str) -> bool {
        lock(&self.left).contains(terminal)
    }

    fn dismiss_reviewer(&self, terminal: &str) -> Result<(), String> {
        lock(&self.dismissed).push(terminal.to_string());
        Ok(())
    }
}

/// A watch on fakes in a temp folder, with floor `p-a` on.
struct Rig {
    dir: PathBuf,
    script: Arc<Mutex<Script>>,
    fake: Arc<FakeRunner>,
    settings: Arc<SettingsStore>,
    watch: Watch,
    building: FakeBuilding,
    changes: Arc<AtomicUsize>,
}

impl Rig {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("ob-qa-watch-{}-{n}", std::process::id()));
        fs::create_dir_all(&dir).expect("temp folder");
        let script = Arc::new(Mutex::new(Script::default()));
        let answering = Arc::clone(&script);
        let fake = Arc::new(FakeRunner::new(move |call| Ok(answer(&answering, call))));
        let settings = Arc::new(SettingsStore::open(dir.join("settings.json")));
        let changes = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&changes);
        let watch = Watch::new(
            fake.clone(),
            Arc::clone(&settings),
            dir.join("data"),
            Box::new(move || {
                counted.fetch_add(1, Ordering::Relaxed);
            }),
        );
        let rig = Self {
            dir,
            script,
            fake,
            settings,
            watch,
            building: FakeBuilding::default(),
            changes,
        };
        rig.add_floor("p-a", Vec::new());
        rig
    }

    fn root(&self, project_id: &str) -> String {
        self.dir.join(project_id).to_string_lossy().into_owned()
    }

    fn data(&self) -> PathBuf {
        self.dir.join("data")
    }

    fn worktree(&self, project_id: &str) -> PathBuf {
        worktrees_dir(&self.data()).join(project_id)
    }

    fn add_floor(&self, project_id: &str, reviews: Vec<Review>) {
        lock(&self.building.floors).push(Floor {
            project_id: project_id.to_string(),
            root: self.root(project_id),
            reviews,
        });
    }

    fn script(&self) -> std::sync::MutexGuard<'_, Script> {
        lock(&self.script)
    }

    fn list(&self, repo: &str, prs: &[Pr]) {
        self.script().lists.insert(repo.to_string(), listing(prs));
    }

    fn qa(&self, change: impl FnOnce(&mut QaSettings)) {
        let mut qa = self.settings.get().qa;
        change(&mut qa);
        let patch = SettingsPatch {
            qa: Some(qa),
            ..SettingsPatch::default()
        };
        self.settings.update(patch).expect("settings");
    }

    fn pass(&self, at: DateTime<Utc>) {
        self.watch.pass(&self.building, at);
    }

    /// Write the reviewer's Stop-hook input for floor `project_id`.
    fn report(&self, project_id: &str, reply: &str) {
        let path = runs_dir(&self.data()).join(project_id).join(REPORT_FILE);
        fs::create_dir_all(path.parent().expect("parent")).expect("runs folder");
        let input = json!({ "hook_event_name": "Stop", "last_assistant_message": reply });
        fs::write(path, input.to_string()).expect("report");
    }

    fn names(&self) -> Vec<String> {
        self.fake.calls().iter().map(name).collect()
    }

    fn count(&self, call: &str) -> usize {
        self.names().iter().filter(|n| *n == call).count()
    }

    fn hires(&self) -> usize {
        lock(&self.building.hires).len()
    }

    fn outcomes(&self) -> Vec<ReviewOutcome> {
        lock(&self.building.recorded)
            .iter()
            .map(|(_, r)| r.outcome.clone())
            .collect()
    }

    fn status(&self, project_id: &str) -> QaStatus {
        self.watch.view(&self.root(project_id)).status
    }

    fn payload(&self, n: usize) -> Value {
        serde_json::from_str(&self.script().payloads[n]).expect("payload json")
    }

    /// A review of #12 at head `a` started on floor `p-a`: seen at minute 0, started at 5.
    fn started() -> Self {
        let rig = Self::new();
        rig.list(WIDGETS, &[pr(12, 'a')]);
        rig.pass(mins(0));
        rig.pass(mins(5));
        assert_eq!(rig.hires(), 1);
        rig
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert_gh_only_posts_reviews(&self.fake.calls());
        }
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// Makes a run end one way.
type Ending = fn(&Rig);

/// Makes one step fail, or work again.
type Failing<'a> = &'a dyn Fn(&Rig, bool);

fn failed(reason: &str) -> ReviewOutcome {
    ReviewOutcome::Failed {
        reason: reason.to_string(),
    }
}

#[test]
fn a_new_pull_request_starts_only_once_its_head_has_stood_still() {
    let rig = Rig::new();
    rig.list(WIDGETS, &[pr(12, 'a')]);
    rig.pass(mins(0));
    assert_eq!(rig.hires(), 0);
    assert_eq!(rig.count("fetch"), 0);
    rig.pass(mins(2));
    assert_eq!(rig.hires(), 0);
    rig.pass(mins(5));
    let steps: Vec<String> = rig
        .names()
        .into_iter()
        .filter(|n| ["fetch", "cat-file", "pr diff", "worktree add"].contains(&n.as_str()))
        .collect();
    assert_eq!(steps, ["fetch", "cat-file", "pr diff", "worktree add"]);
    let worktree = rig.worktree("p-a");
    let handed = worktree.join(HANDOVER_DIR).join(DIFF_FILE);
    assert_eq!(fs::read_to_string(handed).expect("diff file"), DIFF);
    let report = runs_dir(&rig.data()).join("p-a").join(REPORT_FILE);
    let want = brief::job(worktree, report, 12, &title(12));
    assert_eq!(*lock(&rig.building.hires), vec![("p-a".to_string(), want)]);
    let (run, terminal) = rig.watch.view(&rig.root("p-a")).run.expect("run");
    assert_eq!(
        (run.number, run.title, terminal),
        (12, title(12), "t-1".into())
    );
    assert_eq!(run.started_at, "2026-11-12T10:05:00Z");
}

#[test]
fn a_report_is_posted_as_one_comment_review_and_recorded() {
    let rig = Rig::started();
    rig.report(
        "p-a",
        &reply("concerns", &[("src/upload.rs", 12), ("src/upload.rs", 99)]),
    );
    rig.pass(secs(5 * 60 + 15));
    assert_eq!(*lock(&rig.building.dismissed), vec!["t-1".to_string()]);
    assert!(!rig.worktree("p-a").exists());
    assert_eq!(rig.count("worktree remove"), 1);
    assert!(!runs_dir(&rig.data()).join("p-a").join(REPORT_FILE).exists());
    assert_eq!(rig.count("api"), 1);
    let api = rig.fake.calls().into_iter().find(|c| name(c) == "api");
    let args = strings(&api.expect("api call"));
    assert_eq!(args[1], "repos/example-org/widgets/pulls/12/reviews");
    let payload = rig.payload(0);
    assert_eq!(payload["event"], "COMMENT");
    assert_eq!(payload["commit_id"], sha('a'));
    let comments = payload["comments"].as_array().expect("comments");
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0]["line"], 12);
    let body = payload["body"].as_str().expect("body");
    assert!(body.contains("`src/upload.rs:99`"), "{body}");
    assert!(
        !runs_dir(&rig.data())
            .join("p-a")
            .join("review-12.json")
            .exists()
    );
    let posted = ReviewOutcome::Posted {
        verdict: Verdict::Concerns,
        findings: 2,
        review_id: 90210,
    };
    assert_eq!(rig.outcomes(), vec![posted]);
    let recorded = lock(&rig.building.recorded)[0].clone();
    assert_eq!(recorded.0, rig.root("p-a"));
    assert_eq!(
        (recorded.1.repo.as_str(), recorded.1.number, recorded.1.head),
        (WIDGETS, 12, sha('a'))
    );
    assert_eq!(recorded.1.at, "2026-11-12T10:05:15Z");
    let view = rig.watch.view(&rig.root("p-a"));
    assert_eq!((view.run, view.stamp), (None, Some(Verdict::Concerns)));
}

#[test]
fn the_same_head_is_never_started_twice_and_a_new_one_follows_every_push() {
    for every_push in [true, false] {
        let rig = Rig::started();
        rig.qa(|qa| qa.every_push = every_push);
        rig.report("p-a", &reply("pass", &[]));
        rig.pass(secs(5 * 60 + 15));
        for m in (6..=60).step_by(3) {
            rig.pass(mins(m));
        }
        rig.watch.recheck();
        rig.pass(secs(60 * 60 + 15));
        assert_eq!(rig.hires(), 1);
        rig.list(WIDGETS, &[pr(12, 'b')]);
        for m in (61..=90).step_by(5) {
            rig.pass(mins(m));
        }
        assert_eq!(rig.hires(), if every_push { 2 } else { 1 }, "{every_push}");
    }
}

#[test]
fn drafts_and_no_qa_labels_are_skipped() {
    let rig = Rig::new();
    let mut draft = pr(12, 'a');
    draft.draft = true;
    let mut shouting = pr(13, 'b');
    shouting.labels = vec!["No-QA"];
    let mut labelled = pr(14, 'c');
    labelled.labels = vec!["bug", "no-qa"];
    rig.list(WIDGETS, &[draft, shouting, labelled]);
    rig.pass(mins(0));
    rig.pass(mins(5));
    assert_eq!(rig.hires(), 0);
    rig.qa(|qa| qa.skip_drafts = false);
    rig.pass(mins(10));
    let hires = lock(&rig.building.hires).clone();
    assert_eq!(hires.len(), 1);
    assert_eq!(hires[0].1.title, "QA review of #12");
}

#[test]
fn reviews_per_day_holds_a_third_head_until_the_oldest_is_a_day_old() {
    let rig = Rig::new();
    rig.qa(|qa| qa.reviews_per_day = 2);
    let old = |head: char, at: DateTime<Utc>| Review {
        repo: WIDGETS.into(),
        number: 12,
        title: title(12),
        head: sha(head),
        at: timestamp(at),
        outcome: ReviewOutcome::Clean,
    };
    lock(&rig.building.floors)[0].reviews = vec![
        old('a', t0() - TimeDelta::hours(23)),
        old('b', t0() - TimeDelta::hours(1)),
    ];
    rig.list(WIDGETS, &[pr(12, 'c')]);
    rig.pass(mins(0));
    rig.pass(mins(5));
    rig.pass(mins(55));
    assert_eq!(rig.hires(), 0);
    rig.pass(mins(65));
    assert_eq!(rig.hires(), 1);
}

#[test]
fn max_concurrent_one_starts_the_second_floor_after_the_first_ends() {
    let rig = Rig::new();
    rig.add_floor("p-b", Vec::new());
    rig.script().origins.insert(
        rig.root("p-b"),
        Some("https://github.com/example-org/sprockets.git".into()),
    );
    rig.list(WIDGETS, &[pr(12, 'a')]);
    rig.list(SPROCKETS, &[pr(7, 'b')]);
    rig.pass(mins(0));
    rig.pass(mins(5));
    rig.pass(mins(6));
    let floors = || -> Vec<String> {
        lock(&rig.building.hires)
            .iter()
            .map(|(p, _)| p.clone())
            .collect()
    };
    assert_eq!(floors(), ["p-a"]);
    rig.report("p-a", &reply("pass", &[]));
    rig.pass(mins(7));
    assert_eq!(floors(), ["p-a", "p-b"]);
}

#[test]
fn one_floor_never_runs_two_reviews() {
    let rig = Rig::new();
    rig.qa(|qa| qa.max_concurrent = 2);
    rig.list(WIDGETS, &[pr(12, 'a'), pr(13, 'b')]);
    for m in [0, 5, 10, 15] {
        rig.pass(mins(m));
    }
    assert_eq!(rig.hires(), 1);
}

#[test]
fn a_clean_report_posts_a_short_summary_or_nothing() {
    let rig = Rig::started();
    rig.report("p-a", &reply("pass", &[]));
    rig.pass(mins(6));
    assert_eq!(rig.count("api"), 1);
    let payload = rig.payload(0);
    let body = payload["body"].as_str().expect("body");
    assert!(body.contains("No issues found."), "{body}");
    assert_eq!(payload["comments"], json!([]));

    let rig = Rig::started();
    rig.qa(|qa| qa.post_when_clean = false);
    rig.report("p-a", &reply("pass", &[]));
    rig.pass(mins(6));
    assert_eq!(rig.count("api"), 0);
    assert_eq!(rig.outcomes(), vec![ReviewOutcome::Clean]);
    assert_eq!(rig.watch.view(&rig.root("p-a")).stamp, Some(Verdict::Pass));
}

#[test]
fn a_run_that_ends_without_findings_is_a_failure_and_posts_nothing() {
    let cases: [(Ending, &str); 3] = [
        (
            |rig| {
                lock(&rig.building.left).insert("t-1".into());
            },
            LEFT,
        ),
        (|_| {}, OVERRAN),
        (
            |rig| rig.report("p-a", "Looks fine to me, nothing to flag."),
            NOT_FINDINGS,
        ),
    ];
    for (end, reason) in cases {
        let rig = Rig::started();
        rig.pass(mins(24));
        assert!(rig.outcomes().is_empty(), "{reason}");
        end(&rig);
        rig.pass(mins(25));
        assert_eq!(*lock(&rig.building.dismissed), vec!["t-1".to_string()]);
        assert!(!rig.worktree("p-a").exists(), "{reason}");
        assert_eq!(rig.outcomes(), vec![failed(reason)]);
        assert_eq!(rig.count("api"), 0, "{reason}");
    }
}

mod repeats;

#[test]
fn a_post_that_fails_is_retried_after_the_next_check_in_the_summary_form() {
    let rig = Rig::started();
    rig.script()
        .posts
        .push_back(exit(1, "HTTP 422: Validation Failed"));
    rig.report("p-a", &reply("concerns", &[("src/upload.rs", 12)]));
    rig.pass(secs(5 * 60 + 15));
    assert_eq!(rig.count("api"), 1);
    assert_eq!(rig.payload(0)["comments"].as_array().map(Vec::len), Some(1));
    rig.pass(mins(6));
    assert_eq!(rig.count("api"), 1);
    assert!(rig.outcomes().is_empty());
    rig.pass(mins(10));
    assert_eq!(rig.count("api"), 2);
    let second = rig.payload(1);
    assert_eq!(second["comments"], json!([]));
    let body = second["body"].as_str().expect("body");
    assert!(body.contains("Findings:\n- `src/upload.rs:12`"), "{body}");
    assert!(matches!(
        rig.outcomes()[..],
        [ReviewOutcome::Posted {
            review_id: 90210,
            ..
        }]
    ));
    assert_eq!(rig.hires(), 1);
}

#[test]
fn a_post_that_fails_three_times_is_recorded_and_its_payload_kept() {
    let rig = Rig::started();
    for _ in 0..3 {
        rig.script()
            .posts
            .push_back(exit(1, "HTTP 502: Bad Gateway"));
    }
    rig.report("p-a", &reply("concerns", &[("src/upload.rs", 12)]));
    for at in [secs(5 * 60 + 15), mins(6), mins(10), mins(12)] {
        rig.pass(at);
        assert!(rig.outcomes().is_empty());
    }
    rig.pass(mins(15));
    assert_eq!(rig.count("api"), 3);
    assert_eq!(
        rig.outcomes(),
        vec![failed(
            "the review could not be posted: GitHub could not be asked: HTTP 502: Bad Gateway"
        )]
    );
    assert!(
        runs_dir(&rig.data())
            .join("p-a")
            .join("review-12.json")
            .exists()
    );
    rig.pass(mins(20));
    assert_eq!((rig.hires(), rig.count("api")), (1, 3));
}

#[test]
fn signed_out_backs_off_rate_limits_wait_ten_minutes_and_success_resets() {
    let rig = Rig::new();
    rig.qa(|qa| qa.poll_minutes = 1);
    rig.script()
        .lists
        .insert(WIDGETS.into(), exit(4, "To get started with GitHub CLI"));
    rig.pass(mins(0));
    assert_eq!(
        rig.status("p-a"),
        QaStatus::Blocked {
            reason: GhError::SignedOut.words()
        }
    );
    assert_eq!(lock(&rig.watch.state).next_check, Some(mins(2)));
    rig.pass(mins(1));
    assert_eq!(rig.count("pr list"), 1);
    rig.script()
        .lists
        .insert(WIDGETS.into(), exit(1, "API rate limit exceeded for user"));
    rig.pass(mins(2));
    assert_eq!(
        rig.status("p-a"),
        QaStatus::Blocked {
            reason: "GitHub asked the app to slow down".into()
        }
    );
    assert_eq!(lock(&rig.watch.state).next_check, Some(mins(12)));
    rig.list(WIDGETS, &[]);
    rig.pass(mins(12));
    assert!(matches!(
        rig.status("p-a"),
        QaStatus::Watching { open: 0, .. }
    ));
    let state = lock(&rig.watch.state);
    assert_eq!((state.next_check, state.failures), (Some(mins(13)), 0));
}

#[test]
fn a_floor_without_a_github_origin_is_blocked_and_never_asks_github() {
    let rig = Rig::new();
    rig.add_floor("p-b", Vec::new());
    rig.script().origins.insert(rig.root("p-a"), None);
    rig.script().origins.insert(
        rig.root("p-b"),
        Some("https://gitlab.com/example-org/widgets.git".into()),
    );
    rig.pass(mins(0));
    let blocked = QaStatus::Blocked {
        reason: NO_ORIGIN.into(),
    };
    assert_eq!(
        (rig.status("p-a"), rig.status("p-b")),
        (blocked.clone(), blocked)
    );
    assert_eq!(rig.count("pr list"), 0);
    let state = lock(&rig.watch.state);
    assert_eq!((state.next_check, state.failures), (Some(mins(5)), 0));
}

#[test]
fn without_gh_every_floor_is_blocked_with_the_install_sentence() {
    let rig = Rig::new();
    rig.add_floor("p-b", Vec::new());
    rig.script().probe = format!("{GIT}\n");
    rig.pass(mins(0));
    let blocked = QaStatus::Blocked {
        reason: NO_GH.into(),
    };
    assert_eq!(
        (rig.status("p-a"), rig.status("p-b")),
        (blocked.clone(), blocked)
    );
    assert_eq!(lock(&rig.watch.state).failures, 1);
}

#[test]
fn a_failed_fetch_or_hire_records_nothing_and_waits_for_the_next_check() {
    let fetch_fails = |rig: &Rig, fail: bool| {
        rig.script().fetch = if fail {
            exit(128, "fatal: couldn't find remote ref")
        } else {
            ok("")
        };
    };
    let hire_fails = |rig: &Rig, fail: bool| {
        *lock(&rig.building.refuse) = fail.then(|| "Claude Code is not installed".to_string());
    };
    let cases: [(Failing<'_>, &str); 2] = [
        (
            &fetch_fails,
            "git could not fetch the pull request: fatal: couldn't find remote ref",
        ),
        (&hire_fails, "Claude Code is not installed"),
    ];
    for (fail, reason) in cases {
        let rig = Rig::new();
        fail(&rig, true);
        rig.list(WIDGETS, &[pr(12, 'a')]);
        rig.pass(mins(0));
        rig.pass(mins(5));
        assert_eq!(
            rig.status("p-a"),
            QaStatus::Blocked {
                reason: reason.into()
            }
        );
        assert!(!rig.worktree("p-a").exists());
        rig.pass(mins(6));
        rig.pass(mins(8));
        assert_eq!(rig.count("fetch"), 1, "{reason}");
        assert!(rig.outcomes().is_empty());
        fail(&rig, false);
        rig.pass(mins(10));
        assert_eq!(rig.hires(), 1, "{reason}");
        assert!(rig.outcomes().is_empty());
    }
}

#[test]
fn an_empty_diff_is_clean_and_a_huge_one_fails_without_a_reviewer() {
    for (diff, outcome) in [
        (" \n".to_string(), ReviewOutcome::Clean),
        ("+".repeat(DIFF_MAX + 1), failed(TOO_LARGE)),
    ] {
        let rig = Rig::new();
        rig.script().diff = ok(&diff);
        rig.list(WIDGETS, &[pr(12, 'a')]);
        rig.pass(mins(0));
        rig.pass(mins(5));
        assert_eq!(rig.outcomes(), vec![outcome]);
        assert_eq!((rig.hires(), rig.count("worktree add")), (0, 0));
        rig.pass(mins(10));
        assert_eq!(rig.count("pr diff"), 1);
    }
}

#[test]
fn a_floor_switched_off_mid_review_ends_it_and_records_nothing() {
    let rig = Rig::started();
    assert!(rig.worktree("p-a").exists());
    lock(&rig.building.floors).clear();
    rig.pass(mins(6));
    assert_eq!(*lock(&rig.building.dismissed), vec!["t-1".to_string()]);
    assert!(!rig.worktree("p-a").exists());
    assert!(rig.outcomes().is_empty());
    assert_eq!(rig.watch.view(&rig.root("p-a")), View::waiting());
}

#[test]
fn the_first_pass_sweeps_a_worktree_left_behind() {
    let rig = Rig::new();
    let left = rig.worktree("p-a");
    fs::create_dir_all(left.join("src")).expect("left worktree");
    rig.pass(mins(0));
    assert!(!left.exists());
    assert_eq!(rig.count("worktree remove"), 1);
    fs::create_dir_all(&left).expect("again");
    rig.pass(mins(5));
    assert!(left.exists());
}

#[test]
fn the_stamp_follows_the_current_heads() {
    for unlisted in [false, true] {
        let rig = Rig::started();
        rig.qa(|qa| qa.post_when_clean = false);
        rig.report("p-a", &reply("pass", &[]));
        rig.pass(mins(6));
        let stamp = |rig: &Rig| rig.watch.view(&rig.root("p-a")).stamp;
        assert_eq!(stamp(&rig), Some(Verdict::Pass));
        if unlisted {
            rig.list(WIDGETS, &[]);
        } else {
            rig.list(WIDGETS, &[pr(12, 'b')]);
        }
        rig.pass(mins(10));
        assert_eq!(stamp(&rig), None, "{unlisted}");
    }
}

#[test]
fn changed_is_called_only_after_a_pass_that_changed_something() {
    let rig = Rig::new();
    rig.list(WIDGETS, &[pr(12, 'a')]);
    rig.pass(mins(0));
    assert_eq!(rig.changes.load(Ordering::Relaxed), 1);
    rig.pass(secs(15));
    rig.pass(secs(30));
    assert_eq!(rig.changes.load(Ordering::Relaxed), 1);
    rig.pass(mins(5));
    assert_eq!(rig.changes.load(Ordering::Relaxed), 2);
}

#[test]
fn recheck_asks_github_before_the_interval_is_up() {
    let rig = Rig::new();
    rig.pass(mins(0));
    rig.pass(mins(1));
    assert_eq!(rig.count("pr list"), 1);
    rig.watch.recheck();
    rig.pass(secs(75));
    assert_eq!(rig.count("pr list"), 2);
    rig.pass(mins(2));
    assert_eq!(rig.count("pr list"), 2);
}

#[test]
fn the_only_gh_call_that_writes_is_the_review_post() {
    let rig = Rig::started();
    rig.report("p-a", &reply("concerns", &[("src/upload.rs", 12)]));
    for m in 6..=20 {
        rig.pass(mins(m));
    }
    let calls = rig.fake.calls();
    assert!(calls.iter().any(|c| name(c) == "pr list"));
    assert!(calls.iter().any(|c| name(c) == "pr diff"));
    assert_eq!(assert_gh_only_posts_reviews(&calls), 1);
}
