//! # hari-lattice — Hexavalent Lattice Logic Engine
//!
//! Implements a 6-valued logic system for AGI belief representation.
//! Extends Demerzel's tetravalent logic (T/F/U/C) with two additional values:
//!
//! - **T** (True) — verified with evidence
//! - **P** (Probable) — likely true, evidence is suggestive but not conclusive
//! - **U** (Unknown) — insufficient evidence in either direction
//! - **D** (Doubtful) — likely false, evidence suggests negation
//! - **F** (False) — refuted with evidence
//! - **C** (Contradictory) — conflicting evidence from multiple sources
//!
//! The lattice ordering is: F < D < U < P < T, with C as a fixed point
//! representing irreconcilable conflict. This forms a bounded lattice with
//! join (least upper bound) and meet (greatest lower bound) operations.
//!
//! The `BeliefNetwork` builds on this to create a graph of propositions
//! connected by logical relations, with belief propagation through the network.

use petgraph::graph::{DiGraph, NodeIndex};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

// ---------------------------------------------------------------------------
// HexValue — the 6-valued logic type
// ---------------------------------------------------------------------------

/// A value in the hexavalent lattice.
///
/// The ordering F < D < U < P < T forms a chain, with C (Contradictory)
/// sitting outside the main chain — it absorbs other values in certain
/// operations, representing irreconcilable evidence conflict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HexValue {
    /// Verified true with strong evidence
    True,
    /// Probably true — suggestive but not conclusive evidence
    Probable,
    /// Unknown — insufficient evidence either way
    Unknown,
    /// Doubtful — evidence leans toward false
    Doubtful,
    /// Verified false with strong evidence
    False,
    /// Contradictory — irreconcilable conflicting evidence
    Contradictory,
}

impl HexValue {
    /// Numeric rank on the truth chain (F=0, D=1, U=2, P=3, T=4, C=5).
    /// C gets rank 5 but is treated specially in lattice operations.
    fn rank(self) -> u8 {
        match self {
            Self::False => 0,
            Self::Doubtful => 1,
            Self::Unknown => 2,
            Self::Probable => 3,
            Self::True => 4,
            Self::Contradictory => 5,
        }
    }

    /// Convert a rank back to a HexValue.
    fn from_rank(r: u8) -> Self {
        match r {
            0 => Self::False,
            1 => Self::Doubtful,
            2 => Self::Unknown,
            3 => Self::Probable,
            4 => Self::True,
            _ => Self::Contradictory,
        }
    }

    /// Returns true if this value is on the "truth chain" (not Contradictory).
    pub fn is_definite(self) -> bool {
        self != Self::Contradictory
    }

    /// Confidence score in [0.0, 1.0]. Maps the truth chain to a gradient.
    /// Contradictory maps to 0.5 (maximum uncertainty).
    pub fn confidence(self) -> f64 {
        match self {
            Self::True => 1.0,
            Self::Probable => 0.75,
            Self::Unknown => 0.5,
            Self::Doubtful => 0.25,
            Self::False => 0.0,
            Self::Contradictory => 0.5,
        }
    }
}

impl fmt::Display for HexValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::True => write!(f, "T"),
            Self::Probable => write!(f, "P"),
            Self::Unknown => write!(f, "U"),
            Self::Doubtful => write!(f, "D"),
            Self::False => write!(f, "F"),
            Self::Contradictory => write!(f, "C"),
        }
    }
}

// ---------------------------------------------------------------------------
// Lattice trait and HexLattice implementation
// ---------------------------------------------------------------------------

/// A bounded lattice with join, meet, and complement operations.
pub trait Lattice {
    /// Least upper bound (disjunction / OR-like).
    fn join(a: Self, b: Self) -> Self;
    /// Greatest lower bound (conjunction / AND-like).
    fn meet(a: Self, b: Self) -> Self;
    /// Logical complement (negation).
    fn not(a: Self) -> Self;
    /// Bottom element (least element).
    fn bottom() -> Self;
    /// Top element (greatest element).
    fn top() -> Self;
}

/// The hexavalent lattice implementation.
///
/// ## Truth tables
///
/// **Join** (optimistic merge — takes the more-true value):
/// - If either operand is C, result is C (contradiction propagates)
/// - Otherwise, the higher-ranked value wins
///
/// **Meet** (pessimistic merge — takes the more-false value):
/// - If either operand is C, result is C (contradiction propagates)
/// - Otherwise, the lower-ranked value wins
///
/// **Not** (complement):
/// - T <-> F, P <-> D, U <-> U, C <-> C
pub struct HexLattice;

