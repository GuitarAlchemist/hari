//! G-Set CRDT merge for hexavalent observations.
//!
//! Faithful Hari-side implementation of the protocol specified in
//! `governance/demerzel/logic/hex-merge.md`. Mirrors
//! `ix-fuzzy::observations` so the two stay byte-equivalent on shared
//! fixtures.
//!
//! The lattice/propagation machinery in this crate (`BeliefNetwork`,
//! `propagate_with_provenance`) is a separate concept: it operates on
//! a single agent's belief graph with typed semantic relations.
//! `merge` here operates on multi-source G-Set observations with
//! `(source, diagnosis_id, round, ordinal)` dedup keys and
//! `claim_key` grouping. The two coexist; the merge layer feeds
//! consensus values that callers may then perceive into a network.
//!
//! # What this module does
//!
//! - Deduplicates observations by `(source, diagnosis_id, round, ordinal)`
//! - Groups observations by `claim_key`
//! - Synthesizes contradiction observations for same-claim opposite-
//!   polarity pairs using the Belnap-extended weight table
//! - Synthesizes meta-conflict observations for cross-aspect
//!   disagreements on the same action
//! - Applies the staleness budget (default K=5 rounds)
//! - Derives a [`HexDistribution`] by summing weights per variant and
//!   normalizing
//!
//! # Proof obligations
//!
//! The test module verifies the six CRDT correctness obligations
//! from `hex-merge.md §CRDT Correctness Proof Obligations`:
//!
//! 1. Commutativity: `merge(A, B) == merge(B, A)`
//! 2. Associativity: `merge(merge(A, B), C) == merge(A, merge(B, C))`
//! 3. Idempotence: `merge(A, A) == A`
//! 4. Monotonicity: `|merge(A, B)| >= max(|A|, |B|)`
//! 5. Dedup by key: two observations with the same dedup key merge to one
//! 6. Belnap symmetry: synthesized C for `(T, F)` is the same
//!    regardless of which observation was added first

use crate::HexValue;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// A `(source, diagnosis_id, round, ordinal)` dedup key — the value
/// [`HexObservation::dedup_key`] returns. Named here so the
/// tombstone-aware entry point ([`merge_with_tombstones`]) can take a
/// *set* of retracted keys as plain data, without importing any
/// caller-side (e.g. `hari-core`) evidence types.
pub type DedupKey = (String, String, u32, u32);

/// Default staleness budget: observations from rounds older than
/// `current_round - K` are dropped before merging. Matches the
/// default in `hex-merge.md §Staleness Policy` and ix-fuzzy.
pub const DEFAULT_STALENESS_K: u32 = 5;

/// Reserved aspect name for synthesized cross-aspect conflicts. The
/// merge function emits observations with this aspect; agents must
/// never emit it directly.
pub const META_CONFLICT_ASPECT: &str = "meta_conflict";

/// Reserved source name for synthesized contradictions produced by
/// this merge function. Observations from this source are derived,
/// not primary. Same value used by `ix-fuzzy::observations` so
/// downstream consumers can dispatch on it identically.
pub const MERGE_SOURCE: &str = "demerzel-merge";

/// Escalation threshold on `C` mass — above this the distribution
/// should be flagged for human review. Matches
/// `ix-fuzzy::hexavalent::ESCALATION_THRESHOLD`.
pub const ESCALATION_THRESHOLD: f64 = 0.3;

/// A single hexavalent observation contributed by one source about
/// one claim. Wire-compatible with Demerzel's
/// `schemas/session-event.schema.json` and `ix-fuzzy::HexObservation`:
/// `variant` serializes as the single-letter symbol (`"T"`, `"P"`,
/// `"U"`, `"D"`, `"F"`, `"C"`) regardless of how `HexValue` is
/// serialized elsewhere in the workspace.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HexObservation {
    /// Emitting agent identifier (e.g. `"tars"`, `"ix"`,
    /// `"demerzel-merge"`).
    pub source: String,
    /// Content hash of the originating diagnosis. Used for
    /// correlation and dedup.
    pub diagnosis_id: String,
    /// Remediation round number. Used by the staleness filter.
    pub round: u32,
    /// Monotone position within `(source, diagnosis_id, round)`.
    pub ordinal: u32,
    /// The claim this observation takes a position on. Format is
    /// `action_key::aspect`.
    pub claim_key: String,
    /// Hexavalent value the source is asserting. Serialized as the
    /// canonical single-letter symbol via [`hex_letter`].
    #[serde(with = "hex_letter")]
    pub variant: HexValue,
    /// Confidence weight in `(0.0, 1.0]`.
    pub weight: f64,
    /// Optional audit-trail evidence string. Not used by the merge.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub evidence: Option<String>,
}

