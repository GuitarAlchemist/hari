//! `hari-from-ix-autoresearch` — read recorded ix-autoresearch JSONL logs.
//!
//! ```text
//! hari-from-ix-autoresearch <log.jsonl>...           # ResearchTrace JSON on stdout
//! hari-from-ix-autoresearch --report <log.jsonl>...  # IxRunReport JSON on stdout
//! ```
//!
//! Several logs are projected into one stream in the order given. The trace
//! replays with `hari-core replay` or streams through `hari-core serve`; the
//! report replays it under every #35 arm. See
//! `hari_extractor::ix_autoresearch` for the projection and its limits.

use std::process::ExitCode;

use hari_extractor::ix_autoresearch::{parse_log, project, run_report};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("hari-from-ix-autoresearch: {e}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut report = false;
    let mut paths = Vec::new();
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--report" => report = true,
            flag if flag.starts_with("--") => return Err(format!("unknown flag {flag}").into()),
            _ => paths.push(arg),
        }
    }
    if paths.is_empty() {
        return Err("usage: hari-from-ix-autoresearch [--report] <log.jsonl>...".into());
    }

    let runs = paths
        .iter()
        .map(|p| {
            let raw = std::fs::read_to_string(p).map_err(|e| format!("{p}: {e}"))?;
            parse_log(&raw).map_err(|e| format!("{p}: {e}"))
        })
        .collect::<Result<Vec<_>, String>>()?;

    let stdout = std::io::stdout();
    if report {
        serde_json::to_writer_pretty(stdout.lock(), &run_report(&runs))?;
    } else {
        serde_json::to_writer_pretty(stdout.lock(), &project(&runs))?;
    }
    println!();
    Ok(())
}
