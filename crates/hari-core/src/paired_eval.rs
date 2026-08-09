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
    /// Where this fixture came from, stamped by the driver that produced it.
    /// Absent on every hand-authored fixture — which is how [`fixture_provenance`]
    /// reads them as [`CorpusProvenance::Authored`] without anyone saying so.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<FixtureProvenance>,
}

/// A driver's stamp on a fixture it generated (#35 §9 item 4).
///
/// # Why this exists rather than a command-line flag
///
/// §9.4 adopted a standing rule that no §8 verdict may be computed on an
/// authored corpus, and the first cut enforced it from `--corpus authored`
/// on the command line. That made the rule exactly as strong as the flag:
/// `--corpus recorded` over the six hand-authored task fixtures was accepted
/// without complaint and printed `provenance: driver_recorded`. Nothing
/// inspected the fixture. §9.4 says the rule is *"enforced rather than
/// remembered"*; against the corpus it was only the latter.
///
/// The stamp carries a digest of the trace it was computed over, so a fixture
/// whose trace was edited after generation no longer verifies and is refused.
/// **This is tamper-evidence, not tamper-proofing:** [`trace_digest`] is a
/// non-cryptographic hash and anyone who can edit the fixture can recompute it.
/// The failure mode it closes is the one that actually happened — a corpus
/// declared recorded because the operator believed it was.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FixtureProvenance {
    /// The driver that generated this fixture, e.g. `ix_reference/paired_driver`.
    pub driver: String,
    /// The declared generative spec the trace was drawn from (§9.4: the
    /// distribution is the disclosable unit, not the 54 decisions).
    pub spec: String,
    /// The seed this trace was drawn with. With `spec`, it is what makes the
    /// corpus re-derivable rather than merely re-readable.
    pub seed: u64,
    /// [`trace_digest`] of `PairedFixture::trace` at generation time, hex.
    pub trace_digest: String,
}

/// Why a fixture's provenance stamp could not be honoured.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProvenanceDefect {
    /// The stamp does not match the trace it is attached to.
    DigestMismatch {
        driver: String,
        stamped: String,
        recomputed: String,
    },
    /// The caller declared a provenance the fixtures do not support.
    Undeclared { declared: CorpusProvenance },
}

impl std::fmt::Display for ProvenanceDefect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DigestMismatch {
                driver,
                stamped,
                recomputed,
            } => write!(
                f,
                "fixture stamped by `{driver}` carries trace_digest {stamped} but its trace \
                 digests to {recomputed} — the trace was edited after the driver recorded it, \
                 so the stamp attributes numbers to a run that did not produce them"
            ),
            Self::Undeclared { declared } => write!(
                f,
                "--corpus {} was asserted, but no fixture carries a driver provenance stamp. \
                 §9.4's standing rule is enforced against the corpus, not against the command \
                 line: a hand-authored fixture cannot be declared recorded.",
                match declared {
                    CorpusProvenance::Authored => "authored",
                    CorpusProvenance::DriverRecorded => "recorded",
                }
            ),
        }
    }
}

impl std::error::Error for ProvenanceDefect {}

