//! The `IX-unassisted` null baseline (#35 §4).
//!
//! §4 lists `IX-unassisted` as an arm — *"recorded IX behavior with no Hari
//! policy applied"*, the "does Hari do anything" comparison — and §8's **KEEP**
//! rule opens with *"experimental beats `IX-unassisted` on Paired Accuracy"*.
//! Until this module existed the arm had no definition and no implementation
//! anywhere in the workspace, so the first clause of the kill/keep rule was not
//! computable. §9's prerequisite list never named it.
//!
//! # What "no policy" means, and why it is not a strawman
//!
//! IX emits claims with an asserted [`HexValue`]. A policy layer is what decides
//! whether to *act* on such a claim, withhold, or escalate. Remove the policy
//! layer and what remains is **pass-through**: take every report at face value
//! and proceed. So this arm accepts every proposition-bearing assertion and
//! takes no substantive action on anything else.
//!
//! The temptation is to make this baseline weaker — to have it thrash, or
//! accept and reject at random — because a weak baseline is easy to beat. That
//! would be instrument-driven design of exactly the kind §10 exists to prevent,
//! applied to the comparator instead of the metric. Pass-through is the
//! strongest honest reading of "no policy", and if the substrate cannot beat it
//! then §8's answer is that the substrate does not help, which is a result the
//! project has already committed to publishing once.
//!
//! # The measured consequence
//!
//! Under §5.1's taxonomy `Accept` is acting, and the hexavalent arms act on
//! every proposition-bearing event too. The prediction written here first was
//! that this arm would be **decision-identical** to both `RecencyDecay` and
//! `Lie`. The test written to pin that **failed on its first run**, and the
//! true relationship is sharper:
//!
//! * **On every claim assertion, `RecencyDecay` acts exactly where this arm
//!   acts** — zero divergences across the corpus. `HexValue` selects *which*
//!   action, never *whether*. So §8 clause 1 is structurally zero for the
//!   shipped default: the policy layer changes what Hari commits to, never
//!   whether it commits. That is a finding about the substrate, not a defect in
//!   the baseline.
//! * `RecencyDecay`'s only divergence is on `retraction` (5 across the corpus),
//!   where it emits `Retry` and this arm emits nothing — a definitional choice
//!   below, disclosed rather than tuned away.
//! * `Lie` **does** withhold on 18 claim assertions, all via cycle-age decay
//!   rather than evidence. `Lie` is not a §4 arm, so no verdict moves; it is
//!   what makes the first bullet a measurement rather than a tautology.
//!
//! Pinned by `theorem_the_default_arm_never_withholds_where_the_null_baseline_commits`.

use std::collections::BTreeMap;

use hari_lattice::HexValue;

use crate::{
    compute_metrics_for, Action, Goal, PriorityModel, ResearchEvent, ResearchEventOutcome,
    ResearchEventPayload, ResearchReplayReport, ResearchTrace,
};

/// Replay a trace with no Hari policy applied: accept every claim assertion,
/// decide nothing else.
///
/// Produces a [`ResearchReplayReport`] structurally identical to what the other
/// arms emit, so [`score_paired`](crate::score_paired) grades it unchanged.
#[must_use]
pub fn replay_unassisted(trace: ResearchTrace) -> ResearchReplayReport {
    let mut beliefs: BTreeMap<String, HexValue> = BTreeMap::new();
    let mut goals: BTreeMap<String, Goal> = BTreeMap::new();
    let mut outcomes: Vec<ResearchEventOutcome> = Vec::with_capacity(trace.events.len());

    for event in trace.events {
        let actions = unassisted_actions(&event, &mut beliefs, &mut goals);
        let state_summary = format!(
            "[ix-unassisted] {} propositions, {} goals",
            beliefs.len(),
            goals.len()
        );
        outcomes.push(ResearchEventOutcome {
            event,
            actions,
            state_summary,
            derivations: Vec::new(),
            revisions: Vec::new(),
        });
    }

    let final_state_summary = format!(
        "[ix-unassisted] {} propositions accepted at face value, {} goals recorded",
        beliefs.len(),
        goals.len()
    );
    let metrics = compute_metrics_for(&outcomes, &beliefs, &goals, 0.0);

    ResearchReplayReport {
        event_count: outcomes.len(),
        outcomes,
        final_beliefs: beliefs,
        final_goals: goals,
        final_state_summary,
        // Not a `PriorityModel` — this arm has no policy at all. `Flat` is the
        // nearest existing label and would be a lie of the same kind the SL
        // report told before `probe_every_arm_reports_the_model_that_produced_it`
        // caught it, so the arm is identified by `final_state_summary` and by
        // the field name it occupies in `PairedComparison`. Left at the default
        // deliberately, and never read as this arm's identity.
        priority_model: PriorityModel::default(),
        metrics,
        comparison: None,
        revisions: Vec::new(),
        calibration: None,
    }
}

