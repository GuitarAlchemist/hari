# Hex-merge vs Subjective Logic fusion — the correspondence fails, three ways

**Status: complete; all results pinned in
`crates/hari-core/tests/sl_correspondence_probe.rs`.**
Executed 2026-07-20, immediately following the hex-merge algebraic audit
(`2026-07-20-hex-merge-algebraic-audit.md`), whose §6 named this as the
next formal target: sharpen `prior-art-survey.md` §6 Q5 — *is the
hexavalent layer formally "SL in a costume"?* — into a
theorem-or-counterexample. **Answer: counterexample, three independent
ways.** The hexavalent merge is not a discretization of Subjective Logic
fusion, and the deviation is *not* explained by the Contradictory
channel — the two operators belong to different algebraic families
before C ever enters the picture.

## 1. Question and method

Test whether the square commutes:

```
observations ──merge_all──────────▶ HexDistribution
     │                                   │
  from_hex ∘ discounted            affine extension of from_hex
     ▼                                   ▼
opinions ──fold cumulative_fuse──▶ Opinion  ≟  Opinion
```

Both legs are the codebase's **own committed artifacts** — nothing was
invented for this study:

- **Hex side**: `hari_lattice::merge::merge_all` (post-audit-fix: pure
  function of the base-evidence multiset).
- **SL side**: `hari_core::Opinion` — `from_hex` is the committed
  `HexValue → (b, d, u)` table (T = (0.85, 0.05, 0.10), P = (0.55,
  0.15, 0.30), U = (0.05, 0.05, 0.90), D/F mirror P/T, C = (0.45,
  0.45, 0.10)); `cumulative_fuse` is the sole fusion operator;
  `discounted(w)` is the only weight-application mechanism (used at
  0.5 for agent votes).
- The only constructed piece is the right leg's distribution → opinion
  map, and it is forced: a `HexDistribution` is a convex combination of
  the six variants, so the unique affine extension of `from_hex` is
  `(b,d,u) = Σ_v mass(v)·from_hex(v)`.

Method as in `algebra_probe.rs`: deterministic xorshift, quantized
weights, 2000-trial randomized sweeps, exact structural facts pinned as
`theorem_*`, measured behavior pinned as `probe_*` with recorded bounds.
Scope: single claim key, distinct sources (dedup and meta-conflict
layers audited separately), base rate fixed at 0.5.

## 2. Where the square commutes (exactly)

Two places, both pinned:

1. **Full-weight singletons** (`theorem_singleton_commutes_iff_full_
   weight`): one observation at weight 1.0 maps to the same opinion
   along both paths, < 1e-12, all six variants.
2. **Direct T-vs-F conflict, at the decision level** (`theorem_direct_
   conflict_escalates_on_both_sides`): hex synthesizes C-mass exactly
   1/3 > 0.3 → escalation; SL fuses to b = d = 9/19 ≈ 0.4737 > 0.4
   both → conflict branch → Escalate. On the sharpest single input the
   two systems agree about what to *do*.

That is the entire commuting locus found. Everything else deviates.

## 3. Three formal separations

### 3.1 Idempotence (structural — mapping-independent)

`theorem_idempotence_separates_merge_from_cumulative_fusion`. The hex
merge is idempotent on its evidence multiset — a CRDT proof obligation
(`proof_idempotence` in merge.rs). SL cumulative fusion is
**deliberately not idempotent**: fusing an opinion with itself hardens
it (embedded True: b 0.85 → 0.17/0.19 ≈ 0.8947, u 0.10 → 0.0526).
No injective state mapping can make an idempotent operator commute with
a non-idempotent one, so the negative answer to Q5 does not depend on
the choice of embedding table. This is the deepest of the three
separations: **G-Set CRDT semantics and cumulative evidence fusion are
mutually exclusive by construction.** (The SL family member closest to
hex-merge's behavior would be *averaging* fusion, which the codebase
does not implement; even that lacks the C channel — §3.2.)

