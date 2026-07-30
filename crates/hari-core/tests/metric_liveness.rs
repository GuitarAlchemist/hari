//! Metric liveness — does each eval metric actually measure anything?
//!
//! Three metrics on the #35 critical path each looked implemented and each
//! measured something other than what it claimed. All three were found by
//! *using* them, not by reading them:
//!
//! * `ResearchReplayReport::calibration` folded in the whole forecast ledger
//!   while documenting itself as scoped to the trace — it reported the
//!   calibration of unrelated GA/Demerzel beliefs.
//! * Paired Accuracy, the §5.1 primary, had no scorer at all; once built it
//!   turned out to be driven by cycle arithmetic rather than by evidence.
//! * `false_rejection_count`, the §5.3 disqualifier, resolved against the
//!   arm's own `final_beliefs`, so it excused the arm that stayed stuck.
//!
//! Each was invisible to the test suite because each was *self-consistent*.
//! Unit tests confirmed they computed what they computed. Nothing asked whether
//! the number moved when the world moved.
//!
//! This guard asks that. For every metric, a declared expectation of either
//! `Varies` or `Constant { reason }`, checked against the real corpus. A
//! metric declared live that has gone constant is a dead instrument; a metric
//! declared constant that has started varying means the recorded reason is
//! stale. **Both directions fail**, because a stale "known unexercised" note is
//! how `false_rejection_count` would have quietly stayed at zero after the
//! §9.3 fixtures finally exercised it.
//!
//! Constant is not the same as broken — `attention_norm_max` is legitimately
//! zero here because only `PriorityModel::Lie` evolves attention. The point is
//! that the explanation must exist and be auditable, not that constants are
//! forbidden.

use hari_core::{CognitiveLoop, ResearchTrace};
use std::collections::BTreeSet;
use std::fs;

