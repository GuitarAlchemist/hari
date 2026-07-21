# Dogfood corpus — Hari replaying its own research history

Replayable `ResearchTrace` fixtures that project **Hari's own intellectual
history** onto the `ResearchEvent` boundary, so the substrate tracks its own
epistemic record. Unlike `fixtures/ix/` (synthetic IX scenarios) and
`fixtures/revision/` (minimal target-behavior isolations of one revision
semantic each), these traces reconstruct **real** arcs — hypotheses that were
asserted, falsified, corrected, retired, or superseded in this repo's actual
git history.

Each fixture replays clean at HEAD:

```bash
cargo run --release -p hari-core -- replay fixtures/dogfood/<name>.json
```

## Contents

| Fixture | Arc | Companion write-up |
|---|---|---|
| `2026-07-21-research-program.json` | The 2026-07-19 → 07-20 formal-research arc: abandoned substrate study (#13), hex-merge algebraic audit + fixes, propagation audit, hex-merge/SL correspondence, escalation spec v1.2 (#28). 33 events. | `docs/research/2026-07-21-hari-on-hari-replay.md` |

## What makes a dogfood fixture (regeneration / extension rules)

1. **Every proposition is a real claim, every source a real origin, every
   `evidence` map cites the actual doc section and commit SHA.** Timestamps go in
   the evidence map as `as_of` (the `ResearchEvent` type carries no timestamp
   field); use the real commit time from `git log`.
2. **Use the revision variant the history actually supports — never force one.**
   The projection rules are tabulated in the companion doc's §1:
   - `correction` — the project explicitly corrected its own mistake (a named
     prior source replaced by a measured value).
   - `belief_update` + `experiment_result` + `supersession` — one party claimed,
     an independent probe refuted (Hari resolves to `Contradictory`), a fix later
     re-established the claim about the *fixed* code (the successor is the live
     head; the pre-fix claim is the frozen audit tail).
   - `retraction` (no selector) — a designed study withdrawn *before data*:
     recomputes to `Unknown`, because withdrawn is not falsified.
   - `relation_withdrawal` — an append-only relation that stopped holding.
3. **Replay-to-HEAD must succeed and the report is captured in the companion
   doc.** The doc records the match/divergence table between Hari's final state
   and the true history; every divergence is classified as a projection bug or a
   substrate finding.
4. **Prefer additive extension.** New arcs are new fixtures + new doc sections;
   do not rewrite an existing fixture's history (that would defeat the
   append-only, replayable-past discipline the substrate itself enforces).

## Doctrine

**A session that asserts a conjecture and then falsifies it should append the
arc to this corpus.** The value of dogfooding is the gap it measures: what a
human researcher concluded vs what the substrate concludes from the same events.
The `2026-07-21` fixture already bought one finding a synthetic fixture could not
(the single-source corroboration cap under-rating deductive refutations —
companion doc §5.1); that gap only appears when the trace carries *real*
reasoning. Keep feeding the substrate its own history and the divergences stay
measured over time.
