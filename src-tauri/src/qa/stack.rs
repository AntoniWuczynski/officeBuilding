//! Stacked pull requests: finding the stacks among the open ones, and saying in a review and
//! in a reviewer's brief that it covers a whole stack.
//!
//! A pull request is stacked on another when its base branch is that one's head branch. A
//! stack is a chain of them, bottom first, ending at a pull request nothing is stacked on (its
//! top). Two pull requests stacked on one parent are two stacks that share their lower part.

use super::brief::Job;
use super::github::PullRequest;
use super::record::Covered;
use super::review::{HEADER, ReviewPayload};

/// The stacks among `prs` as indices into it, bottom first, one per top, in the order of the
/// tops. A pull request nothing is stacked on or under is a stack of one.
///
/// A pull request has a parent when exactly one other pull request of the repository has its
/// base branch as head branch (a fork's branch names are not the repository's, and two with
/// one head branch leave it unclear). Pull requests that would be each other's parents, however
/// far round, have none, so every pull request ends up in a stack and no walk goes on forever.
#[must_use]
pub fn group(prs: &[&PullRequest]) -> Vec<Vec<usize>> {
    let mut parent: Vec<Option<usize>> = prs
        .iter()
        .enumerate()
        .map(|(i, pr)| {
            let mut parents = prs.iter().enumerate().filter(|(j, other)| {
                *j != i
                    && !other.from_fork
                    && !pr.base_ref.is_empty()
                    && other.head_ref == pr.base_ref
            });
            match (parents.next(), parents.next()) {
                (Some((j, _)), None) => Some(j),
                _ => None,
            }
        })
        .collect();
    let looped: Vec<bool> = (0..prs.len())
        .map(|start| {
            let mut at = parent[start];
            for _ in 0..prs.len() {
                match at {
                    Some(i) if i == start => return true,
                    Some(i) => at = parent[i],
                    None => return false,
                }
            }
            false
        })
        .collect();
    for (i, looped) in looped.iter().enumerate() {
        if *looped {
            parent[i] = None;
        }
    }
    (0..prs.len())
        .filter(|i| !parent.contains(&Some(*i)))
        .map(|top| {
            let mut chain = vec![top];
            while let Some(below) = parent[*chain.last().unwrap_or(&top)] {
                chain.push(below);
            }
            chain.reverse();
            chain
        })
        .collect()
}

/// What a review of `members` (bottom first) covers. Empty for one pull request: that is no
/// stack.
#[must_use]
pub fn covers(members: &[&PullRequest]) -> Vec<Covered> {
    if members.len() < 2 {
        return Vec::new();
    }
    members
        .iter()
        .map(|pr| Covered {
            number: pr.number,
            head: pr.head.clone(),
        })
        .collect()
}

/// "#11, #12 and #13".
fn numbers(covers: &[Covered]) -> String {
    let each: Vec<String> = covers.iter().map(|c| format!("#{}", c.number)).collect();
    match each.split_last() {
        Some((last, first)) if !first.is_empty() => format!("{} and {last}", first.join(", ")),
        _ => each.join(""),
    }
}

/// `payload` with its body saying the review covers the whole stack `covers`, right under the
/// header (a retry finds the review it already posted by the header). Unchanged for no stack.
#[must_use]
pub fn announce(mut payload: ReviewPayload, covers: &[Covered]) -> ReviewPayload {
    if let (false, Some(rest)) = (covers.is_empty(), payload.body.strip_prefix(HEADER)) {
        payload.body = format!(
            "{HEADER}\n\nThis review covers the whole stack of {} pull requests, {}, not one \
             pull request. It is posted on the top of the stack.{rest}",
            covers.len(),
            numbers(covers)
        );
    }
    payload
}

