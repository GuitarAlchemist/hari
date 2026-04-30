//! Subjective Logic baseline integration tests.
//!
//! Exercises the SL pipeline against the same six fixtures the
//! Phase 5 rollup uses so the 3-way comparison can run in CI without
//! regressing.
//!
//! Scope intentionally narrow: opinion-arithmetic invariants live in
//! the unit tests inside `subjective_logic.rs`. This file checks
//! trace-shape invariants (non-trivial action streams, populated
//! metrics) and the 3-way comparison driver.

use hari_core::{
    action_kind, compare_replay_three_way, process_research_trace_subjective_logic, Action,
    Opinion, ResearchTrace, SubjectiveLogicConfig,
};
use hari_lattice::HexValue;
use std::fs;

const FIXTURES: &[&str] = &[
    "../../fixtures/ix/cognition_divergence.json",
    "../../fixtures/ix/conflicting_benchmark.json",
    "../../fixtures/ix/heavy_contradiction.json",
    "../../fixtures/ix/long_recovery.json",
    "../../fixtures/ix/racing_goals.json",
    "../../fixtures/ix/slow_evidence.json",
    "../../fixtures/ix/swarm_dissent.json",
];

fn load_trace(path: &str) -> ResearchTrace {
    let raw = fs::read_to_string(path).expect("fixture must be readable");
    match serde_json::from_str::<ResearchTrace>(&raw) {
        Ok(t) => t,
        Err(_) => {
            let events: Vec<hari_core::ResearchEvent> =
                serde_json::from_str(&raw).expect("fixture must be a trace or event array");
            events.into()
        }
    }
}

#[test]
fn opinion_vacuous_invariant_holds() {
    let v = Opinion::vacuous(0.5);
    let s = v.belief + v.disbelief + v.uncertainty;
    assert!((s - 1.0).abs() < 1e-12);
}

#[test]
fn opinion_from_hex_true_projects_above_half() {
    let p = Opinion::from_hex(HexValue::True, 0.5).projected_probability();
    assert!(p > 0.5, "True opinion should project above 0.5, got {p}");
}

#[test]
fn cumulative_fuse_is_commutative_for_two_non_degenerate() {
    let a = Opinion::from_hex(HexValue::Probable, 0.5);
    let b = Opinion::from_hex(HexValue::Doubtful, 0.5);
    let ab = Opinion::cumulative_fuse(a, b);
    let ba = Opinion::cumulative_fuse(b, a);
    assert!((ab.belief - ba.belief).abs() < 1e-9);
    assert!((ab.disbelief - ba.disbelief).abs() < 1e-9);
    assert!((ab.uncertainty - ba.uncertainty).abs() < 1e-9);
}

/// Every fixture must produce a non-trivial action stream when run
/// through SL — i.e., not all `Wait` and not all `Escalate`. The shape
/// should be diverse enough that the metric pipeline has signal to
/// chew on.
#[test]
fn every_fixture_produces_non_trivial_sl_action_stream() {
    let cfg = SubjectiveLogicConfig::default();
    for path in FIXTURES {
        let trace = load_trace(path);
        let report = process_research_trace_subjective_logic(trace, cfg);
        let mut kinds: std::collections::BTreeSet<&'static str> = Default::default();
        let mut total: usize = 0;
        for o in &report.outcomes {
            for a in &o.actions {
                kinds.insert(action_kind(a));
                total += 1;
            }
        }
        assert!(total > 0, "{path}: SL produced zero actions");

        // "All Wait" or "all Escalate" without anything else would be a
        // degenerate stream. Allow Log to coexist; require at least
        // one non-Log non-Wait action OR mixed Wait/Escalate.
        let non_log_kinds: Vec<&&str> = kinds.iter().filter(|k| **k != "Log").collect();
        assert!(
            !non_log_kinds.is_empty(),
            "{path}: SL produced only Log actions, no recommendations"
        );
        let only_wait = non_log_kinds.len() == 1 && **non_log_kinds[0] == *"Wait";
        let only_escalate = non_log_kinds.len() == 1 && **non_log_kinds[0] == *"Escalate";
        assert!(
            !only_wait,
            "{path}: SL stream is degenerate (only Wait + Log)"
        );
        assert!(
            !only_escalate,
            "{path}: SL stream is degenerate (only Escalate + Log)"
        );
    }
}

