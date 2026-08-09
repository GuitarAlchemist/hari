# IX reference client

A minimal Python client for Hari's Phase 6 stdio-JSONL streaming protocol.
Closes the "IX-side reference client" gap noted in `ROADMAP.md` Phase 6.

The protocol itself is language-agnostic plain JSON (see
`docs/research/phase6-design.md` §3); this directory just demonstrates one
end of it. An IX maintainer can copy `hari_client.py` into their own
codebase and use it directly — it has no third-party dependencies and
fits in a single file.

## Files

| File                | Role                                                        |
|---------------------|-------------------------------------------------------------|
| `hari_client.py`    | `HariSession` context manager + typed `HariProtocolError`.  |
| `run_session.py`    | CLI driver: streams a `ResearchTrace` file through `serve`. |
| `paired_driver.py`  | #35 paired-task driver: records a corpus and produces the roll-up in one command. |

All three files are stdlib-only and target Python 3.9+.

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

## The paired-task driver (#35 §9 item 4, user story 12)

```bash
cargo build --release -p hari-core

# Record a corpus and produce the roll-up + §8 verdict in ONE command.
python clients/ix_reference/paired_driver.py --out target/ix-driver

# Same seed must give byte-identical traces.
python clients/ix_reference/paired_driver.py --check-determinism
```

"Hari-on and Hari-off" is one pass, not two runs: `replay --paired --compare3
--bootstrap` already replays each recorded trace under `ix_unassisted` (off) and
`recency_decay` / `lie` / `subjective_logic` (on). Pairing requires one
recording — two live runs would destroy it.

Every fixture the driver writes carries a `provenance` stamp: the driver name,
the generative spec id, the seed, and a digest of the trace. `hari-core`
recomputes that digest and refuses a fixture whose trace was edited afterwards,
so `--corpus recorded` is read off the corpus rather than asserted on the command
line (pre-registration §9.4, §9.9).

**The driver's `SPEC` is a candidate distribution, not a ratified one.** Choosing
the task distribution is the outstanding owner call recorded in §9.4, and §9.9
records why no §8 verdict may yet rest on the corpus this driver produces.

## What this client is NOT

This is a **reference**, not a real IX integration. `run_session.py` does not
generate hypotheses or run experiments, and `paired_driver.py` samples a declared
distribution rather than recording a live autoresearch session — that is what an
actual IX-side loop driving real benchmarks would do, and is the remaining ⏸ item
under Phase 6 in `ROADMAP.md`. The point of this directory is to prove the Hari
side of the protocol and the eval boundary work end-to-end from outside the Rust
workspace, in a language an IX maintainer is likely to use.
