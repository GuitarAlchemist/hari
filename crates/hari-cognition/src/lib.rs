//! # hari-cognition — Lie Algebra Inspired Cognitive Architecture
//!
//! This is the research core of Project Hari. The central idea:
//!
//! ## Intuition
//!
//! In physics, Lie groups describe continuous symmetries (rotations, translations),
//! and their Lie algebras describe infinitesimal generators of those symmetries.
//! We borrow this framework for cognition:
//!
//! - **Cognitive states** live in a high-dimensional space
//! - **Cognitive operations** (attention shifts, belief updates, goal changes)
//!   are transformations of that space
//! - These operations form a **symmetry group** — they preserve certain invariants
//!   (e.g., coherence, identity continuity)
//! - The **Lie algebra** of this group gives us infinitesimal generators —
//!   the "basis moves" from which all cognitive transformations can be composed
//! - **Evolution** of cognitive state is a differential equation on the Lie algebra:
//!   dψ/dt = H(ψ) where H is the cognitive Hamiltonian
//!
//! ## Why this matters for AGI
//!
//! 1. **Composability**: Complex thoughts = combinations of simple generators
//! 2. **Continuity**: Smooth evolution prevents catastrophic belief jumps
//! 3. **Invariants**: Symmetry preservation = identity/value stability
//! 4. **Analysis**: Lie algebra tools (commutators, exponential map) give us
//!    formal ways to analyze cognitive dynamics
//!
//! This is exploratory research code. The math is real; whether it's the
//! right math for cognition is what we're investigating.

use nalgebra::{DMatrix, DVector};
use std::fmt;

// ---------------------------------------------------------------------------
// SymmetryGroup — cognitive transformations as symmetry operations
// ---------------------------------------------------------------------------

/// A cognitive symmetry operation, represented as a matrix transformation.
///
/// In the Lie group picture, each symmetry operation is an element of GL(n),
/// the general linear group. We use matrices because they compose naturally
/// and have well-understood algebraic structure.
///
/// Examples of cognitive symmetries:
/// - Attention rotation: shift focus from one concept to another
/// - Belief scaling: strengthen or weaken a belief
/// - Goal projection: project state onto a goal-relevant subspace
#[derive(Debug, Clone)]
pub struct SymmetryGroup {
    /// Human-readable name for this symmetry group
    pub name: String,
    /// Dimension of the cognitive state space this group acts on
    pub dimension: usize,
    /// Generator matrices for this group's Lie algebra.
    /// Any group element can be obtained via exp(linear combination of generators).
    generators: Vec<DMatrix<f64>>,
}

impl SymmetryGroup {
    /// Create a new symmetry group with the given generators.
    ///
    /// Each generator must be a square matrix of size `dimension x dimension`.
    /// Generators should be linearly independent for a non-degenerate algebra.
    pub fn new(name: impl Into<String>, dimension: usize, generators: Vec<DMatrix<f64>>) -> Self {
        for (i, g) in generators.iter().enumerate() {
            assert_eq!(
                (g.nrows(), g.ncols()),
                (dimension, dimension),
                "Generator {i} has wrong dimensions: expected {dimension}x{dimension}"
            );
        }
        Self {
            name: name.into(),
            dimension,
            generators,
        }
    }

    /// Number of generators (= dimension of the Lie algebra).
    pub fn algebra_dimension(&self) -> usize {
        self.generators.len()
    }

    /// Get a reference to the i-th generator.
    pub fn generator(&self, i: usize) -> &DMatrix<f64> {
        &self.generators[i]
    }

    /// Create the identity transformation for this group.
    pub fn identity(&self) -> DMatrix<f64> {
        DMatrix::identity(self.dimension, self.dimension)
    }

