---
description: Run one Cherny-loop iteration over Hari's belief state (ingest + replay + diff)
allowed-tools: Bash(cargo:*), Bash(./target/release/hari_harness*), Read, Bash(ls:*), Bash(git:*)
---

# /hari-harness

Run one Cherny-loop iteration over Hari's persistent belief state.

> **Install (one-time, per-clone).** `.claude/` is gitignored in this repo, so
> copy this file into your local commands directory:
>
> ```pwsh
> New-Item -ItemType Directory -Force .claude/commands | Out-Null
> Copy-Item docs/loops/hari-harness.command.md .claude/commands/hari-harness.md
> ```
>
> Then `/hari-harness` becomes available in Claude Code.

This command:
1. Ingests new events from $ARGUMENTS (notes / ix-autoresearch / raw JSONL).
2. Appends to `state/harness/events.jsonl`.
3. Replays the full log through Hari's CognitiveLoop.
4. Writes `belief-state.json`, `belief-snapshot.json`, `belief-diff.json`.
5. Exits **2** if any `Action::Escalate` fired (so `/loop` can branch).

## Default behavior

If invoked with no arguments, run a **dry inspection**: print the current `belief-snapshot.json` if it exists, otherwise explain that the harness has no state yet and show usage.

## With arguments

Pass `$ARGUMENTS` straight through to the binary. Typical forms:

- `/hari-harness --notes path/to/session-notes.txt --source claude-2026-05-12`
- `/hari-harness --autoresearch state/autoresearch/run.jsonl --target target_chatbot`
- `/hari-harness --events state/raw/events.jsonl`

Build the binary in release mode first if it's missing.

## On exit code 2

The harness fired one or more escalations. Read `state/harness/belief-diff.json` and surface the diff to the user — these are **rule candidates** for promotion to CLAUDE.md per the Cherny self-improvement loop. Do **not** auto-promote; leave the rule-writing call to the human.

## On exit code 1

Hard error. Read stderr, propose a fix, do not retry blindly.

## Implementation notes

- Pipeline: `crates/hari-extractor/src/bin/hari_harness.rs`
- Pattern doc: `docs/loops/hari-cherny-harness.md`
- The event log (`state/harness/events.jsonl`) is gitignored; the snapshot is the public artifact.
- Mercury (`--notes`) requires `INCEPTION_API_KEY` in env; the structured inputs do not.
