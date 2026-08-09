//! # Project Hari — Cognitive-State Research Sandbox
//!
//! Main binary that demonstrates the cognitive loop with all subsystems.

use hari_core::{
    compare_replay, compare_replay_three_way, forecast, operator_model, reliability,
    source_reliability, Action, CognitiveLoop, PriorityModel, Request, ResearchEvent,
    ResearchEventPayload, ResearchTrace, Response, StreamingSession, SubjectiveLogicConfig,
};
use hari_lattice::{HexValue, Relation};
use hari_swarm::{Agent, AgentRole, TrustModel};
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::{env, fs, process};
use tracing::{info, warn};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.get(1).map(String::as_str) == Some("replay") {
        // Parse trailing args. Accepts:
        //   replay <trace.json>              (batch trace, dimension+events)
        //   replay --compare <trace.json>    (batch trace through both models)
        //   replay --session <session.jsonl> (Phase 6 recorded session file)
        //   replay --calibration <trace.json>
        //       (attach the forecast ledger's calibration block to the report)
        //   replay --paired <paired-fixture.json>
        //       (score the #35 §5.1 primary metric against bundled ground truth)
        //   replay --paired --compare3 <paired-fixture.json>
        //       (the same ground truth graded against all three arms at once —
        //        §5.1 defines the primary metric per arm, so this is the shape
        //        the eval actually consumes)
        //   replay --paired --compare3 --bootstrap [--corpus recorded] <f1.json> …
        //       (#35 §6: aggregate a whole corpus by the trace-clustered paired
        //        bootstrap and apply §8's kill/keep rule to the output)
        let mut compare = false;
        let mut compare3 = false;
        let mut session_mode = false;
        let mut calibration = false;
        let mut paired = false;
        let mut bootstrap = false;
        let mut provenance = hari_core::CorpusProvenance::Authored;
        let mut path: Option<&str> = None;
        let mut corpus: Vec<&str> = Vec::new();
        let mut i = 2;
        while i < args.len() {
            let a = &args[i];
            match a.as_str() {
                "--compare" => compare = true,
                "--compare3" => compare3 = true,
                "--session" => session_mode = true,
                "--calibration" => calibration = true,
                "--paired" => paired = true,
                "--bootstrap" => bootstrap = true,
                // §9.4's standing rule is enforced in code, so the caller has
                // to say where the corpus came from. The default is the
                // conservative one: everything committed in this repo today is
                // authored, and an authored corpus never yields a §8 verdict.
                "--corpus" => {
                    i += 1;
                    match args.get(i).map(String::as_str) {
                        Some("authored") => provenance = hari_core::CorpusProvenance::Authored,
                        Some("recorded") => {
                            provenance = hari_core::CorpusProvenance::DriverRecorded;
                        }
                        other => {
                            eprintln!(
                                "hari-core replay: --corpus takes `authored` or `recorded`, got {}",
                                other.unwrap_or("nothing")
                            );
                            process::exit(2);
                        }
                    }
                }
                other if !other.starts_with("--") => {
                    path = Some(other);
                    corpus.push(other);
                }
                other => {
                    eprintln!("hari-core replay: unknown flag {other}");
                    process::exit(2);
                }
            }
            i += 1;
        }
        // The bootstrap aggregates a corpus, so it is the one mode that takes
        // more than one path — and it only makes sense over the paired scorer.
        if bootstrap && !(paired && compare3) {
            eprintln!(
                "hari-core replay: --bootstrap requires --paired --compare3 (it aggregates the \
                 paired scores of several fixtures)"
            );
            process::exit(2);
        }
        if !bootstrap && corpus.len() > 1 {
            eprintln!(
                "hari-core replay: only --bootstrap takes more than one fixture path; got {}",
                corpus.len()
            );
            process::exit(2);
        }
        // `--paired --compare3` is the one legal combination: `--paired`
        // selects the scorer, `--compare3` selects how many arms it grades.
        // Every other pairing picks two incompatible output schemas.
        let paired_three_way = paired && compare3;
        let exclusive_count = [compare, compare3 && !paired, session_mode, paired]
            .iter()
            .filter(|x| **x)
            .count();
        if exclusive_count > 1 {
            eprintln!(
                "hari-core replay: --compare, --compare3, --session, and --paired are mutually \
                 exclusive (except --paired --compare3, which grades all three arms)"
            );
            process::exit(2);
        }
        // `--compare3` emits a three-arm wrapper object rather than a single
        // `ResearchReplayReport`, so it has nowhere to hang a calibration
        // block yet. Reject the combination loudly instead of silently
        // dropping the flag. Per-arm calibration is NOT "the next slice", as
        // this comment used to say — #35 §9 reclassified it behind items 3 and
        // 4, and §9.4 then made item 4 the sole remaining blocker.
        //
        // `--paired` is rejected for the same reason and one more: per-arm
        // calibration needs each arm to emit forecasts from its own posterior
        // (#35 §9.2), which does not exist yet. Silently dropping the flag
        // would let a paired run read as if it had been calibration-scored.
        if calibration && (compare3 || session_mode || paired) {
            eprintln!(
                "hari-core replay: --calibration is not yet supported with --compare3, --session, \
                 or --paired"
            );
            process::exit(2);
        }
        let result = if session_mode {
            replay_session(path)
        } else if bootstrap {
            replay_paired_bootstrap(&corpus, provenance)
        } else if paired_three_way {
            replay_paired_three_way(path)
        } else if compare3 {
            replay_trace_three_way(path)
        } else if paired {
            replay_paired(path)
        } else {
            replay_trace(path, compare, calibration)
        };
        if let Err(err) = result {
            eprintln!("hari-core replay failed: {err}");
            process::exit(1);
        }
        return;
    }

    if args.get(1).map(String::as_str) == Some("forecast") {
        // Jarvis Track J2 tracer bullet. Emit / resolve / calibration over
        // the append-only JSONL ledger (HARI_STATE_DIR/forecasts/).
        if let Err(err) = run_forecast_cli(&args[2..]) {
            eprintln!("hari-core forecast failed: {err}");
            process::exit(1);
        }
        return;
    }

    if args.get(1).map(String::as_str) == Some("reliability") {
        // Giskard Track G2 tracer bullet. Read-only report over GA's PR
        // grade cards; the pooled entry is the A/B baseline.
        if let Err(err) = run_reliability_cli(&args[2..]) {
            eprintln!("hari-core reliability failed: {err}");
            process::exit(1);
        }
        return;
    }

    if args.get(1).map(String::as_str) == Some("operator") {
        // Giskard Track G1 (ops face) tracer bullet. Seen/acknowledged/stale
        // registry over operator signals; `status` is read-only, surface/ack
        // append to the ledger.
        if let Err(err) = run_operator_cli(&args[2..]) {
            eprintln!("hari-core operator failed: {err}");
            process::exit(1);
        }
        return;
    }

    if args.get(1).map(String::as_str) == Some("source-reliability") {
        // Issue #14 tracer bullet. `emit` replays a trace and appends per-source
        // outcome rows to the cross-session ledger; `report` is read-only and
        // prints per-source reliability with the pooled A/B baseline.
        if let Err(err) = run_source_reliability_cli(&args[2..]) {
            eprintln!("hari-core source-reliability failed: {err}");
            process::exit(1);
        }
        return;
    }

    if args.get(1).map(String::as_str) == Some("serve") {
        // Phase 6 stdio JSONL service. Synchronous request/response.
        // Logs to stderr; protocol on stdin/stdout.
        let _ = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .with_target(false)
            .with_writer(std::io::stderr)
            .try_init();
        if let Err(err) = serve_stdio() {
            eprintln!("hari-core serve failed: {err}");
            process::exit(1);
        }
        return;
    }

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .init();

    run_substrate_decision_demo();
}