/// What we assert about a metric's behaviour across the corpus.
enum Liveness {
    /// Must take at least two distinct values. A live instrument.
    Varies,
    /// Legitimately constant, with the reason recorded here so it can be
    /// audited and so it fails loudly once the reason stops holding.
    Constant(&'static str),
}

/// Every `ReplayMetrics` field, with its declared liveness under the **default**
/// priority model (`RecencyDecay`) over `fixtures/ix/*.json`.
///
/// `action_counts_by_kind` is a map rather than a scalar and is covered by the
/// per-kind assertions in `phase5_replay.rs`, so it is not listed here.
fn declared() -> Vec<(&'static str, Liveness)> {
    vec![
        ("contradiction_recovery_cycles", Liveness::Varies),
        ("false_acceptance_count", Liveness::Varies),
        ("goal_completion_rate", Liveness::Varies),
        ("consensus_stability", Liveness::Varies),
        (
            "false_rejection_count",
            // #35 §9 item 1: every Wait in this corpus lands on a
            // propositionless payload (goal_update / relation_declaration), so
            // there is no abstention-on-a-claim to charge. Reaching a non-zero
            // value requires ≥12 claim-events with lagging cycle stamps, which
            // no fixture has. The §9.3 paired fixtures are what will exercise
            // it — and when they do, this declaration must be updated, which is
            // exactly what the "started varying" direction of this test forces.
            Liveness::Constant("no Wait in fixtures/ix lands on a proposition; see #35 §9 item 1"),
        ),
        (
            "attention_norm_max",
            // Correct by design, not a defect: attention is evolved only by
            // PriorityModel::Lie. Verified non-zero (1.08 – 8.63) on the lie
            // arm of `replay --compare3` across all eight fixtures.
            Liveness::Constant(
                "only PriorityModel::Lie evolves attention; non-zero on the lie arm",
            ),
        ),
    ]
}

/// Payload fields that name a proposition. Goal `key` is included: the goal
/// keys are matched against propositions to derive `goal_completion_rate`, so a
/// rename that missed them would silently change the metric for the wrong
/// reason and make the invariance test pass vacuously.
const PROPOSITION_FIELDS: [&str; 5] = ["proposition", "key", "from", "to", "superseded_by"];

/// Round-trip the trace through JSON, applying `f` to the event array.
///
/// Deliberately structural rather than an exhaustive `match` on
/// `ResearchEventPayload`: a new payload variant carrying a `proposition` is
/// picked up automatically. An exhaustive match would compile fine while
/// quietly leaving the new variant untransformed, which is how a metamorphic
/// test rots into a test that transforms nothing.
fn transform_events(
    trace: &ResearchTrace,
    f: impl Fn(&mut Vec<serde_json::Value>),
) -> ResearchTrace {
    let mut value = serde_json::to_value(trace).expect("trace serializes");
    let events = value
        .get_mut("events")
        .and_then(|e| e.as_array_mut())
        .expect("trace has an events array");
    let mut owned = std::mem::take(events);
    f(&mut owned);
    *events = owned;
    serde_json::from_value(value).expect("transformed trace deserializes")
}

/// Rename every proposition so lexicographic order is **inverted**. Metrics are
/// aggregated through `BTreeMap`s, so if any of them leaked iteration order
/// into its result this is what would expose it.
fn invert_proposition_order(trace: &ResearchTrace) -> ResearchTrace {
    transform_events(trace, |events| {
        let mut names: BTreeSet<String> = BTreeSet::new();
        for event in events.iter() {
            let Some(payload) = event.get("payload") else {
                continue;
            };
            for field in PROPOSITION_FIELDS {
                if let Some(name) = payload.get(field).and_then(|v| v.as_str()) {
                    names.insert(name.to_string());
                }
            }
        }
        let total = names.len();
        let mapping: std::collections::BTreeMap<String, String> = names
            .into_iter()
            .enumerate()
            .map(|(i, name)| (name, format!("p{:03}", total - 1 - i)))
            .collect();

        for event in events.iter_mut() {
            let Some(payload) = event.get_mut("payload") else {
                continue;
            };
            for field in PROPOSITION_FIELDS {
                let renamed = payload
                    .get(field)
                    .and_then(|v| v.as_str())
                    .and_then(|s| mapping.get(s))
                    .cloned();
                if let Some(renamed) = renamed {
                    payload[field] = serde_json::Value::String(renamed);
                }
            }
        }
    })
}

/// Rename every `source`, also inverting order.
fn invert_source_order(trace: &ResearchTrace) -> ResearchTrace {
    transform_events(trace, |events| {
        let sources: BTreeSet<String> = events
            .iter()
            .filter_map(|e| e.get("source").and_then(|v| v.as_str()))
            .map(str::to_string)
            .collect();
        let total = sources.len();
        let mapping: std::collections::BTreeMap<String, String> = sources
            .into_iter()
            .enumerate()
            .map(|(i, s)| (s, format!("agent-{:03}", total - 1 - i)))
            .collect();
        for event in events.iter_mut() {
            let renamed = event
                .get("source")
                .and_then(|v| v.as_str())
                .and_then(|s| mapping.get(s))
                .cloned();
            if let Some(renamed) = renamed {
                event["source"] = serde_json::Value::String(renamed);
            }
        }
    })
}

/// Remove every route by which an `Accept` can be invalidated: drop retractions
/// and corrections, and make every remaining assertion agree on `Probable`, so
/// nothing is downgraded to `Doubtful`/`False` or driven `Contradictory`.
fn remove_every_invalidation_route(trace: &ResearchTrace) -> ResearchTrace {
    transform_events(trace, |events| {
        events.retain(|e| {
            !matches!(
                e.get("payload")
                    .and_then(|p| p.get("type"))
                    .and_then(|t| t.as_str()),
                Some("retraction") | Some("correction")
            )
        });
        for event in events.iter_mut() {
            if let Some(payload) = event.get_mut("payload") {
                if payload.get("value").is_some() {
                    payload["value"] = serde_json::Value::String("Probable".to_string());
                }
            }
        }
    })
}

fn scalar_metrics(trace: ResearchTrace) -> serde_json::Value {
    let report = CognitiveLoop::new(trace.dimension).process_research_trace(trace);
    let mut value = serde_json::to_value(&report.metrics).expect("metrics serialize");
    // Action counts are a per-kind map; the scalar metrics are what §5 reads.
    value
        .as_object_mut()
        .expect("metrics object")
        .remove("action_counts_by_kind");
    value
}

fn corpus_paths() -> Vec<std::path::PathBuf> {
    let dir = std::path::Path::new("../../fixtures/ix");
    let mut paths: Vec<_> = std::fs::read_dir(dir)
        .expect("fixtures/ix must exist")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("json"))
        .collect();
    paths.sort();
    assert!(
        paths.len() >= 8,
        "expected the eight-fixture corpus, found {}",
        paths.len()
    );
    paths
}

