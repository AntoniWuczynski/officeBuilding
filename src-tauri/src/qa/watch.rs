//! The pull request watch: on its own thread, it asks GitHub for A Human's open pull requests
//! on each floor with QA on, starts a reviewer on each new head commit once it has stood still,
//! settles what the reviewer said and posts it.
//!
//! Lock order: the Office lock, then this watch's lock, never the other way. The watch calls
//! the `Building`, the runner and `changed` with its own lock released, always.

use super::brief;
use super::diff::ChangedLines;
use super::findings::{self, Report};
use super::github::{self, Found, GhError, GitHub, PullRequest, Repo};
use super::record::{Review, ReviewOutcome, Verdict};
use super::review::{self, Placement};
use super::worktree::Git;
use super::{runs_dir, worktrees_dir};
use crate::model::{QaRun, QaStatus};
use crate::poll::{self, Outcome, Poller, Schedule};
use crate::process::Runner;
use crate::settings::{QaSettings, SettingsStore};
use crate::store::write_json_atomic;
use crate::sync::lock;
use chrono::{DateTime, Utc};
use clock::{delta, later, since, timestamp, within_day};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

/// How often a pass runs.
pub const TICK: Duration = Duration::from_secs(15);
/// How long a head commit must have stood before it is reviewed.
pub const QUIET: Duration = Duration::from_mins(3);
/// How long a reviewer may run.
pub const REVIEW_LIMIT: Duration = Duration::from_mins(20);
/// The longest wait between GitHub checks after repeated failures.
pub const MAX_BACKOFF: Duration = Duration::from_mins(30);
/// The wait after GitHub says to slow down.
pub const RATE_LIMIT_WAIT: Duration = Duration::from_mins(10);
/// How often posting one review is tried.
pub const POST_TRIES: u32 = 3;
/// How often starting one review may fail before that head is given up.
pub const START_TRIES: u32 = 3;
/// The largest diff a reviewer is given, in bytes.
pub const DIFF_MAX: usize = 2 * 1024 * 1024;

const NO_QA_LABEL: &str = "no-qa";
const REPORT_FILE: &str = "report.json";
const NO_GH: &str = "GitHub's gh command was not found on this Mac. Install it, then run gh auth login in a terminal";
const NO_GIT: &str = "git was not found on this Mac";
const NO_ORIGIN: &str = "this floor's repo has no GitHub origin";
const LEFT: &str = "the reviewer left before it reported";
const TOO_LARGE: &str = "the diff is too large to review";
const UNREAD: &str = "the reviewer's report could not be read";

/// What the watch needs from the office. `Office` implements it. Tests use a fake.
pub trait Building: Send + Sync {
    /// Every standing floor with QA on.
    fn qa_on(&self) -> Vec<Floor>;
    /// Keep a finished review in the floor's record.
    fn record_review(&self, root: &str, review: Review) -> Result<(), String>;
    /// Seat a reviewer. Returns its terminal id.
    fn hire_reviewer(&self, project_id: &str, job: &brief::Job) -> Result<String, String>;
    fn reviewer_left(&self, terminal: &str) -> bool;
    fn dismiss_reviewer(&self, terminal: &str) -> Result<(), String>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Floor {
    pub project_id: String,
    /// The absolute repo root: the key its record is stored under.
    pub root: String,
    /// Every stored review, oldest first.
    pub reviews: Vec<Review>,
}

/// What the snapshot shows for one floor that is on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct View {
    /// Never `Off`.
    pub status: QaStatus,
    /// The running review with its reviewer's terminal.
    pub run: Option<(QaRun, String)>,
    pub stamp: Option<Verdict>,
}

impl View {
    fn waiting() -> Self {
        Self {
            status: QaStatus::Waiting,
            run: None,
            stamp: None,
        }
    }
}

/// One pull request at one head: what a review is of.
#[derive(Debug, Clone)]
struct Pull {
    repo: Repo,
    number: u64,
    title: String,
    head: String,
}

/// A listed pull request, with when its current head was first seen.
struct Open {
    pr: PullRequest,
    since: DateTime<Utc>,
}

