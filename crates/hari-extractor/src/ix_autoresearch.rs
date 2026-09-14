//! `hari-from-ix-autoresearch` — the consumer IX's autoresearch contract names.
//!
//! IX's `crates/ix-autoresearch/SCHEMA.md` pins an append-only JSONL run log
//! (`schema_version: 1`, events `run_start` / `iteration` / `run_complete`) and
//! names this adapter as its Hari consumer. This module is that adapter: a pure,
//! deterministic projection from recorded IX logs to a [`ResearchTrace`], plus a
//! run report that replays the projection under the #35 arms and sets each arm's
//! decisions beside the decision IX itself made.
//!
//! Recorded logs, not live runs: pairing needs one recording replayed under
//! every arm (pre-registration `2026-07-28` §3). The projected events are
//! ordinary `ResearchEvent`s, so the same stream also drives the Phase-6
//! `serve` session — `stream_parity` in the tests pins that the two paths agree.
//!
//! # Projection (SCHEMA.md layer 2, as Hari reads it)
//!
//! One `iteration` line becomes one `experiment_result`:
//!
//! * `proposition` — `{target}/config-{hash12}-is-an-improvement`, where
//!   `target` is the module segment of `run_start.target`
//!   (`ix_autoresearch::target_grammar::GrammarTarget` → `target_grammar`).
//! * `value` — `Probable` when IX accepted, `Doubtful` when it rejected,
//!   `Unknown` when the evaluation errored. SCHEMA.md pegs an errored line at
//!   confidence 0.10; Hari reads a failed evaluation as *no evidence* about the
//!   config rather than evidence against it, and says so here instead of
//!   rounding 0.10 to `False`.
//! * `source` — `ix-autoresearch/{run_id}`; `cycle` — position in the projected
//!   stream, 1-based, so recency decay never fires on a recorded stream
//!   (§9.6's natural stamping).
//! * `evidence` — the raw fields: `run_id`, `iteration`, `config_hash`,
//!   `accepted`, `reward`, `elapsed_ms`, `cache_hit`, plus `error` and
//!   `strategy_state` when present.
//!
//! Contradiction is *not* computed here. SCHEMA.md's `contradicted_by` is the
//! consumer's job, and in Hari the belief network does it by merging repeated
//! observations of one proposition.
//!
//! # What the run report may and may not be used for
//!
//! It is instrument characterisation over a recorded IX stream. It is **not** a
//! §8 keep/kill input and makes no decision-quality claim: the label rule below
//! is descriptive, not a pre-registered ground truth.

use std::collections::{BTreeMap, BTreeSet};

use hari_core::{
    compare_replay_three_way, replay_unassisted, Action, ResearchEvent, ResearchEventPayload,
    ResearchReplayReport, ResearchTrace, SubjectiveLogicConfig,
};
use hari_lattice::HexValue;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

/// The only raw-log schema version this adapter reads.
pub const IX_SCHEMA_VERSION: u64 = 1;

/// Identifies the run report's own shape.
pub const RUN_REPORT_SCHEMA: &str = "hari/ix-autoresearch-run-report/v0.1";

/// Why a log could not be read.
#[derive(Debug, Error, PartialEq)]
pub enum IxLogError {
    /// A line other than the last failed to parse. SCHEMA.md: mid-stream parse
    /// failure is a hard error; only a trailing one is crash truncation.
    #[error("line {line}: not valid JSON mid-stream: {detail}")]
    MidStreamParse { line: usize, detail: String },
    #[error("line {line}: schema_version {found}, this adapter reads {IX_SCHEMA_VERSION}")]
    SchemaVersion { line: usize, found: u64 },
    #[error("line {line}: {detail}")]
    Shape { line: usize, detail: String },
    #[error("log has no run_start line")]
    MissingRunStart,
}

/// One `iteration` line, keeping only the fields the projection reads.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IxIteration {
    pub iteration: u64,
    pub config_hash: String,
    #[serde(default)]
    pub reward: Option<f64>,
    pub accepted: bool,
    #[serde(default)]
    pub previous_hash: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub elapsed_ms: u64,
    #[serde(default)]
    pub strategy_state: Option<Value>,
    #[serde(default)]
    pub cache_hit: bool,
}

