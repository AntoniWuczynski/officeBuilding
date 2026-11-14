//! Reviews that must not be doubled, and pull requests that must not be starved.

use super::*;
use crate::qa::review::HEADER;

#[test]
fn a_repeated_post_first_finds_the_review_that_already_went_up() {
    let rig = Rig::started();
    // The first post timed out on this side, but GitHub had taken it.
    rig.script().posts.push_back(Captured::default());
    rig.report("p-a", &reply("concerns", &[("src/upload.rs", 12)]));
    rig.pass(secs(5 * 60 + 15));
    assert_eq!(rig.count("api"), 1);
    assert!(rig.outcomes().is_empty());
    rig.script().reviews = ok(&format!(
        "11\t{}\tSomeone else's\n777\t{}\t{HEADER}\\n\\nVerdict: problems found.\n",
        sha('a'),
        sha('a')
    ));
    rig.pass(mins(10));
    assert_eq!(rig.count("api list"), 1);
    assert_eq!(rig.count("api"), 1, "no second post");
    assert!(matches!(
        rig.outcomes()[..],
        [ReviewOutcome::Posted { review_id: 777, .. }]
    ));
}

#[test]
fn two_floors_on_one_repository_review_a_head_once() {
    let rig = Rig::new();
    rig.add_floor("p-b", Vec::new());
    rig.qa(|qa| qa.max_concurrent = 4);
    rig.list(WIDGETS, &[pr(12, 'a')]);
    rig.pass(mins(0));
    rig.pass(mins(5));
    assert_eq!(rig.hires(), 1);
    rig.pass(mins(10));
    assert_eq!(rig.hires(), 1);
    assert_eq!(rig.count("worktree add"), 1);
}

#[test]
fn a_pull_request_that_cannot_start_does_not_block_the_others() {
    let rig = Rig::new();
    rig.list(WIDGETS, &[pr(5, 'a'), pr(9, 'b')]);
    rig.script().diff = exit(1, "HTTP 406: diff too large");
    for at in [0, 5, 10, 15, 20, 25] {
        rig.pass(mins(at));
    }
    let asked: Vec<String> = rig
        .fake
        .calls()
        .iter()
        .filter(|c| c.program == Path::new(GH) && strings(c)[1] == "diff")
        .map(|c| strings(c)[2].clone())
        .collect();
    assert_eq!(asked, ["5", "9", "5", "9", "5"], "each gets a turn");
    let outcomes = rig.outcomes();
    assert_eq!(outcomes.len(), 1, "{outcomes:?}");
    assert!(matches!(&outcomes[0], ReviewOutcome::Failed { reason }
        if reason.starts_with("the review could not be started: ")));
}

#[test]
fn a_start_that_fails_outside_the_pull_request_is_never_given_up() {
    let rig = Rig::new();
    rig.list(WIDGETS, &[pr(12, 'a')]);
    *lock(&rig.building.refuse) = Some("Claude Code is not installed".to_string());
    for at in [0, 5, 10, 15, 20, 25] {
        rig.pass(mins(at));
    }
    assert!(rig.outcomes().is_empty());
    *lock(&rig.building.refuse) = None;
    rig.pass(mins(30));
    assert_eq!(rig.hires(), 1);
}

#[test]
fn a_head_another_floor_reviewed_is_skipped_while_that_floor_cannot_check() {
    let rig = Rig::new();
    let done = Review {
        repo: WIDGETS.into(),
        number: 12,
        title: title(12),
        head: sha('a'),
        at: timestamp(mins(0)),
        outcome: ReviewOutcome::Clean,
    };
    rig.add_floor("p-b", vec![done]);
    let failing = rig.root("p-b");
    rig.script().origins.insert(failing, None);
    rig.list(WIDGETS, &[pr(12, 'a')]);
    rig.pass(mins(0));
    rig.pass(mins(5));
    rig.pass(mins(10));
    assert_eq!(rig.hires(), 0);
}

#[test]
fn a_recheck_waits_out_a_rate_limit_and_one_asked_during_a_check_is_kept() {
    let rig = Rig::new();
    rig.script()
        .lists
        .insert(WIDGETS.into(), exit(1, "API rate limit exceeded for user"));
    rig.pass(mins(0));
    assert_eq!(rig.count("pr list"), 1);
    rig.watch.recheck_at(mins(1));
    rig.pass(mins(1));
    assert_eq!(rig.count("pr list"), 1, "the hold is kept");
    rig.list(WIDGETS, &[]);
    rig.watch.recheck_at(mins(11));
    rig.pass(mins(11));
    assert_eq!(rig.count("pr list"), 2);
    // Asked after the check began, it stays asked until a pass takes it.
    rig.watch.recheck_at(mins(12));
    rig.pass(mins(12));
    assert_eq!(rig.count("pr list"), 3);
    rig.pass(mins(13));
    assert_eq!(rig.count("pr list"), 3);
}
