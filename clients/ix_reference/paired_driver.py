#!/usr/bin/env python3
"""Paired-task driver — #35 §9 item 4 / user story 12.

Runs the same traces Hari-on and Hari-off and produces the corpus roll-up in
**one command**, over the replay boundary that already exists.

    python clients/ix_reference/paired_driver.py --out target/ix-driver

What "Hari-on and Hari-off" means here
--------------------------------------

The comparison is a *counterfactual shadow replay* (issue: "never two live
runs, because nondeterminism destroys the pairing"). `hari-core replay --paired
--compare3 --bootstrap` already replays one recorded trace under four arms in a
single pass: `ix_unassisted` is Hari-off, `recency_decay` / `lie` /
`subjective_logic` are Hari-on. So the driver does not re-run anything twice —
it records the traces, stamps them, and hands the corpus to that boundary.

Why the driver generates rather than replays a live IX session
--------------------------------------------------------------

Pre-registration §9.4: the bootstrap needs a *population*, and hand-authored
traces are not draws from one. §9.4's 2026-07-30 correction states the
alternative plainly: *"A pre-registered generative distribution over
decision-relevant parameters, mechanically sampled, would give the bootstrap a
population in exactly the same conditional sense this section grants the IX
driver."* What shrinks is the disclosable surface — from "which 54 decisions" to
"which distribution" — and the distribution is `SPEC` below, in one place, in
the open.

**This does not ratify the distribution.** §9.4 records that choosing the task
distribution is the one remaining pre-registration decision and an *owner call*.
The driver makes a §8 verdict mechanically reachable; whether `SPEC` is §2's
task distribution is not the driver's call and is not claimed here.

Determinism
-----------

Same `--seed` produces byte-identical fixtures. The generator is a vendored
SplitMix64 rather than `random`, for the reason `paired_eval.rs` vendors the
same algorithm: a published corpus must not move when an implementation
underneath it changes. `--check-determinism` re-derives the corpus twice and
compares digests.

Provenance
----------

Every generated fixture carries a `provenance` block naming the driver, the
spec, the seed, and an FNV-1a-64 digest of the trace. `hari-core` recomputes
that digest and refuses a fixture whose trace was edited after generation, so
`--corpus recorded` is a fact about the corpus rather than an assertion on the
command line. This is tamper-evidence, not tamper-proofing (see
`FixtureProvenance` in `paired_eval.rs`).
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

# Allow `python clients/ix_reference/paired_driver.py …` from the repo root.
sys.path.insert(0, str(Path(__file__).resolve().parent))
from run_session import default_binary  # noqa: E402

DRIVER = "ix_reference/paired_driver"

# ---------------------------------------------------------------------------
# The declared generative distribution (§9.4: the disclosable unit)
# ---------------------------------------------------------------------------

SPEC = {
    "id": "flaky-vs-real/v1",
    # §2's task: a micro-benchmark run N times with injected perturbations.
    # Decision-relevant parameters — the ones any arm can actually read — are
    # the asserted HexValue and the number of corroborating assertions. Cosmetic
    # payload fields are drawn too, and §9.3.2 already established that no arm
    # reads them; they are recorded so the trace is legible, not to do work.
    "runs": [8, 16, 24, 32],
    "corroborations": [1, 2, 3],
    "asserted": ["Probable", "True", "Doubtful"],
    # Whether the injected perturbation was a real regression. Ground truth by
    # construction — but only for each pair's *claim outcome*, which feeds §5.3's
    # secondary counts. The act/abstain labels are structural (§9.3's G2 ground)
    # and do not read it, so this parameter does not move the primary paired
    # metric. See `generate_trace` and pre-registration §9.9.
    "real_regression_rate_in_16ths": 7,
    "themes": ["accuracy", "cache", "cost", "latency", "memory", "throughput"],
    "pairs_per_trace": 3,
}


class SplitMix64:
    """The generator `paired_eval::SplitMix64` uses, in Python.

    Vendored for the same reason: the corpus a published interval was computed
    over must be re-derivable from `(spec, seed)` alone, and `random`'s output
    is a property of the interpreter rather than of this file.
    """

    MASK = (1 << 64) - 1

    def __init__(self, seed: int) -> None:
        self.state = seed & self.MASK

    def next_u64(self) -> int:
        self.state = (self.state + 0x9E3779B97F4A7C15) & self.MASK
        z = self.state
        z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & self.MASK
        z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & self.MASK
        return z ^ (z >> 31)

    def index(self, n: int) -> int:
        return self.next_u64() % n

    def choice(self, options: list):
        return options[self.index(len(options))]


def fnv1a64(data: bytes) -> str:
    """`paired_eval::trace_digest`, in Python. Must agree byte for byte."""
    h = 0xCBF29CE484222325
    for b in data:
        h ^= b
        h = (h * 0x100000001B3) & ((1 << 64) - 1)
    return f"{h:016x}"


def canonical_trace_json(trace: dict) -> bytes:
    """Serialize a trace exactly as `serde_json::to_vec` would.

    Rust emits struct fields in declaration order and `BTreeMap` keys in sorted
    order, with no whitespace. Building the dicts in declaration order and
    sorting evidence keys at construction reproduces that, so the digest this
    computes is the digest `hari-core` recomputes.
    """
    return json.dumps(trace, separators=(",", ":"), ensure_ascii=False).encode("utf-8")


# ---------------------------------------------------------------------------
# Generation
# ---------------------------------------------------------------------------


def belief_update(cycle: int, source: str, proposition: str, value: str, evidence: dict) -> dict:
    return {
        "cycle": cycle,
        "source": source,
        "payload": {
            "type": "belief_update",
            "proposition": proposition,
            "value": value,
            "evidence": {k: evidence[k] for k in sorted(evidence)},
        },
    }


def generate_trace(theme: str, rng: SplitMix64) -> tuple[dict, list, list]:
    """One trace plus its ground truth, drawn from ``SPEC``.

    Each pair is a should-act / should-abstain couple. The halves are separated
    structurally, not by the injected trigger. The act half asserts a
    corroborated regression; the abstain half carries no proposition at all
    (`goal_update`) — §9.3's G2 "commitability" ground, the shape every arm
    treats as a non-decision. It draws no `real` and no corroboration, so
    ``SPEC["real_regression_rate_in_16ths"]`` reaches only the claim outcomes
    feeding §5.3's secondary counts and never the primary paired metric:
    regenerating this corpus at 0 or 16 sixteenths leaves clause 1, the cheap
    comparison and the verdict identical.

    That is a construct-validity limit on this corpus, not a power limit, and
    adding traces does not repair it — see pre-registration §9.9 and report §6
    limit 7. Making the abstain half a should-abstain *claim* (an uncorroborated
    ``belief_update`` whose perturbation was drawn as variance) is the repair,
    and is an owner scoping call left to a separate future slice.
    """
    events: list[dict] = []
    labels: list[dict] = []
    claims: list[dict] = []

    for p in range(SPEC["pairs_per_trace"]):
        runs = rng.choice(SPEC["runs"])
        corroborations = rng.choice(SPEC["corroborations"])
        asserted = rng.choice(SPEC["asserted"])
        real = rng.index(16) < SPEC["real_regression_rate_in_16ths"]
        proposition = f"{theme}-regression-{p:02d}"

        # --- should-act half: the injected regression, asserted and corroborated
        act_index = len(events)
        for c in range(corroborations):
            events.append(
                belief_update(
                    len(events) + 1,
                    f"ix-runner-{c}",
                    proposition,
                    asserted,
                    {
                        "runs": runs,
                        "injected": "regression" if real else "variance",
                        "p99_ms": 100 + rng.index(400),
                    },
                )
            )

        # --- should-abstain half: same benchmark, no claim to commit to
        abstain_index = len(events)
        events.append(
            {
                "cycle": len(events) + 1,
                "source": "ix-runner-0",
                "payload": {
                    "type": "goal_update",
                    "key": f"{theme}-characterise-{p:02d}",
                    "description": f"characterise {theme} run {p}",
                    # Binary fractions only. The digest is computed over the
                    # JSON text, and Rust and Python do not always agree on the
                    # shortest round-tripping decimal for an arbitrary f64 —
                    # `0.5 + 42/100` printed as `0.92` on one side and
                    # `0.9199999999999999` on the other, which made a correctly
                    # generated fixture fail its own digest check. Halves of a
                    # power of two are exact, so both sides print them alike.
                    "priority": 0.5 + rng.index(32) / 64.0,
                    "status": asserted,
                },
            }
        )

        labels.append({"event_index": act_index, "pair": proposition, "expect": "act"})
        labels.append({"event_index": abstain_index, "pair": proposition, "expect": "abstain"})
        claims.append(
            {"proposition": proposition, "outcome": "stood" if real else "withdrawn"}
        )

    return {"dimension": 4, "events": events}, labels, claims


def generate_corpus(seed: int, themes: list[str]) -> dict[str, dict]:
    """`{stem: fixture}` for the whole corpus, deterministic in ``seed``."""
    out: dict[str, dict] = {}
    for i, theme in enumerate(themes):
        # One stream per theme, seeded from the corpus seed, so adding a theme
        # does not perturb the traces already drawn.
        rng = SplitMix64(seed + i * 0x9E3779B9)
        trace, labels, claims = generate_trace(theme, rng)
        fixture = {
            "trace": trace,
            "labels": labels,
            "claims": claims,
            "provenance": {
                "driver": DRIVER,
                "spec": SPEC["id"],
                "seed": seed + i * 0x9E3779B9,
                "trace_digest": fnv1a64(canonical_trace_json(trace)),
            },
        }
        out[f"{theme}-task"] = fixture
    return out


def write_corpus(corpus: dict[str, dict], out_dir: Path) -> list[Path]:
    out_dir.mkdir(parents=True, exist_ok=True)
    paths = []
    for stem, fixture in corpus.items():
        path = out_dir / f"{stem}.json"
        path.write_text(json.dumps(fixture, indent=2) + "\n", encoding="utf-8")
        paths.append(path)
    return paths


# ---------------------------------------------------------------------------
# The one command
# ---------------------------------------------------------------------------


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--seed", type=int, default=20260728, help="Corpus seed.")
    parser.add_argument(
        "--out", type=Path, default=Path("target/ix-driver"), help="Corpus output directory."
    )
    parser.add_argument("--binary", default=None, help="Path to hari-core.")
    parser.add_argument(
        "--corpus",
        choices=["authored", "recorded"],
        default=None,
        help="Assert the corpus provenance. Omit to let hari-core read it off the fixtures.",
    )
    parser.add_argument(
        "--check-determinism",
        action="store_true",
        help="Re-derive the corpus and compare digests; emit nothing else.",
    )
    parser.add_argument(
        "--generate-only", action="store_true", help="Write the corpus, do not replay it."
    )
    args = parser.parse_args()

    if args.check_determinism:
        a = generate_corpus(args.seed, SPEC["themes"])
        b = generate_corpus(args.seed, SPEC["themes"])
        da = {k: v["provenance"]["trace_digest"] for k, v in a.items()}
        db = {k: v["provenance"]["trace_digest"] for k, v in b.items()}
        if da != db:
            print(f"NOT DETERMINISTIC: {da} != {db}", file=sys.stderr)
            return 1
        for stem, digest in da.items():
            print(f"{stem} {digest}")
        print(f"deterministic: {len(da)} traces at seed {args.seed}")
        return 0

    corpus = generate_corpus(args.seed, SPEC["themes"])
    paths = write_corpus(corpus, args.out)
    print(f"recorded {len(paths)} traces into {args.out} at seed {args.seed}", file=sys.stderr)
    if args.generate_only:
        return 0

    binary = args.binary or default_binary()
    cmd = [binary, "replay", "--paired", "--compare3", "--bootstrap"]
    if args.corpus:
        cmd += ["--corpus", args.corpus]
    cmd += [str(p) for p in paths]
    print(f"$ {' '.join(cmd)}", file=sys.stderr)
    completed = subprocess.run(cmd, capture_output=True, text=True)
    sys.stderr.write(completed.stderr)
    if completed.returncode != 0:
        return completed.returncode

    sys.stdout.write(completed.stdout)
    report = json.loads(completed.stdout)
    kk = report["kill_keep"]
    print(
        f"\n#35 §8: verdict={kk['verdict']} clause1={kk['clause1']} "
        f"clause2={kk['clause2']} provenance={kk['provenance']}",
        file=sys.stderr,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