/// One recorded IX run.
#[derive(Debug, Clone, PartialEq)]
pub struct IxRun {
    pub run_id: String,
    pub target: String,
    pub strategy: Value,
    pub seed: u64,
    pub iterations: Vec<IxIteration>,
    /// `run_complete` was present. Its absence means an interrupted run, which
    /// SCHEMA.md makes replay-tolerant.
    pub complete: bool,
}

/// Parse one raw SCHEMA-v1 log.
pub fn parse_log(raw: &str) -> Result<IxRun, IxLogError> {
    let lines: Vec<(usize, &str)> = raw
        .lines()
        .enumerate()
        .map(|(i, l)| (i + 1, l.trim()))
        .filter(|(_, l)| !l.is_empty())
        .collect();
    let last = lines.last().map(|(n, _)| *n);

    let mut run: Option<IxRun> = None;
    for (line, text) in &lines {
        let v: Value = match serde_json::from_str(text) {
            Ok(v) => v,
            Err(_) if Some(*line) == last => break,
            Err(e) => {
                return Err(IxLogError::MidStreamParse {
                    line: *line,
                    detail: e.to_string(),
                })
            }
        };
        let shape = |detail: &str| IxLogError::Shape {
            line: *line,
            detail: detail.to_string(),
        };
        let version = v
            .get("schema_version")
            .and_then(Value::as_u64)
            .ok_or_else(|| shape("missing schema_version"))?;
        if version != IX_SCHEMA_VERSION {
            return Err(IxLogError::SchemaVersion {
                line: *line,
                found: version,
            });
        }
        match v.get("event").and_then(Value::as_str) {
            Some("run_start") => {
                let field = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
                run = Some(IxRun {
                    run_id: field("run_id").ok_or_else(|| shape("run_start without run_id"))?,
                    target: field("target").ok_or_else(|| shape("run_start without target"))?,
                    strategy: v.get("strategy").cloned().unwrap_or(Value::Null),
                    seed: v.get("seed").and_then(Value::as_u64).unwrap_or_default(),
                    iterations: Vec::new(),
                    complete: false,
                });
            }
            Some("iteration") => {
                let it: IxIteration =
                    serde_json::from_value(v.clone()).map_err(|e| shape(&e.to_string()))?;
                run.as_mut()
                    .ok_or(IxLogError::MissingRunStart)?
                    .iterations
                    .push(it);
            }
            Some("run_complete") => {
                run.as_mut().ok_or(IxLogError::MissingRunStart)?.complete = true
            }
            _ => return Err(shape("unknown or missing `event`")),
        }
    }
    run.ok_or(IxLogError::MissingRunStart)
}

/// `ix_autoresearch::target_grammar::GrammarTarget` → `target_grammar`.
fn target_slug(target: &str) -> &str {
    let segments: Vec<&str> = target.split("::").collect();
    if segments.len() >= 2 {
        segments[segments.len() - 2]
    } else {
        target
    }
}

/// The proposition an iteration asserts about its candidate config.
#[must_use]
pub fn claim_for(target: &str, config_hash: &str) -> String {
    let hex = config_hash
        .strip_prefix("autoresearch:")
        .unwrap_or(config_hash);
    let short: String = hex.chars().take(12).collect();
    format!("{}/config-{short}-is-an-improvement", target_slug(target))
}

fn asserted_value(it: &IxIteration) -> HexValue {
    match (&it.error, it.accepted) {
        (Some(_), _) => HexValue::Unknown,
        (None, true) => HexValue::Probable,
        (None, false) => HexValue::Doubtful,
    }
}

/// Project one or more runs, in the order given, into one trace.
#[must_use]
pub fn project(runs: &[IxRun]) -> ResearchTrace {
    let mut events = Vec::new();
    for run in runs {
        for it in &run.iterations {
            let mut evidence = BTreeMap::new();
            evidence.insert("run_id".to_string(), Value::from(run.run_id.clone()));
            evidence.insert("iteration".to_string(), Value::from(it.iteration));
            evidence.insert(
                "config_hash".to_string(),
                Value::from(it.config_hash.clone()),
            );
            evidence.insert("accepted".to_string(), Value::from(it.accepted));
            evidence.insert(
                "reward".to_string(),
                it.reward.map_or(Value::Null, Value::from),
            );
            evidence.insert("elapsed_ms".to_string(), Value::from(it.elapsed_ms));
            evidence.insert("cache_hit".to_string(), Value::from(it.cache_hit));
            if let Some(error) = &it.error {
                evidence.insert("error".to_string(), Value::from(error.clone()));
            }
            if let Some(state) = &it.strategy_state {
                evidence.insert("strategy_state".to_string(), state.clone());
            }
            events.push(ResearchEvent {
                cycle: events.len() as u64 + 1,
                source: format!("ix-autoresearch/{}", run.run_id),
                payload: ResearchEventPayload::ExperimentResult {
                    proposition: claim_for(&run.target, &it.config_hash),
                    value: asserted_value(it),
                    evidence,
                },
            });
        }
    }
    ResearchTrace::from(events)
}

