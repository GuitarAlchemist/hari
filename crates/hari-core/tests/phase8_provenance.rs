//! Phase 8 — derivation provenance.
//!
//! When `BeliefNetwork` propagation derives a value for a proposition,
//! the cognitive loop now records a structured `Derivation` chain on
//! the `ResearchEventOutcome` and emits one human-readable
//! `Action::Log` per derived belief. This test suite pins:
//!
//! 1. The structured `derivations` field is populated when propagation
//!    actually fires.
//! 2. Multi-hop chains (Implies + Supports) record one derivation per
//!    hop, in the correct round.
//! 3. `Contradicts` records the contributed value AFTER the NOT() rule
//!    so consumers don't have to reconstruct the algebra.
//! 4. **Regression**: relation-free fixtures produce zero derivations
//!    AND the JSON shape stays byte-equal (the `derivations` field is
//!    `skip_serializing_if = Vec::is_empty`).
//! 5. The wire format is stable: `derivations` round-trips through
//!    serde and is omitted entirely when empty.

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
// 1. Multi-hop derivation produces structured provenance
// ---------------------------------------------------------------------------

#[test]
fn multi_hop_derivation_records_provenance_per_hop() {
    let trace = load_trace("../../fixtures/ix/derivation.json");
    let mut loop_ = CognitiveLoop::new(trace.dimension);
    let report = loop_.process_research_trace(trace);

    // Find the outcome of the cycle-4 event (cosmic-flux-bounded =
    // True). That's the event that should have triggered:
    //   round 1 → cosmic-flux-stable derived True
    //   round 2 → downstream-pipeline-stable derived True
    let cycle_4_outcome = report
        .outcomes
        .iter()
        .find(|o| o.event.cycle == 4)
        .expect("cycle 4 outcome present");

    // Two derivations expected (one per derived proposition).
    assert!(
        cycle_4_outcome.derivations.len() >= 2,
        "expected at least 2 derivations for the 2-hop chain, got {}: {:?}",
        cycle_4_outcome.derivations.len(),
        cycle_4_outcome.derivations
    );

    let stable = cycle_4_outcome
        .derivations
        .iter()
        .find(|d| d.proposition == "cosmic-flux-stable")
        .expect("cosmic-flux-stable derivation");
    assert_eq!(stable.previous_value, HexValue::Unknown);
    assert_eq!(stable.new_value, HexValue::True);
    assert_eq!(stable.round, 1);
    assert_eq!(stable.contributions.len(), 1);
    let c = &stable.contributions[0];
    assert_eq!(c.source, "cosmic-flux-bounded");
    assert_eq!(c.relation, Relation::Implies);
    assert_eq!(c.source_value, HexValue::True);
    assert_eq!(c.contributed_value, HexValue::True);

    let downstream = cycle_4_outcome
        .derivations
        .iter()
        .find(|d| d.proposition == "downstream-pipeline-stable")
        .expect("downstream-pipeline-stable derivation");
    assert_eq!(downstream.round, 2, "second hop fires in round 2");
    assert_eq!(downstream.contributions[0].source, "cosmic-flux-stable");
}

// ---------------------------------------------------------------------------
// 2. Per-derivation Log actions are emitted alongside the structured field
// ---------------------------------------------------------------------------

