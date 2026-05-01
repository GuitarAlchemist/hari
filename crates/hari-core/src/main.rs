//! # Project Hari — Cognitive-State Research Sandbox
//!
//! Main binary that demonstrates the cognitive loop with all subsystems.

use hari_core::{
    compare_replay, compare_replay_three_way, Action, CognitiveLoop, PriorityModel, Request,
    ResearchEvent, ResearchEventPayload, ResearchTrace, Response, StreamingSession,
    SubjectiveLogicConfig,
};
use hari_lattice::{HexValue, Relation};
use hari_swarm::{Agent, AgentRole, TrustModel};
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::{env, fs, process};
use tracing::{info, warn};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.get(1).map(String::as_str) == Some("replay") {
        // Parse trailing args. Accepts:
        //   replay <trace.json>              (batch trace, dimension+events)
        //   replay --compare <trace.json>    (batch trace through both models)
        //   replay --session <session.jsonl> (Phase 6 recorded session file)
        let mut compare = false;
        let mut compare3 = false;
        let mut session_mode = false;
        let mut path: Option<&str> = None;
        let mut i = 2;
        while i < args.len() {
            let a = &args[i];
            match a.as_str() {
                "--compare" => compare = true,
                "--compare3" => compare3 = true,
                "--session" => session_mode = true,
                other if !other.starts_with("--") => path = Some(other),
                other => {
                    eprintln!("hari-core replay: unknown flag {other}");
                    process::exit(2);
                }
            }
            i += 1;
        }
        let exclusive_count = [compare, compare3, session_mode]
            .iter()
            .filter(|x| **x)
            .count();
        if exclusive_count > 1 {
            eprintln!(
                "hari-core replay: --compare, --compare3, and --session are mutually exclusive"
            );
            process::exit(2);
        }
        let result = if session_mode {
            replay_session(path)
        } else if compare3 {
            replay_trace_three_way(path)
        } else {
            replay_trace(path, compare)
        };
        if let Err(err) = result {
            eprintln!("hari-core replay failed: {err}");
            process::exit(1);
        }
        return;
    }

    if args.get(1).map(String::as_str) == Some("serve") {
        // Phase 6 stdio JSONL service. Synchronous request/response.
        // Logs to stderr; protocol on stdin/stdout.
        let _ = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .with_target(false)
            .with_writer(std::io::stderr)
            .try_init();
        if let Err(err) = serve_stdio() {
            eprintln!("hari-core serve failed: {err}");
            process::exit(1);
        }
        return;
    }

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .init();

    run_substrate_decision_demo();
}

