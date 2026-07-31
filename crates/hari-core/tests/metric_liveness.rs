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
         fixture, so §5.4's \"no policy can move them\" would hold again. If SL has stopped \
         deriving goal.status from its posterior, the §5.4 amendment needs revisiting."
    );
    // Was 5 of 8. The goal-starvation fix removed `long_recovery` from the list —
    // both arms now reach 1.0 there — leaving 4: three driven by the hexavalent
    // write being upgrade-only (staleness) and one by a genuine posterior
    // difference on `heavy_contradiction`. If this drops to 3, check whether the
    // staleness defect was fixed, because §5.4 records that doing so is what
    // would move this metric in SL's favour.
    assert_eq!(
        goal_moved_by_sl.len(),
        4,
        "SL moves goal_completion_rate on {} fixture(s); the recorded finding is 4 of 8 after \
         the starvation fix: {goal_moved_by_sl:#?}",
        goal_moved_by_sl.len()
    );
}

// ---------------------------------------------------------------------------
// Generated traces — the properties above hold *on eight fixtures*, which is a
// corpus-conditional claim, not a property. These re-run them over hundreds of
// generated traces so a metric cannot satisfy them by accident of the corpus.
//
// Traces are generated by **recombining real events** (all 115 of them, pooled
// across fixtures) rather than synthesising payloads. Every event is then
// schema-valid by construction, no generator can drift from the payload enum as
// variants are added, and the search still explores orderings, interleavings,
// and proposition/goal pairings that no fixture contains.
//
// Deterministic: fixed-seed xorshift64*, no external deps — matching the idiom
// in `dp_postulates_probe.rs`.
// ---------------------------------------------------------------------------

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// Every event in `fixtures/ix/*.json`, as JSON, pooled for recombination.
fn event_pool() -> Vec<serde_json::Value> {
    let mut pool = Vec::new();
    for path in corpus_paths() {
        let trace = load_trace(&path.to_string_lossy());
        let value = serde_json::to_value(&trace).expect("serializes");
        if let Some(events) = value.get("events").and_then(|e| e.as_array()) {
            pool.extend(events.iter().cloned());
        }
    }
    assert!(
        pool.len() >= 100,
        "expected a pool of ~115 real events, got {}",
        pool.len()
    );
    pool
}

/// Sample `3..=24` events from the pool, with replacement.
fn generate_trace(rng: &mut Rng, pool: &[serde_json::Value]) -> ResearchTrace {
    let len = 3 + rng.below(22);
    let events: Vec<serde_json::Value> = (0..len)
        .map(|_| pool[rng.below(pool.len())].clone())
        .collect();
    serde_json::from_value(serde_json::json!({ "dimension": 4, "events": events }))
        .expect("recombined events deserialize")
}

const TRIALS: usize = 400;

/// Metrics that may differ in their **last bit** under a proposition rename,
/// and why.
///
/// `consensus_stability` averages per-proposition stabilities by iterating a
/// `BTreeMap` keyed by proposition, so renaming changes the summation order, and
/// floating-point addition is not associative. Observed spread is 1 ULP
/// (`0.8148148148148148` vs `0.8148148148148149`).
///
/// This is a genuine order dependence, just an inconsequential one: it is ~14
/// orders of magnitude below anything §6's paired bootstrap could resolve. It is
/// listed rather than absorbed into a blanket tolerance so that a *new* metric
/// developing order sensitivity fails instead of being quietly forgiven.
const FLOAT_REORDER_TOLERANT: [&str; 1] = ["consensus_stability"];

/// Tolerance for the metrics above. Four orders of magnitude above the observed
/// 1-ULP spread, and still far below any difference that could change a verdict.
const REORDER_TOLERANCE: f64 = 1e-12;

