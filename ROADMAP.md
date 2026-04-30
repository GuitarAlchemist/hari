# Hari Roadmap

## Strategic Frame

Hari is a research-state substrate for autoresearch systems. Its job is not to be the researcher, optimizer, or paper reader. Its job is to track uncertain claims, preserve contradictory evidence, coordinate agent beliefs, and recommend what needs more investigation.

The first useful integration target is IX autoresearch:

```text
IX
  -> generates hypotheses
  -> runs experiments
  -> reports observations
  -> asks what to investigate next

Hari
  -> stores research claims as beliefs
  -> combines evidence without collapsing uncertainty
  -> detects contradiction and unstable consensus
  -> recommends investigate / accept / retry / escalate
```

## Guiding Principles

- Keep every milestone testable against a simpler baseline.
- Treat Lie algebra dynamics as an experimental mechanism, not as assumed value.
- Prefer replayable scenarios over live demos.
- Measure whether Hari improves research decisions before expanding scope.
- Keep IX, GA, and Hari responsibilities separate.

## System Roles

### Hari

Hari owns the epistemic state:

- Belief values for research claims.
- Contradiction detection.
- Consensus across agents.
- Goal and investigation prioritization.
- Metrics over belief stability and decision quality.

### IX

IX owns autoresearch execution:

- Hypothesis generation.
- Experiment planning.
- Tool and benchmark execution.
- Result summarization.
- Scenario replay and evaluation.

### GA

GA can provide domain context and scenario material:

- Domain-specific claim templates.
- Realistic evidence streams.
- Expected outcomes for benchmark scenarios.
- Qualitative review of research usefulness.

## Phase 0: Stabilize The Repo

Goal: make the project buildable and honest.

Tasks:

- Fix the current `hari-swarm` build failure by initializing `Agent::cognitive_state`.
- Decide whether `hari-swarm` is library-only or needs a binary.
- Align Docker Compose with actual binaries.
- Ensure `cargo test --all` passes.
- Keep README status synchronized with actual behavior.

Exit criteria:

- `cargo test --all` passes.
- The default run path works.
- Docker configuration does not reference missing binaries.

## Phase 1: Scenario Runner

Goal: replace hard-coded demos with replayable research scenarios.

Tasks:

- Define a JSON scenario format for cycles, goals, events, agents, and expected outcomes.
- Add a scenario runner to `hari-core`.
- Emit a machine-readable run report.
- Add fixture scenarios for conflicting evidence, noisy benchmarks, and agent disagreement.

Example event:

```json
{
  "cycle": 12,
  "source": "ix-agent-critic",
  "proposition": "prompt-template-a-improves-pass-rate",
  "value": "Probable",
  "evidence": {
    "benchmark": "swe-mini",
    "delta": 0.07,
    "runs": 5
  }
}
```

Exit criteria:

- A 50-cycle scenario can be replayed deterministically.
- The run report includes final beliefs, actions, consensus, and metrics.

## Phase 2: Research Claim API

Goal: let IX send research observations into Hari.

Tasks:

- Define typed events: `BeliefUpdate`, `ExperimentResult`, `AgentVote`, `Retraction`, `GoalUpdate`.
- Map experiment results into hexavalent values.
- Preserve evidence metadata for later audit.
- Return action recommendations to IX: `Investigate`, `Retry`, `Accept`, `Escalate`, `Wait`.

Exit criteria:

- IX can submit a research trace to Hari.
- Hari can return recommended follow-up actions.
- Evidence provenance is retained in reports.

## Phase 3: Baselines And Metrics

Goal: make Hari scientifically comparable.

Baselines:

- Flat confidence score baseline.
- Lattice-only baseline.
- Lattice plus swarm consensus baseline.
- Experimental lattice plus swarm plus cognition path.

Metrics:

- Contradiction detection rate.
- Contradiction recovery time.
- False acceptance rate.
- False escalation rate.
- Consensus stability under noisy evidence.
- Goal completion rate.
- Action usefulness against expected scenario outcomes.
- Cognitive state boundedness.

Exit criteria:

- The same scenario can run in baseline and experimental modes.
- The report shows whether Hari improved any metric and where it regressed.

## Phase 4: Trust-Weighted Swarm — **partial (opt-in, off by default)**

Goal: make agent roles operational.

Status: an opt-in `TrustModel::{Equal, RoleWeighted}` enum lives on `hari-swarm`, with `Equal` as the default to preserve current behavior bit-for-bit. Calling the new `Swarm::consensus_with(p, RoleWeighted)` weights each vote by the voter's `self_trust` and runs through the new `compute_consensus_weighted` (regression-pinned to match the unweighted version when weights are uniform). Calling the new `Swarm::process_all_with(RoleWeighted)` filters incoming belief messages whose recipient's `message_trust` is below the constant `MESSAGE_TRUST_THRESHOLD = 0.5`; filtered messages are surfaced via `InboxStats::filtered` for the minority-report metric. Six tests cover the new paths; 128 → 134 tests overall, all green.

