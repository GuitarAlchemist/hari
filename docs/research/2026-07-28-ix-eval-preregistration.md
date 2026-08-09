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
| `IX-unassisted` | pass-through acceptance: every claim taken at face value, nothing else decided (`unassisted.rs`; pinned §9.5) | null baseline — the "does Hari do anything" comparison |
| `RecencyDecay` | current default `PriorityModel` (ADR-0001) | incumbent — what shipping today already gives you |
| `SubjectiveLogic` | Opinion-fusion pipeline, ~600 lines | **cheap baseline — the one that matters** |
| experimental | **the shipped `RecencyDecay` `CognitiveLoop`** (pinned §9.5) | the claim |

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
`Wait` is abstaining. Two degenerate cases are pinned explicitly. **Correction
(2026-07-30, adversarial review):** this text claimed both "occur nowhere in
`fixtures/ix/`". Only the *mixed* case does. The *empty* case occurs 27 times
under `SubjectiveLogic` — every `goal_update` and `relation_declaration`, which
SL logs without recommending — and is precisely what awards SL the abstain half
of every G2 pair in §9.3. The original claim was measured on the default arm
alone. The pinned behaviour is unchanged; only the assertion that it was
unexercised was wrong. The two cases: an outcome with no substantive action at all abstains, and an outcome
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

* **Staleness.** The hexavalent write is *upgrade-only* — it assigns
  `goal.status` only in the `True | Probable` arm; `Contradictory` escalates
  without touching status. So a goal keeps credit after its own evidence
  collapses. SL's write is unconditional and tracks the posterior down.

  **Correction (2026-07-30, adversarial review).** Between `79ad578` and its
  follow-up fix the word *upgrade-only* was false of the code. The
  `True | Probable` filter selects which **beliefs** qualify to be written; the
  write itself was unconditional, so `Probable` landed on an achieved `True`.
  Since `top_goal` evicts only on `status == True`, the un-achieved goal was
  re-admitted to candidacy, displaced the real top goal and swallowed its
  actions. `79ad578` therefore also **falsified its own headline claim** that
  emitted action sequences were byte-identical: `heavy_contradiction` lost three
  `Escalate`s (13 → 10) and `long_recovery` two (8 → 6), in **both** hexavalent
  arms. That claim had been checked against per-fixture *wait* counts, which did
  not move — an inadequate check. With the downgrade guard in place the action
  sequences are now identical to pre-`79ad578` on all eight fixtures and all
  three arms, and every number in this section reproduces unchanged. Pinned by
  `theorem_an_achieved_goal_is_not_un_achieved_by_a_softer_belief`, which fails
  on the pre-fix code.
* **Starvation.** `top_goal` filters out only goals whose
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
- **Implementation (NOT BUILT — corrected 2026-07-30).** No bootstrap
  aggregator exists anywhere in the workspace, and §9's prerequisite list never
  included one. The present tense below describes a design, not code, and is a
  **missing prerequisite** ranked alongside item 4. As designed, the aggregator
  is a **pure function** — decision-outcome
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
   `Accept`, so the four probe claims spanning "1 unreplicated
   anecdote at `Doubtful`" to "500 runs at `True`" **still all act** under it.
   (b) It re-imports the defect the §5.3 amendment just removed: abstaining via
   `Escalate` is uncharged, and `Lie` emits 13 `Escalate`s on
   `heavy_contradiction` — 13 free abstentions — while SL's claim-`Wait`s stay
   chargeable. (`d1cd78d` briefly "corrected" this to 10, which was the count
   only inside the `79ad578` defect window; the guard in that same commit
   restored 13, so the edit made the number wrong and the sentence
   self-contradictory. Reverted.) (c) Under it, `act` reduces to `belief ∉ {Unknown, Contradictory}`,
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
ahead of any fixture.

**What that does and does not establish — corrected 2026-07-30.** Git certifies
the **commit sequence**, not when the artifacts were written. The 356-line
generator and 54 pairs landed 2m41s after this declaration commit, so they
plainly existed before the commits were sequenced, and the claim "before a
single pair was authored" is not something git checks. §9.4 states the general
form of this (blind pre-registration is unavailable to this author); it is
repeated here because a reader stopping at this section would otherwise be told
the history verifies something it cannot. What the sequence does establish is
that the specification was **fixed and public before the measurements existed**,
which is the property §10 actually needs — no number in §9.3.2 or §9.3.3 could
have influenced the text above it.

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
amended corpus was authored (`f0c6ec5`).