#[test]
fn derivation_log_actions_name_the_sources() {
    let mut loop_ = CognitiveLoop::new(4);
    loop_.process_research_event(ResearchEvent {
        cycle: 1,
        source: "m".into(),
        payload: ResearchEventPayload::RelationDeclaration {
            from: "premise".into(),
            to: "conclusion".into(),
            relation: Relation::Implies,
        },
    });
    let outcome = loop_.process_research_event(ResearchEvent {
        cycle: 2,
        source: "exp".into(),
        payload: ResearchEventPayload::ExperimentResult {
            proposition: "premise".into(),
            value: HexValue::True,
            evidence: Default::default(),
        },
    });

    // The structured derivation is present...
    assert_eq!(outcome.derivations.len(), 1);
    assert_eq!(outcome.derivations[0].proposition, "conclusion");

    // ...AND a "Derived 'conclusion'" log action mentions the source.
    // Under the RecencyDecay default the substantive Accept(premise)
    // action is high-priority so all actions stay (no all-suppressed
    // collapse).
    let derived_logs: Vec<&str> = outcome
        .actions
        .iter()
        .filter_map(|a| match a {
            hari_core::Action::Log(s) if s.starts_with("Derived ") => Some(s.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(derived_logs.len(), 1, "exactly one derivation log expected");
    let log = derived_logs[0];
    assert!(log.contains("'conclusion'"), "log: {log}");
    assert!(log.contains("premise"), "log must name the source: {log}");
    assert!(log.contains("Implies"), "log must name the relation: {log}");
}

// ---------------------------------------------------------------------------
// 3. Regression: relation-free fixtures produce zero derivations and the
//    JSON shape stays byte-equal (the `derivations` field is skip-empty).
// ---------------------------------------------------------------------------

#[test]
fn relation_free_fixtures_emit_no_derivations() {
    for path in [
        "../../fixtures/ix/cognition_divergence.json",
        "../../fixtures/ix/swarm_dissent.json",
        "../../fixtures/ix/long_recovery.json",
    ] {
        let trace = load_trace(path);
        let mut loop_ = CognitiveLoop::new(trace.dimension);
        let report = loop_.process_research_trace(trace);
        for outcome in &report.outcomes {
            assert!(
                outcome.derivations.is_empty(),
                "{path}: relation-free fixture produced {} derivations on event cycle {}",
                outcome.derivations.len(),
                outcome.event.cycle
            );
        }
    }
}

#[test]
fn empty_derivations_are_omitted_from_json() {
    // The `skip_serializing_if = "Vec::is_empty"` on
    // ResearchEventOutcome::derivations means existing recorded
    // sessions and stored reports keep their byte-shape — nothing
    // appears for events that did no propagation.
    let mut loop_ = CognitiveLoop::new(4);
    let outcome = loop_.process_research_event(ResearchEvent {
        cycle: 1,
        source: "exp".into(),
        payload: ResearchEventPayload::ExperimentResult {
            proposition: "p".into(),
            value: HexValue::True,
            evidence: Default::default(),
        },
    });
    let s = serde_json::to_string(&outcome).unwrap();
    assert!(
        !s.contains("derivations"),
        "empty derivations must not appear in the serialized outcome; got: {s}"
    );

    // Sanity: round-trip still works (default empties on absence).
    let back: hari_core::ResearchEventOutcome = serde_json::from_str(&s).unwrap();
    assert!(back.derivations.is_empty());
}

// ---------------------------------------------------------------------------
// 4. Non-empty derivations DO appear in JSON with the full contribution chain
// ---------------------------------------------------------------------------

#[test]
fn populated_derivations_round_trip_through_serde() {
    let mut loop_ = CognitiveLoop::new(4);
    loop_.process_research_event(ResearchEvent {
        cycle: 1,
        source: "m".into(),
        payload: ResearchEventPayload::RelationDeclaration {
            from: "a".into(),
            to: "b".into(),
            relation: Relation::Supports,
        },
    });
    let outcome = loop_.process_research_event(ResearchEvent {
        cycle: 2,
        source: "exp".into(),
        payload: ResearchEventPayload::ExperimentResult {
            proposition: "a".into(),
            value: HexValue::True,
            evidence: Default::default(),
        },
    });

    assert_eq!(outcome.derivations.len(), 1);
    let s = serde_json::to_string(&outcome).unwrap();
    // The new field appears with full structure...
    assert!(s.contains("\"derivations\""));
    assert!(s.contains("\"proposition\":\"b\""));
    assert!(s.contains("\"new_value\":\"True\""));
    assert!(s.contains("\"contributions\""));
    assert!(s.contains("\"source\":\"a\""));
    assert!(s.contains("\"relation\":\"Supports\""));

    // ...and round-trips losslessly.
    let back: hari_core::ResearchEventOutcome = serde_json::from_str(&s).unwrap();
    assert_eq!(back.derivations.len(), 1);
    assert_eq!(back.derivations[0].proposition, "b");
    assert_eq!(back.derivations[0].contributions[0].source, "a");
    assert_eq!(
        back.derivations[0].contributions[0].relation,
        Relation::Supports
    );
}

// ---------------------------------------------------------------------------
// 5. Contradicts contribution carries NOT(source) so consumers don't replay the rule
// ---------------------------------------------------------------------------

#[test]
fn contradicts_contribution_records_negated_value() {
    let mut loop_ = CognitiveLoop::new(4);
    // Wire stable as already True, then declare anomaly Contradicts stable.
    loop_
        .state
        .beliefs
        .add_proposition("stable", HexValue::True);
    loop_.process_research_event(ResearchEvent {
        cycle: 1,
        source: "m".into(),
        payload: ResearchEventPayload::RelationDeclaration {
            from: "anomaly".into(),
            to: "stable".into(),
            relation: Relation::Contradicts,
        },
    });
    // Drive anomaly True → stable should derive Contradictory.
    let outcome = loop_.process_research_event(ResearchEvent {
        cycle: 2,
        source: "exp".into(),
        payload: ResearchEventPayload::ExperimentResult {
            proposition: "anomaly".into(),
            value: HexValue::True,
            evidence: Default::default(),
        },
    });

    let stable = outcome
        .derivations
        .iter()
        .find(|d| d.proposition == "stable")
        .expect("stable derivation present");
    assert_eq!(stable.new_value, HexValue::Contradictory);
    let c = &stable.contributions[0];
    assert_eq!(c.source, "anomaly");
    assert_eq!(c.relation, Relation::Contradicts);
    assert_eq!(c.source_value, HexValue::True);
    assert_eq!(
        c.contributed_value,
        HexValue::False,
        "Contradicts contributes NOT(source); consumers see the post-NOT value"
    );
}
