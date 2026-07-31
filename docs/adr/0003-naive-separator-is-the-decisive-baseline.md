# ADR-0003: The naive separator is the decisive baseline; an LLM proposer is secondary

Date: 2026-07-30

## Status

Proposed. Taken during `/grill-with-docs`, before any code exists.

## Context

The idea arrived phrased as *"must beat a plain-LLM-brainstorm baseline"*, and
that is the comparison most readers will care about: if you can get the same
answer by handing an LLM the evidence maps, the substrate has earned nothing.
A future reader will look at a pre-registration whose decisive comparator is a
one-line mechanical rule and ask why the obvious question isn't the one being
answered.

The A/B doctrine's actual requirement is narrower and, here, harder: every new
behaviour must be comparable against a **simpler baseline in the same run**. The
simpler baseline for distinction discovery is not an LLM — it is the dumbest
mechanical rule that could work.

This matters more than usual right now. #35 had just concluded that the shipped
substrate is indistinguishable from naive acceptance on both of its
pre-registered metrics, precisely because a naive comparator was measured. Not
measuring one here would be repeating that mistake by omission.

## Decision

- **Decisive comparator: the naive separator** — propose any evidence key whose
  value differs between some positive and some negative source, with no
  within-side constancy requirement and no verification of whether the implied
  split actually dissolves the contradiction.
- **An LLM proposer is a secondary**, reported and never decisive.

## Why not the LLM

Three properties, each disqualifying on its own for a *decisive* role:

1. **Non-determinism.** The repo's 359 tests are deterministic replay. A decisive
   comparator that returns different answers on reruns cannot produce a fixed
   corpus result, and the pre-registration would be committing to a number that
   does not exist.
2. **Prompt wording is an unregistered researcher degree of freedom.** The score
   moves with the phrasing, and nothing in §10 constrains phrasing. That is a
   p-hacking surface with no guard on it.
3. **It cannot run in CI**, so the comparison would decay the moment nobody was
   paying for it.

The LLM comparison is still worth running — it answers "do you need a substrate
at all", which is the more interesting question. It is kept as a secondary
exactly as §5.2 keeps its secondaries: reported, never substituted for the
primary in a conclusion.

## Why the naive baseline is not a strawman

It is genuinely competitive. With three sources per side, `runs` differs between
some positive and some negative source almost always, so the naive rule fires
and proposes it. The constancy requirement is what rejects it. On the
undissolvable-separator instances the naive rule proposes a key that separates
but does not dissolve, and verification is what rejects that.

So the two rules diverge on cases that are constructed to matter, and the naive
one is not obviously wrong in advance — which is the shape a baseline should
have. A baseline that cannot possibly win is not evidence.

## Consequences

- The headline claim becomes a **precision** claim: distinction discovery
  proposes only distinctions it has verified dissolve the contradiction. That is
  a checkable property, not a judgement.
- If distinction discovery does not beat the naive separator, the honest
  conclusion is that the constancy test and the verification step earn nothing,
  and the negative result is published — the same disposition ADR-0001 records
  for the Lie hypothesis.
- Two comparators are deliberately **not** both decisive. §5.1 already rejected
  that shape when it demoted four "primary" metrics to one over the family-wise
  error rate; two decisive comparators is the same problem at smaller scale, and
  it would let an LLM having a bad day kill a real mechanical result.
