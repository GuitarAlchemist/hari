//! Entrenchment counterfactual — issue #14 consumer EXPERIMENT.
//!
//! Read-only science that INFORMS (does not decide) the owner's pending #14
//! consumer call. It asks one falsifiable question: does weighting base
//! evidence by a source's *earned* reliability (the entrenchment ordering from
//! [`source_reliability`]) beat today's uniform weighting on the roadmap's
//! primary metric, `false_acceptance_count`?
//!
//! The A/B is honest:
//! - **arm U (uniform)** — today's behaviour, every evidence entry weight `1.0`.
//! - **arm R (reliability)** — evidence weight = the source's cross-session
//!   `smoothed_precision`, learned in Phase A, injected via the additive opt-in
//!   [`CognitiveLoop::set_source_weights`] hook (default `None` = arm U,
//!   byte-identical, pinned by the lib unit test).
//!
//! Two corpora, reported and asserted **separately, never blended** into one
//! headline number:
//! 1. the real replayable traces (`fixtures/ix`, `fixtures/revision`,
//!    `fixtures/demerzel`) — small, so the honest expectation is a *null* (the
//!    arms cannot separate); this test pins that null so a future change that
//!    makes reliability bite on the real corpus is visible;
//! 2. a deterministic, documented **synthetic family** with known source
//!    quality (a globally-reliable `clean` source and a globally-unreliable
//!    `noisy` one), built to give the mechanism statistical teeth and to expose
//!    *both* signs of the effect.
//!
//! The load-bearing structural finding this harness pins: a per-source evidence
//! `weight` reaches a belief value **only** through the merge's
//! Contradictory-escalation share **during a ledger recompute** (a selective
//! retraction / correction). It never touches the upstream accept/escalate
//! scoring path, so weighting base evidence alone cannot change *which* claims
//! are Accepted in the first place — only what a belief recomputes to after a
//! revision. That is why the primary `false_acceptance_count` metric barely
//! moves even when final beliefs diverge, and it is the single most important
//! input to the owner's "where does entrenchment wire in?" decision.

use hari_core::{source_reliability, CognitiveLoop, ResearchTrace};
use hari_lattice::HexValue;
use std::collections::BTreeMap;
use std::fs;

/// Injected timestamps — the ledger is never written to disk here, but
/// `outcomes_from_report` / `report` require canonical UTC.
const RECORDED_AT: &str = "2026-07-20T12:00:00Z";
const GENERATED_AT: &str = "2026-07-20T18:00:00Z";

/// Per-arm measurement of one finished replay.
#[derive(Debug, Clone)]
struct ArmResult {
    /// Roadmap primary metric (`ReplayMetrics::false_acceptance_count`).
    false_acceptance_count: u32,
    /// Escalations whose proposition nonetheless ended True/Probable, summed
    /// over sources (the source-reliability "cried wolf" tally) for THIS arm.
    false_escalations: u32,
    /// Total escalations attributed to a source's proposition.
    escalations: u32,
    /// Total `Accept` actions attributed to sources.
    accepted: u32,
    /// Final authoritative belief per touched proposition.
    final_beliefs: BTreeMap<String, HexValue>,
}

/// Replay one trace under arm U (`weights = None`) or arm R (`Some`).
fn replay_arm(trace: &ResearchTrace, weights: Option<&BTreeMap<String, f64>>) -> ArmResult {
    let mut loop_ = CognitiveLoop::new(trace.dimension);
    if let Some(w) = weights {
        loop_.set_source_weights(w.clone());
    }
    let report = loop_.process_research_trace(trace.clone());
    // Re-derive the per-source outcome rows for THIS arm's own final beliefs,
    // so false_escalations reflect this arm's recomputed end-states.
    let rows = source_reliability::outcomes_from_report(
        "arm",
        RECORDED_AT,
        &report.outcomes,
        &report.final_beliefs,
    );
    ArmResult {
        false_acceptance_count: report.metrics.false_acceptance_count,
        false_escalations: rows.iter().map(|r| r.false_escalations).sum(),
        escalations: rows.iter().map(|r| r.escalations).sum(),
        accepted: rows.iter().map(|r| r.accepted).sum(),
        final_beliefs: report.final_beliefs,
    }
}

/// Phase A: replay a corpus with uniform weights, aggregate the source-outcome
/// ledger, and return both the report and the per-source weight table arm R
/// uses (`weight = smoothed_precision`, the metric the entrenchment ordering
/// ranks on — defined for every source, neutral 0.5 for the never-accepted).
fn learn_weights(
    corpus: &[(String, ResearchTrace)],
) -> (
    source_reliability::SourceReliabilityReport,
    BTreeMap<String, f64>,
) {
    let mut rows = Vec::new();
    for (name, trace) in corpus {
        let mut loop_ = CognitiveLoop::new(trace.dimension);
        let report = loop_.process_research_trace(trace.clone());
        rows.extend(source_reliability::outcomes_from_report(
            name,
            RECORDED_AT,
            &report.outcomes,
            &report.final_beliefs,
        ));
    }
    let rep = source_reliability::report(&rows, 0, GENERATED_AT);
    let weights = rep
        .by_source
        .iter()
        .map(|(s, e)| (s.clone(), e.smoothed_precision))
        .collect();
    (rep, weights)
}

