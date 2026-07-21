# Hari on Hari — replaying the project's own research arc through the substrate

**Status: complete; the trace replays clean at HEAD and the report is
reproduced below.** Fixture: `fixtures/dogfood/2026-07-21-research-program.json`
(33 events). Regenerate with:

```bash
cargo run --release -p hari-core -- replay fixtures/dogfood/2026-07-21-research-program.json
```

This is a dogfooding exercise: instead of a synthetic scenario, the trace is a
faithful projection of Hari's **own** intellectual history over 2026-07-19 →
2026-07-20 — the abandoned substrate-role study (#13), the hex-merge algebraic
audit and its fixes, the propagation audit, the hex-merge/Subjective-Logic
correspondence result, and its supersession by escalation spec v1.2 (#28). Hari
tracks its own epistemic history, and we ask the one interesting question: **does
Hari's final belief state match the humans' final state?** Every divergence is
either a projection bug or a substrate finding, and each is classified below.

The exercise also lands on the day the belief-revision machinery it exercises
became fully committed: `retraction` (with selectors), `correction`,
`relation_withdrawal`, and `supersession` all reached the `ResearchEvent`
boundary at `7c99f90` (issue #16, slices 1–3). This is the first replay to drive
all four on real intellectual history rather than target-behavior fixtures.

## 1. Method — projection rules

Each real research event maps to one typed `ResearchEvent`. The mapping is
mechanical, and the reconstruction choices are called out honestly:

| History | Wire event | Rule |
|---|---|---|
| A claim asserted by a doc/module/design | `belief_update` | value = the claim's stance (`True` for an asserted theorem, `Probable`/`Unknown` for a hypothesis or conjecture) |
| A probe/measurement result | `experiment_result` | value = what the probe found; `source` = the probe |
| A hand-analysis error later measured the opposite | `correction` | retracts the mispredicting source, replaces with the measured value — the causal "this corrects that" link a bare `belief_update` cannot express |
| A designed study withdrawn before data | `retraction` (no selector) | whole-proposition withdrawal → recomputes to `Unknown` (withdrawn ≠ falsified) |
| A defect fixed so a claim now holds of the fixed code | `supersession` | the pre-fix claim is retired (frozen audit tail); a successor claim about the fixed code becomes the live head |
| A finding re-judged a defect and replaced by a ratified spec | `supersession` | v1.1 finding retired; v1.2 claim is the live head |
| An append-only relation that stopped holding | `relation_withdrawal` | tombstone the edge; propagation recomputes without it |

`source` names the real origin (`preregistration-design`,
`algebraic-audit-probe`, `sl-correspondence-probe`, `spec-v1.2-owner-ratification`,
…); `evidence` maps cite the actual doc section and commit SHA; `cycle` is a
sequential index (Hari's `ResearchEvent` carries no timestamp field, so real
commit times live in the evidence maps as `as_of`).

**Reconstruction choices, stated for audit.**

- **Assertion-then-refutation vs correction.** Where the project explicitly
  *corrected its own mistake* (the anti-dilution misprediction; the
  fixpoint-reconvergence conjecture) the event is a `correction` — the honest
  primitive, because a named prior source was replaced. Where two genuinely
  independent parties disagreed and a fix later re-established the claim (the
  module *claimed* commutativity; a probe *refuted* it) the events are a
  `belief_update` + `experiment_result` pair (which Hari resolves to
  `Contradictory`) followed by a `supersession` onto the fixed successor.
- **No `agent_vote`.** This arc had no consensus-vote events over the tracked
  claims. The pre-registration's "multi-agent scoring" selected a *research
  question*, not a belief about one of these propositions, so forcing an
  `agent_vote` would misrepresent the history. It is deliberately absent.
- **One derived node.** `hari-substrate-has-distinct-value` is never asserted
  directly. It is the target of a `Supports` edge — first from the empirical
  hypothesis (withdrawn when the study was abandoned), then re-founded on the
  formal `hex-merge-is-a-distinct-algebraic-family` result. Its value is
  *derived* by propagation, which is exactly how the value case actually shifted
  from the empirical to the formal track.

## 2. Variant breakdown (33 events)

| variant | count |
|---|---|
| `belief_update` | 13 |
| `experiment_result` | 5 |
| `correction` | 3 |
| `supersession` | 4 |
| `retraction` (no selector) | 1 |
| `relation_declaration` | 2 |
| `relation_withdrawal` | 1 |
| `goal_update` | 4 |
| **total** | **33** |

## 3. The replay report (key excerpts)

`final_beliefs` (14 directly-asserted propositions):

```
escalation-survives-corroboration-flooding:        True
fixpoint-divergence-is-transient:                   Doubtful
hex-escalation-immune-to-both-flooding-directions:  True
hex-merge-associative:                              Contradictory   (retired)
hex-merge-associative-purefold:                     True
hex-merge-equals-discretized-sl:                    False
hex-merge-is-a-distinct-algebraic-family:           True
hex-merge-permutation-invariant:                    Contradictory   (retired)
hex-merge-permutation-invariant-purefold:           True
hex-sl-form-an-exact-support-abstention-mirror:     True            (retired)
hexavalent-beats-sprt-on-ix-autoresearch:           Unknown
ix-grammar-loop-yields-organic-contradictions:      Doubtful
propagation-edge-order-independent-nary:            True
propagation-is-edge-order-independent:              Contradictory   (retired)
```

`final_goals`: `settle-substrate-question-q5 = True`,
`run-empirical-substrate-study = Doubtful`.

`revisions` (the belief-revision delta log — supersessions carry the successor):

```
c 6 correction     ix-grammar-loop-yields-organic-contradictions : Probable      -> Doubtful
c15 correction     escalation-survives-corroboration-flooding    : False         -> Probable
c16 supersession   hex-merge-permutation-invariant               : Contradictory -> Contradictory  [=> hex-merge-permutation-invariant-purefold]
c18 supersession   hex-merge-associative                         : Contradictory -> Contradictory  [=> hex-merge-associative-purefold]
c21 correction     fixpoint-divergence-is-transient              : Probable      -> Doubtful
c24 supersession   propagation-is-edge-order-independent         : Contradictory -> Contradictory  [=> propagation-edge-order-independent-nary]
c32 supersession   hex-sl-form-an-exact-support-abstention-mirror: True          -> True           [=> hex-escalation-immune-to-both-flooding-directions]
```

The derived node, from the cycle-29 `relation_declaration` provenance:

```json
{ "proposition": "hari-substrate-has-distinct-value",
  "previous_value": "Unknown", "new_value": "True",
  "contributions": [ { "source": "hex-merge-is-a-distinct-algebraic-family",
                       "source_value": "True", "relation": "Supports",
                       "contributed_value": "True" } ], "round": 1 }
```

`metrics`: `false_acceptance_count = 6`, `goal_completion_rate = 0.5`,
`consensus_stability = 0.80`, `contradiction_recovery_cycles = 1`, actions
`{Accept: 17, Escalate: 3, Investigate: 1, Log: 43, Retry: 1, Wait: 11}`. The
three `Escalate`s are exactly the three moments a claim went `Contradictory`
(commutativity, associativity, edge-order-independence, each contradicted by its
probe before the fix). `false_acceptance_count = 6` is Hari observing what a
research program in flux looks like from the inside: six beliefs it recommended
accepting were later overturned by revision — which is the correct read of this
arc, not a defect.

## 4. Does Hari's final state match the humans'?

**Yes, on 13 of 15 tracked propositions and both goals. The two divergences are
a single substrate finding.** The three load-bearing checks all pass:

- The abandoned study's hypothesis ends **`Unknown`** (withdrawn untested), not
  `False`.
- The ratified escalation spec v1.2 is the **live head** of its supersession
  chain; the v1.1 mirror is its frozen audit tail.
- The falsified-then-fixed conjectures end retired (frozen `Contradictory`) with
  their fixed successors live and `True`; the never-fixed negative result
  (`hex-merge ≠ SL`) ends cleanly `False`.

| Claim | True history | Hari final | Verdict |
|---|---|---|---|
| `hexavalent-beats-sprt-on-ix-autoresearch` (H1) | withdrawn **untested** at the tracer gate | `Unknown` | **match** — the target case |
| `ix-grammar-loop-yields-organic-contradictions` | **False** (impossible: configs never repeat) | `Doubtful` | **divergence** → substrate finding §5 |
| `run-empirical-substrate-study` (goal) | abandoned | `Doubtful` | match |
| `hari-substrate-has-distinct-value` | established, re-founded on the formal track | `True` (derived) | match (§5 report note) |
| `hex-merge-permutation-invariant` | over-broad claim, retired for the fixed version | `Contradictory`, retired | match |
| `hex-merge-permutation-invariant-purefold` | holds unconditionally post-fix | `True` (live) | match |
| `hex-merge-associative` | over-broad claim, retired | `Contradictory`, retired | match |
| `hex-merge-associative-purefold` | holds unconditionally post-fix | `True` (live) | match |
| `escalation-survives-corroboration-flooding` | anti-dilutive (misprediction corrected) | `True` | match |
| `fixpoint-divergence-is-transient` | **False** (conjecture falsified at trial 6) | `Doubtful` | **divergence** → substrate finding §5 |
| `propagation-is-edge-order-independent` | False pre-fix, retired | `Contradictory`, retired | match |
| `propagation-edge-order-independent-nary` | holds post-fix | `True` (live) | match |
| `hex-merge-equals-discretized-sl` | **False** (counterexample; a true negative) | `False` | match |
| `hex-merge-is-a-distinct-algebraic-family` | True | `True` | match |
| `hex-sl-form-an-exact-support-abstention-mirror` (v1.1) | correct finding, superseded by v1.2 | `True`, retired | match |
| `hex-escalation-immune-to-both-flooding-directions` (v1.2) | ratified live head | `True` (live) | match |
| `settle-substrate-question-q5` (goal) | answered — distinct | `True` | match |

## 5. The two divergences, classified

### 5.1 Substrate finding — the single-source corroboration cap under-rates deductive refutations

Both divergences are the **same** mechanism. `ix-grammar-loop-yields-organic-
contradictions` and `fixpoint-divergence-is-transient` were each *corrected* from
a positive stance to a negative one by a single authoritative source (the tracer-
gate impossibility argument; the propagation probe's trial-6 counterexample). The
`correction` path recomputes the belief from surviving evidence through
`recompute_from_ledger`, which applies the **single-source corroboration cap**:
a strong pole (`True`/`False`) asserted by only one distinct source downgrades to
the adjacent value. So `False`-from-one-source lands at **`Doubtful`**, and Hari
reads both refutations as "probably-not" rather than "no".

For the humans, both are as close to certain as this project's results get. The
grammar-loop premise is *impossible*, not merely improbable — proven by
inspection plus a `repeats=0` probe over 2000 iterations (a measure-zero
argument). The reconvergence conjecture was falsified by an explicit
counterexample. Neither is a noisy empirical single-source claim of the kind the
corroboration cap was designed to discipline.

- **Why this is a substrate finding, not a projection bug.** The cap is a real
  doctrine (`fixtures/revision/README.md`, merge-weight slice): "a lone
  uncorroborated surviving `True`/`False` downgrades to `Probable`/`Doubtful`".
  It is correct epistemic humility for *earned* empirical strength — one flaky
  benchmark should not assert `True`. But it has **no notion that some single
  sources warrant full strength**: a deductive impossibility proof and one noisy
  benchmark run are treated identically, because weight at the boundary is
  uniform `1.0` and confidence is counted as distinct-source cardinality, not
  source *kind*. The substrate cannot currently say "this one source is a proof."
- **The projection nuance, disclosed.** A second corroborating source on either
  claim would lift it to full `False` (this is exactly what happens to
  `escalation-survives-corroboration-flooding`: the `correction` recomputes to
  `Probable`, then the independent `sl-correspondence-probe` corroboration at
  cycle 27 combines up to `True`). We *chose* to model each refutation with its
  single real source rather than invent a second, because a mathematical proof
  genuinely has one author. The mismatch — that certainty-by-proof needs a
  second witness to reach full strength — is the finding, and it is worth a
  ROADMAP note against issue #14 (source reliability, "earned not declared"): a
  proof-carrying or high-reliability source should be able to license a strong
  pole alone.

### 5.2 Report-surface note — derived-only endpoints are absent from `final_beliefs`

Hari derives `hari-substrate-has-distinct-value = True` correctly: the cycle-29
provenance (§3) shows the `Unknown → True` derivation through the formal
`Supports` edge, after the empirical `Supports` edge was withdrawn at cycle 8 —
the value case shifting from the empirical to the formal track, faithfully. The
`final_state_summary` counts it ("15 beliefs" vs the 14 in `final_beliefs`). But
the `final_beliefs` map surfaces only propositions that were the *direct* subject
of a belief-bearing event, so a purely-derived relation endpoint does not appear
in the rollup. This is a report-completeness observation, not a reasoning error —
the derivation is present and correct in per-event provenance. Consumers that
need the full final belief set (including derived nodes) must read
`outcomes[*].derivations`, not `final_beliefs` alone.

## 6. What the substrate got right that is worth stating

- **Withdrawn ≠ falsified.** The abandoned study's hypothesis ends `Unknown`, not
  `False`. A no-selector `retraction` recomputes over an empty survivor set to
  `Unknown` — Hari correctly represents "we never found out" as distinct from "we
  found it false". This is the single most important epistemic distinction in the
  whole arc and the substrate gets it exactly right.
- **The audit trail survives the fix.** Every falsified-then-fixed claim keeps
  its retired head frozen at `Contradictory` with a supersession pointer to the
  live `True` successor. The chain `hex-merge-permutation-invariant (Contradictory,
  retired) → …-purefold (True, live)` is the recoverable history of "the original
  claim was contradicted; the fixed claim holds" — replay-to-cycle-N still shows
  the original as it stood.
- **A genuine negative result stays negative.** `hex-merge ≠ discretized SL` ends
  `False` and is never revisited — the one result in this arc with no fix and no
  supersession, correctly rendered as a durable negative.
- **Supersession is the anti-rot mechanism working on live history.** The v1.1
  mirror finding was true when written and is preserved as a frozen `True` tail,
  while the v1.2 ratification is the live head — exactly the retirement mechanism
  the design cites the Demerzel persona-count rot to motivate, here exercised on
  the project's own spec evolution.

## 7. Doctrine

This corpus is the substrate's memory of its own reasoning. **Sessions that
assert-and-then-falsify a conjecture should append the arc to
`fixtures/dogfood/`** (see its `README.md`), so the divergence between what the
humans concluded and what the substrate would conclude stays measured over time.
The single-source-cap finding (§5.1) is the first thing this exercise bought that
a synthetic fixture could not: it surfaced only because the real history contained
deductive refutations, which no target-behavior fixture models.
