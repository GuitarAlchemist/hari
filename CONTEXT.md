# hari — Context

Project Hari is an experimental Rust research sandbox for belief-state reasoning,
cognitive-state dynamics, and multi-agent consensus under uncertainty. It is a
four-crate Cargo workspace (`hari-lattice`, `hari-cognition`, `hari-swarm`,
`hari-core`) with a typed `ResearchEvent` boundary to external autoresearch
systems (IX).

## Domain language

Grown lazily (`/grill-with-docs`); terms below are settled usage. Decisions with
rationale live in `docs/adr/`.

- **Hexavalent logic / `HexValue`** — six truth values `True | Probable |
  Unknown | Doubtful | False | Contradictory`. The chain `F < D < U < P < T` is
  an ordered lattice; **Contradictory sits outside the chain** as an absorbing
  fixed point. Irreconcilable evidence is preserved, never collapsed — the
  epistemic-humility invariant.
- **Priority model** — the pluggable action-ranking policy in `hari-core`:
  `Flat` (ablation), `RecencyDecay` (**default**, see ADR-0001), `Lie`
  (opt-in research knob), `SubjectiveLogic` (data-best non-Lie option;
  short-circuits to its own Opinion-fusion pipeline).
- **Research event boundary** — `ResearchEvent` / `ResearchTrace` /
  `ResearchReplayReport`: the serde-tagged contract between Hari and IX.
  Recommended actions returned to IX: `Investigate / Retry / Accept /
  Escalate / Wait`.
- **False acceptance** — accepting a claim that later evidence contradicts;
  the primary quality metric on fixture replays (lower is better).
- **A/B doctrine** — every new behavior must be comparable against a simpler
  baseline in the same run (`pooled` rows, `Flat` model, pre-bridge modes).
  Ecosystem law; negative results are kept and published in `docs/research/`.
- **Substrate decision** — the Phase-5 owner call that demoted Lie from
  default after it lost to Subjective Logic (ADR-0001); also the subject of
  the self-referential demo scenario (`run_substrate_decision_demo`).
- **Entrenchment ordering** — source reliability treated as AGM entrenchment
  (issue #14): surfaced in reports, never auto-applied to trust.
- **Trust model** — swarm consensus weighting: `Equal` (default) vs
  `RoleWeighted` (Phase 4, opt-in via `SessionConfig`); role trust profiles
  `explorer / critic / integrator / guardian`.
- **Session-memoryless discipline** — sessions carry no memory; durable state
  lives in `state/` ledgers (digests, forecasts, harness) and git. See
  `docs/research/2026-07-21-loop-engineering.md` §2.
- **Distinction discovery** — the operation proposed when a proposition reaches
  `Contradictory`: find a condition under which the disagreeing sources are each
  *right*, dissolving the contradiction rather than resolving it. Contrast with
  fusion (Subjective Logic), which averages the conflict into a probability.
  The claim being tested is that irreconcilability is **generative** — a signal
  that a proposition is underspecified, not merely that evidence is bad.
- **Splitting distinction / separating key** — an evidence key whose values are
  constant across the sources asserting one polarity and constant-but-different
  across those asserting the other. The proposed dissolution of a
  `Contradictory` proposition. Verified, never merely suggested: a candidate is
  only proposed if the split it implies actually makes both source sets
  internally consistent.
- **Competing candidates** — the other separating keys found alongside the
  proposed one. Always reported, because recovering the true distinction from a
  field of one is not the same achievement as recovering it from a field of
  twelve, and an unreported field lets the first read as the second.
- **Naive separator** — the A/B baseline for distinction discovery: propose any
  evidence key whose value differs between some positive and some negative
  source, with no within-side constancy requirement and no verification. The
  simpler thing that could work, and the decisive comparator; an LLM proposer is
  a secondary, reported never decisive, because it cannot run deterministically.
- **Exact-set recovery** — the primary metric for distinction discovery: an
  instance scores 1 only when the proposed candidate set is *exactly* the
  injected distinction. Requires no ranking, so no tie-break can decide it, and
  it is degenerate-proof in both directions — proposing everything fails on the
  extras, proposing nothing fails on the miss.
- **Irreducible contradiction** — sources disagree with no key separating them.
  The correct output is the empty set. The should-not-discover half of the
  pairing discipline: without it, "always propose your best guess" is never
  charged.
- **Undissolvable separator** — a key that partitions the source sets but whose
  split leaves a side still holding both polarities. The correct output is again
  empty. Distinguishes a *verified* proposal from a merely separating one, and is
  the case where the naive separator and distinction discovery must diverge.
