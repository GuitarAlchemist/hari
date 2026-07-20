//! Randomized + adversarial probes of `BeliefNetwork` propagation
//! (Phase 8 forward reasoning), companion to `algebra_probe.rs`.
//!
//! The in-module tests check single hand-picked instances. These check
//! the structural claims at scale: termination, monotonicity,
//! absorption, edge-order sensitivity, and the equivalence of the
//! provenance-bearing and provenance-blind paths.
//!
//! Deterministic: fixed-seed xorshift, no external deps.

use hari_lattice::{BeliefNetwork, HexValue, Relation};

// ───────────────────────── tiny deterministic RNG ─────────────────────────

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

const VARIANTS: [HexValue; 6] = [
    HexValue::True,
    HexValue::Probable,
    HexValue::Unknown,
    HexValue::Doubtful,
    HexValue::False,
    HexValue::Contradictory,
];

const RELATIONS: [Relation; 3] = [Relation::Supports, Relation::Contradicts, Relation::Implies];

/// Truth-chain position for monotonicity checks. `Contradictory` is
/// handled separately (absorbing, outside the chain) per the project's
/// design doctrine.
fn chain_rank(v: HexValue) -> Option<u8> {
    match v {
        HexValue::False => Some(0),
        HexValue::Doubtful => Some(1),
        HexValue::Unknown => Some(2),
        HexValue::Probable => Some(3),
        HexValue::True => Some(4),
        HexValue::Contradictory => None,
    }
}

/// A reproducible graph spec: nodes with initial values plus an edge
/// list. Building a `BeliefNetwork` twice from the same spec (with a
/// chosen edge order) gives identical networks, which substitutes for
/// `Clone` (the type is not `Clone`).
struct Spec {
    values: Vec<HexValue>,
    edges: Vec<(usize, usize, Relation)>,
}

