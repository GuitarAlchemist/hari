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
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

// ---------------------------------------------------------------------------
// Phase 6 — IX autoresearch streaming integration
// ---------------------------------------------------------------------------
pub mod protocol;
pub mod session;

pub use protocol::{
    InitialAgent, InitialGoal, RecommendationResponse, Request, Response, SessionConfig,
    ShadowCompare,
};
pub use session::{StreamingSession, TraceRecorder};

// ---------------------------------------------------------------------------
// Jarvis Track J2 — forecast records (the prediction half of the world
// model). Contract: ga:docs/contracts/2026-07-02-hari-forecast-record.contract.md
// ---------------------------------------------------------------------------
pub mod forecast;

pub use forecast::{ForecastRecord, Observable as ForecastObservable, Outcome as ForecastOutcome};

// ---------------------------------------------------------------------------
// Giskard Track G2 — agent reliability model over GA's PR grade cards
// (mentalics-of-agents; input schema ga:state/quality/pr-grades/SCHEMA.json).
// ---------------------------------------------------------------------------
pub mod reliability;

pub use reliability::{GradeCard, ReliabilityReport};

// ---------------------------------------------------------------------------
// Giskard Track G1 (ops face) — operator model (mentalics-of-the-human): a
// seen/acknowledged/stale registry over signals surfaced to the operator,
// plus the "don't re-notify what's already acknowledged" decision. Substrate
// half only; the GA producer/consumer halves are a later slice.
// ---------------------------------------------------------------------------
pub mod operator_model;

pub use operator_model::{OperatorEvent, SignalState};

// ---------------------------------------------------------------------------
// Cross-session source reliability (issue #14) — trust earned by outcomes.
// Generalises G2 (agents ← PR-grade cards) to any `source` ← the fate of the
// claims it made in replayed IX traces, accumulated across sessions. Design:
// docs/research/2026-07-20-source-reliability-design.md; contract:
// docs/contracts/source-reliability-summary.contract.md (v0.1 DRAFT).
// Read-only entrenchment ordering; no automated trust change (max_autonomy:
// draft — full trust-calibration integration awaits owner review).
// ---------------------------------------------------------------------------
pub mod source_reliability;

pub use source_reliability::{
    SourceOutcomeRecord, SourceReliabilityEntry, SourceReliabilityReport,
};

// ---------------------------------------------------------------------------
// Subjective Logic prior-art baseline (Jøsang 2016) — see
// `docs/research/prior-art-survey.md` §4.
// ---------------------------------------------------------------------------
pub mod subjective_logic;

pub use subjective_logic::{
    compare_replay_three_way, process_research_trace_subjective_logic, Opinion,
    ReplayComparisonThreeWay, SubjectiveLogicConfig, ThreeWayDivergence, ThreeWayReplayReport,
};

// ---------------------------------------------------------------------------
// Evidence lineage export (issue #15) — portable, JSON-first externalisation
// of the provenance Hari already tracks internally. Contract + schema:
// docs/contracts/hari-evidence-lineage.contract.md (v0.1 DRAFT). Shape anchor
// only; automatic export from a live ResearchReplayReport is a later slice.
// ---------------------------------------------------------------------------
pub mod lineage;

pub use lineage::{LineageBundle, LineageEdge, LineageNode, LineageRun};

// ---------------------------------------------------------------------------
// Paired-decision scoring — the #35 §5.1 primary metric. Ground truth lives in
// a sidecar rather than on ResearchEvent, so the Hari↔IX boundary carries no
// eval scaffolding and every existing trace replays byte-identically.
// ---------------------------------------------------------------------------
pub mod paired_eval;

pub use paired_eval::{
    score_false_rejections, score_paired, score_paired_three_way, ClaimLabel, ClaimOutcome,
    DecisionLabel, ExpectedDecision, FalseRejectionScore, PairDefect, PairOutcome, PairSeparation,
    PairedArm, PairedFixture, PairedScore, ThreeWayPairedReport,
};

// ---------------------------------------------------------------------------
// PriorityModel — strategy enum used by score_actions
// ---------------------------------------------------------------------------

/// Strategy for prioritising candidate actions in [`CognitiveLoop::score_actions`].
///
/// **Default**: `RecencyDecay` (post-Phase-5 substrate decision, see
/// `ROADMAP.md` §"Cognition Substrate Choice"). Lie's negative result on
/// `false_acceptance_count` against the SL prior-art baseline made
/// privileging it as a default no longer defensible; `RecencyDecay` is
/// the simplest non-Lie option that ties or beats Lie on most fixtures
/// and stays in-tree.
///
/// `Flat` (priority 1.0 for every action, original order) remains
/// available for ablation and for fixtures where production order
/// matters. `Lie` stays in the codebase as an opt-in research knob —
/// the experimental Lie-algebra path is no longer privileged but is
/// preserved so its instrumented attributes (interpretability,
/// continuity, commutativity) can still be studied. `SubjectiveLogic`
/// routes around the action-scoring abstraction entirely (it has its
/// own Opinion-fusion decision pipeline) — it's reached via a
/// short-circuit branch at the top of `process_research_event`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PriorityModel {
    /// All actions get priority 1.0; original order preserved.
    /// Pre-Phase-5 default; now used for ablation only.
    Flat,
    /// `priority = exp(-lambda * (current_cycle - perception_cycle))`.
    /// **Default** since the Phase 5 substrate decision.
    #[default]
    RecencyDecay,
    /// `priority = base * (1 + alpha * proj(attention, action_axis))`.
    /// Opt-in research knob; not the default.
    Lie,
    /// Subjective Logic prior-art baseline (Jøsang 2016). Cumulative
    /// fusion of per-proposition Opinions; recommendations driven by
    /// projected probability and uncertainty thresholds rather than
    /// the hexavalent action-scoring ladder. `process_research_event`
    /// short-circuits to the SL pipeline at the top when this is set,
    /// bypassing perception integration, swarm bridging, belief
    /// propagation, and the `score_actions_with_cycles` step — none
    /// of which the SL pipeline uses or needs.
    SubjectiveLogic,
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
    ///
    /// The optional `retracts` selector (issue #16, belief-revision slice)
    /// names *which* prior evidence to withdraw. When present, the
    /// targeted evidence is tombstoned and the proposition is recomputed
    /// from the surviving evidence (evidence-recompute is authoritative);
    /// when absent, this stays byte-for-byte the pre-slice
    /// whole-proposition reset to `Unknown` (the A/B baseline the
    /// selective path is measured against). The field is additive and
    /// backward-compatible: traces without it deserialize unchanged.
    Retraction {
        proposition: String,
        reason: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retracts: Option<RetractionSelector>,
    },
    /// Retire a whole claim in favour of a successor claim (issue #16).
    ///
    /// Records a directed `proposition → superseded_by` lineage edge and
    /// *freezes* the superseded proposition: its last value stays
    /// inspectable (never reset) but it is marked retired so a consumer
    /// can tell the live chain head from the audit tail. Chains compose
    /// (`A → B → C`); only the head is live. Mirrors the Demerzel
    /// persona-count rot the design cites. Freezing the superseded
    /// claim's *downstream mass* is a no-op in this slice (no dependency
    /// fixture exercises it yet — design §10).
    Supersession {
        proposition: String,
        superseded_by: String,
        reason: String,
    },
    /// Atomic retract-plus-replace (issue #16, belief-revision slice): "that
    /// result was wrong; here is the corrected one."
    ///
    /// A correction is a [`Retraction`](Self::Retraction) of the named prior
    /// evidence (via the same `retracts` selector machinery) *plus* a
    /// [`BeliefUpdate`](Self::BeliefUpdate) of the corrected `(value,
    /// evidence)`, executed as one causally-linked event: the report records
    /// it with `cause = correction`, which a bare `belief_update` cannot
    /// express. Both the tombstoned original and the replacement stay in the
    /// evidence ledger for audit; the belief is recomputed over the survivors
    /// (evidence-recompute is authoritative), so the correction's new value is
    /// merged in exactly as any other source assertion.
    ///
    /// `retracts` is the evidence being corrected. It follows the contract's
    /// "required" shape but is `#[serde(default)]` for robustness: an empty
    /// selector (`{}`) corrects *all* prior evidence for the proposition, the
    /// same wildcard rule as [`RetractionSelector`]. A correction with nothing
    /// to correct degrades to a plain belief update over the ledger.
    Correction {
        proposition: String,
        reason: String,
        #[serde(default)]
        retracts: RetractionSelector,
        value: HexValue,
        #[serde(default)]
        evidence: Evidence,
    },
    /// Withdraw a previously-declared relation edge (issue #16,
    /// belief-revision slice) — the first non-append-only relation path.
    ///
    /// Relations were append-only (see the [`RelationDeclaration`] doc-comment
    /// above). A withdrawal **tombstones** the matching `from -relation-> to`
    /// edge in the [`BeliefNetwork`] via
    /// [`BeliefNetwork::withdraw_relation`] — the edge is *marked withdrawn,
    /// not deleted* (audit-preservation), so it stays inspectable while
    /// propagation skips it. The beliefs the edge induced then dissolve on the
    /// next propagation pass: derivation-only propositions (no direct
    /// evidence) reset to their `Unknown` base and re-derive over the reduced
    /// edge set, the same recompute doctrine as evidence retraction. An
    /// unmatched `(from, to, relation)` triple is a logged no-op.
    RelationWithdrawal {
        from: String,
        to: String,
        relation: hari_lattice::Relation,
        reason: String,
    },
    /// Update or create a goal that should influence future prioritization.
    GoalUpdate {
        key: String,
        description: String,
        priority: f64,
        status: Option<HexValue>,
    },
    /// Declare a logical relation between two propositions in the
    /// belief network, enabling derivation via belief propagation.
    ///
    /// Either or both endpoints are auto-created with `HexValue::Unknown`
    /// if they don't already exist, so IX can declare a relation before
    /// any direct evidence has landed for either proposition. Once
    /// declared, every subsequent belief-changing event triggers a
    /// `propagate_until_stable` pass — derived beliefs flow through the
    /// graph automatically.
    ///
    /// Relations are append-only in this slice: no withdrawal or
    /// reversal. To "undo" a relation, declare a new one in the
    /// opposite direction or with a different value, or `Retraction`
    /// the affected proposition to reset its value.
    RelationDeclaration {
        from: String,
        to: String,
        relation: hari_lattice::Relation,
    },
}