struct Run {
    pull: Pull,
    lines: ChangedLines,
    terminal: String,
    started: DateTime<Utc>,
}

/// A settled review waiting to be posted.
struct Queued {
    pull: Pull,
    report: Report,
    lines: ChangedLines,
    /// Tries made so far.
    tries: u32,
    /// The pass of the last try.
    tried_in: u64,
}

/// One floor's desk, by repo root.
struct Desk {
    project_id: String,
    status: QaStatus,
    /// The repo and pull requests of the last good check, and when it was.
    repo: Option<Repo>,
    open: Vec<Open>,
    checked: Option<DateTime<Utc>>,
    stamp: Option<Verdict>,
    /// Start nothing until the next check.
    hold: bool,
    /// Failed starts by (number, head). A pull request that failed is tried after the rest.
    start_failures: BTreeMap<(u64, String), u32>,
    run: Option<Run>,
    queue: Vec<Queued>,
}

impl Desk {
    fn new(project_id: &str) -> Self {
        Self {
            project_id: project_id.to_string(),
            status: QaStatus::Waiting,
            repo: None,
            open: Vec::new(),
            checked: None,
            stamp: None,
            hold: false,
            start_failures: BTreeMap::new(),
            run: None,
            queue: Vec::new(),
        }
    }

    fn view(&self) -> View {
        View {
            status: self.status.clone(),
            run: self.run.as_ref().map(|run| {
                let qa_run = QaRun {
                    number: run.pull.number,
                    title: run.pull.title.clone(),
                    started_at: timestamp(run.started),
                    session_id: None,
                };
                (qa_run, run.terminal.clone())
            }),
            stamp: self.stamp,
        }
    }

    /// The verdict standing for the listed pull requests' current heads.
    fn restamp(&mut self, stored: &[Review]) {
        let Some(repo) = self.repo.as_ref().map(ToString::to_string) else {
            self.stamp = None;
            return;
        };
        let mut pass = false;
        for open in &self.open {
            let newest = stored
                .iter()
                .rev()
                .find(|r| r.repo == repo && r.number == open.pr.number && r.head == open.pr.head);
            match newest.map(|r| &r.outcome) {
                Some(ReviewOutcome::Posted {
                    verdict: Verdict::Concerns,
                    ..
                }) => {
                    self.stamp = Some(Verdict::Concerns);
                    return;
                }
                Some(
                    ReviewOutcome::Posted {
                        verdict: Verdict::Pass,
                        ..
                    }
                    | ReviewOutcome::Clean,
                ) => pass = true,
                Some(ReviewOutcome::Failed { .. }) | None => {}
            }
        }
        self.stamp = pass.then_some(Verdict::Pass);
    }

    /// The listed pull request with the lowest number that is due.
    /// `taken` says whether another floor on the same repository has the head in hand.
    fn due(
        &self,
        qa: &QaSettings,
        stored: &[Review],
        now: DateTime<Utc>,
        taken: &dyn Fn(u64, &str) -> bool,
    ) -> Option<Pull> {
        let repo = self.repo.as_ref()?;
        let checked = self.checked?;
        let slug = repo.to_string();
        let slug = slug.as_str();
        let of = |number: u64| {
            stored
                .iter()
                .filter(move |r| r.repo == slug && r.number == number)
        };
        self.open
            .iter()
            .filter(|o| {
                let pr = &o.pr;
                (!pr.draft || !qa.skip_drafts)
                    && !pr
                        .labels
                        .iter()
                        .any(|l| l.eq_ignore_ascii_case(NO_QA_LABEL))
                    && !of(pr.number).any(|r| r.head == pr.head)
                    && (qa.every_push || of(pr.number).next().is_none())
                    && of(pr.number).filter(|r| within_day(&r.at, now)).count()
                        < usize::try_from(qa.reviews_per_day).unwrap_or(usize::MAX)
                    && o.since + delta(QUIET) <= checked
                    && !taken(pr.number, &pr.head)
                    && !self
                        .queue
                        .iter()
                        .any(|q| q.pull.repo == *repo && q.pull.number == pr.number)
            })
            .min_by_key(|o| {
                let failed = self
                    .start_failures
                    .get(&(o.pr.number, o.pr.head.clone()))
                    .copied();
                (failed.unwrap_or(0), o.pr.number)
            })
            .map(|o| Pull {
                repo: repo.clone(),
                number: o.pr.number,
                title: o.pr.title.clone(),
                head: o.pr.head.clone(),
            })
    }
}

