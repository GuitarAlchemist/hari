# Algebraic audit of hex-merge — two theorems, two defects

**Status: complete, results pinned in `crates/hari-lattice/tests/algebra_probe.rs`.**
Executed 2026-07-20 in one sitting; no external data, no cross-repo
changes, no waiting. This is the formal-track answer to "do fundamental
research on Hari" after the empirical track died at the tracer gate
(`2026-07-19-substrate-role-preregistration.md` §11): the substrate's
own algebra was checkable *today*, and its in-module `proof_*` tests
each cover exactly one hand-picked instance. These probes check the
same claims over thousands of seeded-random inputs plus adversarial
constructions.

## 1. Method

Randomized property probes (deterministic xorshift, quantized weights so
equality is exact) plus targeted adversarial constructions, asserting the
claims made in `merge.rs`'s own comments: "reproducible across runs
regardless of input order — load-bearing for CRDT correctness" (step 1),
and content-derived synthesis ids as "the property that restores
associativity" (`synthesis_diagnosis_id`).

## 2. Theorems (pinned, previously unproven at scale)

1. **Permutation invariance holds for well-formed input** — distinct
   dedup keys, 2000 random multisets.
2. **Associativity holds conditional on globally distinct dedup keys** —
   1000 random triples, both groupings, state-carried vs flat. Re-merge
   idempotence likewise (1000 trials).
3. **Escalation is anti-dilutive.** Naive arithmetic says corroboration
   washes out C-mass below the 0.3 escalation threshold after one
   supporting observation. Wrong: each corroborating P conflicts with the
   standing F ((P,F) → 0.8 Belnap weight) and synthesizes *more* C.
   Measured trajectory rises monotonically 0.333 → 0.435 toward the
   0.8/1.8 ≈ 0.444 asymptote. **An escalated contradiction cannot be
   muted by consensus-flooding while the dissent stands.** This is a
   real, previously unstated robustness property — arguably the first
   formally verified claim about what the hexavalent layer *adds*.

## 3. Defect: key-collision order dependence (one defect, two symptoms)

Two observations with the same `(source, diagnosis_id, round, ordinal)`
but different payloads are representable, and first-wins dedup keeps
whichever arrived first. Input order therefore leaks into output —
falsifying the CRDT claim on representable input — and **this is the
sole root cause of the associativity failure**: the unconditional
associativity probe fails (seed 0xC0FFEE, trial 28) because an inner
merge synthesizes C from a payload that loses its dedup battle in the
flat ordering; with globally distinct keys the same probe passes 1000/1000.

Candidate fixes, all epistemics calls rather than refactors: reject as
malformed; deterministic content tie-break; or treat a
self-contradicting source as itself synthesizing C. The pinned
`known_divergence_key_collision_is_order_dependent` test must flip with
whichever fix is chosen.

## 4. Defect: ghost contradiction under staleness (issue #16 in miniature)

A synthesized C is stamped `round = max(parents)`, so it outlives its
older parent's staleness window. A consumer carrying `MergedState`
forward and one recomputing from raw evidence get **different answers to
"is this claim contradictory?"** — carried: yes; recomputed: no — with no
documented rule for which is authoritative. Construction: parents at
rounds 1 and 5, window K=3 at round 5; pinned as
`known_divergence_staleness_ghost_contradiction`.

This is precisely the retraction/withdrawal-semantics question of
issue #16: a derived contradiction has no defined lifecycle when the
evidence beneath it expires. The same question will reappear verbatim
for explicit `Retraction` events.

## 5. Impact honestly stated

Nothing in `hari-core` or `hari-swarm` calls `merge` today — both
defects are latent. But the module's stated purpose is cross-repo
wire-compatible state (Demerzel, `ix-fuzzy`), and the port "matches
`ix-fuzzy::observations` exactly," so **both defects almost certainly
exist in `ix-fuzzy` too** — checking is a one-file port of the probe
file. The moment any consumer carries merged state across staleness
windows or ingests unvalidated observations, these stop being latent.

## 6. What this changes about the research program

The empirical track needs repeated observations with organic
disagreement, which the ecosystem is only starting to collect
(ga chatbot-qa verdicts ledger, dc6f9f7e). The formal track needs
nothing and produced falsifiable results in one afternoon: the domain is
six values, every algebraic claim is decidable at scale, and two of the
module's three documented claims turned out to be conditional or false.
Remaining formal targets, in rough value order: the same audit for
`ix-fuzzy` (parity check), `BeliefNetwork` propagation
(termination/monotonicity), and the SL-correspondence question — whether
hex-merge commutes with Subjective Logic fusion under the natural
discretization, which would sharpen `prior-art-survey.md` §6 Q5 into a
theorem-or-counterexample.
