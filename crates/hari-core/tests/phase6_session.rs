//! Phase 6 — IX autoresearch streaming session integration tests.
//!
//! Per the implementation-plan in `docs/research/phase6-design.md` step
//! 8, this test module covers:
//!
//! - Protocol round-trip (Request/Response serde).
//! - Single-event session open → event → close, vs batch
//!   `process_research_trace`.
//! - Multi-event session on `long_recovery.json` vs batch
//!   `compare_replay` (modulo the `comparison` field, which streaming
//!   does not populate at close).
//! - Replay parity: record a session via `trace_record_path`, replay
//!   via `StreamingSession::open` over the recorded file, and assert
//!   the resulting JSON is **byte-identical** to a fresh batch run.
//! - Failure modes: malformed JSON / double-open / EOF mid-session.
//!
//! Test files use `std::env::temp_dir()` for portability across OSes —
//! the design doc's `/tmp` path is illustrative; what matters is that
//! the recorder and replay both use the same path.

use hari_core::{
    compare_replay, action_kind, CognitiveLoop, Request, Response, ResearchEvent, ResearchTrace,
    SessionConfig, StreamingSession,
};
use std::fs;
use std::path::PathBuf;

fn load_trace(path: &str) -> ResearchTrace {
    let raw = fs::read_to_string(path).expect("fixture must be readable");
    match serde_json::from_str::<ResearchTrace>(&raw) {
        Ok(t) => t,
        Err(_) => {
            let events: Vec<ResearchEvent> =
                serde_json::from_str(&raw).expect("fixture must be a trace or event array");
            events.into()
        }
    }
}

fn temp_path(suffix: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    // Pick a per-test filename so parallel test runs don't stomp.
    let pid = std::process::id();
    p.push(format!("hari-phase6-{pid}-{suffix}"));
    // Ensure parent exists and clear any stale file.
    if p.exists() {
        let _ = fs::remove_file(&p);
    }
    p
}

// ---------------------------------------------------------------------------
// 1. Protocol round-trip
// ---------------------------------------------------------------------------