impl Lattice for HexValue {
    fn join(a: Self, b: Self) -> Self {
        // Contradictory absorbs everything in join
        if a == Self::Contradictory || b == Self::Contradictory {
            return Self::Contradictory;
        }
        // Otherwise take the more-true value
        if a.rank() >= b.rank() {
            a
        } else {
            b
        }
    }

    fn meet(a: Self, b: Self) -> Self {
        // Contradictory absorbs everything in meet
        if a == Self::Contradictory || b == Self::Contradictory {
            return Self::Contradictory;
        }
        // Otherwise take the more-false value
        if a.rank() <= b.rank() {
            a
        } else {
            b
        }
    }

    fn not(a: Self) -> Self {
        match a {
            Self::True => Self::False,
            Self::Probable => Self::Doubtful,
            Self::Unknown => Self::Unknown,
            Self::Doubtful => Self::Probable,
            Self::False => Self::True,
            Self::Contradictory => Self::Contradictory,
        }
    }

    fn bottom() -> Self {
        Self::False
    }

    fn top() -> Self {
        Self::True
    }
}

impl HexLattice {
    /// Combine two evidence streams. If they strongly disagree (one T-ish, one F-ish),
    /// the result is Contradictory. Otherwise delegates to join.
    pub fn combine_evidence(a: HexValue, b: HexValue) -> HexValue {
        // Detect contradiction: one side believes true-ish, other false-ish
        let a_positive = matches!(a, HexValue::True | HexValue::Probable);
        let b_positive = matches!(b, HexValue::True | HexValue::Probable);
        let a_negative = matches!(a, HexValue::False | HexValue::Doubtful);
        let b_negative = matches!(b, HexValue::False | HexValue::Doubtful);

        if (a_positive && b_negative) || (a_negative && b_positive) {
            HexValue::Contradictory
        } else {
            HexValue::join(a, b)
        }
    }
}

// ---------------------------------------------------------------------------
// BeliefNetwork — graph of propositions with hexavalent truth values
// ---------------------------------------------------------------------------

/// The type of logical relation between two propositions in the belief network.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Relation {
    /// A supports B (evidence for A is evidence for B)
    Supports,
    /// A contradicts B (evidence for A is evidence against B)
    Contradicts,
    /// A implies B (if A is true, B must be true)
    Implies,
}

/// One edge's contribution to a single derived value, captured during
/// provenance-bearing propagation.
///
/// `contributed_value` is the *value the edge fed into
/// `combine_evidence`*, which is the source value for `Supports`,
/// `NOT(source)` for `Contradicts`, and the source value for `Implies`
/// — the latter only present when the antecedent was True/Probable
/// (silent implications are not recorded).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Contribution {
    pub source: String,
    pub source_value: HexValue,
    pub relation: Relation,
    pub contributed_value: HexValue,
}

/// A single belief change produced by a propagation round, with the
/// edge contributions that combined into the new value. The vehicle
/// for forward-reasoning provenance: an IX consumer can read a
/// `Vec<Derivation>` and reconstruct the audit chain that led to any
/// derived belief.
///
/// `round` is 1-indexed: round 1 = changes derived directly from
/// already-set values; round 2 = changes derived from round-1
/// derivations; etc. Multi-hop chains read as monotonically
/// increasing rounds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Derivation {
    pub proposition: String,
    pub previous_value: HexValue,
    pub new_value: HexValue,
    pub contributions: Vec<Contribution>,
    pub round: usize,
}

/// A proposition node in the belief network.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proposition {
    /// Human-readable label for this proposition
    pub label: String,
    /// Current hexavalent truth value
    pub value: HexValue,
    /// How much evidence backs this value (0.0 = none, 1.0 = overwhelming)
    pub evidence_weight: f64,
}

/// A directed graph of propositions connected by logical relations.
///
/// Each node holds a `Proposition` with a hexavalent truth value.
/// Edges represent logical relations (supports, contradicts, implies).
/// Belief propagation flows through edges to update downstream beliefs.
pub struct BeliefNetwork {
    graph: DiGraph<Proposition, Relation>,
    index: HashMap<String, NodeIndex>,
}

