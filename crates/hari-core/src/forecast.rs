//! # Forecast records — Jarvis Track J2 tracer bullet
//!
//! Gives hari the *prediction* half of its world model: a belief emits a
//! testable expectation (probability + horizon + pinned observable), the
//! scorer matches the outcome at horizon and writes a Brier score, and the
//! calibration ledger accumulates per-belief error over time.
//!
//! Implements the GA-authored coordination contract
//! `ga:docs/contracts/2026-07-02-hari-forecast-record.contract.md`
//! (schema `hari-forecast-record-v0.1.0`, **v0.1 DRAFT** — tribunal pending).
//! hari-side answers to the contract's open questions:
//! `docs/research/2026-07-02-forecast-record-hari-review.md`.
//!
//! Design constraints inherited from the contract:
//! - The observable is pinned at emission time; scoring is mechanical.
//!   Anything that would require judgement resolves as `void`.
//! - Contradictory beliefs each emit their own forecast on the same
//!   observable; resolution pressure comes from the Brier ledger.
//! - `void` is a first-class outcome — an unreadable observable at horizon
//!   is recorded, never silently dropped.
//! - One forecast, one resolution. The ledger is append-only JSONL: the
//!   scorer appends a *resolved copy* of the record (same `forecast_id`),
//!   and readers keep the last record per `forecast_id`.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::hash_map::RandomState;
use std::collections::BTreeMap;
use std::fs;
use std::hash::{BuildHasher, Hasher};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Locked once the contract freezes (v0.1 is still DRAFT).
pub const FORECAST_SCHEMA: &str = "hari-forecast-record-v0.1.0";

/// WHERE the outcome will be read from, pinned at emission time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Observable {
    /// Repo-relative artifact path in the producing repo
    /// (e.g. `ga:state/fleet/presence.json`).
    pub source: String,
    /// JSON-pointer-ish path within the artifact. Segments are `/`-separated;
    /// a `key=value` segment selects the first array element whose `key`
    /// string-equals `value` (e.g. `/limbs/id=sensor:quality-snapshot/status`).
    pub field: String,
    /// The claim being forecast: `== <literal>` or `!= <literal>`.
    pub predicate: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Prediction {
    /// P(predicate holds at horizon). 0.5 is an honest "no idea".
    pub probability: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
}

/// Contract enum: serialized as `"true"` / `"false"` / `"void"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    True,
    False,
    Void,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Resolution {
    pub resolved_at: String,
    pub outcome: Outcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observed_value: Option<String>,
    /// `(probability - outcome)^2` with outcome in {0,1}; absent for void.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brier: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Links {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ForecastRecord {
    pub schema: String,
    pub forecast_id: String,
    pub belief_id: String,
    pub emitted_at: String,
    pub observable: Observable,
    pub prediction: Prediction,
    pub horizon: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution: Option<Resolution>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub links: Option<Links>,
}

impl ForecastRecord {
    pub fn is_pending(&self) -> bool {
        self.resolution.is_none()
    }

    /// Horizons and `now` must both be UTC `Z`-suffixed RFC 3339 strings
    /// (the only form this module emits), so lexicographic order is
    /// chronological order.
    pub fn is_past_horizon(&self, now: &str) -> bool {
        self.horizon.as_str() <= now
    }
}

/// Build a new pending record. `emitted_at` is injected (not read from the
/// clock) so emission is deterministic under test.
pub fn emit(
    belief_id: &str,
    observable: Observable,
    probability: f64,
    rationale: Option<String>,
    horizon: &str,
    supersedes: Option<String>,
    emitted_at: &str,
) -> ForecastRecord {
    ForecastRecord {
        schema: FORECAST_SCHEMA.to_string(),
        forecast_id: uuidv7(),
        belief_id: belief_id.to_string(),
        emitted_at: emitted_at.to_string(),
        observable,
        prediction: Prediction {
            probability,
            rationale,
        },
        horizon: horizon.to_string(),
        resolution: None,
        links: supersedes.map(|s| Links {
            supersedes: Some(s),
        }),
    }
}

