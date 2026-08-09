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
    apply_kill_keep, bootstrap_paired_difference, check_corpus, cluster_from_arms,
    fixture_provenance, pooling_violation, reconcile_provenance, score_paired_all_arms,
    trace_digest, ArmBindingError, BootstrapConfig, CalibrationMargin, ClauseStatus, CorpusDefect,
    CorpusProvenance, FixtureProvenance, KillKeepVerdict, PairOutcome, PairedArm, PairedDelta,
    PairedScore, ProvenanceDefect, TraceCluster,
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
        treatment: "experimental".to_string(),
        baseline: "baseline".to_string(),
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
    bootstrap_paired_difference(clusters, BootstrapConfig::default())
        .expect("hand-built clusters name one arm pair")
}

/// `run` for clusters built by `cluster_from_arms`, where the arm names are the
/// point of the test rather than boilerplate.
fn run_named(clusters: &[TraceCluster]) -> Option<hari_core::paired_eval::PairedBootstrap> {
    bootstrap_paired_difference(clusters, BootstrapConfig::default())
        .expect("the corpus names one arm pair")
}

/// A [`PairedArm`] carrying only the per-pair verdicts — enough for
/// `cluster_from_arms`, which reads nothing else.
fn arm_with_pairs(arm: &str, pairs: &[&str]) -> PairedArm {
    let per_pair: Vec<PairOutcome> = pairs
        .iter()
        .map(|p| PairOutcome {
            pair: (*p).to_string(),
            act_correct: true,
            abstain_correct: true,
        })
        .collect();
    PairedArm {
        arm: arm.to_string(),
        priority_model: None,
        conditioned_abstention: Default::default(),
        paired: PairedScore {
            pairs: per_pair.len(),
            both_correct: per_pair.len(),
            paired_accuracy: Some(1.0),
            act_correct: per_pair.len(),
            act_total: per_pair.len(),
            abstain_correct: per_pair.len(),
            abstain_total: per_pair.len(),
            per_pair,
            defects: Vec::new(),
        },
        false_rejections: Default::default(),
        false_acceptances: Default::default(),
    }
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
            treatment: "experimental".to_string(),
            baseline: "baseline".to_string(),
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
            treatment: "experimental".to_string(),
            baseline: "baseline".to_string(),
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
            treatment: "experimental".to_string(),
            baseline: "baseline".to_string(),
            pairs: cluster("t1", &[true, false]).pairs,
            unmatched: vec!["t1-orphan".to_string()],
        },
    ];
    let b = run(&clusters).expect("interval");
    assert_eq!(b.unmatched, vec!["t1/t1-orphan".to_string()]);
}

/// **The mutation this pins.** `bootstrap_paired_difference` used to re-accept
/// arm identity as two free `&str` at the call site. Transposing the two string
/// literals in `main.rs` published `treatment: recency_decay,
/// treatment_accuracy: 0.6667` — the shipped substrate credited with
/// SubjectiveLogic's entire advantage — under a sign-correct interval, a passing
/// dual rule and a fully green suite.
///
/// There is no literal left to transpose. What remains transposable is the pair
/// of arms handed to `cluster_from_arms`, so that is what is exercised here: the
/// labels and the sign must both follow the data, and neither may be settable
/// independently of it.
#[test]
fn the_published_arm_labels_follow_the_arms_the_clusters_were_built_from() {
    let forward = task_clusters(|g| &g.subjective_logic, |g| &g.recency_decay);
    let transposed = task_clusters(|g| &g.recency_decay, |g| &g.subjective_logic);

    let f = run_named(&forward).expect("interval");
    let t = run_named(&transposed).expect("interval");

    assert_eq!(
        (f.treatment.as_str(), f.baseline.as_str()),
        ("subjective_logic", "recency_decay")
    );
    assert_eq!(
        (t.treatment.as_str(), t.baseline.as_str()),
        ("recency_decay", "subjective_logic"),
        "labels did not follow the transposition"
    );
    // The sign follows too, so a transposed run cannot publish the shipped arm's
    // name over the cheap baseline's advantage.
    assert!((f.difference - 1.0 / 3.0).abs() < 1e-12, "{}", f.difference);
    assert!((t.difference + 1.0 / 3.0).abs() < 1e-12, "{}", t.difference);
    assert!((f.treatment_accuracy - 2.0 / 3.0).abs() < 1e-12);
    assert!((t.treatment_accuracy - 1.0 / 3.0).abs() < 1e-12);
}

