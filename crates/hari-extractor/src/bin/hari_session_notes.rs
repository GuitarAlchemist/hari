//! `hari-session-notes` — convert a session's free-text notes into a Hari
//! belief snapshot.
//!
//! ## What this is
//!
//! Bridges contribution #2 from the 2026-05-13 Hari design discussion: a
//! cross-session memory layer for the multi-agent Claude work. The CLI
//! reads NL session notes from a file or stdin, pipes each line through
//! [`hari_extractor::MercuryExtractor`], collects the resulting events
//! into a [`ResearchTrace`], replays the trace through [`CognitiveLoop`],
//! and prints the final belief state + any contradictions surfaced.
//!
//! Designed so a session's terminal-end summary can be saved to a file
//! and passed in here at the start of the next session — the receiving
//! session gets a structured view of "what does the ecosystem currently
//! believe" without re-reading prose.
//!
//! ## Usage
//!
//! ```pwsh
//! $env:INCEPTION_API_KEY = "sk_..."   # or already in user env
//!
//! # From a notes file (one observation per line):
//! cargo run -p hari-extractor --bin hari-session-notes -- `
//!   --source claude-session-1234 --notes session-notes.txt
//!
//! # From stdin:
//! Get-Content session-notes.txt | `
//!   cargo run -p hari-extractor --bin hari-session-notes -- --source claude-session-1234
//!
//! # Dump trace as JSON (replay-compatible with `hari-core replay`):
//! cargo run -p hari-extractor --bin hari-session-notes -- `
//!   --source claude-session-1234 --notes session-notes.txt --emit-trace > trace.json
//! ```
//!
//! ## Output
//!
//! By default, a JSON object with three top-level keys:
//! - `extracted`: the events that came out of Mercury (for audit).
//! - `report`: the [`ResearchReplayReport`] from `process_research_trace`.
//! - `belief_state`: a compact `{ proposition → value }` map of final beliefs.
//!
//! Pass `--emit-trace` to get the raw `ResearchTrace` instead — useful
//! when you want to checkpoint a session's belief contribution to disk
//! for the next session to replay.

use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::ExitCode;

use hari_core::{CognitiveLoop, ResearchEvent, ResearchTrace};
use hari_extractor::{MercuryConfig, MercuryExtractor};
use serde::Serialize;

#[derive(Debug)]
struct Args {
    source: String,
    notes_path: Option<PathBuf>,
    emit_trace: bool,
    dimension: usize,
}

fn parse_args() -> Result<Args, String> {
    let mut source: Option<String> = None;
    let mut notes_path: Option<PathBuf> = None;
    let mut emit_trace = false;
    let mut dimension: usize = 4;

    let mut iter = std::env::args().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--source" => source = iter.next().or_else(|| Some(String::new())),
            "--notes" => notes_path = iter.next().map(PathBuf::from),
            "--emit-trace" => emit_trace = true,
            "--dimension" => {
                dimension = iter
                    .next()
                    .and_then(|s| s.parse().ok())
                    .ok_or_else(|| "--dimension expects a usize argument".to_string())?;
            }
            "-h" | "--help" => {
                print_usage();
                std::process::exit(0);
            }
            other => return Err(format!("unrecognized argument: {other}")),
        }
    }

    let source = source.ok_or_else(|| "--source is required".to_string())?;
    if source.trim().is_empty() {
        return Err("--source must be non-empty".to_string());
    }

    Ok(Args { source, notes_path, emit_trace, dimension })
}

fn print_usage() {
    eprintln!(
        "Usage: hari-session-notes --source <id> [--notes <path>] [--emit-trace] [--dimension N]\n\
         \n\
         Reads NL notes (one per line) from --notes <path> or stdin, extracts each\n\
         into a ResearchEvent via Mercury, replays the resulting trace through Hari's\n\
         CognitiveLoop, and prints either the full session report (default) or the raw\n\
         ResearchTrace JSON (--emit-trace).\n\
         \n\
         Requires INCEPTION_API_KEY in the env."
    );
}

fn read_notes(path: Option<&PathBuf>) -> std::io::Result<Vec<String>> {
    let lines: Vec<String> = match path {
        Some(p) => {
            let f = std::fs::File::open(p)?;
            BufReader::new(f).lines().collect::<Result<_, _>>()?
        }
        None => {
            let mut buf = String::new();
            std::io::stdin().read_to_string(&mut buf)?;
            buf.lines().map(|s| s.to_string()).collect()
        }
    };
    Ok(lines
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && !s.starts_with('#'))
        .collect())
}

#[derive(Serialize)]
struct SessionReport<'a> {
    extracted: &'a [ResearchEvent],
    report: &'a hari_core::ResearchReplayReport,
    belief_state: &'a std::collections::BTreeMap<String, hari_lattice::HexValue>,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: {e}");
            print_usage();
            return ExitCode::from(2);
        }
    };

    let notes = match read_notes(args.notes_path.as_ref()) {
        Ok(n) => n,
        Err(e) => {
            eprintln!("error reading notes: {e}");
            return ExitCode::from(1);
        }
    };

    if notes.is_empty() {
        eprintln!("error: no non-comment, non-blank lines found in input");
        return ExitCode::from(1);
    }

    let cfg = match MercuryConfig::from_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(1);
        }
    };
    let extractor = match MercuryExtractor::new(cfg) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(1);
        }
    };

    let mut events: Vec<ResearchEvent> = Vec::with_capacity(notes.len());
    for (i, note) in notes.iter().enumerate() {
        let cycle = (i + 1) as u64;
        match extractor.extract(cycle, &args.source, note).await {
            Ok(ev) => events.push(ev),
            Err(e) => {
                eprintln!("warn: dropping note #{cycle} (extraction failed: {e})\n  > {note}");
                // Continue rather than abort — partial belief snapshots are
                // still useful, and one fuzzy note shouldn't poison the whole
                // session import.
            }
        }
    }

    if events.is_empty() {
        eprintln!("error: all notes failed to extract");
        return ExitCode::from(1);
    }

    let trace = ResearchTrace {
        dimension: args.dimension,
        events: events.clone(),
    };

    if args.emit_trace {
        match serde_json::to_string_pretty(&trace) {
            Ok(s) => {
                println!("{s}");
                return ExitCode::SUCCESS;
            }
            Err(e) => {
                eprintln!("error serializing trace: {e}");
                return ExitCode::from(1);
            }
        }
    }

    let mut loop_ = CognitiveLoop::new(args.dimension);
    let report = loop_.process_research_trace(trace);

    let session = SessionReport {
        extracted: &events,
        report: &report,
        belief_state: &report.final_beliefs,
    };

    match serde_json::to_string_pretty(&session) {
        Ok(s) => {
            println!("{s}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error serializing report: {e}");
            ExitCode::from(1)
        }
    }
}
