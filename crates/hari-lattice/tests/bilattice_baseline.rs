//! Bilattice baseline study — does hari's single-chain hexavalent order
//! lose decisions that a proper Belnap bilattice would keep?
//!
//! This is a **baseline for comparison, not a product feature**: the
//! reference bilattice lives entirely in the inline `mod bilattice`
//! below, never in `src/`. The probes run hex's
//! `combine_evidence_set` / `BeliefNetwork` propagation against the same
//! inputs mapped into a Belnap bilattice, and classify every divergence.
//!
//! Background. Belnap's FOUR distinguishes two orders that hex's single
//! chain F<D<U<P<T conflates: the TRUTH order (f ≤t {n,b} ≤t t — how
//! true) and the KNOWLEDGE order (n ≤k {f,t} ≤k b — how much is known),
//! with meet/join in each. The standard evidence-combination operator is
//! the KNOWLEDGE join ⊕ ( = ≤k join): it accumulates what every source
//! asserts, so t ⊕ f = b (one says true, one says false → BOTH), and n
//! (nothing known) is its identity. Hex instead folds evidence with the
//! TRUTH-order join (`combine_evidence`'s no-conflict branch), where
//! U (≈ n) out-ranks D (≈ weak f) — so an Unknown contribution ERASES a
//! standing Doubtful (`join(D,U)=U`). See the 2026-07-20 propagation
//! audit §4.1 ("Unknown erases Doubtful") and its
//! `known_gap_contradicts_cannot_lower_below_unknown`.
//!
//! Two embeddings of hex's 6 values are probed (both natural; see doc).
//! Embedding A is SIGN-ONLY into Belnap FOUR: P↦t, D↦f, U↦n, C↦b (T↦t,
//! F↦f); the weak/strong distinction is dropped, testing hex's SIGN
//! structure against the canonical 4-valued bilattice. Embedding B is a
//! GRADED evidence-pair (pro,con), each in {0,1,2}: the standard product
//! bilattice (chain × opposite chain), T=(2,0) P=(1,0) U=(0,0) D=(0,1)
//! F=(0,2) C=(2,2), with knowledge join ⊕ = componentwise max and
//! negation = swap; it keeps hex's 6-way resolution, testing the chain
//! against a bilattice that retains the P/D lean.
//!
//! Decision proxy (stated once): a value "accepts" iff it sits at or
//! above P on the truth side with no standing negative — hex: value ∈
//! {P,T}; FOUR: value = t; pair: pro>0 ∧ con=0. "conflict" (escalate)
//! is hex C / FOUR b / pair (pro>0 ∧ con>0). A "decision flip" is a
//! disagreement on the accept OR conflict verdict.
//!
//! Deterministic: fixed-seed xorshift, no external deps. House style —
//! `theorem_*` pins equivalences that hold on every trial;
//! `known_divergence_*` pins current divergent behavior with the
//! divergence class documented (must-flip-if-fixed convention).

use hari_lattice::{BeliefNetwork, HexLattice, HexValue, Lattice, Relation};

// ═══════════════════════════ reference bilattice ═══════════════════════════

mod bilattice {
    use hari_lattice::HexValue;

    /// A `Bilat` value: a bilattice element supporting the knowledge
    /// join ⊕, Belnap negation, an embedding from `HexValue`, and the
    /// decision predicates. Both embeddings implement this so the
    /// propagation mirror is generic over the choice.
    pub trait Bilat: Copy + PartialEq + std::fmt::Debug {
        /// Knowledge-order join (≤k join) — the standard Belnap evidence
        /// combination. Accumulates every source's assertion; the ⊕
        /// identity is "nothing known".
        fn oplus(self, other: Self) -> Self;
        /// Belnap negation: swaps the truth poles, fixes the knowledge
        /// poles (¬t=f, ¬f=t, ¬n=n, ¬b=b). Commutes with hex's `not`
        /// under both embeddings.
        fn neg(self) -> Self;
        /// The ⊕ identity ("nothing known", ≈ hex Unknown) — the seed
        /// for an empty fold and the initial value of an un-evidenced
        /// node in the propagation mirror.
        fn nothing() -> Self;
        /// Embed a hex value.
        fn embed(h: HexValue) -> Self;
        /// Accept proxy: positive truth with no standing negative.
        fn accepts(self) -> bool;
        /// Conflict proxy: simultaneous positive and negative (≈ Belnap
        /// `both`). Also gates `Implies` off (an antecedent in conflict
        /// is not "true-ish").
        fn conflict(self) -> bool;
    }

