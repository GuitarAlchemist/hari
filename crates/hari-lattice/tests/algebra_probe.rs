//! Randomized + adversarial probes of the algebraic claims made by
//! `hari_lattice::merge`.
//!
//! The in-module `proof_*` tests each check ONE hand-picked instance.
//! These probes check the same claims over thousands of seeded-random
//! inputs and a few adversarial constructions. Each test asserts the
//! DOCUMENTED claim (module comments: "restores associativity",
//! "load-bearing for CRDT correctness", "reproducible across runs
//! regardless of input order") — a failure here is a counterexample to
//! the documentation, not necessarily to the intended design.
//!
//! Deterministic: fixed-seed xorshift, no external deps.

use hari_lattice::merge::{merge, merge_all, HexObservation, MergedState};
use hari_lattice::HexValue;

// ───────────────────────── tiny deterministic RNG ─────────────────────────

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64*
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

const SOURCES: [&str; 4] = ["tars", "ix", "ga", "demerzel"];
const CLAIMS: [&str; 5] = [
    "k1::valuable",
    "k1::safe",
    "k2::valuable",
    "k2::cheap",
    "k3",
];
const VARIANTS: [HexValue; 6] = [
    HexValue::True,
    HexValue::Probable,
    HexValue::Unknown,
    HexValue::Doubtful,
    HexValue::False,
    HexValue::Contradictory,
];

fn rand_obs(rng: &mut Rng) -> HexObservation {
    HexObservation {
        source: SOURCES[rng.below(4) as usize].to_string(),
        diagnosis_id: format!("d{}", rng.below(4)),
        round: rng.below(6) as u32,
        ordinal: rng.below(3) as u32,
        claim_key: CLAIMS[rng.below(5) as usize].to_string(),
        variant: VARIANTS[rng.below(6) as usize],
        // Weights quantized to 1/16 steps in (0,1] so equality is exact.
        weight: (rng.below(16) + 1) as f64 / 16.0,
        evidence: None,
    }
}

/// Random multiset with DISTINCT dedup keys (well-formed input).
fn rand_wellformed(rng: &mut Rng, n: usize) -> Vec<HexObservation> {
    let mut out: Vec<HexObservation> = Vec::new();
    'outer: for _ in 0..n * 3 {
        if out.len() == n {
            break;
        }
        let o = rand_obs(rng);
        for e in &out {
            if e.dedup_key() == o.dedup_key() {
                continue 'outer;
            }
        }
        out.push(o);
    }
    out
}

fn shuffle(rng: &mut Rng, v: &mut [HexObservation]) {
    for i in (1..v.len()).rev() {
        let j = rng.below((i + 1) as u64) as usize;
        v.swap(i, j);
    }
}

fn states_equal(a: &MergedState, b: &MergedState) -> bool {
    if a.observations != b.observations || a.contradictions != b.contradictions {
        return false;
    }
    VARIANTS
        .iter()
        .all(|&v| (a.distribution.get(v) - b.distribution.get(v)).abs() < 1e-12)
}

// ───────────────────────── probes ─────────────────────────

/// THEOREM 1 (pinned): output is "reproducible across runs regardless
/// of input order" — TRUE for well-formed input (distinct dedup keys).
/// Checked over 2000 random multisets. See
/// `known_divergence_key_collision_is_order_dependent` for the
/// boundary where the claim fails.
#[test]
fn probe_permutation_invariance_wellformed() {
    let mut rng = Rng(0xDEADBEEF);
    for trial in 0..2000 {
        let base = rand_wellformed(&mut rng, 8);
        let mut perm = base.clone();
        shuffle(&mut rng, &mut perm);
        let sa = merge_all(&base);
        let sb = merge_all(&perm);
        assert!(
            states_equal(&sa, &sb),
            "permutation changed merge output (trial {trial})\nbase: {base:#?}"
        );
    }
}

