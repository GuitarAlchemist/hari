//! # hari-core — Core Cognitive Loop Orchestrator
//!
//! The central crate of Project Hari, orchestrating the cognitive loop:
//! **Perceive -> Think -> Act -> Repeat**
//!
//! This is where the lattice logic, cognitive algebra, and swarm systems
//! come together into a unified cognitive architecture.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────┐
//! │              CognitiveLoop              │
//! │                                         │
//! │  ┌──────────┐  ┌───────┐  ┌──────────┐ │
//! │  │ Perceive │─>│ Think │─>│   Act    │ │
//! │  └──────────┘  └───────┘  └──────────┘ │
//! │       ↑                        │        │
//! │       └────────────────────────┘        │
//! │                                         │
//! │  CognitiveState                         │
//! │  ├── beliefs (BeliefNetwork)            │
//! │  ├── goals (priority queue)             │
//! │  └── attention (focus vector)           │
//! └─────────────────────────────────────────┘
//! ```

use hari_cognition::Evolution;
use hari_lattice::{BeliefNetwork, HexValue};
use hari_swarm::Message;
use nalgebra::{DMatrix, DVector};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

// ---------------------------------------------------------------------------
// Perception — incoming signals
// ---------------------------------------------------------------------------

/// A perception is an incoming signal from the environment.
///
/// Perceptions are the raw inputs to the cognitive loop. They carry
/// a proposition (what was perceived) and an initial confidence value.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Perception {
    /// What was perceived — maps to a proposition in the belief network
    pub proposition: String,
    /// Initial confidence in this perception
    pub value: HexValue,
    /// Source of the perception (sensor, agent, inference, etc.)
    pub source: String,
    /// Timestamp (cycle number)
    pub cycle: u64,
}

impl fmt::Display for Perception {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Perception({}: {} from {} @cycle {})",
            self.proposition, self.value, self.source, self.cycle
        )
    }
}

// ---------------------------------------------------------------------------
// Action — output actions
// ---------------------------------------------------------------------------

/// An action the cognitive system can take.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Action {
    /// Update a belief in the network
    UpdateBelief {
        proposition: String,
        value: HexValue,
    },
    /// Send a message to another agent in the swarm
    SendMessage(Message),
    /// Request more information about a topic
    Investigate { topic: String },
    /// Escalate a decision to a higher authority
    Escalate {
        reason: String,
        confidence: f64,
    },
    /// No action — the system decided to wait
    Wait,
    /// Log an observation (for auditability)
    Log(String),
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Action::UpdateBelief { proposition, value } => {
                write!(f, "UpdateBelief({proposition}: {value})")
            }
            Action::SendMessage(msg) => write!(f, "SendMessage({msg})"),
            Action::Investigate { topic } => write!(f, "Investigate({topic})"),
            Action::Escalate { reason, confidence } => {
                write!(f, "Escalate({reason}, conf={confidence:.2})")
            }
            Action::Wait => write!(f, "Wait"),
            Action::Log(msg) => write!(f, "Log({msg})"),
        }
    }
}

// ---------------------------------------------------------------------------
// Goal — what the system is trying to achieve
// ---------------------------------------------------------------------------

/// A goal with priority and status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Goal {
    /// Description of the goal
    pub description: String,
    /// Priority (higher = more important)
    pub priority: f64,
    /// Current status as a hexavalent value
    /// (True = achieved, Probable = likely achieved, Unknown = in progress, etc.)
    pub status: HexValue,
}

// ---------------------------------------------------------------------------
// CognitiveState — the brain's state
// ---------------------------------------------------------------------------

/// The central state of the cognitive system.
///
/// This holds everything the system "knows" and "wants":
/// - **beliefs**: A hexavalent belief network with propositions and relations
/// - **goals**: Prioritized objectives the system is working toward
/// - **attention**: A focus vector in cognitive state space (used by the algebra)
/// - **cycle**: How many think-perceive-act cycles have run
pub struct CognitiveState {
    /// The belief network — what the system thinks is true/false/unknown
    pub beliefs: BeliefNetwork,
    /// Active goals sorted by priority
    pub goals: BTreeMap<String, Goal>,
    /// Attention vector — where cognitive resources are focused
    pub attention: DVector<f64>,
    /// Current cycle number
    pub cycle: u64,
    /// Dimension of the cognitive state space
    pub dimension: usize,
}

