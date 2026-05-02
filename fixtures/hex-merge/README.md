# Hex-merge conformance fixtures

A shared corpus for verifying that two implementations of Demerzel's
`logic/hex-merge.md` G-Set CRDT merge produce equivalent output:

- Hari: `hari_lattice::merge::merge`
- IX: `ix_fuzzy::observations::merge`

Each fixture is a JSON file with `input` (observations + optional
staleness params) and `expected` (observation count, contradictions
list, distribution masses, escalation flag). The wire format uses
the canonical single-letter symbols `T/P/U/D/F/C` from
Demerzel's `hexavalent-state.schema.json`, matching `ix-types::Hexavalent`.

## How byte-equivalence is proven

When both `hari_lattice::merge` and `ix_fuzzy::observations::merge`
load the same fixture file and produce output matching the embedded
`expected` block, the two implementations agree on that input by
transitivity. The strongest pin is `expected.contradictions[].diagnosis_id`
— the content-derived id formula from `hex-merge.md`'s associativity
fix — which is the load-bearing surface for re-merge correctness.

## Running

Hari side (this repo):

```
cargo test -p hari-lattice --test hex_merge_conformance
```

IX side (a follow-up): a mirror of this directory at
`ix/fixtures/hex-merge/` with an analogous test in `ix-fuzzy`. Until
that lands, this corpus only proves Hari-side spec conformance, not
cross-repo equivalence. See the IX-side PRD
(`ix/governance/demerzel/docs/prd/07-hari.md`) for the graduation path.

## Adding a fixture

1. Create `NN_descriptive_name.json` with the schema below.
2. Hand-compute the expected output (or run with `--nocapture` and
   read the actual to verify by inspection, then commit).
3. The runner picks up new files automatically; no test code changes
   required.

```jsonc
{
  "name": "human_readable_name",
  "description": "what this fixture proves",
  "input": {
    "observations": [
      {
        "source": "tars",
        "diagnosis_id": "d",
        "round": 0,
        "ordinal": 0,
        "claim_key": "ix_stats::valuable",
        "variant": "T",         // T/P/U/D/F/C
        "weight": 0.9,
        "evidence": null         // optional
      }
    ],
    "current_round": null,       // u32 or null
    "staleness_k": null          // u32 or null
  },
  "expected": {
    "observations_count": 2,
    "contradictions_count": 0,
    "contradictions": [],
    "distribution": {
      "T": 0.5625, "P": 0.4375, "U": 0.0,
      "D": 0.0, "F": 0.0, "C": 0.0
    },
    "escalation_triggered": false
  }
}
```

`expected.contradictions[].diagnosis_id` is optional. When present,
the conformance test pins it exactly — that is the strongest claim
(it pins the synthesis-id formula). When absent, only `(source,
claim_key, variant, weight)` are checked.

## Current corpus

| File | Scenario |
|---|---|
| `01_agreement_no_contradiction.json` | T+P same-side agreement, no synthesis |
| `02_direct_full_contradiction.json` | T+F → C at full weight, escalation |
| `03_direct_soft_contradiction.json` | P+D → C at 0.5 multiplier, no escalation |
| `04_meta_conflict_cross_aspect.json` | Different aspects, same action → meta_conflict |
| `05_staleness_drops_old_rounds.json` | Round filter at K=5 |
| `06_dedup_collapses_duplicates.json` | Same dedup key, first-write wins |
| `07_empty_yields_uniform.json` | Empty input → uniform 1/6 fallback |
