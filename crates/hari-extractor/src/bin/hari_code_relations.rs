//! `hari-code-relations` — JSON call/dep graph → `RelationDeclaration` events.
//!
//! ## What this is
//!
//! Bridges Hari's `BeliefNetwork` propagation to source-code structure.
//! When a static analyzer (`ix-code-analyze`, sentrux, or a hand-written
//! tool) emits a JSON graph of edges `{ from, to, relation }`, this
//! binary converts each edge to a `ResearchEventPayload::RelationDeclaration`
//! so the belief network can propagate verdicts along it.
//!
//! ## Why this exists
//!
//! Hari's `Relation::{Supports, Contradicts, Implies}` enable derived
//! belief updates: if "function A calls function B" is `Implies` and B
//! becomes `False`, A derives `Doubtful` automatically. Today nothing
//! produces such relation events from real code — analyzers write
//! call/dep graphs but stop there. This binary is the smallest possible
//! bridge.
//!
//! ## Input contract
//!
//! A single JSON document on disk:
//!
//! ```json
//! {
//!   "edges": [
//!     { "from": "fn:foo", "to": "fn:bar", "relation": "Implies" },
//!     { "from": "fn:bar", "to": "fn:baz", "relation": "Supports" },
//!     { "from": "fn:qux", "to": "fn:foo", "relation": "Contradicts" }
//!   ]
//! }
//! ```
//!
//! `relation` is one of `Supports | Contradicts | Implies` (matching
//! `hari_lattice::Relation`).
//!
//! ## Output
//!
//! JSONL on stdout — one `ResearchEvent` per line, ready to pipe into
//! `hari-harness --events`. Cycles start at `--start-cycle` (default 1).
//!
//! ## Usage
//!
//! ```pwsh
//! # One-shot: graph → events on stdout
//! hari-code-relations --graph call-graph.json --source ix-code-analyze
//!
//! # Pipe straight into the harness:
//! hari-code-relations --graph cg.json --source ix-code-analyze > /tmp/rels.jsonl
//! hari-harness --events /tmp/rels.jsonl
//! ```

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use hari_core::{ResearchEvent, ResearchEventPayload};
use hari_lattice::Relation;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Graph {
    edges: Vec<Edge>,
}

#[derive(Debug, Deserialize)]
struct Edge {
    from: String,
    to: String,
    relation: Relation,
}

fn parse_args() -> Result<(PathBuf, String, u64), String> {
    let mut graph: Option<PathBuf> = None;
    let mut source = String::from("hari-code-relations");
    let mut start_cycle: u64 = 1;

    let mut iter = std::env::args().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--graph" => graph = iter.next().map(PathBuf::from),
            "--source" => {
                source = iter
                    .next()
                    .ok_or_else(|| "--source expects a value".to_string())?
            }
            "--start-cycle" => {
                start_cycle = iter
                    .next()
                    .and_then(|s| s.parse().ok())
                    .ok_or_else(|| "--start-cycle expects a u64".to_string())?;
            }
            "-h" | "--help" => {
                print_usage();
                std::process::exit(0);
            }
            other => return Err(format!("unrecognized argument: {other}")),
        }
    }
    let graph = graph.ok_or_else(|| "--graph <path> is required".to_string())?;
    Ok((graph, source, start_cycle))
}

fn print_usage() {
    eprintln!(
        "Usage: hari-code-relations --graph <path.json>\n\
         \x20                       [--source <name>] [--start-cycle <N>]\n\
         \n\
         Reads a JSON edge-list {{ edges: [{{from,to,relation}}] }}, emits one\n\
         ResearchEvent::RelationDeclaration per edge as JSONL on stdout."
    );
}

fn main() -> ExitCode {
    let (graph_path, source, start_cycle) = match parse_args() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("error: {e}");
            print_usage();
            return ExitCode::from(2);
        }
    };

    let s = match std::fs::read_to_string(&graph_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error reading {}: {e}", graph_path.display());
            return ExitCode::from(1);
        }
    };
    let graph: Graph = match serde_json::from_str(&s) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("error parsing graph: {e}");
            return ExitCode::from(1);
        }
    };

    let mut stdout = std::io::stdout().lock();
    for (i, edge) in graph.edges.into_iter().enumerate() {
        let cycle = start_cycle + i as u64;
        let event = ResearchEvent {
            cycle,
            source: source.clone(),
            payload: ResearchEventPayload::RelationDeclaration {
                from: edge.from,
                to: edge.to,
                relation: edge.relation,
            },
        };
        let line = match serde_json::to_string(&event) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("error serializing event at cycle {cycle}: {e}");
                return ExitCode::from(1);
            }
        };
        if let Err(e) = writeln!(stdout, "{line}") {
            eprintln!("error writing stdout: {e}");
            return ExitCode::from(1);
        }
    }

    ExitCode::SUCCESS
}
