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

`goal_completion_rate` therefore looked **eligible for reinstatement as a §5.2
secondary** — it discriminates between arms.

**Second amendment (2026-07-30, adversarial review): do not reinstate.** The
discrimination is real and is almost entirely artifact. Decomposed per fixture
**as measured before the starvation fix landed** (see the third amendment below,
which supersedes the `long_recovery` row and the aggregate):

| fixture | decay | SL | mechanism |
|---|---|---|---|
| cognition_divergence | 0.500 | 0.000 | **staleness** — decay credits `alpha-prompt-helps` as `Probable` while its own belief is `Contradictory` |
| swarm_dissent | 0.667 | 0.333 | staleness (`omicron-router-better`) |
| racing_goals | 0.400 | 0.200 | staleness, author-supplied (`lambda-tool-correct`); decay changes **no** goal status on this fixture |
| long_recovery | 0.667 | 1.000 | **starvation** — `gamma-method-correct` holds `top_goal` zero times in 22 events |
| heavy_contradiction | 0.333 | 1.000 | genuine posterior difference |

Two distinct substrate defects drive four of the five, and they push in
**opposite directions**:

* **Staleness.** The hexavalent write (`lib.rs:1384-1404`) is *upgrade-only* — it
  assigns `goal.status` only in the `True | Probable` arm; `Contradictory`
  escalates without touching status. So a goal keeps credit after its own
  evidence collapses. SL's write is unconditional and tracks the posterior down.
* **Starvation.** `top_goal` (`lib.rs:848-853`) filters out only goals whose
  status is already `True`, so *any* goal that never reaches `True` — including
  one stuck at `Unknown` — holds the slot indefinitely and blocks every
  lower-priority goal. The hex arms set status for `top_goal` alone; SL sets it
  for every goal with an opinion.

The comment at `subjective_logic.rs:450-451` claiming SL "mirrors the
Lie/RecencyDecay treatment ... so the metric is comparable" is false on
coverage, on directionality, and on timing.

The one residual, `heavy_contradiction`, is not a goal-completion capability
either: SL fuses `Contradictory → Probable` where `hari-lattice` deliberately
preserves irreconcilable evidence. Scoring that collapse as achievement, with no
ground truth on whether `Probable` is right, penalises the project's core
epistemic commitment and rewards credulity.

**Aggregate, and the trap.** Corpus means are decay **0.4458** vs SL **0.4417** —
SL is marginally *worse*, so excluding it costs SL nothing today and reinstating
it would not flatter SL. But all three of SL's losses are decay's stale credit.
**Fixing the derivation and then reinstating — the exit this section
pre-authorises — is precisely the move that would make SL look better.** Recorded
here, before anyone fixes it, so that sequence cannot later read as neutral.

Treatment: single-arm diagnostics, documented as not comparable across arms —
the same disposition §5.3 gave `ReplayMetrics::false_rejection_count`.
Reinstatement requires all four of: the same write rule and coverage across arms;
`top_goal`'s tie-break made name-free; the starvation defect fixed; and
"achieved" graded against authored ground truth rather than the arm's own
posterior. The last is the §9 item 2 blocker and is not reachable before §9.3.

Stated plainly rather than dressed as eligibility: this is exclusion for the
foreseeable term. The honest counter-argument is that each round produces a
fresh reason to exclude the one secondary that discriminates, which is hard to
distinguish from suppression — the mitigation is that these reasons are
measured, and the exit conditions above are concrete and falsifiable.

It stays out of the **primary** comparison regardless: §5.1 admits exactly one
metric. Nothing here changes the primary or the decision rule, and no eval
outcome has been inspected.

**Third amendment (2026-07-30): the starvation defect is fixed; the verdict is
unchanged.** Goal status is now refreshed for every goal each cycle rather than
for `top_goal` alone. Action emission stays top-goal-only — one attention target
per cycle — and the emitted action sequences are unchanged, verified against
per-fixture action counts recorded before the change.

What moved:

* `long_recovery` 0.667 → **1.000** for the hexavalent arms, so it is now **tied**
  with SL and drops out of the discriminating set. Arms differ on **4 of 8**, not
  5: three from staleness, one from the `heavy_contradiction` posterior difference.
