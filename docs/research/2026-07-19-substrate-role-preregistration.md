# Pre-registration — substrate-role, open-loop (Phase 7 candidate)

**Status: v0.2 — ABANDONED at the §6 tracer gate, 2026-07-19. Design does
not survive; see §11.** The gate was checked by static inspection of
`ix-autoresearch` plus one throwaway probe, before any corpus was
generated and before any IX-side change was made. Kill criterion §7.1
fired. §§1–10 are preserved unedited as the pre-registered design so the
abandonment is auditable against what was actually committed to in
advance.

Original status line, for the record: *v0.1 DRAFT — not owner-approved, no
data collected, no IX-side changes authorized.* This document exists to be
attacked before it is executed. Nothing in it commits the project to the
phase; §9 lists the decisions that must be made first, all of which are
owner calls.

Selected from five candidate research questions by a multi-agent scoring +
adversarial-review pass (2026-07-19). The other four — SAE/Contradicts
correspondence, feature-entropy→hallucination, disjoint feature sets,
non-Lie learned-dynamics baseline — were deferred or dropped; see §8.

## 1. The question

Does Hari's typed, contradiction-preserving epistemic layer measurably
improve decision quality in an autonomous experiment loop, relative to
credible simpler substrates?

This is the empirical form of `prior-art-survey.md` §6 Q5 — *if IX could
plug in Subjective Logic or a 20-line threshold instead, what is the case
for Hari?* — and it targets the one asset the survey identified as
defensible: the **niche**, not the machinery (§4). It is deliberately
**open-loop**: replay only. It does **not** discharge the ROADMAP's
closed-loop Near-Term Milestone, which stays blocked on IX-side consumer
work (see §9.5).

## 2. Hypotheses

**H1** — On replayed `ix-autoresearch` grammar-target runs with the eval
cache disabled and evaluation noise injected under a **bimodal "flaky
config"** regime, the hexavalent pipeline achieves a lower per-run oracle
false-acceptance rate, **at matched oracle false-rejection rate**, than
each baseline in §3.

**H0** — The hexavalent pipeline is statistically indistinguishable from,
or worse than, the EWMA/SPRT baseline and the `SubjectiveLogic` control in
both noise regimes: any memory-bearing accept policy captures the available
value and the contradiction-preserving lattice adds nothing.

No advantage is claimed or expected in the i.i.d. Gaussian regime. That
regime is a control, not a second chance.

## 3. Baselines — three tiers, ascending honesty

Phase 5's failure was a strawman baseline. The counter-design is to make
the hardest baseline the one that decides the verdict.

1. **Floor — IX's own Greedy/SA acceptance rule** (`policy.rs`). The real
   incumbent. Beating *only* this proves nothing except that cross-iteration
   memory helps.
2. **Must-beat — EWMA-of-reward + Wald SPRT-style sequential threshold.**
   ~20 lines, no logic. Credible prior art: sequential hypothesis testing is
   provably near-optimal for accepting a noisy scalar. **Tuning budget
   identical to Hari's reward→HexValue mapping, on the same calibration
   split.** Parity here is the whole point; unequal tuning invalidates the
   comparison.
3. **Internal control — `PriorityModel::SubjectiveLogic`** on the identical
   event stream. The baseline that killed Lie. If SL ties Hari, any win over
   (1) and (2) is *SL in a Hari costume*, and that is what gets reported
   (§7.3).

## 4. Metrics

**Primary** — per-run **oracle false-acceptance rate**: fraction of Accept
decisions on configs whose noise-free reward (from grammar's deterministic
reward function) is below the pre-registered improvement threshold, compared
at **matched oracle false-rejection rate**, with **run** as the unit of
analysis.

Both error rates are computable without off-policy counterfactuals because
the oracle exists. This is the design's load-bearing trick: it supplies the
false-rejection counterweight that `phase5-fixture-rollup.md` §6 flagged and
never landed, closing the degenerate never-Accept optimum that made
`false_acceptance_count` unsafe to optimize alone.

