//! # Subjective Logic baseline (Jøsang 2016)
//!
//! Minimum-viable Subjective Logic implementation provided as a parallel
//! prior-art baseline against which Project Hari's `Lie` and `RecencyDecay`
//! priority models can be compared. See
//! `docs/research/prior-art-survey.md` §4 — the survey explicitly flags
//! Jøsang's framework as the most relevant prior art and the missing
//! comparator. This module exists so the Phase 5 verdict can be evaluated
//! against a credible non-Hari implementation, not because SL is being
//! adopted as the substrate.
//!
//! ## Scope
//!
//! Binary opinions only (`b`, `d`, `u`, `a` over a single proposition).
//! The cumulative-fusion operator from Jøsang's textbook is the sole
//! evidence-combination rule. Trust transitivity, multinomial opinions,
//! belief discounting, and the consensus operator are intentionally out
//! of scope — see the closing caveat in §X of the rollup doc.
//!
//! ## Public surface
//!
//! - [`Opinion`] — the binary `(b, d, u, a)` 4-tuple with
//!   `b + d + u = 1`.
//! - [`SubjectiveLogicConfig`] — thresholds for the
//!   opinion-to-action recommendation engine plus the default base rate.
//! - [`process_research_trace_subjective_logic`] — drop-in analog of
//!   `CognitiveLoop::process_research_trace` that produces a
//!   `ResearchReplayReport` whose `metrics` field is computed by the
//!   same `compute_metrics_for` function the Lie/RecencyDecay paths use.
//!
//! ## Why this is a free function and NOT a `PriorityModel` variant
//!
//! SL doesn't fit the action-scoring abstraction. `PriorityModel` is a
//! re-rank/suppress hook over a candidate-action list; SL produces
//! decisions directly from per-proposition opinions. Forcing SL into
//! `score_actions` would either lose information (compute an opinion,
//! throw it away, score a pre-baked candidate set) or violate the
//! abstraction (swap out the candidate-generation pipeline mid-stream).
//! The honest shape is a parallel pipeline.

use crate::{
    action_kind, compute_metrics_for, Action, Goal, ReplayMetrics, ResearchEvent,
    ResearchEventOutcome, ResearchEventPayload, ResearchReplayReport, ResearchTrace,
};
use hari_lattice::HexValue;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ---------------------------------------------------------------------------
// Opinion — the binary (b, d, u, a) tuple
// ---------------------------------------------------------------------------

/// A binary subjective-logic opinion over a single proposition.
///
/// Invariant: `belief + disbelief + uncertainty == 1` (held to within
/// floating-point tolerance after every operator). The projected
/// probability `P = b + u * a` is the SL analogue of a point-estimate.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Opinion {
    /// Belief mass — evidence supporting the proposition.
    pub belief: f64,
    /// Disbelief mass — evidence against the proposition.
    pub disbelief: f64,
    /// Uncertainty mass — absence/ambiguity of evidence.
    pub uncertainty: f64,
    /// Base rate `a ∈ [0, 1]` — prior probability before any evidence.
    pub base_rate: f64,
}

impl Opinion {
    /// The vacuous opinion: no evidence, full uncertainty, configurable
    /// prior `a`. This is the canonical starting point for fusion.
    pub fn vacuous(base_rate: f64) -> Self {
        let a = base_rate.clamp(0.0, 1.0);
        Self {
            belief: 0.0,
            disbelief: 0.0,
            uncertainty: 1.0,
            base_rate: a,
        }
    }

    /// Clamp every mass to `[0, 1]` and renormalise so that
    /// `b + d + u = 1`. Used after operators that can drift from the
    /// invariant by floating-point error.
    fn normalised(mut self) -> Self {
        self.belief = self.belief.max(0.0);
        self.disbelief = self.disbelief.max(0.0);
        self.uncertainty = self.uncertainty.max(0.0);
        let s = self.belief + self.disbelief + self.uncertainty;
        if s > 0.0 && (s - 1.0).abs() > 1e-12 {
            self.belief /= s;
            self.disbelief /= s;
            self.uncertainty /= s;
        }
        self.base_rate = self.base_rate.clamp(0.0, 1.0);
        self
    }