/// Jarvis Track J2 CLI: `hari-core forecast <emit|resolve|calibration> …`.
///
/// - `emit --belief <slug> --probability <p> --source <s> --field <f>
///   --predicate <pred> --horizon <rfc3339Z> [--rationale <text>]
///   [--supersedes <forecast_id>]` — append a pending record, print it.
/// - `resolve --source <s> --artifact <local-path> [--now <rfc3339Z>]` —
///   score every pending record past horizon whose observable.source
///   matches; unreadable artifact resolves as `void`, never dropped.
/// - `calibration` — per-belief Brier mean + count over the whole ledger.
/// - `check [--now <rfc3339Z>]` — read-only tripwire: exits nonzero listing
///   any forecast past horizon that still has no resolution, so unresolved
///   predictions fail CI instead of rotting silently (issue #29).
fn run_forecast_cli(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .map(String::as_str)
    }

    let dir = forecast::ledger_dir();
    match args.first().map(String::as_str) {
        Some("emit") => {
            let usage = "usage: hari-core forecast emit --belief <slug> --probability <p> \
                         --source <s> --field <f> --predicate <pred> --horizon <rfc3339Z> \
                         [--rationale <text>] [--supersedes <forecast_id>]";
            let belief = flag(args, "--belief").ok_or(usage)?;
            let probability: f64 = flag(args, "--probability").ok_or(usage)?.parse()?;
            if !(0.0..=1.0).contains(&probability) {
                return Err("probability must be in [0, 1]".into());
            }
            let observable = forecast::Observable {
                source: flag(args, "--source").ok_or(usage)?.to_string(),
                field: flag(args, "--field").ok_or(usage)?.to_string(),
                predicate: flag(args, "--predicate").ok_or(usage)?.to_string(),
            };
            let horizon = flag(args, "--horizon").ok_or(usage)?;
            if !forecast::is_canonical_utc(horizon) {
                return Err(format!(
                    "--horizon must be canonical YYYY-MM-DDTHH:MM:SSZ UTC \
                     (no offsets, no fractional seconds): {horizon:?}"
                )
                .into());
            }
            let record = forecast::emit(
                belief,
                observable,
                probability,
                flag(args, "--rationale").map(String::from),
                horizon,
                flag(args, "--supersedes").map(String::from),
                &forecast::rfc3339_now(),
            );
            let path = forecast::append(&dir, &record)?;
            println!("{}", serde_json::to_string_pretty(&record)?);
            info!("forecast appended to {}", path.display());
            Ok(())
        }
        Some("resolve") => {
            let usage = "usage: hari-core forecast resolve --source <s> \
                         --artifact <local-path> [--now <rfc3339Z>]";
            let source = flag(args, "--source").ok_or(usage)?;
            let artifact_path = flag(args, "--artifact").ok_or(usage)?;
            let now = flag(args, "--now")
                .map(String::from)
                .unwrap_or_else(forecast::rfc3339_now);
            if !forecast::is_canonical_utc(&now) {
                return Err(format!(
                    "--now must be canonical YYYY-MM-DDTHH:MM:SSZ UTC \
                     (no offsets, no fractional seconds): {now:?}"
                )
                .into());
            }
            // Unreadable artifact at horizon → void, per contract.
            let artifact: Option<serde_json::Value> = fs::read_to_string(artifact_path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok());
            if artifact.is_none() {
                warn!("artifact unreadable at {artifact_path}; due forecasts resolve as void");
            }
            let (records, skipped) = forecast::load(&dir)?;
            if skipped > 0 {
                warn!("ledger: skipped {skipped} malformed line(s)");
            }
            let mut resolved = Vec::new();
            for mut record in records {
                if !record.is_pending()
                    || record.observable.source != source
                    || !record.is_past_horizon(&now)
                {
                    continue;
                }
                record.resolution = Some(forecast::resolve(&record, artifact.as_ref(), &now));
                forecast::append(&dir, &record)?;
                resolved.push(record);
            }
            println!("{}", serde_json::to_string_pretty(&resolved)?);
            info!("resolved {} forecast(s)", resolved.len());
            Ok(())
        }
        Some("calibration") => {
            let (records, skipped) = forecast::load(&dir)?;
            if skipped > 0 {
                warn!("ledger: skipped {skipped} malformed line(s)");
            }
            println!(
                "{}",
                serde_json::to_string_pretty(&forecast::calibration(&records))?
            );
            Ok(())
        }
        Some("check") => {
            let now = flag(args, "--now")
                .map(String::from)
                .unwrap_or_else(forecast::rfc3339_now);
            if !forecast::is_canonical_utc(&now) {
                return Err(format!(
                    "--now must be canonical YYYY-MM-DDTHH:MM:SSZ UTC \
                     (no offsets, no fractional seconds): {now:?}"
                )
                .into());
            }
            let (records, skipped) = forecast::load(&dir)?;
            if skipped > 0 {
                warn!("ledger: skipped {skipped} malformed line(s)");
            }
            let overdue = forecast::overdue_unresolved(&records, &now);
            if overdue.is_empty() {
                println!("forecast check OK: no unresolved forecasts past horizon as of {now}");
                return Ok(());
            }
            eprintln!(
                "forecast check FAILED: {} unresolved forecast(s) past horizon as of {now}:",
                overdue.len()
            );
            for rec in &overdue {
                eprintln!(
                    "  {} belief={} horizon={} p={} — {} {} {}",
                    rec.forecast_id,
                    rec.belief_id,
                    rec.horizon,
                    rec.prediction.probability,
                    rec.observable.source,
                    rec.observable.field,
                    rec.observable.predicate,
                );
            }
            Err(format!(
                "{} forecast(s) overdue for resolution — run `forecast resolve` \
                 or supersede them",
                overdue.len()
            )
            .into())
        }
        _ => Err("usage: hari-core forecast <emit|resolve|calibration|check> …".into()),
    }
}

