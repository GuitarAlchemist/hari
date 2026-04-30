# Phase 5 Factory Spec — NLSpec

**Satisfaction Target: 0.90**
**Complexity Class: complicated**

Spec-in / software-out brief for an autonomous build loop driven by `/octo:factory`. The loop runs until `scripts/check-phase5-done.sh` returns 0 OR a kill criterion fires.

## Purpose

Implement Phase 5 of Project Hari (Cognition Integration) as defined in `docs/research/phase5-implementation-plan.md`: wire the `hari-cognition` (Lie-algebra dynamics) crate into the main cognitive loop so that state evolution actually changes decisions, AND add a non-Lie baseline (`RecencyDecay`) so the two paths can be compared on the same scenario. The plan contains the design decisions, the concrete state→decision coupling, the baseline definition, and the step-by-step implementation order. This document is the *driver* for the loop; the *what* lives in the plan.

The roadmap explicitly permits "no measurable benefit" as a legitimate Phase 5 outcome — divergence between Lie and baseline paths is the load-bearing requirement, not Lie superiority.

## Actors

- **Factory loop** — the autonomous driver invoking implementation agents per wave.
- **`hari-core` crate** — primary edit target: `CognitiveLoop`, `cycle()`, `ResearchReplayReport`, `score_actions`, `--compare` flag.
- **`hari-cognition` crate** — adds `SymmetryGroup` constructor helpers; must remain dep-isolated (only `nalgebra`, `serde`, `tracing`, `thiserror`).
- **`hari-lattice` and `hari-swarm` crates** — read-only for this phase; do not modify.
- **`scripts/check-phase5-done.sh`** — the only termination authority. Read-only for the loop.
- **Human reviewer** — gates merge of `cognition_divergence.json` fixture and reads the final results.

## Behaviors

### Behavior 1: PriorityModel enum

Add a `PriorityModel` enum (`Lie | Flat | RecencyDecay`) and a `priority_model: PriorityModel` field on `CognitiveLoop`, defaulting to `Flat`. Add a constructor `with_model`. Wires the type through with no behavior change. Touches `crates/hari-core/src/lib.rs`.

### Behavior 2: score_actions method

Introduce `score_actions(&self, actions: Vec<Action>) -> Vec<(Action, f64)>` on `CognitiveLoop`. `Flat` returns 1.0 for all. Refactor `cycle()` to call it before returning actions. Existing tests pass because order is preserved. Touches `crates/hari-core/src/lib.rs`.

### Behavior 3: RecencyDecay branch

Implement `RecencyDecay` in `score_actions`: `priority(action) = exp(-λ * (current_cycle - perception_cycle))` with `λ = 0.2`. Track perception cycles per action. Add a `θ_wait` field with default 0.1 — actions below threshold get suppressed to `Action::Wait`. Add unit tests. Touches `crates/hari-core/src/lib.rs`.

### Behavior 4: goal_axis helper

Add `goal_axis(&self, key: &str) -> Option<usize>` on `CognitiveState`. Returns `Some(dim)` when the goal exists in `state.goals` (BTreeMap-ordered indexing), else `None` falling back to dim 0. Pure addition. Touches `crates/hari-core/src/lib.rs`.

### Behavior 5: SymmetryGroup constructor helpers

Add three constructor helpers to `SymmetryGroup` in `hari-cognition`:
- `attention_rotation(d, i, j)` — skew-symmetric rotation generator coupling dims i and j
- `belief_scaling(d, weights)` — diagonal generator weighted by HexValue ranks
- `goal_projection(d, target)` — projection generator `e_t e_t^T - I/d`

Unit-test that the rotation generator is skew-symmetric and the scaling generator is diagonal. **Must not add any new deps** to `hari-cognition/Cargo.toml`. Touches `crates/hari-cognition/src/lib.rs` only.

### Behavior 6: Wire init_algebra to seeded generators

