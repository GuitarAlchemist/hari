//! `hari-harness` — one Cherny-loop iteration over a Hari belief state.
//!
//! ## What this is
//!
//! Glues the three Hari ingestion paths (Mercury-extracted prose, the
//! ix-autoresearch JSONL adapter, and raw event JSONL) into a single
//! command that:
//!
//! 1. Appends new events to a persistent **event log** (`events.jsonl`).
//! 2. Replays the **entire** log through [`CognitiveLoop::process_research_trace`].
//! 3. Writes the resulting **belief snapshot** + a **diff** vs. the previous one.
//! 4. Exits non-zero when any [`Action::Escalate`] fired (so a `/loop`
//!    wrapper can branch on "humans should look at this").
//!
//! The event log is the source of truth; the snapshot is a derived view
//! the next session reads instead of re-parsing transcripts.
//!
//! ## Why this exists
//!
//! Boris Cherny's three loops (scheduled `/loop`, CLAUDE.md self-
//! improvement, execution-verification) all need **persistent
//! institutional memory** between iterations. The de-facto memory today
//! is markdown — fine for prose, weak for cross-session reasoning.
//! Hari already ships structured belief state with provenance and a
//! first-class `Contradictory` value; this binary is the bridge that
//! makes one Cherny-loop tick read/write that state.
//!
//! ## CLI
//!
//! ```text
//! hari-harness [--state-dir DIR] [--source SOURCE]
//!              [--notes PATH]          (prose; one belief per line; Mercury-extracted)
//!              [--autoresearch PATH]   (ix-autoresearch JSONL log)
//!              [--target TARGET]       (required when --autoresearch is set)
//!              [--events PATH]         (raw ResearchEvent JSONL — no extraction)
//!              [--cargo-test PATH]     (cargo test --message-format=json output)
//!              [--dimension N]         (default 4)
//!              [--no-escalate-fails]   (do not exit non-zero on Escalate)
//! ```
//!
//! Defaults: `--state-dir state/harness`, `--source hari-harness`.
//! Layout under `state-dir`:
//!
//! ```text
//! events.jsonl         append-only event log; source of truth
//! belief-state.json    latest ResearchReplayReport (full)
//! belief-snapshot.json compact { proposition → HexValue } view
//! belief-diff.json     what changed vs. the previous run
//! ```
//!
//! ## Exit codes
//!
//! | code | meaning                                                |
//! |-----:|--------------------------------------------------------|
//! |    0 | no escalations; safe to continue the loop              |
//! |    1 | hard error (I/O, parse, missing required flag)         |
//! |    2 | one or more `Action::Escalate` fired (review candidate) |

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use hari_core::{Action, CognitiveLoop, ResearchEvent, ResearchTrace};
use hari_extractor::{MercuryConfig, MercuryExtractor};
use hari_lattice::HexValue;
use serde::Serialize;
use serde_json::Value;

#[derive(Debug)]
struct Args {
    state_dir: PathBuf,
    source: String,
    notes_path: Option<PathBuf>,
    autoresearch_path: Option<PathBuf>,
    autoresearch_target: Option<String>,
    events_path: Option<PathBuf>,
    cargo_test_path: Option<PathBuf>,
    dimension: usize,
    escalate_fails: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut state_dir = PathBuf::from("state/harness");
    let mut source = String::from("hari-harness");
    let mut notes_path: Option<PathBuf> = None;
    let mut autoresearch_path: Option<PathBuf> = None;
    let mut autoresearch_target: Option<String> = None;
    let mut events_path: Option<PathBuf> = None;
    let mut cargo_test_path: Option<PathBuf> = None;
    let mut dimension: usize = 4;
    let mut escalate_fails = true;