#[derive(Default)]
struct State {
    tools: Found,
    /// The tools were looked for at least once.
    looked: bool,
    /// The start-up sweep of left worktrees has run.
    swept: bool,
    /// None: check at the next pass.
    next_check: Option<DateTime<Utc>>,
    /// Checks in a row that were not `Ok`.
    failures: u32,
    check_now: bool,
    /// GitHub asked the app to slow down: no check is brought forward before this.
    limited_until: Option<DateTime<Utc>>,
    passes: u64,
    floors: BTreeMap<String, Desk>,
}

/// What one pass carries between its steps.
struct Pass<'a> {
    building: &'a dyn Building,
    now: DateTime<Utc>,
    qa: QaSettings,
    number: u64,
    /// Each floor's stored reviews, plus what this pass recorded.
    stored: BTreeMap<String, Vec<Review>>,
    recorded: bool,
}

impl Pass<'_> {
    /// Keep a finished review, in the building and for the rest of this pass.
    fn record(&mut self, root: &str, pull: &Pull, outcome: ReviewOutcome) {
        let review = Review {
            repo: pull.repo.to_string(),
            number: pull.number,
            title: pull.title.clone(),
            head: pull.head.clone(),
            at: timestamp(self.now),
            outcome,
        };
        // A store that cannot be saved still holds the review in memory, which is what
        // keeps the commit from being reviewed twice while the app runs.
        let _ = self.building.record_review(root, review.clone());
        self.stored
            .entry(root.to_string())
            .or_default()
            .push(review);
        self.recorded = true;
    }
}

/// How a floor's check went wrong, for the spacing of the next one.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Fault {
    /// Waiting does not fix it.
    Lasting,
    Failed,
    RateLimited,
}

/// How starting a review went.
enum Begun {
    Running(Run),
    /// Final for this commit before any reviewer existed.
    Recorded(ReviewOutcome),
    /// The head moved between the check and the fetch.
    Moved,
}

/// Why a review could not be started.
enum Unstarted {
    /// GitHub refused the pull request's own diff: counts towards `START_TRIES`.
    Head(String),
    /// Something outside the pull request (tools, network, disk, the reviewer): retried.
    Desk(String),
}

impl From<String> for Unstarted {
    fn from(why: String) -> Self {
        Self::Desk(why)
    }
}

impl From<&str> for Unstarted {
    fn from(why: &str) -> Self {
        Self::Desk(why.to_string())
    }
}

pub struct Watch {
    runner: Arc<dyn Runner>,
    settings: Arc<SettingsStore>,
    data: PathBuf,
    /// Called with no lock held, after anything a snapshot shows has changed.
    changed: Box<dyn Fn() + Send + Sync>,
    state: Mutex<State>,
    poller: OnceLock<Poller>,
}

impl Watch {
    #[must_use]
    pub fn new(
        runner: Arc<dyn Runner>,
        settings: Arc<SettingsStore>,
        data: PathBuf,
        changed: Box<dyn Fn() + Send + Sync>,
    ) -> Self {
        Self {
            runner,
            settings,
            data,
            changed,
            state: Mutex::new(State::default()),
            poller: OnceLock::new(),
        }
    }

    /// What to show for the floor at `root`. A floor never checked: `Waiting`, no run, no stamp.
    #[must_use]
    pub fn view(&self, root: &str) -> View {
        lock(&self.state)
            .floors
            .get(root)
            .map_or_else(View::waiting, Desk::view)
    }

