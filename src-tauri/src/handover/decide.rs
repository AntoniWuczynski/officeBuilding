//! The decision for one desk whose tool's subscription may be used up. Pure.

use super::Work;
use super::record::Outcome;
use super::view::{BlockWhy, OfferWhy};

#[expect(
    clippy::struct_excessive_bools,
    reason = "one named fact per rule of the decision table"
)]
pub struct Facts<'a> {
    /// The building started this desk in a terminal of its own that is still running, and it is
    /// neither a manager nor a QA reviewer.
    pub ours: bool,
    /// The desk's tool has a live record, from this desk or from another on the same login.
    pub used_up: bool,
    /// This desk's own transcript proved it.
    pub own: bool,
    /// The automatic decision for this desk's own record is being taken now.
    pub fresh: bool,
    pub automatic: bool,
    /// A Human has chosen what the receiving tool starts with.
    pub chosen: bool,
    /// The receiving tool has a live record.
    pub receiver_used_up: bool,
    /// How this desk's earlier handover for this limit went, if there was one.
    pub before: Option<&'a Outcome>,
    pub work: &'a Work,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Hand it over now, without asking.
    HandOver,
    /// Show the control and wait for A Human.
    Offer(OfferWhy),
    /// Used up, and it stays where it is.
    Stay(BlockWhy),
    /// Nothing to do and nothing to show.
    Nothing,
}