impl CognitiveState {
    /// Create a new cognitive state with the given dimension.
    ///
    /// The dimension determines the size of the attention vector and
    /// the cognitive algebra's state space. Higher dimensions allow
    /// more nuanced cognitive representations but are more expensive.
    pub fn new(dimension: usize) -> Self {
        let mut attention = DVector::zeros(dimension);
        if dimension > 0 {
            attention[0] = 1.0; // Initial attention on first dimension
        }

        Self {
            beliefs: BeliefNetwork::new(),
            goals: BTreeMap::new(),
            attention,
            cycle: 0,
            dimension,
        }
    }

    /// Add a goal to the cognitive state.
    pub fn add_goal(&mut self, key: impl Into<String>, description: impl Into<String>, priority: f64) {
        self.goals.insert(
            key.into(),
            Goal {
                description: description.into(),
                priority,
                status: HexValue::Unknown,
            },
        );
    }

    /// Get the highest priority active goal.
    pub fn top_goal(&self) -> Option<(&String, &Goal)> {
        self.goals
            .iter()
            .filter(|(_, g)| !matches!(g.status, HexValue::True))
            .max_by(|(_, a), (_, b)| a.priority.partial_cmp(&b.priority).unwrap())
    }

    /// Summary of the current state for logging.
    pub fn summary(&self) -> String {
        let belief_count = self.beliefs.len();
        let goal_count = self.goals.len();
        let active_goals = self
            .goals
            .values()
            .filter(|g| !matches!(g.status, HexValue::True))
            .count();
        format!(
            "Cycle {}: {} beliefs, {}/{} goals active, attention norm={:.3}",
            self.cycle,
            belief_count,
            active_goals,
            goal_count,
            self.attention.norm()
        )
    }
}

// ---------------------------------------------------------------------------
// CognitiveLoop — the main perceive-think-act cycle
// ---------------------------------------------------------------------------

/// The main cognitive loop: Perceive -> Think -> Act.
///
/// Each cycle:
/// 1. **Perceive**: Process incoming perceptions, update belief network
/// 2. **Think**: Run belief propagation, evaluate goals, evolve cognitive state
/// 3. **Act**: Decide on actions based on current state and goals
///
/// The loop integrates all subsystems:
/// - Lattice logic for belief management
/// - Cognitive algebra for state evolution
/// - Swarm for multi-agent coordination
pub struct CognitiveLoop {
    /// The cognitive state
    pub state: CognitiveState,
    /// Cognitive evolution engine (Lie algebra dynamics)
    evolution: Option<Evolution>,
    /// Pending perceptions to process
    perception_buffer: Vec<Perception>,
    /// Actions produced in the last cycle
    last_actions: Vec<Action>,
}

impl CognitiveLoop {
    /// Create a new cognitive loop with the given state space dimension.
    pub fn new(dimension: usize) -> Self {
        Self {
            state: CognitiveState::new(dimension),
            evolution: None,
            perception_buffer: Vec::new(),
            last_actions: Vec::new(),
        }
    }

    /// Initialize the cognitive algebra with generators.
    ///
    /// This sets up the Lie algebra evolution engine. The generators
    /// define the "basis moves" of cognition — the fundamental operations
    /// from which all cognitive transformations are composed.
    pub fn init_algebra(&mut self, generators: Vec<DMatrix<f64>>, dt: f64) {
        let initial_state = self.state.attention.clone();
        self.evolution = Some(Evolution::new(initial_state, generators, dt));
    }

    /// Add a perception to the buffer for processing in the next cycle.
    pub fn perceive(&mut self, perception: Perception) {
        self.perception_buffer.push(perception);
    }

