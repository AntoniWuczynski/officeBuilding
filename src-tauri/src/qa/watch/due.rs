//! Which listed pull request, or stack of them, a floor reviews next.

use super::clock::{delta, within_day};
use super::{Desk, NO_QA_LABEL, Open, Pull, QUIET};
use crate::qa::github::PullRequest;
use crate::qa::record::Review;
use crate::qa::stack;
use crate::settings::QaSettings;
use chrono::{DateTime, Utc};

/// Whether `pr` is one A Human has put outside QA: a draft (when drafts are skipped) or one
/// labelled `no-qa`.
pub(super) fn left_out(qa: &QaSettings, pr: &PullRequest) -> bool {
    (pr.draft && qa.skip_drafts)
        || pr
            .labels
            .iter()
            .any(|l| l.eq_ignore_ascii_case(NO_QA_LABEL))
}

impl Desk {
    /// What is reviewed together: each listed pull request alone, or with stacks reviewed as
    /// one, each stack's pull requests bottom first (its top last). Pull requests left out of
    /// QA are in none, so a stack is cut at them: the pull requests above one are a stack that
    /// starts on its branch.
    fn units(&self, qa: &QaSettings) -> Vec<Vec<&Open>> {
        let kept: Vec<&Open> = self.open.iter().filter(|o| !left_out(qa, &o.pr)).collect();
        if !qa.review_stacks {
            return kept.into_iter().map(|o| vec![o]).collect();
        }
        let prs: Vec<&PullRequest> = kept.iter().map(|o| &o.pr).collect();
        stack::group(&prs)
            .into_iter()
            .flat_map(|chain| {
                let unit: Vec<&Open> = chain.into_iter().map(|i| kept[i]).collect();
                // A chain whose top lacks a lower pull request's commits is reviewed one by one.
                let diverged = unit.split_last().is_some_and(|(top, lower)| {
                    lower
                        .iter()
                        .any(|o| self.diverged.contains(&(o.pr.number, top.pr.number)))
                });
                if diverged {
                    unit.into_iter().map(|o| vec![o]).collect()
                } else {
                    vec![unit]
                }
            })
            .collect()
    }

    /// The listed pull request with the lowest number that is due, as the top of its stack when
    /// stacks are reviewed as one. A stack is due like a pull request is, counted by its top:
    /// no review of this top at this head over these pull requests at these heads, the first
    /// review of the top unless every push is reviewed, fewer than `reviews_per_day` of the top
    /// in a day, and every pull request in it has stood still for `QUIET`. So a push to any
    /// pull request in the stack brings it due again.
    /// `taken` says whether another floor on the same repository has the head in hand.
    pub(super) fn due(
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
        self.units(qa)
            .into_iter()
            .map(|unit| {
                let prs: Vec<&PullRequest> = unit.iter().map(|o| &o.pr).collect();
                (stack::covers(&prs), unit)
            })
            .filter(|(covers, unit)| {
                let Some(top) = unit.last() else {
                    return false;
                };
                let pr = &top.pr;
                !of(pr.number)
                    .any(|r| r.head == pr.head && (!qa.review_stacks || r.covers == *covers))
                    && (qa.every_push || of(pr.number).next().is_none())
                    && of(pr.number).filter(|r| within_day(&r.at, now)).count()
                        < usize::try_from(qa.reviews_per_day).unwrap_or(usize::MAX)
                    && unit.iter().all(|o| o.since + delta(QUIET) <= checked)
                    && !taken(pr.number, &pr.head)
                    && !self
                        .queue
                        .iter()
                        .any(|q| q.pull.repo == *repo && q.pull.number == pr.number)
            })
            .filter_map(|(covers, unit)| {
                let (top, bottom) = (unit.last()?, unit.first()?);
                let failed = self
                    .start_failures
                    .get(&(top.pr.number, top.pr.head.clone()))
                    .copied();
                let pull = Pull {
                    repo: repo.clone(),
                    number: top.pr.number,
                    title: top.pr.title.clone(),
                    head: top.pr.head.clone(),
                    covers,
                    base: bottom.pr.base_ref.clone(),
                };
                Some(((failed.unwrap_or(0), top.pr.number), pull))
            })
            .min_by_key(|(key, _)| *key)
            .map(|(_, pull)| pull)
    }
}
