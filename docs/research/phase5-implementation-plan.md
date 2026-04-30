# Phase 5 Implementation Plan: Cognition Integration

## Preconditions (Phase 0 checks)

Before starting Phase 5, verify these don't block work:
- `cargo test --all` passes locally. `CLAUDE.md` calls out a `hari-swarm` build issue (uninitialized `Agent::cognitive_state`) and `docker-compose.yml` references a missing `hari-swarm` binary. Phase 5 does not require docker, and the existing tests in `crates/hari-core/src/lib.rs` show the swarm is at least loadable, so if `cargo test --all` is green, proceed without fixing those. If it fails, fix the `Agent::cognitive_state` initialization first — Phase 5 will not be runnable otherwise.
- The `evolution` field in `CognitiveLoop` (`crates/hari-core/src/lib.rs:350`) is `Option<Evolution>` and never `Some` in production paths. The dead code at lines 423–436 (`if let Some(ref mut evo) = self.evolution { ... }`) currently does nothing — it computes coefficients then discards them with a `// We can't easily step here` comment. Phase 5's first job is to delete that placeholder.

## 1. Design decisions to nail down

| Question | Recommendation | Alternative | Rationale |
|---|---|---|---|
| State dimension | **D = 4** (keep current default at `crates/hari-core/src/lib.rs:207`) | D = 8 to allow more goals/clusters | D = 4 is enough for the 3–4 goals/clusters in the existing fixture and keeps matrix-exp cheap. Bump only if scenarios outgrow it. |
| Mapping propositions/goals → dimensions | **Goal-axis mapping**: each goal in `state.goals` (BTreeMap-ordered) gets the next free dimension index. Propositions inherit the dimension of their associated goal (matched by key prefix or explicit `goal_key` field on `Goal`). Propositions with no goal go to a shared "background" dimension 0. | Hash-based bucketing (proposition name → dim mod D) | Goal-axis mapping is interpretable and stable across replays. Hashing makes test assertions noisy. |
| Generators to seed | **Three**: (a) skew-symmetric "attention rotation" coupling dim 0 ↔ active-goal dim, (b) diagonal "belief-confidence scaling" weighted by `HexValue` rank, (c) projection generator `e_i e_i^T - I/D` toward the top goal's axis | Add commutators of (a)(b)(c) as a 4th generator to test non-commuting coupling | Three is the smallest set that exercises rotation, scaling, and projection — the three things `crates/hari-cognition/src/lib.rs` already proves it can do. The roadmap explicitly named these three at Phase 5 task 1. |
| What "action priority" means | **Scalar priority score per `Action` candidate**, sorted descending; `cycle()` keeps the top-K (initially K = all, but actions below a threshold are dropped to `Action::Wait`) | Strict priority queue with preemption | Score per action lets us A/B against the baseline using identical action sets. A queue would force changing the `Action` enum's downstream interpretation. |
| Coupling location | Hook into the `// --- THINK ---` block of `cycle()` (`crates/hari-core/src/lib.rs:415`), AFTER belief propagation, BEFORE goal evaluation | Wrap action selection externally in `process_research_event` | Hooking into `cycle()` lets both replay and the scripted demo benefit. Wrapping externally would skip the demo's 10-cycle path. |
| Where the baseline lives | A new `PriorityModel` enum (`Lie` / `Flat` / `RecencyDecay`) on `CognitiveLoop`, defaulting to `Flat` for backward compat | A second struct `BaselineLoop` that duplicates `cycle()` | Single loop with a strategy enum keeps the action set identical and the diff small. Two loops invite drift. |

## 2. Concrete state→decision coupling (pick one)

**The coupling**: After belief propagation each cycle, compute a Hamiltonian whose coefficients come from current perceptions: `h_i = α * sum_of_perception_strengths_targeting_dimension_i`, where strength is `+1` for True/Probable, `-1` for Doubtful/False, `0` for Unknown, `±2` for Contradictory split across both signs. Step `Evolution::step` once per cycle. Then for each candidate action:

```
priority(action) = base_priority(action) * (1 + α * proj(attention, action_axis))
```

where `action_axis = e_dim_of_target_goal_or_proposition` and `α = 0.5` (tunable). `Action::Wait` and `Action::Log` always get a flat low priority. Actions are returned in descending priority order; if the top action has priority < `θ_wait` (default 0.1) the loop emits `Action::Wait` instead.

