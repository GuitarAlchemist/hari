//! # Project Hari — AGI Research Platform
//!
//! Main binary that demonstrates the cognitive loop with all subsystems.

use hari_core::{Action, CognitiveLoop, Perception};
use hari_lattice::HexValue;
use hari_swarm::{Agent, AgentRole, Message, MessagePayload, Swarm};
use tracing::{info, warn};

fn main() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .init();

    info!("=== Project Hari — AGI Research Platform ===");
    info!("Initializing cognitive systems...");

    // --- Initialize the cognitive loop ---
    let mut cognitive_loop = CognitiveLoop::new(4);

    // Set up goals
    cognitive_loop
        .state
        .add_goal("understand-environment", "Build model of the environment", 0.8);
    cognitive_loop
        .state
        .add_goal("maintain-coherence", "Keep beliefs consistent", 0.9);

    info!("Cognitive loop initialized: {}", cognitive_loop.state.summary());

    // --- Initialize the swarm ---
    let mut swarm = Swarm::new();

    swarm.add_agent(Agent::new(
        "explorer",
        AgentRole {
            name: "explorer".to_string(),
            self_trust: 0.7,
            message_trust: 0.6,
        },
    ));
    swarm.add_agent(Agent::new(
        "critic",
        AgentRole {
            name: "critic".to_string(),
            self_trust: 0.9,
            message_trust: 0.3,
        },
    ));
    swarm.add_agent(Agent::new(
        "integrator",
        AgentRole {
            name: "integrator".to_string(),
            self_trust: 0.5,
            message_trust: 0.8,
        },
    ));

    info!("Swarm initialized with {} agents", swarm.len());

    // --- Simulate a few cognitive cycles ---

    // Cycle 1: Perceive something
    info!("\n--- Cycle 1: Initial perception ---");
    cognitive_loop.perceive(Perception {
        proposition: "environment-safe".to_string(),
        value: HexValue::Probable,
        source: "initial-scan".to_string(),
        cycle: 0,
    });

    let actions = cognitive_loop.cycle();
    for action in &actions {
        info!("  Action: {}", action);
    }

    // Broadcast the perception to the swarm
    swarm.send(Message {
        from: "core".to_string(),
        to: "*".to_string(),
        payload: MessagePayload::BeliefUpdate {
            proposition: "environment-safe".to_string(),
            value: HexValue::Probable,
        },
    });
    swarm.process_all();

    // Cycle 2: Conflicting perception
    info!("\n--- Cycle 2: Conflicting signal ---");
    cognitive_loop.perceive(Perception {
        proposition: "environment-safe".to_string(),
        value: HexValue::Doubtful,
        source: "deep-scan".to_string(),
        cycle: 1,
    });

    let actions = cognitive_loop.cycle();
    for action in &actions {
        match action {
            Action::Escalate { reason, confidence } => {
                warn!("  ESCALATION: {} (confidence: {:.2})", reason, confidence);
            }
            _ => info!("  Action: {}", action),
        }
    }

    // Check swarm consensus
    let consensus = swarm.consensus("environment-safe");
    info!(
        "  Swarm consensus on 'environment-safe': {} (agreement: {:.0}%)",
        consensus.consensus,
        consensus.agreement * 100.0
    );

    // Cycle 3: New information
    info!("\n--- Cycle 3: New topic ---");
    cognitive_loop.perceive(Perception {
        proposition: "resources-available".to_string(),
        value: HexValue::True,
        source: "resource-scan".to_string(),
        cycle: 2,
    });

    let actions = cognitive_loop.cycle();
    for action in &actions {
        info!("  Action: {}", action);
    }

    // Final state
    info!("\n--- Final State ---");
    info!("{}", cognitive_loop.state.summary());

    if let Some(belief) = cognitive_loop.state.beliefs.get("environment-safe") {
        info!("  environment-safe: {}", belief.value);
    }
    if let Some(belief) = cognitive_loop.state.beliefs.get("resources-available") {
        info!("  resources-available: {}", belief.value);
    }

    info!("\n=== Project Hari — Cycle complete ===");
}
