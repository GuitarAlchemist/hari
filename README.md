# Project Hari — Cognitive-State Research Sandbox

## Purpose

Project Hari is an **experimental Rust sandbox** that aims to be a typed, contradiction-preserving **epistemic substrate** for autoresearch systems. Its job is not to *be* the researcher — it tracks uncertain claims, preserves contradictory evidence, coordinates agent beliefs under explicit trust, derives downstream beliefs from declared logical relations, and recommends what needs more investigation. An external system (IX, in the working integration target) drives experiments and feeds results in; Hari maintains the belief state and recommends next actions.

The original "Lie-inspired state evolution beats simpler baselines" hypothesis was tested and **does not survive comparison against a Subjective Logic baseline** on the Phase 5 fixtures. SL beats Lie on `false_acceptance_count` 3/6 fixtures, ties 3/6, and never loses. The project's defensible value claim is the substrate (typed claim layer + reasoning + trust-aware consensus + audit trail), not the Lie-algebra dynamics that motivated the original hypothesis. Lie remains in the codebase as an opt-in research knob.

## Status

Active research code. **Not** production software, **not** a stable API, **not** an external-facing tool. But the substrate is concretely shipped and tested — `cargo test --all` passes 159 tests across 11 suites at HEAD, and every milestone listed below has either ✅ landed or ⏸ been deliberately deferred (no half-done features in `main`).

For phase-by-phase status see [`ROADMAP.md`](ROADMAP.md).

## What Hari does today

End-to-end, reachable from the streaming protocol via `SessionConfig`:

| Capability | Default? | Reachability |
|---|---|---|
| **Hexavalent belief tracking** with contradiction preservation | always on | `BeliefUpdate` / `ExperimentResult` events |
| **Action recommendations** (`Investigate / Retry / Accept / Escalate / Wait`) | always on | response per event |
| **Four priority models for action scoring** | `RecencyDecay` (post-Phase-5 substrate decision) | `priority_model: "Flat" \| "RecencyDecay" \| "Lie" \| "SubjectiveLogic"` |
| **Forward reasoning** via `Implies` / `Supports` / `Contradicts` relations | active when relations are declared (no-op otherwise) | `RelationDeclaration` events |
| **Derivation provenance** — every derived belief carries the edge contributions that produced it | always on (omitted from JSON when empty) | `ResearchEventOutcome.derivations` |
| **Trust-aware swarm consensus** | opt-in (`Equal` is default) | `trust_model: "RoleWeighted"`, `initial_agents: [...]`, `use_swarm_consensus: true` |
| **Streaming protocol** (stdio JSONL) | — | `hari-core serve` with deterministic replay parity (`replay --session`) |
| **Python reference client** | — | `clients/ix_reference/` (stdlib-only) |

The four `PriorityModel` variants are all routable via `SessionConfig.priority_model`:

- **`Flat`** — every action priority 1.0, original order. Pre-Phase-5 default; now used for ablation.
- **`RecencyDecay`** — `priority = exp(-λ·age)`. Default since the substrate decision.
- **`Lie`** — `priority = base · (1 + α · proj(attention, axis))`. Opt-in research knob.
- **`SubjectiveLogic`** — short-circuits to Opinion-fusion pipeline (Jøsang 2016 prior art); structurally bypasses the action-scoring abstraction. Data-best non-Lie option per Phase 5; not the default (would be an explicit owner call).

## Architecture

Four library crates with a deliberate dependency hierarchy plus an out-of-tree reference client:

```
hari-lattice    (no internal deps)               6-valued logic, BeliefNetwork, propagate_with_provenance
    ↑
hari-cognition  (depends on nalgebra)            Lie algebra dynamics, SymmetryGroup, Evolution
    ↑
hari-swarm      (depends on lattice + cognition) Agent / Message / Swarm, TrustModel, weighted consensus
    ↑
hari-core       (depends on all three)           CognitiveLoop, ResearchEvent boundary,
                                                 SubjectiveLogic pipeline, streaming protocol, binary
```

`hari-lattice` and `hari-cognition` MUST stay leaf-ish — circular deps are easy to introduce.

`hari-swarm` is **library-only**: its capabilities reach the IX boundary through `hari-core` (via the Phase 4 bridge that routes `AgentVote` events into the swarm and exposes `TrustModel` on `SessionConfig`). There is no separate `hari-swarm` binary.

## Running it

### Build and run the demo

```bash
cargo build --release
cargo run --release -p hari-core
```

10-cycle scripted simulation showing perception, swarm consensus on key propositions, and action recommendations under the default `PriorityModel::RecencyDecay`.

### Replay an IX research trace

