//! Phase 6 — subprocess-level integration tests for `hari-core serve`.
//!
//! In-process [`hari_core::StreamingSession`] behaviour is exhaustively
//! covered by `phase6_session.rs`. These tests exercise the binary
//! entry point itself: the line-buffered stdin reader in `main.rs`'s
//! `serve_stdio`, the `handle_request` dispatcher (including the
//! `already_open` / `no_session` / `invalid_json` paths that the
//! in-process tests bypass), and stdin-EOF unclean-shutdown semantics.
//!
//! The binary is located via `env!("CARGO_BIN_EXE_hari-core")`, which
//! cargo populates for integration tests against the crate's binary
//! target. No extra build step is required.

use hari_core::{Request, ResearchEvent, ResearchTrace, Response, SessionConfig};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

// ---------------------------------------------------------------------------
// ServeChild — small RAII harness around a `hari-core serve` subprocess
// ---------------------------------------------------------------------------

/// RAII wrapper that spawns `hari-core serve`, exposes line-oriented
/// `send`/`recv` helpers, and kills the child on drop so a panicking
/// test never leaves a zombie process.
struct ServeChild {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
}

impl ServeChild {
    fn spawn() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_hari-core"))
            .arg("serve")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // Stderr is the subprocess's tracing sink. Keep it piped so
            // debug logs are reachable when tests fail; we never drain
            // it, but the volume is tiny (no INFO! calls in the serve
            // path) and the OS pipe buffer absorbs it.
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn hari-core serve");
        let stdin = child.stdin.take().expect("stdin");
        let stdout = BufReader::new(child.stdout.take().expect("stdout"));
        Self {
            child,
            stdin: Some(stdin),
            stdout,
        }
    }

    fn send(&mut self, req: &Request) {
        let s = serde_json::to_string(req).expect("serialize request");
        self.send_raw(&s);
    }

    fn send_raw(&mut self, line: &str) {
        let stdin = self.stdin.as_mut().expect("stdin already closed");
        stdin.write_all(line.as_bytes()).expect("write line");
        stdin.write_all(b"\n").expect("write newline");
        stdin.flush().expect("flush");
    }

    fn recv(&mut self) -> Response {
        let mut line = String::new();
        let n = self.stdout.read_line(&mut line).expect("read response");
        assert!(n > 0, "unexpected EOF on stdout while expecting a response");
        serde_json::from_str(line.trim_end()).unwrap_or_else(|e| {
            panic!("response not parseable as Response: {e}\nraw line: {line:?}")
        })
    }

    /// Close stdin so the server sees EOF. Used by the unclean-shutdown
    /// test.
    fn close_stdin(&mut self) {
        self.stdin = None;
    }

    /// Drain any remaining stdout lines (after we've stopped writing)
    /// and return them as raw strings. Returns when the child closes
    /// stdout.
    fn drain_stdout(&mut self) -> Vec<String> {
        let mut out = Vec::new();
        let mut line = String::new();
        loop {
            line.clear();
            let n = self.stdout.read_line(&mut line).expect("drain stdout");
            if n == 0 {
                break;
            }
            out.push(line.trim_end().to_string());
        }
        out
    }

    fn wait(mut self) -> std::process::ExitStatus {
        self.stdin = None;
        self.child.wait().expect("wait child")
    }
}