**Why this one**: it is a one-line change to the existing action vector, it uses `attention` literally as written in `CognitiveState` (`crates/hari-core/src/lib.rs:258`), and it makes the `Investigate` vs `Wait` decision the observable A/B point — which is exactly what the conflicting_benchmark fixture exercises (Probable → Doubtful → Retraction in 3 events).

## 3. The non-Lie baseline

`PriorityModel::Flat`: every produced action gets priority 1.0, returned in production order (current behavior). Actions never get suppressed to `Wait`.

`PriorityModel::RecencyDecay` (the comparison baseline): `priority(action) = exp(-λ * (current_cycle - perception_cycle))` with `λ = 0.2`. No matrix math, no attention vector. Same `θ_wait` threshold, same action set, same suppression rule. This is the head-to-head competitor — it captures "recent perceptions matter more" without any algebraic structure.

Both baselines must produce the exact same `Action` variants as the Lie path so `ResearchReplayReport` is symmetric.

## 4. Metrics & report shape

Extend `ResearchReplayReport` (`crates/hari-core/src/lib.rs:212`):

```rust
pub struct ResearchReplayReport {
    // existing fields unchanged
    pub priority_model: PriorityModel,
    pub metrics: ReplayMetrics,
    pub comparison: Option<ReplayComparison>, // Some when --compare flag used
}

pub struct ReplayMetrics {
    pub contradiction_recovery_cycles: Option<u64>, // cycles between Contradictory and next non-Contradictory
    pub false_acceptance_count: u32,                // Accept emitted on a value later retracted
    pub goal_completion_rate: f64,
    pub consensus_stability: f64,                   // 1 - (flips / events) per touched proposition
    pub attention_norm_max: f64,                    // boundedness check
    pub action_counts_by_kind: BTreeMap<String, u32>,
}

pub struct ReplayComparison {
    pub baseline: ReplayMetrics,
    pub experimental: ReplayMetrics,
    pub action_divergence: Vec<ActionDivergence>, // events where the two paths chose differently
}

pub struct ActionDivergence {
    pub event_index: usize,
    pub baseline_actions: Vec<Action>,
    pub experimental_actions: Vec<Action>,
}
```

Add a `--compare` flag to the `replay` subcommand in `crates/hari-core/src/main.rs:183` that runs the same trace through `Lie` and `RecencyDecay` and emits both with a divergence list. Keep the default single-model behavior intact.

## 5. Step-by-step implementation order

Each step is independently committable. Crate listed first.