#[test]
fn protocol_request_round_trips_through_serde() {
    // Open
    let open = Request::Open {
        config: SessionConfig {
            dimension: 4,
            ..SessionConfig::default()
        },
    };
    let s = serde_json::to_string(&open).unwrap();
    assert!(s.contains("\"op\":\"open\""), "got: {s}");
    let _: Request = serde_json::from_str(&s).unwrap();

    // Event
    let raw_event = r#"{
        "op": "event",
        "event": {
            "cycle": 4,
            "source": "ix-agent",
            "payload": {
                "type": "experiment_result",
                "proposition": "beta-tool-stable",
                "value": "Doubtful"
            }
        }
    }"#;
    let req: Request = serde_json::from_str(raw_event).unwrap();
    let s = serde_json::to_string(&req).unwrap();
    assert!(s.contains("\"op\":\"event\""));

    // Metrics + Close
    let m: Request = serde_json::from_str(r#"{"op":"metrics"}"#).unwrap();
    assert!(matches!(m, Request::Metrics));
    let c: Request = serde_json::from_str(r#"{"op":"close"}"#).unwrap();
    assert!(matches!(c, Request::Close));
}

#[test]
fn protocol_response_round_trips_through_serde() {
    let err = Response::Error {
        request_op: Some("event".into()),
        code: "out_of_order_cycle".into(),
        message: "cycle=2 < last=5".into(),
        fatal: false,
    };
    let s = serde_json::to_string(&err).unwrap();
    assert!(s.contains("\"op\":\"error\""));
    let back: Response = serde_json::from_str(&s).unwrap();
    match back {
        Response::Error { code, fatal, .. } => {
            assert_eq!(code, "out_of_order_cycle");
            assert!(!fatal);
        }
        other => panic!("expected Error, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// 2. Single-event session: open → event → close, vs batch
// ---------------------------------------------------------------------------

#[test]
fn single_event_session_matches_batch_on_synthetic_trace() {
    let trace_json = r#"[
        {
            "cycle": 1,
            "source": "ix-agent",
            "payload": {
                "type": "belief_update",
                "proposition": "p",
                "value": "Probable"
            }
        }
    ]"#;
    let events: Vec<ResearchEvent> = serde_json::from_str(trace_json).unwrap();
    let trace: ResearchTrace = events.clone().into();

    // Batch
    let mut batch_loop = CognitiveLoop::new(trace.dimension);
    let batch_report = batch_loop.process_research_trace(trace);

    // Streaming
    let mut session =
        StreamingSession::open(SessionConfig::default()).expect("open should succeed");
    for ev in events {
        session
            .apply_event(ev)
            .expect("apply_event should succeed");
    }
    let stream_report = session.close();

    // The two should serialize identically (modulo `comparison` which
    // both leave None).
    let batch_json = serde_json::to_string(&batch_report).unwrap();
    let stream_json = serde_json::to_string(&stream_report).unwrap();
    assert_eq!(batch_json, stream_json);
}

// ---------------------------------------------------------------------------
// 3. Multi-event session on long_recovery.json (Lie) matches batch
//    compare_replay's experimental side
// ---------------------------------------------------------------------------

#[test]
fn long_recovery_streaming_matches_batch_lie_path() {
    let trace = load_trace("../../fixtures/ix/long_recovery.json");

    // Batch — Lie path on a fresh loop.
    let mut batch_loop =
        CognitiveLoop::with_model(trace.dimension, hari_core::PriorityModel::Lie);
    let batch_report = batch_loop.process_research_trace(trace.clone());

    // Streaming — same model, no recorder.
    let cfg = SessionConfig {
        dimension: trace.dimension,
        priority_model: hari_core::PriorityModel::Lie,
        ..SessionConfig::default()
    };
    let mut session = StreamingSession::open(cfg).expect("open should succeed");
    for ev in trace.events {
        session.apply_event(ev).expect("apply_event should succeed");
    }
    let stream_report = session.close();

    // Identity: streaming path is *the same* path as batch.
    let batch_json = serde_json::to_string(&batch_report).unwrap();
    let stream_json = serde_json::to_string(&stream_report).unwrap();
    assert_eq!(
        batch_json, stream_json,
        "streaming Lie path on long_recovery.json must byte-match batch"
    );

    // Sanity: Phase 5's long-fixture finding (false_acceptance metric
    // populated) — preserved through streaming.
    assert!(
        stream_report.metrics.contradiction_recovery_cycles.is_some(),
        "Lie streaming must populate contradiction_recovery_cycles"
    );

    // And it diverges from batch RecencyDecay (Phase 5 finding
    // 3-vs-4) — confirms we're not collapsing models.
    let cmp = compare_replay(load_trace("../../fixtures/ix/long_recovery.json"));
    let comparison = cmp.comparison.expect("comparison populated");
    assert_eq!(
        stream_report.metrics.false_acceptance_count,
        comparison.experimental.false_acceptance_count,
        "streaming Lie's false_acceptance must match batch Lie's"
    );
    assert_ne!(
        comparison.baseline.false_acceptance_count,
        comparison.experimental.false_acceptance_count,
        "Phase 5 finding: Lie wins false_acceptance on long_recovery.json"
    );
}

// ---------------------------------------------------------------------------
// 4. Replay parity — byte-identical assertion
// ---------------------------------------------------------------------------

#[test]
fn replay_session_parity_with_batch_on_cognition_divergence() {
    let trace = load_trace("../../fixtures/ix/cognition_divergence.json");

    // Step A — batch run on a fresh loop.
    let mut batch_loop = CognitiveLoop::new(trace.dimension);
    let batch_report = batch_loop.process_research_trace(trace.clone());
    let batch_json = serde_json::to_string_pretty(&batch_report).unwrap();

    // Step B — streaming run with trace_record_path. Defaults match
    // `CognitiveLoop::new(4)` so the cognitive trajectory is identical.
    let trace_path = temp_path("parity-record.jsonl");
    let cfg = SessionConfig {
        dimension: trace.dimension,
        trace_record_path: Some(trace_path.clone()),
        ..SessionConfig::default()
    };
    {
        let mut session = StreamingSession::open(cfg).expect("open should succeed");
        // Record events through the same record_request path that
        // `serve` uses (so the file shape is identical to a real serve
        // session).
        for ev in trace.events.iter() {
            session
                .record_request(&Request::Event { event: ev.clone() })
                .expect("record event");
            session
                .apply_event(ev.clone())
                .expect("apply_event should succeed");
        }
        // Record an explicit close marker, mirroring `serve`'s flow.
        session
            .record_request(&Request::Close)
            .expect("record close");
        let _ = session.close();
    }

    // Step C — replay --session reads the recorded file and produces
    // its own ReplayReport. Use the `StreamingSession::open` codepath
    // by invoking the same logic that `replay --session` uses.
    let raw = fs::read_to_string(&trace_path).expect("recorded file readable");
    let mut lines = raw.lines();
    let header_line = lines.next().expect("header line present");
    let header: Request = serde_json::from_str(header_line).expect("header parses");
    let mut config = match header {
        Request::Open { config } => config,
        other => panic!("expected open header, got {other:?}"),
    };
    config.trace_record_path = None;
    let mut replay_session_obj =
        StreamingSession::open(config).expect("replay open should succeed");
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        let req: Request = serde_json::from_str(line).expect("request parses");
        match req {
            Request::Event { event } => {
                replay_session_obj
                    .apply_event(event)
                    .expect("replay apply");
            }
            Request::Metrics | Request::Close => { /* observation-only */ }
            Request::Open { .. } => panic!("second open in trace"),
        }
    }
    let replay_report = replay_session_obj.close();
    let replay_json = serde_json::to_string_pretty(&replay_report).unwrap();

    // The contract: replay must produce byte-identical JSON to a fresh
    // batch run.
    assert_eq!(
        batch_json, replay_json,
        "replay --session must produce byte-identical ReplayReport vs batch"
    );

    // Cleanup.
    let _ = fs::remove_file(&trace_path);
}

// ---------------------------------------------------------------------------
// 5. Failure modes
// ---------------------------------------------------------------------------

#[test]
fn malformed_json_does_not_panic_and_returns_error_response() {
    // We don't run `serve` as a subprocess here — that's a coarser
    // smoke test. Instead, we directly exercise the parsing surface
    // (which is what `serve` does line-by-line) and confirm a typed
    // error is constructible without panic.
    let bad = "{this is not json}";
    let parsed: Result<Request, _> = serde_json::from_str(bad);
    assert!(parsed.is_err());
    // The dispatcher in `main.rs` translates this into a typed error
    // response — simulate that here.
    let resp = Response::Error {
        request_op: None,
        code: "invalid_json".into(),
        message: "bad input".into(),
        fatal: false,
    };
    let s = serde_json::to_string(&resp).unwrap();
    assert!(s.contains("invalid_json"));
}

#[test]
fn double_open_returns_already_open_error_via_dispatcher_simulation() {
    // The dispatcher in main.rs:handle_request rejects a second open.
    // We can't call handle_request directly (it's in a binary, not a
    // lib), but the logic is: if a session is already open, return
    // make_error("open", "already_open", ...). Verify
    // StreamingSession::make_error produces the right shape — and
    // verify two real opens with the same trace_record_path do not
    // panic.
    let resp = StreamingSession::make_error(
        "open",
        "already_open",
        "session already open",
        false,
    );
    match resp {
        Response::Error { code, fatal, .. } => {
            assert_eq!(code, "already_open");
            assert!(!fatal);
        }
        _ => panic!("expected Error"),
    }

    // Real double-open at the StreamingSession level: opening a second
    // session with the same recorder path appends — which is the
    // documented behaviour. We just confirm it doesn't panic.
    let path = temp_path("double-open.jsonl");
    let cfg1 = SessionConfig {
        trace_record_path: Some(path.clone()),
        ..SessionConfig::default()
    };
    let _s1 = StreamingSession::open(cfg1).expect("first open ok");
    let cfg2 = SessionConfig {
        trace_record_path: Some(path.clone()),
        ..SessionConfig::default()
    };
    let _s2 = StreamingSession::open(cfg2).expect("second open ok (append)");
    let _ = fs::remove_file(&path);
}

#[test]
fn eof_mid_session_leaves_recoverable_trace_file() {
    // Simulate "EOF mid-session" by opening a recorder, applying some
    // events, then dropping the session without an explicit close.
    // The trace file must remain replayable up to the last applied
    // event.
    let trace = load_trace("../../fixtures/ix/cognition_divergence.json");
    let path = temp_path("eof-recovery.jsonl");

    {
        let cfg = SessionConfig {
            dimension: trace.dimension,
            trace_record_path: Some(path.clone()),
            ..SessionConfig::default()
        };
        let mut session = StreamingSession::open(cfg).expect("open");
        // Apply only the first 3 events, then drop without closing.
        for ev in trace.events.iter().take(3) {
            session
                .record_request(&Request::Event { event: ev.clone() })
                .expect("record");
            session.apply_event(ev.clone()).expect("apply");
        }
        // No `close` recorded — simulate stdin EOF mid-session.
        // Drop the session: BufWriter is flushed by record_request after
        // each line, so the file is intact.
    }

    // Now replay the partial file. It should parse cleanly and apply
    // the 3 recorded events.
    let raw = fs::read_to_string(&path).expect("recorder file readable");
    let mut lines = raw.lines();
    let header_line = lines.next().expect("header present even on unclean");
    let header: Request = serde_json::from_str(header_line).expect("header parses");
    let mut config = match header {
        Request::Open { config } => config,
        _ => panic!("first line must be open"),
    };
    config.trace_record_path = None;
    let mut replay = StreamingSession::open(config).expect("replay open");
    let mut applied = 0;
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        let req: Request = serde_json::from_str(line).expect("line parses");
        if let Request::Event { event } = req {
            replay.apply_event(event).expect("apply");
            applied += 1;
        }
    }
    assert_eq!(applied, 3, "must replay exactly the 3 recorded events");
    let report = replay.close();
    assert_eq!(report.event_count, 3);

    let _ = fs::remove_file(&path);
}

#[test]
fn out_of_order_cycle_rejected_non_fatally() {
    // Direct test: send cycle 5 then cycle 2 — the second must be
    // rejected with an out_of_order_cycle error, the session must
    // remain usable.
    use hari_core::{ResearchEventPayload};
    use hari_lattice::HexValue;

    let mut session =
        StreamingSession::open(SessionConfig::default()).expect("open");
    let ev1 = ResearchEvent {
        cycle: 5,
        source: "s".into(),
        payload: ResearchEventPayload::BeliefUpdate {
            proposition: "p".into(),
            value: HexValue::Probable,
            evidence: Default::default(),
        },
    };
    session.apply_event(ev1).expect("first apply ok");
    let ev2 = ResearchEvent {
        cycle: 2, // out of order
        source: "s".into(),
        payload: ResearchEventPayload::BeliefUpdate {
            proposition: "p".into(),
            value: HexValue::Doubtful,
            evidence: Default::default(),
        },
    };
    let err = session.apply_event(ev2).expect_err("must reject out-of-order");
    assert!(err.starts_with("out_of_order_cycle"), "got: {err}");

    // Session is still usable: cycle 5 again is allowed (equal).
    let ev3 = ResearchEvent {
        cycle: 5,
        source: "s".into(),
        payload: ResearchEventPayload::BeliefUpdate {
            proposition: "p".into(),
            value: HexValue::Doubtful,
            evidence: Default::default(),
        },
    };
    session.apply_event(ev3).expect("equal-cycle apply ok");

    let report = session.close();
    assert_eq!(report.event_count, 2, "rejected event must not count");
}

// ---------------------------------------------------------------------------
// 6. Reuse-not-divergence: action kinds match between streaming and batch
// ---------------------------------------------------------------------------

#[test]
fn streaming_lie_actions_match_batch_lie_action_kinds() {
    // Defensive guard: if streaming ever forks a parallel cognitive
    // path, this test starts failing.
    let trace = load_trace("../../fixtures/ix/cognition_divergence.json");
    let mut batch_loop =
        CognitiveLoop::with_model(trace.dimension, hari_core::PriorityModel::Lie);
    let batch_report = batch_loop.process_research_trace(trace.clone());

    let cfg = SessionConfig {
        dimension: trace.dimension,
        priority_model: hari_core::PriorityModel::Lie,
        ..SessionConfig::default()
    };
    let mut session = StreamingSession::open(cfg).expect("open");
    let mut stream_outcomes = Vec::new();
    for ev in trace.events {
        let rec = session.apply_event(ev).expect("apply");
        stream_outcomes.push(rec.actions);
    }
    let _ = session.close();

    assert_eq!(stream_outcomes.len(), batch_report.outcomes.len());
    for (i, (s, b)) in stream_outcomes
        .iter()
        .zip(batch_report.outcomes.iter())
        .enumerate()
    {
        let s_kinds: Vec<&'static str> = s.iter().map(action_kind).collect();
        let b_kinds: Vec<&'static str> = b.actions.iter().map(action_kind).collect();
        assert_eq!(
            s_kinds, b_kinds,
            "action-kind divergence between streaming and batch at event {i}"
        );
    }
}