    // ---- Embedding A: canonical Belnap FOUR (sign-only) ----

    /// Belnap's four truth values. `None` = neither (⊥ knowledge),
    /// `Both` = contradiction (⊤ knowledge), `True`/`False` the poles.
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub enum Four {
        None,
        False,
        True,
        Both,
    }

    impl Bilat for Four {
        fn oplus(self, other: Self) -> Self {
            // Knowledge join = least upper bound in n ≤k {f,t} ≤k b.
            // Track "has positive assertion" / "has negative assertion".
            let pos = self.pos() || other.pos();
            let neg = self.neg_() || other.neg_();
            match (pos, neg) {
                (true, true) => Four::Both,
                (true, false) => Four::True,
                (false, true) => Four::False,
                (false, false) => Four::None,
            }
        }
        fn neg(self) -> Self {
            match self {
                Four::True => Four::False,
                Four::False => Four::True,
                Four::None => Four::None,
                Four::Both => Four::Both,
            }
        }
        fn nothing() -> Self {
            Four::None
        }
        fn embed(h: HexValue) -> Self {
            match h {
                HexValue::True | HexValue::Probable => Four::True,
                HexValue::False | HexValue::Doubtful => Four::False,
                HexValue::Unknown => Four::None,
                HexValue::Contradictory => Four::Both,
            }
        }
        fn accepts(self) -> bool {
            self == Four::True
        }
        fn conflict(self) -> bool {
            self == Four::Both
        }
    }

    impl Four {
        fn pos(self) -> bool {
            matches!(self, Four::True | Four::Both)
        }
        fn neg_(self) -> bool {
            matches!(self, Four::False | Four::Both)
        }
    }

    // ---- Embedding B: graded evidence-pair (pro, con) ----

    /// A pair of independent evidence degrees in {0,1,2} (none / weak /
    /// strong). The standard product bilattice: knowledge order is
    /// componentwise ≤ (⊕ = componentwise max), truth order is pro↑ con↓,
    /// negation swaps the components. Keeps hex's weak/strong lean.
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub struct Pc {
        pub pro: u8,
        pub con: u8,
    }

    impl Bilat for Pc {
        fn oplus(self, other: Self) -> Self {
            Pc {
                pro: self.pro.max(other.pro),
                con: self.con.max(other.con),
            }
        }
        fn neg(self) -> Self {
            Pc {
                pro: self.con,
                con: self.pro,
            }
        }
        fn nothing() -> Self {
            Pc { pro: 0, con: 0 }
        }
        fn embed(h: HexValue) -> Self {
            match h {
                HexValue::True => Pc { pro: 2, con: 0 },
                HexValue::Probable => Pc { pro: 1, con: 0 },
                HexValue::Unknown => Pc { pro: 0, con: 0 },
                HexValue::Doubtful => Pc { pro: 0, con: 1 },
                HexValue::False => Pc { pro: 0, con: 2 },
                HexValue::Contradictory => Pc { pro: 2, con: 2 },
            }
        }
        fn accepts(self) -> bool {
            self.pro > 0 && self.con == 0
        }
        fn conflict(self) -> bool {
            self.pro > 0 && self.con > 0
        }
    }
}

use bilattice::{Bilat, Four, Pc};

// ─────────────── Belnap propagation mirror (generic over embedding) ───────────────

