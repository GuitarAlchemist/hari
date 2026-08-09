//! #35 story 15 — *"a reliability-diagram bound pinned as a `probe_*` regression
//! test, so that a later change that silently degrades calibration fails CI."*
//!
//! # Why a diagram bound rather than a Brier bound
//!
//! Mean Brier is a proper score: it folds calibration and discrimination into
//! one number. A change that makes the ledger systematically overconfident can
//! leave Brier almost still, which is exactly the silent degradation story 15
//! names. The reliability diagram separates the two — per-bin
//! `|predicted − observed|` — and `expected_calibration_error` is the scalar a
//! probe can bind.
//!
//! # What this probe does and does not bind
//!
//! It binds the **instrument as a pure function**: `forecast::reliability_diagram`
//! over a synthetic `Vec<ForecastRecord>` built here in memory. It does **not**
//! traverse `ReplayCalibration`, `ResearchReplayReport::with_calibration`, the
//! on-disk forecast ledger, or the report/CLI boundary — `ReplayCalibration`
//! carries no diagram, so there is no such boundary to reach today. Emptying
//! `with_calibration` leaves every test in this file green; the two `lib` tests
//! that pin that wiring are what catch it.
//!
//! It does **not** compare policies, and it does not touch #35 §8 clause 2.
//! Clause 2 needs each arm to emit forecasts from its own posterior (§9 item 2),
//! which does not exist; the forecast ledger is arm-independent. Clause 2 stays
//! `undefined`, and no probe here can or should change that.

use hari_core::forecast::{
    self, reliability_diagram, ForecastRecord, Observable, Outcome, Prediction, Resolution,
};

/// A resolved forecast at a stated probability with a known outcome. Everything
/// but `probability` and `outcome` is fixed, so the diagram is a pure function
/// of the arguments and the probe is deterministic.
fn resolved(id: &str, belief: &str, probability: f64, hit: bool) -> ForecastRecord {
    ForecastRecord {
        schema: "hari-forecast-record-v0.1".to_string(),
        forecast_id: id.to_string(),
        belief_id: belief.to_string(),
        emitted_at: "2026-07-28T00:00:00Z".to_string(),
        observable: Observable {
            source: "ga:state/fleet/presence.json".to_string(),
            field: "/status".to_string(),
            predicate: "== green".to_string(),
        },
        prediction: Prediction {
            probability,
            rationale: None,
        },
        horizon: "2026-07-29T00:00:00Z".to_string(),
        resolution: Some(Resolution {
            resolved_at: "2026-07-29T00:00:00Z".to_string(),
            outcome: if hit { Outcome::True } else { Outcome::False },
            observed_value: None,
            brier: Some((probability - f64::from(u8::from(hit))).powi(2)),
        }),
        links: None,
    }
}

/// A deterministic well-calibrated ledger: at each stated probability `p`, the
/// predicate holds in exactly `p` of the cases. Ten forecasts per decile.
fn well_calibrated_ledger() -> Vec<ForecastRecord> {
    let mut out = Vec::new();
    for tenth in 0..=10u32 {
        let p = f64::from(tenth) / 10.0;
        let hits = tenth as usize; // p of 10 hit, by construction
        for i in 0..10usize {
            out.push(resolved(
                &format!("f-{tenth:02}-{i}"),
                "belief-under-test",
                p,
                i < hits,
            ));
        }
    }
    out
}

/// **The bound.** A well-calibrated ledger must stay well calibrated. The
/// threshold is loose enough that binning artifacts cannot trip it and tight
/// enough that a systematic shift does — see the guard below, which shows a
/// 0.20 confidence shift blowing straight through it.
const MAX_EXPECTED_CALIBRATION_ERROR: f64 = 0.05;

