//! Does the hexavalent merge correspond to Subjective Logic fusion
//! under the codebase's own discretization? Probes for
//! `docs/research/2026-07-20-hexmerge-sl-correspondence.md`
//! (prior-art-survey.md §6 Q5, sharpened).
//!
//! Both sides of the comparison are the code's OWN committed artifacts:
//!
//! - Hex side: `hari_lattice::merge::merge_all` — the CRDT merge
//!   (pure function of the base-evidence multiset, post-audit-fix).
//! - SL side: `hari_core::Opinion` — `from_hex` is the codebase's
//!   committed HexValue → (b, d, u) embedding; `cumulative_fuse` is
//!   its sole fusion operator; `discounted` is its only mechanism for
//!   applying an evidence weight (used at 0.5 for agent votes).
//!
//! The two composite maps under test:
//!
//! - **map-then-fuse**: embed each observation via
//!   `from_hex(variant, 0.5).discounted(weight)`, then fold
//!   `cumulative_fuse` over the set.
//! - **merge-then-map**: run `merge_all`, then push the resulting
//!   `HexDistribution` through the unique affine extension of
//!   `from_hex` (a distribution is a convex combination of variants,
//!   so `(b,d,u) = Σ_v mass(v) · from_hex(v)`).
//!
//! If the hexavalent layer were "SL in a costume" the square would
//! commute (at least approximately, away from the C channel). The
//! probes below measure where it does and — mostly — where it does
//! not. Method imitates `hari-lattice/tests/algebra_probe.rs`:
//! deterministic xorshift, quantized weights, no new deps; exact
//! structural facts pinned as `theorem_*`, measured behavior pinned
//! as `probe_*` with bounds derived from the recorded runs.

use hari_core::{Opinion, SubjectiveLogicConfig};
use hari_lattice::merge::{merge_all, HexObservation, MergedState, ESCALATION_THRESHOLD};
use hari_lattice::HexValue;

// ───────────────────────── tiny deterministic RNG ─────────────────────────

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64* — same generator as algebra_probe.rs.
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

const VARIANTS: [HexValue; 6] = [
    HexValue::True,
    HexValue::Probable,
    HexValue::Unknown,
    HexValue::Doubtful,
    HexValue::False,
    HexValue::Contradictory,
];

const BASE_RATE: f64 = 0.5;

/// One observation on the shared claim from a distinct source.
/// Distinct sources per index: no dedup collisions, no same-source
/// synthesis skip — the comparison isolates fusion semantics, not the
/// dedup layer (audited separately in algebra_probe.rs).
fn obs(i: usize, variant: HexValue, weight: f64) -> HexObservation {
    HexObservation {
        source: format!("s{i}"),
        diagnosis_id: "d0".into(),
        round: 0,
        ordinal: 0,
        claim_key: "k::valuable".into(),
        variant,
        weight,
        evidence: None,
    }
}

fn rand_obs_set(rng: &mut Rng, n: usize) -> Vec<HexObservation> {
    (0..n)
        .map(|i| {
            let v = VARIANTS[rng.below(6) as usize];
            // Weights quantized to 1/16 steps in (0,1], as in
            // algebra_probe.rs.
            let w = (rng.below(16) + 1) as f64 / 16.0;
            obs(i, v, w)
        })
        .collect()
}

// ───────────────────────── the two composite maps ─────────────────────────

/// The code's own observation → opinion embedding: committed
/// `from_hex` table, weight applied through the code's only
/// weight-application mechanism.
fn map_obs(o: &HexObservation) -> Opinion {
    Opinion::from_hex(o.variant, BASE_RATE).discounted(o.weight)
}

/// map-then-fuse: fold cumulative fusion over the embedded set.
/// `cumulative_fuse` is commutative; the left fold in input order is
/// deterministic for these constructions.
fn fuse_mapped(observations: &[HexObservation]) -> Opinion {
    observations
        .iter()
        .map(map_obs)
        .fold(Opinion::vacuous(BASE_RATE), Opinion::cumulative_fuse)
}

/// merge-then-map: affine extension of `from_hex` over the merged
/// distribution. This is the unique extension of the committed
/// pointwise map to distributions (a distribution is a convex
/// combination of the six variants).
fn map_merged(state: &MergedState) -> Opinion {
    let mut b = 0.0;
    let mut d = 0.0;
    let mut u = 0.0;
    for v in VARIANTS {
        let mass = state.distribution.get(v);
        let op = Opinion::from_hex(v, BASE_RATE);
        b += mass * op.belief;
        d += mass * op.disbelief;
        u += mass * op.uncertainty;
    }
    Opinion {
        belief: b,
        disbelief: d,
        uncertainty: u,
        base_rate: BASE_RATE,
    }
}

