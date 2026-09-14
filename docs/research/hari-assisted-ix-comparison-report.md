# Recorded IX autoresearch runs through Hari: characterisation report

**Status:** instrument characterisation on recorded data. **This is not a §8
keep/kill input and must not be cited as one.**
**Issue:** [#13](https://github.com/GuitarAlchemist/hari/issues/13) (outputs named
there: this report, sample run reports under `fixtures/ix-real-or-synthetic/`)
**Governing documents:** `2026-07-19-substrate-role-preregistration.md` (§6 gate,
§11 kill), `2026-07-28-ix-eval-preregistration.md` (§4 arms, §9.4 standing rule,
§9.6), `2026-08-09-ix-eval-paired-bootstrap-report.md`
**Date:** 2026-09-14 · **Repo state:** `main` @ `0f17b6b` plus this branch ·
**IX state:** `GuitarAlchemist/ix` @ `73b78bf`

---

## 0. What this is, and what it is not

Until this branch, no real IX autoresearch output had ever reached Hari. Every
Hari number came from authored fixtures or `paired_driver.py`'s generated
corpus. IX's `crates/ix-autoresearch/SCHEMA.md` names a Hari consumer,
`hari-from-ix-autoresearch`, that did not exist, and the ROADMAP Phase-6 ⏸ item
("requires real IX-side autoresearch … against real benchmarks") was open for the
same reason.

This branch builds that consumer and runs real IX logs through it. It answers a
narrow question: **what does each #35 arm decide when it receives the stream a
real ix-autoresearch run actually emits?**

It does **not** do the following:

- It does not issue a §8 verdict. The task distribution is still unratified
  (§9.4).
- It does not pre-register the label rule in §2. That rule is a disclosed
  candidate.
- It does not inject noise. The 2026-07-19 §10 objection to injecting noise into a
  deterministic target stands, and nothing here reopens it.

## 1. The scenario

| | |
|---|---|
| IX target | `ix_autoresearch::target_grammar::GrammarTarget` (`default_smoke`: 6 rules, deterministic in-process eval) |
| Runs | `ix-autoresearch run --target grammar --iterations 100 --seed 42`, once with `--strategy greedy` and once with `--strategy sa` |
| Recorded at | IX `73b78bf` (stamped as `git_sha` in each log) |
| Fixtures | `fixtures/ix-real-or-synthetic/grammar-{greedy,sa}-seed42.log.jsonl` — raw logs, unedited |
| Run reports | `grammar-greedy-seed42.report.json`, `grammar-sa-seed42.report.json`, `grammar-greedy-then-sa-seed42.report.json` (both logs as one stream) |

Reproduce a report:

```bash
cargo run -p hari-extractor --bin hari-from-ix-autoresearch -- \
  --report fixtures/ix-real-or-synthetic/grammar-sa-seed42.log.jsonl
```

Drop `--report` to get the `ResearchTrace` instead. That trace replays with
`hari-core replay` or streams through `hari-core serve`.
`committed_run_reports_regenerate_from_committed_logs` pins that each committed
report is exactly what its log produces.

## 2. The boundary

**Projection** (`crates/hari-extractor/src/ix_autoresearch.rs`, SCHEMA.md layer 2)

Each `iteration` line becomes one `experiment_result`:

- **Proposition:** `target_grammar/config-{hash12}-is-an-improvement`
- **Value:** `Probable` if IX accepted, `Doubtful` if IX rejected, `Unknown` if the eval errored
- **Source:** `ix-autoresearch/{run_id}`
- **Cycle:** the event's position in the stream
- **Evidence:** the raw fields

On errors, this departs from SCHEMA.md. SCHEMA.md pegs an errored line at 0.10.
Hari reads a failed eval as no evidence about the config, not as evidence against
it.

**Arms**, per §4 of the 2026-07-28 pre-registration:

| arm | what it is |
|---|---|
| `ix_policy` | IX's own `accepted` flag, read off the raw log — the run *without* Hari |
| `ix_unassisted` | Hari's pass-through null baseline (`unassisted.rs`) |
| `recency_decay` | the shipped default, §9.5's `experimental` |
| `subjective_logic` | the cheap baseline |

`Lie` is not reported, because §4 excludes it.

**Decision per claim.** A claim is *endorse* when the arm emits `Accept` at
`True`/`Probable`. It is *reject* when the arm emits `Accept` at
`Doubtful`/`False`. It is *withhold* otherwise.

**Candidate label (not pre-registered).** A claim is *improved* when the
candidate's reward beats the reward of the incumbent it was proposed against.
The incumbent is the previous line's `previous_hash`. Iteration 0 is unlabeled,
because the log does not carry the baseline's reward.

- A *false endorsement* endorses a claim that is not improved.
- A *missed improvement* fails to endorse a claim that is improved.

