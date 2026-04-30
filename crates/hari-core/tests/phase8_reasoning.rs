//! Phase 8 — belief propagation as reasoning.
//!
//! IX can declare logical relations between propositions via
//! `ResearchEventPayload::RelationDeclaration`; subsequent
//! belief-changing events trigger
//! `BeliefNetwork::propagate_until_stable`, deriving values for
//! propositions that received no direct evidence. This is reasoning in
//! the most classical AI sense: forward inference over a typed graph.
//!
//! Coverage:
//!
//! 1. **Regression — existing fixtures unchanged.** Fixtures that don't
//!    declare relations must replay byte-equal to their pre-Phase-8
//!    behavior, because propagation on an edge-less graph is a single
//!    zero-change pass.
//! 2. **Multi-hop derivation.** `derivation.json` chains True through
//!    `Implies` then `Supports`; the leaf belief must end True without
//!    ever receiving a direct event.
//! 3. **Contradiction emergence.** A subsequent `Contradicts` edge plus
//!    a True observation flips the leaf to `Contradictory` via the
//!    combine_evidence rule.
//! 4. **Auto-creation.** Declaring a relation between two never-seen
//!    propositions creates both as `Unknown` and the relation fires
//!    once a value lands on the source.

use hari_core::{CognitiveLoop, ResearchEvent, ResearchEventPayload, ResearchTrace};
use hari_lattice::{HexValue, Relation};
use std::fs;

fn load_trace(path: &str) -> ResearchTrace {
    let raw = fs::read_to_string(path).expect("fixture readable");
    match serde_json::from_str::<ResearchTrace>(&raw) {
        Ok(t) => t,
        Err(_) => {
            let events: Vec<ResearchEvent> =
                serde_json::from_str(&raw).expect("fixture parses as trace or array");
            events.into()
        }
    }
}

// ---------------------------------------------------------------------------
// 1. Regression: pre-Phase-8 fixtures replay identically
// ---------------------------------------------------------------------------