/// A belief network over a `Bilat` value, mirroring `BeliefNetwork`'s
/// edge rules (Supports = pass source, Contradicts = negate source,
/// Implies = pass source only when it is "true-ish") but combining
/// contributions with the knowledge join ⊕ instead of
/// `combine_evidence_set`. Synchronous update, exactly like hex
/// `propagate`: read a snapshot, compute all changes, apply after.
struct BelnapNet<V: Bilat> {
    values: Vec<V>,
    edges: Vec<(usize, usize, Relation)>,
}

impl<V: Bilat> BelnapNet<V> {
    fn propagate(&mut self) -> usize {
        let mut updates: Vec<(usize, V)> = Vec::new();
        let n = self.values.len();
        for target in 0..n {
            let mut acc = self.values[target];
            let mut any = false;
            for &(from, to, rel) in &self.edges {
                if to != target {
                    continue;
                }
                let sv = self.values[from];
                let contrib = match rel {
                    Relation::Supports => sv,
                    Relation::Contradicts => sv.neg(),
                    // hex fires Implies only on a True/Probable antecedent
                    // — i.e. a pure-positive source. `accepts()` is that
                    // predicate under both embeddings.
                    Relation::Implies => {
                        if sv.accepts() {
                            sv
                        } else {
                            continue;
                        }
                    }
                };
                acc = acc.oplus(contrib);
                any = true;
            }
            if any && acc != self.values[target] {
                updates.push((target, acc));
            }
        }
        let changed = updates.len();
        for (i, v) in updates {
            self.values[i] = v;
        }
        changed
    }

    fn stabilize(&mut self, max: usize) {
        for _ in 0..max {
            if self.propagate() == 0 {
                return;
            }
        }
    }
}

// ───────────────────────── tiny deterministic RNG ─────────────────────────

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64* — same generator as algebra_probe / propagation_probe.
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

const RELATIONS: [Relation; 3] = [Relation::Supports, Relation::Contradicts, Relation::Implies];

fn rand_hex(rng: &mut Rng) -> HexValue {
    VARIANTS[rng.below(6) as usize]
}

/// hex accept proxy: chain position ≥ P (Probable or True).
fn hex_accepts(v: HexValue) -> bool {
    matches!(v, HexValue::Probable | HexValue::True)
}

/// hex conflict proxy: the Contradictory fixed point.
fn hex_conflict(v: HexValue) -> bool {
    v == HexValue::Contradictory
}

fn fold_bilat<V: Bilat>(values: &[HexValue]) -> V {
    values
        .iter()
        .fold(V::nothing(), |acc, &h| acc.oplus(V::embed(h)))
}

// ═══════════════════════════ (a) single-combination probes ═══════════════════════════

/// THEOREM S1 (pinned): at the single-combination level, hex's
/// `combine_evidence_set` and BOTH Belnap embeddings agree on the
/// DECISION — accept and conflict verdicts are identical on every input
/// — even though the retained VALUE differs (see the known divergences
/// below). Rationale: hex reports conflict iff the set holds C or both a
/// positive and a negative; ⊕ reports `both` under exactly the same
/// condition; and hex accepts iff the set holds a positive and no
/// negative and no C, which is exactly when ⊕ yields pure positive. So
/// the chain conflation costs no *single-combination* decision — the
/// cost, if any, is downstream (propagation), where an erased value
/// changes what the next round sees. 4000 random multisets.
#[test]
fn theorem_single_combination_decision_agrees() {
    let mut rng = Rng(0xB1_1A_77_1C);
    let mut info_loss = 0usize; // hex erased a negative that Belnap kept
    for trial in 0..4000 {
        let k = 1 + rng.below(8) as usize;
        let set: Vec<HexValue> = (0..k).map(|_| rand_hex(&mut rng)).collect();

        let hex = HexLattice::combine_evidence_set(set.iter().copied());
        let a: Four = fold_bilat(&set);
        let b: Pc = fold_bilat(&set);

        assert_eq!(
            hex_accepts(hex),
            a.accepts(),
            "accept verdict diverges (FOUR) on {set:?} → hex {hex:?} / {a:?} (trial {trial})"
        );
        assert_eq!(
            hex_accepts(hex),
            b.accepts(),
            "accept verdict diverges (pair) on {set:?} → hex {hex:?} / {b:?} (trial {trial})"
        );
        assert_eq!(
            hex_conflict(hex),
            a.conflict(),
            "conflict verdict diverges (FOUR) on {set:?} → hex {hex:?} / {a:?} (trial {trial})"
        );
        assert_eq!(
            hex_conflict(hex),
            b.conflict(),
            "conflict verdict diverges (pair) on {set:?} → hex {hex:?} / {b:?} (trial {trial})"
        );

        // Information-retention gap: hex collapsed to Unknown while the
        // bilattice still carries a standing negative.
        if hex == HexValue::Unknown && b.con > 0 {
            info_loss += 1;
        }
    }
    eprintln!("single-combination info-loss (hex U while pair keeps con): {info_loss}/4000");
    assert!(
        info_loss > 0,
        "expected the Unknown-erasure class to be exercised"
    );
}