    /// Map a hexavalent value to an opinion using the recommended
    /// mapping table from the task spec.
    ///
    /// Choice notes:
    /// - `True`/`False` peg most mass on belief/disbelief and leave a
    ///   small residual uncertainty so cumulative fusion never produces
    ///   a degenerate `u = 0` (which makes the operator blow up).
    /// - `Probable`/`Doubtful` are ~55/15/30 — moderate commitment with
    ///   real residual uncertainty.
    /// - `Unknown` is essentially vacuous (5/5/90) — the small symmetric
    ///   non-zero belief/disbelief avoids `u = 1` exactly, which keeps
    ///   fusion well-defined when chained immediately after another
    ///   vacuous opinion (the textbook fusion formula has `u_a + u_b -
    ///   u_a u_b` in the denominator and we want that to stay > 0).
    /// - `Contradictory` is rendered as a balanced (0.45, 0.45, 0.10)
    ///   high-conflict opinion. SL doesn't have a native "Contradictory"
    ///   bucket — irreconcilable evidence shows up as roughly equal
    ///   `b` and `d`. A near-zero `u` keeps it from being silently
    ///   absorbed by a strong subsequent observation.
    pub fn from_hex(value: HexValue, base_rate: f64) -> Self {
        let a = base_rate.clamp(0.0, 1.0);
        let (b, d, u) = match value {
            HexValue::True => (0.85, 0.05, 0.10),
            HexValue::Probable => (0.55, 0.15, 0.30),
            HexValue::Unknown => (0.05, 0.05, 0.90),
            HexValue::Doubtful => (0.15, 0.55, 0.30),
            HexValue::False => (0.05, 0.85, 0.10),
            HexValue::Contradictory => (0.45, 0.45, 0.10),
        };
        Self {
            belief: b,
            disbelief: d,
            uncertainty: u,
            base_rate: a,
        }
    }

    /// Cumulative fusion (`⊕`) per Jøsang, *Subjective Logic* (Springer
    /// 2016), §12.3. For two opinions `ω_a = (b_a, d_a, u_a, a_a)` and
    /// `ω_b = (b_b, d_b, u_b, a_b)`:
    ///
    /// ```text
    /// k = u_a + u_b - u_a u_b
    /// b = (b_a u_b + b_b u_a) / k
    /// d = (d_a u_b + d_b u_a) / k
    /// u = (u_a u_b) / k
    /// a = (a_a u_b + a_b u_a - (a_a + a_b) u_a u_b) / (u_a + u_b - 2 u_a u_b)
    /// ```
    ///
    /// Edge cases:
    /// - When both opinions are dogmatic (`u_a = u_b = 0`), the formula
    ///   degenerates. We fall back to averaging the masses, which is
    ///   the textbook's recommended limit for the dogmatic case.
    /// - When one opinion is dogmatic and the other isn't, the
    ///   dogmatic one wins (consistent with cumulative fusion treating
    ///   `u = 0` as "no further evidence can move me").
    /// - The base-rate denominator `u_a + u_b - 2 u_a u_b` can be zero
    ///   when both `u`s are 1; in that case we keep `ω_a`'s base rate.
    ///
    /// Cumulative fusion is **commutative** but not associative in the
    /// general case (Jøsang §12.3.1). Order-independence for a stream
    /// of evidence is therefore approximate — the per-event projected
    /// probability sequence depends mildly on order, but the fixed
    /// point of a finite stream typically does not. Our test
    /// `cumulative_fuse_is_commutative` checks the operator on a single
    /// pair; longer streams aren't expected to be exactly invariant.
    pub fn cumulative_fuse(a: Opinion, b: Opinion) -> Opinion {
        // Dogmatic-dogmatic limit: average the masses.
        if a.uncertainty == 0.0 && b.uncertainty == 0.0 {
            return Opinion {
                belief: 0.5 * (a.belief + b.belief),
                disbelief: 0.5 * (a.disbelief + b.disbelief),
                uncertainty: 0.0,
                base_rate: 0.5 * (a.base_rate + b.base_rate),
            }
            .normalised();
        }
        // One-side-dogmatic: take the dogmatic side. Cumulative fusion
        // semantics treat `u = 0` as "no further evidence can move me".
        if a.uncertainty == 0.0 {
            return a;
        }
        if b.uncertainty == 0.0 {
            return b;
        }

        let k = a.uncertainty + b.uncertainty - a.uncertainty * b.uncertainty;
        let belief = (a.belief * b.uncertainty + b.belief * a.uncertainty) / k;
        let disbelief = (a.disbelief * b.uncertainty + b.disbelief * a.uncertainty) / k;
        let uncertainty = (a.uncertainty * b.uncertainty) / k;

        let denom = a.uncertainty + b.uncertainty - 2.0 * a.uncertainty * b.uncertainty;
        let base_rate = if denom.abs() < 1e-12 {
            a.base_rate
        } else {
            (a.base_rate * b.uncertainty + b.base_rate * a.uncertainty
                - (a.base_rate + b.base_rate) * a.uncertainty * b.uncertainty)
                / denom
        };

        Opinion {
            belief,
            disbelief,
            uncertainty,
            base_rate,
        }
        .normalised()
    }