impl BeliefNetwork {
    /// Create an empty belief network.
    pub fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            index: HashMap::new(),
        }
    }

    /// Add a proposition to the network. Returns its node index.
    pub fn add_proposition(&mut self, label: impl Into<String>, value: HexValue) -> NodeIndex {
        let label = label.into();
        let idx = self.graph.add_node(Proposition {
            label: label.clone(),
            value,
            evidence_weight: 0.5,
        });
        self.index.insert(label, idx);
        idx
    }

    /// Connect two propositions with a logical relation.
    pub fn add_relation(&mut self, from: NodeIndex, to: NodeIndex, relation: Relation) {
        self.graph.add_edge(from, to, relation);
    }

    /// Label-based variant of [`Self::add_relation`].
    ///
    /// Either or both endpoints are created with `HexValue::Unknown` if
    /// they don't already exist — so callers can declare relations
    /// before any belief landed on the propositions involved. This is
    /// the entry point used by `hari-core`'s `RelationDeclaration`
    /// research event, where IX may declare a relation involving a
    /// proposition that has not yet received an `experiment_result` or
    /// `belief_update`.
    ///
    /// Returns `(from_index, to_index)` so callers can chain further
    /// graph operations without re-looking-up the labels.
    pub fn declare_relation(
        &mut self,
        from_label: &str,
        to_label: &str,
        relation: Relation,
    ) -> (NodeIndex, NodeIndex) {
        let from_idx = match self.index.get(from_label) {
            Some(&idx) => idx,
            None => self.add_proposition(from_label, HexValue::Unknown),
        };
        let to_idx = match self.index.get(to_label) {
            Some(&idx) => idx,
            None => self.add_proposition(to_label, HexValue::Unknown),
        };
        self.graph.add_edge(from_idx, to_idx, relation);
        (from_idx, to_idx)
    }

    /// Look up a proposition by label.
    pub fn get(&self, label: &str) -> Option<&Proposition> {
        self.index.get(label).map(|&idx| &self.graph[idx])
    }

    /// Get a mutable reference to a proposition by label.
    pub fn get_mut(&mut self, label: &str) -> Option<&mut Proposition> {
        let idx = *self.index.get(label)?;
        Some(&mut self.graph[idx])
    }

    /// Number of propositions in the network.
    pub fn len(&self) -> usize {
        self.graph.node_count()
    }

    /// Whether the network is empty.
    pub fn is_empty(&self) -> bool {
        self.graph.node_count() == 0
    }

    /// Run one round of belief propagation through the network.
    ///
    /// For each edge (A -> B):
    /// - Supports: B's value is joined with A's value
    /// - Contradicts: B's value is combined with NOT(A's value)
    /// - Implies: if A is True/Probable, B is joined with A's value
    ///
    /// Returns the number of nodes whose values changed.
    ///
    /// This is the trust-blind / provenance-blind path; if you need a
    /// per-derivation audit trail call [`Self::propagate_with_provenance`]
    /// instead — same algorithm, additional structured output.
    pub fn propagate(&mut self) -> usize {
        // Collect updates first to avoid borrow issues
        let mut updates: Vec<(NodeIndex, HexValue)> = Vec::new();

        let node_indices: Vec<NodeIndex> = self.graph.node_indices().collect();
        for &target in &node_indices {
            let mut incoming_values: Vec<HexValue> = Vec::new();

            // Walk all edges pointing to this node
            let edge_indices: Vec<_> = self.graph.edge_indices().collect();
            for edge_idx in edge_indices {
                let (src, tgt) = self.graph.edge_endpoints(edge_idx).unwrap();
                if tgt != target {
                    continue;
                }
                let source_value = self.graph[src].value;
                let relation = *self.graph.edge_weight(edge_idx).unwrap();

                let contribution = match relation {
                    Relation::Supports => source_value,
                    Relation::Contradicts => HexValue::not(source_value),
                    Relation::Implies => {
                        if matches!(source_value, HexValue::True | HexValue::Probable) {
                            source_value
                        } else {
                            continue; // Implication only fires when antecedent is true-ish
                        }
                    }
                };
                incoming_values.push(contribution);
            }

            if !incoming_values.is_empty() {
                let current = self.graph[target].value;
                let combined = incoming_values
                    .into_iter()
                    .fold(current, HexLattice::combine_evidence);
                if combined != current {
                    updates.push((target, combined));
                }
            }
        }

        let changed = updates.len();
        for (idx, value) in updates {
            self.graph[idx].value = value;
        }
        changed
    }

    /// Provenance-bearing variant of [`Self::propagate`].
    ///
    /// Runs the same single-round algorithm, but for every node whose
    /// value changes records a [`Derivation`] capturing the
    /// `(previous_value, new_value)` pair plus a [`Contribution`] for
    /// each incoming edge that fed into the combined value (including
    /// edges that contributed `Unknown`, so callers can audit "was
    /// this edge silent or did it actually fire?").
    ///
    /// Returns `(changed_count, derivations)` where
    /// `derivations.len() == changed_count` — each changed node yields
    /// exactly one derivation record.
    pub fn propagate_with_provenance(&mut self) -> (usize, Vec<Derivation>) {
        let mut updates: Vec<(NodeIndex, HexValue, Vec<Contribution>, HexValue)> = Vec::new();
        let node_indices: Vec<NodeIndex> = self.graph.node_indices().collect();
        for &target in &node_indices {
            let mut incoming_values: Vec<HexValue> = Vec::new();
            let mut contribs: Vec<Contribution> = Vec::new();
            let edge_indices: Vec<_> = self.graph.edge_indices().collect();
            for edge_idx in edge_indices {
                let (src, tgt) = self.graph.edge_endpoints(edge_idx).unwrap();
                if tgt != target {
                    continue;
                }
                let source_value = self.graph[src].value;
                let relation = *self.graph.edge_weight(edge_idx).unwrap();
                let contribution = match relation {
                    Relation::Supports => source_value,
                    Relation::Contradicts => HexValue::not(source_value),
                    Relation::Implies => {
                        if matches!(source_value, HexValue::True | HexValue::Probable) {
                            source_value
                        } else {
                            // Implication is silent on non-true antecedents — record nothing.
                            continue;
                        }
                    }
                };
                incoming_values.push(contribution);
                contribs.push(Contribution {
                    source: self.graph[src].label.clone(),
                    source_value,
                    relation,
                    contributed_value: contribution,
                });
            }
            if incoming_values.is_empty() {
                continue;
            }
            let current = self.graph[target].value;
            let combined = incoming_values
                .into_iter()
                .fold(current, HexLattice::combine_evidence);
            if combined != current {
                updates.push((target, combined, contribs, current));
            }
        }
        let changed = updates.len();
        let mut derivations = Vec::with_capacity(changed);
        for (idx, value, contributions, previous_value) in updates {
            let proposition = self.graph[idx].label.clone();
            self.graph[idx].value = value;
            derivations.push(Derivation {
                proposition,
                previous_value,
                new_value: value,
                contributions,
                round: 0, // round filled in by `propagate_until_stable_with_provenance`
            });
        }
        (changed, derivations)
    }

    /// Provenance-bearing variant of [`Self::propagate_until_stable`].
    ///
    /// Returns `(iterations, derivations)`:
    /// - `iterations`: same semantics as [`Self::propagate_until_stable`]
    ///   (1 if there was nothing to do; N+1 if changes happened in
    ///   rounds 1..N and round N+1 confirmed convergence).
    /// - `derivations`: every per-node change across all rounds, in
    ///   the order they were applied. Each `Derivation::round` records
    ///   which round (1-indexed) produced it, so multi-hop chains are
    ///   readable as a sequence.
    pub fn propagate_until_stable_with_provenance(
        &mut self,
        max_iterations: usize,
    ) -> (usize, Vec<Derivation>) {
        let mut all_derivations: Vec<Derivation> = Vec::new();
        for i in 0..max_iterations {
            let (changed, mut derivs) = self.propagate_with_provenance();
            for d in derivs.iter_mut() {
                d.round = i + 1;
            }
            all_derivations.append(&mut derivs);
            if changed == 0 {
                return (i + 1, all_derivations);
            }
        }
        (max_iterations, all_derivations)
    }

    /// Run propagation until convergence or max iterations.
    /// Returns the total number of iterations run.
    pub fn propagate_until_stable(&mut self, max_iterations: usize) -> usize {
        for i in 0..max_iterations {
            if self.propagate() == 0 {
                return i + 1;
            }
        }
        max_iterations
    }
}