    /// Ask GitHub on the next pass, and make that pass now. Not while GitHub has asked the
    /// app to slow down: that wait is kept.
    pub fn recheck(&self) {
        self.recheck_at(Utc::now());
    }

    fn recheck_at(&self, now: DateTime<Utc>) {
        {
            let mut state = lock(&self.state);
            if state.limited_until.is_none_or(|until| now >= until) {
                state.check_now = true;
            }
        }
        if let Some(poller) = self.poller.get() {
            poller.wake();
        }
    }

    /// One pass: settle what finished, ask GitHub when it is time, start what is due.
    pub fn pass(&self, building: &dyn Building, now: DateTime<Utc>) {
        let qa = self.settings.get().qa;
        let floors = building.qa_on();
        let (number, roots, before) = {
            let mut state = lock(&self.state);
            state.passes += 1;
            let roots: BTreeSet<String> = state
                .floors
                .keys()
                .cloned()
                .chain(floors.iter().map(|f| f.root.clone()))
                .collect();
            let before = views(&state, &roots);
            (state.passes, roots, before)
        };
        let mut pass = Pass {
            building,
            now,
            qa,
            number,
            stored: floors
                .iter()
                .map(|f| (f.root.clone(), f.reviews.clone()))
                .collect(),
            recorded: false,
        };
        if !lock(&self.state).looked {
            self.tools(&floors);
        }
        self.drop_gone(building, &floors);
        self.settle(&mut pass);
        let check = {
            let mut state = lock(&self.state);
            // Taken here, so a recheck asked during the check is kept for the next pass.
            let asked = std::mem::take(&mut state.check_now);
            asked || state.next_check.is_none_or(|at| now >= at)
        };
        if check {
            self.check(&mut pass, &floors);
        }
        self.start_due(&mut pass, &floors);
        let changed = {
            let mut state = lock(&self.state);
            for (root, desk) in &mut state.floors {
                desk.restamp(pass.stored.get(root).map_or(&[], Vec::as_slice));
            }
            views(&state, &roots) != before
        };
        if changed || pass.recorded {
            (self.changed)();
        }
    }

    fn worktree(&self, project_id: &str) -> PathBuf {
        worktrees_dir(&self.data).join(project_id)
    }

    fn report(&self, project_id: &str) -> PathBuf {
        runs_dir(&self.data).join(project_id).join(REPORT_FILE)
    }

    fn git(&self) -> Option<Git> {
        let git = lock(&self.state).tools.git.clone()?;
        Some(Git::new(Arc::clone(&self.runner), git))
    }

    fn github(&self) -> Option<GitHub> {
        let gh = lock(&self.state).tools.gh.clone()?;
        Some(GitHub::new(Arc::clone(&self.runner), gh))
    }

    /// Find `gh` and `git` unless both are known, then sweep once `git` is known.
    fn tools(&self, floors: &[Floor]) -> Found {
        let known = lock(&self.state).tools.clone();
        if floors.is_empty() {
            return known;
        }
        let tools = if known.gh.is_some() && known.git.is_some() {
            known
        } else {
            let found = github::find(self.runner.as_ref(), &crate::pty::login_shell());
            let mut state = lock(&self.state);
            state.tools = found.clone();
            state.looked = true;
            found
        };
        let sweep = {
            let mut state = lock(&self.state);
            let due = !state.swept && tools.git.is_some();
            state.swept |= due;
            due
        };
        if sweep {
            // A worktree a crash left behind. Nothing has started yet, so none is in use.
            for floor in floors {
                self.remove_worktree(&floor.root, &floor.project_id);
            }
        }
        tools
    }

    /// Best effort: a worktree that stays is replaced by the next `add`, or swept at start-up.
    fn remove_worktree(&self, root: &str, project_id: &str) {
        if let Some(git) = self.git() {
            let _ = git.remove(Path::new(root), &self.worktree(project_id));
        }
    }

    /// End a run's reviewer and clear what it worked in.
    fn end_run(&self, building: &dyn Building, root: &str, project_id: &str, terminal: &str) {
        // An error means the terminal was already gone.
        let _ = building.dismiss_reviewer(terminal);
        self.remove_worktree(root, project_id);
        // Not there is fine: there is nothing left to read.
        let _ = fs::remove_file(self.report(project_id));
    }