/// Giskard Track G2 CLI: `hari-core reliability <grades-dir> [--now <rfc3339Z>]`.
///
/// Reads every `pr-grade-v1` card in `<grades-dir>` (GA's
/// `state/quality/pr-grades/`) and prints the per-agent reliability report
/// as pretty JSON. Read-only: nothing is written anywhere. `--now` injects
/// the report timestamp for deterministic runs.
fn run_reliability_cli(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let usage = "usage: hari-core reliability <grades-dir> [--now <YYYY-MM-DDTHH:MM:SSZ>]";
    let dir = args
        .iter()
        .find(|a| !a.starts_with("--"))
        .ok_or(usage)?
        .as_str();
    let now = args
        .iter()
        .position(|a| a == "--now")
        .and_then(|i| args.get(i + 1))
        .map(String::to_string)
        .unwrap_or_else(forecast::rfc3339_now);
    if !forecast::is_canonical_utc(&now) {
        return Err(
            format!("--now must be canonical YYYY-MM-DDTHH:MM:SSZ UTC, got {now:?}").into(),
        );
    }
    let (cards, skipped) = reliability::load_grades(std::path::Path::new(dir))?;
    if skipped > 0 {
        warn!("grades dir: skipped {skipped} non-card file(s)");
    }
    let report = reliability::report(&cards, skipped, &now);
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

/// Giskard Track G1 CLI: `hari-core operator <surface|ack|status> …`.
///
/// - `surface --signal <id> --kind <k> --now <rfc3339Z> [--ttl-secs <n>]` —
///   append a `Surfaced` event (surfaced_at = `--now`), print it.
/// - `ack --signal <id> --now <rfc3339Z>` — append an `Acknowledged` event
///   (acknowledged_at = `--now`), print it.
/// - `status [--now <rfc3339Z>]` — read-only: fold the ledger and print each
///   signal's state + `should_notify` + `is_stale` as pretty JSON. `--now`
///   defaults to the wall clock.
///
/// All timestamps are validated against `forecast::is_canonical_utc`.
fn run_operator_cli(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .map(String::as_str)
    }
    fn require_canonical(now: &str) -> Result<(), Box<dyn std::error::Error>> {
        if !forecast::is_canonical_utc(now) {
            return Err(format!(
                "--now must be canonical YYYY-MM-DDTHH:MM:SSZ UTC \
                 (no offsets, no fractional seconds): {now:?}"
            )
            .into());
        }
        Ok(())
    }

    let dir = operator_model::ledger_dir();
    match args.first().map(String::as_str) {
        Some("surface") => {
            let usage = "usage: hari-core operator surface --signal <id> --kind <k> \
                         --now <rfc3339Z> [--ttl-secs <n>]";
            let signal = flag(args, "--signal").ok_or(usage)?;
            let kind = flag(args, "--kind").ok_or(usage)?;
            let now = flag(args, "--now").ok_or(usage)?;
            require_canonical(now)?;
            let ttl_secs = match flag(args, "--ttl-secs") {
                Some(s) => Some(s.parse::<u64>()?),
                None => None,
            };
            let event = operator_model::OperatorEvent::Surfaced {
                signal_id: signal.to_string(),
                kind: kind.to_string(),
                surfaced_at: now.to_string(),
                ttl_secs,
            };
            let path = operator_model::append(&dir, &event)?;
            println!("{}", serde_json::to_string_pretty(&event)?);
            info!("operator event appended to {}", path.display());
            Ok(())
        }
        Some("ack") => {
            let usage = "usage: hari-core operator ack --signal <id> --now <rfc3339Z>";
            let signal = flag(args, "--signal").ok_or(usage)?;
            let now = flag(args, "--now").ok_or(usage)?;
            require_canonical(now)?;
            let event = operator_model::OperatorEvent::Acknowledged {
                signal_id: signal.to_string(),
                acknowledged_at: now.to_string(),
            };
            let path = operator_model::append(&dir, &event)?;
            println!("{}", serde_json::to_string_pretty(&event)?);
            info!("operator event appended to {}", path.display());
            Ok(())
        }
        Some("status") => {
            let now = flag(args, "--now")
                .map(String::from)
                .unwrap_or_else(forecast::rfc3339_now);
            require_canonical(&now)?;
            let (events, skipped) = operator_model::load(&dir)?;
            if skipped > 0 {
                warn!("operator ledger: skipped {skipped} malformed line(s)");
            }
            let report = operator_model::status_report(&events, &now);
            println!("{}", serde_json::to_string_pretty(&report)?);
            Ok(())
        }
        _ => Err("usage: hari-core operator <surface|ack|status> …".into()),
    }
}

/// Issue #14 CLI: `hari-core source-reliability <emit|report> …`.
///
/// - `emit --trace <path> --session <id> [--now <rfc3339Z>]` — replay the trace
///   (default `RecencyDecay`, matching the plain `replay` command), derive one
///   per-source outcome row per source, append them to the cross-session ledger
///   (`HARI_STATE_DIR/source-reliability/`), and print them. Additive: the
///   trace's own `replay` output is unaffected.
/// - `report [--now <rfc3339Z>]` — read-only: aggregate the whole ledger and
///   print per-source reliability with the pooled A/B baseline and the
///   entrenchment ordering as pretty JSON.
///
/// All timestamps are validated against `forecast::is_canonical_utc`.
fn run_source_reliability_cli(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .map(String::as_str)
    }

    let dir = source_reliability::ledger_dir();
    match args.first().map(String::as_str) {
        Some("emit") => {
            let usage = "usage: hari-core source-reliability emit --trace <path> \
                         --session <id> [--now <rfc3339Z>]";
            let trace_path = flag(args, "--trace").ok_or(usage)?;
            let session = flag(args, "--session").ok_or(usage)?;
            let now = flag(args, "--now")
                .map(String::from)
                .unwrap_or_else(forecast::rfc3339_now);
            if !forecast::is_canonical_utc(&now) {
                return Err(format!(
                    "--now must be canonical YYYY-MM-DDTHH:MM:SSZ UTC \
                     (no offsets, no fractional seconds): {now:?}"
                )
                .into());
            }
            let trace = parse_trace(&fs::read_to_string(trace_path)?)?;
            let mut cognitive_loop = CognitiveLoop::new(trace.dimension);
            let report = cognitive_loop.process_research_trace(trace);
            let rows = source_reliability::outcomes_from_report(
                session,
                &now,
                &report.outcomes,
                &report.final_beliefs,
            );
            for row in &rows {
                source_reliability::append(&dir, row)?;
            }
            println!("{}", serde_json::to_string_pretty(&rows)?);
            info!(
                "appended {} source-outcome row(s) for session {session} to {}",
                rows.len(),
                dir.display()
            );
            Ok(())
        }
        Some("report") => {
            let now = flag(args, "--now")
                .map(String::from)
                .unwrap_or_else(forecast::rfc3339_now);
            if !forecast::is_canonical_utc(&now) {
                return Err(format!(
                    "--now must be canonical YYYY-MM-DDTHH:MM:SSZ UTC, got {now:?}"
                )
                .into());
            }
            let (rows, skipped) = source_reliability::load(&dir)?;
            if skipped > 0 {
                warn!("source-reliability ledger: skipped {skipped} malformed line(s)");
            }
            let report = source_reliability::report(&rows, skipped, &now);
            println!("{}", serde_json::to_string_pretty(&report)?);
            Ok(())
        }
        _ => Err("usage: hari-core source-reliability <emit|report> …".into()),
    }
}