/// KNOWN DIVERGENCE (pinned): Unknown erases Doubtful. The two-value set
/// {D, U} combines to hex `Unknown` — "no information" out-ranks a
/// standing negative on the single chain — whereas the knowledge join
/// KEEPS the negative: FOUR → `False`, pair → (0,1) (weak con). No
/// decision flips here (both sides are non-accept), but the retained
/// value differs, and THAT is what flips a decision one round later in
/// propagation (see `known_divergence_propagation_decision_flip_*`).
/// This is audit §4.1 made executable.
///
/// Must-flip-if-fixed: if hex adopts a knowledge-order combine so that
/// D survives U, this test's hex expectation changes.
#[test]
fn known_divergence_unknown_erases_doubtful() {
    let set = [HexValue::Doubtful, HexValue::Unknown];

    let hex = HexLattice::combine_evidence_set(set.iter().copied());
    assert_eq!(hex, HexValue::Unknown, "hex laundered the doubt through U");

    let a: Four = fold_bilat(&set);
    assert_eq!(a, Four::False, "FOUR keeps the negative: n ⊕ f = f");

    let b: Pc = fold_bilat(&set);
    assert_eq!(b, Pc { pro: 0, con: 1 }, "pair keeps the doubt at con=1");
    assert!(!b.accepts() && !b.conflict());
}

/// KNOWN DIVERGENCE (pinned): hex's truth-order join DILUTES accumulated
/// negative evidence, the graded knowledge join ACCUMULATES it. Two
/// negatives {F, D} fold to hex `Doubtful` — the *weaker* of the two,
/// because `join` takes the higher chain rank (D > F) — while the pair
/// bilattice takes componentwise max of con, (0,2) = strong False. Hex
/// is systematically optimistic on the negative side: piling on doubt
/// can only make a belief LESS false. Both are non-accept, so no
/// single-combination flip; the optimism compounds in propagation.
///
/// Must-flip-if-fixed: a knowledge-order combine would return F here.
#[test]
fn known_divergence_hex_join_dilutes_negatives() {
    let set = [HexValue::False, HexValue::Doubtful];

    let hex = HexLattice::combine_evidence_set(set.iter().copied());
    assert_eq!(
        hex,
        HexValue::Doubtful,
        "hex join keeps the WEAKER negative"
    );

    let b: Pc = fold_bilat(&set);
    assert_eq!(b, Pc { pro: 0, con: 2 }, "pair accumulates to strong con");
}

/// THEOREM S2 (pinned): hex negation and Belnap negation commute with
/// both embeddings — `embed(not(h)) == neg(embed(h))` for every hex
/// value. This is what makes the `Contradicts` edge (which contributes
/// `not(source)`) a fair comparison in the propagation mirror: negating
/// then embedding equals embedding then negating.
#[test]
fn theorem_negation_commutes_with_embedding() {
    for &h in &VARIANTS {
        let n = HexValue::not(h);
        assert_eq!(
            Four::embed(n),
            Four::embed(h).neg(),
            "FOUR neg mismatch on {h:?}"
        );
        assert_eq!(
            Pc::embed(n),
            Pc::embed(h).neg(),
            "pair neg mismatch on {h:?}"
        );
    }
}