/// Count propositions whose final belief differs between the two arms.
fn belief_divergences(u: &ArmResult, r: &ArmResult) -> Vec<(String, HexValue, HexValue)> {
    let mut keys: std::collections::BTreeSet<&String> = u.final_beliefs.keys().collect();
    keys.extend(r.final_beliefs.keys());
    keys.into_iter()
        .filter_map(|k| {
            let uv = u.final_beliefs.get(k).copied();
            let rv = r.final_beliefs.get(k).copied();
            match (uv, rv) {
                (Some(a), Some(b)) if a != b => Some((k.clone(), a, b)),
                (Some(a), None) => Some((k.clone(), a, HexValue::Unknown)),
                (None, Some(b)) => Some((k.clone(), HexValue::Unknown, b)),
                _ => None,
            }
        })
        .collect()
}

fn load_trace(path: &str) -> ResearchTrace {
    let raw = fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse {path}: {e}"))
}

/// Replay every trace in a corpus twice and print a per-trace + aggregate A/B
/// table. Returns the aggregate (Σ arm-U, Σ arm-R, Σ divergences).
fn run_corpus(label: &str, corpus: &[(String, ResearchTrace)]) -> (ArmResult, ArmResult, usize) {
    let (rep, weights) = learn_weights(corpus);

    println!("\n================ {label} ================");
    println!("Phase A — learned per-source reliability (weight = smoothed_precision):");
    println!(
        "  pooled: accepted={} false_acc={} smoothed={:.3}",
        rep.pooled.accepted, rep.pooled.false_acceptances, rep.pooled.smoothed_precision
    );
    for rung in &rep.entrenchment {
        println!(
            "  {:<40} weight={:.3} accepted={:>2} beats_pooled={}",
            rung.source, rung.smoothed_precision, rung.accepted, rung.beats_pooled_baseline
        );
    }

    println!("\nPhase B — counterfactual replay (arm U = uniform, arm R = reliability):");
    println!(
        "  {:<34} | {:>18} | {:>18} | {:>6}",
        "trace", "U falseAcc/falseEsc", "R falseAcc/falseEsc", "belief\u{0394}"
    );

    let mut sum_u = ArmResult {
        false_acceptance_count: 0,
        false_escalations: 0,
        escalations: 0,
        accepted: 0,
        final_beliefs: BTreeMap::new(),
    };
    let mut sum_r = sum_u.clone();
    let mut total_div = 0usize;

    for (name, trace) in corpus {
        let u = replay_arm(trace, None);
        let r = replay_arm(trace, Some(&weights));
        let divs = belief_divergences(&u, &r);
        total_div += divs.len();

        println!(
            "  {:<34} | {:>7}/{:<10} | {:>7}/{:<10} | {:>6}",
            name,
            u.false_acceptance_count,
            u.false_escalations,
            r.false_acceptance_count,
            r.false_escalations,
            divs.len()
        );
        for (p, uv, rv) in &divs {
            println!("        \u{0394} {p}: U={uv:?} vs R={rv:?}");
        }

        sum_u.false_acceptance_count += u.false_acceptance_count;
        sum_u.false_escalations += u.false_escalations;
        sum_u.escalations += u.escalations;
        sum_u.accepted += u.accepted;
        sum_r.false_acceptance_count += r.false_acceptance_count;
        sum_r.false_escalations += r.false_escalations;
        sum_r.escalations += r.escalations;
        sum_r.accepted += r.accepted;
    }

    println!("  {:-<34}-+-{:-<18}-+-{:-<18}-+-{:-<6}", "", "", "", "");
    println!(
        "  {:<34} | {:>7}/{:<10} | {:>7}/{:<10} | {:>6}",
        "TOTAL",
        sum_u.false_acceptance_count,
        sum_u.false_escalations,
        sum_r.false_acceptance_count,
        sum_r.false_escalations,
        total_div
    );
    println!(
        "  totals: U accepted={} escalations={} | R accepted={} escalations={}",
        sum_u.accepted, sum_u.escalations, sum_r.accepted, sum_r.escalations
    );

    (sum_u, sum_r, total_div)
}

// ---------------------------------------------------------------------------
// Corpus 1 — the real replayable traces.
// ---------------------------------------------------------------------------

