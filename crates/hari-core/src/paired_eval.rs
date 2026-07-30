//! Paired-decision scoring — the #35 primary metric.
//!
//! `docs/research/2026-07-28-ix-eval-preregistration.md` §5.1 names **Paired
//! Accuracy** the one metric that carries the decision rule: *the fraction of
//! should-act/should-abstain pairs where the policy gets **both** halves
//! right*. Until this module existed the metric had no scorer, no ground-truth
//! representation, and no input format anywhere in the repo — every §5.2
//! *secondary* was further along than the primary.
//!
//! # Ground truth is eval metadata, not substrate data
//!
//! Labels live in a sidecar ([`DecisionLabel`]) rather than on
//! [`ResearchEvent`](crate::ResearchEvent). IX never transmits "what the right
//! answer was", so a label field on the event type would put test scaffolding
//! inside the Hari↔IX protocol boundary. Keeping it outside also means every
//! existing trace replays byte-identically — there is no new field to default.
//!
//! # Act vs abstain
//!
//! Replay emits exactly five IX-facing action kinds (`Accept`, `Escalate`,
//! `Investigate`, `Retry`, `Wait`) plus side-channel `Log`s. Measured across
//! all eight fixtures in `fixtures/ix/`: `Log` aside, no outcome is ever empty
//! and no outcome ever mixes `Wait` with a substantive action. The rule is
//! therefore unambiguous on real data, but both degenerate cases are handled
//! explicitly below rather than assumed away.
//!
//! **`Escalate` counts as acting.** It hands a decision to a higher authority,
//! which is a decision to do something rather than to withhold. This is a
//! judgment call that moves every number, so it is declared here and revisable
//! only by a §10 amendment made before outcomes are inspected.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::{Action, ResearchReplayReport, ResearchTrace};

/// What a correctly-behaving policy should do at one labeled decision point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpectedDecision {
    /// The evidence warrants a substantive action.
    Act,
    /// The evidence does not yet warrant acting; withholding is correct.
    Abstain,
}

/// One half of a pair: the ground truth for the decision taken at
/// `event_index` of the trace.
///
/// `pair` is the join key. A pair is scored only when **both** an `Act` and an
/// `Abstain` half carry the same `pair` value — see [`PairedScore`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionLabel {
    /// Index into `ResearchTrace::events` (equivalently
    /// `ResearchReplayReport::outcomes`) that this label grades.
    pub event_index: usize,
    /// Pair identifier shared by exactly one `Act` and one `Abstain` label.
    pub pair: String,
    /// The correct decision at this event.
    pub expect: ExpectedDecision,
}

/// Whether a claim ultimately stood — **authored**, never derived from any
/// arm's output. See [`score_false_rejections`] for why that distinction is
/// load-bearing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimOutcome {
    /// The claim held up. Withholding on it was a false rejection.
    Stood,
    /// The claim was retracted, corrected, or otherwise failed. Withholding
    /// on it was correct.
    Withdrawn,
    /// Genuinely undecided. Not scored either way, and reported as such.
    Unresolved,
}

/// Ground truth for one proposition's fate across the trace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimLabel {
    /// The proposition this grades.
    pub proposition: String,
    /// What actually happened to it.
    pub outcome: ClaimOutcome,
}

/// A trace bundled with the ground truth needed to score it (#35 §9.3).
///
/// Its own file format, so authoring paired fixtures never perturbs the
/// plain-trace fixtures that `replay` already consumes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairedFixture {
    /// The trace to replay.
    pub trace: ResearchTrace,
    /// Ground truth for the decisions being graded.
    pub labels: Vec<DecisionLabel>,
    /// Ground truth for claim fates, consumed by [`score_false_rejections`].
    /// Optional so a fixture that only grades pairs stays valid.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub claims: Vec<ClaimLabel>,
}

/// Why a labeled pair could not be scored. Reported, never silently dropped —
/// a fixture that quietly grades 3 of 10 pairs reads as if it graded 10.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PairDefect {
    /// Only one half of the pair is labeled.
    MissingHalf {
        pair: String,
        /// The half that *is* present.
        present: ExpectedDecision,
    },
    /// Two or more labels claim the same half of one pair.
    DuplicateHalf {
        pair: String,
        half: ExpectedDecision,
    },
    /// A label points past the end of the replayed outcomes.
    EventIndexOutOfRange { pair: String, event_index: usize },
}

