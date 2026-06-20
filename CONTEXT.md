# CONTEXT — hari domain glossary

> The shared language of Project Hari. `/grill-with-docs` grows this lazily as
> terms get resolved; `/improve-codebase-architecture`, `/qa`, and `/tdd` read it
> so their output uses **our** words. This is a **seed** — add terms when a real
> ambiguity is resolved, not speculatively.

## What hari is

An experimental Rust sandbox that aims to be a typed, **contradiction-preserving
epistemic substrate** for autoresearch systems. It does not *do* the research —
it tracks uncertain claims, preserves conflicting evidence, coordinates agent
beliefs under explicit trust, derives downstream beliefs from declared logical
relations, and recommends what needs more investigation. An external system (**IX**)
drives experiments and feeds results in via a typed event boundary. Sibling of
**Guitar Alchemist** (ga), **Demerzel** (governance), and **Prime Radiant**.

## Architecture invariant

**Four library crates**, strict bottom-up dependency — circular deps are easy to
introduce, so `hari-lattice` and `hari-cognition` MUST stay leaf-ish:
`hari-lattice` → `hari-cognition` → `hari-swarm` → `hari-core` (the only binary).

## Core terms (seed)

- **HexValue** — the hexavalent logic primitive: `True | Probable | Unknown |
  Doubtful | False | Contradictory`. The chain `F < D < U < P < T` is an ordered
  lattice; **`Contradictory` sits outside the chain** and is preserved by
  `join`/`meet`, never collapsed — this is the epistemic-humility design choice.
- **ResearchEvent** — the typed boundary between Hari and IX. Serde-tagged
  payloads (`belief_update`, `experiment_result`, `agent_vote`, `retraction`,
  `goal_update`); evidence kept as an audit `BTreeMap`. Each event returns an
  **Action** (`Investigate | Retry | Accept | Escalate | Wait`).
- **CognitiveLoop** — `hari-core`'s Perceive → Think → Act cycle over a
  `CognitiveState` (a `BeliefNetwork`, prioritized goals, an `attention` vector).
- **PriorityModel** — action-scoring variant: `Flat | RecencyDecay | Lie |
  SubjectiveLogic`. **Default is `RecencyDecay`** (the Lie hypothesis lost to the
  Subjective-Logic baseline in Phase 5); pinned by test, don't change without owner.
- **Swarm / TrustModel** — `hari-swarm` Agents carry roles (`explorer / critic /
  integrator / guardian`) with `self_trust` + `message_trust`. `RoleWeighted`
  consensus is opt-in; **`Equal` is the default** — switching it is a project call.
- **Cherny-loop harness** — `hari_harness` runs one Boris-Cherny iteration over the
  persistent belief state. Reads `state/harness/belief-snapshot.json` (authoritative
  facts) + `belief-diff.json` (to-do queue); **exit 2 = `Escalate`**, needs a human.

## Conventions

See `CLAUDE.md` for authoritative build/test commands, the crate hierarchy, and
the design philosophy (epistemic humility, replayable-over-live, A/B-able).
