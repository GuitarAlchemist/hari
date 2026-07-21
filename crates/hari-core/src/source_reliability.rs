//! # Cross-session source reliability — issue #14 tracer bullet
//!
//! Trust earned by *outcomes*, not only configured by role. Where
//! [`crate::reliability`] (Giskard G2) grades *agents* from GA's PR-grade
//! cards, this module grades **any source** — the `source` field of a
//! [`crate::ResearchEvent`] (agent / runner / benchmark / tool / …) — from the
//! fate of the claims it made inside replayed IX traces, and accumulates that
//! judgement **across sessions** in an append-only JSONL ledger.
//!
//! ## What it computes
//!
//! For one finished replay, [`outcomes_from_report`] folds the per-event
//! outcomes into one immutable [`SourceOutcomeRecord`] per source, tallying:
//! - `accepted` — `Accept` actions emitted on that source's events;
//! - `false_acceptances` — of those, the ones the rest of the trace later
//!   invalidated (retracted / went Contradictory / flipped polarity), via the
//!   shared [`crate::accept_was_invalidated`] predicate so the per-source count
//!   can never drift from the aggregate `false_acceptance_count` metric;
//! - `escalations` / `false_escalations` — `Escalate` actions whose
//!   proposition nonetheless ended True/Probable (the alarm turned out fine).
//!
//! [`report`] then sums those rows across every session in the ledger into a
//! [`SourceReliabilityReport`]: a `pooled` entry (all sources together — the
//! **A/B baseline** any per-source claim must beat, same doctrine as G2) plus a
//! per-source split, and an explicit **entrenchment ordering**
//! ([`SourceReliabilityReport::entrenchment`]) — sources ranked by earned
//! precision. That ordering is the read-only realisation of
//! `docs/research/2026-07-20-compounding-strategy.md` §5: *source reliability =
//! AGM epistemic entrenchment*, the missing ranking over conflicting base
//! evidence. It is **surfaced, never auto-applied** — no default trust changes,
//! no hidden global mutable state (issue #14 non-goals). Same "exists and is
//! honest, nothing automated consumes it yet" scope as G1/G2.
//!
//! ## Design constraints (inherited from `reliability.rs` / `operator_model.rs`)
//! - Append-only JSONL, one file per emission day, under
//!   `HARI_STATE_DIR/source-reliability/`; rows are immutable and carry
//!   `session_id` + `recorded_at` for audit (append-only lineage, never
//!   rewritten — compounding-strategy §4.6).
//! - Timestamps are *injected* and validated against
//!   [`crate::forecast::is_canonical_utc`] at the write boundary.
//! - Smoothing reuses G2's pinned prior (weight 2.0, mean 0.5) so the two
//!   reliability views compose; retuning is explicit, pinned by
//!   [`tests::pins_prior_and_trust_thresholds`].
//! - Honest degradation: a source with zero `Accept`s has `precision: None`
//!   (never a NaN); an unreadable/absent ledger is empty, not fatal.

use crate::{Action, ResearchEventOutcome};
use hari_lattice::HexValue;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

/// Schema of one persisted per-source, per-session tally row.
pub const SOURCE_OUTCOME_SCHEMA: &str = "hari-source-outcome-v0.1.0";
/// Schema of the aggregated report this module emits.
pub const SOURCE_RELIABILITY_REPORT_SCHEMA: &str = "hari-source-reliability-report-v0.1.0";

/// Pseudo-count weight of the neutral prior in the smoothed precision — same
/// value as [`crate::reliability::PRIOR_WEIGHT`] so the two views compose.
pub const PRIOR_WEIGHT: f64 = 2.0;
/// Neutral prior mean: an unobserved source scores 0.5 (honest "no idea").
pub const PRIOR_MEAN: f64 = 0.5;

/// Minimum `accepted` count before a source is eligible to out-rank the pooled
/// baseline in the entrenchment ordering — small samples stay pinned to the
/// prior. Pinned by [`tests::pins_prior_and_trust_thresholds`].
pub const MIN_ACCEPTED_FOR_TRUST: u32 = 3;
/// Margin a source's smoothed precision must clear over the pooled baseline's
/// before `beats_pooled_baseline` is set. Pinned alongside the prior.
pub const TRUST_MARGIN: f64 = 0.05;

