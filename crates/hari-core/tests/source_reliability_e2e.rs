//! Cross-session source-reliability tracer bullet — end-to-end (issue #14).
//!
//! Drives the full slice on a real IX fixture: replay a `ResearchTrace`
//! through the cognitive loop, derive per-source outcome rows from the
//! resulting outcomes + final beliefs, and aggregate them into the
//! cross-session report. Ledger filesystem round-trip is unit-tested inside
//! `source_reliability.rs`; this file pins the *behavioural story* that a
//! source is graded by the fate of the claims it made.

use hari_core::{source_reliability, CognitiveLoop, ResearchTrace};
use std::fs;

fn replay(
    path: &str,
) -> (
    Vec<hari_core::ResearchEventOutcome>,
    std::collections::BTreeMap<String, hari_lattice::HexValue>,
) {
    let raw = fs::read_to_string(path).expect("fixture must be readable");
    let trace: ResearchTrace = serde_json::from_str(&raw).expect("fixture must parse");
    let mut loop_ = CognitiveLoop::new(trace.dimension);
    let report = loop_.process_research_trace(trace);
    (report.outcomes, report.final_beliefs)
}

/// The conflicting_benchmark fixture is
/// `belief_update(evaluator, reliable=Probable) → belief_update(critic,
/// reliable=Doubtful) → retraction(runner)`. The evaluator's accepted claim is
/// later retracted; the critic's contradiction is a warranted alarm. Under the
/// default `RecencyDecay` model the source grading must reflect exactly that.
#[test]
fn conflicting_benchmark_penalises_the_retracted_claimer_not_the_critic() {
    let (outcomes, final_beliefs) = replay("../../fixtures/ix/conflicting_benchmark.json");
    let rows = source_reliability::outcomes_from_report(
        "e2e",
        "2026-07-20T12:00:00Z",
        &outcomes,
        &final_beliefs,
    );
    let by: std::collections::BTreeMap<_, _> = rows.iter().map(|r| (r.source.clone(), r)).collect();

    // The evaluator's Probable claim was Accepted, then retracted → a false
    // acceptance attributed to the evaluator.
    let evaluator = by
        .get("ix-agent-evaluator")
        .expect("evaluator made a claim");
    assert_eq!(evaluator.accepted, 1);
    assert_eq!(evaluator.false_acceptances, 1);

    // The critic raised a contradiction that did NOT resolve to True/Probable,
    // so its escalation is warranted (not a false escalation).
    let critic = by.get("ix-agent-critic").expect("critic made a claim");
    assert_eq!(critic.escalations, 1);
    assert_eq!(critic.false_escalations, 0);

    // The runner only retracted — no claim, no row.
    assert!(!by.contains_key("ix-runner"));
}

/// The pooled entry is the A/B baseline: its tallies are exactly the sum of the
/// per-source tallies, on any real fixture. This is the invariant a consumer
/// relies on to compare a source against "the pool it came from".
#[test]
fn pooled_is_the_sum_of_sources_across_fixtures() {
    for fixture in [
        "../../fixtures/ix/conflicting_benchmark.json",
        "../../fixtures/ix/heavy_contradiction.json",
        "../../fixtures/ix/long_recovery.json",
        "../../fixtures/ix/swarm_dissent.json",
    ] {
        let (outcomes, final_beliefs) = replay(fixture);
        let rows = source_reliability::outcomes_from_report(
            "e2e",
            "2026-07-20T12:00:00Z",
            &outcomes,
            &final_beliefs,
        );
        let report = source_reliability::report(&rows, 0, "2026-07-20T18:00:00Z");

        let sum_accepted: u32 = report.by_source.values().map(|e| e.accepted).sum();
        let sum_false: u32 = report.by_source.values().map(|e| e.false_acceptances).sum();
        assert_eq!(
            report.pooled.accepted, sum_accepted,
            "pooled.accepted must equal the source sum for {fixture}"
        );
        assert_eq!(
            report.pooled.false_acceptances, sum_false,
            "pooled.false_acceptances must equal the source sum for {fixture}"
        );
        // Smoothed precision is always a finite score in [0, 1] — never NaN.
        assert!(report.pooled.smoothed_precision.is_finite());
        assert!((0.0..=1.0).contains(&report.pooled.smoothed_precision));
    }
}
