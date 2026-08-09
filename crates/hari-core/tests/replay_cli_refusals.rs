//! Subprocess-level tests for `hari-core replay`'s #35 argument flow.
//!
//! The 200-odd lines that parse `--paired --compare3 --bootstrap`, apply
//! §9.3.2's corpus rules to real filesystem paths, and read §9.4's provenance
//! off the fixtures had no test of any kind. Every refusal below was verified by
//! hand during review and then left unpinned — including the pooling rule, whose
//! real input is `Path::file_stem()` over user-supplied paths rather than the
//! bare stems `pooling_violation`'s unit test passes it. That step is exactly
//! where the rule can silently stop working: a `.json.bak` path, or a stem-less
//! argument falling back to the whole path.
//!
//! Template: `phase6_serve_subprocess.rs`. The binary comes from
//! `CARGO_BIN_EXE_hari-core`, so no extra build step is needed.

use std::path::PathBuf;
use std::process::{Command, Output};

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/ix/paired")
}

fn task(theme: &str) -> String {
    fixtures()
        .join(format!("{theme}-task.json"))
        .display()
        .to_string()
}

fn replay(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_hari-core"))
        .arg("replay")
        .args(args)
        .output()
        .expect("spawn hari-core replay")
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).to_string()
}

// ---------------------------------------------------------------------------
// The argument-flow guards
// ---------------------------------------------------------------------------

