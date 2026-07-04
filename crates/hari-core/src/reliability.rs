//! # Agent reliability model — Giskard Track G2 tracer bullet
//!
//! Mentalics-of-agents: ingest GA's post-merge PR grade cards
//! (`ga:state/quality/pr-grades/<merge-sha>.json`, schema `pr-grade-v1`,
//! produced by GA's `/grade-last-pr` skill) and compute a per-agent,
//! per-task-class reliability report.
//!
//! Scope pinned by the BACKLOG G2 tracer: the model *exists and is honest* —
//! no automated decision consumes it yet. Routing integration is a later
//! slice and a cross-repo contract (tribunal required).
//!
//! Design constraints:
//! - Grade cards without the additive `agent` field degrade explicitly to
//!   agent `"unknown"` — never silently dropped, never guessed.
//! - The report always carries the `pooled` entry (all agents together):
//!   that is the simpler baseline any consumer must beat before trusting
//!   the per-agent split (roadmap rule: every milestone A/B-able against a
//!   simpler baseline).
//! - Smoothing is a fixed pseudo-count prior pinned by
//!   `test_pins_prior_and_alignment_scores`, so any retuning is explicit —
//!   same convention as the Lie tunables pin.
//! - Read-only compute: the grade ledger lives in GA; hari models it and
//!   writes nothing.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

/// Version of the *report* this module emits (the input schema is GA's
/// `pr-grade-v1`, owned by `ga:state/quality/pr-grades/SCHEMA.json`).
pub const RELIABILITY_REPORT_SCHEMA: &str = "hari-agent-reliability-report-v0.1.0";

/// Input schema literal this module accepts.
pub const GRADE_SCHEMA: &str = "pr-grade-v1";

/// Pseudo-count weight of the neutral prior in the smoothed score.
pub const PRIOR_WEIGHT: f64 = 2.0;
/// Neutral prior mean (an unobserved agent scores 0.5 — honest "no idea").
pub const PRIOR_MEAN: f64 = 0.5;

/// The `alignment` enum of `pr-grade-v1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Alignment {
    High,
    Medium,
    Low,
}

/// Alignment → outcome score in `[0, 1]`.
pub fn alignment_score(alignment: Alignment) -> f64 {
    match alignment {
        Alignment::High => 1.0,
        Alignment::Medium => 0.5,
        Alignment::Low => 0.0,
    }
}

/// The slice of a `pr-grade-v1` card this module reads. Unknown fields are
/// ignored so additive producer changes never break ingestion.
#[derive(Debug, Clone, Deserialize)]
pub struct GradeCard {
    pub schema: String,
    pub pr_number: u64,
    pub merge_sha: String,
    pub title: String,
    pub alignment: Alignment,
    /// Additive `pr-grade-v1` field: identity that *authored* the PR
    /// (e.g. `google-labs-jules[bot]`, a lane, or a human login). Cards
    /// graded before the field existed have `None` → agent `"unknown"`.
    #[serde(default)]
    pub agent: Option<String>,
}

/// Task class = conventional-commit prefix of the PR title, lowercased,
/// scope and `!` stripped: `feat` / `fix` / `docs` / `refactor` / `test` /
/// `chore` / `perf` / `ci` / `build` / `style` — anything else → `other`.
pub fn task_class(title: &str) -> String {
    const KNOWN: [&str; 10] = [
        "feat", "fix", "docs", "refactor", "test", "chore", "perf", "ci", "build", "style",
    ];
    let head = title.split(':').next().unwrap_or("");
    let head = head.split('(').next().unwrap_or("").trim();
    let head = head.strip_suffix('!').unwrap_or(head).to_ascii_lowercase();
    if KNOWN.contains(&head.as_str()) {
        head
    } else {
        "other".to_string()
    }
}

/// Read every `*.json` grade card in `dir`. Files that fail to parse or
/// whose `schema` is not [`GRADE_SCHEMA`] (e.g. `SCHEMA.json` itself) are
/// counted as skipped, never fatal. A missing directory is an empty ledger.
pub fn load_grades(dir: &Path) -> io::Result<(Vec<GradeCard>, usize)> {
    let mut cards = Vec::new();
    let mut skipped = 0usize;
    if !dir.exists() {
        return Ok((cards, skipped));
    }
    let mut paths: Vec<_> = fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    paths.sort();
    for path in paths {
        match fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<GradeCard>(&text).ok())
        {
            Some(card) if card.schema == GRADE_SCHEMA => cards.push(card),
            _ => skipped += 1,
        }
    }
    Ok((cards, skipped))
}

