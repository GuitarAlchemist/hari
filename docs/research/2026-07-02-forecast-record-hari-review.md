# hari-side review — forecast-record contract v0.1 (Jarvis Track J2)

Reviews the GA-authored coordination contract
`ga:docs/contracts/2026-07-02-hari-forecast-record.contract.md`
(schema `hari-forecast-record-v0.1.0`). Status after this review:
**contract shape accepted hari-side; still v0.1 DRAFT until the Demerzel
tribunal signs off.** The tracer bullet is implemented against the draft in
`crates/hari-core/src/forecast.rs` + the `hari-core forecast` CLI mode.

## Answers to the contract's open questions

1. **Ledger location and rotation** — accepted as proposed:
   `HARI_STATE_DIR/forecasts/YYYY-MM-DD.jsonl`, defaulting to
   `state/forecasts/` at the repo root (tracked in git — same JSON-on-disk
   handoff pattern as the rest of the ecosystem). One file per **emission**
   day. Append-only is kept literal: the scorer never rewrites a line, it
   appends a *resolved copy* of the record (same `forecast_id`, same file,
   since the file is keyed by `emitted_at`); readers keep the last record
   per `forecast_id`. This preserves the full audit trail — you can see both
   the pending original and its resolution.

2. **`belief_id`: internal id or stable slug** — **slug**, as proposed.
   The tracer's first belief is `ga-quality-snapshot-pipeline-healthy`.
   Ids must not leak storage; slugs also survive hari-internal refactors
   of the belief network, which is exactly what a cross-repo ledger needs.

3. **Cross-repo read path** — **local sibling clone**, as proposed. The
   scorer takes an explicit `--artifact <local-path>` (e.g.
   `../ga/state/fleet/presence.json`); it never fetches over the network.
   The raw-URL path (with its ~5 min cache caveat) stays available to any
   *future* scheduled runner as an invocation detail — the contract shape
   is unaffected because the artifact path is not part of the record; only
   the pinned `observable.source` is.

4. **Minimum viable calibration output** — **per-belief Brier mean +
   count**, as proposed, with one addition: the output also carries `void`
   and `pending` counts per belief. Rationale: a belief whose forecasts
   keep resolving void has a *readability* problem, not a calibration
   problem, and hiding that count would overstate ledger health exactly
   when the world is broken (the same failure mode the contract's `void`
   design guards against). Bucketed reliability curves are deferred to a
   post-freeze iteration.

## What the tracer bullet implements

One belief type, one scorer, end to end (`hari-core forecast …`):

- `emit` — append a pending record (UUIDv7 id, pinned observable,
  probability + rationale, horizon).
- `resolve --source <s> --artifact <path> [--now <ts>]` — score every
  pending record past horizon on that source. Unreadable artifact, missing
  field, or unscorable predicate → `void`, recorded, never dropped.
  Brier = `(probability − outcome)²`, outcome ∈ {0, 1}, absent for void.
- `calibration` — per-belief `{scored, void, pending, mean_brier}`.

Field syntax implemented exactly as the contract's pointer-ish form:
`/`-separated segments; `key=value` selects the first array element whose
`key` string-equals `value` (e.g.
`/limbs/id=sensor:quality-snapshot/status`). Predicates are `== <literal>`
/ `!= <literal>` only — anything else is not mechanically scorable and
resolves void.

Both pending and resolved records validate against
`ga:docs/contracts/hari-forecast-record.schema.json` (checked with
`jsonschema` during implementation).

## First live forecast

`state/forecasts/2026-07-02.jsonl` holds the first record:
`ga-quality-snapshot-pipeline-healthy` predicts
`/limbs/id=sensor:quality-snapshot/status == green` in
`ga:state/fleet/presence.json` at `2026-07-03T18:00:00Z`, p = 0.85.
Note the honest wrinkle: the observable ships in ga PR #503; if that PR is
unmerged at horizon, the sibling-clone read misses the artifact and the
forecast resolves **void** — which is the contract working as designed,
not a failure.

## Deliberately out of scope (v0.1 tracer)

- No integration with `CognitiveLoop` / `BeliefNetwork` — forecasts are
  CLI-emitted. Wiring beliefs to auto-emit is the next slice, after the
  contract freezes.
- No scheduled resolver. Resolution is a manual/cron invocation of
  `forecast resolve`; a workflow lane comes later and must register in
  ga's presence `LANES` once it has run history.
- No rolling windows, no re-scoring, no reliability curves — per contract.
