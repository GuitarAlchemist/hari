# Hari Evidence Lineage Export — contract v0.1 DRAFT

- **Contract:** `hari-evidence-lineage`
- **Version:** v0.1.0 — **DRAFT** (shape open for review; freeze at the Phase-4 milestone of the parent epic, `GuitarAlchemist/hari#12`)
- **Schema:** [`hari-evidence-lineage.schema.json`](hari-evidence-lineage.schema.json) (`hari-evidence-lineage-v0.1.0`)
- **Examples:** [`fixtures/lineage/hari-lineage.example.json`](../../fixtures/lineage/hari-lineage.example.json) (bundle), [`fixtures/lineage/hari-lineage.example.jsonl`](../../fixtures/lineage/hari-lineage.example.jsonl) (stream)
- **Issue:** `GuitarAlchemist/hari#15`
- **Status:** JSON shape accepted hari-side as a draft; remains v0.1 DRAFT until the Demerzel tribunal signs off and the parent epic reaches its freeze milestone.

## Purpose

Hari already tracks derivation provenance internally — `hari_lattice::Derivation`
+ `Contribution` capture how each derived belief was produced, and
`ResearchReplayReport` records every event outcome and final belief/goal state.
This contract defines a **portable, JSON-first externalisation** of that
provenance so TARS, IX, Streeling, and Demerzel can consume Hari's reasoning as
an auditable **evidence bundle** without linking against Hari's Rust types.

It borrows the *general idea* of provenance graphs (entities + relationships)
but deliberately stays **JSON-first and local-first**: no RDF, no ontology
dependency, no triple store, no database. A lineage export is a plain JSON
object (or a JSONL stream) written to disk, exactly like every other JSON-on-disk
handoff in the GuitarAlchemist ecosystem.

## Design principles

1. **JSON-first, local-first.** One JSON file (`.json`) or one append-only
   stream (`.jsonl`). Readable with `serde_json` / `json.load` and nothing else.
2. **Complements replay, does not replace it.** Replay (`hari-core replay`) is
   the authoritative re-execution; lineage is a *read-only projection* of a run
   for audit and cross-repo consumption. A lineage export never needs to be
   executed and never feeds back into the cognitive loop. See
   [`docs/research/evidence-lineage-export.md`](../research/evidence-lineage-export.md).
3. **Honest degradation.** Every field beyond the minimal required set is
   optional; a consumer that hits an absent field degrades explicitly rather
   than failing. Absence of a `redaction` block means nothing was redacted.
4. **No private-detail leakage.** Raw source payloads are never exported without
   a declared `redaction` model (see below).
5. **Additive evolution.** New node kinds, relationships, or optional fields are
   additive; changing `lineage_version`'s major/minor is a coordinated,
   signed-off change (one-way door), mirroring the discipline on `ResearchEvent`
   payload tags and OPTIC-K partitions elsewhere in the ecosystem.

## Model

A lineage export is a directed graph of typed **nodes** connected by typed
**edges**, wrapped in a small envelope.

```
{ lineage_version, run, nodes[], edges[], redaction? }
```

### Entities (node `kind`)

| kind               | Hari origin                                             | Meaning |
|--------------------|---------------------------------------------------------|---------|
| `source_item`      | `ResearchEvent.source`                                  | An agent, runner, benchmark, or channel that emitted evidence. Carries a coarse `origin`, never raw private content. |
| `claim`            | asserted `proposition` + value                          | A single asserted proposition/value, before it becomes a settled belief. |
| `belief_state`     | `BeliefNetwork` node / `Proposition`                    | A proposition's hexavalent value at a point in the run (optional `evidence_weight`, `cycle`). |
| `experiment_event` | `ResearchEventPayload::ExperimentResult`                | A benchmark/experiment result with its `evidence` blob. |
| `agent_vote`       | `ResearchEventPayload::AgentVote`                       | One agent's vote on a proposition (optional `role`). |
| `consensus_result` | `Swarm::consensus_with` output                          | A fused consensus value under a `trust_model` (`Equal` / `RoleWeighted`). |
| `derived_belief`   | `hari_lattice::Derivation`                              | A belief change produced by propagation: `previous_value` → `new_value` at a 1-indexed `round`. |
| `recommendation`   | `Action` returned to IX                                 | A recommended `action` (`Investigate` / `Retry` / `Accept` / `Escalate` / `Wait` / …). |
| `run_report`       | `ResearchReplayReport`                                  | The run this lineage was exported from; anchors `is_part_of_run`. |

### Relationships (edge `rel`)