Replace the dead block at `crates/hari-core/src/lib.rs:423-436` (the `if let Some(ref mut evo)` that discards coefficients with a `// We can't easily step here` comment) with a real `Evolution::step` call. When `priority_model == Lie`, seed generators via Behavior 5 helpers and step once per cycle using perception-derived Hamiltonian coefficients: `h_i = α * sum_of_perception_strengths_targeting_dimension_i` where strength is `+1` (True/Probable), `-1` (Doubtful/False), `0` (Unknown), `±2` split (Contradictory). Clamp `attention.norm()` to ≤10.0 via renormalization (escape hatch for instability). Touches `crates/hari-core/src/lib.rs`.

### Behavior 7: Lie branch in score_actions

Implement `Lie` branch: `priority(action) = base_priority(action) * (1 + α * proj(attention, action_axis))` with `α = 0.5`. `action_axis = e_dim_of_target_goal_or_proposition`. `Action::Wait` and `Action::Log` get a flat low priority. Same `θ_wait` suppression rule as `RecencyDecay`. Add tests showing the `Lie` branch produces a different action ordering than `Flat` on a synthetic 3-event fixture. Touches `crates/hari-core/src/lib.rs`.

### Behavior 8: ReplayMetrics and ReplayComparison structs

Extend `ResearchReplayReport` (`crates/hari-core/src/lib.rs:212`) with `priority_model`, `metrics: ReplayMetrics`, and `comparison: Option<ReplayComparison>`. Define `ReplayMetrics { contradiction_recovery_cycles, false_acceptance_count, goal_completion_rate, consensus_stability, attention_norm_max, action_counts_by_kind }` and `ReplayComparison { baseline, experimental, action_divergence: Vec<ActionDivergence> }`. Implement metric computation as post-hoc analysis of the outcome list — no loop changes. Touches `crates/hari-core/src/lib.rs`.

### Behavior 9: --compare flag

Add `--compare` flag to the `replay` subcommand in `crates/hari-core/src/main.rs:183`. Runs the same trace through `Lie` and `RecencyDecay` on fresh `CognitiveLoop` instances and emits a `ResearchReplayReport` with `comparison` populated. Default single-model behavior remains intact. Update `parse_trace_accepts_object_form` test to round-trip the new optional fields.

### Behavior 10: cognition_divergence.json fixture

Author `fixtures/ix/cognition_divergence.json` — a 6-event IX-style trace engineered so that `Lie` and `RecencyDecay` disagree on at least one event. Must be a plausible research scenario (mix of Probable/Doubtful/Contradictory belief updates with realistic evidence metadata), NOT a synthetic input crafted only to trigger divergence on one path. Verify with `cargo run -p hari-core -- replay --compare fixtures/ix/cognition_divergence.json`.

### Behavior 11: Integration test

Add `crates/hari-core/tests/phase5_replay.rs`. Asserts:
- Replay of `conflicting_benchmark.json` and `cognition_divergence.json` succeeds in both `Lie` and `RecencyDecay` modes.
- `comparison.action_divergence` is non-empty for `cognition_divergence.json`.
- `comparison.experimental.attention_norm_max < 10.0` and `comparison.baseline.attention_norm_max < 10.0`.
- At least one divergence is between `Investigate` and `Wait` (the load-bearing decision).

## Constraints

### Hard rules (do not violate)

1. The ONLY way to declare done is `scripts/check-phase5-done.sh` returning exit code 0. Do not edit the script. If the script appears wrong, escalate to a human.
2. `crates/hari-cognition/Cargo.toml` must not gain any deps beyond `nalgebra`, `serde`, `tracing`, `thiserror`. Dependency hierarchy from `CLAUDE.md`: `hari-cognition` cannot import from `hari-swarm` or `hari-core`; `hari-lattice` cannot import from any sibling crate.
3. One commit per implementation behavior (11 commits expected). No mega-commits.
4. Action set is shared between baseline and experimental paths — no Lie-only `Action` variants. Reports must be symmetric.
5. Do not delete failing tests to make `cargo test --all` pass — fix the cause.
6. The `cognition_divergence.json` fixture must be plausible; a human reviews it before declaring done.

### Budget caps and kill criteria

The loop MUST stop and escalate to a human if any of these fire:

| Cap | Threshold |
|---|---|
| Max iterations (full check-script attempts) | 30 |
| Max wall-clock | 6 hours |
| Same-error streak | 3 (same exit code, no progress in stderr) |
| Test regression | 1 (a previously-passing unrelated test now fails) |
| Cognition crate dep added | 1 |
| Exit code 13 (no divergence) persistence after redesign | 1 |

