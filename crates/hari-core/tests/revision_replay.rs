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
use hari_lattice::HexValue;
use std::fs;

fn load_trace(path: &str) -> ResearchTrace {
    let raw = fs::read_to_string(path).expect("fixture must be readable");
    serde_json::from_str::<ResearchTrace>(&raw).expect("fixture must deserialize as a trace")
}

const F_DISSOLVE: &str = "../../fixtures/revision/retraction_dissolves_derived_contradiction.json";
const F_PARTIAL: &str = "../../fixtures/revision/partial_retraction_downgrades.json";
const F_SUPERSEDE: &str = "../../fixtures/revision/supersession_chain.json";
const F_CORRECTION: &str = "../../fixtures/revision/correction_replaces_claim.json";
const F_WITHDRAWAL: &str =
    "../../fixtures/revision/relation_withdrawal_reverts_derived_belief.json";

/// Fixture 1: two sources disagree (`True` + `False` → derived
/// `Contradictory`), then one retracts its `False`. The derived
/// contradiction **dissolves** and the belief recomputes over `{evaluator:
/// True}`. Under the merge-weight slice's single-source corroboration cap a
/// lone surviving source no longer yields full `True`, so the belief lands on
/// `Probable` — the load-bearing result is that the `C` is gone, not the
/// exact strength. See the README note and the design doc §8 addendum for why
/// this is `Probable` rather than the design's original `True` (fixtures 1 and
/// 2 have identical single-source survivor sets, so a uniform cap must treat
/// them alike).
#[test]
fn retraction_dissolves_derived_contradiction() {
    let trace = load_trace(F_DISSOLVE);
    let mut cl = CognitiveLoop::new(trace.dimension);
    let report = cl.process_research_trace(trace);

    assert_eq!(
        report.final_beliefs.get("deploy-is-safe"),
        Some(&HexValue::Probable),
        "after retracting the critic's False, the C dissolves and the belief recomputes \
         to Probable (single uncorroborated surviving source)"
    );

    // The report carries the previous→current delta with cause=retraction.
    let delta = report
        .revisions
        .iter()
        .find(|d| d.proposition == "deploy-is-safe")
        .expect("a retraction revision delta must be reported");
    assert_eq!(delta.previous, HexValue::Contradictory);
    assert_eq!(delta.current, HexValue::Probable);
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
/// than being reset to `Unknown` — the row the LWW baseline gets wrong — and
/// is **downgraded** `True → Probable` by the merge-weight slice's
/// single-source corroboration cap: one uncorroborated surviving source no
/// longer licenses full `True`. This is the design's originally-intended
/// fixture-2 semantic (§8.2), now delivered.
#[test]
fn partial_retraction_downgrades_to_probable() {
    let trace = load_trace(F_PARTIAL);
    let mut cl = CognitiveLoop::new(trace.dimension);
    let report = cl.process_research_trace(trace);

    let value = report.final_beliefs.get("model-v3-beats-baseline").copied();
    assert_eq!(
        value,
        Some(HexValue::Probable),
        "belief survives on the remaining source but downgrades True -> Probable"
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

/// `correction` fixture: a source asserts `False`, then corrects itself to
/// `True` in one atomic event. The correction tombstones the mislabeled
/// original and merges in the replacement, so the belief recomputes over
/// `{ix-runner: True}` — capped to `Probable` by the single-source rule, the
/// same uniform cap fixtures 1 & 2 land on. The report carries **one**
/// revision delta whose `cause = correction` — the causal link a bare
/// `belief_update` cannot express, distinguishing a correction from a plain
/// retraction.
#[test]
fn correction_replaces_claim_with_causal_link() {
    let trace = load_trace(F_CORRECTION);
    let mut cl = CognitiveLoop::new(trace.dimension);
    let report = cl.process_research_trace(trace);

    assert_eq!(
        report.final_beliefs.get("model-v3-latency-ok"),
        Some(&HexValue::Probable),
        "the corrected claim recomputes over {{ix-runner: True}}, single-source capped to Probable"
    );

    let deltas: Vec<_> = report
        .revisions
        .iter()
        .filter(|d| d.proposition == "model-v3-latency-ok")
        .collect();
    assert_eq!(
        deltas.len(),
        1,
        "a correction emits exactly one revision delta"
    );
    let delta = deltas[0];
    assert_eq!(
        delta.previous,
        HexValue::False,
        "the mislabeled original was False"
    );
    assert_eq!(delta.current, HexValue::Probable, "corrected to Probable");
    assert_eq!(
        delta.cause,
        RevisionCause::Correction,
        "cause distinguishes a correction from a plain retraction"
    );
    assert_eq!(delta.cycle, 2);
}

/// `correction` fixture, historical replay: replaying only through cycle 1
/// (before the correction) shows the original `False`. The correction never
/// rewrites the past — current-vs-historical is a replay-endpoint choice, and
/// the tombstoned original stays in the trace.
#[test]
fn historical_replay_before_correction_shows_original() {
    let full = load_trace(F_CORRECTION);
    let truncated = ResearchTrace {
        dimension: full.dimension,
        events: full.events.into_iter().take(1).collect(),
    };
    let mut cl = CognitiveLoop::new(truncated.dimension);
    let report = cl.process_research_trace(truncated);
    assert_eq!(
        report.final_beliefs.get("model-v3-latency-ok"),
        Some(&HexValue::False),
        "replay-to-cycle-1 reproduces the historical (pre-correction) False"
    );
    assert!(
        report.revisions.is_empty(),
        "no revision events before the correction"
    );
}

/// `relation_withdrawal` fixture: a direct belief supports a derived belief
/// through a declared relation; withdrawing that relation reverts the derived
/// belief on the next propagation, while the withdrawn edge stays inspectable
/// (tombstone, not delete) and the base belief is untouched.
#[test]
fn relation_withdrawal_reverts_derived_belief() {
    let trace = load_trace(F_WITHDRAWAL);
    let mut cl = CognitiveLoop::new(trace.dimension);
    let report = cl.process_research_trace(trace);

    // The base belief (direct evidence) is untouched by the withdrawal.
    assert_eq!(
        cl.state.beliefs.get("benchmark-x-passes").map(|p| p.value),
        Some(HexValue::True),
        "the base belief with direct evidence survives the withdrawal"
    );
    // The derived belief reverts to its Unknown base — nothing supports it now.
    // (`deploy-is-safe` is reached only through relation events, so it is not a
    // `touched_proposition` in `final_beliefs`; read it from the network.)
    assert_eq!(
        cl.state.beliefs.get("deploy-is-safe").map(|p| p.value),
        Some(HexValue::Unknown),
        "the derived belief reverts once its only supporting edge is withdrawn"
    );

    // The withdrawn edge is tombstoned, not deleted — still inspectable.
    assert!(
        cl.state.beliefs.is_relation_withdrawn(
            "benchmark-x-passes",
            "deploy-is-safe",
            hari_lattice::Relation::Supports,
        ),
        "the withdrawn edge stays inspectable (audit-preservation)"
    );
    assert_eq!(cl.state.beliefs.withdrawn_relation_count(), 1);

    // The revert is reported as a relation_withdrawal revision delta.
    let delta = report
        .revisions
        .iter()
        .find(|d| d.proposition == "deploy-is-safe")
        .expect("the reverted derived belief is reported");
    assert_eq!(delta.previous, HexValue::True);
    assert_eq!(delta.current, HexValue::Unknown);
    assert_eq!(delta.cause, RevisionCause::RelationWithdrawal);
    assert_eq!(delta.cycle, 3);
}

/// A/B baseline for relation withdrawal: the current append-only "undo" is a
/// `Retraction` of the derived proposition (reset-to-`Unknown`), which resets
/// *only* the named node and leaves the inducing edge live — so on the next
/// propagation the belief is re-derived right back. Withdrawal beats it: the
/// edge is gone from propagation, so the revert *sticks*.
#[test]
fn withdrawal_sticks_where_retracting_the_derived_belief_would_not() {
    use hari_core::{ResearchEvent, ResearchEventPayload};
    use hari_lattice::Relation;

    // Baseline: same setup, but instead of withdrawing the edge, retract the
    // derived belief and then re-assert the base + let it re-propagate.
    let mut cl = CognitiveLoop::new(4);
    cl.process_research_event(ResearchEvent {
        cycle: 1,
        source: "ix-runner".into(),
        payload: ResearchEventPayload::BeliefUpdate {
            proposition: "benchmark-x-passes".into(),
            value: HexValue::True,
            evidence: Default::default(),
        },
    });
    cl.process_research_event(ResearchEvent {
        cycle: 2,
        source: "ix-planner".into(),
        payload: ResearchEventPayload::RelationDeclaration {
            from: "benchmark-x-passes".into(),
            to: "deploy-is-safe".into(),
            relation: Relation::Supports,
        },
    });
    // Retract the derived belief (the append-only "undo"): resets the node…
    cl.process_research_event(ResearchEvent {
        cycle: 3,
        source: "ix-planner".into(),
        payload: ResearchEventPayload::Retraction {
            proposition: "deploy-is-safe".into(),
            reason: "no longer supported".into(),
            retracts: None,
        },
    });
    // …but the edge is still live, so any further propagation re-derives it.
    // Force a propagation pass by re-asserting the base belief.
    cl.process_research_event(ResearchEvent {
        cycle: 4,
        source: "ix-runner".into(),
        payload: ResearchEventPayload::BeliefUpdate {
            proposition: "benchmark-x-passes".into(),
            value: HexValue::True,
            evidence: Default::default(),
        },
    });
    assert_eq!(
        cl.state.beliefs.get("deploy-is-safe").map(|p| p.value),
        Some(HexValue::True),
        "retracting the derived node does NOT stick: the live edge re-derives it"
    );

    // Withdrawal arm: the fixture. The revert sticks because the edge is gone.
    let trace = load_trace(F_WITHDRAWAL);
    let mut w = CognitiveLoop::new(trace.dimension);
    w.process_research_trace(trace);
    assert_eq!(
        w.state.beliefs.get("deploy-is-safe").map(|p| p.value),
        Some(HexValue::Unknown),
        "withdrawing the edge makes the revert stick"
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
    // (fixture, proposition, surviving (source, value) evidence after the
    // retraction). The survivor recompute routes through the same weighted
    // merge + corroboration cap the boundary uses, so the source identity is
    // load-bearing (it is what the cap counts).
    type Case = (
        &'static str,
        &'static str,
        &'static [(&'static str, HexValue)],
    );
    let cases: &[Case] = &[
        // Fixture 1: critic's False retracted → {evaluator: True}.
        (
            F_DISSOLVE,
            "deploy-is-safe",
            &[("ix-agent-evaluator", HexValue::True)],
        ),
        // Fixture 2: evaluator's True retracted → {runner: True}.
        (
            F_PARTIAL,
            "model-v3-beats-baseline",
            &[("ix-runner", HexValue::True)],
        ),
    ];

    for (path, prop, survivors) in cases {
        let trace = load_trace(path);

        // Experimental arm: the selective evidence-recompute path.
        let mut experimental = CognitiveLoop::new(trace.dimension);
        let report = experimental.process_research_trace(trace.clone());
        let actual = report.final_beliefs.get(*prop).copied().unwrap();

        // Retraction fidelity: the implemented value IS the survivor recompute,
        // through the real engine (weighted merge + single-source cap).
        let from_scratch =
            CognitiveLoop::recompute_belief(survivors.iter().map(|(s, v)| (*s, *v, 1.0)));
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
                "type": "supersession", "proposition": "p", "superseded_by": "q", "reason": "r" } },
            { "cycle": 3, "source": "s", "payload": {
                "type": "correction", "proposition": "p", "reason": "r",
                "retracts": { "source": "s", "cycle": 1 }, "value": "True",
                "evidence": { "note": "corrected" } } },
            { "cycle": 4, "source": "s", "payload": {
                "type": "relation_withdrawal", "from": "p", "to": "q",
                "relation": "Supports", "reason": "r" } }
        ]
    }"#;
    let trace: ResearchTrace = serde_json::from_str(object_form).expect("object form parses");
    assert_eq!(trace.events.len(), 4);

    let array_form = r#"[
        { "cycle": 1, "source": "s", "payload": {
            "type": "retraction", "proposition": "p", "reason": "r",
            "retracts": { "cycle": 1 } } },
        { "cycle": 2, "source": "s", "payload": {
            "type": "supersession", "proposition": "p", "superseded_by": "q", "reason": "r" } },
        { "cycle": 3, "source": "s", "payload": {
            "type": "correction", "proposition": "p", "reason": "r",
            "retracts": { "cycle": 1 }, "value": "True" } },
        { "cycle": 4, "source": "s", "payload": {
            "type": "relation_withdrawal", "from": "p", "to": "q",
            "relation": "Supports", "reason": "r" } }
    ]"#;
    let events: Vec<ResearchEvent> = serde_json::from_str(array_form).expect("array form parses");
    let trace2: ResearchTrace = events.into();
    assert_eq!(trace2.events.len(), 4);
}