| rel                       | Direction (from → to)                          | Meaning |
|---------------------------|------------------------------------------------|---------|
| `uses`                    | consumer → input                               | A node consumed another as input (e.g. a `belief_state` uses a `claim`; a `recommendation` uses a `consensus_result`). |
| `supports`                | source → target                                | `hari_lattice::Relation::Supports` lifted into the export. |
| `contradicts`             | source → target                                | `hari_lattice::Relation::Contradicts`. |
| `is_derived_from`         | derived → antecedent                           | A `derived_belief`'s antecedent(s), one edge per `Contribution`; carries the propagation `round`. |
| `is_attributed_to`        | evidence → `source_item`                       | Provenance attribution of a claim/experiment/vote to its source. |
| `is_part_of_run`          | any node → `run_report`                        | Run membership. |
| `is_revised_by`           | old belief → revision                          | A belief_state/derived_belief was later revised (by a derivation, retraction, or consensus). |
| `leads_to_recommendation` | belief/consensus → `recommendation`            | The belief or consensus that triggered a recommended action. |

Edges may carry an optional `round` (for propagation-derived edges) and an
optional `evidence` blob (e.g. the `contributed_value` from a `Contribution`).

### Redaction model

Raw `evidence` blobs can contain private detail (internal reviewer ids, customer
strings). The exporter MAY withhold detail; when it does it MUST declare a
top-level `redaction` object naming the `policy` and the `redacted_node_ids`.
The hard invariant, enforced by the schema: a `source_item` with
`"redacted": true` MUST NOT carry a `detail` field. Coarse `origin` labels and
belief/vote *values* stay in the export (they are the auditable substance);
only the private payload is dropped. Absence of `redaction` asserts that nothing
was withheld.

## Serialisation forms

- **Bundle** (`hari-lineage.json`): one JSON object conforming to the schema
  root. Best for a complete, self-contained audit artifact.
- **Stream** (`hari-lineage.jsonl`): newline-delimited records conforming to
  `#/$defs/streamRecord`. Line 1 is a `meta` record (`lineage_version` + `run` +
  optional `redaction`); subsequent lines are `{"record":"node","node":{…}}` or
  `{"record":"edge","edge":{…}}`. Replay-friendly: a consumer folds the stream
  without loading the whole graph, and an exporter can append lineage as a run
  proceeds. The bundle and stream carry identical information.

Suggested filenames (per issue #15): `hari-lineage.json`, `hari-lineage.jsonl`,
`belief-derivation-lineage.json`, `recommendation-lineage.json` — the last two
are filtered projections (derivation subgraph / recommendation subgraph) of the
same schema, not separate schemas.

## Cross-repo compatibility (see the research note for detail)

- **TARS `EvidenceBundle`** — a Hari lineage bundle is intended to drop into a
  TARS evidence slot: `source_item`/`claim`/`experiment_event` map to TARS
  evidence items, and the `is_attributed_to` / `supports` / `contradicts` edges
  give TARS the attribution and polarity it needs. The mapping is documented,
  not yet frozen.
- **IX scorecard** — `run_report.metrics` mirrors `ResearchReplayReport::metrics`
  (`false_acceptance_count`, `contradiction_recovery_cycles`, …), so an IX
  scorecard can read a lineage bundle as the provenance side of the same run it
  already scores. The A/B doctrine holds: `run_report` names the `priority_model`
  so a scorecard can compare lineage across models.

## Non-goals (from issue #15)

- No RDF / ontology dependency. No triple store.
- No database — a bundle or stream on disk is the whole substrate.
- Lineage does not replace replay; it is a read-only projection of a run.
- No private-detail export without a declared redaction model.

## Open questions (to resolve before freeze)

1. Should `belief-derivation-lineage.json` / `recommendation-lineage.json`
   filtered projections be produced by the exporter, or left to consumers to
   filter? (Leaning: consumer-side filter; one schema, one exporter.)
2. Exact TARS `EvidenceBundle` field names for the mapping table — needs a TARS
   round-trip (tracks `GuitarAlchemist/tars#102`, `#90`).
3. Whether `consensus_result` should record per-agent weights or only the fused
   value + participants (v0.1 records participants only).

## Reference implementation

A minimal serde anchor for the bundle lives in
`crates/hari-core/src/lineage.rs` (`LineageBundle` / `LineageNode` /
`LineageEdge`), with a round-trip + schema-shape test in
`crates/hari-core/tests/lineage_export.rs` that reads the example fixture. It is
a *shape anchor* for the contract, not a wired-up exporter — automatic export
from a live `ResearchReplayReport` is the next slice, after the shape freezes.