/// Self-referential demo: Hari tracks the Phase 5 substrate decision
/// as a set of claims, with the four canonical agents voting and
/// declared relations driving derivation.
///
/// This is *not* Hari researching itself — Hari is the substrate;
/// the experiment author (the human + Claude in real life, this
/// hard-coded `events` Vec in the demo) is external. What it shows
/// is that the post-Phase-8 substrate can hold and reason about
/// claims of any kind, including claims about its own configuration.
///
/// Uses `PriorityModel::Flat` so every action shows up in the demo
/// output. The post-Phase-5 default is `RecencyDecay`, under which
/// log-only events (e.g., relation declarations on their own) get
/// suppressed to `Wait`. Switch via `with_model(...)` for production
/// use.
fn run_substrate_decision_demo() {
    info!("=== Project Hari — substrate-decision demo ===");
    info!("Hari tracks claims about its own Phase 5 substrate decision:");
    info!("  - the original Lie hypothesis");
    info!("  - the SL prior-art finding that disconfirmed it");
    info!("  - the demote-Lie / RecencyDecay-default conclusion");
    info!("Provenance + trust-aware consensus drive the derivation.");

    let mut loop_ = CognitiveLoop::with_model(4, PriorityModel::Flat);
    loop_.use_swarm_consensus = true;
    loop_.trust_model = TrustModel::RoleWeighted;

    // Seed the four canonical agent roles into the swarm with explicit
    // trust profiles. The bridge will route their AgentVote events
    // into this swarm and recompute consensus under RoleWeighted.
    for (id, name, self_trust, message_trust) in [
        ("guardian", "guardian", 0.95, 0.4),
        ("critic", "critic", 0.9, 0.3),
        ("explorer", "explorer", 0.4, 0.85),
        ("integrator", "integrator", 0.7, 0.7),
    ] {
        loop_.swarm.add_agent(Agent::new(
            id,
            AgentRole {
                name: name.to_string(),
                self_trust,
                message_trust,
            },
        ));
    }
    info!(
        "Loop initialized: model={:?}, trust={:?}, swarm={} agents",
        loop_.priority_model,
        loop_.trust_model,
        loop_.swarm.len()
    );

    // The script: a sequence of research events that walks Hari
    // through the Phase 5 substrate decision as claims.
    let events: Vec<ResearchEvent> = vec![
        // Goal: settle the substrate question
        ResearchEvent {
            cycle: 1,
            source: "owner".to_string(),
            payload: ResearchEventPayload::GoalUpdate {
                key: "decide-cognition-substrate".to_string(),
                description: "Settle whether Lie or a simpler baseline should be default"
                    .to_string(),
                priority: 0.95,
                status: Some(HexValue::Unknown),
            },
        },
        // Logical structure of the decision: SL beating Lie should
        // contradict the original Lie-superiority hypothesis, and
        // imply the demotion + RecencyDecay-default conclusions.
        ResearchEvent {
            cycle: 2,
            source: "ix-modeller".to_string(),
            payload: ResearchEventPayload::RelationDeclaration {
                from: "sl-beats-lie-on-false-acceptance".to_string(),
                to: "lie-beats-simpler-baselines".to_string(),
                relation: Relation::Contradicts,
            },
        },
        ResearchEvent {
            cycle: 3,
            source: "ix-modeller".to_string(),
            payload: ResearchEventPayload::RelationDeclaration {
                from: "sl-beats-lie-on-false-acceptance".to_string(),
                to: "should-demote-lie".to_string(),
                relation: Relation::Implies,
            },
        },
        ResearchEvent {
            cycle: 4,
            source: "ix-modeller".to_string(),
            payload: ResearchEventPayload::RelationDeclaration {
                from: "should-demote-lie".to_string(),
                to: "recency-decay-safe-default".to_string(),
                relation: Relation::Implies,
            },
        },
        // The original hypothesis lands as Probable.
        ResearchEvent {
            cycle: 5,
            source: "ix-original-hypothesis".to_string(),
            payload: ResearchEventPayload::BeliefUpdate {
                proposition: "lie-beats-simpler-baselines".to_string(),
                value: HexValue::Probable,
                evidence: Default::default(),
            },
        },
        // The Phase 5 data arrives — SL beats Lie. This triggers
        // propagation: Contradicts flips lie-beats-simpler-baselines to
        // Contradictory; Implies derives should-demote-lie = True;
        // chained Implies derives recency-decay-safe-default = True.
        ResearchEvent {
            cycle: 6,
            source: "ix-experiment-runner".to_string(),
            payload: ResearchEventPayload::ExperimentResult {
                proposition: "sl-beats-lie-on-false-acceptance".to_string(),
                value: HexValue::True,
                evidence: {
                    let mut e = std::collections::BTreeMap::new();
                    e.insert("fixtures_won".to_string(), serde_json::Value::from(3));
                    e.insert("fixtures_tied".to_string(), serde_json::Value::from(3));
                    e.insert("fixtures_lost".to_string(), serde_json::Value::from(0));
                    e
                },
            },
        },
        // The four agents vote. Under RoleWeighted, guardian + critic
        // (high self_trust) carry more weight than explorer (low).
        ResearchEvent {
            cycle: 7,
            source: "guardian".to_string(),
            payload: ResearchEventPayload::AgentVote {
                proposition: "should-demote-lie".to_string(),
                value: HexValue::Probable,
                evidence: Default::default(),
            },
        },
        ResearchEvent {
            cycle: 8,
            source: "critic".to_string(),
            payload: ResearchEventPayload::AgentVote {
                proposition: "should-demote-lie".to_string(),
                value: HexValue::Probable,
                evidence: Default::default(),
            },
        },
        ResearchEvent {
            cycle: 9,
            source: "explorer".to_string(),
            payload: ResearchEventPayload::AgentVote {
                proposition: "should-demote-lie".to_string(),
                value: HexValue::Doubtful,
                evidence: Default::default(),
            },
        },
        ResearchEvent {
            cycle: 10,
            source: "integrator".to_string(),
            payload: ResearchEventPayload::AgentVote {
                proposition: "should-demote-lie".to_string(),
                value: HexValue::Probable,
                evidence: Default::default(),
            },
        },
    ];

    for event in events {
        info!("");
        info!("--- cycle {} ({}) ---", event.cycle, event.source);
        let outcome = loop_.process_research_event(event);
        for action in &outcome.actions {
            match action {
                Action::Escalate { reason, confidence } => {
                    warn!("  ESCALATE: {} (confidence: {:.2})", reason, confidence);
                }
                _ => info!("  -> {}", action),
            }
        }
        if !outcome.derivations.is_empty() {
            info!("  derivations:");
            for d in &outcome.derivations {
                let chain: Vec<String> = d
                    .contributions
                    .iter()
                    .map(|c| format!("{}({:?},{:?})", c.source, c.relation, c.source_value))
                    .collect();
                info!(
                    "    {}: {:?} -> {:?} (round {}) from [{}]",
                    d.proposition,
                    d.previous_value,
                    d.new_value,
                    d.round,
                    chain.join(", ")
                );
            }
        }
    }

    // --- Final substrate-decision conclusion ---
    info!("");
    info!("=== Final substrate-decision belief state ===");
    for prop in [
        "lie-beats-simpler-baselines",
        "sl-beats-lie-on-false-acceptance",
        "should-demote-lie",
        "recency-decay-safe-default",
    ] {
        let value = loop_
            .state
            .beliefs
            .get(prop)
            .map(|b| format!("{}", b.value))
            .unwrap_or_else(|| "<not in network>".to_string());
        info!("  {}: {}", prop, value);
    }

    // Swarm consensus tally for the decision claim, both modes side by
    // side — shows how trust-weighting changes the head count.
    info!("");
    info!("=== Swarm consensus on 'should-demote-lie' ===");
    let equal = loop_
        .swarm
        .consensus_with("should-demote-lie", TrustModel::Equal);
    let weighted = loop_
        .swarm
        .consensus_with("should-demote-lie", TrustModel::RoleWeighted);
    info!(
        "  Equal (1-vote-per-agent):     {} (agreement: {:.0}%)",
        equal.consensus,
        equal.agreement * 100.0
    );
    info!(
        "  RoleWeighted (by self_trust): {} (agreement: {:.0}%)",
        weighted.consensus,
        weighted.agreement * 100.0
    );

    info!("");
    info!("=== demo complete ===");
    info!("Hari held the substrate decision as a tracked claim, derived");
    info!("its conclusion from the relation graph + experimental evidence,");
    info!("and reflected the four-agent vote under trust-weighted consensus.");
    info!("The researcher (you, plus Claude in this conversation) is");
    info!("external — Hari is the substrate, not the autoresearch system.");
}

