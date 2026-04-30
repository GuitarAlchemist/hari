#!/usr/bin/env bash
# Mechanical exit criterion for Phase 5 (Cognition Integration).
#
# Returns 0 only when the implementation is genuinely complete. Any non-zero
# exit code with a stderr message indicates which gate failed — the factory
# loop should read that message and iterate.
#
# Prereqs: cargo, jq. (jq is the only non-default dep; install via your
# package manager.)
#
# Usage:
#   scripts/check-phase5-done.sh
#
# Exit codes:
#   0  PASS — Phase 5 done
#   10 cargo test --all failed
#   11 fixtures/ix/cognition_divergence.json missing
#   12 cargo run -- replay --compare did not execute (likely flag missing)
#   13 action_divergence empty or absent (Lie path is a no-op)
#   14 attention_norm_max >= 10.0 in at least one path (numerical instability)
#   20 jq not installed

set -euo pipefail

cd "$(dirname "$0")/.."

command -v jq >/dev/null 2>&1 || { echo "FAIL: jq not installed (required for JSON parsing)" >&2; exit 20; }

echo "[1/4] cargo test --all" >&2
cargo test --all --quiet >/dev/null 2>&1 || { echo "FAIL: cargo test --all" >&2; exit 10; }

echo "[2/4] fixture present" >&2
[ -f fixtures/ix/cognition_divergence.json ] \
    || { echo "FAIL: fixtures/ix/cognition_divergence.json missing" >&2; exit 11; }

echo "[3/4] cargo run -- replay --compare" >&2
out=$(cargo run --release -p hari-core --quiet -- replay --compare fixtures/ix/cognition_divergence.json 2>/dev/null) \
    || { echo "FAIL: replay --compare did not run (flag may not exist yet)" >&2; exit 12; }

echo "[4/4] divergence and boundedness gates" >&2
echo "$out" | jq -e '.comparison.action_divergence | length >= 1' >/dev/null \
    || { echo "FAIL: comparison.action_divergence is empty or missing — Lie path produced no observable difference" >&2; exit 13; }

echo "$out" | jq -e '.comparison.experimental.attention_norm_max < 10 and .comparison.baseline.attention_norm_max < 10' >/dev/null \
    || { echo "FAIL: attention_norm_max >= 10.0 — numerical instability or unbounded growth" >&2; exit 14; }

echo "PASS: Phase 5 exit criterion satisfied"
