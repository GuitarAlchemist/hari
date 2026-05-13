# Hari as Substrate for the QA Architect Tribunal — Design Plan

**Status:** Pending — design only; no code in this plan, no cross-repo edits.
**Date:** 2026-05-13
**Owner:** spareilleux
**Reversibility:** Two-way door. Hari sits behind the tribunal as one possible aggregation engine; the existing YAML-fixture path stays operational. Flipping back is a config change.
**Revisit trigger:** When the tribunal needs to (a) track verdict evolution across review rounds, (b) preserve genuine reviewer disagreement instead of averaging it away, or (c) emit `Retraction` semantics when a reviewer revises a verdict.

## Problem statement

Memory note `project_chatbot_autonomy_gap.md` flags `qa-architect-cycle Phase 1-3 TODO`. The current tribunal lives in:

- `ga/docs/contracts/2026-05-02-qa-verdict.contract.md` — verdict shape + judge protocol
- Various ad-hoc judge prompts and YAML fixtures under `ga/state/qa/`

The tribunal collects N judge verdicts on a single proposition (a PR, a tab, a chord-rec output) and reports a consensus. Today's aggregation is **mean-or-mode over a fixed schema**:

- Three judges each return a score and a brief rationale.
- The tribunal averages the scores (or takes majority if categorical).
- Disagreement collapses into a single point estimate; nuance is lost.

This is the same failure mode Subjective Logic (already present in Hari) is designed to fix.

## Why Hari is the natural substrate

Hari's `hari_lattice` already implements:

| Tribunal need | Hari primitive |
|---|---|
| Multi-agent voting | `ResearchEventPayload::AgentVote { proposition, value, evidence }` |
| Belief aggregation across rounds | `BeliefNetwork::propagate_until_stable_with_provenance` |
| Genuine disagreement preserved | `HexValue::Contradictory` (a first-class value, not an error) |
| Reviewer changing their mind | `ResearchEventPayload::Retraction { proposition, reason }` |
| Uncertainty without point-estimate collapse | `PriorityModel::SubjectiveLogic` (Opinion fusion) |
| Cross-round time series | `ResearchTrace` JSONL replay |
| Forensic explainability | `derivations: Vec<Derivation>` carrying every belief change |

The tribunal is the canonical use case for the substrate Hari already shipped.

## Proposed architecture

```text
ga/state/qa/round-<N>/
  reviewer-A.verdict.yaml     ← human or LLM judge writes here
  reviewer-B.verdict.yaml
  reviewer-C.verdict.yaml
                   │
                   ▼
        hari-qa-adapter (new sidecar — hari-side)
                   │ — converts each verdict YAML to ResearchEvent::AgentVote
                   ▼
        ResearchTrace { dimension: 4, events: [...] }
                   │
                   ▼
        hari-core::CognitiveLoop::process_research_trace
                   │ ← PriorityModel: SubjectiveLogic (the Hari setting)
                   ▼
        ResearchReplayReport
        ├── final_beliefs: { "PR-186-is-mergeable": Contradictory, ... }
        ├── derivations:   forensic trail of every belief update
        └── metrics:       contradiction_recovery_cycles, consensus_stability
                   │
                   ▼
        Tribunal verdict (existing surface — UI/CLI/api consumers unchanged)
```

The sidecar is **strictly hari-side**. ga emits its existing per-reviewer YAMLs; Hari reads them and aggregates. The existing ga aggregation path stays in place as a fallback.

## API surface (sketch — for a future `hari-qa-adapter` crate)

```rust
/// Convert one tribunal round of per-reviewer verdicts into a ResearchTrace
/// the CognitiveLoop can replay.
pub fn round_to_trace(
    round_id: &str,
    proposition: &str,
    verdicts: &[ReviewerVerdict],
) -> ResearchTrace;

/// Reviewer verdict shape — mirrors ga's qa-verdict.contract.md v1, plus a
/// HexValue mapping for genuine disagreement.
pub struct ReviewerVerdict {
    pub reviewer_id: String,
    pub value: HexValue,             // Doubtful / Probable / etc.
    pub confidence: Option<f64>,     // optional Beta/SL weight
    pub rationale: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Aggregate verdict — what the tribunal returns to its caller.
pub struct TribunalReport {
    pub round_id: String,
    pub proposition: String,
    pub consensus: HexValue,         // Contradictory when reviewers disagree
    pub agreement_strength: f64,     // from PriorityModel::SubjectiveLogic
    pub dissenting_reviewers: Vec<String>,
    pub provenance: Vec<hari_lattice::Derivation>,
}
```

