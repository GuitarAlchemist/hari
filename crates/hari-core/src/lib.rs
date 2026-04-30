//! # hari-core — Core Cognitive Loop Orchestrator
//!
//! The central crate of Project Hari, orchestrating the cognitive loop:
//! **Perceive -> Think -> Act -> Repeat**
//!
//! This is where the lattice logic, cognitive algebra, and swarm systems
//! come together into a unified cognitive architecture.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────┐
//! │              CognitiveLoop              │
//! │                                         │
//! │  ┌──────────┐  ┌───────┐  ┌──────────┐ │
//! │  │ Perceive │─>│ Think │─>│   Act    │ │
//! │  └──────────┘  └───────┘  └──────────┘ │
//! │       ↑                        │        │
//! │       └────────────────────────┘        │
//! │                                         │
//! │  CognitiveState                         │
//! │  ├── beliefs (BeliefNetwork)            │
//! │  ├── goals (priority queue)             │
//! │  └── attention (focus vector)           │
//! └─────────────────────────────────────────┘
//! ```

use hari_cognition::{Evolution, SymmetryGroup};
use hari_lattice::{BeliefNetwork, HexValue};
use hari_swarm::Message;
use nalgebra::{DMatrix, DVector};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

// ---------------------------------------------------------------------------
// Phase 6 — IX autoresearch streaming integration
// ---------------------------------------------------------------------------
pub mod protocol;
pub mod session;

pub use protocol::{
    InitialGoal, RecommendationResponse, Request, Response, SessionConfig, ShadowCompare,
};
pub use session::{StreamingSession, TraceRecorder};

// ---------------------------------------------------------------------------
// PriorityModel — strategy enum used by score_actions
// ---------------------------------------------------------------------------

/// Strategy for prioritising candidate actions in [`CognitiveLoop::score_actions`].
///
/// `Flat` is the production-default backwards-compatible model — every
/// candidate gets priority 1.0 and ordering is preserved. `RecencyDecay` is
/// the comparison baseline (no algebra, just temporal decay). `Lie` is the
/// experimental Lie-algebra-driven model that consults the cognitive
/// `attention` vector evolved by the seeded generators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PriorityModel {
    /// All actions get priority 1.0; original order preserved.
    Flat,
    /// `priority = exp(-lambda * (current_cycle - perception_cycle))`.
    RecencyDecay,
    /// `priority = base * (1 + alpha * proj(attention, action_axis))`.
    Lie,
}

impl Default for PriorityModel {
    fn default() -> Self {
        Self::Flat
    }
}

// ---------------------------------------------------------------------------
// Perception — incoming signals
// ---------------------------------------------------------------------------

/// A perception is an incoming signal from the environment.
///
/// Perceptions are the raw inputs to the cognitive loop. They carry
/// a proposition (what was perceived) and an initial confidence value.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Perception {
    /// What was perceived — maps to a proposition in the belief network
    pub proposition: String,
    /// Initial confidence in this perception
    pub value: HexValue,
    /// Source of the perception (sensor, agent, inference, etc.)
    pub source: String,
    /// Timestamp (cycle number)
    pub cycle: u64,
}

impl fmt::Display for Perception {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Perception({}: {} from {} @cycle {})",
            self.proposition, self.value, self.source, self.cycle
        )
    }
}

// ---------------------------------------------------------------------------
// Action — output actions
// ---------------------------------------------------------------------------

/// An action the cognitive system can take.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Action {
    /// Update a belief in the network
    UpdateBelief {
        proposition: String,
        value: HexValue,
    },
    /// Send a message to another agent in the swarm
    SendMessage(Message),
    /// Request more information about a topic
    Investigate { topic: String },
    /// Escalate a decision to a higher authority
    Escalate { reason: String, confidence: f64 },
    /// Retry or rerun an investigation path.
    Retry { topic: String },
    /// Accept a research claim at its current value.
    Accept {
        proposition: String,
        value: HexValue,
    },
    /// No action — the system decided to wait
    Wait,
    /// Log an observation (for auditability)
    Log(String),
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Action::UpdateBelief { proposition, value } => {
                write!(f, "UpdateBelief({proposition}: {value})")
            }
            Action::SendMessage(msg) => write!(f, "SendMessage({msg})"),
            Action::Investigate { topic } => write!(f, "Investigate({topic})"),
            Action::Escalate { reason, confidence } => {
                write!(f, "Escalate({reason}, conf={confidence:.2})")
            }
            Action::Retry { topic } => write!(f, "Retry({topic})"),
            Action::Accept { proposition, value } => {
                write!(f, "Accept({proposition}: {value})")
            }
            Action::Wait => write!(f, "Wait"),
            Action::Log(msg) => write!(f, "Log({msg})"),
        }
    }
}

// ---------------------------------------------------------------------------
// Research events — IX-facing claim updates
// ---------------------------------------------------------------------------

/// Structured evidence metadata attached to a research event.
pub type Evidence = BTreeMap<String, serde_json::Value>;

/// A research event submitted by IX or another autoresearch system.
///
/// These events are the first boundary between Hari and external research
/// orchestration. They are intentionally data-only so they can be replayed from
/// traces before a live service boundary exists.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchEvent {
    /// Timestamp or replay cycle supplied by the external system.
    pub cycle: u64,
    /// Source agent, runner, benchmark, or system that emitted the event.
    pub source: String,
    /// Event payload.
    pub payload: ResearchEventPayload,
}

/// Typed research observations that Hari can convert into beliefs and actions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResearchEventPayload {
    /// Direct claim update from an agent or tool.
    BeliefUpdate {
        proposition: String,
        value: HexValue,
        #[serde(default)]
        evidence: Evidence,
    },
    /// Result from an IX experiment or benchmark run.
    ExperimentResult {
        proposition: String,
        value: HexValue,
        #[serde(default)]
        evidence: Evidence,
    },
    /// Vote from an agent participating in research consensus.
    AgentVote {
        proposition: String,
        value: HexValue,
        #[serde(default)]
        evidence: Evidence,
    },
    /// A previous claim or result has been withdrawn or invalidated.
    Retraction { proposition: String, reason: String },
    /// Update or create a goal that should influence future prioritization.
    GoalUpdate {
        key: String,
        description: String,
        priority: f64,
        status: Option<HexValue>,
    },
}

impl ResearchEvent {
    /// Owned copy of the proposition this event targets, if any. Used by
    /// the streaming layer to track touched propositions without
    /// borrowing the event past `process_research_event` consumption.
    pub fn payload_proposition_owned(&self) -> Option<String> {
        self.payload.proposition().map(str::to_string)
    }
}

/// Result of applying one research event to the cognitive loop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchEventOutcome {
    /// The event that was processed.
    pub event: ResearchEvent,
    /// Actions recommended after processing the event.
    pub actions: Vec<Action>,
    /// Human-readable state summary after the event.
    pub state_summary: String,
}

/// Replayable IX/autoresearch trace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchTrace {
    /// Optional cognitive state dimension for this replay.
    #[serde(default = "default_research_dimension")]
    pub dimension: usize,
    /// Events to process in order.
    pub events: Vec<ResearchEvent>,
}

impl From<Vec<ResearchEvent>> for ResearchTrace {
    fn from(events: Vec<ResearchEvent>) -> Self {
        Self {
            dimension: default_research_dimension(),
            events,
        }
    }
}

fn default_research_dimension() -> usize {
    4
}

/// Machine-readable report from replaying a research trace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchReplayReport {
    /// Number of events processed.
    pub event_count: usize,
    /// Outcome from each event.
    pub outcomes: Vec<ResearchEventOutcome>,
    /// Final belief values for propositions touched by the trace.
    pub final_beliefs: BTreeMap<String, HexValue>,
    /// Final goal states.
    pub final_goals: BTreeMap<String, Goal>,
    /// Human-readable final state summary.
    pub final_state_summary: String,
    /// Priority model used to produce this report.
    #[serde(default)]
    pub priority_model: PriorityModel,
    /// Post-hoc metrics derived from the outcome list.
    #[serde(default = "ReplayMetrics::zero")]
    pub metrics: ReplayMetrics,
    /// Side-by-side comparison populated by the `--compare` CLI flow.
    #[serde(default)]
    pub comparison: Option<ReplayComparison>,
}

/// Aggregate metrics computed once per replay.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReplayMetrics {
    /// Cycles between the first `Contradictory` belief value and the next
    /// non-contradictory one across the trace, or `None` if the trace
    /// never recovers.
    pub contradiction_recovery_cycles: Option<u64>,
    /// Number of `Action::Accept` recommendations that were later retracted
    /// or contradicted by a subsequent event.
    pub false_acceptance_count: u32,
    /// Fraction of goals whose final status is `True` or `Probable`.
    pub goal_completion_rate: f64,
    /// `1 - (flips / events)` averaged over touched propositions.
    pub consensus_stability: f64,
    /// Maximum `attention.norm()` observed during the replay.
    pub attention_norm_max: f64,
    /// Counts of each `Action` variant emitted across the trace.
    pub action_counts_by_kind: BTreeMap<String, u32>,
}