Corollary of the same fact (`theorem_agreeing_corroboration_diverges_
without_any_contradiction`): with all sources agreeing — no
contradiction anywhere — hex-merge is blind to corroboration (n
agreeing T's normalize back to 100 % T; merge-then-map is constant at
(0.85, 0.05, 0.10)) while SL hardens monotonically (b_n = 17n/(18n+2),
gap ≈ 0.086 at n = 12 and growing; for all-Unknown chains SL erodes
u = 9/(9+n) below 0.5 at n = 10 while hex holds u = 0.90). The
hexavalent distribution is an **averaging** statistic; cumulative
fusion is an **accumulating** one.

### 3.2 The C channel is anti-dilutive; SL conflict is not (and mirror)

`theorem_consensus_flooding_separates_the_two`, the sharpest
*behavioral* separation. Standing T-vs-F conflict, flood with n
corroborating full-weight P's from fresh sources:

- **Hex**: each P itself conflicts with the standing F ((P,F) → 0.8
  Belnap weight), synthesizing more C. C-mass rises monotonically from
  1/3 toward 0.8/1.8 ≈ 0.444; escalation never muted (the audit's
  anti-dilution theorem, re-confirmed here).
- **SL**: each P adds belief evidence. Conflict flag (d > 0.4) clears
  at **n = 4**; belief crosses the Accept threshold at **n = 22**
  (exact in evidence space: b = (18 + 11n/3)/(38 + 11n/3 + n) > 0.7
  iff n > 21.5). At n = 22 SL **accepts the flooded proposition**
  while hex holds C ≈ 0.437, still escalated, dissent unresolved.

`theorem_unknown_flooding_mirrors_the_separation` — the mirror, found
by stray cells in the confusion matrix (§4): flood the same conflict
with *Unknowns* instead. U synthesizes nothing but carries full
normalization mass, so hex C-mass = 1/(3+n): **one** Unknown mutes the
escalation flag (1/4 < 0.3). SL's embedded Unknown is near-vacuous
evidence ((r,s) = (1/9, 1/9)): fused b = d → 0.5 from below and the
conflict flag never clears.

So (as measured, spec v1.1): hex escalation was anti-dilutive against
*support* but fully dilutable by *abstentions*; SL conflict is the
exact mirror. Neither contradiction detector simulates the other —
they disagree about what counts as washing out a conflict. This mirror
was not a design goal stated anywhere in either module; the probe
surfaced it.