/// Self-referential demo: Hari tracks the Phase 5 substrate decision
/// as a set of claims, with the four canonical agents voting and
/// declared relations driving derivation.
///
/// This is *not* Hari researching itself — Hari is the substrate;
/// the experiment author (the human + Claude in real life, this
/// hard-coded `events` Vec in the demo) is external. What it shows
/// is that the post-Phase-8 substrate can hold and reason about
/// claims of any kind, including claims about its own configuration.
///
/// Uses `PriorityModel::Flat` so every action shows up in the demo
/// output. The post-Phase-5 default is `RecencyDecay`, under which
/// log-only events (e.g., relation declarations on their own) get
/// suppressed to `Wait`. Switch via `with_model(...)` for production
/// use.
fn run_substrate_decision_demo() {
    info!("=== Project Hari — substrate-decision demo ===");
    info!("Hari tracks claims about its own Phase 5 substrate decision:");
    info!("  - the original Lie hypothesis");
    info!("  - the SL prior-art finding that disconfirmed it");
    info!("  - the demote-Lie / RecencyDecay-default conclusion");
    info!("Provenance + trust-aware consensus drive the derivation.");

    let mut loop_ = CognitiveLoop::with_model(4, PriorityModel::Flat);
    loop_.use_swarm_consensus = true;
    loop_.trust_model = TrustModel::RoleWeighted;

    // Seed the four canonical agent roles into the swarm with explicit
    // trust profiles. The bridge will route their AgentVote events
    // into this swarm and recompute consensus under RoleWeighted.
    for (id, name, self_trust, message_trust) in [
        ("guardian", "guardian", 0.95, 0.4),
        ("critic", "critic", 0.9, 0.3),
        ("explorer", "explorer", 0.4, 0.85),
        ("integrator", "integrator", 0.7, 0.7),
    ] {
        loop_.swarm.add_agent(Agent::new(
            id,
            AgentRole {
                name: name.to_string(),
                self_trust,
                message_trust,
            },
        ));
    }
    info!(
        "Loop initialized: model={:?}, trust={:?}, swarm={} agents",
        loop_.priority_model,
        loop_.trust_model,
        loop_.swarm.len()
    );

    // The script: a sequence of research events that walks Hari
    // through the Phase 5 substrate decision as claims.
    let events: Vec<ResearchEvent> = vec![
        // Goal: settle the substrate question
        ResearchEvent {
            cycle: 1,
            source: "owner".to_string(),
            payload: ResearchEventPayload::GoalUpdate {
                key: "decide-cognition-substrate".to_string(),
                description: "Settle whether Lie or a simpler baseline should be default"
                    .to_string(),
                priority: 0.95,
                status: Some(HexValue::Unknown),
            },
        },
        // Logical structure of the decision: SL beating Lie should
        // contradict the original Lie-superiority hypothesis, and
        // imply the demotion + RecencyDecay-default conclusions.
        ResearchEvent {
            cycle: 2,
            source: "ix-modeller".to_string(),
            payload: ResearchEventPayload::RelationDeclaration {
                from: "sl-beats-lie-on-false-acceptance".to_string(),
                to: "lie-beats-simpler-baselines".to_string(),
                relation: Relation::Contradicts,
            },
        },
        ResearchEvent {
            cycle: 3,
            source: "ix-modeller".to_string(),
            payload: ResearchEventPayload::RelationDeclaration {
                from: "sl-beats-lie-on-false-acceptance".to_string(),
                to: "should-demote-lie".to_string(),
                relation: Relation::Implies,
            },
        },
        ResearchEvent {
            cycle: 4,
            source: "ix-modeller".to_string(),
            payload: ResearchEventPayload::RelationDeclaration {
                from: "should-demote-lie".to_string(),
                to: "recency-decay-safe-default".to_string(),
                relation: Relation::Implies,
            },
        },
        // The original hypothesis lands as Probable.
        ResearchEvent {
            cycle: 5,
            source: "ix-original-hypothesis".to_string(),
            payload: ResearchEventPayload::BeliefUpdate {
                proposition: "lie-beats-simpler-baselines".to_string(),
                value: HexValue::Probable,
                evidence: Default::default(),
            },
        },
        // The Phase 5 data arrives — SL beats Lie. This triggers
        // propagation: Contradicts flips lie-beats-simpler-baselines to
        // Contradictory; Implies derives should-demote-lie = True;
        // chained Implies derives recency-decay-safe-default = True.
        ResearchEvent {
            cycle: 6,
            source: "ix-experiment-runner".to_string(),
            payload: ResearchEventPayload::ExperimentResult {
                proposition: "sl-beats-lie-on-false-acceptance".to_string(),
                value: HexValue::True,
                evidence: {
                    let mut e = std::collections::BTreeMap::new();
                    e.insert("fixtures_won".to_string(), serde_json::Value::from(3));
                    e.insert("fixtures_tied".to_string(), serde_json::Value::from(3));
                    e.insert("fixtures_lost".to_string(), serde_json::Value::from(0));
                    e
                },
            },
        },
        // The four agents vote. Under RoleWeighted, guardian + critic
        // (high self_trust) carry more weight than explorer (low).
        ResearchEvent {
            cycle: 7,
            source: "guardian".to_string(),
            payload: ResearchEventPayload::AgentVote {
                proposition: "should-demote-lie".to_string(),
                value: HexValue::Probable,
                evidence: Default::default(),
            },
        },
        ResearchEvent {
            cycle: 8,
            source: "critic".to_string(),
            payload: ResearchEventPayload::AgentVote {
                proposition: "should-demote-lie".to_string(),
                value: HexValue::Probable,
                evidence: Default::default(),
            },
        },
        ResearchEvent {
            cycle: 9,
            source: "explorer".to_string(),
            payload: ResearchEventPayload::AgentVote {
                proposition: "should-demote-lie".to_string(),
                value: HexValue::Doubtful,
                evidence: Default::default(),
            },
        },
        ResearchEvent {
            cycle: 10,
            source: "integrator".to_string(),
            payload: ResearchEventPayload::AgentVote {
                proposition: "should-demote-lie".to_string(),
                value: HexValue::Probable,
                evidence: Default::default(),
            },
        },
    ];

    for event in events {
        info!("");
        info!("--- cycle {} ({}) ---", event.cycle, event.source);
        let outcome = loop_.process_research_event(event);
        for action in &outcome.actions {
            match action {
                Action::Escalate { reason, confidence } => {
                    warn!("  ESCALATE: {} (confidence: {:.2})", reason, confidence);
                }
                _ => info!("  -> {}", action),
            }
        }
        if !outcome.derivations.is_empty() {
            info!("  derivations:");
            for d in &outcome.derivations {
                let chain: Vec<String> = d
                    .contributions
                    .iter()
                    .map(|c| format!("{}({:?},{:?})", c.source, c.relation, c.source_value))
                    .collect();
                info!(
                    "    {}: {:?} -> {:?} (round {}) from [{}]",
                    d.proposition,
                    d.previous_value,
                    d.new_value,
                    d.round,
                    chain.join(", ")
                );
            }
        }
    }

    // --- Final substrate-decision conclusion ---
    info!("");
    info!("=== Final substrate-decision belief state ===");
    for prop in [
        "lie-beats-simpler-baselines",
        "sl-beats-lie-on-false-acceptance",
        "should-demote-lie",
        "recency-decay-safe-default",
    ] {
        let value = loop_
            .state
            .beliefs
            .get(prop)
            .map(|b| format!("{}", b.value))
            .unwrap_or_else(|| "<not in network>".to_string());
        info!("  {}: {}", prop, value);
    }

    // Swarm consensus tally for the decision claim, both modes side by
    // side — shows how trust-weighting changes the head count.
    info!("");
    info!("=== Swarm consensus on 'should-demote-lie' ===");
    let equal = loop_
        .swarm
        .consensus_with("should-demote-lie", TrustModel::Equal);
    let weighted = loop_
        .swarm
        .consensus_with("should-demote-lie", TrustModel::RoleWeighted);
    info!(
        "  Equal (1-vote-per-agent):     {} (agreement: {:.0}%)",
        equal.consensus,
        equal.agreement * 100.0
    );
    info!(
        "  RoleWeighted (by self_trust): {} (agreement: {:.0}%)",
        weighted.consensus,
        weighted.agreement * 100.0
    );

    info!("");
    info!("=== demo complete ===");
    info!("Hari held the substrate decision as a tracked claim, derived");
    info!("its conclusion from the relation graph + experimental evidence,");
    info!("and reflected the four-agent vote under trust-weighted consensus.");
    info!("The researcher (you, plus Claude in this conversation) is");
    info!("external — Hari is the substrate, not the autoresearch system.");
}