    /// Construct a skew-symmetric "attention rotation" generator coupling
    /// dimensions `i` and `j`.
    ///
    /// The resulting matrix `G` satisfies `G^T = -G` and rotates the (i, j)
    /// plane: `G[i,j] = -1`, `G[j,i] = +1`. Exponentiating it produces a
    /// rotation in the i–j subspace, leaving other dimensions untouched.
    ///
    /// Returns the zero matrix when `i == j` so callers do not have to
    /// special-case degenerate goal axes.
    pub fn attention_rotation(d: usize, i: usize, j: usize) -> DMatrix<f64> {
        assert!(i < d && j < d, "rotation indices out of range");
        let mut m = DMatrix::zeros(d, d);
        if i == j {
            return m;
        }
        m[(i, j)] = -1.0;
        m[(j, i)] = 1.0;
        m
    }

    /// Construct a diagonal "belief scaling" generator weighted by a vector
    /// of per-dimension scalars (typically derived from `HexValue` ranks).
    ///
    /// The result is a `d x d` diagonal matrix whose `(k, k)` entry is
    /// `weights[k]`. When the supplied weight vector is shorter than `d`,
    /// missing entries default to zero; longer vectors are truncated.
    pub fn belief_scaling(d: usize, weights: &[f64]) -> DMatrix<f64> {
        let mut m = DMatrix::zeros(d, d);
        for k in 0..d {
            m[(k, k)] = weights.get(k).copied().unwrap_or(0.0);
        }
        m
    }

    /// Construct a "goal projection" generator `e_t e_t^T - I/d`.
    ///
    /// This pulls cognitive state toward the `target` axis while subtracting
    /// the trace-preserving mean so the generator is traceless. It is not
    /// idempotent (matrix exponentials of traceless generators are
    /// volume-preserving in spirit).
    pub fn goal_projection(d: usize, target: usize) -> DMatrix<f64> {
        assert!(target < d, "projection target out of range");
        let mut m = DMatrix::zeros(d, d);
        m[(target, target)] = 1.0;
        let mean = 1.0 / d as f64;
        for k in 0..d {
            m[(k, k)] -= mean;
        }
        m
    }
}

impl fmt::Display for SymmetryGroup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SymmetryGroup({}, dim={}, generators={})",
            self.name,
            self.dimension,
            self.generators.len()
        )
    }
}

// ---------------------------------------------------------------------------
// CognitiveAlgebra — Lie algebra structure for cognitive operations
// ---------------------------------------------------------------------------

/// The Lie algebra of a cognitive symmetry group.
///
/// Provides the key algebraic operations:
/// - **Commutator** [A, B] = AB - BA: measures how two operations interfere
/// - **Linear combination**: compose generators with real coefficients
/// - **Exponential map**: convert algebra elements to group elements
///
/// The commutator is particularly important: if [A, B] = 0, the operations
/// commute (order doesn't matter). Non-zero commutators reveal cognitive
/// operations that are order-dependent — attention before reasoning gives
/// different results than reasoning before attention.
pub struct CognitiveAlgebra {
    group: SymmetryGroup,
}

impl CognitiveAlgebra {
    /// Create a Lie algebra from its symmetry group.
    pub fn new(group: SymmetryGroup) -> Self {
        Self { group }
    }

    /// The Lie bracket (commutator): [A, B] = AB - BA.
    ///
    /// This measures the "non-commutativity" of two operations.
    /// If [A, B] = 0, they commute. Otherwise, order matters.
    pub fn commutator(a: &DMatrix<f64>, b: &DMatrix<f64>) -> DMatrix<f64> {
        a * b - b * a
    }

    /// Compute the structure constants of the algebra.
    ///
    /// Structure constants c^k_{ij} satisfy: [G_i, G_j] = sum_k c^k_{ij} G_k
    /// They completely characterize the algebra's structure.
    ///
    /// Returns a 3D tensor flattened as Vec<Vec<Vec<f64>>> indexed [i][j][k].
    // i/j/k are tensor indices passed to `generator(i)` and `constants[i][j][k]`,
    // not iterator-replaceable single-axis traversals.
    #[allow(clippy::needless_range_loop)]
    pub fn structure_constants(&self) -> Vec<Vec<Vec<f64>>> {
        let n = self.group.algebra_dimension();
        let mut constants = vec![vec![vec![0.0f64; n]; n]; n];

        for i in 0..n {
            for j in 0..n {
                let bracket = Self::commutator(self.group.generator(i), self.group.generator(j));

                // Project the commutator onto each generator using trace inner product:
                // c^k_{ij} = Tr(bracket * G_k^T) / Tr(G_k * G_k^T)
                for k in 0..n {
                    let gk = self.group.generator(k);
                    let numerator = (&bracket * gk.transpose()).trace();
                    let denominator = (gk * gk.transpose()).trace();
                    if denominator.abs() > 1e-12 {
                        constants[i][j][k] = numerator / denominator;
                    }
                }
            }
        }

        constants
    }