    /// Forget floors no longer on. A running review is ended and nothing is recorded.
    fn drop_gone(&self, building: &dyn Building, floors: &[Floor]) {
        let gone: Vec<(String, Desk)> = {
            let mut state = lock(&self.state);
            let on: BTreeSet<&str> = floors.iter().map(|f| f.root.as_str()).collect();
            let roots: Vec<String> = state
                .floors
                .keys()
                .filter(|root| !on.contains(root.as_str()))
                .cloned()
                .collect();
            let gone = roots
                .into_iter()
                .filter_map(|root| state.floors.remove(&root).map(|desk| (root, desk)))
                .collect();
            for floor in floors {
                state
                    .floors
                    .entry(floor.root.clone())
                    .or_insert_with(|| Desk::new(&floor.project_id));
            }
            gone
        };
        for (root, desk) in gone {
            if let Some(run) = desk.run {
                self.end_run(building, &root, &desk.project_id, &run.terminal);
            }
        }
    }

    /// End each run whose reviewer reported, left or overran, and act on what it said.
    fn settle(&self, pass: &mut Pass<'_>) {
        let runs: Vec<(String, String, String, DateTime<Utc>)> = lock(&self.state)
            .floors
            .iter()
            .filter_map(|(root, desk)| {
                let run = desk.run.as_ref()?;
                Some((
                    root.clone(),
                    desk.project_id.clone(),
                    run.terminal.clone(),
                    run.started,
                ))
            })
            .collect();
        for (root, project_id, terminal, started) in runs {
            let report = self.report(&project_id);
            let said = if report.exists() {
                fs::read_to_string(&report)
                    .map_err(|_| UNREAD.to_string())
                    .and_then(|text| findings::from_stop(&text))
            } else if pass.building.reviewer_left(&terminal) {
                Err(LEFT.to_string())
            } else if since(pass.now, started) >= REVIEW_LIMIT {
                Err(overran())
            } else {
                continue;
            };
            let Some(run) = lock(&self.state)
                .floors
                .get_mut(&root)
                .and_then(|desk| desk.run.take())
            else {
                continue;
            };
            self.end_run(pass.building, &root, &project_id, &terminal);
            match said {
                Err(reason) => {
                    pass.record(&root, &run.pull, ReviewOutcome::Failed { reason });
                }
                Ok(report) if report.findings.is_empty() && !pass.qa.post_when_clean => {
                    pass.record(&root, &run.pull, ReviewOutcome::Clean);
                }
                Ok(report) => {
                    let queued = Queued {
                        pull: run.pull,
                        report,
                        lines: run.lines,
                        tries: 0,
                        tried_in: 0,
                    };
                    self.post(pass, &root, &project_id, queued);
                }
            }
        }
    }

