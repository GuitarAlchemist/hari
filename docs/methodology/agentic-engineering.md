# Agentic Engineering — the harness is the work

> A read-on-demand reference, **not** an always-loaded instruction block. Distilled from Matt
> Pocock's "Agentic Engineering Workflow" (aihero.dev) + Ousterhout's *A Philosophy of Software
> Design*, and mapped to **this repo's** existing machinery. Read it when you're deciding *how* to
> direct AI on a non-trivial change — not on every turn. (Mirrors `ga/docs/methodology/agentic-engineering.md`,
> adapted to hari.)

## The one idea

**Optimise the harness, not the model.** The model is the engine; the *harness* — prompts, skills,
the codebase itself, the environment the agent runs in — is roughly half the system and the half you
fully control. The load-bearing consequence:

> *"How do you optimise token spend? Have a codebase that's easier to make changes in."*

A deeper, lower-duplication, better-documented codebase lets a **cheaper model** do the same work with
fewer tokens. hari is itself a *piece of harness* for the wider ecosystem — a typed, contradiction-
preserving belief substrate that other agents query (via `hari_query_belief` / `hari_snapshot` /
`hari_diff` / `hari_consensus`) instead of re-deriving epistemic state. Keeping its three crates deep
and its semantics test-pinned is what makes that substrate trustworthy.

## Strategic over tactical

AI ate **tactical** programming (writing syntax, chasing bugs, making commits) — it's cheaper and
faster than you at it. Your leverage is **strategic** programming (Ousterhout):

- **Design the hard parts up front.** Decide the consequential things before delegating — in hari that's
  the `HexValue` semantics, the `PriorityModel` default (`RecencyDecay`), and the swarm `TrustModel`
  default (`Equal`). Each is pinned by a test, so a change is a deliberate owner call, not a drift.
- **Scope tasks tightly.** A well-scoped task is one an AFK agent can finish with no further context.
- **Own the interfaces / seams between modules.** This is where bugs and rework concentrate.
- **Keep just-enough docs that point agents to the right place** — not exhaustive, navigational.

"Your skills are the ceiling on what AI can do." Delegate the tactical; keep the strategic mindset.

## DX ≈ AX

Agent experience ≈ developer experience. What makes a codebase pleasant for a senior human makes it
tractable for an agent: **deep modules** (a lot of behaviour behind a small interface), **low
duplication**, **clear seams**, **guardrails** (types, tests, invariants). Improving the codebase
*is* improving the harness. The
[`/improve-codebase-architecture`](../../.claude/skills/improve-codebase-architecture/SKILL.md)
vocabulary (module / interface / depth / seam / **deletion test**) is the shared language, read against
[`CONTEXT.md`](../../CONTEXT.md) (the seed domain glossary). The three crates are already deep modules
with small interfaces: the **belief lattice** (`crates/hari-lattice` — a hexavalent graph that keeps
`Contradictory` first-class rather than collapsing it), the **cognitive loop** (`crates/hari-core` —
Perceive→Think→Act behind one `process_research_event`), and the **swarm** (`crates/hari-swarm` —
`Swarm::consensus_with(proposition, trust_model)` behind `Equal`/`RoleWeighted` variants).

## Procedures vs abilities (and context hygiene)

- **Procedure** — a skill *you* invoke to stay in the driver's seat (`/grill-me`, `/to-prd`,
  `/to-issues`, `/improve-codebase-architecture` — the aihero set, installed project-scoped in #6).
  Prefer these; keep the thinking in the human. (CLAUDE.md already says: *"Prefer existing
  planning/review/quality tooling over adding new skills."*)
- **Ability** — a skill the *model* self-invokes. Every ability leaks its description into the context
  window. Too many = bloat; mark deliberate procedures `disable-model-invocation: true`.

Matt's blank-slate test: periodically strip skills / MCP / CLAUDE.md back toward nothing, watch what
the agent does unaided, then **layer back only the procedures you deliberately choose**. Treat a long
CLAUDE.md as a smell (hari's is ~120 lines — keep detail in read-on-demand docs like this one and the
phase docs under `docs/research/`).

## Queues, not loops

The unit of AFK work is a **queue** of well-scoped tasks, not an infinite prompt loop. Tasks flow
**triage → explore → implement → review → merge**, pulled off by labelled agents. hari already speaks
this: GitHub Issues + the canonical triage labels (see
[docs/agents/triage-labels.md](../agents/triage-labels.md)) and
[docs/agents/issue-tracker.md](../agents/issue-tracker.md). The sandboxed substrate for AFK runs is the
`hari-core` service — `docker compose run --rm hari-core ./hari-core serve` (streaming protocol) or
`./hari-core replay <fixture>` (deterministic replay), defined read-only in `docker-compose.yml`. Keep
**human-in-the-loop checkpoints**, but push them as far toward the final output as the work safely allows.

## Build self-improving systems

When a model finds a deep bug, the lesson is **not** "the model is great" — it's *"I should have a
system that catches this."* hari bakes this into its **roadmap rule: every milestone must be testable
against a simpler baseline.** That's why the Lie-inspired state evolution was *demoted from default* on
a negative result (it doesn't beat a Subjective-Logic baseline — `docs/research/phase5-results.md` §6),
and why the substrate decisions are guarded by tests: `test_priority_model_default_is_recency_decay`
pins the default, `divergence_test_pins_alpha_and_dt` pins the Lie tunables, and
`default_mode_preserves_pre_bridge_outcomes_on_swarm_dissent` regression-pins the Phase-4 bridge. CI
enforces the floor (`ci.yml`: `cargo fmt --check` + clippy + `cargo test --all`). Prefer extending these
A/B-able baselines over one-shotting fixes — *"if someone keeps stealing your bike, buy a lock."*

## Make review seamless

The bottleneck is human review, so spend the harness on making review *fast*. hari's distinctive
contribution to ecosystem review is **disagreement preserved, not averaged**: `Swarm::consensus_with`
under `TrustModel::RoleWeighted` weights each agent's vote by `self_trust` and filters messages by
`message_trust`, and a divided panel surfaces as `Contradictory` (a first-class value) rather than a
muddied mean. The Phase-6 streaming multiplexer (`hari_session_open` / `hari_session_event` /
`hari_session_close`) exposes this live, and `hari_snapshot` / `hari_diff` show exactly what a run
changed. You stay the gate on security and on "did the system do a good job," but you make that gate one
structured signal.

## You own the product

AI is weak at original ideas and at deciding *what* to build. Choose the features; ask "what can I
**remove**, how do I make this **simpler**." The classic product-design fundamentals still hold — AI
just implements them faster.

## The two action steps Matt actually recommends

1. **Strip to a blank slate, then layer deliberately.** Remove the bloat; re-add only procedures you
   choose and can customise.
2. **Move work AFK.** Scope a task tightly, hand it to a sandboxed agent (a git worktree off `main`, or
   the read-only `hari-core` container), review the result. Two of you, then three, then five — then you
   review.

---

*Pointer, not gospel: this doc is read when you're deciding how to direct a non-trivial change. It is
deliberately **not** wired into the always-loaded instruction set — that would contradict its own
context-hygiene advice.*