/// Names which prior evidence a [`ResearchEventPayload::Retraction`]
/// withdraws (issue #16). At the `ResearchEvent` boundary evidence is
/// addressed by `source` and/or `cycle`; an entry matches when every
/// present field matches. An empty selector (`{}`) matches all prior
/// evidence for the proposition, same as omitting `retracts`. The
/// contract's `dedup_key` form is a merge-layer concern and is not part
/// of this boundary slice.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RetractionSelector {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cycle: Option<u64>,
}

impl RetractionSelector {
    /// Whether this selector names a specific piece of evidence (has at
    /// least one constraint). An unconstrained selector behaves like the
    /// whole-proposition retraction.
    fn is_targeted(&self) -> bool {
        self.source.is_some() || self.cycle.is_some()
    }

    /// Whether a ledger entry from `source` at `cycle` is matched by this
    /// selector. Every present field must match; absent fields are
    /// wildcards.
    fn matches(&self, source: &str, cycle: u64) -> bool {
        self.source.as_deref().is_none_or(|s| s == source) && self.cycle.is_none_or(|c| c == cycle)
    }
}

impl ResearchEvent {
    /// Owned copy of the *primary* proposition this event targets, if
    /// any. Returns the single proposition for direct claim events
    /// (BeliefUpdate / ExperimentResult / AgentVote / Retraction),
    /// `None` for events without a single primary proposition
    /// (`GoalUpdate`, `RelationDeclaration` — the latter touches two).
    /// Use [`Self::touched_propositions`] when both endpoints matter.
    pub fn payload_proposition_owned(&self) -> Option<String> {
        self.payload.proposition().map(str::to_string)
    }

    /// All propositions this event touches, in declaration order. Empty
    /// for `GoalUpdate`; one entry for direct claim events; two for
    /// `RelationDeclaration` (`from` then `to`). Used by the streaming
    /// layer to track which propositions appear in the session's
    /// `final_beliefs` snapshot, including auto-created relation
    /// endpoints.
    pub fn touched_propositions(&self) -> Vec<String> {
        match &self.payload {
            ResearchEventPayload::BeliefUpdate { proposition, .. }
            | ResearchEventPayload::ExperimentResult { proposition, .. }
            | ResearchEventPayload::AgentVote { proposition, .. }
            | ResearchEventPayload::Retraction { proposition, .. } => vec![proposition.clone()],
            ResearchEventPayload::RelationDeclaration { from, to, .. } => {
                vec![from.clone(), to.clone()]
            }
            ResearchEventPayload::Supersession {
                proposition,
                superseded_by,
                ..
            } => vec![proposition.clone(), superseded_by.clone()],
            ResearchEventPayload::Correction { proposition, .. } => vec![proposition.clone()],
            ResearchEventPayload::RelationWithdrawal { from, to, .. } => {
                vec![from.clone(), to.clone()]
            }
            ResearchEventPayload::GoalUpdate { .. } => Vec::new(),
        }
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
    /// Phase-8 forward-reasoning provenance: every belief change
    /// produced by `propagate_until_stable_with_provenance` after this
    /// event, in chronological round order. Empty when no relations
    /// have been declared (and skipped from JSON via
    /// `skip_serializing_if`) so legacy fixtures and recorded sessions
    /// stay byte-equal.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub derivations: Vec<hari_lattice::Derivation>,
    /// Belief-revision deltas produced by this event (issue #16): the
    /// `(previous → current, cause)` change a selective `Retraction` or
    /// `Supersession` caused. Empty (and skipped from JSON) for every
    /// non-revision event and for the legacy whole-proposition
    /// retraction, so existing fixtures and recorded sessions stay
    /// byte-equal.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub revisions: Vec<RevisionDelta>,
}

/// What kind of belief-revision event produced a [`RevisionDelta`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RevisionCause {
    /// A selective `Retraction` tombstoned evidence and recomputed.
    Retraction,
    /// A `Correction` atomically retracted prior evidence and replaced it
    /// with a corrected value (issue #16). Distinguished from a plain
    /// `Retraction` so a consumer can see the belief moved *because it was
    /// corrected*, not merely withdrawn.
    Correction,
    /// A `RelationWithdrawal` tombstoned a relation edge; a belief that the
    /// edge had induced reverted on re-propagation (issue #16).
    RelationWithdrawal,
    /// A `Supersession` retired a claim for a successor.
    Supersession,
}

/// A single `report_delta_between_previous_and_current_belief` record
/// (design §4 semantic 5): the belief change a revision event caused,
/// emitted so an IX consumer can audit *why* a value moved. For a
/// `Retraction` this is the recompute-over-survivors delta; for a
/// `Supersession` `previous == current` (the retired claim freezes at
/// its last value) and `superseded_by` names the successor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RevisionDelta {
    /// The proposition whose belief the revision acted on.
    pub proposition: String,
    /// Belief value before the revision was applied.
    pub previous: HexValue,
    /// Belief value after the revision (recomputed from survivors, or
    /// frozen for supersession).
    pub current: HexValue,
    /// Why the belief was revised.
    pub cause: RevisionCause,
    /// Cycle of the revision event.
    pub cycle: u64,
    /// Successor claim, set only for `Supersession`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub superseded_by: Option<String>,
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
    /// Belief-revision deltas across the whole trace, in event order
    /// (issue #16) — the flattened `revisions` of every outcome. Empty
    /// (and skipped from JSON) for traces with no selective retraction
    /// or supersession, so pre-slice reports stay byte-equal.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub revisions: Vec<RevisionDelta>,
    /// Forecast calibration for the beliefs this trace touched (#35 §9.2),
    /// attached explicitly by [`ResearchReplayReport::with_calibration`].
    /// `None` (and skipped from JSON) unless a caller supplies forecast
    /// records, so every existing replay stays byte-equal — replay itself
    /// reads no ledger and performs no I/O.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calibration: Option<ReplayCalibration>,
}

/// Calibration block attached to a replay report: the per-belief Brier means
/// from [`forecast::calibration`] plus a pooled roll-up.
///
/// The pooled entry is the A/B baseline the per-belief numbers must be read
/// against (the same role `pooled` plays in `reliability` and
/// `source_reliability`) — a policy that improves one belief's calibration
/// while degrading the pool has not improved calibration.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ReplayCalibration {
    /// Per-belief calibration, keyed by `belief_id`. Scoped to the beliefs the
    /// replayed trace touched — see [`ResearchReplayReport::with_calibration`].
    pub per_belief: BTreeMap<String, forecast::BeliefCalibration>,
    /// Resolved-and-scored forecasts across the in-scope beliefs.
    pub scored: usize,
    /// Forecasts resolved as `Void` (observable never materialised).
    pub void: usize,
    /// Forecasts still awaiting resolution.
    pub pending: usize,
    /// Brier mean over every scored forecast, pooled across beliefs.
    /// `None` when nothing is scored — an unscored ledger has no calibration,
    /// which is distinct from a calibration of 0.0 (perfect).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pooled_mean_brier: Option<f64>,
}

impl ReplayCalibration {
    /// Fold forecast records into a calibration block.
    ///
    /// Pure: takes records the caller already loaded rather than reading the
    /// ledger itself, so replay stays I/O-free and deterministic.
    pub fn from_records(records: &[forecast::ForecastRecord]) -> Self {
        let per_belief = forecast::calibration(records);
        let mut scored = 0usize;
        let mut void = 0usize;
        let mut pending = 0usize;
        // Brier sum recovered as `mean_b * scored_b` per belief. Counts add
        // exactly, so this is the true pooled mean over all scored forecasts —
        // NOT the mean of per-belief means, which would weight a belief with
        // one forecast the same as one with fifty.
        let mut brier_sum = 0.0f64;
        for cal in per_belief.values() {
            scored += cal.scored;
            void += cal.void;
            pending += cal.pending;
            if let Some(mean) = cal.mean_brier {
                brier_sum += mean * cal.scored as f64;
            }
        }
        Self {
            per_belief,
            scored,
            void,
            pending,
            pooled_mean_brier: (scored > 0).then(|| brier_sum / scored as f64),
        }
    }
}

impl ResearchReplayReport {
    /// Every belief this replay touched: the propositions named by its events,
    /// plus whatever survives in `final_beliefs`. Union rather than either
    /// alone — a retracted claim can leave `final_beliefs` while still having
    /// been reasoned about, and propagation can enter `final_beliefs` without
    /// any event naming it.
    fn touched_beliefs(&self) -> BTreeSet<String> {
        let mut touched: BTreeSet<String> = self.final_beliefs.keys().cloned().collect();
        for outcome in &self.outcomes {
            touched.extend(outcome.event.touched_propositions());
        }
        touched
    }