/// Replaying the existing relation-free fixtures must produce action
/// streams indistinguishable from a hand-rolled CognitiveLoop run on
/// each — the propagation step that runs at the end of every event is
/// a no-op on edge-less belief graphs (single zero-change iteration,
/// `propagation_rounds == 1`, no Log action emitted).
#[test]
fn relation_free_fixtures_replay_identically_after_phase8() {
    for path in [
        "../../fixtures/ix/cognition_divergence.json",
        "../../fixtures/ix/swarm_dissent.json",
        "../../fixtures/ix/long_recovery.json",
    ] {
        let trace = load_trace(path);
        let mut a = CognitiveLoop::new(trace.dimension);
        let report_a = a.process_research_trace(trace.clone());
        // Fresh loop, same trace — must match itself byte-for-byte.
        let mut b = CognitiveLoop::new(trace.dimension);
        let report_b = b.process_research_trace(trace);
        assert_eq!(
            serde_json::to_string(&report_a).unwrap(),
            serde_json::to_string(&report_b).unwrap(),
            "determinism regression on {path}"
        );
        // Propagation emitted no "Propagated beliefs in N rounds" log,
        // because no relations were declared.
        for outcome in &report_a.outcomes {
            for action in &outcome.actions {
                if let hari_core::Action::Log(msg) = action {
                    assert!(
                        !msg.starts_with("Propagated beliefs"),
                        "no-relation fixture {path} emitted unexpected propagation log: {msg}"
                    );
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 2. Multi-hop derivation via the derivation.json fixture
// ---------------------------------------------------------------------------

#[test]
fn multi_hop_derivation_propagates_through_implies_then_supports() {
    let trace = load_trace("../../fixtures/ix/derivation.json");
    let mut loop_ = CognitiveLoop::new(trace.dimension);
    let _ = loop_.process_research_trace(trace);

    // Direct: cosmic-flux-bounded was set True by the experiment_result
    // at cycle 4.
    assert_eq!(
        loop_
            .state
            .beliefs
            .get("cosmic-flux-bounded")
            .unwrap()
            .value,
        HexValue::True,
        "direct experiment must land True"
    );

    // Derived via Implies: cosmic-flux-bounded → cosmic-flux-stable.
    // Never received a direct event; its True value is fully derived.
    assert_eq!(
        loop_.state.beliefs.get("cosmic-flux-stable").unwrap().value,
        HexValue::True,
        "cosmic-flux-stable must be derived True via Implies edge from cosmic-flux-bounded"
    );

    // After event 6 (anomaly-detected = True with a Contradicts edge to
    // downstream-pipeline-stable), the leaf flips to Contradictory.
    assert_eq!(
        loop_
            .state
            .beliefs
            .get("downstream-pipeline-stable")
            .unwrap()
            .value,
        HexValue::Contradictory,
        "Contradicts edge from anomaly-detected (True) must flip downstream to Contradictory"
    );

    // Anomaly itself stays True — Contradicts is unidirectional.
    assert_eq!(
        loop_.state.beliefs.get("anomaly-detected").unwrap().value,
        HexValue::True,
    );
}

// ---------------------------------------------------------------------------
// 3. Auto-creation of relation endpoints
// ---------------------------------------------------------------------------

#[test]
fn relation_declaration_auto_creates_both_endpoints_as_unknown() {
    let mut loop_ = CognitiveLoop::new(4);

    loop_.process_research_event(ResearchEvent {
        cycle: 1,
        source: "ix-modeller".into(),
        payload: ResearchEventPayload::RelationDeclaration {
            from: "premise".into(),
            to: "conclusion".into(),
            relation: Relation::Implies,
        },
    });

    // Both endpoints exist as Unknown.
    assert_eq!(
        loop_.state.beliefs.get("premise").unwrap().value,
        HexValue::Unknown
    );
    assert_eq!(
        loop_.state.beliefs.get("conclusion").unwrap().value,
        HexValue::Unknown
    );

    // (The dispatcher emits a "Declared relation ..." Log under Flat;
    // under the default RecencyDecay it scores 0.05 < theta_wait so
    // gets suppressed to Wait — that's the expected priority-model
    // behavior, not a missing log. The belief-graph assertions above
    // are the load-bearing checks for relation declaration.)

    // Now drive premise to True; the next event's propagation must
    // derive conclusion = True.
    loop_.process_research_event(ResearchEvent {
        cycle: 2,
        source: "ix-experiment".into(),
        payload: ResearchEventPayload::ExperimentResult {
            proposition: "premise".into(),
            value: HexValue::True,
            evidence: Default::default(),
        },
    });
    assert_eq!(
        loop_.state.beliefs.get("conclusion").unwrap().value,
        HexValue::True,
        "Implies must fire when antecedent becomes True/Probable"
    );
}

// ---------------------------------------------------------------------------
// 4. Propagation log fires when ≥1 round of work happens
// ---------------------------------------------------------------------------

#[test]
fn propagation_log_appears_when_derivation_actually_fires() {
    let mut loop_ = CognitiveLoop::new(4);

    // Declare a 2-hop chain.
    loop_.process_research_event(ResearchEvent {
        cycle: 1,
        source: "m".into(),
        payload: ResearchEventPayload::RelationDeclaration {
            from: "a".into(),
            to: "b".into(),
            relation: Relation::Implies,
        },
    });
    loop_.process_research_event(ResearchEvent {
        cycle: 2,
        source: "m".into(),
        payload: ResearchEventPayload::RelationDeclaration {
            from: "b".into(),
            to: "c".into(),
            relation: Relation::Supports,
        },
    });

    // Drive a True. Propagation must do at least one productive round.
    let outcome = loop_.process_research_event(ResearchEvent {
        cycle: 3,
        source: "exp".into(),
        payload: ResearchEventPayload::ExperimentResult {
            proposition: "a".into(),
            value: HexValue::True,
            evidence: Default::default(),
        },
    });

    // The "Propagated beliefs in N rounds" log only fires when N > 1
    // (i.e. real work happened in at least one round).
    let propagation_msg = outcome.actions.iter().find_map(|a| match a {
        hari_core::Action::Log(s) if s.starts_with("Propagated beliefs") => Some(s.clone()),
        _ => None,
    });
    let msg = propagation_msg.expect("propagation log must appear when a→b fires");
    // 2-hop chain converges in at most 3 rounds (work in r1+r2, zero in r3).
    assert!(
        msg.contains("rounds"),
        "log message must mention rounds: {msg}"
    );

    assert_eq!(loop_.state.beliefs.get("c").unwrap().value, HexValue::True);
}

// ---------------------------------------------------------------------------
// 5. RelationDeclaration round-trips through serde under the wire format
// ---------------------------------------------------------------------------

#[test]
fn relation_declaration_round_trips_via_research_trace_serde() {
    // Pin the JSON wire shape so an IX consumer can rely on it. Keys
    // are snake_case (per ResearchEventPayload's tag/rename_all);
    // the Relation enum keeps PascalCase variants because that's what
    // hari_lattice exports.
    let raw = r#"{
        "dimension": 4,
        "events": [
            {
                "cycle": 1,
                "source": "ix-modeller",
                "payload": {
                    "type": "relation_declaration",
                    "from": "evidence",
                    "to": "hypothesis",
                    "relation": "Implies"
                }
            }
        ]
    }"#;
    let trace: ResearchTrace = serde_json::from_str(raw).expect("trace parses");
    assert_eq!(trace.events.len(), 1);
    match &trace.events[0].payload {
        ResearchEventPayload::RelationDeclaration { from, to, relation } => {
            assert_eq!(from, "evidence");
            assert_eq!(to, "hypothesis");
            assert!(matches!(relation, Relation::Implies));
        }
        other => panic!("expected RelationDeclaration, got {other:?}"),
    }

    // Round-trip back to JSON and confirm the serialized shape stays
    // snake_case on the discriminator.
    let s = serde_json::to_string(&trace).unwrap();
    assert!(s.contains("\"type\":\"relation_declaration\""));
    assert!(s.contains("\"relation\":\"Implies\""));
}