1. **(hari-core)** Add `PriorityModel` enum and a `priority_model: PriorityModel` field on `CognitiveLoop` defaulting to `Flat`. No behavior change yet — wires the enum through. Add a constructor `with_model`.
2. **(hari-core)** Introduce a private `score_actions(&self, actions: Vec<Action>) -> Vec<(Action, f64)>` method. `Flat` returns 1.0 for all. Refactor `cycle()` to call it before returning. Tests still pass because order is preserved.
3. **(hari-core)** Implement `RecencyDecay` in `score_actions`. Track perception cycle per action by attaching the source cycle to a parallel `last_perception_cycles` vec inside `cycle()`. Add a `θ_wait` field with default 0.1. Add unit tests asserting `RecencyDecay` suppresses old-perception actions to `Wait`.
4. **(hari-core)** Define the goal→dimension mapping helper `goal_axis(&self, key: &str) -> usize` on `CognitiveState`. Return `Some(dim)` when the goal exists in `goals`, else `None` falling back to dim 0. Pure addition, unit-tested in isolation.
5. **(hari-cognition)** Add three constructor helpers to `SymmetryGroup`: `attention_rotation(d, i, j)`, `belief_scaling(d, weights)`, `goal_projection(d, target)`. They return `DMatrix<f64>` so `hari-cognition` keeps zero downstream deps. Unit-test that the rotation generator is skew-symmetric and the scaling generator is diagonal. **Risk-free for circular deps** — these are functions on existing types.
6. **(hari-core)** Wire `init_algebra` to the seeded generators when `priority_model == Lie`. Replace the dead block at `lib.rs:423-436` with a real `Evolution::step` call using perception-derived coefficients (section 2 above). Assert `state_norm` boundedness — clamp via renormalization if it exceeds 10.0 (escape hatch).
7. **(hari-core)** Implement `Lie` branch in `score_actions`: scalar priority = `(1 + α * proj)` as in section 2. Add tests showing the Lie branch produces a different action ordering than `Flat` on a synthetic 3-event fixture.
8. **(hari-core)** Add `ReplayMetrics` and `ReplayComparison` structs. Implement metric computation by walking the outcome list — pure post-hoc analysis, no loop changes.
9. **(hari-core)** Add `--compare` to `main.rs replay` flow: run trace through both models on fresh `CognitiveLoop` instances, populate `comparison`. Update `parse_trace_accepts_object_form` test to round-trip the new optional fields (they're `Option`, so old fixtures still load).
10. **(fixtures)** Add `fixtures/ix/cognition_divergence.json` — a 6-event scenario engineered (by inspection) to make Lie and baseline disagree at least once. Verify with `cargo run -- replay --compare`.
11. **(hari-core, integration test)** New `tests/phase5_replay.rs` that runs `conflicting_benchmark.json` through both models, asserts divergence list is non-empty for `cognition_divergence.json`, and asserts `attention_norm_max < 10.0`.

No step pulls `hari-swarm` or `hari-core` into `hari-cognition`. The dependency hierarchy in `CLAUDE.md` is preserved.

## 6. Test strategy

- **Unit (hari-cognition)**: each generator helper produces a matrix with the expected algebraic property (skew-symmetric, diagonal, idempotent-ish projection).
- **Unit (hari-core)**: `score_actions` for each `PriorityModel` on hand-built action lists; `goal_axis` mapping is stable across `BTreeMap` ordering.
- **Property (hari-core)**: random sequences of perceptions never push `attention.norm()` above 10.0 (boundedness invariant) over 200 cycles.
- **Integration**: replay `conflicting_benchmark.json` and `cognition_divergence.json` with `--compare`, assert that:
  - `RecencyDecay` and `Lie` agree on the `Retraction` event (both should `Retry`).
  - On `cognition_divergence.json`, `action_divergence` is non-empty and at least one divergence is between `Investigate` and `Wait` (the load-bearing decision).
- **No flaky tests**: matrix-exp Taylor series is deterministic at fixed `dt`, so divergence assertions can be exact.

## 7. Risks & escape hatches

| Risk | Symptom | Kill criterion |
|---|---|---|
| Numerical instability in `exp` | `attention` blows up after many cycles | If `attention_norm_max > 10.0` even with clamping in any seeded fixture, freeze `dt` at 0.01 and reduce α. If still unstable after one rev, drop the `belief_scaling` (non-orthogonal) generator. |
| Unbounded attention norm | Same as above, slow drift | Hard renormalize to unit norm each cycle. If renormalization changes divergence outcomes vs unrenormalized in `cognition_divergence.json`, the coupling is too sensitive — kill the Lie path. |
| Baseline trivially better | `RecencyDecay` wins on contradiction recovery and false acceptance for both fixtures | Document it in the report and stop investing in the Lie path. Phase 5 is allowed to conclude "no measurable benefit." Roadmap explicitly permits this outcome. |
| No-op coupling | `action_divergence` is empty across fixtures | α is too small or the goal-axis mapping is degenerate. Try α ∈ {1.0, 2.0}. If still no divergence after one rev, the coupling design is wrong — escalate to redesigning the Hamiltonian source (e.g., couple to belief gradients instead of perception strengths). |
| Cognition crate dragged into swarm/core types | Compile error in `hari-cognition` after step 5 | Strict rule: only `nalgebra` and `std` may be imported in that crate. Enforce in `hari-cognition/Cargo.toml` review. |

The exit criterion from the roadmap becomes operational: a green `cargo test --all` plus a `--compare` report on `cognition_divergence.json` showing at least one `ActionDivergence` is sufficient to declare the phase done. Demonstrating *useful* divergence (recovery time, false-accept rate) is a stretch goal that may legitimately fail.

## Critical Files for Implementation

- `crates/hari-core/src/lib.rs` (CognitiveLoop, cycle, ResearchReplayReport — most edits land here)
- `crates/hari-core/src/main.rs` (replay subcommand, `--compare` flag)
- `crates/hari-cognition/src/lib.rs` (generator constructor helpers; do not add deps)
- `fixtures/ix/conflicting_benchmark.json` (existing reference scenario; add a sibling `cognition_divergence.json`)
- `ROADMAP.md` (update Phase 5 status when exit criterion is met)