/// FNV-1a-64 over the canonical JSON serialization of a trace, hex.
///
/// Deliberately not a cryptographic hash and deliberately not a dependency: the
/// property needed is that an edit to the trace changes the digest, which FNV-1a
/// gives in ~10 lines with no crate that a version bump could move. See
/// [`FixtureProvenance`] for the threat model this does and does not address.
#[must_use]
pub fn trace_digest(trace: &ResearchTrace) -> String {
    let bytes = serde_json::to_vec(trace).unwrap_or_default();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// Read a fixture's provenance from the fixture itself.
///
/// An unstamped fixture is [`Authored`](CorpusProvenance::Authored) — the
/// conservative reading, and the one every fixture committed before the driver
/// existed gets without being edited. A stamped fixture is
/// [`DriverRecorded`](CorpusProvenance::DriverRecorded) only if its digest still
/// matches the trace it is attached to.
pub fn fixture_provenance(
    fixture: &PairedFixture,
) -> Result<CorpusProvenance, Box<ProvenanceDefect>> {
    let Some(stamp) = &fixture.provenance else {
        return Ok(CorpusProvenance::Authored);
    };
    let recomputed = trace_digest(&fixture.trace);
    if recomputed != stamp.trace_digest {
        return Err(Box::new(ProvenanceDefect::DigestMismatch {
            driver: stamp.driver.clone(),
            stamped: stamp.trace_digest.clone(),
            recomputed,
        }));
    }
    Ok(CorpusProvenance::DriverRecorded)
}

/// Reconcile what the corpus proves with what the caller declared.
///
/// The corpus is authoritative. A caller may always *downgrade* to
/// [`Authored`](CorpusProvenance::Authored) — declining a verdict is never
/// unsafe — but may never declare `DriverRecorded` over fixtures that carry no
/// stamp, which is the assertion §9.4's rule was found resting on.
pub fn reconcile_provenance(
    derived: CorpusProvenance,
    declared: Option<CorpusProvenance>,
) -> Result<CorpusProvenance, Box<ProvenanceDefect>> {
    match (derived, declared) {
        (_, None) => Ok(derived),
        (CorpusProvenance::DriverRecorded, Some(CorpusProvenance::Authored)) => {
            Ok(CorpusProvenance::Authored)
        }
        (CorpusProvenance::Authored, Some(CorpusProvenance::DriverRecorded)) => {
            Err(Box::new(ProvenanceDefect::Undeclared {
                declared: CorpusProvenance::DriverRecorded,
            }))
        }
        _ => Ok(derived),
    }
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

    /// §5.2's **Conditioned Abstention Rate**, derived from the same graded
    /// pairs — see [`ConditionedAbstention`] for what "conditioned" means here
    /// and for the overlap with Abstain Accuracy.
    #[must_use]
    pub fn conditioned_abstention(&self) -> ConditionedAbstention {
        // Under §5.1's binary taxonomy every decision is either acting or
        // abstaining, so "abstained on an Act half" is exactly the Act half the
        // policy got wrong. No new observation is required and none is invented.
        let abstained_when_unwarranted = self.act_total - self.act_correct;
        let abstained_when_warranted = self.abstain_correct;
        let rate = |num: usize, den: usize| (den > 0).then(|| num as f64 / den as f64);
        let rate_when_warranted = rate(abstained_when_warranted, self.abstain_total);
        let rate_when_unwarranted = rate(abstained_when_unwarranted, self.act_total);
        ConditionedAbstention {
            abstain_halves: self.abstain_total,
            abstained_when_warranted,
            act_halves: self.act_total,
            abstained_when_unwarranted,
            rate_when_warranted,
            rate_when_unwarranted,
            discrimination: rate_when_warranted
                .zip(rate_when_unwarranted)
                .map(|(w, u)| w - u),
        }
    }
}

/// §5.2's Conditioned Abstention Rate — the abstention rate **conditioned on
/// whether abstaining was the right call**, which is what the AgentAbstain
/// paired design makes available and an unconditional abstention rate does not.
///
/// # What this adds over §5.2's Act/Abstain accuracies, and what it does not
///
/// `rate_when_warranted` is **numerically identical to Abstain Accuracy** under
/// §5.1's taxonomy: abstaining on a labeled `Abstain` half *is* getting that
/// half right, so the two quantities count the same events. That is a property
/// of the adopted taxonomy rather than a defect, and it is stated here rather
/// than left for a reader to rediscover — a report that presented the two as
/// independent corroboration would be double-counting one measurement.
///
/// The metric's information content over the existing three is therefore the
/// **`rate_when_unwarranted` leg and the `discrimination` between them**: a
/// policy that abstains without reading the condition — the degenerate
/// always-`Wait` policy §5.1's pairing exists to catch — scores `1.0` on both
/// legs and `0.0` discrimination, while its Abstain Accuracy alone reads a
/// flattering `1.0`.
///
/// Every field is derived from the graded pairs at the replay boundary. Nothing
/// here reads an arm's posterior, so it is comparable across arms for the same
/// reason [`score_false_acceptances`] is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct ConditionedAbstention {
    /// Labeled `Abstain` halves — the condition under which abstaining is right.
    pub abstain_halves: usize,
    /// Of those, the ones where the policy did abstain.
    pub abstained_when_warranted: usize,
    /// Labeled `Act` halves — the condition under which abstaining is an error.
    pub act_halves: usize,
    /// Of those, the ones where the policy abstained anyway.
    pub abstained_when_unwarranted: usize,
    /// `abstained_when_warranted / abstain_halves`. `None` when the condition
    /// was never exercised — an unexercised rate is not a rate of zero.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_when_warranted: Option<f64>,
    /// `abstained_when_unwarranted / act_halves`. `None` when unexercised.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_when_unwarranted: Option<f64>,
    /// `rate_when_warranted − rate_when_unwarranted`. Zero for any policy whose
    /// abstention ignores the condition, however high its Abstain Accuracy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discrimination: Option<f64>,
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
#[must_use]
pub fn outcome_acted(actions: &[Action]) -> bool {
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

/// Arm-independent false-**acceptance** scoring (#35 §5.2).
///
/// # Why this exists beside `ReplayMetrics::false_acceptance_count`
///
/// The intrinsic counter carries the same defect the §5.3 amendment removed
/// from `false_rejection_count`, pointing the other way.
/// `accept_was_invalidated` has three routes to charging an `Accept`: a later
/// `Retraction`/`Correction` in the trace (arm-independent), the proposition
/// ending `Contradictory`, or a polarity flip. The last two read *the arm's
/// own* `final_beliefs`.
///
/// That is not a rounding concern. Measured across `fixtures/ix/`:
///
/// | arm | `false_acceptance_count` | charged via its own `Contradictory` |
/// |---|---|---|
/// | `RecencyDecay` | 12 | **5** |
/// | `Lie` | 8 | **2** |
/// | `SubjectiveLogic` | 3 | **0** |
///
/// SL is never charged that way because it resolves contradictions to a
/// probability. The hexavalent arms are charged because they **preserve** them
/// — which `hari-lattice` documents as a deliberate design choice, not a
/// failure to converge. So the intrinsic metric penalises the substrate's
/// defining feature, and does so only for the arms that have it.
///
/// This scorer takes authored [`ClaimLabel`]s instead: an `Accept` is a false
/// acceptance when the claim it committed to was `Withdrawn`, full stop. No
/// arm's posterior enters the verdict. **Use this for any cross-arm
/// comparison.**
///
/// # What this does not claim
///
/// It does **not** overturn the Phase-5 result. Removing the own-`Contradictory`
/// charges leaves `Lie` at 6 against SL's 3, so the direction survives on this
/// corpus — the margin is partly artifactual, the sign is not. A future
/// comparison should use this scorer; the existing verdict does not need
/// reopening on this basis alone.
#[must_use]
pub fn score_false_acceptances(
    report: &ResearchReplayReport,
    claims: &[ClaimLabel],
) -> FalseAcceptanceScore {
    let truth: BTreeMap<&str, ClaimOutcome> = claims
        .iter()
        .map(|c| (c.proposition.as_str(), c.outcome))
        .collect();

    let mut score = FalseAcceptanceScore::default();
    for outcome in &report.outcomes {
        for action in &outcome.actions {
            let Action::Accept { proposition, .. } = action else {
                continue;
            };
            match truth.get(proposition.as_str()) {
                Some(ClaimOutcome::Withdrawn) => {
                    score.scored += 1;
                    score.false_acceptances += 1;
                }
                Some(ClaimOutcome::Stood) => {
                    score.scored += 1;
                    score.warranted += 1;
                }
                Some(ClaimOutcome::Unresolved) => score.unresolved += 1,
                // Never silently forgiven, for the same reason a wait on an
                // unlabeled claim is not excused: missing ground truth is a
                // fixture defect, not a free pass.
                None => score.unlabeled.push(proposition.clone()),
            }
        }
    }
    score.unlabeled.sort();
    score.unlabeled.dedup();
    score
}

/// Result of [`score_false_acceptances`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FalseAcceptanceScore {
    /// Accepts that had authored ground truth and were graded.
    pub scored: usize,
    /// Graded accepts on claims that did **not** hold up.
    pub false_acceptances: usize,
    /// Graded accepts on claims that stood: committing was correct.
    pub warranted: usize,
    /// Accepts on claims explicitly labeled `Unresolved`.
    pub unresolved: usize,
    /// Accepted claims carrying no label.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unlabeled: Vec<String>,
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
    /// Stable identifier for this arm, independent of any `PriorityModel`.
    pub arm: String,
    /// The model this arm ran under, when it has one. `None` for
    /// `IX-unassisted`, which has no policy at all — labelling it with the
    /// nearest enum variant would repeat exactly the mislabel that
    /// `probe_every_arm_reports_the_model_that_produced_it` was written to
    /// catch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority_model: Option<PriorityModel>,
    /// §5.1 primary plus its §5.2 act/abstain decomposition.
    pub paired: PairedScore,
    /// §5.2's Conditioned Abstention Rate, derived from `paired`. Carried on
    /// the arm rather than left as a method so it reaches every JSON consumer
    /// of a paired run — a metric only computable in Rust is not a reported one.
    #[serde(default)]
    pub conditioned_abstention: ConditionedAbstention,
    /// §5.3 disqualifier input.
    pub false_rejections: FalseRejectionScore,
    /// §5.2 secondary, scored arm-independently — see
    /// [`score_false_acceptances`] for why the intrinsic counter cannot be
    /// compared across arms.
    pub false_acceptances: FalseAcceptanceScore,
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
    /// The §4 null baseline. `None` when the pair was ungradeable in that arm.
    pub unassisted: Option<bool>,
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
pub struct PairedComparison {
    /// The underlying three-arm replay, unmodified.
    pub replay: ThreeWayReplayReport,
    /// §4's null baseline — "does Hari do anything at all". §8's KEEP rule
    /// clause 1 is measured against this arm.
    pub unassisted: PairedArm,
    pub recency_decay: PairedArm,
    pub lie: PairedArm,
    pub subjective_logic: PairedArm,
    /// Pairs where the three arms did not all reach the same verdict. Empty
    /// means the fixture does not discriminate between the models at all,
    /// which for a §9.3 fixture is a failure of the fixture, not a result.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub separating_pairs: Vec<PairSeparation>,
}

impl PairedComparison {
    /// `true` when no pair separated the arms — every model graded identically.
    #[must_use]
    pub fn is_undiscriminating(&self) -> bool {
        self.separating_pairs.is_empty()
    }
}

/// Replay a [`PairedFixture`] under `RecencyDecay`, `Lie`, and Subjective
/// Logic, grading all three against the fixture's single label set.
#[must_use]
pub fn score_paired_all_arms(
    fixture: PairedFixture,
    sl_config: SubjectiveLogicConfig,
) -> PairedComparison {
    let PairedFixture {
        trace,
        labels,
        claims,
        // Provenance governs whether a §8 verdict may be emitted at all
        // (`fixture_provenance`); it is not an input to any grade.
        provenance: _,
    } = fixture;
    // The null baseline replays the same trace under no policy at all.
    let unassisted_report = crate::replay_unassisted(trace.clone());
    let replay = compare_replay_three_way(trace, sl_config);

    let grade = |arm: &str, model: Option<PriorityModel>, report: &ResearchReplayReport| {
        let paired = score_paired(report, &labels);
        PairedArm {
            arm: arm.to_string(),
            priority_model: model,
            conditioned_abstention: paired.conditioned_abstention(),
            paired,
            false_rejections: score_false_rejections(report, &claims),
            false_acceptances: score_false_acceptances(report, &claims),
        }
    };

    let unassisted = grade("ix_unassisted", None, &unassisted_report);
    let recency_decay = grade(
        "recency_decay",
        Some(replay.recency_decay.priority_model),
        &replay.recency_decay,
    );
    let lie = grade("lie", Some(replay.lie.priority_model), &replay.lie);
    let subjective_logic = grade(
        "subjective_logic",
        Some(replay.subjective_logic.priority_model),
        &replay.subjective_logic,
    );

    let separating_pairs = separations(&unassisted, &recency_decay, &lie, &subjective_logic);

    PairedComparison {
        replay,
        unassisted,
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
fn separations(
    unassisted: &PairedArm,
    decay: &PairedArm,
    lie: &PairedArm,
    sl: &PairedArm,
) -> Vec<PairSeparation> {
    let verdicts = |arm: &PairedArm| -> BTreeMap<String, bool> {
        arm.paired
            .per_pair
            .iter()
            .map(|p| (p.pair.clone(), p.both_correct()))
            .collect()
    };
    let (u, d, l, s) = (
        verdicts(unassisted),
        verdicts(decay),
        verdicts(lie),
        verdicts(sl),
    );

    let mut names: Vec<&String> = u
        .keys()
        .chain(d.keys())
        .chain(l.keys())
        .chain(s.keys())
        .collect();
    names.sort();
    names.dedup();

    names
        .into_iter()
        .filter_map(|pair| {
            let (uv, dv, lv, sv) = (
                u.get(pair).copied(),
                d.get(pair).copied(),
                l.get(pair).copied(),
                s.get(pair).copied(),
            );
            let all_agree = uv == dv && dv == lv && lv == sv;
            (!all_agree).then(|| PairSeparation {
                pair: pair.clone(),
                unassisted: uv,
                recency_decay: dv,
                lie: lv,
                subjective_logic: sv,
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// §6 — the trace-clustered paired bootstrap, and §8 applied to its output
// ---------------------------------------------------------------------------

/// One pair's contribution to the paired difference.
///
/// The estimator reads only `treatment_correct - baseline_correct`, so the two
/// arms' identities live on [`PairedBootstrap`] rather than here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairedDelta {
    /// The pair identifier, shared by both arms — the join key.
    pub pair: String,
    /// The treatment arm got **both** halves of this pair right (§5.1).
    pub treatment_correct: bool,
    /// The baseline arm got both halves right.
    pub baseline_correct: bool,
}

impl PairedDelta {
    /// `+1`, `0` or `-1` — the quantity §6 aggregates.
    #[must_use]
    pub fn delta(&self) -> f64 {
        f64::from(self.treatment_correct) - f64::from(self.baseline_correct)
    }
}

/// One trace's paired deltas — the **resampling unit** §6 pre-registers.
///
/// §6 resamples traces rather than decisions because ~20 decisions inside one
/// trace share a belief state; treating them as independent would inflate the
/// effective sample size by up to ~20× and manufacture significance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceCluster {
    /// Identifier for the trace this cluster came from.
    pub trace: String,
    /// [`PairedArm::arm`] of the arm whose `treatment_correct` column this
    /// cluster carries. Set by [`cluster_from_arms`] from the arm itself, never
    /// supplied by hand — see that function for why the binding is mechanical.
    pub treatment: String,
    /// [`PairedArm::arm`] of the arm whose `baseline_correct` column this
    /// cluster carries.
    pub baseline: String,
    /// Every pair gradeable in **both** arms.
    pub pairs: Vec<PairedDelta>,
    /// Pairs gradeable in exactly one arm. Excluded from the estimate and
    /// named, for the same reason [`PairDefect`] exists: a corpus that quietly
    /// aggregates a subset reads as if it aggregated everything.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unmatched: Vec<String>,
}

/// Join two arms' per-pair verdicts into one resampling unit.
///
/// [`theorem_gradeability_is_arm_independent`](self) asserts the two arms grade
/// the same pairs, so `unmatched` is expected to stay empty — which is exactly
/// why it is reported rather than assumed.
///
/// # Arm identity travels with the data
///
/// The two [`PairedArm::arm`] identifiers are copied onto the cluster. They are
/// the *only* source of the arm names on a published [`PairedBootstrap`]:
/// [`bootstrap_paired_difference`] derives them from the clusters and refuses a
/// corpus whose clusters disagree. Before that binding existed the estimator
/// re-accepted arm identity as two free `&str` at the call site, so transposing
/// two string literals published a sign-correct interval under swapped arm
/// names — the shipped substrate credited with the cheap baseline's advantage —
/// with the whole suite green. Every other number here is mechanically derived
/// from the graded pairs; the label that says *which arm won* now is too.
#[must_use]
pub fn cluster_from_arms(trace: &str, treatment: &PairedArm, baseline: &PairedArm) -> TraceCluster {
    let base: BTreeMap<&str, bool> = baseline
        .paired
        .per_pair
        .iter()
        .map(|p| (p.pair.as_str(), p.both_correct()))
        .collect();

    let mut pairs = Vec::new();
    let mut unmatched = Vec::new();
    for p in &treatment.paired.per_pair {
        match base.get(p.pair.as_str()) {
            Some(baseline_correct) => pairs.push(PairedDelta {
                pair: p.pair.clone(),
                treatment_correct: p.both_correct(),
                baseline_correct: *baseline_correct,
            }),
            None => unmatched.push(p.pair.clone()),
        }
    }
    let graded: std::collections::BTreeSet<&str> = treatment
        .paired
        .per_pair
        .iter()
        .map(|p| p.pair.as_str())
        .collect();
    for pair in base.keys() {
        if !graded.contains(pair) {
            unmatched.push((*pair).to_string());
        }
    }
    unmatched.sort();
    unmatched.dedup();

    TraceCluster {
        trace: trace.to_string(),
        treatment: treatment.arm.clone(),
        baseline: baseline.arm.clone(),
        pairs,
        unmatched,
    }
}

/// Why a corpus of [`TraceCluster`]s cannot be aggregated into one interval.
///
/// Each variant names a state in which the arm labels a published
/// [`PairedBootstrap`] would carry are not the arms its numbers came from.
/// Refusing is the only safe response: a mislabeled interval is worse than no
/// interval, because it reads as a result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArmBindingError {
    /// Two clusters were built from different arm pairs.
    Mismatch {
        trace: String,
        treatment: String,
        baseline: String,
        expected_treatment: String,
        expected_baseline: String,
    },
    /// A cluster carries an empty arm identifier, so nothing binds its column.
    Unlabeled { trace: String },
    /// Both columns name the same arm — the difference would be zero by
    /// construction and would read as two arms agreeing.
    SameArm { trace: String, arm: String },
}

impl std::fmt::Display for ArmBindingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Mismatch {
                trace,
                treatment,
                baseline,
                expected_treatment,
                expected_baseline,
            } => write!(
                f,
                "cluster `{trace}` was built from {treatment} vs {baseline}, but the corpus is \
                 {expected_treatment} vs {expected_baseline} — the arms a published interval \
                 would be labeled with are not the arms it was computed from"
            ),
            Self::Unlabeled { trace } => write!(
                f,
                "cluster `{trace}` carries an empty arm identifier, so its column is unattributable"
            ),
            Self::SameArm { trace, arm } => write!(
                f,
                "cluster `{trace}` names `{arm}` as both treatment and baseline"
            ),
        }
    }
}

impl std::error::Error for ArmBindingError {}

/// §6's pre-registered aggregation parameters.
///
/// **The seed is deliberately not settable from the command line.** §6 requires
/// a fixed seed so a re-run reproduces the interval exactly; a tunable seed
/// would also make the interval shoppable, which is the failure mode
/// pre-registration exists to prevent. It is a constant here and is echoed into
/// every [`PairedBootstrap`] so a reader can reproduce the number.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BootstrapConfig {
    /// §6: B = 10,000.
    pub resamples: usize,
    /// The fixed seed §6 requires. Defaults to the pre-registration's date.
    pub seed: u64,
    /// §6's dual rule is written for a 95% interval.
    pub alpha: f64,
}

impl Default for BootstrapConfig {
    fn default() -> Self {
        Self {
            resamples: 10_000,
            seed: 20_260_728,
            alpha: 0.05,
        }
    }
}

/// The §6 aggregator's output, carrying everything §7 and §8 need to read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PairedBootstrap {
    /// The arm whose improvement is being tested.
    pub treatment: String,
    /// The arm it is tested against.
    pub baseline: String,
    /// Resampling units (§6 clusters by trace).
    pub clusters: usize,
    /// Pairs gradeable in both arms.
    pub pairs: usize,
    /// The treatment arm's Paired Accuracy over `pairs`.
    pub treatment_accuracy: f64,
    /// The baseline arm's Paired Accuracy over the same pairs.
    pub baseline_accuracy: f64,
    /// Point estimate of the paired difference (`treatment - baseline`).
    pub difference: f64,
    /// Percentile lower bound at `alpha`.
    pub ci_low: f64,
    /// Percentile upper bound at `alpha`.
    pub ci_high: f64,
    /// Two-sided bootstrap achieved significance level (see module notes).
    pub p_value: f64,
    /// Echo of [`BootstrapConfig::resamples`].
    pub resamples: usize,
    /// Echo of [`BootstrapConfig::seed`] — §6 requires it in the report.
    pub seed: u64,
    /// Echo of [`BootstrapConfig::alpha`].
    pub alpha: f64,
    /// §6 dual rule, first half.
    pub ci_excludes_zero: bool,
    /// §6 dual rule, second half.
    pub p_below_alpha: bool,
    /// Both halves — the only thing §8 clause 1 reads.
    pub dual_rule_passes: bool,
    /// §7's realized intra-cluster correlation. `None` when it is undefined
    /// (fewer than two clusters, or a totally degenerate variance decomposition).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icc: Option<f64>,
    /// §7's design effect, `1 + (m̄ − 1)·ICC`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub design_effect: Option<f64>,
    /// §7's achieved effective *n*, which the report must state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective_n: Option<f64>,
    /// `true` when `effective_n` must not be read as what the corpus supports.
    ///
    /// The ICC design-effect correction is a correction for **between**-trace
    /// correlation only. A corpus of clones has *no* between-trace variance, so
    /// ICC comes out at 0, the design effect at 1, and the effective *n* equal
    /// to the raw pair count — while the corpus in fact holds a handful of
    /// distinct decision situations replicated many times. That is §9.4's
    /// finding, and the arithmetic cannot see it, so it is flagged instead of
    /// left to a reader to notice.
    pub effective_n_overstates: bool,
    /// How many distinct delta sequences the clusters hold. `1` means the
    /// corpus is clones of one trace.
    pub distinct_cluster_signatures: usize,
    /// §9.4's condition: no between-cluster variance, so the interval is a
    /// property of whoever wrote the corpus rather than of a population.
    pub zero_between_cluster_variance: bool,
    /// Every single decision agreed. Distinct from "not significant": the arms
    /// are identical on this corpus, not merely indistinguishable on it.
    pub every_delta_is_zero: bool,
    /// `trace/pair` for every pair gradeable in exactly one arm.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unmatched: Vec<String>,
}

/// Deterministic PRNG (SplitMix64). Vendored rather than taken from `rand` so
/// the interval is reproducible across dependency upgrades — §6 requires a
/// re-run to reproduce it *exactly*, which a crate-versioned generator does not
/// guarantee.
struct SplitMix64(u64);

impl SplitMix64 {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform-ish index into `0..n`. The modulo bias is bounded by `n / 2^64`
    /// and is irrelevant at these sizes; determinism is what matters here.
    fn index(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
}

/// §6's aggregator: a **paired bootstrap clustered by trace**.
///
/// Pure — decision-outcome records in, CI and p out — so it is testable on
/// hand-built inputs with no replay engine in the loop, which is the property
/// §6 asks for.
///
/// # What it computes
///
/// The statistic is the difference in Paired Accuracy, `mean(delta)` over every
/// gradeable pair. Each of `B` resamples draws `clusters.len()` **traces** with
/// replacement and recomputes that mean over the drawn traces' pairs — so a
/// trace contributing more pairs carries more weight, exactly as it does in the
/// point estimate.
///
/// The interval is the percentile one at `alpha`. The p-value is the two-sided
/// achieved significance level
/// `2·min(Pr[θ* ≤ 0], Pr[θ* ≥ 0])`, each tail computed as `(count + 1)/(B + 1)`
/// and the whole clamped to `1.0`. The `+1` is the standard finite-`B`
/// correction; without it a p-value of exactly zero is reportable, which no
/// finite resampling can justify.
///
/// # Arm identity is derived, never supplied
///
/// The `treatment`/`baseline` labels on the result come from the clusters — see
/// [`cluster_from_arms`] — and a corpus whose clusters disagree is refused
/// rather than aggregated under one of the two candidate labelings.
///
/// # Returns
///
/// `Ok(None)` when no pair is gradeable in both arms. An empty corpus has no
/// interval — which is not an interval of zero width at zero, and is not the
/// same thing as a corpus that could not be labeled.
pub fn bootstrap_paired_difference(
    clusters: &[TraceCluster],
    config: BootstrapConfig,
) -> Result<Option<PairedBootstrap>, ArmBindingError> {
    let usable: Vec<&TraceCluster> = clusters.iter().filter(|c| !c.pairs.is_empty()).collect();
    let total_pairs: usize = usable.iter().map(|c| c.pairs.len()).sum();
    if usable.is_empty() || total_pairs == 0 {
        return Ok(None);
    }
    let (treatment, baseline) = bind_arms(&usable)?;
    let (treatment, baseline) = (treatment.as_str(), baseline.as_str());

    let deltas: Vec<f64> = usable
        .iter()
        .flat_map(|c| c.pairs.iter().map(PairedDelta::delta))
        .collect();
    let difference = deltas.iter().sum::<f64>() / total_pairs as f64;
    let treatment_hits: usize = usable
        .iter()
        .flat_map(|c| c.pairs.iter())
        .filter(|p| p.treatment_correct)
        .count();
    let baseline_hits: usize = usable
        .iter()
        .flat_map(|c| c.pairs.iter())
        .filter(|p| p.baseline_correct)
        .count();

    // Resample traces, never decisions.
    let mut rng = SplitMix64(config.seed);
    let k = usable.len();
    let mut stats: Vec<f64> = Vec::with_capacity(config.resamples);
    for _ in 0..config.resamples {
        let mut sum = 0.0;
        let mut n = 0usize;
        for _ in 0..k {
            let c = usable[rng.index(k)];
            for p in &c.pairs {
                sum += p.delta();
                n += 1;
            }
        }
        stats.push(if n == 0 { 0.0 } else { sum / n as f64 });
    }
    stats.sort_by(|a, b| a.partial_cmp(b).expect("bootstrap statistics are finite"));

    let b = stats.len();
    let lo_idx = ((config.alpha / 2.0) * b as f64).floor() as usize;
    let hi_idx = (((1.0 - config.alpha / 2.0) * b as f64).ceil() as usize).saturating_sub(1);
    let ci_low = stats[lo_idx.min(b - 1)];
    let ci_high = stats[hi_idx.min(b - 1)];

    let le = stats.iter().filter(|s| **s <= 0.0).count();
    let ge = stats.iter().filter(|s| **s >= 0.0).count();
    let tail = |count: usize| (count as f64 + 1.0) / (b as f64 + 1.0);
    let p_value = (2.0 * tail(le).min(tail(ge))).min(1.0);

    let ci_excludes_zero = (ci_low > 0.0 && ci_high > 0.0) || (ci_low < 0.0 && ci_high < 0.0);
    let p_below_alpha = p_value < config.alpha;

    let (icc, design_effect, effective_n) = cluster_correlation(&usable, difference);

    let signatures: std::collections::BTreeSet<Vec<i8>> = usable
        .iter()
        .map(|c| c.pairs.iter().map(|p| p.delta() as i8).collect())
        .collect();
    let degenerate = every_cluster_has_the_same_mean(&usable);

    let mut unmatched: Vec<String> = clusters
        .iter()
        .flat_map(|c| c.unmatched.iter().map(|p| format!("{}/{p}", c.trace)))
        .collect();
    unmatched.sort();

    Ok(Some(PairedBootstrap {
        treatment: treatment.to_string(),
        baseline: baseline.to_string(),
        clusters: k,
        pairs: total_pairs,
        treatment_accuracy: treatment_hits as f64 / total_pairs as f64,
        baseline_accuracy: baseline_hits as f64 / total_pairs as f64,
        difference,
        ci_low,
        ci_high,
        p_value,
        resamples: config.resamples,
        seed: config.seed,
        alpha: config.alpha,
        ci_excludes_zero,
        p_below_alpha,
        dual_rule_passes: ci_excludes_zero && p_below_alpha,
        icc,
        design_effect,
        effective_n,
        effective_n_overstates: signatures.len() <= 1,
        distinct_cluster_signatures: signatures.len(),
        zero_between_cluster_variance: degenerate,
        every_delta_is_zero: deltas.iter().all(|d| *d == 0.0),
        unmatched,
    }))
}

/// Derive the corpus's arm pair from its clusters, refusing anything that would
/// publish an interval under labels its numbers did not come from.
fn bind_arms(usable: &[&TraceCluster]) -> Result<(String, String), ArmBindingError> {
    let first = usable[0];
    for c in usable {
        if c.treatment.is_empty() || c.baseline.is_empty() {
            return Err(ArmBindingError::Unlabeled {
                trace: c.trace.clone(),
            });
        }
        if c.treatment == c.baseline {
            return Err(ArmBindingError::SameArm {
                trace: c.trace.clone(),
                arm: c.treatment.clone(),
            });
        }
        if c.treatment != first.treatment || c.baseline != first.baseline {
            return Err(ArmBindingError::Mismatch {
                trace: c.trace.clone(),
                treatment: c.treatment.clone(),
                baseline: c.baseline.clone(),
                expected_treatment: first.treatment.clone(),
                expected_baseline: first.baseline.clone(),
            });
        }
    }
    Ok((first.treatment.clone(), first.baseline.clone()))
}

/// §9.4's degeneracy condition: **no between-cluster variance**, so every
/// resample returns the same statistic and the interval's width is a property
/// of the corpus's design rather than of any sampling process.
///
/// Computed from equality of cluster *means*, which is what "no between-cluster
/// variance" actually says. An earlier cut tested identity of the whole delta
/// *sequence*, which is strictly stronger: two clusters with deltas `[+1, −1]`
/// and `[−1, +1]` have distinct signatures, equal means, zero between-cluster
/// variance and a zero-width interval — and would have been reported as **not**
/// degenerate. A false negative in a degeneracy detector is the direction that
/// matters, because [`apply_kill_keep`] refuses to read a dual-rule pass on a
/// degenerate corpus as a pass.
///
/// [`PairedBootstrap::distinct_cluster_signatures`] and
/// `effective_n_overstates` keep the stricter signature test: they answer §7's
/// "is this corpus clones of one trace", which is a different question.
fn every_cluster_has_the_same_mean(usable: &[&TraceCluster]) -> bool {
    // Deltas are ±1 and 0, so a cluster mean is a ratio of small integers and
    // is not generally exact in IEEE-754. Compare the cross-products instead,
    // which are sums of ±1 over at most a few thousand pairs and so are exact.
    let scaled = |c: &TraceCluster| {
        let sum: f64 = c.pairs.iter().map(PairedDelta::delta).sum();
        (sum, c.pairs.len() as f64)
    };
    let (s0, n0) = scaled(usable[0]);
    usable.iter().all(|c| {
        let (s, n) = scaled(c);
        // s/n == s0/n0 without dividing: both sides are sums of ±1 over ≤ a few
        // thousand pairs, so the cross-products are exact in f64.
        (s * n0 - s0 * n).abs() < f64::EPSILON
    })
}

/// §7's ICC, design effect and effective *n*, by one-way ANOVA on the per-pair
/// deltas. Returns `None`s when the decomposition is undefined — fewer than two
/// clusters, or no variance at all to attribute.
fn cluster_correlation(
    clusters: &[&TraceCluster],
    grand_mean: f64,
) -> (Option<f64>, Option<f64>, Option<f64>) {
    let k = clusters.len();
    let n: usize = clusters.iter().map(|c| c.pairs.len()).sum();
    if k < 2 || n <= k {
        return (None, None, None);
    }

    let mut ms_between = 0.0;
    let mut ss_within = 0.0;
    for c in clusters {
        let m = c.pairs.len() as f64;
        let mean = c.pairs.iter().map(PairedDelta::delta).sum::<f64>() / m;
        ms_between += m * (mean - grand_mean).powi(2);
        ss_within += c
            .pairs
            .iter()
            .map(|p| (p.delta() - mean).powi(2))
            .sum::<f64>();
    }
    ms_between /= (k - 1) as f64;
    let ms_within = ss_within / (n - k) as f64;

    let sum_sq: f64 = clusters
        .iter()
        .map(|c| (c.pairs.len() as f64).powi(2))
        .sum();
    let m0 = (n as f64 - sum_sq / n as f64) / (k - 1) as f64;
    let denominator = ms_between + (m0 - 1.0) * ms_within;
    if denominator <= 0.0 {
        return (None, None, None);
    }

    let icc = ((ms_between - ms_within) / denominator).clamp(0.0, 1.0);
    let mean_size = n as f64 / k as f64;
    let design_effect = 1.0 + (mean_size - 1.0) * icc;
    let effective_n = n as f64 / design_effect;
    (Some(icc), Some(design_effect), Some(effective_n))
}

/// §9.3.2's non-pooling rule, checkable.
///
/// The `*-isolation` and `*-task` corpora measure different things — one holds
/// the asserted `HexValue` fixed and varies corroboration alone, the other
/// bundles asserted confidence with reproduction per §2's injected trigger —
/// and §9.3.2 states plainly that **the two are never pooled**. Pooling them
/// would average `SubjectiveLogic`'s degenerate always-`Wait` zero on isolation
/// into its task result and launder it into one number.
///
/// Returns the first conflicting pair of trace ids, so a caller can name them.
#[must_use]
pub fn pooling_violation(traces: &[&str]) -> Option<(String, String)> {
    let isolation = traces.iter().find(|t| t.ends_with("-isolation"))?;
    let task = traces.iter().find(|t| t.ends_with("-task"))?;
    Some(((*isolation).to_string(), (*task).to_string()))
}

/// The suffixes §9.3 declares a corpus by. A trace id ending in neither belongs
/// to no declared corpus, and [`check_corpus`] refuses it.
pub const DECLARED_CORPUS_SUFFIXES: [&str; 2] = ["-isolation", "-task"];

/// Why a set of trace ids is not a corpus §6 may aggregate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CorpusDefect {
    /// §9.3.2's rule: the isolation and task corpora are never pooled.
    Pooled { isolation: String, task: String },
    /// A trace id belonging to neither declared corpus.
    UndeclaredCorpus { trace: String },
    /// The same trace id twice — one trace counted as two resampling units.
    DuplicateTrace { trace: String },
}

impl std::fmt::Display for CorpusDefect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pooled { isolation, task } => write!(
                f,
                "#35 §9.3.2 forbids pooling the isolation and task corpora — got both \
                 `{isolation}` and `{task}`. Run them separately."
            ),
            Self::UndeclaredCorpus { trace } => write!(
                f,
                "`{trace}` belongs to no corpus §9.3 declares (expected a name ending {}). A \
                 fixture the non-pooling rule does not recognise would join any corpus it is \
                 handed to and be published as one of its traces.",
                DECLARED_CORPUS_SUFFIXES.join(" or ")
            ),
            Self::DuplicateTrace { trace } => write!(
                f,
                "`{trace}` appears twice — the same trace is not two independent resampling \
                 units, and `clusters` would be published as a fact about the corpus."
            ),
        }
    }
}