/// Single-letter wire format (`"T"`/`"P"`/`"U"`/`"D"`/`"F"`/`"C"`)
/// for [`HexValue`]. Matches `ix-types::Hexavalent`'s serde mapping
/// and Demerzel's `hexavalent-state.schema.json`. Used inside the
/// merge module via `#[serde(with = "hex_letter")]` so existing
/// `HexValue` consumers (and pre-existing JSON fixtures) keep their
/// long-form wire format unchanged.
mod hex_letter {
    use super::HexValue;
    use serde::{de::Error, Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &HexValue, s: S) -> Result<S::Ok, S::Error> {
        let sym = match v {
            HexValue::True => "T",
            HexValue::Probable => "P",
            HexValue::Unknown => "U",
            HexValue::Doubtful => "D",
            HexValue::False => "F",
            HexValue::Contradictory => "C",
        };
        s.serialize_str(sym)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<HexValue, D::Error> {
        let s = <&str as Deserialize>::deserialize(d)?;
        match s {
            "T" => Ok(HexValue::True),
            "P" => Ok(HexValue::Probable),
            "U" => Ok(HexValue::Unknown),
            "D" => Ok(HexValue::Doubtful),
            "F" => Ok(HexValue::False),
            "C" => Ok(HexValue::Contradictory),
            other => Err(D::Error::custom(format!(
                "expected one of T/P/U/D/F/C, got {other:?}"
            ))),
        }
    }
}

impl HexObservation {
    /// Deduplication key: `(source, diagnosis_id, round, ordinal)`.
    /// Two observations with the same key are the same observation.
    pub fn dedup_key(&self) -> (String, String, u32, u32) {
        (
            self.source.clone(),
            self.diagnosis_id.clone(),
            self.round,
            self.ordinal,
        )
    }

    /// Split the `claim_key` into `(action_key, aspect)`. Returns
    /// the full claim_key as action_key with `"valuable"` aspect if
    /// there's no `::` separator. Uses `rfind` so action_keys
    /// containing `::` (e.g. Rust test paths) split on the LAST
    /// occurrence.
    pub fn action_and_aspect(&self) -> (&str, &str) {
        match self.claim_key.rfind("::") {
            Some(idx) => (&self.claim_key[..idx], &self.claim_key[idx + 2..]),
            None => (self.claim_key.as_str(), "valuable"),
        }
    }

    /// `true` iff the variant is on the positive side of the truth
    /// axis (T or P). Used by the meta-conflict detection rule.
    pub fn is_positive(&self) -> bool {
        matches!(self.variant, HexValue::True | HexValue::Probable)
    }

    /// `true` iff the variant is on the negative side (D or F).
    pub fn is_negative(&self) -> bool {
        matches!(self.variant, HexValue::Doubtful | HexValue::False)
    }
}

/// A normalized mass distribution over the six hexavalent variants.
/// Stored as a fixed array indexed by [`merge_rank`] so the layout
/// matches `ix-fuzzy::HexavalentDistribution` exactly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HexDistribution {
    masses: [f64; 6],
}

impl HexDistribution {
    /// Uniform 1/6 mass on each of the six variants.
    pub fn uniform() -> Self {
        Self {
            masses: [1.0 / 6.0; 6],
        }
    }

    /// Build a distribution from per-variant masses. Caller is
    /// responsible for normalization (the merge function divides by
    /// total before constructing).
    pub fn from_tpudfc(t: f64, p: f64, u: f64, d: f64, f: f64, c: f64) -> Self {
        Self {
            masses: [t, p, u, d, f, c],
        }
    }

    /// Mass on a specific variant.
    pub fn get(&self, v: HexValue) -> f64 {
        self.masses[merge_rank(v) as usize]
    }

    /// `true` iff `C` exceeds [`ESCALATION_THRESHOLD`] as a share of
    /// the *informative* mass — `Unknown` is excluded from the
    /// denominator (spec v1.2, issue #28). Abstention is the absence
    /// of evidence and must not be able to mute an evidence-based
    /// alarm; an all-`Unknown` distribution never escalates. Callers
    /// should check this immediately after each merge.
    pub fn escalation_triggered(&self) -> bool {
        let informative: f64 = self.masses.iter().sum::<f64>() - self.get(HexValue::Unknown);
        informative > 0.0 && self.get(HexValue::Contradictory) / informative > ESCALATION_THRESHOLD
    }
}

/// Result of merging a set of observations.
#[derive(Debug, Clone)]
pub struct MergedState {
    /// Deduplicated, staleness-filtered observations from all
    /// sources, plus any synthesized contradiction observations.
    pub observations: Vec<HexObservation>,
    /// Synthesized contradiction observations only (subset of
    /// `observations` with `source == MERGE_SOURCE`).
    pub contradictions: Vec<HexObservation>,
    /// Derived hexavalent distribution.
    pub distribution: HexDistribution,
}

/// The Belnap-extended weight table. Returns `Some(multiplier)` if
/// the pair should synthesize a `C` observation; `None` if not.
/// Multiplier is applied to `min(weight_a, weight_b)`.
///
/// Matches the table in `hex-merge.md §Belnap-extended Contradiction
/// Table` and `ix-fuzzy::observations::belnap_weight` exactly.
pub fn belnap_weight(a: HexValue, b: HexValue) -> Option<f64> {
    use HexValue::*;
    // Normalize order — table is symmetric across the diagonal.
    let (lo, hi) = if merge_rank(a) <= merge_rank(b) {
        (a, b)
    } else {
        (b, a)
    };
    match (lo, hi) {
        (True, False) => Some(1.0),
        (True, Doubtful) => Some(0.8),
        (Probable, False) => Some(0.8),
        (Probable, Doubtful) => Some(0.5),
        _ => None,
    }
}

