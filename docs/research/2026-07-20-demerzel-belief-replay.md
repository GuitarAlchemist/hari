# Tracer bullet: Demerzel's beliefs replayed through hari's substrate

**Status: complete, end-to-end, read-only on Demerzel.** First slice of
"hari as Demerzel's live belief substrate" — the step before any talk of
autonomy: Demerzel's `state/beliefs/*.belief.json` are hand-maintained
snapshots (proposition, truth_value, confidence, supporting/contradicting
evidence with per-source reliability), structurally hari-shaped but dead.
This projects them into a `ResearchTrace` and lets the substrate — not a
hand edit — compute what the recorded evidence supports.

Pipeline (every layer, one command each):

```text
Demerzel state/beliefs/*.belief.json
  → scripts/demerzel/beliefs_to_trace.py          (projection, v0 mapping)
  → fixtures/demerzel/beliefs_2026-07-20.json     (9 events, 5 propositions)
  → hari-core replay                              (RecencyDecay default)
  → ResearchReplayReport                          (verdicts + actions)
```

## 1. Result: 3 agree, 1 real disagreement, 1 mapping artifact

| belief | Demerzel (hand) | hari (substrate) | reading |
|---|---|---|---|
| framework-integrity | T @ 0.98 | True | agree |
| consumer-repo-integration | T @ 0.85 | True | agree |
| remediation-effectiveness | P @ 0.86 | Probable | agree exactly |
| confidence-calibration | P @ 0.92 | True | v0 mapping artifact (§3) |
| **behavioral-test-coverage** | **T @ 0.8** | **Contradictory** + `Escalate` | **real disagreement** |

The headline: `behavioral-test-coverage` has carried **T @ 0.8 since
2026-03-17 while holding its own contradicting evidence** — the
2026-03-23 belief-refresh recorded that the framework expanded beyond the
proposition's scope (13 Streeling departments, 7 ga agents vs the "14
original personas" the claim asserts), marked it `contradicting`… and the
hand-maintained verdict stayed True for four months. Hari's replay emits
`Escalate("has contradictory evidence", confidence 0.5)` at exactly the
cycle the refresh evidence lands. A live substrate would have flagged
this on 2026-03-23. That is the entire argument for wiring, in one row.

Also visible from the projection's stderr: **9 of Demerzel's 14 beliefs
are evidence-free placeholders** (`U @ 0.0`, empty evidence arrays —
slot-health and visual-quality checks). The substrate can't audit what
has no evidence; those files are aspirations, not beliefs. A governance
layer's belief corpus being 64% empty shells is itself a finding about
where "next level" effort goes: feeding evidence in, not adding
machinery.

## 2. Why this direction (and not the reverse)

Temporal order is REAL in this data — evidence items carry timestamps and
later evidence genuinely revises earlier state — which makes hari's
pairwise `combine_evidence` the correct operator (the propagation audit's
distinction: pairwise for ordered streams, n-ary sets for simultaneous
contributions). The projection sorts events by evidence timestamp, so
replay is deterministic and re-runnable as Demerzel's files evolve.

## 3. v0 mapping, its one artifact, and the fix path

`supporting` → True (reliability ≥ 0.9) / Probable; `contradicting` →
False (≥ 0.9) / Doubtful. Crude, and the same critique the SL reward
mapping received applies. It produced exactly one artifact:
`confidence-calibration` (hand: P @ 0.92) maps its single rel-0.92
supporting item to True. A mapping that caps single-source claims at
Probable — reserving True for corroboration by a second source — would
match hari's own epistemic instincts and remove the artifact. Deferred
until a second real disagreement motivates tuning; tuning a mapping to
match the hand labels would defeat the point of checking them.

## 4. Next slices (in order, each one small)

1. **CI check, Demerzel-side:** run the projection + replay in Demerzel's
   pipeline; a `Contradictory` verdict on a belief whose file still says
   T/P fails the check or files a tribunal item. That single wire makes
   the hand-maintained corpus un-rottable.
2. **Write-back contract:** a `substrate_verdict` field (or sidecar) per
   belief file, updated by replay, never by hand — contract-first per the
   ecosystem's doctrine (spec in Demerzel, like hex-merge v1.1).
3. **Evidence for the 9 empty beliefs:** slot-health and visual-quality
   already have producers elsewhere in the ecosystem (build results, ga
   quality snapshots); projecting those into evidence items is the same
   pattern as the ga verdict ledger.
4. **Mapping v1** per §3, only when motivated.

## 5. Scope honesty

One-shot replay of 9 events, RecencyDecay default, no cross-repo writes,
no Demerzel governance hooks. This demonstrates the wire is short and the
substrate disagrees productively with the hand corpus; it does not make
Demerzel autonomous, and per `2026-07-20-hexmerge-sl-correspondence.md`
§3 the escalation signal it relies on is still mutable by
abstention-flooding — that design hole should close before any autonomy
is gated on these verdicts.