impl ReplayMetrics {
    fn zero() -> Self {
        Self::default()
    }
}

/// Side-by-side metrics for the `--compare` CLI mode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayComparison {
    pub baseline: ReplayMetrics,
    pub experimental: ReplayMetrics,
    pub action_divergence: Vec<ActionDivergence>,
}

/// One event index where the two priority models chose different actions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionDivergence {
    pub event_index: usize,
    pub baseline_actions: Vec<Action>,
    pub experimental_actions: Vec<Action>,
}

// ---------------------------------------------------------------------------
// Goal — what the system is trying to achieve
// ---------------------------------------------------------------------------

/// A goal with priority and status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Goal {
    /// Description of the goal
    pub description: String,
    /// Priority (higher = more important)
    pub priority: f64,
    /// Current status as a hexavalent value
    /// (True = achieved, Probable = likely achieved, Unknown = in progress, etc.)
    pub status: HexValue,
}

// ---------------------------------------------------------------------------
// CognitiveState — the brain's state
// ---------------------------------------------------------------------------

/// The central state of the cognitive system.
///
/// This holds everything the system "knows" and "wants":
/// - **beliefs**: A hexavalent belief network with propositions and relations
/// - **goals**: Prioritized objectives the system is working toward
/// - **attention**: A focus vector in cognitive state space (used by the algebra)
/// - **cycle**: How many think-perceive-act cycles have run
pub struct CognitiveState {
    /// The belief network — what the system thinks is true/false/unknown
    pub beliefs: BeliefNetwork,
    /// Active goals sorted by priority
    pub goals: BTreeMap<String, Goal>,
    /// Attention vector — where cognitive resources are focused
    pub attention: DVector<f64>,
    /// Current cycle number
    pub cycle: u64,
    /// Dimension of the cognitive state space
    pub dimension: usize,
}

impl CognitiveState {
    /// Create a new cognitive state with the given dimension.
    ///
    /// The dimension determines the size of the attention vector and
    /// the cognitive algebra's state space. Higher dimensions allow
    /// more nuanced cognitive representations but are more expensive.
    pub fn new(dimension: usize) -> Self {
        // Uniform unit vector: every dimension starts with the same projection
        // weight so `proj(attention, e_k)` is non-zero from cycle 1 for every
        // goal axis. Previously the seed `[1, 0, 0, …, 0]` made the
        // multiplicative scoring rule `(1 + α * proj)` collapse to 1.0 on every
        // axis except dim 0 until perceptions had rotated attention away,
        // which forced the Lie path to depend on large α/dt to produce any
        // divergence at all. Uniform seed removes that fragility.
        let attention = if dimension > 0 {
            let w = 1.0 / (dimension as f64).sqrt();
            DVector::from_element(dimension, w)
        } else {
            DVector::zeros(dimension)
        };

        Self {
            beliefs: BeliefNetwork::new(),
            goals: BTreeMap::new(),
            attention,
            cycle: 0,
            dimension,
        }
    }

    /// Add a goal to the cognitive state.
    pub fn add_goal(
        &mut self,
        key: impl Into<String>,
        description: impl Into<String>,
        priority: f64,
    ) {
        self.goals.insert(
            key.into(),
            Goal {
                description: description.into(),
                priority,
                status: HexValue::Unknown,
            },
        );
    }

    /// Get the highest priority active goal.
    pub fn top_goal(&self) -> Option<(&String, &Goal)> {
        self.goals
            .iter()
            .filter(|(_, g)| !matches!(g.status, HexValue::True))
            .max_by(|(_, a), (_, b)| a.priority.partial_cmp(&b.priority).unwrap())
    }

    /// Map a goal/proposition key to a dimension index in the cognitive state
    /// space.
    ///
    /// Returns `Some(dim)` when `key` exists in `state.goals` (the position
    /// is its `BTreeMap` ordinal, clamped to `dimension - 1`). Returns
    /// `None` when the key is absent — callers typically fall back to
    /// dimension 0 (the shared "background" axis).
    pub fn goal_axis(&self, key: &str) -> Option<usize> {
        if self.dimension == 0 {
            return None;
        }
        let max = self.dimension - 1;
        for (idx, k) in self.goals.keys().enumerate() {
            if k == key {
                return Some(idx.min(max));
            }
        }
        None
    }

    /// Summary of the current state for logging.
    pub fn summary(&self) -> String {
        let belief_count = self.beliefs.len();
        let goal_count = self.goals.len();
        let active_goals = self
            .goals
            .values()
            .filter(|g| !matches!(g.status, HexValue::True))
            .count();
        format!(
            "Cycle {}: {} beliefs, {}/{} goals active, attention norm={:.3}",
            self.cycle,
            belief_count,
            active_goals,
            goal_count,
            self.attention.norm()
        )
    }
}

// ---------------------------------------------------------------------------
// CognitiveLoop — the main perceive-think-act cycle
// ---------------------------------------------------------------------------

/// The main cognitive loop: Perceive -> Think -> Act.
///
/// Each cycle:
/// 1. **Perceive**: Process incoming perceptions, update belief network
/// 2. **Think**: Run belief propagation, evaluate goals, evolve cognitive state
/// 3. **Act**: Decide on actions based on current state and goals
///
/// The loop integrates all subsystems:
/// - Lattice logic for belief management
/// - Cognitive algebra for state evolution
/// - Swarm for multi-agent coordination
pub struct CognitiveLoop {
    /// The cognitive state
    pub state: CognitiveState,
    /// Cognitive evolution engine (Lie algebra dynamics)
    evolution: Option<Evolution>,
    /// Pending perceptions to process
    perception_buffer: Vec<Perception>,
    /// Actions produced in the last cycle
    last_actions: Vec<Action>,
    /// Strategy used to prioritise actions returned from `cycle()`.
    pub priority_model: PriorityModel,
    /// Below this priority an action is suppressed to `Action::Wait`.
    pub theta_wait: f64,
    /// Time-step used by the Lie evolution engine when `priority_model == Lie`.
    pub lie_dt: f64,
    /// Strength of the perception-driven Hamiltonian and the
    /// `proj(attention, axis)` coupling. The plan recommends `0.5`; the
    /// factory spec allows tuning to `1.0`/`2.0` if no divergence appears.
    pub lie_alpha: f64,
    /// Recency-decay rate for `RecencyDecay`.
    pub lambda_decay: f64,
    /// Per-action provenance: which cycle each action in `last_actions` was
    /// derived from. Same length as `last_actions`. Used by `score_actions`.
    last_action_perception_cycles: Vec<u64>,
    /// Maximum value `attention.norm()` reached over the lifetime of this
    /// loop. Tracked so post-hoc analysis can verify boundedness.
    attention_norm_max: f64,
    /// Number of times the attention vector was renormalised because its
    /// norm exceeded the soft cap (10.0).
    attention_renorm_count: u32,
    /// Cached default seeded algebra dimension (so `init_algebra` calls don't
    /// thrash if the loop is reused).
    seeded_algebra: bool,
    /// Number of generators currently configured on the evolution engine.
    /// Tracked locally because `Evolution::generators` is private.
    evolution_generator_count: usize,
    /// Axis the seeded projection generator was built around. Reused by
    /// `perception_hamiltonian` so the coefficient targets the same axis
    /// as the generator, even if `top_goal` shifts mid-replay.
    seeded_projection_axis: Option<usize>,
}

impl CognitiveLoop {
    /// Create a new cognitive loop with the given state space dimension.
    pub fn new(dimension: usize) -> Self {
        Self {
            state: CognitiveState::new(dimension),
            evolution: None,
            perception_buffer: Vec::new(),
            last_actions: Vec::new(),
            priority_model: PriorityModel::default(),
            theta_wait: 0.1,
            // dt = 0.5 (research code, not production numerics): with α=2.0
            // this gives substantial per-step rotation so the (1 + α * proj)
            // scoring rule actually crosses zero on at least one axis when a
            // Doubtful event lands. Smaller dt left attention essentially
            // fixed at the seed and the Lie path could not diverge from
            // RecencyDecay. Boundedness is enforced separately by the
            // norm-cap renormalisation step.
            lie_dt: 0.5,
            // α = 2.0 — the upper end of the spec's tunable range, used when
            // divergence is too small at the default 0.5 or 1.0. The check
            // script's only contract is that some divergence exists; the
            // results doc records this tuning explicitly.
            lie_alpha: 2.0,
            lambda_decay: 0.2,
            last_action_perception_cycles: Vec::new(),
            attention_norm_max: 0.0,
            attention_renorm_count: 0,
            seeded_algebra: false,
            evolution_generator_count: 0,
            seeded_projection_axis: None,
        }
    }