    /// Ask GitHub about every floor, retry waiting posts, and space the next check.
    fn check(&self, pass: &mut Pass<'_>, floors: &[Floor]) {
        let tools = self.tools(floors);
        let mut failed = false;
        let mut limited = false;
        for floor in floors {
            let checked = self.check_floor(&tools, floor);
            let mut state = lock(&self.state);
            let Some(desk) = state.floors.get_mut(&floor.root) else {
                continue;
            };
            match checked {
                Ok((repo, listed)) => {
                    desk.status = QaStatus::Watching {
                        repo: repo.to_string(),
                        checked_at: timestamp(pass.now),
                        open: u32::try_from(listed.len()).unwrap_or(u32::MAX),
                    };
                    let known = std::mem::take(&mut desk.open);
                    desk.open = listed
                        .into_iter()
                        .map(|pr| {
                            let since = known
                                .iter()
                                .find(|o| o.pr.number == pr.number && o.pr.head == pr.head)
                                .map_or(pass.now, |o| o.since);
                            Open { pr, since }
                        })
                        .collect();
                    desk.repo = Some(repo);
                    desk.checked = Some(pass.now);
                }
                Err((reason, fault)) => {
                    desk.status = QaStatus::Blocked { reason };
                    failed |= fault == Fault::Failed;
                    limited |= fault == Fault::RateLimited;
                }
            }
        }
        let waiting: Vec<(String, String, Queued)> = {
            let mut state = lock(&self.state);
            let mut waiting = Vec::new();
            for (root, desk) in &mut state.floors {
                let (now, later): (Vec<_>, Vec<_>) = std::mem::take(&mut desk.queue)
                    .into_iter()
                    .partition(|q| q.tried_in != pass.number);
                desk.queue = later;
                waiting.extend(
                    now.into_iter()
                        .map(|q| (root.clone(), desk.project_id.clone(), q)),
                );
            }
            waiting
        };
        for (root, project_id, queued) in waiting {
            self.post(pass, &root, &project_id, queued);
        }
        let outcome = if limited {
            Outcome::RetryAfter(RATE_LIMIT_WAIT)
        } else if failed {
            Outcome::Failed
        } else {
            Outcome::Ok
        };
        let every = Duration::from_secs(u64::from(pass.qa.poll_minutes) * 60);
        let schedule = Schedule {
            every,
            max_backoff: MAX_BACKOFF.max(every),
        };
        let mut state = lock(&self.state);
        state.failures = if outcome == Outcome::Ok {
            0
        } else {
            state.failures.saturating_add(1)
        };
        let wait = poll::next_wait(schedule, state.failures, outcome);
        state.next_check = Some(later(pass.now, wait));
        state.limited_until = limited.then(|| later(pass.now, wait));
        for desk in state.floors.values_mut() {
            desk.hold = false;
        }
    }

    /// One floor's open pull requests, or why they could not be listed.
    fn check_floor(
        &self,
        tools: &Found,
        floor: &Floor,
    ) -> Result<(Repo, Vec<PullRequest>), (String, Fault)> {
        let (Some(gh), Some(git)) = (&tools.gh, &tools.git) else {
            let missing = if tools.gh.is_none() { NO_GH } else { NO_GIT };
            return Err((missing.to_string(), Fault::Failed));
        };
        let origin = Git::new(Arc::clone(&self.runner), git.clone())
            .origin(Path::new(&floor.root))
            .map_err(|e| (e, Fault::Failed))?;
        let repo = origin
            .as_deref()
            .and_then(Repo::from_remote)
            .ok_or_else(|| (NO_ORIGIN.to_string(), Fault::Lasting))?;
        GitHub::new(Arc::clone(&self.runner), gh.clone())
            .mine(&repo)
            .map(|listed| (repo, listed))
            .map_err(|e| {
                let fault = if e == GhError::RateLimited {
                    Fault::RateLimited
                } else {
                    Fault::Failed
                };
                (e.words(), fault)
            })
    }

