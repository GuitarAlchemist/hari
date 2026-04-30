# Phase 6 Design — IX Autoresearch Integration

**Status**: Design proposal. Not implemented. Targets the workflow described in `ROADMAP.md` Phase 6 and builds on Phase 5's `process_research_event` / `compare_replay` primitives.

**Author scope**: Design-only. Code snippets are illustrative.

## 1. TL;DR

- **Transport**: **stdio JSONL service** as the recommended default. One Hari subprocess per session; IX writes `Request` JSONL lines to stdin, reads `Response` JSONL lines from stdout. Library API kept as a first-class Rust-side option (no work needed; it already exists). HTTP deferred until a multi-host requirement actually appears.
- **Session shape**: explicit `open` → many `event` → optional `metrics` polls → `close`, with one response per request. State is in-memory; `close` returns final `ReplayMetrics`.
- **Replay parity**: every session writes a `ResearchTrace`-superset trace file (events plus `SessionConfig` header) on `open`; `replay --session <file>` re-runs the exact stream byte-for-byte through the same `process_research_event` codepath.
- **Reuse**: streaming uses `process_research_event` unchanged. The streaming layer is a thin protocol skin; no parallel cognitive codepath. The Phase 5 long-fixture finding (Lie wins `false_acceptance` 3 vs 4 on `long_recovery.json`) is preserved because the same scoring path runs.
- **Out of scope**: auth, multi-tenant, perf at scale, persistent stores, hot-reload of `SessionConfig`.

## 2. Transport recommendation

### Picked: stdio JSONL service

A long-running `hari-core serve` subprocess. IX spawns it (or attaches via a wrapper), writes one JSON object per line on stdin, reads one JSON object per line on stdout. Errors go to stderr (operational logs) or to stdout as a typed `error` response (protocol errors). No framing beyond `\n`.

**Why this fits Hari specifically**:

1. **Replay parity is trivial.** Stdin is already a stream of JSONL events. Tee it to a file and you have a replayable trace. The existing `replay <path>` already consumes a `Vec<ResearchEvent>` from a file; teaching it to also consume the streaming envelope is a small extension.
2. **No new dependencies.** No `tokio`, no `tonic`, no `axum`. Hari is already pre-PoC research code (see `CLAUDE.md`); pulling in an async runtime and HTTP stack would dwarf the rest of the workspace.
3. **Language-agnostic on the IX side.** IX is Python-leaning per the roadmap; subprocess + line-IO is the lowest-friction shape across Python, TS, and Rust.
4. **Determinism.** A single in-process `CognitiveLoop` per session gives bit-identical replay. No request reordering, no retry-induced double-application.
5. **Honest about scale.** Phase 5 ran D=4, ≤22 events. There is no evidence Hari needs distributed transport; building HTTP/gRPC now would be premature ceremony.

### Library API (Rust-only): kept as a first-class alternative

Already exists — IX-in-Rust just adds `hari-core = { path = "..." }` and calls `CognitiveLoop::process_research_event` directly. **Recommended for the in-tree integration test** (`tests/ix_integration.rs`-style) and for any future Rust-native autoresearch driver. Phase 6 should ensure the public API surface is stable enough to depend on without re-exporting internals.

Tradeoff: couples the consumer to Rust and to Hari's internal type churn. Fine for tests; not fine as the IX↔Hari boundary.

### HTTP/gRPC: explicitly deferred

Tradeoffs that make this wrong-now:

- Adds tokio + an HTTP framework + serialization codegen (gRPC) for a system that today processes dozens of events per session.
- Replay parity requires either request logging middleware or a separate trace recorder — extra surface area for bugs.
- Multi-host isn't a requirement — the IX↔Hari loop is one researcher driving one substrate.
- Auth/TLS/CORS surface area that the project explicitly doesn't want yet.

**Reconsider when**: IX is itself distributed (multiple hypothesis-generators feeding one Hari), OR Hari needs to be embedded behind a service mesh, OR Phase 6 evaluation reveals stdio framing limits.

## 3. Session protocol

### Lifecycle