// ═══════════════════════════ (b) propagation probes ═══════════════════════════

/// A reproducible graph spec (same shape as propagation_probe.rs), used
/// to build a hex `BeliefNetwork` and a `BelnapNet` from identical
/// inputs.
struct Spec {
    values: Vec<HexValue>,
    edges: Vec<(usize, usize, Relation)>,
}

fn rand_spec(rng: &mut Rng, n: usize, m: usize) -> Spec {
    let values = (0..n).map(|_| rand_hex(rng)).collect();
    let edges = (0..m)
        .map(|_| {
            (
                rng.below(n as u64) as usize,
                rng.below(n as u64) as usize,
                RELATIONS[rng.below(3) as usize],
            )
        })
        .collect();
    Spec { values, edges }
}

fn build_hex(spec: &Spec) -> BeliefNetwork {
    let mut net = BeliefNetwork::new();
    for (i, v) in spec.values.iter().enumerate() {
        net.add_proposition(format!("n{i}"), *v);
    }
    for &(from, to, rel) in &spec.edges {
        net.declare_relation(&format!("n{from}"), &format!("n{to}"), rel);
    }
    net
}

fn build_belnap<V: Bilat>(spec: &Spec) -> BelnapNet<V> {
    BelnapNet {
        values: spec.values.iter().map(|&v| V::embed(v)).collect(),
        edges: spec.edges.clone(),
    }
}

fn hex_fixpoint(spec: &Spec) -> Vec<HexValue> {
    let mut net = build_hex(spec);
    net.propagate_until_stable(10_000);
    (0..spec.values.len())
        .map(|i| net.get(&format!("n{i}")).unwrap().value)
        .collect()
}

/// Counts of per-node decision flips between hex and a Belnap embedding
/// at the propagation fixpoint.
#[derive(Default, Debug)]
struct FlipCounts {
    /// hex accepts, Belnap does not (usually Belnap flags conflict).
    hex_accepts_belnap_not: usize,
    /// Belnap accepts, hex does not.
    belnap_accepts_hex_not: usize,
    /// hex clean (accept/reject), Belnap conflict — the escalation hex missed.
    hex_clean_belnap_conflict: usize,
}

fn compare_propagation<V: Bilat>(rng: &mut Rng, trials: usize) -> FlipCounts {
    let mut counts = FlipCounts::default();
    for _ in 0..trials {
        let n = 2 + rng.below(10) as usize;
        let m = rng.below(28) as usize + 2;
        let spec = rand_spec(rng, n, m);

        let hex_fix = hex_fixpoint(&spec);
        let mut bel: BelnapNet<V> = build_belnap(&spec);
        bel.stabilize(10_000);

        for (&hv, &bv) in hex_fix.iter().zip(bel.values.iter()) {
            if hex_accepts(hv) && !bv.accepts() {
                counts.hex_accepts_belnap_not += 1;
            }
            if bv.accepts() && !hex_accepts(hv) {
                counts.belnap_accepts_hex_not += 1;
            }
            if !hex_conflict(hv) && bv.conflict() {
                counts.hex_clean_belnap_conflict += 1;
            }
        }
    }
    counts
}

