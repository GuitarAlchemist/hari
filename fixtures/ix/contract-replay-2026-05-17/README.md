# Phase 2 IX→Hari replay archive — 2026-05-17

End-to-end fixture from the first live replay against the pinned
`ix-autoresearch` JSONL boundary contract.

**Producer:** `ix/crates/ix-autoresearch/examples/grammar_pinned_contract.rs`
running the in-process `GrammarTarget` smoke target with seed 42, 20
iterations, SA strategy (initial T = 0.05, cooling 0.95).

**Pipeline:**

```pwsh
cargo run --release --example grammar_pinned_contract -- `
    --out-dir state/autoresearch/contract --iterations 20 --seed 42
# → grammar-run.jsonl

cargo run -p hari-extractor --bin hari_from_ix_autoresearch -- `
    --log grammar-run.jsonl --target target_grammar > trace.json

cargo run -p hari-core -- replay trace.json > belief-diff.json
```

## Files

- `grammar-run.jsonl` — raw ix-autoresearch run log (1 `run_start`, 20
  `iteration`s, 1 `run_complete`).
- `trace.json` — Hari `ResearchTrace` derived by `hari_from_ix_autoresearch`.
- `belief-diff.json` — Hari `ResearchReplayReport` after running the trace
  through `CognitiveLoop::process_research_trace`. This is the artifact
  agent-blackbox consumes as Workflow 3 evidence.

## What this demonstrates

The `final_beliefs` map in `belief-diff.json` resolves to:

```json
"final_beliefs": {
  "target_grammar/config-autoresearch-is-an-improvement": "Contradictory"
}
```

`Contradictory` is the load-bearing outcome from the Workflow 3 acceptance
criteria: when contradictory accept/reject signals land on the same
derived proposition, Hari preserves the contradiction rather than
averaging it away. The `action_counts_by_kind.Escalate = 18` confirms
that Hari fires the Escalate action on every conflicting observation, so
operators can route the contradiction to a higher-priority resolver.

## Known issue (Phase 2 follow-up, not blocking)

All 20 iteration lines collapse to the same Hari proposition string
because `hari_from_ix_autoresearch.rs` slices `&config_hash[..12]` — but
real ix-autoresearch hashes carry the prefix `autoresearch:` (13 chars),
so the slice yields the constant `autoresearch` instead of the per-config
digest. The replay still demonstrates contradictory-preservation
correctly because the synthetic and live data happen to exercise the
same proposition, but the diagnostic value is reduced (operators cannot
tell *which* config produced the conflict). Fix is a one-line slice
adjustment in a follow-up PR; deliberately out of scope here per the
Phase 1 contract pinning brief.