fn replay_trace(
    path: Option<&str>,
    compare: bool,
    calibration: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = path.ok_or("usage: hari-core replay [--compare] [--calibration] <trace.json>")?;
    let trace_json = fs::read_to_string(path)?;
    let trace = parse_trace(&trace_json)?;

    let report = if compare {
        compare_replay(trace)
    } else {
        let mut cognitive_loop = CognitiveLoop::new(trace.dimension);
        cognitive_loop.process_research_trace(trace)
    };
    // The ledger read lives here, at the CLI edge, so the replay path itself
    // stays pure and deterministic (#35 §9.2). A missing ledger dir is not an
    // error — an empty record set yields an all-zero calibration block, which
    // is the honest answer for "nothing has been forecast yet".
    let report = if calibration {
        let (records, skipped) = forecast::load(&forecast::ledger_dir())?;
        if skipped > 0 {
            warn!("forecast ledger: skipped {skipped} unparsable record(s)");
        }
        report.with_calibration(&records)
    } else {
        report
    };
    serde_json::to_writer_pretty(std::io::stdout(), &report)?;
    println!();

    Ok(())
}

/// `replay --paired` — replay a [`PairedFixture`] and score the #35 §5.1
/// primary metric against the ground truth bundled with it.
///
/// Emits `{ "report": …, "paired": … }` so the graded report stays inspectable
/// beside its score. Scoring is pure: the labels travel in the fixture, so no
/// ledger or state directory is read.
fn replay_paired(path: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    let path = path.ok_or("usage: hari-core replay --paired <paired-fixture.json>")?;
    let fixture: hari_core::PairedFixture = serde_json::from_str(&fs::read_to_string(path)?)?;

    let mut cognitive_loop = CognitiveLoop::new(fixture.trace.dimension);
    let report = cognitive_loop.process_research_trace(fixture.trace);
    let paired = hari_core::score_paired(&report, &fixture.labels);
    let false_rejections = hari_core::score_false_rejections(&report, &fixture.claims);

    // Defects are the reason a pair went ungraded. Surfacing them on stderr
    // means a fixture that silently grades 3 of 10 pairs cannot pass unnoticed.
    for defect in &paired.defects {
        warn!("paired fixture defect: {defect:?}");
    }
    if paired.is_ungraded() {
        warn!("no pair was gradeable — paired_accuracy is absent, which is not a score of 0.0");
    }
    if !false_rejections.unlabeled.is_empty() {
        warn!(
            "waits on unlabeled claims (no ground truth, NOT excused): {:?}",
            false_rejections.unlabeled
        );
    }

    serde_json::to_writer_pretty(
        std::io::stdout(),
        &serde_json::json!({
            "report": report,
            "paired": paired,
            // Arm-independent (§5.3). `report.metrics.false_rejection_count` is
            // the intrinsic, self-resolved counter — not comparable across arms.
            "false_rejections": false_rejections,
        }),
    )?;
    println!();
    Ok(())
}