fn real_corpus() -> Vec<(String, ResearchTrace)> {
    let mut c = Vec::new();
    for f in [
        "cognition_divergence",
        "conflicting_benchmark",
        "derivation",
        "heavy_contradiction",
        "long_recovery",
        "racing_goals",
        "slow_evidence",
        "swarm_dissent",
    ] {
        c.push((
            format!("ix/{f}"),
            load_trace(&format!("../../fixtures/ix/{f}.json")),
        ));
    }
    for f in [
        "correction_replaces_claim",
        "partial_retraction_downgrades",
        "relation_withdrawal_reverts_derived_belief",
        "retraction_dissolves_derived_contradiction",
        "supersession_chain",
    ] {
        c.push((
            format!("revision/{f}"),
            load_trace(&format!("../../fixtures/revision/{f}.json")),
        ));
    }
    c.push((
        "demerzel/beliefs".into(),
        load_trace("../../fixtures/demerzel/beliefs_2026-07-20.json"),
    ));
    c
}

#[test]
fn real_corpus_arms_do_not_separate_null_result() {
    let corpus = real_corpus();
    let (u, r, div) = run_corpus("REAL CORPUS", &corpus);

    // The honest, pinned finding: on this small real corpus the arms are
    // indistinguishable on the primary metric. Weight only bites at a
    // recompute-escalation tip, and none of these traces places a surviving
    // cross-source True/False conflict on the knife-edge of the 0.3 escalation
    // threshold, so reliability-weighting changes nothing a consumer would act
    // on. If a future fixture changes that, this assertion fires and the
    // finding must be revisited.
    assert_eq!(
        u.false_acceptance_count, r.false_acceptance_count,
        "real corpus: false_acceptance_count must not separate the arms"
    );
    assert_eq!(
        div, 0,
        "real corpus: no final-belief divergence expected between arms"
    );
}

// ---------------------------------------------------------------------------
// Corpus 2 — the synthetic family (deterministic, documented).
//
// Two sources of *known* quality:
//   clean  — asserts claims that hold (high earned precision).
//   noisy  — asserts claims later retracted (low earned precision).
// Phase A over the corpus learns clean ≈ 0.83, noisy ≈ 0.17 (smoothed prior
// 2.0/0.5 over 4 accepts each). The two "test" traces then place a clean-vs-
// noisy True/False conflict behind a selective retraction so the belief is
// *recomputed* from the weighted survivors — the one site where weight bites.
// ---------------------------------------------------------------------------

