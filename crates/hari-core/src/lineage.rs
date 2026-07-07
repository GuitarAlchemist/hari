//! # Evidence lineage export (issue #15)
//!
//! A portable, **JSON-first, local-first** externalisation of the provenance
//! Hari already tracks internally (`hari_lattice::Derivation` + `Contribution`,
//! [`crate::ResearchReplayReport`]). A lineage export is a directed graph of
//! typed **nodes** connected by typed **edges**, wrapped in a small envelope —
//! consumable by TARS, IX, Streeling, and Demerzel with `serde_json` and
//! nothing else. No RDF, no ontology, no database.
//!
//! Contract + schema:
//! - `docs/contracts/hari-evidence-lineage.contract.md` (v0.1 DRAFT)
//! - `docs/contracts/hari-evidence-lineage.schema.json`
//!   (`hari-evidence-lineage-v0.1.0`)
//! - examples: `fixtures/lineage/hari-lineage.example.{json,jsonl}`
//!
//! This module is a **shape anchor** for the contract, not a wired-up
//! exporter: it deserialises and re-serialises a conforming bundle so the Rust
//! types and the JSON schema cannot silently drift. Automatic export from a
//! live [`crate::ResearchReplayReport`] is the next slice, after the shape
//! freezes (see the contract's "Open questions").
//!
//! Design choices that mirror the rest of hari-core:
//! - Nodes keep their kind-specific fields in a flattened `extra` map so the
//!   type round-trips any schema-conforming node without enumerating every
//!   per-kind field here — the JSON schema is the authoritative shape, this is
//!   the byte-preserving anchor. This is the same "preserve evidence verbatim
//!   for audit" stance as [`crate::Evidence`].
//! - Everything beyond the required envelope is optional; a consumer that hits
//!   an absent field degrades explicitly (honest degradation).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Current lineage schema identity. Bumping the major/minor is a coordinated,
/// signed-off change (one-way door), like the `ResearchEvent` payload tags.
pub const LINEAGE_VERSION: &str = "hari-evidence-lineage-v0.1.0";

/// Top-level lineage export envelope (the bundle form). Matches the root of
/// `hari-evidence-lineage.schema.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LineageBundle {
    /// Schema identity string, e.g. [`LINEAGE_VERSION`].
    pub lineage_version: String,
    /// Header identifying the replay/session this lineage came from.
    pub run: LineageRun,
    /// Typed nodes (entities).
    pub nodes: Vec<LineageNode>,
    /// Typed edges (relationships).
    pub edges: Vec<LineageEdge>,
    /// Declares that private source detail was withheld. Absent means nothing
    /// was redacted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redaction: Option<LineageRedaction>,
}

/// Run header. `run_id` + `priority_model` are required; everything else is
/// optional and skipped when absent to keep exports byte-stable.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LineageRun {
    pub run_id: String,
    pub priority_model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dimension: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generated_at: Option<String>,
}

/// A lineage node. `id` + `kind` are the discriminating required fields; the
/// remaining kind-specific fields are preserved verbatim in `extra` so this
/// type never drifts from the JSON schema.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LineageNode {
    pub id: String,
    /// One of the nine entity kinds (`source_item`, `claim`, `belief_state`,
    /// `experiment_event`, `agent_vote`, `consensus_result`, `derived_belief`,
    /// `recommendation`, `run_report`).
    pub kind: String,
    /// Kind-specific fields, preserved for audit.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// A typed edge between two node ids. `from` / `to` / `rel` are required; the
/// propagation `round` and per-edge `evidence` blob are optional.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LineageEdge {
    pub from: String,
    pub to: String,
    /// One of the eight relationships (`uses`, `supports`, `contradicts`,
    /// `is_derived_from`, `is_attributed_to`, `is_part_of_run`,
    /// `is_revised_by`, `leads_to_recommendation`).
    pub rel: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub round: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<BTreeMap<String, serde_json::Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// Declares that private source detail was withheld. The hard invariant (a
/// redacted `source_item` carries no `detail`) is enforced by the JSON schema,
/// not re-checked here — this type only records the declaration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LineageRedaction {
    pub policy: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub redacted_node_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl LineageBundle {
    /// Number of `source_item` nodes withheld under the redaction policy, or 0
    /// when nothing was redacted. A tiny consuming decision that exercises the
    /// redaction model (honest degradation: no redaction block => 0).
    pub fn redacted_source_count(&self) -> usize {
        self.redaction
            .as_ref()
            .map(|r| r.redacted_node_ids.len())
            .unwrap_or(0)
    }
}
