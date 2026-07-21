# Belief revision, retraction, and relation withdrawal — design

**Status: design ratified (owner-delegated this session, 2026-07-20). This
is a DOCS + FIXTURES slice — no core Rust changes. `max_autonomy: draft`
per issue #16. The implementation slice is scoped in §9; the fixtures in
`fixtures/revision/` are target-behavior fixtures, not yet consumed by
`hari-core replay`.**

Issue: `GuitarAlchemist/hari#16` (parent epic `#12`). Companion contract:
`docs/contracts/retraction-events.contract.md`. This design ratifies
**evidence-recompute as authoritative** and shows that under that doctrine
retraction, correction, relation withdrawal, and supersession all reduce
to one mechanism — *append a tombstone, recompute from surviving
evidence* — with no cascade logic.

## 0. The one-sentence design

A retraction never erases anything: it appends a tombstone that filters a
named piece of evidence out of the **current-belief** recompute while
leaving it in the trace **for audit**, and because every derived belief is
recomputed from surviving base evidence on every replay (never carried
forward), retraction of a parent dissolves its derivations automatically.
This design **ratifies evidence-recompute as authoritative** — the
substrate already implements this doctrine in `hari-lattice::merge`
(commit `924b5a4`); belief revision is that same doctrine lifted to the
`ResearchEvent` boundary.

## 1. Why this is decidable now (not open)

Issue #16's core question was sharpened by the merge algebraic audit
(`docs/research/2026-07-20-hex-merge-algebraic-audit.md` §4) into:

> **What is the lifecycle of a *derived* contradiction when the evidence
> beneath it is withdrawn or expires?**

with two mutually exclusive answers:

1. **Evidence-recompute is authoritative** — derived observations are pure
   cache; retraction/expiry of a parent dissolves the derived `C`.
2. **Derived observations are first-class evidence** — the record "these
   sources conflicted at round N" survives its parents; carried state and
   recompute permanently diverge.

The audit's fix commits (`924b5a4`) **implemented option 1**, and did so
not by preference but because option 2 was shown *not locally patchable*:
a third defect proved that any carried derived state can go stale against
its own evidence, so making derivations first-class would require
versioning every derivation against its parents — a much larger design
that the issue's non-goals explicitly warn against ("do not implement
complex multi-premise logic in the same slice"). The three properties now
pinned in `crates/hari-lattice/tests/algebra_probe.rs`:

- synthesized `C` stamped `round = min(parents)` — expires exactly when
  the pair stops coexisting;
- incoming `MERGE_SOURCE` (derived) observations dropped on input and
  re-derived from base evidence every merge;
- **carried state == evidence recompute** under 1500 randomized staleness
  windows, plus unconditional permutation-invariance and associativity.

**This design ratifies option 1** as the belief-revision doctrine and
defends it (§3) against the objection that it violates
contradiction-preservation. The owner may still veto in favour of
contradiction-immortality; §3.3 states what that reversal would cost.

## 2. Scope: four event types, three deferred

The issue lists seven candidate event types. This design specifies **four**
and defers **three**, per tracer-bullet discipline (the smallest
end-to-end slice that touches every layer) and the issue's non-goal
against over-building.

### Specified

| Event | Wire tag | Shape (target) | What it does |
|---|---|---|---|
| **Retraction** | `retraction` | `{ proposition, reason, retracts? }` | Filters named prior evidence (or all evidence for the proposition) out of current-belief recompute; evidence preserved for audit. |
| **Correction** | `correction` | `{ proposition, reason, retracts, value, evidence }` | Atomic retraction-plus-replacement: "that result was wrong; here is the corrected one." Causally links the withdrawn and replacement evidence. |
| **RelationWithdrawal** | `relation_withdrawal` | `{ from, to, relation, reason }` | Tombstones a previously-declared relation edge; propagation recomputes without it. Makes relations non-append-only for the first time. |
| **Supersession** | `supersession` | `{ proposition, superseded_by, reason }` | Retires a whole claim in favour of a successor claim; records a directed `A → superseded_by → B` lineage edge. The superseded claim freezes (no downstream mass) but stays inspectable. |

`Retraction` already exists in the enum as `{ proposition, reason }`; this
design **adds an optional `retracts` field** (backward-compatible) and
**changes its execution** from "mutate the belief node to `Unknown`" to
"filter evidence and recompute" (§4). The other three are new variants.

### Deferred, with justification

