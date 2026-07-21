# Bilattice baseline study — does the single-chain conflation cost decisions?

**Status: complete; results pinned in
`crates/hari-lattice/tests/bilattice_baseline.rs` (8 tests, all green).
Reference bilattice lives in that test's inline `mod bilattice` — a
baseline for comparison, NOT a `src/` feature.** Companion to the
2026-07-20 propagation audit, which stated two design gaps
(§4.1 "Unknown erases Doubtful", §4.2 "propagation cannot derive
negative belief") and tied them to "the Belnap two-order literature"
without measuring the cost. This study builds the Belnap baseline those
gaps gesture at and measures the decision cost directly, with an A/B
against `combine_evidence_set` and `BeliefNetwork` propagation.

The headline: **the single chain conflates Belnap's truth and knowledge
orders, and it costs decisions — but ONLY through propagation, never in
a single evidence combination, and the cost is asymmetric.**

## 1. The question, made precise

Belnap's FOUR carries two orders that hex's chain F<D<U<P<T collapses
into one:

- **truth** order `f ≤t {n,b} ≤t t` (how true a thing is), and
- **knowledge** order `n ≤k {f,t} ≤k b` (how much is known),

each with its own meet/join. The canonical evidence-combination operator
is the **knowledge join** `⊕` ( = `≤k` join): it accumulates every
source's assertion, so `t ⊕ f = b` (one source says true, one says false
→ *both*), and `n` ("nothing known") is its identity. Hex's
`combine_evidence`/`combine_evidence_set` instead folds the no-conflict
case with the **truth-order join**, where `U` (≈ `n`) out-ranks `D`
(≈ weak `f`). So `join(D, U) = U`: an "I don't know" contribution
**erases** a standing "probably false". That erasure is exactly audit
§4.1, and it is the seam this study probes.

## 2. The embeddings (both defensible, both probed)

Hex has 6 values, FOUR has 4, so the embedding is a modelling choice.
Two are natural; both are implemented and every probe runs both.

**Embedding A — sign-only, into canonical Belnap FOUR.**
`T,P ↦ t`; `F,D ↦ f`; `U ↦ n`; `C ↦ b`. The weak/strong distinction is
dropped. This tests hex's *sign* structure against the textbook
four-valued bilattice. `⊕` is the FOUR knowledge join; negation is
`¬t=f, ¬f=t, ¬n=n, ¬b=b`.

**Embedding B — graded evidence-pair `(pro, con)`, each in {0,1,2}.**
The standard *product* bilattice (a chain × its opposite): `T=(2,0)`,
`P=(1,0)`, `U=(0,0)`, `D=(0,1)`, `F=(0,2)`, `C=(2,2)`. Knowledge order
is componentwise `≤`, so `⊕` = componentwise `max`; truth order is
`pro↑ con↓`; negation swaps the components. This one **keeps** hex's
6-way resolution (P/D remain distinct from T/F), so it tests the chain
against a bilattice that retains the same lean hex claims to have.

Both embeddings commute with negation
(`embed(not(h)) == neg(embed(h))`, pinned in
`theorem_negation_commutes_with_embedding`), which is what makes the
`Contradicts` edge — it contributes `not(source)` — a fair comparison.

**Decision proxy** (stated once, used throughout): a value *accepts* iff
it is positive with no standing negative — hex `∈ {P,T}`; FOUR `= t`;
pair `pro>0 ∧ con=0`. A value is *conflict* (escalate) iff hex `C` /
FOUR `b` / pair `pro>0 ∧ con>0`. A **decision flip** is any disagreement
on the accept-or-conflict verdict. "Accept" = chain position ≥ P is the
choice the task fixed; everything below is a deliberate proxy for an
IX Accept/Investigate boundary.

The `belnap_weight` table already in `hari-lattice::merge` is a
*weighted-distribution* mechanism (Belnap-extended contradiction
multipliers over a mass distribution) and is orthogonal to this study,
which is about the *ordering* under `combine_evidence_set` and
propagation, not the merge weights.

## 3. Single-combination: the conflation costs information, but NOT a decision

`theorem_single_combination_decision_agrees` (4000 random hex multisets,
sizes 1–8): hex's `combine_evidence_set` and **both** embeddings agree
on the accept verdict AND the conflict verdict on **every** input.

The reason is structural, not statistical. Hex reports conflict iff the
set holds `C` or holds both a positive and a negative; `⊕` reports
`both` under exactly that same condition. Hex accepts iff the set holds
a positive and no negative and no `C`; that is exactly when `⊕` yields a
pure positive. So at the level of one combination, **the single chain
loses no decision.**

What it *does* lose is retained information, and two known divergences
pin it:

| probe | input | hex | FOUR (A) | pair (B) | decision flip? |
|---|---|---|---|---|---|
| `known_divergence_unknown_erases_doubtful` | {D, U} | **U** | False | (0,1) | no (both non-accept) |
| `known_divergence_hex_join_dilutes_negatives` | {F, D} | **D** | — | (0,2) | no (both non-accept) |

The first is audit §4.1 made executable: an Unknown launders a standing
Doubtful off the chain, while `⊕` keeps the negative (`n ⊕ f = f`, or
`(0,1)` in the graded pair). The second is the same optimism the other
way up: two negatives fold to hex `Doubtful` — the *weaker* of the two,
because truth-join takes the higher chain rank — where `⊕` accumulates
to strong `False` `(0,2)`. Hex is **systematically optimistic on the
negative side**: piling on doubt can only make a belief *less* false.
Neither flips a single-combination decision — both results are
non-accept — but the erased value is what flips a decision one round
later, in propagation. `info-loss` rate (hex collapsed to `U` while the
pair still carried con): **137 / 4000** random multisets.

## 4. Propagation: the conflation DOES cost decisions

Propagation is where the erased information bites. The study runs each
random graph through hex `BeliefNetwork::propagate_until_stable` and
through a generic `BelnapNet<V>` mirror that uses the SAME edge rules
(Supports = pass source, Contradicts = negate source, Implies = gated on
a true-ish antecedent) but combines contributions with `⊕` instead of
`combine_evidence_set`. The mirror terminates on every graph
(`theorem_belnap_mirror_terminates`; `⊕` is monotone-increasing and
bounded, negation preserves the knowledge order).

### 4a. A guaranteed, realizable decision flip

`known_divergence_propagation_decision_flip_erased_doubt` constructs a
5-node graph (all `Supports`/driver edges, no withdrawal needed) where
hex reaches **Accept** and both bilattices reach **Conflict** on the
same node:

- `b` starts Unknown (the target);
- `x` starts Doubtful and Supports `b` (a standing negative);
- `u0` starts Unknown and Supports `x` (launders `x`'s OWN doubt);
- `y` starts Unknown and Supports `b`;
- `t0` starts True and Supports `y` (turns `y` positive a round later).

Round 1 (hex): `x` = `combine{D, U}` = **U** — its own doubt is laundered
off before it can reach `b`; `y` = `combine{U, T}` = **T**; `b` sees
`x`'s *pre-update* D against only U's and stays U. Round 2: `b` =
`combine{U, U(x), T(y)}` = **T** → **ACCEPT**. The negative `x` carried
never coexisted with the positive in `b`'s view, because hex erased it
from `x` first.

The bilattice keeps `x`'s con (`n ⊕ f = f`; `(0,1)` stays), so when
`y`'s positive lands, `b` = **both/conflict** → **ESCALATE**. Same
graph, opposite decision. This is the chain conflation costing a
decision on an input class that is entirely realizable: *a claim that
receives doubtful evidence whose source is later independently
reassured, and separately receives positive evidence.* Hex forgets the
doubt; a bilattice remembers it and escalates.

### 4b. How often, and in which direction

`known_divergence_propagation_decision_flips_both_directions`
(4000 random graphs per embedding, deterministic seed):

| direction | FOUR (A) | pair (B) | meaning |
|---|---|---|---|
| hex accepts, bilattice withholds | 44 | 52 | hex over-accepts |
| **hex clean, bilattice escalates** | **118** | **132** | **hex misses a conflict** |
| bilattice accepts, hex withholds | 6 | 6 | hex over-conflicts (reverse) |

The dominant divergence is **missed escalation**: ~3% of random graphs
have a node where hex reports a clean verdict and the bilattice flags a
contradiction hex's chain destroyed. This is the substrate's stated
epistemic-humility doctrine ("irreconcilable evidence must be
preserved") leaking: the single chain silently reconciles evidence the
two-order structure keeps in tension.

### 4c. A falsified conjecture (the honest bit)

I first conjectured the divergence was **one-directional** — hex only
ever *optimistic*, the chain able to lose negative/conflict information
but never invent it. The randomized probe **falsified it** (seed
`0x0D15EA5E`): 6 nodes per embedding reach `Contradictory` in hex while
the bilattice keeps them pure-positive `True` (the "reverse" row above).

Mechanism: hex's `C` is reached by **eager per-round** conflict
detection and is both **absorbing and contagious** (`join(x,C)=C`,
`not(C)=C`). In a cyclic subgraph a single manufactured `C` floods along
every reachable Supports/Contradicts edge, permanently. Where an
upstream sign-divergence (hex laundered a negative that the bilattice
kept, flipping the sign a `Contradicts` edge then negates) means the
bilattice never accumulated `both` at the origin, the bilattice's
downstream node stays acceptable while hex's is flooded with `C`. So hex
is **usually optimistic, occasionally pessimistic via C-contagion** —
and the two effects do not cancel; missed-escalation dominates
the reverse ~20:1. This is the third hand-analysis in this formal track
that a probe caught wrong (after the anti-dilution misprediction and the
"transient divergence" conjecture in the two prior audits) — logged
here rather than buried because the failure mode (analytic optimism
about your own operator) is the point.

## 5. Verdict and recommendation

**Does the chain conflation cost decisions in practice? Yes — but only
in propagation, and asymmetrically.**

- **Single evidence combination: no cost.** `combine_evidence_set`
  agrees with both Belnap embeddings on every accept/conflict decision
  over 4000 trials. The conflation costs *retained negative
  information*, not the verdict. If hex only ever combined evidence once,
  the single chain would be decision-equivalent to a bilattice.
- **Propagation: real cost, dominated by missed escalations.** ~3% of
  random graphs contain a node where hex returns a clean verdict that a
  Belnap bilattice escalates to conflict, plus a guaranteed constructed
  witness where hex *accepts* what a bilattice escalates. The cost is
  driven entirely by the §4.1/§4.2 gaps operating across rounds: an
  Unknown launders a standing Doubtful (§4.1), and `Contradicts` cannot
  derive negative belief (§4.2, pinned here as
  `known_divergence_contradicts_cannot_derive_negative`), so negatives
  evaporate between rounds and later positives read clean.

**Recommendation: add a knowledge-order combination operator; do NOT
adopt a full bilattice, and do NOT change the default yet.** The A/B
evidence points at a targeted fix, not a substrate rewrite:

1. The decision cost lives entirely in *propagation's cross-round
   combination*, and entirely in the direction of **lost negatives**.
   The graded pair embedding (B) shows a minimal-surface fix: track
   `(pro, con)` as the propagation-round accumulator and combine with
   `⊕` (componentwise max) instead of truth-join, collapsing back to a
   hex value only for display/decision. That kills the dominant
   missed-escalation class (a `Doubtful` survives an `Unknown`; a
   `Contradicts` edge can lower a belief) while leaving
   `combine_evidence_set`'s single-combination behavior — already
   decision-correct — untouched.
2. This is A/B-able exactly as the roadmap requires: the current
   truth-join propagation is the simpler baseline, the `⊕`-accumulating
   propagation is the challenger, and
   `known_divergence_propagation_decision_flips_both_directions` is the
   scoreboard. It should be built as a tracer-bullet slice on the
   `hari-lattice` propagation path with the flip counts as the
   acceptance metric — NOT wired into `hari-core` defaults until the
   negative result is reproduced against a real IX trace corpus.
3. Full Belnap adoption is **not** warranted by this data: the reverse
   (hex-pessimistic C-contagion) divergence is real but rare (6/4000),
   and a full two-order rewrite would touch every consumer of `HexValue`
   for a ~3% propagation-decision delta on random graphs whose relevance
   to real IX traces is unmeasured. The honest scope is "give
   propagation a knowledge-order accumulator," then re-measure on
   fixtures.

Every claim above is executable: `cargo test -p hari-lattice --test
bilattice_baseline`. The `theorem_*` tests pin what holds on all trials;
the `known_divergence_*` tests pin current divergent behavior under the
must-flip-if-fixed convention, so if any of these gaps is closed, the
corresponding test breaks loudly and forces this document to be updated.

## 6. Test inventory

Theorems (equivalences that hold on every trial):

- `theorem_single_combination_decision_agrees` — hex ≡ both embeddings
  on accept/conflict verdict, 4000 multisets (§3).
- `theorem_negation_commutes_with_embedding` — `embed∘not = neg∘embed`,
  both embeddings, all 6 values (§2).
- `theorem_belnap_mirror_terminates` — the `⊕`/negation propagation
  mirror converges on every graph, 2000 graphs × 2 embeddings (§4).

Known divergences (current behavior pinned, must-flip-if-fixed):

- `known_divergence_unknown_erases_doubtful` — {D,U} → hex U, bilattice
  keeps the negative (audit §4.1).
- `known_divergence_hex_join_dilutes_negatives` — {F,D} → hex D (weaker),
  `⊕` accumulates to strong F.
- `known_divergence_propagation_decision_flip_erased_doubt` — constructed
  5-node graph: hex Accept vs bilattice Conflict (§4a).
- `known_divergence_contradicts_cannot_derive_negative` — audit §4.2 seen
  from the bilattice: hex leaves target Unknown, `⊕` derives False.
- `known_divergence_propagation_decision_flips_both_directions` — 4000
  graphs/embedding: bidirectional, missed-escalation dominant (§4b, §4c).