/// `replay --paired --compare3` — grade one label set against all three arms.
///
/// This is the shape #35 §5.1 consumes: the primary metric is defined per arm,
/// so a single-arm score answers a question the pre-registration never asks.
fn replay_paired_three_way(path: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    let path = path.ok_or("usage: hari-core replay --paired --compare3 <paired-fixture.json>")?;
    let fixture: hari_core::PairedFixture = serde_json::from_str(&fs::read_to_string(path)?)?;
    let graded = hari_core::score_paired_all_arms(fixture, SubjectiveLogicConfig::default());

    // Defects derive from the label set and the outcome count, both of which
    // are arm-independent — so reporting them once per arm would be noise.
    // Warn per arm anyway *if* they differ, because that difference would mean
    // the arms are being graded against different decisions.
    for (name, arm) in [
        ("ix_unassisted", &graded.unassisted),
        ("recency_decay", &graded.recency_decay),
        ("lie", &graded.lie),
        ("subjective_logic", &graded.subjective_logic),
    ] {
        for defect in &arm.paired.defects {
            warn!("paired fixture defect [{name}]: {defect:?}");
        }
        if arm.paired.is_ungraded() {
            warn!("[{name}] no pair was gradeable — paired_accuracy is absent, not 0.0");
        }
        if !arm.false_rejections.unlabeled.is_empty() {
            warn!(
                "[{name}] waits on unlabeled claims (no ground truth, NOT excused): {:?}",
                arm.false_rejections.unlabeled
            );
        }
    }
    // A §9.3 fixture exists to discriminate between the arms. One that grades
    // all three identically has measured nothing, and saying so here is the
    // difference between a null result and an unnoticed dud.
    if graded.is_undiscriminating() {
        warn!("no pair separated the arms — this fixture does not discriminate between the models");
    }

    serde_json::to_writer_pretty(std::io::stdout(), &graded)?;
    println!();
    Ok(())
}

/// `replay --paired --compare3 --bootstrap <f1.json> …` — #35 §6 and §8.
///
/// Replays each paired fixture through the existing `--paired --compare3`
/// boundary, treats **one fixture as one trace cluster** (§6's resampling
/// unit), aggregates by the trace-clustered paired bootstrap, and applies §8's
/// kill/keep rule mechanically to the result.
///
/// Two comparisons are emitted, both of which the pre-registration names:
/// `RecencyDecay` (the §9.5 `experimental` arm) against `IX-unassisted` — which
/// is §8 clause 1 itself — and `SubjectiveLogic` against `RecencyDecay`, the
/// §4 "does the expensive machinery beat the cheap machinery" comparison.
///
/// The per-fixture detail stays reachable through `--paired --compare3` on a
/// single file; this mode emits accuracies only, so the corpus roll-up is
/// readable.
fn replay_paired_bootstrap(
    paths: &[&str],
    provenance: hari_core::CorpusProvenance,
) -> Result<(), Box<dyn std::error::Error>> {
    if paths.is_empty() {
        return Err("usage: hari-core replay --paired --compare3 --bootstrap \
                    [--corpus authored|recorded] <fixture.json> …"
            .into());
    }

    // §9.3.2: the isolation and task corpora are never pooled. Refuse rather
    // than silently average one into the other.
    let stems: Vec<&str> = paths
        .iter()
        .map(|p| {
            std::path::Path::new(p)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(p)
        })
        .collect();
    if let Some((isolation, task)) = hari_core::pooling_violation(&stems) {
        return Err(format!(
            "#35 §9.3.2 forbids pooling the isolation and task corpora — got both `{isolation}` \
             and `{task}`. Run them separately."
        )
        .into());
    }

    let mut summaries = Vec::new();
    let mut clause1_clusters = Vec::new();
    let mut cheap_clusters = Vec::new();

    for path in paths {
        let fixture: hari_core::PairedFixture = serde_json::from_str(&fs::read_to_string(path)?)?;
        let graded = hari_core::score_paired_all_arms(fixture, SubjectiveLogicConfig::default());

        // One fixture is one cluster. The cluster id is the file stem, so a
        // reader can trace any resampling unit back to the file it came from.
        let trace = std::path::Path::new(path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(path);

        for (name, arm) in [
            ("ix_unassisted", &graded.unassisted),
            ("recency_decay", &graded.recency_decay),
            ("lie", &graded.lie),
            ("subjective_logic", &graded.subjective_logic),
        ] {
            for defect in &arm.paired.defects {
                warn!("paired fixture defect [{trace}/{name}]: {defect:?}");
            }
            if arm.paired.is_ungraded() {
                warn!("[{trace}/{name}] no pair was gradeable — this trace contributes nothing");
            }
        }

        clause1_clusters.push(hari_core::cluster_from_arms(
            trace,
            &graded.recency_decay,
            &graded.unassisted,
        ));
        cheap_clusters.push(hari_core::cluster_from_arms(
            trace,
            &graded.subjective_logic,
            &graded.recency_decay,
        ));
        summaries.push(serde_json::json!({
            "fixture": path,
            "trace": trace,
            "pairs": graded.recency_decay.paired.pairs,
            "paired_accuracy": {
                "ix_unassisted": graded.unassisted.paired.paired_accuracy,
                "recency_decay": graded.recency_decay.paired.paired_accuracy,
                "lie": graded.lie.paired.paired_accuracy,
                "subjective_logic": graded.subjective_logic.paired.paired_accuracy,
            },
            "false_acceptances": {
                "ix_unassisted": graded.unassisted.false_acceptances.false_acceptances,
                "recency_decay": graded.recency_decay.false_acceptances.false_acceptances,
                "subjective_logic": graded.subjective_logic.false_acceptances.false_acceptances,
            },
            "false_rejections": {
                "ix_unassisted": graded.unassisted.false_rejections.false_rejections,
                "recency_decay": graded.recency_decay.false_rejections.false_rejections,
                "subjective_logic": graded.subjective_logic.false_rejections.false_rejections,
            },
            "separating_pairs": graded.separating_pairs.len(),
        }));
    }

    let config = hari_core::BootstrapConfig::default();
    let clause1 = hari_core::bootstrap_paired_difference(
        "recency_decay",
        "ix_unassisted",
        &clause1_clusters,
        config,
    );
    let cheap = hari_core::bootstrap_paired_difference(
        "subjective_logic",
        "recency_decay",
        &cheap_clusters,
        config,
    );

    // §8 clause 2 reads per-arm calibration, which needs each arm to emit
    // forecasts from its own posterior (#35 §9 item 2). That does not exist, so
    // the clause is passed `None` and reports `undefined` — never silently
    // treated as satisfied.
    let decision = hari_core::apply_kill_keep(clause1.as_ref(), None, provenance);

    if let Some(b) = &clause1 {
        if b.zero_between_cluster_variance {
            warn!(
                "the corpus has zero between-cluster variance ({} distinct delta sequences over \
                 {} traces) — the interval is a design artifact, not a sampling one (§9.4)",
                b.distinct_cluster_signatures, b.clusters
            );
        }
    }
    for line in &decision.rationale {
        info!("{line}");
    }

    serde_json::to_writer_pretty(
        std::io::stdout(),
        &serde_json::json!({
            "corpus": summaries,
            "bootstrap": {
                "clause1_experimental_vs_unassisted": clause1,
                "cheap_baseline_sl_vs_experimental": cheap,
            },
            "kill_keep": decision,
        }),
    )?;
    println!();
    Ok(())
}

/// `replay --compare3` — runs the trace through `RecencyDecay`, `Lie`,
/// and the Subjective Logic baseline on fresh state. Emits a wrapper
/// JSON object so the existing `--compare` schema stays untouched (the
/// Phase 6 replay-parity test asserts the unchanged shape).
fn replay_trace_three_way(path: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    let path = path.ok_or("usage: hari-core replay --compare3 <trace.json>")?;
    let trace_json = fs::read_to_string(path)?;
    let trace = parse_trace(&trace_json)?;

    let report = compare_replay_three_way(trace, SubjectiveLogicConfig::default());
    serde_json::to_writer_pretty(std::io::stdout(), &report)?;
    println!();
    Ok(())
}

/// Phase 6 — replay a recorded session-trace file (the JSONL written by
/// `serve` when `trace_record_path` is set). Reads the file, requires
/// the first line to be an `Open` request, then re-feeds every
/// subsequent `Event` through `StreamingSession::apply_event`. `Metrics`
/// and `Close` lines are no-ops on replay (they only observed state).
///
/// Emits the resulting `ResearchReplayReport` as pretty JSON to stdout,
/// matching the format of the existing batch `replay` command.
fn replay_session(path: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    let path = path.ok_or("usage: hari-core replay --session <session.jsonl>")?;
    let raw = fs::read_to_string(path)?;
    let mut lines = raw.lines();
    let header_line = lines
        .next()
        .ok_or("session trace is empty (no open header)")?;
    let header: Request = serde_json::from_str(header_line)
        .map_err(|e| format!("first line must be an open request: {e}"))?;
    let mut config = match header {
        Request::Open { config } => config,
        other => {
            return Err(format!(
                "first line must be `op: open`, got: {}",
                serde_json::to_string(&other).unwrap_or_default()
            )
            .into());
        }
    };
    // Replay must NOT re-record into trace_record_path (the file we're
    // currently reading). Strip it.
    config.trace_record_path = None;
    let mut session = StreamingSession::open(config)?;
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        // Tolerate non-Request lines (e.g. trailing close-marker objects
        // emitted by unclean-shutdown handling) by ignoring deserialize
        // errors that aren't Event-shaped.
        let req: Request = match serde_json::from_str(line) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("replay --session: skipping malformed line: {e}");
                continue;
            }
        };
        match req {
            Request::Event { event } => {
                if let Err(e) = session.apply_event(event) {
                    eprintln!("replay --session: event rejected: {e}");
                }
            }
            Request::Metrics
            | Request::Close
            | Request::Reliability { .. }
            | Request::OperatorStatus { .. } => {
                // Observation-only / lifecycle markers — no-ops on replay.
            }
            Request::Open { .. } => {
                return Err("session trace contains a second `open` request".into());
            }
        }
    }
    let report = session.close();
    serde_json::to_writer_pretty(std::io::stdout(), &report)?;
    println!();
    Ok(())
}