    /// Mix this opinion with `vacuous(base_rate)` at `weight ∈ [0, 1]`.
    /// Used to apply a smaller-weight discount to `agent_vote` events
    /// — voting evidence is treated as ~half the strength of a direct
    /// `belief_update` or `experiment_result`. This preserves the SL
    /// invariant `b + d + u = 1`.
    pub fn discounted(self, weight: f64) -> Self {
        let w = weight.clamp(0.0, 1.0);
        let v = Opinion::vacuous(self.base_rate);
        Opinion {
            belief: w * self.belief + (1.0 - w) * v.belief,
            disbelief: w * self.disbelief + (1.0 - w) * v.disbelief,
            uncertainty: w * self.uncertainty + (1.0 - w) * v.uncertainty,
            base_rate: self.base_rate,
        }
        .normalised()
    }

    /// Projected probability `P = b + u * a`. SL's point-estimate of
    /// the proposition's truth value.
    pub fn projected_probability(&self) -> f64 {
        (self.belief + self.uncertainty * self.base_rate).clamp(0.0, 1.0)
    }
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Tunable thresholds for the SL recommendation engine. Defaults match
/// the task spec: belief-Accept fires above 0.7, disbelief-Accept
/// (i.e., Accept-as-False) fires symmetrically, conflict (`b > 0.4` AND
/// `d > 0.4`) escalates, high uncertainty (`u > 0.7`) investigates.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SubjectiveLogicConfig {
    /// Default base rate `a` for a freshly-minted opinion. 0.5 means
    /// no prior bias.
    pub default_base_rate: f64,
    /// Threshold for `b` above which the engine emits an
    /// `Accept(prop, True/Probable)`.
    pub belief_accept_threshold: f64,
    /// Threshold for `d` above which the engine emits an
    /// `Accept(prop, False/Doubtful)`.
    pub disbelief_accept_threshold: f64,
    /// Both `b` and `d` above this threshold → conflict → Escalate.
    pub conflict_threshold: f64,
    /// `u` above this threshold → Investigate.
    pub uncertainty_investigate_threshold: f64,
    /// `b` above this threshold (inside the True/Probable bucket)
    /// renders as `True`; below it as `Probable`.
    pub strong_belief_value_threshold: f64,
    /// Discount applied to `agent_vote` payloads via [`Opinion::discounted`].
    /// Default 0.5 — a vote is treated as half-strength evidence relative
    /// to a direct `belief_update`.
    pub agent_vote_weight: f64,
}

impl Default for SubjectiveLogicConfig {
    fn default() -> Self {
        Self {
            default_base_rate: 0.5,
            belief_accept_threshold: 0.7,
            disbelief_accept_threshold: 0.7,
            conflict_threshold: 0.4,
            uncertainty_investigate_threshold: 0.7,
            strong_belief_value_threshold: 0.85,
            agent_vote_weight: 0.5,
        }
    }
}

// ---------------------------------------------------------------------------
// Recommendation engine
// ---------------------------------------------------------------------------

