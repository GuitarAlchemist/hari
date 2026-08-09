# The §6 aggregator, and §8 applied to it — the #35 report

**Status:** the tracer bullet is complete end to end, §9 item 4 included as of the
2026-08-09 review amendment. **This is still not a §8 verdict on the corpora
below**, and §4 says exactly why it cannot be one; §7 and §9.9 record what the
driver did and did not settle.
**Pre-registration:** `2026-07-28-ix-eval-preregistration.md` (binding; committed),
amended at §9.8 and §9.9 by the branch carrying this report.
**Issue:** [#35](https://github.com/GuitarAlchemist/hari/issues/35)
**Date:** 2026-08-09 · **Author:** Stephane Pareilleux
**Repo state:** `claude/35-counterfactual-replay`, 408 tests passing.
**Every number in §3, §4 and §5 is unchanged by the review amendment** and
reproduces from §2's recipe exactly as first published.
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
| §9.3.2 — the isolation and task corpora are never pooled | `check_corpus`, which refuses the run |

**Amended 2026-08-09 after review.** Five more of the report's own claims moved
from prose into tests, and one from a command-line flag into the corpus. None
moves a published number; §3 and §4 reproduce byte for byte.

| claim | was | is now |
|---|---|---|
| the arm names on a published interval | two `&str` at the call site — transposing them credited the shipped substrate with SL's advantage, suite green | derived from `PairedArm::arm` through `TraceCluster`; a corpus whose clusters disagree is refused |
| §9.4's provenance | `--corpus recorded` accepted over any file | read off each fixture's driver stamp and its trace digest; asserting `recorded` over unstamped fixtures is refused |
| §8 clause 1 | `dual_rule_passes` alone | `undefined` when zero between-cluster variance makes the pass automatic (§4) |
| zero between-cluster variance | identity of the whole delta sequence — a false negative on equal-mean clusters | equality of cluster means |
| `(count+1)/(B+1)` and `unmatched` | asserted in prose, deletable with a green suite | pinned, each by one discriminating test |
| §9.3.2's corpus rule | a suffix match that admitted any name it did not recognise, and duplicate paths | positive membership plus distinct trace ids |

Thirty-four tests in `crates/hari-core/tests/paired_bootstrap.rs`, eleven CLI
subprocess tests in `replay_cli_refusals.rs`, four calibration probes in
`calibration_reliability_probe.rs`. The load-bearing
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

Neither command passes `--corpus`, and neither needs to. **Provenance is derived
from the fixtures**, not declared on the command line: each fixture's driver
stamp plus a trace digest `hari-core` recomputes and refuses on mismatch.
`--corpus` is an optional assertion checked against what was derived — asserting
`recorded` over unstamped fixtures is refused, and asserting `authored` over
stamped ones downgrades to `authored` and withholds. The twelve fixtures above
carry no stamp, so both corpora derive as `authored`, and an authored corpus
never yields a verdict (§9.4).

Driver-recorded fixtures do exist. One is committed as a worked example —
`fixtures/ix/paired/driver/accuracy-task.json`, stamped
`{driver, spec, seed, trace_digest}` and derived as `driver_recorded` — and the
six-trace driver corpus it belongs to is regenerated on demand by
`clients/ix_reference/paired_driver.py` (§7). That corpus is never pooled with
either corpus above, and §6 limit 7 records why no §8 verdict computed on it may
be read as a result.

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

**This comparison carries no ICC, and the absence is a consequence rather than an
omission.** §7 requires the report to state the realized ICC and effective *n*.
For clause 1 they are **undefined**: every delta is zero, so the one-way ANOVA's
`ms_between + (m₀ − 1)·ms_within` denominator is zero, and `icc`,
`design_effect` and `effective_n` are absent from the JSON rather than reported
as `0`. There is no variance to decompose when there is no variance. §5.3 below
gives the realized values for the comparison that *has* them —
`SubjectiveLogic` vs the shipped arm — which is the comparison §7's instruction
turns out to be answerable for.

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

### 3.3 §5.2's fourth metric — Conditioned Abstention Rate

The PRD's metric set names four (Act Accuracy, Abstain Accuracy, Paired Accuracy,
**Conditioned Abstention Rate**) and §5.2 carries CAR forward as a secondary. It
had no implementation until this change; it is now computed per policy and
emitted per fixture under `corpus[].conditioned_abstention`.