/// Clusters built from different arm pairs cannot be aggregated into one
/// interval: whichever label were chosen, half the numbers came from elsewhere.
#[test]
fn a_corpus_whose_clusters_disagree_about_their_arms_is_refused() {
    let mut clusters = vec![cluster("t0", &[true, false]), cluster("t1", &[true, true])];
    clusters[1].baseline = "ix_unassisted".to_string();

    let err = bootstrap_paired_difference(&clusters, BootstrapConfig::default())
        .expect_err("disagreeing clusters must be refused, not silently labeled");
    assert_eq!(
        err,
        ArmBindingError::Mismatch {
            trace: "t1".to_string(),
            treatment: "experimental".to_string(),
            baseline: "ix_unassisted".to_string(),
            expected_treatment: "experimental".to_string(),
            expected_baseline: "baseline".to_string(),
        }
    );
}

#[test]
fn an_unattributable_cluster_is_refused_rather_than_published() {
    let mut unlabeled = vec![cluster("t0", &[true, false])];
    unlabeled[0].treatment = String::new();
    assert_eq!(
        bootstrap_paired_difference(&unlabeled, BootstrapConfig::default()).expect_err("refused"),
        ArmBindingError::Unlabeled {
            trace: "t0".to_string()
        }
    );

    // An arm compared against itself has a difference of zero by construction,
    // which would read as two arms agreeing.
    let mut self_paired = vec![cluster("t0", &[true, false])];
    self_paired[0].baseline = "experimental".to_string();
    assert_eq!(
        bootstrap_paired_difference(&self_paired, BootstrapConfig::default()).expect_err("refused"),
        ArmBindingError::SameArm {
            trace: "t0".to_string(),
            arm: "experimental".to_string()
        }
    );
}

/// **The finite-`B` correction, pinned exactly.** `(count+1)/(B+1)` is asserted
/// in the report as a property of the instrument — *"a p-value of exactly zero
/// is not reportable"*. Replacing it with `count/B` left the whole suite green
/// and published `p = 0.0000` on the corpus below, which is the number the
/// report says finite resampling cannot justify.
///
/// A degenerate corpus is the discriminating input: every one of the 10,000
/// resamples returns `+2/3`, so `le = 0` and the uncorrected tail is exactly
/// `0/10000`. `> 0.0` alone would not discriminate every way of getting the
/// correction wrong, so the value is pinned.
#[test]
fn probe_the_finite_b_correction_is_pinned_exactly() {
    let clusters: Vec<TraceCluster> = (0..6)
        .map(|i| cluster(&format!("clone{i}"), &[true, true, false]))
        .collect();
    let b = run(&clusters).expect("interval");

    assert_eq!(
        b.p_value,
        2.0 / 10_001.0,
        "the two-sided ASL must be 2·(0+1)/(B+1) exactly, not 2·0/B"
    );
    assert!(b.p_value > 0.0, "no finite resampling justifies p = 0");
    assert_eq!(b.resamples, 10_000);
}

