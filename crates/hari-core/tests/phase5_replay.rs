//! Phase 5 replay integration tests.
//!
//! These verify the load-bearing contract of Phase 5: that running the same
//! IX-style trace through `Lie` and `RecencyDecay` priority models on fresh
//! `CognitiveLoop` instances produces (a) divergent action lists for at
//! least one event in the `cognition_divergence.json` fixture, (b) bounded
//! attention norms in both paths, and (c) at least one divergence is
//! between `Investigate` and `Wait` — the decision the roadmap calls out
//! as the observable A/B point for the cognition integration.

use hari_core::{action_kind, compare_replay, Action, CognitiveLoop, PriorityModel, ResearchTrace};
use std::fs;

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
fn conflicting_benchmark_replays_in_both_models() {
    let trace = load_trace("../../fixtures/ix/conflicting_benchmark.json");

    let mut lie_loop = CognitiveLoop::with_model(trace.dimension, PriorityModel::Lie);
    let lie_report = lie_loop.process_research_trace(trace.clone());
    assert_eq!(lie_report.event_count, 3);

    let mut decay_loop = CognitiveLoop::with_model(trace.dimension, PriorityModel::RecencyDecay);
    let decay_report = decay_loop.process_research_trace(trace);
    assert_eq!(decay_report.event_count, 3);

    // Boundedness contract.
    assert!(lie_report.metrics.attention_norm_max < 10.0);
    assert!(decay_report.metrics.attention_norm_max < 10.0);
}

#[test]
fn cognition_divergence_replays_with_nonempty_divergence() {
    let trace = load_trace("../../fixtures/ix/cognition_divergence.json");
    let report = compare_replay(trace);

    let comparison = report
        .comparison
        .expect("compare_replay must populate the comparison field");
    assert!(
        !comparison.action_divergence.is_empty(),
        "cognition_divergence.json must produce at least one ActionDivergence"
    );
    // Boundedness on both paths — matches the check-phase5-done.sh contract.
    assert!(
        comparison.experimental.attention_norm_max < 10.0,
        "experimental attention_norm_max must stay under 10.0"
    );
    assert!(
        comparison.baseline.attention_norm_max < 10.0,
        "baseline attention_norm_max must stay under 10.0"
    );
}

#[test]
fn cognition_divergence_contains_investigate_vs_wait() {
    let trace = load_trace("../../fixtures/ix/cognition_divergence.json");
    let report = compare_replay(trace);
    let divergences = report
        .comparison
        .expect("comparison populated")
        .action_divergence;

    let has_investigate_vs_wait = divergences.iter().any(|d| {
        let baseline_kinds: Vec<&'static str> =
            d.baseline_actions.iter().map(action_kind).collect();
        let exp_kinds: Vec<&'static str> = d.experimental_actions.iter().map(action_kind).collect();
        let exp_is_wait_only = exp_kinds.len() == 1 && exp_kinds[0] == "Wait";
        let baseline_has_investigate = baseline_kinds.contains(&"Investigate");
        let baseline_is_wait_only = baseline_kinds.len() == 1 && baseline_kinds[0] == "Wait";
        let exp_has_investigate = exp_kinds.contains(&"Investigate");
        (exp_is_wait_only && baseline_has_investigate)
            || (baseline_is_wait_only && exp_has_investigate)
    });

    assert!(
        has_investigate_vs_wait,
        "expected at least one Investigate-vs-Wait divergence; got: {:?}",
        divergences
            .iter()
            .map(|d| (
                d.baseline_actions
                    .iter()
                    .map(action_kind)
                    .collect::<Vec<_>>(),
                d.experimental_actions
                    .iter()
                    .map(action_kind)
                    .collect::<Vec<_>>()
            ))
            .collect::<Vec<_>>()
    );
}

#[test]
fn lie_alpha_zero_collapses_lie_to_flat_on_substantive_actions() {
    // Sanity: when α = 0 the Lie scoring rule `(1 + α * proj)` collapses to
    // 1.0 for every substantive action (Investigate, Accept, Retry, Escalate,
    // UpdateBelief, SendMessage). The Lie path *also* down-prioritises the
    // side-channel actions Log and Wait to 0.05 so they don't crowd out
    // recommendations — that suppression is independent of α and is a
    // deliberate part of the model contract, NOT something α=0 should undo.
    //
    // So the right contract for "Lie@α=0 ≡ Flat" is: after filtering Log and
    // Wait from both, the remaining substantive action lists must match.
    let trace = load_trace("../../fixtures/ix/cognition_divergence.json");

    let mut lie_zero = CognitiveLoop::with_model(trace.dimension, PriorityModel::Lie);
    lie_zero.lie_alpha = 0.0;
    let lie_report = lie_zero.process_research_trace(trace.clone());

    let mut flat_loop = CognitiveLoop::with_model(trace.dimension, PriorityModel::Flat);
    let flat_report = flat_loop.process_research_trace(trace);

    assert_eq!(lie_report.outcomes.len(), flat_report.outcomes.len());
    let strip = |actions: &[Action]| -> Vec<&'static str> {
        actions
            .iter()
            .map(action_kind)
            .filter(|k| *k != "Log" && *k != "Wait")
            .collect()
    };
    for (i, (lie_out, flat_out)) in lie_report
        .outcomes
        .iter()
        .zip(flat_report.outcomes.iter())
        .enumerate()
    {
        let lie_kinds = strip(&lie_out.actions);
        let flat_kinds = strip(&flat_out.actions);
        assert_eq!(
            lie_kinds, flat_kinds,
            "Lie@α=0 substantive actions must match Flat at event {i}: lie={lie_kinds:?} flat={flat_kinds:?}"
        );
    }
}

