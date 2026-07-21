//! Randomized probe of the relation-withdrawal tombstone step (issue #16,
//! belief-revision slice).
//!
//! [`BeliefNetwork::withdraw_relation`] is the relation-side parallel of the
//! observation-side [`hari_lattice::merge_with_tombstones`]: it marks an edge
//! withdrawn without deleting it, and propagation skips it. Two documented
//! properties, each over 1000+ seeded-random trials:
//!
//!   (a) **withdraw == never-declared** — withdrawing a subset of edges
//!       *before* propagating yields the same converged node values as a
//!       network that only ever declared the surviving edges. This is what
//!       makes "a withdrawn edge contributes nothing" hold: the beliefs a
//!       withdrawn edge would have induced never appear.
//!   (b) **withdrawal order-independence** — withdrawing the same subset of
//!       edges in two different orders converges to byte-equal node values,
//!       because propagation reads a set, not a sequence.
//!
//! House style mirrors `tests/tombstone_probe.rs`: fixed-seed xorshift64*, no
//! external deps, exact equality on discrete hex values. Graphs are kept
//! acyclic (edges only point forward, `i -> j` with `i < j`) so
//! `propagate_until_stable` always converges.

use hari_lattice::{BeliefNetwork, HexValue, Relation};

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

const N_NODES: usize = 5;
const VARIANTS: [HexValue; 6] = [
    HexValue::True,
    HexValue::Probable,
    HexValue::Unknown,
    HexValue::Doubtful,
    HexValue::False,
    HexValue::Contradictory,
];
const RELATIONS: [Relation; 3] = [Relation::Supports, Relation::Contradicts, Relation::Implies];

fn label(i: usize) -> String {
    format!("n{i}")
}

/// One forward edge `from -relation-> to` with `from < to`.
#[derive(Clone, Copy)]
struct Edge {
    from: usize,
    to: usize,
    relation: Relation,
}

/// Seed base node values (positions matter; every node exists so an
/// isolated node still carries a base value) and return them.
fn seed_nodes(net: &mut BeliefNetwork, base: &[HexValue]) {
    for (i, v) in base.iter().enumerate() {
        net.add_proposition(label(i), *v);
    }
}

fn final_values(net: &BeliefNetwork) -> Vec<HexValue> {
    (0..N_NODES)
        .map(|i| net.get(&label(i)).expect("node exists").value)
        .collect()
}

/// (a) Withdrawing a subset of edges before propagation is byte-equal to a
/// network that only declared the surviving edges.
#[test]
fn withdraw_equals_never_declared() {
    let mut rng = Rng(0x0DDBA11C0FFEE777);
    for _ in 0..1500 {
        // Random base values.
        let base: Vec<HexValue> = (0..N_NODES)
            .map(|_| VARIANTS[rng.below(6) as usize])
            .collect();

        // Random forward edges; each included edge is independently marked
        // for withdrawal or not.
        let mut edges: Vec<Edge> = Vec::new();
        let mut withdrawn_flags: Vec<bool> = Vec::new();
        for i in 0..N_NODES {
            for j in (i + 1)..N_NODES {
                if rng.below(2) == 0 {
                    edges.push(Edge {
                        from: i,
                        to: j,
                        relation: RELATIONS[rng.below(3) as usize],
                    });
                    withdrawn_flags.push(rng.below(2) == 0);
                }
            }
        }

        // Network A: declare every edge, then withdraw the flagged subset.
        let mut with_withdrawn = BeliefNetwork::new();
        seed_nodes(&mut with_withdrawn, &base);
        for e in &edges {
            with_withdrawn.declare_relation(&label(e.from), &label(e.to), e.relation);
        }
        for (e, &flag) in edges.iter().zip(withdrawn_flags.iter()) {
            if flag {
                assert!(
                    with_withdrawn.withdraw_relation(&label(e.from), &label(e.to), e.relation),
                    "a declared edge must be withdrawable"
                );
            }
        }
        with_withdrawn.propagate_until_stable(20);

        // Network B: declare only the surviving edges.
        let mut never_declared = BeliefNetwork::new();
        seed_nodes(&mut never_declared, &base);
        for (e, &flag) in edges.iter().zip(withdrawn_flags.iter()) {
            if !flag {
                never_declared.declare_relation(&label(e.from), &label(e.to), e.relation);
            }
        }
        never_declared.propagate_until_stable(20);

        assert_eq!(
            final_values(&with_withdrawn),
            final_values(&never_declared),
            "withdraw-before-propagate must equal never-declared; base={base:?}"
        );
    }
}

/// (b) Withdrawing the same subset of edges in two different orders
/// converges to byte-equal node values — propagation depends only on which
/// edges are withdrawn, not the order they were withdrawn in.
#[test]
fn withdrawal_order_independent() {
    let mut rng = Rng(0x5EEDCA57C0DE1234);
    for _ in 0..1500 {
        let base: Vec<HexValue> = (0..N_NODES)
            .map(|_| VARIANTS[rng.below(6) as usize])
            .collect();

        let mut edges: Vec<Edge> = Vec::new();
        let mut withdrawn_flags: Vec<bool> = Vec::new();
        for i in 0..N_NODES {
            for j in (i + 1)..N_NODES {
                if rng.below(2) == 0 {
                    edges.push(Edge {
                        from: i,
                        to: j,
                        relation: RELATIONS[rng.below(3) as usize],
                    });
                    withdrawn_flags.push(rng.below(2) == 0);
                }
            }
        }

        let to_withdraw: Vec<Edge> = edges
            .iter()
            .zip(withdrawn_flags.iter())
            .filter(|(_, &f)| f)
            .map(|(e, _)| *e)
            .collect();

        let build = || {
            let mut net = BeliefNetwork::new();
            seed_nodes(&mut net, &base);
            for e in &edges {
                net.declare_relation(&label(e.from), &label(e.to), e.relation);
            }
            net
        };

        // Forward order.
        let mut a = build();
        for e in to_withdraw.iter() {
            a.withdraw_relation(&label(e.from), &label(e.to), e.relation);
        }
        a.propagate_until_stable(20);

        // Reverse order.
        let mut b = build();
        for e in to_withdraw.iter().rev() {
            b.withdraw_relation(&label(e.from), &label(e.to), e.relation);
        }
        b.propagate_until_stable(20);

        assert_eq!(
            final_values(&a),
            final_values(&b),
            "withdrawal order must not affect the converged state; base={base:?}"
        );
    }
}
