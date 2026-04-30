//! # hari-swarm — Multi-Agent Swarm System
//!
//! A swarm of cognitive agents that communicate and reach consensus
//! using hexavalent logic. Each agent maintains its own belief state
//! and can exchange messages with other agents.
//!
//! ## Architecture
//!
//! - **Agent**: An individual cognitive entity with beliefs and identity
//! - **Message**: Typed inter-agent communication (belief updates, queries, votes)
//! - **Swarm**: The collective, managing agents and message routing
//! - **Consensus**: Hexavalent voting mechanism for collective decisions
//!
//! ## Design Philosophy
//!
//! The swarm is designed for parallelism — agents process messages independently,
//! and consensus is computed from independent votes. This maps naturally to
//! async execution with tokio.

use hari_lattice::{BeliefNetwork, HexLattice, HexValue};
use nalgebra::DVector;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

// ---------------------------------------------------------------------------
// Message — inter-agent communication
// ---------------------------------------------------------------------------

/// A message exchanged between agents in the swarm.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Sending agent's ID
    pub from: String,
    /// Target agent's ID (or "*" for broadcast)
    pub to: String,
    /// The message payload
    pub payload: MessagePayload,
}

/// The content of an inter-agent message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessagePayload {
    /// Share a belief: "I believe proposition X has value V"
    BeliefUpdate {
        proposition: String,
        value: HexValue,
    },
    /// Request another agent's belief about a proposition
    BeliefQuery { proposition: String },
    /// Response to a belief query
    BeliefResponse {
        proposition: String,
        value: HexValue,
    },
    /// Cast a vote on a proposition (for consensus)
    Vote {
        proposition: String,
        value: HexValue,
    },
    /// Free-form text message
    Text(String),
}

impl fmt::Display for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{} -> {}] {:?}", self.from, self.to, self.payload)
    }
}

// ---------------------------------------------------------------------------
// Agent — individual cognitive entity
// ---------------------------------------------------------------------------

/// An individual agent in the swarm with its own belief state.
///
/// Each agent has:
/// - A unique identity
/// - A local belief network (its view of the world)
/// - A message inbox
/// - A role that influences how it processes information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRole {
    /// Role name (e.g., "explorer", "critic", "integrator")
    pub name: String,
    /// How much this agent trusts its own beliefs vs. others' (0.0 to 1.0)
    pub self_trust: f64,
    /// How much this agent trusts incoming messages (0.0 to 1.0)
    pub message_trust: f64,
}

/// A cognitive agent in the swarm.
///
/// Each agent combines a hexavalent belief network (from hari-lattice)
/// with a cognitive state vector (from hari-cognition), enabling both
/// discrete logical reasoning and continuous cognitive dynamics.
pub struct Agent {
    /// Unique identifier for this agent
    pub id: String,
    /// The agent's role, which affects its behavior
    pub role: AgentRole,
    /// The agent's local belief network (discrete hexavalent logic)
    pub beliefs: BeliefNetwork,
    /// The agent's cognitive state vector (continuous Lie algebra space)
    pub cognitive_state: DVector<f64>,
    /// Inbox of received messages
    inbox: Vec<Message>,
}

impl Agent {
    /// Create a new agent with the given ID and role.
    pub fn new(id: impl Into<String>, role: AgentRole) -> Self {
        Self::with_cognitive_dimension(id, role, 4)
    }

    /// Create a new agent with an explicit cognitive state dimension.
    pub fn with_cognitive_dimension(
        id: impl Into<String>,
        role: AgentRole,
        cognitive_dimension: usize,
    ) -> Self {
        Self {
            id: id.into(),
            role,
            beliefs: BeliefNetwork::new(),
            cognitive_state: DVector::zeros(cognitive_dimension),
            inbox: Vec::new(),
        }
    }

    /// Receive a message into the inbox.
    pub fn receive(&mut self, message: Message) {
        self.inbox.push(message);
    }

    /// Process all messages in the inbox, updating beliefs accordingly.
    ///
    /// Returns the number of belief updates made.
    pub fn process_inbox(&mut self) -> usize {
        let messages: Vec<Message> = self.inbox.drain(..).collect();
        let mut updates = 0;

        for msg in messages {
            match msg.payload {
                MessagePayload::BeliefUpdate { proposition, value } => {
                    // Integrate the incoming belief weighted by message_trust
                    if let Some(prop) = self.beliefs.get_mut(&proposition) {
                        let combined = HexLattice::combine_evidence(prop.value, value);
                        if combined != prop.value {
                            prop.value = combined;
                            updates += 1;
                        }
                    } else {
                        // New proposition — add it with the received value
                        self.beliefs.add_proposition(proposition, value);
                        updates += 1;
                    }
                }
                MessagePayload::BeliefResponse { proposition, value } => {
                    // Same as belief update for now
                    if let Some(prop) = self.beliefs.get_mut(&proposition) {
                        let combined = HexLattice::combine_evidence(prop.value, value);
                        if combined != prop.value {
                            prop.value = combined;
                            updates += 1;
                        }
                    }
                }
                // Queries and text don't update beliefs directly
                _ => {}
            }
        }

        updates
    }

