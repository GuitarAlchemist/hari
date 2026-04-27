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

## Phase 4: Trust-Weighted Swarm

Goal: make agent roles operational.

Tasks:

- Apply `self_trust` and `message_trust` during belief integration.
- Track source reliability over repeated scenarios.
- Distinguish consensus strength from raw agreement.
- Add minority-report handling for plausible dissent.

Exit criteria:

- Agent roles change outcomes in measurable ways.
- Reports can explain why one source was trusted more than another.

## Phase 5: Cognition Integration

Goal: make `hari-cognition` affect decisions.

Tasks:

- Map active goals and claim clusters into attention/state dimensions.
- Let perceptions perturb the cognitive state.
- Let state evolution affect action priority.
- Add a non-Lie priority baseline.
- Compare Lie-inspired state updates against the baseline.

Exit criteria:

- `hari-cognition` changes at least one observable decision.
- The difference is measured against a simpler heuristic.
- The system can show when the Lie-inspired path helped, hurt, or made no difference.

## Phase 6: IX Autoresearch Loop

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

Exit criteria:

- IX can run an autoresearch trace with and without Hari.
- Hari-assisted runs are compared to baseline IX runs.
- Results are summarized in a report suitable for roadmap decisions.

## Near-Term Milestone

Hari can run a 50-cycle JSON research scenario in baseline and experimental modes, produce a metrics report, and show whether Lie-inspired state evolution changes research decisions compared with a simple priority baseline.

## Open Questions

- What exact confidence thresholds should map experiment outcomes to hexavalent values?
- Should contradictory evidence decay, persist forever, or require explicit resolution?
- Should consensus optimize for correctness, caution, or investigation value?
- Which IX research tasks are most suitable for first evaluation?
- What role should GA play: scenario generator, domain oracle, or external evaluator?
