# Hari Roadmap

## Enhancement Summary (deepened 2026-07-21)

Six parallel research/review agents deepened the OPEN items (completed phases untouched):
calibration prior-art, eval design, cognition-reframe axes, cross-repo conformance,
architecture review, simplicity/YAGNI review. Insights appear as `> Research insights`
blocks under each open section, with citations. Key findings:

1. **Stale item found**: Phase 4's deferred "source reliability over repeated scenarios"
   was shipped by issue #14 — closed below, with the swarm-integration remainder noted.
2. **Live drift found**: hari's `fixtures/hex-merge/` copy has byte-drifted from the
   canonical `Demerzel/fixtures/hex-merge/` corpus (CRLF/LF); ix already consumes the
   canonical corpus via submodule. The "deferred" fixture suite mostly exists — hari is
   the last consumer on a copy.
3. **Architectural liability named**: the SubjectiveLogic short-circuit is a parallel
   substrate, not a priority model — it silently degrades Phase 8 relations and #16
   corrections, and gates the "promote SL to default" option.
4. **Reframe axes adjudicated** (with the matched-capacity control specified):
   continuity/stability KEEP (cheap, falsifiable), interpretability KEEP-conditional
   (probe-AUC vs control on held-out labels), structure-constant analysis CUT.
5. **Eval protocol designed** for the Near-Term Milestone: paired counterfactual replay
   over recorded traces (the existing shadow loop is the engine), should-act/should-abstain
   paired tasks, trace-clustered bootstrap, pre-registered decision rule.
6. **Both open calibration questions** resolve to "empirical claim with a pinned
   falsifier" — concrete defaults proposed under Open Questions.

Agent disagreements are flagged inline as **[flagged]** — they are owner calls, not
settled conclusions.

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

## Phase 0: Stabilize The Repo — **complete**

Goal: make the project buildable and honest.

Status: all exit criteria met.

- ✅ `Agent::cognitive_state` initialization fixed; `hari-swarm` builds clean.
- ✅ Decision: `hari-swarm` is **library-only**. Its capabilities are reachable from `hari-core` via `SessionConfig.{trust_model, use_swarm_consensus, initial_agents}` since the Phase 4 bridge — there's no separate binary or compose service for it.
- ✅ `docker-compose.yml` aligned with actual binaries: single `hari-core` service, no missing-binary references.
- ✅ `cargo test --all` passes (159 tests across 11 suites at the time of writing).
- ✅ The default run path (`cargo run --release -p hari-core`) executes the 10-cycle demo cleanly.

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
- ✅→ **Closed as owned-by-#14** (2026-07-21 deepening): "Track source reliability over repeated scenarios" was superseded by the issue-#14 source-reliability ledger (per-source precision, pooled baseline, entrenchment ordering) — `AgentVote` sources and `ResearchEvent.source` are the same identity space. Building agent-level history inside hari-swarm would duplicate that store and violate its session-scoped, library-only design. The live remainder: future trust calibration should *read* the #14 ledger in hari-core and feed swarm roles through the existing `SessionConfig.initial_agents` seam — surfaced, never auto-applied, awaiting owner review.
- ⏸ Distinguish consensus *strength* from raw agreement on the report side. The current `ConsensusResult.agreement` is intentionally a head count under both models (pinned by `agreement_ratio_remains_a_head_count_under_role_weighted`); a separate `weight_share` field is a small follow-up if it turns out to be useful.