**Secondary**

- Per-run oracle false-rejection rate, reported at *all* operating points.
- Precision/recall of `Escalate` and `Investigate` against oracle flaky-config
  labels — the only part of the 5-symbol action vocabulary open-loop replay
  can score.
- Oracle regret of the final accepted-best config vs the true best in-run.
- Decision-layer cost: wall-clock per decision, and implementation LOC, so
  the write-up prices the lattice against the 20-line baseline.
- **Self-applied mapping ablation** — sensitivity over the reward→HexValue
  grid. Phase 5 criticized SL's mapping crudeness; this applies that critique
  to ourselves (see §7.5).

## 5. Sample size and power

- **n = 200** seeded grammar runs per noise family × **2** pre-registered
  families = **400 evaluation runs** of ~100 iterations each.
- **+50 calibration runs per family**, disjoint seeds. HexValue thresholds
  and EWMA/SPRT parameters are tuned on these and **frozen before any
  evaluation run is scored**.
- ≈40,000 evaluations total — one overnight laptop job. Compare Phase 5's
  ~101 hand-authored events.
- All four policies replay the **identical** stream, so comparisons are
  paired within run. Wilcoxon signed-rank on per-run differences at n=200
  retains >80% power for dz ≈ 0.25 after Bonferroni across 3 comparisons × 2
  regimes.
- **Iteration-level analysis is pre-registered as forbidden.** SA cooling,
  monotone best-so-far, and shared seeds make iterations autocorrelated.
  Effective n is runs, not events.

## 6. Tracer bullet — build this before anything scales

One seeded run, end to end through every layer:

1. Run `ix-autoresearch` `target_grammar` with the eval cache **disabled**
   and Gaussian noise injected; emit the JSONL log per its `SCHEMA.md`.
2. Build the minimal `hari-from-ix-autoresearch` projection — named as a
   consumer in that SCHEMA but **nonexistent here today**
   (`state/harness/events.jsonl` is a 17-line hand-written stub) — pinned
   against `jsonl-event.schema.json`.
3. Replay the projected stream through all four policies.
4. Emit one oracle-scored false-accept / false-reject report.

**Abandon gate.** Confirm that noise-driven re-evaluations of the same
`config_hash` actually consolidate to `HexValue::Contradictory` at Hari's
lattice, as the SCHEMA's Layer-2 view claims. **If they do not, the
phenomenon the lattice preserves does not occur in this pipeline** and the
design returns to the drawing board — at a cost of days, not weeks.

## 7. Kill criteria — stated before any data

1. **Tracer gate** — no organic `Contradictory` consolidations on the tracer
   run ⇒ abandon before generating the 400-run corpus.
2. **Primary kill** — hexavalent does not beat matched-budget EWMA/SPRT on
   oracle false-acceptance at matched false-rejection in the bimodal regime
   ⇒ the distinctive-machinery claim is dead. Publish the null as the answer
   to survey §6 Q5, recommend "SL or simpler" as the substrate stance, and
   **do not re-tune against the evaluation runs**.
3. **SL-costume kill** — `SubjectiveLogic` ties or beats hexavalent
   everywhere ⇒ report that any advantage over external baselines is not a
   hexavalent result, regardless of how those comparisons went.
4. **Confound kill** — winner flips between noise families such that the
   verdict tracks the noise model rather than the policy ⇒ report
   "confounded, no verdict". Do not headline the favorable family.
5. **Mapping kill** — verdict does not survive the reward→HexValue ablation
   grid ⇒ the mapping, not the substrate, is the finding, and is reported
   as such.

## 8. Why not the other four

