# Research-Question PRD: SAE-Contradicts Alignment

**Version:** 0.1 (draft) | **Date:** 2026-05-02 | **Status:** Proposed, not started

---

## Question

Do Hari's `Contradicts` relations between propositions correspond to (anti-)alignment in the latent feature space of an SAE-decomposed LLM?

## Hypothesis

- **H1 (alignment):** For propositions A and B that Hari has marked `Contradicts`, the SAE feature activations of A and B share little — measurable as low cosine similarity, low Jaccard over top-k active features, or detectable anti-correlation along specific feature directions.
- **H0 (null):** SAE feature activations are independent of Hari's declared logical relations. Contradicts pairs and Independent pairs have indistinguishable similarity distributions.

## Why this question

- **Connects three substrates** that were built independently this session: Hari's belief layer (declared structure), SAE features (learned structure), and the hex-merge conformance corpus pattern (cross-implementation byte-equality).
- **Falsifiable in either direction.** If H1 holds, SAE features become a cheap source of `Contradicts` evidence — LLM-aided contradiction detection without using the LLM as judge. If H0 holds, that's still useful: it establishes a calibration baseline showing that `Contradicts` carries information *not* captured by surface-level activation overlap, which sharpens the meaning of the relation.
- **Honors Hari's stated philosophy**: "Algebraic structure is treated as a hypothesis to test." Phase 5 tested Lie's negative result; this would test whether the hexavalent relation layer aligns with learned representations.

## Methodology

### Inputs

- A labeled-pair corpus: `(A, B, relation)` triples where `relation ∈ {Implies, Supports, Contradicts, Independent}`. Target: 50+ pairs per relation for statistical power. Source: hand-crafted from existing `fixtures/ix/*.json` traces plus author-curated additions.
- An SAE-decomposable LLM. Qwen 2-0.5B with Qwen-Scope SAEs is the obvious starting point — small, public, tractable.
- The SAE itself: loaded Qwen-Scope weights at the published layer.

### Procedure

1. **Forward-pass each proposition** through the LLM, extract residual-stream activations at the SAE's trained layer.
2. **SAE-decode** to a sparse feature vector φ(p) ∈ ℝᵈ (typically d = 16k-65k features, mostly zero).
3. **For each pair (A, B), compute three similarity metrics:**
   - `cos(φ(A), φ(B))` — high for similar concepts.
   - `cos(φ(A), -φ(B))` — high if features are mirror images, i.e. anti-aligned.
   - Jaccard over top-k active features (k = 32 default).
4. **Bin pairs by relation, compare distributions.** Mann-Whitney U for `Contradicts` vs `Independent`, two-tailed for the symmetric metrics.

### Success criteria

