//! Phase 6 — `StreamingSession`: thin streaming envelope over
//! [`crate::CognitiveLoop`].
//!
//! Per `docs/research/phase6-design.md` §7: streaming is a thin protocol
//! skin. **All cognitive work delegates to
//! [`CognitiveLoop::process_research_event`]** — there is no parallel
//! cognitive codepath. The Phase 5 long-fixture finding (Lie wins
//! `false_acceptance` 3 vs 4 on `long_recovery.json`) depends on this
//! identity.
//!
//! The session also runs an optional **shadow loop** in lockstep when
//! `compare_with` is set; both loops are deterministic on the same event
//! stream, so the trace recorder need only capture inbound events to
//! reproduce both halves on replay.
//!
//! ## Failure modes
//!
//! Implements `phase6-design.md` §6:
//!
//! - **Out-of-order cycles** (event cycle < last seen): rejected with
//!   `out_of_order_cycle`. Equal cycles are allowed (within-cycle
//!   ordering is a real IX pattern, e.g. goal_update then belief_update
//!   at cycle 1).
//! - **`event` before `open`**: never reachable here — the dispatcher in
//!   `main.rs` enforces it.
//! - **Trace IO failure**: surfaced as a fatal `trace_io` error from
//!   [`StreamingSession::open`] / [`StreamingSession::record_request`].

use crate::protocol::RecommendationResponse;
use crate::{
    action_lists_equivalent, compute_metrics_for, diff_outcomes, CognitiveLoop, Goal,
    PriorityModel, ReplayComparison, ReplayMetrics, Request, ResearchEvent, ResearchEventOutcome,
    ResearchReplayReport, Response, SessionConfig, ShadowCompare,
};
use hari_lattice::HexValue;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

/// A live streaming session over one [`CognitiveLoop`] (plus optional
/// shadow). Created via [`StreamingSession::open`]; events are applied
/// via [`StreamingSession::apply_event`]; the session is finalized via
/// [`StreamingSession::close`] which returns the same
/// [`ResearchReplayReport`] shape `replay` produces today.
pub struct StreamingSession {
    config: SessionConfig,
    primary: CognitiveLoop,
    shadow: Option<CognitiveLoop>,
    outcomes: Vec<ResearchEventOutcome>,
    /// Shadow outcomes when `compare_with` is set. Same length as
    /// `outcomes`.
    shadow_outcomes: Vec<ResearchEventOutcome>,
    /// Propositions touched by any event (so close-time `final_beliefs`
    /// has the same shape as batch).
    touched_propositions: BTreeMap<String, ()>,
    last_cycle: Option<u64>,
    recorder: Option<TraceRecorder>,
    session_id: String,
    closed: bool,
}

/// JSONL trace recorder. Appends one line per inbound request and flushes
/// after each line so a panic / EOF mid-session leaves the file
/// replayable up to the last accepted event.
pub struct TraceRecorder {
    path: PathBuf,
    writer: BufWriter<std::fs::File>,
}

