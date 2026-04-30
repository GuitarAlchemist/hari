# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Context

Project Hari is an **experimental Rust research sandbox** for belief-state reasoning, cognitive-state dynamics, and multi-agent consensus under uncertainty. It is **pre-proof-of-concept** — APIs are unstable and many subsystems are stubs (e.g., the Hamiltonian in `hari-cognition` is hard-coded; trust-weighted message handling in `hari-swarm` is rudimentary; `hari-cognition` is not yet driving the main cognitive loop).

The hypothesis being tested: that some cognitive operations may be modeled as composable transformations over state, and that combining a 6-valued epistemic logic with Lie-algebra-inspired state evolution and swarm consensus produces measurably different research decisions than a simpler baseline. The roadmap explicitly requires every milestone to be testable against a simpler baseline — when adding new behaviors, prefer designs that can be A/B'd against a flat-confidence or lattice-only path. See `ROADMAP.md` for phasing; the next strategic target is **IX autoresearch integration**, where Hari acts as the epistemic substrate (not the researcher).

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

`Agent` carries an `AgentRole` with `self_trust` and `message_trust` parameters; the four canonical roles seeded in the demo are `explorer / critic / integrator / guardian` with distinct trust profiles. Message routing supports `to: "*"` for broadcast. `Swarm::process_all()` and `Swarm::consensus(prop)` are the two main integration points. Trust parameters exist on the type but are not yet meaningfully applied during belief integration — that's Phase 4.

## Scenario Fixtures

`fixtures/ix/*.json` are replayable IX-style traces consumed by `hari-core replay`. The conflicting_benchmark fixture demonstrates a `belief_update → contradicting belief_update → retraction` sequence. New fixtures should target a specific scenario shape (conflicting evidence, noisy benchmarks, agent disagreement) and be deterministically replayable for 50+ cycles where appropriate.

## Docker

`docker-compose.yml` defines `hari-core` and `hari-swarm` services. **Note**: there is currently no `hari-swarm` binary — the swarm crate is library-only and the compose service references a missing binary. This is a known issue tracked in Phase 0 of the roadmap. Don't be surprised; either fix it as part of your task or leave it alone, but don't paper over it by adding a stub binary unless that's the intent.