impl Default for BeliefNetwork {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hex_value_display() {
        assert_eq!(format!("{}", HexValue::True), "T");
        assert_eq!(format!("{}", HexValue::Probable), "P");
        assert_eq!(format!("{}", HexValue::Unknown), "U");
        assert_eq!(format!("{}", HexValue::Doubtful), "D");
        assert_eq!(format!("{}", HexValue::False), "F");
        assert_eq!(format!("{}", HexValue::Contradictory), "C");
    }

    #[test]
    fn test_confidence_scores() {
        assert_eq!(HexValue::True.confidence(), 1.0);
        assert_eq!(HexValue::False.confidence(), 0.0);
        assert_eq!(HexValue::Unknown.confidence(), 0.5);
        assert_eq!(HexValue::Contradictory.confidence(), 0.5);
    }

    #[test]
    fn test_is_definite() {
        assert!(HexValue::True.is_definite());
        assert!(HexValue::Unknown.is_definite());
        assert!(!HexValue::Contradictory.is_definite());
    }

    // -- Lattice operations --

    #[test]
    fn test_join_takes_more_true() {
        assert_eq!(
            HexValue::join(HexValue::Probable, HexValue::Doubtful),
            HexValue::Probable
        );
        assert_eq!(
            HexValue::join(HexValue::False, HexValue::True),
            HexValue::True
        );
        assert_eq!(
            HexValue::join(HexValue::Unknown, HexValue::Unknown),
            HexValue::Unknown
        );
    }

