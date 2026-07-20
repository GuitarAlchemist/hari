#!/usr/bin/env python3
"""Project Demerzel's hand-maintained belief files into a hari ResearchTrace.

Reads ``state/beliefs/*.belief.json`` from a Demerzel checkout and emits a
replayable trace (object form: ``{dimension, events}``) on stdout, so that
hari's substrate — not a hand edit — computes what the evidence actually
supports:

    python scripts/demerzel/beliefs_to_trace.py ../Demerzel > fixtures/demerzel/beliefs.json
    cargo run --release -p hari-core -- replay fixtures/demerzel/beliefs.json

v0 mapping (deliberately crude, documented in
docs/research/2026-07-20-demerzel-belief-replay.md — the same crudeness
critique the SL reward mapping received applies here):

  - each ``evidence.supporting`` item    -> belief_update, value True if
    reliability >= 0.9 else Probable
  - each ``evidence.contradicting`` item -> belief_update, value False if
    reliability >= 0.9 else Doubtful
  - events are ordered by evidence timestamp (temporal order is REAL here,
    which is exactly when hari's pairwise combine_evidence is the right
    operator); cycle numbers follow that order
  - proposition ids are stable slugs derived from the belief filename
    (``demerzel/<stem>``), per the ecosystem's slugs-not-internal-ids rule
  - beliefs with no evidence at all are SKIPPED and listed on stderr —
    no evidence, no events, no substrate opinion; their hand-maintained
    ``truth_value`` has nothing to be checked against

The original file's truth_value/confidence ride along in each event's
evidence map (``demerzel_truth_value`` / ``demerzel_confidence``) purely as
audit-trail context; hari does not read them.
"""

import json
import sys
from pathlib import Path


def slug(stem: str) -> str:
    # Drop a leading YYYY-MM-DD- date prefix if present; the date is
    # evidence metadata, not identity.
    parts = stem.split("-")
    if len(parts) > 3 and parts[0].isdigit() and parts[1].isdigit() and parts[2].isdigit():
        stem = "-".join(parts[3:])
    return f"demerzel/{stem}"


def value_for(side: str, reliability: float) -> str:
    if side == "supporting":
        return "True" if reliability >= 0.9 else "Probable"
    return "False" if reliability >= 0.9 else "Doubtful"


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    beliefs_dir = Path(sys.argv[1]) / "state" / "beliefs"
    if not beliefs_dir.is_dir():
        print(f"not a Demerzel checkout: {beliefs_dir} missing", file=sys.stderr)
        return 2

    staged = []  # (timestamp, source, payload)
    skipped = []
    for f in sorted(beliefs_dir.glob("*.belief.json")):
        d = json.loads(f.read_text(encoding="utf-8"))
        prop = slug(f.name.removesuffix(".belief.json"))
        ev = d.get("evidence", {}) or {}
        items = [("supporting", e) for e in ev.get("supporting", []) or []] + [
            ("contradicting", e) for e in ev.get("contradicting", []) or []
        ]
        if not items:
            skipped.append((prop, d.get("truth_value"), d.get("confidence")))
            continue
        for side, e in items:
            rel = float(e.get("reliability", 0.5))
            staged.append(
                (
                    e.get("timestamp") or d.get("last_updated") or "",
                    e.get("source", "demerzel-unknown"),
                    {
                        "type": "belief_update",
                        "proposition": prop,
                        "value": value_for(side, rel),
                        "evidence": {
                            "belief_file": f.name,
                            "side": side,
                            "claim": e.get("claim", ""),
                            "reliability": rel,
                            "timestamp": e.get("timestamp"),
                            "demerzel_truth_value": d.get("truth_value"),
                            "demerzel_confidence": d.get("confidence"),
                        },
                    },
                )
            )

    staged.sort(key=lambda t: (t[0], t[2]["proposition"], t[2]["evidence"]["side"]))
    events = [
        {"cycle": i + 1, "source": src, "payload": payload}
        for i, (_, src, payload) in enumerate(staged)
    ]

    json.dump({"dimension": 4, "events": events}, sys.stdout, indent=2, ensure_ascii=False)
    sys.stdout.write("\n")

    for prop, tv, conf in skipped:
        print(f"skipped (no evidence): {prop} [hand-maintained {tv}@{conf}]", file=sys.stderr)
    print(f"{len(events)} events from {beliefs_dir}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
