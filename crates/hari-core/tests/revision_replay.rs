//! Belief-revision replay integration tests (issue #16, retraction tracer
//! slice).
//!
//! Each of the three `fixtures/revision/*.json` fixtures is replayed end to
//! end through `process_research_trace` and asserted against the expected
//! semantics in `fixtures/revision/README.md`. The doctrine under all three
//! is **evidence-recompute is authoritative**: a selective retraction
//! tombstones the named evidence and recomputes the belief from the
//! survivors; a supersession retires a claim for a successor while keeping
//! the retired claim inspectable.
//!
//! The A/B baseline (design §7) is the pre-slice whole-proposition
//! last-write-wins-to-`Unknown` handler. The fidelity test shows the
//! selective path equals a from-scratch survivor recompute and *differs*
//! from that baseline on the fixtures — the row the baseline gets wrong.

use hari_core::{
    CognitiveLoop, PriorityModel, ResearchEvent, ResearchEventPayload, ResearchTrace, RevisionCause,
};
use hari_lattice::{HexLattice, HexValue};
use std::fs;

fn load_trace(path: &str) -> ResearchTrace {
    let raw = fs::read_to_string(path).expect("fixture must be readable");
    serde_json::from_str::<ResearchTrace>(&raw).expect("fixture must deserialize as a trace")
}

const F_DISSOLVE: &str = "../../fixtures/revision/retraction_dissolves_derived_contradiction.json";
const F_PARTIAL: &str = "../../fixtures/revision/partial_retraction_downgrades.json";
const F_SUPERSEDE: &str = "../../fixtures/revision/supersession_chain.json";

/// Fixture 1: two sources disagree (`True` + `False` → derived
/// `Contradictory`), then one retracts its `False`. Current belief
/// recomputes over `{True}` → `True`; the derived contradiction dissolves.
#[test]
fn retraction_dissolves_derived_contradiction() {
    let trace = load_trace(F_DISSOLVE);
    let mut cl = CognitiveLoop::new(trace.dimension);
    let report = cl.process_research_trace(trace);

    assert_eq!(
        report.final_beliefs.get("deploy-is-safe"),
        Some(&HexValue::True),
        "after retracting the critic's False, the belief recomputes to True and the C dissolves"
    );

    // The report carries the previous→current delta with cause=retraction.
    let delta = report
        .revisions
        .iter()
        .find(|d| d.proposition == "deploy-is-safe")
        .expect("a retraction revision delta must be reported");
    assert_eq!(delta.previous, HexValue::Contradictory);
    assert_eq!(delta.current, HexValue::True);
    assert_eq!(delta.cause, RevisionCause::Retraction);
    assert_eq!(delta.cycle, 3);
}

/// Fixture 1, historical replay: replaying only through the conflict
/// (cycles 1–2, before the retraction) still shows `Contradictory`. The
/// conflict that once stood is fully recoverable — retraction never
/// rewrites the past (current-vs-historical is a replay-endpoint choice).
#[test]
fn historical_replay_to_conflict_still_shows_contradiction() {
    let full = load_trace(F_DISSOLVE);
    let truncated = ResearchTrace {
        dimension: full.dimension,
        events: full.events.into_iter().take(2).collect(),
    };
    let mut cl = CognitiveLoop::new(truncated.dimension);
    let report = cl.process_research_trace(truncated);
    assert_eq!(
        report.final_beliefs.get("deploy-is-safe"),
        Some(&HexValue::Contradictory),
        "replay-to-conflict reproduces the historical Contradictory"
    );
    assert!(
        report.revisions.is_empty(),
        "no revision events before the retraction"
    );
}

