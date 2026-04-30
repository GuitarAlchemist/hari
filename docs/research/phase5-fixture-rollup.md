# Phase 5 Fixture Rollup

**Date**: 2026-04-29 (SL baseline added 2026-04-29)
**Scope**: Five `fixtures/ix/*.json` traces (`long_recovery` plus four newly authored)
replayed through `cargo run --release -p hari-core --quiet -- replay --compare`.
Captured: `comparison.{baseline,experimental}.{contradiction_recovery_cycles,
false_acceptance_count, goal_completion_rate, consensus_stability,
attention_norm_max, action_counts_by_kind}` and
`comparison.action_divergence | length`.

**SL baseline addendum (2026-04-29)**: A minimum-viable Subjective Logic
implementation (Jøsang 2016, binary opinions + cumulative fusion) was added as
the prior-art comparator the survey called for. All six fixtures
(the five from §2 plus `cognition_divergence.json`) were re-run through
`cargo run --release -p hari-core --quiet -- replay --compare3`. The new SL
column and 3-way verdict are in §7 below; the original §1–§6 are unchanged
except for the headline call-out.

## 1. TL;DR

The Lie path's 1-event `false_acceptance_count` advantage on `long_recovery.json`
**holds and arguably strengthens** across the four new fixtures, but the win is
narrow and comes packaged with a meaningful side-effect (over-aggressive
suppression on confident steady-stream evidence — "Wait-shaming"). Of the five
fixtures with a defined `false_acceptance_count` reading, Lie wins three (one
fewer false accept on `long_recovery`, two on `heavy_contradiction`, one on
`racing_goals`) and ties two (`slow_evidence` at 0/0, `swarm_dissent` at 1/1).
Lie never **loses** the false-acceptance metric. The other four metrics
(`contradiction_recovery_cycles`, `goal_completion_rate`, `consensus_stability`,
`attention_norm_max`) tie or are uninformative on every fixture except
`swarm_dissent`, where Lie populates `contradiction_recovery_cycles=3` while
RecencyDecay reports `None` — but that is a metric-machinery artifact (Lie
suppresses the Escalate-with-"contradictory" actions to `Wait`, so the
first-contradictory detector latches on a later event), not a substantive recovery
win.

**Verdict**: **holds, narrowly**. Lie reduces false-acceptance count on three of
five fixtures and never increases it. But on the steady-stream fixture
(`slow_evidence`) it suppresses three confident `Accept`s to `Wait` — a
quantitatively visible Wait-shaming cost that ties the false-acceptance metric
only because the underlying claim was correct. Phase 6 should not adopt Lie as
default on the strength of this result alone; the win is consistent with the 22-
event finding but the supporting fixtures are still author-designed and the cost
side is real.

## 2. Per-fixture metrics table

All fixtures: dimension=4, lie_alpha=2.0, lie_dt=0.5, baseline = `RecencyDecay`,
experimental = `Lie`. Lower is better for `false_acceptance_count` and
`contradiction_recovery_cycles`; higher is better for `goal_completion_rate` and
`consensus_stability`; `attention_norm_max` is bounded-only (must stay < 10).

| Fixture | Events | `recovery_cycles` (B/E) | `false_accept` (B/E) | `goal_completion` (B/E) | `consensus` (B/E) | `attn_norm_max` (B/E) | divergences |
|---|---:|---:|---:|---:|---:|---:|---:|
| `long_recovery` | 22 | 5 / 5 | **4 / 3** | 0.667 / 0.667 | 0.340 / 0.340 | 0.0 / 2.747 | 2 |
| `heavy_contradiction` | 17 | 6 / 6 | **4 / 2** | 0.333 / 0.333 | 0.389 / 0.389 | 0.0 / 3.390 | 2 |
| `slow_evidence` | 19 | None / None | 0 / 0 | 1.000 / 1.000 | 0.821 / 0.821 | 0.0 / 8.633 | 3 |
| `racing_goals` | 21 | None / None | **1 / 0** | 0.400 / 0.400 | 0.760 / 0.760 | 0.0 / 6.950 | 6 |
| `swarm_dissent` | 22 | None / **3** † | 1 / 1 | 0.667 / 0.667 | 0.872 / 0.872 | 0.0 / 2.499 | 6 |