/// One event's pass-through response.
fn unassisted_actions(
    event: &ResearchEvent,
    beliefs: &mut BTreeMap<String, HexValue>,
    goals: &mut BTreeMap<String, Goal>,
) -> Vec<Action> {
    match &event.payload {
        // A claim arrives with an asserted value. With no policy layer, it is
        // taken at face value.
        ResearchEventPayload::BeliefUpdate {
            proposition, value, ..
        }
        | ResearchEventPayload::ExperimentResult {
            proposition, value, ..
        }
        | ResearchEventPayload::AgentVote {
            proposition, value, ..
        }
        | ResearchEventPayload::Correction {
            proposition, value, ..
        } => {
            beliefs.insert(proposition.clone(), *value);
            vec![Action::Accept {
                proposition: proposition.clone(),
                value: *value,
            }]
        }

        // A withdrawal is an instruction, not a claim to decide about: there is
        // no Accept/Wait/Escalate to take. State is updated and nothing is
        // emitted. Note this means the arm never emits `Wait`, so it can never
        // be charged a §5.3 false rejection — a structural property of having no
        // policy, and one the disqualifier should be read against accordingly.
        ResearchEventPayload::Retraction { proposition, .. } => {
            beliefs.insert(proposition.clone(), HexValue::Unknown);
            Vec::new()
        }
        ResearchEventPayload::Supersession { proposition, .. } => {
            beliefs.insert(proposition.clone(), HexValue::Unknown);
            Vec::new()
        }

        // Record what we are told; decide nothing.
        ResearchEventPayload::GoalUpdate {
            key,
            description,
            priority,
            status,
        } => {
            goals.insert(
                key.clone(),
                Goal {
                    description: description.clone(),
                    priority: *priority,
                    status: status.unwrap_or(HexValue::Unknown),
                },
            );
            Vec::new()
        }

        // Structural declarations carry no claim to commit to.
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Evidence;

    fn ev(cycle: u64, payload: ResearchEventPayload) -> ResearchEvent {
        ResearchEvent {
            cycle,
            source: "ix".to_string(),
            payload,
        }
    }

    #[test]
    fn every_claim_assertion_is_accepted_at_face_value() {
        let trace = ResearchTrace {
            dimension: 4,
            events: vec![
                ev(
                    1,
                    ResearchEventPayload::ExperimentResult {
                        proposition: "p".to_string(),
                        value: HexValue::Doubtful,
                        evidence: Evidence::new(),
                    },
                ),
                ev(
                    2,
                    ResearchEventPayload::GoalUpdate {
                        key: "g".to_string(),
                        description: "d".to_string(),
                        priority: 0.5,
                        status: None,
                    },
                ),
            ],
        };
        let report = replay_unassisted(trace);

        // Even a Doubtful claim is accepted — that is what "no policy" means,
        // and it is why the arm is a meaningful null rather than a strawman.
        assert!(matches!(
            report.outcomes[0].actions.as_slice(),
            [Action::Accept { .. }]
        ));
        assert!(report.outcomes[1].actions.is_empty());
        assert_eq!(report.final_beliefs.get("p"), Some(&HexValue::Doubtful));
    }

    /// The arm has no policy, so it has no reason to withhold — and therefore
    /// can never be charged under §5.3. Pinned so the disqualifier is read with
    /// that in mind rather than as evidence of good judgment.
    #[test]
    fn the_unassisted_arm_never_withholds() {
        let trace = ResearchTrace {
            dimension: 4,
            events: vec![
                ev(
                    1,
                    ResearchEventPayload::BeliefUpdate {
                        proposition: "p".to_string(),
                        value: HexValue::Probable,
                        evidence: Evidence::new(),
                    },
                ),
                ev(
                    2,
                    ResearchEventPayload::Retraction {
                        proposition: "p".to_string(),
                        reason: "withdrawn".to_string(),
                        retracts: None,
                    },
                ),
            ],
        };
        let report = replay_unassisted(trace);
        assert!(report
            .outcomes
            .iter()
            .flat_map(|o| &o.actions)
            .all(|a| !matches!(a, Action::Wait)));
    }
}