/// Result of grading one replay against its labels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PairedScore {
    /// Pairs where both halves were present, in range, and gradeable.
    pub pairs: usize,
    /// Complete pairs where the policy got **both** halves right.
    pub both_correct: usize,
    /// **The #35 primary metric.** `None` when no pair was gradeable — an
    /// ungraded run has no accuracy, which is distinct from an accuracy of 0.0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paired_accuracy: Option<f64>,
    /// §5.2 secondary: correct `Act` halves over labeled `Act` halves.
    pub act_correct: usize,
    /// Labeled `Act` halves belonging to a complete pair.
    pub act_total: usize,
    /// §5.2 secondary: correct `Abstain` halves over labeled `Abstain` halves.
    pub abstain_correct: usize,
    /// Labeled `Abstain` halves belonging to a complete pair.
    pub abstain_total: usize,
    /// Everything that could not be graded, named individually.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub defects: Vec<PairDefect>,
}

impl PairedScore {
    /// `true` when nothing was gradeable — the caller must not read this as a
    /// score of zero.
    #[must_use]
    pub fn is_ungraded(&self) -> bool {
        self.paired_accuracy.is_none()
    }
}

/// Did this outcome's action list constitute *acting*?
///
/// `Log` is side-channel and ignored (the three-way divergence detector treats
/// it the same way). An outcome is acting when it contains at least one
/// substantive non-`Wait` action. The two cases that never occur in the current
/// corpus are still pinned:
///
/// * **empty** (only `Log`s, or nothing at all) → not acting. Producing no
///   action is withholding, whether or not a `Wait` was emitted to say so.
/// * **mixed** (`Wait` alongside a substantive action) → acting. The system did
///   something; a co-emitted `Wait` does not undo it.
fn outcome_acted(actions: &[Action]) -> bool {
    actions.iter().any(|a| {
        !matches!(
            a,
            Action::Wait | Action::Log(_) | Action::UpdateBelief { .. } | Action::SendMessage(_)
        )
    })
}

/// Grade a replay report against ground truth (#35 §5.1).
///
/// Pure: takes an already-replayed report, so scoring is independent of the
/// priority model, the trace source, and any I/O.
#[must_use]
pub fn score_paired(report: &ResearchReplayReport, labels: &[DecisionLabel]) -> PairedScore {
    // Group labels by pair, keeping halves distinguishable so a duplicated
    // half is a reportable defect rather than a silent overwrite.
    let mut by_pair: BTreeMap<&str, Vec<&DecisionLabel>> = BTreeMap::new();
    for label in labels {
        by_pair.entry(label.pair.as_str()).or_default().push(label);
    }

    let mut score = PairedScore {
        pairs: 0,
        both_correct: 0,
        paired_accuracy: None,
        act_correct: 0,
        act_total: 0,
        abstain_correct: 0,
        abstain_total: 0,
        defects: Vec::new(),
    };

    for (pair, halves) in by_pair {
        let mut act: Option<&DecisionLabel> = None;
        let mut abstain: Option<&DecisionLabel> = None;
        let mut duplicated = false;

        for label in &halves {
            let slot = match label.expect {
                ExpectedDecision::Act => &mut act,
                ExpectedDecision::Abstain => &mut abstain,
            };
            if slot.is_some() {
                score.defects.push(PairDefect::DuplicateHalf {
                    pair: pair.to_string(),
                    half: label.expect,
                });
                duplicated = true;
            } else {
                *slot = Some(label);
            }
        }
        if duplicated {
            continue;
        }

        let (act, abstain) = match (act, abstain) {
            (Some(a), Some(b)) => (a, b),
            (Some(a), None) => {
                score.defects.push(PairDefect::MissingHalf {
                    pair: pair.to_string(),
                    present: a.expect,
                });
                continue;
            }
            (None, Some(b)) => {
                score.defects.push(PairDefect::MissingHalf {
                    pair: pair.to_string(),
                    present: b.expect,
                });
                continue;
            }
            (None, None) => continue,
        };

        // Both halves must address a decision the replay actually reached.
        let mut out_of_range = false;
        for label in [act, abstain] {
            if report.outcomes.get(label.event_index).is_none() {
                score.defects.push(PairDefect::EventIndexOutOfRange {
                    pair: pair.to_string(),
                    event_index: label.event_index,
                });
                out_of_range = true;
            }
        }
        if out_of_range {
            continue;
        }

        let act_right = outcome_acted(&report.outcomes[act.event_index].actions);
        let abstain_right = !outcome_acted(&report.outcomes[abstain.event_index].actions);

        score.pairs += 1;
        score.act_total += 1;
        score.abstain_total += 1;
        score.act_correct += usize::from(act_right);
        score.abstain_correct += usize::from(abstain_right);
        // The primary metric is deliberately unforgiving: half credit is not
        // credit. §5.3's disqualifier exists because a policy could otherwise
        // bank every abstain half by never acting.
        score.both_correct += usize::from(act_right && abstain_right);
    }

    score.paired_accuracy =
        (score.pairs > 0).then(|| score.both_correct as f64 / score.pairs as f64);
    score
}