- **Strong support for H1:** `Contradicts` pairs have significantly lower mean similarity than `Independent` baselines (one-sided Mann-Whitney p < 0.05, effect size Cliff's δ > 0.33).
- **Weak support:** directional trend without significance — flag for more data.
- **Refutation:** distributions overlap entirely, or `Contradicts` similarity ≥ `Supports`. Report honestly per Hari's "negative results survive" philosophy. Phase 5 demoted Lie based on data; this would do the same.

### What ships regardless of outcome

- **Primitive:** `ix-sae` crate — forward-pass + decode + top-k feature surfacing for any aligned SAE. Useful beyond this experiment.
- **Fixtures:** `Demerzel/fixtures/sae-relations/*.json` with labeled proposition pairs. Reusable for any future SAE-on-Hari-relations work.
- **Reproducibility:** every result re-runnable from fixtures + `cargo run` alone. No notebook-dependent analysis.

## Architecture

```
ix-sae (NEW primitive crate)
    ├── SparseAutoEncoder { encoder, decoder, threshold }
    ├── encode(activations: &Array1<f32>) -> SparseFeatures
    ├── load_qwen_scope(path: &Path) -> Result<SparseAutoEncoder>
    └── top_k_features(features: &SparseFeatures, k: usize) -> Vec<(usize, f32)>

ix-nn (EXISTING — Qwen forward pass)
    └── activate_at_layer(model, input, layer) -> Array2<f32>

hari (EXISTING — relation declarations)
    └── BeliefNetwork::declare_relation(A, B, Contradicts)

ix-experiments/sae-contradicts (NEW binary crate)
    ├── Load Demerzel/fixtures/sae-relations/*.json
    ├── For each pair: forward-pass through Qwen → SAE → metrics
    ├── Emit per-pair scores as Hari ResearchEvents
    └── Run a Hari session over the events for relation-aware analysis
```

## Phases

### P0 — `ix-sae` math primitive (small)

- `ix-sae::SparseAutoEncoder` with `encode(activations) -> sparse_features` and a basic `decode(sparse) -> reconstructed`.
- Synthetic-activation fixtures + expected-sparse-code fixtures.
- No model integration yet. Tests prove the math is right against published Qwen-Scope reference values.
- Ships independent of the rest.

### P1 — Qwen-Scope loader (small)

- Parse Qwen-Scope's published weight format (likely SafeTensors).
- Round-trip test: `encode → decode` reconstruction MSE within published threshold.
- Still no inference; just weight-loading parity.

### P2 — model integration (significant, possibly load-bearing)

- Wire `ix-nn`'s Qwen forward pass to extract residual stream at the SAE's trained layer.
- **This is the engineering risk.** Getting Qwen actually running through `ix-nn` (currently focused on smaller transformers) may require non-trivial work. Worth scoping before committing.
- Alternative escape hatch: shell out to `llama.cpp` or candle for the forward pass and only use `ix-sae` for the SAE step. Less elegant but unblocks the research.

### P3 — the actual experiment (medium)

- Build the labeled-pair fixture set in `Demerzel/fixtures/sae-relations/`.
- Run pipeline end-to-end.
- Compute statistics.
- Report results in `hari/docs/research/2026-MM-DD-sae-contradicts-results.md`, mirroring `phase5-results.md` format.

## Out of scope (deliberate)

- **Training SAEs from scratch.** Use Qwen-Scope's published weights.
- **Multi-layer feature analysis.** Start with the one published layer, only.
- **Causal interventions / feature steering.** Purely observational analysis first.
- **Generalizing to non-Qwen models.** Lock in the experiment first; generalization is a follow-up.
- **Live IX integration.** This is research code; replayable fixtures are sufficient.

## Cross-repo dependencies

- **IX:** `ix-sae` new crate; `ix-nn` Qwen forward pass (existing or new); `ix-experiments/sae-contradicts` binary.
- **Hari:** relation declarations as the ground truth for labels. Existing `BeliefNetwork::declare_relation` API, no new code needed.
- **Demerzel:** canonical fixtures at `Demerzel/fixtures/sae-relations/` following the same pattern as `fixtures/hex-merge/` (per the 2026-05-02 compounding-cycle Proposal A).

## Open questions before starting

1. **Is `ix-nn` Qwen-capable today?** If not, P2 may dwarf the rest of the work. Worth a small spike to confirm before committing to the full plan.
2. **Where does the labeled-pair corpus come from?** 50+ pairs per relation is a real curation effort. Could bootstrap from existing IX traces but most pairs would need hand-checking.
3. **Statistical power.** 50 pairs may be too few for the effect sizes we care about. Worth an a-priori power calculation.
4. **Does "Independent" mean anything coherent in Hari?** Currently Hari has Implies/Supports/Contradicts but no explicit "Independent" — would need to be either an absence-of-relation marker or a new relation type.

## Decision needed before starting

Whether to commit to this question vs. one of the alternatives I sketched in conversation:

- **Q1 (this PRD):** Do `Contradicts` relations align with anti-correlated SAE features?
- **Q2:** Can feature entropy predict hallucination in `ix-autoresearch`?
- **Q3:** Do contradictory inputs activate disjoint feature sets, independent of Hari?

Q1 is the most ecosystem-coherent (uses today's hex-merge work, Hari's relation API, IX's primitive-collection pattern). Q2 is the most directly useful but has the hardest ground-truth labeling problem. Q3 is the simplest but doesn't exercise Hari at all.

**Recommended decision:** Q1 if the goal is to make Hari + IX + interpretability into one coherent research program. Q3 if the goal is fastest-to-result on a self-contained question.