/// `unmatched` is documented as *"expected to stay empty — which is exactly why
/// it is reported rather than assumed"*, and the anomaly it surfaces is the one
/// `PairSeparation` calls invalidating for every cross-arm number: the two arms
/// graded against different decisions. Deleting the reverse-direction scan left
/// the suite green, because no test called `cluster_from_arms` with per-pair
/// sets that differ. This exercises **both** directions.
#[test]
fn cluster_from_arms_names_pairs_missing_from_either_direction() {
    let treatment = arm_with_pairs("treatment_arm", &["shared", "treatment-only"]);
    let baseline = arm_with_pairs("baseline_arm", &["shared", "baseline-only"]);

    let c = cluster_from_arms("t0", &treatment, &baseline);

    assert_eq!(
        c.pairs.iter().map(|p| p.pair.as_str()).collect::<Vec<_>>(),
        vec!["shared"],
        "only pairs gradeable in both arms enter the estimate"
    );
    assert_eq!(
        c.unmatched,
        vec!["baseline-only".to_string(), "treatment-only".to_string()],
        "a pair missing from either arm must be named; deleting either scan drops one"
    );
    assert_eq!(
        (c.treatment.as_str(), c.baseline.as_str()),
        ("treatment_arm", "baseline_arm")
    );
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

/// **The two ways around the suffix match** (review S8). `pooling_violation`
/// refuses a run only when both suffixes are present, so a fixture belonging to
/// neither corpus, and the same fixture repeated, both got through.
#[test]
fn a_fixture_belonging_to_no_declared_corpus_may_not_join_one() {
    // The exact laundering that was verified against the shipped CLI: pooling
    // `propositionless_abstention` into the task corpus made a corpus §9.4 calls
    // degenerate report `zero_between_cluster_variance: false`.
    assert_eq!(
        check_corpus(&["accuracy-task", "propositionless_abstention"]),
        Err(CorpusDefect::UndeclaredCorpus {
            trace: "propositionless_abstention".to_string()
        })
    );
    // And it is refused whichever corpus it is mixed into, and on its own.
    assert!(check_corpus(&["accuracy-isolation", "propositionless_abstention"]).is_err());
    assert!(check_corpus(&["propositionless_abstention"]).is_err());
    // A declared corpus still passes, and so does an empty argument list —
    // "no fixtures" is the CLI's usage error, not a corpus defect.
    assert_eq!(check_corpus(&["accuracy-task", "cache-task"]), Ok(()));
    assert_eq!(check_corpus(&[]), Ok(()));
}

#[test]
fn the_same_trace_twice_is_not_two_resampling_units() {
    assert_eq!(
        check_corpus(&["accuracy-task", "cache-task", "accuracy-task"]),
        Err(CorpusDefect::DuplicateTrace {
            trace: "accuracy-task".to_string()
        })
    );
}

#[test]
fn check_corpus_still_enforces_the_non_pooling_rule() {
    assert_eq!(
        check_corpus(&["accuracy-isolation", "cache-task"]),
        Err(CorpusDefect::Pooled {
            isolation: "accuracy-isolation".to_string(),
            task: "cache-task".to_string(),
        })
    );
}

// ---------------------------------------------------------------------------
// §9.4 read off the corpus rather than asserted on the command line
// ---------------------------------------------------------------------------

/// Every fixture committed before the driver existed is unstamped, and reads as
/// authored without anyone having to say so — which is what makes §9.4's rule
/// hold against the corpus rather than against the operator's memory.
#[test]
fn an_unstamped_fixture_is_authored_and_cannot_be_declared_recorded() {
    let fixture = task_fixture("accuracy");
    assert!(fixture.provenance.is_none(), "the corpus is hand-authored");
    assert_eq!(
        fixture_provenance(&fixture).expect("no stamp to verify"),
        CorpusProvenance::Authored
    );

    // `--corpus recorded` over it was previously accepted and printed
    // `provenance: driver_recorded` with nothing inspecting the fixture.
    let refused = reconcile_provenance(
        CorpusProvenance::Authored,
        Some(CorpusProvenance::DriverRecorded),
    )
    .expect_err("an authored corpus may not be declared recorded");
    assert_eq!(
        *refused,
        ProvenanceDefect::Undeclared {
            declared: CorpusProvenance::DriverRecorded
        }
    );
}

#[test]
fn a_stamped_fixture_is_recorded_only_while_its_digest_matches_its_trace() {
    let mut fixture = task_fixture("accuracy");
    fixture.provenance = Some(FixtureProvenance {
        driver: "ix_reference/paired_driver".to_string(),
        spec: "flaky-vs-real/v1".to_string(),
        seed: 20_260_728,
        trace_digest: trace_digest(&fixture.trace),
    });
    assert_eq!(
        fixture_provenance(&fixture).expect("digest matches"),
        CorpusProvenance::DriverRecorded
    );

    // Edit the trace and the stamp no longer attributes these decisions to the
    // run that produced it.
    fixture.trace.events.pop();
    let err = fixture_provenance(&fixture).expect_err("an edited trace must not stay recorded");
    assert!(
        matches!(*err, ProvenanceDefect::DigestMismatch { .. }),
        "{err:?}"
    );

    // Declining a verdict is always allowed: a recorded corpus may be run as
    // authored, which is the safe direction.
    assert_eq!(
        reconcile_provenance(
            CorpusProvenance::DriverRecorded,
            Some(CorpusProvenance::Authored)
        )
        .expect("downgrade is always permitted"),
        CorpusProvenance::Authored
    );
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
            treatment: "experimental".to_string(),
            baseline: "baseline".to_string(),
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

/// **The check §9.4's standing rule does not already make** (review S3). Clause 1
/// used to read `dual_rule_passes` alone. On a corpus with no between-cluster
/// variance the dual rule passes *automatically* whenever the point estimate is
/// non-zero: every resample returns the same statistic, so the interval has zero
/// width, excludes zero, and `p = 2/(B+1) < α` unconditionally.
///
/// The only thing standing between that and a mechanical §8 pass was
/// `CorpusProvenance`, which is a caller assertion. A degenerate corpus plus one
/// mis-set flag would then have converted the corpus's *design* into a clause-1
/// pass. It cannot now, whatever the flag says.
#[test]
fn a_dual_rule_pass_on_a_corpus_with_no_between_cluster_variance_is_undefined() {
    let clusters: Vec<TraceCluster> = (0..6)
        .map(|i| cluster(&format!("clone{i}"), &[true, true, false]))
        .collect();
    let b = run(&clusters).expect("interval");
    // Preconditions: the rule really does pass, and the corpus really is degenerate.
    assert!(b.dual_rule_passes);
    assert!(b.zero_between_cluster_variance);

    let decision = apply_kill_keep(
        Some(&b),
        Some(CalibrationMargin {
            experimental_mean_brier: 0.10,
            subjective_logic_mean_brier: 0.30,
        }),
        // The provenance a mis-set flag would supply.
        CorpusProvenance::DriverRecorded,
    );
    assert_eq!(
        decision.clause1,
        ClauseStatus::Undefined,
        "a zero-width interval on a corpus with no population is not a measured effect"
    );
    assert_eq!(decision.clause2, ClauseStatus::Passes);
    assert_eq!(
        decision.verdict,
        KillKeepVerdict::Kill,
        "an undefined clause cannot satisfy KEEP"
    );
    assert!(
        decision
            .rationale
            .iter()
            .any(|r| r.contains("no between-cluster variance")),
        "the reason must be named: {:?}",
        decision.rationale
    );
}

/// Degeneracy may manufacture a pass; it can never manufacture a failure. The
/// published clause-1 result on both §9.3 corpora is `fails` by an
/// identically-zero difference, and that reading must not move.
#[test]
fn a_failing_dual_rule_still_fails_on_a_degenerate_corpus() {
    let b = failing_bootstrap();
    assert!(
        b.zero_between_cluster_variance,
        "identical arms, six clones"
    );
    let decision = apply_kill_keep(Some(&b), None, CorpusProvenance::DriverRecorded);
    assert_eq!(decision.clause1, ClauseStatus::Fails);
}

/// The degeneracy detector is named for a **variance** property, so it must be
/// computed as one. Testing identity of the whole delta sequence instead is
/// strictly stronger and gives a false negative exactly where it matters: two
/// clusters with deltas `[+1, −1]` and `[−1, +1]` have distinct signatures,
/// equal means, and a zero-width interval that would have been reported as a
/// sampling result.
#[test]
fn clusters_with_equal_means_but_different_orderings_are_still_degenerate() {
    let flip = |trace: &str, up_first: bool| TraceCluster {
        trace: trace.to_string(),
        treatment: "experimental".to_string(),
        baseline: "baseline".to_string(),
        pairs: vec![
            PairedDelta {
                pair: format!("{trace}-a"),
                treatment_correct: up_first,
                baseline_correct: !up_first,
            },
            PairedDelta {
                pair: format!("{trace}-b"),
                treatment_correct: !up_first,
                baseline_correct: up_first,
            },
        ],
        unmatched: Vec::new(),
    };
    let clusters: Vec<TraceCluster> = (0..6).map(|i| flip(&format!("t{i}"), i % 2 == 0)).collect();

    let b = run(&clusters).expect("interval");
    assert_eq!(
        b.distinct_cluster_signatures, 2,
        "the sequences genuinely differ — which is why signature identity misses this"
    );
    assert!(
        (b.ci_high - b.ci_low).abs() < 1e-12,
        "precondition: the interval has zero width"
    );
    assert!(
        b.zero_between_cluster_variance,
        "equal cluster means are zero between-cluster variance, whatever the ordering"
    );
}

// ---------------------------------------------------------------------------
// §5.2's fourth metric — Conditioned Abstention Rate (issue story 11)
// ---------------------------------------------------------------------------

/// The degenerate policy §5.1's pairing exists to catch. Abstain Accuracy alone
/// reads a flattering `1.0`; the conditioned rates show the abstention is not
/// conditioned on anything.
#[test]
fn an_always_abstaining_policy_scores_zero_discrimination_however_good_its_abstain_accuracy() {
    let always_abstain = PairedScore {
        pairs: 8,
        both_correct: 0,
        paired_accuracy: Some(0.0),
        act_correct: 0,
        act_total: 8,
        abstain_correct: 8,
        abstain_total: 8,
        per_pair: Vec::new(),
        defects: Vec::new(),
    };
    let car = always_abstain.conditioned_abstention();

    assert_eq!(car.rate_when_warranted, Some(1.0));
    assert_eq!(
        car.rate_when_unwarranted,
        Some(1.0),
        "it abstains on the act halves too — the leg Abstain Accuracy cannot show"
    );
    assert_eq!(
        car.discrimination,
        Some(0.0),
        "abstention that ignores the condition discriminates nothing"
    );
    // The flattering number that made the guard necessary.
    assert_eq!(
        always_abstain.abstain_correct as f64 / always_abstain.abstain_total as f64,
        1.0
    );
}

/// The overlap, pinned rather than left to be rediscovered: under §5.1's binary
/// taxonomy `rate_when_warranted` counts the same events as Abstain Accuracy. A
/// report presenting them as independent corroboration would double-count one
/// measurement.
#[test]
fn the_warranted_leg_is_abstain_accuracy_on_every_arm_of_the_task_corpus() {
    for theme in TASK_CORPUS {
        let graded = score_paired_all_arms(task_fixture(theme), SubjectiveLogicConfig::default());
        for arm in [
            &graded.unassisted,
            &graded.recency_decay,
            &graded.lie,
            &graded.subjective_logic,
        ] {
            let s = &arm.paired;
            assert!(s.abstain_total > 0, "{theme}/{} grades nothing", arm.arm);
            let abstain_accuracy = s.abstain_correct as f64 / s.abstain_total as f64;
            assert_eq!(
                arm.conditioned_abstention.rate_when_warranted,
                Some(abstain_accuracy),
                "{theme}/{}",
                arm.arm
            );
            // And the metric is carried on the arm, so it reaches JSON consumers.
            assert_eq!(arm.conditioned_abstention, s.conditioned_abstention());
        }
    }
}

/// A perfectly conditioned policy: abstains exactly when abstaining is right.
#[test]
fn a_perfectly_conditioned_policy_scores_full_discrimination() {
    let perfect = PairedScore {
        pairs: 5,
        both_correct: 5,
        paired_accuracy: Some(1.0),
        act_correct: 5,
        act_total: 5,
        abstain_correct: 5,
        abstain_total: 5,
        per_pair: Vec::new(),
        defects: Vec::new(),
    };
    let car = perfect.conditioned_abstention();
    assert_eq!(car.rate_when_warranted, Some(1.0));
    assert_eq!(car.rate_when_unwarranted, Some(0.0));
    assert_eq!(car.discrimination, Some(1.0));
    assert_eq!(car.abstained_when_unwarranted, 0);
    assert_eq!(car.abstained_when_warranted, 5);
}

/// An unexercised condition has no rate, for the same reason an ungraded run has
/// no accuracy — `None` is not a rate of zero.
#[test]
fn an_unexercised_condition_yields_no_rate_rather_than_zero() {
    let car = PairedScore {
        pairs: 0,
        both_correct: 0,
        paired_accuracy: None,
        act_correct: 0,
        act_total: 0,
        abstain_correct: 0,
        abstain_total: 0,
        per_pair: Vec::new(),
        defects: Vec::new(),
    }
    .conditioned_abstention();
    assert_eq!(car.rate_when_warranted, None);
    assert_eq!(car.rate_when_unwarranted, None);
    assert_eq!(car.discrimination, None);
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

fn task_fixture(theme: &str) -> PairedFixture {
    let path = format!("../../fixtures/ix/paired/{theme}-task.json");
    serde_json::from_str(&std::fs::read_to_string(&path).expect("fixture must exist"))
        .expect("fixture must parse")
}

fn task_clusters(
    treatment: fn(&hari_core::PairedComparison) -> &hari_core::PairedArm,
    baseline: fn(&hari_core::PairedComparison) -> &hari_core::PairedArm,
) -> Vec<TraceCluster> {
    TASK_CORPUS
        .iter()
        .map(|theme| {
            let graded =
                score_paired_all_arms(task_fixture(theme), SubjectiveLogicConfig::default());
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
    let b = bootstrap_paired_difference(&clusters, BootstrapConfig::default())
        .expect("the corpus names one arm pair")
        .expect("54 pairs must be gradeable");

    assert_eq!(
        (b.treatment.as_str(), b.baseline.as_str()),
        ("recency_decay", "ix_unassisted"),
        "the published arm labels must be the arms the clusters were built from"
    );
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
    let b = bootstrap_paired_difference(&clusters, BootstrapConfig::default())
        .expect("the corpus names one arm pair")
        .expect("54 pairs must be gradeable");

    assert_eq!(
        (b.treatment.as_str(), b.baseline.as_str()),
        ("subjective_logic", "recency_decay")
    );
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