/// Aggregate over one bucket of outcome scores.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ReliabilityEntry {
    pub n: usize,
    /// Raw mean of outcome scores; absent when `n == 0`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mean: Option<f64>,
    /// `(sum + PRIOR_WEIGHT · PRIOR_MEAN) / (n + PRIOR_WEIGHT)` — pulls
    /// small samples toward the neutral prior; equals [`PRIOR_MEAN`] at
    /// `n == 0`.
    pub smoothed: f64,
}

impl ReliabilityEntry {
    fn from_scores(scores: &[f64]) -> Self {
        let n = scores.len();
        let sum: f64 = scores.iter().sum();
        ReliabilityEntry {
            n,
            mean: (n > 0).then(|| sum / n as f64),
            smoothed: (sum + PRIOR_WEIGHT * PRIOR_MEAN) / (n as f64 + PRIOR_WEIGHT),
        }
    }
}

/// Per-agent view: overall entry + per-task-class split.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct AgentReliability {
    pub overall: ReliabilityEntry,
    pub by_class: BTreeMap<String, ReliabilityEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReliabilityReport {
    pub schema: String,
    pub generated_at: String,
    /// Cards ingested / files skipped (non-cards, malformed).
    pub graded_prs: usize,
    pub skipped: usize,
    /// The simpler baseline: all agents pooled. A consumer preferring the
    /// per-agent split over this pooled score is making an A/B-able claim.
    pub pooled: ReliabilityEntry,
    pub by_agent: BTreeMap<String, AgentReliability>,
}

