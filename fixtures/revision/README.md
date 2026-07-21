# Belief-revision replay fixtures (target-behavior)

Three deterministic replay fixtures for the belief-revision design
(`docs/research/belief-revision-and-retraction.md`, issue #16). Each
isolates one semantic and each is an **A/B case** against the naive
last-write-wins baseline (design §7).

**These are target-behavior fixtures.** They use the **proposed** wire
payloads (`retraction` with a `retracts` selector, and `supersession`) that
`hari-core replay` does **not yet consume** — the `retracts` field and the
`supersession` / `correction` / `relation_withdrawal` variants are not on the
`ResearchEvent` boundary yet (see the design doc §9 and
`docs/contracts/retraction-events.contract.md`). Replaying them today either
ignores the `retracts` selector (`retraction` degrades to today's
whole-proposition reset) or fails to deserialize (`supersession`). They are
the **acceptance targets** for the implementation slice, not currently-green
regressions. When that slice lands, wire these as regression tests pinned
against the LWW baseline.

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

**Expected (current belief):** the belief **survives on the remaining single
source, downgraded True → Probable** (single-source cap; corroboration by a
second source is what licensed `True`). It is **not** reset to `Unknown`.
**Baseline (LWW-to-Unknown):** resets to `Unknown`, erasing a belief that
still has standing evidence — this is the row the baseline gets wrong.

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
