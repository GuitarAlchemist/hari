//! Randomized probe of the belief-revision tombstone step (issue #16,
//! merge-weight slice; design §9 item 2).
//!
//! [`hari_lattice::merge_with_tombstones`] is the design's tombstone-aware
//! entry point: it drops every observation whose dedup key is in the
//! retracted set, then merges. Two documented properties, each over 1000+
//! seeded-random trials:
//!
//!   (a) **tombstone == never-present** —
//!       `merge_with_tombstones(all, T)` is byte-equal (canonicalized
//!       observations + distribution) to `merge_all(all \ T)`, the merge of
//!       the multiset that never contained the tombstoned observations. This
//!       is what makes "evidence-recompute is authoritative" hold: a
//!       tombstoned observation contributes exactly nothing.
//!   (b) **tombstone-filter commutes with merge order** — the result depends
//!       only on the surviving multiset, not on the order observations or
//!       tombstones are presented, inheriting `merge`'s permutation
//!       invariance.
//!
//! House style mirrors `tests/algebra_probe.rs`: fixed-seed xorshift64*, no
//! external deps, exact equality on discrete hex values.

use hari_lattice::{
    merge_all, merge_with_tombstones, DedupKey, HexObservation, HexValue, MergedState, MERGE_SOURCE,
};
use std::collections::BTreeSet;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

const SOURCES: [&str; 4] = ["tars", "ix", "evaluator", "critic"];
const CLAIMS: [&str; 3] = ["deploy::valuable", "deploy::safe", "bench::valuable"];
const VARIANTS: [HexValue; 6] = [
    HexValue::True,
    HexValue::Probable,
    HexValue::Unknown,
    HexValue::Doubtful,
    HexValue::False,
    HexValue::Contradictory,
];

fn gen_obs(rng: &mut Rng, k: usize) -> HexObservation {
    HexObservation {
        source: SOURCES[rng.below(SOURCES.len() as u64) as usize].to_string(),
        diagnosis_id: format!("d{}", rng.below(3)),
        round: rng.below(4) as u32,
        // ordinal = k keeps every generated observation's dedup key
        // distinct, so tombstoning one never accidentally removes another.
        ordinal: k as u32,
        claim_key: CLAIMS[rng.below(CLAIMS.len() as u64) as usize].to_string(),
        variant: VARIANTS[rng.below(VARIANTS.len() as u64) as usize],
        weight: 0.1 + (rng.below(10) as f64) / 10.0,
        evidence: None,
    }
}

fn canonicalize(obs: &mut [HexObservation]) {
    obs.sort_by(|a, b| {
        a.source
            .cmp(&b.source)
            .then(a.diagnosis_id.cmp(&b.diagnosis_id))
            .then(a.round.cmp(&b.round))
            .then(a.ordinal.cmp(&b.ordinal))
            .then(a.claim_key.cmp(&b.claim_key))
    });
}

fn states_equal(a: &MergedState, b: &MergedState) -> bool {
    let mut ao = a.observations.clone();
    let mut bo = b.observations.clone();
    canonicalize(&mut ao);
    canonicalize(&mut bo);
    if ao != bo {
        return false;
    }
    for v in VARIANTS {
        if (a.distribution.get(v) - b.distribution.get(v)).abs() > 1e-9 {
            return false;
        }
    }
    true
}

fn shuffle(rng: &mut Rng, v: &mut [HexObservation]) {
    for i in (1..v.len()).rev() {
        let j = rng.below((i + 1) as u64) as usize;
        v.swap(i, j);
    }
}

/// (a) Tombstoning a set of observations is byte-equal to merging the
/// multiset that never contained them. A `MERGE_SOURCE` observation should
/// never appear in a tombstone set (synthesis is derived, re-derived every
/// merge, and dropped on input already), so we only ever tombstone base keys.
#[test]
fn tombstone_equals_never_present() {
    let mut rng = Rng(0x1D872B41A3C9F00D);
    for _ in 0..1500 {
        let n = 1 + rng.below(8) as usize;
        let all: Vec<HexObservation> = (0..n).map(|k| gen_obs(&mut rng, k)).collect();

        // Choose a random subset of base keys to tombstone.
        let mut tombstoned: BTreeSet<DedupKey> = BTreeSet::new();
        for o in &all {
            if o.source != MERGE_SOURCE && rng.below(2) == 0 {
                tombstoned.insert(o.dedup_key());
            }
        }

        let survivors: Vec<HexObservation> = all
            .iter()
            .filter(|o| !tombstoned.contains(&o.dedup_key()))
            .cloned()
            .collect();

        let with_tombstones = merge_with_tombstones(&all, &tombstoned, None, None);
        let never_present = merge_all(&survivors);

        assert!(
            states_equal(&with_tombstones, &never_present),
            "tombstone must equal never-present; tombstoned={tombstoned:?} all={all:?}",
        );
    }
}

/// (b) The tombstoned merge depends only on the surviving multiset — not on
/// the order the observations arrive. Two independent shuffles of the same
/// input, under the same tombstone set, produce byte-equal state.
#[test]
fn tombstone_filter_commutes_with_merge_order() {
    let mut rng = Rng(0x9B7A3E15C0FFEE42);
    for _ in 0..1500 {
        let n = 1 + rng.below(8) as usize;
        let all: Vec<HexObservation> = (0..n).map(|k| gen_obs(&mut rng, k)).collect();

        let mut tombstoned: BTreeSet<DedupKey> = BTreeSet::new();
        for o in &all {
            if o.source != MERGE_SOURCE && rng.below(2) == 0 {
                tombstoned.insert(o.dedup_key());
            }
        }

        let mut a = all.clone();
        let mut b = all.clone();
        shuffle(&mut rng, &mut a);
        shuffle(&mut rng, &mut b);

        let sa = merge_with_tombstones(&a, &tombstoned, None, None);
        let sb = merge_with_tombstones(&b, &tombstoned, None, None);
        assert!(
            states_equal(&sa, &sb),
            "tombstoned merge must not depend on observation order; all={all:?}",
        );
    }
}
