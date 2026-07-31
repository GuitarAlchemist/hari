# The §8 case as it stands — an interim assessment, not the verdict

**Status:** INTERIM. **This is not the §8 conclusion and must not be cited as
one.**
**Pre-registration:** `2026-07-28-ix-eval-preregistration.md` (binding; committed
on `main`)
**Issue:** [#35](https://github.com/GuitarAlchemist/hari/issues/35)
**Date:** 2026-07-30 · **Author:** Stephane Pareilleux
**Repo state:** `main` @ `05f53e7`, 359 tests passing

---

## 0. Why this document exists, and what it deliberately is not

§8's kill/keep rule is applied *mechanically to the bootstrap output*. That
bootstrap cannot run: §9.4 established that authored fixtures give it no
population, and adopted a standing rule that **no §6 dual-rule test and no §8
verdict may ever be computed on an authored fixture**. That rule is not
suspended here.

So this is an *interim assessment*: what every instrument that currently exists
says, assembled in one place, with the limits attached. It is written now
because the measurements have converged on an answer, and leaving that answer
implicit across seven pre-registration subsections while continuing to build
instruments would be a way of not saying it.

Every figure below is generated from the committed fixtures by the committed
binary. None is transcribed by hand — an earlier commit in this series
introduced a factual error precisely that way.

## 1. The question

> Does routing a recorded IX autoresearch session through Hari produce
> measurably better Accept/Wait/Escalate recommendations than the same session
> without Hari — and does it beat the cheap baseline, not just the null one?

Arms, pinned in §9.5: `experimental` := the shipped `RecencyDecay`
`CognitiveLoop`. `IX-unassisted` := pass-through acceptance — every claim taken
at face value, nothing else decided.

## 2. What every instrument says

### 2.1 The primary metric (§5.1 Paired Accuracy)

| corpus | IX-unassisted | RecencyDecay | Lie | SubjectiveLogic |
|---|---|---|---|---|
| isolation | 0.333 | **0.333** | 0.333 | 0.000 |
| task | 0.333 | **0.333** | 0.333 | **0.667** |

Per ground, task corpus — G1 replication / G2 commitability / G3 withdrawn
basis:

| arm | G1 | G2 | G3 |
|---|---|---|---|
| IX-unassisted | 0.000 | 1.000 | 0.000 |
| RecencyDecay | 0.000 | 1.000 | 0.000 |
| SubjectiveLogic | **1.000** | 1.000 | 0.000 |

### 2.2 The secondary where Phase 5 was decided (§5.2 false acceptances)

Scored arm-independently against authored `ClaimLabel`s (§9.7), task corpus:

| | IX-unassisted | RecencyDecay | Lie | SubjectiveLogic |
|---|---|---|---|---|
| false acceptances | 54 | **54** | 54 | **36** |
| warranted accepts | 108 | 108 | 108 | 108 |

SL declines 18 commitments and **all 18 are false ones** — its warranted count
is unchanged. On this corpus its caution is targeted, not merely greater.

### 2.3 The caution tax (§5.3 false rejections)

| corpus | IX-unassisted | RecencyDecay | Lie | SubjectiveLogic |
|---|---|---|---|---|
| isolation | 0 | 0 | 0 | **108** (54 excused) |
| task | 0 | 0 | 0 | **0** (18 excused) |

SL is **not** disqualified on the task corpus. Its isolation-corpus 108 is the
degenerate always-`Wait` (§9.3.2) and is not evidence of caution — an arm that
accepts nothing cannot be praised for what it declines.

### 2.4 The structural results

* **§9.5.** On all 83 claim assertions in `fixtures/ix/`, `RecencyDecay` acts
  exactly where `IX-unassisted` acts. Zero divergences. `HexValue` selects
  *which* action, never *whether*.
* **§9.6.** Over **400 generated traces** stamped as a recorder would stamp
  them: zero separations. With a positive control confirming the comparison is
  live under deliberately stale stamping.
* **§9.7.** The intrinsic `false_acceptance_count` charges hexavalent arms for
  their own `Contradictory` verdicts (5 of `RecencyDecay`'s 12, 2 of `Lie`'s 8,
  0 of SL's 3) — it penalises contradiction preservation, which is a documented
  design choice rather than a failure to converge.

## 3. The case, stated plainly

**§8 clause 1 — "experimental beats `IX-unassisted` on Paired Accuracy" —
evaluates to exactly zero, on every instrument that exists, and no recorded
corpus can change it.** The shipped substrate's policy layer changes *what*
Hari commits to, never *whether* it commits; the primary metric measures only
the latter.

**The secondary that was expected to rescue it does not.** On arm-independent
false acceptances the shipped arm is again identical to pass-through, 54 to 54,
while the cheap baseline reaches 36.

So the pre-registered conclusion §8 names is live:

> ~600 lines of Subjective Logic already deliver the benefit; the additional
> substrate does not.

On the evidence assembled here, **the case points to KILL**.

## 4. The limits — every one of which is load-bearing

1. **This is not the verdict.** No bootstrap, no CI, no p-value, no dual rule.
   §9.4's standing rule bars a §8 conclusion on authored fixtures and is not
   suspended. A reader who cites §3 of this document as "the eval result" is
   misusing it.
2. **The corpora are degenerate.** 54 pairs, but three distinct decision
   situations replicated eighteen times (§9.4). Effective *n* is 3, not 54.
3. **Two of three grounds are unanimous.** G2 is 1.000 for everything with a
   policy; G3 is 0.000 for everything. **The entire cross-arm difference lives
   in G1**, one ground of three, which the §9.3.1 mix declared in advance would
   behave exactly this way. A result concentrated in a single pre-declared
   ground is weaker than an aggregate makes it look.
4. **"Better" here means better on two metrics.** It does not mean SL is a
   better substrate. Contradiction preservation, provenance, revision and
   relation propagation are not measured by either metric, and §9.7 shows one
   of them is actively *penalised* by the instrument that exists.
5. **`Lie` is not an arm** (§4) and appears above only as context.
6. **Phase 5 is not overturned** (§9.7). Removing the artifactual charges leaves
   `Lie` at 6 against SL's 3 — the margin narrows, the sign holds.

## 5. What would change the conclusion

Three routes, from §9.6, unchanged by anything here:

1. **Change the experimental arm** to `SubjectiveLogic`. But §4 names SL the
   *cheap baseline*, so this changes the question from "does the substrate
   help" to "does opinion fusion help", and leaves §8 clause 2 comparing SL to
   itself.
2. **Change §5.1's taxonomy** so act/abstain depends on `HexValue`, and
   therefore on evidence. §9.3 rejected the alternative on three measured
   grounds. Adopting it *now* — after these numbers — would be the
   outcome-driven amendment §10 exists to prevent. It may still be the right
   call; if taken, **the ordering must be disclosed at the claim**, not in a
   footnote.
3. **Accept KILL and publish**, as §8 already commits to and as this project
   has done once before.

A fourth thing is worth building regardless of which route is taken: **a metric
that measures what the substrate actually does.** Every instrument here grades
*whether to act*. Nothing grades whether the belief state that results is a
better representation of a contradictory, revisable, partially-retracted
evidence base — which is what `hari-lattice` was built for. That is not a
rescue for §8, which is pre-registered and must be answered as written. It is
the observation that #35 may have pre-registered the wrong question about this
substrate, and that finding out is itself worth the eval having run.

## 6. Recommendation

Take route 3 unless the owner has a reason for 1 or 2. Publish the negative
result, then decide separately whether a second pre-registration — aimed at
belief-state quality rather than act/abstain — is worth writing.

Do **not** build the §9 item 4 driver in the expectation of a different answer
on either metric (§9.6, §9.7). It remains worth building for §6's population,
for the calibration criterion, and for §5.2 measures that no authored corpus can
exercise — but not as a rescue.
