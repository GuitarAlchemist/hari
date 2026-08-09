# The §6 aggregator, and §8 applied to it — the #35 report

**Status:** the tracer bullet is complete end to end except §9 item 4. **This is
not a §8 verdict**, and §4 below says exactly why it cannot be one.
**Pre-registration:** `2026-07-28-ix-eval-preregistration.md` (binding; committed),
amended at §9.8 by the branch carrying this report.
**Issue:** [#35](https://github.com/GuitarAlchemist/hari/issues/35)
**Date:** 2026-08-09 · **Author:** Stephane Pareilleux
**Repo state:** `claude/35-counterfactual-replay`, 376 tests passing.
**Supersedes nothing.** `2026-07-30-ix-eval-interim-assessment.md` stands; this
report adds the aggregator that document said could not run, and reaches the
same conclusion by the mechanical route rather than by assembling instruments.

---

## 0. What this is

#35's slice is a vertical tracer bullet: **task → pre-registration → paired
fixtures → replay boundary → paired metrics → bootstrap → one report.** Every
stage but one existed before this change. The missing one was the aggregator:
§6 pre-registered a trace-clustered paired bootstrap, §8 applies the kill/keep
rule *"mechanically to the bootstrap output"*, and a 2026-07-30 correction
recorded that **no bootstrap existed anywhere in the workspace**. §8 had nothing
to be applied to.

It exists now, and this report is what it says.

## 1. What landed

`paired_eval::bootstrap_paired_difference` — a pure function: per-trace decision
records in, interval and p out, no replay engine in the loop, which is the
property §6 asks for so it can be tested on hand-built inputs.

* **B = 10,000**, resampling **traces** and never decisions (§6).
* **Fixed seed `20260728`, a constant in the source and not settable from the
  command line.** §6 requires exact reproducibility; a tunable seed is also a
  shoppable one. The generator is a vendored SplitMix64 rather than a crate, so
  a dependency upgrade cannot move a published interval.
* Percentile 95% CI; two-sided achieved significance level
  `2·min(Pr[θ*≤0], Pr[θ*≥0])` with the standard `(count+1)/(B+1)` finite-`B`
  correction, so a p-value of exactly zero is not reportable.
* §7's realized ICC, design effect and effective *n* — with the caveat in §5.3
  below, which is a finding rather than a footnote.
* `apply_kill_keep` applies §8 mechanically, including §9.4's standing rule.

Two committed rules moved from prose into code:

| rule | enforced by |
|---|---|
| §9.4 — no §8 verdict on an authored corpus | `CorpusProvenance::Authored` → `WithheldByStandingRule` |
| §9.3.2 — the isolation and task corpora are never pooled | `pooling_violation`, which refuses the run |

Seventeen tests in `crates/hari-core/tests/paired_bootstrap.rs`. The load-bearing
one is `clustering_by_trace_refuses_the_significance_that_independence_would_manufacture`:
the same forty decisions, presented as four correlated traces, must **fail** the
dual rule that the same decisions presented as forty independent ones pass.
Mutating the implementation to resample decisions instead of traces fails that
test and one other, and nothing else — so it discriminates on exactly the
property §6 clusters for.

## 2. How to reproduce every number below

```bash
cargo build --release -p hari-core

# the task corpus (§9.3.2's amended operationalisation)
./target/release/hari-core replay --paired --compare3 --bootstrap \
  fixtures/ix/paired/{accuracy,cache,cost,latency,memory,throughput}-task.json

# the isolation corpus (§9.3.1 as originally authored) — never pooled with it
./target/release/hari-core replay --paired --compare3 --bootstrap \
  fixtures/ix/paired/{accuracy,cache,cost,latency,memory,throughput}-isolation.json
```

Both default to `--corpus authored`, which is the conservative reading: every
fixture committed in this repo is authored, and an authored corpus never yields
a verdict. `--corpus recorded` exists for the §9 item 4 driver and has no input
to consume today.

## 3. Results

### 3.1 §8 clause 1 — the shipped arm against the null baseline

`experimental` is the shipped `RecencyDecay` `CognitiveLoop`; `IX-unassisted` is
pass-through acceptance (both pinned in §9.5).

| | task corpus | isolation corpus |
|---|---|---|
| traces (clusters) | 6 | 6 |
| pairs | 54 | 54 |
| `RecencyDecay` Paired Accuracy | 0.3333 | 0.3333 |
| `IX-unassisted` Paired Accuracy | 0.3333 | 0.3333 |
| **difference** | **0.0000** | **0.0000** |
| 95% CI | [0.0000, 0.0000] | [0.0000, 0.0000] |
| p | 1.0000 | 1.0000 |
| dual rule | **fails** | **fails** |
| every decision agrees | **yes** | **yes** |

The interval is a point at zero not because 54 pairs are too few to resolve a
difference, but because **there is no difference at any decision**. All 10,000
resamples return exactly 0.0. `every_delta_is_zero` is the field that carries
this, and it is the distinction between "indistinguishable" and "identical".

### 3.2 The cheap baseline against the shipped arm

§4 names `SubjectiveLogic` the baseline the substrate must not lose to.

| | task corpus | isolation corpus |
|---|---|---|
| `SubjectiveLogic` | 0.6667 | 0.0000 |
| `RecencyDecay` | 0.3333 | 0.3333 |
| difference | **+0.3333** | **−0.3333** |
| 95% CI | [0.3333, 0.3333] | [−0.3333, −0.3333] |
| p | 0.0002 | 0.0002 |
| distinct cluster signatures | **1** | **1** |
| zero between-cluster variance | **yes** | **yes** |

Both intervals clear the §6 dual rule, and **neither may be read as a result.**
The six traces of each corpus are clones (§9.4), so every resample is the same
corpus and the interval has zero width by construction. The aggregator reports
the separation and the degeneracy from the same call, which is why they appear
in the same table.

The sign flip between the corpora is §9.3.3's finding, unchanged: SL wins the
task corpus because `True` clears its `b > 0.7` gate, and loses the isolation
corpus because it accepts nothing there at all. Pooling would average the two
into a wash; §9.3.2 forbids it and the tool now refuses.

## 4. §8, applied mechanically

Verbatim from `apply_kill_keep` on the task corpus:

```
clause1: fails      recency_decay vs ix_unassisted is 0.0000 (95% CI [0.0000, 0.0000],
                    p = 1.0000) — §6 dual rule fails — the two arms agree at every single
                    decision, so the difference is identically zero rather than merely
                    insignificant.
clause2: undefined  per-arm calibration requires each arm to emit forecasts from its own
                    posterior (§9 item 2), which does not exist.
verdict: withheld_by_standing_rule
```

Three things follow, in order of how much they constrain what can honestly be
said.

**KEEP is unreachable, and not narrowly.** §8 keeps the substrate only if both
clauses pass. Clause 1 fails by an identically-zero difference. Clause 2 cannot
pass because the instrument it reads does not exist, and an undefined clause is
not a passing one. Neither more traces nor a different seed touches either.

**The verdict is nevertheless withheld, not KILL.** §9.4's standing rule bars any
§8 verdict on an authored corpus, and this report does not suspend it. Calling
this a KILL would take the one liberty the pre-registration most explicitly
forbids — and would do so in the direction that flatters the author, since KILL
is the conclusion §9.6 and the interim assessment already point at.

**The distinction is thinner than it looks, and saying so is the honest part.**
The formal verdict awaits a driver-recorded corpus. But §9.6 pinned, over 400
generated traces stamped as a recorder would stamp them, that **no naturally
stamped trace separates the shipped arm from the null baseline** — the arm's
act/abstain decision is a pure function of `state.cycle - event.cycle`, and a
recorder stamps events on arrival. So the corpus item 4 would produce is one on
which clause 1 is zero for the same mechanical reason. The withholding is a
procedural obligation, not live uncertainty about the answer.

## 5. What the aggregator found that the instruments before it did not

### 5.1 The difference is identical, not insignificant

Every prior record of §8 clause 1 said "evaluates to exactly zero". That was
measured arm-for-arm. Through the aggregator it is now a property of the
estimator's output: `every_delta_is_zero`, CI `[0,0]`, p exactly 1.0. It closes
the reading that the zero was a small-sample artifact — with 54 paired decisions
and B = 10,000, a null result would normally leave that open. Here it does not.

### 5.2 Clustering, demonstrated rather than asserted

§6's reason for clustering was argued, never shown. It is now shown on a
hand-built input: forty decisions inside four traces fail the dual rule that the
same forty decisions treated as independent pass. The design choice buys exactly
what it was pre-registered to buy.

### 5.3 §7's ICC cannot see this corpus's degeneracy — a caveat that is itself a finding

§7 requires the report to state the realized ICC and effective *n*. Here they
are, task corpus, `SubjectiveLogic` vs the shipped arm:

| quantity | value |
|---|---|
| realized ICC | **0.0** |
| design effect | 1.0 |
| effective *n* | **54.0** |

Read naively that says clustering costs nothing and the corpus is worth its full
54 decisions. **It is not.** The ICC design-effect correction corrects for
*between*-trace correlation, and this corpus has none — its six traces are
identical, so there is no between-cluster variance to attribute. Its degeneracy
is replication *within* the design (§9.4: three distinct decision situations,
replicated eighteen times), and the ICC is structurally blind to it. A corpus of
clones therefore produces the *most* reassuring possible ICC.

The output carries an `effective_n_overstates` flag, set whenever between-cluster
variance is zero, pinned by a test. §7's instruction stands; the number it
produces on a degenerate corpus is not a measure of that corpus's information
content, and this is the shape of error §7's "the report must state the achieved
effective n" was written to prevent while inadvertently inviting.

## 6. Limits

1. **No §8 verdict is issued** (§4). Everything here is instrument
   characterisation on authored fixtures under §9.4's standing rule.
2. **The corpora are degenerate.** 54 pairs, 3 distinct decision situations.
   Every interval in §3.2 has zero width by construction.
3. **§8 clause 2 is untested, not passed.** Per-arm calibration does not exist.
   `ResearchReplayReport.calibration` exists and is scoped to touched beliefs,
   but all four arms derive the same touched-belief set from the same trace, so
   it cannot separate them (§9 item 2).
4. **The primary metric grades whether to act, never what is committed to.** The
   interim assessment's §5 point stands: nothing here grades whether the
   resulting belief state is a better representation of a contradictory,
   revisable, partially-retracted evidence base — which is what `hari-lattice`
   was built for. That is a different pre-registration, not a rescue for this one.
5. **The aggregator was built after clause 1 was measured at zero**, disclosed in
   §9.8. It formalises a zero rather than discovering one, which bounds but does
   not erase the concern.
6. **`Lie` is not an arm** (§4) and appears nowhere in the decision.

## 7. What remains of #35

**§9 item 4 — the paired driver in `clients/ix_reference` — is the only
outstanding critical-path item, and it is the whole of what remains.** It is what
supplies §6 a population, what makes the abstention and calibration measures
exercisable, and what converts §4's withheld verdict into a verdict.

It should not be built in the expectation of a different answer on the primary
metric (§9.6) or on `false_acceptance_count` (§9.7). Its value is that §8 cannot
be *answered* without it, not that the answer might change.

Also outstanding, and deliberately not attempted here: the per-arm forecast
emission hook (§9 item 2), whose two design constraints — resolution must not be
self-referential, and granularity is asymmetric between a continuous posterior
and a six-valued one — are recorded in §9 and must be settled before it is built.

## 8. Recommendation

Unchanged from the interim assessment, now reached mechanically rather than by
assembling instruments: **the pre-registered KEEP condition is unreachable on
every instrument that exists, and the substance of the pre-registered negative
result — ~600 lines of Subjective Logic already deliver the benefit on both
graded metrics; the additional substrate does not — is what the data supports.**

The formal §8 verdict stays withheld until item 4 records a corpus. Publishing
that negative result, as §8 and the A/B doctrine commit to and as this project
has done once already for `Lie`, is the expected outcome and not a failure of the
eval. The eval worked: it made the claim falsifiable, and then falsified it.

Two owner calls are unblocked by this report and are not taken in it:

1. Whether to build item 4 to close §8 formally, or to accept the case as made.
2. Whether to write a second pre-registration aimed at belief-state quality
   rather than act/abstain (limit 4). #35 may have pre-registered the wrong
   question about this substrate; finding that out is worth the eval having run,
   and it is not a licence to amend this one after the fact.
