//! Reviewing a stack of pull requests as one.

use super::*;
use crate::qa::record::Covered;
use crate::qa::review::HEADER;
use crate::qa::stack::COVERS_LINE;

/// A pull request merging into `base`, made on `branch`.
fn linked(number: u64, head: char, base: &str, branch: &str) -> Pr {
    Pr {
        base: base.into(),
        branch: branch.into(),
        ..pr(number, head)
    }
}

/// #1 on main, #2 on #1, #3 on #2, at heads a, b and c.
fn chain() -> [Pr; 3] {
    [
        linked(1, 'a', "main", "one"),
        linked(2, 'b', "one", "two"),
        linked(3, 'c', "two", "three"),
    ]
}

fn stacks_on() -> Rig {
    let rig = Rig::new();
    rig.qa(|qa| qa.review_stacks = true);
    rig
}

fn covered(parts: &[(u64, char)]) -> Vec<Covered> {
    parts
        .iter()
        .map(|(number, head)| Covered {
            number: *number,
            head: sha(*head),
        })
        .collect()
}

/// The `gh` calls of the named kind, as arguments.
fn gh_calls(rig: &Rig, kind: &str) -> Vec<Vec<String>> {
    rig.fake
        .calls()
        .iter()
        .filter(|c| name(c) == kind)
        .map(strings)
        .collect()
}

fn recorded(rig: &Rig) -> Vec<Review> {
    lock(&rig.building.recorded)
        .iter()
        .map(|(_, r)| r.clone())
        .collect()
}

/// The reviewer of the running review reports "pass", with nothing found.
fn finish(rig: &Rig, at: DateTime<Utc>) {
    rig.report("p-a", &reply("pass", &[]));
    rig.pass(at);
}

#[test]
fn with_the_setting_off_each_pull_request_of_a_chain_is_reviewed_alone() {
    let rig = Rig::new();
    rig.list(WIDGETS, &chain());
    rig.pass(mins(0));
    rig.pass(mins(5));
    assert_eq!(rig.hires(), 1);
    assert_eq!(lock(&rig.building.hires)[0].1.title, "QA review of #1");
    assert_eq!(rig.count("pr diff"), 1);
    assert_eq!(rig.count("api compare"), 0);
    let listed = gh_calls(&rig, "pr list");
    assert_eq!(
        listed[0].last().map(String::as_str),
        Some("number,title,headRefOid,isDraft,labels")
    );
    finish(&rig, mins(6));
    assert_eq!(recorded(&rig)[0].number, 1);
    assert!(recorded(&rig)[0].covers.is_empty());
}

#[test]
fn a_chain_is_reviewed_once_on_its_top_with_the_whole_stacks_diff() {
    let rig = stacks_on();
    rig.list(WIDGETS, &chain());
    rig.pass(mins(0));
    rig.pass(mins(5));
    assert_eq!(rig.hires(), 1);
    let listed = gh_calls(&rig, "pr list");
    assert_eq!(
        listed[0].last().map(String::as_str),
        Some("number,title,headRefOid,isDraft,labels,baseRefName,headRefName,isCrossRepository")
    );
    assert_eq!(
        rig.count("pr diff"),
        1,
        "the top's own diff places the comments"
    );
    assert_eq!(
        gh_calls(&rig, "api compare"),
        vec![vec![
            "api".to_string(),
            format!("repos/{WIDGETS}/compare/main...{}", sha('c')),
            "-H".to_string(),
            "Accept: application/vnd.github.diff".to_string(),
        ]]
    );
    let fetched: Vec<String> = gh_calls(&rig, "fetch")
        .iter()
        .map(|args| args[5].clone())
        .collect();
    assert_eq!(fetched, ["pull/3/head"]);
    let hires = lock(&rig.building.hires).clone();
    assert_eq!(hires[0].1.title, "QA review of #3");
    assert!(
        hires[0]
            .1
            .task
            .contains("top of a stack of 3 pull requests (#1, #2 and #3)")
    );

    rig.report("p-a", &reply("concerns", &[("src/upload.rs", 12)]));
    rig.pass(mins(6));
    let posts = gh_calls(&rig, "api");
    assert_eq!(posts.len(), 1);
    assert_eq!(posts[0][1], format!("repos/{WIDGETS}/pulls/3/reviews"));
    let body = rig.payload(0)["body"].as_str().expect("body").to_string();
    assert!(
        body.starts_with(
            "Office Building Quality Assurance\n\nThis review covers the whole stack of 3 pull \
             requests, #1, #2 and #3, not one pull request."
        ),
        "{body}"
    );
    assert_eq!(rig.payload(0)["commit_id"], sha('c'));
    let reviews = recorded(&rig);
    assert_eq!(reviews.len(), 1);
    assert_eq!(reviews[0].number, 3);
    assert_eq!(reviews[0].head, sha('c'));
    assert_eq!(reviews[0].covers, covered(&[(1, 'a'), (2, 'b'), (3, 'c')]));
    // Nothing is left to review: the lower pull requests are covered.
    for at in [10, 15, 20, 25] {
        rig.pass(mins(at));
    }
    assert_eq!(rig.hires(), 1);
}