    /// Start the due pull request of each free floor, in project id order, up to the cap.
    fn start_due(&self, pass: &mut Pass<'_>, floors: &[Floor]) {
        let (mut running, mut due) = {
            let state = lock(&self.state);
            let running = state.floors.values().filter(|d| d.run.is_some()).count();
            let due: Vec<(String, String, Pull)> = floors
                .iter()
                .filter_map(|floor| {
                    let desk = state.floors.get(&floor.root)?;
                    let free = desk.run.is_none()
                        && !desk.hold
                        && matches!(desk.status, QaStatus::Watching { .. });
                    let stored = pass.stored.get(&floor.root).map_or(&[][..], Vec::as_slice);
                    let repo = desk.repo.as_ref()?;
                    let slug = repo.to_string();
                    let taken = |number: u64, head: &str| {
                        let same =
                            |r: &Review| r.repo == slug && r.number == number && r.head == head;
                        let is = |p: &Pull| p.repo == *repo && p.number == number && p.head == head;
                        state
                            .floors
                            .iter()
                            .filter(|(other, _)| **other != floor.root)
                            .any(|(_, d)| {
                                d.run.as_ref().is_some_and(|run| is(&run.pull))
                                    || d.queue.iter().any(|q| is(&q.pull))
                            })
                            // A floor without a good check this session still keeps its reviews.
                            || pass
                                .stored
                                .iter()
                                .filter(|(other, _)| **other != floor.root)
                                .any(|(_, rs)| rs.iter().any(same))
                    };
                    let pull = free.then(|| desk.due(&pass.qa, stored, pass.now, &taken))??;
                    Some((floor.root.clone(), floor.project_id.clone(), pull))
                })
                .collect();
            (running, due)
        };
        due.sort_by(|a, b| a.1.cmp(&b.1));
        let cap = usize::try_from(pass.qa.max_concurrent).unwrap_or(usize::MAX);
        // Two floors on one repository may both find the same head due in one pass.
        let mut begun: Vec<(Repo, u64, String)> = Vec::new();
        for (root, project_id, pull) in due {
            if running >= cap {
                break;
            }
            let key = (pull.repo.clone(), pull.number, pull.head.clone());
            if begun.contains(&key) {
                continue;
            }
            match self.begin(pass, &root, &project_id, &pull) {
                Ok(Begun::Running(run)) => {
                    self.start_settled(&root, &pull);
                    begun.push(key);
                    if let Some(desk) = lock(&self.state).floors.get_mut(&root) {
                        desk.run = Some(run);
                    }
                    running += 1;
                }
                Ok(Begun::Recorded(outcome)) => {
                    self.start_settled(&root, &pull);
                    begun.push(key);
                    pass.record(&root, &pull, outcome);
                }
                Ok(Begun::Moved) => self.hold(&root, None),
                Err(unstarted) => {
                    self.remove_worktree(&root, &project_id);
                    let (reason, head) = match unstarted {
                        Unstarted::Head(why) => (why, true),
                        Unstarted::Desk(why) => (why, false),
                    };
                    if head && self.start_failed(&root, &pull) {
                        let reason = format!("the review could not be started: {reason}");
                        pass.record(&root, &pull, ReviewOutcome::Failed { reason });
                    }
                    self.hold(&root, Some(reason));
                }
            }
        }
    }

    /// Forget the failed starts of a pull request that has now started or been settled.
    fn start_settled(&self, root: &str, pull: &Pull) {
        if let Some(desk) = lock(&self.state).floors.get_mut(root) {
            desk.start_failures
                .remove(&(pull.number, pull.head.clone()));
        }
    }

    /// Count a failed start. True when the head has failed `START_TRIES` times and is given up.
    fn start_failed(&self, root: &str, pull: &Pull) -> bool {
        let mut state = lock(&self.state);
        let Some(desk) = state.floors.get_mut(root) else {
            return false;
        };
        let key = (pull.number, pull.head.clone());
        let failures = desk.start_failures.entry(key.clone()).or_insert(0);
        *failures += 1;
        let give_up = *failures >= START_TRIES;
        if give_up {
            desk.start_failures.remove(&key);
        }
        give_up
    }

    /// Start nothing more on the floor until the next check, blocked for `reason` if given.
    fn hold(&self, root: &str, reason: Option<String>) {
        if let Some(desk) = lock(&self.state).floors.get_mut(root) {
            desk.hold = true;
            if let Some(reason) = reason {
                desk.status = QaStatus::Blocked { reason };
            }
        }
    }