    /// Get this agent's vote on a proposition.
    pub fn vote(&self, proposition: &str) -> HexValue {
        self.beliefs
            .get(proposition)
            .map(|p| p.value)
            .unwrap_or(HexValue::Unknown)
    }

    /// Number of pending messages in inbox.
    pub fn inbox_len(&self) -> usize {
        self.inbox.len()
    }
}

// ---------------------------------------------------------------------------
// Consensus — hexavalent voting mechanism
// ---------------------------------------------------------------------------

/// Result of a consensus vote.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusResult {
    /// The proposition voted on
    pub proposition: String,
    /// Individual votes from each agent
    pub votes: HashMap<String, HexValue>,
    /// The consensus value
    pub consensus: HexValue,
    /// Agreement ratio (fraction of agents that agree with consensus)
    pub agreement: f64,
}

/// Compute consensus from a set of hexavalent votes.
///
/// Algorithm:
/// 1. Count votes in each category
/// 2. If any votes are Contradictory, check if majority is contradictory
/// 3. Otherwise, find the "center of mass" on the truth chain
/// 4. Map the center to the nearest HexValue
///
/// This is a novel consensus mechanism designed for epistemic humility —
/// it preserves uncertainty and contradiction rather than forcing binary outcomes.
pub fn compute_consensus(votes: &HashMap<String, HexValue>) -> HexValue {
    if votes.is_empty() {
        return HexValue::Unknown;
    }

    let total = votes.len() as f64;

    // Count each value
    let mut counts: HashMap<HexValue, usize> = HashMap::new();
    for &v in votes.values() {
        *counts.entry(v).or_insert(0) += 1;
    }

    // If majority is Contradictory, consensus is Contradictory
    let c_count = *counts.get(&HexValue::Contradictory).unwrap_or(&0);
    if c_count as f64 / total > 0.5 {
        return HexValue::Contradictory;
    }

    // Check for strong disagreement: significant True-ish AND False-ish votes
    let positive =
        counts.get(&HexValue::True).unwrap_or(&0) + counts.get(&HexValue::Probable).unwrap_or(&0);
    let negative =
        counts.get(&HexValue::False).unwrap_or(&0) + counts.get(&HexValue::Doubtful).unwrap_or(&0);

    if positive > 0 && negative > 0 {
        let min_faction = positive.min(negative) as f64;
        if min_faction / total > 0.3 {
            // Significant disagreement — consensus is Contradictory
            return HexValue::Contradictory;
        }
    }

    // Compute weighted center on the truth chain (excluding C votes)
    let definite_votes: Vec<HexValue> = votes
        .values()
        .copied()
        .filter(|v| v.is_definite())
        .collect();

    if definite_votes.is_empty() {
        return HexValue::Contradictory;
    }

    let avg_confidence: f64 =
        definite_votes.iter().map(|v| v.confidence()).sum::<f64>() / definite_votes.len() as f64;

    // Map average confidence to nearest HexValue
    if avg_confidence >= 0.875 {
        HexValue::True
    } else if avg_confidence >= 0.625 {
        HexValue::Probable
    } else if avg_confidence >= 0.375 {
        HexValue::Unknown
    } else if avg_confidence >= 0.125 {
        HexValue::Doubtful
    } else {
        HexValue::False
    }
}

// ---------------------------------------------------------------------------
// Swarm — the collective
// ---------------------------------------------------------------------------

/// A collection of agents that communicate and reach consensus.
///
/// The swarm manages agent lifecycle and message routing.
/// Designed for async execution — agents can process messages in parallel.
pub struct Swarm {
    agents: HashMap<String, Agent>,
}

impl Swarm {
    /// Create a new empty swarm.
    pub fn new() -> Self {
        Self {
            agents: HashMap::new(),
        }
    }

    /// Add an agent to the swarm.
    pub fn add_agent(&mut self, agent: Agent) {
        self.agents.insert(agent.id.clone(), agent);
    }

    /// Number of agents in the swarm.
    pub fn len(&self) -> usize {
        self.agents.len()
    }