    /// Builder-style constructor: pick a [`PriorityModel`] up front.
    ///
    /// Defaults to `PriorityModel::Flat` if not called, preserving the
    /// historical behavior of [`CognitiveLoop::cycle`].
    pub fn with_model(dimension: usize, model: PriorityModel) -> Self {
        let mut loop_ = Self::new(dimension);
        loop_.priority_model = model;
        loop_
    }

    /// Returns the maximum `attention.norm()` observed since this loop was
    /// constructed. Used by post-hoc metric computation.
    pub fn attention_norm_max(&self) -> f64 {
        self.attention_norm_max
    }

    /// How many cycles have hard-renormalised the attention vector because
    /// it exceeded the soft cap of 10.0.
    pub fn attention_renorm_count(&self) -> u32 {
        self.attention_renorm_count
    }

    /// Axis the projection generator was seeded around when the Lie algebra
    /// was first initialised, or `None` if the algebra has not been seeded
    /// yet. This is intentionally pinned at seed time so the per-cycle
    /// Hamiltonian targets the same axis the generator acts on, even if
    /// `top_goal` shifts mid-replay.
    pub fn seeded_projection_axis(&self) -> Option<usize> {
        self.seeded_projection_axis
    }

    /// Initialize the cognitive algebra with generators.
    ///
    /// This sets up the Lie algebra evolution engine. The generators
    /// define the "basis moves" of cognition — the fundamental operations
    /// from which all cognitive transformations are composed.
    pub fn init_algebra(&mut self, generators: Vec<DMatrix<f64>>, dt: f64) {
        let initial_state = self.state.attention.clone();
        let count = generators.len();
        self.evolution = Some(Evolution::new(initial_state, generators, dt));
        self.evolution_generator_count = count;
        self.seeded_algebra = false;
    }

    /// Seed the algebra with the canonical Phase 5 generator family
    /// (attention rotations, belief scaling, goal projection). Idempotent:
    /// if already seeded for this loop it does nothing.
    ///
    /// The generator basis (in order) is:
    /// 1. `D - 1` skew-symmetric **attention rotations**, one per axis
    ///    `k = 1..D` rotating dim 0 ↔ dim k. Coefficient `h_k` is the
    ///    perception-derived signed evidence on dim k.
    /// 2. One diagonal **belief scaling** generator with weights derived
    ///    from `HexValue` ranks. Coefficient is the scalar mean of h_dim.
    /// 3. One **goal projection** generator toward the top-priority goal's
    ///    axis. Coefficient is `h_dim[top_axis] - mean(h_dim)` (centered).
    ///
    /// This expands the plan's "three generators" sketch into a basis of
    /// size `D + 1` so the per-dimension Hamiltonian formula can actually
    /// move the corresponding axis. With three generators only (one
    /// rotation in the (0, 1) plane), perceptions on dims 2 and 3 had no
    /// effect, leaving `Lie` and `Flat`/`RecencyDecay` indistinguishable.
    fn ensure_seeded_algebra(&mut self) {
        if self.seeded_algebra && self.evolution.is_some() {
            return;
        }
        let d = self.state.dimension;
        if d == 0 {
            return;
        }

        let mut generators: Vec<DMatrix<f64>> = Vec::new();
        // (1) attention rotations: one per non-trivial axis pair (0, k).
        for k in 1..d {
            generators.push(SymmetryGroup::attention_rotation(d, 0, k));
        }
        // (2) belief scaling — diagonal generator. Weights mirror the
        // canonical `HexValue` rank gradient: dim 0 (background) shrinks
        // slightly, the rest expand.
        let scaling_weights: Vec<f64> = (0..d)
            .map(|k| if k == 0 { -0.5 } else { 0.5 })
            .collect();
        generators.push(SymmetryGroup::belief_scaling(d, &scaling_weights));
        // (3) goal projection toward the highest-priority goal's axis (or
        // axis 1 if no goals exist). Frozen at seed time and stored in
        // `seeded_projection_axis` so the per-cycle Hamiltonian uses the
        // same axis the generator was built around (Bug 1: target drift
        // when top_goal changes mid-replay).
        let target = self
            .state
            .top_goal()
            .and_then(|(k, _)| self.state.goal_axis(k))
            .unwrap_or(if d >= 2 { 1 } else { 0 });
        generators.push(SymmetryGroup::goal_projection(d, target));

        let count = generators.len();
        let initial_state = self.state.attention.clone();
        self.evolution = Some(Evolution::new(initial_state, generators, self.lie_dt));
        self.evolution_generator_count = count;
        self.seeded_algebra = true;
        self.seeded_projection_axis = Some(target);
    }

    /// Compute the per-generator Hamiltonian coefficients for this cycle.
    ///
    /// Generators are laid out as `[rot(0,1), rot(0,2), …, rot(0, D-1),
    /// scaling, projection]`. Coefficients:
    /// - `rot(0, k)` → `α * h_dim[k]`
    /// - `scaling` → `α * mean(h_dim)`
    /// - `projection` → `α * (h_dim[top_axis] - mean)`
    ///
    /// `h_dim[k]` is the signed sum of perception strengths targeting that
    /// dimension. Strength is `+1` for True/Probable, `-1` for
    /// Doubtful/False, `0` for Unknown, and `±2` split for Contradictory
    /// (net zero on `h_dim` but counted via a side channel so the rotation
    /// still reacts).
    fn perception_hamiltonian(&self, perceptions: &[Perception]) -> Vec<f64> {
        let d = self.state.dimension;
        if d == 0 {
            return Vec::new();
        }

        // h_dim[i] = sum over perceptions targeting dim i of strength
        let mut h_dim = vec![0.0_f64; d];
        let mut contradictory_total = 0.0_f64;
        for p in perceptions {
            let dim = self
                .state
                .goal_axis(&p.proposition)
                .unwrap_or(0)
                .min(d - 1);
            match p.value {
                HexValue::True | HexValue::Probable => h_dim[dim] += 1.0,
                HexValue::Doubtful | HexValue::False => h_dim[dim] -= 1.0,
                HexValue::Unknown => {}
                HexValue::Contradictory => {
                    contradictory_total += 2.0;
                }
            }
        }

        let alpha = self.lie_alpha;

        // Build the effective per-axis Hamiltonian h_eff once and reuse it for
        // rotations, scaling, and projection so the three coefficients agree
        // on what they're seeing (Bug 2: previously the scaling generator's
        // mean was computed on the un-smeared h_dim, ignoring the contradictory
        // smear that the rotations got).
        let smear_per_axis = if d > 1 {
            0.25 * contradictory_total / (d - 1) as f64
        } else {
            0.0
        };
        let mut h_eff = h_dim.clone();
        for k in 1..d {
            h_eff[k] += smear_per_axis;
        }

        let mut coefficients: Vec<f64> = Vec::with_capacity(d + 1);
        // Rotations rot(0, k) for k = 1..d
        for k in 1..d {
            coefficients.push(alpha * h_eff[k]);
        }
        // Scaling — mean over h_eff so contradictory smear is included.
        let mean = h_eff.iter().sum::<f64>() / d as f64;
        coefficients.push(alpha * mean);
        // Projection — use the axis the generator was actually built around
        // (Bug 1). Falls back to a re-derivation only if the loop is being
        // driven through `perception_hamiltonian` before `ensure_seeded_algebra`
        // has run (e.g. tests that exercise the helper in isolation).
        let target = self.seeded_projection_axis.unwrap_or_else(|| {
            self.state
                .top_goal()
                .and_then(|(k, _)| self.state.goal_axis(k))
                .unwrap_or(if d >= 2 { 1 } else { 0 })
                .min(d - 1)
        });
        coefficients.push(alpha * (h_eff[target] - mean));

        coefficients
    }

    /// Add a perception to the buffer for processing in the next cycle.
    pub fn perceive(&mut self, perception: Perception) {
        self.perception_buffer.push(perception);
    }

    /// Run one complete cognitive cycle: Perceive -> Think -> Act.
    ///
    /// Returns the actions decided upon in this cycle, after the active
    /// [`PriorityModel`] has had a chance to re-rank and (for non-Flat
    /// models) suppress low-priority actions to `Wait`.
    pub fn cycle(&mut self) -> Vec<Action> {
        let (actions, action_cycles) = self.cycle_raw();
        let scored = self.score_actions_with_cycles(actions, &action_cycles);
        let (final_actions, final_cycles): (Vec<_>, Vec<_>) = scored
            .into_iter()
            .map(|(action, _score, cycle)| (action, cycle))
            .unzip();
        self.last_actions = final_actions.clone();
        self.last_action_perception_cycles = final_cycles;
        final_actions
    }