The `HexValue::Contradictory` consensus is the killer feature — today's tribunal can't distinguish "all three reviewers are uncertain" from "two reviewers say accept, one says reject"; Hari does, by construction.

## Cross-repo contract

Per CLAUDE.md collaboration discipline, the tribunal lives at a cross-repo boundary:

- **ga owns** the verdict YAML schema (in `ga/docs/contracts/2026-05-02-qa-verdict.contract.md`).
- **hari owns** the aggregation semantics (this doc + the future `hari-qa-adapter`).
- **Demerzel owns** the tribunal lifecycle policy (per Galactic Protocol — when to convene, which propositions, which reviewers).

The handoff is **YAML files at known paths**, identical to the existing fixture pattern. Hari stays one read away from ga's state; ga stays unaware of Hari's existence (it can fall back to its in-process aggregation if `hari-qa-adapter` isn't deployed).

## Implementation milestones

### Milestone 1 — Read-only sidecar (~½ day)

1. New crate `crates/hari-qa-adapter` in this workspace.
2. YAML reader for the `qa-verdict.contract.md` v1 schema (use `serde_yaml`).
3. `round_to_trace` function as sketched above.
4. Unit tests against fixture rounds with all three patterns:
   - Unanimous Probable → `consensus: Probable`
   - 2 Probable + 1 Doubtful → `consensus: Contradictory` (the key test)
   - One reviewer issues a Retraction in round 2 → `consensus` updates accordingly
5. CLI binary `hari-qa-aggregate` that takes a `--round-dir <path>` and prints the `TribunalReport` as JSON.

No ga changes at all. ga continues emitting its YAMLs; an out-of-band script (or a Demerzel governance hook) invokes `hari-qa-aggregate` and consumes the JSON.

### Milestone 2 — Time-series across rounds (~½ day)

Once round-N aggregation works, accumulate rounds 1..N into a single `ResearchTrace`. Hari's `BeliefNetwork` natively handles this — `final_beliefs` reflects the latest stable value across the whole history.

### Milestone 3 — Demerzel hook (cross-repo coordination required)

A Demerzel governance policy that automatically convenes the tribunal at named gates (PR merge, qa-architect-cycle Phase transition). This is the riskier bit because it touches Demerzel's policy schema. Defer until M1+M2 are battle-tested.

## What this is NOT

- **Not a replacement** for the existing tribunal. Hari is one aggregation engine alongside; the ga-side averaging path continues to work.
- **Not a live service**. The first milestone is offline batch — read YAMLs, emit JSON, done. Live streaming via `hari-core::session` (Phase 6) is a separate enhancement.
- **Not LLM-in-the-loop**. The judges may be LLMs (their job, separate concern); the aggregation is pure math over Subjective Logic / 6-valued lattice. `hari-extractor` is irrelevant here — verdicts come in as YAML, not free text.

## Cross-repo coordination

- **ga (PR #209 in flight):** Cherny loops doc work is conceptually adjacent (cross-repo coordination patterns). Wait for #209 to land before drafting any ga-side doc; this hari-side plan stands on its own.
- **ix:** No direct dependency. ix-autoresearch may eventually consume tribunal verdicts to decide whether to retry a failed experiment; that's a separate integration.
- **Demerzel:** Mention this plan in the next `governance/demerzel/` constitutional update so the tribunal contract acknowledges Hari as one of the valid aggregation engines.

## Acceptance criteria for Milestone 1

- [ ] `cargo build -p hari-qa-adapter` clean.
- [ ] Three fixture-driven unit tests pass: unanimous, contradictory, retraction-in-round-2.
- [ ] `hari-qa-aggregate --round-dir <fixture>` emits a `TribunalReport` JSON that distinguishes the three patterns by `consensus` field.
- [ ] No changes to ga or Demerzel repos.

## Acceptance criteria for "Hari is now THE aggregation engine"

- [ ] Demerzel policy explicitly names Hari as the canonical Subjective Logic aggregator for QA tribunal verdicts.
- [ ] A live PR review round (e.g. one of today's PRs #207/#208) routes through `hari-qa-aggregate` and produces a verdict matching what manual aggregation would have produced.
- [ ] The forensic `derivations` trail is preserved in `ga/state/qa/round-<N>/hari-report.json` for post-hoc audit.

## One-way door log

- **None at the M1 boundary.** Sidecar is purely additive.
- **Milestone 3 introduces a soft one-way door:** Demerzel policy referencing Hari makes Hari a governance-contract dependency. Document the rollback procedure (revert the policy, re-enable in-process aggregation) when M3 lands.
