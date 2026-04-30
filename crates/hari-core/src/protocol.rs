//! Phase 6 — IX autoresearch streaming protocol envelope types.
//!
//! Per `docs/research/phase6-design.md` §3 ("Session protocol"). The
//! transport is **stdio JSONL**: one JSON object per line on stdin (a
//! [`Request`]) eliciting one JSON object per line on stdout (a
//! [`Response`]). The protocol is strictly synchronous request/response —
//! Hari never initiates. See the design doc for rationale (replay parity,
//! no parallel cognitive codepath, no async runtime).
//!
//! Both enums are serde-tagged on `op` with `snake_case` rename so the
//! wire format matches the design doc verbatim.
//!
//! ## Replay parity
//!
//! When a session is opened with `trace_record_path` set, every inbound
//! [`Request`] is appended verbatim to that file. `replay --session
//! <file>` then re-feeds those lines through the same [`Request`]
//! deserializer and through the same `process_research_event` codepath as
//! the live `serve` mode — guaranteeing byte-identical
//! `ResearchReplayReport`s. See `phase6-design.md` §5.

use crate::{
    Action, Goal, PriorityModel, ReplayMetrics, ResearchEvent, ResearchReplayReport,
};
use hari_lattice::HexValue;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// SessionConfig — applied at `open` time (immutable for the session)
// ---------------------------------------------------------------------------

/// Per-session configuration accepted on `open`.
///
/// See `docs/research/phase6-design.md` §4 ("Configuration surface"). All
/// fields except `compare_with`, `trace_record_path`, and `initial_goals`
/// have defaults that match the in-tree `CognitiveLoop::new(4)` field
/// values — so an empty `SessionConfig::default()` reproduces today's
/// batch behaviour.
///
/// `lie_alpha` and `lie_dt` defaults are **pinned** by
/// `divergence_test_pins_alpha_and_dt` in `phase5_replay.rs`. Do not
/// change them without auditing the cognition_divergence fixture.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    /// Cognitive state space dimension.
    #[serde(default = "default_dimension")]
    pub dimension: usize,
    /// Strategy used to score and filter actions.
    #[serde(default)]
    pub priority_model: PriorityModel,
    /// Below-this-priority actions become `Action::Wait`.
    #[serde(default = "default_theta_wait")]
    pub theta_wait: f64,
    /// Lie evolution timestep.
    #[serde(default = "default_lie_dt")]
    pub lie_dt: f64,
    /// Lie scoring/Hamiltonian coupling strength.
    #[serde(default = "default_lie_alpha")]
    pub lie_alpha: f64,
    /// `RecencyDecay` rate.
    #[serde(default = "default_lambda_decay")]
    pub lambda_decay: f64,

    /// If set, every event is also processed through a shadow loop with
    /// this priority model. Recommendation responses include a `compare`
    /// block. Per the design doc, this is **per-session**, not per-event,
    /// so the shadow loop's attention vector evolves consistently with
    /// the entire history.
    #[serde(default)]
    pub compare_with: Option<PriorityModel>,

    /// If set, every inbound request is appended to this path as JSONL
    /// for replay parity. Recommended for any session that must be
    /// reproducible.
    #[serde(default)]
    pub trace_record_path: Option<PathBuf>,

    /// Goals seeded before the first event. Equivalent to sending
    /// `goal_update` events at cycle 0. Empty by default.
    #[serde(default)]
    pub initial_goals: Vec<InitialGoal>,
}

/// A goal seeded at session open. See [`SessionConfig::initial_goals`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitialGoal {
    pub key: String,
    pub description: String,
    pub priority: f64,
    #[serde(default)]
    pub status: Option<HexValue>,
}

fn default_dimension() -> usize {
    4
}
fn default_theta_wait() -> f64 {
    0.1
}
fn default_lie_dt() -> f64 {
    0.5
}
fn default_lie_alpha() -> f64 {
    2.0
}
fn default_lambda_decay() -> f64 {
    0.2
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            dimension: default_dimension(),
            priority_model: PriorityModel::default(),
            theta_wait: default_theta_wait(),
            lie_dt: default_lie_dt(),
            lie_alpha: default_lie_alpha(),
            lambda_decay: default_lambda_decay(),
            compare_with: None,
            trace_record_path: None,
            initial_goals: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Request / Response — the wire envelope
// ---------------------------------------------------------------------------

/// Inbound request (IX → Hari). Tagged on `op`.
///
/// Variants per `docs/research/phase6-design.md` §3 ("Message shapes").
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// Open a new session with the given config. Must be the first
    /// request on a connection.
    Open { config: SessionConfig },
    /// Apply a research event. Wraps the existing [`ResearchEvent`] —
    /// streaming does not duplicate the type.
    Event { event: ResearchEvent },
    /// Cheap on-demand snapshot of running metrics + current beliefs/goals.
    Metrics,
    /// Close the session and return the final `ResearchReplayReport`.
    Close,
}

/// Outbound response (Hari → IX). Tagged on `op`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    /// Reply to `open`.
    Opened {
        session_id: String,
        trace_path: Option<PathBuf>,
        hari_version: String,
        config_echo: SessionConfig,
    },
    /// Reply to `event`.
    Recommendation(RecommendationResponse),
    /// Reply to `metrics`.
    MetricsSnapshot {
        metrics: ReplayMetrics,
        beliefs: BTreeMap<String, HexValue>,
        goals: BTreeMap<String, Goal>,
    },
    /// Reply to `close`. Final report has the same shape `replay`
    /// produces today.
    Closed {
        final_report: ResearchReplayReport,
        /// True when the session was finalized after stdin EOF (or some
        /// other mid-flight termination). Replay-parity files for unclean
        /// sessions still parse — replay applies events up to the last
        /// fully-flushed line, then synthesizes a `close` marker.
        #[serde(default)]
        unclean: bool,
    },
    /// Typed protocol error (non-fatal unless `fatal == true`). See
    /// `phase6-design.md` §6 ("Failure modes").
    Error {
        request_op: Option<String>,
        code: String,
        message: String,
        #[serde(default)]
        fatal: bool,
    },
}

/// Per-event recommendation. See `phase6-design.md` §3.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecommendationResponse {
    pub event_index: usize,
    pub actions: Vec<Action>,
    pub state_summary: String,
    pub running_metrics: ReplayMetrics,
    /// Populated only when `compare_with` was set on `open`.
    #[serde(default)]
    pub compare: Option<ShadowCompare>,
}

/// Side-by-side shadow output for a single event. See
/// [`SessionConfig::compare_with`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShadowCompare {
    pub shadow_model: PriorityModel,
    pub shadow_actions: Vec<Action>,
    pub diverged: bool,
}
