# Pre-registration — IX evaluation milestone (flaky-vs-real benchmark discrimination)

**Status:** DRAFT — not yet binding.
**Becomes binding when:** this file is git-committed on `main` *and* the
prerequisites in §9 are landed. No eval run may be reported against an
uncommitted version of this document.
**Issue:** [#35](https://github.com/GuitarAlchemist/hari/issues/35) (PRD)
**Roadmap:** `ROADMAP.md` §"Sequencing (2026-07-21 synthesis)", step 2.
**Author:** Stephane Pareilleux · **Date authored:** 2026-07-28
**Data seen at authoring time:** none. No eval trace has been recorded or
replayed. This document is written before any outcome is observable.

---

## 1. The question

> Does routing a recorded IX autoresearch session through Hari produce
> measurably better Accept/Wait/Escalate recommendations than the same session
> without Hari — and does it beat the cheap baseline, not just the null one?

Everything Hari has measured so far is fixture replay authored by us. The
Phase-5 negative result (`Lie` loses to `SubjectiveLogic` on
`false_acceptance_count`) was itself measured on six hand-authored fixtures, and
the fixture-selection critique in `phase5-results.md` §6 applies to this eval
too. The purpose of pre-registering is to make the answer falsifiable *before*
we can see which answer flatters the substrate.

## 2. Task and ground truth

**Task:** flaky-vs-real benchmark discrimination. IX runs a micro-benchmark N
times with injected perturbations. Some perturbations are genuine regressions;
some are pure run-to-run variance. Hari observes the resulting `ResearchEvent`
stream and recommends `Accept` / `Wait` / `Escalate`.

**Ground truth is mechanical, not judged.** We inject the perturbation, so the
correct answer is known by construction and recorded in the fixture. No human
grades a decision.

**Pairing.** Each task ships as a should-act / should-abstain pair of
`ResearchTrace`s differing by *exactly one* injected trigger (a real regression
vs. injected variance). This is the guard against degenerate policies: an
always-`Accept` or always-`Wait` policy scores 100% on one half of every pair
and 0% on the other, so it cannot exceed 50% Paired Accuracy. Any policy that
does not clear 50% paired is reported as no better than a constant.

**Fixtures** land under `fixtures/ix/`, alongside the existing `slow_evidence`
and `heavy_contradiction` traces, and must be deterministically replayable.

## 3. Design

**Counterfactual shadow replay, not live A/B.** One `ResearchTrace` is recorded
per task instance, then replayed under every policy arm via the existing
`replay --compare3` path (`crates/hari-core/src/main.rs:32`). Two live runs
would destroy the pairing through nondeterminism; the whole point is that every
arm sees byte-identical input.

**Unit of analysis: one decision.** Not one trace. A trace contributes ~20
decisions, and decisions within a trace are correlated — §6 handles that
explicitly rather than pretending they are independent.

## 4. Arms

| Arm | What it is | Role |
|---|---|---|
| `IX-unassisted` | recorded IX behavior with no Hari policy applied | null baseline — the "does Hari do anything" comparison |
| `RecencyDecay` | current default `PriorityModel` (ADR-0001) | incumbent — what shipping today already gives you |
| `SubjectiveLogic` | Opinion-fusion pipeline, ~600 lines | **cheap baseline — the one that matters** |
| experimental | any Hari policy under test | the claim |

`Lie` is **not** an arm. It was demoted on evidence (`phase5-fixture-rollup.md`
§7) and adding it back would be re-litigating a settled negative result.

This eval **does not change the default `PriorityModel`.** ADR-0001 stands and
`test_priority_model_default_is_recency_decay` stays green. The eval produces
the data a future default change would need; it is not itself that change.

## 5. Metrics

### 5.1 Primary — exactly one

**Paired Accuracy**: the fraction of should-act/should-abstain *pairs* where the
policy gets **both** halves right.

**Act/abstain taxonomy — declared 2026-07-29, before any outcome inspected.**
An outcome *acts* when it contains at least one substantive action; `Log` is
side-channel and ignored, as are the bookkeeping `UpdateBelief` /
`SendMessage`. So `Accept`, `Escalate`, `Investigate`, and `Retry` are acting;
`Wait` is abstaining. Two cases that occur nowhere in `fixtures/ix/` are pinned
anyway: an outcome with no substantive action at all abstains, and an outcome
mixing `Wait` with a substantive action acts. **`Escalate` counts as acting** —
handing a decision upward is doing something rather than withholding.

This choice moves every number, and §9 item 3 shows it also decides whether the
metric is measurable at all: an alternative taxonomy — *act* = commit to the
claim (`Accept`), *abstain* = withhold commitment (`Wait`, `Investigate`,
`Escalate`) — would make act/abstain a function of `HexValue`, and therefore of
evidence, which the current taxonomy is not. **That alternative is not adopted
here.** Adopting it is an owner call and requires a §10 amendment made before
outcomes are inspected.

One primary metric, declared here, and only this one carries the decision rule.
The PRD listed four metrics as a "primary set"; promoting all four would create
a multiplicity problem that silently inflates the false-positive rate — with
four correlated metrics at α=0.05 the family-wise error rate approaches ~0.15.
The other three move to secondary.

### 5.2 Secondary — reported, never decisive

Act Accuracy · Abstain Accuracy · Conditioned Abstention Rate ·
`false_acceptance_count` · `false_rejection_count` · calibration (mean Brier,
per belief, from `forecast::calibration()`).

Secondary metrics are reported with CIs but **no** secondary result may be
substituted for the primary in the conclusion. If the primary fails and a
secondary passes, the eval failed.

### 5.3 Disqualifier — the caution tax

A policy that wins Paired Accuracy while showing **worse** `false_rejection_count`
than its comparator is **disqualified**, not celebrated. Emitting more `Wait`s
is not free, and without this rule a maximally cautious policy could bank the
abstain half of every pair. `false_rejection_count` does not exist in
`ReplayMetrics` today — see §9.

**Amendment (2026-07-29): the disqualifier must be scored arm-independently.**
`ReplayMetrics::false_rejection_count`, added earlier the same day, resolves
each `Wait` against *the replay's own* `final_beliefs` and excuses a wait whose
claim ended `Contradictory` ("withholding on irreconcilable evidence is
correct"). Whether a claim ends `Contradictory` is an arm's **output**, not a
fact about the world, so the excusal lands unevenly. Measured on
`heavy_contradiction`:

| arm | waits | waits on a claim | charged |
|---|---|---|---|
| RecencyDecay | 3 | 0 | 0 |
| Lie | 5 | **2** | **0** |
| SubjectiveLogic | 6 | 6 | **5** |

RecencyDecay's zero is honest — it never withheld on a claim. Lie's is the
artifact: it withheld on `epsilon-cot-helps` and `zeta-self-consistency` and was
charged for neither, because it left both `Contradictory`. Subjective Logic
withheld on the same claims and *resolved* them to `Probable`, so its waits were
charged. Same trace, same evidence: **the intrinsic counter rewards staying
stuck and penalises reaching a conclusion.**

Because §5.3 can disqualify a policy that *won* the primary metric, this would
have disqualified the arm the existing data already favours — SL beats Lie on
`false_acceptance_count` on 3/6 fixtures and never loses.

The disqualifier is therefore scored by `paired_eval::score_false_rejections`,
which grades `Wait`s against authored `ClaimLabel` ground truth and is pinned
arm-independent by `the_verdict_does_not_depend_on_the_arms_own_beliefs`.
Waits on claims with no label are **reported, never excused** — missing ground
truth is a fixture defect, not a free pass. `ReplayMetrics::false_rejection_count`
survives as single-arm diagnostics and is documented as not comparable across
arms. No eval outcome was inspected; the artifact was found while exploring
whether replay could emit its own forecasts (§9 item 2).

### 5.4 Excluded, with cause

`consensus_stability` and `goal_completion_rate` are **excluded from the primary
comparison**. Both are tied by construction across all existing fixtures: they
read event payloads *upstream* of the policy layer, so no policy can move them.
Reporting an artifactual tie as evidence of equivalence would be dishonest.
They may be reinstated only if their derivation is fixed to read
post-policy state, and only by a committed amendment under §10.

**Amendment (2026-07-29): the tie was asserted, not measured — and it holds for
only one of the two.** Now checked by
`metric_liveness.rs::the_section_5_4_exclusions_hold_only_where_measured`:

* `consensus_stability` — **genuinely tied** across all three arms on all eight
  fixtures. The exclusion stands, now for a measured reason.
* `goal_completion_rate` — **not tied.** It is identical between `RecencyDecay`
  and `Lie`, but `SubjectiveLogic` moves it on **5 of 8** fixtures
  (`cognition_divergence` 0.5→0.0, `heavy_contradiction` 0.333→1.0,
  `long_recovery` 0.667→1.0, `racing_goals` 0.4→0.2, `swarm_dissent`
  0.667→0.333). The mechanism is explicit:
  `process_research_trace_subjective_logic` assigns
  `goal.status = hex_value_for_opinion(op, &config)` — goal status derived from
  SL's posterior, which **is** post-policy state, the exact condition this
  section names for reinstatement.

So the exclusion was sound for the two-arm Phase 5 comparison it was authored
against, and became wrong when SL was added as a third arm. The reasoning stayed
in place while the premise stopped being true.

`goal_completion_rate` is therefore **eligible for reinstatement as a §5.2
secondary** — it discriminates between arms. Reinstating it is an owner call
and does not happen in this amendment; a metric that has been excluded through
the period when arms were compared cannot be quietly promoted afterwards. It
stays out of the **primary** comparison regardless: §5.1 admits exactly one
metric. Nothing here changes the primary or the decision rule, and no eval
outcome has been inspected.

## 6. Statistics

- **Aggregation:** paired bootstrap **clustered by trace**, resampling traces
  (not decisions), B = 10,000.
- **Determinism:** fixed seed, recorded in the report. A re-run must reproduce
  the interval exactly.
- **Why clustered:** ~20 decisions within one trace share a belief state. Treating
  them as independent would inflate effective n by up to ~20× and manufacture
  significance. Resampling at the trace level is the conservative choice.
- **Implementation:** the aggregator is a **pure function** — decision-outcome
  records in, CI/p out — so it is unit-testable on hand-built inputs with a
  known-sign effect, with no replay engine in the loop.

### Dual acceptance rule

An improvement is accepted **only if both** hold:

1. the 95% CI on the paired difference **excludes zero**, and
2. **p < 0.05**.

Either alone is insufficient. This is stricter than convention and deliberately
so: with one primary metric and a small trace count, the cost of a false
positive here is a substrate we keep for no reason.

## 7. Minimum detectable effect — and the honest power problem

**The planned sample is only powered for a large effect. This is the most
important number in this document.**

Planned: 5–10 traces × ~20 decisions ≈ 100–200 paired decisions. But clustering
is what governs power, and the effective sample is far smaller:

| assumed ICC | design effect (m=20) | effective n (from 150) |
|---|---|---|
| 0.05 | 1.95 | ~77 |
| 0.10 | 2.90 | ~52 |
| 0.20 | 4.80 | ~31 |

At ICC = 0.10 and 80% power, α = 0.05, the detectable difference in Paired
Accuracy is roughly **0.15–0.20 absolute** — and the true driver is the
discordant-pair rate, which we cannot know until pilot data exists.

**Consequences, accepted in advance:**

- A real improvement smaller than ~15 points **will not be detected**. A null
  result from this eval means "no large effect," never "no effect."
- The report must state the achieved effective n and the realized ICC.
- **Committed before unblinding:** the ICC will be estimated from the first
  recorded traces, and the MDE recomputed and **committed as an amendment
  (§10) before any outcome is inspected.** Estimating ICC from the same data
  used for the verdict, after seeing it, is not permitted.
- If the recomputed MDE exceeds 0.20, the honest move is to **add traces or
  abandon the run**, not to report an underpowered null as evidence of
  equivalence.

## 8. The kill/keep rule

Applied **mechanically** to the bootstrap output. The conclusion follows from
this rule, not from narrative in the report.

**KEEP** the substrate only if **both**:

1. experimental beats `IX-unassisted` on Paired Accuracy under the §6 dual rule; **and**
2. experimental does **not lose** to `SubjectiveLogic` on calibration (mean Brier).

**KILL** — publish the negative result — if either fails. Specifically, if
`SubjectiveLogic` matches or beats the experimental arm, the report must state
plainly:

> ~600 lines of Subjective Logic already deliver the benefit; the additional
> substrate does not.

Per the A/B doctrine, that negative result is **kept and published**, exactly as
the Phase-5 Lie result was. This project's credibility rests on having done that
once already.

## 9. Prerequisites — the eval cannot run until these land

Verified against `main` @ `859b29c` on 2026-07-28. **Amendment (§10): items 1
and 2 are satisfied by the branch carrying this edit; 3 and 4 remain open.**
Nothing in §5–§8 changed — this records prerequisite status only, and no eval
outcome has been inspected.

**Second amendment (2026-07-29, `main` @ `f34d60c`).** Item 2's calibration
block shipped ledger-global: it folded in every record in
`HARI_STATE_DIR/forecasts/` regardless of whether the replayed trace mentioned
the belief, while documenting itself as "calibration for the beliefs this trace
touched". Replaying `conflicting_benchmark.json` therefore reported
`pooled_mean_brier: 0.68125` over four GA/Demerzel beliefs, none of which the
fixture names — a number that reads as if the trace's own claim had been scored.
`with_calibration` is now scoped to touched beliefs and the regression is
pinned by `calibration_excludes_forecasts_about_beliefs_the_trace_never_touched`.
Two consequences for §8, both recorded below. Still no eval outcome inspected.

Item 1 surfaced a finding that matters for §5.3 and for the Conditioned
Abstention Rate secondary: **no Wait in the existing eight-fixture corpus is an
abstention on a claim.** Every one lands on a `goal_update` or
`relation_declaration`, which carry no proposition, so `false_rejection_count`
is a correct `0` corpus-wide and both abstention measures are unexercised until
the §9.3 paired fixtures exist. The metric is pinned by forced-abstention tests
instead.

1. ~~`false_rejection_count` does not exist.~~ **LANDED.** `ReplayMetrics` now
   carries `false_rejection_count` beside `false_acceptance_count`, counting
   `Action::Wait` on a claim that went on to stand. Conservative by design: a
   claim ending `Contradictory` is not counted (waiting on irreconcilable
   evidence is correct), and a later retraction excuses the wait. `Escalate` is
   not scored — it carries a reason, not a proposition, so there is no sound
   attribution. §5.3 is now enforceable.
2. ~~Calibration is orphaned.~~ **LANDED.** `ResearchReplayReport.calibration`
   is an opt-in `Option<ReplayCalibration>` (per-belief Brier from
   `forecast::calibration()` plus a scored-count-weighted pooled roll-up),
   attached via `with_calibration` and reachable as `replay --calibration`.
   Replay itself stays I/O-free, and the block is scoped to the beliefs the
   trace touched (see the second amendment).

   **The §8 calibration half is unexercised, and per-arm calibration is not a
   plumbing gap.** Two findings from the scoping fix:

   *Corpus.* Once scoped, **no fixture in the eight-fixture corpus has a single
   forecast about any belief it touches** — all eight report `0 beliefs, 0
   scored`. The ledger's four beliefs are `demerzel-belief-lint-reduces-warns`
   and three `ga-*` claims; fixture propositions are `benchmark-x-is-reliable`
   and kin. Zero overlap. So the calibration half of §8 is as unexercised as the
   abstention measures, and for the same reason: the corpus was never built to
   drive it. §9.3's paired fixtures must ship **with forecasts emitted about
   their propositions**, or the calibration criterion stays undefined.

   *Per-arm.* The earlier claim that `--compare3` "has nowhere to hang the
   block" was wrong — each arm's `ResearchReplayReport` already carries the
   `Option<ReplayCalibration>` field. The real obstacle is that it would hang
   three *identical* blocks. The touched-belief set derives from the trace's
   events, which are the same for every arm; measured on
   `cognition_divergence`, `heavy_contradiction`, and `swarm_dissent`, all
   three arms end with identical belief sets even where their action sequences
   diverge. A shared ledger scoped by a shared belief set cannot separate arms.
   Cross-arm calibration requires **each arm to emit its own forecasts from its
   own posterior** — a forecast-emission hook in replay, which is a new
   capability and a design question (what does an arm predict, and against
   which observable?), not wiring. Reclassified from "next blocking item" to a
   design prerequisite ranked behind 3 and 4, since paired fixtures are what
   would give an emission hook anything to predict about.

   *Feasibility of the emission hook, explored 2026-07-29.* The machinery
   exists: `Opinion::projected_probability()` (`P = b + u·a`) is implemented,
   the SL pipeline keeps live `Opinion`s and only quantises to `HexValue` when
   building `final_beliefs`, and `Opinion::from_hex` supplies an already-committed
   HexValue→probability ladder (0.90 / 0.70 / 0.50 / 0.30 / 0.10, with
   `Contradictory` → 0.50), so the mapping would not be freshly invented.

   Two constraints found, both of which must be settled before building it:

   * **Resolution must not be self-referential.** The obvious design — predict
     whether a claim still stands at trace end, resolve against `final_beliefs`
     — grades each arm against its own output, measuring self-consistency
     rather than accuracy. An arm that never updates would score perfectly.
     This is the same defect the §5.3 amendment records, and the same fix
     applies: authored ground truth (`ClaimLabel`), not derived state.
   * **Granularity is asymmetric.** SL carries a continuous posterior; the
     hex-valued arms can emit only six distinct probabilities and are
     quantisation-limited. "SL calibrates better" would then be partly a claim
     about representational resolution rather than about being better informed.
     That must be declared before any calibration outcome is inspected.
3. ~~Paired fixtures do not exist.~~ **PARTIALLY LANDED — and it surfaced a
   construct-validity problem that outranks the rest of this list.**

   Landed: `paired_eval` scores Paired Accuracy (`score_paired`), ground truth
   lives in a sidecar `PairedFixture` rather than on `ResearchEvent` (IX never
   transmits the right answer, so the protocol boundary carries no eval
   scaffolding), and `replay --paired` runs it end-to-end.
   `fixtures/ix/paired/propositionless_abstention.json` grades 1.0 on one pair.
   Ungradeable pairs — missing half, duplicated half, index past the replay —
   are reported as named `PairDefect`s, never silently dropped, and an ungraded
   run yields `paired_accuracy: null` rather than `0.0`.

   **The problem.** Under the default `RecencyDecay` model, whether Hari acts or
   abstains on a claim is a pure function of `state.cycle - event.cycle` and is
   *independent of the claim's evidence*. Two probes, both on
   `main` @ `e0bd425`:

   * 16 `belief_update`s with **identical** value (`Probable`) and identical
     evidence, differing only in trace position: `Accept` for the first 12,
     `Wait` for the rest. The boundary is exactly age 12, as
     `exp(-0.2 · 12) = 0.0907 < θ_wait = 0.1` predicts.
   * Four claims at the **same** position spanning the full evidence range —
     from `{"runs": 1, "note": "single anecdote, unreplicated"}` at `Doubtful`
     to `{"runs": 500, "p": 0.001}` at `True` — all act.

   `HexValue` selects *which* action (`Accept` / `Investigate` / `Escalate`);
   it never decides *whether* to act. Only cycle-age does. So a paired
   fixture's abstain half would encode "this claim arrives ≥12 claim-events
   after its stamp", not "this claim is insufficiently supported" — and Paired
   Accuracy would measure whether a policy's decay schedule matches the fixture
   author's cycle arithmetic. That is a tautology, and it is the mirror image
   of the §5.4 exclusions: those metrics are *tied* by construction, this one
   would be *driven* by construction.

   Corollary, which also explains item 1's finding: decay-driven abstention on
   a claim that goes on to stand is precisely a **false rejection**. The 16-claim
   probe scores `false_rejection_count: 4`, the first non-zero this metric has
   ever produced. §5.3 correctly reads decay-driven caution as a tax rather
   than a virtue — so labelling such an abstention "correct" would put §5.1 and
   §5.3 in direct contradiction.

   The shipped fixture therefore uses the only pair authorable today: commit to
   a corroborated claim (act) versus a `goal_update`, which carries no
   proposition and so offers nothing to commit to (abstain). It is deliberately
   weak and labelled as such in the fixture itself. **An evidence-insufficiency
   pair is not authorable, and §9.3 cannot be completed at eval scale until the
   decision below is made.**
4. **The driver does not exist.** `clients/ix_reference` currently holds
   `hari_client.py` and `run_session.py`; the paired Hari-on/Hari-off driver is
   unwritten.

**Ranking after the item 3 finding.** The blocking question is no longer which
artifact to build next but whether §5.1 measures a capability at all. Until the
act/abstain taxonomy question in §5.1 is settled, authoring 5–10 traces × ~20
pairs would produce a corpus whose abstain halves encode cycle arithmetic — 200
labels that cannot be reused if the taxonomy changes. **Settle the taxonomy,
then finish 3, then 4.**

Items 1 and 2 were the cheapest and were prerequisites for the decision rule
itself; they landed first. **The remaining blockers are 3 and 4, in that
order** — per-arm calibration is now ranked behind them, because both the
abstention measures (item 1's finding) and the calibration criterion (item 2's)
turned out to be unexercised by the existing corpus. Two of §8's inputs are
instruments with nothing yet to measure; §9.3 is what supplies the signal, and
it must carry emitted forecasts as well as paired traces.

## 10. Amendment policy

This document may be amended **only** by a git commit that (a) states what
changed, (b) states why, and (c) is made **before** the affected outcome is
inspected. The commit history is the audit trail.

Amendments after unblinding are permitted **only** as clearly-labeled
post-hoc analysis, which **cannot** satisfy §8 and cannot be reported as a
confirmatory result.

## 11. What would falsify the Hari claim

Stated now so it cannot be redefined later. Any of:

- Paired Accuracy for the experimental arm ≤ 50% (no better than a constant policy).
- No significant gain over `IX-unassisted` under the dual rule.
- `SubjectiveLogic` matching or beating the experimental arm on calibration.
- A Paired Accuracy win purchased with worse `false_rejection_count` (§5.3).

## 12. Out of scope

- The `SubjectiveLogic` short-circuit re-seam vs. baseline-only relabel — an
  architecture question, better as an ADR than as part of this eval.
- Changing the default `PriorityModel`.
- The second task (contradictory-results-across-configs, e.g. release vs. debug)
  — sketched in #35, deliberately not built here. This slice is a tracer bullet:
  task → pre-registration → driver → one report, end to end, thin.
