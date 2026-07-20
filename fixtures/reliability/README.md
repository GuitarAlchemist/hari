# `fixtures/reliability/` — cross-session source-reliability examples (issue #14)

Example artifacts for the source-reliability tracer bullet
(`crates/hari-core/src/source_reliability.rs`). Design:
`docs/research/2026-07-20-source-reliability-design.md`; contract:
`docs/contracts/source-reliability-summary.contract.md` (v0.1 DRAFT).

These are **illustrative**, not test oracles — the tracer's assertions live in
`source_reliability.rs` unit tests and `tests/source_reliability_e2e.rs`. Both
files here are the verbatim output of replaying
`fixtures/ix/conflicting_benchmark.json`:

```bash
# Derive per-source outcome rows and append them to the ledger.
HARI_STATE_DIR=/tmp/hari-state \
  cargo run -p hari-core -- source-reliability emit \
    --trace fixtures/ix/conflicting_benchmark.json \
    --session e2e-demo --now 2026-07-20T12:00:00Z

# Read-only aggregate across every session in the ledger.
HARI_STATE_DIR=/tmp/hari-state \
  cargo run -p hari-core -- source-reliability report --now 2026-07-20T18:00:00Z
```

- **`example-outcomes.jsonl`** — the two per-source rows `emit` appends to
  `HARI_STATE_DIR/source-reliability/2026-07-20.jsonl` (one JSON object per
  line; the ledger's on-disk shape).
- **`example-report.json`** — the aggregated `report` over those rows.

The story the numbers tell: `ix-agent-evaluator` claimed
`benchmark-x-is-reliable = Probable`, that claim was Accepted and then
**retracted** → one false acceptance (precision `0.0`). `ix-agent-critic` raised
the contradicting `Doubtful`, which escalated; the belief never resolved to
True/Probable, so the alarm was **warranted** (`false_escalations: 0`). Trust is
earned by outcomes, not by role.