**How much that is worth: very little — corrected 2026-07-30.** This passage
originally called it "the strongest defence available for a post-hoc
amendment". §9.4 then establishes the opposite and voids it: every cell was
computable in advance from published constants — the `from_hex` ladder and the
`b > 0.7` gate give G1, decay/`Lie` evidence-blindness was already measured,
G2 was already measured on the shipped weak pair, and G3-for-SL follows from
retraction-reset semantics readable in the source. A prediction that could not
have come out otherwise is not evidence of disinterest. **Its residual value is
detecting author arithmetic errors, and nothing more.** Both sentences stood in
this document at once; a reader stopping here was misled, which is why the
correction lives at the claim rather than only in §9.4.

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

**Superseded by §9.4 below**: item 3 cannot supply what §6 consumes, so the
"3 then 4" ordering is inverted.

### 9.4 Ranking inversion — item 4 precedes item 3, and item 3 can never finish

**Recorded 2026-07-30 after an independent review** (Fable 5, advisory, given
the pre-registration, the fixtures, the generator and the arm constants, and
asked to verify rather than accept the summary it was given).

The §9 ordering — "settle the taxonomy, then finish 3, then 4" — is **wrong**,
and authoring the corpus is what demonstrated it. Item 3 cannot supply what §6
consumes, no matter how it is authored.

**The corpus degeneracy is worse than §9.3.3 recorded: three effective decision
situations, not nine.** §9.3.3 attributed it to the six traces being one
template. The review found it also holds *within* a trace: the three pairs of a
ground are clones of each other, differing only in the proposition slug, which
no arm reads except as an opinion-map key. Verified directly — within every
ground, in every arm, on both corpora, all three pairs return identical
`(act_correct, abstain_correct)` verdicts. The tell was already in §9.3.2 and
§9.3.3's own tables: every per-ground score is exactly `0.000` or `1.000`,
never ⅓ or ⅔. So the corpus holds **3** distinct decision situations replicated
18 times each, and the defect is not a trace-count problem that varying traces
would fix. The generator produces no variation at any level.

**Why no authored corpus can fix it.** §6 aggregates by a paired bootstrap
clustered by trace. A bootstrap estimates sampling variance across traces drawn
from a population. Hand-authored traces are not draws from anything: the
"variance" it would find is a property of whoever wrote the generator, and with
deterministic arms the resulting CI width is a design artifact. The unit of
analysis is not the defect — clustering by trace is correct, since decisions
within a trace share belief state. **The defect is that there is no
population.**

**Blind pre-registration of a trace distribution is not available to this
author, and the record proves it.** §9.3.2's prediction landed in every cell
because the arms are deterministic over a small input space, so the outcome of
any candidate design is computable before it is run. Randomising cosmetic
parameters yields zero variance (this corpus is the existence proof);
randomising decision-relevant ones requires choosing a distribution over them,
which is the same discretion one level up. When the designer can compute
outcomes from designs, blindness is not an integrity tool that exists. The ones
that do exist are **disclosure, prediction committed in advance, mechanical
generation, and a second decision-maker** — and this document uses all four
rather than claiming an innocence it cannot have.

**Correction (2026-07-30, adversarial review): the heading's "can never finish"
is a policy, not a theorem, and is retained only as the former.** A
pre-registered generative distribution over decision-relevant parameters,
mechanically sampled, would give the bootstrap a population in exactly the same
*conditional* sense this section grants the IX driver — and this section already
concedes the driver "relocates authorship rather than escaping it". Once that is
conceded, the difference between item 3 and item 4 is **surface area and task
fidelity, not possible versus impossible**. The measured degeneracy below is
real and the rule that follows is deliberate; the impossibility framing
overreached and is withdrawn.

**Standing rule, adopted here: no dual-rule test (§6) and no keep/kill verdict
(§8) may ever be computed on an authored fixture.** Authored fixtures are
instrument characterisation only. This is not a comment on the two corpora's
quality; it follows from there being no population to bootstrap over. Both
corpora are **frozen** and retained permanently as regression instruments:

* `*-isolation.json` holds the asserted `HexValue` fixed and varies only
  corroboration. It is the cleanest probe in the repo of whether a
  representation *sees* replication, and it produced the finding that SL's
  discrimination (`b=0.647` vs `0.550`) exists in the posterior and is
  discarded at the `b > 0.7` gate.