/// The type of source a record is about. The current trace boundary only
/// exposes the `source` *string* and the payload *channel* (which mechanism it
/// spoke through), so this defaults to [`SourceType::Unknown`]; the richer
/// taxonomy from issue #14 (`model` / `repo` / `research_digest` / …) needs
/// producer-supplied metadata that traces don't yet carry — see the design
/// doc's open questions. Kept as an additive field so producers can annotate it
/// later without a schema break (same degradation discipline as G2's `agent`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    Agent,
    Model,
    Benchmark,
    Repo,
    Tool,
    ResearchDigest,
    HumanReview,
    IxAutoresearchRun,
    #[default]
    Unknown,
}

/// One immutable per-source, per-session tally. The cross-session ledger is an
/// append-only list of these; [`report`] sums them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceOutcomeRecord {
    pub schema: String,
    /// Which replay/session produced this row (audit + dedup key).
    pub session_id: String,
    /// The source identity — the `source` field of the events it made.
    pub source: String,
    /// Reserved producer annotation; `Unknown` until traces carry it.
    #[serde(default)]
    pub source_type: SourceType,
    pub recorded_at: String,
    /// Belief-bearing events (`belief_update` / `experiment_result` /
    /// `agent_vote`) this source emitted in the session.
    pub claims: u32,
    /// `Accept` actions emitted on this source's events.
    pub accepted: u32,
    /// Of `accepted`, the ones the rest of the trace later invalidated.
    pub false_acceptances: u32,
    /// `Escalate` actions emitted on this source's events (attributable to a
    /// proposition).
    pub escalations: u32,
    /// Of `escalations`, the ones whose proposition ended True/Probable — the
    /// alarm turned out unwarranted.
    pub false_escalations: u32,
}

