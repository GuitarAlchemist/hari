//! `hari-mcp` — MCP server exposing Hari's belief state to the federation.
//!
//! Implements the Model Context Protocol over stdio JSON-RPC 2.0, mirroring
//! ix-agent's hand-rolled dispatcher pattern. Eight tools are exposed:
//!
//! | tool                       | purpose                                                       |
//! |----------------------------|---------------------------------------------------------------|
//! | `hari_query_belief`        | Read one proposition's current HexValue from the snapshot     |
//! | `hari_snapshot`            | Return the full `{ proposition → HexValue }` snapshot          |
//! | `hari_diff`                | Return the last `belief-diff.json` (added/changed/unchanged)   |
//! | `hari_record_observation`  | Append a ResearchEvent to the log and replay                  |
//! | `hari_consensus`           | Run hari-swarm consensus over a set of inline AgentVotes      |
//! | `hari_session_open`        | Open a live `StreamingSession` (Phase 6) and return its id     |
//! | `hari_session_event`       | Apply one ResearchEvent to a named session, return recommendation |
//! | `hari_session_close`       | Close the session and return its final `ResearchReplayReport`  |
//!
//! All state lives under a configurable `state-dir` (default
//! `state/harness`) — the same directory `hari-harness` reads/writes.
//! That means /loop schedules and MCP-driven sessions share institutional
//! memory.
//!
//! ## Why hand-rolled JSON-RPC
//!
//! ix-agent's MCP server is hand-rolled (see `crates/ix-agent/src/main.rs`).
//! Pulling in `rmcp` here would force the entire workspace onto a heavier
//! async stack we don't otherwise need. The wire surface is small enough
//! that ~300 lines of explicit JSON-RPC is the right tradeoff.

use std::collections::{BTreeMap, HashMap};
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use hari_core::{
    Action, CognitiveLoop, Evidence, ResearchEvent, ResearchEventPayload, ResearchTrace,
    SessionConfig, StreamingSession,
};
use hari_lattice::HexValue;
use hari_swarm::{Agent, AgentRole, Swarm, TrustModel};
use serde::Deserialize;
use serde_json::{json, Value};

const JSONRPC_PARSE_ERROR: i64 = -32700;
const JSONRPC_INVALID_REQUEST: i64 = -32600;
const JSONRPC_METHOD_NOT_FOUND: i64 = -32601;
const JSONRPC_INVALID_PARAMS: i64 = -32602;
const JSONRPC_INTERNAL_ERROR: i64 = -32603;

const PROTOCOL_VERSION: &str = "2024-11-05";

fn state_dir_from_env() -> PathBuf {
    std::env::var("HARI_STATE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("state/harness"))
}

/// In-process registry of live streaming sessions, keyed by `session_id`.
/// Lives in `main` and threaded through `handle_tools_call`. The MCP
/// reader is single-threaded, so a plain `HashMap` is enough — no Mutex.
type SessionMap = HashMap<String, StreamingSession>;

fn main() {
    let state_dir = state_dir_from_env();
    eprintln!("[hari-mcp] MCP server starting; state-dir={}", state_dir.display());

    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut sessions: SessionMap = HashMap::new();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                eprintln!("[hari-mcp] stdin read error: {e}");
                break;
            }
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let value: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => {
                write_error(&mut stdout, Value::Null, JSONRPC_PARSE_ERROR, format!("parse error: {e}"));
                continue;
            }
        };
        let id = value.get("id").cloned().unwrap_or(Value::Null);
        let method = value.get("method").and_then(|v| v.as_str()).unwrap_or("");
        let params = value.get("params").cloned();
        eprintln!("[hari-mcp] method={method}");

        match method {
            "initialize" => write_response(&mut stdout, id, handle_initialize()),
            "notifications/initialized" => { /* notification — no response */ }
            "tools/list" => write_response(&mut stdout, id, handle_tools_list()),
            "tools/call" => match handle_tools_call(params, &state_dir, &mut sessions) {
                Ok(v) => write_response(&mut stdout, id, v),
                Err((code, msg)) => write_error(&mut stdout, id, code, msg),
            },
            "" => write_error(&mut stdout, id, JSONRPC_INVALID_REQUEST, "missing method".into()),
            other => write_error(
                &mut stdout,
                id,
                JSONRPC_METHOD_NOT_FOUND,
                format!("method not found: {other}"),
            ),
        }
    }
}