* Corpus means are now decay **0.4875** vs SL **0.4417** — decay went from a
  statistical wash to *ahead*, because the starved goals it was being denied
  credit for are now counted.
* Reinstatement condition 3 ("the starvation defect fixed") is satisfied. The
  other three are not.
* `known_violation_top_goal_ties_are_broken_by_goal_name` is **dissolved** and
  replaced by a theorem: which goal wins a priority tie no longer determines what
  gets evaluated, so `goal_completion_rate` is name-invariant under a tie. The
  tie-break itself is untouched, which was the point — it was the symptom.

**Correction to the trap recorded above.** The second amendment warned that
"fixing the derivation and then reinstating is precisely the move that would
flatter SL." That is true of the **staleness** fix, not of the starvation fix —
the starvation fix moved the metric in *decay's* favour. Both defects were
lumped together as "the derivation" and they push in opposite directions. The
warning stands, and now applies specifically: the remaining unfixed defect is the
upgrade-only write, and fixing *it* is what would move this metric toward SL.
That defect stays pinned by
`known_violation_goal_status_is_never_revised_downward`, and it is deliberately
not fixed here — revising status downward would change what "achieved" means,
which is a §5.2 semantics decision rather than a starvation bug.

Also landed: `CognitiveState::add_goal` no longer resets `status` to `Unknown`
when a goal is re-declared, so a priority-revising `goal_update` can no longer
silently discard a completion the policy had established. An authored `status` in
the `goal_update` payload still wins, since it is applied after — that is an
explicit assertion by IX, not an accident.

