# Source-Reliability Summary — contract v0.1 DRAFT

- **Contract:** `source-reliability-summary`
- **Version:** v0.1.0 — **DRAFT** (shape open for review; freeze at the Phase-4
  milestone of the parent epic, `GuitarAlchemist/hari#12`)
- **Schemas:** `hari-source-outcome-v0.1.0` (one ledger row) and
  `hari-source-reliability-report-v0.1.0` (the aggregate) — defined in
  `crates/hari-core/src/source_reliability.rs`.
- **Examples:** [`fixtures/reliability/example-outcomes.jsonl`](../../fixtures/reliability/example-outcomes.jsonl)
  (ledger rows), [`fixtures/reliability/example-report.json`](../../fixtures/reliability/example-report.json)
  (aggregate).
- **Design:** [`docs/research/2026-07-20-source-reliability-design.md`](../research/2026-07-20-source-reliability-design.md)
- **Issue:** `GuitarAlchemist/hari#14` (parent epic `#12`)
- **Status:** shape implemented and tested hari-side as a draft; remains v0.1
  DRAFT until an owner signs off on a *consumer* (the decision that gates on the
  entrenchment ordering — see the design doc §7). No cross-repo consumer yet.

## Purpose

Define the portable, JSON-on-disk shape of Hari's **cross-session source
reliability** record: how each source's claims turned out, accumulated across
replayed sessions, aggregated into a per-source precision summary with a pooled
A/B baseline and an epistemic-entrenchment ordering. It lets a future consumer
(conflicting-base-evidence resolution in the substrate; a trust-calibration
step) rank sources by *earned* reliability without linking against Hari's Rust
types.

## Design principles

1. **JSON-first, local-first.** The ledger is append-only JSONL
   (`hari-source-outcome-v0.1.0`, one object per line) under
   `HARI_STATE_DIR/source-reliability/`, one file per emission day. The
   aggregate (`hari-source-reliability-report-v0.1.0`) is a plain JSON object
   printed by `source-reliability report`. No database.
2. **Append-only, immutable rows.** A session appends new rows; rows are never
   rewritten. Aggregation is summation, so it is monotone and order-independent.
3. **Pooled is the baseline.** Every report carries a `pooled` entry (all
   sources summed). A consumer preferring a per-source score over `pooled` is
   making an A/B-able claim — the roadmap's simpler-baseline rule.
4. **Surfaced, never auto-applied.** `beats_pooled_baseline` and the
   `entrenchment` ordering are read-only signals. This contract does **not**
   authorise any automated trust mutation (issue #14 non-goal). A consumer that
   gates on them is a separate, signed-off change.
5. **Honest degradation.** `accepted_claim_precision` / `false_escalation_rate`
   are absent (not NaN) when their denominator is zero; `source_type` defaults
   to `unknown`; malformed ledger lines are skipped, not fatal.
6. **Additive evolution.** New optional fields (e.g. a populated `source_type`,
   a staleness-weighted precision) are additive; a major/minor schema bump is a
   coordinated, signed-off change.

## Ledger row — `hari-source-outcome-v0.1.0`

One immutable per-source, per-session tally.

| field | type | meaning |
|---|---|---|
| `schema` | string | `"hari-source-outcome-v0.1.0"` |
| `session_id` | string | replay/session that produced the row (audit + dedup) |
| `source` | string | the graded source — `ResearchEvent.source` |
| `source_type` | enum | `agent`/`model`/`benchmark`/`repo`/`tool`/`research_digest`/`human_review`/`ix_autoresearch_run`/`unknown`; **reserved**, `unknown` until producers annotate it |
| `recorded_at` | string | injected canonical UTC `YYYY-MM-DDTHH:MM:SSZ`; names the day file |
| `claims` | u32 | belief-bearing events the source emitted |
| `accepted` | u32 | `Accept` actions on the source's events |
| `false_acceptances` | u32 | of `accepted`, later retracted / went Contradictory / flipped polarity |
| `escalations` | u32 | `Escalate` actions attributable to the source's proposition |
| `false_escalations` | u32 | of `escalations`, those whose proposition ended True/Probable |

## Aggregate — `hari-source-reliability-report-v0.1.0`

```
{ schema, generated_at, rows, skipped, pooled, by_source{}, entrenchment[] }
```

`pooled` and each `by_source[*]` value is a **reliability entry**:

| field | type | meaning |
|---|---|---|
| `sessions` | usize | ledger rows in this bucket |
| `claims` / `accepted` / `false_acceptances` / `escalations` / `false_escalations` | u32 | summed tallies |
| `accepted_claim_precision` | f64? | `1 − false_acceptances/accepted`; absent when `accepted == 0` |
| `smoothed_precision` | f64 | `((accepted − false_acceptances) + 2.0·0.5)/(accepted + 2.0)`; defined always |
| `false_escalation_rate` | f64? | `false_escalations/escalations`; absent when `escalations == 0` |

Each `entrenchment[]` rung (sources ranked by `smoothed_precision` desc, then id
asc):

| field | type | meaning |
|---|---|---|
| `source` | string | the source |
| `smoothed_precision` | f64 | its earned precision |
| `accepted` | u32 | sample size behind it |
| `beats_pooled_baseline` | bool | `accepted ≥ 3` **and** `smoothed_precision ≥ pooled.smoothed_precision + 0.05` |

The smoothing prior (weight `2.0`, mean `0.5`) and the trust thresholds
(`MIN_ACCEPTED_FOR_TRUST = 3`, `TRUST_MARGIN = 0.05`) are pinned by a test;
retuning is an explicit change. The prior matches
`hari-agent-reliability-report` (G2) so the two views compose.

## Producer / consumer

- **Producer (hari, implemented):** `hari-core source-reliability emit --trace
  <path> --session <id> [--now <ts>]` replays a trace and appends one row per
  source. Additive: it does not alter the trace's own `replay` output.
- **Reader (hari, implemented):** `hari-core source-reliability report [--now
  <ts>]` aggregates the whole ledger, read-only.
- **Consumer (NOT yet — owner gate):** the decision that gates on
  `entrenchment` / `beats_pooled_baseline` (trust calibration; conflicting-base
  -evidence resolution) is the next slice and requires owner sign-off. Until it
  lands, this record *exists and is honest* but drives no automated behaviour —
  the same discipline as Giskard G1/G2.

## Compatibility

Additive `ResearchEvent` payload changes flow through transparently (a new
belief-bearing channel just adds to `claims`/`accepted`). Consumers MUST ignore
unknown fields and MUST treat an absent optional metric as "not enough data",
never zero.
