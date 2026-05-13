# Hari × Federation — MCP and Cross-Crate Wire

**Status:** Implemented. `hari-mcp` server + three sidecar binaries land in PR #1.

## What this is

Hari is now reachable from any agent in the GuitarAlchemist MCP federation (ix-mcp, ga, tars, sentrux) via a fifth peer: **`hari-mcp`**. Add it to your `.mcp.json`:

```json
{
  "mcpServers": {
    "hari": {
      "command": "C:/Users/spare/source/repos/hari/target/release/hari-mcp.exe",
      "args": [],
      "env": { "HARI_STATE_DIR": "C:/Users/spare/source/repos/hari/state/harness" }
    }
  }
}
```

`HARI_STATE_DIR` is optional (default `state/harness` relative to CWD). Point it at a shared path if you want multiple agents reading the same belief state.

## Tools

| tool | reads | writes | purpose |
|---|---|---|---|
| `hari_query_belief` | snapshot | — | "What does Hari currently believe about *X*?" |
| `hari_snapshot` | snapshot | — | Full `{ proposition → HexValue }` map |
| `hari_diff` | diff | — | What changed since the previous harness run |
| `hari_record_observation` | events.jsonl | events.jsonl + snapshot | Append a typed event and replay |
| `hari_consensus` | — | — | Run swarm consensus over inline AgentVotes |

`hari_consensus` is pure — it accepts the votes in the call and returns the result without touching disk. That makes it safe to call from any agent without coordinating on a state dir.

## Sidecar binaries (all in `hari-extractor`)

| binary | input | output |
|---|---|---|
| `hari_harness` | `--notes` / `--autoresearch` / `--events` / `--cargo-test` | belief-state.json + diff |
| `hari_session_notes` | NL notes file | belief snapshot for next session |
| `hari_from_ix_autoresearch` | ix-autoresearch JSONL log | ResearchTrace JSON |
| `hari_review_aggregator` | per-reviewer AgentVote JSONL | TribunalReport JSON |
| `hari_code_relations` | call-graph JSON | RelationDeclaration JSONL |

The two newest (`hari_review_aggregator`, `hari_code_relations`) close the parallel-reviewer pattern and the structural-belief-propagation pattern respectively.

## Parallel reviewer aggregation (Tier-1 use case)

Dispatch the same PR to several LLM personas in parallel (Cherny's `feedback_octopus_parallel_decisions` pattern), have each emit one line of JSON, run the aggregator:

```pwsh
# 1. Each personas writes a vote to votes.jsonl (one line each).
# 2. Aggregate.
hari_review_aggregator --votes votes.jsonl --trust-model RoleWeighted
# Exits 2 if any consensus is Contradictory.
```

The killer feature is `HexValue::Contradictory` as a first-class consensus value: when 3 reviewers vote Probable/True/Doubtful, today's mean-or-mode average loses the disagreement. Hari surfaces it.

## Code-relation propagation (Tier-2 use case)

Static analyzers (`ix-code-analyze`, sentrux) emit call/dep graphs but stop there. `hari_code_relations` is the smallest bridge — it converts a JSON edge list into `RelationDeclaration` events the BeliefNetwork can propagate along:

```pwsh
hari_code_relations --graph call-graph.json --source ix-code-analyze \
  > /tmp/rels.jsonl

hari_harness --events /tmp/rels.jsonl --state-dir state/code
```

After replay, if `fn:MercuryClient::chat` becomes `False`, Hari auto-derives `Doubtful` for `fn:LlmMusicalQueryExtractor::extract` via the declared `Implies` relation.

## cargo-test ingestion (Tier-2 use case)

The harness now consumes libtest JSON directly:

```pwsh
cargo test --release -- -Z unstable-options --format json --report-time 2> /dev/null \
  | Tee-Object -FilePath state/cargo-test.jsonl > $null

hari_harness --state-dir state/harness --cargo-test state/cargo-test.jsonl --source rust-ci
```

Each `{ "type":"test", "event":"ok"|"failed", "name": ... }` becomes one `ExperimentResult`. Multiple `True`s on the same test → consolidation. A `False` after a string of `True`s on `test/foo-passes` is a flaky-test signal.

## What's still design-only

- **`hari-core serve` integration with the harness.** The Phase-6 streaming protocol exists; wiring the harness to push events into a long-running session (instead of one-shot CLI invocations) is the natural follow-up but a different shape. Tracked as M2 of `docs/design/2026-05-13-hari-qa-tribunal-substrate-plan.md`.
- **Demerzel governance hook.** Cross-repo coordination required. Tracked as M3 of the tribunal plan.
- **Real `ix-code-analyze` → `hari_code_relations` integration.** Today the graph JSON has to be hand-shaped to match the `{ edges: [...] }` schema; an `ix-code-analyze --emit-hari-graph` flag is the obvious next move on the ix side.