/// Phase 6 — stdio JSONL service. Reads `Request` lines from stdin,
/// writes `Response` lines to stdout. Errors that should be visible to
/// the IX consumer are returned as `Response::Error`; structural /
/// I/O errors that prevent further protocol use go to stderr and exit
/// non-zero.
///
/// Implements the failure-mode table from `phase6-design.md` §6:
/// malformed JSON → invalid_json non-fatal; unknown op → unknown_op
/// non-fatal; double-open → already_open non-fatal; out-of-order cycle
/// → out_of_order_cycle non-fatal; EOF mid-session → write a final
/// `closed` response with `unclean: true` plus a close marker to the
/// trace file.
fn serve_stdio() -> Result<(), Box<dyn std::error::Error>> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let reader = BufReader::new(stdin.lock());
    let mut writer = BufWriter::new(stdout.lock());

    let mut session: Option<StreamingSession> = None;

    for line_result in reader.lines() {
        let line = match line_result {
            Ok(l) => l,
            Err(e) => {
                // Hard I/O error on stdin — finalize unclean and exit.
                eprintln!("hari-core serve: stdin read error: {e}");
                if let Some(s) = session.take() {
                    write_unclean_close(&mut writer, s)?;
                }
                return Err(e.into());
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        let req: Request = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let resp = Response::Error {
                    request_op: None,
                    code: "invalid_json".into(),
                    message: format!("could not parse request line: {e}"),
                    fatal: false,
                };
                write_response(&mut writer, &resp)?;
                continue;
            }
        };
        let response = handle_request(&mut session, req);
        write_response(&mut writer, &response)?;
        // If this was a close that succeeded, drop the session.
        if let Response::Closed { .. } = &response {
            session = None;
        }
    }

    // EOF on stdin. If a session was still live, finalize unclean.
    if let Some(s) = session.take() {
        write_unclean_close(&mut writer, s)?;
    }
    writer.flush()?;
    Ok(())
}