/// Map an opinion to a small action list, mirroring Hari's
/// `recommend_for_claim` shape so the metric pipeline can reuse the
/// same `Action::Accept`-counting / `Action::Escalate`-spotting logic.
fn recommend_from_opinion(
    proposition: &str,
    op: &Opinion,
    cfg: &SubjectiveLogicConfig,
) -> Vec<Action> {
    // Conflict (high `b` AND high `d`) takes precedence — this is SL's
    // analogue of `Contradictory` and we want it to win over a
    // simultaneously-tripping uncertainty branch.
    let b = op.belief;
    let d = op.disbelief;
    let u = op.uncertainty;

    if b > cfg.conflict_threshold && d > cfg.conflict_threshold {
        // Reason includes the literal substring "contradictory" so
        // `compute_metrics_for`'s contradiction-detection pass can latch
        // onto SL conflicts the same way it latches onto Lie/RecencyDecay
        // Escalate-with-"contradictory" reasons. This is a mechanical
        // bridge — SL's "conflict" semantic IS Hari's "Contradictory"
        // semantic (irreconcilable evidence), so the metric pipeline
        // treats them uniformly.
        return vec![Action::Escalate {
            reason: format!(
                "subjective-logic conflict (contradictory) on '{}': belief={:.3} disbelief={:.3}",
                proposition, b, d
            ),
            confidence: 0.5,
        }];
    }

    if b > cfg.belief_accept_threshold {
        let value = if b >= cfg.strong_belief_value_threshold {
            HexValue::True
        } else {
            HexValue::Probable
        };
        return vec![Action::Accept {
            proposition: proposition.to_string(),
            value,
        }];
    }

    if d > cfg.disbelief_accept_threshold {
        let value = if d >= cfg.strong_belief_value_threshold {
            HexValue::False
        } else {
            HexValue::Doubtful
        };
        return vec![Action::Accept {
            proposition: proposition.to_string(),
            value,
        }];
    }

    if u > cfg.uncertainty_investigate_threshold {
        return vec![Action::Investigate {
            topic: proposition.to_string(),
        }];
    }

    vec![Action::Wait]
}

/// Map an opinion to the `HexValue` that best summarises it for the
/// final-beliefs map fed into `compute_metrics_for`. Uses projected
/// probability with a small uncertainty allowance so the contradiction
/// detector can see `Contradictory` when both belief and disbelief are
/// non-trivial.
fn hex_value_for_opinion(op: &Opinion, cfg: &SubjectiveLogicConfig) -> HexValue {
    let b = op.belief;
    let d = op.disbelief;
    let u = op.uncertainty;
    if b > cfg.conflict_threshold && d > cfg.conflict_threshold {
        return HexValue::Contradictory;
    }
    if u > cfg.uncertainty_investigate_threshold {
        return HexValue::Unknown;
    }
    let p = op.projected_probability();
    if p >= 0.85 {
        HexValue::True
    } else if p >= 0.55 {
        HexValue::Probable
    } else if p >= 0.45 {
        HexValue::Unknown
    } else if p >= 0.15 {
        HexValue::Doubtful
    } else {
        HexValue::False
    }
}

// ---------------------------------------------------------------------------
// Trace processing
// ---------------------------------------------------------------------------

/// Internal SL state: per-proposition running opinions plus the goal map
/// (mirrored from the trace so the rollup's `goal_completion_rate`
/// metric has something to read).
///
/// `pub(crate)` so `CognitiveLoop` can hold an instance and route
/// `process_research_event` to [`process_event`] when
/// `priority_model == PriorityModel::SubjectiveLogic`. External
/// consumers should still go through
/// [`process_research_trace_subjective_logic`] or pick the
/// `SubjectiveLogic` variant on `SessionConfig.priority_model`.
#[derive(Debug, Default)]
pub(crate) struct SubjectiveLogicState {
    pub(crate) opinions: BTreeMap<String, Opinion>,
    pub(crate) goals: BTreeMap<String, Goal>,
}

