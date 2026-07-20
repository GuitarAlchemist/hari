# Cross-session source reliability & trust calibration — design

**Status: DRAFT (max_autonomy: draft). Issue: [#14][14] (parent epic #12).
Date: 2026-07-20.** Fulfils issue #14's `source_reliability_design` and
`replay_fixture_plan` evidence. The tracer-bullet slice is implemented and
tested (`crates/hari-core/src/source_reliability.rs`,
`tests/source_reliability_e2e.rs`); the trust-*calibration* integration
(feeding entrenchment back into consensus / evidence-recompute) is deliberately
**not** built here and awaits owner review — see §7.

Companion artifacts: contract
`docs/contracts/source-reliability-summary.contract.md` (v0.1 DRAFT); examples
`fixtures/reliability/`.

## 1. Problem

Hari already reasons with trust *within* a session: `TrustModel::RoleWeighted`
weights consensus by an agent's **configured** `self_trust`. What it cannot do
is *learn* which sources — agents, runners, benchmarks, tools — actually produce
useful evidence, and carry that judgement across sessions. Trust is declared,
never earned.

Two independent needs converge on the same object:

1. **Issue #14** wants trust "earned by outcomes, not only configured by role",
   with metrics that separate correctness, usefulness, calibration, and
   minority-report value, persisted as auditable cross-session summaries.
2. **The compounding-strategy synthesis** (`docs/research/2026-07-20-compounding-strategy.md`
   §5) identifies the *same* ledger as the missing **epistemic-entrenchment
   ordering**: foundationalist evidence-recompute can decide whether a derived
   belief is still supported, but it cannot rank **conflicting base evidence**.
   AGM entrenchment is that ranking, and "source reliability = entrenchment".
   This is the principled reason to feed the ledger rather than let it starve
   like G2's did.

So the design target is: an append-only, cross-session record of how each
source's claims *turned out*, aggregated into a per-source reliability summary
whose **entrenchment ordering** a future consumer (conflicting-base-evidence
resolution in the substrate) can gate on — surfaced and auditable, never a
hidden global mutable trust knob.

## 2. Relation to G2 (`reliability.rs`) — generalise, don't duplicate

Giskard Track G2 (`crates/hari-core/src/reliability.rs`) already grades
**agents** by ingesting GA's `pr-grade-v1` cards (an *external, human/LLM-graded*
alignment signal) and reporting per-agent × task-class precision against a
`pooled` baseline. This module is the **same shape one level more general**:

| | G2 `reliability.rs` | #14 `source_reliability.rs` |
|---|---|---|
| Graded unit | agent (PR author) | **any source** (`ResearchEvent.source`) |
| Outcome signal | GA `pr-grade-v1.alignment` (external grade) | **internal fate** of the claim in a replayed trace |
| Grain | one card = one PR | one row = one source × one session |
| Baseline | `pooled` (all agents) | `pooled` (all sources) — same doctrine |
| Smoothing | prior weight 2.0, mean 0.5 | **same constants**, so the two compose |
| Consumer | none yet (honest) | entrenchment ordering, surfaced not applied |

They are **complementary, not redundant**: G2 scores agents on *externally
graded* PR outcomes; #14 scores arbitrary sources on *replay-internal* outcomes.
A future unifier could treat a G2 per-agent precision as a Bayesian **prior**
for that agent's #14 source precision (both are `[0,1]` precision scores under
the identical prior). This design keeps them as separate reports with a shared
prior and shared vocabulary rather than merging prematurely (the roadmap's
A/B-first discipline: prove the source view beats pooled before entangling it
with G2).

The false-acceptance predicate is now **physically shared**: the aggregate
`false_acceptance_count` metric and per-source `false_acceptances` both call
`accept_was_invalidated` in `lib.rs`, so the two counts can never drift.

## 3. Source types and channels

Issue #14 lists candidate source types: `agent`, `model`, `benchmark`, `repo`,
`tool`, `research_digest`, `human_review`, `streeling_learning`,
`ix_autoresearch_run`. The current trace boundary (`ResearchEvent`) exposes only
a `source` **string** and a payload **channel** (which mechanism the source
spoke through: `belief_update` / `experiment_result` / `agent_vote`). It does
**not** carry a producer-declared source *type*.