/// KNOWN DIVERGENCE (pinned, propagation, aggregate): at the propagation
/// fixpoint hex and both Belnap embeddings flip decisions in BOTH
/// directions, but the two directions are NOT symmetric.
///
/// HISTORY: I conjectured the divergence was one-directional — hex only
/// ever OPTIMISTIC (accepts / stays clean where the bilattice
/// escalates), the single chain able to lose negative information but
/// never invent it. The randomized probe FALSIFIED it (seed 0x0D15EA5E):
/// a handful of nodes reach `Contradictory` in hex while the bilattice
/// keeps them pure-positive/`True`. Mechanism: hex's `C` is reached by
/// EAGER per-round conflict detection and is both ABSORBING and
/// CONTAGIOUS (`join(x,C)=C`, `not(C)=C`), so in a cyclic subgraph a
/// single manufactured `C` floods along Supports/Contradicts edges;
/// where an upstream sign-divergence means the bilattice never
/// accumulated `both` at the origin, its downstream node stays
/// acceptable. So hex is USUALLY optimistic and OCCASIONALLY pessimistic
/// via C-contagion — the third wrong hand-analysis in this formal track
/// caught by a probe.
///
/// The pinned, robust facts (4000 random graphs per embedding) are that
/// both directions occur (each count > 0, so the divergence is
/// bidirectional) and that missed-escalation (hex clean, bilattice
/// conflict) is the DOMINANT divergence, dwarfing the reverse
/// (hex-pessimistic) direction. The exact per-seed counts are logged,
/// not asserted, to stay robust to probe refactoring. Observed at
/// authorship: FOUR hex_accepts_belnap_not=44, belnap_accepts_hex_not=6,
/// hex_clean_belnap_conflict=118; pair 52, 6, 132.
///
/// Must-flip-if-fixed: a knowledge-order combine collapses the dominant
/// (missed-escalation) direction toward zero; this test's dominance
/// assertion would then need revisiting.
#[test]
fn known_divergence_propagation_decision_flips_both_directions() {
    let mut rng = Rng(0x0D_15_EA_5E);
    let a = compare_propagation::<Four>(&mut rng, 4000);
    let b = compare_propagation::<Pc>(&mut rng, 4000);
    eprintln!("propagation flips FOUR: {a:?}");
    eprintln!("propagation flips pair: {b:?}");

    for (label, c) in [("FOUR", &a), ("pair", &b)] {
        assert!(
            c.hex_accepts_belnap_not > 0,
            "{label}: expected hex-optimistic accept flips"
        );
        assert!(
            c.hex_clean_belnap_conflict > 0,
            "{label}: expected missed-escalation flips"
        );
        assert!(
            c.belnap_accepts_hex_not > 0,
            "{label}: expected the (rare) reverse direction — probe is a bidirectionality witness"
        );
        assert!(
            c.hex_clean_belnap_conflict > c.belnap_accepts_hex_not,
            "{label}: missed-escalation must dominate the reverse direction"
        );
    }
}

/// KNOWN DIVERGENCE (pinned, propagation): hex's single-chain
/// propagation reaches ACCEPT on a node the Belnap bilattice escalates
/// to CONFLICT — a genuine decision flip, on a realizable input class.
///
/// Construction (all `Supports`/driver edges, no withdrawal needed):
///   * `b`  starts Unknown — the target.
///   * `x`  starts Doubtful and Supports `b` — a standing negative.
///   * `u0` starts Unknown  and Supports `x` — lifts x's OWN doubt away.
///   * `y`  starts Unknown  and Supports `b`.
///   * `t0` starts True     and Supports `y` — turns y positive a round later.
///
/// Round 1 (hex): `x` = combine{D, U} = U (its doubt is laundered off);
/// `y` = combine{U, T} = T; `b` still sees x's *pre-update* D against
/// only U's → stays U. Round 2: `b` = combine{U, U(x), T(y)} = T →
/// ACCEPT. The negative x carried never coexisted with the positive in
/// `b`'s view, because hex erased it from x first.
///
/// Belnap keeps x's con (n ⊕ f = f / (0,1) stays), so when y's positive
/// lands, `b` = both/conflict → ESCALATE. Same graph, opposite decision.
///
/// Must-flip-if-fixed: adopting a knowledge-order combine (D survives U)
/// makes hex escalate `b` too, flipping this test's hex expectation.
#[test]
fn known_divergence_propagation_decision_flip_erased_doubt() {
    let spec = Spec {
        // 0=b, 1=x, 2=u0, 3=y, 4=t0
        values: vec![
            HexValue::Unknown,  // b
            HexValue::Doubtful, // x
            HexValue::Unknown,  // u0
            HexValue::Unknown,  // y
            HexValue::True,     // t0
        ],
        edges: vec![
            (1, 0, Relation::Supports), // x -> b
            (2, 1, Relation::Supports), // u0 -> x  (launders x's doubt)
            (3, 0, Relation::Supports), // y -> b
            (4, 3, Relation::Supports), // t0 -> y  (lifts y positive)
        ],
    };

    let hex_fix = hex_fixpoint(&spec);
    assert_eq!(hex_fix[0], HexValue::True, "hex accepts b");
    assert!(hex_accepts(hex_fix[0]));

    let mut a: BelnapNet<Four> = build_belnap(&spec);
    a.stabilize(10_000);
    assert_eq!(a.values[0], Four::Both, "FOUR escalates b to conflict");
    assert!(a.values[0].conflict() && !a.values[0].accepts());

    let mut b: BelnapNet<Pc> = build_belnap(&spec);
    b.stabilize(10_000);
    assert!(
        b.values[0].conflict() && !b.values[0].accepts(),
        "pair escalates b to conflict, got {:?}",
        b.values[0]
    );

    // The flip is exactly the erased-doubt mechanism: x ends Unknown in
    // hex (doubt laundered) but retains con in the bilattice.
    assert_eq!(
        hex_fix[1],
        HexValue::Unknown,
        "hex laundered x's doubt to U"
    );
    assert!(b.values[1].con > 0, "bilattice retained x's con");
}