fn write_response(stdout: &mut io::Stdout, id: Value, result: Value) {
    let resp = json!({ "jsonrpc": "2.0", "id": id, "result": result });
    let _ = writeln!(stdout, "{}", resp);
    let _ = stdout.flush();
}

fn write_error(stdout: &mut io::Stdout, id: Value, code: i64, message: String) {
    let resp = json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    });
    let _ = writeln!(stdout, "{}", resp);
    let _ = stdout.flush();
}

fn handle_initialize() -> Value {
    json!({
        "protocolVersion": PROTOCOL_VERSION,
        "capabilities": { "tools": {} },
        "serverInfo": { "name": "hari-mcp", "version": env!("CARGO_PKG_VERSION") }
    })
}

fn handle_tools_list() -> Value {
    json!({
        "tools": [
            {
                "name": "hari_query_belief",
                "description": "Return the current HexValue for one proposition from the latest snapshot, or 'Unknown' if absent.",
                "inputSchema": {
                    "type": "object",
                    "properties": { "proposition": { "type": "string" } },
                    "required": ["proposition"]
                }
            },
            {
                "name": "hari_snapshot",
                "description": "Return the entire { proposition → HexValue } snapshot the harness most recently wrote.",
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "hari_diff",
                "description": "Return the last belief-diff (added/changed/unchanged) since the previous harness run.",
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "hari_record_observation",
                "description": "Append one ResearchEvent (belief_update | experiment_result | agent_vote | retraction) to the event log and replay. Returns the resulting snapshot + any escalations.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "source": { "type": "string" },
                        "kind": { "type": "string", "enum": ["belief_update", "experiment_result", "agent_vote"] },
                        "proposition": { "type": "string" },
                        "value": { "type": "string", "enum": ["True", "Probable", "Unknown", "Doubtful", "False", "Contradictory"] },
                        "evidence": { "type": "object" }
                    },
                    "required": ["source", "kind", "proposition", "value"]
                }
            },
            {
                "name": "hari_consensus",
                "description": "Run hari-swarm consensus over a list of inline AgentVotes. Each vote is { agent_id, role?, value }. Returns the consensus HexValue, agreement ratio, and per-agent vote map.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "proposition": { "type": "string" },
                        "trust_model": { "type": "string", "enum": ["Equal", "RoleWeighted"] },
                        "votes": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "agent_id": { "type": "string" },
                                    "role": { "type": "string" },
                                    "self_trust": { "type": "number" },
                                    "message_trust": { "type": "number" },
                                    "value": { "type": "string" }
                                },
                                "required": ["agent_id", "value"]
                            }
                        }
                    },
                    "required": ["proposition", "votes"]
                }
            },
            {
                "name": "hari_session_open",
                "description": "Open a live Phase-6 StreamingSession. Returns the session_id which must be passed to hari_session_event and hari_session_close. The `config` argument is a partial SessionConfig — all fields optional; missing fields use defaults (dimension=4, RecencyDecay, etc.).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "config": { "type": "object", "description": "Partial SessionConfig JSON" }
                    }
                }
            },
            {
                "name": "hari_session_event",
                "description": "Apply one ResearchEvent to a live session. Returns the RecommendationResponse: action list, state summary, and any propagation derivations. Out-of-order cycles or closed sessions return MCP errors.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "session_id": { "type": "string" },
                        "event": { "type": "object", "description": "ResearchEvent { cycle, source, payload }" }
                    },
                    "required": ["session_id", "event"]
                }
            },
            {
                "name": "hari_session_close",
                "description": "Close a live session and return its final ResearchReplayReport (same shape `hari-core replay` emits). The session is removed from the in-memory map after closing.",
                "inputSchema": {
                    "type": "object",
                    "properties": { "session_id": { "type": "string" } },
                    "required": ["session_id"]
                }
            }
        ]
    })
}

