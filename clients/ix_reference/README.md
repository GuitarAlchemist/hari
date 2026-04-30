# IX reference client

A minimal Python client for Hari's Phase 6 stdio-JSONL streaming protocol.
Closes the "IX-side reference client" gap noted in `ROADMAP.md` Phase 6.

The protocol itself is language-agnostic plain JSON (see
`docs/research/phase6-design.md` §3); this directory just demonstrates one
end of it. An IX maintainer can copy `hari_client.py` into their own
codebase and use it directly — it has no third-party dependencies and
fits in a single file.

## Files

| File             | Role                                                           |
|------------------|----------------------------------------------------------------|
| `hari_client.py` | `HariSession` context manager + typed `HariProtocolError`.     |
| `run_session.py` | CLI driver: streams a `ResearchTrace` file through `serve`.    |

Both files are stdlib-only and target Python 3.9+.

## Quickstart

```bash
# 1. Build the binary.
cargo build -p hari-core

# 2. Stream a fixture through the streaming protocol.
python clients/ix_reference/run_session.py fixtures/ix/cognition_divergence.json

# 3. Same fixture, but with a Lie shadow running in lockstep.
python clients/ix_reference/run_session.py fixtures/ix/cognition_divergence.json \
    --compare-with Lie

# 4. Or run with Lie as the primary and Flat shadow.
python clients/ix_reference/run_session.py fixtures/ix/cognition_divergence.json \
    --priority-model Lie --compare-with Flat
```

`run_session.py` discovers `target/release/hari-core` or `target/debug/hari-core`
relative to the repo root. Use `--binary <path>` to override.

## Protocol coverage

`run_session.py` exercises every operation in the protocol:

| Op         | When                                       |
|------------|--------------------------------------------|
| `open`     | Once, with `dimension` + `priority_model` (and optional `compare_with`). |
| `event`    | Once per event in the trace file.          |
| `metrics`  | Once mid-stream as a demo snapshot.        |
| `close`    | Once at the end; returns the final report. |

The `HariSession` class also handles the typed error envelope
(`Response::Error → HariProtocolError`), so non-fatal protocol violations
(e.g. `out_of_order_cycle`) propagate as Python exceptions without
killing the session.

## What this client is NOT

This is a **reference**, not a real IX integration. It does not generate
hypotheses, run experiments, or evaluate Hari's recommendations against
ground truth — that is what an actual IX-side autoresearch loop driving
real benchmarks would do, and is the remaining ⏸ item under Phase 6 in
`ROADMAP.md`. The point of this directory is solely to prove the Hari
side of the protocol works end-to-end from outside the Rust workspace,
in a language an IX maintainer is likely to use.