† `swarm_dissent` `recovery_cycles` divergence is a metric-side artifact, not a
substantive Lie advantage. See §4.

Action-count totals (Accept / Escalate / Retry / Wait — Logs omitted):

| Fixture | Baseline (Accept/Escalate/Retry/Wait) | Experimental (Accept/Escalate/Retry/Wait) |
|---|---|---|
| `long_recovery` | 11 / 8 / 3 / 4 | 10 / 8 / 2 / 6 |
| `heavy_contradiction` | 6 / 13 / 1 / 3 | 4 / 13 / 1 / 5 |
| `slow_evidence` | 16 / 0 / 0 / 3 | 13 / 0 / 0 / 6 |
| `racing_goals` | 12 / 1 / 0 / 8 | 6 / 1 / 0 / 14 |
| `swarm_dissent` | 7 / 27 / 0 / 3 | 7 / 19 / 0 / 7 |

Lie consistently emits more `Wait`s and fewer `Accept`s. On `racing_goals` the
gap is dramatic: half the baseline's Accepts are replaced with Waits.

## 3. Win/loss tally per metric

Metric-by-metric scoreboard across the five fixtures (Lie = experimental,
RecencyDecay = baseline). Treats `None` matches as ties; ignores
`swarm_dissent` recovery as artifact.

| Metric | Lie wins | Baseline wins | Ties | Uninformative |
|---|---:|---:|---:|---:|
| `false_acceptance_count` | **3** | 0 | 2 | 0 |
| `contradiction_recovery_cycles` | 0 | 0 | 2 | 3 |
| `goal_completion_rate` | 0 | 0 | 5 | 0 |
| `consensus_stability` | 0 | 0 | 5 | 0 |
| `attention_norm_max` (bounded only) | n/a | n/a | n/a | bounded ✓ |

So the false-acceptance advantage is the **only metric on which the two paths
ever differ across the five fixtures**. Goal completion, consensus stability,
and contradiction recovery never differ in a way that's explained by the
algebra (the `swarm_dissent` recovery delta is a side-effect of Lie suppressing
the Escalate-with-"contradictory" emission to `Wait`).

## 4. Patterns observed

**Where Lie reliably wins.** Fixtures with multiple competing goals where one
goal accumulates several cycles of strong positive evidence (axis-0 dominance)
*before* Doubtful/False perceptions arrive on a different goal's axis. In that
shape — the "starved-axis" pattern — the Lie rotation generator with negative
coefficient `α * h_eff[k] = -2` rotates attention strongly into `proj < -0.45`
on the axis being penalised. The multiplicative score `1 + α * proj` drops
below `θ_wait = 0.1` and the substantive `Accept(prop, Doubtful)` action is
suppressed to `Wait`. This is exactly what avoids a future-retracted `Accept`
becoming a `false_acceptance_count++` later.

`heavy_contradiction`, `racing_goals`, and `long_recovery` all have this shape
(top-priority goal evidenced first across cycles 4–6, then negative perceptions
on lower-axis goals at cycles 6–9). All three show Lie reducing
`false_acceptance_count` by 1–2.

**Where Lie reliably loses (or ties for a bad reason).** Single-proposition
steady-stream fixtures and fixtures dominated by `agent_vote` payloads.

* `slow_evidence` is a 19-event fixture with one substantive proposition
  (`iota-summary-faithful`) accumulating 15 confirming Probable/True votes from
  diverse benchmarks. Lie suppresses 3 of those Accepts to Wait. The
  baseline accepts all 16. False-acceptance is 0/0 only because the proposition
  *should* be accepted — there is no future retraction to penalise the
  baseline. If the proposition had later been retracted, Lie would have looked
  good; in this fixture it just looks paranoid. This is the **Wait-shaming
  failure mode** the fixture was authored to surface, and it surfaced cleanly.
* `swarm_dissent` is dominated by `agent_vote` payloads on a single
  proposition. Lie suppresses 4 Escalate-with-"contradictory" emissions to
  `Wait`, which (a) loses information for downstream consumers, (b) artificially
  shifts `contradiction_recovery_cycles` from `None` to 3 because the
  first-contradictory detector now latches on a later event. The substantive
  metrics (`false_acceptance_count`, `consensus_stability`,
  `goal_completion_rate`) are tied, so this is best read as Lie costing Escalate
  signals without buying anything measurable.