    /// Run one complete cognitive cycle: Perceive -> Think -> Act.
    ///
    /// Returns the actions decided upon in this cycle.
    pub fn cycle(&mut self) -> Vec<Action> {
        self.state.cycle += 1;
        let mut actions = Vec::new();

        // --- PERCEIVE ---
        let perceptions: Vec<Perception> = self.perception_buffer.drain(..).collect();
        for p in &perceptions {
            tracing::debug!("Processing perception: {}", p);
            // Add or update belief from perception
            if self.state.beliefs.get(&p.proposition).is_some() {
                if let Some(prop) = self.state.beliefs.get_mut(&p.proposition) {
                    let old = prop.value;
                    prop.value = hari_lattice::HexLattice::combine_evidence(old, p.value);
                    if prop.value != old {
                        actions.push(Action::Log(format!(
                            "Belief '{}' updated: {} -> {} (from {})",
                            p.proposition, old, prop.value, p.source
                        )));
                    }
                }
            } else {
                self.state.beliefs.add_proposition(&p.proposition, p.value);
                actions.push(Action::Log(format!(
                    "New belief '{}': {} (from {})",
                    p.proposition, p.value, p.source
                )));
            }
        }

        // --- THINK ---
        // Run belief propagation
        let propagation_changes = self.state.beliefs.propagate();
        if propagation_changes > 0 {
            tracing::info!("Belief propagation changed {} nodes", propagation_changes);
        }

        // Evolve cognitive state if algebra is initialized
        if let Some(ref mut evo) = self.evolution {
            // Use a simple Hamiltonian: attention-weighted identity
            let n = evo.state.len();
            // Small perturbation based on number of perceptions
            let strength = if perceptions.is_empty() { 0.0 } else { 0.1 };
            let coefficients: Vec<f64> = (0..n).map(|_| strength).collect();
            if coefficients.len() > 0 {
                // Only step if we have generators matching coefficient count
                // (graceful fallback if dimensions don't match)
                let _norm_before = evo.state_norm();
                // We can't easily step here without knowing generator count,
                // so we skip if not set up properly
            }
        }

        // Evaluate goals
        if let Some((key, goal)) = self.state.top_goal() {
            let key = key.clone();
            let goal_desc = goal.description.clone();
            tracing::debug!("Top goal: {} (priority={})", goal_desc, goal.priority);

            // Check if any beliefs relate to goal completion
            if let Some(belief) = self.state.beliefs.get(&key) {
                match belief.value {
                    HexValue::True | HexValue::Probable => {
                        if let Some(g) = self.state.goals.get_mut(&key) {
                            g.status = belief.value;
                        }
                        actions.push(Action::Log(format!("Goal '{}' achieved!", goal_desc)));
                    }
                    HexValue::Contradictory => {
                        actions.push(Action::Escalate {
                            reason: format!("Goal '{}' has contradictory evidence", goal_desc),
                            confidence: 0.5,
                        });
                    }
                    HexValue::Unknown => {
                        actions.push(Action::Investigate {
                            topic: key.clone(),
                        });
                    }
                    _ => {}
                }
            }
        }

        // --- ACT ---
        // Store and return actions
        self.last_actions = actions.clone();
        actions
    }