/// Walk the contract's pointer-ish `field` syntax. Returns `None` on any
/// miss — the caller maps that to a `void` outcome.
pub fn lookup_field<'a>(doc: &'a Value, field: &str) -> Option<&'a Value> {
    let mut cur = doc;
    for seg in field.split('/').filter(|s| !s.is_empty()) {
        cur = match cur {
            Value::Object(map) => map.get(seg)?,
            Value::Array(items) => {
                if let Some((key, want)) = seg.split_once('=') {
                    items
                        .iter()
                        .find(|it| it.get(key).and_then(Value::as_str) == Some(want))?
                } else {
                    items.get(seg.parse::<usize>().ok()?)?
                }
            }
            _ => return None,
        };
    }
    Some(cur)
}

/// Scalar projection used on both sides of a predicate. Strings compare
/// unquoted; numbers and booleans via their JSON text; arrays/objects have
/// no scalar form (predicate evaluation returns `None` → void).
fn scalar_string(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Bool(_) | Value::Number(_) | Value::Null => Some(value.to_string()),
        Value::Array(_) | Value::Object(_) => None,
    }
}

/// Evaluate `== <literal>` / `!= <literal>` against a field value.
/// `None` means the predicate could not be scored mechanically.
pub fn eval_predicate(predicate: &str, value: &Value) -> Option<bool> {
    let p = predicate.trim();
    let (negated, rhs) = if let Some(r) = p.strip_prefix("==") {
        (false, r)
    } else {
        (true, p.strip_prefix("!=")?)
    };
    let lhs = scalar_string(value)?;
    Some((lhs == rhs.trim()) != negated)
}

/// Score one record against the artifact content (or its absence).
/// Unreadable artifact, missing field, or unscorable predicate → `void`.
pub fn resolve(record: &ForecastRecord, artifact: Option<&Value>, resolved_at: &str) -> Resolution {
    let observed = artifact.and_then(|doc| lookup_field(doc, &record.observable.field));
    let observed_value = observed.and_then(scalar_string);
    match observed.and_then(|v| eval_predicate(&record.observable.predicate, v)) {
        None => Resolution {
            resolved_at: resolved_at.to_string(),
            outcome: Outcome::Void,
            observed_value,
            brier: None,
        },
        Some(holds) => {
            let target = if holds { 1.0 } else { 0.0 };
            Resolution {
                resolved_at: resolved_at.to_string(),
                outcome: if holds { Outcome::True } else { Outcome::False },
                observed_value,
                brier: Some((record.prediction.probability - target).powi(2)),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Ledger — append-only JSONL, one file per emission day, under
// `HARI_STATE_DIR/forecasts/` (default `state/forecasts/`).
// ---------------------------------------------------------------------------

pub fn ledger_dir() -> PathBuf {
    std::env::var_os("HARI_STATE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("state"))
        .join("forecasts")
}

/// Append one record to the day file named after its `emitted_at` date.
/// Resolved copies land in the same file as their pending original, so
/// last-record-per-id semantics hold within one file scan.
pub fn append(dir: &Path, record: &ForecastRecord) -> io::Result<PathBuf> {
    // Ordering-critical fields must be canonical before they hit the ledger —
    // see `is_canonical_utc` for the fractional-seconds hazard.
    for (name, value) in [
        ("emitted_at", &record.emitted_at),
        ("horizon", &record.horizon),
    ] {
        if !is_canonical_utc(value) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{name} is not canonical YYYY-MM-DDTHH:MM:SSZ UTC: {value:?}"),
            ));
        }
    }
    fs::create_dir_all(dir)?;
    let path = dir.join(format!("{}.jsonl", &record.emitted_at[..10]));
    let line =
        serde_json::to_string(record).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    writeln!(file, "{line}")?;
    Ok(path)
}

/// Load the whole ledger, keeping the *last* record per `forecast_id`
/// (a resolved copy supersedes its pending original). Malformed lines are
/// skipped with a count so a corrupt line can't take the ledger down.
pub fn load(dir: &Path) -> io::Result<(Vec<ForecastRecord>, usize)> {
    let mut by_id: BTreeMap<String, (usize, ForecastRecord)> = BTreeMap::new();
    let mut skipped = 0usize;
    let mut seq = 0usize;
    if !dir.exists() {
        return Ok((Vec::new(), 0));
    }
    let mut files: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
        .collect();
    files.sort();
    for path in files {
        for line in fs::read_to_string(&path)?.lines() {
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<ForecastRecord>(line) {
                Ok(rec) => {
                    by_id.insert(rec.forecast_id.clone(), (seq, rec));
                    seq += 1;
                }
                Err(_) => skipped += 1,
            }
        }
    }
    let mut records: Vec<(usize, ForecastRecord)> = by_id.into_values().collect();
    records.sort_by_key(|(s, _)| *s);
    Ok((records.into_iter().map(|(_, r)| r).collect(), skipped))
}

/// The tripwire condition: forecasts whose horizon has passed but which
/// still carry no resolution. A healthy ledger returns empty — every past
/// forecast has been scored (or superseded). Read-only; `now` is injected
/// so CI can run it deterministically. Order matches ledger order.
///
/// This is the "writer is the CI check" loop shape from the compounding
/// strategy doc §4: an overdue-unresolved forecast is drift, and surfacing
/// it as a build failure is what stops the prediction from rotting silently
/// (issue #29).
pub fn overdue_unresolved<'a>(records: &'a [ForecastRecord], now: &str) -> Vec<&'a ForecastRecord> {
    records
        .iter()
        .filter(|r| r.is_pending() && r.is_past_horizon(now))
        .collect()
}