/// KNOWN DIVERGENCE (pinned, propagation): the audit's
/// `known_gap_contradicts_cannot_lower_below_unknown` seen from the
/// bilattice. A True source that Contradicts `b` contributes NOT(T)=F,
/// but hex's `join(U,F)=U` discards it — `b` stays Unknown forever. The
/// bilattice DERIVES the negative: `b` → FOUR False / pair (0,2). Hex
/// cannot derive negative belief through propagation; the knowledge
/// join can.
///
/// Must-flip-if-fixed: if hex gains meet-down semantics for negative
/// evidence, `b` becomes Doubtful/False and this hex expectation flips
/// (mirrors the audit's own must-flip note).
#[test]
fn known_divergence_contradicts_cannot_derive_negative() {
    let spec = Spec {
        values: vec![HexValue::True, HexValue::Unknown], // a, b
        edges: vec![(0, 1, Relation::Contradicts)],
    };

    let hex_fix = hex_fixpoint(&spec);
    assert_eq!(hex_fix[1], HexValue::Unknown, "hex cannot lower b below U");

    let mut a: BelnapNet<Four> = build_belnap(&spec);
    a.stabilize(10_000);
    assert_eq!(a.values[1], Four::False, "FOUR derives b False from ¬True");

    let mut b: BelnapNet<Pc> = build_belnap(&spec);
    b.stabilize(10_000);
    assert_eq!(
        b.values[1],
        Pc { pro: 0, con: 2 },
        "pair derives strong con"
    );
}

/// THEOREM P-TERM (pinned): the Belnap propagation mirror terminates on
/// every graph (knowledge join is monotone-increasing and bounded, and
/// negation preserves the knowledge order, so the whole system is a
/// monotone map on a finite lattice). Guards the `stabilize(10_000)`
/// caps used above from silently hitting the ceiling.
#[test]
fn theorem_belnap_mirror_terminates() {
    let mut rng = Rng(0x7E_47_1A_A1);
    for trial in 0..2000 {
        let n = 2 + rng.below(10) as usize;
        let m = rng.below(28) as usize + 2;
        let spec = rand_spec(&mut rng, n, m);

        let mut a: BelnapNet<Four> = build_belnap(&spec);
        let mut rounds = 0;
        while a.propagate() != 0 {
            rounds += 1;
            assert!(
                rounds < 10_000,
                "FOUR mirror did not converge (trial {trial})"
            );
        }

        let mut b: BelnapNet<Pc> = build_belnap(&spec);
        let mut rounds_b = 0;
        while b.propagate() != 0 {
            rounds_b += 1;
            assert!(
                rounds_b < 10_000,
                "pair mirror did not converge (trial {trial})"
            );
        }
    }
}