    /// Whether the swarm is empty.
    pub fn is_empty(&self) -> bool {
        self.agents.is_empty()
    }

    /// Send a message to a specific agent or broadcast to all.
    pub fn send(&mut self, message: Message) {
        if message.to == "*" {
            // Broadcast: send to all except sender
            let from = message.from.clone();
            let ids: Vec<String> = self.agents.keys().cloned().collect();
            for id in ids {
                if id != from {
                    let mut msg = message.clone();
                    msg.to = id.clone();
                    if let Some(agent) = self.agents.get_mut(&id) {
                        agent.receive(msg);
                    }
                }
            }
        } else if let Some(agent) = self.agents.get_mut(&message.to) {
            agent.receive(message);
        }
    }

    /// All agents process their inboxes. Returns total number of belief updates.
    pub fn process_all(&mut self) -> usize {
        self.agents
            .values_mut()
            .map(|agent| agent.process_inbox())
            .sum()
    }

    /// Run a consensus vote on a proposition across all agents.
    pub fn consensus(&self, proposition: &str) -> ConsensusResult {
        let mut votes = HashMap::new();
        for (id, agent) in &self.agents {
            votes.insert(id.clone(), agent.vote(proposition));
        }

        let consensus = compute_consensus(&votes);

        let agreement = if votes.is_empty() {
            0.0
        } else {
            let agreeing = votes.values().filter(|&&v| v == consensus).count();
            agreeing as f64 / votes.len() as f64
        };

        ConsensusResult {
            proposition: proposition.to_string(),
            votes,
            consensus,
            agreement,
        }
    }

    /// Get a reference to an agent by ID.
    pub fn agent(&self, id: &str) -> Option<&Agent> {
        self.agents.get(id)
    }

    /// Get a mutable reference to an agent by ID.
    pub fn agent_mut(&mut self, id: &str) -> Option<&mut Agent> {
        self.agents.get_mut(id)
    }
}

impl Default for Swarm {
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

    fn make_agent(id: &str, self_trust: f64) -> Agent {
        Agent::new(
            id,
            AgentRole {
                name: "tester".to_string(),
                self_trust,
                message_trust: 0.5,
            },
        )
    }

    // -- Message tests --

    #[test]
    fn test_message_display() {
        let msg = Message {
            from: "alice".to_string(),
            to: "bob".to_string(),
            payload: MessagePayload::Text("hello".to_string()),
        };
        let s = format!("{}", msg);
        assert!(s.contains("alice"));
        assert!(s.contains("bob"));
    }

    // -- Agent tests --

    #[test]
    fn test_agent_creation() {
        let agent = make_agent("alice", 0.8);
        assert_eq!(agent.id, "alice");
        assert_eq!(agent.inbox_len(), 0);
        assert!(agent.beliefs.is_empty());
        assert_eq!(agent.cognitive_state.len(), 4);
    }

    #[test]
    fn test_agent_creation_with_explicit_cognitive_dimension() {
        let agent = Agent::with_cognitive_dimension(
            "alice",
            AgentRole {
                name: "tester".to_string(),
                self_trust: 0.8,
                message_trust: 0.5,
            },
            8,
        );

        assert_eq!(agent.cognitive_state.len(), 8);
    }

    #[test]
    fn test_agent_receive_and_process() {
        let mut agent = make_agent("bob", 0.8);

        let msg = Message {
            from: "alice".to_string(),
            to: "bob".to_string(),
            payload: MessagePayload::BeliefUpdate {
                proposition: "sky-is-blue".to_string(),
                value: HexValue::True,
            },
        };

        agent.receive(msg);
        assert_eq!(agent.inbox_len(), 1);

        let updates = agent.process_inbox();
        assert_eq!(updates, 1);
        assert_eq!(agent.inbox_len(), 0);
        assert_eq!(agent.vote("sky-is-blue"), HexValue::True);
    }

    #[test]
    fn test_agent_vote_unknown_for_missing() {
        let agent = make_agent("charlie", 0.5);
        assert_eq!(agent.vote("anything"), HexValue::Unknown);
    }

    // -- Consensus tests --

    #[test]
    fn test_consensus_unanimous_true() {
        let mut votes = HashMap::new();
        votes.insert("a".to_string(), HexValue::True);
        votes.insert("b".to_string(), HexValue::True);
        votes.insert("c".to_string(), HexValue::True);

        assert_eq!(compute_consensus(&votes), HexValue::True);
    }

    #[test]
    fn test_consensus_unanimous_false() {
        let mut votes = HashMap::new();
        votes.insert("a".to_string(), HexValue::False);
        votes.insert("b".to_string(), HexValue::False);

        assert_eq!(compute_consensus(&votes), HexValue::False);
    }