/// Fixture 2: a belief corroborated by two `True` sources loses one support
/// to a selective retraction. It **survives on the remaining source** rather
/// than being reset to `Unknown` — the row the LWW baseline gets wrong.
///
/// Note: the substrate has no per-source weight at this boundary, so the
/// survivor recompute yields `True` (not the finer-grained `Probable` a
/// corroboration cap would give — that is deferred to the merge-weight
/// slice, design §9 item 2). The load-bearing A/B property — survives vs
/// erased — holds either way and is asserted in
/// [`retraction_fidelity_beats_lww_baseline`].
#[test]
fn partial_retraction_survives_on_remaining_evidence() {
    let trace = load_trace(F_PARTIAL);
    let mut cl = CognitiveLoop::new(trace.dimension);
    let report = cl.process_research_trace(trace);

    let value = report.final_beliefs.get("model-v3-beats-baseline").copied();
    assert_eq!(
        value,
        Some(HexValue::True),
        "belief survives on the remaining source, not reset to Unknown"
    );
    assert_ne!(
        value,
        Some(HexValue::Unknown),
        "must NOT collapse to the LWW baseline's Unknown"
    );
    assert!(
        report
            .revisions
            .iter()
            .any(|d| d.proposition == "model-v3-beats-baseline"
                && d.cause == RevisionCause::Retraction),
        "the partial retraction is reported as a revision"
    );
}

/// Fixture 3: a claim evolves `14 → 20 → 27` via supersession. Only the head
/// is live; the superseded claims are retired but inspectable, and the chain
/// is recorded as the audit trail.
#[test]
fn supersession_chain_retires_claims_but_keeps_them_inspectable() {
    let trace = load_trace(F_SUPERSEDE);
    let mut cl = CognitiveLoop::new(trace.dimension);
    let report = cl.process_research_trace(trace);

    // All three claims remain inspectable at their asserted value.
    for claim in [
        "persona-count-is-14",
        "persona-count-is-20",
        "persona-count-is-27",
    ] {
        assert_eq!(
            report.final_beliefs.get(claim),
            Some(&HexValue::True),
            "{claim} stays inspectable at its last value"
        );
    }

    // The retired claims know their successor; the head does not.
    assert_eq!(
        cl.superseded_by("persona-count-is-14"),
        Some("persona-count-is-20")
    );
    assert_eq!(
        cl.superseded_by("persona-count-is-20"),
        Some("persona-count-is-27")
    );
    assert_eq!(
        cl.superseded_by("persona-count-is-27"),
        None,
        "the live chain head is not superseded"
    );

    // The chain is reported as two supersession deltas in order.
    let chain: Vec<(&str, &str)> = report
        .revisions
        .iter()
        .filter(|d| d.cause == RevisionCause::Supersession)
        .map(|d| {
            (
                d.proposition.as_str(),
                d.superseded_by
                    .as_deref()
                    .expect("supersession names a successor"),
            )
        })
        .collect();
    assert_eq!(
        chain,
        vec![
            ("persona-count-is-14", "persona-count-is-20"),
            ("persona-count-is-20", "persona-count-is-27"),
        ]
    );
}

/// Strip the `retracts` selector from every `Retraction` in a trace,
/// turning the selective retractions into the naive whole-proposition
/// retractions the pre-slice handler performs. This reconstructs the A/B
/// **baseline arm** from the *same* input, so the comparison exercises the
/// real naive handler rather than asserting a hardcoded value.
fn without_selectors(trace: &ResearchTrace) -> ResearchTrace {
    let events = trace
        .events
        .iter()
        .map(|e| {
            let payload = match &e.payload {
                ResearchEventPayload::Retraction {
                    proposition,
                    reason,
                    ..
                } => ResearchEventPayload::Retraction {
                    proposition: proposition.clone(),
                    reason: reason.clone(),
                    retracts: None,
                },
                other => other.clone(),
            };
            ResearchEvent {
                cycle: e.cycle,
                source: e.source.clone(),
                payload,
            }
        })
        .collect();
    ResearchTrace {
        dimension: trace.dimension,
        events,
    }
}