"Conditioned" means *conditioned on whether abstaining was the right call*, which
is what the paired design makes available. Isolation corpus, one fixture (all six
are clones, so one row is the corpus):

| policy | abstains when abstaining is right | abstains when acting is right | discrimination |
|---|---|---|---|
| `IX-unassisted` | 0.3333 | 0.0000 | **+0.3333** |
| `RecencyDecay` | 0.3333 | 0.0000 | **+0.3333** |
| `Lie` | 0.3333 | 0.0000 | **+0.3333** |
| `SubjectiveLogic` | **1.0000** | **1.0000** | **0.0000** |

**One honest caveat, stated rather than left to be rediscovered.** Under §5.1's
adopted taxonomy the first column is *numerically identical* to Abstain Accuracy
— abstaining on a labeled abstain half is getting that half right, so the two
count the same events. Presenting them as independent corroboration would
double-count one measurement. The metric's information content over the existing
three is the **second** column and the discrimination.

That is not academic here. SL's `1.0000` in column one is the flattering reading
of the degenerate always-`Wait` behaviour §9.3.2 recorded on the isolation
corpus; column two shows it abstaining just as often when acting was correct, and
the discrimination collapses to zero. A policy that abstains without reading the
condition is exactly what §5.1's pairing exists to catch, and CAR is where it
shows up as a number rather than as a footnote.

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

**Clause 1 no longer rests on `dual_rule_passes` alone.** A corpus with no
between-cluster variance clears the §6 dual rule *automatically* whenever the
point estimate is non-zero — every resample returns the same statistic, so the
interval has zero width, excludes zero, and `p = 2/(B+1) < α` unconditionally.
That is what §3.2's two intervals are. Clause 1 now reports **`undefined`**
rather than `passes` in that state. The distinction matters because provenance is
the caller's assertion: before this, a degenerate corpus plus one mis-set
`--corpus recorded` would have converted the corpus's *design* into a mechanical
clause-1 pass. A **failing** dual rule is still `fails` — degeneracy can
manufacture a pass, never a failure — so the result above is unchanged.

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
7. **The driver corpus is a smoke corpus, not the eval's corpus** (§9.9). Six
   generated traces, 18 pairs, realized effective *n* ≈ 7. It demonstrates that a
   mechanical §8 verdict is reachable and that provenance is enforced against the
   corpus; it is nowhere near §7's planned 100–200 decisions, and §7's own
   instruction bars reading an underpowered null as equivalence. Its `SPEC` is a
   candidate distribution, not a ratified one.

   **Power is not the binding constraint, and this is the more serious half.**
   The driver's pairing is **structural**: every one of its 18 pairs is §9.3's
   **G2 commitability** ground — an act half carrying a corroborated proposition
   against an abstain half (`goal_update`) carrying none — where the authored
   corpora at least mixed G1/G2/G3. `SPEC`'s `real_regression_rate_in_16ths`
   consequently reaches only §5.3's secondary false-acceptance and
   false-rejection counts and **not the primary paired metric**: regenerating the
   corpus at 0 and at 16 sixteenths leaves clause 1 at 0.0000 [0.0000, 0.0000]
   p 1.0000, the cheap comparison at −0.7778 [−1.0000, −0.4444], and the verdict
   at KILL — identical to the committed 7. The corpus is trivially separable by
   payload type, which is why `IX-unassisted` scores a perfect 1.0000 on it. So
   it does not measure flaky-vs-real discrimination at all, and **adding traces
   cannot repair that**: a corpus of any size drawn from this `SPEC` would still
   measure commitability, at effective *n* far above 7 and still construct-invalid
   for §2's task. Construct validity, not sample size, is the first thing the
   distribution ratification of §9.4 must fix. Rebuilding the abstain half as a
   should-abstain *claim*, so that the injected trigger is what separates the
   halves, is an owner scoping call left to a separate future slice and is not
   attempted here (§9.9).