* `*-task.json` bundles asserted confidence with reproduction per §2's trigger.

Their findings stand and need no statistics, being deterministic and
universal across the arms — in particular G3, where **no arm carries provenance
withdrawal into its act/abstain decision**.

**Consequence for §1.** This was foreseeable from the document's own first
page, which exists to escape "fixture replay authored by us". Authoring six
more fixtures was always in tension with that premise. §2 defines the task as
*recorded* IX sessions with injected perturbations, and item 4 was pre-registered
as blocking from day one — so nothing is abandoned by declining to author a
third corpus. **Item 4 is now the sole remaining blocker.** Between-trace
variance must come from harness randomness — which perturbations are injected,
their magnitudes, seeds, run counts, event ordering — that is, from §2's task,
not from an author's pen.

**What this does not claim.** The driver relocates authorship rather than
escaping it: the perturbation-injection distribution is still chosen by us. What
shrinks is the surface — from "which 54 decisions" to "which distribution" —
and a distribution is disclosable in a way that 54 hand-placed decisions are
not. Any eventual §8 verdict is therefore **conditional on the declared task
distribution, not on the world**, and must be reported in those terms. That
distribution is the one remaining pre-registration decision and is an owner
call (§9.3.3), to be taken on a drafted spec with the arm-boundary straddles
enumerated, never on an open question.

### 9.5 The two open arm definitions, pinned — and §8 clause 1 measured

**Amendment (2026-07-30).** §9.4 left item 4 as the sole blocker, but building
it required two definitions §4 never supplied: what `experimental` is, and what
`IX-unassisted` does. Both are pinned here. Neither is a preference: each is
determined by text already committed, and the reasoning is given so a reader can
disagree with the derivation rather than only with the conclusion.

**`experimental` := the shipped `RecencyDecay` `CognitiveLoop`.** §1 asks
whether *"routing a recorded IX autoresearch session through Hari"* helps.
"Hari", as shipped, is the default `PriorityModel` — `RecencyDecay` per ADR-0001.
`Lie` is excluded on evidence (§4) and `SubjectiveLogic` is named as the *cheap
baseline* the substrate must not lose to, so neither can also be the claim. That
leaves exactly one candidate. This does **not** change any default;
`test_priority_model_default_is_recency_decay` stays green.

**`IX-unassisted` := pass-through acceptance.** §4 defines it as "recorded IX
behavior with no Hari policy applied". A policy layer is what decides whether to
act on a claim, withhold, or escalate; remove it and what remains is taking
every report at face value. So the arm accepts every proposition-bearing
assertion and takes no substantive action on anything else
(`crates/hari-core/src/unassisted.rs`).

The temptation is to define this baseline weakly — thrashing, or accepting at
random — because a weak null is easy to beat. That would be instrument-driven
design of exactly the kind §10 forbids, applied to the **comparator** instead of
the metric. Pass-through is the strongest honest reading of "no policy".

One definitional choice is disclosed rather than tuned: a `retraction` is an
instruction, not a claim to decide about, so this arm emits nothing on one. The
hexavalent arms emit `Retry`. That is the *only* place the shipped default
diverges from the baseline, and matching the arm here would have meant fitting
the comparator to the thing it measures.

#### §8 clause 1 is structurally zero for the shipped substrate

Measured, not predicted — and the first prediction was **wrong**, caught by its
own test. The claim written first was flat decision-identity across both
hexavalent arms; `theorem_the_default_arm_never_withholds_where_the_null_baseline_commits`
failed on first run against `cognition_divergence`. The true relationship:

* **On every one of the corpus's 83 claim assertions, `RecencyDecay` acts
  exactly where `IX-unassisted` acts.** Zero divergences. `HexValue` selects
  *which* action — `Accept` / `Investigate` / `Escalate`, all acting under §5.1
  — and never *whether*.
* `Lie` withholds on 18 claim assertions, every one via cycle-age decay rather
  than evidence. Not a §4 arm, so no verdict moves; it is what makes the line
  above a measurement rather than a tautology.

On both §9.3 corpora, all four arms score:

| | IX-unassisted | RecencyDecay | Lie | SubjectiveLogic |
|---|---|---|---|---|
| isolation | 0.333 | 0.333 | 0.333 | 0.000 |
| task | 0.333 | 0.333 | 0.333 | **0.667** |

