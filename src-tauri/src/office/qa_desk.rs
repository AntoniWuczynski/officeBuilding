//! The QA desk's side of the office: a floor's switch, the snapshot's `qa` list and the
//! link to a stored review.

use super::{Inner, Office};
use crate::model::{QaFloor, QaStatus};
use crate::qa::brief::Job;
use crate::qa::record::{MAX_REVIEWS, QaRecord, Review, ReviewOutcome};
use crate::qa::watch::{Building, Floor, Watch};
use crate::settings::SettingsStore;
use std::sync::Arc;

/// How many reviews a floor's entry in the snapshot lists.
const LISTED_REVIEWS: usize = 5;

/// The pull request's page on GitHub, plus a `#pullrequestreview-<id>` anchor for a posted review.
#[must_use]
pub fn review_url(review: &Review) -> String {
    let page = format!("https://github.com/{}/pull/{}", review.repo, review.number);
    match review.outcome {
        ReviewOutcome::Posted { review_id, .. } => format!("{page}#pullrequestreview-{review_id}"),
        ReviewOutcome::Clean | ReviewOutcome::Failed { .. } => page,
    }
}

impl Office {
    /// Use `settings` for this office's defaults.
    #[must_use]
    pub fn with_settings(mut self, settings: Arc<SettingsStore>) -> Self {
        self.settings = Some(settings);
        self
    }

    /// Whether a floor with no say of its own has QA on.
    fn qa_default_on(&self) -> bool {
        self.settings
            .as_ref()
            .is_some_and(|settings| settings.get().qa.default_on)
    }

    /// Switch a floor's QA desk. Switching it on pins the floor.
    pub fn set_floor_qa(&self, project_id: &str, enabled: bool) -> Result<(), String> {
        if project_id == self.setup_project_id {
            return Err("the plant room has no QA desk".to_string());
        }
        let mut inner = self.lock();
        let root = Self::root_key(&inner, project_id)?;
        let records = &mut inner.store.records;
        records.qa.entry(root.clone()).or_default().enabled = Some(enabled);
        if enabled && !records.pinned_floors.contains(&root) {
            records.pinned_floors.push(root);
        }
        inner.store.save()
    }

    /// The page of a stored review: the review itself when one was posted, else its pull request.
    pub fn review_link(&self, project_id: &str, number: u64, head: &str) -> Result<String, String> {
        let inner = self.lock();
        let root = Self::root_key(&inner, project_id)
            .map_err(|_| "that review is no longer on this floor's record".to_string())?;
        inner
            .store
            .records
            .qa
            .get(&root)
            .and_then(|record| {
                record
                    .reviews
                    .iter()
                    .rev()
                    .find(|r| r.number == number && r.head == head)
            })
            .map(review_url)
            .ok_or_else(|| "that review is no longer on this floor's record".to_string())
    }

    /// Use `watch` for this office's QA desks.
    #[must_use]
    pub fn with_qa(mut self, watch: Arc<Watch>) -> Self {
        self.qa = Some(watch);
        self
    }