/// 3-way comparison on `long_recovery.json` — the headline fixture
/// from §7b of `phase5-results.md`. Reports SL's
/// `false_acceptance_count`, `goal_completion_rate`, and
/// `contradiction_recovery_cycles` and asserts they are computed.
#[test]
fn three_way_comparison_on_long_recovery_populates_metrics() {
    let trace = load_trace("../../fixtures/ix/long_recovery.json");
    let report = compare_replay_three_way(trace, SubjectiveLogicConfig::default());

    // All three reports should have processed every event.
    assert_eq!(report.recency_decay.event_count, 22);
    assert_eq!(report.lie.event_count, 22);
    assert_eq!(report.subjective_logic.event_count, 22);

    // Metrics must populate (i.e. not be the all-default sentinel).
    let sl = &report.comparison.subjective_logic;
    let lie = &report.comparison.lie;
    let decay = &report.comparison.recency_decay;

    // false_acceptance_count is a u32 — always populated; check the
    // metric machinery actually saw the trace by asserting the action
    // counts map is non-empty.
    assert!(
        !sl.action_counts_by_kind.is_empty(),
        "SL action_counts_by_kind unexpectedly empty"
    );
    assert!(
        !lie.action_counts_by_kind.is_empty(),
        "Lie action_counts_by_kind unexpectedly empty"
    );
    assert!(
        !decay.action_counts_by_kind.is_empty(),
        "RecencyDecay action_counts_by_kind unexpectedly empty"
    );

    // goal_completion_rate is a fraction in [0, 1]. SL receives the
    // same goal_update events the other paths do.
    assert!(
        (0.0..=1.0).contains(&sl.goal_completion_rate),
        "SL goal_completion_rate out of range: {}",
        sl.goal_completion_rate
    );

    // Echo to stderr so test output captures the 3-way numbers — useful
    // when iterating on the rollup doc.
    eprintln!(
        "[long_recovery 3-way] decay: false_accept={} recovery={:?} goal_comp={:.3}",
        decay.false_acceptance_count, decay.contradiction_recovery_cycles, decay.goal_completion_rate
    );
    eprintln!(
        "[long_recovery 3-way] lie:   false_accept={} recovery={:?} goal_comp={:.3}",
        lie.false_acceptance_count, lie.contradiction_recovery_cycles, lie.goal_completion_rate
    );
    eprintln!(
        "[long_recovery 3-way] sl:    false_accept={} recovery={:?} goal_comp={:.3}",
        sl.false_acceptance_count, sl.contradiction_recovery_cycles, sl.goal_completion_rate
    );
}

/// Sanity check: SL must produce at least one Accept across the six
/// "real" fixtures (i.e., not just the 3-event conflicting_benchmark).
/// If it never accepts anything, the recommendation thresholds are
/// almost certainly wrong.
#[test]
fn sl_accepts_at_least_one_proposition_across_fixtures() {
    let cfg = SubjectiveLogicConfig::default();
    let mut total_accepts = 0u32;
    for path in FIXTURES {
        let trace = load_trace(path);
        let report = process_research_trace_subjective_logic(trace, cfg);
        for o in &report.outcomes {
            for a in &o.actions {
                if matches!(a, Action::Accept { .. }) {
                    total_accepts += 1;
                }
            }
        }
    }
    assert!(
        total_accepts > 0,
        "SL never accepted anything across {} fixtures — threshold mis-tuned",
        FIXTURES.len()
    );
}

/// Three-way divergences must exist on at least one of the six
/// scenario fixtures, i.e. SL must actually produce different action
/// streams from Lie/RecencyDecay somewhere. If the three pipelines
/// agree everywhere, SL isn't a real comparator.
#[test]
fn three_way_divergence_exists_on_at_least_one_fixture() {
    let cfg = SubjectiveLogicConfig::default();
    let mut total_div = 0usize;
    for path in FIXTURES {
        let trace = load_trace(path);
        let report = compare_replay_three_way(trace, cfg);
        total_div += report.comparison.divergence_pairs.len();
    }
    assert!(
        total_div > 0,
        "SL never disagreed with Lie or RecencyDecay across {} fixtures — comparator is degenerate",
        FIXTURES.len()
    );
}