```
IX                                    Hari (hari-core serve)
 |                                          |
 |  {"op":"open", config:{...}} ----------> |
 |  <----------- {"op":"opened", session_id, trace_path}
 |                                          |
 |  {"op":"event", event:{cycle,source,payload}} -->
 |  <----------- {"op":"recommendation", actions:[...], state_summary, running_metrics}
 |                                          |
 |  ... many event/recommendation pairs ... |
 |                                          |
 |  {"op":"metrics"} ----------------------> |
 |  <----------- {"op":"metrics_snapshot", metrics:{...}, beliefs:{...}}
 |                                          |
 |  {"op":"close"} -------------------------> |
 |  <----------- {"op":"closed", final_report:{ResearchReplayReport}}
 |                                          |
 |  EOF on stdin (or explicit "shutdown") -> process exits
```

One request, one response. Strictly synchronous from IX's point of view. No server-pushed messages. This keeps the protocol replayable from the request stream alone.

### Message shapes (illustrative)

**Open**:
```json
{
  "op": "open",
  "config": {
    "dimension": 4,
    "priority_model": "Lie",
    "lie_alpha": 2.0,
    "lie_dt": 0.5,
    "theta_wait": 0.1,
    "lambda_decay": 0.2,
    "compare_with": null,
    "trace_record_path": "traces/session-2026-04-29-abc.jsonl"
  }
}
```

**Opened**:
```json
{
  "op": "opened",
  "session_id": "abc-123",
  "trace_path": "traces/session-2026-04-29-abc.jsonl",
  "hari_version": "0.1.0",
  "config_echo": { "...": "the config Hari actually applied, post-defaults" }
}
```

**Event** (just wraps the existing `ResearchEvent`):
```json
{
  "op": "event",
  "event": {
    "cycle": 4,
    "source": "ix-agent-critic",
    "payload": {
      "type": "experiment_result",
      "proposition": "beta-tool-stable",
      "value": "Doubtful",
      "evidence": { "runs": 3, "variance": 0.18 }
    }
  }
}
```

**Recommendation** (per-event response):
```json
{
  "op": "recommendation",
  "event_index": 4,
  "actions": [
    { "Accept": { "proposition": "beta-tool-stable", "value": "Doubtful" } },
    { "Log": "Recorded 3 experiment fields for 'beta-tool-stable'" }
  ],
  "state_summary": "Cycle 4: 2 beliefs, 2/2 goals active, attention norm=1.046",
  "running_metrics": {
    "events_processed": 4,
    "false_acceptance_count": 0,
    "attention_norm_max": 1.311,
    "action_counts_by_kind": { "Accept": 2, "Log": 4, "Investigate": 1 }
  },
  "compare": null
}
```

If `compare_with` was set on `open`, `compare` is populated:
```json
"compare": {
  "shadow_model": "RecencyDecay",
  "shadow_actions": [
    { "Investigate": { "topic": "beta-tool-stable" } }
  ],
  "diverged": true
}
```

**Metrics poll** (optional, on demand):
```json
{ "op": "metrics" }
```
Returns `ReplayMetrics` plus current beliefs and goals. Cheap; can be called as often as IX wants.

**Close**:
```json
{ "op": "close" }
```
Returns the full `ResearchReplayReport` (same shape `replay` produces today).

**Error** (Hari → IX, instead of `recommendation` when something is wrong):
```json
{
  "op": "error",
  "request_op": "event",
  "code": "out_of_order_cycle",
  "message": "event cycle=2 < last seen cycle=5; cycle must be monotonic non-decreasing",
  "fatal": false
}
```

### Why request/response, not bidirectional streaming

Two reasons. First, the cognitive loop is fundamentally **sequential** — each event mutates state that the next event depends on. Pipelining requests would just queue up at the loop boundary. Second, sequential request/response means the trace recording on disk is **identical** to the wire protocol in order, so `replay` can simply read the recorded file line-by-line. Bidirectional streaming would force a separate trace format.