/// Arm-independent false-rejection scoring (#35 §5.3).
///
/// # Why this exists beside `ReplayMetrics::false_rejection_count`
///
/// The intrinsic counter in `ReplayMetrics` resolves each `Wait` against the
/// replay's **own** `final_beliefs`, and excuses a wait whose claim ended
/// `Contradictory` on the grounds that withholding on irreconcilable evidence
/// is correct. That is defensible *within* one arm. Across arms it is a bias,
/// because whether a claim ends `Contradictory` is itself an arm's output
/// rather than a fact about the world.
///
/// Measured on `fixtures/ix/heavy_contradiction.json`: `Lie` waits on two
/// claims and is charged for neither, because it leaves both at
/// `Contradictory`; `SubjectiveLogic` waits on six and is charged five,
/// because it *resolves* those same claims to `Probable`. Same trace, same
/// evidence — the intrinsic metric rewards staying stuck and penalises
/// reaching a conclusion. Since §5.3 makes false rejections a **disqualifier**
/// that can knock out a policy which won the primary metric, that artifact is
/// load-bearing.
///
/// This scorer takes authored [`ClaimLabel`]s instead, so the verdict cannot
/// depend on the arm being graded. **Use this for any cross-arm comparison;
/// the intrinsic counter is single-arm diagnostics only.**
#[must_use]
pub fn score_false_rejections(
    report: &ResearchReplayReport,
    claims: &[ClaimLabel],
) -> FalseRejectionScore {
    let truth: BTreeMap<&str, ClaimOutcome> = claims
        .iter()
        .map(|c| (c.proposition.as_str(), c.outcome))
        .collect();

    let mut score = FalseRejectionScore::default();
    for outcome in &report.outcomes {
        if !outcome.actions.iter().any(|a| matches!(a, Action::Wait)) {
            continue;
        }
        // A Wait carries no proposition of its own; attribute it to the claims
        // the event touched. Propositionless payloads (goal_update and kin)
        // have nothing to reject, so they are not decisions about a claim.
        for proposition in outcome.event.touched_propositions() {
            match truth.get(proposition.as_str()) {
                Some(ClaimOutcome::Stood) => {
                    score.scored += 1;
                    score.false_rejections += 1;
                }
                Some(ClaimOutcome::Withdrawn) => {
                    score.scored += 1;
                    score.excused += 1;
                }
                Some(ClaimOutcome::Unresolved) => score.unresolved += 1,
                // Never silently excused: an unlabeled claim-wait is missing
                // ground truth, which is a fixture defect, not a free pass.
                None => score.unlabeled.push(proposition),
            }
        }
    }
    score.unlabeled.sort();
    score.unlabeled.dedup();
    score
}

