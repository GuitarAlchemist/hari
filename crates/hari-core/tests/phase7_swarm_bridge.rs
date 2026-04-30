//! Phase 4-bridge — `AgentVote` events route through `hari-swarm`.
//!
//! These tests pin the wire-up between the IX research-event boundary
//! and the swarm crate's `TrustModel`. Coverage:
//!
//! 1. **Default preserves pre-bridge behavior.** With
//!    `use_swarm_consensus = false`, replaying `swarm_dissent.json`
//!    through `CognitiveLoop::new(4)` produces the same
//!    `ResearchReplayReport.outcomes` as a hand-rolled fresh loop —
//!    proving the bridge is opt-in and adds no behavior change at
//!    rest.
//! 2. **`AgentVote` always populates the swarm.** Even with bridging
//!    off, the swarm accumulates one auto-created agent per unique
//!    `event.source` so cross-cutting tools can query who voted what.
//! 3. **Bridging on (Equal mode) drives perception via consensus.**
//!    The cognitive loop's perceived value for a proposition under
//!    `use_swarm_consensus` matches `swarm.consensus_with(p, Equal)`,
//!    not the raw vote.
//! 4. **`RoleWeighted` with declared roles changes outcomes.** Same
//!    fixture, same config except `trust_model = RoleWeighted` plus
//!    `initial_agents` declaring lopsided roles, must produce a
//!    different action stream than `Equal` — the headline empirical
//!    claim of the bridge.

use hari_core::{
    CognitiveLoop, InitialAgent, ResearchEvent, ResearchTrace, SessionConfig, StreamingSession,
};
use hari_lattice::HexValue;
use hari_swarm::{AgentRole, TrustModel};
use std::fs;

fn load_swarm_dissent() -> ResearchTrace {
    let raw = fs::read_to_string("../../fixtures/ix/swarm_dissent.json")
        .expect("swarm_dissent fixture readable");
    serde_json::from_str(&raw).expect("fixture parses as ResearchTrace")
}

fn unique_sources(trace: &ResearchTrace) -> Vec<String> {
    let mut out: Vec<String> = trace
        .events
        .iter()
        .filter(|e| {
            matches!(
                &e.payload,
                hari_core::ResearchEventPayload::AgentVote { .. }
            )
        })
        .map(|e| e.source.clone())
        .collect();
    out.sort();
    out.dedup();
    out
}

// ---------------------------------------------------------------------------
// 1. Default-mode regression: bridge off → outcomes byte-equal to fresh loop
// ---------------------------------------------------------------------------

#[test]
fn default_mode_preserves_pre_bridge_outcomes_on_swarm_dissent() {
    let trace = load_swarm_dissent();

    // Fresh loop, no SessionConfig involvement.
    let mut bare = CognitiveLoop::new(trace.dimension);
    let bare_report = bare.process_research_trace(trace.clone());

    // Same fixture but constructed via SessionConfig::default() — which
    // sets `use_swarm_consensus = false`. Outcomes must match exactly.
    let mut session = StreamingSession::open(SessionConfig {
        dimension: trace.dimension,
        ..SessionConfig::default()
    })
    .expect("open default session");
    for ev in trace.events.iter() {
        session.apply_event(ev.clone()).expect("apply event");
    }
    let bridge_off_report = session.close();

    let bare_json = serde_json::to_string(&bare_report).unwrap();
    let bridge_off_json = serde_json::to_string(&bridge_off_report).unwrap();
    assert_eq!(
        bare_json, bridge_off_json,
        "use_swarm_consensus=false must be a no-op vs the bare CognitiveLoop"
    );
}

// ---------------------------------------------------------------------------
// 2. Swarm always populated, even when bridging is off
// ---------------------------------------------------------------------------

#[test]
fn agent_vote_populates_swarm_even_when_bridging_disabled() {
    let trace = load_swarm_dissent();
    let expected_sources = unique_sources(&trace);
    assert!(
        !expected_sources.is_empty(),
        "fixture sanity: swarm_dissent must contain agent votes"
    );

    // Bridging OFF — swarm should still mirror voters.
    let mut loop_ = CognitiveLoop::new(trace.dimension);
    let _ = loop_.process_research_trace(trace.clone());
    for src in &expected_sources {
        assert!(
            loop_.swarm.agent(src).is_some(),
            "swarm must have an auto-created entry for vote source {src}"
        );
    }
    // Sanity: no extra phantom agents.
    assert_eq!(loop_.swarm.len(), expected_sources.len());
}

// ---------------------------------------------------------------------------
// 3. Bridging on: perception value tracks swarm consensus
// ---------------------------------------------------------------------------

#[test]
fn bridging_on_perceives_swarm_consensus_not_raw_vote() {
    // Two votes on the same proposition from two distinct voters with
    // opposite views. Under TrustModel::Equal, the swarm consensus
    // after both votes lands on Contradictory (significant
    // disagreement), and that should be what the cognitive loop's main
    // belief network sees — NOT just the latest raw vote (`Doubtful`).
    let mut loop_ = CognitiveLoop::new(4);
    loop_.use_swarm_consensus = true;
    loop_.trust_model = TrustModel::Equal;

    let to_event = |cycle, src: &str, val| ResearchEvent {
        cycle,
        source: src.into(),
        payload: hari_core::ResearchEventPayload::AgentVote {
            proposition: "p".into(),
            value: val,
            evidence: Default::default(),
        },
    };

    loop_.process_research_event(to_event(1, "agent-a", HexValue::True));
    loop_.process_research_event(to_event(2, "agent-b", HexValue::False));

    // The swarm now has two agents holding opposing definite views;
    // the existing consensus algorithm's "significant disagreement"
    // check fires (positive=1 of 2 = 50% > 30%), yielding Contradictory.
    let final_belief = loop_
        .state
        .beliefs
        .get("p")
        .map(|p| p.value)
        .expect("belief must be set after AgentVote");
    assert_eq!(
        final_belief,
        HexValue::Contradictory,
        "main belief must reflect swarm consensus (Contradictory), not the raw last vote (False)"
    );
}