    let mut iter = std::env::args().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--state-dir" => {
                state_dir = iter
                    .next()
                    .map(PathBuf::from)
                    .ok_or_else(|| "--state-dir expects a path".to_string())?;
            }
            "--source" => {
                source = iter
                    .next()
                    .ok_or_else(|| "--source expects a value".to_string())?
            }
            "--notes" => notes_path = iter.next().map(PathBuf::from),
            "--autoresearch" => autoresearch_path = iter.next().map(PathBuf::from),
            "--target" => autoresearch_target = iter.next(),
            "--events" => events_path = iter.next().map(PathBuf::from),
            "--cargo-test" => cargo_test_path = iter.next().map(PathBuf::from),
            "--dimension" => {
                dimension = iter
                    .next()
                    .and_then(|s| s.parse().ok())
                    .ok_or_else(|| "--dimension expects a usize".to_string())?;
            }
            "--no-escalate-fails" => escalate_fails = false,
            "-h" | "--help" => {
                print_usage();
                std::process::exit(0);
            }
            other => return Err(format!("unrecognized argument: {other}")),
        }
    }

    if autoresearch_path.is_some() && autoresearch_target.is_none() {
        return Err("--autoresearch requires --target".to_string());
    }

    Ok(Args {
        state_dir,
        source,
        notes_path,
        autoresearch_path,
        autoresearch_target,
        events_path,
        cargo_test_path,
        dimension,
        escalate_fails,
    })
}

fn print_usage() {
    eprintln!(
        "Usage: hari-harness [--state-dir DIR] [--source S]\n\
         \x20             [--notes PATH] [--autoresearch PATH --target T]\n\
         \x20             [--events PATH] [--cargo-test PATH]\n\
         \x20             [--dimension N] [--no-escalate-fails]\n\
         \n\
         Runs one Cherny-loop iteration: ingests new events, replays the full\n\
         event log through Hari's CognitiveLoop, writes belief snapshot + diff.\n\
         \n\
         --cargo-test consumes output of `cargo test --message-format=json` and\n\
         maps each {{type:'test', event:'ok'|'failed'}} line to an ExperimentResult.\n\
         \n\
         Exit 0 = no escalations, 1 = error, 2 = at least one Escalate fired.\n\
         \n\
         Mercury (--notes) requires INCEPTION_API_KEY in env."
    );
}

/// Parse one line of `cargo test --message-format=json` into a
/// ResearchEvent. Each test outcome becomes one ExperimentResult; the
/// proposition encodes the test path so reruns consolidate.
///
/// Only `{ "type": "test", "event": "ok"|"failed", "name": "..." }` is
/// mapped. Other JSON line types (compiler diagnostics, suite events)
/// are silently ignored — they're not single-test verdicts.
fn cargo_test_line_to_event(v: &Value, source: &str, cycle: u64) -> Option<ResearchEvent> {
    if v.get("type").and_then(Value::as_str) != Some("test") {
        return None;
    }
    let event_kind = v.get("event").and_then(Value::as_str)?;
    let name = v.get("name").and_then(Value::as_str)?;
    let value = match event_kind {
        "ok" => HexValue::True,
        "failed" => HexValue::False,
        // started / ignored / timeout etc. are not verdicts
        _ => return None,
    };
    let mut evidence: BTreeMap<String, Value> = BTreeMap::new();
    evidence.insert("test_name".into(), serde_json::json!(name));
    evidence.insert("event".into(), serde_json::json!(event_kind));
    if let Some(stdout) = v.get("stdout").and_then(Value::as_str) {
        evidence.insert("stdout".into(), serde_json::json!(stdout));
    }
    Some(ResearchEvent {
        cycle,
        source: format!("cargo-test:{source}"),
        payload: hari_core::ResearchEventPayload::ExperimentResult {
            proposition: format!("test/{name}-passes"),
            value,
            evidence,
        },
    })
}

#[derive(Serialize)]
struct BeliefDiff {
    previous_count: usize,
    current_count: usize,
    added: BTreeMap<String, HexValue>,
    changed: BTreeMap<String, (HexValue, HexValue)>,
    unchanged: usize,
}

impl BeliefDiff {
    fn between(
        previous: &BTreeMap<String, HexValue>,
        current: &BTreeMap<String, HexValue>,
    ) -> Self {
        let mut added = BTreeMap::new();
        let mut changed = BTreeMap::new();
        let mut unchanged = 0usize;
        for (k, v_new) in current {
            match previous.get(k) {
                None => {
                    added.insert(k.clone(), *v_new);
                }
                Some(v_old) if v_old != v_new => {
                    changed.insert(k.clone(), (*v_old, *v_new));
                }
                Some(_) => unchanged += 1,
            }
        }
        BeliefDiff {
            previous_count: previous.len(),
            current_count: current.len(),
            added,
            changed,
            unchanged,
        }
    }
}

fn read_notes_lines(path: &Path) -> std::io::Result<Vec<String>> {
    let f = std::fs::File::open(path)?;
    Ok(BufReader::new(f)
        .lines()
        .map_while(Result::ok)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && !s.starts_with('#'))
        .collect())
}