/// Compare two metric objects that differ only by proposition ordering.
///
/// Everything not in [`FLOAT_REORDER_TOLERANT`] must be **exactly** equal —
/// including the integer counts, where a one-off difference would be a real bug
/// rather than a rounding artifact. Returns the keys that needed the tolerance,
/// so a caller can assert the tolerance is actually being exercised as expected
/// rather than silently covering for something new.
fn assert_equivalent_under_reordering(
    before: &serde_json::Value,
    after: &serde_json::Value,
    context: &str,
) -> BTreeSet<String> {
    let (a, b) = (
        before.as_object().expect("metrics object"),
        after.as_object().expect("metrics object"),
    );
    assert_eq!(
        a.keys().collect::<Vec<_>>(),
        b.keys().collect::<Vec<_>>(),
        "{context}: metric key sets differ"
    );

    let mut needed_tolerance = BTreeSet::new();
    for (key, lhs) in a {
        let rhs = &b[key];
        if lhs == rhs {
            continue;
        }
        assert!(
            FLOAT_REORDER_TOLERANT.contains(&key.as_str()),
            "{context}: {key} changed under a proposition rename ({lhs} -> {rhs}). It is not on \
             the float-summation tolerance list, so this is order sensitivity in the metric \
             itself, not rounding."
        );
        let (lhs_f, rhs_f) = (
            lhs.as_f64()
                .unwrap_or_else(|| panic!("{context}: {key} tolerant but not numeric: {lhs}")),
            rhs.as_f64()
                .unwrap_or_else(|| panic!("{context}: {key} tolerant but not numeric: {rhs}")),
        );
        assert!(
            (lhs_f - rhs_f).abs() <= REORDER_TOLERANCE,
            "{context}: {key} moved by {} under a proposition rename, which is far more than \
             float summation order can explain ({lhs_f} -> {rhs_f}).",
            (lhs_f - rhs_f).abs()
        );
        needed_tolerance.insert(key.clone());
    }
    needed_tolerance
}

/// First belief in sorted order — deliberately order-dependent, used as the
/// positive control that an order-inverting rename really perturbs iteration.
fn first_belief(trace: &ResearchTrace) -> Option<String> {
    CognitiveLoop::new(trace.dimension)
        .process_research_trace(trace.clone())
        .final_beliefs
        .keys()
        .next()
        .cloned()
}

/// `no_metric_depends_on_what_propositions_are_named`, over generated traces.
#[test]
fn theorem_metrics_are_name_invariant_over_generated_traces() {
    let pool = event_pool();
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut canary_moved = 0usize;
    let mut tolerated: BTreeSet<String> = BTreeSet::new();
    let mut tie_traces = 0usize;

    for trial in 0..TRIALS {
        let trace = generate_trace(&mut rng, &pool);
        let renamed = invert_proposition_order(&trace);

        let before = scalar_metrics(trace.clone());
        let after = scalar_metrics(renamed.clone());

        // The `goal_completion_rate` exemption that used to live here is gone.
        // It was needed while goal status was written for `top_goal` only, which
        // made the metric depend on which goal won a priority tie. Now that every
        // goal's status is refreshed each cycle, name invariance holds even under
        // a tie — asserted directly by
        // `theorem_goal_completion_rate_is_name_invariant_under_a_priority_tie`.
        // Ties are still counted, to confirm generation reaches them at all.
        if has_tied_goal_priorities(&trace) {
            tie_traces += 1;
        }

        tolerated.extend(assert_equivalent_under_reordering(
            &before,
            &after,
            &format!(
                "trial {trial} (trace: {})",
                serde_json::to_string(&trace).unwrap_or_default()
            ),
        ));

        // Same positive control as the corpus test. Not every generated trace
        // has two distinct propositions, so this is counted across trials
        // rather than asserted on each one.
        if first_belief(&trace) != first_belief(&renamed) {
            canary_moved += 1;
        }
    }

    assert!(
        canary_moved * 2 >= TRIALS,
        "the rename perturbed belief ordering on only {canary_moved}/{TRIALS} trials; the \
         invariance claim is mostly being made about unperturbed inputs"
    );

    // The exemption must be doing real work, and must not be doing all of it:
    // if every trial had a tie, `goal_completion_rate` would never be checked;
    // if none did, the known violation has stopped being reachable and its
    // exemption should go.
    // Generation must still reach priority ties, because a tie is the condition
    // under which `goal_completion_rate` used to become name-dependent. No
    // exemption is applied any more — these trials are held to exact equality
    // like the rest — but if generation stopped producing ties we would lose the
    // coverage that proves the starvation fix holds under one.
    assert!(
        tie_traces > 0,
        "no generated trace declared two goals at the same priority, so nothing here exercises \
         the tie case the goal-starvation fix was measured against — widen generation"
    );

    // The tolerance list is an allowance, not a prediction: if a listed metric
    // stops needing it, the entry is stale and should go, because a stale
    // allowance is exactly how a future order bug would get forgiven silently.
    assert_eq!(
        tolerated,
        FLOAT_REORDER_TOLERANT
            .iter()
            .map(|s| (*s).to_string())
            .collect::<BTreeSet<_>>(),
        "the set of metrics needing the float-reordering tolerance has changed. If a metric no \
         longer needs it, drop it from FLOAT_REORDER_TOLERANT so the exact-equality guarantee \
         tightens; if a new one appears, the assertion above would already have failed."
    );
}