fn replay_trace(path: Option<&str>, compare: bool) -> Result<(), Box<dyn std::error::Error>> {
    let path = path.ok_or("usage: hari-core replay [--compare] <trace.json>")?;
    let trace_json = fs::read_to_string(path)?;
    let trace = parse_trace(&trace_json)?;

    let report = if compare {
        compare_replay(trace)
    } else {
        let mut cognitive_loop = CognitiveLoop::new(trace.dimension);
        cognitive_loop.process_research_trace(trace)
    };
    serde_json::to_writer_pretty(std::io::stdout(), &report)?;
    println!();

    Ok(())
}

/// `replay --compare3` — runs the trace through `RecencyDecay`, `Lie`,
/// and the Subjective Logic baseline on fresh state. Emits a wrapper
/// JSON object so the existing `--compare` schema stays untouched (the
/// Phase 6 replay-parity test asserts the unchanged shape).
fn replay_trace_three_way(path: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    let path = path.ok_or("usage: hari-core replay --compare3 <trace.json>")?;
    let trace_json = fs::read_to_string(path)?;
    let trace = parse_trace(&trace_json)?;

    let report = compare_replay_three_way(trace, SubjectiveLogicConfig::default());
    serde_json::to_writer_pretty(std::io::stdout(), &report)?;
    println!();
    Ok(())
}