The label is derived from recorded rewards and is never sent to Hari.

## 3. Results

### 3.1 Greedy, 100 iterations (IX accepted 5)

| arm | endorse | reject | withhold | differs from IX | false endorsements | missed improvements | Contradictory |
|---|---|---|---|---|---|---|---|
| ix_policy | 5 | 95 | 0 | — | 0 | 0 | — |
| ix_unassisted | 5 | 95 | 0 | 0 | 0 | 0 | 0 |
| recency_decay | 5 | 95 | 0 | 0 | 0 | 0 | 0 |
| subjective_logic | 0 | 0 | 100 | 100 | 0 | 4 | 0 |

### 3.2 Simulated annealing, 100 iterations (IX accepted 13)

| arm | endorse | reject | withhold | differs from IX | false endorsements | missed improvements | Contradictory |
|---|---|---|---|---|---|---|---|
| ix_policy | 13 | 87 | 0 | — | 2 | 0 | — |
| ix_unassisted | 13 | 87 | 0 | 0 | 2 | 0 | 0 |
| recency_decay | 13 | 87 | 0 | 0 | 2 | 0 | 0 |
| subjective_logic | 0 | 0 | 100 | 100 | 0 | 10 | 0 |

IX's 2 false endorsements are SA's exploratory accepts of worse configs. That is
the job of an annealing policy, not an error. Grading them against
*is-an-improvement* measures the claim wording SCHEMA.md chose as much as it
measures IX.

### 3.3 Both runs as one stream (200 claims)

- There are 197 distinct propositions. **3 are repeated**: the first iterations,
  before Greedy and SA diverge at a shared seed.
- **0 of the repeats conflict.**
- Every arm ends with 0 `Contradictory` beliefs.
- The decision table is the sum of §3.1 and §3.2, except SL, which misses 14
  improvements.

### 3.4 The one thing no recorded run exercises

`a_conflicting_repeat_of_one_config_ends_contradictory` builds a stream in
memory: a recorded log plus a repeat of iteration 1 with the accept flag
flipped. On that stream:

- `recency_decay` ends that proposition `Contradictory`.
- `recency_decay` escalates on the repeat instead of rejecting it.
- `ix_unassisted` does neither.

So SCHEMA.md's "contradictory findings preserved" criterion holds in Hari. It is
simply never triggered by this target.

## 4. What the results say

1. **No decision-quality gain, and none is possible on this stream.**
   `recency_decay` and `ix_unassisted` both reproduce IX's decision at 200 of 200
   claims. Every recorded claim is a single observation stamped at its arrival.
   So the hexavalent arm's value is IX's flag, and its action is `Accept` at that
   value. This is §9.6's mechanism, observed on recorded IX output rather than on
   generated traces.
2. **The §6 gate of 2026-07-19 is confirmed, and is now a live test rather than a
   deleted probe.** Within a run, 100 of 100 configs are distinct.
   Contradiction preservation, the substrate's defining feature, has nothing to
   act on.
3. **SL's apparent safety is abstention.** One observation fuses to `b = 0.55`,
   below SL's 0.7 accept gate, so SL commits to nothing on a single-pass stream.
   Its 0 false endorsements are paid for with every real improvement missed. This
   is the §9.3.2 always-`Wait` degeneracy again, now on real input. The §5.3
   caution-tax rule treats it the same way: it is not a win.
4. **Extra complexity, the other half of #13's acceptance criterion.** The
   adapter adds a 475-line module, a 57-line binary and 227 lines of tests. Replay of a
   100-iteration run takes well under a second. Against zero decision change on
   this target, that complexity buys:
   - the boundary itself,
   - the gate as a binding,
   - a working path for any future target that *does* re-observe a claim.

   It does not buy a better accept decision.

## 5. Limits

1. One target, one seed, two strategies. This is not a population, and §9.4's
   standing rule is not tested by it.
2. The target is deterministic. The flaky-vs-real task of 2026-07-28 §2 cannot be
   posed on it without injected noise, and this branch declines to inject any.
3. The label is a candidate rule that SA's exploration violates by design (§3.2).
4. `ix_policy` is IX's decision recorded once. It is not a counterfactual IX run.
   "IX followed Hari's recommendation" (#13's candidate flow, step 5) is not
   exercised, because Hari's recommendation never reached the running loop.

## 6. What would change the picture

The constraint from 2026-07-19 §11 still decides everything: **a claim has to be
observed more than once.**

Within IX, two sources fit that constraint without changing the kernel:

- **Repeated evaluation of a fixed config.** A discrete target, or
  re-evaluation after `resume_experiment`.
- **Several strategies or seeds that assert overlapping configs.** §3.3 shows
  this happens, but only 3 times in 200, and those 3 never disagree.

Choosing either one is a task-distribution decision, and §9.4 makes that the
owner's call. This report does not make it.
