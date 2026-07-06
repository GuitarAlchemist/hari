# Evidence lineage export — design note

Design rationale for the JSON evidence lineage export (`GuitarAlchemist/hari#15`,
parent epic `#12`). Contract:
[`docs/contracts/hari-evidence-lineage.contract.md`](../contracts/hari-evidence-lineage.contract.md)
(v0.1 DRAFT), schema `hari-evidence-lineage-v0.1.0`.

## Problem

Hari tracks provenance internally — `hari_lattice::Derivation` + `Contribution`
record how each derived belief was produced, and `ResearchReplayReport` records
every event outcome and the final belief/goal state. That provenance is *legible
to Hari* but not *portable*: a consumer (TARS, IX, Streeling, Demerzel) that
wants to audit "why did Hari recommend Escalate on `downstream-pipeline-stable`?"
would have to link against Hari's Rust types or re-run a replay.

The lineage export makes that provenance a **plain JSON artifact on disk** that
any consumer can read with `serde_json` / `json.load` and nothing else.

## Why not a provenance standard (PROV/RDF)?

The candidate concepts (`source_item`, `claim`, `belief_state`, `derived_belief`,
…) and relationships (`is_derived_from`, `is_attributed_to`, …) borrow the
*shape* of provenance graphs. But adopting RDF/PROV would drag in ontology
tooling, namespaces, and (in practice) a triple store — all explicit non-goals.
The ecosystem's canonical handoff is JSON-on-disk; a node/edge JSON graph gives
the same auditability with zero new dependencies. This mirrors the forecast-record
and pr-grade contracts: small, typed, local-first JSON.

## Lineage complements replay — it does not replace it

This is the load-bearing design decision. **Replay** (`hari-core replay
<trace>`) is the authoritative re-execution: given a trace it deterministically
reproduces beliefs, actions, and metrics. **Lineage** is a *read-only projection*
of a single run — a snapshot of what led to what. Consequences:

- Lineage is never executed and never feeds back into the `CognitiveLoop`.
  Round-tripping a lineage bundle yields the same bytes; it has no dynamics.
- Replay stays the source of truth. If lineage and a fresh replay ever disagree,
  replay wins and the exporter has a bug.
- Because it is a projection, lineage can be *filtered* (a derivation-only
  subgraph, a recommendation-only subgraph) without inventing new schemas — the
  candidate `belief-derivation-lineage.json` / `recommendation-lineage.json`
  artifacts are filtered views of the one schema.

The stream form (`.jsonl`) is deliberately replay-friendly: a `meta` line then
node/edge lines, foldable incrementally, so lineage can be appended as a run
proceeds and consumed without loading the whole graph.

## TARS `EvidenceBundle` compatibility

TARS consumes evidence as bundles for its cross-model theory validator. A Hari
lineage bundle is intended to drop into that slot:

- `source_item` / `claim` / `experiment_event` / `agent_vote` → TARS evidence
  items (the "what was observed, by whom").
- `is_attributed_to` edges → TARS attribution (which source backs which claim).
- `supports` / `contradicts` edges → evidence polarity, which is exactly the
  signed relationship TARS reasons over.
- `derived_belief` + `is_derived_from` → the inference chain TARS can replay
  symbolically against its own grammar.

The field-name mapping is documented but **not frozen** — it needs a TARS
round-trip (tracks `GuitarAlchemist/tars#102`, `tars#90`). The v0.1 goal is that
the *shape* is compatible: entities + attributed, polarised relationships, no
Hari-internal types required.

## IX scorecard compatibility

IX already scores Hari runs (`ResearchReplayReport::metrics`:
`false_acceptance_count`, `contradiction_recovery_cycles`, `goal_completion_rate`,
`consensus_stability`, …). The lineage `run_report` node carries a subset of
those same metrics plus the `priority_model`, so an IX scorecard can treat a
lineage bundle as the **provenance side of a run it already scores** — the
scorecard answers "how good was the run", the lineage answers "why did it decide
that". Because `run_report` names the `priority_model`, lineage respects the
project's A/B doctrine: you can diff lineage across `RecencyDecay` vs
`SubjectiveLogic` for the same trace. (`GuitarAlchemist/ix#207`.)

## Cost / operational notes

- **Cost: zero.** Export is a pure in-process serialisation of data Hari already
  holds; no model calls, no network. Fits the issue's `free-local` budget.
- **Size** is linear in events + derivations. The stream form bounds peak memory
  for large runs (fold, don't load).
- **Determinism.** Given a fixed trace + priority model, the bundle is
  byte-stable except for the injected `generated_at` timestamp.

## Status and next slice

v0.1 DRAFT. This slice ships the contract, schema, and validated examples plus a
minimal serde *shape anchor* (`crates/hari-core/src/lineage.rs` +
`tests/lineage_export.rs`). Deliberately **out of scope** for v0.1: wiring an
automatic exporter from a live `ResearchReplayReport`, the filtered-projection
CLI, and the frozen TARS field mapping. Those follow once the shape freezes at
the parent epic's Phase-4 milestone.