impl std::error::Error for CorpusDefect {}

/// §9.3.2's rule, plus the two ways around it that the suffix match alone left
/// open (#35 review S8).
///
/// [`pooling_violation`] refuses a run only when some id ends `-isolation`
/// **and** some id ends `-task`. Two corpora that are not that mixture still get
/// through:
///
/// * **A fixture belonging to neither.** `propositionless_abstention` — a
///   committed paired fixture that is deliberately weak — pooled freely into the
///   task corpus and made a corpus §9.4 calls degenerate *look* non-degenerate,
///   which is the exact laundering §9.3.2 exists to prevent, achieved with a
///   filename the rule did not recognise.
/// * **The same fixture twice.** Repeating a path yielded `clusters: 3` from one
///   trace. `distinct_cluster_signatures` flags it, but the cluster count is
///   published as a fact about the corpus.
///
/// So membership is checked positively — every id must name a declared corpus —
/// and ids must be distinct. Refusing is the conservative direction: a corpus
/// this rejects can always be renamed or split, whereas a laundered interval is
/// already published by the time anyone notices.
pub fn check_corpus(traces: &[&str]) -> Result<(), CorpusDefect> {
    if let Some(trace) = traces
        .iter()
        .find(|t| !DECLARED_CORPUS_SUFFIXES.iter().any(|s| t.ends_with(s)))
    {
        return Err(CorpusDefect::UndeclaredCorpus {
            trace: (*trace).to_string(),
        });
    }
    if let Some((isolation, task)) = pooling_violation(traces) {
        return Err(CorpusDefect::Pooled { isolation, task });
    }
    let mut seen = std::collections::BTreeSet::new();
    for t in traces {
        if !seen.insert(*t) {
            return Err(CorpusDefect::DuplicateTrace {
                trace: (*t).to_string(),
            });
        }
    }
    Ok(())
}