/// L∞ deviation over the (b, d, u) simplex coordinates.
fn deviation(a: &Opinion, b: &Opinion) -> f64 {
    (a.belief - b.belief)
        .abs()
        .max((a.disbelief - b.disbelief).abs())
        .max((a.uncertainty - b.uncertainty).abs())
}

/// SL's conflict flag exactly as `recommend_from_opinion` /
/// `hex_value_for_opinion` compute it (both `b` and `d` above the
/// configured conflict threshold → Escalate / Contradictory).
fn sl_conflict(op: &Opinion, cfg: &SubjectiveLogicConfig) -> bool {
    op.belief > cfg.conflict_threshold && op.disbelief > cfg.conflict_threshold
}

// ───────────────────────── theorems (exact, pinned) ─────────────────────────

/// THEOREM A (structural separation): the hex merge is idempotent on
/// its evidence multiset (a CRDT proof obligation, pinned in
/// merge.rs::proof_idempotence), while SL cumulative fusion is
/// deliberately NOT idempotent — fusing an opinion with itself hardens
/// it (uncertainty strictly drops, belief strictly rises for a
/// T-embedded opinion). No injective state mapping can make an
/// idempotent operator commute with a non-idempotent one, so the
/// hexavalent merge CANNOT be cumulative fusion in a costume — under
/// the code's own mapping or any other faithful one. The quantitative
/// gap for the committed embedding of `True` is pinned below.
#[test]
fn theorem_idempotence_separates_merge_from_cumulative_fusion() {
    // Hex side: duplicating the multiset changes nothing (dedup key).
    let a = obs(0, HexValue::True, 1.0);
    let once = merge_all(std::slice::from_ref(&a));
    let twice = merge_all(&[a.clone(), a.clone()]);
    for v in VARIANTS {
        assert!(
            (once.distribution.get(v) - twice.distribution.get(v)).abs() < 1e-12,
            "hex merge lost idempotence — CRDT obligation broken"
        );
    }

    // SL side: self-fusion of the same evidence strictly hardens.
    let op = Opinion::from_hex(HexValue::True, BASE_RATE);
    let fused = Opinion::cumulative_fuse(op, op);
    // b: 0.85 → 2·0.85·0.10 / 0.19 = 0.894736…; u: 0.10 → 0.052631…
    assert!(
        (fused.belief - 0.85_f64 * 0.2 / 0.19).abs() < 1e-12,
        "expected b = 0.17/0.19, got {}",
        fused.belief
    );
    assert!(fused.belief - op.belief > 0.04, "self-fusion must harden b");
    assert!(
        op.uncertainty - fused.uncertainty > 0.04,
        "self-fusion must erode u"
    );
}

/// THEOREM B (agreement on the sharpest input): direct T-vs-F conflict
/// escalates on BOTH sides, with exact pinned values. Hex synthesizes
/// C at weight 1.0 → C-mass 1/3 > 0.3 escalation threshold. SL fuses
/// the embedded T and F opinions to b = d = 9/19 ≈ 0.4737, tripping
/// the conflict branch (> 0.4 both). At the decision level the two
/// systems agree here; the disagreement is in what happens NEXT (see
/// theorem_consensus_flooding_separates_the_two).
#[test]
fn theorem_direct_conflict_escalates_on_both_sides() {
    let cfg = SubjectiveLogicConfig::default();
    let set = [obs(0, HexValue::True, 1.0), obs(1, HexValue::False, 1.0)];

    // Hex side.
    let merged = merge_all(&set);
    let c = merged.distribution.get(HexValue::Contradictory);
    assert!((c - 1.0 / 3.0).abs() < 1e-12, "C-mass must be exactly 1/3");
    assert!(merged.distribution.escalation_triggered());

    // SL side: (0.85,0.05,0.10) ⊕ (0.05,0.85,0.10):
    // k = 0.19, b = d = (0.85·0.1 + 0.05·0.1)/0.19 = 0.09/0.19 = 9/19.
    let fused = fuse_mapped(&set);
    assert!((fused.belief - 9.0 / 19.0).abs() < 1e-12);
    assert!((fused.disbelief - 9.0 / 19.0).abs() < 1e-12);
    assert!(sl_conflict(&fused, &cfg), "SL conflict branch must fire");
}