/// Candidate ground truth for one iteration: did the candidate's reward beat
/// the incumbent it was proposed against?
///
/// The incumbent before iteration *i* is the config named by iteration *i−1*'s
/// `previous_hash` (the post-decision incumbent). Iteration 0 of a run is
/// unlabeled — its incumbent is the baseline, whose reward the log does not
/// carry — and so is any errored iteration. Derived mechanically from recorded
/// rewards, never from any arm's output, and never transmitted to Hari.
#[must_use]
pub fn improvement_labels(run: &IxRun) -> Vec<Option<bool>> {
    let rewards: BTreeMap<&str, f64> = run
        .iterations
        .iter()
        .filter_map(|it| it.reward.map(|r| (it.config_hash.as_str(), r)))
        .collect();
    let mut incumbent: Option<&str> = None;
    run.iterations
        .iter()
        .map(|it| {
            let label = match (incumbent.and_then(|h| rewards.get(h)), it.reward) {
                (Some(prev), Some(r)) => Some(r > *prev),
                _ => None,
            };
            incumbent = it.previous_hash.as_deref();
            label
        })
        .collect()
}

/// What an arm did about one iteration's claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimDecision {
    /// Committed to the config being an improvement (`Accept` at `True`/`Probable`).
    Endorse,
    /// Committed to it not being one (`Accept` at `Doubtful`/`False`).
    Reject,
    /// Committed to neither (`Wait`, `Investigate`, `Escalate`, or nothing).
    Withhold,
}

fn decision_from_actions(actions: &[Action]) -> ClaimDecision {
    for action in actions {
        if let Action::Accept { value, .. } = action {
            return match value {
                HexValue::True | HexValue::Probable => ClaimDecision::Endorse,
                HexValue::Doubtful | HexValue::False => ClaimDecision::Reject,
                HexValue::Unknown | HexValue::Contradictory => ClaimDecision::Withhold,
            };
        }
    }
    ClaimDecision::Withhold
}

/// One arm's decisions over the whole stream.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArmSummary {
    pub arm: String,
    pub endorse: usize,
    pub reject: usize,
    pub withhold: usize,
    /// Iterations where this arm's decision differs from IX's own.
    pub differs_from_ix_policy: usize,
    /// Labeled iterations graded below.
    pub graded: usize,
    /// Endorsed a candidate that did not beat its incumbent.
    pub false_endorsements: usize,
    /// Did not endorse a candidate that did beat its incumbent.
    pub missed_improvements: usize,
    /// Propositions this arm ends holding as `Contradictory`. `None` for
    /// `ix_policy`, which holds no belief state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contradictory_final_beliefs: Option<usize>,
}

/// Provenance of one run in the report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunSummary {
    pub run_id: String,
    pub target: String,
    pub strategy: Value,
    pub seed: u64,
    pub iterations: usize,
    pub ix_accepted: usize,
    pub complete: bool,
}

/// The run report: every arm's decisions over one recorded stream.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IxRunReport {
    pub schema: String,
    pub runs: Vec<RunSummary>,
    /// Claim assertions in the stream (one per iteration).
    pub claims: usize,
    pub distinct_propositions: usize,
    /// Propositions asserted more than once — the precondition for any
    /// contradiction to exist at all (hari#13 §6 gate).
    pub repeated_propositions: usize,
    /// Of those, how many were asserted with conflicting accepted flags.
    pub conflicting_propositions: usize,
    pub label_rule: String,
    /// `ix_policy` (IX's own accept flag — the run without Hari),
    /// `ix_unassisted`, `recency_decay`, `subjective_logic`. `Lie` is not a
    /// #35 §4 arm and is not reported.
    pub arms: Vec<ArmSummary>,
}

