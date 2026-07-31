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
//! `Investigate`, `Retry`, `Wait`) plus side-channel `Log`s. Two degenerate
//! cases are possible in principle and are handled explicitly below rather
//! than assumed away. Measured across all eight fixtures in `fixtures/ix/`:
//!
//! * **Mixed** (`Wait` beside a substantive action) occurs **nowhere**, in any
//!   arm. Genuinely hypothetical.
//! * **Empty** (`Log`s only) occurs **27 times under `SubjectiveLogic`** — on
//!   every `goal_update` and `relation_declaration`, which SL logs without
//!   recommending. Zero times under `RecencyDecay` or `Lie`.
//!
//! An earlier version of this comment claimed no outcome is ever empty. That
//! was measured on the default arm alone and was false for SL — and the empty
//! branch is not a curiosity: it is exactly what awards SL the abstain half of
//! every G2 pair in the §9.3 corpora. The rule is unambiguous on real data;
//! the claim that one of its branches is unexercised was not.
//!
//! **`Escalate` counts as acting.** It hands a decision to a higher authority,
//! which is a decision to do something rather than to withhold. This is a
//! judgment call that moves every number, so it is declared here and revisable
//! only by a §10 amendment made before outcomes are inspected.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::subjective_logic::{
    compare_replay_three_way, SubjectiveLogicConfig, ThreeWayReplayReport,
};
use crate::{Action, PriorityModel, ResearchReplayReport, ResearchTrace};

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

/// How one gradeable pair fared, kept alongside the aggregate.
///
/// Exists because the aggregate cannot answer the question §9.3 raises: *which*
/// pairs separate the arms. A corpus whose abstain halves encode evidence
/// insufficiency is expected to score 0.0 under the evidence-blind hexavalent
/// arms and non-zero under Subjective Logic — and the per-pair list is what
/// distinguishes that intended result from a fixture that is simply broken.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairOutcome {
    /// The pair identifier shared by both halves.
    pub pair: String,
    /// The policy acted at the `Act` half, as it should have.
    pub act_correct: bool,
    /// The policy withheld at the `Abstain` half, as it should have.
    pub abstain_correct: bool,
}

impl PairOutcome {
    /// Both halves right — the unit the primary metric counts.
    #[must_use]
    pub fn both_correct(&self) -> bool {
        self.act_correct && self.abstain_correct
    }
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
    /// Per-pair verdicts for every gradeable pair, ordered by pair name.
    /// Ungradeable pairs are absent here and present in `defects` instead.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub per_pair: Vec<PairOutcome>,
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
        per_pair: Vec::new(),
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
        score.per_pair.push(PairOutcome {
            pair: pair.to_string(),
            act_correct: act_right,
            abstain_correct: abstain_right,
        });
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
        // NOTE — this disagrees with `outcome_acted` on one case both modules
        // claim to have pinned. A *mixed* outcome (`Wait` beside a substantive
        // action) is classified as **acting** by §5.1's taxonomy, yet charged
        // here, because this triggers on the presence of any `Wait`. The case
        // is currently unreachable — zero mixed outcomes across every arm,
        // fixture and paired corpus — so no number is affected. Recorded so the
        // divergence is found here rather than the hard way if a future policy
        // makes it reachable; resolving it then is a §10 amendment, since it
        // changes what the §5.3 disqualifier charges.
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

// ---------------------------------------------------------------------------
// Three-arm scoring — the shape §5.1 actually asks for
// ---------------------------------------------------------------------------

/// One arm's grades, tagged with the model that produced them.
///
/// `false_rejections` is the authored-ground-truth scorer, never
/// `ReplayMetrics::false_rejection_count` — see [`score_false_rejections`] for
/// why the intrinsic counter cannot be compared across arms.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PairedArm {
    /// The model this arm ran under.
    pub priority_model: PriorityModel,
    /// §5.1 primary plus its §5.2 act/abstain decomposition.
    pub paired: PairedScore,
    /// §5.3 disqualifier input.
    pub false_rejections: FalseRejectionScore,
}