    /// Attach forecast calibration to this report (#35 §9.2).
    ///
    /// Opt-in by construction: a report only carries calibration when a caller
    /// hands it records, which is what keeps every existing replay byte-equal.
    /// Records are supplied by the caller (`forecast::load`), never read here.
    ///
    /// **Scoped to this trace.** The ledger is ecosystem-wide — it carries
    /// forecasts about GA and Demerzel beliefs that a hari fixture never
    /// mentions — so records whose `belief_id` is not among the beliefs this
    /// replay touched are dropped. Without that filter the block reports the
    /// calibration of unrelated forecasts, which reads as if the trace's own
    /// claims had been scored. Zero in-scope records yields an all-zero block
    /// with `pooled_mean_brier: None`: the honest "nothing has been forecast
    /// about these beliefs yet".
    #[must_use]
    pub fn with_calibration(mut self, records: &[forecast::ForecastRecord]) -> Self {
        let touched = self.touched_beliefs();
        let in_scope: Vec<forecast::ForecastRecord> = records
            .iter()
            .filter(|r| touched.contains(&r.belief_id))
            .cloned()
            .collect();
        self.calibration = Some(ReplayCalibration::from_records(&in_scope));
        self
    }
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
    /// Number of `Action::Wait` recommendations on a claim that went on to
    /// *stand* — caution that cost the trace a correct acceptance (#35 §5.3).
    ///
    /// The mirror of `false_acceptance_count`, and the price of abstention.
    /// Without it a policy can win an accuracy comparison by emitting more
    /// `Wait`s and never paying for the ones that were unnecessary.
    ///
    /// **Single-arm diagnostics only — do not compare this across arms.** It
    /// resolves each wait against *this replay's own* `final_beliefs`, and
    /// excuses a wait whose claim ended `Contradictory`. Whether a claim ends
    /// `Contradictory` is an arm's output, not a fact, so the excusal lands
    /// unevenly: on `heavy_contradiction`, `Lie` waits on two claims and is
    /// charged for neither (it leaves both `Contradictory`) while
    /// `SubjectiveLogic` waits on six and is charged five (it resolves them to
    /// `Probable`). The counter rewards staying stuck. For cross-arm work use
    /// [`paired_eval::score_false_rejections`], which grades against authored
    /// ground truth and cannot depend on the arm being graded.
    pub false_rejection_count: u32,
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
        // Re-declaring a goal revises its description and priority; it does not
        // un-achieve it. A blanket `insert` reset `status` to `Unknown`, so a
        // priority-revising `goal_update` silently discarded a completion the
        // policy had already established — observed on
        // `fixtures/ix/long_recovery.json`, whose cycle-30 `goal_update` reset
        // `alpha-prompt-helps` from `True` back to `Probable`.
        let description = description.into();
        self.goals
            .entry(key.into())
            .and_modify(|goal| {
                goal.description = description.clone();
                goal.priority = priority;
            })
            .or_insert(Goal {
                description,
                priority,
                status: HexValue::Unknown,
            });
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
    /// Phase 4-bridge: when an `AgentVote` event is processed, the vote
    /// is always recorded into this swarm. Empty by default; agents are
    /// auto-created on first vote with a neutral role
    /// (`self_trust = 0.5`, `message_trust = 0.5`) unless declared
    /// up-front via `SessionConfig::initial_agents`.
    pub swarm: hari_swarm::Swarm,
    /// `TrustModel` applied when computing swarm consensus (only consulted
    /// if `use_swarm_consensus`). Defaults to `Equal`.
    pub trust_model: hari_swarm::TrustModel,
    /// When true, `AgentVote` events perceive the swarm consensus value
    /// instead of the raw vote — closing the Phase 4 loop into the IX
    /// pipeline. Default `false` preserves pre-bridge behavior.
    pub use_swarm_consensus: bool,
    /// SL config used when `priority_model == SubjectiveLogic`. Ignored
    /// otherwise. Defaults to `SubjectiveLogicConfig::default()`.
    pub sl_config: subjective_logic::SubjectiveLogicConfig,
    /// SL state, lazily allocated on the first event under
    /// `PriorityModel::SubjectiveLogic`. Stays `None` for non-SL
    /// runs so the per-loop memory footprint is unchanged.
    sl_state: Option<subjective_logic::SubjectiveLogicState>,
    /// Per-proposition evidence ledger for belief-revision recompute
    /// (issue #16). Every belief-bearing event (`BeliefUpdate` /
    /// `ExperimentResult` / `AgentVote`) appends an entry; a selective
    /// `Retraction` tombstones matching entries and the proposition is
    /// recomputed from the survivors. Pure bookkeeping — appending emits
    /// no actions and does not touch belief values, so traces without a
    /// selective retraction replay bit-identically. Retracted entries
    /// are kept flagged, never removed (preserve-for-audit / tombstone,
    /// not delete).
    evidence_log: BTreeMap<String, Vec<EvidenceEntry>>,
    /// Propositions retired by a `Supersession` event (issue #16),
    /// mapped to the successor claim. A superseded claim keeps its last
    /// value but is marked here so a consumer can tell the live chain
    /// head from the audit tail.
    superseded: BTreeMap<String, String>,
    /// Optional per-source evidence weights (issue #14 *consumer experiment*).
    /// `None` (the default) means every belief-bearing event is ledgered at
    /// the uniform `weight = 1.0` the merge-weight slice hard-codes, so a
    /// replay is byte-identical to pre-experiment behaviour. When `Some`, the
    /// evidence-ledger entry for a source uses `map[source]` (falling back to
    /// `1.0` for an unlisted source), letting a counterfactual replay weight
    /// base evidence by a source's *earned* reliability — the entrenchment
    /// ordering from [`source_reliability`]. Read-only science: this is set
    /// only by the entrenchment-counterfactual harness, never by any default
    /// path, and it is surfaced for A/B, not wired into consensus.
    source_weights: Option<BTreeMap<String, f64>>,
}