/// Where a corpus came from. §9.4 adopted a **standing rule** that no §6 dual-rule
/// test and no §8 verdict may ever be computed on an authored fixture, because
/// hand-authored traces are not draws from any population and the resulting
/// interval is a property of their author. Encoding it here means the rule is
/// enforced rather than remembered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CorpusProvenance {
    /// Hand-authored fixtures — instrument characterisation only.
    Authored,
    /// Recorded by the §9 item 4 driver from an IX session.
    DriverRecorded,
}

/// One §8 clause's state. `Undefined` is distinct from `Fails`: it means the
/// instrument the clause reads does not exist yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClauseStatus {
    Passes,
    Fails,
    /// Not measurable — reported as such, never silently treated as a pass.
    Undefined,
}

/// §8's outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KillKeepVerdict {
    /// Both clauses hold on a recorded corpus.
    Keep,
    /// Either clause fails or is undefined.
    Kill,
    /// §9.4's standing rule: the corpus is authored, so no verdict is emitted.
    WithheldByStandingRule,
}

/// §8 clause 2's input: mean Brier for the experimental arm and for
/// `SubjectiveLogic`. Lower is better, so "does not lose" is `experimental ≤ sl`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CalibrationMargin {
    pub experimental_mean_brier: f64,
    pub subjective_logic_mean_brier: f64,
}