    /// Fetch, diff, check out and hire.
    fn begin(
        &self,
        pass: &Pass<'_>,
        root: &str,
        project_id: &str,
        pull: &Pull,
    ) -> Result<Begun, Unstarted> {
        let git = self.git().ok_or(NO_GIT)?;
        let github = self.github().ok_or(NO_GH)?;
        let root = Path::new(root);
        git.fetch_pull(root, pull.number)?;
        if !git.has_commit(root, &pull.head) {
            return Ok(Begun::Moved);
        }
        let diff = github.diff(&pull.repo, pull.number).map_err(|e| match e {
            GhError::Failed(_) => Unstarted::Head(e.words()),
            GhError::SignedOut | GhError::RateLimited => Unstarted::Desk(e.words()),
        })?;
        if diff.trim().is_empty() {
            return Ok(Begun::Recorded(ReviewOutcome::Clean));
        }
        if diff.len() > DIFF_MAX {
            return Ok(Begun::Recorded(ReviewOutcome::Failed {
                reason: TOO_LARGE.to_string(),
            }));
        }
        let worktree = self.worktree(project_id);
        git.add(root, &worktree, &pull.head)?;
        brief::hand_over(&worktree, &diff)
            .map_err(|e| format!("the diff could not be handed over: {e}"))?;
        let report = self.report(project_id);
        match fs::remove_file(&report) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                return Err(format!("an old report could not be removed: {e}").into());
            }
            _ => {}
        }
        let job = brief::job(worktree, report, pull.number, &pull.title);
        let terminal = pass.building.hire_reviewer(project_id, &job)?;
        Ok(Begun::Running(Run {
            pull: pull.clone(),
            lines: ChangedLines::parse(&diff),
            terminal,
            started: pass.now,
        }))
    }

    /// Try posting `queued` once more.
    fn post(&self, pass: &mut Pass<'_>, root: &str, project_id: &str, mut queued: Queued) {
        queued.tries += 1;
        let placement = if queued.tries == 1 {
            Placement::Inline
        } else {
            Placement::Summary
        };
        let payload = review::build(&queued.report, &queued.lines, &queued.pull.head, placement);
        let file = runs_dir(&self.data)
            .join(project_id)
            .join(format!("review-{}.json", queued.pull.number));
        let posted = self
            .github()
            .ok_or_else(|| NO_GH.to_string())
            .and_then(|gh| {
                let pull = &queued.pull;
                // A post that failed on this side may have reached GitHub: look before repeating it.
                if queued.tries > 1
                    && let Some(id) = gh
                        .review_of(&pull.repo, pull.number, &pull.head)
                        .map_err(|e| e.words())?
                {
                    return Ok(id);
                }
                write_json_atomic(&file, &payload)?;
                gh.post(&pull.repo, pull.number, &file)
                    .map_err(|e| e.words())
            });
        match posted {
            Ok(review_id) => {
                // The review is on GitHub: a payload left behind is only clutter.
                let _ = fs::remove_file(&file);
                let outcome = ReviewOutcome::Posted {
                    verdict: queued.report.verdict,
                    findings: u32::try_from(queued.report.findings.len()).unwrap_or(u32::MAX),
                    review_id,
                };
                pass.record(root, &queued.pull, outcome);
            }
            Err(_) if queued.tries < POST_TRIES => {
                queued.tried_in = pass.number;
                if let Some(desk) = lock(&self.state).floors.get_mut(root) {
                    desk.queue.push(queued);
                }
            }
            Err(words) => {
                let reason = format!("the review could not be posted: {words}");
                pass.record(root, &queued.pull, ReviewOutcome::Failed { reason });
            }
        }
    }
}

/// What `view` returns for each of `roots`.
fn views(state: &State, roots: &BTreeSet<String>) -> Vec<View> {
    roots
        .iter()
        .map(|root| {
            state
                .floors
                .get(root)
                .map_or_else(View::waiting, Desk::view)
        })
        .collect()
}

/// Why a review that ran out of time was ended.
fn overran() -> String {
    format!(
        "the review ran past {} minutes and was stopped",
        REVIEW_LIMIT.as_secs() / 60
    )
}

/// Start the watch's thread ("qa-watch").
pub fn start(watch: &Arc<Watch>, building: Arc<dyn Building>) -> Result<(), String> {
    let watched = Arc::clone(watch);
    let schedule = Schedule {
        every: TICK,
        max_backoff: TICK,
    };
    let running = Poller::start("qa-watch", schedule, move || {
        watched.pass(building.as_ref(), Utc::now());
        // The watch spaces its own GitHub checks, so the poller never holds a wake back.
        Outcome::Ok
    })?;
    watch
        .poller
        .set(running)
        .map_err(|_| "the QA watch is already running".to_string())
}

mod clock;

#[cfg(test)]
mod tests;