No eval outcome has been inspected.

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
   weak and labelled as such in the fixture itself.

   **Third amendment (2026-07-30). The scorer now grades all three arms
   against one label set — and the weak fixture already produces a result.**

   §5.1 defines the primary metric *per arm*, but `replay --paired` scored a
   single arm: it built one `CognitiveLoop` on the default model, so producing
   the §5.1 table meant three fixture files differing only in the model, with
   the same ground truth copied into each. Drift between those copies would be
   indistinguishable from a policy difference once the numbers were in. Labels
   grade the *decision point*, which is a property of the trace, so
   `score_paired_three_way` replays one `PairedFixture` under `RecencyDecay`,
   `Lie`, and SL and grades all three against its single label set. Reachable
   as `replay --paired --compare3`. Arm-independence of gradeability is pinned
   by `theorem_gradeability_is_arm_independent`; a fixture no arm splits on is
   reported as `is_undiscriminating` rather than presented as agreement.

   **The measured result reverses the expectation recorded above.** On
   `propositionless_abstention.json` — the deliberately weak fixture — the arms
   split, and not in SL's favour:

   | arm | paired accuracy | act half | abstain half | false rejections |
   |---|---|---|---|---|
   | RecencyDecay | **1.000** | correct | correct | 0 |
   | Lie | **1.000** | correct | correct | 0 |
   | SubjectiveLogic | **0.000** | **wrong** | correct | **1** |

   SL fuses the single corroborated `belief_update` to `b=0.550 d=0.150
   u=0.300` (`P=0.700`), which does not clear its accept threshold, so it emits
   `Wait` and misses the **act** half — on a claim labelled `stood`, which
   §5.3 charges as a false rejection.

   This matters for how §9.3 is authored. The correction above establishes that
   SL abstains on evidence where the hexavalent arms are evidence-blind, and
   the natural inference — that an evidence-based paired corpus would score 0.0
   for decay and `Lie` by construction and thereby favour SL — is now shown to
   be **half the picture**. Evidence-sensitivity is symmetric: it is a benefit
   on abstain halves and a cost on act halves. The primary metric charges SL
   for under-commitment exactly as it charges decay for over-commitment, which
   is what a paired metric is for. No prediction about the corpus outcome
   follows from either half alone, and none is made here.

   Pinned by `subjective_logic_pays_for_its_caution_on_the_act_half`. One
   fixture, one pair — this is a demonstration that the instrument
   discriminates, not evidence about the arms.

   **A latent mislabel surfaced while wiring this, and is fixed.**
   `process_research_trace_subjective_logic` built its report with
   `priority_model: Default::default()`, justified in a comment as "left at its
   default (`PriorityModel::Flat`)". That ceased to be true when the
   post-Phase-5 substrate decision moved the default to `RecencyDecay`: from
   then on every SL report, including the `subjective_logic` arm of every
   `--compare3` run, claimed to be a `RecencyDecay` report. Nothing read the
   field, so nothing failed — the same shape as the four instrument defects
   §9 already records. Under §5.1 the label becomes load-bearing, because a
   number that cannot be attributed to the policy that produced it is not a
   comparison. Now `PriorityModel::SubjectiveLogic`, pinned across the corpus
   and on all three arms by `probe_every_arm_reports_the_model_that_produced_it`.
   No metric moved: the full suite passes unchanged, which is itself the
   evidence that nothing had been reading it.

   **Correction (2026-07-30, adversarial review).** The conclusion drawn above —
   "an evidence-insufficiency pair is not authorable" — is **false**, and the
   §5.1 taxonomy stands unamended. The scoped claim holds: `RecencyDecay` and
   `Lie` are evidence-blind on the act/abstain decision. Generalising it to *the
   metric* was the error. `SubjectiveLogic` abstains on evidence:
   `recommend_from_opinion` falls through to `Action::Wait`
   (`crates/hari-core/src/subjective_logic.rs:345`) when belief and disbelief are
   both sub-threshold and uncertainty is below the investigate threshold — no
   cycle arithmetic anywhere in it, and SL short-circuits before
   `score_actions_with_cycles` entirely. Measured on `swarm_dissent`:

   | arm | waits | on a proposition |
   |---|---|---|
   | RecencyDecay | 3 | **0** |
   | Lie | 7 | 4 |
   | SubjectiveLogic | 16 | **16** |

   So this is not a defect in the primary metric. It is a **result**, of the same
   class as the Phase-5 Lie verdict: *abstention is evidence-driven under
   Subjective Logic and evidence-blind under the hexavalent arms.* §9.3 fixtures
   are authorable and gradeable — for SL — and should be authored
   `swarm_dissent`-shaped with `cycle` stamps tracking position so decay never
   fires.

   **The alternative taxonomy (§5.1) is rejected**, on three measured grounds
   rather than on preference. (a) It does not fix the observation offered as its
   reason: `recommend_for_claim` maps `True | Probable | Doubtful | False` all to
   `Accept` (`lib.rs:2362`), so the four probe claims spanning "1 unreplicated
   anecdote at `Doubtful`" to "500 runs at `True`" **still all act** under it.
   (b) It re-imports the defect the §5.3 amendment just removed: abstaining via
   `Escalate` is uncharged, and `Lie` emits 13 `Escalate`s on
   `heavy_contradiction` — 13 free abstentions — while SL's claim-`Wait`s stay
   chargeable. (c) Under it, `act` reduces to `belief ∉ {Unknown, Contradictory}`,
   a `final_beliefs` readout promoted to primary. Switching a construct because
   it makes the instrument respond is instrument-driven redefinition, which §10
   does not license — §10 guards against *outcome*-driven amendment, which is a
   different and later failure.

   Two mechanism corrections while here, neither changing a verdict. The corpus
   `Wait`s under `RecencyDecay` are **not** decay-driven: a `goal_update` emits
   only a `Log`, `Log` scores a flat `0.05`, and `0.05 < θ_wait = 0.1` suppresses
   the list to `[Wait]` at age 0. And decay-driven abstention needs every event
   stamped `cycle: 1`, which no natural trace does — it is a fixture an author
   must deliberately construct, not something the substrate forces.
4. **The driver does not exist.** `clients/ix_reference` currently holds
   `hari_client.py` and `run_session.py`; the paired Hari-on/Hari-off driver is
   unwritten.

**Ranking after the item 3 finding.** The blocking question is no longer which
artifact to build next but whether §5.1 measures a capability at all. Until the
act/abstain taxonomy question in §5.1 is settled, authoring 5–10 traces × ~20
pairs would produce a corpus whose abstain halves encode cycle arithmetic — 200
labels that cannot be reused if the taxonomy changes. **Settle the taxonomy,
then finish 3, then 4.**