    /// Linear combination of generators: sum_i coefficients[i] * G_i.
    ///
    /// This produces an element of the Lie algebra, which can be
    /// exponentiated to get a group element (a cognitive transformation).
    pub fn linear_combination(&self, coefficients: &[f64]) -> DMatrix<f64> {
        assert_eq!(
            coefficients.len(),
            self.group.algebra_dimension(),
            "Need exactly {} coefficients",
            self.group.algebra_dimension()
        );

        let dim = self.group.dimension;
        let mut result = DMatrix::zeros(dim, dim);
        for (i, &c) in coefficients.iter().enumerate() {
            result += c * self.group.generator(i);
        }
        result
    }

    /// Exponential map: algebra element -> group element.
    ///
    /// Uses Padé approximation via Taylor series truncation:
    /// exp(A) ≈ I + A + A²/2! + A³/3! + ...
    ///
    /// This converts an infinitesimal cognitive operation into a
    /// finite transformation that can be applied to cognitive states.
    pub fn exp(matrix: &DMatrix<f64>, order: usize) -> DMatrix<f64> {
        let dim = matrix.nrows();
        let mut result = DMatrix::identity(dim, dim);
        let mut term = DMatrix::identity(dim, dim);

        for k in 1..=order {
            term = &term * matrix / (k as f64);
            result += &term;
        }
        result
    }

    /// Access the underlying symmetry group.
    pub fn group(&self) -> &SymmetryGroup {
        &self.group
    }
}

// ---------------------------------------------------------------------------
// Representation — mapping internal states to observable behavior
// ---------------------------------------------------------------------------

/// A representation maps cognitive states to observable outputs.
///
/// In Lie theory, a representation is a homomorphism from a group to GL(V)
/// for some vector space V. Here, V is the "observable space" — what an
/// external observer can measure about the cognitive system.
///
/// This creates a principled separation between internal state (potentially
/// high-dimensional, abstract) and external behavior (lower-dimensional,
/// measurable).
#[derive(Debug, Clone)]
pub struct Representation {
    /// Name of this representation
    pub name: String,
    /// Dimension of the observable space
    pub output_dimension: usize,
    /// The representation matrix: maps internal state to observable state.
    /// Shape: output_dimension x input_dimension.
    pub matrix: DMatrix<f64>,
}

impl Representation {
    /// Create a new representation with the given mapping matrix.
    pub fn new(name: impl Into<String>, matrix: DMatrix<f64>) -> Self {
        Self {
            name: name.into(),
            output_dimension: matrix.nrows(),
            matrix,
        }
    }

    /// Map an internal cognitive state to an observable output.
    pub fn observe(&self, state: &DVector<f64>) -> DVector<f64> {
        &self.matrix * state
    }

    /// Dimension of the internal state this representation expects.
    pub fn input_dimension(&self) -> usize {
        self.matrix.ncols()
    }
}

// ---------------------------------------------------------------------------
// Evolution — cognitive state dynamics on the Lie algebra
// ---------------------------------------------------------------------------

