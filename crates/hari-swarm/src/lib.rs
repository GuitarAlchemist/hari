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
// TrustModel — Phase 4: how `AgentRole::self_trust` and `message_trust`
// are applied during belief integration and consensus
// ---------------------------------------------------------------------------

/// How the swarm uses each agent's trust parameters.
///
/// Two modes, both A/B-able against each other on the same scenario:
///
/// - [`TrustModel::Equal`] — current behavior preserved bit-for-bit.
///   Trust fields on `AgentRole` are stored but ignored. One agent, one
///   vote. Every message is integrated. **Default.**
/// - [`TrustModel::RoleWeighted`] — Phase 4 trust-aware behavior.
///   Consensus weights each vote by the voter's `self_trust`. Inbox
///   integration drops messages whose recipient `message_trust` is below
///   [`MESSAGE_TRUST_THRESHOLD`] — those are surfaced separately as a
///   "minority report" count via [`InboxStats::filtered`].
///
/// The threshold and the weighting are intentionally simple and explicit
/// (not learned, not scenario-adaptive) so the difference between modes
/// is attributable to one knob, not to a black-box policy. Phase 4's
/// "track source reliability over repeated scenarios" sub-task is
/// deliberately deferred — it needs scenario-replay infrastructure that
/// the swarm crate alone does not own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TrustModel {
    /// Trust fields ignored; equal-weight one-vote-per-agent. Default.
    #[default]
    Equal,
    /// Consensus weighted by `self_trust`; inbox messages dropped when
    /// `message_trust < MESSAGE_TRUST_THRESHOLD`.
    RoleWeighted,
}

/// Threshold above which a message is integrated under
/// [`TrustModel::RoleWeighted`]. Pinned by `message_trust_threshold_is_pinned`.
pub const MESSAGE_TRUST_THRESHOLD: f64 = 0.5;