/// Stable ordering over hexavalent variants for canonicalization
/// inside this module. Matches `ix-fuzzy::observations::variant_rank`
/// (T=0, P=1, U=2, D=3, F=4, C=5) so distribution layouts are
/// identical between the two implementations.
///
/// Note this differs from `HexValue::rank` (the truth-chain ordering
/// used by lattice operations: F=0, D=1, U=2, P=3, T=4, C=5). The
/// merge module needs the wire-compatible layout; the lattice module
/// needs truth-axis ordering. Both are correct for their callers.
fn merge_rank(v: HexValue) -> u8 {
    match v {
        HexValue::True => 0,
        HexValue::Probable => 1,
        HexValue::Unknown => 2,
        HexValue::Doubtful => 3,
        HexValue::False => 4,
        HexValue::Contradictory => 5,
    }
}

/// Content-derived `diagnosis_id` for a synthesized observation.
/// Produced from the sorted dedup keys of the contributing
/// observations plus a `kind` discriminator and the target
/// `claim_key`. Two calls with the same inputs — even across
/// separate merge invocations — produce the same id, so dedup
/// collapses re-merged synthesis output. This is the property that
/// restores associativity.
fn synthesis_diagnosis_id(
    kind: &str,
    claim_key: &str,
    a: &HexObservation,
    b: &HexObservation,
) -> String {
    let ka = format!("{}|{}|{}|{}", a.source, a.diagnosis_id, a.round, a.ordinal);
    let kb = format!("{}|{}|{}|{}", b.source, b.diagnosis_id, b.round, b.ordinal);
    let (lo, hi) = if ka <= kb { (ka, kb) } else { (kb, ka) };
    format!("merge:{kind}:{claim_key}:{lo}+{hi}")
}

/// Resolve all observations sharing one dedup key into a single
/// deterministic observation.
///
/// `dedup_key`'s contract says "two observations with the same key are
/// the same observation", but the type cannot enforce it, so divergent
/// payloads under one key are representable and must resolve
/// order-independently. The fold is associative, commutative, and
/// idempotent over the version multiset — required so that carried
/// re-merges agree with flat merges when versions of the same key
/// arrive on different sides of a merge boundary:
///
/// - identical payloads → that payload (true duplicates);
/// - same variant, divergent weight/evidence/claim_key → keep the
///   variant at the MINIMUM weight (conservative confidence), with
///   the lexicographically least claim_key and evidence;
/// - divergent variants → the source has contradicted itself within a
///   single observation slot. Nothing licenses picking a winner, and
///   the domain has a value for exactly this: `Contradictory`, at the
///   minimum weight (matching the Belnap `min` convention). The
///   evidence marker is derived from the key alone (never from the
///   version count or contents) so it folds identically regardless of
///   how versions are grouped across merge boundaries.
fn resolve_key_versions(versions: Vec<HexObservation>) -> HexObservation {
    debug_assert!(!versions.is_empty());
    let first = &versions[0];

    let mut variants: Vec<HexValue> = versions.iter().map(|v| v.variant).collect();
    variants.sort_by_key(|v| merge_rank(*v));
    variants.dedup();

    let min_weight = versions
        .iter()
        .map(|v| v.weight)
        .fold(f64::INFINITY, f64::min);
    let min_claim = versions.iter().map(|v| v.claim_key.clone()).min().unwrap();
    let min_evidence = versions.iter().filter_map(|v| v.evidence.clone()).min();

    if variants.len() == 1 {
        HexObservation {
            source: first.source.clone(),
            diagnosis_id: first.diagnosis_id.clone(),
            round: first.round,
            ordinal: first.ordinal,
            claim_key: min_claim,
            variant: variants[0],
            weight: min_weight,
            evidence: min_evidence,
        }
    } else {
        // Key-derived marker: identical however the versions are
        // grouped. Folded through `min` with the versions' own
        // evidence so that resolving in stages (carried merges)
        // yields the same evidence as resolving flat.
        let marker = format!(
            "self-conflict:{}|{}|r{}|o{}",
            first.source, first.diagnosis_id, first.round, first.ordinal
        );
        let evidence = match min_evidence {
            Some(e) => Some(e.min(marker)),
            None => Some(marker),
        };
        HexObservation {
            source: first.source.clone(),
            diagnosis_id: first.diagnosis_id.clone(),
            round: first.round,
            ordinal: first.ordinal,
            claim_key: min_claim,
            variant: HexValue::Contradictory,
            weight: min_weight,
            evidence,
        }
    }
}