/// Phase 6 — replay a recorded session-trace file (the JSONL written by
/// `serve` when `trace_record_path` is set). Reads the file, requires
/// the first line to be an `Open` request, then re-feeds every
/// subsequent `Event` through `StreamingSession::apply_event`. `Metrics`
/// and `Close` lines are no-ops on replay (they only observed state).
///
/// Emits the resulting `ResearchReplayReport` as pretty JSON to stdout,
/// matching the format of the existing batch `replay` command.
fn replay_session(path: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    let path = path.ok_or("usage: hari-core replay --session <session.jsonl>")?;
    let raw = fs::read_to_string(path)?;
    let mut lines = raw.lines();
    let header_line = lines
        .next()
        .ok_or("session trace is empty (no open header)")?;
    let header: Request = serde_json::from_str(header_line)
        .map_err(|e| format!("first line must be an open request: {e}"))?;
    let mut config = match header {
        Request::Open { config } => config,
        other => {
            return Err(format!(
                "first line must be `op: open`, got: {}",
                serde_json::to_string(&other).unwrap_or_default()
            )
            .into());
        }
    };
    // Replay must NOT re-record into trace_record_path (the file we're
    // currently reading). Strip it.
    config.trace_record_path = None;
    let mut session = StreamingSession::open(config)?;
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        // Tolerate non-Request lines (e.g. trailing close-marker objects
        // emitted by unclean-shutdown handling) by ignoring deserialize
        // errors that aren't Event-shaped.
        let req: Request = match serde_json::from_str(line) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("replay --session: skipping malformed line: {e}");
                continue;
            }
        };
        match req {
            Request::Event { event } => {
                if let Err(e) = session.apply_event(event) {
                    eprintln!("replay --session: event rejected: {e}");
                }
            }
            Request::Metrics | Request::Close => {
                // Observation-only / lifecycle markers — no-ops on replay.
            }
            Request::Open { .. } => {
                return Err("session trace contains a second `open` request".into());
            }
        }
    }
    let report = session.close();
    serde_json::to_writer_pretty(std::io::stdout(), &report)?;
    println!();
    Ok(())
}