## 4. Configuration surface

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    // --- Cognitive loop construction (per-session, immutable after open) ---
    pub dimension: usize,                 // default 4
    pub priority_model: PriorityModel,    // default Flat
    pub theta_wait: f64,                  // default 0.1
    pub lie_dt: f64,                      // default 0.5
    pub lie_alpha: f64,                   // default 2.0
    pub lambda_decay: f64,                // default 0.2

    // --- Comparison shadow (optional) ---
    /// If set, every event is also processed through a shadow loop with this
    /// model. The recommendation response includes a `compare` block.
    pub compare_with: Option<PriorityModel>,

    // --- Trace recording (optional, recommended) ---
    /// If set, every incoming request is appended as JSONL to this path.
    /// Replay parity requires this; defaulting to None makes ad-hoc sessions
    /// non-reproducible by design (caller opt-in).
    pub trace_record_path: Option<PathBuf>,

    // --- Initial state (optional) ---
    /// Goals to seed before the first event. Equivalent to sending
    /// goal_update events at cycle 0.
    pub initial_goals: Vec<InitialGoal>,
}
```

**Per-session**: everything above. Immutable after `open` — changing `lie_alpha` mid-session would invalidate replay determinism.

**Global / process-level** (CLI flags on `hari-core serve`): logging level, max in-flight sessions (capped at 1 for now), stdin/stdout encoding (utf-8 only). Nothing cognitive.

**Why `compare_with` is per-session not per-event**: the shadow loop has its own attention vector that depends on the entire history. Toggling it on/off mid-session would produce non-reproducible shadow decisions.

## 5. Trace recording for replay parity

The hard constraint from `ROADMAP.md` Guiding Principles: "Prefer replayable scenarios over live demos." A streaming session must be byte-for-byte reproducible.

### Recording

When `trace_record_path` is set on `open`, Hari writes a JSONL file with one record per inbound request:

```
{"op":"open","config":{...},"timestamp":"..."}
{"op":"event","event":{...}}
{"op":"event","event":{...}}
{"op":"metrics"}
{"op":"event","event":{...}}
{"op":"close"}
```

The recorded file is a strict superset of `ResearchTrace`: the `open` line carries `dimension` + priority model + α/dt/θ in `config`; the `event` lines unwrap to `ResearchEvent`s; `metrics`/`close` lines are no-ops on replay (they didn't change loop state, only observed it).

### Replay

Extend `hari-core replay` with a `--session <file>` mode:

```
cargo run -p hari-core --release -- replay --session traces/session-abc.jsonl
```

Replay semantics:

1. Read the first line; require `op == "open"`. Construct a `CognitiveLoop` from `config` using the same field-by-field path the live `serve` mode uses.
2. For each subsequent `event` line, call `process_research_event` and capture the resulting `ResearchEventOutcome`.
3. Skip `metrics` lines (observation-only).
4. On `close`, produce a `ResearchReplayReport` and emit it to stdout — same format as today's `replay` output.
5. If a `compare_with` was in the config, run the shadow loop in parallel (mirroring Phase 5's `compare_replay`).

**Determinism contract**: a session recorded today and replayed tomorrow produces the same `ResearchReplayReport.outcomes` and `metrics`. The check is a regression test that records a synthetic session, replays it, and asserts byte-equality on the report (modulo timestamps, which are excluded from the report).

**What replay does NOT cover**: the `running_metrics` snapshots returned to IX during the live session. Those are derivable from the outcome list in the final report; mid-stream snapshots can be reconstructed by replaying up to event N. There's no need to record them.

## 6. Failure modes & error contracts

| Scenario | Behavior |
|---|---|
| Malformed JSON on stdin | Respond with `{"op":"error", "code":"invalid_json", "fatal":false}`. Skip the line; continue. Do **not** advance cycle. |
| Unknown `op` | `{"code":"unknown_op", "fatal":false}`. No state change. |
| `event` before `open` | `{"code":"no_session", "fatal":false}`. Reject. |
| Two `open`s in one connection | `{"code":"already_open", "fatal":false}`. The first session continues; the second is ignored. (Multi-session is out of scope.) |
| Event `cycle` non-monotonic (less than last seen) | `{"code":"out_of_order_cycle", "fatal":false}`. Reject the event. The loop's behavior depends on cycle ordering for Phase 5's `RecencyDecay` and `Lie` decay/projection logic — out-of-order events would silently corrupt metrics. |
| Event `cycle` repeats (equal to last seen) | Allowed. Multiple events per cycle is a real IX pattern (e.g., `goal_update` then `belief_update` at cycle 1). Both fixtures already do this. |
| Unknown `payload.type` | Serde rejects on deserialize → `invalid_event` error, non-fatal. |
| `HexValue` deserialization fails | Same. |
| IX disconnects mid-session (EOF on stdin) | Hari finalizes: writes a `close` record to the trace file with an `unclean: true` marker, computes final metrics, emits a final report on stdout, then exits. The trace file stays replayable — `replay --session` synthesizes the same outcome list up to the last applied event. |
| Hari panic mid-session | `tracing::error!` to stderr; exit non-zero. The trace file is intact up to the last fully-flushed event (recommend `BufWriter` with explicit flush after each line). IX can replay to inspect. |
| Trace file write fails (disk full, permissions) | Treat as fatal: `{"code":"trace_io", "fatal":true}` and exit. Replay parity is the value proposition; if we can't record, we shouldn't pretend we're running. |
| Out-of-order within same cycle (e.g., `belief_update` for prop A then `goal_update` for prop B) | Allowed and expected. Within-cycle ordering is significant and replayed exactly as recorded. |
| `close` followed by another `event` | `{"code":"session_closed", "fatal":false}`. Ignore. |

**Error envelope is non-exception-driven**: errors are JSON responses, not protocol-level exceptions. IX never has to parse stderr to recover.

## 7. Reuse vs divergence from existing primitives

### Reuse (no changes)
- `CognitiveLoop::process_research_event` — unchanged. Streaming is a thin envelope; the cognitive path is identical to what `replay` calls today. This is critical for preserving the Phase 5 long-fixture finding (`false_acceptance` 3 vs 4 on `long_recovery.json`) — same code, same answer.
- `Action`, `ResearchEvent`, `ResearchEventPayload`, `ResearchEventOutcome`, `ReplayMetrics`, `ReplayComparison` — unchanged.
- `compute_metrics` — unchanged. Called once at session close on the accumulated `outcomes` list, just like batch replay.

### Light additions (new code, no semantic divergence)
- `pub struct SessionConfig` (shape in §4).
- `pub struct StreamingSession { loop_: CognitiveLoop, shadow: Option<CognitiveLoop>, outcomes: Vec<ResearchEventOutcome>, recorder: Option<TraceRecorder>, last_cycle: Option<u64> }`.
- `impl StreamingSession::handle_request(&mut self, req: Request) -> Response` — pure dispatcher; no cognitive logic of its own.
- `running_metrics()` — a cheap variant of `compute_metrics` that takes the accumulated `outcomes` so far and returns the same struct. Same code path, just called incrementally. Mid-stream values like `contradiction_recovery_cycles` may be `None` until recovery happens; that's correct, not a bug.
- `hari-core serve` binary mode — new entry point in `main.rs` alongside the existing `replay` mode.
- `hari-core replay --session <file>` — new flag on the existing `replay` subcommand.

### Streaming analog of `--compare`

`SessionConfig::compare_with` runs a **shadow loop** in lockstep with the primary. Every `event` request applies the event to both loops; the response includes the shadow's actions in `compare`. Cost: 2× the cognitive work per event, which is irrelevant at Phase 6's scale. Replay parity holds because both loops are deterministic on the same event stream and the trace records only the inbound events, not the shadow output.

The existing `compare_replay` function stays the batch entry point; it's a 4-line wrapper around `StreamingSession` if we want the implementations to literally share code, or it stays standalone if we don't. The test-equivalence assertion (running the same trace through batch `compare_replay` and through a streaming session must produce identical `ReplayComparison`) is the regression check.

## 8. Out of scope (explicit)

Phase 6 deliberately does NOT address:

- **Authentication / authorization.** Single-tenant research code. Add when a second tenant exists.
- **Multi-session concurrency.** One `serve` process serves one session. Spawn more processes if you need more sessions.
- **Performance tuning.** D=4, dozens of events, single-threaded. No throughput targets.
- **Persistent storage.** Sessions are ephemeral; trace files on disk are the only persistence. No database.
- **Hot config reload.** `SessionConfig` is fixed at `open` time. Restart the session to change it.
- **Bidirectional streaming / server-pushed events.** Strict request/response. Hari does not initiate.
- **gRPC / Protobuf schemas.** JSON only. Reconsider when there's a polyglot consumer that needs strong typing.
- **Distributed Hari.** No clustering, no sharding, no replication. Hari is one process.
- **Live tuning of `lie_alpha`/`lie_dt`.** Per Phase 5 §7a: the current α=2.0/dt=0.5 defaults are pinned by `divergence_test_pins_alpha_and_dt`. Mid-session changes would silently shift divergence behavior.
- **GA integration.** GA's role is per-roadmap "scenario generator / domain oracle / external evaluator" — outside the IX↔Hari loop.

## 9. Implementation plan sketch

A future agent should be able to execute Phase 6 in roughly this order. Each step is independently testable.

1. **Add `SessionConfig` to `hari-core/src/lib.rs`** with the shape from §4. Derive `Serialize`/`Deserialize`. Provide `Default` matching the current `CognitiveLoop::new(4)` field values. Expose a constructor `CognitiveLoop::from_config(&SessionConfig) -> Self` that wires `priority_model`, `theta_wait`, `lie_dt`, `lie_alpha`, `lambda_decay`, and seeds `initial_goals`. Test: round-trip serde, plus a unit test asserting `CognitiveLoop::from_config(&SessionConfig::default())` matches `CognitiveLoop::new(4)` field-by-field.

2. **Define the protocol envelope types** in a new `crates/hari-core/src/protocol.rs`. `Request` and `Response` enums tagged on `op`. Keep `ResearchEvent` as the inner type for the `event` variant — do not duplicate it. Test: serde round-trip for each variant against hand-written JSON fixtures matching §3.

3. **Implement `StreamingSession`** in `crates/hari-core/src/session.rs`. Holds the primary `CognitiveLoop`, optional shadow, accumulated outcomes, optional `TraceRecorder`, and `last_cycle` for monotonicity checks. Single public method `handle_request(&mut self, req: Request) -> Response`. All cognitive work delegates to `process_research_event`. Test: replay the existing `cognition_divergence.json` and `long_recovery.json` fixtures by feeding events one at a time; assert the final `ReplayReport` matches what `process_research_trace` produces in batch.

4. **Implement `TraceRecorder`** — thin `BufWriter` wrapper that appends one JSON line per inbound request and flushes after each line. Test: record a synthetic session, read the file back, assert request lines match what was sent.

5. **Add `hari-core serve` binary mode** in `main.rs`. Reads JSONL from stdin, writes JSONL to stdout, dispatches to `StreamingSession`. Errors as JSON responses (not panics). Test (integration): pipe a fixture trace through `serve` and assert stdout matches expected recommendations.

6. **Add `replay --session <file>`** to the existing `replay` subcommand. Reads a recorded session file, replays via `StreamingSession`, emits `ResearchReplayReport`. Test: record a session via `serve`, replay it via `replay --session`, assert byte-equality on the report (excluding any timestamp fields if present).

7. **Add the streaming-vs-batch equivalence regression test.** Run `compare_replay` on a fixture in batch mode, run the same fixture through `StreamingSession` with `compare_with` set, assert the resulting `ReplayComparison` is identical. This is the "no parallel codepath" guarantee in §7 made concrete.

8. **Document the `serve` mode in `README.md` and `CLAUDE.md`.** Two new commands. Update the architecture section in `CLAUDE.md` with a one-line note about `StreamingSession` being a thin wrapper, not a parallel cognitive path.

### Critical files (touched / created)

- `crates/hari-core/src/lib.rs` — add `SessionConfig`, `from_config`.
- `crates/hari-core/src/protocol.rs` — **new**, envelope types.
- `crates/hari-core/src/session.rs` — **new**, `StreamingSession`, `TraceRecorder`.
- `crates/hari-core/src/main.rs` — add `serve` mode, `replay --session` flag.
- `crates/hari-core/tests/streaming_equivalence.rs` — **new**, the equivalence regression.
- `README.md`, `CLAUDE.md` — doc updates.
- `fixtures/ix/sessions/` — **new directory** for recorded session fixtures (for replay tests).

No changes required in `hari-lattice`, `hari-cognition`, or `hari-swarm`.