```bash
cargo run --release -p hari-core -- replay fixtures/ix/cognition_divergence.json
cargo run --release -p hari-core -- replay --compare3 fixtures/ix/long_recovery.json
cargo run --release -p hari-core -- replay --session traces/recorded.jsonl
```

`--compare` runs the trace through `RecencyDecay` baseline + `Lie` experimental and emits divergence. `--compare3` adds the SL baseline. `--session` replays a session-trace file recorded by `serve`.

### Streaming protocol (stdio JSONL)

```bash
cargo run --release -p hari-core -- serve
```

Spawn-and-pipe protocol per [`docs/research/phase6-design.md`](docs/research/phase6-design.md). One Hari subprocess per session; IX writes `Request` JSONL to stdin, reads `Response` JSONL from stdout. Sessions are replayable byte-for-byte when `trace_record_path` is set on `open`.

A stdlib-only Python reference client lives at [`clients/ix_reference/`](clients/ix_reference/) — copy `hari_client.py` and adapt.

### Sandboxed Docker run

```bash
docker compose up hari-core
docker compose run --rm hari-core ./hari-core serve
docker compose run --rm hari-core ./hari-core replay /path.json
```

Read-only fs, 4GB / 2 CPU caps, tmpfs `/tmp`. See [`docker-compose.yml`](docker-compose.yml) for the configuration.

### Tests

```bash
cargo test --all                                       # 159 tests across 11 suites
cargo test -p hari-core --test phase8_reasoning        # forward-reasoning suite
cargo test -p hari-core --test phase8_provenance       # derivation provenance
cargo test -p hari-core --test phase6_serve_subprocess # binary entry point over real stdio
```

## What's still open

⏸ items that need either external work or an explicit owner decision:

- **Real IX integration with benchmarks.** The wire is solid (subprocess-tested + reference client smoke-tested), but producing data that informs the substrate choice on real autoresearch tasks needs IX itself, not fixtures.
- **Default change to `SubjectiveLogic` or `RoleWeighted`.** Both would be small code changes (~10 lines each) but are project-direction calls, not refactors. The Phase 5 data supports SL; nothing yet justifies switching `use_swarm_consensus` on by default.
- **Cross-session source-reliability tracking.** Phase 4 sub-task; needs scenario-replay infra that tracks "which agents' votes led to right vs wrong decisions across runs." Speculative without real IX feedback.
- **Multi-premise rules** (`X AND Y → Z`). Current relations are pairwise; no use case has yet forced more.
- **Relation withdrawal / reversal.** Append-only for now.
- **Counterfactual fork primitive.** Cheap to add but mission-creep without an IX use case.

## Design philosophy

**Epistemic humility** — contradictory evidence is preserved, not forced into a binary collapse. Consensus mechanisms acknowledge minority views (`InboxStats::filtered` surfaces dropped low-trust messages). Goals can be escalated rather than forced to resolution. Algebraic structure is treated as a hypothesis to test (Phase 5 tested Lie's against simpler baselines; the data didn't favor it, and the default reflects that).

**Replayable over live** — every milestone is testable against fixtures (`fixtures/ix/*.json`); real-time live behavior is a downstream concern. Stream sessions can record verbatim and replay byte-equal.

**A/B-able by construction** — every new behavior ships with a baseline-vs-experimental comparison. New `PriorityModel` variants must coexist with existing ones; new trust models opt in. Defaults are pinned by tests so they can't drift silently.

**No premature ceremony** — auth, multi-tenant, persistence, distributed Hari are explicitly deferred until specific triggers fire (second tenant, distributed IX, etc.). Single-tenant research code, by design.

## Repository structure

```
hari/
├── Cargo.toml                  Workspace manifest
├── Dockerfile                  Multi-stage Rust → minimal Debian runtime
├── docker-compose.yml          Sandboxed hari-core service
├── README.md                   This file
├── ROADMAP.md                  Phased plan with shipped/⏸/open status
├── CLAUDE.md                   Codebase-internal instructions for Claude Code
├── clients/
│   └── ix_reference/           Python reference client for the streaming protocol
├── crates/
│   ├── hari-core/              Cognitive loop, ResearchEvent boundary, binary
│   ├── hari-lattice/           6-valued logic + BeliefNetwork + propagation
│   ├── hari-cognition/         Lie algebra dynamics, SymmetryGroup, Evolution
│   └── hari-swarm/             Agent / Message / Swarm, TrustModel, weighted consensus
├── docs/research/              Phase plans, results, and design docs
└── fixtures/ix/                Replayable IX-style traces
```

## Related projects

- **Demerzel** — governance and self-modifying systems
- **Prime Radiant** — visual knowledge representation
- **Guitar Alchemist** — domain-specific reasoning engines