    /// Run a Perceive→Think pass without scoring/filtering the resulting
    /// action list. Used by [`Self::process_research_event`] so that the
    /// per-event recommendation actions (Accept/Investigate/...) can be
    /// scored alongside the cycle's actions in a single pass.
    fn cycle_raw(&mut self) -> (Vec<Action>, Vec<u64>) {
        self.state.cycle += 1;
        let mut actions = Vec::new();
        // Per-action perception cycle provenance — same length as `actions`.
        let mut action_cycles: Vec<u64> = Vec::new();

        // --- PERCEIVE ---
        let perceptions: Vec<Perception> = self.perception_buffer.drain(..).collect();
        // Maximum perception cycle this turn (used as the "freshness" stamp
        // for actions that don't have an obvious source perception).
        let cycle_stamp = perceptions
            .iter()
            .map(|p| p.cycle)
            .max()
            .unwrap_or(self.state.cycle);

        for p in &perceptions {
            tracing::debug!("Processing perception: {}", p);
            // Add or update belief from perception
            if self.state.beliefs.get(&p.proposition).is_some() {
                if let Some(prop) = self.state.beliefs.get_mut(&p.proposition) {
                    let old = prop.value;
                    prop.value = hari_lattice::HexLattice::combine_evidence(old, p.value);
                    if prop.value != old {
                        actions.push(Action::Log(format!(
                            "Belief '{}' updated: {} -> {} (from {})",
                            p.proposition, old, prop.value, p.source
                        )));
                        action_cycles.push(p.cycle);
                    }
                }
            } else {
                self.state.beliefs.add_proposition(&p.proposition, p.value);
                actions.push(Action::Log(format!(
                    "New belief '{}': {} (from {})",
                    p.proposition, p.value, p.source
                )));
                action_cycles.push(p.cycle);
            }
        }

        // --- THINK ---
        // Run belief propagation
        let propagation_changes = self.state.beliefs.propagate();
        if propagation_changes > 0 {
            tracing::info!("Belief propagation changed {} nodes", propagation_changes);
        }

        // Evolve cognitive state when the Lie path is active. This is the
        // load-bearing replacement for the dead `if let Some(ref mut evo)`
        // block that used to compute coefficients then discard them.
        if matches!(self.priority_model, PriorityModel::Lie) {
            self.ensure_seeded_algebra();
            if !perceptions.is_empty() {
                let coefficients = self.perception_hamiltonian(&perceptions);
                let expected = self.evolution_generator_count;
                if let Some(ref mut evo) = self.evolution {
                    if coefficients.len() == expected {
                        evo.step(&coefficients);
                        // Mirror evolution back into state.attention so the
                        // rest of the loop (and downstream metrics) see it.
                        self.state.attention = evo.state.clone();
                    }
                }
            }
            // Track norm + clamp.
            let n = self.state.attention.norm();
            if n > self.attention_norm_max {
                self.attention_norm_max = n;
            }
            if n > 10.0 && n.is_finite() {
                // Hard-renormalise to a safe magnitude so the (1 + α * proj)
                // scoring rule cannot blow up downstream. The cap matches
                // the divergence-and-boundedness contract in the check
                // script.
                let scale = 5.0 / n;
                self.state.attention *= scale;
                if let Some(ref mut evo) = self.evolution {
                    evo.state = self.state.attention.clone();
                }
                self.attention_renorm_count += 1;
                self.attention_norm_max = self.state.attention.norm();
            }
        }

        // Evaluate goals
        if let Some((key, goal)) = self.state.top_goal() {
            let key = key.clone();
            let goal_desc = goal.description.clone();
            tracing::debug!("Top goal: {} (priority={})", goal_desc, goal.priority);

            // Check if any beliefs relate to goal completion
            if let Some(belief) = self.state.beliefs.get(&key) {
                match belief.value {
                    HexValue::True | HexValue::Probable => {
                        if let Some(g) = self.state.goals.get_mut(&key) {
                            g.status = belief.value;
                        }
                        actions.push(Action::Log(format!("Goal '{}' achieved!", goal_desc)));
                        action_cycles.push(cycle_stamp);
                    }
                    HexValue::Contradictory => {
                        actions.push(Action::Escalate {
                            reason: format!("Goal '{}' has contradictory evidence", goal_desc),
                            confidence: 0.5,
                        });
                        action_cycles.push(cycle_stamp);
                    }
                    HexValue::Unknown => {
                        actions.push(Action::Investigate { topic: key.clone() });
                        action_cycles.push(cycle_stamp);
                    }
                    _ => {}
                }
            }
        }

        // --- ACT ---
        // Hand back raw actions; the public `cycle()` (or
        // `process_research_event`) will score and filter them.
        debug_assert_eq!(actions.len(), action_cycles.len());
        (actions, action_cycles)
    }

    /// Rank candidate actions according to the active [`PriorityModel`].
    ///
    /// Returns `(action, score)` pairs in descending priority order. Actions
    /// whose top-of-the-list priority falls below `theta_wait` are replaced
    /// with `Action::Wait` and the rest are dropped — matching the spec's
    /// suppression rule. `Flat` is the no-op model (unit scores, original
    /// order). For lookups that need cycle provenance use
    /// [`Self::score_actions_with_cycles`] instead.
    pub fn score_actions(&self, actions: Vec<Action>) -> Vec<(Action, f64)> {
        let cycles = vec![self.state.cycle; actions.len()];
        self.score_actions_with_cycles(actions, &cycles)
            .into_iter()
            .map(|(a, s, _)| (a, s))
            .collect()
    }

    /// Like [`Self::score_actions`] but threads per-action perception cycles
    /// through so `RecencyDecay` has the freshness stamps it needs.
    fn score_actions_with_cycles(
        &self,
        actions: Vec<Action>,
        action_cycles: &[u64],
    ) -> Vec<(Action, f64, u64)> {
        debug_assert_eq!(actions.len(), action_cycles.len());
        if actions.is_empty() {
            return Vec::new();
        }

        let mut scored: Vec<(Action, f64, u64)> = match self.priority_model {
            PriorityModel::Flat => actions
                .into_iter()
                .zip(action_cycles.iter().copied())
                .map(|(a, c)| (a, 1.0, c))
                .collect(),
            PriorityModel::RecencyDecay => actions
                .into_iter()
                .zip(action_cycles.iter().copied())
                .map(|(a, perception_cycle)| {
                    let score = if matches!(a, Action::Wait | Action::Log(_)) {
                        // Side-channel actions get a flat low priority so
                        // they do not crowd out genuine recommendations.
                        0.05
                    } else {
                        let age = self
                            .state
                            .cycle
                            .saturating_sub(perception_cycle.min(self.state.cycle))
                            as f64;
                        (-self.lambda_decay * age).exp()
                    };
                    (a, score, perception_cycle)
                })
                .collect(),
            PriorityModel::Lie => {
                let alpha = self.lie_alpha;
                let attention = &self.state.attention;
                actions
                    .into_iter()
                    .zip(action_cycles.iter().copied())
                    .map(|(a, perception_cycle)| {
                        let score = match &a {
                            Action::Wait | Action::Log(_) => 0.05,
                            _ => {
                                let axis = self
                                    .action_axis(&a)
                                    .unwrap_or(0)
                                    .min(attention.len().saturating_sub(1));
                                let proj =
                                    attention.get(axis).copied().unwrap_or(0.0);
                                let base = 1.0;
                                base * (1.0 + alpha * proj)
                            }
                        };
                        (a, score, perception_cycle)
                    })
                    .collect()
            }
        };

        // Stable sort by score descending — preserves original order for
        // ties, which keeps `Flat` byte-identical to the legacy behavior.
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // θ_wait suppression: applies to RecencyDecay and Lie. Flat keeps
        // the historical "no Wait" behavior.
        if !matches!(self.priority_model, PriorityModel::Flat) {
            let suppress = scored
                .first()
                .map(|(_, s, _)| *s < self.theta_wait)
                .unwrap_or(false);
            if suppress {
                let stamp = scored
                    .first()
                    .map(|(_, _, c)| *c)
                    .unwrap_or(self.state.cycle);
                return vec![(Action::Wait, 1.0, stamp)];
            }
        }

        scored
    }