`IX-unassisted` is identical to the shipped default on every ground of both
corpora. **So §8 clause 1 — "experimental beats `IX-unassisted` on Paired
Accuracy" — evaluates to exactly zero.**

**What that means, stated carefully.** The substrate's policy layer changes
*what* Hari commits to, not *whether* it commits. Paired Accuracy measures only
the latter, so on this metric the shipped substrate is indistinguishable from
naive acceptance. It is a **negative result of the same class as the Phase-5
`Lie` verdict**, and §8 already commits to publishing that class of result.

**What it does not mean.** Three limits, all load-bearing:

1. This is measured on **authored fixtures**, which §9.4's standing rule bars
   from producing a §6 dual-rule test or a §8 verdict. It is instrument
   characterisation. The eval has still not run.
2. It is a statement about the **primary metric**, not about the substrate.
   `HexValue`, contradiction preservation, provenance and revision may well earn
   their keep on *what* is committed — `false_acceptance_count` is where the
   Phase-5 comparison lived, and it is a §5.2 secondary here. §5.2 forbids
   substituting a secondary for the primary in the conclusion, and that
   prohibition binds in this direction too.
3. It sharpens rather than removes the need for item 4. A driver-recorded corpus
   could still separate the arms — but only if it contains decisions where
   *whether* to act is genuinely in question, which is precisely what the
   reporter model and perturbation distribution must be designed to produce.

**Consequence for §8.** With clause 1 at zero for the shipped default, the
kill/keep rule as written resolves to **KILL** unless the experimental arm is
changed or the metric is. Both are owner calls, and both must be made *before*
a driver corpus is recorded, not after. This section records that the rule now
has a determinate answer on every instrument that exists — which is the outcome
pre-registration is for, and the reason it was written before the data.

### 9.6 The driver cannot rescue §8 clause 1 — established, not argued

§9.5 measured clause 1 at zero on the eight authored fixtures. The natural hope
is that item 4 fixes this: that a driver-recorded corpus, with real
perturbations, real noise and a real reporter model, would produce decisions
where *whether* to act is genuinely in question. **It cannot**, and this is
worth knowing before the driver is built rather than after.

The mechanism is not statistical. `RecencyDecay`'s act/abstain decision is a
pure function of `state.cycle - event.cycle`: it withholds only once
`exp(-λ·age) < θ_wait`, i.e. age ≥ 12. A recorder stamps events as they arrive,
so age is ~0 and the arm always acts — which is exactly what pass-through does.
Evidence, source, asserted value and payload content are all irrelevant to it.
No distribution over any of them changes the decision, because none of them is
an input to it.

Pinned over **400 randomly composed traces** restamped so `cycle` tracks
position, by
`theorem_no_naturally_stamped_trace_separates_the_default_arm_from_the_null_baseline`
— generated rather than authored, so the result cannot be an artifact of eight
fixtures someone chose. Zero separations across ~1,000 claim assertions.

The companion `probe_a_deliberately_stale_stamping_does_separate_them` is the
positive control: stamp every event `cycle: 1`, let age exceed 12, and the arm
*does* withhold where pass-through commits. So "never separates" is a
measurement rather than a broken comparison — and the only stamping that
separates them is the one §9 item 3 already identified as a tautology, a fixture
an author must deliberately construct and no recorder produces.

**Therefore the routes to a non-zero clause 1 are exactly three**, and building
the driver opens no fourth:

1. **Change the experimental arm.** `SubjectiveLogic` does separate — 0.667 vs
   0.333 on the task corpus. But §4 names it the *cheap baseline*, so promoting
   it to `experimental` changes the question from "does the substrate help" to
   "does opinion fusion help", and leaves §8 clause 2 comparing SL to itself.
2. **Change §5.1's taxonomy.** The rejected alternative — *act* = `Accept`,
   *abstain* = `Wait`/`Investigate`/`Escalate` — makes act/abstain a function of
   `HexValue` and therefore of evidence. §9.3 rejected it on three measured
   grounds, and adopting it now, after seeing that the current taxonomy yields
   zero, would be precisely the outcome-driven amendment §10 forbids. If it is
   adopted, that ordering must be disclosed.
3. **Accept KILL** and publish, as §8 already commits to.