/// KNOWN DIVERGENCE #1 (defect, pinned as current behavior): two
/// observations sharing a dedup key but disagreeing in payload.
/// `dedup_key`'s doc says "two observations with the same key ARE the
/// same observation", but the type does not enforce it, and first-wins
/// dedup keeps whichever arrived first — input order leaks into the
/// output, falsifying the step-1 claim "reproducible across runs
/// regardless of input order — load-bearing for CRDT correctness" on
/// representable input. This is also the SOLE root cause of the
/// associativity failure: `probe_associativity_globally_distinct_keys`
/// passes over 1000 random triples, while the same probe with
/// cross-set collisions fails.
///
/// Candidate fixes (owner call — epistemics, not refactoring): reject
/// colliding-key/differing-payload input as malformed; deterministic
/// content tie-break; or treat a self-contradicting source as
/// synthesizing `C`. Whichever is chosen, THIS TEST MUST FLIP —
/// it asserts the defective behavior so the fix is loud.
#[test]
fn known_divergence_key_collision_is_order_dependent() {
    let a = HexObservation {
        source: "tars".into(),
        diagnosis_id: "d0".into(),
        round: 0,
        ordinal: 0,
        claim_key: "k1::valuable".into(),
        variant: HexValue::True,
        weight: 1.0,
        evidence: None,
    };
    let b = HexObservation {
        variant: HexValue::False, // same key, opposite verdict
        weight: 0.5,
        ..a.clone()
    };
    let ab = merge_all(&[a.clone(), b.clone()]);
    let ba = merge_all(&[b, a]);
    // Current (defective) behavior: first-wins.
    assert_eq!(ab.observations[0].variant, HexValue::True);
    assert_eq!(ba.observations[0].variant, HexValue::False);
    assert!(
        !states_equal(&ab, &ba),
        "key-collision order-dependence FIXED — update the audit doc \
         (docs/research/2026-07-20-hex-merge-algebraic-audit.md §3) and \
         convert this into a permutation-invariance pin"
    );
}

/// THEOREM 2 (pinned): the content-derived synthesis-id design
/// ("the property that restores associativity",
/// `synthesis_diagnosis_id` doc) — TRUE conditional on globally
/// distinct dedup keys. Checked over 1000 random triples: carrying
/// merge(A∪B).observations into a merge with C equals the flat
/// merge(A∪B∪C), in both groupings.
///
/// The UNCONDITIONAL claim is false: the same probe with dedup keys
/// distinct only within each set (collisions possible across sets)
/// fails — first observed at seed 0xC0FFEE trial 28. The break is
/// entirely downstream of the key-collision defect (divergence #1):
/// an inner merge synthesizes `C` from a payload that loses its dedup
/// battle in the flat ordering, and the carried synthesis has no
/// surviving derivation. One defect, two symptoms.
#[test]
fn probe_associativity_globally_distinct_keys() {
    let mut rng = Rng(0xC0FFEE);
    for trial in 0..1000 {
        let pool = rand_wellformed(&mut rng, 12);
        if pool.len() < 12 {
            continue;
        }
        let a = pool[0..4].to_vec();
        let b = pool[4..8].to_vec();
        let c = pool[8..12].to_vec();

        let flat: Vec<_> = pool.clone();
        let s_flat = merge_all(&flat);

        let ab_state = merge_all(&a.iter().chain(b.iter()).cloned().collect::<Vec<_>>());
        let left: Vec<_> = ab_state
            .observations
            .iter()
            .chain(c.iter())
            .cloned()
            .collect();
        let s_left = merge_all(&left);

        let bc_state = merge_all(&b.iter().chain(c.iter()).cloned().collect::<Vec<_>>());
        let right: Vec<_> = a
            .iter()
            .cloned()
            .chain(bc_state.observations.iter().cloned())
            .collect();
        let s_right = merge_all(&right);

        assert!(
            states_equal(&s_left, &s_flat),
            "left-carried != flat with globally distinct keys (trial {trial})"
        );
        assert!(
            states_equal(&s_right, &s_flat),
            "right-carried != flat with globally distinct keys (trial {trial})"
        );
    }
}

/// Re-merge stability: merge(merge(X).observations) == merge(X).
/// Follows from the same content-derived-id design; checked at scale.
#[test]
fn probe_remerge_idempotence_randomized() {
    let mut rng = Rng(0xB16B00B5);
    for trial in 0..1000 {
        let x = rand_wellformed(&mut rng, 10);
        let s1 = merge_all(&x);
        let s2 = merge_all(&s1.observations);
        assert!(
            states_equal(&s1, &s2),
            "re-merge of own output changed state (trial {trial})"
        );
    }
}