#[test]
fn a_push_to_any_pull_request_of_the_stack_brings_it_due_again_after_it_stood_still() {
    let rig = stacks_on();
    rig.list(WIDGETS, &chain());
    rig.pass(mins(0));
    rig.pass(mins(5));
    finish(&rig, mins(6));
    assert_eq!(rig.hires(), 1);
    // The bottom moves on, the top does not.
    rig.list(
        WIDGETS,
        &[
            linked(1, 'd', "main", "one"),
            linked(2, 'b', "one", "two"),
            linked(3, 'c', "two", "three"),
        ],
    );
    rig.pass(mins(10));
    assert_eq!(rig.hires(), 1, "the push has not stood still yet");
    rig.pass(mins(15));
    assert_eq!(rig.hires(), 2);
    assert_eq!(gh_calls(&rig, "api compare").len(), 2);
    finish(&rig, mins(16));
    assert_eq!(
        recorded(&rig)[1].covers,
        covered(&[(1, 'd'), (2, 'b'), (3, 'c')])
    );
}

#[test]
fn with_every_push_off_only_the_first_review_of_a_top_is_made() {
    let rig = stacks_on();
    rig.qa(|qa| qa.every_push = false);
    rig.list(WIDGETS, &chain());
    rig.pass(mins(0));
    rig.pass(mins(5));
    finish(&rig, mins(6));
    rig.list(
        WIDGETS,
        &[
            linked(1, 'd', "main", "one"),
            linked(2, 'b', "one", "two"),
            linked(3, 'c', "two", "three"),
        ],
    );
    for at in [10, 15, 20] {
        rig.pass(mins(at));
    }
    assert_eq!(rig.hires(), 1);
}

#[test]
fn a_fork_makes_each_branch_its_own_stack_reviewed_on_its_own_top() {
    let rig = stacks_on();
    rig.list(
        WIDGETS,
        &[
            linked(1, 'a', "main", "one"),
            linked(2, 'b', "one", "two"),
            linked(3, 'c', "one", "three"),
        ],
    );
    rig.pass(mins(0));
    rig.pass(mins(5));
    rig.report("p-a", &reply("pass", &[]));
    rig.pass(mins(6));
    assert_eq!(rig.hires(), 2, "the second branch follows at once");
    rig.report("p-a", &reply("pass", &[]));
    rig.pass(mins(7));
    let reviews = recorded(&rig);
    assert_eq!(reviews.iter().map(|r| r.number).collect::<Vec<_>>(), [2, 3]);
    assert_eq!(reviews[0].covers, covered(&[(1, 'a'), (2, 'b')]));
    assert_eq!(reviews[1].covers, covered(&[(1, 'a'), (3, 'c')]));
    let compares = gh_calls(&rig, "api compare");
    assert_eq!(
        compares[0][1],
        format!("repos/{WIDGETS}/compare/main...{}", sha('b'))
    );
    assert_eq!(
        compares[1][1],
        format!("repos/{WIDGETS}/compare/main...{}", sha('c'))
    );
}

#[test]
fn a_cycle_or_a_missing_link_does_not_loop_and_each_is_reviewed_alone() {
    let rig = stacks_on();
    rig.list(
        WIDGETS,
        &[
            linked(1, 'a', "two", "one"),
            linked(2, 'b', "one", "two"),
            linked(3, 'c', "gone", "three"),
        ],
    );
    rig.pass(mins(0));
    rig.pass(mins(5));
    assert_eq!(rig.hires(), 1);
    assert_eq!(rig.count("api compare"), 0);
    assert_eq!(rig.count("pr diff"), 1);
    rig.report("p-a", &reply("pass", &[]));
    rig.pass(mins(6));
    rig.report("p-a", &reply("pass", &[]));
    rig.pass(mins(7));
    rig.report("p-a", &reply("pass", &[]));
    rig.pass(mins(8));
    let reviews = recorded(&rig);
    assert_eq!(
        reviews.iter().map(|r| r.number).collect::<Vec<_>>(),
        [1, 2, 3]
    );
    assert!(reviews.iter().all(|r| r.covers.is_empty()));
    assert_eq!(rig.count("api compare"), 0);
}

