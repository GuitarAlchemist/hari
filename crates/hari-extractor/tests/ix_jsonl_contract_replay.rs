//! End-to-end test: synthetic ix-autoresearch JSONL → `ResearchTrace` →
//! `CognitiveLoop::process_research_trace` → `ResearchReplayReport`.
//!
//! This mirrors the Phase 2 boundary specified in the IX-side
//! `crates/ix-autoresearch/SCHEMA.md` Layer 1 contract (which the IX repo
//! ships with structural tests under `tests/jsonl_contract.rs`). The IX
//! side guarantees that real producer output matches the schema embedded
//! below; this test guarantees that this crate's `hari_from_ix_autoresearch`
//! projection plus `hari-core` replay correctly preserves contradictory
//! findings — the load-bearing acceptance criterion from
//! `agent-blackbox/docs/ix-real-problems-plan.md` Workflow 3.
//!
//! We avoid declaring a build-time dep on the IX repo by embedding a small
//! synthetic log here. CI on the IX side enforces the schema; this side
//! verifies the consumer holds the other end of the contract.

use std::io::Write;
use std::process::{Command, Stdio};

use hari_core::{CognitiveLoop, ResearchTrace};
use hari_lattice::HexValue;

/// Synthetic ix-autoresearch log that conforms to the layer-1 schema and
/// exercises Hari's contradictory-evidence consolidation. Two iteration
/// lines with the SAME `config_hash` carry differing `accepted` flags;
/// Hari should resolve the final belief on the derived proposition to
/// `HexValue::Contradictory`.
const SYNTHETIC_LOG: &str = r#"{"event":"run_start","schema_version":1,"run_id":"test-run-0001","timestamp":"2026-05-17T10:00:00Z","target":"ix_autoresearch::target_grammar::GrammarTarget","strategy":{"kind":"Greedy"},"seed":42,"git_sha":null,"git_sha_reason":"synthetic-fixture","baseline_config":{"rule_weights":[0.25,0.25,0.25,0.25],"temperature":1.0},"eval_inputs_hash":null}
{"event":"iteration","schema_version":1,"iteration":0,"timestamp":"2026-05-17T10:00:01Z","config":{"rule_weights":[0.30,0.25,0.25,0.20],"temperature":1.0},"config_hash":"contractfixturehash01_contradictory_pair","score":{"parse_success_rate":0.30,"ess_stability":0.95},"reward":0.40,"accepted":true,"previous_hash":null,"error":null,"elapsed_ms":5,"strategy_state":null,"cache_hit":false}
{"event":"iteration","schema_version":1,"iteration":1,"timestamp":"2026-05-17T10:00:02Z","config":{"rule_weights":[0.30,0.25,0.25,0.20],"temperature":1.0},"config_hash":"contractfixturehash01_contradictory_pair","score":{"parse_success_rate":0.28,"ess_stability":0.95},"reward":0.38,"accepted":false,"previous_hash":"contractfixturehash01_contradictory_pair","error":null,"elapsed_ms":4,"strategy_state":null,"cache_hit":false}
{"event":"iteration","schema_version":1,"iteration":2,"timestamp":"2026-05-17T10:00:03Z","config":{"rule_weights":[0.30,0.25,0.25,0.20],"temperature":1.0},"config_hash":"contractfixturehash01_contradictory_pair","score":{"parse_success_rate":0.29,"ess_stability":0.95},"reward":0.39,"accepted":true,"previous_hash":"contractfixturehash01_contradictory_pair","error":null,"elapsed_ms":5,"strategy_state":null,"cache_hit":false}
{"event":"run_complete","schema_version":1,"timestamp":"2026-05-17T10:00:04Z","iterations":3,"accepted":2}
"#;

/// Convert IX-shaped iteration lines to `ResearchEvent`s using the same
/// projection logic that `hari_from_ix_autoresearch.rs` applies. We
/// re-implement the small projection here rather than shelling out to
/// the bin so the test runs the same way on every CI lane.
fn project_to_trace(jsonl: &str, target: &str) -> ResearchTrace {
    use hari_core::{Evidence, ResearchEvent, ResearchEventPayload};
    use serde_json::Value;

    let mut events = Vec::new();
    for line in jsonl.lines().filter(|l| !l.trim().is_empty()) {
        let v: Value = serde_json::from_str(line).expect("synthetic JSON parses");
        if v.get("event").and_then(Value::as_str) != Some("iteration") {
            continue;
        }
        let iteration = v.get("iteration").and_then(Value::as_u64).unwrap();
        let config_hash = v.get("config_hash").and_then(Value::as_str).unwrap();
        let accepted = v.get("accepted").and_then(Value::as_bool).unwrap();
        let error = v.get("error").and_then(Value::as_str);
        let reward = v.get("reward").and_then(Value::as_f64);
        let elapsed_ms = v.get("elapsed_ms").and_then(Value::as_u64);
        let cache_hit = v.get("cache_hit").and_then(Value::as_bool).unwrap_or(false);

        let value = match (error, accepted) {
            (Some(_), _) => HexValue::False,
            (None, true) => HexValue::Probable,
            (None, false) => HexValue::Doubtful,
        };

        let proposition = format!(
            "{target}/config-{}-is-an-improvement",
            &config_hash[..config_hash.len().min(12)]
        );

        let mut evidence: Evidence = std::collections::BTreeMap::new();
        if let Some(r) = reward {
            evidence.insert("reward".into(), serde_json::json!(r));
        }
        if let Some(ms) = elapsed_ms {
            evidence.insert("elapsed_ms".into(), serde_json::json!(ms));
        }
        if cache_hit {
            evidence.insert("cache_hit".into(), serde_json::json!(true));
        }
        if let Some(e) = error {
            evidence.insert("error".into(), serde_json::json!(e));
        }
        evidence.insert("config_hash".into(), serde_json::json!(config_hash));

        events.push(ResearchEvent {
            cycle: iteration,
            source: format!("ix-autoresearch:{target}"),
            payload: ResearchEventPayload::ExperimentResult {
                proposition,
                value,
                evidence,
            },
        });
    }

    ResearchTrace {
        dimension: 4,
        events,
    }
}