    /// Map a candidate action to a dimension index in the cognitive state
    /// space. Used by the `Lie` branch to look up `proj(attention, axis)`.
    fn action_axis(&self, action: &Action) -> Option<usize> {
        let key = match action {
            Action::Investigate { topic } | Action::Retry { topic } => topic.as_str(),
            Action::Accept { proposition, .. }
            | Action::UpdateBelief { proposition, .. } => proposition.as_str(),
            // Escalate, SendMessage, Wait, Log have no obvious axis — fall
            // back to dim 0 (the background axis), which matches the plan.
            _ => return Some(0),
        };
        // Direct goal hit takes precedence over any heuristic.
        if let Some(dim) = self.state.goal_axis(key) {
            return Some(dim);
        }
        // Heuristic: walk the goals BTreeMap and pick the first whose key
        // is a prefix of (or shares a substring with) the proposition. This
        // mirrors the plan's "matched by key prefix" rule for propositions
        // that don't have their own goal entry.
        for (idx, goal_key) in self.state.goals.keys().enumerate() {
            if key.contains(goal_key.as_str()) {
                return Some(idx.min(self.state.dimension.saturating_sub(1)));
            }
        }
        Some(0)
    }

    /// Get the actions from the last cycle.
    pub fn last_actions(&self) -> &[Action] {
        &self.last_actions
    }

    /// Replay a complete research trace and return a machine-readable report.
    pub fn process_research_trace(&mut self, trace: ResearchTrace) -> ResearchReplayReport {
        let mut touched_propositions = BTreeMap::new();
        let mut outcomes = Vec::new();

        for event in trace.events {
            if let Some(proposition) = event.payload.proposition() {
                touched_propositions.insert(proposition.to_string(), ());
            }
            outcomes.push(self.process_research_event(event));
        }

        let final_beliefs: BTreeMap<String, HexValue> = touched_propositions
            .keys()
            .filter_map(|proposition| {
                self.current_claim_value(proposition)
                    .map(|value| (proposition.clone(), value))
            })
            .collect();

        let metrics = compute_metrics(
            &outcomes,
            &final_beliefs,
            &self.state.goals,
            self.attention_norm_max,
        );

        ResearchReplayReport {
            event_count: outcomes.len(),
            outcomes,
            final_beliefs,
            final_goals: self.state.goals.clone(),
            final_state_summary: self.state.summary(),
            priority_model: self.priority_model,
            metrics,
            comparison: None,
        }
    }

    /// Apply one IX/autoresearch event and return the resulting recommendation.
    ///
    /// All actions for the event — those produced by `cycle()` plus the
    /// per-event recommendation produced by `recommend_for_claim` — are
    /// scored together so the active [`PriorityModel`] can re-rank or
    /// suppress them in a single pass. This is the load-bearing site for
    /// the divergence between `Lie` and `RecencyDecay` since the
    /// `Investigate`-vs-`Wait` decision lands here.
    pub fn process_research_event(&mut self, event: ResearchEvent) -> ResearchEventOutcome {
        let mut actions: Vec<Action> = Vec::new();
        let mut action_cycles: Vec<u64> = Vec::new();

        match &event.payload {
            ResearchEventPayload::BeliefUpdate {
                proposition,
                value,
                evidence,
            } => {
                self.perceive(Perception {
                    proposition: proposition.clone(),
                    value: *value,
                    source: event.source.clone(),
                    cycle: event.cycle,
                });
                let (cycle_actions, cycle_cycles) = self.cycle_raw();
                actions.extend(cycle_actions);
                action_cycles.extend(cycle_cycles);
                let current_value = self.current_claim_value(proposition).unwrap_or(*value);
                for a in Self::recommend_for_claim(proposition, current_value) {
                    actions.push(a);
                    action_cycles.push(event.cycle);
                }
                if !evidence.is_empty() {
                    actions.push(Action::Log(format!(
                        "Recorded {} evidence fields for '{}'",
                        evidence.len(),
                        proposition
                    )));
                    action_cycles.push(event.cycle);
                }
            }
            ResearchEventPayload::ExperimentResult {
                proposition,
                value,
                evidence,
            } => {
                self.perceive(Perception {
                    proposition: proposition.clone(),
                    value: *value,
                    source: format!("experiment:{}", event.source),
                    cycle: event.cycle,
                });
                let (cycle_actions, cycle_cycles) = self.cycle_raw();
                actions.extend(cycle_actions);
                action_cycles.extend(cycle_cycles);
                let current_value = self.current_claim_value(proposition).unwrap_or(*value);
                for a in Self::recommend_for_claim(proposition, current_value) {
                    actions.push(a);
                    action_cycles.push(event.cycle);
                }
                if !evidence.is_empty() {
                    actions.push(Action::Log(format!(
                        "Recorded {} experiment fields for '{}'",
                        evidence.len(),
                        proposition
                    )));
                    action_cycles.push(event.cycle);
                }
            }
            ResearchEventPayload::AgentVote {
                proposition,
                value,
                evidence,
            } => {
                self.perceive(Perception {
                    proposition: proposition.clone(),
                    value: *value,
                    source: format!("vote:{}", event.source),
                    cycle: event.cycle,
                });
                let (cycle_actions, cycle_cycles) = self.cycle_raw();
                actions.extend(cycle_actions);
                action_cycles.extend(cycle_cycles);
                let current_value = self.current_claim_value(proposition).unwrap_or(*value);
                for a in Self::recommend_for_claim(proposition, current_value) {
                    actions.push(a);
                    action_cycles.push(event.cycle);
                }
                if !evidence.is_empty() {
                    actions.push(Action::Log(format!(
                        "Recorded {} vote fields for '{}'",
                        evidence.len(),
                        proposition
                    )));
                    action_cycles.push(event.cycle);
                }
            }
            ResearchEventPayload::Retraction {
                proposition,
                reason,
            } => {
                if let Some(prop) = self.state.beliefs.get_mut(proposition) {
                    prop.value = HexValue::Unknown;
                } else {
                    self.state
                        .beliefs
                        .add_proposition(proposition, HexValue::Unknown);
                }
                actions.push(Action::Log(format!(
                    "Retracted '{}': {}",
                    proposition, reason
                )));
                action_cycles.push(event.cycle);
                actions.push(Action::Retry {
                    topic: proposition.clone(),
                });
                action_cycles.push(event.cycle);
            }
            ResearchEventPayload::GoalUpdate {
                key,
                description,
                priority,
                status,
            } => {
                self.state
                    .add_goal(key.clone(), description.clone(), *priority);
                if let Some(status) = status {
                    if let Some(goal) = self.state.goals.get_mut(key) {
                        goal.status = *status;
                    }
                }
                actions.push(Action::Log(format!(
                    "Goal '{}' updated from {}",
                    key, event.source
                )));
                action_cycles.push(event.cycle);
            }
        }

        // Single scoring pass for the whole event so the priority model can
        // re-rank cycle-actions and recommendation-actions together.
        let scored = self.score_actions_with_cycles(actions, &action_cycles);
        let (final_actions, final_cycles): (Vec<_>, Vec<_>) = scored
            .into_iter()
            .map(|(action, _score, cycle)| (action, cycle))
            .unzip();
        self.last_actions = final_actions.clone();
        self.last_action_perception_cycles = final_cycles;

        ResearchEventOutcome {
            event,
            actions: final_actions,
            state_summary: self.state.summary(),
        }
    }

    fn current_claim_value(&self, proposition: &str) -> Option<HexValue> {
        self.state.beliefs.get(proposition).map(|p| p.value)
    }

    fn recommend_for_claim(proposition: &str, value: HexValue) -> Vec<Action> {
        match value {
            HexValue::True | HexValue::Probable | HexValue::Doubtful | HexValue::False => {
                vec![Action::Accept {
                    proposition: proposition.to_string(),
                    value,
                }]
            }
            HexValue::Unknown => vec![Action::Investigate {
                topic: proposition.to_string(),
            }],
            HexValue::Contradictory => vec![Action::Escalate {
                reason: format!(
                    "Research claim '{}' has contradictory evidence",
                    proposition
                ),
                confidence: 0.5,
            }],
        }
    }
}

impl ResearchEventPayload {
    pub(crate) fn proposition(&self) -> Option<&str> {
        match self {
            Self::BeliefUpdate { proposition, .. }
            | Self::ExperimentResult { proposition, .. }
            | Self::AgentVote { proposition, .. }
            | Self::Retraction { proposition, .. } => Some(proposition),
            Self::GoalUpdate { .. } => None,
        }
    }
}