/// THEOREM C (the sharp behavioral separation — the headline): under
/// consensus flooding (standing T-vs-F conflict + n corroborating
/// P observations from fresh sources), the two systems move in
/// OPPOSITE directions.
///
/// - Hex: each corroborating P itself conflicts with the standing F
///   ((P,F) → 0.8 in the Belnap table) and synthesizes MORE C. C-mass
///   rises monotonically from 1/3 toward 0.8/1.8 ≈ 0.444; escalation
///   can never be muted (algebra_probe.rs::theorem_escalation_is_
///   antidilutive).
/// - SL: each corroborating P adds belief evidence and erodes both u
///   and the disbelief share. The conflict flag (d > 0.4) clears
///   after finitely many corroborations, and belief later crosses the
///   Accept threshold (b > 0.7): SL de-escalates and ultimately
///   ACCEPTS the flooded proposition while the dissent still stands.
///
/// Measured on this construction (and confirmed analytically in
/// evidence space, where cumulative fusion is exactly additive:
/// T ↦ (r,s) = (17,1), F ↦ (1,17), P ↦ (11/3,1) with W = 2): SL's
/// conflict flag clears at n = 4 corroborating P's, and SL crosses
/// the Accept threshold at n = 22 — while hex C-mass at n = 22 is
/// ≈ 0.437, still rising toward 0.8/1.8 ≈ 0.444 and still escalated.
/// This is a formal divergence, not a tuning artifact: hex-merge's
/// C-channel is anti-dilutive by construction, SL cumulative fusion
/// is evidence-additive by construction.
#[test]
fn theorem_consensus_flooding_separates_the_two() {
    let cfg = SubjectiveLogicConfig::default();
    let mut set = vec![obs(0, HexValue::True, 1.0), obs(1, HexValue::False, 1.0)];

    let mut sl_conflict_cleared_at: Option<usize> = None;
    let mut sl_accept_at: Option<usize> = None;
    let mut prev_c = 0.0;

    for n in 0..=25 {
        let merged = merge_all(&set);
        let c = merged.distribution.get(HexValue::Contradictory);
        let fused = fuse_mapped(&set);

        // Hex side: monotone non-decreasing C-mass, never de-escalates.
        assert!(
            c >= prev_c - 1e-12,
            "hex C-mass dropped under corroboration at n={n}: {prev_c} -> {c}"
        );
        assert!(
            merged.distribution.escalation_triggered(),
            "hex escalation muted at n={n} — anti-dilution lost"
        );
        prev_c = c;

        // SL side: record the crossings.
        if sl_conflict_cleared_at.is_none() && n > 0 && !sl_conflict(&fused, &cfg) {
            sl_conflict_cleared_at = Some(n);
        }
        if sl_accept_at.is_none() && fused.belief > cfg.belief_accept_threshold {
            sl_accept_at = Some(n);
        }

        set.push(obs(2 + n, HexValue::Probable, 1.0));
    }

    // Pinned crossings, measured on this deterministic construction.
    assert_eq!(
        sl_conflict_cleared_at,
        Some(4),
        "SL conflict flag should clear after 4 corroborating P's"
    );
    assert_eq!(
        sl_accept_at,
        Some(22),
        "SL belief should cross the Accept threshold after 22 P's \
         (evidence space: b = (18 + 11n/3)/(38 + 11n/3 + n) > 0.7 iff n > 21.5)"
    );
    // At the point where SL Accepts, hex is still escalated with
    // C-mass near its 0.444 asymptote (at n = 25: 21/48 = 0.4375).
    assert!(prev_c > 0.43 && prev_c < 0.8 / 1.8 + 1e-9);
    assert!(prev_c > ESCALATION_THRESHOLD);
}

