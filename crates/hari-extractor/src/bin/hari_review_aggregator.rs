//! `hari-review-aggregator` — multi-reviewer consensus via `AgentVote` + swarm.
//!
//! ## What this is
//!
//! Takes a JSONL file of per-reviewer verdicts (one vote per line) and runs
//! `hari-swarm::Swarm::consensus_with` over them. The output is a single
//! `TribunalReport` per proposition: consensus `HexValue`, agreement ratio,
//! and the per-agent vote map. Genuine disagreement surfaces as
//! `HexValue::Contradictory` instead of being averaged away.
//!
//! ## Why this exists
//!
//! Boris Cherny's parallel-reviewer pattern (the `feedback_octopus_parallel_decisions`
//! memory in ix) currently emits free-text reviews that a human synthesizes.
//! This binary moves the synthesis into Hari's swarm-consensus substrate so
//! the verdict is structured, the disagreement is preserved, and the
//! `TribunalReport` becomes the kind of artifact an MCP `hari_query_belief`
//! call can pull back later.
//!
//! ## Input format (one JSON object per line)
//!
//! ```json
//! { "proposition": "pr-208-is-safe-to-merge",
//!   "agent_id": "claude-opus-4.7",
//!   "role": "critic",
//!   "self_trust": 0.7,
//!   "message_trust": 0.7,
//!   "value": "Probable",
//!   "rationale": "no breaking changes; flag default OFF" }
//! ```
//!
//! `agent_id` + `value` are required. `role` defaults to `"voter"` with
//! `self_trust = message_trust = 0.5`. `rationale` is passed through to
//! the report but not used in the math. `proposition` may be omitted on a
//! line if `--proposition` is given on the CLI.
//!
//! ## Usage
//!
//! ```pwsh
//! hari-review-aggregator --votes pr-208-votes.jsonl --trust-model RoleWeighted
//! hari-review-aggregator --votes votes.jsonl --proposition pr-208-is-safe-to-merge
//! ```
//!
//! Output: pretty JSON `TribunalReport[]` on stdout, one report per
//! distinct proposition. Exit non-zero if any consensus is
//! `HexValue::Contradictory`.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::ExitCode;

use hari_lattice::HexValue;
use hari_swarm::{Agent, AgentRole, Swarm, TrustModel};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
struct VoteLine {
    #[serde(default)]
    proposition: Option<String>,
    agent_id: String,
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    self_trust: Option<f64>,
    #[serde(default)]
    message_trust: Option<f64>,
    value: HexValue,
    #[serde(default)]
    rationale: Option<String>,
}

#[derive(Serialize)]
struct TribunalReport {
    proposition: String,
    consensus: HexValue,
    agreement: f64,
    votes: BTreeMap<String, HexValue>,
    rationales: BTreeMap<String, String>,
    trust_model: String,
    dissenting_agents: Vec<String>,
}

fn parse_args() -> Result<(PathBuf, String, Option<String>), String> {
    let mut votes_path: Option<PathBuf> = None;
    let mut trust_model = String::from("Equal");
    let mut proposition_override: Option<String> = None;

    let mut iter = std::env::args().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--votes" => votes_path = iter.next().map(PathBuf::from),
            "--trust-model" => {
                trust_model = iter.next().ok_or_else(|| "--trust-model expects a value".to_string())?;
            }
            "--proposition" => proposition_override = iter.next(),
            "-h" | "--help" => {
                print_usage();
                std::process::exit(0);
            }
            other => return Err(format!("unrecognized argument: {other}")),
        }
    }

    let votes_path = votes_path.ok_or_else(|| "--votes <path> is required".to_string())?;
    if !matches!(trust_model.as_str(), "Equal" | "RoleWeighted") {
        return Err(format!("--trust-model must be Equal or RoleWeighted, got {trust_model}"));
    }
    Ok((votes_path, trust_model, proposition_override))
}

fn print_usage() {
    eprintln!(
        "Usage: hari-review-aggregator --votes <path.jsonl>\n\
         \x20                          [--trust-model Equal|RoleWeighted]\n\
         \x20                          [--proposition <fallback>]\n\
         \n\
         Reads per-reviewer JSONL votes, runs hari-swarm consensus,\n\
         emits TribunalReport JSON on stdout. Exits non-zero if any\n\
         consensus is Contradictory."
    );
}

fn main() -> ExitCode {
    let (votes_path, trust_model_name, proposition_override) = match parse_args() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("error: {e}");
            print_usage();
            return ExitCode::from(2);
        }
    };

    let trust_model = match trust_model_name.as_str() {
        "Equal" => TrustModel::Equal,
        "RoleWeighted" => TrustModel::RoleWeighted,
        _ => unreachable!(),
    };

    let f = match std::fs::File::open(&votes_path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("error opening {}: {e}", votes_path.display());
            return ExitCode::from(1);
        }
    };

    // Group votes by proposition.
    let mut by_prop: BTreeMap<String, Vec<VoteLine>> = BTreeMap::new();
    for (i, line) in BufReader::new(f).lines().enumerate() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                eprintln!("error reading line {}: {e}", i + 1);
                return ExitCode::from(1);
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        let mut vote: VoteLine = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("warn: line {} skipped (parse: {e})", i + 1);
                continue;
            }
        };
        if vote.proposition.is_none() {
            vote.proposition = proposition_override.clone();
        }
        let prop = match &vote.proposition {
            Some(p) => p.clone(),
            None => {
                eprintln!("warn: line {} skipped (no proposition + no --proposition)", i + 1);
                continue;
            }
        };
        by_prop.entry(prop).or_default().push(vote);
    }

    if by_prop.is_empty() {
        eprintln!("error: no usable votes found");
        return ExitCode::from(1);
    }

    let mut reports: Vec<TribunalReport> = Vec::new();
    let mut any_contradictory = false;

    for (prop, votes) in by_prop {
        let mut swarm = Swarm::new();
        let mut rationales: BTreeMap<String, String> = BTreeMap::new();
        for v in &votes {
            let role = AgentRole {
                name: v.role.clone().unwrap_or_else(|| "voter".into()),
                self_trust: v.self_trust.unwrap_or(0.5),
                message_trust: v.message_trust.unwrap_or(0.5),
            };
            let mut agent = Agent::new(&v.agent_id, role);
            agent.beliefs.add_proposition(&prop, v.value);
            swarm.add_agent(agent);
            if let Some(r) = &v.rationale {
                rationales.insert(v.agent_id.clone(), r.clone());
            }
        }
        let result = swarm.consensus_with(&prop, trust_model);
        if result.consensus == HexValue::Contradictory {
            any_contradictory = true;
        }

        let votes_btree: BTreeMap<String, HexValue> = result.votes.into_iter().collect();
        let dissenting: Vec<String> = votes_btree
            .iter()
            .filter(|(_, v)| **v != result.consensus)
            .map(|(k, _)| k.clone())
            .collect();

        reports.push(TribunalReport {
            proposition: result.proposition,
            consensus: result.consensus,
            agreement: result.agreement,
            votes: votes_btree,
            rationales,
            trust_model: trust_model_name.clone(),
            dissenting_agents: dissenting,
        });
    }

    let out = serde_json::to_string_pretty(&reports).unwrap_or_else(|_| "[]".into());
    println!("{out}");

    if any_contradictory {
        ExitCode::from(2)
    } else {
        ExitCode::SUCCESS
    }
}