Delivered (in `hari-swarm`):
- `TrustModel::{Equal, RoleWeighted}` (default `Equal`).
- `compute_consensus_weighted(votes, weights)` with the uniform-weights regression invariant pinned by `weighted_consensus_matches_unweighted_when_uniform`.
- `Swarm::consensus_with` / `Swarm::process_all_with` and `Agent::process_inbox_with` returning `InboxStats { applied, filtered }`. Pre-existing `consensus()`, `process_all()`, `process_inbox()` keep their signatures and behavior — `Equal` is what they delegate to.
- `MESSAGE_TRUST_THRESHOLD = 0.5` pinned by `message_trust_threshold_is_pinned`.

Delivered (bridge into `hari-core`):
- `SessionConfig` gains `use_swarm_consensus: bool` (default `false`), `trust_model: TrustModel` (default `Equal`), `initial_agents: Vec<InitialAgent>` (default empty). All three serde-default so existing JSON traces still parse.
- `CognitiveLoop` gains `swarm: Swarm`, `trust_model: TrustModel`, `use_swarm_consensus: bool` with zero-behavior-change defaults.
- `process_research_event` for `AgentVote` always records the vote into the swarm (auto-creating an agent with neutral role if the source isn't pre-declared); when `use_swarm_consensus` is on, the *perceived* value for the cognitive loop is `swarm.consensus_with(proposition, trust_model).consensus` rather than the raw vote. `BeliefUpdate` and `ExperimentResult` events are unchanged — they remain direct perceptions, not the swarm's responsibility.
- Five new tests in `crates/hari-core/tests/phase7_swarm_bridge.rs` covering: default-mode regression on `swarm_dissent.json`, swarm population even when bridging is off, perceived-value-matches-consensus when bridging is on, `RoleWeighted` produces a different action stream than `Equal` on the same fixture with declared lopsided roles, and auto-creation of undeclared sources.

Exit criteria status:
- ✅ Agent roles change outcomes in measurable ways. The headline swarm-side test `role_weighted_consensus_diverges_from_equal_when_trust_is_lopsided` shows a 1-high-trust + 3-low-trust dissent fixture moving from `Doubtful` (Equal) to `Contradictory` (RoleWeighted). The bridge-side test `role_weighted_changes_outcomes_vs_equal_with_declared_initial_agents` shows the same effect propagating into the cognitive loop's action stream on `swarm_dissent.json`.
- ✅ Reports can explain why one source was trusted more than another. The `InboxStats::filtered` count surfaces dropped low-trust messages; `consensus_with(RoleWeighted)` makes the weighting itself the explanation (it's a one-knob policy, not a black box).
- ✅ Bridge into the IX research-event boundary. `AgentVote` events now actually drive a swarm; `TrustModel` is reachable from the streaming protocol via `SessionConfig.trust_model`.
- ⏸ Track source reliability over repeated scenarios. Still deferred — needs cross-session reliability tracking that the current single-session loop doesn't own. Not blocked technically; just out of scope for this slice.
- ⏸ Distinguish consensus *strength* from raw agreement on the report side. The current `ConsensusResult.agreement` is intentionally a head count under both models (pinned by `agreement_ratio_remains_a_head_count_under_role_weighted`); a separate `weight_share` field is a small follow-up if it turns out to be useful.

## Phase 5: Cognition Integration — **complete (negative result)**

Goal: make `hari-cognition` affect decisions.

Status: implemented and shipped (commits `3dbbbeb`, `5ecece0`, `feb151e`, `1fa9d73`). The mechanical exit criterion (`scripts/check-phase5-done.sh` exit 0) is satisfied. The substantive research outcome is a **negative result on the project's original hypothesis**: Lie-inspired state evolution does not produce measurable decision-quality improvement over either simple baselines (`Flat`, `RecencyDecay`) or the Subjective Logic prior-art baseline. SL beats Lie on `false_acceptance_count` on 3/6 fixtures, ties on 3, never loses (see `docs/research/phase5-fixture-rollup.md` §7 and `docs/research/phase5-results.md` §6).

Delivered:
- `PriorityModel::{Flat, RecencyDecay, Lie}` action-scoring strategies.
- `SymmetryGroup` constructor helpers (`attention_rotation`, `belief_scaling`, `goal_projection`) and the seeded D+1 generator basis.
- `ReplayMetrics`, `ReplayComparison`, `ActionDivergence` with bug-fixed `contradiction_recovery_cycles`.
- Six fixtures covering distinct scenarios; `replay --compare` for 2-way and `replay --compare3` for 3-way (with SL).
- 122 tests passing across the workspace; defaults pinned by `divergence_test_pins_alpha_and_dt`.

Open: see "Cognition Substrate Choice" below — the project-direction call about whether to keep, demote, reframe, or cut `hari-cognition`.

## Phase 6: IX Autoresearch Loop — **implemented**

Goal: close the loop with IX.

Workflow:

```text
1. IX generates a research hypothesis.
2. IX runs one or more experiments.
3. IX submits results and agent votes to Hari.
4. Hari updates claim beliefs and consensus.
5. Hari recommends the next action.
6. IX uses that recommendation to continue, retry, or escalate.
```

Status: streaming protocol implemented (commit `80eef21`). `hari-core serve` runs a synchronous stdio JSONL session; `replay --session <file>` produces byte-identical reproduction of recorded sessions. Subprocess-level integration coverage (`crates/hari-core/tests/phase6_serve_subprocess.rs`) drives the binary over real stdio across the golden path and the dispatcher error branches (`already_open`, `no_session`, `invalid_json`, EOF mid-session). A stdlib-only Python reference client (`clients/ix_reference/`) demonstrates the protocol from outside the Rust workspace and is what an IX maintainer would copy as a starting point. The streaming layer reuses `process_research_event` verbatim — no parallel cognitive codepath. Design recorded in `docs/research/phase6-design.md`.

Exit criteria status:
- ✅ IX can run an autoresearch trace with and without Hari (via `hari-core serve` or `replay --compare3`).
- ✅ Binary entry point exercised over stdio (the dispatcher in `main.rs::handle_request` is now under test, not just the in-process `StreamingSession`).
- ✅ Reference client exists out-of-tree (Python, stdlib-only) and has been smoke-tested end-to-end on `cognition_divergence.json` for `Flat`, `Lie`, and shadow-comparison modes.
- ⏸ Hari-assisted runs vs. baseline IX runs comparison: requires real IX-side autoresearch to actually drive the protocol against real benchmarks; not done.
- ⏸ Results report suitable for roadmap decisions: not done.

What's NOT yet implemented as part of Phase 6:
- A real IX-side autoresearch loop driving `hari-core serve` end-to-end against actual benchmarks (vs. fixtures). The reference client in `clients/ix_reference/` proves the wire works; producing data that informs the Cognition Substrate Choice still needs IX itself.
- Authenticated / multi-tenant deployment (explicitly out of scope per the design doc).

## Open: Cognition Substrate Choice

The Phase 5 negative result against the SL baseline opens a real project-direction question. Three honest paths:

1. **Reframe `hari-cognition`'s value claim.** If the Lie-algebra machinery has value, it's not in `false_acceptance_count`. Possible alternative axes to instrument: interpretability of the attention trajectory, smooth-state continuity preservation across cycles, structure-constant analysis of which cognitive ops commute. None of these are tested.
2. **Demote Lie to research-mode-only.** Switch the default `PriorityModel` to `RecencyDecay` or — adopting the SL data verdict — switch the default decision engine to `SubjectiveLogic`. Lie remains in the codebase as an experimental knob.
3. **Cut `hari-cognition`.** Reduce maintenance surface. The streaming substrate (Phase 6) plus `RecencyDecay` or SL plus the existing belief network and swarm machinery deliver the project's defensible value claim (typed contradiction-preserving claim layer for autoresearch). The Lie-algebra hypothesis becomes a documented experiment-that-didn't-pay.

**Decision (post-Phase-5)**: path 2 (**demote**) executed. The default `PriorityModel` is now `RecencyDecay` — pinned by `test_priority_model_default_is_recency_decay`. `Lie` stays in the codebase as an opt-in research knob (`PriorityModel::Lie`); `Flat` stays for ablation. The `hari-cognition` crate is **not** cut — its instrumentation could still inform path 1 (reframe) on attributes the data didn't measure (interpretability, continuity, commutativity). Promoting `SubjectiveLogic` to a `PriorityModel` variant remains an open follow-up; it currently runs as a separate pipeline via `process_research_trace_subjective_logic`.

## Near-Term Milestone

**Original** (pre-SL data): Hari can run a 50-cycle JSON research scenario in baseline and experimental modes, produce a metrics report, and show whether Lie-inspired state evolution changes research decisions compared with a simple priority baseline. — *Delivered.*

**Updated**: Hari operates as the epistemic substrate for a real IX autoresearch session over its streaming protocol, producing reproducible recommendations whose quality is measured against a non-Hari-assisted IX baseline. Requires the Phase 6 IX-side client work above.

## Open Questions

- What exact confidence thresholds should map experiment outcomes to hexavalent values?
- Should contradictory evidence decay, persist forever, or require explicit resolution?
- Should consensus optimize for correctness, caution, or investigation value?
- Which IX research tasks are most suitable for first evaluation?
- What role should GA play: scenario generator, domain oracle, or external evaluator?