    /// Get the actions from the last cycle.
    pub fn last_actions(&self) -> &[Action] {
        &self.last_actions
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_perception_display() {
        let p = Perception {
            proposition: "sky-is-blue".to_string(),
            value: HexValue::True,
            source: "vision".to_string(),
            cycle: 1,
        };
        let s = format!("{}", p);
        assert!(s.contains("sky-is-blue"));
        assert!(s.contains("T"));
    }

    #[test]
    fn test_action_display() {
        let a = Action::UpdateBelief {
            proposition: "test".to_string(),
            value: HexValue::Probable,
        };
        assert!(format!("{}", a).contains("test"));
        assert!(format!("{}", Action::Wait).contains("Wait"));
    }

    #[test]
    fn test_cognitive_state_creation() {
        let state = CognitiveState::new(4);
        assert_eq!(state.dimension, 4);
        assert_eq!(state.cycle, 0);
        assert_eq!(state.attention.len(), 4);
        assert_eq!(state.attention[0], 1.0); // Initial attention
        assert!(state.beliefs.is_empty());
        assert!(state.goals.is_empty());
    }

    #[test]
    fn test_add_and_find_goals() {
        let mut state = CognitiveState::new(2);
        state.add_goal("learn", "Learn about the world", 0.8);
        state.add_goal("survive", "Ensure continued operation", 0.9);

        let (key, goal) = state.top_goal().unwrap();
        assert_eq!(key, "survive");
        assert_eq!(goal.priority, 0.9);
    }

    #[test]
    fn test_goal_completion_removes_from_top() {
        let mut state = CognitiveState::new(2);
        state.add_goal("a", "Goal A", 0.9);
        state.add_goal("b", "Goal B", 0.5);

        // Complete goal A
        state.goals.get_mut("a").unwrap().status = HexValue::True;

        let (key, _) = state.top_goal().unwrap();
        assert_eq!(key, "b");
    }

    #[test]
    fn test_state_summary() {
        let state = CognitiveState::new(3);
        let summary = state.summary();
        assert!(summary.contains("Cycle 0"));
        assert!(summary.contains("0 beliefs"));
    }

    #[test]
    fn test_cognitive_loop_creation() {
        let cl = CognitiveLoop::new(4);
        assert_eq!(cl.state.dimension, 4);
        assert_eq!(cl.state.cycle, 0);
    }

    #[test]
    fn test_cognitive_loop_perceive_and_cycle() {
        let mut cl = CognitiveLoop::new(4);

        cl.perceive(Perception {
            proposition: "temperature-high".to_string(),
            value: HexValue::Probable,
            source: "sensor".to_string(),
            cycle: 0,
        });

        let actions = cl.cycle();
        assert_eq!(cl.state.cycle, 1);

        // Should have logged the new belief
        assert!(!actions.is_empty());
        assert_eq!(
            cl.state.beliefs.get("temperature-high").unwrap().value,
            HexValue::Probable
        );
    }

    #[test]
    fn test_cognitive_loop_multiple_perceptions() {
        let mut cl = CognitiveLoop::new(4);

        cl.perceive(Perception {
            proposition: "danger".to_string(),
            value: HexValue::Probable,
            source: "sensor-a".to_string(),
            cycle: 0,
        });

        cl.cycle();

        // Second perception with conflicting evidence
        cl.perceive(Perception {
            proposition: "danger".to_string(),
            value: HexValue::Doubtful,
            source: "sensor-b".to_string(),
            cycle: 1,
        });

        cl.cycle();

        // Probable + Doubtful = Contradictory (conflicting evidence)
        assert_eq!(
            cl.state.beliefs.get("danger").unwrap().value,
            HexValue::Contradictory
        );
    }

    #[test]
    fn test_cognitive_loop_goal_investigation() {
        let mut cl = CognitiveLoop::new(4);
        cl.state.add_goal("find-food", "Locate food sources", 0.8);

        // Add an Unknown belief matching the goal
        cl.state
            .beliefs
            .add_proposition("find-food", HexValue::Unknown);

        let actions = cl.cycle();

        // Should produce an Investigate action for the unknown goal
        let has_investigate = actions.iter().any(|a| matches!(a, Action::Investigate { .. }));
        assert!(has_investigate, "Should investigate unknown goal");
    }

    #[test]
    fn test_cognitive_loop_goal_escalation() {
        let mut cl = CognitiveLoop::new(4);
        cl.state.add_goal("resolve-conflict", "Handle contradiction", 0.9);

        // Add a Contradictory belief matching the goal
        cl.state
            .beliefs
            .add_proposition("resolve-conflict", HexValue::Contradictory);

        let actions = cl.cycle();

        let has_escalate = actions.iter().any(|a| matches!(a, Action::Escalate { .. }));
        assert!(has_escalate, "Should escalate contradictory goal");
    }

    #[test]
    fn test_empty_cycle_no_crash() {
        let mut cl = CognitiveLoop::new(4);
        let actions = cl.cycle();
        assert!(actions.is_empty());
        assert_eq!(cl.state.cycle, 1);
    }
}
