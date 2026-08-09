//! The #35 §6 aggregator — trace-clustered paired bootstrap — and the §8
//! kill/keep rule applied mechanically to its output.
//!
//! `docs/research/2026-07-28-ix-eval-preregistration.md` §6 specified this
//! aggregator and then recorded, in a 2026-07-30 correction, that **no
//! bootstrap aggregator existed anywhere in the workspace**. It was the last
//! missing §6 prerequisite: §8 says the kill/keep rule is applied
//! *"mechanically to the bootstrap output"*, and there was no bootstrap output.
//!
//! These tests grade the aggregator on hand-built inputs with a known-sign
//! effect — §6 requires it to be a pure function precisely so this is possible
//! with no replay engine in the loop — and then run it end-to-end over the
//! committed §9.3 corpus through the existing replay boundary.
//!
//! The properties that matter, in order:
//!
//! 1. **Clustering is real.** The same decisions, presented as four correlated
//!    traces rather than forty independent ones, must *not* clear the dual
//!    rule. That is the whole reason §6 clusters.
//! 2. **Determinism.** A re-run reproduces the interval exactly (§6).
//! 3. **Degeneracy is reported, never hidden.** A corpus with zero
//!    between-cluster variance yields an interval that is a design artifact,
//!    and the output must say so (§9.4).
//! 4. **The verdict is mechanical**, including §9.4's standing rule that no §8
//!    verdict may be computed on an authored fixture.

use hari_core::paired_eval::{
    apply_kill_keep, bootstrap_paired_difference, cluster_from_arms, pooling_violation,
    score_paired_all_arms, BootstrapConfig, CalibrationMargin, ClauseStatus, CorpusProvenance,
    KillKeepVerdict, PairedDelta, TraceCluster,
};
use hari_core::{PairedFixture, SubjectiveLogicConfig};

// ---------------------------------------------------------------------------
// Hand-built inputs — §6's "unit-testable on hand-built inputs with a
// known-sign effect, with no replay engine in the loop".
// ---------------------------------------------------------------------------

/// `deltas` reads as: `true` — treatment right and baseline wrong (+1);
/// `false` — both right (0). Nothing else is needed to exercise the estimator,
/// since only the difference enters it.
fn cluster(trace: &str, deltas: &[bool]) -> TraceCluster {
    TraceCluster {
        trace: trace.to_string(),
        pairs: deltas
            .iter()
            .enumerate()
            .map(|(i, up)| PairedDelta {
                pair: format!("{trace}-p{i}"),
                treatment_correct: true,
                baseline_correct: !*up,
            })
            .collect(),
        unmatched: Vec::new(),
    }
}

fn run(clusters: &[TraceCluster]) -> Option<hari_core::paired_eval::PairedBootstrap> {
    bootstrap_paired_difference(
        "experimental",
        "baseline",
        clusters,
        BootstrapConfig::default(),
    )
}

#[test]
fn a_known_sign_effect_across_many_traces_clears_the_dual_rule() {
    // Ten traces where the treatment wins three of four pairs, two where it
    // wins none. Between-cluster variance is real and the effect is large.
    let mut clusters: Vec<TraceCluster> = (0..10)
        .map(|i| cluster(&format!("t{i:02}"), &[true, true, true, false]))
        .collect();
    clusters.push(cluster("t10", &[false, false, false, false]));
    clusters.push(cluster("t11", &[false, false, false, false]));

    let b = run(&clusters).expect("48 gradeable pairs must yield an interval");

    assert_eq!(b.clusters, 12);
    assert_eq!(b.pairs, 48);
    assert!(
        (b.difference - 0.625).abs() < 1e-12,
        "point estimate {}",
        b.difference
    );
    assert!(
        b.ci_low > 0.0,
        "CI must exclude zero: [{}, {}]",
        b.ci_low,
        b.ci_high
    );
    assert!(b.ci_excludes_zero);
    assert!(b.p_value < 0.05, "p = {}", b.p_value);
    assert!(b.p_below_alpha);
    assert!(b.dual_rule_passes, "both halves of the §6 dual rule hold");
}