**Fixture-shape pattern that consistently breaks Lie**: heavy positive evidence
on a *single* axis with no competing goal pressure. Attention rotates so far in
the positive direction on that axis that the projection generator's coefficient
saturates and the scaling generator pumps the non-target axes to high norm
(`slow_evidence` reached 8.63, the closest any fixture got to the 10.0 cap).
That high norm doesn't suppress the on-target Accept, but it does push *future
perceptions on the same axis* into the negative-rotation regime once any single
Probable arrives, and Lie then suppresses what would otherwise have been a
clean confirmatory Accept. Wait-shaming is the natural consequence of the
multiplicative scoring rule combined with strong attention rotation.

**Pattern that is invisible to both paths**: `consensus_stability` and
`goal_completion_rate` are identical on every fixture. Both metrics are
computed from the *belief network* (which the Lie/RecencyDecay scoring layer
doesn't touch) and the *final goal status*, which both paths receive
identically because `goal_update` events are not action-routed. Until the
algebra feeds back into belief integration or goal resolution, these two
metrics will stay tied by construction.

## 5. What this means for Phase 5's verdict

The Phase 5 verdict was "**Continue with caveats** — the only measurable Lie
advantage is one fewer false acceptance on a 22-event fixture, well within
statistical noise". After four additional fixtures, that verdict is
**slightly strengthened, with a real cost surfaced**:

* The false-acceptance advantage **does generalise** across multi-goal
  fixtures with the starved-axis shape: 3 wins, 2 ties (one tie due to the
  ground claim being correct, the other a single-event noise tie), 0 losses
  out of 5. That is suggestive but not conclusive.
* The advantage is **mechanism-explainable**: the same negative-rotation
  dynamic that suppresses the long_recovery cycle-6 Doubtful suppresses the
  heavy_contradiction cycle-6 Doubtful and the racing_goals cycle-8 Doubtful.
  The fixture-shape that triggers it is well-defined.
* The cost is **also mechanism-explainable**: on single-axis dominated
  evidence streams the same dynamic suppresses confirmatory Accepts.
  `slow_evidence` makes this concrete (3 lost Accepts on a 16-Accept stream).
* Phase 6 should **NOT** ship with `Lie` as default on the basis of a 3/5 win
  rate on author-designed fixtures with a known cost mode. The plan-of-record
  ("continue with caveats; tighten coupling before Phase 6") still holds. The
  recommended Phase 6 next steps:
  1. Soften the multiplicative rule to a clamped form (e.g.
     `max(0.1, 1 + α * proj)`) so Wait-shaming is bounded.
  2. Action-specific `base` weighted by goal priority, so the Lie path can
     differentiate confirmatory Accepts (high-priority goal, positive
     evidence) from cautious Accepts (low-priority goal, doubtful evidence).
  3. Run all five fixtures plus a SL baseline before any default switch.
  4. Author at least 2 more fixtures from *real IX traces* once the
     integration is wired up, so the result is not entirely dependent on
     synthetic data.

## 6. Caveats

* **Small N**: 5 fixtures, ~20 events each, totalling 101 events. A 3/5 win
  rate on `false_acceptance_count` is consistent with a real effect AND with
  noise; a binomial test on win-vs-tie has no power at this sample size.
* **Author-designed**: all five fixtures are hand-written. They were not
  hand-tuned per fixture to produce a Lie win, but the *shape* (multi-goal,
  axis-0 dominant, negative perception on a starved axis) is exactly the
  shape long_recovery has — so the four new fixtures sample from a region of
  trace-space where Lie was already known to win. The single-axis
  steady-stream fixture (`slow_evidence`) was deliberately chosen as a
  known-disadvantageous shape, and Lie does cost there. A truly diverse
  natural distribution of IX traces is not represented.
* **No SL baseline**: Subjective Logic was not run against any of these
  fixtures. The comparison is `Lie` vs `RecencyDecay` only, as in §6 of the
  Phase 5 results doc. The Phase 5 plan flags SL as a future direction;
  nothing in this rollup changes that.
* **Wait-shaming is a real cost on `slow_evidence`** (3 missed Accepts on a
  16-Accept stream). The current `false_acceptance_count` metric does not
  count missed-Accept-on-correct-claim as a cost. A `false_rejection_count`
  metric would make Lie's cost visible alongside its benefit; not adding it
  in this loop because the schema change is out of scope, but flagging.
* **Metric-side artifact in `swarm_dissent`**:
  `contradiction_recovery_cycles` differs (None vs 3) because Lie suppresses
  the Escalate-with-"contradictory" actions to `Wait`. The
  recovery-detection pass keys off those Escalates, so suppressing them
  shifts which event is logged as "first contradictory". This is a
  side-effect of the Lie filter, not a substantive Lie improvement on
  contradiction recovery. The metric machinery is doing what it was written
  to do; the interpretation just needs care.
* **Hard rule #6 respected**: `lie_alpha = 2.0`, `lie_dt = 0.5` defaults are
  unchanged. All measurements above use the pinned defaults.

## 7. Subjective Logic baseline (added 2026-04-29)

The prior-art survey (`docs/research/prior-art-survey.md` §4) identified
Jøsang's Subjective Logic as the most relevant comparator and explicitly
flagged its absence as the largest reinvention risk. A minimum-viable
implementation now lives in `crates/hari-core/src/subjective_logic.rs` and is
exposed via `replay --compare3 <trace>`. This section reports the 3-way
results and the resulting verdict.

### 7.1 Mapping choices

**`HexValue → Opinion(b, d, u, a)`** with `a = 0.5` (no prior bias):

| HexValue | b | d | u | Justification |
|---|---:|---:|---:|---|
| True | 0.85 | 0.05 | 0.10 | High belief, residual `u` so cumulative fusion stays well-defined. |
| Probable | 0.55 | 0.15 | 0.30 | Moderate commitment with real residual uncertainty. |
| Unknown | 0.05 | 0.05 | 0.90 | Near-vacuous; tiny non-zero `b`/`d` keeps the fusion denominator > 0. |
| Doubtful | 0.15 | 0.55 | 0.30 | Mirror of Probable. |
| False | 0.05 | 0.85 | 0.10 | Mirror of True. |
| Contradictory | 0.45 | 0.45 | 0.10 | SL has no native "Contradictory"; balanced `b`/`d` with low `u` is the standard rendering of irreconcilable evidence. |

**Fusion operator**: cumulative fusion (`⊕`) per Jøsang §12.3, with
dogmatic-pair (both `u = 0`) handled by mass-averaging and one-side-dogmatic
returning the dogmatic side. Commutative (verified by unit test);
associative only approximately, which is documented in the module header.

**`agent_vote` discounting**: `agent_vote` payloads are mixed with
`vacuous(a)` at weight 0.5 before fusing (so an agent vote contributes
~half the strength of a direct `belief_update`/`experiment_result`).
Justification: agent votes are weaker evidence than direct experiments;
this is the simplest possible substitute for proper trust transitivity,
which is out of scope.

**`Opinion → Action`** thresholds (`SubjectiveLogicConfig::default()`):

| Condition | Action | Notes |
|---|---|---|
| `b > 0.7` and `b ≥ 0.85` | `Accept(prop, True)` | Strong positive evidence. |
| `b > 0.7` and `b < 0.85` | `Accept(prop, Probable)` | Moderate positive. |
| `d > 0.7` and `d ≥ 0.85` | `Accept(prop, False)` | Mirror. |
| `d > 0.7` and `d < 0.85` | `Accept(prop, Doubtful)` | Mirror. |
| `b > 0.4` AND `d > 0.4` | `Escalate{reason=…contradictory…}` | SL conflict — emitted with the literal substring "contradictory" so `compute_metrics_for`'s contradiction-detection pass picks it up uniformly. |
| `u > 0.7` | `Investigate{topic}` | High residual uncertainty. |
| otherwise | `Wait` | |

**Goal status**: where a `goal_update` key matches a touched proposition,
the goal's final status is mapped from the running opinion's projected
probability (`P ≥ 0.55` → Probable, `P ≥ 0.85` → True, etc.). This was
deliberately included so `goal_completion_rate` is non-trivially populated;
the side-effect is that SL inflates `goal_completion_rate` on
`long_recovery` and `heavy_contradiction` to 1.0 because every touched
proposition's running opinion eventually projects above 0.55. See §7.4
honest caveat.

### 7.2 Per-fixture metrics table — 3-way

All measurements: dimension=4 (matches each fixture's header), default SL
config (`belief_accept_threshold=0.7`, `conflict_threshold=0.4`,
`uncertainty_investigate_threshold=0.7`, `default_base_rate=0.5`).
Lower is better for `false_accept` and `recovery_cycles`; higher is
better for `goal_completion` and `consensus`.

| Fixture | `recov_cycles` (D/L/SL) | `false_accept` (D/L/SL) | `goal_comp` (D/L/SL) | `consensus` (D/L/SL) | 3-way divs |
|---|---:|---:|---:|---:|---:|
| `long_recovery` | 5 / 5 / **4** | 4 / 3 / **1** | 0.667 / 0.667 / 1.000* | 0.340 / 0.340 / 0.340 | 16 |
| `heavy_contradiction` | 6 / 6 / None† | 4 / **2** / **2** | 0.333 / 0.333 / 1.000* | 0.389 / 0.389 / 0.389 | 13 |
| `slow_evidence` | None / None / None | 0 / 0 / 0 | 1.000 / 1.000 / 1.000 | 0.821 / 0.821 / 0.821 | 9 |
| `racing_goals` | None / None / None | 1 / **0** / **0** | 0.400 / 0.400 / 0.200‡ | 0.760 / 0.760 / 0.760 | 16 |
| `swarm_dissent` | None / 3§ / None | 1 / 1 / **0** | 0.667 / 0.667 / 0.333‡ | 0.872 / 0.872 / 0.872 | 19 |
| `cognition_divergence` | None / None / None | 1 / 1 / **0** | 0.500 / 0.500 / 0.000‡ | 0.500 / 0.500 / 0.500 | 6 |

D = `RecencyDecay`, L = `Lie`, SL = `SubjectiveLogic`.

\* SL inflates `goal_completion_rate` on the recovery fixtures because its
opinion-to-status projection treats any non-vacuous opinion above
`P ≥ 0.55` as `Probable`. Mechanism, not a substantive consensus advantage.

† `heavy_contradiction` 3-way: SL never trips the `b > 0.4 AND d > 0.4`
conflict band on the relevant proposition because cumulative fusion
collapses contradictory evidence into a low-uncertainty mid-`b`/mid-`d`
opinion that doesn't simultaneously cross both 0.4 thresholds. Without an
Escalate-with-"contradictory" emission the metric pre-pass never marks
"first contradictory", so recovery is undefined. Same machinery artifact
class as `swarm_dissent` for Lie.

‡ SL's `goal_completion_rate` _decreases_ on three fixtures
(`racing_goals`, `swarm_dissent`, `cognition_divergence`). Mechanism:
when SL is dominated by `agent_vote` events at half-strength, the running
opinion never projects above 0.55 and the goal's final status stays at
`Unknown`/`Doubtful`. This is a real metric — SL is more conservative
about declaring goals "achieved" — but it cuts both ways depending on
fixture shape.

§ `swarm_dissent` Lie recovery is a Lie-side metric artifact (see §6),
not a Lie advantage.

Action-count totals (Accept / Escalate / Retry / Wait — Logs omitted):

| Fixture | RecencyDecay | Lie | SubjectiveLogic |
|---|---|---|---|
| `long_recovery` | 11 / 8 / 3 / 4 | 10 / 8 / 2 / 6 | **3 / 3 / 3 / 9** |
| `heavy_contradiction` | 6 / 13 / 1 / 3 | 4 / 13 / 1 / 5 | **6 / 0 / 1 / 6** |
| `slow_evidence` | 16 / 0 / 0 / 3 | 13 / 0 / 0 / 6 | **13 / 0 / 0 / 3** |
| `racing_goals` | 12 / 1 / 0 / 8 | 6 / 1 / 0 / 14 | **5 / 1 / 0 / 7** |
| `swarm_dissent` | 7 / 27 / 0 / 3 | 7 / 19 / 0 / 7 | **3 / 0 / 0 / 16** |
| `cognition_divergence` | 2 / 2 / 0 / 2 | 1 / 2 / 0 / 4 | **0 / 1 / 0 / 3** |

SL is the most-`Wait`, least-`Accept` of the three on every multi-event
fixture. On `swarm_dissent` it emits zero Escalates where Lie/Decay emit
19/27 — SL's high `u` band suppresses both the Escalate and the Accept
branches on dissent-heavy `agent_vote` streams, falling through to
`Wait` and `Investigate`. On `heavy_contradiction` SL emits zero
Escalates because cumulative fusion smooths the True+False evidence
toward a balanced low-`u` opinion that doesn't cross the dual-0.4 band.

### 7.3 3-way win/loss tally per metric

Treats `None` matches as ties; ignores fixture cells where neither model
is informative. "Wins" requires strict improvement on the metric.

| Metric | Lie wins | SL wins | Decay wins | Lie==SL | All tie | Uninformative |
|---|---:|---:|---:|---:|---:|---:|
| `false_acceptance_count` | 0 | **3** | 0 | 2 | 1 | 0 |
| `contradiction_recovery_cycles` | 0 | 1 (long_recovery) | 0 | 0 | 1 (artifact) | 4 |
| `goal_completion_rate` | 0 | 2 (artifact, see *) | 3 (vs SL: 0.4/0.667/0.5) | 4 | 0 | 0 |
| `consensus_stability` | 0 | 0 | 0 | 0 | 6 | 0 |

Reading this table:

- **`false_acceptance_count`**: SL **wins outright** vs the Lie path on
  3 fixtures (`long_recovery`: 1 vs 3; `racing_goals`: 0 = 0 tied;
  `swarm_dissent`: 0 vs 1; `cognition_divergence`: 0 vs 1). On
  `heavy_contradiction` SL ties Lie at 2 (both beat Decay's 4). On
  `slow_evidence` everyone is at 0. **SL is never beaten by Lie on this
  metric across the six fixtures.** SL's headline result over
  `RecencyDecay` is 5 wins (long_recovery 1 vs 4, heavy_contradiction
  2 vs 4, racing_goals 0 vs 1, swarm_dissent 0 vs 1, cognition_divergence
  0 vs 1) and 1 tie (slow_evidence at 0/0).

- **`contradiction_recovery_cycles`**: SL improves by 1 cycle on
  `long_recovery` (4 vs 5). Loses by metric-machinery non-emission on
  `heavy_contradiction`. Uninformative on the other four.

- **`goal_completion_rate`**: SL diverges from the others — both up
  (long_recovery, heavy_contradiction, both via the projection-to-status
  rule that flatters multi-goal recovery fixtures) and down
  (racing_goals 0.200, swarm_dissent 0.333, cognition_divergence 0.000).
  This metric is **not currently a fair comparator** for SL; the
  opinion-to-goal-status mapping is doing real work that is
  fixture-specific in either direction.

- **`consensus_stability`**: Identical on all six fixtures. Same
  reason as the Lie/Decay parity — the metric reads from event payload
  values, which are upstream of any of the three pipelines.

### 7.4 Verdict

**SL is competitive with Lie on the headline metric and dominant over
RecencyDecay.** Specifically:

- On `false_acceptance_count` — the only metric on which Lie measurably
  beat RecencyDecay in the original 5-fixture rollup — **SL ties or
  beats Lie on all six fixtures, and beats RecencyDecay on five of six**.
  SL's `long_recovery` reading (1 false acceptance) is **strictly better
  than Lie's 3** (which was already the headline Lie win over Decay's 4).
- On `contradiction_recovery_cycles` SL improves on Lie's `long_recovery`
  reading (4 vs 5). This is the first time *any* of the three paths has
  shown a substantive recovery-cycles advantage on a fixture that
  populates the metric.
- SL pays for these wins by being **substantially more conservative**:
  3 Accepts on `long_recovery` vs Lie's 10 vs Decay's 11. On
  `slow_evidence` SL ties Lie at 13 Accepts (vs Decay's 16) — i.e., SL
  also pays the Wait-shaming cost on confident steady-stream evidence,
  but does not pay extra on top of Lie. On `heavy_contradiction` SL
  emits **zero Escalates** where Decay emits 13, which is a real loss
  of signal to downstream consumers even when the false-acceptance
  metric agrees.

**Mapping crudeness honestly hurts SL too.** The minimum-viable choices
in §7.1 leave SL worse on `goal_completion_rate` (3 fixtures) and
worse on `Escalate` emission (`heavy_contradiction`, `swarm_dissent`).
A production-quality SL implementation (see caveats below) would likely
recover most of these losses. We did not tune the thresholds to make SL
look better.

### 7.5 What this means for the survey's "Hari at risk of reinventing SL" concern

**The concern is now empirically supported, not abstract.** With the
data in §7.2:

- SL alone outperforms `RecencyDecay` on `false_acceptance_count` on
  five of six fixtures — the *exact metric* where the Lie path's
  measurable advantage was supposed to be Hari's signal.
- SL ties or beats the Lie path on that metric on all six fixtures, while
  being a much smaller, much better-understood implementation (no Lie
  algebra, no attention vector, no α/dt to pin, no Hamiltonian seeding,
  no boundedness contract — just `(b, d, u, a)` per proposition with
  Jøsang's published fusion formula).
- The Lie path's "first measurable advantage" finding from
  `phase5-results.md` §7b is **not an advantage over a credible
  prior-art baseline**. It is an advantage over a strawman (recency
  decay with no logic).

This is the headline finding of the SL baseline. **The Phase 5 verdict
"Continue with caveats" is now significantly weaker.** The honest read:
the 22-event `long_recovery` fixture's 1-event Lie win, after this
addendum, looks like the algebra reproducing what cumulative-fusion
already gets you for free. Phase 6 needs to confront this directly —
either the Lie path needs a metric on which it beats SL (not just
RecencyDecay), or `hari-cognition`'s Lie-algebra layer needs a different
justification than "measurable advantage on false_acceptance_count".

### 7.6 Caveats — what minimum-viable SL leaves out

This is a deliberately small SL implementation. A production deployment
would need:

1. **Trust transitivity / belief discounting**. We approximate
   `agent_vote` weakening with a constant 0.5 `Opinion::discounted`
   mix — Jøsang's framework has a proper trust-network discounting
   operator (`⊗`, see §6 of *Subjective Logic*). Implementing it
   would let the SL path differentiate trusted-source votes from
   anonymous ones, which the swarm fixtures should benefit from.
2. **Multinomial opinions**. Hari's `HexValue` has six values; we
   collapse to a binary opinion per proposition. The natural SL
   generalisation is multinomial opinions over the
   `{T, P, U, D, F, C}` simplex. That is two orders of magnitude more
   work and is probably the right shape if SL becomes the substrate.
3. **Consensus operator (`⊕̄`)**. Jøsang distinguishes
   sequential evidence (cumulative fusion, what we use) from
   simultaneous-from-independent-sources evidence (consensus). The
   IX trace does mix them. Skipping the consensus operator was an
   explicit scope cut.
4. **Base-rate handling beyond a single `default_base_rate = 0.5`**.
   Real SL implementations let the base rate evolve with
   subjective evidence about the meta-prior. We hold `a` constant
   per proposition.
5. **Conflict-coefficient detection**. Jøsang provides a formal
   `conflict(a, b)` measure beyond "both `b` and `d` are large".
   The threshold rule we use (`b > 0.4 AND d > 0.4`) is a
   convenient stand-in; the formal measure would catch more
   conflicts and miss fewer.
6. **Goal status from opinion**. The "project to status" rule used
   for `goal_completion_rate` is ad-hoc and is the cause of the
   metric's instability across fixtures (§7.2). A real
   implementation would either decline to populate goal status from
   SL opinions or use a stricter threshold (e.g. `P > 0.7` for
   Probable, `P > 0.9` for True) so the metric is comparable.

The survey's quote — "decades of formal results, conflict-aware fusion
operators, and explicit uncertainty mass — features Hari currently
lacks" — is fairly characterising how much our SL implementation
*does not* have. What we have is the formula skeleton, enough to make
the comparison honest.
