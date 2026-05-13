//! `hari-from-ix-autoresearch` — convert an ix-autoresearch JSONL run log
//! into a Hari `ResearchTrace`.
//!
//! ## What this is
//!
//! Bridges contribution #1 from the 2026-05-13 design discussion: Hari as
//! meta-controller over ix-autoresearch hypothesis exploration. Rather
//! than coupling the two crates (which would force a shared types
//! dependency and complicate `ix-autoresearch`'s otherwise self-contained
//! design), we treat ix-autoresearch's JSONL log as a published contract
//! and adapt on the Hari side.
//!
//! ## Wire contract
//!
//! ix-autoresearch's `crates/ix-autoresearch/src/log.rs` emits JSONL where
//! every line is a `LogEvent<C, S>` tagged via `#[serde(tag = "event")]`.
//! We only care about the `iteration` variant; `run_start` and
//! `run_complete` are parsed but discarded. The relevant fields:
//!
//! - `iteration` (usize) → maps to `cycle`
//! - `accepted` (bool) + `error` (Option<String>) → HexValue inference
//! - `config_hash` (String) → proposition identity (target-config pair)
//! - `reward` (Option<f64>) → evidence
//! - `elapsed_ms` (u64) → evidence
//! - `cache_hit` (bool) → evidence (cached results are weaker signal)
//!
//! HexValue mapping:
//! - `error.is_some()` → False        (eval crashed; config is unviable)
//! - `accepted = true`  → Probable    (strategy accepted; not yet True)
//! - `accepted = false` → Doubtful    (strategy rejected; not necessarily False)
//!
//! Probable is intentionally chosen over True because acceptance under
//! Greedy/SA is a single-sample claim. Multiple Probable observations on
//! the same proposition let Hari's `process_research_trace` consolidate
//! to True via repeated agreement; that's how the BeliefNetwork is
//! supposed to learn confidence over time.
//!
//! ## Usage
//!
//! ```pwsh
//! cargo run -p hari-extractor --bin hari-from-ix-autoresearch -- `
//!   --log path/to/ix-autoresearch-run.jsonl `
//!   --target target_chatbot
//! # → ResearchTrace JSON on stdout, replay-compatible with `hari-core replay`.
//! ```

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::ExitCode;

use hari_core::{Evidence, ResearchEvent, ResearchEventPayload, ResearchTrace};
use hari_lattice::HexValue;
use serde_json::Value;

fn main() -> ExitCode {
    let (log_path, target, dimension) = match parse_args() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("error: {e}");
            print_usage();
            return ExitCode::from(2);
        }
    };

    let events = match read_iterations(&log_path, &target) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("error reading log: {e}");
            return ExitCode::from(1);
        }
    };

    if events.is_empty() {
        eprintln!(
            "error: no `iteration` events found in {}",
            log_path.display()
        );
        return ExitCode::from(1);
    }

    let trace = ResearchTrace { dimension, events };
    match serde_json::to_string_pretty(&trace) {
        Ok(s) => {
            println!("{s}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error serializing trace: {e}");
            ExitCode::from(1)
        }
    }
}

fn parse_args() -> Result<(PathBuf, String, usize), String> {
    let mut log: Option<PathBuf> = None;
    let mut target: Option<String> = None;
    let mut dimension: usize = 4;

    let mut iter = std::env::args().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--log" => log = iter.next().map(PathBuf::from),
            "--target" => target = iter.next(),
            "--dimension" => {
                dimension = iter
                    .next()
                    .and_then(|s| s.parse().ok())
                    .ok_or_else(|| "--dimension expects a usize".to_string())?;
            }
            "-h" | "--help" => {
                print_usage();
                std::process::exit(0);
            }
            other => return Err(format!("unrecognized argument: {other}")),
        }
    }

    let log = log.ok_or_else(|| "--log <path> is required".to_string())?;
    let target = target.ok_or_else(|| "--target <name> is required".to_string())?;
    Ok((log, target, dimension))
}

fn print_usage() {
    eprintln!(
        "Usage: hari-from-ix-autoresearch --log <path.jsonl> --target <name> [--dimension N]\n\
         \n\
         Reads an ix-autoresearch JSONL run log, maps each `iteration` event to a\n\
         Hari ResearchEvent, and emits a ResearchTrace JSON on stdout.\n\
         \n\
         Lines that fail to parse are skipped with a stderr warning (matching\n\
         ix-autoresearch's own replay-tolerant contract). Lines with `event` types\n\
         other than `iteration` are silently ignored — only the work-unit signal matters."
    );
}

fn read_iterations(path: &PathBuf, target: &str) -> std::io::Result<Vec<ResearchEvent>> {
    let f = std::fs::File::open(path)?;
    let mut out = Vec::new();
    for (i, line) in BufReader::new(f).lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let v: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("warn: line {} skipped (parse error: {e})", i + 1);
                continue;
            }
        };
        if v.get("event").and_then(Value::as_str) != Some("iteration") {
            continue;
        }
        if let Some(event) = iteration_to_research_event(&v, target) {
            out.push(event);
        }
    }
    Ok(out)
}

fn iteration_to_research_event(v: &Value, target: &str) -> Option<ResearchEvent> {
    let iteration = v.get("iteration").and_then(Value::as_u64)?;
    let config_hash = v.get("config_hash").and_then(Value::as_str)?;
    let accepted = v.get("accepted").and_then(Value::as_bool).unwrap_or(false);
    let error = v.get("error").and_then(Value::as_str);
    let reward = v.get("reward").and_then(Value::as_f64);
    let elapsed_ms = v.get("elapsed_ms").and_then(Value::as_u64);
    let cache_hit = v.get("cache_hit").and_then(Value::as_bool).unwrap_or(false);

    let value = match (error, accepted) {
        (Some(_), _) => HexValue::False,
        (None, true) => HexValue::Probable,
        (None, false) => HexValue::Doubtful,
    };

    // Proposition encodes the (target, config-fingerprint) pair so the
    // BeliefNetwork can consolidate repeated observations of the same
    // config under one node. config_hash is a stable digest emitted by
    // ix-autoresearch — no need to rehash here.
    let proposition = format!(
        "{target}/config-{}-is-an-improvement",
        &config_hash[..config_hash.len().min(12)]
    );

    let mut evidence: Evidence = std::collections::BTreeMap::new();
    if let Some(r) = reward {
        evidence.insert("reward".into(), serde_json::json!(r));
    }
    if let Some(ms) = elapsed_ms {
        evidence.insert("elapsed_ms".into(), serde_json::json!(ms));
    }
    if cache_hit {
        evidence.insert("cache_hit".into(), serde_json::json!(true));
    }
    if let Some(e) = error {
        evidence.insert("error".into(), serde_json::json!(e));
    }
    evidence.insert("config_hash".into(), serde_json::json!(config_hash));

    Some(ResearchEvent {
        cycle: iteration,
        source: format!("ix-autoresearch:{target}"),
        payload: ResearchEventPayload::ExperimentResult {
            proposition,
            value,
            evidence,
        },
    })
}