/// **The property clustering exists for.** Forty decisions that are perfectly
/// correlated inside four traces carry far less information than forty
/// independent ones. Treating them as independent manufactures significance;
/// §6 clusters by trace to prevent exactly that.
#[test]
fn clustering_by_trace_refuses_the_significance_that_independence_would_manufacture() {
    let clustered = vec![
        cluster("t0", &[true; 10]),
        cluster("t1", &[true; 10]),
        cluster("t2", &[false; 10]),
        cluster("t3", &[false; 10]),
    ];
    // The same forty deltas, each in its own cluster — the mistake §6 forbids.
    let independent: Vec<TraceCluster> = clustered
        .iter()
        .flat_map(|c| c.pairs.iter())
        .enumerate()
        .map(|(i, p)| TraceCluster {
            trace: format!("d{i:02}"),
            pairs: vec![p.clone()],
            unmatched: Vec::new(),
        })
        .collect();

    let c = run(&clustered).expect("clustered interval");
    let i = run(&independent).expect("independent interval");

    // Same data, same point estimate — only the resampling unit differs.
    assert!((c.difference - 0.5).abs() < 1e-12);
    assert!((i.difference - 0.5).abs() < 1e-12);

    assert!(
        (c.ci_high - c.ci_low) > (i.ci_high - i.ci_low),
        "clustered CI [{}, {}] must be wider than independent [{}, {}]",
        c.ci_low,
        c.ci_high,
        i.ci_low,
        i.ci_high
    );
    assert!(
        !c.dual_rule_passes,
        "four correlated traces must not clear the dual rule (CI [{}, {}], p {})",
        c.ci_low, c.ci_high, c.p_value
    );
    assert!(
        i.dual_rule_passes,
        "the independence mistake would have cleared it — which is the point"
    );
}

#[test]
fn the_interval_is_reproducible_under_the_fixed_seed() {
    let clusters: Vec<TraceCluster> = (0..8)
        .map(|i| cluster(&format!("t{i}"), &[i % 3 == 0, i % 2 == 0, false, true]))
        .collect();

    let a = run(&clusters).expect("interval");
    let b = run(&clusters).expect("interval");

    assert_eq!(
        a.ci_low.to_bits(),
        b.ci_low.to_bits(),
        "CI low is not reproducible"
    );
    assert_eq!(
        a.ci_high.to_bits(),
        b.ci_high.to_bits(),
        "CI high is not reproducible"
    );
    assert_eq!(
        a.p_value.to_bits(),
        b.p_value.to_bits(),
        "p-value is not reproducible"
    );
    assert_eq!(a.resamples, 10_000, "§6 pre-registers B = 10,000");
    assert_eq!(a.seed, BootstrapConfig::default().seed);
    assert_eq!(a.seed, b.seed);
}

/// Identical arms have a difference of exactly zero at every decision, not a
/// difference that is merely statistically indistinguishable from zero. The
/// interval must collapse to a point and the dual rule must fail.
#[test]
fn identical_arms_yield_a_point_interval_at_zero_and_fail_the_dual_rule() {
    let clusters: Vec<TraceCluster> = (0..6)
        .map(|i| TraceCluster {
            trace: format!("t{i}"),
            pairs: (0..9)
                .map(|j| PairedDelta {
                    pair: format!("t{i}-p{j}"),
                    treatment_correct: j % 3 == 0,
                    baseline_correct: j % 3 == 0,
                })
                .collect(),
            unmatched: Vec::new(),
        })
        .collect();

    let b = run(&clusters).expect("interval");

    assert_eq!(b.difference, 0.0);
    assert_eq!(b.ci_low, 0.0);
    assert_eq!(b.ci_high, 0.0);
    assert_eq!(b.p_value, 1.0);
    assert!(!b.ci_excludes_zero);
    assert!(!b.dual_rule_passes);
    assert!(
        b.every_delta_is_zero,
        "an identically-zero difference must be reported as such, not as a null result"
    );
}