### Edge cases and failure modes

The check script's stderr maps directly to recovery actions:

| Exit code | Meaning | Loop action |
|---|---|---|
| 10 | `cargo test --all` failed | Read failure, fix cause, retry. Do not delete tests. |
| 11 | Fixture missing | Author `fixtures/ix/cognition_divergence.json` (Behavior 10). |
| 12 | `--compare` flag missing | Behavior 9 incomplete. |
| 13 | `action_divergence` empty (Lie path is a no-op) | Try α ∈ {1.0, 2.0}; then redesign the Hamiltonian source (e.g., couple to belief gradients). After one redesign rev with no movement, escalate. |
| 14 | Attention norm unbounded (numerical instability) | Freeze `dt = 0.01`, drop `belief_scaling` generator if needed, hard-renormalize attention each cycle. |
| 20 | jq missing | Install jq. Not a code problem. |

## Dependencies

### Preconditions

- `cargo test --all` passes on `main` before launch (verified: passes as of 2026-04-29).
- Toolchain: Rust 1.85+ (per Dockerfile pin).
- `jq` installed (required by `scripts/check-phase5-done.sh` for JSON parsing).
- `git` clean working tree before each behavior's commit.

### Crate dependency graph (must be preserved)

```
hari-lattice  (no internal deps)
    ↑
hari-cognition (nalgebra only)
    ↑
hari-swarm    (lattice + cognition)
    ↑
hari-core     (all three)
```

### External references

- `docs/research/phase5-implementation-plan.md` — design decisions, concrete couplings, risk table, kill criteria. Read in full before starting.
- `docs/research/prior-art-survey.md` — context on what's been tried elsewhere; relevant if escalation triggers a reframe.
- `CLAUDE.md` — repo architecture and gotchas.
- `ROADMAP.md` Phase 3 (baselines & metrics) and Phase 5 (cognition integration).

## Acceptance

### Postconditions

The implementation is complete when ALL of the following hold:

1. `scripts/check-phase5-done.sh` exits 0. This script verifies:
   - `cargo test --all` passes.
   - `fixtures/ix/cognition_divergence.json` exists.
   - `cargo run -p hari-core --release -- replay --compare fixtures/ix/cognition_divergence.json` runs and emits JSON.
   - `comparison.action_divergence` array has length ≥ 1.
   - `comparison.experimental.attention_norm_max < 10.0` AND `comparison.baseline.attention_norm_max < 10.0`.

2. `docs/research/phase5-results.md` is filled in with:
   - The verdict (better / no difference / mixed / worse — all legitimate).
   - Headline metrics table comparing baseline vs experimental.
   - Action divergence list with "which was right" annotations.
   - Numerical behavior notes (final α/dt/D, norm trajectory, whether renormalization fired).
   - Decision: continue / continue-with-caveats / reframe.
   - Open questions raised by the run.

3. Final `--compare` JSON output saved to `docs/research/phase5-output-final.json`.

4. `git log --oneline` shows ~11 commits implementing the behaviors above, one per behavior, with no commit deleting tests or modifying `scripts/check-phase5-done.sh`.

5. `crates/hari-cognition/Cargo.toml` has unchanged dependencies (only nalgebra, serde, tracing, thiserror in `[dependencies]`).

### What "done with no measurable benefit" looks like (legitimate negative result)

The check script returns 0 (divergence exists, norms bounded, tests pass) but the metrics in `phase5-results.md` show baseline winning on contradiction recovery / false acceptance / goal completion. This is a legitimate outcome explicitly permitted by the roadmap. Document it honestly in the results file — do NOT loop trying to make the Lie path win on quality metrics. The check script's contract is divergence and boundedness, not Lie superiority.

### Escalation triggers (loop must stop and call a human)

- Any kill criterion fires (see Constraints / budget caps)
- Lie path is observably *worse* than baseline on contradiction recovery — flag for human review even if the check script returns 0
- Any commit that bypasses the check script
- Any change to `crates/hari-cognition/Cargo.toml` deps
- Exit code 13 persists after one redesign of the Hamiltonian source