    #[test]
    fn test_join_contradiction_absorbs() {
        assert_eq!(
            HexValue::join(HexValue::True, HexValue::Contradictory),
            HexValue::Contradictory
        );
        assert_eq!(
            HexValue::join(HexValue::Contradictory, HexValue::False),
            HexValue::Contradictory
        );
    }

    #[test]
    fn test_meet_takes_more_false() {
        assert_eq!(
            HexValue::meet(HexValue::Probable, HexValue::Doubtful),
            HexValue::Doubtful
        );
        assert_eq!(
            HexValue::meet(HexValue::True, HexValue::False),
            HexValue::False
        );
    }

    #[test]
    fn test_meet_contradiction_absorbs() {
        assert_eq!(
            HexValue::meet(HexValue::True, HexValue::Contradictory),
            HexValue::Contradictory
        );
    }

    #[test]
    fn test_not_symmetry() {
        assert_eq!(HexValue::not(HexValue::True), HexValue::False);
        assert_eq!(HexValue::not(HexValue::False), HexValue::True);
        assert_eq!(HexValue::not(HexValue::Probable), HexValue::Doubtful);
        assert_eq!(HexValue::not(HexValue::Doubtful), HexValue::Probable);
        assert_eq!(HexValue::not(HexValue::Unknown), HexValue::Unknown);
        assert_eq!(
            HexValue::not(HexValue::Contradictory),
            HexValue::Contradictory
        );
    }

    #[test]
    fn test_double_negation() {
        for v in [
            HexValue::True,
            HexValue::Probable,
            HexValue::Unknown,
            HexValue::Doubtful,
            HexValue::False,
            HexValue::Contradictory,
        ] {
            assert_eq!(
                HexValue::not(HexValue::not(v)),
                v,
                "Double negation failed for {v}"
            );
        }
    }

    #[test]
    fn test_top_and_bottom() {
        assert_eq!(HexValue::top(), HexValue::True);
        assert_eq!(HexValue::bottom(), HexValue::False);
    }

    // -- Evidence combination --

    #[test]
    fn test_combine_agreeing_evidence() {
        assert_eq!(
            HexLattice::combine_evidence(HexValue::True, HexValue::Probable),
            HexValue::True
        );
        assert_eq!(
            HexLattice::combine_evidence(HexValue::False, HexValue::Doubtful),
            HexValue::Doubtful
        );
    }

    #[test]
    fn test_combine_conflicting_evidence() {
        assert_eq!(
            HexLattice::combine_evidence(HexValue::True, HexValue::False),
            HexValue::Contradictory
        );
        assert_eq!(
            HexLattice::combine_evidence(HexValue::Probable, HexValue::Doubtful),
            HexValue::Contradictory
        );
    }