#[test]
fn bootstrap_requires_the_paired_three_way_scorer() {
    let out = replay(&["--bootstrap", &task("accuracy")]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        stderr(&out).contains("requires --paired --compare3"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn only_bootstrap_takes_more_than_one_fixture_path() {
    let out = replay(&["--paired", &task("accuracy"), &task("cache")]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        stderr(&out).contains("only --bootstrap takes more than one"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn an_unknown_corpus_provenance_is_refused_rather_than_defaulted() {
    let out = replay(&[
        "--paired",
        "--compare3",
        "--bootstrap",
        "--corpus",
        "bogus",
        &task("accuracy"),
    ]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        stderr(&out).contains("takes `authored` or `recorded`"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn the_bootstrap_needs_at_least_one_fixture() {
    let out = replay(&["--paired", "--compare3", "--bootstrap"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("usage:"), "{}", stderr(&out));
}

// ---------------------------------------------------------------------------
// §9.3.2 and its two holes, over real paths
// ---------------------------------------------------------------------------

#[test]
fn the_two_corpora_may_not_be_pooled_through_real_paths() {
    let isolation = fixtures()
        .join("accuracy-isolation.json")
        .display()
        .to_string();
    let out = replay(&[
        "--paired",
        "--compare3",
        "--bootstrap",
        &isolation,
        &task("cache"),
    ]);
    assert_eq!(out.status.code(), Some(1));
    let err = stderr(&out);
    assert!(err.contains("§9.3.2"), "{err}");
    assert!(err.contains("accuracy-isolation"), "{err}");
    assert!(err.contains("cache-task"), "{err}");
}

/// The laundering that the suffix match admitted: a committed paired fixture
/// belonging to neither corpus pooled freely into the task corpus and made a
/// corpus §9.4 calls degenerate report `zero_between_cluster_variance: false`.
#[test]
fn a_fixture_belonging_to_no_declared_corpus_is_refused_by_the_cli() {
    let stray = fixtures()
        .join("propositionless_abstention.json")
        .display()
        .to_string();
    let out = replay(&[
        "--paired",
        "--compare3",
        "--bootstrap",
        &task("accuracy"),
        &stray,
    ]);
    assert_eq!(out.status.code(), Some(1));
    let err = stderr(&out);
    assert!(err.contains("propositionless_abstention"), "{err}");
    assert!(err.contains("belongs to no corpus"), "{err}");
}

#[test]
fn the_same_fixture_twice_is_refused_by_the_cli() {
    let out = replay(&[
        "--paired",
        "--compare3",
        "--bootstrap",
        &task("accuracy"),
        &task("cache"),
        &task("accuracy"),
    ]);
    assert_eq!(out.status.code(), Some(1));
    let err = stderr(&out);
    assert!(err.contains("appears twice"), "{err}");
    assert!(err.contains("accuracy-task"), "{err}");
}

// ---------------------------------------------------------------------------
// §9.4 read off the corpus (review S3)
// ---------------------------------------------------------------------------

/// `--corpus recorded` over the six hand-authored task fixtures used to be
/// accepted, printing `"verdict": "kill", "provenance": "driver_recorded"`,
/// with nothing inspecting the fixtures.
#[test]
fn an_authored_corpus_may_not_be_declared_recorded_on_the_command_line() {
    let out = replay(&[
        "--paired",
        "--compare3",
        "--bootstrap",
        "--corpus",
        "recorded",
        &task("accuracy"),
        &task("cache"),
    ]);
    assert_eq!(out.status.code(), Some(1));
    let err = stderr(&out);
    assert!(
        err.contains("no fixture carries a driver provenance stamp"),
        "{err}"
    );
    assert!(err.contains("§9.4"), "{err}");
}

/// The published run, end to end. Every number in report §3 and §4 comes out of
/// this invocation, so the shape of its output is worth pinning at the CLI and
/// not only at the library boundary.
#[test]
fn the_published_task_corpus_run_withholds_the_verdict_and_names_its_arms() {
    let themes = [
        "accuracy",
        "cache",
        "cost",
        "latency",
        "memory",
        "throughput",
    ];
    let mut args = vec!["--paired", "--compare3", "--bootstrap"];
    let paths: Vec<String> = themes.iter().map(|t| task(t)).collect();
    args.extend(paths.iter().map(String::as_str));

    let out = replay(&args);
    assert!(out.status.success(), "{}", stderr(&out));
    let report: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("the CLI emits one JSON object");

    let clause1 = &report["bootstrap"]["clause1_experimental_vs_unassisted"];
    assert_eq!(clause1["treatment"], "recency_decay");
    assert_eq!(clause1["baseline"], "ix_unassisted");
    assert_eq!(clause1["difference"], 0.0);
    assert_eq!(clause1["every_delta_is_zero"], true);

    let cheap = &report["bootstrap"]["cheap_baseline_sl_vs_experimental"];
    assert_eq!(cheap["treatment"], "subjective_logic");
    assert_eq!(cheap["baseline"], "recency_decay");
    assert_eq!(cheap["zero_between_cluster_variance"], true);

    let kk = &report["kill_keep"];
    assert_eq!(kk["provenance"], "authored");
    assert_eq!(kk["clause1"], "fails");
    assert_eq!(kk["clause2"], "undefined");
    assert_eq!(kk["verdict"], "withheld_by_standing_rule");

    // §5.2's fourth metric reaches the JSON, per policy (story 11).
    let car = &report["corpus"][0]["conditioned_abstention"];
    assert!(
        car["recency_decay"]["rate_when_warranted"].is_number(),
        "{car}"
    );
    assert!(
        car["subjective_logic"]["discrimination"].is_number(),
        "{car}"
    );

    // §9.4's degeneracy rail reaches a human, rather than a channel with no
    // subscriber — it was emitted through `warn!` before any subscriber existed
    // on the replay path, so stderr was empty on every published run.
    let err = stderr(&out);
    assert!(err.contains("zero between-cluster variance"), "{err}");
    assert!(err.contains("§9.4"), "{err}");
}

// ---------------------------------------------------------------------------
// The driver path (#35 §9 item 4 / story 12)
// ---------------------------------------------------------------------------

/// The digest is computed in Python by the driver and recomputed in Rust here,
/// so this pins **cross-language agreement** on `trace_digest`. It is the whole
/// mechanism by which provenance is a fact about the corpus rather than an
/// assertion: if the two implementations drift, a correctly generated fixture
/// stops verifying and this fails.
#[test]
fn a_driver_recorded_fixture_verifies_its_own_digest() {
    let path = fixtures().join("driver/accuracy-task.json");
    let fixture: hari_core::PairedFixture =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("driver smoke fixture"))
            .expect("parses");

    let stamp = fixture
        .provenance
        .as_ref()
        .expect("the driver stamps every fixture it records");
    assert_eq!(stamp.driver, "ix_reference/paired_driver");
    assert_eq!(stamp.spec, "flaky-vs-real/v1");
    assert_eq!(
        hari_core::trace_digest(&fixture.trace),
        stamp.trace_digest,
        "the Rust and Python digests disagree — provenance would refuse a corpus \
         the driver correctly generated"
    );
    assert_eq!(
        hari_core::fixture_provenance(&fixture).expect("digest verifies"),
        hari_core::CorpusProvenance::DriverRecorded
    );
}

/// A driver-recorded corpus reaches a **mechanical** §8 verdict — the thing
/// §9.4's standing rule makes unreachable on every authored corpus, and the
/// thing story 14 asks for. The verdict is KILL, which is the pre-registered
/// negative result and not a favourable one.
#[test]
fn a_driver_recorded_corpus_reaches_a_mechanical_verdict() {
    let path = fixtures()
        .join("driver/accuracy-task.json")
        .display()
        .to_string();
    let out = replay(&["--paired", "--compare3", "--bootstrap", &path]);
    assert!(out.status.success(), "{}", stderr(&out));
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).expect("one JSON object");

    let kk = &report["kill_keep"];
    assert_eq!(
        kk["provenance"], "driver_recorded",
        "provenance is derived from the fixture, with no --corpus flag passed"
    );
    assert_eq!(kk["verdict"], "kill");
    assert_eq!(
        kk["clause2"], "undefined",
        "per-arm calibration still does not exist (§9 item 2)"
    );
}