**This does not make item 4 pointless.** The driver is still what supplies §6 a
population, still what makes the abstention and calibration measures
exercisable, and still the only route to a §5.2 comparison — including
`false_acceptance_count`, where the Phase-5 comparison actually lived and where
the hexavalent machinery may well earn its keep. What §9.6 removes is one
specific hope: that recording data would change the primary metric's verdict for
the shipped arm. It would not.

### 9.7 `false_acceptance_count` is arm-biased too — and it closes §9.6's escape hatch

§9.6 left one hope standing: that `false_acceptance_count`, the §5.2 secondary
where the Phase-5 comparison actually lived, is *"where the hexavalent machinery
may well earn its keep"*. Two findings, and the hatch closes.

**The intrinsic counter carries the §5.3 defect, pointing the other way.**
`accept_was_invalidated` charges an `Accept` by three routes: a later
`Retraction`/`Correction` in the trace (arm-independent), the proposition ending
`Contradictory`, or a polarity flip. The last two read *the arm's own*
`final_beliefs`. Measured across `fixtures/ix/`:

| arm | `false_acceptance_count` | charged via its own `Contradictory` |
|---|---|---|
| `RecencyDecay` | 12 | **5** |
| `Lie` | 8 | **2** |
| `SubjectiveLogic` | 3 | **0** |

SL is never charged that way because it resolves contradictions to a
probability. The hexavalent arms are charged because they **preserve** them —
which `hari-lattice` documents as a deliberate design choice, not a failure to
converge. **The metric penalises the substrate's defining feature, and only for
the arms that have it.**

*This does not overturn Phase 5.* Removing the own-`Contradictory` charges leaves
`Lie` at 6 against SL's 3: the margin is partly artifactual, the sign is not. The
existing verdict does not need reopening on this basis alone, and
`score_false_acceptances` now exists so future comparisons are not exposed to it.

**Scored arm-independently, the substrate again equals naive acceptance.** On the
task corpus, against authored `ClaimLabel`s:

| | IX-unassisted | RecencyDecay | Lie | SubjectiveLogic |
|---|---|---|---|---|
| false acceptances | 54 | **54** | **54** | **36** |
| warranted accepts | 108 | 108 | 108 | 108 |

The hexavalent arms commit to all 54 claims that did not hold up — exactly what
pass-through does. SL commits to 36. Its 18 declined commitments are **all**
false ones: its warranted count is identical at 108, so on this corpus its
caution is perfectly targeted rather than merely more cautious.

*The isolation corpus is not evidence of the same thing.* SL scores 0 false
acceptances there only because it accepts nothing at all — the degenerate
always-`Wait` §9.3.2 records. A zero earned by never committing is not a virtue,
and pooling the two corpora would launder it into one.

**Consequence.** Both the §5.1 primary and the §5.2 secondary where Phase 5 was
decided now show the shipped substrate indistinguishable from naive acceptance,
with SL ahead on both. §9.6's three routes are unchanged, but the case for route
3 (**accept KILL and publish**) is materially stronger than it was, because the
metric that was expected to rescue the substrate does not.

*Still not a verdict.* This is authored-fixture instrument characterisation,
which §9.4's standing rule bars from producing a §6 or §8 conclusion. What it
establishes is that the driver should not be built in the expectation of a
different answer on either metric.


### 9.8 The §6 aggregator is built — and it changes no outcome

**Amendment (2026-08-09).** §6 described a trace-clustered paired bootstrap in
the present tense and then corrected itself on 2026-07-30: *"No bootstrap
aggregator exists anywhere in the workspace."* It was the last missing §6
prerequisite, and it mattered because §8 applies the kill/keep rule
*"mechanically to the bootstrap output"* — of which there was none. It now
exists: `paired_eval::bootstrap_paired_difference`, a pure function taking
per-trace decision records and returning the interval, the p-value and the dual
rule, with B = 10,000, resampling **traces** and never decisions, reachable as
`replay --paired --compare3 --bootstrap`.

**The seed is a constant in the source and is deliberately not settable from the
command line.** §6 requires a re-run to reproduce the interval exactly; a
tunable seed would also make the interval *shoppable*, which is the failure mode
this document exists to prevent. It is `20260728` — this file's date — and is
echoed into every result so a reader can reproduce the number. The generator is
a vendored SplitMix64 rather than a crate, so a dependency upgrade cannot move a
published interval.

