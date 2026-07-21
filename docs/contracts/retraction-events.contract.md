# Retraction & Revision Events — contract v0.1 DRAFT

- **Contract:** `retraction-events`
- **Version:** v0.1.0 — **DRAFT** (shape open for review; freeze at the
  Phase-4 milestone of the parent epic, `GuitarAlchemist/hari#12`)
- **Payloads:** additive `ResearchEventPayload` variants in
  `crates/hari-core/src/lib.rs` (serde `#[serde(tag = "type", rename_all = "snake_case")]`).
  **Implemented** — all four variants (`retraction` with its additive
  `retracts` selector, `supersession`, `correction`, `relation_withdrawal`)
  are on the `ResearchEvent` boundary and exercised by `fixtures/revision/`
  (issue #16 belief-revision slices).
- **Examples:** [`fixtures/revision/`](../../fixtures/revision/) — three
  target-behavior replay fixtures with a `README.md` of expected semantics.
- **Design:** [`docs/research/belief-revision-and-retraction.md`](../research/belief-revision-and-retraction.md)
- **Issue:** `GuitarAlchemist/hari#16` (parent epic `#12`)
- **Status:** design ratified (evidence-recompute authoritative); all four
  wire shapes **implemented** on the `ResearchEvent` boundary and
  fixture-backed. Remains v0.1 DRAFT until an owner signs off on the frozen
  shape. Companion to `hari-evidence-lineage` (#15) and
  `source-reliability-summary` (#14).

## Purpose

Define the portable JSON payload shapes for the four **belief-revision
events** Hari accepts on the `ResearchEvent` boundary — `Retraction`,
`Correction`, `RelationWithdrawal`, `Supersession` — so external
autoresearch systems (IX, TARS, Demerzel) can withdraw, correct, retire, or
reverse prior evidence **without erasing history**. Every revision is an
*appended* event over an immutable trace; current belief is *recomputed*
from surviving evidence; withdrawn evidence is *preserved for audit*. The
mechanism is one doctrine — **evidence-recompute is authoritative** — applied
four ways (design doc §3–4).

## Design principles

1. **Append-only, never erase.** A revision event is added to the trace and,
   at the merge layer, a **tombstone** added to the observation G-Set. No
   observation, relation, or claim is ever removed — tombstoning preserves the
   CRDT order-independence proofs (`hari-lattice::merge`) and the audit trail
   simultaneously. (Resolves the merge audit's open question (a): tombstone,
   not delete.)
2. **Recompute is authoritative.** Current belief =
   `recompute(surviving evidence)`. A carried `MergedState` or cached
   derivation is a cache that a revision may invalidate; when cache and
   recompute disagree, recompute wins. Derivations are re-derived every merge,
   so a retraction of a parent dissolves its derivations with **no cascade
   logic**.
3. **Preserve retracted evidence for audit.** Tombstoned evidence stays in the
   observation set and the lineage export (#15), flagged `retracted: true`,
   contributing **zero mass** to the current distribution but fully visible.
   (Resolves open question (b): retracted evidence stays in the maps.)
4. **Explicit retraction only.** Last-write-wins under a dedup key is an
   *implicit* retraction and is refused (`resolve_key_versions` resolves
   same-slot divergence to `Contradictory`, not a silent winner). Withdrawal
   must be an explicit event.
5. **Current vs historical is a replay-endpoint choice.** Replay-to-HEAD gives
   current belief (filters applied); replay-to-cycle-N gives the belief as it
   stood. Revision never rewrites past reports.
6. **Additive evolution.** These variants are additive to
   `ResearchEventPayload`; `Retraction` gains an *optional* `retracts` field
   (backward-compatible). New optional fields are additive; a tag rename or
   removal is a coordinated, signed-off one-way door, mirroring the discipline
   on the lineage and source-reliability contracts.

## Event payloads

All payloads are tagged by a snake_case `type` field, consistent with the
existing `belief_update` / `experiment_result` / `agent_vote` / `retraction` /
`goal_update` / `relation_declaration` variants.

### `retraction` — withdraw prior evidence

```json
{
  "type": "retraction",
  "proposition": "deploy-is-safe",
  "reason": "critic run used a stale config snapshot",
  "retracts": { "source": "ix-agent-critic", "cycle": 2 }
}
```

| field | type | req | meaning |
|---|---|---|---|
| `type` | `"retraction"` | yes | tag |
| `proposition` | string | yes | proposition whose evidence is withdrawn |
| `reason` | string | yes | human-auditable justification (never dropped) |
| `retracts` | selector | no | which prior evidence to withdraw; **absent = all prior evidence for `proposition`** (backward-compatible with the current whole-proposition reset, but via filter-and-recompute, not a mutation to `Unknown`) |

**Selector** (`retracts`) — references prior evidence by any of:
`{ "source": string }`, `{ "cycle": u64 }`, `{ "source": string, "cycle": u64 }`,
or `{ "dedup_key": [source, diagnosis_id, round, ordinal] }` at the merge
layer. The most specific match wins; an unmatched selector is a no-op that is
still logged (honest degradation).

**Semantics.** Tombstone the matched evidence; recompute the proposition and
any derivation that used it as a parent. Report a delta
`(previous → new, cause: retraction)`. Replaces today's `value = Unknown`
mutation (`lib.rs:1347`), which is the A/B baseline this must beat.

### `correction` — atomic retract-plus-replace

```json
{
  "type": "correction",
  "proposition": "model-v3-latency-ms",
  "reason": "original run mislabeled p50 as p99",
  "retracts": { "source": "ix-runner", "cycle": 1 },
  "value": "Probable",
  "evidence": { "p99_ms": 420, "note": "re-measured, corrected label" }
}
```

| field | type | req | meaning |
|---|---|---|---|
| `type` | `"correction"` | yes | tag |
| `proposition` | string | yes | corrected proposition |
| `reason` | string | yes | why the prior evidence was wrong |
| `retracts` | selector | yes | the evidence being corrected (required — a correction with nothing to correct is a `belief_update`) |
| `value` | HexValue | yes | replacement hexavalent value |
| `evidence` | object | yes | replacement evidence blob (audit-preserved) |

**Semantics.** `Retraction(retracts)` + `BeliefUpdate(value, evidence)`
executed atomically and **causally linked** — the lineage records "this
corrects that" (an `is_revised_by` edge), which a bare `belief_update` cannot
express. Delta reported.

### `relation_withdrawal` — reverse an append-only relation

```json
{
  "type": "relation_withdrawal",
  "from": "benchmark-x-is-reliable",
  "to": "deploy-is-safe",
  "relation": "Supports",
  "reason": "benchmark-x retracted; it no longer supports the deploy claim"
}
```

| field | type | req | meaning |
|---|---|---|---|
| `type` | `"relation_withdrawal"` | yes | tag |
| `from` | string | yes | relation source proposition |
| `to` | string | yes | relation target proposition |
| `relation` | `hari_lattice::Relation` | yes | the relation being withdrawn (must match a prior `relation_declaration`) |
| `reason` | string | yes | justification |

**Semantics.** Relations were append-only (see the `RelationDeclaration`
doc-comment in `lib.rs`). Withdrawal **tombstones** the edge in the
`BeliefNetwork`; propagation recomputes without it, dissolving the beliefs the
edge induced — the same recompute doctrine as evidence retraction. An
unmatched `(from, to, relation)` triple is a logged no-op.

### `supersession` — retire a whole claim for a successor

```json
{
  "type": "supersession",
  "proposition": "persona-count-is-14",
  "superseded_by": "persona-count-is-20",
  "reason": "framework expanded beyond the original 14 personas"
}
```

| field | type | req | meaning |
|---|---|---|---|
| `type` | `"supersession"` | yes | tag |
| `proposition` | string | yes | the claim being retired |
| `superseded_by` | string | yes | the successor claim (the new live head) |
| `reason` | string | yes | justification |

**Semantics.** Records a directed `proposition → superseded_by` lineage edge
and **freezes** the superseded proposition: it contributes no mass to any
downstream (dependent) belief, but its own historical values stay replayable.
The head of a supersession chain is the only live claim; the tail is the audit
trail. Chains compose (`A → B → C`); replay-to-an-earlier-point shows the older
claim as it stood.

## Deltas (report side)

A revision that changes a belief emits a delta into `ResearchReplayReport`:

```json
{ "proposition": "deploy-is-safe", "previous": "Contradictory", "current": "True", "cause": "retraction", "cycle": 3 }
```

`cause` ∈ `retraction | correction | relation_withdrawal | supersession`. This
is the `report_delta_between_previous_and_current_belief` semantic and reuses
the lineage `is_revised_by` edge (#15).

## Cross-contract compatibility

- **Evidence lineage (#15)** — additive: retracted `source_item` / `claim` /
  `experiment_event` nodes gain optional `retracted: true`; a new
  `is_retracted_by` edge (evidence → revision event) joins the existing
  `is_revised_by`. Retracted nodes stay in the bundle (preserve-for-audit); no
  `lineage_version` bump forced.
- **Source reliability (#14)** — a retracted/corrected accepted claim is a
  reliability signal: it flips a prior `Accept` outcome. The revision events
  here are the *inputs* that let the reliability ledger grade a source's
  accepted claims honestly. `SourceReliabilityUpdate` is **not** an event in
  this contract — reliability is earned from graded outcomes, owned by #14, not
  declared inbound (design doc §2, §5.2).

## Non-goals (from issue #16)

- Do not physically delete historical evidence, relations, or claims —
  tombstone only.
- Do not let retraction silently rewrite old reports — past reports are
  reproducible by replay-to-N.
- Do not implement multi-premise / full-AGM revision in this slice — JTMS
  foundationalist recompute only (design doc §5).
- No `RelationReplacement`, `SourceReliabilityUpdate`, or `StalenessObservation`
  variants (deferred — design doc §2).

## Open questions (to resolve before freeze)

1. **Selector granularity** — is `{source, cycle}` enough at the
   `ResearchEvent` boundary, or must consumers address the full
   `(source, diagnosis_id, round, ordinal)` dedup key? (Leaning: `{source,
   cycle}` on the boundary, full key at the merge layer, with a documented
   projection between them.)
2. **Supersession freeze depth** — does freezing a superseded proposition also
   freeze relations *out of* it, or only its mass contribution? (Leaning: mass
   only, until a downstream-dependency fixture motivates more.)
3. **Correction vs Retraction+BeliefUpdate on the wire** — keep `correction` as
   a distinct tag (chosen, for the causal link) or require consumers to send
   two linked events? (Leaning: distinct tag; the link is audit-worthy.)