fn handle_request(session: &mut Option<StreamingSession>, req: Request) -> Response {
    match req {
        Request::Open { config } => {
            if session.is_some() {
                return StreamingSession::make_error(
                    "open",
                    "already_open",
                    "session already open; multi-session is out of scope (see phase6-design.md §6)",
                    false,
                );
            }
            match StreamingSession::open(config) {
                Ok(s) => {
                    let opened = Response::Opened {
                        session_id: s.session_id().to_string(),
                        trace_path: s.trace_path().map(|p| p.to_path_buf()),
                        hari_version: env!("CARGO_PKG_VERSION").to_string(),
                        config_echo: s.config().clone(),
                    };
                    *session = Some(s);
                    opened
                }
                Err(e) => StreamingSession::make_error("open", "trace_io", e, true),
            }
        }
        Request::Event { event } => {
            let Some(s) = session.as_mut() else {
                return StreamingSession::make_error(
                    "event",
                    "no_session",
                    "no open session; send `op: open` first",
                    false,
                );
            };
            if s.is_closed() {
                return StreamingSession::make_error(
                    "event",
                    "session_closed",
                    "session has been closed; events are no longer accepted",
                    false,
                );
            }
            // Record the event in the trace file before applying so a
            // panic mid-apply still leaves the request on disk.
            if let Err(e) = s.record_request(&Request::Event {
                event: event.clone(),
            }) {
                return StreamingSession::make_error(
                    "event",
                    "trace_io",
                    format!("failed to record event: {e}"),
                    true,
                );
            }
            match s.apply_event(event) {
                Ok(rec) => Response::Recommendation(rec),
                Err(e) => {
                    // Out-of-order cycle / session_closed — non-fatal.
                    let code = e
                        .split(':')
                        .next()
                        .unwrap_or("invalid_event")
                        .trim()
                        .to_string();
                    StreamingSession::make_error("event", &code, e, false)
                }
            }
        }
        Request::Metrics => {
            let Some(s) = session.as_mut() else {
                return StreamingSession::make_error(
                    "metrics",
                    "no_session",
                    "no open session",
                    false,
                );
            };
            if let Err(e) = s.record_request(&Request::Metrics) {
                return StreamingSession::make_error(
                    "metrics",
                    "trace_io",
                    format!("failed to record metrics request: {e}"),
                    true,
                );
            }
            let (metrics, beliefs, goals) = s.metrics_snapshot();
            Response::MetricsSnapshot {
                metrics,
                beliefs,
                goals,
            }
        }
        Request::Close => {
            let Some(s) = session.as_mut() else {
                return StreamingSession::make_error(
                    "close",
                    "no_session",
                    "no open session to close",
                    false,
                );
            };
            if let Err(e) = s.record_request(&Request::Close) {
                return StreamingSession::make_error(
                    "close",
                    "trace_io",
                    format!("failed to record close: {e}"),
                    true,
                );
            }
            s.mark_closed();
            let final_report = s.snapshot_report();
            Response::Closed {
                final_report,
                unclean: false,
            }
        }
        Request::Reliability { grades_dir, now } => {
            // Stateless read-only (Giskard Track G2): no session
            // required, and nothing is recorded to any session trace —
            // the report never influences replay parity.
            let now = match now {
                Some(n) if !forecast::is_canonical_utc(&n) => {
                    return StreamingSession::make_error(
                        "reliability",
                        "invalid_timestamp",
                        format!("now must be canonical YYYY-MM-DDTHH:MM:SSZ UTC, got {n:?}"),
                        false,
                    );
                }
                Some(n) => n,
                None => forecast::rfc3339_now(),
            };
            match reliability::load_grades(&grades_dir) {
                Ok((cards, skipped)) => Response::ReliabilityReport {
                    report: reliability::report(&cards, skipped, &now),
                },
                Err(e) => StreamingSession::make_error(
                    "reliability",
                    "grades_io",
                    format!("cannot read grades dir {}: {e}", grades_dir.display()),
                    false,
                ),
            }
        }
        Request::OperatorStatus { now } => {
            // Stateless read-only (Giskard Track G1 ops face): no session
            // required, and nothing is recorded to any session trace —
            // the report never influences replay parity.
            let now = match now {
                Some(n) if !forecast::is_canonical_utc(&n) => {
                    return StreamingSession::make_error(
                        "operator_status",
                        "invalid_timestamp",
                        format!("now must be canonical YYYY-MM-DDTHH:MM:SSZ UTC, got {n:?}"),
                        false,
                    );
                }
                Some(n) => n,
                None => forecast::rfc3339_now(),
            };
            let dir = operator_model::ledger_dir();
            match operator_model::load(&dir) {
                Ok((events, skipped)) => {
                    if skipped > 0 {
                        warn!("operator ledger: skipped {skipped} malformed line(s)");
                    }
                    Response::OperatorStatus {
                        report: operator_model::status_report(&events, &now),
                    }
                }
                Err(e) => StreamingSession::make_error(
                    "operator_status",
                    "ledger_io",
                    format!("cannot read operator ledger {}: {e}", dir.display()),
                    false,
                ),
            }
        }
    }
}

/// On EOF mid-session, write an unclean close marker to the trace file
/// (so `replay --session` knows to stop applying events at that
/// boundary), then emit a final `closed` response with `unclean: true`
/// to stdout.
fn write_unclean_close<W: Write>(writer: &mut W, mut session: StreamingSession) -> io::Result<()> {
    // Append an explicit close-marker line to the trace file (best
    // effort; record_request handles None recorder transparently).
    let _ = session.record_request(&Request::Close);
    session.mark_closed();
    let final_report = session.snapshot_report();
    let resp = Response::Closed {
        final_report,
        unclean: true,
    };
    write_response(writer, &resp)
}

fn write_response<W: Write>(writer: &mut W, resp: &Response) -> io::Result<()> {
    let s =
        serde_json::to_string(resp).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    writer.write_all(s.as_bytes())?;
    writer.write_all(b"\n")?;
    writer.flush()
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
        let round_tripped: hari_core::ResearchReplayReport = serde_json::from_str(&s).unwrap();
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
    fn parse_trace_accepts_revision_payloads_in_both_forms() {
        // Belief-revision variants (issue #16): the `retraction` variant's
        // additive `retracts` selector and the new `supersession`,
        // `correction`, and `relation_withdrawal` variants must deserialize
        // through BOTH parse_trace paths (object + array), per the CLAUDE.md
        // rule to keep the two in lockstep.
        let object_form = parse_trace(
            r#"{
                "dimension": 4,
                "events": [
                    { "cycle": 1, "source": "ix", "payload": {
                        "type": "retraction", "proposition": "p", "reason": "stale",
                        "retracts": { "source": "ix", "cycle": 1 } } },
                    { "cycle": 2, "source": "ix", "payload": {
                        "type": "supersession", "proposition": "p",
                        "superseded_by": "q", "reason": "grew" } },
                    { "cycle": 3, "source": "ix", "payload": {
                        "type": "correction", "proposition": "p", "reason": "mislabeled",
                        "retracts": { "source": "ix", "cycle": 1 }, "value": "Probable",
                        "evidence": { "note": "re-measured" } } },
                    { "cycle": 4, "source": "ix", "payload": {
                        "type": "relation_withdrawal", "from": "p", "to": "q",
                        "relation": "Supports", "reason": "no longer supports" } }
                ]
            }"#,
        )
        .unwrap();
        assert_eq!(object_form.events.len(), 4);

        let array_form = parse_trace(
            r#"[
                { "cycle": 1, "source": "ix", "payload": {
                    "type": "retraction", "proposition": "p", "reason": "stale",
                    "retracts": { "cycle": 1 } } },
                { "cycle": 2, "source": "ix", "payload": {
                    "type": "supersession", "proposition": "p",
                    "superseded_by": "q", "reason": "grew" } },
                { "cycle": 3, "source": "ix", "payload": {
                    "type": "correction", "proposition": "p", "reason": "mislabeled",
                    "retracts": { "cycle": 1 }, "value": "Probable" } },
                { "cycle": 4, "source": "ix", "payload": {
                    "type": "relation_withdrawal", "from": "p", "to": "q",
                    "relation": "Supports", "reason": "no longer supports" } }
            ]"#,
        )
        .unwrap();
        assert_eq!(array_form.events.len(), 4);
        assert_eq!(array_form.dimension, 4);
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