/// Evolves cognitive state over time using Lie algebra dynamics.
///
/// The evolution equation is:
///   dψ/dt = H(ψ, t)
/// where H is the "cognitive Hamiltonian" — a function that returns
/// an element of the Lie algebra given the current state and time.
///
/// We integrate this using simple Euler steps (research code — not
/// production numerics). Each step:
///   ψ(t + dt) = exp(H(ψ, t) * dt) * ψ(t)
///
/// The Hamiltonian encodes what drives cognitive change:
/// - Goals create "potential gradients" that pull state toward desired configurations
/// - Attention creates "kinetic energy" that determines how fast state changes
/// - Beliefs create "constraints" that restrict the space of valid states
pub struct Evolution {
    /// Current cognitive state vector
    pub state: DVector<f64>,
    /// Current time
    pub time: f64,
    /// Time step for integration
    pub dt: f64,
    /// The algebra generators used for evolution
    generators: Vec<DMatrix<f64>>,
}

impl Evolution {
    /// Create a new evolution with initial state and generators.
    pub fn new(initial_state: DVector<f64>, generators: Vec<DMatrix<f64>>, dt: f64) -> Self {
        Self {
            state: initial_state,
            time: 0.0,
            dt,
            generators,
        }
    }

    /// Take one Euler step with the given Hamiltonian coefficients.
    ///
    /// The Hamiltonian is specified as coefficients in the generator basis:
    /// H = sum_i h_i * G_i
    ///
    /// The state evolves as: ψ(t+dt) = exp(H * dt) * ψ(t)
    pub fn step(&mut self, hamiltonian_coefficients: &[f64]) {
        assert_eq!(hamiltonian_coefficients.len(), self.generators.len());

        let dim = self.state.len();
        let mut h_matrix = DMatrix::zeros(dim, dim);
        for (i, &coeff) in hamiltonian_coefficients.iter().enumerate() {
            h_matrix += coeff * &self.generators[i];
        }

        // exp(H * dt) using Taylor series to order 6
        let h_dt = &h_matrix * self.dt;
        let evolution_operator = CognitiveAlgebra::exp(&h_dt, 6);

        self.state = &evolution_operator * &self.state;
        self.time += self.dt;
    }

    /// Evolve for multiple steps with constant Hamiltonian.
    pub fn evolve(&mut self, hamiltonian_coefficients: &[f64], steps: usize) {
        for _ in 0..steps {
            self.step(hamiltonian_coefficients);
        }
    }