    #[test]
    fn test_combine_neutral_evidence() {
        // Unknown doesn't conflict with anything
        assert_eq!(
            HexLattice::combine_evidence(HexValue::True, HexValue::Unknown),
            HexValue::True
        );
        assert_eq!(
            HexLattice::combine_evidence(HexValue::Unknown, HexValue::False),
            HexValue::Unknown
        );
    }

    // -- BeliefNetwork --

    #[test]
    fn test_empty_network() {
        let net = BeliefNetwork::new();
        assert_eq!(net.len(), 0);
        assert!(net.is_empty());
    }

    #[test]
    fn test_add_and_lookup() {
        let mut net = BeliefNetwork::new();
        net.add_proposition("sky-is-blue", HexValue::True);
        net.add_proposition("grass-is-purple", HexValue::False);

        assert_eq!(net.len(), 2);
        assert_eq!(net.get("sky-is-blue").unwrap().value, HexValue::True);
        assert_eq!(net.get("grass-is-purple").unwrap().value, HexValue::False);
        assert!(net.get("nonexistent").is_none());
    }

    #[test]
    fn test_support_propagation() {
        let mut net = BeliefNetwork::new();
        let a = net.add_proposition("evidence", HexValue::True);
        let b = net.add_proposition("hypothesis", HexValue::Unknown);
        net.add_relation(a, b, Relation::Supports);

        let changed = net.propagate();
        assert_eq!(changed, 1);
        // Evidence True + Unknown via combine_evidence = True (both not conflicting)
        assert_eq!(net.get("hypothesis").unwrap().value, HexValue::True);
    }

    #[test]
    fn test_contradiction_propagation() {
        let mut net = BeliefNetwork::new();
        let a = net.add_proposition("evidence-for", HexValue::True);
        let b = net.add_proposition("evidence-against", HexValue::True);
        let c = net.add_proposition("hypothesis", HexValue::Unknown);

        net.add_relation(a, c, Relation::Supports);
        net.add_relation(b, c, Relation::Contradicts);

        net.propagate();
        // True support + True contradiction (NOT True = False) -> Contradictory
        assert_eq!(
            net.get("hypothesis").unwrap().value,
            HexValue::Contradictory
        );
    }

    #[test]
    fn test_implication_fires_on_true() {
        let mut net = BeliefNetwork::new();
        let a = net.add_proposition("premise", HexValue::True);
        let b = net.add_proposition("conclusion", HexValue::Unknown);
        net.add_relation(a, b, Relation::Implies);

        net.propagate();
        assert_eq!(net.get("conclusion").unwrap().value, HexValue::True);
    }

    #[test]
    fn test_implication_silent_on_false() {
        let mut net = BeliefNetwork::new();
        let a = net.add_proposition("premise", HexValue::False);
        let b = net.add_proposition("conclusion", HexValue::Unknown);
        net.add_relation(a, b, Relation::Implies);

        let changed = net.propagate();
        assert_eq!(changed, 0);
        assert_eq!(net.get("conclusion").unwrap().value, HexValue::Unknown);
    }

    #[test]
    fn declare_relation_auto_creates_endpoints_as_unknown() {
        let mut net = BeliefNetwork::new();
        // Both endpoints unseen → declare creates them as Unknown and
        // wires the edge so a subsequent direct update on `from`
        // propagates to `to`.
        net.declare_relation("evidence", "hypothesis", Relation::Implies);
        assert_eq!(net.len(), 2);
        assert_eq!(net.get("evidence").unwrap().value, HexValue::Unknown);
        assert_eq!(net.get("hypothesis").unwrap().value, HexValue::Unknown);

        // Drive the antecedent True; one round of propagation must
        // derive the consequent.
        net.get_mut("evidence").unwrap().value = HexValue::True;
        let changed = net.propagate();
        assert_eq!(changed, 1);
        assert_eq!(net.get("hypothesis").unwrap().value, HexValue::True);
    }

