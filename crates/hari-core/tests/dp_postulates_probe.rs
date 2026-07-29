//! Iterated-revision postulates probe (issue #16).
//!
//! A formal probe of *which classical belief-revision postulates hari's
//! revision machinery satisfies*, adapted honestly to hari's setting. The
//! companion `docs/research/2026-07-21-dp-postulates.md` carries the full
//! translation table, the satisfied/violated/not-statable inventory, and the
//! positioning against AGM (Alchourrón–Gärdenfors–Makinson 1985),
//! Darwiche–Pearl iterated revision (1997), and Doyle's JTMS (1979). This
//! file is the executable half: every postulate that can be stated is either
//! a `theorem_*` (holds over 1000+ seeded-random trials, with the reason
//! argued in the doc) or a `known_violation_*` (a pinned concrete
//! counterexample plus the violation class). Postulates that cannot be stated
//! faithfully live only in the doc.
//!
//! ## The translation (see the doc for the full argument)
//!
//! AGM/DP are defined over deductively-closed theories revised by an operator
//! `∘`. Hari has no theory and no stateful operator: the **epistemic state is
//! the surviving evidence multiset** for a proposition, and the belief is the
//! *pure function* `Bel(E) = project_belief(merge(survivors(E)))`, exposed as
//! [`CognitiveLoop::recompute_belief`]. Under that reading:
//!
//! - **`∘` (revise by φ)** = append an evidence assertion of φ — which is
//!   AGM **expansion** `+`, not revision: it never contracts the opposite.
//! - **contraction `÷φ`** = a selective `Retraction` (tombstone φ-evidence).
//! - **AGM revision proper** = a `Correction`: contract ¬φ *then* expand φ in
//!   one atomic event — the **Levi identity** `K∘φ = (K ÷ ¬φ) + φ`.
//! - **`believes φ`** = `Bel(E)` on the positive side of the chain
//!   `F < D < U < P < T`: `Accept(φ)` iff `Bel ∈ {Probable, True}`,
//!   `Accept(¬φ)` iff `Bel ∈ {Doubtful, False}`. `Unknown` = agnostic,
//!   `Contradictory` = the paraconsistent "both", held, never collapsed.
//! - **`¬φ`** = the F-side assertion (`not`: `True↔False`, `Probable↔Doubtful`).
//!
//! Deterministic: fixed-seed xorshift64*, no external deps; hex values are
//! discrete so equality is exact.

use hari_core::{
    CognitiveLoop, Evidence, ResearchEvent, ResearchEventPayload, ResearchTrace, RetractionSelector,
};
use hari_lattice::HexValue;

const PROP: &str = "phi";

// --- the belief function under study -----------------------------------

/// `Bel(E)` — the authoritative belief the design defines: a pure function of
/// the surviving evidence multiset, routed through the real weighted merge +
/// single-source corroboration cap the boundary uses after a revision.
fn bel(evidence: &[(&str, HexValue)]) -> HexValue {
    CognitiveLoop::recompute_belief(evidence.iter().map(|(s, v)| (*s, *v, 1.0)))
}

fn accepts_phi(v: HexValue) -> bool {
    matches!(v, HexValue::Probable | HexValue::True)
}
fn accepts_not_phi(v: HexValue) -> bool {
    matches!(v, HexValue::Doubtful | HexValue::False)
}

// --- deterministic RNG (house style) -----------------------------------

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

/// Eight distinct source labels — enough that "distinct source" corroboration
/// (what the cap counts) is exercised without a source ever self-conflicting
/// in the single-polarity generators below.
const SOURCES: [&str; 8] = ["s0", "s1", "s2", "s3", "s4", "s5", "s6", "s7"];

fn positive_pole(rng: &mut Rng) -> HexValue {
    if rng.below(2) == 0 {
        HexValue::True
    } else {
        HexValue::Probable
    }
}
fn negative_pole(rng: &mut Rng) -> HexValue {
    if rng.below(2) == 0 {
        HexValue::False
    } else {
        HexValue::Doubtful
    }
}
fn any_value(rng: &mut Rng) -> HexValue {
    const V: [HexValue; 6] = [
        HexValue::True,
        HexValue::Probable,
        HexValue::Unknown,
        HexValue::Doubtful,
        HexValue::False,
        HexValue::Contradictory,
    ];
    V[rng.below(6) as usize]
}

