//! hari#13 — recorded ix-autoresearch runs replayed through Hari.
//!
//! The logs under `fixtures/ix-real-or-synthetic/` are real, unedited
//! `ix-autoresearch run --target grammar --iterations 30 --seed 42` output
//! (Greedy and SA). Tests marked *synthetic* build a stream in memory from a
//! copy of a real log; nothing synthetic is committed as a fixture.

use std::path::PathBuf;

use hari_core::{
    process_research_trace_subjective_logic, CognitiveLoop, PriorityModel, SessionConfig,
    StreamingSession, SubjectiveLogicConfig,
};
use hari_extractor::ix_autoresearch::{
    claim_for, parse_log, project, run_report, ArmSummary, IxLogError, IxRun, IxRunReport,
};
use hari_lattice::HexValue;
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

fn lines(strategy: &str) -> Vec<String> {
    raw(&format!("grammar-{strategy}-seed42.log.jsonl"))
        .lines()
        .map(str::to_string)
        .collect()
}

fn line(text: &str) -> Value {
    serde_json::from_str(text).unwrap()
}

fn arm<'a>(report: &'a IxRunReport, name: &str) -> &'a ArmSummary {
    report
        .arms
        .iter()
        .find(|a| a.arm == name)
        .expect("arm present")
}

#[test]
fn committed_run_reports_regenerate_byte_for_byte_from_committed_logs() {
    for (report_name, runs) in [
        ("grammar-greedy-seed42.report.json", vec![run("greedy")]),
        ("grammar-sa-seed42.report.json", vec![run("sa")]),
        (
            "grammar-greedy-then-sa-seed42.report.json",
            vec![run("greedy"), run("sa")],
        ),
    ] {
        // Exactly what the binary writes: pretty JSON plus a trailing newline.
        // CRLF is normalised because a Windows checkout converts the fixture.
        let regenerated = serde_json::to_string_pretty(&run_report(&runs)).unwrap() + "\n";
        let committed = raw(report_name).replace("\r\n", "\n");
        assert_eq!(committed, regenerated, "{report_name} drifted from its log");
    }
}

#[test]
fn recorded_logs_are_complete_thirty_iteration_runs() {
    for strategy in ["greedy", "sa"] {
        let r = run(strategy);
        assert!(r.complete, "{strategy}: run_complete present");
        assert_eq!(r.iterations.len(), 30);
        assert_eq!(r.seed, 42);
        assert_eq!(r.target, "ix_autoresearch::target_grammar::GrammarTarget");
    }
}

/// hari#13's §6 gate, bound to recordings: a continuous-perturbation target
/// never re-observes a config, so no proposition is asserted twice and no arm
/// can end holding a contradiction.
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