/// §9.4: a corpus of clones gives the bootstrap no population, so the interval
/// it reports is a property of the author rather than of the world. The output
/// must surface that rather than let a tight CI read as evidence.
#[test]
fn zero_between_cluster_variance_is_reported_rather_than_hidden() {
    let clusters: Vec<TraceCluster> = (0..6)
        .map(|i| cluster(&format!("clone{i}"), &[true, true, false]))
        .collect();

    let b = run(&clusters).expect("interval");

    assert_eq!(b.distinct_cluster_signatures, 1);
    assert!(b.zero_between_cluster_variance);
    assert!(!b.every_delta_is_zero, "the effect itself is non-zero here");
    // The interval is a point because every resample is the same corpus.
    assert!((b.ci_low - b.ci_high).abs() < 1e-12);
}

#[test]
fn icc_and_effective_n_are_reported_so_section_7_is_answerable() {
    // Perfect within-cluster correlation: ICC = 1, so the effective sample size
    // is the number of traces, not the number of decisions.
    let clusters = vec![
        cluster("t0", &[true; 10]),
        cluster("t1", &[true; 10]),
        cluster("t2", &[false; 10]),
        cluster("t3", &[false; 10]),
    ];
    let b = run(&clusters).expect("interval");

    assert!((b.icc.expect("ICC") - 1.0).abs() < 1e-9, "icc {:?}", b.icc);
    assert!((b.design_effect.expect("deff") - 10.0).abs() < 1e-9);
    assert!(
        (b.effective_n.expect("effective n") - 4.0).abs() < 1e-9,
        "40 decisions in 4 perfectly-correlated traces are worth 4, not 40"
    );
    assert!(
        !b.effective_n_overstates,
        "these traces genuinely differ, so the correction is the binding one"
    );
}

/// **The correction ICC cannot make.** A corpus of clones has no *between*-trace
/// variance, so the ICC design effect comes out at 1 and the effective n at the
/// raw pair count — while the corpus holds one decision situation repeated. The
/// arithmetic is right and the number is misleading, so it must be flagged at
/// the boundary rather than explained in a report a JSON consumer never reads.
#[test]
fn a_corpus_of_clones_flags_that_its_effective_n_overstates() {
    let clusters: Vec<TraceCluster> = (0..6)
        .map(|i| cluster(&format!("clone{i}"), &[true, true, false]))
        .collect();
    let b = run(&clusters).expect("interval");

    assert_eq!(b.icc, Some(0.0), "no between-cluster variance to attribute");
    assert_eq!(b.design_effect, Some(1.0));
    assert_eq!(b.effective_n, Some(18.0), "the raw pair count, uncorrected");
    assert!(
        b.effective_n_overstates,
        "18 clones of one decision situation are not 18 decisions"
    );
}

#[test]
fn an_empty_corpus_has_no_interval_rather_than_a_zero_one() {
    assert!(run(&[]).is_none());
    assert!(run(&[cluster("t0", &[])]).is_none());
}

#[test]
fn a_pair_graded_in_only_one_arm_is_named_rather_than_dropped() {
    let clusters = vec![
        cluster("t0", &[true, true]),
        TraceCluster {
            trace: "t1".to_string(),
            pairs: cluster("t1", &[true, false]).pairs,
            unmatched: vec!["t1-orphan".to_string()],
        },
    ];
    let b = run(&clusters).expect("interval");
    assert_eq!(b.unmatched, vec!["t1/t1-orphan".to_string()]);
}

/// §9.3.2 states that the isolation and task corpora are **never pooled**.
/// Pooling would average SubjectiveLogic's degenerate always-`Wait` zero on
/// isolation into its task result, which is exactly the laundering §9.3.3 warns
/// about — so the rule is checked, not remembered.
#[test]
fn the_two_corpora_may_not_be_pooled() {
    assert_eq!(
        pooling_violation(&["accuracy-isolation", "cache-task"]),
        Some(("accuracy-isolation".to_string(), "cache-task".to_string()))
    );
    assert_eq!(pooling_violation(&["accuracy-task", "cache-task"]), None);
    assert_eq!(
        pooling_violation(&["accuracy-isolation", "cache-isolation"]),
        None
    );
    assert_eq!(pooling_violation(&[]), None);
}