/// Outcome of a single agent processing its inbox under a given
/// `TrustModel`. `applied + filtered <= number_of_messages_received`
/// (queries and text messages are neither applied nor filtered).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InboxStats {
    /// Belief updates that landed.
    pub applied: usize,
    /// Belief-bearing messages that were dropped because the recipient's
    /// `message_trust` was below the threshold (only ever non-zero under
    /// `TrustModel::RoleWeighted`).
    pub filtered: usize,
}

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
    /// Trust-blind. Returns the number of belief updates made. Equivalent
    /// to `self.process_inbox_with(TrustModel::Equal).applied`.
    pub fn process_inbox(&mut self) -> usize {
        self.process_inbox_with(TrustModel::Equal).applied
    }

    /// Process all messages in the inbox under the given [`TrustModel`].
    ///
    /// Under [`TrustModel::Equal`] every belief-bearing message is
    /// integrated (current behavior). Under [`TrustModel::RoleWeighted`],
    /// belief-bearing messages are dropped without update when the
    /// recipient's `message_trust` is below [`MESSAGE_TRUST_THRESHOLD`];
    /// each dropped message increments [`InboxStats::filtered`].
    ///
    /// Query and text messages are neither applied nor filtered — they
    /// don't touch beliefs in either mode.
    pub fn process_inbox_with(&mut self, model: TrustModel) -> InboxStats {
        let messages: Vec<Message> = self.inbox.drain(..).collect();
        let mut stats = InboxStats::default();
        let trust_gates = matches!(model, TrustModel::RoleWeighted)
            && self.role.message_trust < MESSAGE_TRUST_THRESHOLD;

        for msg in messages {
            match msg.payload {
                MessagePayload::BeliefUpdate { proposition, value } => {
                    if trust_gates {
                        stats.filtered += 1;
                        continue;
                    }
                    if let Some(prop) = self.beliefs.get_mut(&proposition) {
                        let combined = HexLattice::combine_evidence(prop.value, value);
                        if combined != prop.value {
                            prop.value = combined;
                            stats.applied += 1;
                        }
                    } else {
                        // New proposition — add it with the received value
                        self.beliefs.add_proposition(proposition, value);
                        stats.applied += 1;
                    }
                }
                MessagePayload::BeliefResponse { proposition, value } => {
                    if trust_gates {
                        stats.filtered += 1;
                        continue;
                    }
                    if let Some(prop) = self.beliefs.get_mut(&proposition) {
                        let combined = HexLattice::combine_evidence(prop.value, value);
                        if combined != prop.value {
                            prop.value = combined;
                            stats.applied += 1;
                        }
                    }
                }
                // Queries and text don't update beliefs directly
                _ => {}
            }
        }

        stats
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

/// Trust-weighted variant of [`compute_consensus`].
///
/// Same algorithm, but every count is replaced by a sum of the voter's
/// weight from `weights`. Voters not present in `weights` get weight 0
/// (they don't influence the outcome). When all weights in `weights` are
/// equal and non-zero across the voters in `votes`, this MUST return the
/// same value as [`compute_consensus`] — the regression invariant is
/// pinned by `weighted_consensus_matches_unweighted_when_uniform`.
///
/// The weighted disagreement check uses 0.3 of total weight as the
/// "significant minority" threshold, mirroring the unweighted version's
/// 0.3 of total count. The 0.875 / 0.625 / 0.375 / 0.125 confidence
/// boundaries are unchanged.
pub fn compute_consensus_weighted(
    votes: &HashMap<String, HexValue>,
    weights: &HashMap<String, f64>,
) -> HexValue {
    if votes.is_empty() {
        return HexValue::Unknown;
    }
    let weight_of = |k: &str| -> f64 { weights.get(k).copied().unwrap_or(0.0).max(0.0) };
    let total_weight: f64 = votes.keys().map(|k| weight_of(k)).sum();
    if total_weight == 0.0 {
        // Pathological: no voter has any weight. Fall back to unweighted
        // so callers always get a defined result rather than a panic.
        return compute_consensus(votes);
    }

    // Contradictory majority by weight.
    let c_weight: f64 = votes
        .iter()
        .filter(|(_, v)| **v == HexValue::Contradictory)
        .map(|(k, _)| weight_of(k))
        .sum();
    if c_weight / total_weight > 0.5 {
        return HexValue::Contradictory;
    }

    // Significant disagreement by weight.
    let positive_weight: f64 = votes
        .iter()
        .filter(|(_, v)| matches!(**v, HexValue::True | HexValue::Probable))
        .map(|(k, _)| weight_of(k))
        .sum();
    let negative_weight: f64 = votes
        .iter()
        .filter(|(_, v)| matches!(**v, HexValue::False | HexValue::Doubtful))
        .map(|(k, _)| weight_of(k))
        .sum();
    if positive_weight > 0.0 && negative_weight > 0.0 {
        let min_faction = positive_weight.min(negative_weight);
        if min_faction / total_weight > 0.3 {
            return HexValue::Contradictory;
        }
    }

    // Weighted center of mass on definite (non-Contradictory) votes.
    let mut def_weight_sum = 0.0_f64;
    let mut def_conf_weighted_sum = 0.0_f64;
    for (k, v) in votes.iter().filter(|(_, v)| v.is_definite()) {
        let w = weight_of(k);
        def_weight_sum += w;
        def_conf_weighted_sum += w * v.confidence();
    }
    if def_weight_sum == 0.0 {
        return HexValue::Contradictory;
    }
    let avg_confidence = def_conf_weighted_sum / def_weight_sum;

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

    /// All agents process their inboxes (trust-blind). Returns total
    /// number of belief updates. Equivalent to
    /// `self.process_all_with(TrustModel::Equal).applied`.
    pub fn process_all(&mut self) -> usize {
        self.process_all_with(TrustModel::Equal).applied
    }

    /// All agents process their inboxes under the given [`TrustModel`].
    /// Returns the summed [`InboxStats`] across the swarm.
    pub fn process_all_with(&mut self, model: TrustModel) -> InboxStats {
        let mut total = InboxStats::default();
        for agent in self.agents.values_mut() {
            let s = agent.process_inbox_with(model);
            total.applied += s.applied;
            total.filtered += s.filtered;
        }
        total
    }

    /// Run a consensus vote on a proposition across all agents
    /// (trust-blind, equal-weight). Equivalent to
    /// `self.consensus_with(proposition, TrustModel::Equal)`.
    pub fn consensus(&self, proposition: &str) -> ConsensusResult {
        self.consensus_with(proposition, TrustModel::Equal)
    }

    /// Run a consensus vote on a proposition under the given
    /// [`TrustModel`].
    ///
    /// Under [`TrustModel::Equal`] every agent contributes one
    /// equally-weighted vote (current behavior). Under
    /// [`TrustModel::RoleWeighted`] each vote is weighted by the voter's
    /// `self_trust` from `AgentRole` and the consensus is computed via
    /// [`compute_consensus_weighted`]. The reported `agreement` ratio in
    /// either mode is the fraction of voters whose raw vote matches the
    /// final consensus value — agreement is a *count*, not a weight,
    /// because callers asking "how unanimous?" usually want a head count.
    pub fn consensus_with(&self, proposition: &str, model: TrustModel) -> ConsensusResult {
        let mut votes = HashMap::new();
        for (id, agent) in &self.agents {
            votes.insert(id.clone(), agent.vote(proposition));
        }

        let consensus = match model {
            TrustModel::Equal => compute_consensus(&votes),
            TrustModel::RoleWeighted => {
                let weights: HashMap<String, f64> = self
                    .agents
                    .iter()
                    .map(|(id, agent)| (id.clone(), agent.role.self_trust))
                    .collect();
                compute_consensus_weighted(&votes, &weights)
            }
        };

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

    // -- Phase 4: trust-aware swarm --

    /// Build a swarm with one high-trust "guardian" voting True and three
    /// low-trust "integrators" voting False. Used by both
    /// equal-vs-weighted-consensus tests.
    fn dissenting_guardian_swarm() -> Swarm {
        let mut s = Swarm::new();
        let mut guardian = Agent::new(
            "guardian",
            AgentRole {
                name: "guardian".into(),
                self_trust: 0.95,
                message_trust: 0.4,
            },
        );
        guardian.beliefs.add_proposition("p", HexValue::True);
        s.add_agent(guardian);

        for id in ["int-1", "int-2", "int-3"] {
            let mut a = Agent::new(
                id,
                AgentRole {
                    name: "integrator".into(),
                    self_trust: 0.4,
                    message_trust: 0.8,
                },
            );
            a.beliefs.add_proposition("p", HexValue::False);
            s.add_agent(a);
        }
        s
    }

    #[test]
    fn role_weighted_consensus_diverges_from_equal_when_trust_is_lopsided() {
        // Exit criterion 1 from ROADMAP Phase 4: "Agent roles change
        // outcomes in measurable ways."
        let swarm = dissenting_guardian_swarm();

        let equal = swarm.consensus_with("p", TrustModel::Equal).consensus;
        let weighted = swarm
            .consensus_with("p", TrustModel::RoleWeighted)
            .consensus;

        // Equal: 1 True + 3 False → minority faction = 0.25 of count, NOT
        // > 0.3 → falls through to confidence avg = (1+0+0+0)/4 = 0.25 →
        // Doubtful.
        assert_eq!(equal, HexValue::Doubtful, "equal-weighted baseline");

        // Weighted: positive_weight = 0.95, negative_weight = 1.2,
        // total = 2.15. min_faction / total = 0.95 / 2.15 ≈ 0.442 > 0.3
        // → Contradictory. The guardian's veto is loud enough to flip
        // the outcome from "Doubtful" to "irreconcilable" — exactly the
        // role-aware behaviour the milestone calls for.
        assert_eq!(
            weighted,
            HexValue::Contradictory,
            "role-weighted should escalate to Contradictory (guardian dissent ≥ 30% of weight)"
        );

        assert_ne!(equal, weighted, "TrustModel must change the outcome");
    }

    #[test]
    fn weighted_consensus_matches_unweighted_when_uniform() {
        // Regression invariant: with all weights equal and non-zero,
        // compute_consensus_weighted MUST match compute_consensus across
        // every fixture covered by the existing consensus tests.
        let fixtures: Vec<HashMap<String, HexValue>> = vec![
            // unanimous true
            [
                ("a", HexValue::True),
                ("b", HexValue::True),
                ("c", HexValue::True),
            ]
            .iter()
            .map(|(k, v)| (k.to_string(), *v))
            .collect(),
            // unanimous false
            [("a", HexValue::False), ("b", HexValue::False)]
                .iter()
                .map(|(k, v)| (k.to_string(), *v))
                .collect(),
            // mixed positive
            [
                ("a", HexValue::True),
                ("b", HexValue::Probable),
                ("c", HexValue::Probable),
            ]
            .iter()
            .map(|(k, v)| (k.to_string(), *v))
            .collect(),
            // strong disagreement
            [
                ("a", HexValue::True),
                ("b", HexValue::True),
                ("c", HexValue::False),
                ("d", HexValue::False),
            ]
            .iter()
            .map(|(k, v)| (k.to_string(), *v))
            .collect(),
            // all unknown
            [("a", HexValue::Unknown), ("b", HexValue::Unknown)]
                .iter()
                .map(|(k, v)| (k.to_string(), *v))
                .collect(),
            // contradictory majority
            [
                ("a", HexValue::Contradictory),
                ("b", HexValue::Contradictory),
                ("c", HexValue::True),
            ]
            .iter()
            .map(|(k, v)| (k.to_string(), *v))
            .collect(),
        ];

        for (i, votes) in fixtures.iter().enumerate() {
            let weights: HashMap<String, f64> = votes.keys().map(|k| (k.clone(), 1.0)).collect();
            let unweighted = compute_consensus(votes);
            let weighted = compute_consensus_weighted(votes, &weights);
            assert_eq!(
                unweighted, weighted,
                "fixture {i} must produce identical results under uniform weights"
            );
        }
    }

    #[test]
    fn role_weighted_inbox_filters_below_threshold_and_keeps_above() {
        // Exit criterion 2 from ROADMAP Phase 4: trust must be
        // observable. A skeptic with message_trust < 0.5 should drop
        // belief-bearing messages under RoleWeighted but accept them
        // under Equal.
        let make = |mt: f64| {
            Agent::new(
                "skeptic",
                AgentRole {
                    name: "skeptic".into(),
                    self_trust: 0.95,
                    message_trust: mt,
                },
            )
        };
        let msg = || Message {
            from: "noise".into(),
            to: "skeptic".into(),
            payload: MessagePayload::BeliefUpdate {
                proposition: "rumour".into(),
                value: HexValue::Probable,
            },
        };

        // Equal mode: low message_trust ignored, message lands.
        let mut a = make(0.2);
        a.receive(msg());
        let stats = a.process_inbox_with(TrustModel::Equal);
        assert_eq!(
            stats.applied, 1,
            "Equal mode must apply regardless of trust"
        );
        assert_eq!(stats.filtered, 0);
        assert_eq!(a.vote("rumour"), HexValue::Probable);

        // RoleWeighted mode: low message_trust filters the message.
        let mut a = make(0.2);
        a.receive(msg());
        let stats = a.process_inbox_with(TrustModel::RoleWeighted);
        assert_eq!(
            stats.applied, 0,
            "RoleWeighted must drop low-trust messages"
        );
        assert_eq!(stats.filtered, 1, "minority-report counter must increment");
        assert_eq!(a.vote("rumour"), HexValue::Unknown, "belief untouched");

        // RoleWeighted, sufficient trust: message lands.
        let mut a = make(0.8);
        a.receive(msg());
        let stats = a.process_inbox_with(TrustModel::RoleWeighted);
        assert_eq!(stats.applied, 1);
        assert_eq!(stats.filtered, 0);
    }

    #[test]
    fn message_trust_threshold_is_pinned() {
        // The 0.5 boundary is a constant; the test pins both sides.
        // Pin: at exactly MESSAGE_TRUST_THRESHOLD a message is accepted
        // (>=, not >). Just below it, the message is filtered.
        let make = |mt: f64| {
            let mut a = Agent::new(
                "x",
                AgentRole {
                    name: "x".into(),
                    self_trust: 0.5,
                    message_trust: mt,
                },
            );
            a.receive(Message {
                from: "y".into(),
                to: "x".into(),
                payload: MessagePayload::BeliefUpdate {
                    proposition: "q".into(),
                    value: HexValue::True,
                },
            });
            a
        };

        let mut at_threshold = make(MESSAGE_TRUST_THRESHOLD);
        let s = at_threshold.process_inbox_with(TrustModel::RoleWeighted);
        assert_eq!(s.applied, 1, "exactly at threshold must apply (>=, not >)");
        assert_eq!(s.filtered, 0);

        let mut just_below = make(MESSAGE_TRUST_THRESHOLD - 1e-9);
        let s = just_below.process_inbox_with(TrustModel::RoleWeighted);
        assert_eq!(s.applied, 0, "just below threshold must filter");
        assert_eq!(s.filtered, 1);
    }

    #[test]
    fn equal_mode_preserves_existing_process_inbox_signature_and_behavior() {
        // Regression: process_inbox() (no _with) must behave exactly as
        // pre-Phase-4. This pins the back-compat contract.
        let mut a = Agent::new(
            "x",
            AgentRole {
                name: "x".into(),
                self_trust: 0.95,
                // message_trust deliberately below threshold — would be
                // filtered under RoleWeighted, but must NOT be filtered
                // under Equal (which is what process_inbox() implies).
                message_trust: 0.1,
            },
        );
        a.receive(Message {
            from: "y".into(),
            to: "x".into(),
            payload: MessagePayload::BeliefUpdate {
                proposition: "q".into(),
                value: HexValue::True,
            },
        });
        let n: usize = a.process_inbox();
        assert_eq!(n, 1, "process_inbox() must remain trust-blind");
        assert_eq!(a.vote("q"), HexValue::True);
    }

    #[test]
    fn agreement_ratio_remains_a_head_count_under_role_weighted() {
        // Agreement is documented as fraction-of-voters-that-match, not
        // fraction-of-weight. Pin that contract: in the lopsided fixture,
        // weighted consensus is Contradictory but only 0 of 4 raw votes
        // are Contradictory, so agreement should be 0.0 — proving the
        // count-based interpretation.
        let swarm = dissenting_guardian_swarm();
        let result = swarm.consensus_with("p", TrustModel::RoleWeighted);
        assert_eq!(result.consensus, HexValue::Contradictory);
        assert!(
            (result.agreement - 0.0).abs() < 1e-12,
            "agreement is a head count of voters matching the consensus, not a weight share"
        );
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
