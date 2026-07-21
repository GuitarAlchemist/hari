# Compounding knowledge and intelligence over time — strategy synthesis

**Status: complete. Method: five parallel research agents (2026-07-20) —
two read-only internal sweeps (artifact-reuse archaeology across
hari/ix/ga/Demerzel, ~48 artifacts; feedback-loop inventory, ~48 loops)
and three scientific-literature tracks (self-improving agent systems;
verified/archived knowledge; calibration & collective epistemics). All
five reports are summarized here because agent reports persist nowhere
else; key numbers are reproduced so this doc's claims are checkable.**

The strategic question: win the war (the ecosystem gets permanently
better at its next unit of work), not the battle (any single result).

## 1. The finding, in one sentence

Knowledge compounds iff **a verifier gates the write and a consumer's
decision depends on the read**; everything else — volume, structure,
naming, schemas, machinery — is commentary. The cumulative-culture
literature names the same two-part pawl: **verification + selection/
retirement**, not generation or storage (Tennie/Tomasello ratchet;
"Ratchet Effect in Silico" arXiv 2507.21166: verification and
internalization govern sustained accumulation; single agents plateau).

## 2. Internal state of the union (measured, not assumed)

**Loops: 10 of ~48 healthy (21%).** 13 starved, 15 dead, 5 write-only,
4 too-early. 58% have machinery but no live data flow. The decisive
structural split:

- **Drift tripwires** — recompute derived state, diff against a
  committed baseline, fail CI on divergence (ga invariants,
  maintain-gate, ix assumption-graph, streeling freshness) — are
  **almost all healthy**, because the writer IS the CI check:
  one commitment, synchronous, committed to git.
- **Compounding-signal ledgers** — append observations now, let a later
  decision read the accumulated distribution (forecast Brier,
  pr-grades→reliability, gate-ledgers, PDCA, routing-shadow, council)
  — are **almost universally starved or dead**. They need two
  commitments that keep failing to land together: a disciplined
  out-of-band producer AND a consumer that trusts accumulated data
  enough to gate on it. **Intelligence-compounding specifically is what
  fails; tripwires succeed.**

Notable pathologies, verified: the calibration loop was itself
uncalibrated (2 forecasts, 16 days past horizon, `resolve` never run —
fixed today, §6); hari's G2 reliability consumer was built and tested
against a ga ledger holding zero rows ever; ga routing-shadow writes
370 rows/day with zero readers; ix's quality-gate ledger has a
cross-repo contract, dual-schema reader, dedicated writer binary,
security review — and the JSONL was never created. **Engineering effort
correlates inversely with data presence.** March 2026 is a mass grave:
Demerzel's entire conscience/constitution/cases layer was built
end-to-end and abandoned in one week (03-17→23).

> **Correction (2026-07-21, verify-before-archive sweep for #29,
> Demerzel `7e15e05`):** two of this survey's "dead" classifications
> were false kills. The **conscience layer** is alive — it gates
> `run_afk_cycle.py` self-merge (weight ≥ 0.8) and feeds
> demerzel-ideation/autofix — and **state/resilience** feeds
> `build_manifest`'s health harvest into `governance-health.json`.
> Both were consumed by code paths the inventory didn't trace. The
> March-mass-grave items that WERE archived with verdicts: the 9
> slot-*/visual-* placeholder beliefs, `state/cases`,
> constitution evolution-history, LOG.md. Lesson for the method: a
> loop's reader can be a *conditional guard* rather than a data
> transform, and grep-for-readers misses guards — verify liveness
> before every kill.

**Artifacts: ~15 compounding, ~18 static-load-bearing, ~13 orphaned,
~2 rotted** of 48 sampled. Discriminators, ranked by the archaeology:

1. **Executability** — nothing consumed by a test or runtime loader
   rotted; a red build forces the update. Pure prose is a coin flip.
2. **Cited-by-code beats cited-by-docs** — prior-art-survey and
   phase6-design compound because `lib.rs` and tests point at them;
   docs-citing-docs graphs (ADRs, solutions) are weak glue.
3. **Falsifiable, dated, negative verdicts travel furthest** — the
   Phase 5 "Lie loses to SL" result flowed uphill into Demerzel's
   evolution ledger and back out as a PRD. Aspirational framing rots.
4. **Producer death kills ledgers** (visual-belief cohort rotted when
   the visual-critic-loop stopped writing) — the write half fails at
   least as often as the read half; both rules in §4 are needed.
5. **Naming/location matter least at this scale** — well-filed ADRs
   orphan at the same rate. (mathlib's lint-enforced-discoverability
   finding kicks in at 308k declarations; at ~50 artifacts the binding
   constraint is having a consumer at all.)

Meta-rot exhibit: Demerzel's staleness-detection policy is load-bearing
in three repos while the beliefs it governs sat stale for four months;
ADR 0002 "harvest, don't declare" is itself orphaned; hari's
`.claude/skills` (installed 06-14, cited in doctrine) show **zero
invocation traces** — persistence without an invocation loop is how
skills die, which constrains how we persist agents (§6).

## 3. What the literature adds (per track)

**Self-improving agents.** Accumulation is monotone only through an
automatic verifier — FunSearch (Nature 2023) and AlphaEvolve (2025)
promote candidates only when an evaluator scores them; monotonicity is
a property of the verifier, not the model. Ungrounded self-reflection
is placebo and can be net-negative (Huang et al., ICLR 2024: intrinsic
self-correction degrades GSM8K 95.5→89.0; earlier positive results had
oracle leakage). The write-read asymmetry is now measured (retrieval/
utilization, not capacity, is the bottleneck — arXiv 2603.02473), and
A-MEM-style self-rewriting consolidation destroys audit trails —
append-only JSONL with injected timestamps is a defense to preserve.

