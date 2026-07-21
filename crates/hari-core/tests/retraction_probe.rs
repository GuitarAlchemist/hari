//! Randomized probe of the belief-revision recompute doctrine (issue #16,
//! design §9 item 2 / open question 4).
//!
//! The design argues from the merge theorems that retraction is
//! evidence-recompute-authoritative but flags that the claim "is argued
//! from the merge theorems but not itself probed — item 2 of §9 must add
//! the probe before the doctrine is load-bearing." This is that probe,
//! placed at the `hari-core` boundary because that is where retraction
//! filtering happens (the ledger + `recompute_from_ledger` live on
//! `CognitiveLoop`, not in the leaf lattice crate).
//!
//! Two documented properties, over 1500 seeded-random trials each:
//!
//!   (a) **retract-then-recompute == recompute-without** — after a
//!       selective retraction the belief equals a from-scratch
//!       `combine_evidence_set` over the *surviving* evidence, i.e. exactly
//!       what you would get had the retracted evidence never been submitted.
//!   (b) **retraction commutes with merge** — the recomputed belief depends
//!       only on the surviving evidence multiset, not on the order the
//!       evidence arrived nor the order the retractions were issued.
//!
//! Scope: evidence is submitted before the (terminal) retractions, matching
//! the semantic the design claims — a selective retraction recomputes from
//! survivors. Evidence folded in *after* a retraction is new live evidence,
//! a different question outside this invariant.
//!
//! Deterministic: fixed-seed xorshift64*, no external deps, hex values are
//! already discrete so equality is exact.

use hari_core::{
    CognitiveLoop, Evidence, ResearchEvent, ResearchEventPayload, ResearchTrace, RetractionSelector,
};
use hari_lattice::{HexLattice, HexValue};

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

const SOURCES: [&str; 4] = ["evaluator", "critic", "runner", "auditor"];
const VARIANTS: [HexValue; 6] = [
    HexValue::True,
    HexValue::Probable,
    HexValue::Unknown,
    HexValue::Doubtful,
    HexValue::False,
    HexValue::Contradictory,
];

const PROP: &str = "claim-under-revision";

/// One generated piece of evidence with a distinct `(source, cycle)` key.
#[derive(Clone)]
struct Ev {
    source: String,
    cycle: u64,
    value: HexValue,
    retracted: bool,
}

fn belief_update(ev: &Ev) -> ResearchEvent {
    ResearchEvent {
        cycle: ev.cycle,
        source: ev.source.clone(),
        payload: ResearchEventPayload::BeliefUpdate {
            proposition: PROP.to_string(),
            value: ev.value,
            evidence: Evidence::new(),
        },
    }
}

fn retraction(ev: &Ev, at_cycle: u64) -> ResearchEvent {
    ResearchEvent {
        cycle: at_cycle,
        source: ev.source.clone(),
        payload: ResearchEventPayload::Retraction {
            proposition: PROP.to_string(),
            reason: "probe".to_string(),
            // Target the exact evidence by its (source, cycle) key.
            retracts: Some(RetractionSelector {
                source: Some(ev.source.clone()),
                cycle: Some(ev.cycle),
            }),
        },
    }
}

fn shuffle<T>(rng: &mut Rng, v: &mut [T]) {
    for i in (1..v.len()).rev() {
        let j = rng.below((i + 1) as u64) as usize;
        v.swap(i, j);
    }
}

/// Drive one loop over a full trace: all belief_updates (in `update_order`)
/// then all retractions (in `retract_order`), and return the proposition's
/// final belief value from the replay report.
fn run(evs: &[Ev], update_order: &[usize], retract_order: &[usize]) -> HexValue {
    let mut events = Vec::new();
    for &i in update_order {
        events.push(belief_update(&evs[i]));
    }
    let mut stamp = evs.len() as u64 + 1;
    for &i in retract_order {
        if evs[i].retracted {
            events.push(retraction(&evs[i], stamp));
            stamp += 1;
        }
    }
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

#[test]
fn retract_then_recompute_equals_recompute_without() {
    let mut rng = Rng(0x9E3779B97F4A7C15);
    let mut trials = 0;
    for _ in 0..1500 {
        let n = 1 + rng.below(6) as usize; // 1..=6 evidence items
        let mut evs: Vec<Ev> = Vec::new();
        for k in 0..n {
            let mut retracted = rng.below(2) == 0;
            // Guarantee at least one retraction so the recompute path fires.
            if k == n - 1 && !evs.iter().any(|e| e.retracted) {
                retracted = true;
            }
            evs.push(Ev {
                source: SOURCES[rng.below(4) as usize].to_string(),
                cycle: (k as u64) + 1,
                value: VARIANTS[rng.below(6) as usize],
                retracted,
            });
        }

        let expected =
            HexLattice::combine_evidence_set(evs.iter().filter(|e| !e.retracted).map(|e| e.value));

        let update_order: Vec<usize> = (0..n).collect();
        let retract_order: Vec<usize> = (0..n).collect();
        let actual = run(&evs, &update_order, &retract_order);

        assert_eq!(
            actual, expected,
            "retract-then-recompute must equal recompute over survivors {evs:?}",
        );
        trials += 1;
    }
    assert!(trials >= 1500);
}

#[test]
fn retraction_commutes_with_evidence_order() {
    let mut rng = Rng(0xD1B54A32D192ED03);
    for _ in 0..1500 {
        let n = 1 + rng.below(6) as usize;
        let mut evs: Vec<Ev> = Vec::new();
        for k in 0..n {
            let mut retracted = rng.below(2) == 0;
            if k == n - 1 && !evs.iter().any(|e| e.retracted) {
                retracted = true;
            }
            evs.push(Ev {
                source: SOURCES[rng.below(4) as usize].to_string(),
                cycle: (k as u64) + 1,
                value: VARIANTS[rng.below(6) as usize],
                retracted,
            });
        }

        // Two independent orderings of the SAME evidence + retraction sets.
        let mut order_a: Vec<usize> = (0..n).collect();
        let mut order_b: Vec<usize> = (0..n).collect();
        let mut ret_a: Vec<usize> = (0..n).collect();
        let mut ret_b: Vec<usize> = (0..n).collect();
        shuffle(&mut rng, &mut order_a);
        shuffle(&mut rng, &mut order_b);
        shuffle(&mut rng, &mut ret_a);
        shuffle(&mut rng, &mut ret_b);

        let va = run(&evs, &order_a, &ret_a);
        let vb = run(&evs, &order_b, &ret_b);
        assert_eq!(
            va, vb,
            "recomputed belief must not depend on evidence/retraction order {evs:?}",
        );

        // And both equal the order-free survivor recompute.
        let expected =
            HexLattice::combine_evidence_set(evs.iter().filter(|e| !e.retracted).map(|e| e.value));
        assert_eq!(va, expected);
    }
}

impl std::fmt::Debug for Ev {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}@{}={:?}{}",
            self.source,
            self.cycle,
            self.value,
            if self.retracted { "[x]" } else { "" }
        )
    }
}