/// THEOREM C′ (v1.2 — issue #28 closed the mirror separation this
/// theorem originally pinned): flood the same standing T-vs-F
/// conflict with n Unknown observations from fresh sources.
/// `Unknown` synthesizes nothing in the Belnap table but carries
/// full normalization mass, so raw hex C-mass is still exactly
/// 1/(3+n) — under spec v1.1 ONE Unknown muted the escalation flag
/// (1/4 < 0.3). Spec v1.2 escalates on C's share of *informative*
/// mass (U excluded), so the share stays 1/3 for every n and the
/// alarm survives. SL's embedded Unknown is near-vacuous evidence
/// ((r,s) = (1/9, 1/9)): b = d → 0.5 from below and its conflict
/// flag never clears either.
///
/// Together with Theorem C: hex escalation is now immune to BOTH
/// flooding directions (corroborating support and abstention), while
/// SL conflict remains support-mortal. The v1.1 version of this
/// theorem pinned the opposite hex behavior — the flip is the point
/// (see the ratification on issue #28).
#[test]
fn theorem_unknown_flooding_mirrors_the_separation() {
    let cfg = SubjectiveLogicConfig::default();
    let mut set = vec![obs(0, HexValue::True, 1.0), obs(1, HexValue::False, 1.0)];

    // n = 0: both escalated (Theorem B).
    let merged = merge_all(&set);
    assert!(merged.distribution.escalation_triggered());
    assert!(sl_conflict(&fuse_mapped(&set), &cfg));

    for n in 1..=20 {
        set.push(obs(1 + n, HexValue::Unknown, 1.0));
        let merged = merge_all(&set);
        let c = merged.distribution.get(HexValue::Contradictory);
        assert!(
            (c - 1.0 / (3.0 + n as f64)).abs() < 1e-12,
            "raw C-mass should be exactly 1/(3+n) at n={n}"
        );
        assert!(
            merged.distribution.escalation_triggered(),
            "abstention must no longer mute hex escalation (n={n}; spec v1.2)"
        );
        let fused = fuse_mapped(&set);
        assert!(
            sl_conflict(&fused, &cfg),
            "SL conflict must survive Unknown-flooding (n={n}): b={} d={}",
            fused.belief,
            fused.disbelief
        );
    }
}

/// THEOREM D (contradiction-free separation): even with NO
/// contradiction anywhere — all sources agreeing — the square does
/// not commute. Corroboration is invisible to the hex distribution
/// (n agreeing T's normalize back to 100% T; merge-then-map is
/// CONSTANT at the embedded (0.85, 0.05, 0.10)), while SL hardens
/// monotonically toward certainty (belief gap ≈ 0.086 at n = 12,
/// still growing: b_n = 17n/(18n+2) → 0.944…). Same shape for
/// all-Unknown: merge-then-map stays at u = 0.90 while SL fusion of
/// repeated near-vacuous opinions erodes u = 9/(9+n) below 0.5 at
/// n = 10. The hexavalent distribution is an AVERAGING statistic of
/// the evidence; cumulative fusion is an ACCUMULATING one. The
/// deviation is therefore not confined to the C channel.
#[test]
fn theorem_agreeing_corroboration_diverges_without_any_contradiction() {
    // All-True chain.
    let mut gap_prev = 0.0;
    for n in 1..=12 {
        let set: Vec<_> = (0..n).map(|i| obs(i, HexValue::True, 1.0)).collect();
        let merged = merge_all(&set);
        assert!(merged.contradictions.is_empty(), "no C anywhere by design");
        let h = map_merged(&merged);
        // merge-then-map is constant: distribution is 100% True.
        assert!((h.belief - 0.85).abs() < 1e-12);
        assert!((h.uncertainty - 0.10).abs() < 1e-12);
        let s = fuse_mapped(&set);
        let gap = s.belief - h.belief;
        if n == 1 {
            assert!(gap.abs() < 1e-12, "single observation must commute");
        } else {
            assert!(
                gap > gap_prev,
                "SL hardening gap must grow with corroboration (n={n})"
            );
        }
        gap_prev = gap;
    }
    // Measured: at n = 12 the belief gap alone is ≈ 0.0858
    // (17·12/(18·12+2) − 0.85 = 0.08578…), and still growing.
    assert!(gap_prev > 0.08, "expected ≥0.08 belief gap, got {gap_prev}");

    // All-Unknown chain: SL erodes uncertainty that hex preserves.
    let mut below_half_at = None;
    for n in 1..=12 {
        let set: Vec<_> = (0..n).map(|i| obs(i, HexValue::Unknown, 1.0)).collect();
        let h = map_merged(&merge_all(&set));
        assert!((h.uncertainty - 0.90).abs() < 1e-12);
        let s = fuse_mapped(&set);
        if below_half_at.is_none() && s.uncertainty < 0.5 {
            below_half_at = Some(n);
        }
    }
    // Exact form u_n = 9/(9+n): equals 0.5 at n = 9, strictly below
    // at n = 10.
    assert_eq!(
        below_half_at,
        Some(10),
        "SL uncertainty should fall below 0.5 after 10 'Unknown' observations"
    );
}