/// The first rule that matches wins.
#[must_use]
pub fn decide(facts: &Facts<'_>) -> Decision {
    if !facts.ours || !facts.used_up {
        return Decision::Nothing;
    }
    if !facts.chosen {
        return Decision::Stay(BlockWhy::NotChosen);
    }
    if matches!(facts.before, Some(Outcome::Started { .. })) {
        return Decision::Stay(BlockWhy::AlreadyHandedOver);
    }
    if facts.receiver_used_up {
        return Decision::Stay(BlockWhy::BothUsedUp);
    }
    if let Some(Outcome::Failed { reason, .. }) = facts.before {
        return Decision::Offer(OfferWhy::Failed {
            reason: reason.clone(),
        });
    }
    if !facts.automatic {
        return Decision::Offer(OfferWhy::AutomaticOff);
    }
    if !facts.own {
        return Decision::Offer(OfferWhy::SeenElsewhere);
    }
    match facts.work {
        Work::Running { .. } => return Decision::Offer(OfferWhy::WorkRunning),
        Work::Unknown => return Decision::Offer(OfferWhy::WorkUnknown),
        Work::Clear => {}
    }
    if !facts.fresh {
        return Decision::Offer(OfferWhy::NotAtTheTime);
    }
    Decision::HandOver
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handover::{Task, TaskKind};

    const CLEAR: Work = Work::Clear;

    fn go(facts: &Facts<'_>) -> Decision {
        decide(facts)
    }

    fn all_true(work: &Work) -> Facts<'_> {
        Facts {
            ours: true,
            used_up: true,
            own: true,
            fresh: true,
            automatic: true,
            chosen: true,
            receiver_used_up: false,
            before: None,
            work,
        }
    }

    fn started() -> Outcome {
        Outcome::Started { session: None }
    }

    fn failed() -> Outcome {
        Outcome::Failed {
            reason: "no terminal".into(),
            ended: false,
        }
    }

    #[test]
    fn named_rows_of_the_brief() {
        let running = Work::Running {
            tasks: vec![Task {
                kind: TaskKind::Workflow,
                name: None,
            }],
        };
        let s = started();
        let rows: Vec<(&str, Facts<'_>, Decision)> = vec![
            ("known not running", all_true(&CLEAR), Decision::HandOver),
            (
                "known running",
                all_true(&running),
                Decision::Offer(OfferWhy::WorkRunning),
            ),
            (
                "unknown",
                all_true(&Work::Unknown),
                Decision::Offer(OfferWhy::WorkUnknown),
            ),
            (
                "switch off",
                Facts {
                    automatic: false,
                    ..all_true(&CLEAR)
                },
                Decision::Offer(OfferWhy::AutomaticOff),
            ),
            (
                "not chosen",
                Facts {
                    chosen: false,
                    ..all_true(&CLEAR)
                },
                Decision::Stay(BlockWhy::NotChosen),
            ),
            (
                "receiver used up",
                Facts {
                    receiver_used_up: true,
                    ..all_true(&CLEAR)
                },
                Decision::Stay(BlockWhy::BothUsedUp),
            ),
            (
                "already handed over",
                Facts {
                    before: Some(&s),
                    ..all_true(&CLEAR)
                },
                Decision::Stay(BlockWhy::AlreadyHandedOver),
            ),
        ];
        for (name, facts, want) in rows {
            assert_eq!(go(&facts), want, "{name}");
        }
    }

    #[test]
    fn one_row_per_rule_in_order() {
        let f = failed();
        let s = started();
        let running = Work::Running { tasks: vec![] };
        let rows: Vec<(&str, Facts<'_>, Decision)> = vec![
            (
                "1 not ours",
                Facts {
                    ours: false,
                    ..all_true(&CLEAR)
                },
                Decision::Nothing,
            ),
            (
                "1 not used up",
                Facts {
                    used_up: false,
                    ..all_true(&CLEAR)
                },
                Decision::Nothing,
            ),
            (
                "2",
                Facts {
                    chosen: false,
                    ..all_true(&CLEAR)
                },
                Decision::Stay(BlockWhy::NotChosen),
            ),
            (
                "3",
                Facts {
                    before: Some(&s),
                    ..all_true(&CLEAR)
                },
                Decision::Stay(BlockWhy::AlreadyHandedOver),
            ),
            (
                "4",
                Facts {
                    receiver_used_up: true,
                    ..all_true(&CLEAR)
                },
                Decision::Stay(BlockWhy::BothUsedUp),
            ),
            (
                "5",
                Facts {
                    before: Some(&f),
                    ..all_true(&CLEAR)
                },
                Decision::Offer(OfferWhy::Failed {
                    reason: "no terminal".into(),
                }),
            ),
            (
                "6",
                Facts {
                    automatic: false,
                    ..all_true(&CLEAR)
                },
                Decision::Offer(OfferWhy::AutomaticOff),
            ),
            (
                "7",
                Facts {
                    own: false,
                    ..all_true(&CLEAR)
                },
                Decision::Offer(OfferWhy::SeenElsewhere),
            ),
            (
                "8",
                all_true(&running),
                Decision::Offer(OfferWhy::WorkRunning),
            ),
            (
                "9",
                all_true(&Work::Unknown),
                Decision::Offer(OfferWhy::WorkUnknown),
            ),
            (
                "10",
                Facts {
                    fresh: false,
                    ..all_true(&CLEAR)
                },
                Decision::Offer(OfferWhy::NotAtTheTime),
            ),
            ("11", all_true(&CLEAR), Decision::HandOver),
        ];
        for (name, facts, want) in rows {
            assert_eq!(go(&facts), want, "rule {name}");
        }
    }

    #[test]
    fn precedence() {
        let f = failed();
        let s = started();
        // Not chosen wins over a failed attempt, a started one and the other tool being used up.
        let not_chosen = Facts {
            chosen: false,
            before: Some(&f),
            receiver_used_up: true,
            ..all_true(&CLEAR)
        };
        assert_eq!(go(&not_chosen), Decision::Stay(BlockWhy::NotChosen));
        // A failed attempt wins over the switch, the proof, the work and freshness.
        let failed_first = Facts {
            before: Some(&f),
            automatic: false,
            own: false,
            fresh: false,
            ..all_true(&Work::Unknown)
        };
        assert_eq!(
            go(&failed_first),
            Decision::Offer(OfferWhy::Failed {
                reason: "no terminal".into()
            })
        );
        // Already handed over wins over both being used up.
        let twice = Facts {
            before: Some(&s),
            receiver_used_up: true,
            ..all_true(&CLEAR)
        };
        assert_eq!(go(&twice), Decision::Stay(BlockWhy::AlreadyHandedOver));
        // Both used up wins over a failed attempt.
        let both = Facts {
            before: Some(&f),
            receiver_used_up: true,
            ..all_true(&CLEAR)
        };
        assert_eq!(go(&both), Decision::Stay(BlockWhy::BothUsedUp));
        // Not the desk's own proof wins over running work and freshness.
        let elsewhere = Facts {
            own: false,
            fresh: false,
            ..all_true(&Work::Unknown)
        };
        assert_eq!(go(&elsewhere), Decision::Offer(OfferWhy::SeenElsewhere));
        // Work wins over freshness.
        let work_first = Facts {
            fresh: false,
            ..all_true(&Work::Unknown)
        };
        assert_eq!(go(&work_first), Decision::Offer(OfferWhy::WorkUnknown));
        // Not ours with everything else true is nothing.
        let not_ours = Facts {
            ours: false,
            ..all_true(&CLEAR)
        };
        assert_eq!(go(&not_ours), Decision::Nothing);
    }
}