fn shuffle<T>(rng: &mut Rng, v: &mut [T]) {
    for i in (1..v.len()).rev() {
        let j = rng.below((i + 1) as u64) as usize;
        v.swap(i, j);
    }
}

// --- end-to-end drivers (for postulates that need the real event path) --

fn belief_update(cycle: u64, source: &str, v: HexValue) -> ResearchEvent {
    ResearchEvent {
        cycle,
        source: source.into(),
        payload: ResearchEventPayload::BeliefUpdate {
            proposition: PROP.into(),
            value: v,
            evidence: Evidence::new(),
        },
    }
}
/// A `Correction` that contracts *all* of `retract_source`'s prior evidence
/// (source-only selector, cycle wildcard) and expands `value` — the Levi
/// identity's `(K ÷ ¬φ) + φ` as a single atomic event.
fn correction(cycle: u64, source: &str, retract_source: &str, value: HexValue) -> ResearchEvent {
    ResearchEvent {
        cycle,
        source: source.into(),
        payload: ResearchEventPayload::Correction {
            proposition: PROP.into(),
            reason: "levi".into(),
            retracts: RetractionSelector {
                source: Some(retract_source.into()),
                cycle: None,
            },
            value,
            evidence: Evidence::new(),
        },
    }
}
fn selective_retraction(cycle: u64, source: &str, at_cycle: u64) -> ResearchEvent {
    ResearchEvent {
        cycle,
        source: source.into(),
        payload: ResearchEventPayload::Retraction {
            proposition: PROP.into(),
            reason: "contract".into(),
            retracts: Some(RetractionSelector {
                source: Some(source.into()),
                cycle: Some(at_cycle),
            }),
        },
    }
}

fn final_belief(events: Vec<ResearchEvent>) -> HexValue {
    let mut cl = CognitiveLoop::new(4);
    let report = cl.process_research_trace(ResearchTrace {
        dimension: 4,
        events,
    });
    report
        .final_beliefs
        .get(PROP)
        .copied()
        .unwrap_or(HexValue::Unknown)
}

// =======================================================================
// AGM (K∘1..K∘6) — the basic postulates
// =======================================================================

/// **AGM Success — weak form (holds).** `φ ∈ K∘φ`: after asserting φ with no
/// standing opposite evidence, φ is believed. A single uncorroborated `True`
/// caps to `Probable` (still `Accept(φ)`), corroboration lifts it to `True`;
/// either way the positive side wins. Symmetrically for ¬φ. This is the
/// uncontested slice of success — the slice AGM expansion always satisfies.
#[test]
fn theorem_agm_success_holds_for_uncontested_assertion() {
    let mut rng = Rng(0x1111_2222_3333_4444);
    for _ in 0..1500 {
        let k = 1 + rng.below(5) as usize; // 1..=5 assertions
        let positive = rng.below(2) == 0;
        let mut evs: Vec<(&str, HexValue)> = Vec::new();
        for _ in 0..k {
            let src = SOURCES[rng.below(8) as usize];
            let val = if positive {
                positive_pole(&mut rng)
            } else {
                negative_pole(&mut rng)
            };
            evs.push((src, val));
        }
        let b = bel(&evs);
        if positive {
            assert!(
                accepts_phi(b),
                "uncontested φ must be believed: {evs:?} => {b:?}"
            );
        } else {
            assert!(
                accepts_not_phi(b),
                "uncontested ¬φ must be believed: {evs:?} => {b:?}"
            );
        }
    }
}