// ---------------------------------------------------------------------------
// Calibration — v0.1 minimum viable output: per-belief Brier mean + count
// (contract open question 4, mean + count accepted).
// ---------------------------------------------------------------------------

// `Deserialize` is required because `ResearchReplayReport` embeds this type
// and that report round-trips through JSON at the IX boundary; `default` on
// `mean_brier` keeps the skipped-when-`None` form readable back.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BeliefCalibration {
    pub scored: usize,
    pub void: usize,
    pub pending: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mean_brier: Option<f64>,
}

/// One bin of a reliability diagram: forecasts whose stated probability fell in
/// `[lower, upper)`, and how often the predicate actually held.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ReliabilityBin {
    /// Inclusive lower edge of the predicted-probability bin.
    pub lower: f64,
    /// Exclusive upper edge — except the top bin, which includes `1.0`.
    pub upper: f64,
    /// Scored forecasts landing in this bin.
    pub scored: usize,
    /// Mean stated probability over those forecasts. `None` when the bin is
    /// empty — an empty bin has no forecast, which is not a forecast of zero.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mean_predicted: Option<f64>,
    /// Fraction of them that resolved `true` — the diagram's y-axis.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_frequency: Option<f64>,
}

impl ReliabilityBin {
    /// `|mean_predicted − observed_frequency|`. `None` for an empty bin.
    #[must_use]
    pub fn gap(&self) -> Option<f64> {
        self.mean_predicted
            .zip(self.observed_frequency)
            .map(|(p, o)| (p - o).abs())
    }
}

/// A reliability diagram over resolved forecasts, plus the single number a
/// regression probe can bind (#35 story 15).
///
/// # Why this exists beside per-belief mean Brier
///
/// Brier is a *proper score*: it folds calibration and discrimination into one
/// number, so a policy can improve its Brier while getting systematically more
/// overconfident, and a policy can degrade its calibration while its Brier
/// barely moves. Story 15 asks for a **reliability-diagram** bound precisely
/// because that is the failure a scalar score hides.
///
/// `expected_calibration_error` is the scored-count-weighted mean of the
/// per-bin `|predicted − observed|` gaps — the standard ECE. It is the quantity
/// `probe_reliability_diagram_bound_holds_at_the_replay_boundary` binds.
///
/// # What it does not measure
///
/// It reads the forecast ledger, which is *not* per-arm. Each arm emitting
/// forecasts from its own posterior is #35 §9 item 2 and does not exist, so §8
/// clause 2 stays `undefined` and nothing here can change that. This binds the
/// calibration **instrument** against silent degradation; it does not compare
/// policies.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ReliabilityDiagram {
    pub bins: Vec<ReliabilityBin>,
    /// Scored forecasts across every bin.
    pub scored: usize,
    /// Weighted mean absolute gap. `None` when nothing is scored — an unscored
    /// ledger has no calibration error, which is distinct from an error of 0.0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_calibration_error: Option<f64>,
    /// Largest single-bin gap, which a weighted mean can average away.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_calibration_error: Option<f64>,
}