/// A/B baseline (design §7): the selective-retraction path equals a
/// from-scratch recompute over the surviving evidence (retraction fidelity),
/// while the naive whole-proposition LWW handler — run on the *same* trace
/// with its selector stripped — erases the belief to `Unknown`. Proven on
/// both retraction fixtures by running the actual baseline handler, not a
/// hardcoded constant.
#[test]
fn retraction_fidelity_beats_lww_baseline() {
    // (fixture, proposition, surviving evidence values after the retraction)
    let cases: &[(&str, &str, &[HexValue])] = &[
        // Fixture 1: critic's False retracted → {evaluator: True}.
        (F_DISSOLVE, "deploy-is-safe", &[HexValue::True]),
        // Fixture 2: evaluator's True retracted → {runner: True}.
        (F_PARTIAL, "model-v3-beats-baseline", &[HexValue::True]),
    ];

    for (path, prop, survivors) in cases {
        let trace = load_trace(path);

        // Experimental arm: the selective evidence-recompute path.
        let mut experimental = CognitiveLoop::new(trace.dimension);
        let report = experimental.process_research_trace(trace.clone());
        let actual = report.final_beliefs.get(*prop).copied().unwrap();

        // Retraction fidelity: the implemented value IS the survivor recompute.
        let from_scratch = HexLattice::combine_evidence_set(survivors.iter().copied());
        assert_eq!(
            actual, from_scratch,
            "{path}: post-retraction belief must equal a from-scratch survivor recompute"
        );

        // Baseline arm: the SAME trace with the selector stripped, run
        // through the preserved naive whole-proposition handler.
        let baseline_trace = without_selectors(&trace);
        let mut baseline_loop = CognitiveLoop::new(baseline_trace.dimension);
        let baseline_report = baseline_loop.process_research_trace(baseline_trace);
        let baseline = baseline_report.final_beliefs.get(*prop).copied().unwrap();

        assert_eq!(
            baseline,
            HexValue::Unknown,
            "{path}: the naive baseline erases the belief to Unknown"
        );
        assert_ne!(
            actual, baseline,
            "{path}: evidence-recompute must beat (differ from) the naive LWW baseline"
        );
    }
}

/// Regression: an existing trace *with* a whole-proposition retraction (no
/// selector) replays bit-identically — the additive `revisions`/`retracts`
/// fields stay absent and the belief still resets to `Unknown` via the
/// preserved legacy path. (Byte-identity against the pre-slice binary is
/// verified out-of-band; this guards the additivity in-tree.)
#[test]
fn existing_conflicting_benchmark_report_unchanged() {
    let trace = load_trace("../../fixtures/ix/conflicting_benchmark.json");
    let mut cl = CognitiveLoop::with_model(trace.dimension, PriorityModel::RecencyDecay);
    let report = cl.process_research_trace(trace);

    assert_eq!(
        report.final_beliefs.get("benchmark-x-is-reliable"),
        Some(&HexValue::Unknown),
        "whole-proposition retraction still resets to Unknown"
    );
    assert!(
        report.revisions.is_empty(),
        "no revision deltas for the legacy no-selector retraction"
    );

    let json = serde_json::to_string(&report).unwrap();
    assert!(
        !json.contains("\"revisions\""),
        "empty revisions must be skipped from JSON (byte-additive)"
    );
    assert!(
        !json.contains("retracts"),
        "absent selector must be skipped from JSON (byte-additive)"
    );
}

/// The retraction/supersession variants parse through the same serde enum in
/// both the object-form (`ResearchTrace`) and array-form (`Vec<ResearchEvent>`)
/// trace paths CLAUDE.md requires to stay in lockstep.
#[test]
fn new_payloads_parse_in_both_trace_forms() {
    let object_form = r#"{
        "dimension": 4,
        "events": [
            { "cycle": 1, "source": "s", "payload": {
                "type": "retraction", "proposition": "p", "reason": "r",
                "retracts": { "source": "s", "cycle": 1 } } },
            { "cycle": 2, "source": "s", "payload": {
                "type": "supersession", "proposition": "p", "superseded_by": "q", "reason": "r" } }
        ]
    }"#;
    let trace: ResearchTrace = serde_json::from_str(object_form).expect("object form parses");
    assert_eq!(trace.events.len(), 2);

    let array_form = r#"[
        { "cycle": 1, "source": "s", "payload": {
            "type": "retraction", "proposition": "p", "reason": "r",
            "retracts": { "cycle": 1 } } },
        { "cycle": 2, "source": "s", "payload": {
            "type": "supersession", "proposition": "p", "superseded_by": "q", "reason": "r" } }
    ]"#;
    let events: Vec<ResearchEvent> = serde_json::from_str(array_form).expect("array form parses");
    let trace2: ResearchTrace = events.into();
    assert_eq!(trace2.events.len(), 2);
}