/// One recorded piece of evidence in the belief-revision ledger
/// (issue #16). Addressed by `(source, cycle)` at the `ResearchEvent`
/// boundary; `retracted` is the tombstone flag — a retracted entry
/// contributes zero to the recompute but stays in the ledger for audit.
///
/// `weight` is the per-source confidence carried into the merge-routed
/// recompute (merge-weight slice). It is `1.0` for every boundary event
/// today — the ledger has no per-source reliability yet; that is earned,
/// not declared, and owned by the source-reliability ledger (#14). The
/// field exists so the recompute can route through
/// [`hari_lattice::merge`] as weighted observations and so #14 can later
/// supply real weights without another schema change.
#[derive(Debug, Clone, PartialEq)]
struct EvidenceEntry {
    source: String,
    cycle: u64,
    value: HexValue,
    weight: f64,
    retracted: bool,
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
            swarm: hari_swarm::Swarm::new(),
            trust_model: hari_swarm::TrustModel::Equal,
            use_swarm_consensus: false,
            sl_config: subjective_logic::SubjectiveLogicConfig::default(),
            sl_state: None,
            evidence_log: BTreeMap::new(),
            superseded: BTreeMap::new(),
            source_weights: None,
        }
    }

    /// Install a per-source evidence-weight table for the issue #14
    /// entrenchment-counterfactual replay (arm R). Each belief-bearing event
    /// is then ledgered at `weights[source]` instead of the uniform `1.0`
    /// (an unlisted source keeps `1.0`), so a selective retraction / correction
    /// recomputes the belief from evidence weighted by earned reliability. This
    /// is the smallest additive, opt-in hook the counterfactual needs; calling
    /// it is the *only* way to leave the byte-identical default, and it wires
    /// nothing into the accept/escalate scoring path — weight reaches a belief
    /// value solely through the merge's Contradictory-escalation share during a
    /// ledger recompute. Not part of any owner-blessed consumer; read-only
    /// science surfaced for A/B (design §7).
    pub fn set_source_weights(&mut self, weights: BTreeMap<String, f64>) {
        self.source_weights = Some(weights);
    }

    /// The evidence weight to ledger for `source`: `1.0` when no table is
    /// installed (byte-identical default), else the table's entry for the
    /// source (`1.0` if unlisted).
    fn evidence_weight(&self, source: &str) -> f64 {
        match &self.source_weights {
            None => 1.0,
            Some(map) => map.get(source).copied().unwrap_or(1.0),
        }
    }

    /// Builder-style constructor: pick a [`PriorityModel`] up front.
    ///
    /// Without this builder, [`CognitiveLoop::new`] uses
    /// `PriorityModel::default()` (currently `RecencyDecay`). Pass
    /// `PriorityModel::Flat` here when you need pre-Phase-5 behavior
    /// — e.g. ablation tests or replays of fixtures recorded under the
    /// old default.
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
        let scaling_weights: Vec<f64> = (0..d).map(|k| if k == 0 { -0.5 } else { 0.5 }).collect();
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
            let dim = self.state.goal_axis(&p.proposition).unwrap_or(0).min(d - 1);
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
        for h in &mut h_eff[1..d] {
            *h += smear_per_axis;
        }

        let mut coefficients: Vec<f64> = Vec::with_capacity(d + 1);
        // Rotations rot(0, k) for k = 1..d
        coefficients.extend(h_eff[1..d].iter().map(|h| alpha * h));
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
        // Belief propagation moved out of cycle_raw and into the
        // end-of-event provenance pass in `process_research_event`
        // (Phase 8). Doing it once-and-only-once at the end means
        // every derivation flows through `propagate_until_stable_with_provenance`
        // and gets recorded on the outcome. Pre-Phase-8 fixtures
        // declared no relations, so removing this no-op pre-step
        // changes no observable behavior on them — but keeps the
        // provenance audit trail complete for fixtures that DO
        // declare relations.

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

        // Evaluate goals.
        //
        // Action emission stays focused on the **top** goal — one attention
        // target per cycle — but goal *status* is refreshed for every goal below,
        // because status was previously only ever written for whichever goal held
        // the top slot. `top_goal` evicts a goal only once its status reaches
        // `True`, so any goal stuck short of that held the slot indefinitely and
        // starved the rest: on `fixtures/ix/long_recovery.json`,
        // `gamma-method-correct` held it zero times across 22 events and ended
        // `Unknown` while its belief was `Probable`.
        //
        // Ordering is deliberate. `top_goal` is resolved here, against the
        // statuses as they stood at the start of this cycle, and the bulk refresh
        // runs afterwards. Refreshing first would evict a goal in the same cycle
        // it completes rather than the next, changing which goal draws an action
        // — so this ordering keeps the emitted action sequence byte-identical and
        // confines the change to goal status.
        if let Some((key, goal)) = self.state.top_goal() {
            let key = key.clone();
            let goal_desc = goal.description.clone();
            tracing::debug!("Top goal: {} (priority={})", goal_desc, goal.priority);

            // Check if any beliefs relate to goal completion
            if let Some(belief) = self.state.beliefs.get(&key) {
                match belief.value {
                    HexValue::True | HexValue::Probable => {
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

        // Bulk status refresh — every goal, not just the one that drew an action.
        // Collected first because the write borrows `self.state.goals` mutably
        // while the read borrows `self.state.beliefs`.
        //
        // Upgrade-only, matching the emission arms above: a goal whose evidence
        // later collapses keeps its credit. That staleness is a *separate*
        // defect, pinned by `known_violation_goal_status_is_never_revised_downward`,
        // and changing it would alter what "achieved" means rather than which
        // goals get looked at. Deliberately out of scope here.
        //
        // The `True | Probable` filter selects which *beliefs* qualify; it does
        // not by itself make the write an upgrade. Between 79ad578 and this
        // commit it did not: `Probable` was written over an achieved `True`,
        // which un-achieved the goal and re-admitted it to `top_goal`
        // candidacy, where it displaced the real top goal and swallowed that
        // goal's actions. Measured on `heavy_contradiction` (Escalate 13 → 10)
        // and `long_recovery` (8 → 6). The downgrade guard below is what makes
        // the word "upgrade-only" true of the code rather than only of the
        // comment.
        let goal_beliefs: Vec<(String, HexValue)> = self
            .state
            .goals
            .keys()
            .filter_map(|key| {
                self.state
                    .beliefs
                    .get(key)
                    .filter(|belief| matches!(belief.value, HexValue::True | HexValue::Probable))
                    .map(|belief| (key.clone(), belief.value))
            })
            .collect();
        for (key, value) in goal_beliefs {
            if let Some(goal) = self.state.goals.get_mut(&key) {
                if matches!(goal.status, HexValue::True) && !matches!(value, HexValue::True) {
                    continue;
                }
                goal.status = value;
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
                                let proj = attention.get(axis).copied().unwrap_or(0.0);
                                let base = 1.0;
                                base * (1.0 + alpha * proj)
                            }
                        };
                        (a, score, perception_cycle)
                    })
                    .collect()
            }
            // SubjectiveLogic short-circuits at the top of
            // `process_research_event`; if we got here something is
            // structurally wrong with the dispatch.
            PriorityModel::SubjectiveLogic => {
                unreachable!("SubjectiveLogic must short-circuit before action scoring")
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
            Action::Accept { proposition, .. } | Action::UpdateBelief { proposition, .. } => {
                proposition.as_str()
            }
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

        // Flatten every outcome's revision deltas into a trace-level log,
        // in event order (issue #16). Empty for traces without selective
        // retraction or supersession, so pre-slice reports stay byte-equal.
        let revisions: Vec<RevisionDelta> = outcomes
            .iter()
            .flat_map(|o| o.revisions.iter().cloned())
            .collect();

        ResearchReplayReport {
            event_count: outcomes.len(),
            outcomes,
            final_beliefs,
            final_goals: self.state.goals.clone(),
            final_state_summary: self.state.summary(),
            priority_model: self.priority_model,
            metrics,
            comparison: None,
            revisions,
            // Replay performs no I/O; a caller that wants calibration loads
            // the ledger and calls `with_calibration`.
            calibration: None,
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
        // Subjective Logic short-circuit. SL is structurally unlike the
        // hexavalent action-scoring path: it fuses Opinions and emits
        // recommendations from projected probability + uncertainty
        // thresholds, with no perception buffer, no swarm bridging, no
        // BeliefNetwork propagation, and no `score_actions_with_cycles`
        // step. So we dispatch to its own pipeline and return its
        // outcome directly, leaving the hexavalent state untouched.
        if matches!(self.priority_model, PriorityModel::SubjectiveLogic) {
            let state = self.sl_state.get_or_insert_with(Default::default);
            return subjective_logic::process_event(state, event, &self.sl_config);
        }

        let mut actions: Vec<Action> = Vec::new();
        let mut action_cycles: Vec<u64> = Vec::new();
        let mut revisions: Vec<RevisionDelta> = Vec::new();
        // `event` is moved into the outcome at the end; `cycle` is Copy so
        // capture it up front for the post-propagation withdrawal delta.
        let event_cycle = event.cycle;
        // Belief snapshot taken by a `RelationWithdrawal` *before* it resets
        // derivation-only nodes, so the reverted beliefs can be diffed
        // against their post-re-propagation values once the once-per-event
        // propagation pass has run (issue #16). `None` for every other event.
        let mut withdrawal_before: Option<BTreeMap<String, HexValue>> = None;

        // Ledger the asserted evidence for belief-bearing events
        // (issue #16) so a later selective `Retraction` can recompute
        // from the survivors. Inert bookkeeping: it appends no actions
        // and never touches a belief value, so a trace without a
        // selective retraction replays bit-identically. The recorded
        // value is the source's *assertion* (the payload value), which
        // is what evidence-recompute filters on.
        if let ResearchEventPayload::BeliefUpdate {
            proposition, value, ..
        }
        | ResearchEventPayload::ExperimentResult {
            proposition, value, ..
        }
        | ResearchEventPayload::AgentVote {
            proposition, value, ..
        } = &event.payload
        {
            // Uniform confidence at the boundary by default (no per-source
            // reliability is declared here — that is #14, earned not declared).
            // `evidence_weight` returns `1.0` unless the #14 entrenchment-
            // counterfactual harness has installed a reliability-weight table
            // (arm R), in which case base evidence is weighted by earned
            // reliability for the read-only A/B. The recompute routes these
            // through the weighted merge; a corroboration cap over distinct
            // sources — not weight magnitude — is what downgrades a single
            // uncorroborated source (see `recompute_belief`).
            let weight = self.evidence_weight(&event.source);
            self.evidence_log
                .entry(proposition.clone())
                .or_default()
                .push(EvidenceEntry {
                    source: event.source.clone(),
                    cycle: event.cycle,
                    value: *value,
                    weight,
                    retracted: false,
                });
        }

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
                // Phase 4 bridge: always record the vote in the swarm.
                // Auto-create the agent with a neutral role on first
                // vote — explicit roles come from
                // `SessionConfig::initial_agents`.
                self.record_agent_vote(&event.source, proposition, *value);

                // The value the cognitive loop *perceives* is the raw
                // vote by default. When `use_swarm_consensus` is on, it
                // is replaced by the swarm's consensus across all
                // agents that have voted on this proposition so far,
                // computed under `self.trust_model`.
                let perceived_value = if self.use_swarm_consensus {
                    self.swarm
                        .consensus_with(proposition, self.trust_model)
                        .consensus
                } else {
                    *value
                };

                self.perceive(Perception {
                    proposition: proposition.clone(),
                    value: perceived_value,
                    source: format!("vote:{}", event.source),
                    cycle: event.cycle,
                });
                let (cycle_actions, cycle_cycles) = self.cycle_raw();
                actions.extend(cycle_actions);
                action_cycles.extend(cycle_cycles);
                let current_value = self
                    .current_claim_value(proposition)
                    .unwrap_or(perceived_value);
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
                retracts,
            } => {
                match retracts.as_ref().filter(|s| s.is_targeted()) {
                    // Selective retraction (issue #16): tombstone the
                    // matched evidence in the ledger, recompute the
                    // proposition's value from the survivors
                    // (evidence-recompute is authoritative), and report
                    // the delta. This is the path that beats the
                    // whole-proposition LWW-to-Unknown baseline: the
                    // belief survives on remaining evidence instead of
                    // being erased, and a derived contradiction dissolves
                    // for free because nothing is carried forward.
                    Some(selector) => {
                        let previous = self.current_claim_value(proposition);
                        let mut tombstoned = 0usize;
                        if let Some(entries) = self.evidence_log.get_mut(proposition) {
                            for entry in entries.iter_mut() {
                                if !entry.retracted && selector.matches(&entry.source, entry.cycle)
                                {
                                    entry.retracted = true;
                                    tombstoned += 1;
                                }
                            }
                        }
                        let recomputed = self.recompute_from_ledger(proposition);
                        self.set_belief_value(proposition, recomputed);

                        actions.push(Action::Log(format!(
                            "Retracted {} evidence item(s) for '{}' ({}); recomputed {} -> {}",
                            tombstoned,
                            proposition,
                            reason,
                            previous
                                .map(|v| v.to_string())
                                .unwrap_or_else(|| "n/a".to_string()),
                            recomputed,
                        )));
                        action_cycles.push(event.cycle);
                        // Recommend the action appropriate to the
                        // recomputed value (Investigate on Unknown, Accept
                        // on a definite value, Escalate on a standing
                        // contradiction) — the existing per-claim action
                        // semantics, now driven by the survivor recompute.
                        for a in Self::recommend_for_claim(proposition, recomputed) {
                            actions.push(a);
                            action_cycles.push(event.cycle);
                        }
                        if let Some(prev) = previous {
                            revisions.push(RevisionDelta {
                                proposition: proposition.clone(),
                                previous: prev,
                                current: recomputed,
                                cause: RevisionCause::Retraction,
                                cycle: event.cycle,
                                superseded_by: None,
                            });
                        }
                    }
                    // Whole-proposition retraction (no selector). Per the
                    // design (§4 semantic 2) this goes "via the filter path,
                    // not a mutation to Unknown": tombstone *all* the
                    // proposition's evidence and recompute — which yields
                    // Unknown, since the survivor set is now empty
                    // (`combine_evidence_set([]) == Unknown`). So there is
                    // one recompute mechanism, not a special-cased reset.
                    //
                    // This arm is also the A/B baseline the selective path
                    // is measured against (design §7), and existing fixtures
                    // (e.g. fixtures/ix/conflicting_benchmark.json) pin its
                    // report. The design would emit a delta on any
                    // belief-changing revision, but doing so here would
                    // change those recorded reports; per the byte-compat
                    // requirement the legacy action emission (Log + Retry)
                    // is kept verbatim and NO revision delta is emitted for
                    // the no-selector path. The new semantics *replace* the
                    // naive handler (they are not opt-in) — the naive wire
                    // shape stays parseable and its behavior is preserved as
                    // this baseline arm.
                    None => {
                        if let Some(entries) = self.evidence_log.get_mut(proposition) {
                            for entry in entries.iter_mut() {
                                entry.retracted = true;
                            }
                        }
                        let recomputed = self.recompute_from_ledger(proposition);
                        debug_assert_eq!(recomputed, HexValue::Unknown);
                        self.set_belief_value(proposition, recomputed);
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
                }
            }
            ResearchEventPayload::Supersession {
                proposition,
                superseded_by,
                reason,
            } => {
                // Retire the claim: record the directed lineage edge and
                // freeze the superseded proposition at its last value
                // (never reset — it stays inspectable as the audit tail).
                // "No downstream mass" is a no-op in this slice: no
                // dependency fixture exercises propagation out of a
                // superseded claim yet (design §10).
                self.superseded
                    .insert(proposition.clone(), superseded_by.clone());
                let frozen = self.current_claim_value(proposition);
                actions.push(Action::Log(format!(
                    "Superseded '{}' by '{}': {}",
                    proposition, superseded_by, reason
                )));
                action_cycles.push(event.cycle);
                if let Some(value) = frozen {
                    revisions.push(RevisionDelta {
                        proposition: proposition.clone(),
                        previous: value,
                        current: value,
                        cause: RevisionCause::Supersession,
                        cycle: event.cycle,
                        superseded_by: Some(superseded_by.clone()),
                    });
                }
            }
            ResearchEventPayload::Correction {
                proposition,
                reason,
                retracts,
                value,
                evidence,
            } => {
                // Atomic retract-plus-replace (issue #16). Same evidence
                // ledger as a selective retraction: tombstone the corrected
                // evidence, append the replacement as a fresh assertion, then
                // recompute the belief over the survivors (which now include
                // the replacement and exclude the tombstoned original). One
                // recompute engine — the correction's new value is merged in
                // exactly like any other source assertion.
                let previous = self.current_claim_value(proposition);
                let mut tombstoned = 0usize;
                if let Some(entries) = self.evidence_log.get_mut(proposition) {
                    for entry in entries.iter_mut() {
                        if !entry.retracted && retracts.matches(&entry.source, entry.cycle) {
                            entry.retracted = true;
                            tombstoned += 1;
                        }
                    }
                }
                // Append the corrected evidence. It carries the correction's
                // own `(source, cycle)`, so the tombstoned original and its
                // replacement coexist in the ledger — the audit link between
                // "what was wrong" and "what replaced it".
                let weight = self.evidence_weight(&event.source);
                self.evidence_log
                    .entry(proposition.clone())
                    .or_default()
                    .push(EvidenceEntry {
                        source: event.source.clone(),
                        cycle: event.cycle,
                        value: *value,
                        weight,
                        retracted: false,
                    });
                let recomputed = self.recompute_from_ledger(proposition);
                self.set_belief_value(proposition, recomputed);

                actions.push(Action::Log(format!(
                    "Corrected '{}' ({}): tombstoned {} item(s), replaced with {}; \
                     recomputed {} -> {}",
                    proposition,
                    reason,
                    tombstoned,
                    value,
                    previous
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "n/a".to_string()),
                    recomputed,
                )));
                action_cycles.push(event.cycle);
                for a in Self::recommend_for_claim(proposition, recomputed) {
                    actions.push(a);
                    action_cycles.push(event.cycle);
                }
                if !evidence.is_empty() {
                    actions.push(Action::Log(format!(
                        "Recorded {} correction field(s) for '{}'",
                        evidence.len(),
                        proposition
                    )));
                    action_cycles.push(event.cycle);
                }
                // One revision delta, cause = correction — the causal link
                // that distinguishes a correction from a plain retraction.
                if let Some(prev) = previous {
                    revisions.push(RevisionDelta {
                        proposition: proposition.clone(),
                        previous: prev,
                        current: recomputed,
                        cause: RevisionCause::Correction,
                        cycle: event.cycle,
                        superseded_by: None,
                    });
                }
            }
            ResearchEventPayload::RelationWithdrawal {
                from,
                to,
                relation,
                reason,
            } => {
                // Tombstone the relation edge (marked withdrawn, not deleted
                // — audit-preservation), then dissolve the beliefs it induced
                // by evidence-recompute: reset every derivation-only
                // proposition (no surviving direct evidence) to its `Unknown`
                // base and let the once-per-event propagation pass below
                // re-derive over the reduced edge set. Base beliefs (those
                // with ledger evidence) are untouched. An unmatched triple is
                // a logged no-op that touches nothing.
                let matched = self.state.beliefs.withdraw_relation(from, to, *relation);
                if matched {
                    // Snapshot before the reset so the reverted beliefs can
                    // be diffed after re-propagation.
                    withdrawal_before = Some(self.snapshot_belief_values());
                    for label in self.state.beliefs.labels() {
                        let has_evidence = self
                            .evidence_log
                            .get(&label)
                            .map(|es| es.iter().any(|e| !e.retracted))
                            .unwrap_or(false);
                        if !has_evidence {
                            self.set_belief_value(&label, HexValue::Unknown);
                        }
                    }
                    actions.push(Action::Log(format!(
                        "Withdrew relation {:?}: '{}' -> '{}' ({}); re-propagating over the \
                         reduced edge set",
                        relation, from, to, reason
                    )));
                } else {
                    actions.push(Action::Log(format!(
                        "No live relation {:?}: '{}' -> '{}' to withdraw ({}) — no-op",
                        relation, from, to, reason
                    )));
                }
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
            ResearchEventPayload::RelationDeclaration { from, to, relation } => {
                self.state.beliefs.declare_relation(from, to, *relation);
                actions.push(Action::Log(format!(
                    "Declared relation {:?}: '{}' -> '{}'",
                    relation, from, to
                )));
                action_cycles.push(event.cycle);
            }
        }

        // Phase 8 reasoning: after every event, run belief propagation
        // through the network with provenance. On networks with no
        // declared relations this is a single no-op iteration. When
        // propagation actually fires (≥2 iterations: at least one round
        // did work, then a zero-change round confirmed convergence) we
        // surface (a) a one-line Log for human-readable observability,
        // (b) one Log per derived proposition naming its sources, and
        // (c) a structured `derivations` list on the outcome for
        // programmatic auditability.
        let (propagation_rounds, derivations) = self
            .state
            .beliefs
            .propagate_until_stable_with_provenance(10);
        if propagation_rounds > 1 {
            actions.push(Action::Log(format!(
                "Propagated beliefs in {propagation_rounds} rounds"
            )));
            action_cycles.push(event.cycle);
            for d in &derivations {
                let sources: Vec<String> = d
                    .contributions
                    .iter()
                    .map(|c| format!("{}({:?},{:?})", c.source, c.relation, c.source_value))
                    .collect();
                actions.push(Action::Log(format!(
                    "Derived '{}': {:?} -> {:?} (round {}) from [{}]",
                    d.proposition,
                    d.previous_value,
                    d.new_value,
                    d.round,
                    sources.join(", ")
                )));
                action_cycles.push(event.cycle);
            }
        }

        // Relation-withdrawal deltas (issue #16): now that the reduced-edge
        // re-propagation has run, diff every belief that changed against the
        // pre-withdrawal snapshot. A derived belief that reverted (e.g.
        // `True -> Unknown` once its only supporting edge was withdrawn)
        // surfaces here as a `RelationWithdrawal` revision delta, in
        // deterministic proposition order (the snapshot is a BTreeMap).
        if let Some(before) = withdrawal_before {
            for (proposition, prev) in before {
                let current = self.current_claim_value(&proposition).unwrap_or(prev);
                if current != prev {
                    revisions.push(RevisionDelta {
                        proposition,
                        previous: prev,
                        current,
                        cause: RevisionCause::RelationWithdrawal,
                        cycle: event_cycle,
                        superseded_by: None,
                    });
                }
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
            derivations,
            revisions,
        }
    }

    fn current_claim_value(&self, proposition: &str) -> Option<HexValue> {
        self.state.beliefs.get(proposition).map(|p| p.value)
    }

    /// Snapshot every proposition's current belief value (issue #16). Used by
    /// the `RelationWithdrawal` path to capture the pre-withdrawal state so
    /// reverted beliefs can be diffed after re-propagation. A `BTreeMap` so
    /// the emitted deltas are in deterministic proposition order.
    fn snapshot_belief_values(&self) -> BTreeMap<String, HexValue> {
        self.state
            .beliefs
            .labels()
            .into_iter()
            .filter_map(|label| self.current_claim_value(&label).map(|v| (label, v)))
            .collect()
    }

    /// The successor claim a proposition was retired in favour of, if it
    /// has been superseded (issue #16), else `None`. A superseded claim
    /// keeps its last value (inspectable) but is no longer the live chain
    /// head — this is how a consumer distinguishes the two.
    pub fn superseded_by(&self, proposition: &str) -> Option<&str> {
        self.superseded.get(proposition).map(String::as_str)
    }

    /// Set (or create) a proposition's belief value directly. Used by
    /// the belief-revision recompute path (issue #16) after a selective
    /// retraction, which computes the authoritative value from the
    /// evidence ledger rather than folding one perception at a time.
    fn set_belief_value(&mut self, proposition: &str, value: HexValue) {
        if let Some(prop) = self.state.beliefs.get_mut(proposition) {
            prop.value = value;
        } else {
            self.state.beliefs.add_proposition(proposition, value);
        }
    }

    /// Recompute a proposition's authoritative value from its evidence
    /// ledger (issue #16), routing through [`hari_lattice::merge`] with the
    /// belief-revision **tombstone step** (merge-weight slice; design §9
    /// item 2).
    ///
    /// Every ledger entry — surviving *and* retracted — becomes a weighted
    /// [`hari_lattice::HexObservation`]; the retracted ones' dedup keys form
    /// the tombstone set that [`hari_lattice::merge_with_tombstones`] filters
    /// out before merging. "Evidence-recompute is authoritative": a
    /// tombstoned observation contributes zero mass, exactly as if it had
    /// never been submitted, while remaining in this ledger for audit. The
    /// merged distribution is then projected to a single [`HexValue`] by
    /// [`Self::project_belief`], which reads the corroboration structure the
    /// weightless `combine_evidence_set` could not — most importantly the
    /// single-source cap that downgrades an uncorroborated `True`/`False`.
    ///
    /// Because the value is derived fresh from survivors and nothing is
    /// carried, a retraction that removes one side of a conflict dissolves
    /// the derived contradiction with no cascade logic.
    fn recompute_from_ledger(&self, proposition: &str) -> HexValue {
        let mut observations: Vec<hari_lattice::HexObservation> = Vec::new();
        let mut tombstoned: std::collections::BTreeSet<hari_lattice::DedupKey> =
            std::collections::BTreeSet::new();
        if let Some(entries) = self.evidence_log.get(proposition) {
            for (idx, entry) in entries.iter().enumerate() {
                let obs =
                    Self::evidence_obs(&entry.source, entry.cycle, entry.value, entry.weight, idx);
                if entry.retracted {
                    tombstoned.insert(obs.dedup_key());
                }
                observations.push(obs);
            }
        }
        let state = hari_lattice::merge_with_tombstones(&observations, &tombstoned, None, None);
        Self::project_belief(&state)
    }

    /// Build one weighted merge observation from a ledger entry. All entries
    /// for one proposition share a single `claim_key` (they are positions on
    /// the *same* claim), so the merge's direct-contradiction synthesis fires
    /// between opposite-polarity sources; `ordinal = idx` keeps every entry's
    /// dedup key distinct so tombstoning one never removes another.
    fn evidence_obs(
        source: &str,
        cycle: u64,
        value: HexValue,
        weight: f64,
        idx: usize,
    ) -> hari_lattice::HexObservation {
        hari_lattice::HexObservation {
            source: source.to_string(),
            diagnosis_id: "revision-recompute".to_string(),
            round: cycle as u32,
            ordinal: idx as u32,
            claim_key: "belief".to_string(),
            variant: value,
            weight,
            evidence: None,
        }
    }

    /// Project a merged evidence state to the single authoritative belief
    /// value (issue #16 merge-weight slice). The rule, in order:
    ///
    /// 1. **Standing cross-source contradiction dominates.** If the merge
    ///    escalates (`Contradictory` share of *informative* mass over the
    ///    threshold — abstention cannot mute it, spec v1.2) the value is
    ///    `Contradictory`. This is how a live `True`/`False` disagreement
    ///    surfaces; retracting one side drops the synthesized `C` and this
    ///    check no longer fires.
    /// 2. **Base value** = `join` over the surviving *primary* (non-derived)
    ///    variants — the lattice's own agreement combinator. No primary
    ///    evidence → `Unknown`.
    /// 3. **Single-source corroboration cap.** A strong pole (`True`/`False`)
    ///    reached by only *one distinct source* downgrades to the adjacent
    ///    uncertain value (`True → Probable`, `False → Doubtful`); two or more
    ///    independent sources on that side license the strong value. This is
    ///    the merge-weight slice's new capability: corroboration is counted
    ///    over distinct sources, which the weightless `combine_evidence_set`
    ///    could not see. It applies uniformly — the reason fixture 1's
    ///    dissolved belief and fixture 2's partially-retracted belief both
    ///    land on `Probable` from a lone surviving `True`.
    fn project_belief(state: &hari_lattice::MergedState) -> HexValue {
        if state.distribution.escalation_triggered() {
            return HexValue::Contradictory;
        }
        let primaries: Vec<&hari_lattice::HexObservation> = state
            .observations
            .iter()
            .filter(|o| o.source != hari_lattice::MERGE_SOURCE)
            .collect();
        let base = match primaries
            .iter()
            .map(|o| o.variant)
            .reduce(<HexValue as hari_lattice::Lattice>::join)
        {
            Some(v) => v,
            None => return HexValue::Unknown,
        };
        // A join can still surface `Contradictory` if a primary observation
        // itself asserted it (a source directly reporting a contradiction),
        // independent of the escalation share — preserve it.
        let (strong_downgrade, on_side): (HexValue, fn(HexValue) -> bool) = match base {
            HexValue::True => (HexValue::Probable, |v| {
                matches!(v, HexValue::True | HexValue::Probable)
            }),
            HexValue::False => (HexValue::Doubtful, |v| {
                matches!(v, HexValue::False | HexValue::Doubtful)
            }),
            other => return other,
        };
        let distinct_sources: std::collections::BTreeSet<&str> = primaries
            .iter()
            .filter(|o| on_side(o.variant))
            .map(|o| o.source.as_str())
            .collect();
        if distinct_sources.len() >= 2 {
            base
        } else {
            strong_downgrade
        }
    }

    /// Authoritative belief recompute over an explicit set of *surviving*
    /// evidence assertions, routed through the same weighted merge +
    /// projection the boundary uses after a selective retraction (issue #16
    /// merge-weight slice). Each item is `(source, value, weight)`.
    ///
    /// This is the A/B **retraction-fidelity** oracle: the belief after a
    /// selective retraction must equal `recompute_belief` over exactly the
    /// evidence that survived — i.e. exactly what you would get had the
    /// retracted evidence never been submitted. Exposed so tests and probes
    /// assert fidelity against the real engine rather than a reconstruction.
    pub fn recompute_belief<'a>(
        evidence: impl IntoIterator<Item = (&'a str, HexValue, f64)>,
    ) -> HexValue {
        let observations: Vec<hari_lattice::HexObservation> = evidence
            .into_iter()
            .enumerate()
            .map(|(idx, (source, value, weight))| {
                Self::evidence_obs(source, idx as u64, value, weight, idx)
            })
            .collect();
        let empty: std::collections::BTreeSet<hari_lattice::DedupKey> =
            std::collections::BTreeSet::new();
        let state = hari_lattice::merge_with_tombstones(&observations, &empty, None, None);
        Self::project_belief(&state)
    }

    /// Record an `AgentVote` into [`Self::swarm`]. The agent identified
    /// by `source` is auto-created with a neutral role
    /// (`self_trust = 0.5`, `message_trust = 0.5`) if not already
    /// present. Subsequent votes from the same source overwrite that
    /// agent's local belief on the proposition (the swarm's job is to
    /// hold each voter's *latest* position, not a per-vote history).
    ///
    /// Public so test helpers and future tooling can drive votes
    /// without going through `process_research_event`.
    pub fn record_agent_vote(&mut self, source: &str, proposition: &str, value: HexValue) {
        if self.swarm.agent(source).is_none() {
            self.swarm.add_agent(hari_swarm::Agent::new(
                source,
                hari_swarm::AgentRole {
                    name: "auto".to_string(),
                    self_trust: 0.5,
                    message_trust: 0.5,
                },
            ));
        }
        if let Some(agent) = self.swarm.agent_mut(source) {
            if let Some(prop) = agent.beliefs.get_mut(proposition) {
                prop.value = value;
            } else {
                agent.beliefs.add_proposition(proposition, value);
            }
        }
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
            | Self::Retraction { proposition, .. }
            // Supersession's primary proposition is the *retired* claim;
            // the successor is reached via `touched_propositions`.
            | Self::Supersession { proposition, .. }
            // Correction targets a single proposition (retract + replace).
            | Self::Correction { proposition, .. } => Some(proposition),
            // RelationDeclaration and RelationWithdrawal touch two
            // propositions; callers that need both should use
            // `ResearchEvent::touched_propositions` instead.
            Self::RelationDeclaration { .. }
            | Self::RelationWithdrawal { .. }
            | Self::GoalUpdate { .. } => None,
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

/// Whether an `Accept(proposition, value)` emitted at `accept_cycle` was later
/// invalidated by the rest of the trace: a subsequent `Retraction` **or
/// `Correction`** of the same proposition, a final value of `Contradictory`,
/// or a polarity flip between the accepted value and the final value. A
/// correction counts because a corrected accepted claim is itself a
/// reliability signal — it flips a prior `Accept` outcome (retraction-events
/// contract, cross-contract compatibility with #14) — even when the corrected
/// value keeps the same polarity. Single source of truth shared by the
/// aggregate `false_acceptance_count` metric and per-source reliability
/// (`source_reliability.rs`), so the two can never drift.
pub(crate) fn accept_was_invalidated(
    proposition: &str,
    accepted_value: HexValue,
    accept_cycle: u64,
    outcomes: &[ResearchEventOutcome],
    final_beliefs: &BTreeMap<String, HexValue>,
) -> bool {
    let later_retracted = outcomes.iter().any(|later| {
        later.event.cycle > accept_cycle
            && matches!(
                &later.event.payload,
                ResearchEventPayload::Retraction { proposition: p, .. }
                    | ResearchEventPayload::Correction { proposition: p, .. }
                if p == proposition
            )
    });
    let final_value = final_beliefs.get(proposition).copied();
    let became_contradictory = matches!(final_value, Some(HexValue::Contradictory));
    let flipped_polarity = matches!(
        (accepted_value, final_value),
        (
            HexValue::True | HexValue::Probable,
            Some(HexValue::Doubtful | HexValue::False),
        ) | (
            HexValue::Doubtful | HexValue::False,
            Some(HexValue::True | HexValue::Probable),
        )
    );
    later_retracted || became_contradictory || flipped_polarity
}

/// Did a `Wait` on `proposition` turn out to be unnecessary caution?
///
/// The mirror of [`accept_was_invalidated`]: an `Accept` is wrong when the
/// claim later falls over, so a `Wait` is wrong when the claim later *stands*.
/// True when the proposition settles to `True`/`Probable` and is never
/// retracted or corrected after the wait.
///
/// Deliberately conservative in two ways. A proposition that ends
/// `Contradictory` is not counted — waiting on irreconcilable evidence is the
/// correct call, not a false rejection, which is the epistemic-humility
/// invariant showing up in the metric. And a later retraction or correction
/// clears the wait even if the claim is eventually re-asserted, because the
/// caution was justified at the time. Both make this an undercount; since the
/// metric is only ever read as a comparison *between* policies measured the
/// same way (#35 §5.3), a uniform undercount costs nothing and reduces noise.
///
/// Only `Action::Wait` is scored. `Escalate` is also abstention, but it
/// carries a `reason` rather than a proposition — there is no sound
/// attribution — and escalating a genuine contradiction is correct behavior.
pub(crate) fn wait_was_unnecessary(
    proposition: &str,
    wait_cycle: u64,
    outcomes: &[ResearchEventOutcome],
    final_beliefs: &BTreeMap<String, HexValue>,
) -> bool {
    let claim_stands = matches!(
        final_beliefs.get(proposition).copied(),
        Some(HexValue::True | HexValue::Probable)
    );
    if !claim_stands {
        return false;
    }
    let later_withdrawn = outcomes.iter().any(|later| {
        later.event.cycle > wait_cycle
            && matches!(
                &later.event.payload,
                ResearchEventPayload::Retraction { proposition: p, .. }
                    | ResearchEventPayload::Correction { proposition: p, .. }
                if p == proposition
            )
    });
    !later_withdrawn
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
                        first_contradictory.entry(prop.clone()).or_insert(cycle);
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
        let Some(&start) = first_contradictory.get(&prop) else {
            continue;
        };
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
        let cleared_by_retraction =
            matches!(&o.event.payload, ResearchEventPayload::Retraction { .. });
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
    // Doubtful/False, or moved to Contradictory. The per-Accept predicate is
    // shared with per-source reliability (`source_reliability.rs`) via
    // `accept_was_invalidated` so the two counts can never drift.
    let mut false_accepts: u32 = 0;
    for o in outcomes {
        for a in &o.actions {
            if let Action::Accept { proposition, value } = a {
                if accept_was_invalidated(
                    proposition,
                    *value,
                    o.event.cycle,
                    outcomes,
                    final_beliefs,
                ) {
                    false_accepts += 1;
                }
            }
        }
    }

    // --- false_rejection_count ---
    // The mirror of the block above. `Action::Wait` is a unit variant, so the
    // wait is attributed to the proposition its own event asserted; an event
    // with no single proposition (relation edges, goal updates) is skipped
    // rather than guessed at.
    let mut false_rejections: u32 = 0;
    for o in outcomes {
        let Some(prop) = o.event.payload.proposition() else {
            continue;
        };
        for a in &o.actions {
            if matches!(a, Action::Wait)
                && wait_was_unnecessary(prop, o.event.cycle, outcomes, final_beliefs)
            {
                false_rejections += 1;
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
            ResearchEventPayload::BeliefUpdate {
                proposition, value, ..
            }
            | ResearchEventPayload::ExperimentResult {
                proposition, value, ..
            }
            | ResearchEventPayload::AgentVote {
                proposition, value, ..
            } => (proposition.clone(), *value),
            ResearchEventPayload::Retraction { proposition, .. } => {
                (proposition.clone(), HexValue::Unknown)
            }
            // A correction asserts a replacement value; track it as a
            // (proposition, value) flip using the corrected assertion, the
            // same way the belief-bearing events use their payload value.
            ResearchEventPayload::Correction {
                proposition, value, ..
            } => (proposition.clone(), *value),
            // GoalUpdate doesn't touch beliefs; RelationDeclaration and
            // RelationWithdrawal may *cause* belief changes via propagation
            // but don't themselves emit a (proposition, value) tuple to
            // attribute; Supersession freezes a claim at its last value (no
            // new value to flip-track). All skip the per-proposition flip
            // metric.
            ResearchEventPayload::GoalUpdate { .. }
            | ResearchEventPayload::RelationDeclaration { .. }
            | ResearchEventPayload::RelationWithdrawal { .. }
            | ResearchEventPayload::Supersession { .. } => continue,
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
        false_rejection_count: false_rejections,
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
                retracts: None,
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
    fn test_priority_model_default_is_recency_decay() {
        // Post-Phase-5 substrate decision: the default is RecencyDecay.
        // See ROADMAP §"Cognition Substrate Choice". This test pins the
        // decision so it can't drift silently — switching it back to
        // Flat or to Lie requires updating both the default impl and
        // this assertion together.
        let cl = CognitiveLoop::new(4);
        assert_eq!(cl.priority_model, PriorityModel::RecencyDecay);
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

    /// #14 entrenchment-counterfactual hook: `evidence_weight` is `1.0` for
    /// every source until a table is installed, and after install it reads the
    /// table (falling back to `1.0` for an unlisted source). This pins the
    /// byte-identical default (arm U) the counterfactual measures against.
    #[test]
    fn source_weights_default_uniform_then_read_from_table() {
        let mut cl = CognitiveLoop::new(4);
        // No table installed → every source weighted 1.0 (arm U / today).
        assert_eq!(cl.evidence_weight("anyone"), 1.0);
        assert!(cl.source_weights.is_none());

        let mut table = BTreeMap::new();
        table.insert("reliable".to_string(), 0.9);
        table.insert("noisy".to_string(), 0.1);
        cl.set_source_weights(table);
        assert_eq!(cl.evidence_weight("reliable"), 0.9);
        assert_eq!(cl.evidence_weight("noisy"), 0.1);
        // Unlisted source falls back to the neutral 1.0.
        assert_eq!(cl.evidence_weight("stranger"), 1.0);
    }

    // --- false_rejection_count (#35 §5.3) ---------------------------------

    fn belief_update(cycle: u64, prop: &str, value: HexValue) -> ResearchEvent {
        ResearchEvent {
            cycle,
            source: "ix-agent-a".to_string(),
            payload: ResearchEventPayload::BeliefUpdate {
                proposition: prop.to_string(),
                value,
                evidence: Evidence::new(),
            },
        }
    }

    /// Replay `events` with abstention forced on: `theta_wait` above every
    /// achievable priority suppresses each event's action list to a single
    /// `Wait`. The existing fixture corpus never abstains on a *claim* (every
    /// Wait in it lands on a `goal_update` or `relation_declaration`, which
    /// carry no proposition), so forcing it is the only way to exercise this
    /// metric until #35's paired should-act/should-abstain fixtures exist.
    fn replay_always_waiting(events: Vec<ResearchEvent>) -> ResearchReplayReport {
        let mut cognitive_loop = CognitiveLoop::new(4);
        cognitive_loop.theta_wait = f64::INFINITY;
        cognitive_loop.process_research_trace(ResearchTrace {
            dimension: 4,
            events,
        })
    }

    #[test]
    fn wait_on_a_claim_that_goes_on_to_stand_is_a_false_rejection() {
        let report = replay_always_waiting(vec![
            belief_update(1, "benchmark-x-is-reliable", HexValue::Probable),
            belief_update(2, "benchmark-x-is-reliable", HexValue::True),
        ]);

        // The claim stands, so every wait on it was unnecessary caution.
        assert_eq!(
            report.final_beliefs.get("benchmark-x-is-reliable"),
            Some(&HexValue::True)
        );
        assert_eq!(report.metrics.false_rejection_count, 2);
        // The mirror metric must not fire — nothing was accepted.
        assert_eq!(report.metrics.false_acceptance_count, 0);
    }

    #[test]
    fn wait_on_a_claim_that_ends_contradictory_is_not_a_false_rejection() {
        let report = replay_always_waiting(vec![
            belief_update(1, "benchmark-x-is-reliable", HexValue::True),
            belief_update(2, "benchmark-x-is-reliable", HexValue::False),
        ]);

        // Waiting on irreconcilable evidence is the correct call, not a
        // false rejection — the epistemic-humility invariant, in the metric.
        assert_eq!(
            report.final_beliefs.get("benchmark-x-is-reliable"),
            Some(&HexValue::Contradictory)
        );
        assert_eq!(report.metrics.false_rejection_count, 0);
    }

    #[test]
    fn wait_cleared_by_a_later_retraction_is_not_a_false_rejection() {
        // The claim ends True, but it was withdrawn *after* the first wait,
        // so that wait was justified at the time it was made. A wait made
        // after the withdrawal is not excused.
        let report = replay_always_waiting(vec![
            belief_update(1, "benchmark-x-is-reliable", HexValue::True),
            ResearchEvent {
                cycle: 2,
                source: "ix-agent-b".to_string(),
                payload: ResearchEventPayload::Retraction {
                    proposition: "benchmark-x-is-reliable".to_string(),
                    reason: "instrument fault".to_string(),
                    retracts: None,
                },
            },
            belief_update(3, "benchmark-x-is-reliable", HexValue::True),
        ]);

        assert_eq!(
            report.final_beliefs.get("benchmark-x-is-reliable"),
            Some(&HexValue::True)
        );
        assert!(
            wait_was_unnecessary(
                "benchmark-x-is-reliable",
                3,
                &report.outcomes,
                &report.final_beliefs
            ),
            "a wait after the retraction has nothing excusing it"
        );
        assert!(
            !wait_was_unnecessary(
                "benchmark-x-is-reliable",
                1,
                &report.outcomes,
                &report.final_beliefs
            ),
            "the retraction at cycle 2 excuses the wait at cycle 1"
        );
    }

    #[test]
    fn a_wait_with_no_proposition_to_attribute_is_never_a_false_rejection() {
        // Every Wait in today's fixture corpus is of this shape. Goal and
        // relation events carry no single proposition, so a wait on one is
        // skipped rather than guessed at.
        let report = replay_always_waiting(vec![ResearchEvent {
            cycle: 1,
            source: "ix-agent-a".to_string(),
            payload: ResearchEventPayload::GoalUpdate {
                key: "characterise-benchmark-x".to_string(),
                description: "characterise benchmark x".to_string(),
                priority: 0.9,
                status: Some(HexValue::Probable),
            },
        }]);

        assert!(report.metrics.action_counts_by_kind.contains_key("Wait"));
        assert_eq!(report.metrics.false_rejection_count, 0);
    }

    // --- Forecast calibration in the replay report (#35 §9.2) -------------

    /// A scored forecast for `belief` with a hand-set Brier score, so the
    /// pooling arithmetic is tested independently of `resolve`.
    fn scored_forecast(belief: &str, id: &str, brier: f64) -> forecast::ForecastRecord {
        forecast::ForecastRecord {
            schema: "hari-forecast-record-v0.1".to_string(),
            forecast_id: id.to_string(),
            belief_id: belief.to_string(),
            emitted_at: "2026-07-28T00:00:00Z".to_string(),
            observable: forecast::Observable {
                source: "ga:state/fleet/presence.json".to_string(),
                field: "/status".to_string(),
                predicate: "== green".to_string(),
            },
            prediction: forecast::Prediction {
                probability: 0.5,
                rationale: None,
            },
            horizon: "2026-07-29T00:00:00Z".to_string(),
            resolution: Some(forecast::Resolution {
                resolved_at: "2026-07-29T00:00:00Z".to_string(),
                outcome: forecast::Outcome::True,
                observed_value: Some("green".to_string()),
                brier: Some(brier),
            }),
            links: None,
        }
    }

    #[test]
    fn replay_carries_no_calibration_by_default_and_omits_it_from_json() {
        let mut cognitive_loop = CognitiveLoop::new(4);
        let report = cognitive_loop.process_research_trace(ResearchTrace {
            dimension: 4,
            events: vec![ResearchEvent {
                cycle: 1,
                source: "ix-agent-a".to_string(),
                payload: ResearchEventPayload::BeliefUpdate {
                    proposition: "benchmark-x-is-reliable".to_string(),
                    value: HexValue::Probable,
                    evidence: Evidence::new(),
                },
            }],
        });

        // Replay reads no ledger: the block is absent, and absent from JSON,
        // so every pre-slice report stays byte-equal.
        assert!(report.calibration.is_none());
        let json = serde_json::to_string(&report).unwrap();
        assert!(!json.contains("calibration"), "unexpected key in {json}");
    }

    #[test]
    fn pooled_brier_weights_by_scored_count_not_mean_of_means() {
        // belief-a: three forecasts, all perfect (mean 0.0)
        // belief-b: one forecast, badly wrong (mean 0.8)
        let records = vec![
            scored_forecast("belief-a", "f1", 0.0),
            scored_forecast("belief-a", "f2", 0.0),
            scored_forecast("belief-a", "f3", 0.0),
            scored_forecast("belief-b", "f4", 0.8),
        ];
        let cal = ReplayCalibration::from_records(&records);

        assert_eq!(cal.scored, 4);
        assert_eq!((cal.void, cal.pending), (0, 0));
        assert_eq!(cal.per_belief.len(), 2);

        // True pooled mean over all four scored forecasts: 0.8 / 4 = 0.2.
        let pooled = cal.pooled_mean_brier.unwrap();
        assert!((pooled - 0.2).abs() < 1e-12, "pooled was {pooled}");
        // NOT the mean of the two per-belief means ((0.0 + 0.8) / 2 = 0.4),
        // which would let a belief with one forecast outvote one with three.
        assert!(
            (pooled - 0.4).abs() > 1e-9,
            "pooled collapsed to the mean of per-belief means"
        );
    }

    #[test]
    fn pooled_brier_is_none_when_nothing_scored() {
        // An unscored ledger has no calibration — distinct from a calibration
        // of 0.0, which would read as perfect.
        let cal = ReplayCalibration::from_records(&[]);
        assert_eq!((cal.scored, cal.void, cal.pending), (0, 0, 0));
        assert!(cal.pooled_mean_brier.is_none());
    }

    /// Regression: the first cut of `with_calibration` folded in the *whole*
    /// ledger, so replaying a hari fixture reported the calibration of
    /// unrelated GA/Demerzel forecasts — a number that reads as if the trace's
    /// own claims had been scored. Records must be scoped to touched beliefs.
    #[test]
    fn calibration_excludes_forecasts_about_beliefs_the_trace_never_touched() {
        let mut cognitive_loop = CognitiveLoop::new(4);
        let report = cognitive_loop
            .process_research_trace(ResearchTrace {
                dimension: 4,
                events: vec![belief_update(
                    1,
                    "benchmark-x-is-reliable",
                    HexValue::Probable,
                )],
            })
            .with_calibration(&[
                // In scope: the trace reasoned about this claim.
                scored_forecast("benchmark-x-is-reliable", "f-in", 0.10),
                // Out of scope: real ledger entries from sibling repos.
                scored_forecast("ga-chatbot-sonnet5-no-regression", "f-out-1", 0.64),
                scored_forecast("demerzel-belief-lint-reduces-warns", "f-out-2", 0.90),
            ]);

        let cal = report.calibration.expect("calibration was attached");
        assert_eq!(
            cal.per_belief.keys().collect::<Vec<_>>(),
            vec!["benchmark-x-is-reliable"],
            "out-of-scope beliefs leaked into the block"
        );
        assert_eq!(cal.scored, 1);
        // 0.10, not the three-record pooled mean (0.5467) it used to report.
        let pooled = cal.pooled_mean_brier.unwrap();
        assert!((pooled - 0.10).abs() < 1e-12, "pooled was {pooled}");
    }

    #[test]
    fn calibration_is_all_zero_when_no_forecast_concerns_this_trace() {
        // The honest answer for "nothing has been forecast about these beliefs"
        // — not a borrowed score from an unrelated belief.
        let mut cognitive_loop = CognitiveLoop::new(4);
        let report = cognitive_loop
            .process_research_trace(ResearchTrace {
                dimension: 4,
                events: vec![belief_update(
                    1,
                    "benchmark-x-is-reliable",
                    HexValue::Probable,
                )],
            })
            .with_calibration(&[scored_forecast("some-other-repos-belief", "f1", 0.25)]);

        let cal = report.calibration.expect("calibration was attached");
        assert!(cal.per_belief.is_empty());
        assert_eq!((cal.scored, cal.void, cal.pending), (0, 0, 0));
        assert!(cal.pooled_mean_brier.is_none());
    }

    #[test]
    fn attached_calibration_round_trips_through_the_json_boundary() {
        let mut cognitive_loop = CognitiveLoop::new(4);
        let report = cognitive_loop
            .process_research_trace(ResearchTrace {
                dimension: 4,
                events: vec![ResearchEvent {
                    cycle: 1,
                    source: "ix-agent-a".to_string(),
                    payload: ResearchEventPayload::BeliefUpdate {
                        proposition: "benchmark-x-is-reliable".to_string(),
                        value: HexValue::Probable,
                        evidence: Evidence::new(),
                    },
                }],
            })
            // In scope: the same claim the trace above reasons about, so the
            // record survives `with_calibration`'s trace-scoping filter.
            .with_calibration(&[scored_forecast("benchmark-x-is-reliable", "f1", 0.25)]);

        let json = serde_json::to_string(&report).unwrap();
        let back: ResearchReplayReport = serde_json::from_str(&json).unwrap();
        let cal = back
            .calibration
            .expect("calibration survives the round trip");
        assert_eq!(cal.scored, 1);
        assert!((cal.pooled_mean_brier.unwrap() - 0.25).abs() < 1e-12);
        assert_eq!(cal.per_belief["benchmark-x-is-reliable"].scored, 1);
    }
}