/// `no_metric_depends_on_which_agent_spoke...`, over generated traces. Stronger
/// here than on the corpus: recombination mixes sources across fixtures, so one
/// trace can carry agents that never co-occur in any real fixture.
#[test]
fn theorem_metrics_are_source_invariant_over_generated_traces() {
    let pool = event_pool();
    let mut rng = Rng(0xD1B5_4A32_D192_ED03);

    for trial in 0..TRIALS {
        let trace = generate_trace(&mut rng, &pool);
        assert_eq!(
            scalar_metrics(trace.clone()),
            scalar_metrics(invert_source_order(&trace)),
            "trial {trial}: metrics moved when sources were renamed under TrustModel::Equal.\ntrace: {}",
            serde_json::to_string(&trace).unwrap_or_default()
        );
    }
}

/// `invalidation_metrics_collapse...`, over generated traces.
#[test]
fn theorem_invalidation_metrics_collapse_over_generated_traces() {
    let pool = event_pool();
    let mut rng = Rng(0xA076_1D64_78BD_642F);
    let mut exercised = 0usize;

    for trial in 0..TRIALS {
        let trace = generate_trace(&mut rng, &pool);
        if scalar_metrics(trace.clone())["false_acceptance_count"]
            .as_u64()
            .unwrap_or(0)
            > 0
        {
            exercised += 1;
        }

        let harmless = scalar_metrics(remove_every_invalidation_route(&trace));
        assert_eq!(
            harmless["false_acceptance_count"],
            0,
            "trial {trial}: false_acceptance_count is {} with every invalidation route \
             removed.\ntrace: {}",
            harmless["false_acceptance_count"],
            serde_json::to_string(&trace).unwrap_or_default()
        );
        assert!(
            harmless["contradiction_recovery_cycles"].is_null(),
            "trial {trial}: contradiction_recovery_cycles is {} though every assertion \
             agrees.\ntrace: {}",
            harmless["contradiction_recovery_cycles"],
            serde_json::to_string(&trace).unwrap_or_default()
        );
    }

    assert!(
        exercised * 4 >= TRIALS,
        "only {exercised}/{TRIALS} generated traces had a non-zero false_acceptance_count; the \
         sensitivity claim is mostly asserting 0 == 0"
    );
}

/// Replay must survive every recombined trace.
///
/// Recombination produces inputs no fixture does: a retraction for a claim never
/// asserted, a supersession pointing at an absent proposition, a relation
/// withdrawal with no matching edge, the same event twice. The property tests
/// above would surface a panic as a confusing failure, so robustness is asserted
/// on its own.
#[test]
fn theorem_replay_survives_recombined_traces() {
    let pool = event_pool();
    let mut rng = Rng(0x27D4_EB2F_1656_67C5);
    let mut shapes = BTreeSet::new();

    for _ in 0..TRIALS {
        let trace = generate_trace(&mut rng, &pool);
        let kinds: BTreeSet<String> = trace
            .events
            .iter()
            .map(|e| {
                serde_json::to_value(&e.payload)
                    .ok()
                    .and_then(|v| v.get("type").and_then(|t| t.as_str()).map(str::to_string))
                    .unwrap_or_default()
            })
            .collect();
        shapes.insert(kinds.into_iter().collect::<Vec<_>>().join("+"));

        let events = trace.events.len();
        let report = CognitiveLoop::new(trace.dimension).process_research_trace(trace);
        assert_eq!(
            report.outcomes.len(),
            events,
            "replay dropped or invented an outcome"
        );
    }

    // Recombination is only worth the runtime if it reaches payload mixes the
    // fixtures cannot produce between them.
    assert!(
        shapes.len() >= 20,
        "recombination produced only {} distinct payload-type combinations; it is not exploring \
         beyond the corpus",
        shapes.len()
    );
}