/// Staleness divergence — "ghost contradiction". Two semantics are
/// available to a caller holding merge output:
///   (a) evidence-recompute: re-merge the RAW observations under the
///       current staleness window;
///   (b) state-carry: re-merge the previous OUTPUT (base + synthesized)
///       under the current window.
/// Construction: a (round 1, T) and b (round 5, F) conflict; synthesis
/// stamps the C observation with round max(1,5)=5. At current_round=5,
/// K=3 (cutoff 2), the raw recompute drops a — the pair no longer
/// coexists, so no contradiction. The carried C, stamped round 5,
/// survives its own parent's retirement.
///
/// KNOWN DIVERGENCE #2 (design gap, pinned as current behavior): the
/// two semantics DISAGREE — the substrate has two answers to "is this
/// claim contradictory?" and no documented rule for which is
/// authoritative. This is issue #16's retraction question in
/// miniature: a derived contradiction has no defined lifecycle when
/// the evidence beneath it is withdrawn or expires. Currently latent
/// (nothing in hari-core calls merge yet), but any consumer that
/// carries MergedState across staleness windows — the module's stated
/// cross-repo purpose — will hit it.
///
/// When the owner picks a semantics (issue #16), THIS TEST MUST FLIP.
#[test]
fn known_divergence_staleness_ghost_contradiction() {
    let a = HexObservation {
        source: "tars".into(),
        diagnosis_id: "d0".into(),
        round: 1,
        ordinal: 0,
        claim_key: "k::valuable".into(),
        variant: HexValue::True,
        weight: 1.0,
        evidence: None,
    };
    let b = HexObservation {
        source: "ix".into(),
        diagnosis_id: "d1".into(),
        round: 5,
        ordinal: 0,
        claim_key: "k::valuable".into(),
        variant: HexValue::False,
        weight: 1.0,
        evidence: None,
    };

    // Merge while both are live (no staleness yet): synthesizes C.
    let live = merge(&[a.clone(), b.clone()], None, None);
    assert_eq!(live.contradictions.len(), 1, "precondition: C synthesized");

    // (a) evidence-recompute at round 5, K=3.
    let recompute = merge(&[a, b], Some(5), Some(3));
    // (b) state-carry of the earlier output under the same window.
    let carried = merge(&live.observations, Some(5), Some(3));

    // Current behavior: the carried C (stamped round max(1,5)=5)
    // outlives its round-1 parent; the recompute never re-derives it.
    assert_eq!(recompute.contradictions.len(), 0);
    assert_eq!(carried.contradictions.len(), 1);
}

/// THEOREM 3 (pinned): anti-dilution. Naive normalization arithmetic
/// predicts an escalated contradiction can be washed out by piling on
/// corroborating support (C-mass 1/(3+n) < 0.3 after one P). The
/// naive arithmetic is wrong about this merge: each corroborating P
/// itself conflicts with the standing F ((P,F) → 0.8 in the Belnap
/// table) and synthesizes additional C mass. Measured trajectory
/// RISES monotonically from 1/3 toward the 0.8/1.8 ≈ 0.444 asymptote.
/// An escalation cannot be muted by corroboration while the dissent
/// stands — a genuine robustness property of the substrate, worth
/// pinning: it is what makes `escalation_triggered` resistant to
/// consensus-flooding by agreeing sources.
#[test]
fn theorem_escalation_is_antidilutive() {
    let mk = |src: &str, dx: &str, variant, ordinal| HexObservation {
        source: src.into(),
        diagnosis_id: dx.into(),
        round: 0,
        ordinal,
        claim_key: "k::valuable".into(),
        variant,
        weight: 1.0,
        evidence: None,
    };
    let a = mk("tars", "d0", HexValue::True, 0);
    let b = mk("ix", "d1", HexValue::False, 0);

    let mut obs = vec![a, b];
    let mut trajectory = Vec::new();
    for n in 0..20 {
        let state = merge_all(&obs);
        let c_mass = state.distribution.get(HexValue::Contradictory);
        trajectory.push(c_mass);
        if !state.distribution.escalation_triggered() {
            panic!("escalation muted after {n} corroborations; trajectory {trajectory:?}");
        }
        // one more corroborating source-distinct P observation
        obs.push(mk(
            &format!("extra{n}"),
            &format!("dx{n}"),
            HexValue::Probable,
            n,
        ));
    }
    // Naive normalization arithmetic predicts dilution: C-mass 1/(3+n)
    // drops below the 0.3 escalation threshold after ONE corroborating
    // P. The naive arithmetic is WRONG: each corroborating P itself
    // conflicts with the standing F ((P,F) → 0.8 in the Belnap table),
    // synthesizing additional C mass. Corroboration under standing
    // dissent COMPOUNDS the contradiction instead of washing it out —
    // the escalation flag cannot be muted by piling on support. This
    // is a genuine robustness property of the merge; pin it.
    eprintln!("C-mass trajectory under corroboration: {trajectory:?}");
    assert!(
        trajectory.iter().all(|&m| m > 0.3),
        "anti-dilution property lost: {trajectory:?}"
    );
}