    #[test]
    fn test_consensus_mixed_positive() {
        let mut votes = HashMap::new();
        votes.insert("a".to_string(), HexValue::True);
        votes.insert("b".to_string(), HexValue::Probable);
        votes.insert("c".to_string(), HexValue::Probable);

        let result = compute_consensus(&votes);
        assert!(
            matches!(result, HexValue::True | HexValue::Probable),
            "Mixed positive should be True or Probable, got {result}"
        );
    }

    #[test]
    fn test_consensus_disagreement_is_contradictory() {
        let mut votes = HashMap::new();
        votes.insert("a".to_string(), HexValue::True);
        votes.insert("b".to_string(), HexValue::True);
        votes.insert("c".to_string(), HexValue::False);
        votes.insert("d".to_string(), HexValue::False);

        assert_eq!(compute_consensus(&votes), HexValue::Contradictory);
    }

    #[test]
    fn test_consensus_all_unknown() {
        let mut votes = HashMap::new();
        votes.insert("a".to_string(), HexValue::Unknown);
        votes.insert("b".to_string(), HexValue::Unknown);

        assert_eq!(compute_consensus(&votes), HexValue::Unknown);
    }

    #[test]
    fn test_consensus_empty() {
        let votes = HashMap::new();
        assert_eq!(compute_consensus(&votes), HexValue::Unknown);
    }

    #[test]
    fn test_consensus_majority_contradictory() {
        let mut votes = HashMap::new();
        votes.insert("a".to_string(), HexValue::Contradictory);
        votes.insert("b".to_string(), HexValue::Contradictory);
        votes.insert("c".to_string(), HexValue::True);

        assert_eq!(compute_consensus(&votes), HexValue::Contradictory);
    }

    // -- Swarm tests --

    #[test]
    fn test_swarm_creation() {
        let swarm = Swarm::new();
        assert_eq!(swarm.len(), 0);
        assert!(swarm.is_empty());
    }

    #[test]
    fn test_swarm_add_agents() {
        let mut swarm = Swarm::new();
        swarm.add_agent(make_agent("alice", 0.8));
        swarm.add_agent(make_agent("bob", 0.7));
        assert_eq!(swarm.len(), 2);
    }

    #[test]
    fn test_swarm_direct_message() {
        let mut swarm = Swarm::new();
        swarm.add_agent(make_agent("alice", 0.8));
        swarm.add_agent(make_agent("bob", 0.7));

        swarm.send(Message {
            from: "alice".to_string(),
            to: "bob".to_string(),
            payload: MessagePayload::BeliefUpdate {
                proposition: "test".to_string(),
                value: HexValue::Probable,
            },
        });

        assert_eq!(swarm.agent("bob").unwrap().inbox_len(), 1);
        assert_eq!(swarm.agent("alice").unwrap().inbox_len(), 0);
    }

    #[test]
    fn test_swarm_broadcast() {
        let mut swarm = Swarm::new();
        swarm.add_agent(make_agent("alice", 0.8));
        swarm.add_agent(make_agent("bob", 0.7));
        swarm.add_agent(make_agent("charlie", 0.6));

        swarm.send(Message {
            from: "alice".to_string(),
            to: "*".to_string(),
            payload: MessagePayload::Text("hello everyone".to_string()),
        });

        // Bob and Charlie should get the message, not Alice
        assert_eq!(swarm.agent("alice").unwrap().inbox_len(), 0);
        assert_eq!(swarm.agent("bob").unwrap().inbox_len(), 1);
        assert_eq!(swarm.agent("charlie").unwrap().inbox_len(), 1);
    }

    #[test]
    fn test_swarm_process_and_consensus() {
        let mut swarm = Swarm::new();

        let mut alice = make_agent("alice", 0.9);
        alice
            .beliefs
            .add_proposition("agi-possible", HexValue::True);

        let mut bob = make_agent("bob", 0.8);
        bob.beliefs
            .add_proposition("agi-possible", HexValue::Probable);

        let mut charlie = make_agent("charlie", 0.7);
        charlie
            .beliefs
            .add_proposition("agi-possible", HexValue::Probable);

        swarm.add_agent(alice);
        swarm.add_agent(bob);
        swarm.add_agent(charlie);

        let result = swarm.consensus("agi-possible");
        assert!(
            matches!(result.consensus, HexValue::True | HexValue::Probable),
            "Consensus should be positive, got {}",
            result.consensus
        );
        assert_eq!(result.votes.len(), 3);
    }
}
