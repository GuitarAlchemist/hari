#!/usr/bin/env python3
"""Drive an IX-style research trace through ``hari-core serve``.

Reads a ``ResearchTrace`` JSON file (object form ``{"dimension": N, "events": [...]}``
or bare array ``[event, ...]``), opens a streaming session, sends each event,
prints the recommendation actions per event, requests a mid-stream metrics
snapshot, and closes cleanly. Demonstrates every operation in the Phase 6
protocol — meant to be read end-to-end as documentation.

Usage:
    python run_session.py <trace.json> [--binary PATH]
                                       [--priority-model {Flat,RecencyDecay,Lie}]
                                       [--compare-with  {Flat,RecencyDecay,Lie}]
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

# Allow ``python clients/ix_reference/run_session.py …`` from the repo root.
sys.path.insert(0, str(Path(__file__).resolve().parent))
from hari_client import HariProtocolError, HariSession  # noqa: E402


def load_trace(path: Path) -> tuple[int, list[dict]]:
    """Accept both ResearchTrace shapes: object with ``dimension``+``events``,
    or a bare event array (dimension defaults to 4 like ``CognitiveLoop::new``).
    """
    raw = json.loads(path.read_text(encoding="utf-8"))
    if isinstance(raw, list):
        return 4, raw
    return raw.get("dimension", 4), raw["events"]


def default_binary() -> str:
    """Discover ``hari-core`` next to the repo's ``target/`` dir, falling
    back to PATH lookup. Lets the smoke test run from any cwd.
    """
    here = Path(__file__).resolve().parent
    repo_root = here.parent.parent
    suffix = ".exe" if sys.platform == "win32" else ""
    for variant in ("release", "debug"):
        candidate = repo_root / "target" / variant / f"hari-core{suffix}"
        if candidate.exists():
            return str(candidate)
    return f"hari-core{suffix}"


def action_kind(action: object) -> str:
    """Render an Action enum to a short tag for printing.

    Hari serializes ``Action`` as ``{"VariantName": {...payload}}`` for
    payload variants and ``"VariantName"`` (bare string) for unit variants.
    Both shapes appear in real recommendations.
    """
    if isinstance(action, str):
        return action
    if isinstance(action, dict) and len(action) == 1:
        return next(iter(action.keys()))
    return repr(action)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("trace", type=Path, help="Path to a ResearchTrace JSON file.")
    parser.add_argument(
        "--binary",
        default=default_binary(),
        help=f"Path to hari-core binary (default: {default_binary()!r}).",
    )
    parser.add_argument(
        "--priority-model",
        choices=["Flat", "RecencyDecay", "Lie"],
        default="Flat",
        help="Primary priority model. Default Flat (matches CognitiveLoop::new(4)).",
    )
    parser.add_argument(
        "--compare-with",
        choices=["Flat", "RecencyDecay", "Lie"],
        default=None,
        help="If set, run a shadow loop in lockstep and report divergences.",
    )
    args = parser.parse_args()

    dimension, events = load_trace(args.trace)
    config: dict = {
        "dimension": dimension,
        "priority_model": args.priority_model,
    }
    if args.compare_with:
        config["compare_with"] = args.compare_with

    print(f"Spawning {args.binary} serve ...", file=sys.stderr)
    with HariSession.spawn(args.binary) as session:
        opened = session.open_session(config)
        print(
            f"opened session={opened['session_id']} "
            f"hari={opened['hari_version']} "
            f"primary={args.priority_model}"
            + (f" shadow={args.compare_with}" if args.compare_with else "")
        )

        rejected = 0
        diverged = 0
        for i, event in enumerate(events):
            try:
                rec = session.event(event)
            except HariProtocolError as err:
                print(f"  event {i}: REJECTED {err}", file=sys.stderr)
                rejected += 1
                continue
            kinds = ",".join(action_kind(a) for a in rec["actions"]) or "<none>"
            print(f"  event {i:>2} cycle={event['cycle']:<2} actions=[{kinds}]")
            compare = rec.get("compare")
            if compare and compare.get("diverged"):
                diverged += 1
                shadow_kinds = ",".join(action_kind(a) for a in compare["shadow_actions"])
                print(f"    -> shadow ({compare['shadow_model']}) diverged: [{shadow_kinds}]")

        # Mid-stream metrics demo. Cheap; can be called as often as IX wants.
        snap = session.metrics()
        m = snap["metrics"]
        print("metrics_snapshot:")
        print(f"  false_acceptance={m['false_acceptance_count']}")
        print(f"  contradiction_recovery_cycles={m.get('contradiction_recovery_cycles')}")
        print(f"  attention_norm_max={m['attention_norm_max']:.3f}")
        print(f"  action_counts_by_kind={m['action_counts_by_kind']}")
        print(f"  beliefs={len(snap['beliefs'])} goals={len(snap['goals'])}")

        closed = session.close()
        report = closed["final_report"]
        print(
            f"closed event_count={report['event_count']} "
            f"rejected={rejected}"
            + (f" diverged={diverged}/{len(events)}" if args.compare_with else "")
        )

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