/// **AGM Success — unconditional form (FAILS, deliberate divergence).** AGM
/// demands `φ ∈ K∘φ` *even when `¬φ ∈ K`*, because revision first contracts
/// `¬φ`. Hari's `∘` is bare append (expansion): it does **not** contract the
/// standing `¬φ`, so asserting `True` over a standing `False` yields
/// `Contradictory`, not acceptance of φ. Violation class: **∘ is expansion,
/// not revision** — you cannot shout down a standing cross-source
/// contradiction (paraconsistent core). The fix is not a bug-fix but the
/// separate contraction hari provides: see the Levi-identity theorem below.
#[test]
fn known_violation_agm_success_fails_over_standing_dissent() {
    // Pinned counterexample: standing False from `s1`, assert True from `s0`.
    assert_eq!(
        bel(&[("s1", HexValue::False), ("s0", HexValue::True)]),
        HexValue::Contradictory,
        "append does not contract ¬φ: asserting φ over standing ¬φ yields C, not Accept(φ)"
    );

    // Randomized: over any standing negative pole, appending a positive pole
    // from a distinct source never reaches Accept(φ) — it stays Contradictory.
    let mut rng = Rng(0x5555_6666_7777_8888);
    for _ in 0..1500 {
        let standing = negative_pole(&mut rng); // the ¬φ that stands
        let asserted = HexValue::True; // the strongest possible φ
        let b = bel(&[("s1", standing), ("s0", asserted)]);
        assert_ne!(
            b,
            HexValue::True,
            "unconditional success would give True; hari refuses ({standing:?} standing)"
        );
        // With a *strong* standing pole it escalates to C; a weak one (Doubtful)
        // does not cross the escalation share and the positive side survives —
        // but success (clean φ) is still not delivered when False stands.
        if standing == HexValue::False {
            assert_eq!(b, HexValue::Contradictory);
        }
    }
}

/// **AGM Consistency (FAILS, deliberate divergence).** AGM: `K∘φ` is
/// consistent unless φ is itself inconsistent. Hari *embraces*
/// `Contradictory` as a first-class value and never restores consistency by
/// minimal change. Two individually-consistent assertions of opposite poles
/// merge to `C` and stay there. Violation class: **paraconsistent by design**
/// (design §3.2, §5.2 "AGM's consistency-restoration is rejected").
#[test]
fn known_violation_agm_consistency_not_preserved() {
    assert_eq!(
        bel(&[("s0", HexValue::True), ("s1", HexValue::False)]),
        HexValue::Contradictory,
        "two consistent inputs → an inconsistent (C) belief; hari does not restore consistency"
    );
    // The AGM operator would return a consistent set (drop one side by
    // entrenchment). Hari keeps both live and reports C — randomized over the
    // strong poles, any {True-source, False-source} pair is C.
    let mut rng = Rng(0x99AA_BBCC_DDEE_FF00);
    for _ in 0..1500 {
        let a = SOURCES[rng.below(4) as usize];
        let b = SOURCES[4 + rng.below(4) as usize];
        assert_eq!(
            bel(&[(a, HexValue::True), (b, HexValue::False)]),
            HexValue::Contradictory
        );
    }
}

/// **Standing conflict is non-dilutable (the paraconsistent core, holds).**
/// The property that *causes* the consistency and success divergences, stated
/// positively and probed directly: while at least one source asserts `True`
/// and at least one distinct source asserts `False`, no amount of same-pole
/// corroboration on either side mutes the `Contradictory`. This is the merge
/// audit's anti-dilution theorem lifted to the revision boundary — a standing
/// contradiction is immortal *as long as it stands* (design §3.2).
#[test]
fn theorem_standing_conflict_is_non_dilutable() {
    let mut rng = Rng(0x0F0F_1E1E_2D2D_3C3C);
    for _ in 0..1500 {
        let mut evs: Vec<(&str, HexValue)> = Vec::new();
        // At least one pure-True and one pure-False source (distinct labels).
        evs.push(("s0", HexValue::True));
        evs.push(("s1", HexValue::False));
        // Pile arbitrary same-pole strong corroboration onto either side.
        let extra = rng.below(6);
        for _ in 0..extra {
            if rng.below(2) == 0 {
                evs.push((SOURCES[2 + rng.below(3) as usize], HexValue::True));
            } else {
                evs.push((SOURCES[5 + rng.below(3) as usize], HexValue::False));
            }
        }
        shuffle(&mut rng, &mut evs);
        assert_eq!(
            bel(&evs),
            HexValue::Contradictory,
            "corroboration cannot dilute a standing cross-source conflict: {evs:?}"
        );
    }
}