/// Did two goals *ever* share a priority during this replay?
///
/// `CognitiveState::top_goal` resolves ties by `BTreeMap` order, so a tie is
/// exactly the condition under which goal handling becomes name-dependent —
/// see `known_violation_top_goal_ties_are_broken_by_goal_name`.
///
/// Checked at **every prefix**, not on the final state. A goal's priority can be
/// revised by a later `goal_update`, so a tie can exist while `top_goal` is being
/// consulted and be gone by the end of the trace. Trial 243 of
/// `theorem_metrics_are_name_invariant_over_generated_traces` was exactly that:
/// two goals at 0.85 early, one later raised to 0.95, final priorities all
/// distinct — and the metric still moved under a rename. A final-state check
/// would call that trace tie-free and report the known violation as a new bug.
fn has_tied_goal_priorities(trace: &ResearchTrace) -> bool {
    let mut priorities: std::collections::BTreeMap<String, String> = Default::default();
    for event in &trace.events {
        let value = serde_json::to_value(&event.payload).unwrap_or_default();
        if value.get("type").and_then(|t| t.as_str()) != Some("goal_update") {
            continue;
        }
        let (Some(key), Some(priority)) = (
            value.get("key").and_then(|k| k.as_str()),
            value.get("priority"),
        ) else {
            continue;
        };
        priorities.insert(key.to_string(), priority.to_string());

        let distinct: BTreeSet<&String> = priorities.values().collect();
        if distinct.len() < priorities.len() {
            return true;
        }
    }
    false
}

/// **Was a known violation; now a theorem.** `CognitiveState::top_goal` still
/// resolves ties by `BTreeMap` order — `Iterator::max_by` returns the *last*
/// maximum, so the alphabetically-later key wins. That is unchanged and
/// deliberately so: the tie-break was the **symptom**. The disease was that goal
/// *status* was written only for whichever goal held the top slot, so the
/// tie-break decided which goals were ever evaluated at all.
///
/// Now that status is refreshed for every goal each cycle
/// (`crates/hari-core/src/lib.rs`, the bulk refresh after the top-goal action
/// block), which goal happens to win a tie no longer determines what gets
/// looked at, and `goal_completion_rate` is name-invariant under a tie. This
/// test previously asserted the inequality and was expected to fail on fix; it
/// now asserts the equality.
///
/// Worth recording why the *obvious* fix was not taken: switching `max_by` to
/// first-alphabetical merely flips the sign of the difference (0.5 -> 1.0
/// becomes 1.0 -> 0.5) and leaves the metric just as name-dependent. Evaluating
/// every goal is what actually dissolved it.
#[test]
fn theorem_goal_completion_rate_is_name_invariant_under_a_priority_tie() {
    // Two goals, identical priority, statuses driven by identically-shaped
    // evidence. Only the names differ in sort order.
    let trace: ResearchTrace = serde_json::from_value(serde_json::json!({
        "dimension": 4,
        "events": [
            { "cycle": 1, "source": "ix", "payload": {
                "type": "goal_update", "key": "aaa-goal",
                "description": "first goal", "priority": 0.9, "status": "Unknown" } },
            { "cycle": 1, "source": "ix", "payload": {
                "type": "goal_update", "key": "zzz-goal",
                "description": "second goal", "priority": 0.9, "status": "Unknown" } },
            { "cycle": 2, "source": "ix", "payload": {
                "type": "belief_update", "proposition": "aaa-goal",
                "value": "True", "evidence": {} } },
            { "cycle": 3, "source": "ix", "payload": {
                "type": "belief_update", "proposition": "zzz-goal",
                "value": "True", "evidence": {} } }
        ]
    }))
    .expect("counterexample deserializes");

    assert!(
        has_tied_goal_priorities(&trace),
        "the counterexample must actually contain a priority tie"
    );

    let before = scalar_metrics(trace.clone())["goal_completion_rate"]
        .as_f64()
        .expect("numeric");
    let after = scalar_metrics(invert_proposition_order(&trace))["goal_completion_rate"]
        .as_f64()
        .expect("numeric");

    assert_eq!(
        before, after,
        "goal_completion_rate depends on goal names again under a priority tie \
         ({before} vs {after}). The bulk goal-status refresh is what makes the tie-break \
         harmless — if status has gone back to being written for top_goal only, this \
         regresses to the known violation it replaced."
    );
    // Both goals reach belief True, so both must be credited regardless of which
    // one won the tie. A rate below 1.0 means a goal was starved.
    assert!(
        (before - 1.0).abs() < 1e-12,
        "expected both tied goals credited (1.0), got {before} — a goal is still being starved"
    );
}

