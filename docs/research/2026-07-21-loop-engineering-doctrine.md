# Loop-engineering doctrine v1 — in-house measurements, adversarially tested against the literature

**Status:** canonical. Successor to the doctrine sections (§2/§4) of
[2026-07-20-compounding-strategy.md](2026-07-20-compounding-strategy.md);
that document remains the measurement record (the 48-loop inventory),
this one is the rule book. Applies to all four repos (hari, ix, ga,
Demerzel).

**Method:** the five in-house rules — derived from the measured 10/48
loop inventory and the incidents of 2026-07-20/21 — were submitted to a
deep-research harness (105 agents; 23 sources fetched; 113 claims
extracted; 25 top claims put through 3-vote adversarial verification:
18 confirmed, 5 refuted, 2 unverified on infrastructure failure). Every
rule below carries its resulting epistemic status. Sources are cited
inline; verbatim quotes were re-checked against the primary texts by
the verifier agents.

---

## 1. The five rules, with epistemic status

| # | Rule | Status after adversarial sweep |
|---|---|---|
| 1 | **Verifier + consumer, same change.** Knowledge compounds iff a verifier gates the write AND a consumer's decision depends on the read — "no ledger without its first reader," landed in the same change. | **In-house only.** The literature confirms the *weaker* forms (ungated writes produce false records — Valero; detection without an obligated consumer closes few loops — DOCER ~26% fix rate, unverified 1-0) but both external claims supporting the *same-change atomic coupling* were **refuted** (Fluri 97% atomic comment updates: 0-3; "doc-decay root cause is the uncoupled verifier": 0-2). Our 10/48 measurement currently stands alone. Keep the rule — it is our data — but stop citing the literature for it. |
| 2 | **Tripwires survive; ledgers die.** Drift tripwires (writer IS the CI check) survive; append-now-decide-later ledgers die. | **Theorem-grounded.** Error-controlled (after-the-fact) regulation is provably inferior and self-limiting — its information channel passes through the very error it suppresses (Conant & Ashby 1970 §3; Ashby 1958, both verified verbatim, 3-0 twice). And stored "truth" in a drifting system will not keep: old data must be re-validated against the present before it can even be judged relevant (Ashby 1958, 2-1). The mapping from regulator to CI check is analogical, not formal — but the direction of the inequality is a theorem. |
| 3 | **Watch the artifact, not the process.** Freshness sensors must read the produced artifact; a cancelled CI job reads green. | **Confirmed and EXTENDED** (see §2.1). Doc rot is default and silent — 82.3% of top-1000 GitHub projects historically outdated, "documentation gets outdated silently," no intrinsic failure signal (Tan, Wagner & Treude, EMSE 2024, 2-0/3-0). Case evidence extends the rule to the *whole monitoring apparatus*: TWA 800 (one-time verification treated as permanent), Valero 2007 (record marked complete, reality diverged, three reviews read the false record), Deepwater Horizon (safety award during incubation). |
| 4 | **Liveness audits false-kill guard-readers.** Grep-based reader detection misses conditional-guard readers (measured: 2 false kills of 6 in the Demerzel sweep). | **In-house only.** Zero surviving external evidence either way; the good-regulator theorem marshaled to support it was refuted 0-3 (its own proof is contested). Stands on our measurement; open question filed (§5). |
| 5 | **Volume at decision points.** Forecast/calibration loops need volume where decisions happen, not one-off predictions. | **Confirmed with an off-the-shelf framework.** Google's burn-rate alerting quantifies it: a 10-req/hour service pages on a single failure (1,000x burn) — sparse loops must be synthetically exercised or aggregated before their signals are decision-grade (SRE Workbook ch.5, 3-0). See §3 for the template. |

---

## 2. Extensions — failure modes the in-house taxonomy lacked

### 2.1 Drift defeats tripwires two ways (Dekker & Pruchnicki 2013; NASA/Shivers 2011; 3-0 × 3)

The in-house taxonomy assumed a tripwire fails by *dying*. The drift
literature shows two ways it fails while *running*:

1. **Sub-threshold increments.** Normalization of deviance is driven by
   incrementalism: each departure from the last accepted norm is
   individually too small to remark on. A threshold calibrated to the
   last norm never fires on drift composed of sub-threshold steps.