/// Fold one finished replay into per-source tally rows.
///
/// `outcomes` and `final_beliefs` are the `outcomes` / `final_beliefs` fields
/// of a [`crate::ResearchReplayReport`]. Only sources that emitted at least one
/// belief-bearing event get a row (a source that only declared relations or
/// goals made no claim to grade). Rows are returned sorted by source id for
/// deterministic output and stable ledger diffs.
pub fn outcomes_from_report(
    session_id: &str,
    recorded_at: &str,
    outcomes: &[ResearchEventOutcome],
    final_beliefs: &BTreeMap<String, HexValue>,
) -> Vec<SourceOutcomeRecord> {
    // Per-source running tallies.
    #[derive(Default)]
    struct Tally {
        claims: u32,
        accepted: u32,
        false_acceptances: u32,
        escalations: u32,
        false_escalations: u32,
    }
    let mut by_source: BTreeMap<String, Tally> = BTreeMap::new();

    for o in outcomes {
        let source = o.event.source.clone();
        let prop = o.event.payload.proposition().map(str::to_string);

        // A claim = a belief-bearing event (has a proposition *and* carries a
        // value). Retraction/goal/relation events are not claims to grade.
        let is_belief_bearing = matches!(
            &o.event.payload,
            crate::ResearchEventPayload::BeliefUpdate { .. }
                | crate::ResearchEventPayload::ExperimentResult { .. }
                | crate::ResearchEventPayload::AgentVote { .. }
        );
        if is_belief_bearing {
            by_source.entry(source.clone()).or_default().claims += 1;
        }

        for action in &o.actions {
            match action {
                Action::Accept { proposition, value } => {
                    let t = by_source.entry(source.clone()).or_default();
                    t.accepted += 1;
                    if crate::accept_was_invalidated(
                        proposition,
                        *value,
                        o.event.cycle,
                        outcomes,
                        final_beliefs,
                    ) {
                        t.false_acceptances += 1;
                    }
                }
                Action::Escalate { .. } => {
                    // Attribute the escalation to the event's own proposition;
                    // an escalation on an event with no single proposition
                    // (goal/relation) has nothing to grade against.
                    let Some(prop) = prop.as_deref() else {
                        continue;
                    };
                    let t = by_source.entry(source.clone()).or_default();
                    t.escalations += 1;
                    // "False" escalation: we alarmed, but the belief ended
                    // fine (True/Probable). A durable Contradictory/False/
                    // Doubtful end state means the alarm was warranted.
                    if matches!(
                        final_beliefs.get(prop),
                        Some(HexValue::True | HexValue::Probable)
                    ) {
                        t.false_escalations += 1;
                    }
                }
                _ => {}
            }
        }
    }

    by_source
        .into_iter()
        .map(|(source, t)| SourceOutcomeRecord {
            schema: SOURCE_OUTCOME_SCHEMA.to_string(),
            session_id: session_id.to_string(),
            source,
            source_type: SourceType::Unknown,
            recorded_at: recorded_at.to_string(),
            claims: t.claims,
            accepted: t.accepted,
            false_acceptances: t.false_acceptances,
            escalations: t.escalations,
            false_escalations: t.false_escalations,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Aggregation across sessions.
// ---------------------------------------------------------------------------

/// Summed tallies for one bucket (a single source, or `pooled`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SourceReliabilityEntry {
    /// Number of ledger rows (sessions) contributing to this bucket.
    pub sessions: usize,
    pub claims: u32,
    pub accepted: u32,
    pub false_acceptances: u32,
    pub escalations: u32,
    pub false_escalations: u32,
    /// `accepted_claim_precision` = `1 - false_acceptances / accepted`; `None`
    /// when `accepted == 0` (no NaN — honest "no accepted claims yet").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accepted_claim_precision: Option<f64>,
    /// Prior-smoothed precision, defined for every bucket:
    /// `((accepted - false_acceptances) + PRIOR_WEIGHT·PRIOR_MEAN) /
    /// (accepted + PRIOR_WEIGHT)`. Equals [`PRIOR_MEAN`] at `accepted == 0`.
    pub smoothed_precision: f64,
    /// `false_escalations / escalations`; `None` when `escalations == 0`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub false_escalation_rate: Option<f64>,
}

impl SourceReliabilityEntry {
    fn from_rows(rows: &[&SourceOutcomeRecord]) -> Self {
        let mut e = SourceReliabilityEntry {
            sessions: rows.len(),
            ..Default::default()
        };
        for r in rows {
            e.claims += r.claims;
            e.accepted += r.accepted;
            e.false_acceptances += r.false_acceptances;
            e.escalations += r.escalations;
            e.false_escalations += r.false_escalations;
        }
        let correct = e.accepted.saturating_sub(e.false_acceptances) as f64;
        e.accepted_claim_precision = (e.accepted > 0).then(|| correct / e.accepted as f64);
        e.smoothed_precision =
            (correct + PRIOR_WEIGHT * PRIOR_MEAN) / (e.accepted as f64 + PRIOR_WEIGHT);
        e.false_escalation_rate =
            (e.escalations > 0).then(|| e.false_escalations as f64 / e.escalations as f64);
        e
    }
}

/// One rung of the entrenchment ordering: a source, its earned precision, and
/// whether it has earned trust *above* the pooled baseline. Read-only — this is
/// the ranking a future consumer (conflicting-base-evidence resolution) *may*
/// gate on, surfaced for audit, never auto-applied.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceEntrenchment {
    pub source: String,
    pub smoothed_precision: f64,
    pub accepted: u32,
    /// `true` iff `accepted >= MIN_ACCEPTED_FOR_TRUST` and this source's
    /// smoothed precision exceeds the pooled baseline's by at least
    /// [`TRUST_MARGIN`]. The A/B claim: "trust this source over the pool."
    pub beats_pooled_baseline: bool,
}

/// The aggregated cross-session report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceReliabilityReport {
    pub schema: String,
    pub generated_at: String,
    /// Total ledger rows ingested / malformed lines skipped.
    pub rows: usize,
    pub skipped: usize,
    /// All sources pooled — the simpler baseline any per-source trust claim
    /// must beat before a consumer prefers the split (A/B doctrine).
    pub pooled: SourceReliabilityEntry,
    pub by_source: BTreeMap<String, SourceReliabilityEntry>,
    /// Sources ranked by earned precision (desc, then by id): the epistemic
    /// entrenchment ordering (compounding-strategy §5). Surfaced, not applied.
    pub entrenchment: Vec<SourceEntrenchment>,
}