// ---------------------------------------------------------------------------
// §8, applied mechanically
// ---------------------------------------------------------------------------

fn passing_bootstrap() -> hari_core::paired_eval::PairedBootstrap {
    let clusters: Vec<TraceCluster> = (0..12)
        .map(|i| cluster(&format!("t{i:02}"), &[true, true, true, i % 6 == 0]))
        .collect();
    let b = run(&clusters).expect("interval");
    assert!(b.dual_rule_passes, "fixture precondition");
    b
}

fn failing_bootstrap() -> hari_core::paired_eval::PairedBootstrap {
    let clusters: Vec<TraceCluster> = (0..6)
        .map(|i| TraceCluster {
            trace: format!("t{i}"),
            pairs: (0..9)
                .map(|j| PairedDelta {
                    pair: format!("t{i}-p{j}"),
                    treatment_correct: true,
                    baseline_correct: true,
                })
                .collect(),
            unmatched: Vec::new(),
        })
        .collect();
    run(&clusters).expect("interval")
}

#[test]
fn keep_requires_both_clauses_and_a_recorded_corpus() {
    let decision = apply_kill_keep(
        Some(&passing_bootstrap()),
        Some(CalibrationMargin {
            experimental_mean_brier: 0.18,
            subjective_logic_mean_brier: 0.22,
        }),
        CorpusProvenance::DriverRecorded,
    );
    assert_eq!(decision.clause1, ClauseStatus::Passes);
    assert_eq!(decision.clause2, ClauseStatus::Passes);
    assert_eq!(decision.verdict, KillKeepVerdict::Keep);
}

#[test]
fn a_failed_clause_one_kills_regardless_of_calibration() {
    let decision = apply_kill_keep(
        Some(&failing_bootstrap()),
        Some(CalibrationMargin {
            experimental_mean_brier: 0.05,
            subjective_logic_mean_brier: 0.40,
        }),
        CorpusProvenance::DriverRecorded,
    );
    assert_eq!(decision.clause1, ClauseStatus::Fails);
    assert_eq!(decision.clause2, ClauseStatus::Passes);
    assert_eq!(decision.verdict, KillKeepVerdict::Kill);
}

/// Per-arm calibration does not exist (#35 §9 item 2): each arm would have to
/// emit forecasts from its own posterior. An undefined clause cannot be a
/// passing one, so KEEP is unreachable while it stays undefined.
#[test]
fn an_undefined_calibration_clause_cannot_satisfy_keep() {
    let decision = apply_kill_keep(
        Some(&passing_bootstrap()),
        None,
        CorpusProvenance::DriverRecorded,
    );
    assert_eq!(decision.clause1, ClauseStatus::Passes);
    assert_eq!(decision.clause2, ClauseStatus::Undefined);
    assert_eq!(decision.verdict, KillKeepVerdict::Kill);
}

/// §9.4's standing rule, in code rather than in prose: no §8 verdict may ever
/// be computed on an authored fixture, however the clauses come out.
#[test]
fn an_authored_corpus_withholds_the_verdict_while_still_reporting_the_clauses() {
    let decision = apply_kill_keep(
        Some(&passing_bootstrap()),
        Some(CalibrationMargin {
            experimental_mean_brier: 0.10,
            subjective_logic_mean_brier: 0.30,
        }),
        CorpusProvenance::Authored,
    );
    assert_eq!(decision.verdict, KillKeepVerdict::WithheldByStandingRule);
    assert_eq!(decision.clause1, ClauseStatus::Passes);
    assert_eq!(decision.clause2, ClauseStatus::Passes);
    assert!(
        decision.rationale.iter().any(|r| r.contains("§9.4")),
        "the rule that withheld the verdict must be named: {:?}",
        decision.rationale
    );
}

#[test]
fn a_missing_bootstrap_leaves_clause_one_undefined_and_kills() {
    let decision = apply_kill_keep(None, None, CorpusProvenance::DriverRecorded);
    assert_eq!(decision.clause1, ClauseStatus::Undefined);
    assert_eq!(decision.verdict, KillKeepVerdict::Kill);
}