    /// Each standing floor except the setup floor: its project id, root, whether QA is on
    /// there (`default_on` when it has no say), and its record.
    fn qa_records<'a>(
        &'a self,
        inner: &'a Inner,
        default_on: bool,
    ) -> impl Iterator<Item = (&'a String, String, bool, QaRecord)> + 'a {
        inner
            .found
            .roots
            .iter()
            .filter(|(project_id, _)| **project_id != self.setup_project_id)
            .map(move |(project_id, root)| {
                let root = root.to_string_lossy().into_owned();
                let record = inner
                    .store
                    .records
                    .qa
                    .get(&root)
                    .cloned()
                    .unwrap_or_default();
                let enabled = record.enabled.unwrap_or(default_on);
                (project_id, root, enabled, record)
            })
    }

    /// The snapshot's `qa` list: one entry per standing floor except the setup floor.
    pub(super) fn qa_floors(&self, inner: &Inner) -> Vec<QaFloor> {
        self.qa_records(inner, self.qa_default_on())
            .map(|(project_id, root, enabled, record)| {
                let (status, run, stamp) = match (enabled, &self.qa) {
                    (false, _) => (QaStatus::Off, None, None),
                    (true, None) => (QaStatus::Waiting, None, None),
                    (true, Some(watch)) => {
                        let view = watch.view(&root);
                        let run = view.run.map(|(mut run, terminal)| {
                            run.session_id = inner.owned.get(&terminal).cloned();
                            run
                        });
                        (view.status, run, view.stamp)
                    }
                };
                QaFloor {
                    project_id: project_id.clone(),
                    enabled,
                    status,
                    run,
                    reviews: record
                        .reviews
                        .into_iter()
                        .rev()
                        .take(LISTED_REVIEWS)
                        .collect(),
                    stamp,
                }
            })
            .collect()
    }
}

impl Building for Office {
    fn qa_on(&self) -> Vec<Floor> {
        let default_on = self.qa_default_on();
        let inner = self.lock();
        self.qa_records(&inner, default_on)
            .filter(|(_, _, enabled, _)| *enabled)
            .map(|(project_id, root, _, record)| Floor {
                project_id: project_id.clone(),
                root,
                reviews: record.reviews,
            })
            .collect()
    }

    fn record_review(&self, root: &str, review: Review) -> Result<(), String> {
        let mut inner = self.lock();
        let reviews = &mut inner
            .store
            .records
            .qa
            .entry(root.to_string())
            .or_default()
            .reviews;
        reviews.push(review);
        let excess = reviews.len().saturating_sub(MAX_REVIEWS);
        reviews.drain(..excess);
        inner.store.save()
    }

    fn hire_reviewer(&self, project_id: &str, job: &Job) -> Result<String, String> {
        Office::hire_reviewer(self, project_id, job)
    }

    fn reviewer_left(&self, terminal: &str) -> bool {
        Office::reviewer_left(self, terminal)
    }