/// A pair the arms did not agree on, and how each fared.
///
/// `None` means the pair was ungradeable in that arm. Ground truth and trace
/// structure are arm-independent, so in practice a pair is gradeable in all
/// three arms or in none — but that is an invariant worth *reporting* rather
/// than assuming, since a per-arm difference would mean the fixture is being
/// graded against different decisions depending on the model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairSeparation {
    pub pair: String,
    pub recency_decay: Option<bool>,
    pub lie: Option<bool>,
    pub subjective_logic: Option<bool>,
}

/// Every arm's grades against **one** shared label set (#35 §5.1).
///
/// # Why one label set, replayed three times
///
/// §5.1 defines the primary metric per arm; the comparison is the point. The
/// alternative — one fixture file per arm — would put the same ground truth in
/// three places and let them drift, and drift in ground truth is
/// indistinguishable from a policy difference once the numbers are in. Labels
/// grade the *decision point*, which is a property of the trace, not of the
/// model that reads it.
///
/// The three `ResearchReplayReport`s are preserved verbatim in `replay` so a
/// surprising grade can be traced back to the actions that produced it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreeWayPairedReport {
    /// The underlying three-arm replay, unmodified.
    pub replay: ThreeWayReplayReport,
    pub recency_decay: PairedArm,
    pub lie: PairedArm,
    pub subjective_logic: PairedArm,
    /// Pairs where the three arms did not all reach the same verdict. Empty
    /// means the fixture does not discriminate between the models at all,
    /// which for a §9.3 fixture is a failure of the fixture, not a result.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub separating_pairs: Vec<PairSeparation>,
}

impl ThreeWayPairedReport {
    /// `true` when no pair separated the arms — every model graded identically.
    #[must_use]
    pub fn is_undiscriminating(&self) -> bool {
        self.separating_pairs.is_empty()
    }
}

/// Replay a [`PairedFixture`] under `RecencyDecay`, `Lie`, and Subjective
/// Logic, grading all three against the fixture's single label set.
#[must_use]
pub fn score_paired_three_way(
    fixture: PairedFixture,
    sl_config: SubjectiveLogicConfig,
) -> ThreeWayPairedReport {
    let PairedFixture {
        trace,
        labels,
        claims,
    } = fixture;
    let replay = compare_replay_three_way(trace, sl_config);

    let grade = |report: &ResearchReplayReport| PairedArm {
        priority_model: report.priority_model,
        paired: score_paired(report, &labels),
        false_rejections: score_false_rejections(report, &claims),
    };

    let recency_decay = grade(&replay.recency_decay);
    let lie = grade(&replay.lie);
    let subjective_logic = grade(&replay.subjective_logic);

    let separating_pairs = separations(&recency_decay, &lie, &subjective_logic);

    ThreeWayPairedReport {
        replay,
        recency_decay,
        lie,
        subjective_logic,
        separating_pairs,
    }
}