    #[test]
    fn propagate_with_provenance_records_each_hop() {
        // 2-hop chain: a -Implies-> b -Supports-> c. Drive a True,
        // expect two derivations: one for b in round 1, one for c in
        // round 2. Each must carry its incoming contributions.
        let mut net = BeliefNetwork::new();
        net.declare_relation("a", "b", Relation::Implies);
        net.declare_relation("b", "c", Relation::Supports);
        net.get_mut("a").unwrap().value = HexValue::True;

        let (rounds, derivations) = net.propagate_until_stable_with_provenance(10);
        // Convergence: r1 derives b, r2 derives c, r3 confirms zero-change.
        assert_eq!(rounds, 3, "two productive rounds + one zero-change round");
        assert_eq!(derivations.len(), 2);

        let by_prop: HashMap<String, Derivation> = derivations
            .iter()
            .cloned()
            .map(|d| (d.proposition.clone(), d))
            .collect();

        let b_deriv = by_prop.get("b").expect("b must be derived");
        assert_eq!(b_deriv.previous_value, HexValue::Unknown);
        assert_eq!(b_deriv.new_value, HexValue::True);
        assert_eq!(b_deriv.round, 1);
        assert_eq!(b_deriv.contributions.len(), 1);
        assert_eq!(b_deriv.contributions[0].source, "a");
        assert_eq!(b_deriv.contributions[0].relation, Relation::Implies);
        assert_eq!(b_deriv.contributions[0].source_value, HexValue::True);
        assert_eq!(b_deriv.contributions[0].contributed_value, HexValue::True);

        let c_deriv = by_prop.get("c").expect("c must be derived in round 2");
        assert_eq!(c_deriv.round, 2, "c is derived AFTER b — round 2");
        assert_eq!(c_deriv.contributions[0].source, "b");
        assert_eq!(c_deriv.contributions[0].relation, Relation::Supports);
    }

    #[test]
    fn propagate_with_provenance_returns_empty_on_edge_less_graph() {
        let mut net = BeliefNetwork::new();
        net.add_proposition("only", HexValue::True);
        let (rounds, derivations) = net.propagate_until_stable_with_provenance(10);
        assert_eq!(rounds, 1, "single zero-change round");
        assert!(derivations.is_empty());
    }

    #[test]
    fn propagate_with_provenance_records_contradicts_contribution() {
        // Contribution records the contributed_value AFTER NOT() is
        // applied for Contradicts, so consumers don't have to
        // reconstruct the rule.
        let mut net = BeliefNetwork::new();
        net.declare_relation("anomaly", "stable", Relation::Contradicts);
        net.get_mut("anomaly").unwrap().value = HexValue::True;
        // Pre-set stable so combine_evidence(True, NOT(True)=False) = Contradictory.
        net.get_mut("stable").unwrap().value = HexValue::True;

        let (_, derivations) = net.propagate_until_stable_with_provenance(10);
        let stable_deriv = derivations
            .iter()
            .find(|d| d.proposition == "stable")
            .expect("stable must derive");
        assert_eq!(stable_deriv.new_value, HexValue::Contradictory);
        let contrib = &stable_deriv.contributions[0];
        assert_eq!(contrib.source, "anomaly");
        assert_eq!(contrib.relation, Relation::Contradicts);
        assert_eq!(contrib.source_value, HexValue::True);
        assert_eq!(
            contrib.contributed_value,
            HexValue::False,
            "Contradicts contributes NOT(source) = NOT(True) = False"
        );
    }

    #[test]
    fn declare_relation_idempotent_on_existing_nodes() {
        // Declaring a relation twice is a no-op on the node set — the
        // edge gets added twice (multi-graph), but the proposition
        // count and value stay the same.
        let mut net = BeliefNetwork::new();
        net.add_proposition("a", HexValue::True);
        net.add_proposition("b", HexValue::Unknown);
        net.declare_relation("a", "b", Relation::Supports);
        net.declare_relation("a", "b", Relation::Supports);
        assert_eq!(net.len(), 2);
        // Two parallel Supports edges is still equivalent to one for
        // propagation purposes — combine_evidence is idempotent on
        // identical inputs.
        net.propagate();
        assert_eq!(net.get("b").unwrap().value, HexValue::True);
    }

    #[test]
    fn test_propagate_until_stable() {
        let mut net = BeliefNetwork::new();
        let a = net.add_proposition("root", HexValue::True);
        let b = net.add_proposition("mid", HexValue::Unknown);
        let c = net.add_proposition("leaf", HexValue::Unknown);
        net.add_relation(a, b, Relation::Supports);
        net.add_relation(b, c, Relation::Implies);

        let iterations = net.propagate_until_stable(10);
        assert!(iterations <= 10);
        assert_eq!(net.get("leaf").unwrap().value, HexValue::True);
    }
}
