---
module: Cross-Repo Git Workflow
date: 2026-07-19
problem_type: workflow_issue
component: development_workflow
symptoms:
  - "git commit reports 'no changes added to commit' immediately after a successful git add"
  - "a staged 79-line change vanished with no error output and success exit codes throughout"
  - "HEAD moved to a commit authored by another session between the add and the commit"
root_cause: missing_workflow_step
resolution_type: workflow_improvement
severity: critical
tags: [git, concurrent-sessions, multi-agent, atomic-commit, head-guard, windows]
---

# Staged commit silently lost to a concurrent Claude Code session

## Problem

With ~4 Claude Code sessions running simultaneously across the sibling
repos (`hari`, `ix`, `ga`, `Demerzel`), a staged change in
`ga/Scripts/run-prompt-corpus.ps1` (79 additive lines — the per-prompt
verdict ledger) was silently discarded: another session's commit
(`d94f6d4e`, touching the *same file*) landed between this session's
`git add` and `git commit`.

## Symptoms (exact)

```text
On branch feat/chatbot-llm-judge-invariants
Your branch is ahead of 'origin/main' by 1 commit.
Changes not staged for commit: ...
no changes added to commit (use "git add" and/or "git commit -a")
```

- Every command exited 0. Nothing errored.
- `git log --all --grep='per-prompt verdict ledger'` → no commit anywhere.
- `grep -c 'promptRoster' Scripts/run-prompt-corpus.ps1` → 0: the edit
  was gone from the working tree, not just uncommitted.

## Investigation

1. First hypothesis: the commit failed loudly somewhere — no; exit codes
   were clean, and the pre-commit hook output looked normal.
2. `git log -1` showed an unfamiliar commit message → HEAD had advanced
   to another session's commit on the same branch, same file.
3. Verified nothing of ours survived (`git log --grep`, content grep) and
   nothing of theirs was damaged. The concurrent commit had reset the
   working tree state out from under the staged-but-uncommitted change.

## Root cause

The gap between `git add` and `git commit`, issued as **separate shell
invocations**, is a race window. In a multi-session environment another
writer can commit/reset the tree inside that window; git then reports
"no changes added" — a *success-shaped failure*. The exit code cannot be
trusted as evidence the work landed.

## Solution

Guarded atomic commit, adopted for every commit in shared repos:

```bash
PIN=$(git rev-parse HEAD)
# ... make edits ...
NOW=$(git rev-parse HEAD)
[ "$PIN" = "$NOW" ] || { echo "ABORT: HEAD moved"; exit 1; }
git add <files> && git commit -m "..."       # ONE invocation, no gap
git log --grep '<distinctive phrase>'         # verify it LANDED
```

On the retry, the collision turned into a benefit: the other session's
commit (`d94f6d4e`) had *improved* the degraded-environment logic the
lost change depended on, so re-reading before re-applying produced a
strictly better version than blind re-application would have.

## Prevention

- **Stage and commit in one chained invocation** — never two calls.
- **Pin HEAD before editing; re-check at commit time; abort if moved.**
- **Verify by `git log --grep`, never by exit code.**
- **Treat uncommitted modifications in your target area as another
  session's live edit: stop and report, don't touch.**
- **If a change disappears: surface it and re-read before re-applying** —
  the newer work may have obsoleted yours.

The full protocol is repo-persisted at `docs/agents/concurrent-sessions.md`
(hari) so subagents inherit it; any subagent prompt that authorizes
commits in a shared repo must point at it.

## Related

- `hari:docs/agents/concurrent-sessions.md` — the canonical protocol
  (this doc is the incident record; that doc is the rule).
- `ga:docs/solutions/runtime-errors/2026-03-10-windows-git-packed-refs-branch-corruption-workaround.md`
  — different root cause (packed-refs corruption vs. concurrency race),
  same theme: git-on-Windows failure modes that report success.
- hari `.claude/agents/formal-auditor.md` step 8 — agents follow the
  protocol by reference.