/// Pairs where the arms disagree, over the union of every pair any arm graded.
///
/// The union rather than the intersection: a pair gradeable in one arm and not
/// another is exactly the anomaly [`PairSeparation`] documents, and taking the
/// intersection would hide it.
fn separations(decay: &PairedArm, lie: &PairedArm, sl: &PairedArm) -> Vec<PairSeparation> {
    let verdicts = |arm: &PairedArm| -> BTreeMap<String, bool> {
        arm.paired
            .per_pair
            .iter()
            .map(|p| (p.pair.clone(), p.both_correct()))
            .collect()
    };
    let (d, l, s) = (verdicts(decay), verdicts(lie), verdicts(sl));

    let mut names: Vec<&String> = d.keys().chain(l.keys()).chain(s.keys()).collect();
    names.sort();
    names.dedup();

    names
        .into_iter()
        .filter_map(|pair| {
            let (dv, lv, sv) = (
                d.get(pair).copied(),
                l.get(pair).copied(),
                s.get(pair).copied(),
            );
            (dv != lv || dv != sv || lv != sv).then(|| PairSeparation {
                pair: pair.clone(),
                recency_decay: dv,
                lie: lv,
                subjective_logic: sv,
            })
        })
        .collect()
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

    // -----------------------------------------------------------------------
    // Three-arm scoring
    // -----------------------------------------------------------------------

    fn paired_fixture() -> PairedFixture {
        PairedFixture {
            trace: act_then_abstain_trace(),
            labels: vec![
                label(0, "p1", ExpectedDecision::Act),
                label(1, "p1", ExpectedDecision::Abstain),
            ],
            claims: vec![ClaimLabel {
                proposition: "benchmark-x-is-reliable".to_string(),
                outcome: ClaimOutcome::Stood,
            }],
        }
    }

    /// The property that justifies one fixture file instead of three: ground
    /// truth grades the *decision point*, so nothing about the label set may
    /// depend on which model reads the trace.
    ///
    /// Defects and gradeable-pair counts derive from the labels and the outcome
    /// count, both arm-independent — so if these ever diverge, the arms are
    /// being graded against different decisions and no cross-arm number means
    /// anything.
    #[test]
    fn theorem_gradeability_is_arm_independent() {
        let graded = score_paired_three_way(paired_fixture(), SubjectiveLogicConfig::default());
        let arms = [&graded.recency_decay, &graded.lie, &graded.subjective_logic];

        for arm in arms {
            assert_eq!(
                arm.paired.defects, graded.recency_decay.paired.defects,
                "defect list differs by arm — ground truth is not arm-independent"
            );
            assert_eq!(
                arm.paired.pairs, graded.recency_decay.paired.pairs,
                "gradeable-pair count differs by arm"
            );
            let names: Vec<&str> = arm
                .paired
                .per_pair
                .iter()
                .map(|p| p.pair.as_str())
                .collect();
            let expected: Vec<&str> = graded
                .recency_decay
                .paired
                .per_pair
                .iter()
                .map(|p| p.pair.as_str())
                .collect();
            assert_eq!(names, expected, "graded pair names differ by arm");
        }
    }

    /// Each arm is tagged with the model that actually produced it — a report
    /// whose grades cannot be attributed to a policy is not a comparison.
    #[test]
    fn each_arm_reports_its_own_priority_model() {
        let graded = score_paired_three_way(paired_fixture(), SubjectiveLogicConfig::default());
        assert_eq!(
            graded.recency_decay.priority_model,
            PriorityModel::RecencyDecay
        );
        assert_eq!(graded.lie.priority_model, PriorityModel::Lie);
        assert_eq!(
            graded.subjective_logic.priority_model,
            PriorityModel::SubjectiveLogic
        );
    }

    /// `per_pair` must reconstruct the aggregate exactly. They are computed in
    /// the same pass, so a mismatch means one of them was edited without the
    /// other — and the per-pair list is what §9.3 reads to tell an intended
    /// null result from a broken fixture.
    #[test]
    fn theorem_per_pair_reconstructs_the_aggregate() {
        let graded = score_paired_three_way(paired_fixture(), SubjectiveLogicConfig::default());
        for arm in [&graded.recency_decay, &graded.lie, &graded.subjective_logic] {
            let s = &arm.paired;
            assert_eq!(s.per_pair.len(), s.pairs);
            assert_eq!(
                s.per_pair.iter().filter(|p| p.both_correct()).count(),
                s.both_correct
            );
            assert_eq!(
                s.per_pair.iter().filter(|p| p.act_correct).count(),
                s.act_correct
            );
            assert_eq!(
                s.per_pair.iter().filter(|p| p.abstain_correct).count(),
                s.abstain_correct
            );
        }
    }

    /// Subjective Logic withholds where the hexavalent arms commit.
    ///
    /// A single `Probable` report from one source fuses to `b=0.550 u=0.300`
    /// (`P=0.700`), which does not clear SL's accept threshold, so SL emits
    /// `Wait` and misses the **act** half. This is the mirror of the §9.3
    /// prediction — SL's evidence-sensitivity is a cost on act halves, not only
    /// a benefit on abstain halves — and it is pinned here because it is the
    /// first measured evidence that the primary metric is not one-sided.
    #[test]
    fn subjective_logic_pays_for_its_caution_on_the_act_half() {
        let graded = score_paired_three_way(paired_fixture(), SubjectiveLogicConfig::default());

        assert_eq!(graded.recency_decay.paired.paired_accuracy, Some(1.0));
        assert_eq!(graded.lie.paired.paired_accuracy, Some(1.0));
        assert_eq!(graded.subjective_logic.paired.paired_accuracy, Some(0.0));

        let sl = &graded.subjective_logic.paired.per_pair[0];
        assert!(!sl.act_correct, "SL is expected to under-commit here");
        assert!(sl.abstain_correct);

        // And the §5.3 disqualifier charges it: the claim it withheld on stood.
        assert_eq!(graded.subjective_logic.false_rejections.false_rejections, 1);
        assert_eq!(graded.recency_decay.false_rejections.false_rejections, 0);
    }

    /// The separation list is the fixture's diagnostic, so it must name the
    /// disagreeing pair and report each arm's verdict.
    #[test]
    fn separating_pairs_names_the_pair_the_arms_split_on() {
        let graded = score_paired_three_way(paired_fixture(), SubjectiveLogicConfig::default());
        assert!(!graded.is_undiscriminating());
        assert_eq!(
            graded.separating_pairs,
            vec![PairSeparation {
                pair: "p1".to_string(),
                recency_decay: Some(true),
                lie: Some(true),
                subjective_logic: Some(false),
            }]
        );
    }

    /// A fixture every arm grades identically has measured nothing. The report
    /// must say so rather than present three matching numbers as agreement.
    #[test]
    fn a_fixture_no_arm_splits_on_is_reported_as_undiscriminating() {
        // Labels inverted, so every arm gets the act half wrong and the abstain
        // half wrong or right identically — SL included, since it withholds at
        // both indexes.
        let mut fixture = paired_fixture();
        fixture.labels = vec![
            label(1, "p1", ExpectedDecision::Act),
            label(0, "p1", ExpectedDecision::Abstain),
        ];
        let graded = score_paired_three_way(fixture, SubjectiveLogicConfig::default());

        // Vacuity guard: the pair must actually have been graded. An empty
        // separation list is otherwise trivially satisfiable by grading nothing.
        assert_eq!(graded.recency_decay.paired.pairs, 1);
        assert_eq!(graded.subjective_logic.paired.pairs, 1);
        assert!(graded.is_undiscriminating());
        assert!(graded.separating_pairs.is_empty());
    }

    /// An ungradeable pair must not be silently dropped from the separation
    /// list — `None` in one arm is exactly the anomaly worth surfacing.
    #[test]
    fn an_ungradeable_pair_yields_no_separation_entry_but_a_defect_in_every_arm() {
        let mut fixture = paired_fixture();
        fixture
            .labels
            .push(label(0, "orphan", ExpectedDecision::Act));
        let graded = score_paired_three_way(fixture, SubjectiveLogicConfig::default());

        for arm in [&graded.recency_decay, &graded.lie, &graded.subjective_logic] {
            assert_eq!(
                arm.paired.defects,
                vec![PairDefect::MissingHalf {
                    pair: "orphan".to_string(),
                    present: ExpectedDecision::Act,
                }]
            );
        }
        // `orphan` is gradeable in no arm, so it cannot separate them.
        assert!(graded.separating_pairs.iter().all(|s| s.pair != "orphan"));
    }
}