/// clean earns high precision: four held accepts, nothing retracts them.
fn syn_train_clean() -> ResearchTrace {
    let events: String = (1..=4)
        .map(|i| {
            format!(
                r#"{{"cycle":{i},"source":"clean","payload":{{"type":"belief_update","proposition":"clean-claim-{i}","value":"True","evidence":{{}}}}}}"#
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    serde_json::from_str(&format!(r#"{{"dimension":4,"events":[{events}]}}"#)).unwrap()
}

/// noisy earns low precision: four accepts, each later fully retracted.
fn syn_train_noisy() -> ResearchTrace {
    let mut events = Vec::new();
    for i in 1..=4 {
        let assert_c = 2 * i - 1;
        let retract_c = 2 * i;
        events.push(format!(
            r#"{{"cycle":{assert_c},"source":"noisy","payload":{{"type":"belief_update","proposition":"noisy-claim-{i}","value":"True","evidence":{{}}}}}}"#
        ));
        events.push(format!(
            r#"{{"cycle":{retract_c},"source":"cleanup","payload":{{"type":"retraction","proposition":"noisy-claim-{i}","reason":"result could not be reproduced"}}}}"#
        ));
    }
    let events = events.join(",");
    serde_json::from_str(&format!(r#"{{"dimension":4,"events":[{events}]}}"#)).unwrap()
}

/// S1 — reliability HELPS. clean(True) vs noisy(False) on `verdict-1`, plus a
/// throwaway `scratch` observation that a selective retraction removes to force
/// a recompute over the surviving conflict. Under uniform weights the True/
/// False split escalates to Contradictory (C-share 1/3 > 0.3); under
/// reliability weights the unreliable dissenter is downweighted, the share
/// falls below 0.3, and the belief recomputes to Probable — the correct
/// end-state, since `clean` is the reliable source here.
fn syn_test_helps() -> ResearchTrace {
    serde_json::from_str(
        r#"{"dimension":4,"events":[
        {"cycle":1,"source":"clean","payload":{"type":"belief_update","proposition":"verdict-1","value":"True","evidence":{}}},
        {"cycle":2,"source":"noisy","payload":{"type":"belief_update","proposition":"verdict-1","value":"False","evidence":{}}},
        {"cycle":3,"source":"scratch","payload":{"type":"belief_update","proposition":"verdict-1","value":"Unknown","evidence":{}}},
        {"cycle":4,"source":"reviewer","payload":{"type":"retraction","proposition":"verdict-1","reason":"drop the scratch probe, recompute from the standing evidence","retracts":{"source":"scratch","cycle":3}}}
    ]}"#,
    )
    .unwrap()
}

/// S2 — reliability HURTS (minority-report risk). Same conflict on `verdict-2`,
/// but here the globally-reliable `clean` is WRONG on this instance and the
/// downweighted `noisy` dissenter is RIGHT: a later correction confirms False.
/// Arm U keeps the Contradictory escalation (a warranted alarm) at the
/// recompute; arm R suppresses it to a Probable Accept, which the correction
/// then flips — a false acceptance arm R introduced by trusting earned
/// reliability over a correct minority. Exactly issue #14's non-goal "do not
/// punish minority evidence merely because it disagrees."
fn syn_test_hurts() -> ResearchTrace {
    serde_json::from_str(
        r#"{"dimension":4,"events":[
        {"cycle":1,"source":"clean","payload":{"type":"belief_update","proposition":"verdict-2","value":"True","evidence":{}}},
        {"cycle":2,"source":"noisy","payload":{"type":"belief_update","proposition":"verdict-2","value":"False","evidence":{}}},
        {"cycle":3,"source":"scratch","payload":{"type":"belief_update","proposition":"verdict-2","value":"Unknown","evidence":{}}},
        {"cycle":4,"source":"reviewer","payload":{"type":"retraction","proposition":"verdict-2","reason":"drop the scratch probe, recompute from the standing evidence","retracts":{"source":"scratch","cycle":3}}},
        {"cycle":5,"source":"audit","payload":{"type":"correction","proposition":"verdict-2","reason":"ground truth confirms the dissenter: the claim is False","retracts":{"source":"clean","cycle":1},"value":"False","evidence":{}}}
    ]}"#,
    )
    .unwrap()
}

fn synthetic_corpus() -> Vec<(String, ResearchTrace)> {
    vec![
        ("syn/train_clean".into(), syn_train_clean()),
        ("syn/train_noisy".into(), syn_train_noisy()),
        ("syn/test_helps (S1)".into(), syn_test_helps()),
        ("syn/test_hurts (S2)".into(), syn_test_hurts()),
    ]
}

#[test]
fn synthetic_family_shows_both_signs_of_the_effect() {
    let corpus = synthetic_corpus();
    let (_u, _r, _div) = run_corpus("SYNTHETIC CORPUS", &corpus);

    // Learn the reliability gap and confirm the construction produced it.
    let (rep, weights) = learn_weights(&corpus);
    let clean_w = weights["clean"];
    let noisy_w = weights["noisy"];
    // A wide, decisive reliability gap. `clean` lands ~0.63 rather than its
    // train-only 0.83 because cross-session summation (the design's own
    // aggregation) also counts clean being wrong on the two test instances —
    // honest, and still far above noisy. What tips the recompute is `noisy`
    // being the low end of the conflict (< 0.2), which drives the synthesized
    // Contradictory mass below the 0.3 escalation share.
    assert!(
        clean_w > 0.5 && noisy_w < 0.2 && clean_w - noisy_w > 0.4,
        "construction must yield a wide reliability gap: clean={clean_w:.3} noisy={noisy_w:.3}"
    );
    let _ = &rep; // report is printed by run_corpus above.

    // S1: arm R suppresses the spurious contradiction. Under U the recompute
    // escalates verdict-1 to Contradictory; under R it recomputes to Probable.
    let s1 = syn_test_helps();
    let u1 = replay_arm(&s1, None);
    let r1 = replay_arm(&s1, Some(&weights));
    assert_eq!(
        u1.final_beliefs.get("verdict-1"),
        Some(&HexValue::Contradictory),
        "arm U escalates the uniform-weighted conflict"
    );
    assert_eq!(
        r1.final_beliefs.get("verdict-1"),
        Some(&HexValue::Probable),
        "arm R suppresses the unreliable dissenter, recomputing to Probable"
    );
    // R raises no unwarranted escalation here; U raised the (here spurious) one.
    assert!(
        u1.escalations >= 1,
        "arm U raised the Contradictory escalation"
    );

    // S2: same mechanism, opposite value — arm R introduces a false acceptance
    // the warranted arm-U escalation would have caught.
    let s2 = syn_test_hurts();
    let u2 = replay_arm(&s2, None);
    let r2 = replay_arm(&s2, Some(&weights));
    assert!(
        r2.false_acceptance_count > u2.false_acceptance_count,
        "arm R introduces a false acceptance vs arm U on the minority-right case \
         (U={} R={})",
        u2.false_acceptance_count,
        r2.false_acceptance_count
    );
}