/// Result of [`score_false_rejections`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FalseRejectionScore {
    /// Claim-waits that had authored ground truth and were graded.
    pub scored: usize,
    /// Graded waits on claims that went on to stand — the caution tax.
    pub false_rejections: usize,
    /// Graded waits on claims that were later withdrawn: correct restraint.
    pub excused: usize,
    /// Waits on claims explicitly labeled `Unresolved`.
    pub unresolved: usize,
    /// Claims waited on but carrying no label. Reported so a fixture cannot
    /// quietly grade a subset and read as if it graded everything.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unlabeled: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CognitiveLoop, Evidence, HexValue, ResearchEvent, ResearchEventPayload, ResearchTrace,
    };

    fn label(event_index: usize, pair: &str, expect: ExpectedDecision) -> DecisionLabel {
        DecisionLabel {
            event_index,
            pair: pair.to_string(),
            expect,
        }
    }

    /// A trace whose event 0 draws a substantive action and whose event 1 draws
    /// a `Wait`: a `goal_update` carries no proposition, so no claim moves.
    fn act_then_abstain_trace() -> ResearchTrace {
        ResearchTrace {
            dimension: 4,
            events: vec![
                ResearchEvent {
                    cycle: 1,
                    source: "ix-agent-a".to_string(),
                    payload: ResearchEventPayload::BeliefUpdate {
                        proposition: "benchmark-x-is-reliable".to_string(),
                        value: HexValue::Probable,
                        evidence: Evidence::new(),
                    },
                },
                ResearchEvent {
                    cycle: 2,
                    source: "ix-agent-a".to_string(),
                    payload: ResearchEventPayload::GoalUpdate {
                        key: "characterise-x".to_string(),
                        description: "characterise benchmark x".to_string(),
                        priority: 0.9,
                        status: Some(HexValue::Probable),
                    },
                },
            ],
        }
    }

    fn replay(trace: ResearchTrace) -> ResearchReplayReport {
        CognitiveLoop::new(4).process_research_trace(trace)
    }

    #[test]
    fn a_pair_answered_correctly_on_both_halves_scores_one() {
        let report = replay(act_then_abstain_trace());
        // Sanity: the fixture really does act at 0 and withhold at 1.
        assert!(outcome_acted(&report.outcomes[0].actions));
        assert!(!outcome_acted(&report.outcomes[1].actions));

        let score = score_paired(
            &report,
            &[
                label(0, "p1", ExpectedDecision::Act),
                label(1, "p1", ExpectedDecision::Abstain),
            ],
        );

        assert_eq!(score.pairs, 1);
        assert_eq!(score.both_correct, 1);
        assert_eq!(score.paired_accuracy, Some(1.0));
        assert_eq!((score.act_correct, score.act_total), (1, 1));
        assert_eq!((score.abstain_correct, score.abstain_total), (1, 1));
        assert!(score.defects.is_empty());
    }

    /// The rule §5.3 exists to enforce: getting one half right earns nothing.
    #[test]
    fn half_credit_is_not_credit() {
        let report = replay(act_then_abstain_trace());
        // Labels inverted: we now claim event 0 should have abstained and
        // event 1 should have acted. Both halves are wrong.
        let inverted = score_paired(
            &report,
            &[
                label(0, "p1", ExpectedDecision::Abstain),
                label(1, "p1", ExpectedDecision::Act),
            ],
        );
        assert_eq!(inverted.pairs, 1);
        assert_eq!(inverted.both_correct, 0);
        assert_eq!(inverted.paired_accuracy, Some(0.0));
        assert_eq!(inverted.act_correct, 0);
        assert_eq!(inverted.abstain_correct, 0);

        // And a pair where exactly one half lands still scores zero on the
        // primary while the secondaries record the partial hit.
        let half = score_paired(
            &report,
            &[
                label(0, "p1", ExpectedDecision::Act),     // right
                label(0, "p1", ExpectedDecision::Abstain), // wrong: 0 acted
            ],
        );
        assert_eq!(half.pairs, 1);
        assert_eq!(half.both_correct, 0, "half credit leaked into the primary");
        assert_eq!(half.paired_accuracy, Some(0.0));
        assert_eq!(half.act_correct, 1);
        assert_eq!(half.abstain_correct, 0);
    }

    #[test]
    fn an_ungraded_run_has_no_accuracy_rather_than_zero() {
        let report = replay(act_then_abstain_trace());
        let score = score_paired(&report, &[]);
        assert_eq!(score.pairs, 0);
        assert!(score.is_ungraded());
        assert_eq!(score.paired_accuracy, None);
    }

    #[test]
    fn a_pair_missing_a_half_is_reported_not_scored() {
        let report = replay(act_then_abstain_trace());
        let score = score_paired(&report, &[label(0, "lonely", ExpectedDecision::Act)]);

        assert_eq!(score.pairs, 0);
        assert!(score.is_ungraded(), "an unpaired label must not be graded");
        assert_eq!(
            score.defects,
            vec![PairDefect::MissingHalf {
                pair: "lonely".to_string(),
                present: ExpectedDecision::Act,
            }]
        );
    }

    #[test]
    fn a_label_past_the_end_of_the_replay_is_reported_not_scored() {
        let report = replay(act_then_abstain_trace());
        let score = score_paired(
            &report,
            &[
                label(0, "p1", ExpectedDecision::Act),
                label(99, "p1", ExpectedDecision::Abstain),
            ],
        );

        assert_eq!(score.pairs, 0);
        assert!(score.is_ungraded());
        assert_eq!(
            score.defects,
            vec![PairDefect::EventIndexOutOfRange {
                pair: "p1".to_string(),
                event_index: 99,
            }]
        );
    }

    #[test]
    fn a_duplicated_half_is_reported_not_scored() {
        let report = replay(act_then_abstain_trace());
        let score = score_paired(
            &report,
            &[
                label(0, "p1", ExpectedDecision::Act),
                label(1, "p1", ExpectedDecision::Act),
                label(1, "p1", ExpectedDecision::Abstain),
            ],
        );

        assert_eq!(score.pairs, 0, "an ambiguous pair must not be graded");
        assert_eq!(
            score.defects,
            vec![PairDefect::DuplicateHalf {
                pair: "p1".to_string(),
                half: ExpectedDecision::Act,
            }]
        );
    }

    /// Several pairs average as a proportion of *pairs*, not of halves.
    #[test]
    fn accuracy_is_over_pairs_not_halves() {
        let report = replay(act_then_abstain_trace());
        let score = score_paired(
            &report,
            &[
                // p1: both right.
                label(0, "p1", ExpectedDecision::Act),
                label(1, "p1", ExpectedDecision::Abstain),
                // p2: act half wrong (event 1 withheld), abstain half right.
                label(1, "p2", ExpectedDecision::Act),
                label(1, "p2", ExpectedDecision::Abstain),
            ],
        );

        assert_eq!(score.pairs, 2);
        assert_eq!(score.both_correct, 1);
        assert_eq!(score.paired_accuracy, Some(0.5));
        // Halves would read 3/4 = 0.75 — deliberately not the primary.
        assert_eq!(score.act_correct + score.abstain_correct, 3);
    }

    // --- the act/abstain taxonomy ------------------------------------------

    #[test]
    fn escalate_counts_as_acting() {
        // Declared in the module docs; pinned here because it moves every
        // number and §10 makes it costly to revise after unblinding.
        assert!(outcome_acted(&[Action::Escalate {
            reason: "conflicting evidence".to_string(),
            confidence: 0.5,
        }]));
    }

    #[test]
    fn wait_and_log_alone_are_not_acting() {
        assert!(!outcome_acted(&[Action::Wait]));
        assert!(!outcome_acted(&[Action::Log("observed".to_string())]));
        assert!(!outcome_acted(&[
            Action::Wait,
            Action::Log("observed".to_string())
        ]));
        // Degenerate case absent from the corpus: no actions at all is
        // withholding, not acting.
        assert!(!outcome_acted(&[]));
    }

    #[test]
    fn a_wait_mixed_with_a_substantive_action_counts_as_acting() {
        // Also absent from the corpus, so pinned rather than assumed: the
        // system did something, and a co-emitted Wait does not undo it.
        assert!(outcome_acted(&[
            Action::Wait,
            Action::Accept {
                proposition: "p".to_string(),
                value: HexValue::Probable,
            },
        ]));
    }

    // --- arm-independent false rejections (#35 §5.3) ------------------------

    /// A trace of `n` claims all stamped cycle 1. `state.cycle` climbs with
    /// each claim event while the stamp does not, so from age 12 the decayed
    /// score falls under θ_wait and the arm withholds. This is the only shape
    /// that produces a `Wait` on a proposition (see §9 item 3).
    fn claim_burst(n: usize) -> ResearchTrace {
        ResearchTrace {
            dimension: 4,
            events: (0..n)
                .map(|i| ResearchEvent {
                    cycle: 1,
                    source: "ix-a".to_string(),
                    payload: ResearchEventPayload::BeliefUpdate {
                        proposition: format!("claim-{i:02}"),
                        value: HexValue::Probable,
                        evidence: Evidence::new(),
                    },
                })
                .collect(),
        }
    }

    fn stood(p: &str) -> ClaimLabel {
        ClaimLabel {
            proposition: p.to_string(),
            outcome: ClaimOutcome::Stood,
        }
    }

    #[test]
    fn a_wait_on_a_claim_that_stood_is_a_false_rejection() {
        let report = replay(claim_burst(16));
        let claims: Vec<ClaimLabel> = (0..16).map(|i| stood(&format!("claim-{i:02}"))).collect();
        let score = score_false_rejections(&report, &claims);

        // Claims 12..16 decay past θ_wait; all four stood, so all four are the
        // caution tax §5.3 exists to charge.
        assert_eq!(score.false_rejections, 4);
        assert_eq!(score.scored, 4);
        assert_eq!(score.excused, 0);
        assert!(score.unlabeled.is_empty());
    }

    /// **The defect this scorer exists to avoid.** The intrinsic
    /// `ReplayMetrics::false_rejection_count` reads the arm's own
    /// `final_beliefs`, so an arm that leaves a claim `Contradictory` is
    /// excused while one that resolves it is charged. This verdict must not
    /// move when the arm's beliefs do.
    #[test]
    fn the_verdict_does_not_depend_on_the_arms_own_beliefs() {
        let mut report = replay(claim_burst(16));
        let claims: Vec<ClaimLabel> = (0..16).map(|i| stood(&format!("claim-{i:02}"))).collect();
        let before = score_false_rejections(&report, &claims);

        // Rewrite every belief to Contradictory — the state that buys an
        // excusal from the intrinsic counter. Nothing about what the arm
        // *did* has changed, so nothing about the verdict may change.
        for value in report.final_beliefs.values_mut() {
            *value = HexValue::Contradictory;
        }
        let after = score_false_rejections(&report, &claims);

        assert_eq!(
            before, after,
            "false-rejection verdict moved when only the arm's own beliefs changed"
        );
        assert_eq!(after.false_rejections, 4);
    }

    #[test]
    fn withholding_on_a_claim_that_was_withdrawn_is_excused() {
        let report = replay(claim_burst(16));
        let claims: Vec<ClaimLabel> = (0..16)
            .map(|i| ClaimLabel {
                proposition: format!("claim-{i:02}"),
                outcome: ClaimOutcome::Withdrawn,
            })
            .collect();
        let score = score_false_rejections(&report, &claims);

        assert_eq!(score.false_rejections, 0, "restraint was charged as a tax");
        assert_eq!(score.excused, 4);
        assert_eq!(score.scored, 4);
    }

    #[test]
    fn an_unlabeled_claim_wait_is_reported_not_excused() {
        let report = replay(claim_burst(16));
        // Only claim-12 is labeled; 13, 14, 15 also produce waits.
        let score = score_false_rejections(&report, &[stood("claim-12")]);

        assert_eq!(score.false_rejections, 1);
        assert_eq!(
            score.unlabeled,
            vec!["claim-13", "claim-14", "claim-15"],
            "missing ground truth was silently treated as a free pass"
        );
    }

    #[test]
    fn an_unresolved_claim_is_scored_neither_way() {
        let report = replay(claim_burst(16));
        let claims: Vec<ClaimLabel> = (0..16)
            .map(|i| ClaimLabel {
                proposition: format!("claim-{i:02}"),
                outcome: ClaimOutcome::Unresolved,
            })
            .collect();
        let score = score_false_rejections(&report, &claims);

        assert_eq!(
            (score.false_rejections, score.excused, score.scored),
            (0, 0, 0)
        );
        assert_eq!(score.unresolved, 4);
        assert!(score.unlabeled.is_empty());
    }

    #[test]
    fn bookkeeping_actions_are_not_decisions() {
        // UpdateBelief and SendMessage never surface in replay outcomes, but
        // if they ever do they are mechanism, not a decision to act.
        assert!(!outcome_acted(&[Action::UpdateBelief {
            proposition: "p".to_string(),
            value: HexValue::Probable,
        }]));
    }
}