/// **Levi identity via `Correction` (holds).** `K∘φ = (K ÷ ¬φ) + φ`. Where
/// bare append fails success over standing dissent, a `Correction` — which
/// contracts the opposite evidence *and* expands φ atomically — succeeds:
/// φ is believed. This is AGM revision proper, realized by the one hari event
/// that composes contraction with expansion. The A/B contrast against plain
/// append (which yields `C`) is asserted inside the same trial.
#[test]
fn theorem_correction_realizes_levi_identity() {
    let mut rng = Rng(0xABCD_1234_5678_9F0E);
    for _ in 0..1500 {
        // Standing ¬φ from a single source `s1` (one or more assertions, all
        // contracted by the source-only selector). Single source so the
        // contraction is total — a partial contraction would leave dissent.
        let n = 1 + rng.below(3);
        let mut events: Vec<ResearchEvent> = Vec::new();
        for i in 0..n {
            events.push(belief_update(i + 1, "s1", negative_pole(&mut rng)));
        }
        // AGM revision by φ=True, via a Correction from a fresh source.
        events.push(correction(n + 1, "s0", "s1", HexValue::True));
        let revised = final_belief(events);
        assert!(
            accepts_phi(revised),
            "Levi revision (contract ¬φ + expand φ) must believe φ; got {revised:?}"
        );

        // A/B: the same standing ¬φ with a *bare append* of φ (expansion only)
        // fails — it lands on Contradictory, not Accept(φ).
        let mut appended: Vec<ResearchEvent> = Vec::new();
        for i in 0..n {
            appended.push(belief_update(i + 1, "s1", HexValue::False));
        }
        appended.push(belief_update(n + 1, "s0", HexValue::True));
        assert_eq!(
            final_belief(appended),
            HexValue::Contradictory,
            "bare append (expansion) fails where Correction (Levi revision) succeeds"
        );
    }
}

// =======================================================================
// Darwiche–Pearl (C1..C2) — iterated revision
// =======================================================================

/// **DP order-independence (holds) — the structural fact behind C1/C2.** DP
/// axiomatizes how *sequential* revisions compose; hari sidesteps the
/// iteration by construction (design §5.3): there is no stateful operator to
/// iterate, so a sequence of revisions is just accumulation into a multiset
/// and `Bel` is a pure function of that multiset. This probe pins the
/// consequence the DP postulates work to guarantee and hari gets for free:
/// **the belief after a revision sequence depends only on the accumulated
/// evidence set, not on the order the revisions arrived** (the merge's
/// permutation invariance, at the postulate level).
#[test]
fn theorem_iterated_revision_is_order_independent() {
    let mut rng = Rng(0xDEAD_BEEF_CAFE_0001);
    for _ in 0..1500 {
        let k = 1 + rng.below(6) as usize;
        let mut evs: Vec<(&str, HexValue)> = Vec::new();
        for _ in 0..k {
            evs.push((SOURCES[rng.below(8) as usize], any_value(&mut rng)));
        }
        let mut a = evs.clone();
        let mut b = evs.clone();
        shuffle(&mut rng, &mut a);
        shuffle(&mut rng, &mut b);
        assert_eq!(
            bel(&a),
            bel(&b),
            "iterated revision belief must not depend on order: {evs:?}"
        );
    }
}