#[test]
fn a_no_qa_pull_request_cuts_the_stack_and_is_not_reviewed_itself() {
    let rig = stacks_on();
    let mut middle = linked(2, 'b', "one", "two");
    middle.labels = vec!["no-qa"];
    rig.list(
        WIDGETS,
        &[
            linked(1, 'a', "main", "one"),
            middle,
            linked(3, 'c', "two", "three"),
            linked(4, 'd', "three", "four"),
        ],
    );
    rig.pass(mins(0));
    rig.pass(mins(5));
    assert_eq!(lock(&rig.building.hires)[0].1.title, "QA review of #1");
    assert_eq!(rig.count("pr diff"), 1);
    rig.report("p-a", &reply("pass", &[]));
    rig.pass(mins(6));
    // #3 and #4 are a stack that starts on the branch of the pull request left out.
    assert_eq!(lock(&rig.building.hires)[1].1.title, "QA review of #4");
    assert_eq!(
        gh_calls(&rig, "api compare")[0][1],
        format!("repos/{WIDGETS}/compare/two...{}", sha('d'))
    );
    rig.report("p-a", &reply("pass", &[]));
    rig.pass(mins(7));
    let reviews = recorded(&rig);
    assert_eq!(reviews[1].covers, covered(&[(3, 'c'), (4, 'd')]));
    assert_eq!(rig.hires(), 2);
}

#[test]
fn a_draft_on_top_is_left_out_and_the_pull_request_below_is_reviewed_alone() {
    let rig = stacks_on();
    let mut top = linked(2, 'b', "one", "two");
    top.draft = true;
    rig.list(WIDGETS, &[linked(1, 'a', "main", "one"), top]);
    rig.pass(mins(0));
    rig.pass(mins(5));
    assert_eq!(lock(&rig.building.hires)[0].1.title, "QA review of #1");
    assert_eq!(rig.count("api compare"), 0);
    finish(&rig, mins(6));
    for at in [10, 15, 20] {
        rig.pass(mins(at));
    }
    assert_eq!(rig.hires(), 1);
}

#[test]
fn a_stack_on_a_branch_that_is_not_the_users_is_diffed_against_that_branch() {
    let rig = stacks_on();
    rig.list(
        WIDGETS,
        &[
            linked(5, 'a', "someone/else's", "five"),
            linked(6, 'b', "five", "six"),
        ],
    );
    rig.pass(mins(0));
    rig.pass(mins(5));
    assert_eq!(
        gh_calls(&rig, "api compare")[0][1],
        format!("repos/{WIDGETS}/compare/someone/else%27s...{}", sha('b'))
    );
}

#[test]
fn a_stack_that_cannot_start_is_given_up_with_what_it_covered_and_not_asked_again() {
    let rig = stacks_on();
    rig.list(WIDGETS, &chain());
    rig.script().diff = exit(1, "HTTP 406: diff too large");
    for at in [0, 5, 10, 15, 20, 25, 30, 35] {
        rig.pass(mins(at));
    }
    let reviews = recorded(&rig);
    assert_eq!(reviews.len(), 1, "{reviews:?}");
    assert_eq!(reviews[0].number, 3);
    assert_eq!(reviews[0].covers.len(), 3);
    assert_eq!(rig.count("api compare"), 3);
    assert_eq!(rig.hires(), 0);
}

#[test]
fn a_push_to_a_lower_pull_request_without_rebasing_the_top_reviews_them_one_by_one() {
    let rig = stacks_on();
    // #2 was pushed to ('b' is not in the history of the top's head), the top was not rebased.
    rig.script().statuses.insert(sha('b'), "diverged".into());
    rig.list(WIDGETS, &chain());
    rig.pass(mins(0));
    rig.pass(mins(5));
    assert_eq!(rig.count("api compare"), 0, "no stack diff");
    assert_eq!(lock(&rig.building.hires)[0].1.title, "QA review of #1");
    assert_eq!(rig.count("pr diff"), 1);
    for at in [6, 7, 8] {
        rig.report("p-a", &reply("pass", &[]));
        rig.pass(mins(at));
    }
    let reviews = recorded(&rig);
    assert_eq!(
        reviews.iter().map(|r| r.number).collect::<Vec<_>>(),
        [1, 2, 3]
    );
    assert!(reviews.iter().all(|r| r.covers.is_empty()));
}