**Ranking update (2026-07-30).** The taxonomy is settled (item 3's correction)
and the instrument is built (item 3's third amendment): one label set, three
arms, per-pair verdicts. What remains in item 3 is **authoring the corpus** —
`swarm_dissent`-shaped traces with `cycle` stamps tracking position so decay
never fires, carrying forecasts about their own propositions per item 2. The
fixture mix (how many pairs turn on evidence sufficiency versus other grounds)
is a design choice with a foreseeable effect on the numbers, so it is committed
here **before** any corpus is authored: an all-evidence corpus would measure
one capability under a name that promises a general one. Author the mix first,
inspect nothing until it is committed.

### 9.3.1 Fixture mix — declared 2026-07-30, before a single pair was authored

The corpus is authored to this specification. It is committed in its own commit,
ahead of any fixture, so the ordering is checkable in git history rather than
asserted here.

Every abstain half is justified **normatively** — by what a competent research
assistant should do — and never by what any arm happens to emit. No arm was
replayed against any of these shapes before this section was written. The
grounds are three, in equal proportion:

**G1 — replication.** *Abstain:* a claim supported by one source, small `runs`,
explicitly unreplicated. *Act:* the same claim shape supported by two
independent sources with large `runs` in agreement. Single injected trigger:
replication. Normative basis: acting on one small unreplicated result is
precisely the flaky-vs-real failure §2 exists to catch.

**G2 — commitability.** *Abstain:* an event carrying no proposition
(`goal_update`, `relation_declaration`). *Act:* an event carrying a corroborated
proposition. Trigger: presence of a claim. Normative basis: there is no claim to
commit to, so withholding is the only coherent response.

**G3 — withdrawn basis.** *Abstain:* a claim whose supporting
`experiment_result` has just been retracted, leaving it unsupported. *Act:* the
same claim with its support intact. Trigger: the retraction. Normative basis:
re-committing on withdrawn support is exactly the false acceptance §5.2 counts.

**Proportion: exactly equal.** Three pairs per ground per trace — 9 pairs, 18
labeled decisions per trace — across 6 traces: **54 pairs, 108 labeled
decisions**, inside §3's pre-registered 100–200 range. Equal thirds are chosen
so that no ground dominates the aggregate. A 4/3/3 split weighted toward G1 was
considered and rejected: G1 is the ground whose outcome is most predictable in
advance, and over-weighting a predictable ground tilts the aggregate without
measuring anything more.

**Expected discriminating power, declared now so a later "it only worked
because of ground X" is checkable.**

| ground | hexavalent arms (`RecencyDecay`, `Lie`) | `SubjectiveLogic` |
|---|---|---|
| G1 replication | expected to **fail every abstain half** — measured evidence-blindness (§9 item 3), so ~0.0 on G1 | can pass; not guaranteed to |
| G2 commitability | expected to pass — this is the shipped weak pair's shape | measured to **fail the act half** on that pair (third amendment) |
| G3 withdrawn basis | **genuinely open** — no measurement exists of whether any arm carries provenance withdrawal into its act/abstain decision | **genuinely open** |

G1 and G2 are expected to pull in opposite directions, which is the point of
running both. G3 is the only ground whose outcome is unknown to the author at
authoring time, and it is included at full weight for exactly that reason.

**Reporting.** Aggregate Paired Accuracy is the §5.1 primary and remains the
only metric carrying the decision rule. A **per-ground breakdown is reported
alongside it, always** — a win concentrated in one ground must not read as a
general one. Pair identifiers therefore encode the ground (`g1-…`, `g2-…`,
`g3-…`) so the breakdown is derivable from `PairedScore::per_pair` rather than
reconstructed by hand.

**Construction constraints.**

* `cycle` stamps track trace position, so `exp(-λ·age) < θ_wait` never fires and
  no abstain half is satisfiable by decay arithmetic (§9 item 3's tautology
  warning).
* Each pair's two halves differ by the single injected trigger named above and
  by nothing else — same proposition shape, same source vocabulary, same
  evidence keys.
* Ground truth is authored in the `PairedFixture` sidecar; no eval scaffolding
  enters the Hari↔IX protocol boundary.

**What this mix does not settle.** Per-arm calibration still requires each arm
to emit forecasts from its own posterior (item 2), which does not exist. These
fixtures make the *abstention* measures exercisable; they do not by themselves
make the §8 calibration criterion defined.

#### 9.3.2 First measurement, and an underspecification in 9.3.1 it exposed

The corpus authored to §9.3.1 (54 pairs, 108 labels, committed before any
outcome was inspected) measures:

| | RecencyDecay | Lie | SubjectiveLogic |
|---|---|---|---|
| **Paired Accuracy** | **0.333** | **0.333** | **0.000** |
| Act accuracy | 1.000 | 1.000 | **0.000** |
| Abstain accuracy | 0.333 | 0.333 | **1.000** |
| G1 replication | 0.000 | 0.000 | 0.000 |
| G2 commitability | 1.000 | 1.000 | 0.000 |
| G3 withdrawn basis | 0.000 | 0.000 | 0.000 |
| false rejections (§5.3) | 0 | 0 | **108** |

Identical on all six fixtures. **Both policies are degenerate on this corpus,
in opposite directions**: `RecencyDecay` and `Lie` act on every claim, SL acts
on nothing. §2 predicted exactly this failure shape — "an always-`Accept` or
always-`Wait` policy scores 100% on one half of every pair and 0% on the
other" — and the pairing caught it, which is the guard working.

**The finding worth keeping: SL's discrimination is real and is discarded at
the threshold.** On G1 the corroborated claim fuses to `b=0.647 P=0.735` and
the uncorroborated one to `b=0.550 P=0.700`. The posterior separates them. But
`belief_accept_threshold` is `0.7` and the gate is `b > 0.7`, so **both** fall
short and both draw `Wait`. Evidence-sensitivity exists in the representation
and does not reach the action at this evidence level. That is a sharper result
than "SL discriminates" would have been, and it is not visible in any aggregate.

**Two authoring errors, both mine, both in §9.3.1.**

*The evidence map is inert.* §9.3.1 specifies abstain halves with "small
`runs`" and act halves with "large `runs`". No arm reads evidence **content**:
`Evidence` is consulted only for `.len()`, in a log line
(`subjective_logic.rs:551`, `:580`). `runs`, `p99_ms`, `budget` and every other
key are decorative. The only load-bearing dimensions at the `ResearchEvent`
boundary are the asserted `HexValue` and the number of agreeing assertions.
§9.3.1 specified the corpus along an axis that does nothing.

*The one axis that matters was left unspecified.* Having no guidance on
`value`, the corpus was authored with every act half at `Probable`
(`b=0.55` via the `Opinion::from_hex` ladder), which fuses to `0.647` — just
under the `0.7` gate. `True` maps to `b=0.85` and clears it on a single event.
So the corpus sits entirely on one side of SL's decision boundary and cannot
distinguish "SL never acts" from "SL cannot act *here*". The latter is the
truth: SL's act side is reachable, and this corpus does not reach it.

**Amendment (2026-07-30), and it is post-hoc — labelled as such.** This is
written *after* the numbers above were seen, which §10 flags. Two things make
it a specification repair rather than an outcome-driven amendment, and a reader
is entitled to weigh them:

1. Nothing below changes the metric, the taxonomy, the decision rule, the
   grounds, or their proportions. It adds a specification for an axis §9.3.1
   left undefined while specifying an axis that turned out to be inert.
2. The predicted effect is stated **before** the new corpus is authored or run,
   immediately below, and the prediction is checkable against what lands.

*Amended G1 operationalisation.* Act half: `True`, plus an independent
corroborating assertion. Abstain half: `Probable`, single source, no
corroboration. This bundles **two** observables — asserted confidence and
reproduction — where §9.3.1 named one. That is faithful to §2, whose injected
trigger is *"a real regression vs. injected variance"*: a real effect both
reproduces and is reported with confidence. G2 and G3 act halves move to `True`
for the same reason. Abstain halves are unchanged.

*The original corpus is retained, not replaced.* It holds `value` fixed and
varies corroboration alone, which is the tighter contrast and the only thing
that isolates reproduction as a variable. It is renamed to `*-isolation.json`
and its result above stands as reported. The amended corpus lands beside it as
`*-task.json`. **The two are never pooled**: one measures whether an arm can
detect reproduction, the other whether it can grade the task §2 defines.

*Prediction, recorded before the amended corpus exists.*

| ground | RecencyDecay / Lie | SubjectiveLogic |
|---|---|---|
| G1 | **0.000** — acts on both halves regardless of `value` | **1.000** — `True` clears `b>0.7`, single `Probable` does not |
| G2 | **1.000** | **1.000** |
| G3 | **0.000** — nothing carries retraction into the act/abstain decision | **0.000** — retraction resets the opinion to vacuous, then `True` re-clears the gate |
| **aggregate** | **0.333** | **0.667** |

If G3 comes back non-zero for any arm, the prediction was wrong about
provenance withdrawal and that is the more interesting result. If the aggregate
lands anywhere other than 0.333 / 0.667, this amendment mispredicted its own
effect and that must be reported as prominently as the number.

#### 9.3.3 The amended corpus, measured — prediction confirmed exactly

| | RecencyDecay | Lie | SubjectiveLogic |
|---|---|---|---|
| **Paired Accuracy** | **0.333** | **0.333** | **0.667** |
| Act accuracy | 1.000 | 1.000 | 1.000 |
| Abstain accuracy | 0.333 | 0.333 | 0.667 |
| G1 replication | 0.000 | 0.000 | **1.000** |
| G2 commitability | 1.000 | 1.000 | 1.000 |
| G3 withdrawn basis | 0.000 | 0.000 | 0.000 |
| false rejections (§5.3) | 0 | 0 | **0** (18 excused) |
| defects | 0 | 0 | 0 |

**Every cell matches §9.3.2's prediction**, which was committed before the
amended corpus was authored (`f0c6ec5`). That is the strongest defence
available for a post-hoc amendment, and it is offered as such rather than as
proof the amendment was disinterested.

Three things follow, and one of them disqualifies the table above from being
read as an eval result.

*SL clears the §5.3 disqualifier here.* It takes **zero** false rejections on
the task corpus — its 18 abstentions all land on `-flaky` claims, which are
injected variance by construction and therefore excused. So on this corpus SL
wins the primary metric without paying the caution tax that §5.3 exists to
charge. On the isolation corpus the same arm takes **108**. Same policy, same
grounds, same proportions: the disqualifier is entirely determined by whether
the corpus reaches SL's decision boundary.

*No arm carries provenance withdrawal into its act/abstain decision.* G3 is
0.000 for all three, on both corpora. A claim whose only supporting
`experiment_result` was retracted by targeted selector draws exactly the same
response as one whose support stands. For SL the mechanism is visible: the
retraction resets the opinion to vacuous, and the subsequent `True`
re-assertion re-clears `b > 0.7` from scratch. This was the one ground whose
outcome was unknown at authoring time, and it is unanimous — which makes it the
most informative result of the three.

**The corpus does not have 54 independent decisions. It has 9, replicated six
times.** All six traces are generated from one template; the only differences
are theme slugs and evidence values, and §9.3.2 established that evidence
*content* is inert. Verified directly: the `(cycle, source, payload type,
value)` sequence is byte-identical across all six fixtures, and every arm
returns identical per-fixture numbers. §3 requires 5–10 traces because §6
aggregates by a **trace-clustered** paired bootstrap; six copies of one trace
have zero between-cluster variance, so the bootstrap would report a
spuriously tight interval and §7's MDE would be meaningless. **The effective
sample size is 9 pairs.** This is an authoring error of the same kind as the
inert-axis one: the corpus satisfies §3's trace count in file count and not in
substance.

Consequently **no dual-rule test may be run against this corpus**, and the
numbers above are instrument characterisation, not an eval outcome. What they
establish is narrow and real: the scorer grades, the grounds discriminate, and
the arms separate on two of three grounds in a direction that matches a
prediction made in advance.

**What genuine trace independence requires** — the open item, and a design
question rather than more authoring. Between-trace variance has to come from
the *decisions* differing across traces, not from cosmetic relabeling. That
means traces that differ in what an arm actually sees: interleaved rather than
blocked grounds, varying numbers of competing assertions per claim, claims that
sit at different distances from `b > 0.7`, contradictions resolving in
different directions, and pair counts that differ per trace. Designing that
distribution is itself a pre-registration decision — it determines the
between-cluster variance the bootstrap will find, and picking it after seeing
§9.3.3 would be the third post-hoc corpus revision in one sitting. It is
deliberately left to an owner call.

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