/// Phase 6 — stdio JSONL service. Reads `Request` lines from stdin,
/// writes `Response` lines to stdout. Errors that should be visible to
/// the IX consumer are returned as `Response::Error`; structural /
/// I/O errors that prevent further protocol use go to stderr and exit
/// non-zero.
///
/// Implements the failure-mode table from `phase6-design.md` §6:
/// malformed JSON → invalid_json non-fatal; unknown op → unknown_op
/// non-fatal; double-open → already_open non-fatal; out-of-order cycle
/// → out_of_order_cycle non-fatal; EOF mid-session → write a final
/// `closed` response with `unclean: true` plus a close marker to the
/// trace file.
fn serve_stdio() -> Result<(), Box<dyn std::error::Error>> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let reader = BufReader::new(stdin.lock());
    let mut writer = BufWriter::new(stdout.lock());

    let mut session: Option<StreamingSession> = None;

    for line_result in reader.lines() {
        let line = match line_result {
            Ok(l) => l,
            Err(e) => {
                // Hard I/O error on stdin — finalize unclean and exit.
                eprintln!("hari-core serve: stdin read error: {e}");
                if let Some(s) = session.take() {
                    write_unclean_close(&mut writer, s)?;
                }
                return Err(e.into());
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        let req: Request = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let resp = Response::Error {
                    request_op: None,
                    code: "invalid_json".into(),
                    message: format!("could not parse request line: {e}"),
                    fatal: false,
                };
                write_response(&mut writer, &resp)?;
                continue;
            }
        };
        let response = handle_request(&mut session, req);
        write_response(&mut writer, &response)?;
        // If this was a close that succeeded, drop the session.
        if let Response::Closed { .. } = &response {
            session = None;
        }
    }

    // EOF on stdin. If a session was still live, finalize unclean.
    if let Some(s) = session.take() {
        write_unclean_close(&mut writer, s)?;
    }
    writer.flush()?;
    Ok(())
}

fn handle_request(session: &mut Option<StreamingSession>, req: Request) -> Response {
    match req {
        Request::Open { config } => {
            if session.is_some() {
                return StreamingSession::make_error(
                    "open",
                    "already_open",
                    "session already open; multi-session is out of scope (see phase6-design.md §6)",
                    false,
                );
            }
            match StreamingSession::open(config) {
                Ok(s) => {
                    let opened = Response::Opened {
                        session_id: s.session_id().to_string(),
                        trace_path: s.trace_path().map(|p| p.to_path_buf()),
                        hari_version: env!("CARGO_PKG_VERSION").to_string(),
                        config_echo: s.config().clone(),
                    };
                    *session = Some(s);
                    opened
                }
                Err(e) => StreamingSession::make_error("open", "trace_io", e, true),
            }
        }
        Request::Event { event } => {
            let Some(s) = session.as_mut() else {
                return StreamingSession::make_error(
                    "event",
                    "no_session",
                    "no open session; send `op: open` first",
                    false,
                );
            };
            if s.is_closed() {
                return StreamingSession::make_error(
                    "event",
                    "session_closed",
                    "session has been closed; events are no longer accepted",
                    false,
                );
            }
            // Record the event in the trace file before applying so a
            // panic mid-apply still leaves the request on disk.
            if let Err(e) = s.record_request(&Request::Event {
                event: event.clone(),
            }) {
                return StreamingSession::make_error(
                    "event",
                    "trace_io",
                    format!("failed to record event: {e}"),
                    true,
                );
            }
            match s.apply_event(event) {
                Ok(rec) => Response::Recommendation(rec),
                Err(e) => {
                    // Out-of-order cycle / session_closed — non-fatal.
                    let code = e
                        .split(':')
                        .next()
                        .unwrap_or("invalid_event")
                        .trim()
                        .to_string();
                    StreamingSession::make_error("event", &code, e, false)
                }
            }
        }
        Request::Metrics => {
            let Some(s) = session.as_mut() else {
                return StreamingSession::make_error(
                    "metrics",
                    "no_session",
                    "no open session",
                    false,
                );
            };
            if let Err(e) = s.record_request(&Request::Metrics) {
                return StreamingSession::make_error(
                    "metrics",
                    "trace_io",
                    format!("failed to record metrics request: {e}"),
                    true,
                );
            }
            let (metrics, beliefs, goals) = s.metrics_snapshot();
            Response::MetricsSnapshot {
                metrics,
                beliefs,
                goals,
            }
        }
        Request::Close => {
            let Some(s) = session.as_mut() else {
                return StreamingSession::make_error(
                    "close",
                    "no_session",
                    "no open session to close",
                    false,
                );
            };
            if let Err(e) = s.record_request(&Request::Close) {
                return StreamingSession::make_error(
                    "close",
                    "trace_io",
                    format!("failed to record close: {e}"),
                    true,
                );
            }
            s.mark_closed();
            let final_report = s.snapshot_report();
            Response::Closed {
                final_report,
                unclean: false,
            }
        }
    }
}