fn read_jsonl<T: serde::de::DeserializeOwned>(path: &Path) -> std::io::Result<Vec<T>> {
    let f = std::fs::File::open(path)?;
    let mut out = Vec::new();
    for (i, line) in BufReader::new(f).lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<T>(&line) {
            Ok(v) => out.push(v),
            Err(e) => eprintln!("warn: {} line {} skipped: {e}", path.display(), i + 1),
        }
    }
    Ok(out)
}

fn append_jsonl<T: Serialize>(path: &Path, items: &[T]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    for item in items {
        let line = serde_json::to_string(item).map_err(std::io::Error::other)?;
        writeln!(f, "{line}")?;
    }
    Ok(())
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let s = serde_json::to_string_pretty(value).map_err(std::io::Error::other)?;
    let mut f = std::fs::File::create(path)?;
    f.write_all(s.as_bytes())
}

fn next_cycle(log: &[ResearchEvent]) -> u64 {
    log.iter()
        .map(|e| e.cycle)
        .max()
        .unwrap_or(0)
        .saturating_add(1)
}

/// Convert one `iteration` line of an ix-autoresearch JSONL log into a
/// `ResearchEvent::ExperimentResult`. Returns `None` for non-iteration
/// lines (`run_start`, `run_complete`, malformed JSON).
fn iteration_to_event(v: &Value, target: &str, cycle: u64) -> Option<ResearchEvent> {
    if v.get("event").and_then(Value::as_str) != Some("iteration") {
        return None;
    }
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

    let proposition = format!(
        "{target}/config-{}-is-an-improvement",
        &config_hash[..config_hash.len().min(12)]
    );

    let mut evidence: BTreeMap<String, Value> = BTreeMap::new();
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
        cycle,
        source: format!("ix-autoresearch:{target}"),
        payload: hari_core::ResearchEventPayload::ExperimentResult {
            proposition,
            value,
            evidence,
        },
    })
}

async fn collect_new_events(args: &Args, start_cycle: u64) -> Result<Vec<ResearchEvent>, String> {
    let mut new_events: Vec<ResearchEvent> = Vec::new();
    let mut cycle = start_cycle;

    // 1. Raw event JSONL — no extraction, just renumber cycles.
    if let Some(p) = &args.events_path {
        let raw: Vec<ResearchEvent> = read_jsonl(p).map_err(|e| format!("events: {e}"))?;
        for mut e in raw {
            e.cycle = cycle;
            cycle += 1;
            new_events.push(e);
        }
    }

    // 2. ix-autoresearch JSONL — adapter, no Mercury.
    if let Some(p) = &args.autoresearch_path {
        let target = args.autoresearch_target.as_deref().unwrap();
        let f = std::fs::File::open(p).map_err(|e| format!("autoresearch: {e}"))?;
        for (i, line) in BufReader::new(f).lines().enumerate() {
            let line = line.map_err(|e| format!("autoresearch line {}: {e}", i + 1))?;
            if line.trim().is_empty() {
                continue;
            }
            let v: Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("warn: autoresearch line {} skipped: {e}", i + 1);
                    continue;
                }
            };
            if let Some(ev) = iteration_to_event(&v, target, cycle) {
                cycle += 1;
                new_events.push(ev);
            }
        }
    }

    // 3. cargo test JSON lines — each test outcome → one ExperimentResult.
    if let Some(p) = &args.cargo_test_path {
        let f = std::fs::File::open(p).map_err(|e| format!("cargo-test: {e}"))?;
        for (i, line) in BufReader::new(f).lines().enumerate() {
            let line = line.map_err(|e| format!("cargo-test line {}: {e}", i + 1))?;
            if line.trim().is_empty() {
                continue;
            }
            let v: Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(_) => continue, // many cargo lines aren't JSON; skip silently
            };
            if let Some(ev) = cargo_test_line_to_event(&v, &args.source, cycle) {
                cycle += 1;
                new_events.push(ev);
            }
        }
    }

    // 4. Prose notes via Mercury (last, since it's the slow path).
    if let Some(p) = &args.notes_path {
        let notes = read_notes_lines(p).map_err(|e| format!("notes: {e}"))?;
        if !notes.is_empty() {
            let cfg = MercuryConfig::from_env().map_err(|e| e.to_string())?;
            let extractor = MercuryExtractor::new(cfg).map_err(|e| e.to_string())?;
            for note in &notes {
                match extractor.extract(cycle, &args.source, note).await {
                    Ok(ev) => {
                        cycle += 1;
                        new_events.push(ev);
                    }
                    Err(e) => eprintln!("warn: dropping note (extract failed: {e})\n  > {note}"),
                }
            }
        }
    }

    Ok(new_events)
}

