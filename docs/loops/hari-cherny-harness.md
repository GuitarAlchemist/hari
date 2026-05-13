# Hari × Cherny Loops — Harness Pattern

**Status:** Implemented (M0). The `hari-harness` binary is the substrate for all three Cherny-style loops; nothing here is theoretical.

**Date:** 2026-05-12
**Reversibility:** Two-way door. The harness is purely additive — removing the `state/harness/` directory and uninstalling the binary leaves the rest of Hari untouched.

## What problem this solves

Boris Cherny's three loops (*scheduled `/loop`*, *CLAUDE.md self-improvement*, *execution-verification*) all need **persistent institutional memory** between iterations. Today that memory is markdown — fine for prose, weak for cross-session reasoning. Hari already ships structured belief state with provenance and a first-class `Contradictory` value; `hari-harness` is the bridge that makes one Cherny-loop tick read and write that state.

## The pipeline

```text
                  ┌─────────────────────────────────────────────┐
                  │            state/harness/                   │
                  │                                             │
prose notes  ─►───┤   events.jsonl  (append-only, source of truth)
ix-autoresearch ──┤                                             │
raw JSONL events ─┤   belief-state.json  (full replay report)   │
                  │                                             │
                  │   belief-snapshot.json  ({prop → HexValue})  ──► next session
                  │                                             │
                  │   belief-diff.json  (added / changed / unchanged)
                  └─────────────────────────────────────────────┘
                              ▲                                  │
                              │                                  ▼
                              └──── Action::Escalate ◄──── /loop branches on exit 2
```

Inputs are converted to `ResearchEvent`s, appended to the log, and the *entire* log is replayed through `CognitiveLoop::process_research_trace`. The snapshot is a derived view; the log is the source of truth.

## Mapping to Cherny's three loops

### 1. Scheduled automation (`/loop`)

The harness binary is the unit of work for `/loop`. A typical schedule:

```text
# Every hour: ingest fresh autoresearch results
/loop 1h hari-harness --autoresearch state/autoresearch/latest.jsonl --target target_chatbot

# Every 6 hours: ingest the last session's notes (Mercury required)
/loop 6h hari-harness --notes state/session-notes/latest.txt --source claude-session
```

Hari replaces the "markdown summary the next loop ingests" pattern with `belief-snapshot.json` — structured, idempotent, replayable. The next iteration reads the snapshot instead of re-parsing transcripts.

### 2. CLAUDE.md self-improvement

The harness exits **2** when any `Action::Escalate` fires. `/loop` can branch on the exit code to surface rule candidates:

```bash
hari-harness --notes session.txt || \
  echo "::escalations::" && cat state/harness/belief-diff.json
```

Each escalation is a candidate "write a rule for this" signal. The diff distinguishes three kinds:

- **`added`**: a new belief — usually fine.
- **`changed`**: a belief flipped — worth review if it crossed `Contradictory`.
- **`unchanged`**: stability — the loop is converging.

A short human review of the diff promotes recurring `changed` propositions to a CLAUDE.md rule. Hari doesn't write the rule itself; it makes the *signal to write one* visible.

### 3. Execution-verification

The autoresearch adapter is already this pattern: each iteration is "run a thing, parse the verdict, accumulate the belief." The same shape works for `cargo test --json`, `dotnet test --logger trx`, Playwright runs, etc. — emit one event per assertion outcome.

Long horizons matter. Multiple `Probable`s on the same config consolidate to `True`; a `False` short-circuits the meta-loop and stops grinding on a doomed hypothesis. Single-shot verification doesn't need Hari — the value is in the 10th iteration, not the 1st.

## File layout

```text
state/harness/
├── events.jsonl          append-only event log; source of truth (gitignored)
├── belief-state.json     full ResearchReplayReport from the last run
├── belief-snapshot.json  compact { proposition → HexValue } view
└── belief-diff.json      what changed in the last run
```

`events.jsonl` should be gitignored (or rolled into a long-term archive). `belief-snapshot.json` and `belief-diff.json` are the documents the next session reads — small enough to commit if you want a public trail.

## Exit codes

| code | meaning                                          | typical use                        |
|-----:|--------------------------------------------------|------------------------------------|
|    0 | no escalations; safe to continue the loop        | `&&` chain into the next step      |
|    1 | hard error (I/O, parse, missing required flag)   | abort the loop                     |
|    2 | one or more `Action::Escalate` fired             | branch to human-review channel     |

## Operational guardrails

- **Per-call cost.** Notes go through Mercury (~$0.0003/note). A 50-note session ≈ $0.015. Structured inputs (autoresearch JSONL, raw events) are free.
- **Determinism.** Replay is deterministic given the same `events.jsonl`. The harness is safe to run any number of times.
- **Log growth.** The full log is replayed each iteration. At 1k events/day this is fine; archive past 100k.
- **Secrets.** `INCEPTION_API_KEY` is required only for `--notes`. Pure `--autoresearch` or `--events` runs are LLM-free.

## When NOT to use this

- One-shot verification — no belief state to persist; just run the test.
- Single-source-of-truth signals — if `cargo test` already exits non-zero on failure, you don't need Hari to track that.
- High-frequency loops (< 1 minute) — replay cost grows linearly with log size; budget accordingly.