/// On EOF mid-session, write an unclean close marker to the trace file
/// (so `replay --session` knows to stop applying events at that
/// boundary), then emit a final `closed` response with `unclean: true`
/// to stdout.
fn write_unclean_close<W: Write>(writer: &mut W, mut session: StreamingSession) -> io::Result<()> {
    // Append an explicit close-marker line to the trace file (best
    // effort; record_request handles None recorder transparently).
    let _ = session.record_request(&Request::Close);
    session.mark_closed();
    let final_report = session.snapshot_report();
    let resp = Response::Closed {
        final_report,
        unclean: true,
    };
    write_response(writer, &resp)
}

fn write_response<W: Write>(writer: &mut W, resp: &Response) -> io::Result<()> {
    let s =
        serde_json::to_string(resp).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    writer.write_all(s.as_bytes())?;
    writer.write_all(b"\n")?;
    writer.flush()
}

fn parse_trace(trace_json: &str) -> Result<ResearchTrace, serde_json::Error> {
    match serde_json::from_str::<ResearchTrace>(trace_json) {
        Ok(trace) => Ok(trace),
        Err(object_error) => match serde_json::from_str::<Vec<ResearchEvent>>(trace_json) {
            Ok(events) => Ok(events.into()),
            Err(_) => Err(object_error),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_trace_accepts_object_form() {
        let trace = parse_trace(
            r#"{
                "dimension": 6,
                "events": [
                    {
                        "cycle": 1,
                        "source": "ix-agent",
                        "payload": {
                            "type": "belief_update",
                            "proposition": "prompt-a-improves-pass-rate",
                            "value": "Probable"
                        }
                    }
                ]
            }"#,
        )
        .unwrap();

        assert_eq!(trace.dimension, 6);
        assert_eq!(trace.events.len(), 1);
    }

    #[test]
    fn replay_report_round_trips_new_optional_fields() {
        // The new `priority_model`, `metrics`, and `comparison` fields must
        // round-trip via serde and load as defaults from older fixtures
        // that don't include them.
        let mut cognitive_loop = CognitiveLoop::new(4);
        let trace = parse_trace(
            r#"[
                {
                    "cycle": 1,
                    "source": "ix-agent",
                    "payload": {
                        "type": "belief_update",
                        "proposition": "p",
                        "value": "Probable"
                    }
                }
            ]"#,
        )
        .unwrap();
        let report = cognitive_loop.process_research_trace(trace);
        let s = serde_json::to_string(&report).unwrap();
        let round_tripped: hari_core::ResearchReplayReport = serde_json::from_str(&s).unwrap();
        assert!(round_tripped.comparison.is_none());
        // Old fixtures without the new fields must still load — try a JSON
        // shape lacking them entirely.
        let legacy = r#"{
            "event_count": 0,
            "outcomes": [],
            "final_beliefs": {},
            "final_goals": {},
            "final_state_summary": "n/a"
        }"#;
        let loaded: hari_core::ResearchReplayReport = serde_json::from_str(legacy).unwrap();
        assert_eq!(loaded.event_count, 0);
        assert!(loaded.comparison.is_none());
    }

    #[test]
    fn parse_trace_accepts_array_form() {
        let trace = parse_trace(
            r#"[
                {
                    "cycle": 1,
                    "source": "ix-agent",
                    "payload": {
                        "type": "belief_update",
                        "proposition": "prompt-a-improves-pass-rate",
                        "value": "Probable"
                    }
                }
            ]"#,
        )
        .unwrap();

        assert_eq!(trace.dimension, 4);
        assert_eq!(trace.events.len(), 1);
    }
}