2. **Above-threshold absorption.** Signals that DO cross the threshold
   get formally absorbed into the norm by review (every Challenger
   anomaly passed a Flight Readiness Review). The tripwire fires; the
   review board renormalizes; the baseline moves.

**Doctrine consequence — anchored tripwires:** thresholds must be
anchored to an immutable original baseline, not the last accepted
state, and moving the anchor must be a logged, owner-level act (the
same shape as our pinned-test convention: `known_divergence_*` tests
that MUST FLIP when behavior changes — the anchor is the test, and
moving it is a visible commit). Where a threshold is relative
("newer than last_updated"), pair it with an absolute anchor
("and not older than the policy's original expiry").

### 2.2 Silent-green is itself a signal (NASA/Shivers 2011, verbatim; 3-0 × 4)

"If insight and oversight activities seldom find any issues, someone
should ask why." A monitor that never fires is indistinguishable from a
dead monitor — and success breeds trust that suppresses the question.

**Doctrine consequence — sensor self-tests:** every tripwire must
demonstrably be ABLE to fire: a unit test that feeds it a synthetic
violation (we already do this for `forecast check` and the ga freshness
sensor — now doctrine, not habit), plus a liveness review item: any
sensor green for N consecutive periods gets one manual verification
that it still can fire. And the Valero pattern — a record marked
complete while reality diverged, then read as true by three subsequent
reviews — makes **closed-loop verification** doctrine: a "done" record
must be checked against the artifact it claims to describe at least
once after it is written (our substrate-audit compares hand-verdicts to
replay-verdicts; the same shape applies to action items and checklists).

### 2.3 Sensor antipatterns, quantified (Google SRE Workbook ch.5; 3-0 × 2)

- **Instantaneous-threshold alerting** can emit ~144 non-actionable
  alerts/day while the SLO is met — alert fatigue by design, and
  fatigue is how loops with live sensors still die (the reader stops
  reading).
- **Duration-conditioned alerting** ("fire only after N sustained
  minutes") produces false greens: the timer resets on momentary
  recovery — 100% error spikes of 5 min every 10 min never alert while
  consuming 35% of the error budget — and detection time does not scale
  with severity. Google recommends against durations in SLO alerting.

**Doctrine consequence:** our freshness sensors ("newest artifact > N
days old") are threshold sensors on slow signals — fine at daily
cadence. But any future high-frequency loop (per-commit, per-event)
must use burn-rate shapes (§3), not instantaneous thresholds, and
never duration conditions.

### 2.4 The feedback imbalance (Dekker & Pruchnicki 2013, verbatim; 3-0)

Effects on production goals are fast and legibly measurable; erosion of
margins is not. The fast legible loop systematically outcompetes the
slow illegible one — the measurable proxy wins by default. This is the
generative mechanism behind our measured "effort correlates inversely
with data presence": elaborate ledgers were built where the signal was
illegible, and abandoned for exactly that reason.

**Doctrine consequence:** when proposing a loop, state which side of
the imbalance it sits on. A loop whose signal is slow/illegible needs
*structurally* cheaper reads (one number, one place, CI-surfaced) or it
loses to the legible loops regardless of its importance.

### 2.5 Liveness is not sufficient (unverified tier — flagged, not doctrine)

The ML-loop sweep surfaced (but this batch did not verify) the
degenerate-loop literature: a loop can be fully alive — verifier
gating writes, reader consuming — and still compound *harm* (RLHF
alignment collapse; recommender degeneracy where only the *speed* of
degeneration is controllable). Our doctrine currently equates alive
with healthy. Candidate sixth rule, pending verification: **a
compounding loop needs an output-quality gate independent of the
signal it feeds on** (our A/B-against-baseline discipline is exactly
such a gate; the connection should be made explicit when verified).

---

## 3. The prescriptive template we adopt: liveness budgets (burn-rate form)

Google's multiwindow multi-burn-rate pattern (SRE Workbook ch.5,
verified; natively supported by Prometheus/Grafana/Datadog tooling):
alert severity scales with *budget-consumption velocity*, short window
= 1/12 of long window; for a 99.9% SLO: page at 14.4x burn (2%
budget/1h), page at 6x (5%/6h), ticket at 1x (10%/3d).

Adaptation for loop-liveness ("**liveness SLO**"): give each loop an
explicit budget of missed cycles per window (e.g. producer cadence
daily, budget 3 missed days/30). A single miss is a ticket-grade
signal; consecutive misses consume budget at increasing velocity and
escalate. For sparse loops (forecast ledger), the low-traffic remedies
apply in order of preference: **aggregate** (score Brier across all
four repos' forecasts as one monitored unit), **synthesize** (emit
forecasts at every decision point — already doctrine), or lower the
SLO explicitly (never silently).

Requisite variety (Ashby 1958, 3-0) is the theoretical ceiling behind
the budget metaphor: a loop cannot dampen more variation than it can
sense and act on — so a loop's sensing channel (what it reads, how
often) bounds what it can promise, and adding promises without adding
channel is structurally void.