#[test]
fn synthetic_ix_log_replays_to_contradictory_final_belief() {
    // Phase 2 acceptance: same config_hash with differing `accepted` flags
    // must surface as `HexValue::Contradictory` in Hari's final beliefs —
    // NOT averaged away to Unknown/Probable/Doubtful.
    let trace = project_to_trace(SYNTHETIC_LOG, "target_grammar");
    assert_eq!(trace.events.len(), 3, "3 iteration events expected");

    let mut cl = CognitiveLoop::new(trace.dimension);
    let report = cl.process_research_trace(trace);

    // The derived proposition is shared across all three events because
    // they share the config_hash. Hari should mark it Contradictory.
    let prop = "target_grammar/config-contractfixt-is-an-improvement";
    let final_value = report
        .final_beliefs
        .get(prop)
        .unwrap_or_else(|| panic!("expected belief for {prop}; got {:?}", report.final_beliefs));
    assert_eq!(
        *final_value,
        HexValue::Contradictory,
        "contradictory accepted/rejected pair must surface as Contradictory"
    );
}

#[test]
fn synthetic_ix_log_replays_deterministically() {
    // Phase 2 acceptance: replay is deterministic for the same log.
    let trace_a = project_to_trace(SYNTHETIC_LOG, "target_grammar");
    let trace_b = project_to_trace(SYNTHETIC_LOG, "target_grammar");
    let mut cl_a = CognitiveLoop::new(trace_a.dimension);
    let mut cl_b = CognitiveLoop::new(trace_b.dimension);
    let report_a = cl_a.process_research_trace(trace_a);
    let report_b = cl_b.process_research_trace(trace_b);
    assert_eq!(report_a.final_beliefs, report_b.final_beliefs);
    assert_eq!(report_a.event_count, report_b.event_count);
}

#[test]
fn hari_from_ix_autoresearch_bin_round_trips_synthetic_log() {
    // Phase 2 acceptance: the published CLI from Workflow 3 docs —
    //   cargo run -p hari-extractor --bin hari_from_ix_autoresearch -- --log <run.jsonl> --target <target>
    // — must accept a contract-conformant log without error and emit a
    // ResearchTrace JSON that `hari-core` can replay. This test exercises
    // the actual bin (not just the projection function) so changes to the
    // binary's arg parsing or output format are caught here.
    let dir = tempfile::tempdir().unwrap();
    let log_path = dir.path().join("run.jsonl");
    std::fs::write(&log_path, SYNTHETIC_LOG).unwrap();

    let bin = env!("CARGO_BIN_EXE_hari_from_ix_autoresearch");
    let output = Command::new(bin)
        .arg("--log")
        .arg(&log_path)
        .arg("--target")
        .arg("target_grammar")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("spawn hari_from_ix_autoresearch");

    assert!(
        output.status.success(),
        "bin exited non-zero: stdout={}, stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let trace_json = String::from_utf8(output.stdout).expect("utf-8 stdout");
    let trace: ResearchTrace = serde_json::from_str(&trace_json).expect("parse ResearchTrace");
    assert_eq!(trace.events.len(), 3, "expected 3 events from 3 iterations");
}

#[test]
fn hari_from_ix_autoresearch_skips_malformed_lines_per_contract() {
    // Phase 1 schema crash-tolerance: trailing parse failure is treated
    // as a crash-truncated last line and discarded silently. Verifies that
    // the bin matches the IX-side replay-tolerant contract for partial logs.
    let dir = tempfile::tempdir().unwrap();
    let log_path = dir.path().join("partial.jsonl");
    // Truncate the second iteration mid-line.
    let mut f = std::fs::File::create(&log_path).unwrap();
    let lines: Vec<&str> = SYNTHETIC_LOG.lines().collect();
    writeln!(f, "{}", lines[0]).unwrap(); // run_start
    writeln!(f, "{}", lines[1]).unwrap(); // iteration 0
    // Partial second iteration: truncated mid-JSON (no trailing newline,
    // no closing brace). The bin should skip this line with a warning,
    // not abort.
    write!(f, "{}", &lines[2][..40]).unwrap();
    drop(f);

    let bin = env!("CARGO_BIN_EXE_hari_from_ix_autoresearch");
    let output = Command::new(bin)
        .arg("--log")
        .arg(&log_path)
        .arg("--target")
        .arg("target_grammar")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("spawn hari_from_ix_autoresearch");

    assert!(
        output.status.success(),
        "bin exited non-zero on partial log: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let trace: ResearchTrace = serde_json::from_str(&String::from_utf8_lossy(&output.stdout))
        .expect("partial log still yields a valid trace");
    // One complete iteration line was readable; the partial one was skipped.
    assert_eq!(trace.events.len(), 1);
}