impl TraceRecorder {
    /// Open `path` for append (creating if needed).
    pub fn open(path: &Path) -> std::io::Result<Self> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        Ok(Self {
            path: path.to_path_buf(),
            writer: BufWriter::new(file),
        })
    }

    /// Append a serializable record as one JSONL line, then flush.
    pub fn record<T: Serialize>(&mut self, value: &T) -> std::io::Result<()> {
        let s = serde_json::to_string(value)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        self.writer.write_all(s.as_bytes())?;
        self.writer.write_all(b"\n")?;
        self.writer.flush()
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl StreamingSession {
    /// Open a new streaming session from a [`SessionConfig`]. Constructs
    /// the primary `CognitiveLoop` (and optional shadow) and seeds
    /// initial goals. If `trace_record_path` is set, opens the recorder
    /// and writes the `open` request as the first line.
    ///
    /// Errors are surfaced as `Err` strings so the dispatcher can
    /// translate them into `Response::Error { code: "trace_io", fatal:
    /// true }`.
    pub fn open(config: SessionConfig) -> Result<Self, String> {
        let primary = build_loop(&config, config.priority_model);
        let shadow = config.compare_with.map(|m| build_loop(&config, m));

        let recorder = match &config.trace_record_path {
            Some(path) => match TraceRecorder::open(path) {
                Ok(r) => Some(r),
                Err(e) => {
                    return Err(format!("trace_io: cannot open {}: {}", path.display(), e));
                }
            },
            None => None,
        };

        let session_id = synth_session_id();
        let mut session = Self {
            config,
            primary,
            shadow,
            outcomes: Vec::new(),
            shadow_outcomes: Vec::new(),
            touched_propositions: BTreeMap::new(),
            last_cycle: None,
            recorder,
            session_id,
            closed: false,
        };

        // Record the `open` line first so replay sees it as the session
        // header. We pass a Request::Open clone; SessionConfig is Clone.
        if session.recorder.is_some() {
            let header = Request::Open {
                config: session.config.clone(),
            };
            if let Err(e) = session.record_request(&header) {
                return Err(format!("trace_io: {e}"));
            }
        }

        Ok(session)
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn config(&self) -> &SessionConfig {
        &self.config
    }

    pub fn trace_path(&self) -> Option<&Path> {
        self.recorder.as_ref().map(|r| r.path())
    }

    /// Append a request to the trace file (if any).
    pub fn record_request(&mut self, req: &Request) -> std::io::Result<()> {
        if let Some(rec) = self.recorder.as_mut() {
            rec.record(req)
        } else {
            Ok(())
        }
    }

    /// Apply one event to the cognitive loop and return the
    /// recommendation. **MUST** delegate to
    /// `CognitiveLoop::process_research_event` (see module docs).
    ///
    /// Out-of-order cycles are rejected by [`Self::check_cycle`].
    pub fn apply_event(&mut self, event: ResearchEvent) -> Result<RecommendationResponse, String> {
        if self.closed {
            return Err("session_closed: session is already closed".into());
        }
        self.check_cycle(&event)?;

        // Track every proposition the event touches — including both
        // endpoints of a `RelationDeclaration`, which `_owned()`
        // returns None for. Necessary so a session that *only*
        // declares relations (and lets propagation derive everything)
        // still has the derived propositions in `final_beliefs`.
        for prop in event.touched_propositions() {
            self.touched_propositions.insert(prop, ());
        }

        // *** The single load-bearing call: the cognitive path ***
        let outcome = self.primary.process_research_event(event.clone());

        // Shadow runs the same event through its own loop. The shadow's
        // outcome is captured for batch-equivalent comparison at close
        // time, and the per-event divergence flag is exposed in the
        // recommendation response.
        let shadow_outcome = if let Some(shadow_loop) = self.shadow.as_mut() {
            Some(shadow_loop.process_research_event(event))
        } else {
            None
        };

        let event_index = self.outcomes.len();
        let actions = outcome.actions.clone();
        let state_summary = outcome.state_summary.clone();
        self.outcomes.push(outcome);

        let compare = if let Some(shadow_outcome) = shadow_outcome {
            let diverged = !action_lists_equivalent(&actions, &shadow_outcome.actions);
            let cmp = ShadowCompare {
                shadow_model: self
                    .config
                    .compare_with
                    .expect("compare_with must be Some when shadow exists"),
                shadow_actions: shadow_outcome.actions.clone(),
                diverged,
            };
            self.shadow_outcomes.push(shadow_outcome);
            Some(cmp)
        } else {
            None
        };

        let running_metrics = self.compute_running_metrics();

        Ok(RecommendationResponse {
            event_index,
            actions,
            state_summary,
            running_metrics,
            compare,
        })
    }

    /// Cheap snapshot of running metrics, plus a copy of beliefs and goals.
    pub fn metrics_snapshot(
        &self,
    ) -> (
        ReplayMetrics,
        BTreeMap<String, HexValue>,
        BTreeMap<String, Goal>,
    ) {
        let metrics = self.compute_running_metrics();
        let beliefs = self.current_final_beliefs(&self.primary);
        let goals = self.primary.state.goals.clone();
        (metrics, beliefs, goals)
    }

    /// Finalize the session and return the equivalent of what
    /// `process_research_trace` would produce in batch on the same
    /// stream of events. When the session was opened with `compare_with`,
    /// `comparison` is populated with the same `ReplayComparison` shape
    /// `compare_replay` produces (per design doc §7).
    pub fn close(mut self) -> ResearchReplayReport {
        self.closed = true;
        let comparison = self.build_comparison_if_any();
        let final_beliefs = self.current_final_beliefs(&self.primary);
        let final_goals = self.primary.state.goals.clone();
        let metrics = compute_metrics_for(
            &self.outcomes,
            &final_beliefs,
            &final_goals,
            self.primary.attention_norm_max(),
        );
        let final_state_summary = self.primary.state.summary();
        let priority_model = self.primary.priority_model;

        ResearchReplayReport {
            event_count: self.outcomes.len(),
            outcomes: self.outcomes,
            final_beliefs,
            final_goals,
            final_state_summary,
            priority_model,
            metrics,
            comparison,
        }
    }

    /// Build the `ReplayComparison` block — primary path metrics vs
    /// shadow path metrics — when `compare_with` was set on `open`.
    /// Returns `None` if no shadow loop was configured.
    fn build_comparison_if_any(&self) -> Option<ReplayComparison> {
        let shadow_loop = self.shadow.as_ref()?;
        let shadow_beliefs = self.current_final_beliefs(shadow_loop);
        let shadow_metrics = compute_metrics_for(
            &self.shadow_outcomes,
            &shadow_beliefs,
            &shadow_loop.state.goals,
            shadow_loop.attention_norm_max(),
        );
        let primary_beliefs = self.current_final_beliefs(&self.primary);
        let primary_metrics = compute_metrics_for(
            &self.outcomes,
            &primary_beliefs,
            &self.primary.state.goals,
            self.primary.attention_norm_max(),
        );
        let action_divergence = diff_outcomes(&self.shadow_outcomes, &self.outcomes);
        // Convention: `baseline` is the shadow (the comparison model
        // requested via `compare_with`), `experimental` is the primary
        // (the model the session is actually driving). This matches
        // `compare_replay`, which puts `RecencyDecay` on baseline and
        // `Lie` on experimental — i.e. the experiment is the `Lie`
        // primary, the baseline is the shadow `RecencyDecay`.
        Some(ReplayComparison {
            baseline: shadow_metrics,
            experimental: primary_metrics,
            action_divergence,
        })
    }

    fn check_cycle(&mut self, event: &ResearchEvent) -> Result<(), String> {
        if let Some(last) = self.last_cycle {
            if event.cycle < last {
                return Err(format!(
                    "out_of_order_cycle: event cycle={} < last seen cycle={}; cycle must be monotonic non-decreasing",
                    event.cycle, last
                ));
            }
        }
        self.last_cycle = Some(event.cycle);
        Ok(())
    }

    fn compute_running_metrics(&self) -> ReplayMetrics {
        let final_beliefs = self.current_final_beliefs(&self.primary);
        compute_metrics_for(
            &self.outcomes,
            &final_beliefs,
            &self.primary.state.goals,
            self.primary.attention_norm_max(),
        )
    }

    fn current_final_beliefs(&self, loop_: &CognitiveLoop) -> BTreeMap<String, HexValue> {
        self.touched_propositions
            .keys()
            .filter_map(|prop| {
                loop_
                    .state
                    .beliefs
                    .get(prop)
                    .map(|p| (prop.clone(), p.value))
            })
            .collect()
    }

    /// Used by `serve` to detect whether the session has been closed by
    /// the client.
    pub fn is_closed(&self) -> bool {
        self.closed
    }

    /// Mark this session as closed without consuming it. Used by the
    /// dispatcher when the client sends an explicit `close` so that
    /// subsequent `event` requests are rejected with `session_closed`.
    pub fn mark_closed(&mut self) {
        self.closed = true;
    }

    /// Snapshot a `ResearchReplayReport` without consuming the session.
    /// Used to build the `Closed` response while still allowing the
    /// dispatcher to write a final `close` line to the trace file.
    pub fn snapshot_report(&self) -> ResearchReplayReport {
        let comparison = self.build_comparison_if_any();
        let final_beliefs = self.current_final_beliefs(&self.primary);
        let final_goals = self.primary.state.goals.clone();
        let metrics = compute_metrics_for(
            &self.outcomes,
            &final_beliefs,
            &final_goals,
            self.primary.attention_norm_max(),
        );
        let final_state_summary = self.primary.state.summary();
        let priority_model = self.primary.priority_model;
        ResearchReplayReport {
            event_count: self.outcomes.len(),
            outcomes: self.outcomes.clone(),
            final_beliefs,
            final_goals,
            final_state_summary,
            priority_model,
            metrics,
            comparison,
        }
    }

    /// Pop and return the response for an out-of-order request that
    /// should be surfaced as a typed error. Borrowed via the dispatcher,
    /// so the dispatcher can decide whether to also record-and-skip.
    pub fn make_error(
        request_op: &str,
        code: &str,
        message: impl Into<String>,
        fatal: bool,
    ) -> Response {
        Response::Error {
            request_op: Some(request_op.into()),
            code: code.into(),
            message: message.into(),
            fatal,
        }
    }
}

fn build_loop(config: &SessionConfig, model: PriorityModel) -> CognitiveLoop {
    let mut loop_ = CognitiveLoop::with_model(config.dimension, model);
    loop_.theta_wait = config.theta_wait;
    loop_.lie_dt = config.lie_dt;
    loop_.lie_alpha = config.lie_alpha;
    loop_.lambda_decay = config.lambda_decay;
    loop_.use_swarm_consensus = config.use_swarm_consensus;
    loop_.trust_model = config.trust_model;
    for goal in &config.initial_goals {
        loop_
            .state
            .add_goal(goal.key.clone(), goal.description.clone(), goal.priority);
        if let Some(status) = goal.status {
            if let Some(g) = loop_.state.goals.get_mut(&goal.key) {
                g.status = status;
            }
        }
    }
    // Seed declared agents into the swarm so their roles are available
    // to `consensus_with` from the very first vote. Sources that don't
    // appear here are auto-created with a neutral role on first vote.
    for seed in &config.initial_agents {
        loop_
            .swarm
            .add_agent(hari_swarm::Agent::new(seed.id.clone(), seed.role.clone()));
    }
    loop_
}

fn synth_session_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    // Cheap, dependency-free session id. Replay parity does not depend on
    // it being globally unique — the trace file is the source of truth.
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("hari-{now}")
}
