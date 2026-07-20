# Algebraic audit of BeliefNetwork propagation — four theorems, one fixed defect, two design gaps

**Status: complete; the defect is FIXED in the same change; results
pinned in `crates/hari-lattice/tests/propagation_probe.rs`.** Companion
to `2026-07-20-hex-merge-algebraic-audit.md`, same method: the
in-module tests check single hand-picked instances; these probes check
the structural claims over thousands of seeded-random graphs.

Unlike `merge`, propagation is **live**: `hari-core` replay calls
`propagate_until_stable_with_provenance(10)` on every research event,
so defects here were reachable, not latent.

## 1. Theorems (pinned)

1. **Termination is unaided.** Propagation converges on every graph —
   cycles included — without the `max_iterations` cap, within
   `5·N + 2` rounds (1500 random graphs). Argument: each round a
   node's value either climbs the F<D<U<P<T chain or jumps to the
   absorbing `C`; both orders are well-founded.
2. **Per-node monotonicity + C absorption.** Values never move down
   the chain, and a node that reaches `Contradictory` stays there
   (1000 graphs, every round checked).
3. **Provenance path ≡ blind path.** `propagate_with_provenance`
   computes identical values and change counts to `propagate`, with
   `derivations.len() == changed_count` every round (1000 graphs).
4. **Edge-order independence — fixpoint AND per-round** (1000 graphs
   × 3 permutations). This one is post-fix; see §2.

## 2. The defect: edge insertion order chose the final belief

The per-round combination was a pairwise fold of
`combine_evidence(acc, next)` over incoming edges, in **edge insertion
order**. `combine_evidence` is commutative but **not associative**:
conflict detection sees only two values at a time, and
`join(D, U) = U` erases a standing negative before a later positive
arrives.

Probe counterexample (seed 0xF17ED, trial 6, reduced): a node starting
`Doubtful` with simultaneous contributions `{U, T}`:

```text
(D ⊕ U) ⊕ T  =  U ⊕ T  =  True          — doubt laundered through U
(D ⊕ T) ⊕ U  =  C                        — conflict detected
```

Both are fixpoints. **Same evidence, permanently different final
beliefs — True vs Contradictory — selected by the order edges were
added to the graph.** For a substrate whose doctrine is deterministic
replay and preserved contradictions, this is the strongest defect
found in either audit: in one order the contradiction is detected, in
the other it is silently destroyed. The provenance "audit chain"
differed too, so even the justification of a belief depended on a
construction artifact.

Methodological note: I first conjectured the divergence was transient
(fixpoints reconverge). The randomized probe falsified the conjecture
within 6 trials. That is the second wrong hand-analysis today caught
by a probe (after the anti-dilution misprediction in the merge audit).

## 3. The fix: n-ary set combination

`HexLattice::combine_evidence_set` inspects the whole contribution set
(node's current value included) before combining:

- any `Contradictory` member → `Contradictory` (absorbing);
- any positive (T/P) **and** any negative (F/D) member → conflict →
  `Contradictory`, immune to arrival order;
- otherwise → `join` over all members (ACI, so grouping cannot matter).

Both propagation paths now use it. The pairwise `combine_evidence`
remains public with a documented non-associativity caveat — it is the
right operator where argument order carries real temporal meaning,
which is exactly how `hari-core`'s `belief_update` path and
`hari-swarm` use it (an event stream has an order; a set of
simultaneous edge contributions does not).

**Blast radius: zero.** All 24 workspace test suites pass unchanged —
no pinned replay scenario exercised a mixed-sign multi-edge fold,
which is itself a datum about test-corpus thinness (consistent with
every single-instance `proof_*` finding today).

## 4. Design gaps — documented, deliberately NOT fixed (epistemics calls)

1. **Unknown erases Doubtful.** `join(D, U) = U`: in the single chain
   F<D<U<P<T, "no information" out-ranks standing negative belief, so
   an Unknown contribution lifts a Doubtful node. Belnap's four-valued
   logic keeps the *truth* order and the *knowledge* order separate
   precisely to prevent this conflation; hari's single chain merges
   them. Revisiting that is a substrate-design question, not a bug fix.
2. **Propagation cannot derive negative belief.** A `Contradicts` edge
   from a True source contributes `F`, but against a `U`/`D`/`F`
   target there is no positive-vs-negative conflict and join discards
   it — pinned in `known_gap_contradicts_cannot_lower_below_unknown`.
   "A is true and A contradicts B" leaves an Unknown B Unknown
   forever; falsity can only be asserted directly, never derived.
   Whether propagation should have meet-down semantics for negative
   evidence is an owner decision; the current design only lifts or
   escalates to `C`.
3. (Noted, unprobed) `add_proposition` with an existing label inserts
   a second node and re-points the index, orphaning the first —
   API-level hazard, unreachable through `hari-core`'s guarded paths.

## 5. Relation to the research program

Together with the hex-merge audit, the formal track has now produced,
in one day: seven pinned theorems, four fixed defects (three merge, one
propagation), two falsified hand-analyses, and two sharply-stated open
design questions (§4.1, §4.2) that connect hari's single-chain design
to the Belnap two-order literature. All from a six-value domain, no
external data, no cross-repo dependencies.