impl SubjectiveLogicState {
    pub(crate) fn opinion_for(
        &mut self,
        proposition: &str,
        cfg: &SubjectiveLogicConfig,
    ) -> &mut Opinion {
        self.opinions
            .entry(proposition.to_string())
            .or_insert_with(|| Opinion::vacuous(cfg.default_base_rate))
    }
}

/// Replay an IX research trace through the SL baseline. The returned
/// `ResearchReplayReport` is structurally identical to what
/// `CognitiveLoop::process_research_trace` produces — same `outcomes`
/// shape, same `metrics` shape, same `final_beliefs`/`final_goals`
/// — so 3-way comparisons are apples-to-apples.
///
/// `priority_model` on the report is left at its default
/// (`PriorityModel::Flat`) because SL does not score actions through
/// the `PriorityModel` ladder. The `final_state_summary` carries an
/// `[subjective-logic]` prefix so reports are easy to tell apart.
pub fn process_research_trace_subjective_logic(
    trace: ResearchTrace,
    config: SubjectiveLogicConfig,
) -> ResearchReplayReport {
    let mut state = SubjectiveLogicState::default();
    let mut outcomes: Vec<ResearchEventOutcome> = Vec::with_capacity(trace.events.len());
    let mut touched_propositions: BTreeMap<String, ()> = BTreeMap::new();

    for event in trace.events {
        if let Some(p) = event.payload.proposition() {
            touched_propositions.insert(p.to_string(), ());
        }
        let outcome = process_event(&mut state, event, &config);
        outcomes.push(outcome);
    }

    // Final beliefs map: project each running opinion to its closest
    // hexavalent value so `compute_metrics_for`'s false-acceptance
    // detector can read it in the same way it reads the Lie /
    // RecencyDecay paths' belief network.
    let final_beliefs: BTreeMap<String, HexValue> = touched_propositions
        .keys()
        .filter_map(|prop| {
            state
                .opinions
                .get(prop)
                .map(|op| (prop.clone(), hex_value_for_opinion(op, &config)))
        })
        .collect();

    // Push opinion-derived final status onto goals where the goal key
    // matches a touched proposition. Mirrors the Lie/RecencyDecay
    // treatment of goal status so the metric is comparable.
    for (key, goal) in state.goals.iter_mut() {
        if let Some(op) = state.opinions.get(key) {
            goal.status = hex_value_for_opinion(op, &config);
        }
    }

    let metrics: ReplayMetrics = compute_metrics_for(
        &outcomes,
        &final_beliefs,
        &state.goals,
        // SL has no `attention` vector — leave the bound at 0.
        0.0,
    );

    let opinion_count = state.opinions.len();
    let final_state_summary = format!(
        "[subjective-logic] {} touched propositions, {} goals tracked, {} opinions held",
        touched_propositions.len(),
        state.goals.len(),
        opinion_count
    );

    ResearchReplayReport {
        event_count: outcomes.len(),
        outcomes,
        final_beliefs,
        final_goals: state.goals,
        final_state_summary,
        priority_model: Default::default(),
        metrics,
        comparison: None,
    }
}