/// Pooling both strategies at one seed repeats the early (config, incumbent)
/// pairs from before the strategies diverge, but they agree.
#[test]
fn pooled_strategies_repeat_claims_but_never_disagree_on_them() {
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

/// The label is IX Greedy's own accept rule, so `ix_policy` scores 0/0 on a
/// Greedy run by construction — the columns carry no information about IX there.
#[test]
fn ix_policy_matches_the_label_by_construction_on_greedy() {
    let report = run_report(&[run("greedy")]);
    let ix = arm(&report, "ix_policy");
    assert_eq!((ix.false_endorsements, ix.missed_improvements), (0, 0));
}

/// Characterisation, not a verdict: one observation fuses to b = 0.55, under
/// SL's 0.7 accept gate, so SL commits to nothing on a single-pass stream, and
/// misses every labeled improvement.
#[test]
fn subjective_logic_withholds_on_every_single_observation_claim() {
    let report = run_report(&[run("sa")]);
    let sl = arm(&report, "subjective_logic");
    assert_eq!(sl.withhold, report.claims);
    assert_eq!(sl.false_endorsements, 0);
    assert_eq!(arm(&report, "ix_policy").false_endorsements, 1);
    assert_eq!(sl.missed_improvements, 4);
}

/// Synthetic. The genuine conflict: the same config judged against the same
/// incumbent twice, with evidence on opposite sides of the incumbent's reward —
/// what a noisy evaluator can produce. Greedy rejects the first evaluation
/// (below the incumbent) and accepts the second (above it). Same proposition,
/// opposite evidence: the hexavalent arm must end `Contradictory` and escalate.
#[test]
fn a_config_judged_twice_against_one_incumbent_with_opposite_evidence_ends_contradictory() {
    let l = lines("greedy");
    // Recorded: it0 accepted, it1 accepted (incumbent becomes it1), it2 rejected
    // against it1.
    let (it1, it2) = (line(&l[2]), line(&l[3]));
    assert_eq!(
        (it1["accepted"].as_bool(), it2["accepted"].as_bool()),
        (Some(true), Some(false))
    );
    let incumbent_reward = it1["reward"].as_f64().unwrap();
    assert!(it2["reward"].as_f64().unwrap() < incumbent_reward);

    let mut again = it2.clone();
    again["iteration"] = Value::from(3);
    again["reward"] = Value::from(incumbent_reward + 0.05);
    again["accepted"] = Value::from(true);
    again["previous_hash"] = it2["config_hash"].clone();
    let spliced = [
        l[0].clone(),
        l[1].clone(),
        l[2].clone(),
        l[3].clone(),
        again.to_string(),
    ]
    .join("\n");

    let parsed = parse_log(&spliced).unwrap();
    let trace = project(std::slice::from_ref(&parsed));
    let proposition = |i: usize| match &trace.events[i].payload {
        hari_core::ResearchEventPayload::ExperimentResult { proposition, .. } => {
            proposition.clone()
        }
        other => panic!("unexpected payload {other:?}"),
    };
    assert_eq!(
        proposition(2),
        proposition(3),
        "same config, same incumbent"
    );

    let report = run_report(&[parsed]);
    assert_eq!(report.conflicting_propositions, 1);
    let decay = arm(&report, "recency_decay");
    assert_eq!(decay.contradictory_final_beliefs, Some(1));
    assert_eq!(
        decay.differs_from_ix_policy, 1,
        "escalates instead of endorsing"
    );
    assert_eq!(
        arm(&report, "ix_unassisted").contradictory_final_beliefs,
        Some(0)
    );
}

/// Synthetic. The false positive the incumbent-scoped claim exists to prevent:
/// re-evaluate an accepted config after the incumbent has moved to it. Greedy
/// correctly rejects it (it cannot beat itself), so it is a different
/// proposition from its earlier acceptance and nothing is contradictory.
#[test]
fn a_greedy_re_evaluation_of_the_incumbent_is_not_a_contradiction() {
    let l = lines("greedy");
    let mut repeat = line(&l[2]);
    assert_eq!(repeat["accepted"], true);
    repeat["iteration"] = Value::from(3);
    repeat["accepted"] = Value::from(false);
    let spliced = [
        l[0].clone(),
        l[1].clone(),
        l[2].clone(),
        l[3].clone(),
        repeat.to_string(),
    ]
    .join("\n");

    let report = run_report(&[parse_log(&spliced).unwrap()]);
    assert_eq!(report.repeated_propositions, 0);
    for a in report.arms.iter().skip(1) {
        assert_eq!(a.contradictory_final_beliefs, Some(0), "{}", a.arm);
    }
}

/// Synthetic. The one place the projection departs from SCHEMA.md's confidence
/// column: an errored evaluation is no evidence either way — `Unknown`, withheld
/// by IX and by the default arm, and unlabeled.
#[test]
fn an_errored_iteration_is_unknown_withheld_and_unlabeled() {
    let l = lines("greedy");
    let clean = run_report(&[run("greedy")]);
    let mut errored = line(&l[7]);
    assert_eq!(errored["accepted"], false);
    errored["error"] = Value::from("eval failed: synthetic");
    errored["reward"] = Value::Null;
    errored["score"] = Value::Null;
    let mut spliced = l.clone();
    spliced[7] = errored.to_string();
    let parsed = parse_log(&spliced.join("\n")).unwrap();

    match &project(std::slice::from_ref(&parsed)).events[6].payload {
        hari_core::ResearchEventPayload::ExperimentResult {
            value, evidence, ..
        } => {
            assert_eq!(*value, HexValue::Unknown);
            assert_eq!(evidence["error"], "eval failed: synthetic");
        }
        other => panic!("unexpected payload {other:?}"),
    }

    let report = run_report(&[parsed]);
    let ix = arm(&report, "ix_policy");
    assert_eq!(
        (ix.reject, ix.withhold),
        (arm(&clean, "ix_policy").reject - 1, 1)
    );
    let decay = arm(&report, "recency_decay");
    assert_eq!(decay.withhold, 1);
    assert_eq!(decay.differs_from_ix_policy, 0);
    // Its own line and the next one (whose incumbent reward is unchanged but
    // whose label still resolves) — only the errored line loses its label.
    assert_eq!(decay.graded, arm(&clean, "recency_decay").graded - 1);
}

/// The projected stream is what a live IX session would send over `serve`:
/// streaming it event by event must reproduce the batch replay. Checked on the
/// pooled stream (the only one with repeated propositions, where merge order
/// matters) under the default arm and the SL arm.
#[test]
fn projected_stream_through_a_serve_session_matches_batch_replay() {
    let trace = project(&[run("greedy"), run("sa")]);

    let stream = |model: PriorityModel| {
        let mut session = StreamingSession::open(SessionConfig {
            dimension: trace.dimension,
            priority_model: model,
            ..SessionConfig::default()
        })
        .expect("session opens");
        for event in trace.events.clone() {
            session.apply_event(event).expect("event accepted");
        }
        serde_json::to_string(&session.close()).unwrap()
    };

    let decay = CognitiveLoop::with_model(trace.dimension, PriorityModel::RecencyDecay)
        .process_research_trace(trace.clone());
    assert_eq!(
        serde_json::to_string(&decay).unwrap(),
        stream(PriorityModel::RecencyDecay)
    );

    // SL arm: per-event outcomes only. The session's closed report for SL does
    // not match batch — `final_beliefs` comes back empty, so the summary and
    // the intrinsic `false_rejection_count` differ too. That gap is in
    // hari-core's `StreamingSession::close` (SL bypasses the primary loop's
    // belief network), predates this adapter, and is not papered over here.
    let sl =
        process_research_trace_subjective_logic(trace.clone(), SubjectiveLogicConfig::default());
    let streamed: Value = serde_json::from_str(&stream(PriorityModel::SubjectiveLogic)).unwrap();
    assert_eq!(
        serde_json::to_value(&sl.outcomes).unwrap(),
        streamed["outcomes"]
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
fn claim_uses_the_full_target_and_the_incumbent() {
    let target = "ix_autoresearch::target_grammar::GrammarTarget";
    assert_eq!(
        claim_for(target, "autoresearch:abc123def456789", None),
        "ix_autoresearch::target_grammar::GrammarTarget/config-abc123def456-is-an-improvement-over-baseline"
    );
    assert_eq!(
        claim_for(target, "autoresearch:abc123def456789", Some("autoresearch:0123456789abcdef")),
        "ix_autoresearch::target_grammar::GrammarTarget/config-abc123def456-is-an-improvement-over-0123456789ab"
    );
}

#[test]
fn crash_truncation_is_tolerated_and_mid_stream_corruption_is_not() {
    let l = lines("greedy");

    let truncated = format!("{}\n{}\n{{\"event\":\"itera", l[0], l[1]);
    let r = parse_log(&truncated).expect("trailing garbage is crash truncation");
    assert_eq!(r.iterations.len(), 1);
    assert!(!r.complete);

    let corrupt = format!("{}\nnot json\n{}", l[0], l[1]);
    assert!(matches!(
        parse_log(&corrupt),
        Err(IxLogError::MidStreamParse { line: 2, .. })
    ));

    let future = l[0].replace("\"schema_version\":1", "\"schema_version\":2");
    assert!(matches!(
        parse_log(&future),
        Err(IxLogError::SchemaVersion { found: 2, .. })
    ));

    assert_eq!(parse_log(&l[1]), Err(IxLogError::MissingRunStart));
}

#[test]
fn two_logs_concatenated_into_one_file_are_rejected() {
    let both = format!(
        "{}\n{}",
        raw("grammar-greedy-seed42.log.jsonl"),
        raw("grammar-sa-seed42.log.jsonl")
    );
    assert!(matches!(
        parse_log(&both),
        Err(IxLogError::Shape { ref detail, .. }) if detail.contains("second run_start")
    ));
}

#[test]
fn a_missing_iteration_line_is_rejected() {
    let mut l = lines("greedy");
    l.remove(10);
    assert!(matches!(
        parse_log(&l.join("\n")),
        Err(IxLogError::Shape { ref detail, .. }) if detail.contains("iteration 10 where 9 was expected")
    ));
}

/// `resume_experiment` appends iterations after a `run_complete` in the same
/// log and closes with its own. Until that lands the run is not complete.
#[test]
fn an_iteration_after_run_complete_reopens_the_run_until_its_own_run_complete() {
    let l = lines("greedy");
    let mut next = line(&l[30]);
    assert_eq!(next["iteration"], 29);
    next["iteration"] = Value::from(30);

    let appended = format!("{}\n{}", l.join("\n"), next);
    let r = parse_log(&appended).unwrap();
    assert_eq!(r.iterations.len(), 31);
    assert!(!r.complete, "appended iteration without its run_complete");

    let resumed = format!("{appended}\n{}", l[31]);
    assert!(parse_log(&resumed).unwrap().complete);
}