/// Aggregate ledger rows into the cross-session report. `generated_at` is
/// injected so the report is deterministic under test.
pub fn report(
    rows: &[SourceOutcomeRecord],
    skipped: usize,
    generated_at: &str,
) -> SourceReliabilityReport {
    let pooled = SourceReliabilityEntry::from_rows(&rows.iter().collect::<Vec<_>>());

    let mut grouped: BTreeMap<String, Vec<&SourceOutcomeRecord>> = BTreeMap::new();
    for r in rows {
        grouped.entry(r.source.clone()).or_default().push(r);
    }
    let by_source: BTreeMap<String, SourceReliabilityEntry> = grouped
        .into_iter()
        .map(|(source, rs)| (source, SourceReliabilityEntry::from_rows(&rs)))
        .collect();

    // Entrenchment ordering: rank by smoothed precision (desc), then source id
    // (asc) for determinism. `beats_pooled_baseline` is the A/B-able trust
    // claim against the pooled smoothed precision.
    let mut entrenchment: Vec<SourceEntrenchment> = by_source
        .iter()
        .map(|(source, e)| SourceEntrenchment {
            source: source.clone(),
            smoothed_precision: e.smoothed_precision,
            accepted: e.accepted,
            beats_pooled_baseline: e.accepted >= MIN_ACCEPTED_FOR_TRUST
                && e.smoothed_precision >= pooled.smoothed_precision + TRUST_MARGIN,
        })
        .collect();
    entrenchment.sort_by(|a, b| {
        b.smoothed_precision
            .partial_cmp(&a.smoothed_precision)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.source.cmp(&b.source))
    });

    SourceReliabilityReport {
        schema: SOURCE_RELIABILITY_REPORT_SCHEMA.to_string(),
        generated_at: generated_at.to_string(),
        rows: rows.len(),
        skipped,
        pooled,
        by_source,
        entrenchment,
    }
}

// ---------------------------------------------------------------------------
// Ledger — append-only JSONL, one file per emission day, under
// `HARI_STATE_DIR/source-reliability/` (default `state/source-reliability/`).
// ---------------------------------------------------------------------------

pub fn ledger_dir() -> PathBuf {
    std::env::var_os("HARI_STATE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("state"))
        .join("source-reliability")
}

/// Append one row to the day file named after its `recorded_at` date. The
/// timestamp must be canonical UTC (`YYYY-MM-DDTHH:MM:SSZ`) — rejected at the
/// write boundary for the same lexicographic-ordering reason as the forecast
/// and operator ledgers.
pub fn append(dir: &Path, record: &SourceOutcomeRecord) -> io::Result<PathBuf> {
    let ts = &record.recorded_at;
    if !crate::forecast::is_canonical_utc(ts) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("recorded_at is not canonical YYYY-MM-DDTHH:MM:SSZ UTC: {ts:?}"),
        ));
    }
    fs::create_dir_all(dir)?;
    let path = dir.join(format!("{}.jsonl", &ts[..10]));
    let line =
        serde_json::to_string(record).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    writeln!(file, "{line}")?;
    Ok(path)
}

