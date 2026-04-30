# Phase 5 Fixture Rollup

**Date**: 2026-04-29
**Scope**: Five `fixtures/ix/*.json` traces (`long_recovery` plus four newly authored)
replayed through `cargo run --release -p hari-core --quiet -- replay --compare`.
Captured: `comparison.{baseline,experimental}.{contradiction_recovery_cycles,
false_acceptance_count, goal_completion_rate, consensus_stability,
attention_norm_max, action_counts_by_kind}` and
`comparison.action_divergence | length`.

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