fn load_trace(path: &str) -> ResearchTrace {
    let raw = fs::read_to_string(path).expect("fixture must be readable");
    match serde_json::from_str::<ResearchTrace>(&raw) {
        Ok(t) => t,
        Err(_) => {
            let events: Vec<hari_core::ResearchEvent> =
                serde_json::from_str(&raw).expect("fixture must be a trace or event array");
            events.into()
        }
    }
}

/// Replay the whole corpus and collect each metric's distinct values as JSON
/// text, so `Option`s and floats compare without bespoke handling per field.
fn corpus_values() -> (Vec<(String, BTreeSet<String>)>, usize) {
    let fixtures_dir = std::path::Path::new("../../fixtures/ix");
    let entries = std::fs::read_dir(fixtures_dir).expect("fixtures/ix directory must exist");

    let mut fields: Vec<(String, BTreeSet<String>)> = Vec::new();
    let mut fixtures = 0usize;

    for entry in entries {
        let path = entry.expect("readable directory entry").path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let trace = load_trace(&path.to_string_lossy());
        let report = CognitiveLoop::new(trace.dimension).process_research_trace(trace);
        let metrics = serde_json::to_value(&report.metrics).expect("metrics serialize");
        let object = metrics.as_object().expect("metrics is a JSON object");

        for (key, value) in object {
            if key == "action_counts_by_kind" {
                continue;
            }
            match fields.iter_mut().find(|(k, _)| k == key) {
                Some((_, seen)) => {
                    seen.insert(value.to_string());
                }
                None => {
                    let mut seen = BTreeSet::new();
                    seen.insert(value.to_string());
                    fields.push((key.clone(), seen));
                }
            }
        }
        fixtures += 1;
    }

    assert!(
        fixtures >= 8,
        "expected the eight-fixture corpus, found {fixtures}"
    );
    (fields, fixtures)
}

#[test]
fn every_metric_is_either_live_or_declared_constant_with_a_reason() {
    let (observed, fixtures) = corpus_values();
    let declared = declared();

    // No metric may exist without a declaration — a new field added to
    // ReplayMetrics must state whether it is expected to move.
    for (key, _) in &observed {
        assert!(
            declared.iter().any(|(name, _)| name == key),
            "ReplayMetrics::{key} has no liveness declaration in metric_liveness.rs. \
             Add one: either Varies, or Constant with the reason it cannot move."
        );
    }
    // And no declaration may name a field that no longer exists.
    for (name, _) in &declared {
        assert!(
            observed.iter().any(|(key, _)| key == name),
            "metric_liveness.rs declares {name}, which is not in ReplayMetrics any more"
        );
    }

    for (name, liveness) in &declared {
        let values = &observed
            .iter()
            .find(|(key, _)| key == name)
            .expect("checked above")
            .1;

        match liveness {
            Liveness::Varies => assert!(
                values.len() > 1,
                "DEAD INSTRUMENT: {name} is constant at {:?} across all {fixtures} fixtures, but \
                 is declared live. Either the metric is broken, or the corpus no longer \
                 exercises it — and if the latter, say so with Liveness::Constant and a reason \
                 rather than leaving a metric that cannot inform a decision.",
                values.iter().next()
            ),
            Liveness::Constant(reason) => assert!(
                values.len() == 1,
                "STALE DECLARATION: {name} now takes {} distinct values {:?}, but is declared \
                 constant because \"{reason}\". The reason no longer holds. Update the \
                 declaration — and check whether any analysis relied on this metric being \
                 unexercised.",
                values.len(),
                values
            ),
        }
    }
}