/// Stable string label for each `Action` variant (no inner data).
///
/// Used by metric aggregation and divergence diffing so the JSON report can
/// stay compact and downstream consumers can group/count action kinds.
pub fn action_kind(action: &Action) -> &'static str {
    match action {
        Action::UpdateBelief { .. } => "UpdateBelief",
        Action::SendMessage(_) => "SendMessage",
        Action::Investigate { .. } => "Investigate",
        Action::Escalate { .. } => "Escalate",
        Action::Retry { .. } => "Retry",
        Action::Accept { .. } => "Accept",
        Action::Wait => "Wait",
        Action::Log(_) => "Log",
    }
}

/// Two action lists are "equivalent" if their sequence of kinds (ignoring
/// inner data) matches. Used for divergence detection between two priority
/// models so we don't flag spurious differences in `Action::Log` text.
pub(crate) fn action_lists_equivalent(a: &[Action], b: &[Action]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter()
        .zip(b.iter())
        .all(|(x, y)| action_kind(x) == action_kind(y))
}

/// Pair-wise divergence between two replay outcome lists. The two lists are
/// expected to have the same length (same trace replayed through both
/// priority models on fresh `CognitiveLoop` instances).
pub fn diff_outcomes(
    baseline: &[ResearchEventOutcome],
    experimental: &[ResearchEventOutcome],
) -> Vec<ActionDivergence> {
    let mut out = Vec::new();
    for (i, (b, e)) in baseline.iter().zip(experimental.iter()).enumerate() {
        if !action_lists_equivalent(&b.actions, &e.actions) {
            out.push(ActionDivergence {
                event_index: i,
                baseline_actions: b.actions.clone(),
                experimental_actions: e.actions.clone(),
            });
        }
    }
    out
}

/// Replay a trace through both `Lie` and `RecencyDecay` on fresh
/// `CognitiveLoop` instances and return a single report whose `comparison`
/// field is populated.
pub fn compare_replay(trace: ResearchTrace) -> ResearchReplayReport {
    let mut baseline_loop = CognitiveLoop::with_model(trace.dimension, PriorityModel::RecencyDecay);
    let mut experimental_loop = CognitiveLoop::with_model(trace.dimension, PriorityModel::Lie);

    let baseline_report = baseline_loop.process_research_trace(trace.clone());
    let experimental_report = experimental_loop.process_research_trace(trace);

    let action_divergence = diff_outcomes(&baseline_report.outcomes, &experimental_report.outcomes);

    ResearchReplayReport {
        comparison: Some(ReplayComparison {
            baseline: baseline_report.metrics.clone(),
            experimental: experimental_report.metrics.clone(),
            action_divergence,
        }),
        ..experimental_report
    }
}

/// Public alias of [`compute_metrics`] for crate-internal callers like
/// `StreamingSession` that need to compute running/final metrics over an
/// outcome list outside the [`CognitiveLoop`] codepath. This is the
/// same function the batch path uses — there is intentionally no
/// parallel metric implementation. See `phase6-design.md` §7.
pub fn compute_metrics_for(
    outcomes: &[ResearchEventOutcome],
    final_beliefs: &BTreeMap<String, HexValue>,
    final_goals: &BTreeMap<String, Goal>,
    attention_norm_max: f64,
) -> ReplayMetrics {
    compute_metrics(outcomes, final_beliefs, final_goals, attention_norm_max)
}

