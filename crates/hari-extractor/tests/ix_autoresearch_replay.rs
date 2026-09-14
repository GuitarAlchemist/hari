//! hari#13 — recorded ix-autoresearch runs replayed through Hari.
//!
//! The logs under `fixtures/ix-real-or-synthetic/` are real, unedited
//! `ix-autoresearch run --target grammar --iterations 100 --seed 42` output
//! (Greedy and SA). Every assertion below runs on those recordings, except the
//! conflicting-repeat test, which splices a repeat into a copy in memory because
//! no recorded run contains one — which is itself what the recorded tests pin.

use std::path::PathBuf;

use hari_core::{CognitiveLoop, PriorityModel, SessionConfig, StreamingSession};
use hari_extractor::ix_autoresearch::{
    claim_for, parse_log, project, run_report, IxLogError, IxRun, IxRunReport,
};
use serde_json::Value;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/ix-real-or-synthetic")
        .join(name)
}

fn raw(name: &str) -> String {
    std::fs::read_to_string(fixture(name)).expect("fixture readable")
}

fn run(strategy: &str) -> IxRun {
    parse_log(&raw(&format!("grammar-{strategy}-seed42.log.jsonl"))).expect("recorded log parses")
}

fn arm<'a>(report: &'a IxRunReport, name: &str) -> &'a hari_extractor::ix_autoresearch::ArmSummary {
    report
        .arms
        .iter()
        .find(|a| a.arm == name)
        .expect("arm present")
}

#[test]
fn committed_run_reports_regenerate_from_committed_logs() {
    for (report_name, runs) in [
        ("grammar-greedy-seed42.report.json", vec![run("greedy")]),
        ("grammar-sa-seed42.report.json", vec![run("sa")]),
        (
            "grammar-greedy-then-sa-seed42.report.json",
            vec![run("greedy"), run("sa")],
        ),
    ] {
        let committed: Value = serde_json::from_str(&raw(report_name)).unwrap();
        let regenerated = serde_json::to_value(run_report(&runs)).unwrap();
        assert_eq!(committed, regenerated, "{report_name} drifted from its log");
    }
}

#[test]
fn recorded_logs_are_complete_hundred_iteration_runs() {
    for strategy in ["greedy", "sa"] {
        let r = run(strategy);
        assert!(r.complete, "{strategy}: run_complete present");
        assert_eq!(r.iterations.len(), 100);
        assert_eq!(r.seed, 42);
        assert_eq!(r.target, "ix_autoresearch::target_grammar::GrammarTarget");
    }
}

/// hari#13's §6 abandon gate, bound to recordings instead of a deleted probe:
/// a continuous-perturbation target never re-observes a config, so no
/// proposition is asserted twice and no arm can end holding a contradiction.
#[test]
fn a_recorded_grammar_run_never_asserts_a_proposition_twice() {
    for strategy in ["greedy", "sa"] {
        let report = run_report(&[run(strategy)]);
        assert_eq!(report.distinct_propositions, report.claims, "{strategy}");
        assert_eq!(report.repeated_propositions, 0, "{strategy}");
        for a in report.arms.iter().skip(1) {
            assert_eq!(
                a.contradictory_final_beliefs,
                Some(0),
                "{strategy}/{}",
                a.arm
            );
        }
    }
}

/// Pooling both strategies at one seed does repeat configs — the early
/// iterations before the strategies diverge — but they agree, so there is
/// still nothing to contradict.
#[test]
fn pooled_strategies_repeat_configs_but_never_disagree_on_them() {
    let report = run_report(&[run("greedy"), run("sa")]);
    assert_eq!(report.repeated_propositions, 3);
    assert_eq!(report.conflicting_propositions, 0);
}

/// §9.6 on recorded data: the shipped default and pass-through both decide
/// exactly what IX decided, at every iteration.
#[test]
fn neither_the_default_arm_nor_pass_through_departs_from_ix_on_a_recorded_run() {
    for runs in [
        vec![run("greedy")],
        vec![run("sa")],
        vec![run("greedy"), run("sa")],
    ] {
        let report = run_report(&runs);
        for name in ["ix_unassisted", "recency_decay"] {
            let a = arm(&report, name);
            assert_eq!(a.differs_from_ix_policy, 0, "{name}");
            assert_eq!(
                a.false_endorsements,
                arm(&report, "ix_policy").false_endorsements
            );
        }
    }
}

