# Recorded IX autoresearch runs through Hari: characterisation report

**Status:** instrument characterisation on recorded data. **This is not a §8
keep/kill input, makes no decision-quality claim, and must not be cited as
either.**

**Issue:** [#13](https://github.com/GuitarAlchemist/hari/issues/13). This report
and the sample run reports under `fixtures/ix-real-or-synthetic/` are the outputs
#13 names.

**Governing documents** — the §-numbers cited below refer to these:
- `2026-07-19-substrate-role-preregistration.md` — the §6 gate.
- `2026-07-28-ix-eval-preregistration.md` — §3 replay, §4 arms, §9.4 standing
  rule, §9.6. It is not amended here.

**Date:** 2026-09-14

## 0. What this is

Until this branch, no real IX autoresearch output had reached Hari. IX's
`crates/ix-autoresearch/SCHEMA.md` names a Hari consumer,
`hari-from-ix-autoresearch`, that did not exist. This branch builds that consumer
and runs real IX logs through it.

It answers one descriptive question: **what does each #35 arm decide on the stream
a real ix-autoresearch run emits, and how does that compare with the decision IX
itself made?**

It proposes no task distribution. It injects no noise. It does not touch the
pre-registration.

## 1. Real and synthetic inputs

| input | kind |
|---|---|
| `grammar-greedy-seed42.log.jsonl` | **real**: `ix-autoresearch run --target grammar --iterations 30 --strategy greedy --seed 42`, unedited |
| `grammar-sa-seed42.log.jsonl` | **real**: the same command with `--strategy sa`, unedited |
| `grammar-{greedy,sa,greedy-then-sa}-seed42.report.json` | derived: the committed output of `hari-from-ix-autoresearch --report` on the real logs |
| conflicting-repeat stream in `a_conflicting_repeat_of_one_config_ends_contradictory` | **synthetic**: built in memory in the test by splicing a copy of iteration 1, with its accept flag flipped, into the real Greedy log; it is never committed as a fixture |

**Where the logs were recorded:** `git_sha` `d922cbd`, the branch of
GuitarAlchemist/ix#339. That branch changes only a contract test and
`SCHEMA.md`; the kernel and the grammar target are identical to `main` @
`73b78bf`.

To reproduce a report:

```bash
cargo run -p hari-extractor --bin hari-from-ix-autoresearch -- \
  --report fixtures/ix-real-or-synthetic/grammar-sa-seed42.log.jsonl
```

Without `--report`, the command emits the `ResearchTrace`. That trace replays with
`hari-core replay` and streams through `hari-core serve`.

## 2. The boundary

**Projection** (`crates/hari-extractor/src/ix_autoresearch.rs`, SCHEMA.md layer
2). Each `iteration` becomes one `experiment_result`:

- **proposition:** `target_grammar/config-{hash12}-is-an-improvement`
- **value:**
  - `Probable` when IX accepted
  - `Doubtful` when IX rejected
  - `Unknown` when the evaluation errored. SCHEMA.md pegs an errored line at
    confidence 0.10; Hari reads it as no evidence either way.
- **source:** `ix-autoresearch/{run_id}`
- **cycle:** the event's position in the stream
- **evidence:** the raw fields

**Arms**, as defined in §4:

| arm | what it is |
|---|---|
| `ix_policy` | IX's own `accepted` flag, read off the raw log |
| `ix_unassisted` | Hari's pass-through null baseline |
| `recency_decay` | the shipped default |
| `subjective_logic` | the cheap baseline |

`Lie` is not an arm (§4).

**Decision per claim.** Each arm's action on a claim is classified as one of three
decisions:

| decision | the arm emitted |
|---|---|
| *endorse* | `Accept` at `True` or `Probable` |
| *reject* | `Accept` at `Doubtful` or `False` |
| *withhold* | anything else |

**Descriptive label.** A claim is *improved* when the candidate's reward beats its
incumbent's reward. The incumbent is the previous line's `previous_hash`.
Iteration 0 is unlabeled.

- This rule is a disclosed description, not a pre-registered ground truth.
- SA accepts worse configs by design, so an SA "false endorsement" is exploration,
  not an error.

## 3. What the instrument shows

### 3.1 Greedy, 30 iterations (IX accepted 3)

| arm | endorse | reject | withhold | differs from IX | endorsed, not improved | improved, not endorsed | Contradictory |
|---|---|---|---|---|---|---|---|
| ix_policy | 3 | 27 | 0 | — | 0 | 0 | — |
| ix_unassisted | 3 | 27 | 0 | 0 | 0 | 0 | 0 |
| recency_decay | 3 | 27 | 0 | 0 | 0 | 0 | 0 |
| subjective_logic | 0 | 0 | 30 | 30 | 0 | 2 | 0 |

### 3.2 Simulated annealing, 30 iterations (IX accepted 6)

| arm | endorse | reject | withhold | differs from IX | endorsed, not improved | improved, not endorsed | Contradictory |
|---|---|---|---|---|---|---|---|
| ix_policy | 6 | 24 | 0 | — | 1 | 0 | — |
| ix_unassisted | 6 | 24 | 0 | 0 | 1 | 0 | 0 |
| recency_decay | 6 | 24 | 0 | 0 | 1 | 0 | 0 |
| subjective_logic | 0 | 0 | 30 | 30 | 0 | 4 | 0 |

### 3.3 Both runs as one stream (60 claims)

- **Propositions:** 57 distinct. **3 are repeated** (the early iterations, before
  Greedy and SA diverge at a shared seed), and **0 conflict**.
- **Contradictory beliefs:** every arm ends with 0.

### 3.4 Synthetic only: a conflicting repeat

When the test splices a flipped repeat into a real log:

- `recency_decay` ends that proposition `Contradictory` and escalates instead of
  rejecting.
- `ix_unassisted` does neither.

**No recorded run produces this case.**

## 4. Reading

1. **On these recorded streams, `recency_decay` and `ix_unassisted` reproduce
   IX's own decision at every claim** (60 of 60). Every claim is a single
   observation stamped on arrival, so the hexavalent arm's value is IX's flag.
   This matches the mechanism §9.6 describes; it is observed here on recorded
   IX output.
2. **No config repeats within a run.** This is the precondition the §6 gate
   (2026-07-19) identified, now pinned by a test on both sides. Contradiction
   preservation is therefore unobserved on real IX data. It is exercised only on
   the synthetic stream in §3.4.
3. **`subjective_logic` withholds on every single-observation claim.** One
   observation fuses to `b = 0.55`, below its 0.7 accept gate.
4. **Cost of the boundary:** a 475-line module, a 57-line binary and 227 lines of
   tests. Replaying a 30-iteration run takes well under a second.

## 5. Limits

1. The data covers one target, one seed and two strategies, 30 iterations each.
   That is not a population, and §9.4's standing rule applies unchanged.
2. The target is deterministic.
3. The *improved* label is descriptive. SA's exploration contradicts it by design.
4. `ix_policy` is IX's decision recorded once. No loop in this branch follows
   Hari's recommendation (#13's candidate flow, step 5).