/// Merge a set of observations into a [`MergedState`]. Implements
/// the full pipeline:
///
/// 0. Drop incoming [`MERGE_SOURCE`] observations — synthesis is
///    derived cache, re-derived from base evidence every merge
/// 1. Deduplicate by `(source, diagnosis_id, round, ordinal)`,
///    resolving divergent payloads via [`resolve_key_versions`]
/// 2. Apply staleness filter: drop obs with `round < current - K`
/// 3. Group by claim_key, synthesize direct contradictions per the
///    Belnap-extended table (stamped `round = min(parents)`)
/// 4. Group by action_key, synthesize meta-conflicts for cross-
///    aspect disagreements
/// 5. Derive distribution by summing per-variant weights and
///    normalizing
///
/// Steps 0–3 together make the merge a pure, order-independent
/// function of the base-evidence multiset: carried re-merges agree
/// with flat merges and with evidence recomputes under any staleness
/// window (see `tests/algebra_probe.rs`).
///
/// `current_round` and `staleness_k` may be `None` to skip the
/// staleness step.
pub fn merge(
    observations: &[HexObservation],
    current_round: Option<u32>,
    staleness_k: Option<u32>,
) -> MergedState {
    // Step 1: deduplicate by dedup key. BTreeMap keeps order
    // deterministic so the merge output is reproducible across runs
    // regardless of input order — load-bearing for CRDT correctness.
    //
    // Colliding keys with DIVERGENT payloads are resolved by
    // `resolve_key_versions` (an associative-commutative-idempotent
    // fold), not first-write-wins. First-write-wins let input order
    // leak into the output, falsifying the reproducibility claim above
    // and breaking associativity of carried re-merges — see
    // `docs/research/2026-07-20-hex-merge-algebraic-audit.md` §3.
    // NOTE: this deliberately diverges from the Demerzel-canonical /
    // ix-fuzzy first-write semantics until the fix is propagated
    // (canonical home: Demerzel `logic/hex-merge.md`).
    //
    // Step 0: incoming MERGE_SOURCE observations are DERIVED CACHE,
    // not evidence — they are dropped here and re-derived from base
    // evidence in steps 3–4. Carrying them as input let a synthesis
    // computed from a partial view (before a later key-collision
    // resolution or staleness expiry changed its parents) survive
    // into states where a fresh derivation would not produce it.
    // Evidence-recompute is authoritative; synthesis is a pure
    // function of the surviving base observations.
    let mut by_key: BTreeMap<(String, String, u32, u32), Vec<HexObservation>> = BTreeMap::new();
    for obs in observations {
        if obs.source == MERGE_SOURCE {
            continue;
        }
        by_key.entry(obs.dedup_key()).or_default().push(obs.clone());
    }

    let mut resolved: BTreeMap<(String, String, u32, u32), HexObservation> = BTreeMap::new();
    for (key, versions) in by_key {
        resolved.insert(key, resolve_key_versions(versions));
    }

    // Step 2: staleness filter.
    if let (Some(current), Some(k)) = (current_round, staleness_k) {
        let cutoff = current.saturating_sub(k);
        resolved.retain(|_, obs| obs.round >= cutoff);
    }

    let deduped: Vec<HexObservation> = resolved.into_values().collect();

    // Step 3: direct contradictions by claim_key.
    let mut synthesized: Vec<HexObservation> = Vec::new();
    let mut by_claim: BTreeMap<String, Vec<&HexObservation>> = BTreeMap::new();
    for obs in &deduped {
        by_claim.entry(obs.claim_key.clone()).or_default().push(obs);
    }

    for (claim_key, obs_list) in &by_claim {
        // Skip already-synthesized meta_conflict entries — don't
        // double-derive from our own output.
        if claim_key.ends_with(&format!("::{META_CONFLICT_ASPECT}")) {
            continue;
        }
        for (i, a) in obs_list.iter().enumerate() {
            for b in obs_list.iter().skip(i + 1) {
                if a.source == b.source {
                    continue;
                }
                if a.source == MERGE_SOURCE || b.source == MERGE_SOURCE {
                    continue;
                }
                if let Some(mult) = belnap_weight(a.variant, b.variant) {
                    let weight = mult * a.weight.min(b.weight);
                    synthesized.push(HexObservation {
                        source: MERGE_SOURCE.to_string(),
                        diagnosis_id: synthesis_diagnosis_id("direct", claim_key, a, b),
                        // MIN of the parents, not max: the synthesis is
                        // supported only while BOTH parents are inside
                        // the staleness window, and the pair coexists
                        // iff min(parents) >= cutoff. A max stamp let
                        // the synthesized C outlive its older parent
                        // ("ghost contradiction"), making carried state
                        // disagree with an evidence recompute — audit
                        // doc §4. Diverges from Demerzel-canonical
                        // until propagated.
                        round: a.round.min(b.round),
                        ordinal: 0,
                        claim_key: claim_key.clone(),
                        variant: HexValue::Contradictory,
                        weight,
                        evidence: Some(format!(
                            "{}:{:?} vs {}:{:?}",
                            a.source, a.variant, b.source, b.variant
                        )),
                    });
                }
            }
        }
    }

    // Step 4: meta-conflicts (cross-aspect, same action).
    let mut by_action: BTreeMap<String, Vec<&HexObservation>> = BTreeMap::new();
    for obs in &deduped {
        let (action, _aspect) = obs.action_and_aspect();
        by_action.entry(action.to_string()).or_default().push(obs);
    }

    for (action, obs_list) in &by_action {
        let positives: Vec<&HexObservation> = obs_list
            .iter()
            .filter(|o| o.is_positive())
            .copied()
            .collect();
        let negatives: Vec<&HexObservation> = obs_list
            .iter()
            .filter(|o| o.is_negative())
            .copied()
            .collect();

        for pos in &positives {
            for neg in &negatives {
                let (_, pos_aspect) = pos.action_and_aspect();
                let (_, neg_aspect) = neg.action_and_aspect();
                if pos_aspect == neg_aspect {
                    continue;
                }
                if pos.source == neg.source {
                    continue;
                }
                if pos.source == MERGE_SOURCE || neg.source == MERGE_SOURCE {
                    continue;
                }
                let meta_claim = format!("{action}::{META_CONFLICT_ASPECT}");
                synthesized.push(HexObservation {
                    source: MERGE_SOURCE.to_string(),
                    diagnosis_id: synthesis_diagnosis_id("meta", &meta_claim, pos, neg),
                    // min for the same reason as direct synthesis: the
                    // meta-conflict is supported only while both sides
                    // are in-window.
                    round: pos.round.min(neg.round),
                    ordinal: 0,
                    claim_key: meta_claim,
                    variant: HexValue::Contradictory,
                    weight: pos.weight.min(neg.weight),
                    evidence: Some(format!(
                        "cross-aspect: {}:{}:{:?} vs {}:{}:{:?}",
                        pos.source, pos_aspect, pos.variant, neg.source, neg_aspect, neg.variant
                    )),
                });
            }
        }
    }

    // Deduplicate synthesized observations by content-derived dedup
    // key. This is half of the associativity fix — see ix-fuzzy
    // observations.rs §Step 3 commentary for the full argument.
    let mut synth_by_key: BTreeMap<(String, String, u32, u32), HexObservation> = BTreeMap::new();
    for s in synthesized {
        synth_by_key.entry(s.dedup_key()).or_insert(s);
    }

    // Combine primary + synthesized, deduplicating again. Carried-
    // over merge-synthesized entries from a previous call have the
    // SAME content-derived dedup key as newly-synthesized ones, so
    // this collapses them rather than counting twice.
    let mut all_by_key: BTreeMap<(String, String, u32, u32), HexObservation> = BTreeMap::new();
    for obs in deduped.iter().chain(synth_by_key.values()) {
        all_by_key
            .entry(obs.dedup_key())
            .or_insert_with(|| obs.clone());
    }
    let all: Vec<HexObservation> = all_by_key.into_values().collect();

    // Rebuild the contradictions list from the deduplicated set so
    // it reflects only observations that were actually counted.
    let synthesized: Vec<HexObservation> = all
        .iter()
        .filter(|o| o.source == MERGE_SOURCE)
        .cloned()
        .collect();

    // Step 5: derive distribution.
    let mut weights = [0.0_f64; 6];
    for obs in &all {
        weights[merge_rank(obs.variant) as usize] += obs.weight;
    }
    let total: f64 = weights.iter().sum();
    let distribution = if total == 0.0 {
        HexDistribution::uniform()
    } else {
        HexDistribution::from_tpudfc(
            weights[0] / total,
            weights[1] / total,
            weights[2] / total,
            weights[3] / total,
            weights[4] / total,
            weights[5] / total,
        )
    };

    MergedState {
        observations: all,
        contradictions: synthesized,
        distribution,
    }
}