impl Drop for ServeChild {
    fn drop(&mut self) {
        self.stdin = None;
        // Best-effort kill; a clean wait() in the test path makes this
        // a no-op, but if a test panics mid-flight we don't want a
        // zombie.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn load_trace(path: &str) -> ResearchTrace {
    let raw = fs::read_to_string(path).expect("fixture readable");
    match serde_json::from_str::<ResearchTrace>(&raw) {
        Ok(t) => t,
        Err(_) => {
            let events: Vec<ResearchEvent> =
                serde_json::from_str(&raw).expect("fixture parses as trace or array");
            events.into()
        }
    }
}

fn temp_path(suffix: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    let pid = std::process::id();
    p.push(format!("hari-phase6-serve-{pid}-{suffix}"));
    if p.exists() {
        let _ = fs::remove_file(&p);
    }
    p
}

// ---------------------------------------------------------------------------
// 1. Golden path: open → events → close
// ---------------------------------------------------------------------------

#[test]
fn serve_open_events_close_golden_path() {
    let trace = load_trace("../../fixtures/ix/cognition_divergence.json");
    let mut s = ServeChild::spawn();

    // Open
    s.send(&Request::Open {
        config: SessionConfig {
            dimension: trace.dimension,
            ..SessionConfig::default()
        },
    });
    match s.recv() {
        Response::Opened {
            session_id,
            hari_version,
            ..
        } => {
            assert!(!session_id.is_empty(), "session_id must be non-empty");
            assert!(!hari_version.is_empty(), "hari_version must be non-empty");
        }
        other => panic!("expected Opened, got {other:?}"),
    }

    // Stream all events; each should yield a Recommendation.
    for (i, event) in trace.events.iter().enumerate() {
        s.send(&Request::Event {
            event: event.clone(),
        });
        match s.recv() {
            Response::Recommendation(rec) => {
                assert_eq!(
                    rec.event_index, i,
                    "event_index must be the 0-based ordinal of the event in the session"
                );
                assert!(
                    !rec.actions.is_empty(),
                    "every event must produce at least one action (Log at minimum)"
                );
            }
            other => panic!("expected Recommendation at event {i}, got {other:?}"),
        }
    }

    // Close
    s.send(&Request::Close);
    match s.recv() {
        Response::Closed {
            final_report,
            unclean,
        } => {
            assert!(!unclean, "clean close must report unclean: false");
            assert_eq!(
                final_report.event_count,
                trace.events.len(),
                "final_report.event_count must equal events sent"
            );
            assert_eq!(
                final_report.outcomes.len(),
                trace.events.len(),
                "final_report.outcomes must have one entry per event"
            );
        }
        other => panic!("expected Closed, got {other:?}"),
    }

    let status = s.wait();
    assert!(status.success(), "serve must exit 0 after clean close");
}

// ---------------------------------------------------------------------------
// 2. Dispatcher: double-open returns already_open, first session unaffected
// ---------------------------------------------------------------------------

#[test]
fn serve_double_open_returns_already_open_error() {
    let mut s = ServeChild::spawn();

    s.send(&Request::Open {
        config: SessionConfig::default(),
    });
    let first_session_id = match s.recv() {
        Response::Opened { session_id, .. } => session_id,
        other => panic!("expected Opened, got {other:?}"),
    };

    // Second open on the same connection: dispatcher rejects with
    // already_open, non-fatal.
    s.send(&Request::Open {
        config: SessionConfig::default(),
    });
    match s.recv() {
        Response::Error {
            code,
            fatal,
            request_op,
            ..
        } => {
            assert_eq!(code, "already_open");
            assert!(!fatal, "already_open must be non-fatal");
            assert_eq!(request_op.as_deref(), Some("open"));
        }
        other => panic!("expected Error::already_open, got {other:?}"),
    }

    // First session still usable: a metrics request returns a snapshot.
    // No events were applied, so beliefs/goals should be empty and the
    // metrics counters at their zero values.
    s.send(&Request::Metrics);
    match s.recv() {
        Response::MetricsSnapshot {
            metrics,
            beliefs,
            goals,
        } => {
            assert!(beliefs.is_empty(), "no events sent → beliefs empty");
            assert!(goals.is_empty(), "no initial_goals → goals empty");
            assert_eq!(metrics.false_acceptance_count, 0);
        }
        other => panic!("expected MetricsSnapshot after rejected double-open, got {other:?}"),
    }

    // Clean shutdown.
    s.send(&Request::Close);
    let _ = s.recv();
    let _ = first_session_id; // kept for clarity; not asserted further
    assert!(s.wait().success());
}

// ---------------------------------------------------------------------------
// 3. Dispatcher: event-before-open returns no_session
// ---------------------------------------------------------------------------

#[test]
fn serve_event_before_open_returns_no_session() {
    let mut s = ServeChild::spawn();

    let event_json = r#"{"op":"event","event":{"cycle":1,"source":"ix","payload":{"type":"belief_update","proposition":"p","value":"Probable"}}}"#;
    s.send_raw(event_json);
    match s.recv() {
        Response::Error {
            code,
            fatal,
            request_op,
            ..
        } => {
            assert_eq!(code, "no_session");
            assert!(!fatal);
            assert_eq!(request_op.as_deref(), Some("event"));
        }
        other => panic!("expected Error::no_session, got {other:?}"),
    }

    // After the rejection, an open should still succeed — proves the
    // server didn't enter a poisoned state.
    s.send(&Request::Open {
        config: SessionConfig::default(),
    });
    match s.recv() {
        Response::Opened { .. } => {}
        other => panic!("expected Opened after no_session, got {other:?}"),
    }

    s.send(&Request::Close);
    let _ = s.recv();
    assert!(s.wait().success());
}

// ---------------------------------------------------------------------------
// 4. Post-close events return no_session (session is dropped on Close)
// ---------------------------------------------------------------------------

#[test]
fn serve_event_after_close_returns_no_session() {
    // After a successful Close, main.rs drops the session before
    // reading the next request. So a follow-up event hits the
    // "no open session" branch in handle_request, not the
    // "session.is_closed()" branch. This test pins that observable
    // behaviour at the wire layer.
    let mut s = ServeChild::spawn();

    s.send(&Request::Open {
        config: SessionConfig::default(),
    });
    let _ = s.recv();

    s.send(&Request::Close);
    match s.recv() {
        Response::Closed { unclean, .. } => assert!(!unclean),
        other => panic!("expected Closed, got {other:?}"),
    }

    let event_json = r#"{"op":"event","event":{"cycle":1,"source":"ix","payload":{"type":"belief_update","proposition":"p","value":"Probable"}}}"#;
    s.send_raw(event_json);
    match s.recv() {
        Response::Error { code, fatal, .. } => {
            assert_eq!(code, "no_session");
            assert!(!fatal);
        }
        other => panic!("expected Error::no_session after close, got {other:?}"),
    }

    assert!(s.wait().success());
}

// ---------------------------------------------------------------------------
// 5. Malformed JSON line is reported as invalid_json non-fatal; server keeps going
// ---------------------------------------------------------------------------

#[test]
fn serve_malformed_json_emits_invalid_json_and_session_continues() {
    let mut s = ServeChild::spawn();

    s.send_raw("{this is not json}");
    match s.recv() {
        Response::Error { code, fatal, .. } => {
            assert_eq!(code, "invalid_json");
            assert!(!fatal, "invalid_json must be non-fatal");
        }
        other => panic!("expected Error::invalid_json, got {other:?}"),
    }

    // The server still accepts a valid open afterwards.
    s.send(&Request::Open {
        config: SessionConfig::default(),
    });
    match s.recv() {
        Response::Opened { .. } => {}
        other => panic!("expected Opened after invalid_json, got {other:?}"),
    }

    s.send(&Request::Close);
    let _ = s.recv();
    assert!(s.wait().success());
}

// ---------------------------------------------------------------------------
// 6. EOF mid-session emits a final unclean Closed response; trace file remains replayable
// ---------------------------------------------------------------------------

#[test]
fn serve_eof_mid_session_emits_unclean_close_and_writes_replayable_trace() {
    let trace = load_trace("../../fixtures/ix/cognition_divergence.json");
    let trace_path = temp_path("eof.jsonl");

    let mut s = ServeChild::spawn();
    s.send(&Request::Open {
        config: SessionConfig {
            dimension: trace.dimension,
            trace_record_path: Some(trace_path.clone()),
            ..SessionConfig::default()
        },
    });
    match s.recv() {
        Response::Opened { trace_path: tp, .. } => {
            assert_eq!(tp.as_ref(), Some(&trace_path));
        }
        other => panic!("expected Opened, got {other:?}"),
    }

    // Send the first 3 events, read their recommendations.
    for ev in trace.events.iter().take(3) {
        s.send(&Request::Event { event: ev.clone() });
        match s.recv() {
            Response::Recommendation(_) => {}
            other => panic!("expected Recommendation, got {other:?}"),
        }
    }

    // Close stdin. The server's read loop will see EOF, write a final
    // Closed{unclean: true} to stdout, flush, and exit 0.
    s.close_stdin();

    // Drain stdout. The final line must be a Closed{unclean: true}.
    let remaining = s.drain_stdout();
    assert!(
        !remaining.is_empty(),
        "server must emit at least the final Closed line on EOF"
    );
    let last: Response = serde_json::from_str(remaining.last().unwrap())
        .expect("final stdout line must be a Response");
    match last {
        Response::Closed {
            unclean,
            final_report,
        } => {
            assert!(unclean, "EOF must produce unclean: true");
            assert_eq!(
                final_report.event_count, 3,
                "final_report.event_count must reflect the 3 events that were applied"
            );
        }
        other => panic!("expected unclean Closed, got {other:?}"),
    }

    let status = s.wait();
    assert!(
        status.success(),
        "serve must still exit 0 on stdin EOF (it is the documented end-of-session signal)"
    );

    // The recorded trace file must remain a valid input to
    // `replay --session`. Verify by reading and parsing line-by-line:
    // first line is Open, subsequent are Event / Close.
    let raw = fs::read_to_string(&trace_path).expect("trace file readable");
    let mut lines = raw.lines();
    let header_line = lines.next().expect("trace must have a header line");
    let header: Request = serde_json::from_str(header_line).expect("header parses as Request");
    assert!(
        matches!(header, Request::Open { .. }),
        "trace header must be Request::Open"
    );
    let mut event_lines = 0;
    let mut close_lines = 0;
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        let req: Request = serde_json::from_str(line)
            .unwrap_or_else(|e| panic!("trace line did not parse: {e}\n{line}"));
        match req {
            Request::Event { .. } => event_lines += 1,
            Request::Close => close_lines += 1,
            Request::Metrics => {}
            Request::Open { .. } => panic!("second Open in trace"),
        }
    }
    assert_eq!(
        event_lines, 3,
        "trace must contain the 3 applied events verbatim"
    );
    assert!(
        close_lines >= 1,
        "trace must include at least one close marker (the unclean-shutdown one)"
    );

    let _ = fs::remove_file(&trace_path);
}