#[derive(Serialize)]
struct HarnessReport<'a> {
    state_dir: &'a Path,
    events_appended: usize,
    total_events: usize,
    escalations: &'a [EscalationSummary],
    diff_path: PathBuf,
    snapshot_path: PathBuf,
    state_path: PathBuf,
}

#[derive(Serialize, Clone)]
struct EscalationSummary {
    cycle: u64,
    proposition: Option<String>,
    reason: String,
    confidence: f64,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: {e}");
            print_usage();
            return ExitCode::from(1);
        }
    };

    if args.notes_path.is_none()
        && args.autoresearch_path.is_none()
        && args.events_path.is_none()
        && args.cargo_test_path.is_none()
    {
        eprintln!(
            "error: at least one of --notes, --autoresearch, --events, --cargo-test is required"
        );
        return ExitCode::from(1);
    }

    let events_log = args.state_dir.join("events.jsonl");
    let state_path = args.state_dir.join("belief-state.json");
    let snapshot_path = args.state_dir.join("belief-snapshot.json");
    let diff_path = args.state_dir.join("belief-diff.json");

    let existing: Vec<ResearchEvent> = if events_log.exists() {
        match read_jsonl(&events_log) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("error reading event log: {e}");
                return ExitCode::from(1);
            }
        }
    } else {
        Vec::new()
    };

    let previous_snapshot: BTreeMap<String, HexValue> = if snapshot_path.exists() {
        std::fs::File::open(&snapshot_path)
            .ok()
            .and_then(|mut f| {
                let mut s = String::new();
                f.read_to_string(&mut s).ok().map(|_| s)
            })
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    } else {
        BTreeMap::new()
    };

    let new_events = match collect_new_events(&args, next_cycle(&existing)).await {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(1);
        }
    };

    if new_events.is_empty() {
        eprintln!("warn: no new events produced from inputs; replaying existing log only");
    }

    if let Err(e) = append_jsonl(&events_log, &new_events) {
        eprintln!("error appending to event log: {e}");
        return ExitCode::from(1);
    }

    let mut all_events = existing;
    all_events.extend(new_events.iter().cloned());
    let total_events = all_events.len();

    let trace = ResearchTrace {
        dimension: args.dimension,
        events: all_events,
    };

    let mut loop_ = CognitiveLoop::new(args.dimension);
    let report = loop_.process_research_trace(trace);

    // Surface escalations — these are the Cherny-loop "needs human review" signal.
    let mut escalations: Vec<EscalationSummary> = Vec::new();
    for outcome in &report.outcomes {
        for action in &outcome.actions {
            if let Action::Escalate { reason, confidence } = action {
                escalations.push(EscalationSummary {
                    cycle: outcome.event.cycle,
                    proposition: outcome.event.payload_proposition_owned(),
                    reason: reason.clone(),
                    confidence: *confidence,
                });
            }
        }
    }

    if let Err(e) = write_json(&state_path, &report) {
        eprintln!("error writing belief-state.json: {e}");
        return ExitCode::from(1);
    }
    if let Err(e) = write_json(&snapshot_path, &report.final_beliefs) {
        eprintln!("error writing belief-snapshot.json: {e}");
        return ExitCode::from(1);
    }
    let diff = BeliefDiff::between(&previous_snapshot, &report.final_beliefs);
    if let Err(e) = write_json(&diff_path, &diff) {
        eprintln!("error writing belief-diff.json: {e}");
        return ExitCode::from(1);
    }

    let harness_report = HarnessReport {
        state_dir: &args.state_dir,
        events_appended: new_events.len(),
        total_events,
        escalations: &escalations,
        diff_path,
        snapshot_path,
        state_path,
    };
    let s = serde_json::to_string_pretty(&harness_report).unwrap_or_else(|_| "{}".to_string());
    println!("{s}");

    if args.escalate_fails && !escalations.is_empty() {
        ExitCode::from(2)
    } else {
        ExitCode::SUCCESS
    }
}
