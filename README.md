# Project Hari: AGI Research Platform

## Purpose

Project Hari is an **experimental AGI research initiative** exploring a novel cognitive architecture based on **Lie algebra symmetries** and **hexavalent logic**. Rather than building on transformer LLMs, it investigates whether continuous mathematical structures (Lie groups and algebras) can form the foundation of a reasoning system that combines:

- **Discrete epistemic reasoning** (6-valued logic: True/Probable/Unknown/Doubtful/False/Contradictory)
- **Continuous cognitive dynamics** (Lie algebra evolution of cognitive state)
- **Multi-agent consensus** (swarm-based belief integration)

The hypothesis: cognitive operations can be viewed as infinitesimal generators of symmetry transformations. Complex reasoning emerges from composing simple algebraic generators — just as physical symmetries compose in particle physics.

## Status

**Experimental** — Active research phase. Code compiles and demonstrates basic cognitive loops with working subsystems. Not intended for production or external use. No stability guarantees.

## Current Direction

Four integrated Rust crates are under active development:

### 1. `hari-lattice` — Hexavalent Belief Logic
A 6-valued logic system implementing a bounded lattice with join/meet operations, belief networks, and automatic propagation. Each proposition's truth value lives in {True, Probable, Unknown, Doubtful, False, Contradictory}.

### 2. `hari-core` — Cognitive Loop Orchestrator
The central cognitive system running **Perceive → Think → Act** cycles. Manages:
- Perception intake from the environment
- Belief network updates via lattice logic
- Goal prioritization and status tracking
- Action selection based on cognitive state

### 3. `hari-cognition` — Lie Algebra Dynamics
Implements cognitive evolution via Lie algebra generators. Core idea: cognitive operations form a symmetry group whose algebra gives infinitesimal "basis moves" for thought. Uses matrix exponentials and structure constants to analyze cognitive composition.

### 4. `hari-swarm` — Multi-Agent Swarm Consensus
Manages a collective of independent agents with role-based trust parameters (explorer, critic, integrator, guardian). Agents exchange hexavalent belief updates and compute consensus functions designed for epistemic humility.

## Next Milestone

**Proof-of-concept end-to-end workflow:**
- Integrate `hari-cognition`'s Lie algebra evolution into the main cognitive loop
- Implement goal-driven perturbations to the cognitive Hamiltonian
- Run a 50+ cycle simulation with emergent multi-agent coordination
- Document one concrete example where Lie algebra structure reveals non-obvious cognitive patterns

## Running the Code

### Prerequisites
- Rust 1.85+ (stable or nightly)
- Cargo

### Build and Run
```bash
cargo build --release
cargo run --release -p hari-core
```

Expected output: a 10-cycle demonstration of the cognitive loop with swarm consensus votes on propositions.

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
- **Proven effective** — the hypothesis is unproven; this is exploratory science
- **Complete** — many subsystems are stubs (e.g., Hamiltonian is hard-coded)
- **Well-integrated** — `hari-cognition` integration into the main loop is WIP

## Design Philosophy

All code is designed with **epistemic humility**:
- Contradictory evidence is preserved, not forced to binary
- Consensus mechanisms acknowledge minority views
- Goals can be escalated rather than forced to resolution
- States are formally analyzed via algebraic structure, not heuristic

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