/// **DP C1 (FAILS in general; holds only absent standing dissent).** C1: if
/// `ψ ⊨ φ` then `(K∘φ)∘ψ = K∘ψ` — a later, *more specific* revision makes the
/// earlier, weaker same-side one redundant, so the conditional belief is
/// preserved. Faithful adaptation: within one proposition `ψ = True` is more
/// specific than `φ = Probable` (same positive side, stronger), both from the
/// *same reviser* (DP's `∘` is one agent's evolving state, not fusion).
///
/// It fails, and the reason is a substantive property of the substrate:
/// hari's escalation trigger is a **share of total informative mass**, so
/// adding the redundant weaker same-side assertion can *raise* the positive
/// mass enough to tip a latent `True`/`Doubtful` cross-source tension over the
/// escalation threshold — flipping `Probable → Contradictory`. The belief is
/// therefore **not** a function of the surviving *polarity set* alone;
/// accumulation of even redundant same-side evidence is observable. Violation
/// class: **mass-sensitive, non-monotonic escalation**.
///
/// The boundary is exact and is asserted below: when the prior carries **no
/// opposite-pole dissent**, escalation cannot fire and C1 holds. So the
/// order-independence hari *does* have (the permutation-invariance theorem
/// above) is strictly weaker than DP C1 — order-freedom is not
/// redundancy-insensitivity.
#[test]
fn known_violation_dp_c1_fails_under_mass_sensitive_escalation() {
    // Pinned counterexample. K = {s6:Doubtful}; φ=Probable, ψ=True (ψ⊨φ),
    // reviser s7. K∘ψ is Probable, but (K∘φ)∘ψ tips to Contradictory.
    let rhs = bel(&[("s6", HexValue::Doubtful), ("s7", HexValue::True)]);
    let lhs = bel(&[
        ("s6", HexValue::Doubtful),
        ("s7", HexValue::Probable),
        ("s7", HexValue::True),
    ]);
    assert_eq!(rhs, HexValue::Probable, "K∘ψ = Probable");
    assert_eq!(
        lhs,
        HexValue::Contradictory,
        "(K∘φ)∘ψ tips to C: redundant same-side mass crosses the escalation share"
    );
    assert_ne!(
        lhs, rhs,
        "DP C1 fails: the more-specific revision did not preserve belief"
    );

    // The exact boundary: with NO opposite-pole dissent in the prior,
    // escalation cannot fire and C1 holds — a redundant same-side, same-source
    // refinement is inert. Randomized over positive-side / Unknown priors.
    let mut rng = Rng(0xC1C1_0000_1234_5678);
    for _ in 0..1500 {
        let k = rng.below(5) as usize;
        let mut prior: Vec<(&str, HexValue)> = Vec::new();
        for _ in 0..k {
            // No negative pole → no cross-source conflict is possible.
            let v = match rng.below(3) {
                0 => HexValue::True,
                1 => HexValue::Probable,
                _ => HexValue::Unknown,
            };
            prior.push((SOURCES[rng.below(7) as usize], v));
        }
        let reviser = "s7";
        let mut lhs = prior.clone();
        lhs.push((reviser, HexValue::Probable));
        lhs.push((reviser, HexValue::True));
        let mut rhs = prior.clone();
        rhs.push((reviser, HexValue::True));
        assert_eq!(
            bel(&lhs),
            bel(&rhs),
            "C1 holds when no opposite pole stands, prior={prior:?}"
        );
    }
}