// ---------------------------------------------------------------------------
// Metamorphic properties — the half the liveness check cannot reach.
//
// Liveness catches a *dead* metric (constant where it should move) and a *stale*
// declaration. It cannot catch a metric that varies for the wrong reason: Paired
// Accuracy varies, it just varies with trace position rather than with evidence.
// Closing that needs the two directions below — a metric must move when the thing
// it measures moves, and must NOT move when anything else does.
// ---------------------------------------------------------------------------

/// The transforms must actually transform.
///
/// An invariance test whose transform is a no-op passes forever while checking
/// nothing — the exact shape of the three failures this file exists to prevent,
/// applied to the file itself. So each transform is required to visibly change
/// the trace before its invariance is allowed to mean anything.
#[test]
fn the_metamorphic_transforms_are_not_no_ops() {
    for path in corpus_paths() {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let trace = load_trace(&path.to_string_lossy());
        let original = serde_json::to_value(&trace).expect("serializes");

        for (label, transformed) in [
            ("invert_proposition_order", invert_proposition_order(&trace)),
            ("invert_source_order", invert_source_order(&trace)),
            (
                "remove_every_invalidation_route",
                remove_every_invalidation_route(&trace),
            ),
        ] {
            let after = serde_json::to_value(&transformed).expect("serializes");
            assert_ne!(
                original, after,
                "{name}: {label} left the trace byte-identical, so the property it guards is \
                 being asserted about an untransformed input and cannot fail"
            );
        }

        // Positive control. `no_metric_depends_on_what_propositions_are_named`
        // is only meaningful if the rename actually perturbs `BTreeMap`
        // iteration order. This canary is *deliberately* order-dependent — it
        // reads the first key in sorted order — so it must change. If it ever
        // stops changing, the rename has stopped inverting order and the
        // invariance test above has quietly become a tautology.
        let canary = |t: &ResearchTrace| -> Option<String> {
            let report = CognitiveLoop::new(t.dimension).process_research_trace(t.clone());
            report.final_beliefs.keys().next().cloned()
        };
        let before = canary(&trace);
        let after = canary(&invert_proposition_order(&trace));
        assert!(
            before.is_some() && after.is_some(),
            "{name}: canary found no beliefs, so it cannot detect an ordering change"
        );
        assert_ne!(
            before, after,
            "{name}: the first belief in sorted order is unchanged after an order-inverting \
             rename, so the rename is not perturbing BTreeMap order and the invariance test \
             above proves nothing"
        );

        // Renaming must also preserve the *shape* of the trace — same event
        // count and same number of distinct propositions. A rename that
        // collapsed two propositions into one would change metrics for a
        // reason that has nothing to do with ordering.
        let renamed = invert_proposition_order(&trace);
        assert_eq!(
            trace.events.len(),
            renamed.events.len(),
            "{name}: renaming changed the event count"
        );
        let distinct = |t: &ResearchTrace| -> usize {
            t.events
                .iter()
                .flat_map(|e| e.touched_propositions())
                .collect::<BTreeSet<_>>()
                .len()
        };
        assert_eq!(
            distinct(&trace),
            distinct(&renamed),
            "{name}: renaming collapsed distinct propositions together"
        );
    }
}