/// Read a fixture's per-goal final status alongside the belief for the same key.
fn goal_status_vs_belief(path: &str) -> Vec<(String, String, String)> {
    let trace = load_trace(path);
    let report = CognitiveLoop::new(trace.dimension).process_research_trace(trace);
    let value = serde_json::to_value(&report).expect("report serializes");
    let goals = value
        .get("final_goals")
        .and_then(|g| g.as_object())
        .expect("report carries final_goals");
    let beliefs = value
        .get("final_beliefs")
        .and_then(|b| b.as_object())
        .expect("report carries final_beliefs");

    goals
        .iter()
        .map(|(key, goal)| {
            (
                key.clone(),
                goal.get("status")
                    .and_then(|s| s.as_str())
                    .unwrap_or("?")
                    .to_string(),
                beliefs
                    .get(key)
                    .and_then(|b| b.as_str())
                    .unwrap_or("<none>")
                    .to_string(),
            )
        })
        .collect()
}

/// **Was a known violation; now a theorem — goal starvation is fixed.**
///
/// `CognitiveState::top_goal` filters out only goals whose status is already
/// `True`, so *any* goal that never reaches `True` holds the top slot
/// indefinitely. Status used to be written for `top_goal` alone, so every
/// lower-priority goal was blocked for as long as that lasted — its evidence
/// could arrive, be perceived, and move its belief while its goal status was
/// never read. On `long_recovery`, `gamma-method-correct` held the top slot zero
/// times across 22 events and ended `Unknown` with belief `Probable`; the metric
/// reported 0.667 where every goal's belief was True-or-Probable.
///
/// The bulk status refresh evaluates every goal each cycle, so the rate is now
/// 1.0. Action emission is still top-goal-only — one attention target per cycle —
/// which is why the emitted action sequence is unchanged.
///
/// Note this fix is *not* what makes the metric comparable across arms. See
/// `known_violation_goal_status_is_never_revised_downward`, still live: the write
/// remains upgrade-only, so staleness survives and §5.4 still excludes the metric.
#[test]
fn theorem_every_goal_is_evaluated_not_only_the_top() {
    let rows = goal_status_vs_belief("../../fixtures/ix/long_recovery.json");

    let (_, status, belief) = rows
        .iter()
        .find(|(key, _, _)| key == "gamma-method-correct")
        .expect("long_recovery declares gamma-method-correct");

    assert_eq!(
        (status.as_str(), belief.as_str()),
        ("Probable", "Probable"),
        "gamma-method-correct is starved again (status {status}, belief {belief}) — goal status \
         is being written for top_goal only, regressing the fix this theorem replaced."
    );

    // Every goal's belief is True or Probable, so an honest completion rate is 1.0.
    assert!(
        rows.iter()
            .all(|(_, _, belief)| belief == "True" || belief == "Probable"),
        "fixture changed: not every long_recovery goal ends True/Probable"
    );
    let trace = load_trace("../../fixtures/ix/long_recovery.json");
    let rate = scalar_metrics(trace)["goal_completion_rate"]
        .as_f64()
        .expect("numeric");
    assert!(
        (rate - 1.0).abs() < 1e-12,
        "goal_completion_rate is {rate}; expected 1.0 now that every goal is evaluated (it read \
         0.667 while gamma-method-correct was starved)"
    );
}

/// **Known violation — stale goal status.** The THINK block
/// (the bulk status refresh) writes `goal.status` *only* in the `True | Probable`
/// arm. `Contradictory` escalates without touching status, `Unknown`
/// investigates, and `Doubtful`/`False` fall through to `_ => {}`. Status is
/// therefore never revised **downward**: a goal credited as achieved keeps that
/// credit after its own evidence collapses.
///
/// `cognition_divergence`: `alpha-prompt-helps` is written `Probable` early, its
/// belief goes `Contradictory`, and the status is never revised — so the default
/// arm reports `goal_completion_rate` 0.5 while `SubjectiveLogic`, whose write is
/// unconditional and two-sided, reports 0.0 on the same trace.
///
/// This is the second of the two artifacts behind §5.4's decision not to
/// reinstate `goal_completion_rate`, and it pushes in the opposite direction to
/// starvation: staleness inflates the hexavalent arms, starvation deflates them.
#[test]
fn known_violation_goal_status_is_never_revised_downward() {
    let rows = goal_status_vs_belief("../../fixtures/ix/cognition_divergence.json");

    let (_, status, belief) = rows
        .iter()
        .find(|(key, _, _)| key == "alpha-prompt-helps")
        .expect("cognition_divergence declares alpha-prompt-helps");

    assert_eq!(
        (status.as_str(), belief.as_str()),
        ("Probable", "Contradictory"),
        "alpha-prompt-helps no longer keeps stale credit (status {status}, belief {belief}). \
         If the THINK block now revises status downward, delete this known_violation and \
         update §5.4 — the goal_completion_rate decomposition there depends on it."
    );

    // The metric counts it as achieved despite the arm's own belief contradicting it.
    let trace = load_trace("../../fixtures/ix/cognition_divergence.json");
    let rate = scalar_metrics(trace)["goal_completion_rate"]
        .as_f64()
        .expect("numeric");
    assert!(
        (rate - 0.5).abs() < 1e-12,
        "goal_completion_rate is {rate}; expected 0.5, half of it stale credit"
    );
}