    /// Compute the norm of the current state (a measure of "cognitive energy").
    pub fn state_norm(&self) -> f64 {
        self.state.norm()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use nalgebra::{DMatrix, DVector};

    /// Helper: create a simple 2D rotation generator (antisymmetric matrix).
    fn rotation_generator_2d() -> DMatrix<f64> {
        DMatrix::from_row_slice(2, 2, &[0.0, -1.0, 1.0, 0.0])
    }

    /// Helper: create a 2D scaling generator (diagonal matrix).
    fn scaling_generator_2d() -> DMatrix<f64> {
        DMatrix::from_row_slice(2, 2, &[1.0, 0.0, 0.0, -1.0])
    }

    // -- SymmetryGroup tests --

    #[test]
    fn test_symmetry_group_creation() {
        let g = SymmetryGroup::new("test", 2, vec![rotation_generator_2d()]);
        assert_eq!(g.dimension, 2);
        assert_eq!(g.algebra_dimension(), 1);
        assert_eq!(g.name, "test");
    }

    #[test]
    fn test_identity() {
        let g = SymmetryGroup::new("test", 3, vec![]);
        let id = g.identity();
        assert_eq!(id, DMatrix::identity(3, 3));
    }

    #[test]
    fn test_display() {
        let g = SymmetryGroup::new("cognitive", 2, vec![rotation_generator_2d()]);
        let s = format!("{}", g);
        assert!(s.contains("cognitive"));
        assert!(s.contains("dim=2"));
    }

    // -- CognitiveAlgebra tests --

    #[test]
    fn test_commutator_self_is_zero() {
        let a = rotation_generator_2d();
        let bracket = CognitiveAlgebra::commutator(&a, &a);
        for val in bracket.iter() {
            assert!(val.abs() < 1e-12, "Self-commutator should be zero");
        }
    }

    #[test]
    fn test_commutator_antisymmetry() {
        let a = rotation_generator_2d();
        let b = scaling_generator_2d();
        let ab = CognitiveAlgebra::commutator(&a, &b);
        let ba = CognitiveAlgebra::commutator(&b, &a);
        // [A,B] = -[B,A]
        let sum = &ab + &ba;
        for val in sum.iter() {
            assert!(val.abs() < 1e-12, "Commutator should be antisymmetric");
        }
    }

    #[test]
    fn test_linear_combination() {
        let g = SymmetryGroup::new(
            "test",
            2,
            vec![rotation_generator_2d(), scaling_generator_2d()],
        );
        let algebra = CognitiveAlgebra::new(g);

        let result = algebra.linear_combination(&[1.0, 0.0]);
        assert_eq!(result, rotation_generator_2d());

        let result = algebra.linear_combination(&[0.0, 1.0]);
        assert_eq!(result, scaling_generator_2d());

        // Linear combination: 0.5 * rot + 0.5 * scale
        let result = algebra.linear_combination(&[0.5, 0.5]);
        let expected = rotation_generator_2d() * 0.5 + scaling_generator_2d() * 0.5;
        assert_eq!(result, expected);
    }

    #[test]
    fn test_exp_identity() {
        // exp(0) = I
        let zero = DMatrix::zeros(2, 2);
        let result = CognitiveAlgebra::exp(&zero, 10);
        let identity = DMatrix::identity(2, 2);
        let diff = &result - &identity;
        assert!(diff.norm() < 1e-10, "exp(0) should equal identity");
    }

    #[test]
    fn test_exp_rotation() {
        // exp(theta * J) where J is the rotation generator should give a rotation matrix
        let j = rotation_generator_2d();
        let theta = std::f64::consts::PI / 4.0; // 45 degrees
        let rot = CognitiveAlgebra::exp(&(&j * theta), 20);

        // Should be approximately [[cos θ, -sin θ], [sin θ, cos θ]]
        let cos_t = theta.cos();
        let sin_t = theta.sin();
        assert!((rot[(0, 0)] - cos_t).abs() < 1e-6, "cos component wrong");
        assert!((rot[(0, 1)] + sin_t).abs() < 1e-6, "-sin component wrong");
        assert!((rot[(1, 0)] - sin_t).abs() < 1e-6, "sin component wrong");
        assert!((rot[(1, 1)] - cos_t).abs() < 1e-6, "cos component wrong");
    }

    #[test]
    // Tests c^k_{ij} = -c^k_{ji}: i and j swap roles, so iterator-style is wrong.
    #[allow(clippy::needless_range_loop)]
    fn test_structure_constants_antisymmetric() {
        let g = SymmetryGroup::new(
            "test",
            2,
            vec![rotation_generator_2d(), scaling_generator_2d()],
        );
        let algebra = CognitiveAlgebra::new(g);
        let sc = algebra.structure_constants();

        // c^k_{ij} = -c^k_{ji}
        let n = sc.len();
        for i in 0..n {
            for j in 0..n {
                for k in 0..n {
                    assert!(
                        (sc[i][j][k] + sc[j][i][k]).abs() < 1e-10,
                        "Structure constants not antisymmetric"
                    );
                }
            }
        }
    }

    // -- Representation tests --

    #[test]
    fn test_representation_observe() {
        // Project 3D internal state to 2D observable
        let matrix = DMatrix::from_row_slice(2, 3, &[1.0, 0.0, 0.0, 0.0, 1.0, 0.0]);
        let rep = Representation::new("projection", matrix);

        let state = DVector::from_vec(vec![1.0, 2.0, 3.0]);
        let obs = rep.observe(&state);
        assert_eq!(obs.len(), 2);
        assert_eq!(obs[0], 1.0);
        assert_eq!(obs[1], 2.0);
    }

    #[test]
    fn test_representation_dimensions() {
        let matrix = DMatrix::from_row_slice(2, 5, &[0.0; 10]);
        let rep = Representation::new("test", matrix);
        assert_eq!(rep.output_dimension, 2);
        assert_eq!(rep.input_dimension(), 5);
    }

    // -- Evolution tests --

    #[test]
    fn test_evolution_preserves_norm_for_rotation() {
        // Rotation should preserve the norm of the state vector
        let initial = DVector::from_vec(vec![1.0, 0.0]);
        let generators = vec![rotation_generator_2d()];
        let mut evo = Evolution::new(initial, generators, 0.01);

        let initial_norm = evo.state_norm();
        evo.evolve(&[1.0], 100); // Rotate for 100 steps

        // Norm should be preserved (rotation is orthogonal)
        assert!(
            (evo.state_norm() - initial_norm).abs() < 1e-4,
            "Rotation should preserve norm: {} vs {}",
            evo.state_norm(),
            initial_norm
        );
    }

    #[test]
    fn test_evolution_time_advances() {
        let initial = DVector::from_vec(vec![1.0, 0.0]);
        let generators = vec![rotation_generator_2d()];
        let mut evo = Evolution::new(initial, generators, 0.1);

        assert_eq!(evo.time, 0.0);
        evo.step(&[1.0]);
        assert!((evo.time - 0.1).abs() < 1e-12);
        evo.step(&[1.0]);
        assert!((evo.time - 0.2).abs() < 1e-12);
    }

    #[test]
    fn test_attention_rotation_is_skew_symmetric() {
        let g = SymmetryGroup::attention_rotation(4, 0, 2);
        let gt = g.transpose();
        let sum = &g + &gt;
        for v in sum.iter() {
            assert!(
                v.abs() < 1e-12,
                "rotation generator should satisfy G^T = -G"
            );
        }
        // Off-axis pairs are zero.
        assert_eq!(g[(1, 1)], 0.0);
        assert_eq!(g[(0, 1)], 0.0);
        // The active pair is anti-symmetric.
        assert_eq!(g[(0, 2)], -1.0);
        assert_eq!(g[(2, 0)], 1.0);
    }

    #[test]
    fn test_attention_rotation_self_pair_is_zero() {
        let g = SymmetryGroup::attention_rotation(3, 1, 1);
        for v in g.iter() {
            assert_eq!(*v, 0.0);
        }
    }

    #[test]
    fn test_belief_scaling_is_diagonal() {
        let g = SymmetryGroup::belief_scaling(3, &[0.5, -1.0, 2.0]);
        for r in 0..3 {
            for c in 0..3 {
                if r != c {
                    assert!(g[(r, c)].abs() < 1e-12, "off-diagonal must be zero");
                }
            }
        }
        assert_eq!(g[(0, 0)], 0.5);
        assert_eq!(g[(1, 1)], -1.0);
        assert_eq!(g[(2, 2)], 2.0);
    }

    #[test]
    fn test_belief_scaling_short_weights_pad_with_zero() {
        let g = SymmetryGroup::belief_scaling(4, &[1.0, 2.0]);
        assert_eq!(g[(0, 0)], 1.0);
        assert_eq!(g[(1, 1)], 2.0);
        assert_eq!(g[(2, 2)], 0.0);
        assert_eq!(g[(3, 3)], 0.0);
    }

    #[test]
    fn test_goal_projection_is_traceless() {
        let g = SymmetryGroup::goal_projection(4, 2);
        let trace: f64 = (0..4).map(|k| g[(k, k)]).sum();
        assert!(
            trace.abs() < 1e-12,
            "projection generator must be traceless"
        );
        // Target axis carries the bulk of the projection.
        assert!(g[(2, 2)] > 0.0, "target diagonal entry should be positive");
    }

    #[test]
    fn test_evolution_zero_hamiltonian_no_change() {
        let initial = DVector::from_vec(vec![1.0, 2.0]);
        let generators = vec![rotation_generator_2d()];
        let mut evo = Evolution::new(initial.clone(), generators, 0.1);

        evo.evolve(&[0.0], 50);
        for (a, b) in evo.state.iter().zip(initial.iter()) {
            assert!(
                (a - b).abs() < 1e-10,
                "Zero Hamiltonian should not change state"
            );
        }
    }
}