// ───────────────────────── randomized probes (measured, pinned) ─────────────

/// PROBE 1: map-then-fuse vs merge-then-map over 2000 random
/// observation sets (2–8 distinct-source observations on one claim,
/// quantized weights, all six variants). Measured on seed 0x51C0DE:
///
/// - overall:            mean L∞ dev ≈ 0.167, max ≈ 0.686
/// - hex synthesized C:  mean ≈ 0.163 over 1360 trials, max ≈ 0.605
/// - no C synthesized:   mean ≈ 0.174 over  640 trials, max ≈ 0.686
/// - near-commuting trials (dev < 0.01): 24 of 2000
///
/// The square does NOT commute, and the deviation does NOT
/// concentrate on contradiction-bearing inputs — the
/// contradiction-FREE stratum deviates slightly MORE on average
/// (0.174 vs 0.163) and holds the maximum. The C channel is not the
/// obstruction; the averaging-vs-accumulating mismatch (Theorems
/// A/D) and the relative-vs-absolute weight semantics (Theorem E)
/// dominate. Bounds below pin the measured run with a small safety
/// margin.
#[test]
fn probe_map_then_fuse_vs_merge_then_map_randomized() {
    let mut rng = Rng(0x51C0DE);
    let trials = 2000;

    let mut devs_all: Vec<f64> = Vec::with_capacity(trials);
    let mut devs_with_c: Vec<f64> = Vec::new();
    let mut devs_without_c: Vec<f64> = Vec::new();
    let mut commuting = 0usize;

    for _ in 0..trials {
        let n = 2 + rng.below(7) as usize; // 2..=8
        let set = rand_obs_set(&mut rng, n);
        let merged = merge_all(&set);
        let h = map_merged(&merged);
        let s = fuse_mapped(&set);
        let dev = deviation(&h, &s);
        devs_all.push(dev);
        if merged.contradictions.is_empty() {
            devs_without_c.push(dev);
        } else {
            devs_with_c.push(dev);
        }
        if dev < 0.01 {
            commuting += 1;
        }
    }

    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len().max(1) as f64;
    let max = |v: &[f64]| v.iter().cloned().fold(0.0, f64::max);

    eprintln!(
        "probe1: all n={} mean={:.4} max={:.4} | with-C n={} mean={:.4} max={:.4} | no-C n={} mean={:.4} max={:.4} | commuting(<0.01)={}",
        devs_all.len(), mean(&devs_all), max(&devs_all),
        devs_with_c.len(), mean(&devs_with_c), max(&devs_with_c),
        devs_without_c.len(), mean(&devs_without_c), max(&devs_without_c),
        commuting
    );

    // The correspondence FAILS at scale: mean deviation is an order
    // of magnitude above any numerical-noise explanation.
    assert!(
        mean(&devs_all) > 0.12,
        "mean deviation collapsed (measured ≈ 0.167) — did the mapping or merge change?"
    );
    assert!(max(&devs_all) < 0.80, "deviation blew past recorded range");
    // Both strata deviate substantially: the C channel is NOT the
    // obstruction (contradiction-free inputs deviate slightly MORE).
    assert!(!devs_without_c.is_empty());
    assert!(
        mean(&devs_without_c) > 0.12,
        "contradiction-free inputs deviate too (measured ≈ 0.174), got {}",
        mean(&devs_without_c)
    );
    assert!(
        mean(&devs_with_c) > 0.12,
        "contradiction-bearing inputs deviate (measured ≈ 0.163), got {}",
        mean(&devs_with_c)
    );
    // Near-commuting inputs are rare edge cases, not the rule.
    assert!(
        commuting < trials / 20,
        "square started commuting on {commuting}/{trials} trials"
    );
}

