//! # Project Hari — Cognitive-State Research Sandbox
//!
//! Main binary that demonstrates the cognitive loop with all subsystems.

use hari_core::{
    compare_replay, Action, CognitiveLoop, Perception, ResearchEvent, ResearchTrace,
};
use hari_lattice::HexValue;
use hari_swarm::{Agent, AgentRole, Message, MessagePayload, Swarm};
use std::{env, fs, process};
use tracing::{info, warn};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.get(1).map(String::as_str) == Some("replay") {
        // Parse trailing args: optional --compare flag, then trace path.
        // Accepts both `replay --compare <path>` and `replay <path>` for
        // backwards compatibility.
        let mut compare = false;
        let mut path: Option<&str> = None;
        for a in &args[2..] {
            match a.as_str() {
                "--compare" => compare = true,
                other if !other.starts_with("--") => path = Some(other),
                other => {
                    eprintln!("hari-core replay: unknown flag {other}");
                    process::exit(2);
                }
            }
        }
        if let Err(err) = replay_trace(path, compare) {
            eprintln!("hari-core replay failed: {err}");
            process::exit(1);
        }
        return;
    }

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .init();

    info!("=== Project Hari — Cognitive-State Research Sandbox ===");
    info!("Initializing cognitive systems...");

    // --- Initialize the cognitive loop ---
    let mut cognitive_loop = CognitiveLoop::new(4);

    // Set up goals
    cognitive_loop.state.add_goal(
        "understand-environment",
        "Build model of the environment",
        0.8,
    );
    cognitive_loop
        .state
        .add_goal("maintain-coherence", "Keep beliefs consistent", 0.9);

    info!(
        "Cognitive loop initialized: {}",
        cognitive_loop.state.summary()
    );

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
    swarm.add_agent(Agent::new(
        "guardian",
        AgentRole {
            name: "guardian".to_string(),
            self_trust: 0.95,
            message_trust: 0.4,
        },
    ));

    info!("Swarm initialized with {} agents", swarm.len());

    // --- Simulate 10 cognitive cycles ---

    // Perception schedule: simulate a rich environment with evolving signals
    let perceptions: Vec<(u64, &str, HexValue, &str)> = vec![
        (1, "environment-safe", HexValue::Probable, "initial-scan"),
        (2, "environment-safe", HexValue::Doubtful, "deep-scan"),
        (3, "resources-available", HexValue::True, "resource-scan"),
        (
            4,
            "agents-cooperative",
            HexValue::Probable,
            "swarm-observation",
        ),
        (5, "threat-detected", HexValue::Doubtful, "perimeter-scan"),
        (6, "threat-detected", HexValue::Probable, "secondary-scan"),
        (7, "resources-available", HexValue::True, "confirmed-scan"),
        (8, "agents-cooperative", HexValue::True, "consensus-check"),
        (9, "environment-safe", HexValue::Probable, "re-evaluation"),
        (10, "system-stable", HexValue::Probable, "self-diagnostic"),
    ];

    for cycle_num in 1..=10 {
        info!("\n--- Cycle {} ---", cycle_num);

        // Inject any perceptions scheduled for this cycle
        for (sched, prop, value, source) in &perceptions {
            if *sched == cycle_num {
                cognitive_loop.perceive(Perception {
                    proposition: prop.to_string(),
                    value: *value,
                    source: source.to_string(),
                    cycle: cycle_num,
                });

                // Broadcast to swarm
                swarm.send(Message {
                    from: "core".to_string(),
                    to: "*".to_string(),
                    payload: MessagePayload::BeliefUpdate {
                        proposition: prop.to_string(),
                        value: *value,
                    },
                });
            }
        }

        // Process swarm messages
        let swarm_updates = swarm.process_all();
        if swarm_updates > 0 {
            info!("  Swarm processed {} belief updates", swarm_updates);
        }

        // Run cognitive cycle
        let actions = cognitive_loop.cycle();
        for action in &actions {
            match action {
                Action::Escalate { reason, confidence } => {
                    warn!("  ESCALATION: {} (confidence: {:.2})", reason, confidence);
                }
                _ => info!("  Action: {}", action),
            }
        }

        info!("  State: {}", cognitive_loop.state.summary());
    }

    // --- Final Summary ---
    info!("\n=== Final State ===");
    info!("{}", cognitive_loop.state.summary());

    for prop_name in &[
        "environment-safe",
        "resources-available",
        "agents-cooperative",
        "threat-detected",
        "system-stable",
    ] {
        if let Some(belief) = cognitive_loop.state.beliefs.get(prop_name) {
            info!("  {}: {}", prop_name, belief.value);
        }
    }

    // Swarm consensus on key propositions
    info!("\n=== Swarm Consensus ===");
    for prop_name in &["environment-safe", "resources-available", "threat-detected"] {
        let result = swarm.consensus(prop_name);
        info!(
            "  {}: {} (agreement: {:.0}%)",
            prop_name,
            result.consensus,
            result.agreement * 100.0
        );
    }

    info!("\n=== Project Hari — 10 cycles complete ===");
}

