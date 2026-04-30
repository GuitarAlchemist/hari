# Project Hari: Cognitive-State Research Sandbox

## Purpose

Project Hari is an **experimental Rust sandbox for belief-state reasoning, cognitive-state dynamics, and multi-agent consensus under uncertainty**.

The project explores whether a small cognitive loop can combine:

- **Discrete epistemic reasoning** (6-valued logic: True/Probable/Unknown/Doubtful/False/Contradictory)
- **Continuous state evolution** (Lie algebra-inspired transformations over an attention/state vector)
- **Multi-agent consensus** (swarm-based belief sharing and voting)

The working hypothesis is deliberately narrower than "build AGI": some cognitive operations may be modeled as composable transformations over state, and the structure of those transformations may expose useful patterns in belief revision, goal selection, or agent coordination.

This repository is for testing that hypothesis in code. Claims should be treated as experimental until the system can beat simpler baselines on concrete scenarios.

## Status

**Experimental / pre-proof-of-concept** — active research code with several incomplete integrations. Not intended for production or external use. No stability guarantees.

Current known gaps:
- `hari-cognition` is implemented as a standalone algebra/dynamics crate, but is not yet meaningfully driving the main cognitive loop.
- Swarm roles define trust parameters, but trust-weighted message handling is still rudimentary.
- The current demo is a scripted simulation, not evidence of emergent reasoning.

## Current Direction

Four Rust crates define the current research surface:

### 1. `hari-lattice` — Hexavalent Belief Logic
A 6-valued belief logic system with join/meet-style operations, belief networks, and propagation. Each proposition's truth value lives in {True, Probable, Unknown, Doubtful, False, Contradictory}.

### 2. `hari-core` — Cognitive Loop Orchestrator
The central cognitive system running **Perceive → Think → Act** cycles. Manages:
- Perception intake from the environment
- Belief network updates via lattice logic
- Goal prioritization and status tracking
- Action selection based on cognitive state

### 3. `hari-cognition` — Lie Algebra Dynamics
Implements state evolution via Lie algebra-inspired generators. Core idea: cognitive operations can be represented as composable basis transformations. Uses matrix exponentials, commutators, and structure constants to analyze operation order and composition.

### 4. `hari-swarm` — Multi-Agent Swarm Consensus
Manages a collective of independent agents with role-based trust parameters (explorer, critic, integrator, guardian). Agents exchange hexavalent belief updates and compute consensus functions designed for epistemic humility.

## Next Milestone

**Proof-of-concept evaluation workflow:**
- Define one reproducible scenario with conflicting evidence, changing goals, and multiple agents.
- Add a simple non-Lie baseline for state updates.
- Integrate `hari-cognition` so its state evolution changes action selection or goal prioritization.
- Run both systems on the same 50+ cycle simulation.
- Report whether the Lie-inspired model improves a measurable outcome, such as contradiction recovery, goal completion, consensus quality, or stability under noisy evidence.

The milestone is successful only if the Lie-inspired path produces a measurable, explainable difference from the baseline.

See [ROADMAP.md](ROADMAP.md) for the phased plan, including the proposed IX autoresearch integration.

## Running the Code

### Prerequisites
- Rust 1.85+ (stable or nightly)
- Cargo

### Build and Run
```bash
cargo build --release
cargo run --release -p hari-core
```

Expected output: a scripted 10-cycle demonstration of the cognitive loop with swarm consensus votes on propositions.

### Replay an IX Research Trace
```bash
cargo run --release -p hari-core -- replay fixtures/ix/conflicting_benchmark.json
```

Expected output: a JSON report with event outcomes, final touched beliefs, final goals, and a final state summary.

### Docker (Sandboxed)
```bash
docker-compose up hari-core
```

Runs the core system in an isolated, read-only container with resource limits. See `Dockerfile` and `docker-compose.yml` for configuration.

### Tests
```bash
cargo test --all
```

Test coverage verifies:
- Lattice operations (join, meet, negation)
- Belief propagation convergence
- Swarm message routing and consensus
- Lie algebra structure constants
- Cognitive state evolution

## Not Yet

This is **not**:
- **Production software** — no performance tuning, minimal error recovery
- **Stable APIs** — expect breaking changes frequently
- **Consumer-facing** — not a tool for external users
- **A proven cognitive architecture** — the hypothesis is unproven; this is exploratory research code
- **Complete** — many subsystems are stubs (e.g., Hamiltonian is hard-coded)
- **Well-integrated** — `hari-cognition` integration into the main loop is WIP

## Design Philosophy

The project is designed around **epistemic humility**:
- Contradictory evidence is preserved, not forced to binary
- Consensus mechanisms acknowledge minority views
- Goals can be escalated rather than forced to resolution
- Algebraic structure is treated as a hypothesis to test, not an assumed explanation

## Repository Structure

```
hari/
├── Cargo.toml                # Workspace manifest
├── docker-compose.yml        # Multi-agent orchestration
├── Dockerfile                # Rust → release binary → minimal runtime
└── crates/
    ├── hari-core/           # Cognitive loop orchestrator
    ├── hari-lattice/        # 6-valued logic engine
    ├── hari-cognition/      # Lie algebra cognitive dynamics
    └── hari-swarm/          # Multi-agent belief consensus
```

## Related Projects

- **Demerzel** — governance and self-modifying systems
- **Prime Radiant** — visual knowledge representation
- **Guitar Alchemist** — domain-specific reasoning engines
