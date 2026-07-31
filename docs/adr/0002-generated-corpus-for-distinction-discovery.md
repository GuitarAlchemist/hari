# ADR-0002: The distinction-discovery corpus is generated, not authored

Date: 2026-07-30

## Status

Proposed. Records a design decision taken during `/grill-with-docs` on the
"teach Hari to be creative" idea, before any code exists.

## Context

Four days earlier, §9.4 of `docs/research/2026-07-28-ix-eval-preregistration.md`
adopted a **standing rule**: no §6 dual-rule test and no §8 keep/kill verdict may
ever be computed on an authored fixture. The reasoning was that hand-authored
traces are not draws from a population — the variance a bootstrap would find is a
property of whoever wrote the generator, so the confidence interval is a design
artifact. That rule was adopted after measuring a corpus of 54 authored pairs
that collapsed to **three** effective decision situations.

A reader who encounters a *generated* corpus for distinction discovery
immediately after that rule will reasonably suspect it was quietly abandoned.
It was not, and the reason is specific enough to be worth recording, because it
is exactly the kind of justification that gets lost and then re-litigated.

## Decision

Distinction-discovery instances are **sampled from a committed generative
distribution** — sources per side, evidence keys per source, number of spurious
separators, value-collision rates — under a seed derived mechanically from the
spec commit, so any regeneration is visible in git history.

## Why this does not violate §9.4

The two cases differ in where ground truth comes from, and that difference is
the whole argument.

For the IX eval, the trace is supposed to represent a **real autoresearch
session**. There is a world it must be faithful to, we cannot sample from that
world, and so an authored trace is a stand-in whose distribution we invented.
§9.4 is correct about that.

For distinction discovery, the ground truth is **injected by construction**:
we choose the hidden distinction, then generate sources consistent with it. There
is no external world the instance approximates — the instance *is* the problem.
The space of configurations is therefore a genuine population, and instances
drawn from it are genuine draws. A trace-clustered bootstrap over them is
legitimate, which it is not anywhere else in this project.

The distinction is not "generated code is better than authored code". It is that
§9.4's objection was about **absent ground truth**, and here the ground truth is
present by construction.

## Considered alternatives

- **Hand-authored fixtures.** Readable and reviewable, and the corpus would be
  smaller. Rejected: it reproduces the §9.4 failure exactly, in the same week it
  was diagnosed.
- **Harvesting real `Contradictory` propositions from recorded IX traces.**
  Maximum ecological validity and no synthetic-task objection. Rejected because
  nobody knows the true distinction for a real contradiction, so the primary
  metric has nothing to score against — it would need a human judge, which is
  the unfalsifiability the whole design exists to avoid.

## Consequences

- The generative distribution's parameters become a **pre-registration artifact**
  and must be committed before generation, per §9.3.1's precedent — and §9.3.2 is
  the cautionary tale for a spec that leaves the load-bearing axis unspecified
  while specifying an inert one.
- The number of spurious separators is a difficulty knob, so a difficulty curve
  comes free rather than needing a second corpus.
- This corpus is **not** covered by §9.4's standing rule. That rule stays in
  force for everything it names; it is not weakened, extended, or reinterpreted
  by this ADR.
- If a future task claims the same exemption, it must show injected ground truth
  in the same sense. "We generated it" is not the qualifying property.
