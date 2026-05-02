//! Subjective Logic as a first-class `PriorityModel` variant.
//!
//! Post-Phase-5 substrate decision (Lie demoted), SL was the data-best
//! non-Lie option but ran as a separate `process_research_trace_subjective_logic`
//! pipeline — unreachable from `SessionConfig.priority_model`. This
//! suite pins the promotion:
//!
//! 1. **Parity:** running the same trace through
//!    `CognitiveLoop::with_model(N, SubjectiveLogic)` produces outcomes
//!    byte-equal to the standalone `process_research_trace_subjective_logic`.
//! 2. **Streaming reachability:** `SessionConfig` with
//!    `priority_model = "SubjectiveLogic"` routes a session through SL
//!    end-to-end.
//! 3. **JSON wire format:** `PriorityModel::SubjectiveLogic` round-trips
//!    via serde as `"SubjectiveLogic"`.
//! 4. **Different from RecencyDecay:** the data justification — SL
//!    produces a different action stream than the new default on
//!    `swarm_dissent.json`.

use hari_core::{
    process_research_trace_subjective_logic, CognitiveLoop, PriorityModel, ResearchEvent,
    ResearchTrace, SessionConfig, StreamingSession, SubjectiveLogicConfig,
};
use std::fs;

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

// ---------------------------------------------------------------------------
// 1. Parity: SubjectiveLogic via CognitiveLoop matches standalone SL pipeline
// ---------------------------------------------------------------------------

#[test]
fn cognitive_loop_subjective_logic_matches_standalone_sl_pipeline() {
    // Three substantive fixtures with different shapes (multi-agent
    // votes, contradiction recovery, conflict signals). Both paths
    // must produce byte-equal per-event outcomes; aggregated metrics
    // can differ in fields that the two pipelines compute differently
    // (the standalone SL pipeline mirrors `final_state_summary` with a
    // `[subjective-logic]` prefix, whereas the CognitiveLoop path
    // uses CognitiveState::summary). We pin the per-event outcomes
    // explicitly since those are the contract IX clients consume.
    for path in [
        "../../fixtures/ix/swarm_dissent.json",
        "../../fixtures/ix/long_recovery.json",
        "../../fixtures/ix/cognition_divergence.json",
    ] {
        let trace = load_trace(path);
        let cfg = SubjectiveLogicConfig::default();

        // Standalone pipeline.
        let standalone = process_research_trace_subjective_logic(trace.clone(), cfg);

        // CognitiveLoop path.
        let mut loop_ = CognitiveLoop::with_model(trace.dimension, PriorityModel::SubjectiveLogic);
        loop_.sl_config = cfg;
        let via_loop = loop_.process_research_trace(trace);

        assert_eq!(
            standalone.outcomes.len(),
            via_loop.outcomes.len(),
            "{path}: outcome counts must match"
        );
        for (i, (s, l)) in standalone
            .outcomes
            .iter()
            .zip(via_loop.outcomes.iter())
            .enumerate()
        {
            // The actions list is the load-bearing contract for IX.
            let s_json = serde_json::to_string(&s.actions).unwrap();
            let l_json = serde_json::to_string(&l.actions).unwrap();
            assert_eq!(
                s_json, l_json,
                "{path}: per-event actions must match at event {i}"
            );
            // Event itself must be the same (preserves source/cycle/payload).
            assert_eq!(
                serde_json::to_string(&s.event).unwrap(),
                serde_json::to_string(&l.event).unwrap(),
                "{path}: event field must match at event {i}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 2. Streaming session: priority_model="SubjectiveLogic" routes correctly
// ---------------------------------------------------------------------------

#[test]
fn streaming_session_routes_to_subjective_logic_when_configured() {
    let trace = load_trace("../../fixtures/ix/swarm_dissent.json");
    let mut session = StreamingSession::open(SessionConfig {
        dimension: trace.dimension,
        priority_model: PriorityModel::SubjectiveLogic,
        ..SessionConfig::default()
    })
    .expect("open SL session");

    // Drive the trace.
    for ev in trace.events.iter() {
        session.apply_event(ev.clone()).expect("apply SL event");
    }
    let report = session.close();

    // Sanity: SL emits SL-prefixed Log actions for fused beliefs, so
    // the outcome stream should contain at least one such marker
    // confirming we actually went through SL.
    let any_sl_log = report.outcomes.iter().any(|o| {
        o.actions
            .iter()
            .any(|a| matches!(a, hari_core::Action::Log(s) if s.starts_with("SL fused")))
    });
    assert!(
        any_sl_log,
        "streaming SL session must emit at least one 'SL fused' log; got: {:?}",
        report.outcomes.first().map(|o| &o.actions)
    );
}

// ---------------------------------------------------------------------------
// 3. JSON wire format: SubjectiveLogic variant round-trips
// ---------------------------------------------------------------------------

#[test]
fn subjective_logic_priority_model_round_trips_via_json() {
    // Pin the JSON shape so an IX consumer can rely on it.
    let raw = r#"{
        "dimension": 4,
        "priority_model": "SubjectiveLogic"
    }"#;
    let cfg: SessionConfig =
        serde_json::from_str(raw).expect("SessionConfig with SubjectiveLogic must parse");
    assert_eq!(cfg.priority_model, PriorityModel::SubjectiveLogic);

    let s = serde_json::to_string(&cfg).unwrap();
    assert!(
        s.contains("\"priority_model\":\"SubjectiveLogic\""),
        "PriorityModel::SubjectiveLogic must serialize as \"SubjectiveLogic\"; got: {s}"
    );

    // Sanity: round-tripping the four PriorityModel variants is stable.
    for variant in [
        PriorityModel::Flat,
        PriorityModel::RecencyDecay,
        PriorityModel::Lie,
        PriorityModel::SubjectiveLogic,
    ] {
        let s = serde_json::to_string(&variant).unwrap();
        let back: PriorityModel = serde_json::from_str(&s).unwrap();
        assert_eq!(variant, back, "round-trip on {variant:?}");
    }
}

// ---------------------------------------------------------------------------
// 4. SubjectiveLogic produces a different action stream than RecencyDecay
//    on the same fixture — the data justification for promoting it.
// ---------------------------------------------------------------------------

#[test]
fn subjective_logic_diverges_from_recency_decay_on_swarm_dissent() {
    let trace = load_trace("../../fixtures/ix/swarm_dissent.json");

    let mut sl_loop = CognitiveLoop::with_model(trace.dimension, PriorityModel::SubjectiveLogic);
    let sl_report = sl_loop.process_research_trace(trace.clone());

    let mut rd_loop = CognitiveLoop::with_model(trace.dimension, PriorityModel::RecencyDecay);
    let rd_report = rd_loop.process_research_trace(trace);

    // Same event count.
    assert_eq!(sl_report.outcomes.len(), rd_report.outcomes.len());

    // Different per-event action streams.
    let sl_actions: Vec<_> = sl_report
        .outcomes
        .iter()
        .map(|o| serde_json::to_string(&o.actions).unwrap())
        .collect();
    let rd_actions: Vec<_> = rd_report
        .outcomes
        .iter()
        .map(|o| serde_json::to_string(&o.actions).unwrap())
        .collect();

    assert_ne!(
        sl_actions, rd_actions,
        "SubjectiveLogic and RecencyDecay must produce different action streams \
         on swarm_dissent.json — the empirical justification for promoting SL"
    );
}
