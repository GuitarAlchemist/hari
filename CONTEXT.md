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