/// Build the report. `generated_at` is injected (not read from the clock)
/// so report generation is deterministic under test.
pub fn report(cards: &[GradeCard], skipped: usize, generated_at: &str) -> ReliabilityReport {
    let mut pooled: Vec<f64> = Vec::with_capacity(cards.len());
    let mut per_agent: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    let mut per_agent_class: BTreeMap<String, BTreeMap<String, Vec<f64>>> = BTreeMap::new();
    for card in cards {
        let score = alignment_score(card.alignment);
        let agent = card.agent.clone().unwrap_or_else(|| "unknown".to_string());
        let class = task_class(&card.title);
        pooled.push(score);
        per_agent.entry(agent.clone()).or_default().push(score);
        per_agent_class
            .entry(agent)
            .or_default()
            .entry(class)
            .or_default()
            .push(score);
    }
    let by_agent = per_agent
        .into_iter()
        .map(|(agent, scores)| {
            let by_class = per_agent_class
                .remove(&agent)
                .unwrap_or_default()
                .into_iter()
                .map(|(class, s)| (class, ReliabilityEntry::from_scores(&s)))
                .collect();
            (
                agent,
                AgentReliability {
                    overall: ReliabilityEntry::from_scores(&scores),
                    by_class,
                },
            )
        })
        .collect();
    ReliabilityReport {
        schema: RELIABILITY_REPORT_SCHEMA.to_string(),
        generated_at: generated_at.to_string(),
        graded_prs: cards.len(),
        skipped,
        pooled: ReliabilityEntry::from_scores(&pooled),
        by_agent,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn card(agent: Option<&str>, title: &str, alignment: &str, sha_seed: u8) -> String {
        let mut v = json!({
            "schema": "pr-grade-v1",
            "pr_number": 100 + sha_seed as u64,
            "merge_sha": format!("{:040x}", sha_seed as u128),
            "merged_at": "2026-07-04T12:00:00Z",
            "title": title,
            "stated_intent": title,
            "actual_files_changed": ["src/lib.rs"],
            "alignment": alignment,
            "reasons": ["test fixture"],
            "graded_at": "2026-07-04T12:05:00Z",
            "grader": "test-model"
        });
        if let Some(a) = agent {
            v["agent"] = json!(a);
        }
        v.to_string()
    }

    fn parse(s: &str) -> GradeCard {
        serde_json::from_str(s).unwrap()
    }

    /// Retuning the prior or the alignment→score map must be explicit —
    /// same pinning convention as the Lie tunables.
    #[test]
    fn test_pins_prior_and_alignment_scores() {
        assert_eq!((PRIOR_WEIGHT, PRIOR_MEAN), (2.0, 0.5));
        assert_eq!(alignment_score(Alignment::High), 1.0);
        assert_eq!(alignment_score(Alignment::Medium), 0.5);
        assert_eq!(alignment_score(Alignment::Low), 0.0);
    }

    #[test]
    fn task_class_derivation_handles_scope_bang_case_and_unknown() {
        assert_eq!(task_class("feat: add thing"), "feat");
        assert_eq!(task_class("fix(cortex): catch LLM exceptions"), "fix");
        assert_eq!(task_class("refactor!: breaking cleanup"), "refactor");
        assert_eq!(task_class("Docs(readme): typo"), "docs");
        assert_eq!(task_class("Close the pattern-selector seam"), "other");
        assert_eq!(task_class(""), "other");
    }

    #[test]
    fn grade_card_parses_with_and_without_agent_field() {
        let with = parse(&card(Some("google-labs-jules[bot]"), "feat: x", "high", 1));
        assert_eq!(with.agent.as_deref(), Some("google-labs-jules[bot]"));
        // Cards graded before the additive field existed: agent is None,
        // which the report degrades to "unknown" (never dropped).
        let without = parse(&card(None, "fix: y", "low", 2));
        assert!(without.agent.is_none());
        assert_eq!(without.alignment, Alignment::Low);
    }

    #[test]
    fn report_math_pooled_and_per_agent_split() {
        let cards = vec![
            parse(&card(Some("jules"), "feat: a", "high", 1)),
            parse(&card(Some("jules"), "feat: b", "medium", 2)),
            parse(&card(Some("jules"), "fix: c", "high", 3)),
            parse(&card(None, "docs: d", "low", 4)),
        ];
        let rep = report(&cards, 0, "2026-07-04T18:00:00Z");
        assert_eq!(rep.graded_prs, 4);

        // Pooled baseline: (1 + 0.5 + 1 + 0) / 4 = 0.625;
        // smoothed: (2.5 + 1) / 6 = 0.5833…
        assert_eq!(rep.pooled.n, 4);
        assert!((rep.pooled.mean.unwrap() - 0.625).abs() < 1e-12);
        assert!((rep.pooled.smoothed - 3.5 / 6.0).abs() < 1e-12);

        // jules overall: (1 + 0.5 + 1) / 3; smoothed (2.5 + 1) / 5 = 0.7.
        let jules = &rep.by_agent["jules"];
        assert_eq!(jules.overall.n, 3);
        assert!((jules.overall.smoothed - 0.7).abs() < 1e-12);
        // jules/feat: n=2, mean 0.75; jules/fix: n=1, mean 1.0.
        assert_eq!(jules.by_class["feat"].n, 2);
        assert!((jules.by_class["feat"].mean.unwrap() - 0.75).abs() < 1e-12);
        assert_eq!(jules.by_class["fix"].n, 1);

        // The agent-less card lands under "unknown", not dropped.
        assert_eq!(rep.by_agent["unknown"].overall.n, 1);
        assert_eq!(rep.by_agent["unknown"].by_class["docs"].n, 1);
    }

    #[test]
    fn empty_ledger_reports_prior_not_nan() {
        let rep = report(&[], 0, "2026-07-04T18:00:00Z");
        assert_eq!(rep.graded_prs, 0);
        assert_eq!(rep.pooled.n, 0);
        assert!(rep.pooled.mean.is_none());
        assert!((rep.pooled.smoothed - PRIOR_MEAN).abs() < 1e-12);
        assert!(rep.by_agent.is_empty());
    }

    #[test]
    fn load_grades_skips_schema_file_and_malformed_never_fatal() {
        let dir = std::env::temp_dir().join(format!(
            "hari-reliability-test-{}",
            crate::forecast::uuidv7()
        ));
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("a1.json"),
            card(Some("jules"), "feat: a", "high", 1),
        )
        .unwrap();
        // The ledger directory contains SCHEMA.json (a JSON Schema, not a
        // card) and README.md alongside the cards — both must be ignored.
        fs::write(dir.join("SCHEMA.json"), r#"{"$schema": "x", "title": "y"}"#).unwrap();
        fs::write(dir.join("README.md"), "# not json").unwrap();
        fs::write(dir.join("broken.json"), "{ not json").unwrap();

        let (cards, skipped) = load_grades(&dir).unwrap();
        assert_eq!(cards.len(), 1);
        assert_eq!(skipped, 2); // SCHEMA.json + broken.json; README.md not scanned
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn missing_directory_is_an_empty_ledger() {
        let dir = std::env::temp_dir().join(format!(
            "hari-reliability-absent-{}",
            crate::forecast::uuidv7()
        ));
        let (cards, skipped) = load_grades(&dir).unwrap();
        assert!(cards.is_empty());
        assert_eq!(skipped, 0);
    }

    #[test]
    fn report_serializes_with_schema_header() {
        let rep = report(
            &[parse(&card(Some("jules"), "feat: a", "high", 1))],
            0,
            "2026-07-04T18:00:00Z",
        );
        let v: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&rep).unwrap()).unwrap();
        assert_eq!(v["schema"], RELIABILITY_REPORT_SCHEMA);
        assert_eq!(v["by_agent"]["jules"]["overall"]["n"], 1);
        // n == 0 buckets never serialize a NaN — mean is simply absent.
        assert!(v["pooled"]["mean"].is_number());
    }
}