/// Build a [`ReliabilityDiagram`] over the resolved forecasts in `records`.
///
/// `bins` is the number of equal-width bins over `[0, 1]`; `0` is treated as
/// `1`. Void and pending records are excluded — an unresolved forecast has no
/// observed frequency to contribute, and counting it as either outcome would
/// invent data.
#[must_use]
pub fn reliability_diagram(records: &[ForecastRecord], bins: usize) -> ReliabilityDiagram {
    let bins = bins.max(1);
    let width = 1.0 / bins as f64;
    let mut sums = vec![(0usize, 0.0f64, 0usize); bins]; // (scored, Σp, #true)

    for rec in records {
        let Some(res) = &rec.resolution else { continue };
        let hit = match res.outcome {
            Outcome::True => true,
            Outcome::False => false,
            Outcome::Void => continue,
        };
        let p = rec.prediction.probability.clamp(0.0, 1.0);
        // The top bin is closed on the right so a forecast of exactly 1.0 lands
        // in it rather than one past the end.
        let idx = ((p / width).floor() as usize).min(bins - 1);
        sums[idx].0 += 1;
        sums[idx].1 += p;
        sums[idx].2 += usize::from(hit);
    }

    let bins_out: Vec<ReliabilityBin> = sums
        .iter()
        .enumerate()
        .map(|(i, (scored, sum_p, hits))| ReliabilityBin {
            lower: i as f64 * width,
            upper: (i + 1) as f64 * width,
            scored: *scored,
            mean_predicted: (*scored > 0).then(|| sum_p / *scored as f64),
            observed_frequency: (*scored > 0).then(|| *hits as f64 / *scored as f64),
        })
        .collect();

    let scored: usize = bins_out.iter().map(|b| b.scored).sum();
    let (ece, max) = if scored == 0 {
        (None, None)
    } else {
        let weighted: f64 = bins_out
            .iter()
            .filter_map(|b| b.gap().map(|g| g * b.scored as f64))
            .sum();
        let max = bins_out
            .iter()
            .filter_map(ReliabilityBin::gap)
            .fold(0.0f64, f64::max);
        (Some(weighted / scored as f64), Some(max))
    };

    ReliabilityDiagram {
        bins: bins_out,
        scored,
        expected_calibration_error: ece,
        max_calibration_error: max,
    }
}