/// Process a single research event through the SL pipeline. Used by
/// both the standalone trace runner and `CognitiveLoop`'s
/// `PriorityModel::SubjectiveLogic` short-circuit branch.
pub(crate) fn process_event(
    state: &mut SubjectiveLogicState,
    event: ResearchEvent,
    cfg: &SubjectiveLogicConfig,
) -> ResearchEventOutcome {
    let mut actions: Vec<Action> = Vec::new();

    match &event.payload {
        ResearchEventPayload::BeliefUpdate {
            proposition,
            value,
            evidence,
        }
        | ResearchEventPayload::ExperimentResult {
            proposition,
            value,
            evidence,
        } => {
            let fresh = Opinion::from_hex(*value, cfg.default_base_rate);
            let running = *state.opinion_for(proposition, cfg);
            let fused = Opinion::cumulative_fuse(running, fresh);
            *state.opinion_for(proposition, cfg) = fused;

            actions.push(Action::Log(format!(
                "SL fused '{}': b={:.3} d={:.3} u={:.3} P={:.3}",
                proposition,
                fused.belief,
                fused.disbelief,
                fused.uncertainty,
                fused.projected_probability()
            )));
            actions.extend(recommend_from_opinion(proposition, &fused, cfg));

            if !evidence.is_empty() {
                actions.push(Action::Log(format!(
                    "Recorded {} evidence fields for '{}'",
                    evidence.len(),
                    proposition
                )));
            }
        }
        ResearchEventPayload::AgentVote {
            proposition,
            value,
            evidence,
        } => {
            let fresh =
                Opinion::from_hex(*value, cfg.default_base_rate).discounted(cfg.agent_vote_weight);
            let running = *state.opinion_for(proposition, cfg);
            let fused = Opinion::cumulative_fuse(running, fresh);
            *state.opinion_for(proposition, cfg) = fused;

            actions.push(Action::Log(format!(
                "SL fused vote '{}': b={:.3} d={:.3} u={:.3} P={:.3}",
                proposition,
                fused.belief,
                fused.disbelief,
                fused.uncertainty,
                fused.projected_probability()
            )));
            actions.extend(recommend_from_opinion(proposition, &fused, cfg));

            if !evidence.is_empty() {
                actions.push(Action::Log(format!(
                    "Recorded {} vote fields for '{}'",
                    evidence.len(),
                    proposition
                )));
            }
        }
        ResearchEventPayload::Retraction {
            proposition,
            reason,
        } => {
            // SL has no native retraction; the cleanest analog is to
            // reset the opinion to vacuous so subsequent fusion
            // re-accumulates from a no-evidence prior. Mirrors the
            // Lie/RecencyDecay path's "set to Unknown" treatment.
            state
                .opinions
                .insert(proposition.clone(), Opinion::vacuous(cfg.default_base_rate));
            actions.push(Action::Log(format!(
                "Retracted '{}': {} (opinion reset to vacuous)",
                proposition, reason
            )));
            actions.push(Action::Retry {
                topic: proposition.clone(),
            });
        }
        ResearchEventPayload::GoalUpdate {
            key,
            description,
            priority,
            status,
        } => {
            let goal = state.goals.entry(key.clone()).or_insert(Goal {
                description: description.clone(),
                priority: *priority,
                status: HexValue::Unknown,
            });
            goal.description = description.clone();
            goal.priority = *priority;
            if let Some(s) = status {
                goal.status = *s;
            }
            actions.push(Action::Log(format!(
                "Goal '{}' updated from {} (SL)",
                key, event.source
            )));
        }
        // SL operates on Opinion fusion, not the BeliefNetwork — there
        // is no graph to declare relations on. Log and ignore so SL
        // sessions can still consume traces that include
        // RelationDeclaration events without erroring.
        ResearchEventPayload::RelationDeclaration { from, to, relation } => {
            actions.push(Action::Log(format!(
                "SL ignored RelationDeclaration {:?}: '{}' -> '{}' (SL has no relation graph)",
                relation, from, to
            )));
        }
    }

    let state_summary = {
        let prop_opt = event.payload.proposition().map(|p| p.to_string());
        match prop_opt.as_ref().and_then(|p| state.opinions.get(p)) {
            Some(op) => format!(
                "SL[{}]: b={:.2} d={:.2} u={:.2} a={:.2} P={:.3}",
                prop_opt.unwrap_or_else(|| "?".into()),
                op.belief,
                op.disbelief,
                op.uncertainty,
                op.base_rate,
                op.projected_probability()
            ),
            None => "SL: no proposition touched".to_string(),
        }
    };

    ResearchEventOutcome {
        event,
        actions,
        state_summary,
        // SL operates on Opinion fusion, not the BeliefNetwork — no
        // belief-graph derivations apply.
        derivations: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// 3-way comparison driver
// ---------------------------------------------------------------------------

/// Three-way side-by-side metrics: `RecencyDecay` baseline,
/// `Lie` experimental, and the SL baseline. Produced by
/// [`compare_replay_three_way`] for the `--compare3` CLI flow and
/// consumed by the rollup-update workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayComparisonThreeWay {
    pub recency_decay: ReplayMetrics,
    pub lie: ReplayMetrics,
    pub subjective_logic: ReplayMetrics,
    /// Per-event-index action-kind sequences for each model. Each entry
    /// is a tuple `(event_index, [decay_kinds, lie_kinds, sl_kinds])`,
    /// emitted only at indexes where the three models disagree on the
    /// kind sequence. Kept compact (kinds, not full actions) so the
    /// JSON report size stays manageable on long fixtures.
    pub divergence_pairs: Vec<ThreeWayDivergence>,
}

/// One event index where at least two of the three models chose
/// different action-kind sequences. Logs are ignored for divergence
/// detection — they're side-channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreeWayDivergence {
    pub event_index: usize,
    pub recency_decay: Vec<String>,
    pub lie: Vec<String>,
    pub subjective_logic: Vec<String>,
}