8. **The reliability-diagram probe binds a pure function over a synthetic
   ledger, not a boundary.** Story 15's literal ask is met — a `probe_*`
   regression bound with a working negative control
   (`probe_reliability_diagram_bound_holds_over_a_synthetic_ledger`, ECE ≤ 0.05,
   with a companion test showing a 0.20 confidence shift breaking it) — and that
   is the whole of what it binds. The probe builds a `Vec<ForecastRecord>` in
   memory and calls `forecast::reliability_diagram` directly; it does **not**
   traverse `ReplayCalibration`, `with_calibration`, the on-disk forecast ledger,
   or the report/CLI boundary. `ReplayCalibration` carries no diagram, so there
   is no such boundary to reach today, and emptying `with_calibration` leaves
   this probe green — the two `lib` tests pinning that wiring are what catch it.
   It also does **not** touch §8 clause 2, which needs each arm to emit forecasts
   from its own posterior (§9 item 2) and stays `undefined`.

## 7. What remains of #35

**§9 item 4 — the paired driver in `clients/ix_reference` — now exists**
(`paired_driver.py`), and §9.9 records what it did and did not settle. It draws
traces from a declared generative spec, stamps each fixture with a digest
`hari-core` recomputes and refuses on mismatch, and hands the corpus to the
existing `replay --paired --compare3 --bootstrap` boundary in one command. §9.4's
standing rule is therefore enforced **against the corpus** rather than against a
command-line flag: `--corpus recorded` over the hand-authored fixtures is now
refused.

It was not built in the expectation of a different answer on the primary metric
(§9.6) or on `false_acceptance_count` (§9.7), and it did not produce one: on the
driver corpus the shipped arm and the null baseline still agree at every
decision, clause 1 is still identically zero, and the mechanical verdict is
**KILL** — reached through the rule rather than through narrative, which is what
story 14 asks for.

**What remains is not code.** Two things:

1. **The task distribution is not ratified.** §9.4 states that choosing it is the
   one remaining pre-registration decision and an owner call. The driver's `SPEC`
   is a candidate, disclosed in one place; it is not §2's declared distribution
   until an owner says so. No §8 verdict in this report rests on it — §3 and §4
   are unchanged and still authored-corpus characterisation.
2. **§7's pre-unblinding MDE obligation is unfulfilled and now diagnosable**
   (§9.9). The driver corpus's realized effective *n* is ~7, far under §7's
   planned 100–200, so §7's own instruction — *"add traces or abandon the run"* —
   applies before any driver-recorded verdict may be read as a result. **Adding
   traces is necessary and not sufficient**, and it is the second thing to fix,
   not the first: the current pairing is structural G2 commitability, the
   primary metric is invariant to `SPEC`'s ground-truth parameter, and no corpus
   size repairs that (§6 limit 7, §9.9).

Also outstanding, and deliberately not attempted here: the per-arm forecast
emission hook (§9 item 2), whose two design constraints — resolution must not be
self-referential, and granularity is asymmetric between a continuous posterior
and a six-valued one — are recorded in §9 and must be settled before it is built.

## 8. Recommendation

Unchanged from the interim assessment, now reached mechanically rather than by
assembling instruments: **the pre-registered KEEP condition is unreachable on
every instrument that exists, and the substance of the pre-registered negative
result — ~600 lines of Subjective Logic already deliver the benefit **on the task
corpus**, on both graded metrics; the additional substrate does not — is what the
data supports.**

**The corpus qualifier is load-bearing and is not a hedge.** §3.2's own table
shows the sign flipping on the isolation corpus, where `SubjectiveLogic` scores
`0.0000` against `RecencyDecay`'s `0.3333` and is disqualified under §5.3 by 108
false rejections. §9.3.2 forbids pooling the two, so there is no corpus on which
the sentence holds unqualified, and §9.3.3 already recorded why: SL wins the task
corpus because `True` clears its `b > 0.7` gate, and loses the isolation corpus
because it accepts nothing there at all. What survives on **both** corpora is the
weaker and still decisive claim: the shipped substrate does not separate from
naive acceptance at any decision on either.

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