    fn dismiss_reviewer(&self, terminal: &str) -> Result<(), String> {
        Office::dismiss_reviewer(self, terminal)
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::floor_office;
    use super::*;
    use crate::process::{Captured, FakeRunner};
    use crate::qa::record::Verdict;
    use crate::qa::record::fixtures::review;
    use crate::store::Store;
    use std::fs;
    use std::path::Path;

    fn root_of(dir: &Path) -> String {
        dir.join("home/code/app").to_string_lossy().into_owned()
    }

    fn with_default(office: Office, dir: &Path, default_on: bool) -> Office {
        let path = dir.join("settings.json");
        let text = format!(r#"{{"qa":{{"defaultOn":{default_on}}}}}"#);
        fs::write(&path, text).expect("settings");
        office.with_settings(Arc::new(SettingsStore::open(path)))
    }

    fn store_reviews(office: &Office, dir: &Path, reviews: Vec<Review>) {
        office.lock().store.records.qa.insert(
            root_of(dir),
            QaRecord {
                enabled: None,
                reviews,
            },
        );
    }

    #[test]
    fn switching_on_stores_true_and_pins_the_floor_and_off_keeps_the_pin() {
        let (office, dir, floor) = floor_office("qa-switch");
        office.set_floor_qa(&floor.id, true).expect("on");
        let root = root_of(&dir);
        let again = Store::open(dir.join("store.json")).expect("reopen");
        assert_eq!(again.records.qa[&root].enabled, Some(true));
        assert!(again.records.pinned_floors.contains(&root));
        office.set_floor_qa(&floor.id, false).expect("off");
        let again = Store::open(dir.join("store.json")).expect("reopen");
        assert_eq!(again.records.qa[&root].enabled, Some(false));
        assert!(again.records.pinned_floors.contains(&root));
        fs::remove_dir_all(dir).expect("cleanup");
    }

    #[test]
    fn an_unknown_floor_and_the_setup_floor_are_refused() {
        let (office, dir, _floor) = floor_office("qa-refuse");
        assert_eq!(
            office.set_floor_qa("p-nope", true),
            Err("p-nope is not a floor of this building".to_string())
        );
        let setup = office.setup_project_id.clone();
        assert_eq!(
            office.set_floor_qa(&setup, true),
            Err("the plant room has no QA desk".to_string())
        );
        fs::remove_dir_all(dir).expect("cleanup");
    }

    #[test]
    fn the_snapshot_lists_each_standing_floor_but_not_the_setup_floor() {
        let (office, dir, floor) = floor_office("qa-list");
        office.ensure_setup_floor().expect("setup floor");
        office.refresh();
        let snapshot = office.snapshot();
        let ids: Vec<_> = snapshot.qa.iter().map(|q| q.project_id.as_str()).collect();
        assert_eq!(ids, vec![floor.id.as_str()]);
        assert!(
            snapshot
                .projects
                .iter()
                .any(|p| p.id == snapshot.setup_project_id)
        );
        assert_eq!(snapshot.qa[0].status, QaStatus::Off);
        assert!(!snapshot.qa[0].enabled);
        fs::remove_dir_all(dir).expect("cleanup");
    }

    #[test]
    fn a_floor_follows_the_default_until_it_has_a_say() {
        let (office, dir, floor) = floor_office("qa-default");
        let office = with_default(office, &dir, true);
        office.refresh();
        let entry = || office.snapshot().qa.remove(0);
        assert_eq!((entry().enabled, entry().status), (true, QaStatus::Waiting));
        office.set_floor_qa(&floor.id, false).expect("off");
        assert_eq!((entry().enabled, entry().status), (false, QaStatus::Off));
        fs::remove_dir_all(dir).expect("cleanup");
    }

    #[test]
    fn reviews_are_newest_first_and_at_most_five() {
        let (office, dir, _floor) = floor_office("qa-reviews");
        let all: Vec<_> = (1..=7)
            .map(|n| review(n, &format!("{n:040}"), ReviewOutcome::Clean))
            .collect();
        store_reviews(&office, &dir, all);
        let numbers: Vec<_> = office.snapshot().qa[0]
            .reviews
            .iter()
            .map(|r| r.number)
            .collect();
        assert_eq!(numbers, vec![7, 6, 5, 4, 3]);
        fs::remove_dir_all(dir).expect("cleanup");
    }

    #[test]
    fn a_link_goes_to_the_review_or_else_the_pull_request() {
        let (office, dir, floor) = floor_office("qa-link");
        let posted = ReviewOutcome::Posted {
            verdict: Verdict::Concerns,
            findings: 2,
            review_id: 90210,
        };
        let failed = ReviewOutcome::Failed {
            reason: "the reviewer left".into(),
        };
        store_reviews(
            &office,
            &dir,
            vec![
                review(12, &"a".repeat(40), posted),
                review(13, &"b".repeat(40), ReviewOutcome::Clean),
                review(14, &"c".repeat(40), failed),
            ],
        );
        let pull = "https://github.com/example-org/widgets/pull";
        let link = |n, head: &str| office.review_link(&floor.id, n, head);
        assert_eq!(
            link(12, &"a".repeat(40)),
            Ok(format!("{pull}/12#pullrequestreview-90210"))
        );
        assert_eq!(link(13, &"b".repeat(40)), Ok(format!("{pull}/13")));
        assert_eq!(link(14, &"c".repeat(40)), Ok(format!("{pull}/14")));
        let gone = Err("that review is no longer on this floor's record".to_string());
        assert_eq!(link(12, &"b".repeat(40)), gone);
        assert_eq!(office.review_link("p-nope", 12, &"a".repeat(40)), gone);
        fs::remove_dir_all(dir).expect("cleanup");
    }

    #[test]
    fn qa_on_lists_floors_that_are_on_but_never_the_setup_floor() {
        let (office, dir, floor) = floor_office("qa-on");
        office.ensure_setup_floor().expect("setup floor");
        office.refresh();
        assert_eq!(office.qa_on(), Vec::new());
        let office = with_default(office, &dir, true);
        store_reviews(
            &office,
            &dir,
            vec![review(12, &"a".repeat(40), ReviewOutcome::Clean)],
        );
        let on = office.qa_on();
        assert_eq!(on.len(), 1);
        assert_eq!(
            (on[0].project_id.as_str(), on[0].root.clone()),
            (floor.id.as_str(), root_of(&dir))
        );
        assert_eq!(on[0].reviews.len(), 1);
        office.set_floor_qa(&floor.id, false).expect("off");
        assert_eq!(office.qa_on(), Vec::new());
        fs::remove_dir_all(dir).expect("cleanup");
    }

    #[test]
    fn record_review_keeps_the_newest_two_hundred_across_a_reopen() {
        let (office, dir, _floor) = floor_office("qa-record");
        let root = root_of(&dir);
        for n in 1..=201 {
            office
                .record_review(&root, review(n, &format!("{n:040}"), ReviewOutcome::Clean))
                .expect("record");
        }
        let again = Store::open(dir.join("store.json")).expect("reopen");
        let numbers: Vec<u64> = again.records.qa[&root]
            .reviews
            .iter()
            .map(|r| r.number)
            .collect();
        assert_eq!(numbers.len(), MAX_REVIEWS);
        assert_eq!((numbers[0], numbers[MAX_REVIEWS - 1]), (2, 201));
        assert_eq!(again.records.qa[&root].enabled, None);
        fs::remove_dir_all(dir).expect("cleanup");
    }

    #[test]
    fn a_floor_that_is_on_shows_the_watch_view() {
        let (office, dir, floor) = floor_office("qa-view");
        let answer = |stdout: &str| Captured {
            code: Some(0),
            stdout: stdout.into(),
            stderr: String::new(),
        };
        let fake = Arc::new(FakeRunner::new(move |call| {
            let first = call.args.first().map(|a| a.to_string_lossy().into_owned());
            Ok(match first.as_deref() {
                Some("-l") => answer("/opt/homebrew/bin/gh\n/usr/bin/git\n"),
                Some("pr") => answer("[]"),
                _ if call.args.iter().any(|a| a == "remote") => {
                    answer("https://github.com/example-org/widgets.git\n")
                }
                _ => answer(""),
            })
        }));
        let settings = Arc::new(SettingsStore::open(dir.join("settings.json")));
        let watch = Arc::new(Watch::new(
            fake,
            settings,
            dir.join("data"),
            Box::new(|| {}),
        ));
        let office = office.with_qa(Arc::clone(&watch));
        office.refresh();
        let entry = || office.snapshot().qa.remove(0);
        assert_eq!(entry().status, QaStatus::Off);
        office.set_floor_qa(&floor.id, true).expect("on");
        assert_eq!(entry().status, QaStatus::Waiting);
        let now = chrono::DateTime::parse_from_rfc3339("2026-11-12T10:15:00Z")
            .expect("time")
            .with_timezone(&chrono::Utc);
        watch.pass(&office, now);
        let shown = entry();
        assert_eq!(
            shown.status,
            QaStatus::Watching {
                repo: "example-org/widgets".into(),
                checked_at: "2026-11-12T10:15:00Z".into(),
                open: 0,
            }
        );
        assert_eq!((shown.run, shown.stamp), (None, None));
        office.set_floor_qa(&floor.id, false).expect("off");
        assert_eq!(entry().status, QaStatus::Off);
        fs::remove_dir_all(dir).expect("cleanup");
    }
}