/// Characterisation, not a verdict: one observation fuses to b = 0.55, under
/// SL's 0.7 accept gate, so SL commits to nothing on a single-pass stream. Its
/// zero false endorsements are bought by missing every real improvement.
#[test]
fn subjective_logic_withholds_on_every_single_observation_claim() {
    let report = run_report(&[run("sa")]);
    let sl = arm(&report, "subjective_logic");
    assert_eq!(sl.withhold, report.claims);
    assert_eq!(sl.false_endorsements, 0);
    assert_eq!(arm(&report, "ix_policy").false_endorsements, 2);
    assert_eq!(sl.missed_improvements, 10);
}

/// SCHEMA.md's "contradictory findings preserved" criterion, which no recorded
/// run can exercise: splice a repeat of iteration 1 with its accept flag
/// flipped, and the hexavalent arm must end `Contradictory` and escalate.
#[test]
fn a_conflicting_repeat_of_one_config_ends_contradictory() {
    let log = raw("grammar-greedy-seed42.log.jsonl");
    let lines: Vec<&str> = log.lines().collect();
    let mut repeat: Value = serde_json::from_str(lines[2]).unwrap();
    assert_eq!(repeat["iteration"], 1);
    repeat["iteration"] = Value::from(3);
    repeat["accepted"] = Value::from(!repeat["accepted"].as_bool().unwrap());
    let spliced = [lines[0], lines[1], lines[2], lines[3], &repeat.to_string()].join("\n");

    let report = run_report(&[parse_log(&spliced).unwrap()]);
    assert_eq!(report.conflicting_propositions, 1);
    let decay = arm(&report, "recency_decay");
    assert_eq!(decay.contradictory_final_beliefs, Some(1));
    assert_eq!(
        decay.differs_from_ix_policy, 1,
        "the repeat escalates instead of rejecting"
    );
    assert_eq!(
        arm(&report, "ix_unassisted").contradictory_final_beliefs,
        Some(0)
    );
}

/// The projected stream is what a live IX session would send over `serve`:
/// streaming it event by event must reproduce the batch replay exactly.
#[test]
fn projected_stream_through_a_serve_session_matches_batch_replay() {
    let trace = project(&[run("sa")]);
    let batch = CognitiveLoop::with_model(trace.dimension, PriorityModel::RecencyDecay)
        .process_research_trace(trace.clone());

    let mut session = StreamingSession::open(SessionConfig {
        dimension: trace.dimension,
        priority_model: PriorityModel::RecencyDecay,
        ..SessionConfig::default()
    })
    .expect("session opens");
    for event in trace.events {
        session.apply_event(event).expect("event accepted");
    }
    assert_eq!(
        serde_json::to_string(&batch).unwrap(),
        serde_json::to_string(&session.close()).unwrap()
    );
}

#[test]
fn projection_is_deterministic_and_stamps_position() {
    let a = serde_json::to_string(&project(&[run("greedy"), run("sa")])).unwrap();
    let b = serde_json::to_string(&project(&[run("greedy"), run("sa")])).unwrap();
    assert_eq!(a, b);
    let trace = project(&[run("greedy")]);
    assert!(trace
        .events
        .iter()
        .enumerate()
        .all(|(i, e)| e.cycle == i as u64 + 1));
}

#[test]
fn claim_matches_the_schema_md_example() {
    assert_eq!(
        claim_for(
            "ix_autoresearch::target_grammar::GrammarTarget",
            "autoresearch:abc123def456789"
        ),
        "target_grammar/config-abc123def456-is-an-improvement"
    );
}

#[test]
fn crash_truncation_is_tolerated_and_mid_stream_corruption_is_not() {
    let log = raw("grammar-greedy-seed42.log.jsonl");
    let lines: Vec<&str> = log.lines().collect();

    let truncated = format!("{}\n{}\n{{\"event\":\"itera", lines[0], lines[1]);
    let r = parse_log(&truncated).expect("trailing garbage is crash truncation");
    assert_eq!(r.iterations.len(), 1);
    assert!(!r.complete);

    let corrupt = format!("{}\nnot json\n{}", lines[0], lines[1]);
    assert!(matches!(
        parse_log(&corrupt),
        Err(IxLogError::MidStreamParse { line: 2, .. })
    ));

    let future = lines[0].replace("\"schema_version\":1", "\"schema_version\":2");
    assert!(matches!(
        parse_log(&future),
        Err(IxLogError::SchemaVersion { found: 2, .. })
    ));

    assert_eq!(parse_log(lines[1]), Err(IxLogError::MissingRunStart));
}