/// An achieved goal is never un-achieved by a later weaker belief.
///
/// # Why this exists
///
/// `79ad578` added the bulk status refresh and described it — in the commit
/// message, in the code comment, and in §5.4's third amendment — as
/// "upgrade-only". It was not. The `True | Probable` filter selects which
/// *beliefs* qualify to be written; the write itself was unconditional, so a
/// goal sitting at `True` was overwritten with `Probable` the moment its belief
/// softened. Because `top_goal` evicts only on `status == True`, that
/// un-achieved goal was re-admitted to top-goal candidacy, displaced the real
/// top goal, and swallowed its actions.
///
/// The same commit claimed the emitted action sequences were "byte-identical".
/// They were not: `heavy_contradiction` lost three `Escalate`s (13 → 10) and
/// `long_recovery` lost two (8 → 6), in **both** hexavalent arms. The claim was
/// checked against per-fixture *wait* counts, which did not move — an
/// inadequate check that this test replaces with the property itself.
///
/// No test in the suite caught it. The full suite passed on the defect, which
/// is why this pins the mechanism directly rather than a fixture's numbers.
#[test]
fn theorem_an_achieved_goal_is_not_un_achieved_by_a_softer_belief() {
    use hari_core::{CognitiveLoop, ResearchEvent, ResearchEventPayload};
    use hari_lattice::HexValue;

    let mut loop_ = CognitiveLoop::new(4);
    let key = "gamma-goal";
    let ev = |cycle: u64, payload: ResearchEventPayload| ResearchEvent {
        cycle,
        source: "ix-agent-a".to_string(),
        payload,
    };

    loop_.process_research_event(ev(
        1,
        ResearchEventPayload::GoalUpdate {
            key: key.to_string(),
            description: "a goal that gets achieved".to_string(),
            priority: 0.9,
            status: None,
        },
    ));
    // Drive the belief to True from two distinct sources — the projection
    // downgrades a single-source True to Probable (lib.rs, >=2 distinct
    // sources), and this test is about status, not about that rule.
    for (cycle, source) in [(2u64, "ix-agent-a"), (3, "ix-agent-b")] {
        loop_.process_research_event(ResearchEvent {
            cycle,
            source: source.to_string(),
            payload: ResearchEventPayload::BeliefUpdate {
                proposition: key.to_string(),
                value: HexValue::True,
                evidence: Default::default(),
            },
        });
    }
    let achieved = loop_.state.goals.get(key).expect("goal exists").status;
    assert_eq!(
        achieved,
        HexValue::True,
        "precondition failed: the goal never reached True, so this test would \
         pass vacuously on any implementation"
    );

    // Now soften the belief. Retraction resets it, then a Probable assertion
    // lands — the exact shape that un-achieved the goal between 79ad578 and
    // the fix.
    loop_.process_research_event(ev(
        4,
        ResearchEventPayload::Retraction {
            proposition: key.to_string(),
            reason: "supporting run withdrawn".to_string(),
            retracts: None,
        },
    ));
    loop_.process_research_event(ev(
        5,
        ResearchEventPayload::BeliefUpdate {
            proposition: key.to_string(),
            value: HexValue::Probable,
            evidence: Default::default(),
        },
    ));

    let after = loop_.state.goals.get(key).expect("goal exists").status;
    assert_eq!(
        after,
        HexValue::True,
        "an achieved goal was un-achieved by a softer belief (status {after:?}). \
         That re-admits it to top_goal candidacy, where it displaces the real \
         top goal and swallows its actions."
    );
}
