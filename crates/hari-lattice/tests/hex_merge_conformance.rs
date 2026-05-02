//! Hex-merge conformance test suite.
//!
//! Runs every JSON fixture in `fixtures/hex-merge/` through
//! `hari_lattice::merge` and asserts the result matches the embedded
//! expected output. Same fixtures are intended to be runnable by
//! `ix-fuzzy::observations::merge` as a follow-up — when both
//! implementations pass against the same corpus, byte-equality is
//! proven by transitivity.
//!
//! The fixture JSON schema is intentionally narrow:
//!
//! ```jsonc
//! {
//!   "name": "human_readable_name",
//!   "description": "what this fixture proves",
//!   "input": {
//!     "observations": [ {source, diagnosis_id, round, ordinal,
//!                        claim_key, variant, weight, evidence?} ],
//!     "current_round": null | u32,
//!     "staleness_k": null | u32
//!   },
//!   "expected": {
//!     "observations_count": usize,
//!     "contradictions_count": usize,
//!     "contradictions": [ {source, claim_key, variant, weight,
//!                          diagnosis_id?} ],   // optional id check
//!     "distribution": { "T": f64, "P": f64, "U": f64,
//!                       "D": f64, "F": f64, "C": f64 },
//!     "escalation_triggered": bool
//!   }
//! }
//! ```
//!
//! Variants use the canonical single-letter wire format (`T`/`P`/
//! `U`/`D`/`F`/`C`) — same as Demerzel's `hexavalent-state.schema.json`
//! and `ix-types::Hexavalent`. Floats are compared within `1e-9`.

use hari_lattice::merge::merge;
use hari_lattice::{HexObservation, HexValue};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
struct Fixture {
    name: String,
    #[serde(default)]
    description: String,
    input: Input,
    expected: Expected,
}

#[derive(Debug, Deserialize)]
struct Input {
    observations: Vec<HexObservation>,
    #[serde(default)]
    current_round: Option<u32>,
    #[serde(default)]
    staleness_k: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct Expected {
    observations_count: usize,
    contradictions_count: usize,
    #[serde(default)]
    contradictions: Vec<ExpectedContradiction>,
    distribution: ExpectedDistribution,
    escalation_triggered: bool,
}

#[derive(Debug, Deserialize)]
struct ExpectedContradiction {
    source: String,
    claim_key: String,
    #[serde(with = "hex_letter")]
    variant: HexValue,
    weight: f64,
    /// Optional — when present, the synthesized contradiction's
    /// `diagnosis_id` must match exactly. This is the strongest
    /// byte-equal claim because it pins the content-derived id
    /// formula (the associativity fix).
    #[serde(default)]
    diagnosis_id: Option<String>,
}

mod hex_letter {
    use hari_lattice::HexValue;
    use serde::{de::Error, Deserialize, Deserializer};

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<HexValue, D::Error> {
        match <&str as Deserialize>::deserialize(d)? {
            "T" => Ok(HexValue::True),
            "P" => Ok(HexValue::Probable),
            "U" => Ok(HexValue::Unknown),
            "D" => Ok(HexValue::Doubtful),
            "F" => Ok(HexValue::False),
            "C" => Ok(HexValue::Contradictory),
            other => Err(D::Error::custom(format!(
                "variant must be T/P/U/D/F/C, got {other:?}"
            ))),
        }
    }
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
struct ExpectedDistribution {
    T: f64,
    P: f64,
    U: f64,
    D: f64,
    F: f64,
    C: f64,
}

const TOL: f64 = 1e-9;

fn fixture_dir() -> PathBuf {
    // `CARGO_MANIFEST_DIR` is the crate root; fixtures live two
    // levels up at the workspace root.
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR set in tests");
    Path::new(&manifest)
        .ancestors()
        .nth(2)
        .expect("workspace root above crate")
        .join("fixtures")
        .join("hex-merge")
}

fn load_fixtures() -> Vec<(PathBuf, Fixture)> {
    let dir = fixture_dir();
    assert!(
        dir.is_dir(),
        "expected fixture directory at {}",
        dir.display()
    );
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("read fixture dir") {
        let path = entry.expect("dirent").path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let fixture: Fixture =
            serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
        out.push((path, fixture));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    assert!(
        !out.is_empty(),
        "no fixtures found in {} — at least one is required",
        dir.display()
    );
    out
}

#[test]
fn every_fixture_matches_expected() {
    for (path, fixture) in load_fixtures() {
        check(&path, &fixture);
    }
}

fn check(path: &Path, fx: &Fixture) {
    let state = merge(
        &fx.input.observations,
        fx.input.current_round,
        fx.input.staleness_k,
    );
    let label = if fx.description.is_empty() {
        format!("{} ({})", fx.name, path.display())
    } else {
        format!("{} — {} ({})", fx.name, fx.description, path.display())
    };

    assert_eq!(
        state.observations.len(),
        fx.expected.observations_count,
        "{label}: observations_count mismatch (got obs {:#?})",
        state.observations
    );
    assert_eq!(
        state.contradictions.len(),
        fx.expected.contradictions_count,
        "{label}: contradictions_count mismatch (got contradictions {:#?})",
        state.contradictions
    );

    // Match each expected contradiction to an actual one. We don't
    // assume positional order — match by (source, claim_key, variant)
    // and check weight + optional diagnosis_id.
    for expected in &fx.expected.contradictions {
        let actual = state
            .contradictions
            .iter()
            .find(|c| {
                c.source == expected.source
                    && c.claim_key == expected.claim_key
                    && c.variant == expected.variant
            })
            .unwrap_or_else(|| {
                panic!(
                    "{label}: no contradiction matching {{source={:?}, claim_key={:?}, variant={:?}}}; got {:#?}",
                    expected.source, expected.claim_key, expected.variant, state.contradictions
                )
            });
        assert!(
            (actual.weight - expected.weight).abs() < TOL,
            "{label}: weight mismatch on {} (expected {}, got {})",
            expected.claim_key,
            expected.weight,
            actual.weight
        );
        if let Some(want_id) = &expected.diagnosis_id {
            assert_eq!(
                &actual.diagnosis_id, want_id,
                "{label}: diagnosis_id mismatch on {}",
                expected.claim_key
            );
        }
    }

    // Distribution masses.
    let dist = &state.distribution;
    let pairs = [
        (HexValue::True, fx.expected.distribution.T, "T"),
        (HexValue::Probable, fx.expected.distribution.P, "P"),
        (HexValue::Unknown, fx.expected.distribution.U, "U"),
        (HexValue::Doubtful, fx.expected.distribution.D, "D"),
        (HexValue::False, fx.expected.distribution.F, "F"),
        (HexValue::Contradictory, fx.expected.distribution.C, "C"),
    ];
    for (variant, expected_mass, sym) in pairs {
        let actual = dist.get(variant);
        assert!(
            (actual - expected_mass).abs() < TOL,
            "{label}: distribution[{sym}] mismatch (expected {expected_mass}, got {actual})"
        );
    }

    assert_eq!(
        dist.escalation_triggered(),
        fx.expected.escalation_triggered,
        "{label}: escalation_triggered mismatch"
    );
}