#[test]
fn probe_reliability_diagram_bound_holds_over_a_synthetic_ledger() {
    let ledger = well_calibrated_ledger();
    let diagram = reliability_diagram(&ledger, 10);

    assert_eq!(diagram.scored, 110, "every record must be scored");
    let ece = diagram
        .expected_calibration_error
        .expect("a scored ledger has a calibration error");
    assert!(
        ece <= MAX_EXPECTED_CALIBRATION_ERROR,
        "expected calibration error {ece} exceeds the pinned bound \
         {MAX_EXPECTED_CALIBRATION_ERROR} — calibration degraded"
    );
    let max = diagram.max_calibration_error.expect("scored");
    assert!(
        max <= 0.10,
        "worst single bin {max} — a weighted mean can average one bad bin away"
    );
}

/// The bound must be able to fail. A policy that states every probability 0.20
/// higher than it should is the canonical silent degradation: its ordering is
/// unchanged, so discrimination is unchanged, and only the diagram sees it.
#[test]
fn probe_the_reliability_bound_catches_a_systematic_confidence_shift() {
    let shifted: Vec<ForecastRecord> = well_calibrated_ledger()
        .into_iter()
        .map(|mut r| {
            r.prediction.probability = (r.prediction.probability + 0.20).min(1.0);
            r
        })
        .collect();

    let ece = reliability_diagram(&shifted, 10)
        .expected_calibration_error
        .expect("scored");
    assert!(
        ece > MAX_EXPECTED_CALIBRATION_ERROR,
        "a 0.20 overconfidence shift produced ECE {ece}, which the bound would not catch"
    );
}

/// An unresolved or void forecast has no observed frequency. Counting it as
/// either outcome would invent data, so it is excluded — the same rule
/// `calibration` already applies to `mean_brier`.
#[test]
fn void_and_pending_forecasts_are_excluded_rather_than_guessed() {
    let mut records = vec![resolved("f-hit", "b", 0.9, true)];
    let mut pending = resolved("f-pending", "b", 0.1, false);
    pending.resolution = None;
    let mut void = resolved("f-void", "b", 0.1, false);
    void.resolution = Some(Resolution {
        resolved_at: "2026-07-29T00:00:00Z".to_string(),
        outcome: Outcome::Void,
        observed_value: None,
        brier: None,
    });
    records.push(pending);
    records.push(void);

    let diagram = reliability_diagram(&records, 10);
    assert_eq!(
        diagram.scored, 1,
        "only the resolved non-void record counts"
    );
    let ece = diagram.expected_calibration_error.expect("scored");
    assert!((ece - 0.1).abs() < 1e-12, "ece {ece}");

    // And an entirely unscored ledger has no calibration error at all, which is
    // distinct from a perfect one.
    let empty = reliability_diagram(&[], 10);
    assert_eq!(empty.scored, 0);
    assert_eq!(empty.expected_calibration_error, None);
    assert_eq!(empty.max_calibration_error, None);
}

/// The diagram must agree with the per-belief mean Brier already reported, on
/// the one input where both are computable by hand — otherwise the report would
/// carry two calibration numbers derived from the same ledger that disagree.
#[test]
fn the_diagram_and_the_existing_mean_brier_read_the_same_ledger() {
    let records = vec![
        resolved("f1", "b", 1.0, true),  // brier 0.0
        resolved("f2", "b", 0.0, false), // brier 0.0
    ];
    let per_belief = forecast::calibration(&records);
    assert_eq!(per_belief["b"].scored, 2);
    assert_eq!(per_belief["b"].mean_brier, Some(0.0));

    let diagram = reliability_diagram(&records, 10);
    assert_eq!(diagram.scored, 2);
    assert_eq!(
        diagram.expected_calibration_error,
        Some(0.0),
        "perfect forecasts are perfectly calibrated on both instruments"
    );
    // The top bin must be closed on the right, or a forecast of exactly 1.0
    // falls off the end and is silently dropped.
    let top = diagram.bins.last().expect("ten bins");
    assert_eq!(top.scored, 1);
    assert_eq!(top.observed_frequency, Some(1.0));
}
