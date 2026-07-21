# Belief-revision replay fixtures

Three deterministic replay fixtures for the belief-revision design
(`docs/research/belief-revision-and-retraction.md`, issue #16). Each
isolates one semantic and each is an **A/B case** against the naive
last-write-wins baseline (design §7).

**These now replay** (issue #16 retraction tracer slice, commit landing this
slice). `hari-core replay fixtures/revision/<name>.json` consumes all three:
the `retraction` variant's additive `retracts` selector and the new
`supersession` variant are on the `ResearchEvent` boundary, and each fixture
is wired as a regression test (`crates/hari-core/tests/revision_replay.rs`)
pinned against the LWW baseline. The `correction` and `relation_withdrawal`
variants from `docs/contracts/retraction-events.contract.md` are **still
deferred** — no fixture here needs them (design doc §9; tracer-bullet
discipline: implement exactly what the fixtures exercise).

Doctrine under all three: **evidence-recompute is authoritative** — retraction
appends a tombstone, current belief is recomputed from surviving evidence, and
withdrawn evidence is preserved for audit (never deleted).

## `retraction_dissolves_derived_contradiction.json`

Two sources disagree, then one withdraws.

| cycle | event | effect |
|---|---|---|
| 1 | evaluator: `deploy-is-safe` = **True** | belief True |
| 2 | critic: `deploy-is-safe` = **False** | True + False → derived **Contradictory** (Escalate) |
| 3 | critic **retracts** its cycle-2 False | cycle-2 evidence tombstoned; recompute over {True} → **True**; the derived `C` **dissolves** |

**Expected (current belief, replay-to-HEAD):** `True`, no standing
contradiction, no `Escalate`.
**Expected (historical, replay-to-cycle-2):** `Contradictory` — the conflict
that once stood is fully recoverable; the retracted False stays in the trace
flagged `retracted`.
**Baseline (LWW-to-Unknown):** would reset the belief to `Unknown` on
retraction and would **not** dissolve the derived `C` correctly — wrong on
both the value and the derivation.

## `partial_retraction_downgrades.json`

Corroborated belief loses one of its two supports.

| cycle | event | effect |
|---|---|---|
| 1 | evaluator: `model-v3-beats-baseline` = **True** | one support |
| 2 | runner: `model-v3-beats-baseline` = **True** | two independent sources → **True** (corroborated) |
| 3 | evaluator **retracts** its cycle-1 support | one support remains |

**Expected (current belief, as implemented):** the belief **survives on the
remaining source** — recompute over `{runner: True}` → `True`. It is **not**
reset to `Unknown`. This is the load-bearing A/B distinction: survives vs
erased.
**Baseline (LWW-to-Unknown):** resets to `Unknown`, erasing a belief that
still has standing evidence — this is the row the baseline gets wrong.

> **Note on granularity.** The design's original target was a finer-grained
> *downgrade* `True → Probable` (a single-source cap: corroboration by a
> second source is what would license `True`). The `hari-core` boundary has
> no per-source *weight* in this slice — the survivor recompute uses
> `combine_evidence_set`, which has no corroboration cap — so the
> implemented value is `True`, not `Probable`. No pure evidence-recompute can
> give fixture 1 `True` and this fixture `Probable` from identical
> single-source-`True` survivor sets, so the slice follows fixtures 1 & 3 as
> written and defers the corroboration cap to the merge-weight slice (design
> §9 item 2). The A/B win (survives, not erased) holds either way and is
> asserted by `retraction_fidelity_beats_lww_baseline`.

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