/// Wrapper report emitted by the CLI's `--compare3` mode. Holds three
/// individual reports plus the side-by-side metrics struct so
/// downstream tooling can either crunch the raw outcome lists or read
/// the pre-aggregated rollup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreeWayReplayReport {
    pub recency_decay: ResearchReplayReport,
    pub lie: ResearchReplayReport,
    pub subjective_logic: ResearchReplayReport,
    pub comparison: ReplayComparisonThreeWay,
}

/// Run the trace through `Lie`, `RecencyDecay`, and the SL baseline on
/// fresh state. Returns a wrapper report — the three individual
/// `ResearchReplayReport`s are preserved verbatim, plus a
/// `ReplayComparisonThreeWay` summary that pulls out the metric tables.
///
/// We use a wrapper rather than extending `ReplayComparison` because
/// the Phase 6 replay-parity test asserts the existing
/// `ReplayComparison` shape and we don't want to perturb it.
pub fn compare_replay_three_way(
    trace: ResearchTrace,
    sl_config: SubjectiveLogicConfig,
) -> ThreeWayReplayReport {
    use crate::{CognitiveLoop, PriorityModel};

    let mut decay_loop = CognitiveLoop::with_model(trace.dimension, PriorityModel::RecencyDecay);
    let mut lie_loop = CognitiveLoop::with_model(trace.dimension, PriorityModel::Lie);

    let decay_report = decay_loop.process_research_trace(trace.clone());
    let lie_report = lie_loop.process_research_trace(trace.clone());
    let sl_report = process_research_trace_subjective_logic(trace, sl_config);

    let divergence_pairs = three_way_divergences(
        &decay_report.outcomes,
        &lie_report.outcomes,
        &sl_report.outcomes,
    );

    let comparison = ReplayComparisonThreeWay {
        recency_decay: decay_report.metrics.clone(),
        lie: lie_report.metrics.clone(),
        subjective_logic: sl_report.metrics.clone(),
        divergence_pairs,
    };

    ThreeWayReplayReport {
        recency_decay: decay_report,
        lie: lie_report,
        subjective_logic: sl_report,
        comparison,
    }
}