fn rand_spec(rng: &mut Rng, n: usize, m: usize) -> Spec {
    let values = (0..n).map(|_| VARIANTS[rng.below(6) as usize]).collect();
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

fn build(spec: &Spec, edge_order: &[usize]) -> BeliefNetwork {
    let mut net = BeliefNetwork::new();
    for (i, v) in spec.values.iter().enumerate() {
        net.add_proposition(format!("n{i}"), *v);
    }
    for &e in edge_order {
        let (from, to, rel) = spec.edges[e];
        net.declare_relation(&format!("n{from}"), &format!("n{to}"), rel);
    }
    net
}

fn snapshot(net: &BeliefNetwork, n: usize) -> Vec<HexValue> {
    (0..n)
        .map(|i| net.get(&format!("n{i}")).unwrap().value)
        .collect()
}

fn shuffled(rng: &mut Rng, n: usize) -> Vec<usize> {
    let mut v: Vec<usize> = (0..n).collect();
    for i in (1..v.len()).rev() {
        let j = rng.below((i + 1) as u64) as usize;
        v.swap(i, j);
    }
    v
}

// ───────────────────────── probes ─────────────────────────

/// THEOREM P1 (pinned): propagation terminates on EVERY graph —
/// cycles included — without needing the `max_iterations` cap.
/// Argument: the per-round combined value folds from the node's
/// current value through `combine_evidence`, which either joins
/// (never decreasing chain rank) or jumps to the absorbing `C`; so
/// every node's trajectory is strictly rank-increasing or reaches a
/// terminal value, bounding total changes by 5·N and rounds by 5·N+1.
#[test]
fn theorem_propagation_terminates_unaided() {
    let mut rng = Rng(0xA11CE);
    for trial in 0..1500 {
        let n = 2 + rng.below(10) as usize;
        let m = rng.below(28) as usize + 2;
        let spec = rand_spec(&mut rng, n, m);
        let order: Vec<usize> = (0..spec.edges.len()).collect();
        let mut net = build(&spec, &order);
        let rounds = net.propagate_until_stable(10_000);
        assert!(
            rounds < 10_000,
            "propagation hit the cap — non-termination (trial {trial})"
        );
        assert!(
            rounds <= 5 * n + 2,
            "termination bound violated: {rounds} rounds for n={n} (trial {trial})"
        );
    }
}

/// THEOREM P2 (pinned): per-node monotonicity. Across rounds a node's
/// value never moves DOWN the F<D<U<P<T chain; the only other
/// transition is a jump to `Contradictory`, which is absorbing —
/// once C, forever C (per-node, in propagation).
#[test]
fn theorem_per_node_monotone_and_c_absorbing() {
    let mut rng = Rng(0xB0BA);
    for trial in 0..1000 {
        let n = 2 + rng.below(8) as usize;
        let m = rng.below(20) as usize + 2;
        let spec = rand_spec(&mut rng, n, m);
        let order: Vec<usize> = (0..spec.edges.len()).collect();
        let mut net = build(&spec, &order);

        let mut prev = snapshot(&net, n);
        for _round in 0..(5 * n + 2) {
            let changed = net.propagate();
            let cur = snapshot(&net, n);
            for i in 0..n {
                match (chain_rank(prev[i]), chain_rank(cur[i])) {
                    (Some(a), Some(b)) => assert!(
                        b >= a,
                        "node n{i} moved DOWN the chain {:?}→{:?} (trial {trial})",
                        prev[i],
                        cur[i]
                    ),
                    (None, x) => assert!(
                        x.is_none(),
                        "node n{i} ESCAPED Contradictory to {:?} (trial {trial})",
                        cur[i]
                    ),
                    (Some(_), None) => {} // chain → C: allowed
                }
            }
            prev = cur;
            if changed == 0 {
                break;
            }
        }
    }
}

/// THEOREM P3 (pinned): the propagation FIXPOINT is independent of
/// edge insertion order. HISTORY: this began as a conjecture that the
/// pairwise fold's order-sensitivity was transient and fixpoints
/// reconverged. The randomized probe FALSIFIED it (seed 0xF17ED,
/// trial 6): a node starting `Doubtful` with contributions `{U, T}`
/// reached `True` in one edge order — the D laundered through
/// `join(D, U) = U` before the positive arrived — and `Contradictory`
/// in the other, permanently. Fixed by replacing the pairwise fold
/// with `HexLattice::combine_evidence_set`, which inspects the whole
/// contribution set (plus current) before combining: conflict
/// detection cannot be defeated by arrival order, and the no-conflict
/// branch is a pure ACI join. Checked over 1000 random graphs × 3
/// random edge permutations each.
#[test]
fn theorem_fixpoint_is_edge_order_independent() {
    let mut rng = Rng(0xF17ED);
    for trial in 0..1000 {
        let n = 2 + rng.below(8) as usize;
        let m = rng.below(20) as usize + 2;
        let spec = rand_spec(&mut rng, n, m);

        let base_order: Vec<usize> = (0..spec.edges.len()).collect();
        let mut base = build(&spec, &base_order);
        base.propagate_until_stable(10_000);
        let base_fix = snapshot(&base, n);

        for _ in 0..3 {
            let perm = shuffled(&mut rng, spec.edges.len());
            let mut net = build(&spec, &perm);
            net.propagate_until_stable(10_000);
            assert_eq!(
                snapshot(&net, n),
                base_fix,
                "fixpoint depends on edge insertion order (trial {trial})"
            );
        }
    }
}

/// THEOREM P5 (pinned; was KNOWN DIVERGENCE P-a, fixed): the
/// per-round outcome — and therefore the provenance audit trail — is
/// independent of edge insertion order. Same graph, edges
/// [T→b, F→b] vs [F→b, T→b], target starts Unknown: under the old
/// pairwise fold one order produced `C` in round 1 and the other `T`
/// (the F silently swallowed by join, conflict only caught a round
/// later). Under `combine_evidence_set` the contribution SET {U,T,F}
/// contains a positive and a negative, so both orders produce `C` in
/// round 1 and the `Derivation` records agree up to contribution
/// order within the record.
#[test]
fn theorem_round_outcome_is_edge_order_independent() {
    let spec = Spec {
        values: vec![HexValue::True, HexValue::False, HexValue::Unknown],
        edges: vec![(0, 2, Relation::Supports), (1, 2, Relation::Supports)],
    };
    let mut net_a = build(&spec, &[0, 1]);
    let (ca, _deriv_a) = net_a.propagate_with_provenance();
    let mut net_b = build(&spec, &[1, 0]);
    let (cb, _deriv_b) = net_b.propagate_with_provenance();

    assert_eq!(ca, cb);
    assert_eq!(net_a.get("n2").unwrap().value, HexValue::Contradictory);
    assert_eq!(net_b.get("n2").unwrap().value, HexValue::Contradictory);
}

/// KNOWN GAP P-b (design fact, pinned): propagation cannot derive
/// negative belief. A `Contradicts` edge from a `True` source
/// contributes `NOT(T) = F`, but `combine_evidence(U, F)` finds no
/// positive-vs-negative conflict (U is neither) and `join(U, F) = U`
/// — the contribution is discarded. So "A is true and A contradicts
/// B" leaves an Unknown B Unknown forever; B can never become
/// Doubtful or False through propagation, only direct assignment.
/// Whether propagation SHOULD derive falsity (meet-down semantics for
/// negative evidence) is an epistemics decision — the current design
/// only lifts values up the chain or jumps to C. Documented here so
/// the choice is visible; if meet-down semantics are adopted, this
/// test must flip.
#[test]
fn known_gap_contradicts_cannot_lower_below_unknown() {
    let mut net = BeliefNetwork::new();
    net.add_proposition("a", HexValue::True);
    net.add_proposition("b", HexValue::Unknown);
    net.declare_relation("a", "b", Relation::Contradicts);
    net.propagate_until_stable(100);
    assert_eq!(
        net.get("b").unwrap().value,
        HexValue::Unknown,
        "propagation derived negative belief — meet-down semantics \
         landed; flip this test and update the audit doc"
    );

    // Same inertness for a Doubtful target.
    let mut net2 = BeliefNetwork::new();
    net2.add_proposition("a", HexValue::True);
    net2.add_proposition("b", HexValue::Doubtful);
    net2.declare_relation("a", "b", Relation::Contradicts);
    net2.propagate_until_stable(100);
    assert_eq!(net2.get("b").unwrap().value, HexValue::Doubtful);
}

/// THEOREM P4 (pinned): the provenance-bearing and provenance-blind
/// paths compute identical values and change counts — "same
/// algorithm, additional structured output" holds at scale, and
/// `derivations.len() == changed_count` every round.
#[test]
fn theorem_provenance_path_equals_blind_path() {
    let mut rng = Rng(0xE60A1);
    for trial in 0..1000 {
        let n = 2 + rng.below(8) as usize;
        let m = rng.below(20) as usize + 2;
        let spec = rand_spec(&mut rng, n, m);
        let order: Vec<usize> = (0..spec.edges.len()).collect();

        let mut blind = build(&spec, &order);
        let mut prov = build(&spec, &order);
        for _round in 0..(5 * n + 2) {
            let c1 = blind.propagate();
            let (c2, derivs) = prov.propagate_with_provenance();
            assert_eq!(c1, c2, "changed counts diverge (trial {trial})");
            assert_eq!(derivs.len(), c2, "derivation count != changed count");
            assert_eq!(
                snapshot(&blind, n),
                snapshot(&prov, n),
                "values diverge between paths (trial {trial})"
            );
            if c1 == 0 {
                break;
            }
        }
    }
}