**Sequencing, disclosed rather than glossed.** This was built *after* §9.5 and
§9.7 measured clause 1 at zero. It cannot be claimed blind, and §9.4's account
of why blindness is unavailable to this author applies here too. What limits the
damage is that no resampling scheme can move this particular outcome: the
per-decision difference between the shipped arm and the null baseline is
**identically zero at every one of the 54 pairs**, so every one of the 10,000
resamples returns exactly 0.0 and the interval is `[0.0, 0.0]` with p = 1.0. The
aggregator formalises a zero rather than discovering one.

**Two committed rules are now enforced in code rather than remembered.**

* §9.4's standing rule: `CorpusProvenance::Authored` yields
  `KillKeepVerdict::WithheldByStandingRule`, whatever the clauses say. A passing
  dual rule on an authored corpus still produces no verdict — pinned by
  `an_authored_corpus_withholds_the_verdict_while_still_reporting_the_clauses`.
* §9.3.2's non-pooling rule: `pooling_violation` refuses a corpus mixing
  `*-isolation` and `*-task`, which would otherwise launder SL's degenerate
  always-`Wait` zero into its task result.

**A §7 finding, and it is not the reassuring reading.** §7 requires the report to
state the realized ICC and effective *n*. On the task corpus, comparing
`SubjectiveLogic` to the shipped arm, the aggregator returns **ICC = 0, design
effect = 1, effective n = 54**. That is arithmetically correct and would be badly
misread as "clustering costs nothing here". The ICC design-effect correction is a
correction for **between**-trace correlation, and this corpus has none — because
its six traces are clones (§9.4). The degeneracy is replication *within* the
design, which the ICC cannot see at all. The output therefore carries an explicit
`effective_n_overstates` flag, set whenever between-cluster variance is zero, and
pinned by `a_corpus_of_clones_flags_that_its_effective_n_overstates`. §7's
instruction to report the realized ICC stands; the number it produces on a
degenerate corpus is not a measure of that corpus's information content.

**Nothing in §5–§8 changed, and no eval outcome was newly inspected.** Every
number the aggregator produces reproduces §9.3.3, §9.5 and §9.7 exactly. §8
clause 2 remains `undefined` — per-arm calibration still requires each arm to
emit forecasts from its own posterior (§9 item 2) — and an undefined clause
cannot satisfy KEEP, so **KEEP is unreachable on every instrument that exists**.
The report is `2026-08-09-ix-eval-paired-bootstrap-report.md`.

### 9.9 Item 4 lands — and §7's pre-unblinding obligation is disclosed as unfulfilled

**Amendment (2026-08-09, after independent review of the §9.8 branch.)** Nothing
here changes a published number. §9.3.3, §9.5, §9.7, §9.8 and the report's §3–§5
stand exactly as recorded.

**The driver exists.** `clients/ix_reference/paired_driver.py` draws traces from
a declared generative spec (`flaky-vs-real/v1`, one dict in one file), writes
paired fixtures, and hands the corpus to the existing
`replay --paired --compare3 --bootstrap` boundary in one command — user story 12,
and §9.4's item 4. It generates rather than records a live IX session, which is
the route §9.4's own 2026-07-30 correction names: *"A pre-registered generative
distribution over decision-relevant parameters, mechanically sampled, would give
the bootstrap a population in exactly the same conditional sense this section
grants the IX driver."*

**Provenance is now a fact about the corpus.** §9.4 said the standing rule is
*"enforced rather than remembered"*. As first built it was enforced against
forgetfulness only: `--corpus recorded` over the six hand-authored task fixtures
was accepted and printed `provenance: driver_recorded`, with nothing inspecting
the fixture. Each generated fixture now carries a `provenance` stamp including a
digest of its trace; `hari-core` recomputes the digest and refuses a mismatch,
and refuses `--corpus recorded` over unstamped fixtures outright. This is
tamper-evidence, not tamper-proofing — the failure mode it closes is the one that
actually happened, a corpus declared recorded because the operator believed it
was.

**§9.6 is confirmed on recorded data, not merely predicted.** On the driver
corpus the shipped arm and the null baseline agree at **every** decision:
difference 0.0000, CI [0.0000, 0.0000], p = 1.0000, `every_delta_is_zero`. Clause
1 fails, clause 2 is undefined, and the mechanical verdict is **KILL**. §9.6
pinned this over 400 generated traces before the driver existed; the driver
produced exactly the corpus §9.6 said it would.