Honest decision: `SourceOutcomeRecord.source_type` is an **additive, reserved**
field defaulting to `unknown` (the same degradation discipline G2 uses for its
`agent` field). We do **not** guess the taxonomy from the string or channel —
that would manufacture false precision. Populating it richly is an IX-side
producer change (open question, §7). What *is* derivable today — the channel —
is retained implicitly via which tally a claim lands in.

## 4. Metrics

The tracer computes the correctness/usefulness half of issue #14's metric menu,
which is what replay outcomes can honestly support today. Each is defined so it
is falsifiable from a trace alone.

Per source, per session (`SourceOutcomeRecord`, additive tallies):

- **`claims`** — belief-bearing events (`belief_update` / `experiment_result` /
  `agent_vote`) the source emitted. Denominator context.
- **`accepted`** — `Accept` actions the loop emitted on the source's events.
- **`false_acceptances`** — of `accepted`, those the rest of the trace later
  invalidated: a subsequent `Retraction` of the proposition, a final value of
  `Contradictory`, or a polarity flip (accepted True/Probable → final
  Doubtful/False, or vice-versa). Shared predicate with the aggregate metric.
- **`escalations`** — `Escalate` actions on the source's events, attributed to
  the event's proposition.
- **`false_escalations`** — of `escalations`, those whose proposition ended
  **True/Probable** (we alarmed, but it turned out fine). A durable
  Contradictory/False/Doubtful end-state means the alarm was *warranted*.

Aggregated across sessions (`SourceReliabilityEntry`, summed then derived):

- **`accepted_claim_precision`** = `1 − false_acceptances / accepted` — issue
  #14's headline metric. `None` when `accepted == 0` (never NaN).
- **`smoothed_precision`** = `((accepted − false_acceptances) + 2.0·0.5) /
  (accepted + 2.0)` — small samples pinned to the neutral prior; defined for
  every bucket including empty ones. This is what the ordering ranks on.
- **`false_escalation_rate`** = `false_escalations / escalations` — the
  *usefulness* axis (a source that cries wolf).

Mapping to issue #14's named metrics:

| Issue metric | Status here | Note |
|---|---|---|
| `accepted_claim_precision` | ✅ implemented | headline |
| `false_acceptance_contribution` | ✅ `false_acceptances` | direct-claim attribution (see §6 limit) |
| `false_escalation_contribution` | ✅ `false_escalations` | proxy: ended True/Probable |
| `contradiction_recovery_helpfulness` | ⛔ deferred | needs per-source propagation attribution |
| `recommendation_followed_success_rate` | ⛔ deferred | needs a downstream "followed?" signal IX doesn't emit yet |
| `calibration_error` | ⛔ deferred | needs numeric confidences per claim (forecast ledger territory — J2) |
| `staleness_sensitivity` | ⛔ deferred | needs multi-session temporal weighting |
| `confidence_vs_outcome_delta` | ⛔ deferred | as calibration_error |