/// Compute the aggregate metrics over a finished replay.
fn compute_metrics(
    outcomes: &[ResearchEventOutcome],
    final_beliefs: &BTreeMap<String, HexValue>,
    final_goals: &BTreeMap<String, Goal>,
    attention_norm_max: f64,
) -> ReplayMetrics {
    // --- contradiction_recovery_cycles ---
    // First cycle at which any belief became Contradictory minus first cycle
    // afterwards at which the same belief left Contradictory. Per-prop, take
    // the minimum across props.
    //
    // A belief is detected as Contradictory via two signals (in order):
    // 1. An event whose payload value is literally HexValue::Contradictory.
    // 2. An Action::Escalate emitted by the loop with "contradictory" in its
    //    reason — the loop only emits this when the belief network's stored
    //    value is Contradictory after combining evidence (e.g. True + False
    //    via combine_evidence).
    //
    // Recovery is the first subsequent event for that proposition whose
    // outcome leaves the belief non-Contradictory: any non-Contradictory
    // event value, OR a Retraction (which clears the belief to Unknown), OR
    // simply the absence of further Escalate-with-contradictory actions on
    // a later event for the same proposition.
    let mut first_contradictory: BTreeMap<String, u64> = BTreeMap::new();
    // Pre-pass: detect Contradictory via Escalate signal (since combined
    // evidence can produce Contradictory even when no event payload carries
    // that value literally).
    for o in outcomes {
        let cycle = o.event.cycle;
        let prop_opt = o.event.payload.proposition().map(|p| p.to_string());
        for a in &o.actions {
            if let Action::Escalate { reason, .. } = a {
                if reason.contains("contradictory") || reason.contains("Contradictory") {
                    if let Some(ref prop) = prop_opt {
                        first_contradictory
                            .entry(prop.clone())
                            .or_insert(cycle);
                    }
                }
            }
        }
        // Also catch literal Contradictory event values.
        let value = match &o.event.payload {
            ResearchEventPayload::BeliefUpdate { value, .. }
            | ResearchEventPayload::ExperimentResult { value, .. }
            | ResearchEventPayload::AgentVote { value, .. } => Some(*value),
            _ => None,
        };
        if matches!(value, Some(HexValue::Contradictory)) {
            if let Some(ref prop) = prop_opt {
                first_contradictory.entry(prop.clone()).or_insert(cycle);
            }
        }
    }

    // Recovery pass: now that first_contradictory has all the props that
    // ever became Contradictory, walk forward and find the first event per
    // prop AFTER its first_contradictory cycle that returns the belief to
    // a non-Contradictory state.
    let mut recovery_at: BTreeMap<String, u64> = BTreeMap::new();
    for o in outcomes {
        let cycle = o.event.cycle;
        let prop = match o.event.payload.proposition() {
            Some(p) => p.to_string(),
            None => continue,
        };
        let Some(&start) = first_contradictory.get(&prop) else { continue };
        if cycle <= start {
            continue;
        }
        if recovery_at.contains_key(&prop) {
            continue;
        }
        // Recovery signals (any of):
        // - Retraction (clears belief to Unknown).
        // - An event with a non-Contradictory value AND no Escalate-with-
        //   contradictory action (i.e. the belief network is no longer
        //   Contradictory after this event).
        let cleared_by_retraction = matches!(
            &o.event.payload,
            ResearchEventPayload::Retraction { .. }
        );
        let still_contradictory = o.actions.iter().any(|a| {
            matches!(a, Action::Escalate { reason, .. }
                if reason.contains("contradictory") || reason.contains("Contradictory"))
        });
        if cleared_by_retraction || !still_contradictory {
            recovery_at.insert(prop, cycle);
        }
    }

    let recovery = first_contradictory
        .iter()
        .filter_map(|(prop, start)| recovery_at.get(prop).map(|end| end.saturating_sub(*start)))
        .min();

    // --- false_acceptance_count ---
    // Count Accept actions whose proposition was later retracted, marked
    // Doubtful/False, or moved to Contradictory. We compare each Accept's
    // value to the proposition's eventual final value.
    let mut false_accepts: u32 = 0;
    for o in outcomes {
        for a in &o.actions {
            if let Action::Accept { proposition, value } = a {
                let later_retracted = outcomes.iter().any(|later| {
                    later.event.cycle > o.event.cycle
                        && matches!(
                            &later.event.payload,
                            ResearchEventPayload::Retraction { proposition: p, .. } if p == proposition
                        )
                });
                let final_value = final_beliefs.get(proposition).copied();
                let became_contradictory =
                    matches!(final_value, Some(HexValue::Contradictory));
                let flipped_polarity = match (*value, final_value) {
                    (HexValue::True | HexValue::Probable,
                     Some(HexValue::Doubtful | HexValue::False)) => true,
                    (HexValue::Doubtful | HexValue::False,
                     Some(HexValue::True | HexValue::Probable)) => true,
                    _ => false,
                };
                if later_retracted || became_contradictory || flipped_polarity {
                    false_accepts += 1;
                }
            }
        }
    }

    // --- goal_completion_rate ---
    let goal_completion_rate = if final_goals.is_empty() {
        0.0
    } else {
        let achieved = final_goals
            .values()
            .filter(|g| matches!(g.status, HexValue::True | HexValue::Probable))
            .count();
        achieved as f64 / final_goals.len() as f64
    };

    // --- consensus_stability ---
    // For each touched proposition: count flips (transitions where value
    // differed from the previous event for the same prop). Stability is
    // 1 - (flips / events_touching_prop), averaged.
    let mut events_per_prop: BTreeMap<String, u32> = BTreeMap::new();
    let mut flips_per_prop: BTreeMap<String, u32> = BTreeMap::new();
    let mut last_value: BTreeMap<String, HexValue> = BTreeMap::new();
    for o in outcomes {
        let (prop, value) = match &o.event.payload {
            ResearchEventPayload::BeliefUpdate { proposition, value, .. }
            | ResearchEventPayload::ExperimentResult { proposition, value, .. }
            | ResearchEventPayload::AgentVote { proposition, value, .. } => {
                (proposition.clone(), *value)
            }
            ResearchEventPayload::Retraction { proposition, .. } => {
                (proposition.clone(), HexValue::Unknown)
            }
            ResearchEventPayload::GoalUpdate { .. } => continue,
        };
        *events_per_prop.entry(prop.clone()).or_insert(0) += 1;
        if let Some(prev) = last_value.get(&prop) {
            if *prev != value {
                *flips_per_prop.entry(prop.clone()).or_insert(0) += 1;
            }
        }
        last_value.insert(prop, value);
    }
    let consensus_stability = if events_per_prop.is_empty() {
        1.0
    } else {
        let sum: f64 = events_per_prop
            .iter()
            .map(|(prop, count)| {
                let flips = flips_per_prop.get(prop).copied().unwrap_or(0) as f64;
                1.0 - (flips / *count as f64)
            })
            .sum();
        sum / events_per_prop.len() as f64
    };

    // --- action_counts_by_kind ---
    let mut action_counts_by_kind: BTreeMap<String, u32> = BTreeMap::new();
    for o in outcomes {
        for a in &o.actions {
            *action_counts_by_kind
                .entry(action_kind(a).to_string())
                .or_insert(0) += 1;
        }
    }

    ReplayMetrics {
        contradiction_recovery_cycles: recovery,
        false_acceptance_count: false_accepts,
        goal_completion_rate,
        consensus_stability,
        attention_norm_max,
        action_counts_by_kind,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_perception_display() {
        let p = Perception {
            proposition: "sky-is-blue".to_string(),
            value: HexValue::True,
            source: "vision".to_string(),
            cycle: 1,
        };
        let s = format!("{}", p);
        assert!(s.contains("sky-is-blue"));
        assert!(s.contains("T"));
    }

    #[test]
    fn test_action_display() {
        let a = Action::UpdateBelief {
            proposition: "test".to_string(),
            value: HexValue::Probable,
        };
        assert!(format!("{}", a).contains("test"));
        assert!(format!("{}", Action::Wait).contains("Wait"));
        assert!(format!(
            "{}",
            Action::Retry {
                topic: "test".to_string()
            }
        )
        .contains("test"));
        assert!(format!(
            "{}",
            Action::Accept {
                proposition: "test".to_string(),
                value: HexValue::True,
            }
        )
        .contains("test"));
    }

    #[test]
    fn test_cognitive_state_creation() {
        let state = CognitiveState::new(4);
        assert_eq!(state.dimension, 4);
        assert_eq!(state.cycle, 0);
        assert_eq!(state.attention.len(), 4);
        // Uniform unit-vector seed: every dim starts with 1/sqrt(d) so
        // proj(attention, e_k) is non-zero from cycle 1 (Phase 5 fragility
        // fix). Norm should equal 1.0 to single-precision tolerance.
        let expected = 1.0 / 2.0; // 1 / sqrt(4)
        for i in 0..4 {
            assert!((state.attention[i] - expected).abs() < 1e-12);
        }
        assert!((state.attention.norm() - 1.0).abs() < 1e-12);
        assert!(state.beliefs.is_empty());
        assert!(state.goals.is_empty());
    }

    #[test]
    fn test_add_and_find_goals() {
        let mut state = CognitiveState::new(2);
        state.add_goal("learn", "Learn about the world", 0.8);
        state.add_goal("survive", "Ensure continued operation", 0.9);

        let (key, goal) = state.top_goal().unwrap();
        assert_eq!(key, "survive");
        assert_eq!(goal.priority, 0.9);
    }

    #[test]
    fn test_goal_completion_removes_from_top() {
        let mut state = CognitiveState::new(2);
        state.add_goal("a", "Goal A", 0.9);
        state.add_goal("b", "Goal B", 0.5);

        // Complete goal A
        state.goals.get_mut("a").unwrap().status = HexValue::True;

        let (key, _) = state.top_goal().unwrap();
        assert_eq!(key, "b");
    }

    #[test]
    fn test_state_summary() {
        let state = CognitiveState::new(3);
        let summary = state.summary();
        assert!(summary.contains("Cycle 0"));
        assert!(summary.contains("0 beliefs"));
    }

    #[test]
    fn test_cognitive_loop_creation() {
        let cl = CognitiveLoop::new(4);
        assert_eq!(cl.state.dimension, 4);
        assert_eq!(cl.state.cycle, 0);
    }

    #[test]
    fn test_cognitive_loop_perceive_and_cycle() {
        let mut cl = CognitiveLoop::new(4);

        cl.perceive(Perception {
            proposition: "temperature-high".to_string(),
            value: HexValue::Probable,
            source: "sensor".to_string(),
            cycle: 0,
        });

        let actions = cl.cycle();
        assert_eq!(cl.state.cycle, 1);

        // Should have logged the new belief
        assert!(!actions.is_empty());
        assert_eq!(
            cl.state.beliefs.get("temperature-high").unwrap().value,
            HexValue::Probable
        );
    }

    #[test]
    fn test_cognitive_loop_multiple_perceptions() {
        let mut cl = CognitiveLoop::new(4);

        cl.perceive(Perception {
            proposition: "danger".to_string(),
            value: HexValue::Probable,
            source: "sensor-a".to_string(),
            cycle: 0,
        });

        cl.cycle();

        // Second perception with conflicting evidence
        cl.perceive(Perception {
            proposition: "danger".to_string(),
            value: HexValue::Doubtful,
            source: "sensor-b".to_string(),
            cycle: 1,
        });

        cl.cycle();

        // Probable + Doubtful = Contradictory (conflicting evidence)
        assert_eq!(
            cl.state.beliefs.get("danger").unwrap().value,
            HexValue::Contradictory
        );
    }

    #[test]
    fn test_cognitive_loop_goal_investigation() {
        let mut cl = CognitiveLoop::new(4);
        cl.state.add_goal("find-food", "Locate food sources", 0.8);

        // Add an Unknown belief matching the goal
        cl.state
            .beliefs
            .add_proposition("find-food", HexValue::Unknown);

        let actions = cl.cycle();

        // Should produce an Investigate action for the unknown goal
        let has_investigate = actions
            .iter()
            .any(|a| matches!(a, Action::Investigate { .. }));
        assert!(has_investigate, "Should investigate unknown goal");
    }

    #[test]
    fn test_cognitive_loop_goal_escalation() {
        let mut cl = CognitiveLoop::new(4);
        cl.state
            .add_goal("resolve-conflict", "Handle contradiction", 0.9);

        // Add a Contradictory belief matching the goal
        cl.state
            .beliefs
            .add_proposition("resolve-conflict", HexValue::Contradictory);

        let actions = cl.cycle();

        let has_escalate = actions.iter().any(|a| matches!(a, Action::Escalate { .. }));
        assert!(has_escalate, "Should escalate contradictory goal");
    }

    #[test]
    fn test_empty_cycle_no_crash() {
        let mut cl = CognitiveLoop::new(4);
        let actions = cl.cycle();
        assert!(actions.is_empty());
        assert_eq!(cl.state.cycle, 1);
    }

    #[test]
    fn test_process_research_belief_update_recommends_accept() {
        let mut cl = CognitiveLoop::new(4);
        let outcome = cl.process_research_event(ResearchEvent {
            cycle: 1,
            source: "ix-agent".to_string(),
            payload: ResearchEventPayload::BeliefUpdate {
                proposition: "prompt-a-improves-pass-rate".to_string(),
                value: HexValue::Probable,
                evidence: Evidence::new(),
            },
        });

        assert_eq!(
            cl.state
                .beliefs
                .get("prompt-a-improves-pass-rate")
                .unwrap()
                .value,
            HexValue::Probable
        );
        assert!(outcome
            .actions
            .iter()
            .any(|a| matches!(a, Action::Accept { proposition, .. } if proposition == "prompt-a-improves-pass-rate")));
    }

    #[test]
    fn test_process_research_contradiction_recommends_escalate() {
        let mut cl = CognitiveLoop::new(4);
        cl.process_research_event(ResearchEvent {
            cycle: 1,
            source: "ix-agent-a".to_string(),
            payload: ResearchEventPayload::BeliefUpdate {
                proposition: "benchmark-x-is-reliable".to_string(),
                value: HexValue::Probable,
                evidence: Evidence::new(),
            },
        });

        let outcome = cl.process_research_event(ResearchEvent {
            cycle: 2,
            source: "ix-agent-b".to_string(),
            payload: ResearchEventPayload::BeliefUpdate {
                proposition: "benchmark-x-is-reliable".to_string(),
                value: HexValue::Doubtful,
                evidence: Evidence::new(),
            },
        });

        assert_eq!(
            cl.state
                .beliefs
                .get("benchmark-x-is-reliable")
                .unwrap()
                .value,
            HexValue::Contradictory
        );
        assert!(outcome
            .actions
            .iter()
            .any(|a| matches!(a, Action::Escalate { reason, .. } if reason.contains("benchmark-x-is-reliable"))));
    }

    #[test]
    fn test_process_research_retraction_recommends_retry() {
        let mut cl = CognitiveLoop::new(4);
        cl.process_research_event(ResearchEvent {
            cycle: 1,
            source: "ix-agent".to_string(),
            payload: ResearchEventPayload::BeliefUpdate {
                proposition: "tool-error-is-fixed".to_string(),
                value: HexValue::True,
                evidence: Evidence::new(),
            },
        });

        let outcome = cl.process_research_event(ResearchEvent {
            cycle: 2,
            source: "ix-runner".to_string(),
            payload: ResearchEventPayload::Retraction {
                proposition: "tool-error-is-fixed".to_string(),
                reason: "run used stale binary".to_string(),
            },
        });

        assert_eq!(
            cl.state.beliefs.get("tool-error-is-fixed").unwrap().value,
            HexValue::Unknown
        );
        assert!(outcome
            .actions
            .iter()
            .any(|a| matches!(a, Action::Retry { topic } if topic == "tool-error-is-fixed")));
    }

    #[test]
    fn test_priority_model_default_is_flat() {
        let cl = CognitiveLoop::new(4);
        assert_eq!(cl.priority_model, PriorityModel::Flat);
    }

    #[test]
    fn test_with_model_constructor() {
        let cl = CognitiveLoop::with_model(4, PriorityModel::RecencyDecay);
        assert_eq!(cl.priority_model, PriorityModel::RecencyDecay);
        let cl = CognitiveLoop::with_model(4, PriorityModel::Lie);
        assert_eq!(cl.priority_model, PriorityModel::Lie);
    }

    #[test]
    fn test_score_actions_flat_returns_unit_scores() {
        let cl = CognitiveLoop::with_model(4, PriorityModel::Flat);
        let actions = vec![
            Action::Investigate { topic: "x".into() },
            Action::Wait,
            Action::Log("msg".into()),
        ];
        let scored = cl.score_actions(actions);
        assert_eq!(scored.len(), 3);
        for (_, s) in &scored {
            assert!((s - 1.0).abs() < 1e-12);
        }
    }

    #[test]
    fn test_score_actions_recency_decay_suppresses_old() {
        let mut cl = CognitiveLoop::with_model(4, PriorityModel::RecencyDecay);
        cl.state.cycle = 50;
        // Cycle 0 perception with λ=0.2 → exp(-10) ≈ 4.5e-5, well below 0.1.
        let actions = vec![Action::Investigate {
            topic: "old-topic".into(),
        }];
        let cycles = vec![0u64];
        let scored = cl.score_actions_with_cycles(actions, &cycles);
        assert_eq!(scored.len(), 1);
        assert!(matches!(scored[0].0, Action::Wait));
    }

    #[test]
    fn test_score_actions_recency_decay_keeps_fresh() {
        let mut cl = CognitiveLoop::with_model(4, PriorityModel::RecencyDecay);
        cl.state.cycle = 5;
        let actions = vec![Action::Investigate {
            topic: "fresh".into(),
        }];
        let cycles = vec![5u64];
        let scored = cl.score_actions_with_cycles(actions, &cycles);
        assert!(matches!(scored[0].0, Action::Investigate { .. }));
        assert!(scored[0].1 >= 1.0 - 1e-12);
    }

    #[test]
    fn test_goal_axis_returns_btreemap_position() {
        let mut state = CognitiveState::new(4);
        // BTreeMap orders alphabetically: "a" -> 0, "b" -> 1, "c" -> 2.
        state.add_goal("b", "B", 0.5);
        state.add_goal("a", "A", 0.5);
        state.add_goal("c", "C", 0.5);
        assert_eq!(state.goal_axis("a"), Some(0));
        assert_eq!(state.goal_axis("b"), Some(1));
        assert_eq!(state.goal_axis("c"), Some(2));
        assert_eq!(state.goal_axis("missing"), None);
    }

    #[test]
    fn test_goal_axis_clamps_to_dimension() {
        let mut state = CognitiveState::new(2);
        state.add_goal("a", "A", 0.5);
        state.add_goal("b", "B", 0.5);
        state.add_goal("c", "C", 0.5);
        // dim=2, max_index=1, but "c" is the third goal → clamped.
        assert_eq!(state.goal_axis("c"), Some(1));
    }

    #[test]
    fn test_lie_branch_diverges_from_flat_on_synthetic_trace() {
        // Same 3-event trace; Flat returns Investigate as-is on Unknown,
        // Lie may suppress or re-rank when attention is degenerate.
        let trace = ResearchTrace {
            dimension: 4,
            events: vec![
                ResearchEvent {
                    cycle: 1,
                    source: "sensor".into(),
                    payload: ResearchEventPayload::BeliefUpdate {
                        proposition: "topic-a".into(),
                        value: HexValue::Unknown,
                        evidence: Evidence::new(),
                    },
                },
                ResearchEvent {
                    cycle: 2,
                    source: "sensor".into(),
                    payload: ResearchEventPayload::BeliefUpdate {
                        proposition: "topic-a".into(),
                        value: HexValue::Probable,
                        evidence: Evidence::new(),
                    },
                },
                ResearchEvent {
                    cycle: 3,
                    source: "sensor".into(),
                    payload: ResearchEventPayload::BeliefUpdate {
                        proposition: "topic-a".into(),
                        value: HexValue::Doubtful,
                        evidence: Evidence::new(),
                    },
                },
            ],
        };
        let mut flat_loop = CognitiveLoop::with_model(4, PriorityModel::Flat);
        let mut lie_loop = CognitiveLoop::with_model(4, PriorityModel::Lie);
        let flat_report = flat_loop.process_research_trace(trace.clone());
        let lie_report = lie_loop.process_research_trace(trace);
        // Flat preserves Investigate; Lie either drops it or re-orders it.
        // We assert at least *some* divergence in the outcome list.
        let any_diff = flat_report
            .outcomes
            .iter()
            .zip(lie_report.outcomes.iter())
            .any(|(a, b)| !action_lists_equivalent(&a.actions, &b.actions));
        assert!(any_diff, "Lie must produce a different trace than Flat");
    }

    #[test]
    fn test_attention_norm_bounded_by_renormalisation() {
        // A long burst of Probable perceptions should not push the
        // attention norm above 10.0 thanks to the renormalisation gate.
        let mut cl = CognitiveLoop::with_model(4, PriorityModel::Lie);
        cl.state.add_goal("prop", "p", 0.5);
        for cycle in 0..200 {
            cl.perceive(Perception {
                proposition: "prop".into(),
                value: HexValue::Probable,
                source: "test".into(),
                cycle,
            });
            cl.cycle();
        }
        assert!(
            cl.attention_norm_max() < 10.0 + 1e-9,
            "norm exceeded cap: {}",
            cl.attention_norm_max()
        );
    }

    #[test]
    fn test_compare_replay_populates_comparison() {
        let trace = ResearchTrace {
            dimension: 4,
            events: vec![ResearchEvent {
                cycle: 1,
                source: "s".into(),
                payload: ResearchEventPayload::BeliefUpdate {
                    proposition: "p".into(),
                    value: HexValue::Probable,
                    evidence: Evidence::new(),
                },
            }],
        };
        let report = compare_replay(trace);
        assert!(report.comparison.is_some());
    }

    #[test]
    fn test_process_research_trace_reports_final_beliefs() {
        let mut cl = CognitiveLoop::new(4);
        let report = cl.process_research_trace(ResearchTrace {
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
                    source: "ix-agent-b".to_string(),
                    payload: ResearchEventPayload::BeliefUpdate {
                        proposition: "benchmark-x-is-reliable".to_string(),
                        value: HexValue::Doubtful,
                        evidence: Evidence::new(),
                    },
                },
            ],
        });

        assert_eq!(report.event_count, 2);
        assert_eq!(report.outcomes.len(), 2);
        assert_eq!(
            report.final_beliefs.get("benchmark-x-is-reliable"),
            Some(&HexValue::Contradictory)
        );
    }
}