| Candidate | Verdict | Core reason |
|---|---|---|
| SAE / Contradicts | defer (runner-up) | Confounded by construction. `fixtures/ix` holds exactly **one** `Contradicts` declaration; propositions are kebab-case slugs, not sentences, so ~98% of the corpus would be freshly authored by the hypothesis-aware author with Hari as a label column. `Independent` is not in `hari-lattice`'s `Relation` enum, so the null class is synthesized. Topic distance runs *opposite* to H1. Revisit gated on the real-trace corpus this phase produces. |
| Feature entropy → hallucination | defer | Informative version has no power at achievable n (50–200 adjudicated labels); uninformative version re-derives Farquhar et al. 2024 / Ferrando et al. 2025 through a collinear, costlier proxy. Nothing in Hari's substrate is exercised. |
| Disjoint feature sets | **drop** | Hari-independent by its own description — no BeliefNetwork, no relations, no `ResearchEvent` boundary, no ROADMAP decision it moves. Near-certain positive that a bag-of-words baseline matches. Committing the post-negative-result cycle here would be changing the subject. |
| Non-Lie learned baseline | defer → **guard condition** | Moot: Lie already lost to a *simpler* baseline, so beating a *harder* one settles an unneeded counterfactual. Supervision signal is circular with a degenerate never-Accept optimum; 6 fixtures can't support a train/test split; a training stack violates the leaf-crate hierarchy. **Preserved at zero cost**: propose adding to ROADMAP "Open: Cognition Substrate Choice" — *any future claim that structured dynamics beats SL must additionally survive a matched-capacity, matched-tuning learned-dynamics control.* |

## 9. Open decisions — owner only, required before execution

1. **Cross-repo budget.** Corpus generation, the cache-disable flag, and the
   noise-injection hook are **IX-side changes** booked to Hari's research
   program. Only the owner authorizes modifying `ix-autoresearch`, and decides
   where `hari-from-ix-autoresearch` lives without violating the leaf-crate
   hierarchy in CLAUDE.md.
2. **Pre-registration sign-off** on the operational definition of decision
   quality (oracle false-accept at matched false-reject, run as unit), the
   reward→HexValue grid, and the EWMA tuning-parity rule — before the first
   evaluation run is scored.
3. **The commitment device.** Agree in advance, in writing here, that a null
   against EWMA/SPRT or SL is published as the **headline** answer to
   `prior-art-survey.md` §6 Q5. Direction-setting, not a methods detail.
4. **Downstream decision rule.** Does a Hari loss (or SL tie) trigger
   switching the default decision engine to `SubjectiveLogic`? CLAUDE.md
   forbids default changes without explicit owner approval, so this is
   decided **before** data, not negotiated after.
5. **Milestone wording.** This phase deliberately does not discharge the
   closed-loop Near-Term Milestone. Decide whether that wording is amended to
   reflect the open-loop / closed-loop split.
6. **Noise parameters.** Bimodal magnitudes and flaky-config prevalence
   partially determine the result. Set from measured chatbot/OPTIC-K
   evaluation variance, or explicitly accept them as synthetic.

## 10. Honest caveat — read this before approving

The noise-injection rescue **quietly reintroduces the exact failure this
project just published a negative result about, one layer down.**

The grammar target is deterministic. Every contradiction in the study exists
because the experimenter injected it, and the bimodal "flaky config" family —
the only regime where H1 predicts a win — is close to a purpose-built
showcase for a contradiction-preserving lattice, chosen by the same people
who built the lattice. A win would establish only that the lattice helps
under a synthetic noise model grafted onto a loop with no organic epistemic
content. External validity to targets where noise is *real* (chatbot ~30s/iter,
OPTIC-K ~140s/iter) remains unmeasured, because those are precisely the
targets too slow to power.

Compounding this: open-loop replay collapses the action vocabulary to a
post-hoc accept/escalate classifier. **The phase therefore does not test
"Hari as substrate" — the niche claim. It tests "Hari as classifier over a
substrate-shaped event stream."** A reviewer would be right to say the
ROADMAP milestone is untouched when it ends.

And the modal predicted outcome is a **null against twenty lines of
sequential hypothesis testing**. The realistic expected payoff is the
project's second consecutive negative result, purchased with new IX-side
engineering, an adapter crate, and a 400-run corpus.

The argument for proceeding rests entirely on one claim: that this
particular null, unlike any other result currently available, resolves the
substrate question the project cannot move past without answering. If the
owner does not accept that claim, the correct decision is **not to run this
phase**.