/// **Invariance.** Metrics are aggregated through `BTreeMap`s keyed by
/// proposition, so their iteration order is the propositions' lexicographic
/// order. Renaming propositions to invert that order must leave every metric
/// untouched; if it does not, a metric is reading key order as if it were data.
#[test]
fn no_metric_depends_on_what_propositions_are_named() {
    for path in corpus_paths() {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let trace = load_trace(&path.to_string_lossy());
        let before = scalar_metrics(trace.clone());
        let after = scalar_metrics(invert_proposition_order(&trace));

        assert_eq!(
            before, after,
            "{name}: metrics changed when propositions were renamed to invert their \
             lexicographic order. Some metric is reading BTreeMap iteration order as data."
        );
    }
}

/// **Invariance.** The default trust model is `Equal` and per-source evidence
/// weights are off, so under the default configuration no metric may depend on
/// *who* said something. This is the guard that would fail if a future
/// source-weighting change leaked into the default path — the property #14's
/// `evidence_weight` work is explicitly designed to preserve.
#[test]
fn no_metric_depends_on_which_agent_spoke_under_the_default_trust_model() {
    for path in corpus_paths() {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let trace = load_trace(&path.to_string_lossy());
        let before = scalar_metrics(trace.clone());
        let after = scalar_metrics(invert_source_order(&trace));

        assert_eq!(
            before, after,
            "{name}: metrics changed when event sources were renamed. Under TrustModel::Equal \
             with no source weights installed, identity must not move a metric."
        );
    }
}

/// **Sensitivity.** `false_acceptance_count` claims to count `Accept`s later
/// retracted, downgraded, or driven `Contradictory`; `contradiction_recovery_cycles`
/// claims to measure recovery from contradiction. Remove every route to either
/// — drop retractions and corrections, make all assertions agree on `Probable`
/// — and both must collapse. A metric that survives the removal of the thing it
/// counts is not counting that thing.
///
/// This is the direction that was missing when three metrics passed their unit
/// tests while measuring something other than their documentation.
#[test]
fn invalidation_metrics_collapse_when_nothing_can_be_invalidated() {
    let mut nonzero_before = 0usize;

    for path in corpus_paths() {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let trace = load_trace(&path.to_string_lossy());
        let before = scalar_metrics(trace.clone());
        let after = scalar_metrics(remove_every_invalidation_route(&trace));

        if before["false_acceptance_count"].as_u64().unwrap_or(0) > 0 {
            nonzero_before += 1;
        }

        assert_eq!(
            after["false_acceptance_count"], 0,
            "{name}: false_acceptance_count is {} with every invalidation route removed. \
             Nothing is retracted, nothing is downgraded, nothing goes Contradictory — so \
             whatever it is counting, it is not invalidated acceptances.",
            after["false_acceptance_count"]
        );
        assert!(
            after["contradiction_recovery_cycles"].is_null(),
            "{name}: contradiction_recovery_cycles is {} in a trace where every assertion \
             agrees, so no proposition can ever have been Contradictory.",
            after["contradiction_recovery_cycles"]
        );
    }

    // Guard against the test passing because the corpus never invalidated
    // anything in the first place — then 0 -> 0 would prove nothing.
    assert!(
        nonzero_before >= 5,
        "only {nonzero_before} fixture(s) had a non-zero false_acceptance_count to begin with; \
         the sensitivity check needs a corpus that actually exercises invalidation, or it is \
         asserting 0 == 0"
    );
}