#[test]
fn a_chain_whose_every_head_is_in_the_tops_history_is_asked_per_lower_pull_request_at_each_check() {
    let rig = stacks_on();
    rig.script().statuses.insert(sha('a'), "identical".into());
    rig.list(WIDGETS, &chain());
    rig.pass(mins(0));
    rig.pass(mins(5));
    let asked = gh_calls(&rig, "api ancestor");
    assert_eq!(
        asked
            .iter()
            .take(2)
            .map(|a| a[1].clone())
            .collect::<Vec<_>>(),
        [
            format!("repos/{WIDGETS}/compare/{}...{}", sha('a'), sha('c')),
            format!("repos/{WIDGETS}/compare/{}...{}", sha('b'), sha('c')),
        ]
    );
    assert_eq!(lock(&rig.building.hires)[0].1.title, "QA review of #3");
    assert_eq!(rig.count("api compare"), 1);
}

#[test]
fn a_review_of_a_stack_places_inline_comments_only_on_the_tops_own_diff() {
    let rig = stacks_on();
    // The stack's diff has src/upload.rs, the top's own diff only notes.txt line 2.
    rig.script().compare = Some(ok(DIFF));
    rig.script().diff = ok(
        "diff --git a/notes.txt b/notes.txt\n--- a/notes.txt\n+++ b/notes.txt\n@@ -1,2 +1,2 @@\n keep\n-old\n+new\n",
    );
    rig.list(WIDGETS, &chain());
    rig.pass(mins(0));
    rig.pass(mins(5));
    rig.report(
        "p-a",
        &reply("concerns", &[("src/upload.rs", 12), ("notes.txt", 2)]),
    );
    rig.pass(mins(6));
    let payload = rig.payload(0);
    let comments = payload["comments"].as_array().expect("comments");
    assert_eq!(comments.len(), 1, "{payload}");
    assert_eq!(comments[0]["path"], "notes.txt");
    let body = payload["body"].as_str().expect("body");
    assert!(body.contains("`src/upload.rs:12`"), "{body}");
}

#[test]
fn a_retry_finds_the_stack_review_and_not_a_single_review_on_the_same_head() {
    let rig = stacks_on();
    rig.list(WIDGETS, &chain());
    rig.pass(mins(0));
    rig.pass(mins(5));
    rig.script().posts.push_back(Captured::default());
    rig.report("p-a", &reply("concerns", &[("src/upload.rs", 12)]));
    rig.pass(mins(6));
    assert!(recorded(&rig).is_empty());
    // An earlier review of #3 alone is on the head, and no review of the stack yet.
    rig.script().reviews = ok(&format!(
        "31\t{}\t{HEADER}\\n\\nNo issues found.\n",
        sha('c')
    ));
    rig.pass(mins(10));
    assert_eq!(rig.count("api"), 2, "the stack review is posted");
    assert_eq!(recorded(&rig)[0].covers.len(), 3);
}

#[test]
fn a_retry_that_finds_the_stack_review_does_not_post_it_again() {
    let rig = stacks_on();
    rig.list(WIDGETS, &chain());
    rig.pass(mins(0));
    rig.pass(mins(5));
    rig.script().posts.push_back(Captured::default());
    rig.report("p-a", &reply("concerns", &[("src/upload.rs", 12)]));
    rig.pass(mins(6));
    rig.script().reviews = ok(&format!(
        "32\t{}\t{HEADER}\\n\\n{COVERS_LINE} 3 pull requests, #1, #2 and #3, not one pull request.\n",
        sha('c')
    ));
    rig.pass(mins(10));
    assert_eq!(rig.count("api"), 1, "no second post");
    assert!(matches!(
        rig.outcomes()[..],
        [ReviewOutcome::Posted { review_id: 32, .. }]
    ));
}

#[test]
fn with_the_setting_off_a_stack_review_at_the_tops_head_still_counts() {
    let rig = Rig::new();
    let done = Review {
        repo: WIDGETS.into(),
        number: 3,
        title: title(3),
        head: sha('c'),
        at: timestamp(mins(0)),
        outcome: ReviewOutcome::Clean,
        covers: covered(&[(1, 'a'), (2, 'b'), (3, 'c')]),
    };
    rig.add_floor("p-b", vec![done.clone()]);
    rig.building.floors.lock().expect("floors")[0]
        .reviews
        .push(done);
    rig.list(WIDGETS, &[linked(3, 'c', "two", "three")]);
    for at in [0, 5, 10, 15] {
        rig.pass(mins(at));
    }
    assert_eq!(rig.hires(), 0);
}