fn summarize(
    arm: &str,
    decisions: &[ClaimDecision],
    ix: &[ClaimDecision],
    labels: &[Option<bool>],
    report: Option<&ResearchReplayReport>,
) -> ArmSummary {
    let mut s = ArmSummary {
        arm: arm.to_string(),
        contradictory_final_beliefs: report.map(|r| {
            r.final_beliefs
                .values()
                .filter(|v| **v == HexValue::Contradictory)
                .count()
        }),
        ..ArmSummary::default()
    };
    for ((d, ix_d), label) in decisions.iter().zip(ix).zip(labels) {
        match d {
            ClaimDecision::Endorse => s.endorse += 1,
            ClaimDecision::Reject => s.reject += 1,
            ClaimDecision::Withhold => s.withhold += 1,
        }
        if d != ix_d {
            s.differs_from_ix_policy += 1;
        }
        if let Some(improved) = label {
            s.graded += 1;
            let endorsed = *d == ClaimDecision::Endorse;
            if endorsed && !improved {
                s.false_endorsements += 1;
            }
            if !endorsed && *improved {
                s.missed_improvements += 1;
            }
        }
    }
    s
}

/// Replay the projected stream under each arm and summarise the decisions.
#[must_use]
pub fn run_report(runs: &[IxRun]) -> IxRunReport {
    let trace = project(runs);
    let labels: Vec<Option<bool>> = runs.iter().flat_map(improvement_labels).collect();
    let iterations: Vec<&IxIteration> = runs.iter().flat_map(|r| &r.iterations).collect();

    // Read off the raw log, not the projection, so a projection bug shows up as
    // an arm departing from IX rather than moving both sides together.
    let ix: Vec<ClaimDecision> = iterations
        .iter()
        .map(|it| match (&it.error, it.accepted) {
            (Some(_), _) => ClaimDecision::Withhold,
            (None, true) => ClaimDecision::Endorse,
            (None, false) => ClaimDecision::Reject,
        })
        .collect();

    let decisions = |report: &ResearchReplayReport| -> Vec<ClaimDecision> {
        report
            .outcomes
            .iter()
            .map(|o| decision_from_actions(&o.actions))
            .collect()
    };

    let unassisted = replay_unassisted(trace.clone());
    let three = compare_replay_three_way(trace, SubjectiveLogicConfig::default());

    let mut by_claim: BTreeMap<String, BTreeSet<bool>> = BTreeMap::new();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for run in runs {
        for it in &run.iterations {
            let claim = claim_for(&run.target, &it.config_hash);
            *counts.entry(claim.clone()).or_default() += 1;
            by_claim.entry(claim).or_default().insert(it.accepted);
        }
    }

    IxRunReport {
        schema: RUN_REPORT_SCHEMA.to_string(),
        runs: runs
            .iter()
            .map(|r| RunSummary {
                run_id: r.run_id.clone(),
                target: r.target.clone(),
                strategy: r.strategy.clone(),
                seed: r.seed,
                iterations: r.iterations.len(),
                ix_accepted: r.iterations.iter().filter(|it| it.accepted).count(),
                complete: r.complete,
            })
            .collect(),
        claims: iterations.len(),
        distinct_propositions: counts.len(),
        repeated_propositions: counts.values().filter(|n| **n > 1).count(),
        conflicting_propositions: by_claim.values().filter(|s| s.len() > 1).count(),
        label_rule: "improved := reward > reward of the incumbent before this iteration \
                     (previous iteration's previous_hash); iteration 0 and errored \
                     iterations unlabeled. Descriptive rule, not a pre-registered ground truth."
            .to_string(),
        arms: vec![
            summarize("ix_policy", &ix, &ix, &labels, None),
            summarize(
                "ix_unassisted",
                &decisions(&unassisted),
                &ix,
                &labels,
                Some(&unassisted),
            ),
            summarize(
                "recency_decay",
                &decisions(&three.recency_decay),
                &ix,
                &labels,
                Some(&three.recency_decay),
            ),
            summarize(
                "subjective_logic",
                &decisions(&three.subjective_logic),
                &ix,
                &labels,
                Some(&three.subjective_logic),
            ),
        ],
    }
}
