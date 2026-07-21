# Belief-revision replay fixtures

Five deterministic replay fixtures for the belief-revision design
(`docs/research/belief-revision-and-retraction.md`, issue #16). Each
isolates one semantic and each is an **A/B case** against the naive
last-write-wins baseline (design §7).

**All five replay** (issue #16 retraction tracer slice + merge-weight slice +
correction / relation-withdrawal slice). `hari-core replay
fixtures/revision/<name>.json` consumes every one: all **four** belief-revision
variants from `docs/contracts/retraction-events.contract.md` — `retraction`
(with its additive `retracts` selector), `supersession`, `correction`, and
`relation_withdrawal` — are now on the `ResearchEvent` boundary, and each
fixture is wired as a regression test
(`crates/hari-core/tests/revision_replay.rs`) pinned against the LWW baseline.

Doctrine under all three: **evidence-recompute is authoritative** — retraction
appends a tombstone, current belief is recomputed from surviving evidence, and
withdrawn evidence is preserved for audit (never deleted). Since the merge-weight
slice the survivor recompute routes through `hari_lattice::merge` with a
**single-source corroboration cap**: a lone uncorroborated surviving `True`/`False`
downgrades to `Probable`/`Doubtful`; two or more independent sources on that side
license the strong value.

> **Uniform-cap note (merge-weight slice).** The design doc originally expected
> fixture 1 to dissolve to `True` (§8.1) and fixture 2 to downgrade to `Probable`
> (§8.2). But after their retractions **both fixtures have the identical survivor
> set — one source asserting `True`** — so no pure evidence-recompute can give them
> different values. The design was internally inconsistent on this point. Per the
> corroboration rule being *uniform across fixtures*, both now dissolve/downgrade to
> **`Probable`**. This is recorded as an addendum in the design doc §8; the
> load-bearing A/B property (belief *survives / the C dissolves*, vs the baseline's
> erase-to-`Unknown`) holds unchanged.

## `retraction_dissolves_derived_contradiction.json`

Two sources disagree, then one withdraws.

| cycle | event | effect |
|---|---|---|
| 1 | evaluator: `deploy-is-safe` = **True** | belief True |
| 2 | critic: `deploy-is-safe` = **False** | True + False → derived **Contradictory** (Escalate) |
| 3 | critic **retracts** its cycle-2 False | cycle-2 evidence tombstoned; recompute over {evaluator: True} → **Probable**; the derived `C` **dissolves** |

**Expected (current belief, replay-to-HEAD):** `Probable` — the derived
contradiction dissolves (no standing conflict, no `Escalate`), and the lone
surviving `True` is capped to `Probable` by the single-source corroboration rule
(see the uniform-cap note above; the design's original `True` is superseded).
**Expected (historical, replay-to-cycle-2):** `Contradictory` — the conflict
that once stood is fully recoverable; the retracted False stays in the trace
flagged `retracted`.
**Baseline (LWW-to-Unknown):** would reset the belief to `Unknown` on
retraction and would **not** dissolve the derived `C` correctly — wrong on
both the value and the derivation. The A/B win is the dissolution, unchanged by
the `True`-vs-`Probable` strength.

## `partial_retraction_downgrades.json`

Corroborated belief loses one of its two supports.

| cycle | event | effect |
|---|---|---|
| 1 | evaluator: `model-v3-beats-baseline` = **True** | one support |
| 2 | runner: `model-v3-beats-baseline` = **True** | two independent sources → **True** (corroborated) |
| 3 | evaluator **retracts** its cycle-1 support | one support remains |

**Expected (current belief, as implemented):** the belief **survives on the
remaining source** and is **downgraded** — recompute over `{runner: True}` →
**`Probable`**. It is **not** reset to `Unknown`. This is the design's
originally-intended §8.2 semantic, delivered by the merge-weight slice's
single-source corroboration cap: two independent sources made it `True`, one
surviving source caps it at `Probable`.
**Baseline (LWW-to-Unknown):** resets to `Unknown`, erasing a belief that
still has standing evidence — this is the row the baseline gets wrong.

> **Implemented via the merge-weight slice.** The survivor recompute routes
> through `hari_lattice::merge` with a corroboration cap counted over *distinct
> sources* (weights are uniform `1.0` at the boundary — real per-source weights
> are #14, earned not declared). A single surviving source no longer licenses
> `True`, delivering the `True → Probable` downgrade. The A/B win (survives, not
> erased) is asserted by `retraction_fidelity_beats_lww_baseline`; the downgrade
> itself by `partial_retraction_downgrades_to_probable`. Note fixture 1 lands on
> the *same* `Probable` from its own single surviving source — see the uniform-cap
> note at the top.

## `supersession_chain.json`

A claim evolves through a chain; only the head is live. Mirrors the Demerzel
`behavioral-test-coverage` rot (`docs/research/2026-07-20-demerzel-belief-replay.md`
§1), where a claim asserting "14 personas" stayed True for four months while
the framework grew — supersession is the retirement mechanism that would have
caught it.

| cycle | event | effect |
|---|---|---|
| 1 | `persona-count-is-14` = **True** (as of 2026-03-17) | live |
| 2 | **supersede** `…-14` by `…-20` | `…-14` retired (frozen, no downstream mass) |
| 3 | `persona-count-is-20` = **True** (as of 2026-03-23) | new live head |
| 4 | **supersede** `…-20` by `…-27` | `…-20` retired |
| 5 | `persona-count-is-27` = **True** (as of 2026-07-20) | current live head |

**Expected (current belief):** only `persona-count-is-27` is live; `…-14` and
`…-20` are retired but fully inspectable — the chain `14 → 20 → 27` is the
audit trail.
**Expected (historical, replay-to-cycle-1):** `persona-count-is-14` reads
`True` as it stood — supersession never rewrites the past.
**Baseline (LWW):** a bare re-assert would leave the stale `…-14` claim
standing as an independent live belief (the exact four-month rot), with no
recorded link between the successive claims.

## `correction_replaces_claim.json`

A source corrects its own mislabeled result in one atomic event.

| cycle | event | effect |
|---|---|---|
| 1 | ix-runner: `model-v3-latency-ok` = **False** | reported p99 = 920ms, over budget |
| 2 | ix-runner **corrects** its cycle-1 result → **True** | original mislabeled p50 as p99; re-measured p99 = 420ms, within budget |

**Expected (current belief):** the correction tombstones the mislabeled `False`
and merges the replacement `True`, so the belief recomputes over the survivor
set `{ix-runner: True}` → **`Probable`** (the same single-source cap fixtures 1
& 2 land on). The report carries **one** revision delta whose `cause =
correction` — the causal link between the withdrawn evidence and its
replacement, which a bare `belief_update` cannot express and which
distinguishes a correction from a plain retraction.
**Expected (historical, replay-to-cycle-1):** `False` as it stood — the
correction never rewrites the past; the tombstoned original stays in the trace.
**Baseline (a plain `belief_update` to True):** would overwrite the value with
no recorded link that the earlier `False` was *wrong* (not merely superseded by
fresh evidence), losing the correction provenance a #14 reliability grader needs.

## `relation_withdrawal_reverts_derived_belief.json`

A withdrawn relation dissolves the belief it induced — the first non-append-only
relation path.

| cycle | event | effect |
|---|---|---|
| 1 | ix-runner: `benchmark-x-passes` = **True** | base belief (direct evidence) |
| 2 | declare `benchmark-x-passes` –Supports→ `deploy-is-safe` | propagation derives `deploy-is-safe` = **True** |
| 3 | **withdraw** `benchmark-x-passes` –Supports→ `deploy-is-safe` | edge tombstoned; `deploy-is-safe` reverts to **Unknown** on re-propagation |

**Expected (current belief):** `deploy-is-safe` reverts `True → Unknown` — its
only support is gone, so re-propagation over the reduced edge set derives
nothing (evidence-recompute: derivation-only beliefs reset to their `Unknown`
base and re-derive). The base belief `benchmark-x-passes` stays `True`
(untouched — it has direct evidence). The withdrawn edge is **tombstoned, not
deleted**: `is_relation_withdrawn(...)` still reports it, preserving the audit
trail. The revert is reported as a `relation_withdrawal` revision delta.
**Baseline (retract the derived proposition):** the append-only "undo" resets
only the `deploy-is-safe` node to `Unknown` while leaving the inducing edge
live, so the *next* propagation re-derives it right back — the revert does not
stick. Withdrawal beats it (pinned by
`withdrawal_sticks_where_retracting_the_derived_belief_would_not`).
