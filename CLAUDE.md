# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Context

Project Hari is an **experimental Rust research sandbox** for belief-state reasoning, cognitive-state dynamics, and multi-agent consensus under uncertainty. Phase 5 (cognition integration) and Phase 6 (IX autoresearch streaming) are both **implemented**; the original "Lie-inspired state evolution beats simpler baselines" hypothesis was tested and **does not survive comparison against a Subjective Logic baseline** — see `docs/research/phase5-fixture-rollup.md` §7 and `docs/research/phase5-results.md` §6 for data and verdict. SL beats Lie on `false_acceptance_count` on 3/6 fixtures, ties on 3, never loses; the `hari-cognition` crate's value proposition is now an open project-direction question.

The current state: three priority models live alongside each other (`PriorityModel::{Flat, RecencyDecay, Lie}` for action scoring, plus `process_research_trace_subjective_logic` as a parallel decision engine that doesn't fit the action-scoring abstraction). Default is still `Flat` — a future project-direction call decides whether to switch to `RecencyDecay` or `SubjectiveLogic`. Don't change the default without explicit owner approval; the `divergence_test_pins_alpha_and_dt` test pins the Lie tunables so any retuning is explicit. The roadmap requires every milestone to be testable against a simpler baseline — when adding new behaviors, prefer designs that can be A/B'd. Phase 7+ direction (whether to keep, demote, reframe, or cut `hari-cognition`) is open.

## Common Commands

Cargo workspace at the repo root. Toolchain: Rust 1.85+ (the Dockerfile pins `rust:1.85-slim`).

```bash
cargo build --release                         # Build all crates
cargo run --release -p hari-core              # Run the scripted 10-cycle demo
cargo run --release -p hari-core -- replay fixtures/ix/conflicting_benchmark.json
                                              # Replay an IX research trace, emit JSON report to stdout
cargo test --all                              # Run all tests across the workspace
cargo test -p hari-lattice                    # Run tests for a single crate
cargo test -p hari-core parse_trace_accepts_object_form  # Run a single test by name
cargo clippy --all-targets --all-features
cargo fmt --all
docker-compose up hari-core                   # Sandboxed run (read-only fs, mem/cpu limits)
```

The `hari-core` binary has two modes selected by the first positional argument: no args runs the scripted simulation; `replay <path>` deserializes a `ResearchTrace` (object form) or a bare `Vec<ResearchEvent>` (array form) and emits a `ResearchReplayReport` as pretty JSON. When adding new event types, update both `parse_trace` paths.

## Architecture

The workspace is **four crates with a deliberate dependency hierarchy** — understand this before editing, because circular deps are easy to introduce:

```
hari-lattice  (no internal deps)         — 6-valued logic primitives + BeliefNetwork
    ↑
hari-cognition (depends on nalgebra only) — Lie algebra dynamics, SymmetryGroup, Evolution
    ↑
hari-swarm    (depends on lattice + cognition) — Agent, Message, Swarm, consensus
    ↑
hari-core     (depends on all three)      — CognitiveLoop, ResearchEvent boundary, binary
```

`hari-lattice` and `hari-cognition` MUST stay leaf-ish — do not pull in `hari-swarm` or `hari-core` from them.

### The Cognitive Loop (`hari-core`)

`CognitiveLoop` runs **Perceive → Think → Act** cycles over a `CognitiveState` containing a `BeliefNetwork` (from lattice), a `BTreeMap<String, Goal>` of prioritized goals, and an `attention: DVector<f64>` whose dimension matches the cognitive algebra's state space. `Evolution` (from `hari-cognition`) is held as `Option<Evolution>` and is currently optional — the integration of Lie-algebra evolution into action selection is the explicit goal of Phase 5 in the roadmap and is **not yet wired up**. Be careful not to assume `evolution` is `Some` when adding logic to `cycle()`.

### Research Event Boundary

`ResearchEvent` / `ResearchEventPayload` / `ResearchTrace` / `ResearchReplayReport` (in `hari-core`) are the **typed boundary between Hari and external autoresearch systems** (IX). Payload variants are tagged via serde with `#[serde(tag = "type", rename_all = "snake_case")]` — JSON uses `"type": "belief_update"`, `"experiment_result"`, `"agent_vote"`, `"retraction"`, `"goal_update"`. Evidence is preserved as a `BTreeMap<String, serde_json::Value>` for audit. When extending payload types, also extend the recommended-action set (`Action::Investigate / Retry / Accept / Escalate / Wait`) — these are what gets returned to IX.

### Hexavalent Logic (`hari-lattice`)

`HexValue` is `True | Probable | Unknown | Doubtful | False | Contradictory`. The chain `F < D < U < P < T` forms an ordered lattice, with `Contradictory` as a special absorbing fixed point that sits **outside the chain** (rank 5 internally but treated specially in lattice ops). When implementing operations on `HexValue`, do not treat `Contradictory` as merely "rank 5" in `join`/`meet` — irreconcilable evidence must be preserved, not collapsed. This is a deliberate design choice tied to the project's epistemic-humility philosophy.

### Swarm (`hari-swarm`)

`Agent` carries an `AgentRole` with `self_trust` and `message_trust` parameters; the four canonical roles seeded in the demo are `explorer / critic / integrator / guardian` with distinct trust profiles. Message routing supports `to: "*"` for broadcast. `Swarm::process_all()` and `Swarm::consensus(prop)` are the two main integration points and remain trust-blind by default. `TrustModel::RoleWeighted` (Phase 4, opt-in) enables `self_trust`-weighted consensus via `Swarm::consensus_with` and message-filtering by `message_trust >= MESSAGE_TRUST_THRESHOLD` (0.5) via `Swarm::process_all_with`; filtered messages are reported via `InboxStats::filtered`. The default stays `Equal` — switching the default is a project-direction call like the cognition substrate one, not a refactor. Source-reliability tracking across scenarios is **not** part of Phase 4's current slice — that's deferred until scenario-replay infra in `hari-core` exists to own it.

## Scenario Fixtures

`fixtures/ix/*.json` are replayable IX-style traces consumed by `hari-core replay`. The conflicting_benchmark fixture demonstrates a `belief_update → contradicting belief_update → retraction` sequence. New fixtures should target a specific scenario shape (conflicting evidence, noisy benchmarks, agent disagreement) and be deterministically replayable for 50+ cycles where appropriate.

## Docker

`docker-compose.yml` defines `hari-core` and `hari-swarm` services. **Note**: there is currently no `hari-swarm` binary — the swarm crate is library-only and the compose service references a missing binary. This is a known issue tracked in Phase 0 of the roadmap. Don't be surprised; either fix it as part of your task or leave it alone, but don't paper over it by adding a stub binary unless that's the intent.