- **RelationReplacement** — exactly `RelationWithdrawal(old)` composed with
  `RelationDeclaration(new)`. No new semantics; a consumer expresses it as
  two events. Adding a dedicated variant would violate the issue's
  over-building non-goal.
- **SourceReliabilityUpdate** — reliability is **earned from graded
  outcomes, not declared by an event**. It is already owned by the
  source-reliability ledger (#14, `docs/contracts/source-reliability-summary.contract.md`),
  which is the *epistemic-entrenchment ordering* this design leans on in
  §5. Making it an inbound event would invert the design (declared vs
  earned trust). Redirect to #14.
- **StalenessObservation** — staleness is **already implemented** as the
  round-based `K`-window in `hari-lattice::merge` (`DEFAULT_STALENESS_K`).
  The only thing an explicit event would add — marking one observation
  stale *before* its window expires — is a special case of `Retraction`
  with `reason: "stale"`. Fold it into `Retraction`; do not duplicate the
  mechanism.

## 3. The ratification and its defense

### 3.1 The doctrine

**Evidence-recompute is authoritative.** The authoritative value of any
belief — base or derived — is the result of recomputing over the set of
*surviving* (non-retracted, in-window) base evidence. A carried
`MergedState`, a cached derivation, or a hand-maintained snapshot is a
**cache** that may be invalidated by later evidence (staleness expiry,
key-collision resolution, or an explicit retraction). When cache and
recompute disagree, **recompute wins**.

### 3.2 The objection, and why it does not land

**Objection.** Hari's epistemic-humility philosophy (CLAUDE.md,
`hari-lattice`) says irreconcilable evidence must be *preserved*, never
silently collapsed — `Contradictory` sits outside the truth chain as an
absorbing value precisely so a conflict cannot be washed out. But option 1
lets a derived `C` *vanish* when a parent is retracted. Isn't that the
silent erasure the doctrine forbids?

**Resolution — distinguish two things the objection conflates:**

- **Contradiction-preservation** (adopted): *while the conflicting
  evidence stands*, the `C` cannot be diluted, hidden, or out-voted. This
  is a real, proven property — the merge audit's **anti-dilution theorem**
  (§2.3): an escalated contradiction rises monotonically toward its
  asymptote under corroboration-flooding and cannot be muted while the
  dissent stands. Standing conflict is immortal *as long as it stands*.
- **Contradiction-immortality** (rejected): the *record* "these sources
  conflicted at round N" must survive **even after** the evidence is
  legitimately withdrawn.

Retraction acts only on the second. When a source says "my False reading
used a stale config snapshot — withdraw it," the conflict is **genuinely
resolved**: there is no longer a live disagreement, so dissolving the
derived `C` is *correct*, not erasure. Nothing is silently collapsed —
the retraction event and the retracted evidence both remain in the
immutable trace. What changes is only *which evidence is live*.

**Audit is served by the trace, not by immortal derivations.** The fact
that a conflict once existed is fully recoverable: replay the trace to
round N (before the retraction) and the historical `C` reappears, because
the retracted evidence is tombstoned, not deleted. Replay to *now* reflects
the withdrawal. This is exactly the **current-belief vs historical-belief**
distinction the issue's acceptance criteria demand (§6):

- **current belief** = replay to HEAD with retraction filters applied;
- **historical belief** = replay to cycle N — reproducible forever, because
  retraction never rewrites the past.

So the doctrine that survives is: **preserve standing contradictions
absolutely; let legitimately-withdrawn contradictions dissolve; keep the
full history of both in the trace.** That is contradiction-preservation
done right, not a violation of it.

### 3.3 The cost of the alternative (for the owner's veto)

If the owner wants contradiction-immortality instead — "the conflict was
observed; its record must outlive its evidence" — that is a deliberate
reversal of `924b5a4`'s doctrine, not a patch. It requires: versioning
every derivation against the exact parent set that produced it; a rule for
whether explicit retraction cascades into or is blocked by first-class
derivations; and acceptance that carried state and evidence recompute
**permanently diverge**, which breaks the replay-is-authoritative doctrine
in CLAUDE.md and the pinned merge theorems. The merge audit found this is
not locally achievable. Recommendation: **ratify option 1.**

## 4. Semantics — the five behaviors, realized

The issue's five candidate semantics all hold; here is the mechanism for
each.