**Verified corpora.** mathlib is the existence proof (1.9M LOC
compounding since 2017) with a transferable governance playbook:
deprecation-with-grace-periods instead of hard deletes, review-as-
teaching, lint-enforced naming, never-red CI — and the warning that
devs use a median 1.6% of what they import. PBT kills ~50× more
mutants than average unit tests (OOPSLA 2025), and mutation score is
the honest metric for whether a pinned probe carries knowledge — a
probe with low mutation-kill is coverage theater. Quality-diversity
archives give the fixture-admission rules: minimal criterion (a new
fixture must distinguish something the corpus doesn't) and determinism
as an admission gate (one flaky fixture degrades trust in all —
QD reproducibility collapse, arXiv 2409.13315). Counterweight:
compute-matched re-evaluations show learned libraries often lose to
no-library baselines (TroVE re-eval, arXiv 2507.22069) — apply the A/B
doctrine to the knowledge corpus itself.

**Calibration & epistemics.** The evidence-recompute-authoritative
doctrine chosen in the merge/propagation audits is textbook Doyle-1979
JTMS foundationalism, and the rotted Demerzel belief is the exact
failure TMS was invented to prevent; MemStrata (arXiv 2606.26511)
independently shows deterministic supersession beats similarity
retrieval by construction (embeddings can't tell contradiction from
paraphrase, AUROC 0.59) — with the nuance *retire, don't delete*.
Tetlock: scoring alone is inert; volume, teaming, feedback cadence do
the compounding. Loops die of **decision-decoupling** (Google's
well-calibrated internal prediction market shaped no decisions and
died). The negative-results culture is quantitatively vindicated
(registered reports: 31/71 hypotheses supported vs 146/152 in standard
journals). Ossification caveat (SSGM, arXiv 2603.11768): base evidence
must itself stay revisable or stale-derived-belief becomes
stale-base-evidence.

## 4. Doctrine — the rules that fall out

1. **Verifier rule.** No knowledge artifact without an attached
   automatic check. A claim that can be a probe becomes a probe; a
   negative result is pinned by a still-running test (the Phase-5
   pattern) or it is placebo-prone prose. Probe adequacy is measured
   by mutation score, not existence.
2. **Consumer rule.** No new ledger, schema, or contract without its
   first reader AND the decision that gates on it, landing in the same
   change. (Kills the pr-grades pattern: consumer-waits-forever.)
3. **Tripwire conversion.** Where a compounding ledger can be reshaped
   so its writer is a CI step that commits data back to git, do it —
   that converts the failing archetype into the succeeding one.
   Nightly artifact-upload without commit-back is how accumulation
   evaporates (30-day retention).
4. **Retirement operator.** The ratchet needs its pawl: artifacts are
   actively retired on failed verification — deprecated with the
   falsifying probe cited (mathlib grace-period style), never silently
   deleted (MemStrata). The Demerzel belief-replay is the prototype:
   replay → Contradictory → flag/retire. Today the ecosystem has
   verification without retirement.
5. **Admission discipline.** New fixtures/probes earn residence:
   minimal criterion + determinism gate. Archive size is not proof
   strength — 11 probes prove 11 relations.
6. **Append-only with lineage.** Ledgers append; resolution appends a
   resolved copy; superseded evidence is retired, not erased. Resist
   self-rewriting consolidation.

## 5. The research-grade connection: reliability cards are the missing entrenchment ordering

Foundationalist recompute answers "is this derived belief supported?"
but cannot rank **conflicting base evidence** — that is AGM's
epistemic-entrenchment machinery, which the doctrine discarded. The
per-source reliability ledger (G2, currently empty) is exactly that
ordering: source reliability = entrenchment. This reframes pr-grades/
G2 from stalled plumbing into the substrate's missing half, and it is
the principled reason to feed it rather than kill it. (It also closes
the loop with `hari-swarm`'s trust models: entrenchment is trust,
learned from graded outcomes rather than declared.)

## 6. Actions taken with this synthesis (same day)

- **The calibration loop closed for the first time.** `forecast
  resolve` ran; both overdue forecasts resolved **false** (the
  quality-snapshot limb reads `unknown`, not `green`), Brier 0.72 /
  0.64. First calibration datum: overconfident — independently
  agreeing with Demerzel's ML loop (overconfidence_rate 0.333) and
  with the loop inventory's finding that the chatbot-qa producer died
  06-19. The ledger now holds resolved copies (append-only).
- **`formal-auditor` persisted as a repo agent**
  (`.claude/agents/formal-auditor.md`) — the probe-audit method that
  produced 11 theorems / 5 fixes / 3 falsified hand-analyses in one
  weekend, with its when-to-use trigger in the description so it is
  selectable, addressing the zero-invocation-skills failure mode
  directly.
- **Concurrent-session commit protocol persisted**
  (`docs/agents/concurrent-sessions.md`) so subagents inherit it from
  the repo rather than from one session's private memory.
- **Loop triage filed** as a tracked issue (kill / feed / couple per
  starved-dead-write-only loop) rather than as prose in this doc.

## 7. Honest limits

Five agent reports, one day, ~48+48 sampled units; inbound-reference
counts include some noise; several 2026 arXiv IDs surfaced by search
could not be opened in full and are flagged in the underlying reports.
This document is itself prose — by its own §4.1 every claim in it that
can be a check should become one, and until then it should be treated
as the hypothesis set for those checks, not as settled doctrine.