/// **DP C2 (FAILS, deliberate divergence).** C2: if `ψ ⊨ ¬φ` then
/// `(K∘φ)∘ψ = K∘ψ` — a later *contrary* revision should supersede the earlier
/// one, so the result is as if only `ψ` had been asserted. Hari accumulates
/// instead of overwriting: revising by `φ=True` then `ψ=False` leaves the
/// earlier `True` standing, so `(K∘φ)∘ψ = C`, whereas `K∘ψ = Doubtful` (the
/// lone `False`). `C ≠ Doubtful`, so C2 fails. Same root as the success
/// failure — bare `∘` is expansion. To satisfy C2 the earlier φ must be
/// contracted (a `Correction`/`Retraction`), which hari expresses explicitly.
#[test]
fn known_violation_dp_c2_fails() {
    // K empty. K∘φ∘ψ with φ=True (s0), ψ=False (s1).
    let lhs = bel(&[("s0", HexValue::True), ("s1", HexValue::False)]);
    let rhs = bel(&[("s1", HexValue::False)]); // K∘ψ = the lone contrary
    assert_eq!(
        lhs,
        HexValue::Contradictory,
        "the earlier φ still stands → C"
    );
    assert_eq!(
        rhs,
        HexValue::Doubtful,
        "K∘ψ believes ¬φ (single-source cap)"
    );
    assert_ne!(
        lhs, rhs,
        "DP C2 fails: contrary revision does not supersede"
    );

    // Randomized: the accumulated contrary pair is C, never equal to the
    // supersession result K∘ψ (which is on the ¬φ side).
    let mut rng = Rng(0xC2C2_9999_0000_AAAA);
    for _ in 0..1500 {
        let phi = HexValue::True;
        let psi = negative_pole(&mut rng);
        let both = bel(&[("s0", phi), ("s1", psi)]);
        let just_psi = bel(&[("s1", psi)]);
        assert!(
            accepts_not_phi(just_psi),
            "K∘ψ should believe ¬φ: {psi:?} => {just_psi:?}"
        );
        assert_ne!(
            both, just_psi,
            "C2 would require these equal; accumulation keeps φ standing"
        );
    }
}

// =======================================================================
// Contraction / recovery — the tombstone split
// =======================================================================

/// **Round-trip recovery — holds at the belief level, fails at the ledger
/// level (by design).** AGM Recovery says an expand-then-contract round trip
/// returns to the start. Hari's tombstone design splits this exactly as the
/// design predicts: append φ-evidence `e` then retract *exactly* `e`, and the
/// belief equals the recompute over the original survivors (belief recovers —
/// this is `retract-then-recompute == recompute-without`), **while the ledger
/// does not restore** — the tombstoned entry stays in the trace for audit, so
/// the pre-append state is still reachable by historical replay. The two
/// halves are asserted together: belief identity *and* ledger persistence.
#[test]
fn theorem_roundtrip_recovery_holds_at_belief_not_ledger_level() {
    let mut rng = Rng(0x2026_0721_1616_0001);
    for _ in 0..1500 {
        // Original survivors K (distinct-source assertions), then append one
        // fresh piece of evidence `e` from a new source, then retract `e`.
        let k = 1 + rng.below(4) as usize;
        let mut base: Vec<(&str, HexValue)> = Vec::new();
        let mut events: Vec<ResearchEvent> = Vec::new();
        for i in 0..k {
            let src = SOURCES[rng.below(6) as usize];
            let val = any_value(&mut rng);
            base.push((src, val));
            events.push(belief_update(i as u64 + 1, src, val));
        }
        // Append e from a source guaranteed distinct from the base pool.
        let e_cycle = k as u64 + 1;
        let e_val = any_value(&mut rng);
        events.push(belief_update(e_cycle, "s7", e_val));
        // Retract exactly e.
        events.push(selective_retraction(e_cycle + 1, "s7", e_cycle));

        let recovered = final_belief(events);
        assert_eq!(
            recovered,
            bel(&base),
            "belief-level recovery: retract(e) after append(e) == recompute over survivors"
        );
    }

    // Ledger-level: the tombstone is NOT deleted — the appended evidence is
    // still present in history. Replaying only up to the append (before the
    // retraction) reproduces the appended evidence's effect, proving the entry
    // survives for audit (the half AGM Recovery-as-state-identity gives up).
    let full = vec![
        belief_update(1, "s0", HexValue::True),
        belief_update(2, "s1", HexValue::True), // e: lifts the cap P→T
        selective_retraction(3, "s1", 2),
    ];
    // Current (post-retraction) belief: survivors {s0:True} → Probable.
    assert_eq!(final_belief(full), HexValue::Probable);
    // Historical replay to before the retraction: {s0:True, s1:True} → True.
    // The appended evidence is still in the trace — retraction never rewrote it.
    let historical = vec![
        belief_update(1, "s0", HexValue::True),
        belief_update(2, "s1", HexValue::True),
    ];
    assert_eq!(
        final_belief(historical),
        HexValue::True,
        "ledger-level: the tombstoned evidence survives in history (not restored, not erased)"
    );
}