/// Load the whole ledger in append order (day-file name, then line order).
/// Malformed lines are skipped with a count so a corrupt row can't take the
/// ledger down. A missing directory is an empty ledger.
pub fn load(dir: &Path) -> io::Result<(Vec<SourceOutcomeRecord>, usize)> {
    let mut rows = Vec::new();
    let mut skipped = 0usize;
    if !dir.exists() {
        return Ok((rows, 0));
    }
    let mut files: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
        .collect();
    files.sort();
    for path in files {
        for line in fs::read_to_string(&path)?.lines() {
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<SourceOutcomeRecord>(line) {
                Ok(r) => rows.push(r),
                Err(_) => skipped += 1,
            }
        }
    }
    Ok((rows, skipped))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forecast::uuidv7;
    use crate::{ResearchEvent, ResearchEventPayload};

    fn temp_dir(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("hari-source-rel-{tag}-{}", uuidv7()))
    }

    /// Build a belief-bearing outcome with an explicit action list. Lets tests
    /// pin `outcomes_from_report` without depending on loop internals.
    fn outcome(
        cycle: u64,
        source: &str,
        proposition: &str,
        value: HexValue,
        actions: Vec<Action>,
    ) -> ResearchEventOutcome {
        ResearchEventOutcome {
            event: ResearchEvent {
                cycle,
                source: source.to_string(),
                payload: ResearchEventPayload::BeliefUpdate {
                    proposition: proposition.to_string(),
                    value,
                    evidence: Default::default(),
                },
            },
            actions,
            state_summary: String::new(),
            derivations: Vec::new(),
            revisions: Vec::new(),
        }
    }

    fn retraction(cycle: u64, source: &str, proposition: &str) -> ResearchEventOutcome {
        ResearchEventOutcome {
            event: ResearchEvent {
                cycle,
                source: source.to_string(),
                payload: ResearchEventPayload::Retraction {
                    proposition: proposition.to_string(),
                    reason: "test".to_string(),
                    retracts: None,
                },
            },
            actions: Vec::new(),
            state_summary: String::new(),
            derivations: Vec::new(),
            revisions: Vec::new(),
        }
    }

    #[test]
    fn pins_prior_and_trust_thresholds() {
        // Same convention as the Lie tunables / G2 prior: retuning is explicit.
        assert_eq!((PRIOR_WEIGHT, PRIOR_MEAN), (2.0, 0.5));
        assert_eq!(MIN_ACCEPTED_FOR_TRUST, 3);
        assert!((TRUST_MARGIN - 0.05).abs() < 1e-12);
    }

    #[test]
    fn accept_later_retracted_is_a_false_acceptance_for_that_source() {
        // good-source accepts P and it holds True; bad-source accepts Q and it
        // is later retracted → one false acceptance attributed to bad-source.
        let outcomes = vec![
            outcome(
                1,
                "good-source",
                "p",
                HexValue::Probable,
                vec![Action::Accept {
                    proposition: "p".into(),
                    value: HexValue::Probable,
                }],
            ),
            outcome(
                2,
                "bad-source",
                "q",
                HexValue::Probable,
                vec![Action::Accept {
                    proposition: "q".into(),
                    value: HexValue::Probable,
                }],
            ),
            retraction(3, "runner", "q"),
        ];
        let mut final_beliefs = BTreeMap::new();
        final_beliefs.insert("p".to_string(), HexValue::Probable);
        final_beliefs.insert("q".to_string(), HexValue::Unknown); // retraction cleared it

        let rows =
            outcomes_from_report("sess-1", "2026-07-20T12:00:00Z", &outcomes, &final_beliefs);
        let by: BTreeMap<_, _> = rows.iter().map(|r| (r.source.clone(), r)).collect();

        assert_eq!(by["good-source"].accepted, 1);
        assert_eq!(by["good-source"].false_acceptances, 0);
        assert_eq!(by["bad-source"].accepted, 1);
        assert_eq!(by["bad-source"].false_acceptances, 1);
        // The retraction event carries no Accept and is not belief-bearing, so
        // "runner" gets no row at all.
        assert!(!by.contains_key("runner"));
    }

    #[test]
    fn escalation_ending_probable_is_a_false_escalation() {
        let outcomes = vec![outcome(
            1,
            "nervous-source",
            "p",
            HexValue::Probable,
            vec![Action::Escalate {
                reason: "looks contradictory".into(),
                confidence: 0.9,
            }],
        )];
        let mut final_beliefs = BTreeMap::new();
        final_beliefs.insert("p".to_string(), HexValue::True); // ended fine

        let rows =
            outcomes_from_report("sess-1", "2026-07-20T12:00:00Z", &outcomes, &final_beliefs);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].escalations, 1);
        assert_eq!(rows[0].false_escalations, 1);
    }

    #[test]
    fn report_pooled_equals_sum_and_entrenchment_ranks_by_precision() {
        // Two sessions of the same two sources. good-source: 4 accepts, 0 bad;
        // bad-source: 4 accepts, 3 bad.
        let rows = vec![
            SourceOutcomeRecord {
                schema: SOURCE_OUTCOME_SCHEMA.into(),
                session_id: "s1".into(),
                source: "good-source".into(),
                source_type: SourceType::Unknown,
                recorded_at: "2026-07-20T12:00:00Z".into(),
                claims: 2,
                accepted: 2,
                false_acceptances: 0,
                escalations: 0,
                false_escalations: 0,
            },
            SourceOutcomeRecord {
                schema: SOURCE_OUTCOME_SCHEMA.into(),
                session_id: "s2".into(),
                source: "good-source".into(),
                source_type: SourceType::Unknown,
                recorded_at: "2026-07-20T13:00:00Z".into(),
                claims: 2,
                accepted: 2,
                false_acceptances: 0,
                escalations: 0,
                false_escalations: 0,
            },
            SourceOutcomeRecord {
                schema: SOURCE_OUTCOME_SCHEMA.into(),
                session_id: "s1".into(),
                source: "bad-source".into(),
                source_type: SourceType::Unknown,
                recorded_at: "2026-07-20T12:00:00Z".into(),
                claims: 4,
                accepted: 4,
                false_acceptances: 3,
                escalations: 0,
                false_escalations: 0,
            },
        ];
        let rep = report(&rows, 0, "2026-07-20T18:00:00Z");

        // Pooled = sum: 8 accepted, 3 false → precision 0.625.
        assert_eq!(rep.pooled.accepted, 8);
        assert_eq!(rep.pooled.false_acceptances, 3);
        assert!((rep.pooled.accepted_claim_precision.unwrap() - 0.625).abs() < 1e-12);

        // Per-source: good has 2 sessions, 4 accepts, precision 1.0.
        let good = &rep.by_source["good-source"];
        assert_eq!(good.sessions, 2);
        assert_eq!(good.accepted, 4);
        assert!((good.accepted_claim_precision.unwrap() - 1.0).abs() < 1e-12);
        // bad: precision 0.25.
        assert!(
            (rep.by_source["bad-source"]
                .accepted_claim_precision
                .unwrap()
                - 0.25)
                .abs()
                < 1e-12
        );

        // Entrenchment: good ranks first and beats the pooled baseline (4 >= 3
        // accepted, smoothed precision well above pooled + margin); bad does
        // not.
        assert_eq!(rep.entrenchment[0].source, "good-source");
        assert!(rep.entrenchment[0].beats_pooled_baseline);
        let bad_rung = rep
            .entrenchment
            .iter()
            .find(|e| e.source == "bad-source")
            .unwrap();
        assert!(!bad_rung.beats_pooled_baseline);
    }

    #[test]
    fn empty_ledger_reports_prior_not_nan() {
        let rep = report(&[], 0, "2026-07-20T18:00:00Z");
        assert_eq!(rep.rows, 0);
        assert_eq!(rep.pooled.accepted, 0);
        assert!(rep.pooled.accepted_claim_precision.is_none());
        assert!((rep.pooled.smoothed_precision - PRIOR_MEAN).abs() < 1e-12);
        assert!(rep.by_source.is_empty());
        assert!(rep.entrenchment.is_empty());
    }

    #[test]
    fn append_load_roundtrip_and_report_over_two_days() {
        let dir = temp_dir("roundtrip");
        let r1 = SourceOutcomeRecord {
            schema: SOURCE_OUTCOME_SCHEMA.into(),
            session_id: "s1".into(),
            source: "src-a".into(),
            source_type: SourceType::Unknown,
            recorded_at: "2026-07-19T12:00:00Z".into(),
            claims: 1,
            accepted: 1,
            false_acceptances: 0,
            escalations: 0,
            false_escalations: 0,
        };
        let r2 = SourceOutcomeRecord {
            recorded_at: "2026-07-20T12:00:00Z".into(),
            session_id: "s2".into(),
            ..r1.clone()
        };
        append(&dir, &r1).unwrap();
        append(&dir, &r2).unwrap();

        let (loaded, skipped) = load(&dir).unwrap();
        assert_eq!(skipped, 0);
        assert_eq!(loaded.len(), 2);
        let rep = report(&loaded, skipped, "2026-07-20T18:00:00Z");
        assert_eq!(rep.by_source["src-a"].sessions, 2);
        assert_eq!(rep.by_source["src-a"].accepted, 2);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn append_rejects_non_canonical_timestamp() {
        let dir = temp_dir("noncanon");
        let bad = SourceOutcomeRecord {
            schema: SOURCE_OUTCOME_SCHEMA.into(),
            session_id: "s".into(),
            source: "src".into(),
            source_type: SourceType::Unknown,
            recorded_at: "2026-07-20T12:00:00.5Z".into(),
            claims: 0,
            accepted: 0,
            false_acceptances: 0,
            escalations: 0,
            false_escalations: 0,
        };
        let err = append(&dir, &bad).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        assert!(!dir.exists());
    }

    #[test]
    fn load_skips_malformed_lines_never_fatal() {
        let dir = temp_dir("malformed");
        fs::create_dir_all(&dir).unwrap();
        let good = serde_json::to_string(&SourceOutcomeRecord {
            schema: SOURCE_OUTCOME_SCHEMA.into(),
            session_id: "s".into(),
            source: "src".into(),
            source_type: SourceType::Unknown,
            recorded_at: "2026-07-20T12:00:00Z".into(),
            claims: 1,
            accepted: 1,
            false_acceptances: 0,
            escalations: 0,
            false_escalations: 0,
        })
        .unwrap();
        fs::write(
            dir.join("2026-07-20.jsonl"),
            format!("{good}\n{{ not json\n"),
        )
        .unwrap();
        let (rows, skipped) = load(&dir).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(skipped, 1);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn report_serializes_with_schema_header() {
        let rep = report(&[], 0, "2026-07-20T18:00:00Z");
        let v: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&rep).unwrap()).unwrap();
        assert_eq!(v["schema"], SOURCE_RELIABILITY_REPORT_SCHEMA);
        // n == 0 pooled never serializes a NaN precision — it is simply absent.
        assert!(v["pooled"]["accepted_claim_precision"].is_null());
        assert!(v["pooled"]["smoothed_precision"].is_number());
    }
}