/// The §5.4 exclusions rest on a factual claim: that `consensus_stability` and
/// `goal_completion_rate` are *tied by construction* because they read event
/// payloads upstream of the policy layer, so "no policy can move them". A tie
/// asserted in prose and never checked is how a metric gets excluded for the
/// wrong reason, so the claim is measured.
///
/// Measured result, which is not what §5.4 says:
///
/// * `consensus_stability` — genuinely tied across all three arms. Exclusion
///   stands.
/// * `goal_completion_rate` — tied between `RecencyDecay` and `Lie`, but
///   **`SubjectiveLogic` moves it on 5 of 8 fixtures**, because
///   `process_research_trace_subjective_logic` assigns
///   `goal.status = hex_value_for_opinion(...)`, i.e. from its posterior. That
///   is post-policy state, which is precisely the condition §5.4 names for
///   reinstatement.
///
/// So the exclusion was sound for the two-arm Phase 5 comparison it was written
/// against and became wrong when SL was added as an arm. Both facts are pinned:
/// if `consensus_stability` starts moving, or if SL stops moving
/// `goal_completion_rate`, this fails and the §5.4 decision gets revisited.
#[test]
fn the_section_5_4_exclusions_hold_only_where_measured() {
    use hari_core::{compare_replay_three_way, SubjectiveLogicConfig};

    let fixtures_dir = std::path::Path::new("../../fixtures/ix");
    let mut checked = 0usize;
    let mut stability_untied: Vec<String> = Vec::new();
    let mut goal_decay_vs_lie_untied: Vec<String> = Vec::new();
    let mut goal_moved_by_sl: Vec<String> = Vec::new();

    for entry in std::fs::read_dir(fixtures_dir).expect("fixtures/ix must exist") {
        let path = entry.expect("readable entry").path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let three = compare_replay_three_way(
            load_trace(&path.to_string_lossy()),
            SubjectiveLogicConfig::default(),
        );
        let c = &three.comparison;

        let (s_decay, s_lie, s_sl) = (
            c.recency_decay.consensus_stability,
            c.lie.consensus_stability,
            c.subjective_logic.consensus_stability,
        );
        if (s_decay - s_lie).abs() > 1e-12 || (s_decay - s_sl).abs() > 1e-12 {
            stability_untied.push(format!("{name}: {s_decay} / {s_lie} / {s_sl}"));
        }

        let (g_decay, g_lie, g_sl) = (
            c.recency_decay.goal_completion_rate,
            c.lie.goal_completion_rate,
            c.subjective_logic.goal_completion_rate,
        );
        if (g_decay - g_lie).abs() > 1e-12 {
            goal_decay_vs_lie_untied.push(format!("{name}: decay={g_decay} lie={g_lie}"));
        }
        if (g_decay - g_sl).abs() > 1e-12 {
            goal_moved_by_sl.push(format!("{name}: decay={g_decay} sl={g_sl}"));
        }
        checked += 1;
    }

    assert!(
        checked >= 8,
        "expected the eight-fixture corpus, got {checked}"
    );

    assert!(
        stability_untied.is_empty(),
        "consensus_stability is excluded by §5.4 as an artifactual tie, but it now differs \
         across arms on {} case(s): {stability_untied:#?}\nA policy can move it, so it is not \
         artifactual and the exclusion needs re-deciding under §10.",
        stability_untied.len()
    );

    assert!(
        goal_decay_vs_lie_untied.is_empty(),
        "goal_completion_rate was tied between RecencyDecay and Lie; it no longer is: \
         {goal_decay_vs_lie_untied:#?}"
    );

    assert!(
        !goal_moved_by_sl.is_empty(),
        "goal_completion_rate no longer differs between RecencyDecay and SubjectiveLogic on any \
         fixture. It used to differ on 5 of 8, which is why §5.4's \"no policy can move them\" \
         does not hold for this metric. If SL has stopped deriving goal.status from its \
         posterior, the §5.4 amendment recording this needs revisiting."
    );
    assert!(
        goal_moved_by_sl.len() >= 5,
        "SL moved goal_completion_rate on only {} fixture(s); the recorded finding is 5 of 8: \
         {goal_moved_by_sl:#?}",
        goal_moved_by_sl.len()
    );
}