1. **append_retraction_event_without_erasing_history** — a retraction is an
   *appended event* in an immutable trace, and at the merge layer a
   *tombstone observation* added to the G-Set. Nothing is removed. This
   keeps the CRDT character: the observation set grows monotonically (adds
   + tombstones, a 2P-Set-shaped structure that is still a CRDT). Removal
   would break the merge's order-independence proofs; **tombstone, never
   delete** (resolving the audit's open question (a)).
2. **mark_prior_evidence_as_retracted_for_current_belief** — a retraction
   references the evidence it retracts by dedup key
   `(source, diagnosis_id, round, ordinal)` (or, at the `ResearchEvent`
   boundary, by proposition + optional cycle/source selector). With no
   selector it retracts *all* prior evidence for the proposition
   (backward-compatible with today's whole-proposition reset, but via the
   filter path, not a mutation to `Unknown`).
3. **preserve_retracted_evidence_for_audit** — tombstoned evidence stays in
   the observation set and in the lineage export (#15), flagged
   `retracted: true`, contributing **zero mass** to the current
   distribution but fully visible (resolving the audit's open question
   (b): yes, retracted evidence stays in the evidence maps).
4. **recompute_current_belief_with_retraction_filter** — current belief =
   `merge(base_evidence − tombstoned, current_round, K)`. Because
   derivations are re-derived from surviving base evidence every merge and
   never carried, **retraction of a parent dissolves its derived `C`
   automatically — no cascade logic** (the key consequence of option 1).
5. **report_delta_between_previous_and_current_belief** — the replay report
   emits a delta `(previous_value → new_value, cause: retraction/correction/
   supersession)` whenever a revision event changes a belief. This reuses
   the existing `is_revised_by` lineage edge (#15) — a retraction,
   correction, or supersession *is* a revision.

### 4.1 Per-event execution (target)

- **Retraction** — append tombstone(s) for the named evidence; recompute
  the proposition (and, transitively, any derivation that used it as a
  parent — for free, since derivations aren't carried). Delta reported.
  Replaces today's `value = Unknown` mutation.
- **Correction** — tombstone the named evidence *and* append the
  replacement `(value, evidence)` in one atomic, causally-linked event.
  Equivalent to `Retraction` + `BeliefUpdate`, but the link "B corrects A"
  is itself audit-worthy provenance and is recorded as such, so a bare
  `BeliefUpdate` is not a substitute.
- **RelationWithdrawal** — tombstone the relation edge in the
  `BeliefNetwork`; propagation recomputes without it. Relations were
  append-only (see the `RelationDeclaration` doc-comment in `lib.rs`); this
  is their first withdrawal path, and it follows the *same* recompute
  doctrine — the propagation audit
  (`docs/research/2026-07-20-demerzel-belief-replay.md` §2) established
  propagation is evidence-recompute-shaped, so a withdrawn relation
  dissolves the beliefs it induced exactly as a retracted observation
  dissolves its derivations.
- **Supersession** — record `proposition → superseded_by` as a directed
  lineage edge and freeze the superseded proposition: it contributes no
  mass to any *downstream* (dependent) belief, but its own value at each
  historical point stays replayable. The head of the chain is the only
  live claim; the tail is the audit trail.

## 5. Prior art — what we take, what we refuse

The issue asks for a comparison against AGM, iterated/filtered/
paraconsistent revision, and truth-maintenance systems, and explicitly
warns against picking full AGM without weighing it against Hari's
hexavalent/paraconsistent goals. The design is a **principled hybrid**:
JTMS mechanism + AGM entrenchment, minus AGM's consistency-restoration.

### 5.1 JTMS foundationalism = evidence-recompute (the mechanism)

Doyle's Justification-based Truth Maintenance System (1979) gives every
belief a *justification* tracing to a foundation of base facts; a belief is
labelled **in** or **out** according to whether its justifications
currently hold, and withdrawing support retracts the belief automatically
by dependency-directed backtracking. **Derived beliefs have no standing of
their own — they live and die by their support.**

Evidence-recompute-authoritative *is* JTMS foundationalism, exactly (this
correspondence is noted independently in
`docs/research/2026-07-20-compounding-strategy.md` §3, calibration track):

- `MERGE_SOURCE` syntheses = derived beliefs with no independent standing;
- "dropped on input, re-derived every merge" = recomputing the in/out
  labelling from the current justification structure;
- "dissolves when a parent is retracted, no cascade" = dependency-directed
  retraction, which in a pure recompute is free — you don't chase the
  dependency graph, you just recompute.

The rotted Demerzel `behavioral-test-coverage` belief (T @ 0.8 held for
four months while carrying its own contradicting evidence —
`demerzel-belief-replay.md` §1) is *the exact failure JTMS was invented to
prevent*: a derived conclusion outliving its justification. Supersession
(§4.1) is the retirement mechanism that would have caught it.

### 5.2 AGM coherentism — refused for the operator, kept for the ordering

AGM (Alchourrón–Gärdenfors–Makinson 1985) is **coherentist**: it revises a
belief *set* to restore consistency by *minimal change*, using an
**epistemic-entrenchment** ordering to decide what to give up. It tracks no
justifications and operates on the set as a whole.

We **refuse AGM's operator.** Minimal-change-to-restore-consistency
actively *hides* contradictions — revision produces a consistent set — which
is the opposite of the hexavalent goal of holding `Contradictory` as a
first-class, preserved value. Full AGM would also discard the audit trail
(revision yields a new set, not a log) and demand a total entrenchment
ordering over *all* beliefs. All three conflict with the issue's non-goals.

But foundationalist recompute has a **known gap** (compounding-strategy §5):
it answers "is this derived belief supported?" yet **cannot rank conflicting
*base* evidence** — when two standing sources genuinely disagree, recompute
faithfully yields `C` but cannot say who is more likely right. That ranking
is *precisely* AGM's epistemic entrenchment. Hari already has the ordering,
learned rather than declared: the **per-source reliability ledger (#14) is
the entrenchment relation** — source reliability = entrenchment, earned from
graded outcomes. So the full picture:

- **JTMS foundationalism** governs the *derivation lifecycle* (retraction,
  this design);
- **AGM-style entrenchment**, via reliability cards, governs *conflicting-
  base resolution* (#14, a later consumer);
- **AGM's consistency-restoration is rejected** — the contradiction stays
  `C` until real evidence moves, entrenchment only *weights* it.

This also closes the loop with `hari-swarm` trust models: entrenchment is
trust, learned from outcomes instead of declared.

### 5.3 Paraconsistent / iterated / filtered revision

- **Paraconsistent belief revision** is the closest fit — it tolerates
  inconsistency rather than exploding on it, which is Hari's `Contradictory`
  value. Hari's mechanism is *simpler than* paraconsistent revision
  operators: it doesn't revise a belief set under a paraconsistent
  entailment, it recomputes a distribution from an evidence log. We take the
  *spirit* (inconsistency is representable, not fatal) without the operator.
- **Iterated revision** (Darwiche–Pearl) concerns how repeated revisions
  compose; Hari sidesteps the iteration postulates entirely because there is
  no stateful revision operator to iterate — every belief is a pure function
  of the current evidence multiset, so composition is just "recompute
  again." The merge associativity/idempotence theorems are the
  order-independence that iterated revision works hard to axiomatize.
- **Filtered belief revision** (revise only within a relevance filter) maps
  onto the staleness `K`-window and the `retracts` selector: both are
  filters over which evidence participates. We adopt the filter idea; the
  filters are syntactic (round, dedup key), not a relevance logic.

### 5.4 Self-conflicting source, recast as retraction doctrine

`hari-lattice::merge::resolve_key_versions` already handles a source that
emits **divergent payloads under one dedup slot**: it resolves to
`Contradictory` at minimum weight rather than picking a winner. Read through
the revision lens, this is a **refusal of silent retraction**: last-write-
wins under a key would be an *implicit* retraction of the earlier write, and
the merge deliberately refuses it — retraction must be **explicit** (a
`Retraction`/`Correction` event, or a tombstone), never a side effect of key
collision. This is the same doctrine as §4 and the direct reason the naive
baseline in §7 is wrong.

## 6. Deterministic replay and lineage compatibility (acceptance criterion)

- **Deterministic replay** — revision events are appended to the trace and
  re-run by `hari-core replay`. Because recompute is order-independent (the
  pinned merge theorems: permutation invariance, associativity,
  carried == recompute), replaying the same trace always yields the same
  current belief. The trace stays the immutable log; retraction adds to it,
  never rewrites it.
- **Current vs historical, explicit** — one trace, two questions answered by
  choice of replay endpoint: replay-to-HEAD (current, filters applied) vs
  replay-to-cycle-N (historical, as it stood). A report emitted at cycle N is
  reproducible forever.
- **Evidence-lineage export (#15)** — additive, per that contract's
  additive-evolution principle: retracted `source_item`/`claim`/
  `experiment_event` nodes gain an optional `retracted: true`; the revision
  is recorded with the existing `is_revised_by` edge plus a new
  `is_retracted_by` edge (evidence → revision event). Retracted nodes stay in
  the bundle (preserve-for-audit). No `lineage_version` major/minor bump is
  forced — new optional fields only.

## 7. The A/B baseline any implementation must beat

Roadmap doctrine: every milestone is testable against a simpler baseline.
The baseline for retraction is **naive last-write-wins** — which is *exactly
what the current `Retraction` handler does*: on retraction, set the belief to
`Unknown` (`lib.rs:1347`). Any real implementation must beat it on:

| Axis | LWW-to-Unknown baseline | Evidence-recompute (this design) |
|---|---|---|
| **Audit** | loses the retracted evidence's contribution; belief just flips to `Unknown` | retracted evidence tombstoned, still visible, zero mass |
| **Derivation correctness** | touches only the named proposition; a derived `C` that depended on the retracted evidence survives stale | derivation dissolves automatically (parent gone, recompute) |
| **Partial retraction** | resets to `Unknown` even when other supports stand | downgrades (e.g. T→P) — belief survives on remaining evidence |
| **Determinism** | order-sensitive (last write wins) | order-independent (merge theorems) |

**The A/B metric.** Reuse the existing `false_acceptance_count` /
`contradiction_recovery_cycles` from `ResearchReplayReport::metrics`, and add
one **retraction-fidelity** check: *does the post-retraction belief equal a
from-scratch recompute over the non-retracted evidence?* The LWW baseline
diverges (it mutates one node); the target matches **by construction** — it
*is* that recompute. The three fixtures in `fixtures/revision/` are the
concrete A/B cases (§8).

## 8. Fixtures (target-behavior, in `fixtures/revision/`)

Three deterministic replay fixtures, each isolating one semantic and each an
A/B case against the LWW baseline. They use the **proposed** wire payloads
(`retraction` with `retracts`, `correction`, `supersession`) that
`hari-core replay` does **not yet consume** — they are the implementation
slice's acceptance targets, not currently-green fixtures. Expected semantics
per fixture are in `fixtures/revision/README.md`.

1. **`retraction_dissolves_derived_contradiction.json`** — A asserts True, B
   asserts False → derived `C` (Escalate). B retracts its False evidence →
   recompute over {True} → True; the `C` dissolves; the retracted False stays
   flagged. Historical replay-to-conflict still shows `C`.
2. **`partial_retraction_downgrades.json`** — two sources both assert True
   (corroborated). One support is retracted → belief survives on the
   remaining single source, **downgraded** T→P (single-source cap), **not**
   reset to `Unknown`. This is the row the LWW baseline gets wrong.
3. **`supersession_chain.json`** — a claim evolves `14 → 20 → 27` (mirroring
   the Demerzel persona-count rot). Only the chain head is live; the
   superseded claims are retired but the whole chain is inspectable, and
   replay-to-an-earlier-point shows the older claim as it stood.

### 8.1 Addendum (merge-weight slice, 2026-07-20): fixtures 1 & 2 both dissolve to `Probable`

The two fixtures above were *inconsistent as originally written*, and the
merge-weight slice resolved that inconsistency uniformly. Fixture 1 expected
retraction to dissolve the `C` to **`True`**; fixture 2 expected partial
retraction to downgrade to **`Probable`**. But **after their retractions both
fixtures reduce to the identical survivor set — a single source asserting
`True`** (fixture 1: `{evaluator: True}` after the critic's `False` is
withdrawn; fixture 2: `{runner: True}` after the evaluator's `True` is
withdrawn). No pure function of the surviving evidence multiset — which is what
evidence-recompute-authoritative *requires* — can return `True` for one and
`Probable` for the other.

The merge-weight slice adds the **single-source corroboration cap** the design
called for (§7 "downgrades T→P", §8.2), counted over *distinct sources*: a lone
uncorroborated `True`/`False` caps at `Probable`/`Doubtful`; two or more
independent sources license the strong pole. Applied **uniformly** (the roadmap
requires one rule, not a per-fixture special case), it makes **both** fixtures
dissolve/downgrade to `Probable`. Fixture 1's headline — *the derived
contradiction dissolves* — is unchanged and remains the A/B win against the
LWW-to-`Unknown` baseline; only its residual strength moves `True → Probable`.
Fixtures + README were updated to match; the implementation is
`CognitiveLoop::project_belief` (routing through `hari_lattice::merge`), pinned
by `partial_retraction_downgrades_to_probable` and
`retraction_dissolves_derived_contradiction`.

### 8.2 Addendum (correction / relation-withdrawal slice, 2026-07-20): two more fixtures

The final belief-revision slice added the remaining two of the four contract
variants on the `ResearchEvent` boundary, each with its own fixture:

4. **`correction_replaces_claim.json`** — a source asserts `False`, then
   *corrects* itself to `True` in one atomic `correction` event. The
   correction tombstones the mislabeled original (same selector machinery as a
   selective retraction) and merges the replacement, recomputing over
   `{ix-runner: True}` → `Probable` (uniform single-source cap). The report
   carries **one** revision delta with `cause: correction` — the causal link
   between withdrawn evidence and its replacement, distinguishing a correction
   from a plain retraction. A bare `belief_update` cannot express that link.
5. **`relation_withdrawal_reverts_derived_belief.json`** — a base belief
   `Supports` a derived belief through a declared relation; withdrawing the
   relation reverts the derived belief on the next propagation. This realizes
   §4.1's RelationWithdrawal: the edge is **tombstoned in the `BeliefNetwork`,
   not deleted** (`is_relation_withdrawn` still reports it), propagation skips
   it, and derivation-only propositions reset to their `Unknown` base and
   re-derive over the reduced edge set — the same evidence-recompute doctrine
   as observation retraction, now on relations. The A/B baseline (retract the
   derived proposition instead) resets only that node and leaves the inducing
   edge live, so the belief re-derives right back; withdrawal makes the revert
   *stick*.

Implementation: `hari_lattice::BeliefNetwork::withdraw_relation` (leaf-clean —
plain `&str`/`Relation`, no `hari-core` types; propagation skips withdrawn
edges), the `Correction`/`RelationWithdrawal` arms in `process_research_event`,
and `RevisionCause::{Correction, RelationWithdrawal}`. Probed by
`crates/hari-lattice/tests/withdrawal_probe.rs` (withdraw == never-declared;
withdrawal order-independence, 1500 trials each).

## 9. What the implementation slice will need (out of scope here)

1. **Enum extension** (`hari-core::ResearchEventPayload`) — add the optional
   `retracts` field to `Retraction`; add `Correction`, `RelationWithdrawal`,
   `Supersession` variants. Update **both** `parse_trace` paths (object and
   array form) per CLAUDE.md.
2. **Tombstone in `hari-lattice::merge`** — a retraction observation kind (or
   a `tombstones: Vec<DedupKey>` on the merge input) filtered at a new step
   1.5 (after dedup, before staleness): tombstoned base observations
   contribute zero mass but are retained flagged; derivations skip retracted
   parents. Must preserve the pinned CRDT theorems (add a probe:
   *retraction commutes with merge* and *retract-then-recompute ==
   recompute-without*).
3. **Relation withdrawal in `BeliefNetwork`** — a tombstone path parallel to
   `declare_relation`; propagation skips withdrawn edges.
4. **Replay wiring** — `process_research_event` arms for the new variants;
   replace the `Retraction` `value = Unknown` mutation with the filter-and-
   recompute path; emit the revision delta into the report.
5. **Lineage additive fields** (#15) — `retracted`/`is_retracted_by`.
6. **A/B harness** — the retraction-fidelity metric (§7) and the three
   fixtures wired as regression tests, pinned against the LWW baseline.

The slice is a tracer bullet: fixture 1 (the simplest end-to-end case)
first, through every layer, before fixtures 2–3.

## 10. Honest limits

This is design + fixtures only; no code proves it yet. The tombstone/CRDT
claim in §4.1 is argued from the merge theorems but not itself probed — item
2 of §9 must add the probe before the doctrine is load-bearing. The
entrenchment half (§5.2) depends on the #14 reliability ledger, which is
currently empty (`compounding-strategy.md` §2) — conflicting-base resolution
stays unspecified here beyond "reliability is the ordering." The supersession
freeze semantics (§4.1) are the least-exercised of the four and may need a
second fixture once a real downstream-dependency case appears. Per the
ecosystem's own §4.1 verifier rule, every claim here that can become a probe
should — and until item 2 of §9 lands, this document is the hypothesis set,
not settled behavior.