/// `job` with its first prompt saying the diff is the whole stack's. Unchanged for no stack.
#[must_use]
pub fn brief(mut job: Job, covers: &[Covered]) -> Job {
    if !covers.is_empty() {
        job.task = format!(
            "{} This pull request is the top of a stack of {} pull requests ({}). The diff is \
             the change of the whole stack, so review all of it.",
            job.task,
            covers.len(),
            numbers(covers)
        );
    }
    job
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qa::review::{Event, ReviewPayload};

    fn pr(number: u64, base: &str, head: &str) -> PullRequest {
        PullRequest {
            number,
            title: format!("Pull request {number}"),
            head: format!("{number:0>40}"),
            draft: false,
            labels: Vec::new(),
            base_ref: base.into(),
            head_ref: head.into(),
            from_fork: false,
        }
    }

    fn stacks(prs: &[PullRequest]) -> Vec<Vec<u64>> {
        let refs: Vec<&PullRequest> = prs.iter().collect();
        group(&refs)
            .into_iter()
            .map(|s| s.into_iter().map(|i| prs[i].number).collect())
            .collect()
    }

    #[test]
    fn a_chain_is_one_stack_bottom_first_however_it_is_listed() {
        let prs = [pr(3, "b", "c"), pr(1, "main", "a"), pr(2, "a", "b")];
        assert_eq!(stacks(&prs), vec![vec![1, 2, 3]]);
    }

    #[test]
    fn pull_requests_on_nothing_stacked_are_stacks_of_one() {
        let prs = [pr(1, "main", "a"), pr(2, "main", "b"), pr(3, "gone", "c")];
        assert_eq!(stacks(&prs), vec![vec![1], vec![2], vec![3]]);
        assert!(stacks(&[]).is_empty());
    }

    #[test]
    fn a_fork_in_the_stack_makes_each_branch_its_own_stack() {
        let prs = [
            pr(1, "main", "a"),
            pr(2, "a", "b"),
            pr(3, "a", "c"),
            pr(4, "c", "d"),
        ];
        assert_eq!(stacks(&prs), vec![vec![1, 2], vec![1, 3, 4]]);
    }

    #[test]
    fn a_stack_whose_bottom_is_on_a_branch_that_is_not_the_users_is_still_a_stack() {
        let prs = [pr(5, "someone-elses", "a"), pr(6, "a", "b")];
        assert_eq!(stacks(&prs), vec![vec![5, 6]]);
    }

    #[test]
    fn a_cycle_ends_and_every_pull_request_is_in_a_stack() {
        let two = [pr(1, "b", "a"), pr(2, "a", "b")];
        assert_eq!(stacks(&two), vec![vec![1], vec![2]]);
        let tail = [
            pr(1, "c", "a"),
            pr(2, "a", "b"),
            pr(3, "b", "c"),
            pr(4, "a", "d"),
        ];
        assert_eq!(stacks(&tail), vec![vec![2], vec![3], vec![1, 4]]);
    }

    #[test]
    fn a_pull_request_based_on_its_own_branch_is_alone() {
        assert_eq!(stacks(&[pr(1, "a", "a")]), vec![vec![1]]);
    }

    #[test]
    fn a_fork_pull_request_is_not_the_parent_and_two_of_one_branch_leave_it_unclear() {
        let mut forked = pr(1, "main", "main");
        forked.from_fork = true;
        assert_eq!(
            stacks(&[forked, pr(2, "main", "a")]),
            vec![vec![1], vec![2]]
        );
        let twice = [pr(1, "main", "a"), pr(2, "develop", "a"), pr(3, "a", "b")];
        assert_eq!(stacks(&twice), vec![vec![1], vec![2], vec![3]]);
    }

    #[test]
    fn pull_requests_without_branch_names_never_stack() {
        let prs = [pr(1, "", ""), pr(2, "", "")];
        assert_eq!(stacks(&prs), vec![vec![1], vec![2]]);
    }

    #[test]
    fn covers_lists_the_members_and_is_empty_for_one() {
        let (a, b) = (pr(1, "main", "a"), pr(2, "a", "b"));
        assert!(covers(&[&b]).is_empty());
        assert_eq!(
            covers(&[&a, &b]),
            vec![
                Covered {
                    number: 1,
                    head: a.head.clone()
                },
                Covered {
                    number: 2,
                    head: b.head.clone()
                }
            ]
        );
    }

    fn covered(numbers: &[u64]) -> Vec<Covered> {
        numbers
            .iter()
            .map(|n| Covered {
                number: *n,
                head: format!("{n:0>40}"),
            })
            .collect()
    }

    fn payload(body: &str) -> ReviewPayload {
        ReviewPayload {
            commit_id: "a".repeat(40),
            event: Event::Comment,
            body: body.into(),
            comments: Vec::new(),
        }
    }

    #[test]
    fn the_review_says_it_covers_the_whole_stack_under_the_header() {
        let body = format!("{HEADER}\n\nVerdict: problems found.\n\nReviewed commit aaaaaaa.");
        let got = announce(payload(&body), &covered(&[11, 12, 13]));
        assert_eq!(
            got.body,
            format!(
                "{HEADER}\n\nThis review covers the whole stack of 3 pull requests, #11, #12 and \
                 #13, not one pull request. It is posted on the top of the stack.\n\n\
                 Verdict: problems found.\n\nReviewed commit aaaaaaa."
            )
        );
        let two = announce(payload(&body), &covered(&[1, 2]));
        assert!(
            two.body
                .contains("stack of 2 pull requests, #1 and #2, not")
        );
    }

    #[test]
    fn a_review_of_one_pull_request_is_left_as_it_was() {
        let body = format!("{HEADER}\n\nNo issues found.");
        assert_eq!(announce(payload(&body), &[]), payload(&body));
        let odd = payload("Not ours");
        assert_eq!(announce(odd.clone(), &covered(&[1, 2])), odd);
    }

    #[test]
    fn the_reviewer_is_told_the_diff_is_the_whole_stack() {
        let job = Job {
            folder: "/w".into(),
            report: "/r.json".into(),
            title: "QA review of #13".into(),
            task: "Review pull request #13.".into(),
            instructions: "x".into(),
        };
        assert_eq!(brief(job.clone(), &[]), job);
        let got = brief(job, &covered(&[11, 12, 13]));
        assert_eq!(
            got.task,
            "Review pull request #13. This pull request is the top of a stack of 3 pull \
             requests (#11, #12 and #13). The diff is the change of the whole stack, so review \
             all of it."
        );
    }
}