## 4. Refuted-claims record (do not cite these in support of the doctrine)

- Good-regulator theorem as support for rules 1/4 (0-3; proof contested).
- "Sense the cause, not the error" as a *prescriptive* reading of
  Ashby for artifact-watching (1-2; the artifact rule stands on the
  empirical evidence in §1.3 instead).
- Leveson/Rasmussen "loops rot by misalignment, not by stopping" (0-3).
- "Doc decay's root cause is the uncoupled verifier" (0-2).
- Fluri "97% of comment updates are atomic with the code change" (0-3).

## 5. Open questions (carried into the tracker)

1. Rule 4 calibration: is there empirical work on false-positive rates
   of static-analysis liveness/dead-code audits that could calibrate a
   loop-liveness auditor? (No surviving literature found.)
2. Rule 1's same-change coupling: direct empirical support anywhere?
   Both external candidates failed; our 10/48 stands alone — worth a
   registered in-house replication on a second ecosystem.
3. Sparse-ledger aggregation: can Brier-scored forecast loops be
   aggregated across the four repos into one decision-grade monitored
   unit (§3)?
4. Renormalization-resistant tripwire design: immutable anchors and
   logged anchor-moves (§2.1) — what is the enforcement mechanism, and
   who may move an anchor?
5. The §2.5 candidate sixth rule (output-quality gate), pending
   verification of the degenerate-loop literature.

## 6. Application checklist (per repo)

For each repo's loops (inventory: compounding-strategy §2):

- [ ] Every tripwire threshold: anchored to an immutable baseline?
      Anchor-moves logged as commits? (§2.1)
- [ ] Every sensor: has a self-test proving it CAN fire? (§2.2)
- [ ] Every "complete" record that gates anything: closed-loop-verified
      against its artifact at least once post-write? (§2.2, Valero)
- [ ] No duration-conditioned alerting anywhere; burn-rate shape for
      any high-frequency loop. (§2.3)
- [ ] Each loop labeled with its side of the feedback imbalance;
      illegible-signal loops given structurally cheaper reads. (§2.4)
- [ ] Sparse loops: aggregated or synthetically exercised before their
      signals gate decisions. (§3)
- [ ] New-loop admissions: rule 1 unchanged (verifier + first reader +
      gating decision, same change) — in-house rule, our data, still
      the admission bar.

## Sources (verified tier)

- Conant & Ashby 1970, *Every Good Regulator…* — <https://www.tandfonline.com/doi/abs/10.1080/00207727008920220>
- Ashby 1958, *Requisite Variety…* — <https://pespmc1.vub.ac.be/books/AshbyReqVar.pdf>
- Google SRE Workbook ch.5, *Alerting on SLOs* — <https://sre.google/workbook/alerting-on-slos/>
- Dekker & Pruchnicki 2013, *Drifting into failure* — <https://safetydifferently.com/wp-content/uploads/2014/08/SDDriftPaper.pdf>
- NASA/Shivers 2011 (NTRS 20110015770) — <https://ntrs.nasa.gov/api/citations/20110015770/downloads/20110015770.pdf>
- CSB Valero 2007 report — <https://www.csb.gov/file.aspx?DocumentId=5672>
- Tan, Wagner & Treude, EMSE 2024 (doc rot) — <https://link.springer.com/article/10.1007/s10664-023-10397-6>