fn handle_tools_call(
    params: Option<Value>,
    state_dir: &Path,
    sessions: &mut SessionMap,
) -> Result<Value, (i64, String)> {
    let params = params.ok_or((JSONRPC_INVALID_PARAMS, "tools/call missing params".into()))?;
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or((JSONRPC_INVALID_PARAMS, "tools/call missing name".into()))?;
    let args = params.get("arguments").cloned().unwrap_or(json!({}));

    let result_value = match name {
        "hari_query_belief" => tool_query_belief(&args, state_dir)?,
        "hari_snapshot" => tool_snapshot(state_dir)?,
        "hari_diff" => tool_diff(state_dir)?,
        "hari_record_observation" => tool_record_observation(&args, state_dir)?,
        "hari_consensus" => tool_consensus(&args)?,
        "hari_session_open" => tool_session_open(&args, sessions)?,
        "hari_session_event" => tool_session_event(&args, sessions)?,
        "hari_session_close" => tool_session_close(&args, sessions)?,
        other => {
            return Err((
                JSONRPC_METHOD_NOT_FOUND,
                format!("unknown tool: {other}"),
            ));
        }
    };

    Ok(json!({
        "content": [{ "type": "text", "text": serde_json::to_string_pretty(&result_value).unwrap_or_default() }],
        "isError": false,
        "structuredContent": result_value
    }))
}

// ---------------------------------------------------------------------------
// Tool handlers
// ---------------------------------------------------------------------------

fn read_snapshot(state_dir: &Path) -> Result<BTreeMap<String, HexValue>, (i64, String)> {
    let path = state_dir.join("belief-snapshot.json");
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let s = std::fs::read_to_string(&path)
        .map_err(|e| (JSONRPC_INTERNAL_ERROR, format!("read snapshot: {e}")))?;
    serde_json::from_str(&s)
        .map_err(|e| (JSONRPC_INTERNAL_ERROR, format!("parse snapshot: {e}")))
}

fn tool_query_belief(args: &Value, state_dir: &Path) -> Result<Value, (i64, String)> {
    let prop = args
        .get("proposition")
        .and_then(Value::as_str)
        .ok_or((JSONRPC_INVALID_PARAMS, "missing proposition".into()))?;
    let snap = read_snapshot(state_dir)?;
    let value = snap.get(prop).copied().unwrap_or(HexValue::Unknown);
    Ok(json!({ "proposition": prop, "value": value }))
}

fn tool_snapshot(state_dir: &Path) -> Result<Value, (i64, String)> {
    let snap = read_snapshot(state_dir)?;
    Ok(serde_json::to_value(snap).unwrap_or(json!({})))
}

fn tool_diff(state_dir: &Path) -> Result<Value, (i64, String)> {
    let path = state_dir.join("belief-diff.json");
    if !path.exists() {
        return Ok(json!({ "added": {}, "changed": {}, "unchanged": 0, "previous_count": 0, "current_count": 0 }));
    }
    let s = std::fs::read_to_string(&path)
        .map_err(|e| (JSONRPC_INTERNAL_ERROR, format!("read diff: {e}")))?;
    serde_json::from_str(&s).map_err(|e| (JSONRPC_INTERNAL_ERROR, format!("parse diff: {e}")))
}