**What the driver did change.** The cheap comparison on the driver corpus is the
first non-degenerate one in this document's history: 3 distinct cluster
signatures, realized **ICC 0.7222**, design effect 2.4444, effective *n*
**7.36** against 18 raw pairs. §7's clustering correction finally bites, which is
what §7 was written for and what no authored corpus could ever show.

**§7's pre-unblinding obligation was not met, and this is the disclosure.** §7
commits: *"the ICC will be estimated from the first recorded traces, and the MDE
recomputed and committed as an amendment (§10) before any outcome is
inspected."* No such amendment was made before §9.8's numbers were inspected.

* Until this amendment there were **no recorded traces**, so the obligation had
  nothing to attach to — and §9.4's standing rule independently barred every §6
  and §8 reading of the authored corpora, which is why no verdict was ever
  exposed to the gap. That is a defence of the *outcome*, not of the *record*:
  §7 does not say the obligation is conditional, and §9.8 was where it should
  have been said. It is said here.
* The obligation **cannot now be met blind for this branch**, and claiming
  otherwise would be worse than the omission. The driver was written after §9.5,
  §9.7 and §9.8 measured clause 1 at zero, and its corpus was generated and read
  in the same sitting. §9.4's account of why blindness is unavailable to this
  author applies unchanged.
* **The recomputed MDE, disclosed as post-hoc.** At effective *n* ≈ 7 for the
  discriminating comparison, the detectable difference in Paired Accuracy is far
  above §7's 0.20 threshold — §7's table already puts ~31 effective decisions at
  the edge of usefulness. §7's own instruction therefore governs: *"add traces or
  abandon the run, not report an underpowered null as evidence of
  equivalence."* **Consequence, adopted here: no §8 verdict computed on the
  current driver corpus may be read as a result.** The KILL it produces
  demonstrates that the rule is mechanically reachable; it does not settle §8.
* The obligation itself **survives unspent**. Before any driver-recorded corpus
  is used for a verdict, the sequence §7 specifies must actually run: fix the
  corpus size, estimate ICC from it, recompute the MDE, commit it as a §10
  amendment, and only then inspect outcomes. The distribution ratification §9.4
  names as the outstanding owner call is the natural moment for it.

**§7's reporting obligation, and where it is silently unmeetable.** §7 also
requires the report to state the achieved effective *n* and realized ICC. For the
**primary** comparison — clause 1 — they are undefined on every corpus measured
so far, because all deltas are zero and the one-way ANOVA's denominator vanishes;
the three fields are `skip_serializing_if` and so are absent from the JSON rather
than reported as zero. The first cut of the report stated them only for the
comparison that has them (SL vs the shipped arm) without saying why the primary
carried none. The report now says so at §3.1.

**§11's falsifiers, for completeness.** §11's first two clauses are numerically
met on both authored corpora — `RecencyDecay` scores 0.3333 Paired Accuracy
(≤ 50%) and shows no significant gain over `IX-unassisted` under the dual rule.
§9.4's standing rule bars reading that as falsification on an authored corpus.
Recorded here because the silence ran *against* the substrate and should not be
mistaken for an omission of an inconvenient fact.

**Conditioned Abstention Rate is implemented** (§5.2's fourth metric, PRD story
11), per policy, at the replay boundary. One property is worth pinning in this
document because it constrains how the metric may be reported: under §5.1's
adopted taxonomy, abstention rate *conditioned on abstaining being correct* is
numerically identical to Abstain Accuracy. The two may never be presented as
independent corroboration. The metric's content over §5.2's existing three is the
unwarranted-abstention leg and the discrimination between the legs — which is
what exposes `SubjectiveLogic`'s always-`Wait` degeneracy on the isolation corpus
as a 0.0000 discrimination behind a 1.0000 Abstain Accuracy.

## 10. Amendment policy

This document may be amended **only** by a git commit that (a) states what
changed, (b) states why, and (c) is made **before** the affected outcome is
inspected. The commit history is the audit trail.

**Caveat on that audit trail (added 2026-07-30).** Git timestamps are
author-controlled: `79ad578` carries an exactly-round `09:00:00` author and
committer date, i.e. a hand-set time. The audit trail is therefore the commit
**order and content**, which is what §9.3.1's amended note relies on — never the
wall-clock times, and never a claim about when work was performed.

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
