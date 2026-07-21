# ADR-0001: RecencyDecay is the default priority model; Lie demoted to opt-in

Date: 2026-07-21 (records a decision made at Phase-5 close; backfilled from
`docs/research/phase5-results.md` §6 and `phase5-fixture-rollup.md` §7)

## Status

Accepted (owner call). Pinned by `test_priority_model_default_is_recency_decay`;
Lie tunables pinned by `divergence_test_pins_alpha_and_dt`.

## Context

Phase 5 tested the founding hypothesis that Lie-inspired state evolution beats
simpler baselines on IX research-trace replays. Under the A/B doctrine it was
run against a Subjective Logic baseline across six fixtures. SL beat Lie on
`false_acceptance_count` on 3/6 fixtures, tied on 3, and never lost. The
hypothesis **does not survive** the comparison — an honest negative result.

## Decision

- Default `PriorityModel` is **`RecencyDecay`** — simple, explainable, and not
  contradicted by the data.
- `Lie` remains available as an explicit opt-in research knob (not deleted:
  the roadmap values falsifiable re-tests as the event model grows).
- `SubjectiveLogic` remains available and is the data-best non-Lie option, but
  promoting it to default is a separate explicit owner call.
- `Flat` is kept for ablation.

## Consequences

- Changing any default requires explicit owner approval; the pinning tests make
  drive-by retuning impossible without a visible diff.
- New behaviors must ship A/B-comparable against a simpler baseline (roadmap
  requirement reaffirmed by this episode).
- The negative result stays published in `docs/research/` — deleting failed
  hypotheses would break the compounding channel future sessions learn from.