fn replay_trace(path: Option<&str>, compare: bool) -> Result<(), Box<dyn std::error::Error>> {
    let path = path.ok_or("usage: hari-core replay [--compare] <trace.json>")?;
    let trace_json = fs::read_to_string(path)?;
    let trace = parse_trace(&trace_json)?;

    let report = if compare {
        compare_replay(trace)
    } else {
        let mut cognitive_loop = CognitiveLoop::new(trace.dimension);
        cognitive_loop.process_research_trace(trace)
    };
    serde_json::to_writer_pretty(std::io::stdout(), &report)?;
    println!();

    Ok(())
}

fn parse_trace(trace_json: &str) -> Result<ResearchTrace, serde_json::Error> {
    match serde_json::from_str::<ResearchTrace>(trace_json) {
        Ok(trace) => Ok(trace),
        Err(object_error) => match serde_json::from_str::<Vec<ResearchEvent>>(trace_json) {
            Ok(events) => Ok(events.into()),
            Err(_) => Err(object_error),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_trace_accepts_object_form() {
        let trace = parse_trace(
            r#"{
                "dimension": 6,
                "events": [
                    {
                        "cycle": 1,
                        "source": "ix-agent",
                        "payload": {
                            "type": "belief_update",
                            "proposition": "prompt-a-improves-pass-rate",
                            "value": "Probable"
                        }
                    }
                ]
            }"#,
        )
        .unwrap();

        assert_eq!(trace.dimension, 6);
        assert_eq!(trace.events.len(), 1);
    }

    #[test]
    fn replay_report_round_trips_new_optional_fields() {
        // The new `priority_model`, `metrics`, and `comparison` fields must
        // round-trip via serde and load as defaults from older fixtures
        // that don't include them.
        let mut cognitive_loop = CognitiveLoop::new(4);
        let trace = parse_trace(
            r#"[
                {
                    "cycle": 1,
                    "source": "ix-agent",
                    "payload": {
                        "type": "belief_update",
                        "proposition": "p",
                        "value": "Probable"
                    }
                }
            ]"#,
        )
        .unwrap();
        let report = cognitive_loop.process_research_trace(trace);
        let s = serde_json::to_string(&report).unwrap();
        let round_tripped: hari_core::ResearchReplayReport =
            serde_json::from_str(&s).unwrap();
        assert!(round_tripped.comparison.is_none());
        // Old fixtures without the new fields must still load — try a JSON
        // shape lacking them entirely.
        let legacy = r#"{
            "event_count": 0,
            "outcomes": [],
            "final_beliefs": {},
            "final_goals": {},
            "final_state_summary": "n/a"
        }"#;
        let loaded: hari_core::ResearchReplayReport = serde_json::from_str(legacy).unwrap();
        assert_eq!(loaded.event_count, 0);
        assert!(loaded.comparison.is_none());
    }

    #[test]
    fn parse_trace_accepts_array_form() {
        let trace = parse_trace(
            r#"[
                {
                    "cycle": 1,
                    "source": "ix-agent",
                    "payload": {
                        "type": "belief_update",
                        "proposition": "prompt-a-improves-pass-rate",
                        "value": "Probable"
                    }
                }
            ]"#,
        )
        .unwrap();

        assert_eq!(trace.dimension, 4);
        assert_eq!(trace.events.len(), 1);
    }
}