**Minority-report protection** (issue non-goal "do not punish minority evidence
merely because it disagrees"): disagreement is *never* itself penalised. Only a
claim that was **Accepted and then invalidated** counts against a source; a
lone dissenter who turns out *right* (the belief resolves their way) records
**zero** false acceptances and, if they escalated, a **warranted** (non-false)
escalation. The conflicting_benchmark fixture demonstrates exactly this: the
critic who dissented is not penalised; the evaluator whose accepted claim was
retracted is.

## 5. Persistence

Append-only JSONL, one file per emission day, under
`HARI_STATE_DIR/source-reliability/` (default `state/source-reliability/`) —
byte-for-byte the same ledger discipline as `forecast.rs` / `operator_model.rs`:

- Rows are **immutable**; a new session appends new rows, never rewrites old
  ones (append-only lineage — compounding-strategy §4.6; resists the
  self-rewriting-consolidation failure mode).
- `recorded_at` is **injected** (CLI `--now`, defaults to wall clock) and
  validated canonical UTC at the write boundary; the day-file name is its date
  prefix, so the ledger sorts lexicographically = chronologically.
- Malformed lines are skipped with a count, never fatal; a missing directory is
  an empty ledger.
- No database (issue non-goal "do not require persistent database
  infrastructure yet").

Cross-session aggregation is just summation: `pooled` and each `by_source`
entry sum the tallies of every matching row, so adding a session is monotone and
order-independent.

## 6. Honest limits

- **Direct-claim attribution only.** A source is graded on claims it *directly*
  made, not on its *contribution* to a derived belief via propagation. True
  `*_contribution` semantics (crediting/blaming a source for a belief reached
  through the relation graph) needs per-derivation provenance attribution — the
  `Derivation.contributions` data exists but wiring it into blame assignment is
  a later slice. The current `false_acceptances` is the direct-claim floor of
  that contribution.
- **`false_escalations` is a proxy.** "Ended True/Probable" approximates
  "unwarranted alarm"; a belief that ends `Unknown` (e.g. cleared by retraction)
  is treated as neither warranted nor false. Flagged, pinnable, revisable.
- **Escalate carries no proposition** in the `Action` enum, so escalation is
  attributed via the *event's* proposition; an escalation on a goal/relation
  event (no single proposition) is not counted.
- **Ossification caveat** (compounding-strategy §3, SSGM): entrenchment must
  stay revisable. Because rows are per-session and precision is re-summed every
  `report`, a source that improves is not permanently condemned — but there is
  no *decay* weighting yet (`staleness_sensitivity`, deferred), so very old
  outcomes count equally with recent ones. A consumer must not treat the
  ordering as frozen truth.

## 7. What is deliberately NOT built (owner review gate)

Per `max_autonomy: draft`, the slice stops at *exists and is honest*:

- **No automated trust change.** `beats_pooled_baseline` and the entrenchment
  ordering are **surfaced, read-only**. Nothing in the default replay/consensus
  path consumes them (issue non-goal "no hidden global mutable trust without
  audit trail"). Wiring entrenchment into `TrustModel` or into
  conflicting-base-evidence resolution is the **consumer decision** that, per
  the compounding-strategy Consumer Rule (§4.2), must land *with* its gating
  decision — that is the owner's call and the next slice.
- **No G2 unification, no calibration axis, no propagation-contribution
  attribution, no staleness decay** (§4 deferred rows, §6).
- **No producer for `source_type`** — the taxonomy stays `unknown` until IX
  annotates it.

Open questions for the owner:

1. Should entrenchment feed `TrustModel::RoleWeighted` as a *prior* on
   `self_trust`, or a separate `TrustModel::OutcomeWeighted`? (A/B-able either
   way against the declared-trust baseline.)
2. Is the day-grain, no-decay ledger acceptable, or do we want
   `staleness_sensitivity` (recency-weighted precision) before any consumer
   gates on it?
3. Should a G2 per-agent precision seed the prior for that agent's source
   precision, unifying the two reports?

## 8. Replay fixture plan (`replay_fixture_plan` evidence)

- **Tracer driver:** `hari-core source-reliability emit --trace <ix-fixture>
  --session <id>` replays any existing `fixtures/ix/*.json` and appends per-source
  rows; `report` aggregates the ledger. No new IX fixture required — the eight
  existing traces already exercise conflicting evidence, contradiction recovery,
  and swarm dissent.
- **Pinned behavioural story:** `tests/source_reliability_e2e.rs` runs the real
  replay on `conflicting_benchmark.json` and asserts the evaluator gets a false
  acceptance while the dissenting critic gets a warranted (non-false)
  escalation, plus the `pooled == Σ sources` invariant across four fixtures.
- **Unit oracles:** `source_reliability.rs` pins the prior/threshold constants,
  precision math, entrenchment ranking, ledger round-trip, canonical-timestamp
  rejection, and malformed-line skipping.
- **Example artifacts:** `fixtures/reliability/example-outcomes.jsonl` +
  `example-report.json` are the verbatim CLI output for that fixture.
- **Admission discipline** (compounding-strategy §4.5): a *new* IX fixture earns
  residence only if it distinguishes a source-grading case the existing eight
  don't (e.g. a right-minority dissenter, to pin the no-punish rule harder).

[14]: https://github.com/GuitarAlchemist/hari/issues/14