> **Superseded 2026-07-20 by spec v1.2 (issue #28).** The
> abstention-muting half of this mirror was judged a design defect,
> not a finding to preserve: an evidence-based alarm dilutable by
> non-evidence is an exploitable hole once autonomy gates on
> escalation. Ratified fix: escalation compares C against the
> *informative* mass (U excluded from the denominator; all-U never
> escalates). Hex is now immune to both flooding directions while SL
> conflict remains support-mortal — the separation *sharpened* rather
> than closed. `theorem_unknown_flooding_mirrors_the_separation`
> flipped accordingly, and the §4 confusion matrix moved to
> 180/742/1076/2 (agreement 46.1%): the SL-only stray cells that
> revealed this mirror collapsed 10 → 2, because they *were* the
> mirror. Landed: Demerzel `7e6a1cb` (spec v1.2 + fixtures 11/12),
> hari `5d32abb`/`97a3f1f`, ix `76b9c71`. The v1.1 numbers elsewhere
> in this doc are kept as the historical record of what motivated the
> change.

### 3.3 Weight semantics: relative vs absolute

`theorem_singleton_commutes_iff_full_weight`, second half: a
half-weight True singleton already deviates by 0.45 — hex-merge
normalizes the weight away (0.5/0.5 → pure T), SL's `discounted`
treats weight as absolute evidence strength ((0.425, 0.025, 0.55)).
Hex weights are *relative to the multiset*; SL discounts are
*absolute*. A third independent deviation channel, active even on
one-observation inputs.

## 4. Quantitative deviation at scale

`probe_map_then_fuse_vs_merge_then_map_randomized`, 2000 random sets
(2–8 distinct-source observations, all variants, quantized weights,
seed 0x51C0DE), L∞ over (b, d, u):

| stratum              | n    | mean dev | max dev |
|----------------------|------|----------|---------|
| all                  | 2000 | 0.167    | 0.686   |
| hex synthesized C    | 1360 | 0.163    | 0.605   |
| no C synthesized     | 640  | **0.174**| **0.686**|
| near-commuting (<0.01)| 24  | —        | —       |

The central question — does the deviation concentrate exactly where
hex-merge synthesizes C? — is answered **no**, emphatically: the
contradiction-free stratum deviates slightly *more* on average and
holds the maximum. The C channel is not the obstruction; the
averaging-vs-accumulating mismatch and weight semantics dominate.

`probe_escalation_decision_confusion_matrix` (decision level, seed
0xE5CA1A7E, 2000 trials): agree-escalate 172, agree-quiet 923,
hex-alone 895, SL-alone 10 — **54.8 % agreement, barely above chance**.
Hex escalates alone on ~45 % of inputs (its C synthesis fires on every
opposite-polarity pair, including weak D-vs-P conflicts SL fuses
through); the 10 SL-alone cases are precisely the Unknown-dilution
mirror of §3.2.

## 5. Answer to survey Q5

**The hexavalent layer is not SL in a costume, and "it only adds the C
channel" is also false.** What it formally is, relative to SL:

1. A different algebraic family: an idempotent, multiset-averaging,
   CRDT-mergeable statistic — structurally incompatible with
   cumulative fusion under any faithful mapping (§3.1). What it *buys*
   is exactly what SL fusion cannot have: replay/dedup safety and
   order-independent distributed merge. What it *costs* is
   corroboration-blindness: n agreeing witnesses carry no more force
   than one.
2. On top of that, a contradiction channel with no SL counterpart, and
   the channel's distinguishing formal property is **anti-dilution
   under support** (§3.2) — an escalated conflict cannot be
   consensus-flooded away, where SL de-escalates at n = 4 and accepts
   at n = 22. Its dual weakness — dilution by abstention, where one
   Unknown mutes escalation — is the price of living inside a
   normalized distribution, and SL is strictly better on that face.
3. And a weight semantics (relative, not absolute) that deviates on
   even singleton inputs (§3.3).

The sharpened Q5 answer for the survey: *hex-merge ≠ discretized SL
fusion; the layers agree only on full-weight singletons and on the
verdict for a maximally direct conflict; their contradiction detectors
are incomparable (neither dominates), with an exact support/abstention
mirror between them.*

## 6. Limitations, honestly stated

- Quantitative results (§4, and the specific crossings n = 4 / 22 /
  10) are conditional on the code's own `from_hex` table and default
  thresholds; a different embedding would move the numbers. The
  idempotence separation (§3.1) is mapping-independent; the
  flooding/mirror separations are threshold-parameterized but
  direction-stable (monotone-up vs monotone-down trajectories).
- Comparison is against **cumulative** fusion because that is the only
  operator the codebase implements. Jøsang's *averaging* fusion is
  idempotent and would be the fairer analog for §3.1/§4; it still has
  no C-synthesis channel, so §3.2 stands regardless. Implementing it
  is the obvious follow-up if anyone wants the correspondence question
  re-run against the best-possible SL representative.
- Single-claim scope: the meta-conflict (cross-aspect) channel and the
  dedup/staleness layers are outside this study (the latter audited in
  `2026-07-20-hex-merge-algebraic-audit.md`). Base rate fixed at 0.5.
- Nothing in production consumes both layers today; this is a formal
  result about the substrate, not a bug report. It does bear on any
  future proposal to "simplify" the hexavalent layer down to SL: the
  layers are not interchangeable, and the A/B doctrine now has a
  precise statement of what each does that the other cannot.
