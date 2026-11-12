//! What the QA desk remembers about its reviews. These types are stored in `store.json`
//! and sent to the webview as they are.

use serde::{Deserialize, Serialize};

/// The most reviews kept for one floor: the oldest is dropped.
pub const MAX_REVIEWS: usize = 200;

/// What the reviewer concluded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Verdict {
    /// Nothing that should hold the pull request up.
    Pass,
    /// At least one finding worth acting on.
    Concerns,
}

/// How one review ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum ReviewOutcome {
    /// A review was posted on the pull request.
    Posted {
        verdict: Verdict,
        findings: u32,
        review_id: u64,
    },
    /// Nothing to flag, and nothing was posted.
    Clean,
    /// No review came of it: why, in words. No trailing full stop.
    Failed { reason: String },
}

/// One review the desk attempted. Kept so a commit is never reviewed twice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Review {
    /// "owner/name".
    pub repo: String,
    pub number: u64,
    pub title: String,
    /// The head commit that was reviewed: 40 hex characters.
    pub head: String,
    /// When it ended (ISO 8601, UTC, seconds).
    pub at: String,
    pub outcome: ReviewOutcome,
}

/// A floor's QA record in `store.json`, under its absolute repo root.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct QaRecord {
    /// None: follow the Settings default.
    pub enabled: Option<bool>,
    /// Oldest first, at most `MAX_REVIEWS`.
    pub reviews: Vec<Review>,
}

#[cfg(test)]
pub(crate) mod fixtures {
    use super::{Review, ReviewOutcome};

    pub(crate) fn review(number: u64, head: &str, outcome: ReviewOutcome) -> Review {
        Review {
            repo: "example-org/widgets".into(),
            number,
            title: "Add a retry to the uploader".into(),
            head: head.into(),
            at: "2026-11-12T10:05:00Z".into(),
            outcome,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::review;
    use super::*;
    use serde_json::json;

    #[test]
    fn each_outcome_matches_its_wire_json_and_reads_back() {
        let cases = [
            (
                ReviewOutcome::Posted {
                    verdict: Verdict::Concerns,
                    findings: 2,
                    review_id: 90210,
                },
                json!({"kind":"posted","verdict":"concerns","findings":2,"reviewId":90210}),
            ),
            (ReviewOutcome::Clean, json!({"kind":"clean"})),
            (
                ReviewOutcome::Failed {
                    reason: "the reviewer left".into(),
                },
                json!({"kind":"failed","reason":"the reviewer left"}),
            ),
        ];
        for (outcome, wire) in cases {
            assert_eq!(serde_json::to_value(&outcome).expect("value"), wire);
            assert_eq!(
                serde_json::from_value::<ReviewOutcome>(wire).expect("back"),
                outcome
            );
        }
    }

    #[test]
    fn a_review_round_trips() {
        let r = review(12, &"0".repeat(40), ReviewOutcome::Clean);
        let text = serde_json::to_string(&r).expect("json");
        assert_eq!(serde_json::from_str::<Review>(&text).expect("back"), r);
        assert_eq!(
            serde_json::to_value(&r).expect("value")["at"],
            "2026-11-12T10:05:00Z"
        );
    }

    #[test]
    fn an_empty_record_is_the_default() {
        let d = QaRecord::default();
        assert_eq!(d.enabled, None);
        assert!(d.reviews.is_empty());
        assert_eq!(serde_json::from_str::<QaRecord>("{}").expect("empty"), d);
    }
}