/// PROBE 2: decision-level confusion matrix — hex `escalation_
/// triggered` (spec v1.2: C over informative mass > 0.3) vs SL's
/// conflict branch (b > 0.4 AND d > 0.4) over 2000 random sets
/// (seed 0xE5CA1A7E). Measured cells under v1.2:
///
///   hex-esc ∧ SL-conflict:      180   (agree: escalate)
///   ¬hex-esc ∧ ¬SL-conflict:    742   (agree: no escalation)
///   hex-esc ∧ ¬SL-conflict:    1076   (hex escalates alone)
///   ¬hex-esc ∧ SL-conflict:       2   (SL conflicts alone)
///
/// Agreement ≈ 46.1% — below a coin flip. (v1.1 historical cells:
/// 172/923/895/10, agreement ≈ 54.8%.) The asymmetry got MORE
/// one-sided with the issue-#28 change: hex now escalates alone on
/// ~54% of inputs — the U-heavy sets that used to mute it moved from
/// the agree-no column into hex-only — while SL-alone conflicts
/// dropped 10 → 2 because most of those stray cells WERE the
/// Unknown-dilution mirror pinned by Theorem C′, now closed. Neither
/// detector simulates the other.
#[test]
fn probe_escalation_decision_confusion_matrix() {
    let cfg = SubjectiveLogicConfig::default();
    let mut rng = Rng(0xE5CA1A7E);
    let trials = 2000;

    let (mut both, mut neither, mut hex_only, mut sl_only) = (0usize, 0usize, 0usize, 0usize);

    for _ in 0..trials {
        let n = 2 + rng.below(7) as usize;
        let set = rand_obs_set(&mut rng, n);
        let hex_esc = merge_all(&set).distribution.escalation_triggered();
        let slc = sl_conflict(&fuse_mapped(&set), &cfg);
        match (hex_esc, slc) {
            (true, true) => both += 1,
            (false, false) => neither += 1,
            (true, false) => hex_only += 1,
            (false, true) => sl_only += 1,
        }
    }

    eprintln!(
        "probe2: both={both} neither={neither} hex_only={hex_only} sl_only={sl_only} (agreement {:.1}%)",
        100.0 * (both + neither) as f64 / trials as f64
    );

    // Shape of the asymmetry is the finding: hex-alone escalations are
    // massive (measured 1076/2000 under v1.2), SL-alone conflicts are
    // near-extinct (measured 2/2000 — the Theorem C′ mirror is closed
    // by v1.2), and total agreement is below chance.
    assert!(
        hex_only > trials / 5,
        "hex-only escalations collapsed (measured 1076/2000), got {hex_only}"
    );
    assert!(
        sl_only < trials / 50,
        "SL-only conflicts should be near-zero under v1.2 (measured 2/2000), got {sl_only}"
    );
    assert!(both > 0 && neither > 0, "degenerate matrix");
    let agreement = (both + neither) as f64 / trials as f64;
    assert!(
        agreement < 0.70,
        "decision agreement rose past the recorded ≈0.55 band: {agreement}"
    );
}

/// THEOREM E: the near-commuting boundary. Where DOES the square
/// commute? Exactly on singleton evidence — one observation maps to
/// the same opinion along both paths by construction (modulo the
/// weight channel: hex-merge normalizes the weight away, so the
/// merge-then-map path forgets `weight` on singletons while
/// map-then-fuse applies it via `discounted`; the paths agree exactly
/// iff weight = 1). Pinned exactly: full-weight singletons commute to
/// < 1e-12 for all six variants; a half-weight True singleton already
/// deviates by 0.45 in the u channel (hex normalizes back to pure T
/// = (0.85, 0.05, 0.10); SL discounts to (0.425, 0.025, 0.55)).
/// Weight handling is thus a THIRD independent deviation channel:
/// hex-merge weights are relative (normalized within the multiset),
/// SL discounts are absolute.
#[test]
fn theorem_singleton_commutes_iff_full_weight() {
    for v in VARIANTS {
        let set = [obs(0, v, 1.0)];
        let h = map_merged(&merge_all(&set));
        let s = fuse_mapped(&set);
        assert!(
            deviation(&h, &s) < 1e-12,
            "full-weight singleton must commute for {v:?}"
        );
    }
    // Half-weight True singleton: hex normalizes 0.5/0.5 → pure T,
    // SL discounts toward vacuous. Exact deviation: 0.45 (u channel).
    let set = [obs(0, HexValue::True, 0.5)];
    let h = map_merged(&merge_all(&set));
    let s = fuse_mapped(&set);
    let dev = deviation(&h, &s);
    assert!(
        (dev - 0.45).abs() < 1e-12,
        "half-weight singleton should expose the weight-semantics gap at exactly 0.45, got {dev}"
    );
}