// ---------------------------------------------------------------------------
// 4. RoleWeighted with declared roles produces different actions vs Equal
// ---------------------------------------------------------------------------

#[test]
fn role_weighted_changes_outcomes_vs_equal_with_declared_initial_agents() {
    let trace = load_swarm_dissent();

    // Build two sessions: same fixture, same primary model, but Equal
    // vs RoleWeighted. Under RoleWeighted, declare a few "guardian"
    // voices with high self_trust and a few "explorer" voices with
    // low self_trust — the exact roles the fixture's `voter_role`
    // evidence labels reference.
    let lopsided_roles: Vec<InitialAgent> = vec![
        // High-trust skeptics
        InitialAgent {
            id: "ix-agent-guardian".into(),
            role: AgentRole {
                name: "guardian".into(),
                self_trust: 0.95,
                message_trust: 0.4,
            },
        },
        InitialAgent {
            id: "ix-agent-guardian-secondary".into(),
            role: AgentRole {
                name: "guardian".into(),
                self_trust: 0.95,
                message_trust: 0.4,
            },
        },
        InitialAgent {
            id: "ix-agent-guardian-tertiary".into(),
            role: AgentRole {
                name: "guardian".into(),
                self_trust: 0.95,
                message_trust: 0.4,
            },
        },
        InitialAgent {
            id: "ix-agent-critic".into(),
            role: AgentRole {
                name: "critic".into(),
                self_trust: 0.9,
                message_trust: 0.3,
            },
        },
        InitialAgent {
            id: "ix-agent-critic-secondary".into(),
            role: AgentRole {
                name: "critic".into(),
                self_trust: 0.9,
                message_trust: 0.3,
            },
        },
        InitialAgent {
            id: "ix-agent-critic-tertiary".into(),
            role: AgentRole {
                name: "critic".into(),
                self_trust: 0.9,
                message_trust: 0.3,
            },
        },
        // Low-trust optimists
        InitialAgent {
            id: "ix-agent-explorer".into(),
            role: AgentRole {
                name: "explorer".into(),
                self_trust: 0.3,
                message_trust: 0.8,
            },
        },
        InitialAgent {
            id: "ix-agent-explorer-secondary".into(),
            role: AgentRole {
                name: "explorer".into(),
                self_trust: 0.3,
                message_trust: 0.8,
            },
        },
        InitialAgent {
            id: "ix-agent-explorer-tertiary".into(),
            role: AgentRole {
                name: "explorer".into(),
                self_trust: 0.3,
                message_trust: 0.8,
            },
        },
    ];

    let common = SessionConfig {
        dimension: trace.dimension,
        use_swarm_consensus: true,
        initial_agents: lopsided_roles.clone(),
        ..SessionConfig::default()
    };

    // Equal
    let mut equal_session = StreamingSession::open(SessionConfig {
        trust_model: TrustModel::Equal,
        ..common.clone()
    })
    .expect("open Equal");
    for ev in trace.events.iter() {
        equal_session.apply_event(ev.clone()).expect("apply Equal");
    }
    let equal_report = equal_session.close();

    // RoleWeighted
    let mut weighted_session = StreamingSession::open(SessionConfig {
        trust_model: TrustModel::RoleWeighted,
        ..common
    })
    .expect("open RoleWeighted");
    for ev in trace.events.iter() {
        weighted_session
            .apply_event(ev.clone())
            .expect("apply RoleWeighted");
    }
    let weighted_report = weighted_session.close();

    // The action streams must not be identical — that's the whole
    // point of `TrustModel::RoleWeighted` being on the menu. Compare
    // the per-outcome action lists.
    let equal_actions: Vec<_> = equal_report
        .outcomes
        .iter()
        .map(|o| serde_json::to_string(&o.actions).unwrap())
        .collect();
    let weighted_actions: Vec<_> = weighted_report
        .outcomes
        .iter()
        .map(|o| serde_json::to_string(&o.actions).unwrap())
        .collect();

    assert_eq!(
        equal_actions.len(),
        weighted_actions.len(),
        "trust model must not drop or duplicate events"
    );
    assert_ne!(
        equal_actions, weighted_actions,
        "TrustModel::RoleWeighted must produce a different action stream than Equal \
         on swarm_dissent.json with the declared lopsided roles"
    );
}

// ---------------------------------------------------------------------------
// 5. Auto-creation: undeclared sources get a neutral-role agent
// ---------------------------------------------------------------------------

#[test]
fn undeclared_source_is_auto_created_with_neutral_role() {
    let mut loop_ = CognitiveLoop::new(4);
    loop_.use_swarm_consensus = true;
    loop_.trust_model = TrustModel::Equal;

    loop_.process_research_event(ResearchEvent {
        cycle: 1,
        source: "stranger".into(),
        payload: hari_core::ResearchEventPayload::AgentVote {
            proposition: "p".into(),
            value: HexValue::Probable,
            evidence: Default::default(),
        },
    });

    let agent = loop_
        .swarm
        .agent("stranger")
        .expect("undeclared source must be auto-created");
    assert_eq!(agent.role.name, "auto");
    assert!(
        (agent.role.self_trust - 0.5).abs() < 1e-12,
        "neutral self_trust"
    );
    assert!(
        (agent.role.message_trust - 0.5).abs() < 1e-12,
        "neutral message_trust"
    );
}
