//! Shape-anchor tests for the evidence lineage export (issue #15).
//!
//! These guard that the Rust types in `hari_core::lineage` stay in lock-step
//! with the JSON schema + example fixtures. If someone edits the fixture in a
//! way the types can't represent (or vice versa), one of these fails.
//!
//! Contract: `docs/contracts/hari-evidence-lineage.contract.md` (v0.1 DRAFT).

use hari_core::lineage::{LineageBundle, LineageEdge, LineageNode, LINEAGE_VERSION};
use std::collections::BTreeSet;
use std::fs;

const BUNDLE_PATH: &str = "../../fixtures/lineage/hari-lineage.example.json";

fn load_bundle() -> LineageBundle {
    let raw = fs::read_to_string(BUNDLE_PATH).unwrap_or_else(|e| panic!("read {BUNDLE_PATH}: {e}"));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("deserialise bundle: {e}"))
}

/// The example bundle deserialises into the typed model.
#[test]
fn example_bundle_deserialises() {
    let bundle = load_bundle();
    assert_eq!(bundle.lineage_version, LINEAGE_VERSION);
    assert_eq!(bundle.run.priority_model, "RecencyDecay");
    assert!(!bundle.nodes.is_empty());
    assert!(!bundle.edges.is_empty());
}

/// Round-trip: deserialise -> serialise -> deserialise is a fixed point. Proves
/// the types lose no information from the fixture.
#[test]
fn example_bundle_round_trips() {
    let bundle = load_bundle();
    let reserialised = serde_json::to_string(&bundle).expect("serialise");
    let again: LineageBundle = serde_json::from_str(&reserialised).expect("re-deserialise");
    assert_eq!(bundle, again, "round-trip must be a fixed point");
}

/// All nine entity kinds and all eight relationships appear in the example —
/// the fixture is a full coverage exemplar of the schema, and the typed model
/// carries every one.
#[test]
fn example_bundle_covers_all_kinds_and_relationships() {
    let bundle = load_bundle();

    let kinds: BTreeSet<&str> = bundle
        .nodes
        .iter()
        .map(|n: &LineageNode| n.kind.as_str())
        .collect();
    for expected in [
        "source_item",
        "claim",
        "belief_state",
        "experiment_event",
        "agent_vote",
        "consensus_result",
        "derived_belief",
        "recommendation",
        "run_report",
    ] {
        assert!(kinds.contains(expected), "missing node kind: {expected}");
    }

    let rels: BTreeSet<&str> = bundle
        .edges
        .iter()
        .map(|e: &LineageEdge| e.rel.as_str())
        .collect();
    for expected in [
        "uses",
        "supports",
        "contradicts",
        "is_derived_from",
        "is_attributed_to",
        "is_part_of_run",
        "is_revised_by",
        "leads_to_recommendation",
    ] {
        assert!(rels.contains(expected), "missing relationship: {expected}");
    }
}

/// Referential integrity: every edge endpoint resolves to a declared node id.
#[test]
fn example_bundle_edges_reference_known_nodes() {
    let bundle = load_bundle();
    let ids: BTreeSet<&str> = bundle.nodes.iter().map(|n| n.id.as_str()).collect();
    for e in &bundle.edges {
        assert!(
            ids.contains(e.from.as_str()),
            "dangling edge.from: {}",
            e.from
        );
        assert!(ids.contains(e.to.as_str()), "dangling edge.to: {}", e.to);
    }
}

/// The redaction accessor degrades honestly and matches the fixture's declared
/// redaction (one withheld source item).
#[test]
fn redacted_source_count_matches_fixture() {
    let bundle = load_bundle();
    assert_eq!(bundle.redacted_source_count(), 1);
}