fn tool_record_observation(args: &Value, state_dir: &Path) -> Result<Value, (i64, String)> {
    #[derive(Deserialize)]
    struct ObservationArgs {
        source: String,
        kind: String,
        proposition: String,
        value: HexValue,
        #[serde(default)]
        evidence: BTreeMap<String, Value>,
    }
    let obs: ObservationArgs = serde_json::from_value(args.clone())
        .map_err(|e| (JSONRPC_INVALID_PARAMS, format!("invalid arguments: {e}")))?;

    let events_log = state_dir.join("events.jsonl");
    let existing: Vec<ResearchEvent> = if events_log.exists() {
        let f = std::fs::File::open(&events_log)
            .map_err(|e| (JSONRPC_INTERNAL_ERROR, format!("open events log: {e}")))?;
        std::io::BufReader::new(f)
            .lines()
            .map_while(Result::ok)
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| serde_json::from_str::<ResearchEvent>(&l).ok())
            .collect()
    } else {
        Vec::new()
    };

    let cycle = existing.iter().map(|e| e.cycle).max().unwrap_or(0).saturating_add(1);
    let evidence: Evidence = obs.evidence.into_iter().collect();
    let payload = match obs.kind.as_str() {
        "belief_update" => ResearchEventPayload::BeliefUpdate {
            proposition: obs.proposition.clone(),
            value: obs.value,
            evidence,
        },
        "experiment_result" => ResearchEventPayload::ExperimentResult {
            proposition: obs.proposition.clone(),
            value: obs.value,
            evidence,
        },
        "agent_vote" => ResearchEventPayload::AgentVote {
            proposition: obs.proposition.clone(),
            value: obs.value,
            evidence,
        },
        other => {
            return Err((
                JSONRPC_INVALID_PARAMS,
                format!("unknown event kind: {other}"),
            ));
        }
    };
    let new_event = ResearchEvent {
        cycle,
        source: obs.source,
        payload,
    };

    // Append to log first (source of truth) before replaying.
    if let Some(parent) = events_log.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| (JSONRPC_INTERNAL_ERROR, format!("mkdir state-dir: {e}")))?;
    }
    {
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&events_log)
            .map_err(|e| (JSONRPC_INTERNAL_ERROR, format!("open events log for append: {e}")))?;
        let line = serde_json::to_string(&new_event).unwrap_or_default();
        writeln!(f, "{line}").map_err(|e| (JSONRPC_INTERNAL_ERROR, format!("write event: {e}")))?;
    }

    let mut all = existing;
    all.push(new_event.clone());
    let trace = ResearchTrace {
        dimension: 4,
        events: all,
    };
    let mut loop_ = CognitiveLoop::new(4);
    let report = loop_.process_research_trace(trace);

    // Persist snapshot too so the next reader sees the update.
    let snapshot_path = state_dir.join("belief-snapshot.json");
    let snap_s = serde_json::to_string_pretty(&report.final_beliefs).unwrap_or_default();
    std::fs::write(&snapshot_path, snap_s)
        .map_err(|e| (JSONRPC_INTERNAL_ERROR, format!("write snapshot: {e}")))?;

    let escalations: Vec<Value> = report
        .outcomes
        .iter()
        .flat_map(|o| {
            o.actions.iter().filter_map(|a| match a {
                Action::Escalate { reason, confidence } => Some(json!({
                    "cycle": o.event.cycle,
                    "proposition": o.event.payload_proposition_owned(),
                    "reason": reason,
                    "confidence": confidence
                })),
                _ => None,
            })
        })
        .collect();

    Ok(json!({
        "appended": new_event,
        "proposition_value": report.final_beliefs.get(&obs.proposition).copied().unwrap_or(HexValue::Unknown),
        "escalations": escalations,
        "snapshot_size": report.final_beliefs.len()
    }))
}