#[test]
fn projection_axis_pinned_when_top_goal_changes_mid_replay() {
    // Bug 1 regression: after the algebra is seeded with goal A as the top
    // goal, adding a higher-priority goal B mid-replay must NOT shift
    // `seeded_projection_axis`. The stored axis is the one the projection
    // generator was built around; the per-cycle Hamiltonian uses it via
    // `seeded_projection_axis`, so they always agree.
    let mut loop_ = CognitiveLoop::with_model(4, PriorityModel::Lie);
    loop_.state.add_goal("alpha", "first goal", 0.5);

    // Before any cycle, no axis has been pinned.
    assert!(loop_.seeded_projection_axis().is_none());

    // Drive one cycle to seed the algebra.
    loop_.cycle();
    let axis_after_seed = loop_
        .seeded_projection_axis()
        .expect("seeding must record an axis after one cycle");

    // Add a higher-priority goal that would change `top_goal`.
    loop_
        .state
        .add_goal("beta", "second goal, higher priority", 0.9);
    assert_eq!(loop_.state.top_goal().unwrap().0, "beta");

    // Drive another cycle. The pinned axis must not have moved.
    loop_.cycle();
    assert_eq!(
        loop_.seeded_projection_axis(),
        Some(axis_after_seed),
        "seeded_projection_axis must stay pinned after top_goal shifts"
    );
}

#[test]
fn contradictory_perception_moves_attention_in_lie_mode() {
    // The Hamiltonian smear is supposed to keep contradictory evidence
    // visible to the rotation generators (Bug 2 territory: previously the
    // scaling generator's mean ignored the smear, but the rotations and
    // projection should still react). With a single Contradictory
    // perception, attention must change between consecutive cycles in the
    // Lie path. If this fails, the smear or the wiring is dead.
    use hari_core::Perception;
    use hari_lattice::HexValue;

    let mut loop_ = CognitiveLoop::with_model(4, PriorityModel::Lie);
    loop_
        .state
        .add_goal("contested", "claim with conflicting evidence", 0.9);

    let initial_attention = loop_.state.attention.clone();
    loop_.perceive(Perception {
        proposition: "contested".to_string(),
        value: HexValue::Contradictory,
        source: "ix-conflict-detector".to_string(),
        cycle: 1,
    });
    loop_.cycle();
    let after = loop_.state.attention.clone();

    let delta = (&after - &initial_attention).norm();
    assert!(
        delta > 1e-6,
        "Contradictory perception must move attention in Lie mode (delta={delta})"
    );
    // Sanity: the move stays bounded (boundedness contract still holds even
    // with the smear active).
    assert!(
        after.norm() < 10.0,
        "attention norm exploded: {}",
        after.norm()
    );
}

#[test]
fn divergence_test_pins_alpha_and_dt() {
    // The cognition_divergence fixture's pass/fail behaviour is sensitive to
    // `lie_alpha` and `lie_dt`. Pin them explicitly here so a future change
    // to defaults can't silently invert the test's verdict. If the defaults
    // change, this test will start failing and force an audit.
    let loop_ = CognitiveLoop::new(4);
    assert!(
        (loop_.lie_alpha - 2.0).abs() < 1e-12,
        "lie_alpha default changed (was 2.0); audit divergence fixture before changing"
    );
    assert!(
        (loop_.lie_dt - 0.5).abs() < 1e-12,
        "lie_dt default changed (was 0.5); audit divergence fixture before changing"
    );
}