pub fn calibration(records: &[ForecastRecord]) -> BTreeMap<String, BeliefCalibration> {
    let mut out: BTreeMap<String, BeliefCalibration> = BTreeMap::new();
    for rec in records {
        let entry = out.entry(rec.belief_id.clone()).or_default();
        match &rec.resolution {
            None => entry.pending += 1,
            Some(res) => match res.brier {
                Some(b) => {
                    // mean_brier temporarily holds the running sum; divided below.
                    entry.scored += 1;
                    *entry.mean_brier.get_or_insert(0.0) += b;
                }
                None => entry.void += 1,
            },
        }
    }
    for cal in out.values_mut() {
        if let Some(sum) = cal.mean_brier {
            cal.mean_brier = Some(sum / cal.scored as f64);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Clock + id helpers — no new dependencies; std only.
// ---------------------------------------------------------------------------

/// v0.1 accepts exactly the canonical form this module emits:
/// `YYYY-MM-DDTHH:MM:SSZ` — 20 chars, seconds precision, UTC `Z`. Offsets and
/// fractional seconds are rejected at the write boundary: a fractional-second
/// horizon like `…T18:00:00.500Z` would sort *before* `…T18:00:00Z` under the
/// lexicographic comparison in [`ForecastRecord::is_past_horizon`] (`.` < `Z`)
/// and could resolve a forecast before its actual horizon.
pub fn is_canonical_utc(ts: &str) -> bool {
    let b = ts.as_bytes();
    if b.len() != 20 {
        return false;
    }
    for (i, &c) in b.iter().enumerate() {
        let ok = match i {
            4 | 7 => c == b'-',
            10 => c == b'T',
            13 | 16 => c == b':',
            19 => c == b'Z',
            _ => c.is_ascii_digit(),
        };
        if !ok {
            return false;
        }
    }
    let n = |r: std::ops::Range<usize>| ts[r].parse::<u32>().unwrap_or(99);
    (1..=12).contains(&n(5..7))
        && (1..=31).contains(&n(8..10))
        && n(11..13) < 24
        && n(14..16) < 60
        && n(17..19) < 60
}

/// Current UTC time as `YYYY-MM-DDTHH:MM:SSZ`.
pub fn rfc3339_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// Days-since-epoch → (year, month, day). Howard Hinnant's civil_from_days.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// UUIDv7 (sortable by emission time, per the contract's id convention).
/// Random bits come from `RandomState` — uniqueness, not cryptography.
pub fn uuidv7() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let ms = (now.as_millis() as u64) & 0xFFFF_FFFF_FFFF;
    let mut h = RandomState::new().build_hasher();
    h.write_u128(now.as_nanos());
    h.write_u32(std::process::id());
    let r1 = h.finish();
    let mut h2 = RandomState::new().build_hasher();
    h2.write_u64(r1);
    let r2 = h2.finish();
    let hi: u64 = (ms << 16) | 0x7000 | (r1 & 0x0FFF);
    let lo: u64 = 0x8000_0000_0000_0000 | (r2 & 0x3FFF_FFFF_FFFF_FFFF);
    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        hi >> 32,
        (hi >> 16) & 0xFFFF,
        hi & 0xFFFF,
        lo >> 48,
        lo & 0xFFFF_FFFF_FFFF
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn presence_doc() -> Value {
        json!({
            "limbs": [
                { "id": "lane:router", "status": "green" },
                { "id": "sensor:quality-snapshot", "status": "green" }
            ]
        })
    }

    fn sample_record(probability: f64) -> ForecastRecord {
        emit(
            "ga-quality-snapshot-pipeline-healthy",
            Observable {
                source: "ga:state/fleet/presence.json".into(),
                field: "/limbs/id=sensor:quality-snapshot/status".into(),
                predicate: "== green".into(),
            },
            probability,
            Some("test".into()),
            "2026-07-03T18:00:00Z",
            None,
            "2026-07-02T18:00:00Z",
        )
    }

    #[test]
    fn lookup_field_supports_key_value_array_selector() {
        let doc = presence_doc();
        let v = lookup_field(&doc, "/limbs/id=sensor:quality-snapshot/status").unwrap();
        assert_eq!(v, &json!("green"));
        assert!(lookup_field(&doc, "/limbs/id=nope/status").is_none());
        assert_eq!(
            lookup_field(&doc, "/limbs/0/id").unwrap(),
            &json!("lane:router")
        );
    }

    #[test]
    fn predicate_eq_neq_and_malformed() {
        assert_eq!(eval_predicate("== green", &json!("green")), Some(true));
        assert_eq!(eval_predicate("== green", &json!("red")), Some(false));
        assert_eq!(eval_predicate("!= green", &json!("red")), Some(true));
        assert_eq!(eval_predicate("== 3", &json!(3)), Some(true));
        // No operator, or a non-scalar field: not mechanically scorable.
        assert_eq!(eval_predicate("green", &json!("green")), None);
        assert_eq!(eval_predicate("== x", &json!(["x"])), None);
    }

    #[test]
    fn resolve_scores_brier_true_and_false() {
        let rec = sample_record(0.9);
        let res = resolve(&rec, Some(&presence_doc()), "2026-07-03T18:05:00Z");
        assert_eq!(res.outcome, Outcome::True);
        assert!((res.brier.unwrap() - 0.01).abs() < 1e-12);
        assert_eq!(res.observed_value.as_deref(), Some("green"));

        let red = json!({ "limbs": [ { "id": "sensor:quality-snapshot", "status": "red" } ] });
        let res = resolve(&rec, Some(&red), "2026-07-03T18:05:00Z");
        assert_eq!(res.outcome, Outcome::False);
        assert!((res.brier.unwrap() - 0.81).abs() < 1e-12);
    }

    #[test]
    fn resolve_void_on_unreadable_artifact_or_missing_field() {
        let rec = sample_record(0.9);
        let res = resolve(&rec, None, "2026-07-03T18:05:00Z");
        assert_eq!(res.outcome, Outcome::Void);
        assert!(res.brier.is_none());

        let empty = json!({});
        let res = resolve(&rec, Some(&empty), "2026-07-03T18:05:00Z");
        assert_eq!(res.outcome, Outcome::Void);
    }

    #[test]
    fn outcome_serializes_to_contract_strings() {
        assert_eq!(serde_json::to_string(&Outcome::True).unwrap(), "\"true\"");
        assert_eq!(serde_json::to_string(&Outcome::Void).unwrap(), "\"void\"");
    }

    #[test]
    fn ledger_roundtrip_last_record_per_id_wins() {
        let dir = std::env::temp_dir().join(format!("hari-forecast-test-{}", uuidv7()));
        let mut rec = sample_record(0.9);
        append(&dir, &rec).unwrap();
        rec.resolution = Some(resolve(&rec, Some(&presence_doc()), "2026-07-03T18:05:00Z"));
        append(&dir, &rec).unwrap();

        let (records, skipped) = load(&dir).unwrap();
        assert_eq!(skipped, 0);
        assert_eq!(records.len(), 1);
        assert!(records[0].resolution.is_some());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn calibration_mean_brier_and_buckets() {
        let mut scored_hit = sample_record(0.9);
        scored_hit.resolution = Some(resolve(
            &scored_hit,
            Some(&presence_doc()),
            "2026-07-03T18:05:00Z",
        ));
        let mut scored_void = sample_record(0.5);
        scored_void.resolution = Some(resolve(&scored_void, None, "2026-07-03T18:05:00Z"));
        let pending = sample_record(0.7);

        let cal = calibration(&[scored_hit, scored_void, pending]);
        let c = &cal["ga-quality-snapshot-pipeline-healthy"];
        assert_eq!((c.scored, c.void, c.pending), (1, 1, 1));
        assert!((c.mean_brier.unwrap() - 0.01).abs() < 1e-12);
    }

    #[test]
    fn overdue_unresolved_flags_pending_past_horizon() {
        // sample_record is pending with horizon 2026-07-03T18:00:00Z.
        let rec = sample_record(0.85);
        let flagged = overdue_unresolved(std::slice::from_ref(&rec), "2026-07-05T00:00:00Z");
        assert_eq!(flagged.len(), 1);
        assert_eq!(flagged[0].forecast_id, rec.forecast_id);
        // Exactly at horizon also counts — is_past_horizon is `<=`.
        assert_eq!(
            overdue_unresolved(std::slice::from_ref(&rec), "2026-07-03T18:00:00Z").len(),
            1
        );
    }

    #[test]
    fn overdue_unresolved_ignores_resolved_and_not_yet_due() {
        let now = "2026-07-05T00:00:00Z";
        // Resolved past horizon → already scored, not a tripwire.
        let mut resolved = sample_record(0.9);
        resolved.resolution = Some(resolve(
            &resolved,
            Some(&presence_doc()),
            "2026-07-03T18:05:00Z",
        ));
        assert!(overdue_unresolved(std::slice::from_ref(&resolved), now).is_empty());
        // Pending but horizon still in the future → not yet due, passes.
        let mut not_due = sample_record(0.7);
        not_due.horizon = "2026-07-10T18:00:00Z".into();
        assert!(overdue_unresolved(std::slice::from_ref(&not_due), now).is_empty());
    }

    #[test]
    fn uuidv7_shape_version_and_variant() {
        let id = uuidv7();
        assert_eq!(id.len(), 36);
        assert_eq!(id.as_bytes()[14], b'7');
        assert!(matches!(id.as_bytes()[19], b'8' | b'9' | b'a' | b'b'));
        assert_ne!(uuidv7(), id);
    }

    #[test]
    fn canonical_utc_rejects_fractional_seconds_and_offsets() {
        assert!(is_canonical_utc("2026-07-03T18:00:00Z"));
        assert!(is_canonical_utc(&rfc3339_now()));
        // The Codex-flagged hazard: '.' < 'Z' would make this sort as due
        // before the real horizon under lexicographic comparison.
        assert!(!is_canonical_utc("2026-07-03T18:00:00.500Z"));
        assert!(!is_canonical_utc("2026-07-03T18:00:00+00:00"));
        assert!(!is_canonical_utc("2026-07-03 18:00:00Z"));
        assert!(!is_canonical_utc("2026-13-03T18:00:00Z"));
        assert!(!is_canonical_utc("2026-07-03T24:00:00Z"));
        assert!(!is_canonical_utc(""));
    }

    #[test]
    fn append_rejects_non_canonical_horizon() {
        let dir = std::env::temp_dir().join(format!("hari-forecast-test-{}", uuidv7()));
        let mut rec = sample_record(0.9);
        rec.horizon = "2026-07-03T18:00:00.500Z".into();
        let err = append(&dir, &rec).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
        assert!(!dir.exists());
    }

    #[test]
    fn rfc3339_now_shape() {
        let ts = rfc3339_now();
        assert_eq!(ts.len(), 20);
        assert!(ts.ends_with('Z'));
        assert_eq!(&ts[4..5], "-");
        assert_eq!(&ts[10..11], "T");
    }

    #[test]
    fn horizon_comparison_is_lexicographic_utc() {
        let rec = sample_record(0.9);
        assert!(!rec.is_past_horizon("2026-07-03T17:59:59Z"));
        assert!(rec.is_past_horizon("2026-07-03T18:00:00Z"));
    }
}