> **Research insights (2026-07-21).** `weight_share`: simplicity review says CUT (no
> consumer; an afternoon's work if a real IX run ever needs it), architecture review says
> harmless-additive-whenever. **[flagged]** Default: don't build until a RoleWeighted
> decision needs explaining in a real report.

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

> **Research insights (2026-07-21) — eval design for the remainders.**
>
> - **The shadow loop is the counterfactual-replay engine.** 2024–2026 practice for
>   A/B-ing decision support inside agent loops is record-once, replay-under-N-policies
>   ([Record & Replay](https://arxiv.org/html/2505.17716v1), [Causal Agent
>   Replay](https://arxiv.org/html/2606.08275v1)). Run baseline and Hari policies as
>   shadows over ONE recorded IX trace — never two live runs (nondeterminism destroys
>   pairing). `replay --compare3` already does this.
> - **Paired tasks give ground truth by construction.** Adopt the
>   [AgentAbstain](https://arxiv.org/html/2607.10059) design: each eval task ships as a
>   should-act variant + a should-abstain variant differing by one injected trigger, so
>   every decision is gradeable and no always-Accept/always-Wait policy clears 50%.
>   Metrics: Act Accuracy, Abstain Accuracy, Paired Accuracy, Conditioned Abstention Rate.
>   This closes the `phase5-results.md` §2 "which action was right?" gap and the
>   Wait-shaming problem (§6: report `false_rejection_count` alongside
>   `false_acceptance_count` — a win by emitting more Waits must show non-inferior
>   false-rejection or it's disqualified).
> - **Two metric bugs to fix before any eval**: (1) `forecast.rs` (J2) already computes
>   per-belief Brier + calibration buckets and *nothing consumes it* — wiring the
>   forecast ledger into the report is the highest-leverage, lowest-cost addition;
>   (2) `consensus_stability` and `goal_completion_rate` read event payloads upstream of
>   the policy layer, so they're tied by construction on all six fixtures — fix the
>   derivation or exclude them from primary comparison.
> - **Statistics for small n**: analyze per-decision (not per-trace: ~20 decisions ×
>   5–10 traces ≈ 100–200 paired decisions), aggregate with a paired bootstrap
>   *clustered by trace* (B=10,000), and use the dual rule: improvement counts only if
>   the 95% CI excludes zero AND p<0.05
>   ([2511.19794](https://www.arxiv.org/pdf/2511.19794)). Pre-register the primary
>   metric, baselines {IX-unassisted, RecencyDecay, SubjectiveLogic}, decision rule, and
>   MDE in a git-committed doc BEFORE running (the §6 fixture-selection critique applies
>   to us too).
> - **First IX task**: flaky-vs-real benchmark discrimination — IX runs a
>   micro-benchmark N times with injected perturbations (some real regressions, some
>   variance); Hari recommends Accept/Wait/Escalate; ground truth is mechanical. Real-data
>   analogue of `slow_evidence` + `heavy_contradiction`, and it slots directly into the
>   paired-task design. Second: contradictory results across configs (release/debug).
>   NOT a good first task: open-ended "is this direction promising".
> - **Kill/keep rule for the milestone**: Hari-assisted must beat IX-unassisted on the
>   pre-registered primary metric under the dual rule, and must not LOSE to
>   `SubjectiveLogic` on calibration — else the honest conclusion is that ~600 lines of
>   SL delivers the benefit (the §7.5 lesson, pointed at ourselves).

## Open: Cognition Substrate Choice

The Phase 5 negative result against the SL baseline opens a real project-direction question. Three honest paths:

1. **Reframe `hari-cognition`'s value claim.** If the Lie-algebra machinery has value, it's not in `false_acceptance_count`. Possible alternative axes to instrument: interpretability of the attention trajectory, smooth-state continuity preservation across cycles, structure-constant analysis of which cognitive ops commute. None of these are tested.
2. **Demote Lie to research-mode-only.** Switch the default `PriorityModel` to `RecencyDecay` or — adopting the SL data verdict — switch the default decision engine to `SubjectiveLogic`. Lie remains in the codebase as an experimental knob.
3. **Cut `hari-cognition`.** Reduce maintenance surface. The streaming substrate (Phase 6) plus `RecencyDecay` or SL plus the existing belief network and swarm machinery deliver the project's defensible value claim (typed contradiction-preserving claim layer for autoresearch). The Lie-algebra hypothesis becomes a documented experiment-that-didn't-pay.

**Decision (post-Phase-5)**: path 2 (**demote**) executed. The default `PriorityModel` is now `RecencyDecay` — pinned by `test_priority_model_default_is_recency_decay`. `Lie` stays in the codebase as an opt-in research knob (`PriorityModel::Lie`); `Flat` stays for ablation. The `hari-cognition` crate is **not** cut — its instrumentation could still inform path 1 (reframe) on attributes the data didn't measure (interpretability, continuity, commutativity).

**Follow-up shipped**: Subjective Logic is now a first-class `PriorityModel` variant — `PriorityModel::SubjectiveLogic`. It still runs through the existing `subjective_logic::process_event` pipeline (Opinion fusion, projected probability + uncertainty thresholds), but `process_research_event` now short-circuits to it when the variant is set, bypassing the action-scoring abstraction (which SL doesn't use), perception integration, swarm bridging, and belief-graph propagation. Reachable from JSON via `SessionConfig.priority_model = "SubjectiveLogic"`. Per-event outcomes are byte-equal to the standalone `process_research_trace_subjective_logic` (regression-pinned by `cognitive_loop_subjective_logic_matches_standalone_sl_pipeline`). The default stays `RecencyDecay`; switching to SL is an explicit owner call like the previous substrate decision.

**Guard condition** (from the abandoned 2026-07-19 substrate-role pre-registration, §8): any future claim that structured dynamics (Lie or successor) beats the SL baseline must *additionally* survive a matched-capacity, matched-tuning learned-dynamics control. Beating SL alone is no longer sufficient evidence for the structured-dynamics hypothesis — an unstructured learner with the same parameter budget must also lose before the claim stands.

> **Research insights (2026-07-21) — the reframe axes, adjudicated.**
>
> Structural fact shaping all verdicts: hari's generators are hand-seeded and FIXED —
> nothing is learned. Nearly all "Lie structure pays off" literature is about *learned*
> generators constraining a hypothesis space hari doesn't have.
>
> - **Axis (b) continuity/stability — KEEP** (cheapest, most falsifiable). Metrics:
>   spectral radius of `exp(Σ hᵢGᵢ·dt)` per cycle (assert ≤ 1+ε); state drift under
>   ε-perturbed evidence; terminal norm over 50+ events. Honest framing:
>   "norm-preserving generators give boundedness with ZERO tuning" — NOT "Lie beats
>   unstructured on stability," because spectrally-capped unstructured maps get the same
>   guarantee ([DeepKoopFormer](https://www.nature.com/articles/s44387-026-00085-3),
>   [LyaNet](https://proceedings.mlr.press/v162/rodriguez22a/rodriguez22a.pdf)). Expect a
>   tie on raw drift vs a capped control; report both.
> - **Axis (a) interpretability — KEEP only conditionally.** Hand-named axes "aligning"
>   with the goals they were seeded for is circular. The non-circular test: linear-probe
>   AUC / mutual information from the attention vector to *held-out* labeled event
>   classes, vs the control's latent trajectory on the same labels
>   ([TRACE](https://arxiv.org/pdf/2607.06184)). If a plain learned linear map probes
>   equally well, cut this axis too — that result is worth having.
> - **Axis (c) structure-constant/commutativity analysis — CUT.** Commutator analysis is
>   actionable only for controllability decisions (robotics) or as a closure regularizer
>   when *learning* generators ([2309.07860](https://arxiv.org/pdf/2309.07860)). hari has
>   neither: `structure_constants()` returns a compute-once constant nothing branches on.
>   Keep the function + antisymmetry test as a correctness artifact; drop it as a value
>   claim. Revive only if the project pivots to learning generators from replay traces.
> - **The matched-capacity control, concretely**: `ψ ← exp(A(perception)·dt)·ψ` with A a
>   free D×D matrix (16 dof at D=4) driven by the same perception inputs — same state
>   dim, same integrator, no structure. Since hari's 5 fixed generators are the *smaller*
>   model, any hari win is unambiguously structure, not capacity (satisfies the harder
>   ablation direction for free; per
>   [symmetry–data exchange-rate methodology](https://arxiv.org/pdf/2606.01090)). Add a
>   spectrally-capped variant of A for the axis-(b) comparison.
> - **[flagged]** Simplicity review goes further: freeze hari-cognition entirely (no
>   reframe instrumentation) — the crate stays because cutting it breaks
>   `PriorityModel::Lie` and the reproducibility of the flagship negative result (592 of
>   ~19,200 workspace lines, nalgebra-only dep, hari-swarm depends on it), but reframe is
>   closed unless someone arrives with a pre-registered axis AND the control. The
>   architecture review adds: pre-register per-axis comparator values BEFORE
>   instrumenting, else the reframe becomes the roadmap's first A/B-doctrine violation
>   (axes only Lie can score on are unfalsifiable by construction). Owner call: freeze
>   hard, or fund exactly axes (b) then (a) under the control.
>
> **Architectural liability (2026-07-21, must precede any SL-default decision):** the
> SL short-circuit at the top of `process_research_event` is a parallel substrate wearing
> a `PriorityModel` variant — it bypasses the evidence ledger, perception, swarm
> bridging, belief-graph propagation, provenance, and #16 revision, and has already
> accumulated degradation branches (RelationDeclaration/Withdrawal ignored; Correction
> downgraded to reset-to-vacuous). Every new payload variant is now implemented twice or
> degraded once. "Promote SL to default" as currently framed would silently drop Phase 8
> reasoning, Phase 4 consensus, and provenance. Before ANY such decision: either re-seam
> SL as an evidence-fusion strategy inside the shared event shell (graph/revision/swarm
> stay common), or explicitly relabel `PriorityModel::SubjectiveLogic` as a
> comparison-baseline-only, closing the promote branch. This is the one open item that
> gets WORSE on its own as unrelated event types land.

## Phase 8: Belief-Graph Reasoning — **implemented**

Goal: let IX declare logical relations between propositions and have Hari derive new beliefs by propagation — classical forward inference over a typed graph.

Status: implemented. New `ResearchEventPayload::RelationDeclaration { from, to, relation }` variant; the existing `BeliefNetwork` graph + `propagate_until_stable` is now wired into the IX research-event boundary. Every belief-changing event (BeliefUpdate / ExperimentResult / AgentVote / Retraction / RelationDeclaration) triggers `propagate_until_stable(10)` after handling — on networks with no declared relations this is a single zero-change pass, so existing fixtures replay unchanged (regression-pinned).

Delivered:
- `BeliefNetwork::declare_relation(from_label, to_label, relation)` in `hari-lattice` — auto-creates missing endpoints as `HexValue::Unknown`.
- `ResearchEventPayload::RelationDeclaration` variant + `ResearchEvent::touched_propositions()` (returns 0/1/2 propositions per event) used by the streaming layer for the final-beliefs snapshot.
- Propagation pass at the end of every `process_research_event`. When propagation actually does work (≥2 rounds), a `Action::Log("Propagated beliefs in N rounds")` is appended for observability.
- `fixtures/ix/derivation.json`: a 6-event scenario demonstrating multi-hop derivation (Implies then Supports) and contradiction emergence via a later Contradicts edge. Exempted from the `phase5_replay` action-divergence contract since reasoning fixtures are about derivation, not priority-model A/B.
- `crates/hari-core/tests/phase8_reasoning.rs`: 5 tests covering regression-on-relation-free-fixtures, multi-hop derivation, contradiction emergence, auto-creation of endpoints, propagation-log emission, and JSON wire-format round-tripping.

Out of scope for this slice (deliberate):
- Rule-based / Datalog-style multi-premise inference (only pairwise `Supports`/`Contradicts`/`Implies` for now).
- Withdrawal / reversal of declared relations (append-only).

### Phase 8 follow-up: derivation provenance — **shipped**

Every derived belief now carries a structured `Derivation { proposition, previous_value, new_value, contributions, round }` record on the `ResearchEventOutcome`, plus a per-derivation `Action::Log` for human-readable observability. Each `Contribution { source, source_value, relation, contributed_value }` records the edge that fed into the combined value, including the post-NOT value for `Contradicts` so consumers don't have to re-apply the lattice rule.

Delivered:
- `hari_lattice::Derivation` and `hari_lattice::Contribution` types (Serialize/Deserialize, public).
- `BeliefNetwork::propagate_with_provenance() -> (usize, Vec<Derivation>)` and `propagate_until_stable_with_provenance(max) -> (usize, Vec<Derivation>)`, both alongside the existing trust-blind `propagate*` methods.
- `ResearchEventOutcome::derivations: Vec<Derivation>` with `#[serde(default, skip_serializing_if = "Vec::is_empty")]` — keeps existing JSON traces byte-equal (no `derivations` field appears when the list is empty).
- The previously redundant single-round `BeliefNetwork::propagate()` call inside `cycle_raw` was removed. All propagation now happens once-and-only-once at the end of `process_research_event` through the with-provenance API, so the audit trail is complete by construction.
- 6 new tests in `phase8_provenance.rs` covering the multi-hop chain, per-derivation Log emission, regression on relation-free fixtures, JSON skip-empty behavior, and the `Contradicts` post-NOT contribution rule.

## Phase 9: Demerzel Hex-Merge Conformance — **implemented (in-Hari)**

Goal: implement Demerzel's `logic/hex-merge.md` G-Set CRDT merge inside Hari with semantics matching `ix-fuzzy::observations` so the two implementations stay aligned. Closes the conformance gap called out in the IX-side PRD (`ix/governance/demerzel/docs/prd/07-hari.md` v2.0).

Status: shipped as `hari_lattice::merge`. Faithful port of the ix-fuzzy structure: `HexObservation`, `HexDistribution`, `MergedState`, `belnap_weight`, `merge` / `merge_all` / `merge_with_default_staleness`. Same Belnap-extended weight table, same content-derived synthesis ids (the associativity fix), same staleness budget K=5, same uniform fallback for empty input.

Delivered:
- `crates/hari-lattice/src/merge.rs` — pub module re-exported from `hari-lattice` lib root.
- 6 proof-obligation tests mirroring ix-fuzzy: commutativity, associativity, idempotence, monotonicity, dedup-by-key, Belnap symmetry.
- 10 functional tests covering the Belnap table, agreement/disagreement scenarios, meta-conflict cross-aspect detection, staleness filtering, and `action_and_aspect` parsing (including the rfind-based deep module path case from `harness-cargo.md`).
- `HexDistribution::escalation_triggered()` matching `ix-fuzzy::hexavalent::ESCALATION_THRESHOLD = 0.3`.
- 175 tests across the workspace at HEAD (159 prior + 16 merge).

Out of scope for this slice (deliberate):
- Cross-repo byte-equal fixture suite. Right now both implementations are spec-faithful; a shared `fixtures/hex-merge/` directory with serialized inputs and expected outputs that both `hari` and `ix-fuzzy` run against would prove byte-equality. That's a follow-up that needs the fixtures to live somewhere both repos can reach (likely `Demerzel/examples/` or similar).
- `TrustedHexObservation` extension. The PRD lists this as a graduation candidate from Hari → ix-fuzzy; not implemented here because the layering question (does trust live in the merge layer or above it?) is a cross-repo design conversation, not a refactor.
- Wiring `merge` into `hari-core::CognitiveLoop`. Hari's existing `ResearchEventPayload::AgentVote` → swarm-consensus path is a different shape from G-Set merge over `claim_key`. Bridging the two is a future design call.

> **Research insights (2026-07-21) — all three follow-ups investigated.**
>
> - **Fixture suite: mostly already exists, and hari has ALREADY drifted.** The canonical
>   corpus is `Demerzel/fixtures/hex-merge/` (12 fixtures + README — not 7); ix-fuzzy
>   already consumes it via its `governance/demerzel` submodule. hari's local copy has
>   byte-drifted (fixtures 01–07: CRLF vs LF, `cmp` fails) — semantically equal, not
>   byte-equal, no CI parity guard. Exactly the failure the 2026-05-02 cross-repo report
>   predicted. Fix (one small PR): add Demerzel as a submodule, repoint
>   `hex_merge_conformance.rs::fixture_dir()`, delete the local copy — the JSON Schema
>   Test Suite pattern. Byte-equality becomes true by construction; the submodule SHA is
>   the version pin. Demerzel's no-runtime-code invariant is preserved (fixtures are pure
>   data; each consumer owns its runner). Bonus standalone slice: a JSON Schema for the
>   fixture format in Demerzel (today it's README prose). **[flagged]** Simplicity review
>   said "defer until first divergence" — the discovered drift IS the first divergence,
>   so the defer condition has fired.
> - **TrustedHexObservation: trust stays ABOVE the merge — forced by CRDT correctness,
>   not preference.** Per-source trust folded into merge makes merged state depend on the
>   observer's trust table — exactly what a CRDT must not do (cf. [Kleppmann's BFT-CRDT
>   approach](https://martin.kleppmann.com/papers/bft-crdt-papoc22.pdf): reject before
>   merge, never weight inside it; Matrix state resolution: merge trust-blind, auth pass
>   after). Load-bearing subtlety: trust attaches ONLY at the distribution-derivation
>   step — it scales contribution mass but must NOT change whether a contradiction is
>   detected or escalated (else a low-trust source silently mutes a T/F conflict — the
>   v1.2 abstention-muting hole in a new guise). Smallest slice:
>   `project_trusted(state, trust_fn)` downstream of an untouched `merge`; pin
>   `project_trusted(state, |_| 1.0) == state.distribution` (uniform trust = identity)
>   plus a test that a low-trust source's C still appears and still escalates. That test
>   IS the layering decision, executable. All 9 proof obligations survive by
>   construction.
> - **Merge↔CognitiveLoop: partially wired already; the shapes answer different
>   questions.** The #16 revision path already calls `merge_with_tombstones` +
>   `project_belief`; only the live `AgentVote` path routes exclusively to swarm
>   consensus. Consensus collapses to a point value (trust-aware,
>   contradiction-collapsing); merge yields a distribution + synthesized C + escalation
>   (contradiction-preserving). Falsifiable hypothesis: the swarm path hides escalations
>   merge would raise — `swarm_dissent.json` reaches Contradictory only under lopsided
>   RoleWeighted trust, but Belnap `T+F→C` is trust-blind. Smallest A/B (routing change,
>   all building blocks in-tree): `PerceptionModel::{SwarmConsensus, GSetMerge}` on
>   `SessionConfig`, default preserving behavior bit-for-bit; divergence test asserting
>   GSetMerge escalates under `Equal` where SwarmConsensus yields Doubtful. If it can't
>   diverge, that's also an answer. **[flagged]** Simplicity review says CUT (second
>   consensus surface, zero demonstrated decision delta); architecture review says never
>   wire merge as a *parallel* hari-core path but consider re-basing swarm consensus ON
>   merge behind `consensus_with`. Sequencing consensus: fixture suite first (lock
>   conformance before merge is touched), then the layering slice, then — only if an
>   owner wants the question answered — the PerceptionModel A/B.

## Near-Term Milestone

**Original** (pre-SL data): Hari can run a 50-cycle JSON research scenario in baseline and experimental modes, produce a metrics report, and show whether Lie-inspired state evolution changes research decisions compared with a simple priority baseline. — *Delivered.*

**Updated**: Hari operates as the epistemic substrate for a real IX autoresearch session over its streaming protocol, producing reproducible recommendations whose quality is measured against a non-Hari-assisted IX baseline. Requires the Phase 6 IX-side client work above.

## Open Questions

- What exact confidence thresholds should map experiment outcomes to hexavalent values?
- Should contradictory evidence decay, persist forever, or require explicit resolution?
- Should consensus optimize for correctness, caution, or investigation value?
- Which IX research tasks are most suitable for first evaluation?
- What role should GA play: scenario generator, domain oracle, or external evaluator?

> **Research insights (2026-07-21) — the five questions, adjudicated.**
>
> **Q1 (thresholds): make them empirical claims with pinned falsifiers, not parameters.**
> The numbers already exist (hex↔SL embedding + Accept b>0.7 / Escalate C-mass>0.3 in
> `2026-07-20-hexmerge-sl-correspondence.md`) but were chosen for the correspondence
> study, never calibrated against outcomes. Proposed default: threshold ONE scalar — SL
> projected probability `P = b + a·u` (Jøsang) — with pre-registered bands
> (P≥0.85→T, 0.6–0.85→Probable, 0.4–0.6→Unknown, mirrored for D/F); route to
> `Contradictory` only via a two-sided rule (b>τc AND d>τc — C stays off-chain, never
> from a mid-range projection); hard uncertainty gate `u≥0.6→Unknown` (autonomy never
> fires from Unknown). Judge any band set by threshold-weighted Brier reliability
> (Murphy decomposition) over the replay corpus with incumbent bands as the A/B
> baseline; derive Accept cutoffs cost-weighted on a calibration split, then FREEZE
> (AUC-GUIDE). Per-source recalibration only once the #14 ledger has data, and only if
> it beats `pooled` — mirrors the entrenchment doctrine. Falsifier: a reliability-diagram
> bound pinned as a `probe_*` regression. Don't tune against fixtures; wait for real IX
> data (Phase 6 remainder forces this question at the right time).
>
> **Q2 (contradiction lifecycle): CLOSE AS DECIDED — the design already answers it.**
> Contradictions neither decay nor persist unconditionally: they persist exactly while
> their evidence is live and unresolved. Derived C dissolves with its support (JTMS
> foundationalism, already ratified in `belief-revision-and-retraction.md` §5); standing
> C persists anti-dilutively while both sides are in the K-window; explicit resolution
> only via #16 events (Correction/Supersession/Retraction); entrenchment (AGM ordering
> via #14) may WEIGHT a conflict but never collapse it — take AGM's ordering, refuse
> AGM's consistency-restoration. The single falsifiable commitment: **no code path
> reduces C-mass without a change in the live evidence set** — pin as
> `contradiction_has_no_independent_decay` and `entrenchment_orders_but_does_not_collapse`.
> The only defensible time term is the existing K-window on evidence liveness, never a
> half-life on C.
>
> **Q3 (consensus objective): CUT as a question** — unanswerable in the abstract; the
> false-acceptance/false-escalation metric pair already operationalizes the trade-off,
> and the answer falls out of milestone data.
>
> **Q4 (first IX task): ANSWERED by the eval research** — flaky-vs-real benchmark
> discrimination (mechanical ground truth via injected perturbations), then
> contradictory-across-configs. This is the critical-path item: the one genuinely
> blocking decision is the owner picking the task.
>
> **Q5 (GA's role): DEFER** — irrelevant until the milestone produces data; no GA
> involvement is needed to hit it.

## Sequencing (2026-07-21 synthesis)

Critical path to the Updated Near-Term Milestone (everything else is off-path):

1. Owner picks the first IX task (Q4 — recommend flaky-vs-real benchmark discrimination).
2. Pre-register metrics/baselines/decision rule/MDE (git-committed doc; wire the unused
   `forecast.rs` calibration ledger into the report; fix or exclude the two
   tied-by-construction metrics).
3. Extend `clients/ix_reference` into the paired-task driver (Hari-on vs Hari-off over
   recorded traces via the shadow loop).
4. Run, bootstrap, apply the pre-registered rule, write ONE report in `docs/research/`.

Rework-minimizing order for the rest (architecture review): (a) decide the
fusion-substrate question — the SL short-circuit liability and the merge/consensus
question are one decision about how many evidence-combination pipelines hari-core owns;
(b) land the Demerzel-submodule fixture switch (locks conformance before merge is
touched); (c) close stale roadmap bullets (done in this pass); (d) TrustedHexObservation
layering slice after (a); (e) reframe instrumentation only after the Phase 6 report
format exists, so the axes are born falsifiable.