/// §8, applied mechanically.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KillKeepDecision {
    pub verdict: KillKeepVerdict,
    /// experimental beats `IX-unassisted` on Paired Accuracy under §6's dual rule.
    pub clause1: ClauseStatus,
    /// experimental does not lose to `SubjectiveLogic` on calibration.
    pub clause2: ClauseStatus,
    pub provenance: CorpusProvenance,
    /// Why each clause landed where it did, and — when withheld — which rule
    /// withheld it. Narrative in the report must follow this, not lead it.
    pub rationale: Vec<String>,
}

/// Apply §8's kill/keep rule to the §6 output.
///
/// **KEEP only if both** clauses pass; **KILL** if either fails. An `Undefined`
/// clause cannot satisfy KEEP — clause 2 is undefined today because per-arm
/// calibration requires each arm to emit forecasts from its own posterior
/// (§9 item 2), which does not exist.
///
/// On an [`Authored`](CorpusProvenance::Authored) corpus the clauses are still
/// reported and the verdict is **withheld** per §9.4.
///
/// # Clause 1 does not read `dual_rule_passes` alone
///
/// A corpus with no between-cluster variance clears the §6 dual rule
/// **automatically** whenever the point estimate is non-zero: every resample
/// returns the same statistic, so the interval has zero width, excludes zero,
/// and `p = 2/(B+1) < α` unconditionally. That is not a measured effect; it is
/// the corpus's design. Clause 1 is therefore reported `Undefined` — not
/// `Passes` — in that state, and a KEEP cannot be reached through it.
///
/// This matters because it is the one check §9.4's standing rule does *not*
/// already cover. Provenance is asserted by the caller, so a corpus mis-declared
/// `DriverRecorded` would otherwise convert a design artifact into a mechanical
/// clause-1 pass. A **failing** dual rule is left as `Fails`: degeneracy can
/// manufacture a pass, never a failure.
#[must_use]
pub fn apply_kill_keep(
    clause1_bootstrap: Option<&PairedBootstrap>,
    calibration: Option<CalibrationMargin>,
    provenance: CorpusProvenance,
) -> KillKeepDecision {
    let mut rationale = Vec::new();

    let clause1 = match clause1_bootstrap {
        None => {
            rationale.push(
                "§8 clause 1: no bootstrap output — nothing was gradeable in both arms."
                    .to_string(),
            );
            ClauseStatus::Undefined
        }
        // Checked before the pass branch: a degenerate corpus clears the dual
        // rule by construction, so reading that as a pass would let the corpus's
        // design decide §8.
        Some(b) if b.dual_rule_passes && b.zero_between_cluster_variance => {
            rationale.push(format!(
                "§8 clause 1: UNDEFINED — {} vs {} clears the §6 dual rule ({:.4}, 95% CI \
                 [{:.4}, {:.4}], p = {:.4}) on a corpus with **no between-cluster variance** \
                 ({} distinct delta sequences over {} traces). Every resample returns the same \
                 statistic, so the interval has zero width and clears the rule whatever the \
                 point estimate is. §9.4: that interval is a property of the corpus's design, \
                 not a sampling one, so it measures nothing clause 1 can read.",
                b.treatment,
                b.baseline,
                b.difference,
                b.ci_low,
                b.ci_high,
                b.p_value,
                b.distinct_cluster_signatures,
                b.clusters
            ));
            ClauseStatus::Undefined
        }
        Some(b) if b.dual_rule_passes => {
            rationale.push(format!(
                "§8 clause 1: {} beats {} by {:.4} (95% CI [{:.4}, {:.4}], p = {:.4}) — §6 dual rule holds.",
                b.treatment, b.baseline, b.difference, b.ci_low, b.ci_high, b.p_value
            ));
            ClauseStatus::Passes
        }
        Some(b) => {
            let how = if b.every_delta_is_zero {
                " — the two arms agree at every single decision, so the difference is identically zero rather than merely insignificant"
            } else {
                ""
            };
            rationale.push(format!(
                "§8 clause 1: {} vs {} is {:.4} (95% CI [{:.4}, {:.4}], p = {:.4}) — §6 dual rule fails{how}.",
                b.treatment, b.baseline, b.difference, b.ci_low, b.ci_high, b.p_value
            ));
            ClauseStatus::Fails
        }
    };

    let clause2 = match calibration {
        None => {
            rationale.push(
                "§8 clause 2: undefined — per-arm calibration requires each arm to emit \
                 forecasts from its own posterior (§9 item 2), which does not exist."
                    .to_string(),
            );
            ClauseStatus::Undefined
        }
        Some(m) if m.experimental_mean_brier <= m.subjective_logic_mean_brier => {
            rationale.push(format!(
                "§8 clause 2: experimental mean Brier {:.4} ≤ SubjectiveLogic {:.4} — does not lose.",
                m.experimental_mean_brier, m.subjective_logic_mean_brier
            ));
            ClauseStatus::Passes
        }
        Some(m) => {
            rationale.push(format!(
                "§8 clause 2: experimental mean Brier {:.4} > SubjectiveLogic {:.4} — loses on calibration.",
                m.experimental_mean_brier, m.subjective_logic_mean_brier
            ));
            ClauseStatus::Fails
        }
    };

    let verdict = match provenance {
        CorpusProvenance::Authored => {
            rationale.push(
                "Verdict WITHHELD: §9.4's standing rule bars any §6 dual-rule test and any §8 \
                 verdict on an authored fixture corpus — hand-authored traces are not draws \
                 from a population, so the interval is a property of their author. The clauses \
                 above are instrument characterisation."
                    .to_string(),
            );
            KillKeepVerdict::WithheldByStandingRule
        }
        CorpusProvenance::DriverRecorded => {
            if clause1 == ClauseStatus::Passes && clause2 == ClauseStatus::Passes {
                KillKeepVerdict::Keep
            } else {
                rationale.push(
                    "Verdict KILL: §8 keeps the substrate only if both clauses pass; an \
                     undefined clause cannot satisfy KEEP."
                        .to_string(),
                );
                KillKeepVerdict::Kill
            }
        }
    };

    KillKeepDecision {
        verdict,
        clause1,
        clause2,
        provenance,
        rationale,
    }
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

    /// The false-acceptance verdict must not depend on the arm's own posterior
    /// — the defect the §5.3 amendment removed from `false_rejection_count`,
    /// which the intrinsic `false_acceptance_count` still carries.
    #[test]
    fn the_false_acceptance_verdict_does_not_depend_on_the_arms_own_beliefs() {
        let mut report = replay(act_then_abstain_trace());
        let claims = vec![ClaimLabel {
            proposition: "benchmark-x-is-reliable".to_string(),
            outcome: ClaimOutcome::Withdrawn,
        }];
        let before = score_false_acceptances(&report, &claims);
        assert_eq!(
            before.false_acceptances, 1,
            "precondition: the accept is charged"
        );

        // Drive the arm's own belief to every value in turn, including the two
        // the intrinsic counter reads. The verdict must not move.
        for value in [
            HexValue::True,
            HexValue::Probable,
            HexValue::Unknown,
            HexValue::Doubtful,
            HexValue::False,
            HexValue::Contradictory,
        ] {
            report
                .final_beliefs
                .insert("benchmark-x-is-reliable".to_string(), value);
            assert_eq!(
                score_false_acceptances(&report, &claims),
                before,
                "verdict moved when the arm's own belief became {value:?}"
            );
        }
    }

    /// An accepted claim with no authored label is reported, never forgiven.
    #[test]
    fn an_unlabeled_accept_is_reported_not_forgiven() {
        let report = replay(act_then_abstain_trace());
        let score = score_false_acceptances(&report, &[]);
        assert_eq!((score.scored, score.false_acceptances), (0, 0));
        assert_eq!(score.unlabeled, vec!["benchmark-x-is-reliable".to_string()]);
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
            provenance: None,
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
        let graded = score_paired_all_arms(paired_fixture(), SubjectiveLogicConfig::default());
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
        let graded = score_paired_all_arms(paired_fixture(), SubjectiveLogicConfig::default());
        assert_eq!(
            graded.recency_decay.priority_model,
            Some(PriorityModel::RecencyDecay)
        );
        assert_eq!(graded.lie.priority_model, Some(PriorityModel::Lie));
        assert_eq!(
            graded.subjective_logic.priority_model,
            Some(PriorityModel::SubjectiveLogic)
        );
        // The null baseline has no policy, so it must carry no model label.
        assert_eq!(graded.unassisted.priority_model, None);
        assert_eq!(graded.unassisted.arm, "ix_unassisted");
    }

    /// `per_pair` must reconstruct the aggregate exactly. They are computed in
    /// the same pass, so a mismatch means one of them was edited without the
    /// other — and the per-pair list is what §9.3 reads to tell an intended
    /// null result from a broken fixture.
    #[test]
    fn theorem_per_pair_reconstructs_the_aggregate() {
        let graded = score_paired_all_arms(paired_fixture(), SubjectiveLogicConfig::default());
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
        let graded = score_paired_all_arms(paired_fixture(), SubjectiveLogicConfig::default());

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
        let graded = score_paired_all_arms(paired_fixture(), SubjectiveLogicConfig::default());
        assert!(!graded.is_undiscriminating());
        assert_eq!(
            graded.separating_pairs,
            vec![PairSeparation {
                pair: "p1".to_string(),
                unassisted: Some(true),
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
        let graded = score_paired_all_arms(fixture, SubjectiveLogicConfig::default());

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
        let graded = score_paired_all_arms(fixture, SubjectiveLogicConfig::default());

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
