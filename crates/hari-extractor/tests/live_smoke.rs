//! Live smoke test against real Mercury 2.
//!
//! Skips when `INCEPTION_API_KEY` is unset, so a fresh clone without
//! credentials sees a passing test run (the `#[test]` is `ignored` if the
//! env var is missing). In CI, the GitHub repo secret of the same name is
//! mapped to the env var, so this test exercises the full path.

use hari_core::ResearchEventPayload;
use hari_extractor::{MercuryConfig, MercuryExtractor, API_KEY_ENV_VAR};
use hari_lattice::HexValue;

fn key_available() -> bool {
    std::env::var(API_KEY_ENV_VAR)
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
}

#[tokio::test]
async fn extract_belief_update_from_research_note() {
    if !key_available() {
        eprintln!("Skipping: {API_KEY_ENV_VAR} not set");
        return;
    }
    let cfg = MercuryConfig::from_env().expect("env-configured key present");
    let extractor = MercuryExtractor::new(cfg).expect("extractor builds");

    // Canonical IX-autoresearch shape: an evaluator agent reporting a
    // stable benchmark result with a hedged confidence.
    let note = "Agent ix-agent-evaluator: benchmark-x ran 5 times and held a stable pass-rate. \
                Probably reliable.";

    let event = extractor
        .extract(42, "ix-agent-evaluator", note)
        .await
        .expect("Mercury extracts cleanly");

    assert_eq!(event.cycle, 42);
    assert_eq!(event.source, "ix-agent-evaluator");

    match event.payload {
        ResearchEventPayload::BeliefUpdate {
            proposition,
            value,
            evidence,
        }
        | ResearchEventPayload::ExperimentResult {
            proposition,
            value,
            evidence,
        } => {
            // The proposition must mention benchmark-x — exact wording is
            // model-dependent, but the canonical claim is non-negotiable.
            assert!(
                proposition.to_lowercase().contains("benchmark-x")
                    || proposition.to_lowercase().contains("benchmark x"),
                "proposition should reference benchmark-x, got: {proposition}"
            );
            // "Probably" maps to Probable, not True — system prompt
            // explicitly handles this hedge.
            assert!(
                matches!(value, HexValue::Probable | HexValue::True),
                "hedged 'probably reliable' should map to Probable (True acceptable), got: {value:?}"
            );
            // The 5-runs count should land somewhere in the evidence map.
            let has_runs = evidence
                .get("runs")
                .and_then(|v| v.as_u64())
                .map(|n| n >= 1)
                .unwrap_or(false);
            assert!(
                has_runs,
                "evidence should include a runs count >= 1, got: {evidence:?}"
            );
        }
        other => panic!("expected BeliefUpdate or ExperimentResult, got: {other:?}"),
    }
}

#[tokio::test]
async fn extract_retraction_carries_reason() {
    if !key_available() {
        eprintln!("Skipping: {API_KEY_ENV_VAR} not set");
        return;
    }
    let cfg = MercuryConfig::from_env().unwrap();
    let extractor = MercuryExtractor::new(cfg).unwrap();

    let note = "Retract the earlier claim that benchmark-x is reliable. \
                The result changes after prompt paraphrase, so the prior conclusion is invalid.";

    let event = extractor
        .extract(43, "ix-agent-critic", note)
        .await
        .expect("Mercury extracts cleanly");

    match event.payload {
        ResearchEventPayload::Retraction {
            proposition,
            reason,
            ..
        } => {
            assert!(proposition.to_lowercase().contains("benchmark-x"));
            assert!(!reason.is_empty(), "reason should be non-empty");
        }
        // Mercury might also model this as a BeliefUpdate with value=False
        // or Doubtful — that's a reasonable interpretation too. Accept both.
        ResearchEventPayload::BeliefUpdate { value, .. } => {
            assert!(
                matches!(
                    value,
                    HexValue::False | HexValue::Doubtful | HexValue::Contradictory
                ),
                "if not a Retraction, value should be False/Doubtful/Contradictory, got: {value:?}"
            );
        }
        other => panic!("expected Retraction or down-weighting BeliefUpdate, got: {other:?}"),
    }
}