## 11. Gate result — the design is dead, and not for the reason §10 predicted

**The §6 abandon gate cannot be passed. Not "is unlikely to pass" —
cannot, structurally.**

The gate asks whether noise-driven **re-evaluations of the same
`config_hash`** consolidate to `HexValue::Contradictory`. That question
presupposes the same config is evaluated more than once in a run. It never
is.

`ix-autoresearch`'s grammar target perturbs by drawing a **continuous**
Gaussian on every rule weight and a **log-uniform** temperature
(`crates/ix-autoresearch/src/target_grammar.rs:173-199`):

```rust
let perturbed = w + z * self.weight_sigma;   // continuous, per weight
let log_t = log_lo + rng.random::<f64>() * (log_hi - log_lo);
let new_temperature = log_t.exp();           // continuous
```

Two candidates serializing byte-identically is a measure-zero event.
Confirmed empirically — 2000 perturbation steps from the smoke baseline at
seed 42:

```text
PROBE: iters=2000 distinct_configs=2000 repeats=0
```

So no claim in this pipeline ever receives a second observation. Without a
second observation there is no conflicting evidence about the same
proposition, and **`Contradictory` cannot arise organically** no matter how
much noise is injected. Injecting noise makes each single evaluation
*wrong more often*; it does not make any evaluation *disagree with a prior
evaluation of the same thing*, because there is no prior evaluation.

The `CacheBridge` does not indicate otherwise. It exists to serve
`resume_experiment` — cross-run reuse under a fixed seed — not intra-run
repeats. Its `cache_hit_count` on a fresh grammar run is 0 by construction.

### What this costs and what it saves

Two IX-side changes were pre-registered as needed (§9.1). Neither is:

- **Cache-disable already exists.** `Experiment::cache_salt() -> None`
  disables caching per-target, by design
  (`crates/ix-autoresearch/src/cache.rs:47-53`). No change required.
- **Noise injection needs no core change either.** `GrammarTarget` and its
  `Experiment` impl are fully public, so a noise wrapper is a newtype in a
  consumer crate.

**The cross-repo authorization this document spent §9.1 asking for was
never actually necessary.** Total cost of discovering the design is dead:
reading ~200 lines of `ix-autoresearch` and one 38-line throwaway probe,
since deleted. `ix` is unmodified. No corpus, no adapter crate, no
400 runs.

### The methodological point

§10 predicted the design's most likely failure was *a null against twenty
lines of SPRT* — a real but expensive result. The actual failure was
cheaper and more fundamental: **the phenomenon the study was designed to
measure does not occur in the system chosen to measure it.** §10 was
looking in the right direction (this pipeline has no organic epistemic
content) but understated the case — it argued the contradictions would be
*synthetic*, when in fact they would be *absent*.

This is the tracer-bullet discipline working exactly as CLAUDE.md claims it
should: the thin end-to-end slice surfaced a fatal unknown before any layer
was built out. Note that the fatal fact was available by *reading*, not
running. The gate could have been checked the day the design was drafted.

### What survives

- **The gate itself was correctly specified.** It named the load-bearing
  assumption precisely enough to be falsified by inspection. That is the
  design's one clear success and the pattern worth repeating.
- **The §8 guard condition** is untouched by this and still worth adopting.
- **The real constraint, now stated sharply:** any future "does Hari's
  contradiction-preserving layer help?" study needs a target where **the
  same proposition is observed more than once**. That is a hard filter on
  candidate targets and it eliminates every single-pass optimizer loop,
  which is what `ix-autoresearch` is. Repeated-observation structure is not
  a nice-to-have for this research program — it is the precondition.
- **Issue #13 should close** as *designed, gated, abandoned — reasons
  recorded* rather than remain `ready-for-human`.

The substrate question from `prior-art-survey.md` §6 Q5 remains open. This
document does not answer it. It removes one candidate way of answering it,
for a stated and checkable reason.