fn three_way_divergences(
    decay: &[ResearchEventOutcome],
    lie: &[ResearchEventOutcome],
    sl: &[ResearchEventOutcome],
) -> Vec<ThreeWayDivergence> {
    let n = decay.len().min(lie.len()).min(sl.len());
    let mut out = Vec::new();
    for i in 0..n {
        let dk: Vec<String> = decay[i]
            .actions
            .iter()
            .filter(|a| !matches!(a, Action::Log(_)))
            .map(|a| action_kind(a).to_string())
            .collect();
        let lk: Vec<String> = lie[i]
            .actions
            .iter()
            .filter(|a| !matches!(a, Action::Log(_)))
            .map(|a| action_kind(a).to_string())
            .collect();
        let sk: Vec<String> = sl[i]
            .actions
            .iter()
            .filter(|a| !matches!(a, Action::Log(_)))
            .map(|a| action_kind(a).to_string())
            .collect();
        if dk != lk || dk != sk || lk != sk {
            out.push(ThreeWayDivergence {
                event_index: i,
                recency_decay: dk,
                lie: lk,
                subjective_logic: sk,
            });
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Tests — Opinion arithmetic only. Trace-level assertions live in
// `tests/subjective_logic_baseline.rs` so they can pull in fixtures.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vacuous_invariant_holds() {
        let v = Opinion::vacuous(0.5);
        let s = v.belief + v.disbelief + v.uncertainty;
        assert!((s - 1.0).abs() < 1e-12);
        assert_eq!(v.uncertainty, 1.0);
        assert_eq!(v.base_rate, 0.5);
    }

    #[test]
    fn from_hex_invariant_holds_for_every_value() {
        for v in [
            HexValue::True,
            HexValue::Probable,
            HexValue::Unknown,
            HexValue::Doubtful,
            HexValue::False,
            HexValue::Contradictory,
        ] {
            let op = Opinion::from_hex(v, 0.5);
            let s = op.belief + op.disbelief + op.uncertainty;
            assert!(
                (s - 1.0).abs() < 1e-12,
                "from_hex({:?}) mass = {} not 1",
                v,
                s
            );
        }
    }

    #[test]
    fn from_hex_true_projects_above_half() {
        let p = Opinion::from_hex(HexValue::True, 0.5).projected_probability();
        assert!(p > 0.5, "True opinion should project above 0.5, got {p}");
    }

    #[test]
    fn from_hex_false_projects_below_half() {
        let p = Opinion::from_hex(HexValue::False, 0.5).projected_probability();
        assert!(p < 0.5, "False opinion should project below 0.5, got {p}");
    }

    #[test]
    fn cumulative_fuse_is_commutative_for_two_non_degenerate_opinions() {
        let a = Opinion::from_hex(HexValue::Probable, 0.5);
        let b = Opinion::from_hex(HexValue::Doubtful, 0.5);
        let ab = Opinion::cumulative_fuse(a, b);
        let ba = Opinion::cumulative_fuse(b, a);
        assert!((ab.belief - ba.belief).abs() < 1e-9);
        assert!((ab.disbelief - ba.disbelief).abs() < 1e-9);
        assert!((ab.uncertainty - ba.uncertainty).abs() < 1e-9);
    }

    #[test]
    fn cumulative_fuse_with_vacuous_is_identity() {
        let a = Opinion::from_hex(HexValue::True, 0.5);
        let v = Opinion::vacuous(0.5);
        let fused = Opinion::cumulative_fuse(a, v);
        assert!((fused.belief - a.belief).abs() < 1e-9);
        assert!((fused.disbelief - a.disbelief).abs() < 1e-9);
        assert!((fused.uncertainty - a.uncertainty).abs() < 1e-9);
    }

    #[test]
    fn discounted_keeps_invariant_and_moves_toward_vacuous() {
        let a = Opinion::from_hex(HexValue::True, 0.5);
        let half = a.discounted(0.5);
        let s = half.belief + half.disbelief + half.uncertainty;
        assert!((s - 1.0).abs() < 1e-12);
        assert!(half.uncertainty > a.uncertainty);
        assert!(half.belief < a.belief);
    }

    #[test]
    fn recommend_high_belief_emits_accept_true() {
        let cfg = SubjectiveLogicConfig::default();
        let op = Opinion {
            belief: 0.9,
            disbelief: 0.05,
            uncertainty: 0.05,
            base_rate: 0.5,
        };
        let actions = recommend_from_opinion("p", &op, &cfg);
        assert!(matches!(
            actions.first(),
            Some(Action::Accept {
                value: HexValue::True,
                ..
            })
        ));
    }

    #[test]
    fn recommend_balanced_belief_disbelief_emits_escalate() {
        let cfg = SubjectiveLogicConfig::default();
        let op = Opinion {
            belief: 0.45,
            disbelief: 0.45,
            uncertainty: 0.10,
            base_rate: 0.5,
        };
        let actions = recommend_from_opinion("p", &op, &cfg);
        assert!(matches!(actions.first(), Some(Action::Escalate { .. })));
    }

    #[test]
    fn recommend_high_uncertainty_emits_investigate() {
        let cfg = SubjectiveLogicConfig::default();
        let op = Opinion::vacuous(0.5);
        let actions = recommend_from_opinion("p", &op, &cfg);
        assert!(matches!(actions.first(), Some(Action::Investigate { .. })));
    }
}