fn tool_consensus(args: &Value) -> Result<Value, (i64, String)> {
    #[derive(Deserialize)]
    struct VoteIn {
        agent_id: String,
        #[serde(default)]
        role: Option<String>,
        #[serde(default)]
        self_trust: Option<f64>,
        #[serde(default)]
        message_trust: Option<f64>,
        value: HexValue,
    }
    #[derive(Deserialize)]
    struct ConsensusArgs {
        proposition: String,
        #[serde(default = "default_trust_model")]
        trust_model: String,
        votes: Vec<VoteIn>,
    }
    fn default_trust_model() -> String {
        "Equal".into()
    }

    let cargs: ConsensusArgs = serde_json::from_value(args.clone())
        .map_err(|e| (JSONRPC_INVALID_PARAMS, format!("invalid arguments: {e}")))?;

    if cargs.votes.is_empty() {
        return Err((JSONRPC_INVALID_PARAMS, "votes must be non-empty".into()));
    }

    let trust_model = match cargs.trust_model.as_str() {
        "Equal" => TrustModel::Equal,
        "RoleWeighted" => TrustModel::RoleWeighted,
        other => {
            return Err((
                JSONRPC_INVALID_PARAMS,
                format!("unknown trust_model: {other}"),
            ));
        }
    };

    let mut swarm = Swarm::new();
    for v in cargs.votes.iter() {
        let role = AgentRole {
            name: v.role.clone().unwrap_or_else(|| "voter".into()),
            self_trust: v.self_trust.unwrap_or(0.5),
            message_trust: v.message_trust.unwrap_or(0.5),
        };
        let mut agent = Agent::new(&v.agent_id, role);
        agent.beliefs.add_proposition(&cargs.proposition, v.value);
        swarm.add_agent(agent);
    }

    let result = swarm.consensus_with(&cargs.proposition, trust_model);
    Ok(json!({
        "proposition": result.proposition,
        "consensus": result.consensus,
        "agreement": result.agreement,
        "votes": result.votes
    }))
}

// ---------------------------------------------------------------------------
// Streaming session tools — Phase 6 multiplexer
// ---------------------------------------------------------------------------

fn tool_session_open(args: &Value, sessions: &mut SessionMap) -> Result<Value, (i64, String)> {
    let config_value = args.get("config").cloned().unwrap_or(json!({}));
    let config: SessionConfig = serde_json::from_value(config_value)
        .map_err(|e| (JSONRPC_INVALID_PARAMS, format!("invalid SessionConfig: {e}")))?;
    let session = StreamingSession::open(config)
        .map_err(|e| (JSONRPC_INTERNAL_ERROR, format!("open: {e}")))?;
    let id = session.session_id().to_string();
    let trace_path = session.trace_path().map(|p| p.display().to_string());
    let config_echo = serde_json::to_value(session.config()).unwrap_or(json!({}));
    sessions.insert(id.clone(), session);
    Ok(json!({
        "session_id": id,
        "trace_path": trace_path,
        "hari_version": env!("CARGO_PKG_VERSION"),
        "config_echo": config_echo,
        "open_sessions": sessions.len()
    }))
}

fn tool_session_event(args: &Value, sessions: &mut SessionMap) -> Result<Value, (i64, String)> {
    let session_id = args
        .get("session_id")
        .and_then(Value::as_str)
        .ok_or((JSONRPC_INVALID_PARAMS, "missing session_id".into()))?;
    let event_value = args
        .get("event")
        .cloned()
        .ok_or((JSONRPC_INVALID_PARAMS, "missing event".into()))?;
    let event: ResearchEvent = serde_json::from_value(event_value)
        .map_err(|e| (JSONRPC_INVALID_PARAMS, format!("invalid event: {e}")))?;

    let session = sessions
        .get_mut(session_id)
        .ok_or((JSONRPC_INVALID_PARAMS, format!("unknown session_id: {session_id}")))?;

    let rec = session
        .apply_event(event)
        .map_err(|e| (JSONRPC_INTERNAL_ERROR, format!("apply_event: {e}")))?;

    Ok(serde_json::to_value(rec).unwrap_or(json!({})))
}

fn tool_session_close(args: &Value, sessions: &mut SessionMap) -> Result<Value, (i64, String)> {
    let session_id = args
        .get("session_id")
        .and_then(Value::as_str)
        .ok_or((JSONRPC_INVALID_PARAMS, "missing session_id".into()))?;
    let session = sessions
        .remove(session_id)
        .ok_or((JSONRPC_INVALID_PARAMS, format!("unknown session_id: {session_id}")))?;
    let report = session.close();
    Ok(serde_json::to_value(report).unwrap_or(json!({})))
}