/// Convenience: merge with no staleness filtering.
pub fn merge_all(observations: &[HexObservation]) -> MergedState {
    merge(observations, None, None)
}

/// Convenience: merge applying [`DEFAULT_STALENESS_K`] against a
/// given current round.
pub fn merge_with_default_staleness(
    observations: &[HexObservation],
    current_round: u32,
) -> MergedState {
    merge(observations, Some(current_round), Some(DEFAULT_STALENESS_K))
}

/// Merge after excluding every observation whose [`HexObservation::dedup_key`]
/// is in `tombstoned` — the belief-revision tombstone step (issue #16,
/// design §9 item 2).
///
/// **Evidence-recompute is authoritative.** A tombstoned base observation
/// contributes *zero mass* to the result — the merge behaves exactly as if
/// that observation had never been submitted — while the caller keeps the
/// retracted observation in its own audit ledger (this crate never sees it
/// again). The filter is applied *before* the pipeline, which is byte-equal
/// to filtering it at the design's "step 1.5" for full-key tombstones:
/// dedup groups by the same key, so dropping the whole key up front removes
/// exactly the observations step 1.5 would, and the surviving multiset — and
/// therefore the [`MergedState`] — is identical.
///
/// This is the design's *tombstone-aware entry point in the lattice crate*.
/// It deliberately takes **plain data only** — a slice of observations plus a
/// set of dedup keys — and imports no caller types, keeping `hari-lattice` a
/// leaf. The two pinned properties (see `tests/tombstone_probe.rs`):
///
/// - **tombstone == never-present**: `merge_with_tombstones(all, T)` is
///   byte-equal to `merge_all(all \ T)` (merging the multiset that never
///   contained the tombstoned observations);
/// - **commutes with merge order**: the result depends only on the surviving
///   multiset, not on the order observations or tombstones are presented,
///   inheriting [`merge`]'s permutation-invariance.
pub fn merge_with_tombstones(
    observations: &[HexObservation],
    tombstoned: &BTreeSet<DedupKey>,
    current_round: Option<u32>,
    staleness_k: Option<u32>,
) -> MergedState {
    let surviving: Vec<HexObservation> = observations
        .iter()
        .filter(|o| !tombstoned.contains(&o.dedup_key()))
        .cloned()
        .collect();
    merge(&surviving, current_round, staleness_k)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn canonicalize(obs: &mut [HexObservation]) {
        obs.sort_by(|a, b| {
            a.source
                .cmp(&b.source)
                .then(a.diagnosis_id.cmp(&b.diagnosis_id))
                .then(a.round.cmp(&b.round))
                .then(a.ordinal.cmp(&b.ordinal))
                .then(a.claim_key.cmp(&b.claim_key))
                .then(merge_rank(a.variant).cmp(&merge_rank(b.variant)))
        });
    }

    fn obs(
        source: &str,
        diagnosis_id: &str,
        round: u32,
        ordinal: u32,
        claim_key: &str,
        variant: HexValue,
        weight: f64,
    ) -> HexObservation {
        HexObservation {
            source: source.to_string(),
            diagnosis_id: diagnosis_id.to_string(),
            round,
            ordinal,
            claim_key: claim_key.to_string(),
            variant,
            weight,
            evidence: None,
        }
    }

    fn states_equal(a: &MergedState, b: &MergedState) -> bool {
        let mut a_obs = a.observations.clone();
        let mut b_obs = b.observations.clone();
        canonicalize(&mut a_obs);
        canonicalize(&mut b_obs);
        if a_obs.len() != b_obs.len() {
            return false;
        }
        for (x, y) in a_obs.iter().zip(b_obs.iter()) {
            if x != y {
                return false;
            }
        }
        let variants = [
            HexValue::True,
            HexValue::Probable,
            HexValue::Unknown,
            HexValue::Doubtful,
            HexValue::False,
            HexValue::Contradictory,
        ];
        for v in variants {
            if (a.distribution.get(v) - b.distribution.get(v)).abs() > 1e-9 {
                return false;
            }
        }
        true
    }

    // -- Proof obligations ----------------------------------------

    #[test]
    fn proof_commutativity() {
        let a = [
            obs(
                "tars",
                "dx1",
                0,
                0,
                "ix_stats::valuable",
                HexValue::True,
                0.8,
            ),
            obs(
                "ix",
                "dx1",
                0,
                1,
                "ix_stats::valuable",
                HexValue::Probable,
                0.6,
            ),
        ];
        let b = [obs(
            "tars",
            "dx2",
            0,
            0,
            "ix_fft::valuable",
            HexValue::Doubtful,
            0.7,
        )];

        let ab: Vec<_> = a.iter().cloned().chain(b.iter().cloned()).collect();
        let ba: Vec<_> = b.iter().cloned().chain(a.iter().cloned()).collect();

        assert!(states_equal(&merge_all(&ab), &merge_all(&ba)));
    }

    #[test]
    fn proof_associativity() {
        let a = [obs("tars", "d", 0, 0, "k::valuable", HexValue::True, 1.0)];
        let b = [obs("ix", "d", 0, 1, "k::valuable", HexValue::False, 1.0)];
        let c = [obs("tars", "d", 0, 2, "k::safe", HexValue::Probable, 0.5)];

        let ab_all: Vec<_> = a.iter().cloned().chain(b.iter().cloned()).collect();
        let ab_state = merge_all(&ab_all);
        let ab_then_c: Vec<_> = ab_state
            .observations
            .iter()
            .cloned()
            .chain(c.iter().cloned())
            .collect();
        let left_assoc = merge_all(&ab_then_c);

        let bc_all: Vec<_> = b.iter().cloned().chain(c.iter().cloned()).collect();
        let bc_state = merge_all(&bc_all);
        let a_then_bc: Vec<_> = a
            .iter()
            .cloned()
            .chain(bc_state.observations.iter().cloned())
            .collect();
        let right_assoc = merge_all(&a_then_bc);

        assert!(states_equal(&left_assoc, &right_assoc));
    }

    #[test]
    fn proof_idempotence() {
        let a = vec![
            obs("tars", "d", 0, 0, "k::valuable", HexValue::True, 0.8),
            obs("ix", "d", 0, 1, "k::valuable", HexValue::False, 0.9),
        ];
        let doubled: Vec<_> = a.iter().cloned().chain(a.iter().cloned()).collect();

        assert!(states_equal(&merge_all(&a), &merge_all(&doubled)));
    }

    #[test]
    fn proof_monotonicity() {
        let a = vec![
            obs("tars", "d1", 0, 0, "k::valuable", HexValue::True, 0.8),
            obs("ix", "d1", 0, 1, "k::valuable", HexValue::Probable, 0.6),
        ];
        let b = vec![obs("tars", "d2", 0, 0, "k::safe", HexValue::Doubtful, 0.5)];
        let ab: Vec<_> = a.iter().cloned().chain(b.iter().cloned()).collect();

        let sa = merge_all(&a);
        let sb = merge_all(&b);
        let sab = merge_all(&ab);

        assert!(sab.observations.len() >= sa.observations.len());
        assert!(sab.observations.len() >= sb.observations.len());
    }

    #[test]
    fn proof_dedup_by_key() {
        let a = obs("tars", "d", 0, 0, "k::valuable", HexValue::True, 0.8);
        let a_dup = HexObservation {
            weight: 0.3,
            ..a.clone()
        };
        let result = merge_all(&[a, a_dup]);

        let primary_count = result
            .observations
            .iter()
            .filter(|o| o.source == "tars")
            .count();
        assert_eq!(primary_count, 1);
        let remaining = result
            .observations
            .iter()
            .find(|o| o.source == "tars")
            .unwrap();
        // Same-variant collision resolves to the MINIMUM weight
        // (order-independent), not first-write-wins — see
        // `resolve_key_versions`. Previously asserted 0.8 (first).
        assert!((remaining.weight - 0.3).abs() < 1e-9);
    }

    #[test]
    fn proof_belnap_symmetry() {
        let a = obs("tars", "d", 0, 0, "k::valuable", HexValue::True, 1.0);
        let b = obs("ix", "d", 0, 1, "k::valuable", HexValue::False, 1.0);

        let ab = merge_all(&[a.clone(), b.clone()]);
        let ba = merge_all(&[b, a]);

        assert_eq!(ab.contradictions.len(), 1);
        assert_eq!(ba.contradictions.len(), 1);
        assert!((ab.contradictions[0].weight - ba.contradictions[0].weight).abs() < 1e-9);
        assert!((ab.contradictions[0].weight - 1.0).abs() < 1e-9);
    }

    // -- Functional tests ----------------------------------------

    #[test]
    fn belnap_table_matches_spec() {
        use HexValue::*;

        assert_eq!(belnap_weight(True, False), Some(1.0));
        assert_eq!(belnap_weight(False, True), Some(1.0));
        assert_eq!(belnap_weight(True, Doubtful), Some(0.8));
        assert_eq!(belnap_weight(Doubtful, True), Some(0.8));
        assert_eq!(belnap_weight(Probable, False), Some(0.8));
        assert_eq!(belnap_weight(False, Probable), Some(0.8));
        assert_eq!(belnap_weight(Probable, Doubtful), Some(0.5));
        assert_eq!(belnap_weight(Doubtful, Probable), Some(0.5));

        // Same-side pairs (agreement, NOT contradiction).
        assert_eq!(belnap_weight(True, Probable), None);
        assert_eq!(belnap_weight(Doubtful, False), None);

        // U preserves; C is terminal; same-variant never synthesizes.
        for v in [True, Probable, Unknown, Doubtful, False, Contradictory] {
            assert_eq!(belnap_weight(Unknown, v), None);
            assert_eq!(belnap_weight(v, Unknown), None);
            assert_eq!(belnap_weight(Contradictory, v), None);
            assert_eq!(belnap_weight(v, Contradictory), None);
            assert_eq!(belnap_weight(v, v), None);
        }
    }

    #[test]
    fn tars_ix_agreement_produces_no_contradiction() {
        let obs_list = vec![
            obs("tars", "d", 0, 0, "ix_stats::valuable", HexValue::True, 0.9),
            obs(
                "ix",
                "d",
                0,
                1,
                "ix_stats::valuable",
                HexValue::Probable,
                0.7,
            ),
        ];
        let state = merge_all(&obs_list);
        assert_eq!(state.contradictions.len(), 0);
        assert!(state.distribution.get(HexValue::Contradictory) < 1e-9);
    }

    #[test]
    fn tars_ix_disagreement_escalates() {
        let obs_list = vec![
            obs(
                "tars",
                "d",
                0,
                0,
                "ix_git_gc::valuable",
                HexValue::True,
                1.0,
            ),
            obs("ix", "d", 0, 1, "ix_git_gc::valuable", HexValue::False, 1.0),
        ];
        let state = merge_all(&obs_list);
        assert_eq!(state.contradictions.len(), 1);
        assert_eq!(state.contradictions[0].variant, HexValue::Contradictory);
        assert!((state.contradictions[0].weight - 1.0).abs() < 1e-9);
        let c_mass = state.distribution.get(HexValue::Contradictory);
        assert!(c_mass > 0.33 - 1e-9);
        assert!(state.distribution.escalation_triggered());
    }

    /// Spec v1.2 (issue #28): escalation is a share of *informative*
    /// mass, so abstention cannot mute it. Under v1.1 a single
    /// Unknown observation here dropped C-mass to 1/4 < 0.3 and
    /// silenced the alarm.
    #[test]
    fn escalation_immune_to_abstention_flooding() {
        let mut obs_list = vec![
            obs(
                "tars",
                "d",
                0,
                0,
                "ix_git_gc::valuable",
                HexValue::True,
                1.0,
            ),
            obs("ix", "d", 0, 1, "ix_git_gc::valuable", HexValue::False, 1.0),
        ];
        assert!(merge_all(&obs_list).distribution.escalation_triggered());
        for n in 0..20 {
            obs_list.push(obs(
                &format!("bystander-{n}"),
                "d",
                0,
                2 + n,
                "ix_git_gc::valuable",
                HexValue::Unknown,
                1.0,
            ));
            assert!(
                merge_all(&obs_list).distribution.escalation_triggered(),
                "abstention muted escalation at n={}",
                n + 1
            );
        }
    }

    /// Spec v1.2 (issue #28): zero informative mass means nothing
    /// conflicts — all-Unknown input must not escalate.
    #[test]
    fn all_unknown_does_not_escalate() {
        let obs_list = vec![
            obs("a", "d", 0, 0, "claim", HexValue::Unknown, 1.0),
            obs("b", "d", 0, 1, "claim", HexValue::Unknown, 1.0),
        ];
        assert!(!merge_all(&obs_list).distribution.escalation_triggered());
    }

    #[test]
    fn meta_conflict_cross_aspect_same_action() {
        let obs_list = vec![
            obs(
                "tars",
                "d",
                0,
                0,
                "restart_gpu::valuable",
                HexValue::True,
                0.9,
            ),
            obs("ix", "d", 0, 1, "restart_gpu::safe", HexValue::False, 1.0),
        ];
        let state = merge_all(&obs_list);
        let meta_conflicts: Vec<_> = state
            .contradictions
            .iter()
            .filter(|o| o.claim_key.ends_with("::meta_conflict"))
            .collect();
        assert_eq!(meta_conflicts.len(), 1);
        assert_eq!(meta_conflicts[0].variant, HexValue::Contradictory);
        assert!((meta_conflicts[0].weight - 0.9).abs() < 1e-9);
    }

    #[test]
    fn same_source_cross_aspect_is_not_meta_conflict() {
        let obs_list = vec![
            obs(
                "tars",
                "d",
                0,
                0,
                "restart_gpu::valuable",
                HexValue::True,
                0.9,
            ),
            obs("tars", "d", 0, 1, "restart_gpu::safe", HexValue::False, 1.0),
        ];
        let state = merge_all(&obs_list);
        assert_eq!(state.contradictions.len(), 0);
    }

    #[test]
    fn staleness_filter_drops_old_rounds() {
        let obs_list = vec![
            obs("tars", "d", 0, 0, "k::valuable", HexValue::True, 1.0),
            obs("tars", "d", 3, 0, "k::valuable", HexValue::True, 1.0),
            obs("tars", "d", 10, 0, "k::valuable", HexValue::True, 1.0),
        ];
        let state = merge(&obs_list, Some(10), Some(5));
        assert_eq!(state.observations.len(), 1);
        assert_eq!(state.observations[0].round, 10);
    }

    #[test]
    fn empty_input_yields_uniform_distribution() {
        let state = merge_all(&[]);
        assert_eq!(state.observations.len(), 0);
        for v in [
            HexValue::True,
            HexValue::Probable,
            HexValue::Unknown,
            HexValue::Doubtful,
            HexValue::False,
            HexValue::Contradictory,
        ] {
            assert!((state.distribution.get(v) - 1.0 / 6.0).abs() < 1e-9);
        }
    }

    #[test]
    fn dedup_same_variant_resolves_to_min_weight_both_orders() {
        let a = obs("tars", "d", 0, 0, "k::valuable", HexValue::True, 0.5);
        let b = HexObservation {
            weight: 0.9,
            ..a.clone()
        };
        // Both input orders resolve to the same (minimum) weight —
        // the old first-write-wins semantics only agreed with this
        // when the smaller weight happened to arrive first.
        for input in [[a.clone(), b.clone()], [b, a]] {
            let state = merge_all(&input);
            let primary = state
                .observations
                .iter()
                .find(|o| o.source == "tars")
                .unwrap();
            assert!((primary.weight - 0.5).abs() < 1e-9);
        }
    }

    #[test]
    fn action_and_aspect_splits_correctly() {
        let o = obs("s", "d", 0, 0, "ix_stats::valuable", HexValue::True, 1.0);
        let (action, aspect) = o.action_and_aspect();
        assert_eq!(action, "ix_stats");
        assert_eq!(aspect, "valuable");

        let o2 = obs(
            "s",
            "d",
            0,
            0,
            "test:ix_math::eigen::jacobi::valuable",
            HexValue::True,
            1.0,
        );
        let (action, aspect) = o2.action_and_aspect();
        assert_eq!(action, "test:ix_math::eigen::jacobi");
        assert_eq!(aspect, "valuable");
    }

    #[test]
    fn action_and_aspect_default_aspect_when_no_delimiter() {
        let o = obs("s", "d", 0, 0, "ix_stats", HexValue::True, 1.0);
        let (action, aspect) = o.action_and_aspect();
        assert_eq!(action, "ix_stats");
        assert_eq!(aspect, "valuable");
    }
}