#[test]
fn long_recovery_fixture_populates_contradiction_recovery_metric() {
    // The cognition_divergence fixture is too short to exercise the
    // contradiction_recovery_cycles metric (no Contradictory→recovery
    // sequence). long_recovery.json is the 22-event fixture that includes
    // explicit Retractions following True+False evidence pairs. Both Lie
    // and RecencyDecay paths must populate the metric on this fixture; if
    // either is None, the metric machinery has a regression.
    let trace = load_trace("../../fixtures/ix/long_recovery.json");
    let report = compare_replay(trace);
    let comparison = report
        .comparison
        .expect("compare_replay must populate comparison");
    assert!(
        comparison.baseline.contradiction_recovery_cycles.is_some(),
        "RecencyDecay must populate contradiction_recovery_cycles on long_recovery.json"
    );
    assert!(
        comparison
            .experimental
            .contradiction_recovery_cycles
            .is_some(),
        "Lie must populate contradiction_recovery_cycles on long_recovery.json"
    );
    // Boundedness still holds on the longer fixture.
    assert!(
        comparison.experimental.attention_norm_max < 10.0,
        "experimental attention_norm_max must stay under 10.0 on long fixture (got {})",
        comparison.experimental.attention_norm_max
    );
}

#[test]
fn all_fixtures_satisfy_check_contract() {
    // Roll-up guard: for every IX trace fixture in `fixtures/ix/*.json`,
    // running `compare_replay` must satisfy the same contract the
    // `check-phase5-done.sh` script enforces — non-empty action_divergence
    // and both attention norms strictly under 10.0.
    //
    // The pre-existing `conflicting_benchmark.json` fixture is too short
    // (3 events, single proposition, no goal_updates) to drive the
    // algebra past the suppression threshold and is exempted from the
    // divergence requirement; the boundedness contract still applies.
    // It is the only fixture exempted; any new fixture authored under
    // Phase 5+ MUST diverge under the seeded defaults.
    let fixtures_dir = std::path::Path::new("../../fixtures/ix");
    let entries = std::fs::read_dir(fixtures_dir).expect("fixtures/ix directory must exist");

    let mut checked = 0usize;
    for entry in entries {
        let entry = entry.expect("readable directory entry");
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let path_str = path.to_string_lossy().to_string();
        let trace = load_trace(&path_str);
        let report = compare_replay(trace);
        let comparison = report
            .comparison
            .unwrap_or_else(|| panic!("compare_replay must populate comparison for {path_str}"));

        // Boundedness — every fixture, including conflicting_benchmark.
        assert!(
            comparison.baseline.attention_norm_max < 10.0,
            "{path_str}: baseline attention_norm_max must stay under 10.0 (got {})",
            comparison.baseline.attention_norm_max
        );
        assert!(
            comparison.experimental.attention_norm_max < 10.0,
            "{path_str}: experimental attention_norm_max must stay under 10.0 (got {})",
            comparison.experimental.attention_norm_max
        );

        // Divergence — required for every Phase-5 fixture (whose whole
        // point is to exercise priority-model differences). Two
        // exclusions:
        //  - the legacy conflicting_benchmark (pre-Phase-5 trivial case)
        //  - Phase-8 reasoning fixtures (whose point is belief
        //    propagation, not action divergence — they test the
        //    `RelationDeclaration` mechanism and therefore won't
        //    necessarily diverge between baseline and experimental
        //    priority models)
        let exempt_from_divergence_contract = path
            .file_name()
            .and_then(|s| s.to_str())
            .map(|s| matches!(s, "conflicting_benchmark.json" | "derivation.json"))
            .unwrap_or(false);
        if !exempt_from_divergence_contract {
            assert!(
                !comparison.action_divergence.is_empty(),
                "{path_str}: action_divergence must be non-empty for Phase-5+ fixtures"
            );
        }

        checked += 1;
    }
    assert!(
        checked >= 5,
        "expected to check at least 5 fixtures (long_recovery + 4 rollup + conflicting_benchmark + cognition_divergence); checked {checked}"
    );
}

#[test]
fn lie_and_recency_agree_on_retraction_event() {
    // The plan specifies that on a Retraction event both priority models
    // should produce a Retry recommendation — that's not where we expect
    // disagreement. This guards against the Lie path accidentally killing
    // the retract→retry signal.
    let trace = load_trace("../../fixtures/ix/conflicting_benchmark.json");
    let mut lie_loop = CognitiveLoop::with_model(trace.dimension, PriorityModel::Lie);
    let mut decay_loop = CognitiveLoop::with_model(trace.dimension, PriorityModel::RecencyDecay);
    let lie_report = lie_loop.process_research_trace(trace.clone());
    let decay_report = decay_loop.process_research_trace(trace);

    // The retraction event is the third event in the conflicting_benchmark
    // fixture. Both paths must include a Retry recommendation.
    let retraction_index = 2;
    let lie_actions = &lie_report.outcomes[retraction_index].actions;
    let decay_actions = &decay_report.outcomes[retraction_index].actions;

    let lie_has_retry = lie_actions
        .iter()
        .any(|a| matches!(a, Action::Retry { .. }));
    let decay_has_retry = decay_actions
        .iter()
        .any(|a| matches!(a, Action::Retry { .. }));

    assert!(
        lie_has_retry,
        "Lie path should preserve Retry on retraction"
    );
    assert!(
        decay_has_retry,
        "RecencyDecay path should preserve Retry on retraction"
    );
}