// ---------------------------------------------------------------------------
// End to end over the committed §9.3 corpus, through the replay boundary
// ---------------------------------------------------------------------------

const TASK_CORPUS: [&str; 6] = [
    "accuracy",
    "cache",
    "cost",
    "latency",
    "memory",
    "throughput",
];

fn task_clusters(
    treatment: fn(&hari_core::PairedComparison) -> &hari_core::PairedArm,
    baseline: fn(&hari_core::PairedComparison) -> &hari_core::PairedArm,
) -> Vec<TraceCluster> {
    TASK_CORPUS
        .iter()
        .map(|theme| {
            let path = format!("../../fixtures/ix/paired/{theme}-task.json");
            let fixture: PairedFixture =
                serde_json::from_str(&std::fs::read_to_string(&path).expect("fixture must exist"))
                    .expect("fixture must parse");
            let graded = score_paired_all_arms(fixture, SubjectiveLogicConfig::default());
            cluster_from_arms(theme, treatment(&graded), baseline(&graded))
        })
        .collect()
}

/// §9.5 measured this arm-for-arm; here it is through the §6 aggregator. The
/// shipped substrate and naive acceptance differ at **no** decision, so §8
/// clause 1 is not merely insignificant — it is identically zero.
#[test]
fn the_shipped_arm_does_not_separate_from_the_null_baseline_on_the_task_corpus() {
    let clusters = task_clusters(|g| &g.recency_decay, |g| &g.unassisted);
    let b = bootstrap_paired_difference(
        "recency_decay",
        "ix_unassisted",
        &clusters,
        BootstrapConfig::default(),
    )
    .expect("54 pairs must be gradeable");

    assert_eq!(b.clusters, 6);
    assert_eq!(b.pairs, 54);
    assert!(b.unmatched.is_empty());
    assert_eq!(b.difference, 0.0);
    assert!(b.every_delta_is_zero);
    assert!(!b.dual_rule_passes, "§8 clause 1 does not hold");

    let decision = apply_kill_keep(Some(&b), None, CorpusProvenance::Authored);
    assert_eq!(decision.clause1, ClauseStatus::Fails);
    assert_eq!(decision.clause2, ClauseStatus::Undefined);
    assert_eq!(decision.verdict, KillKeepVerdict::WithheldByStandingRule);
}

/// The cheap baseline *does* separate — and the corpus is six clones, so the
/// interval that separation produces is a design artifact. Both facts have to
/// come out of the same call, or a reader sees only the first.
#[test]
fn subjective_logic_separates_from_the_shipped_arm_on_a_corpus_with_no_population() {
    let clusters = task_clusters(|g| &g.subjective_logic, |g| &g.recency_decay);
    let b = bootstrap_paired_difference(
        "subjective_logic",
        "recency_decay",
        &clusters,
        BootstrapConfig::default(),
    )
    .expect("54 pairs must be gradeable");

    assert!(
        (b.difference - 1.0 / 3.0).abs() < 1e-12,
        "difference {}",
        b.difference
    );
    assert!(!b.every_delta_is_zero);
    assert_eq!(
        b.distinct_cluster_signatures, 1,
        "the six task fixtures are clones — §9.4's finding, now mechanical"
    );
    assert!(
        b.zero_between_cluster_variance,
        "the CI is a design artifact and the output must say so"
    );
    // The interval clears the §6 dual rule — and it must not be read as a
    // result. Both facts come out of the same call, which is the point.
    assert!(b.dual_rule_passes);
    assert!(b.effective_n_overstates);
    assert_eq!(
        b.effective_n,
        Some(54.0),
        "uncorrected: ICC cannot see clones"
    );

    let decision = apply_kill_keep(Some(&b), None, CorpusProvenance::Authored);
    assert_eq!(
        decision.verdict,
        KillKeepVerdict::WithheldByStandingRule,
        "a passing dual rule on an authored corpus still yields no verdict"
    );
}
