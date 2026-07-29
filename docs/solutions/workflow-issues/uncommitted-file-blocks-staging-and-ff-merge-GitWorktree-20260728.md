---
module: Concurrent-Session Git Workflow
date: 2026-07-28
problem_type: workflow_issue
component: development_workflow
symptoms:
  - "git merge --ff-only aborts with 'Your local changes to the following files would be overwritten by merge'"
  - "the branch is provably fast-forwardable (git merge-base --is-ancestor exits 0) yet the merge still refuses"
  - "git add <file> would stage another session's unfinished work along with your own"
  - "the file you need to edit shows ' M' in git status and you did not touch it"
root_cause: shared_working_tree_contention
resolution_type: workflow_improvement
severity: high
tags: [git, worktree, concurrent-sessions, multi-agent, fast-forward, staging, rust, windows]
---

# An uncommitted file owned by another session blocks both staging and fast-forward merge

## Problem

Four Claude Code sessions plus Codex and Gemini CLIs work in this repo at once.
Session A left `crates/hari-core/src/lib.rs` modified-but-uncommitted — 71
insertions of unrelated in-progress work (issue #14 `source_weights`). Session B
needed to add two fields to structs in that **same file** (issue #35
`ReplayCalibration`, `false_rejection_count`).

Both obvious paths are wrong:

- `git add crates/hari-core/src/lib.rs` stages the *whole file*, so committing
  would silently publish Session A's unfinished work under Session B's commit
  message. Staging is per-file, not per-hunk, and there is no non-interactive
  `git add -p`.
- Branching and merging back fails too — see below.

This is the steady state, not an edge case: at time of writing, **4 of 8 live
worktrees were dirty**.

## Symptoms (exact)

```text
$ git merge --ff-only feat/replay-calibration
error: Your local changes to the following files would be overwritten by merge:
	crates/hari-core/src/lib.rs
Please commit your changes or stash them before you merge.
Updating 5bc6703..67f4747
Aborting
```

```text
$ git status --porcelain -- crates/hari-core/src/lib.rs
 M crates/hari-core/src/lib.rs
```

Note the **leading space**: ` M` is modified-unstaged, `M ` is staged. Exit code
is `0` either way, so test the *output*, never `$?`.

## Investigation

1. **Wait for the other session to commit** — rejected as the only plan. The
   blocking edit turned out to be **8 days stale** (working-copy mtime
   `2026-07-20 23:47`), so "wait" is unbounded. See Prevention for the staleness
   check that catches this.
2. **`git stash` the other session's work** — rejected outright. Destructive to
   another agent's in-flight state and forbidden by
   `docs/agents/concurrent-sessions.md`.
3. **Commit their work along with mine** — rejected. It attributes unfinished,
   untested work to an unrelated commit message and defeats the atomic-commit
   protocol.
4. **Extract only my hunks via `git apply --cached`** — technically possible
   (the two changes sit in different line ranges) but fragile: hunk offsets
   interleave in the combined diff and any edit invalidates the extraction.
5. **Check ancestry and assume the merge is safe** — this is the trap. Verified:

   ```text
   $ git merge-base --is-ancestor main feat/replay-calibration; echo "exit=$?"
   exit=0
   ```

   The branch **is** fast-forwardable, and the merge **still** aborts. Ancestry
   is not the blocker; working-tree overlap is. A green ancestry check proves
   nothing about whether the merge will run.

## Root cause

A fast-forward merge is a **checkout**. It must rewrite every working-tree file
that differs between the two commits. Git refuses to overwrite a file with
uncommitted local modifications — correctly, and it aborts *before* touching
anything, leaving both `main` and the other session's work intact.

So the blocker is not the branch's history at all. It is the intersection of
two sets:

```
{ files my branch changes }  ∩  { files dirty in the working tree }
```

Non-empty ⇒ blocked. This is a property of the **working tree**, and one
repository has exactly one of those per checkout. That is the whole problem, and
it points directly at the fix: get another working tree.

## Solution

Develop in a `git worktree` branched from the clean base. A worktree is a second
working directory backed by the same object store, with its own index and
`HEAD`, so it is completely unaffected by dirt in the primary tree.

```bash
git worktree add .claude/worktrees/<short-name> -b <branch> main
```

Then work, test, and commit entirely inside it:

```bash
cd .claude/worktrees/<short-name>
# edit, cargo test -p <crate>, cargo fmt, cargo clippy
git add -A && git commit -m "..."
```

`.claude/worktrees/` is **already gitignored** (`.gitignore:24`), so the
worktree never appears in `git status` and never pollutes the primary tree.
Verified: a freshly created worktree reports a completely empty
`git status --porcelain` despite system-wide `core.autocrlf=true` and no
`.gitattributes` — the CRLF warning is cosmetic stderr noise and is not
inherited as phantom dirt.

Merge back only once the overlap set is empty:

```bash
git merge --ff-only <branch>
```

If the other session is still holding the file, **leave the branch unmerged and
say so**. The work is committed and safe; a pending fast-forward is a normal,
honest end state, not a failure.

**Use the repo's own convention, not the plugin's.** The
`compound-engineering:git-worktree` skill says "NEVER call `git worktree add`
directly" and creates worktrees under `.worktrees/` — that path is **not** in
this repo's `.gitignore` and would show up as untracked noise. This repo already
uses `.claude/worktrees/` (Claude Code's own `agent-<hex>` worktrees live
there); Codex uses `C:/tmp/hari-*`.

## Prevention

- **Check the overlap set before promising a merge, not the ancestry.** This is
  the one command that predicts the abort without running it:

  ```bash
  comm -12 \
    <(git diff --name-only main...<branch> | sort) \
    <(git status --porcelain --untracked-files=no | awk '{print $2}' | sort)
  ```

  Empty output ⇒ the fast-forward will succeed. Non-empty ⇒ it will abort, and
  the listed files are exactly why.

- **Gate every shared-file edit on a dirty check first.** `git status --porcelain
  -- <file>`; non-empty means another session owns it. Cheapest possible check,
  run it before planning the work, not after writing it.

- **Size the other session's work before deciding.** `git diff --stat -- <file>`.
  71 insertions is real work — never stage it. A one-line whitespace touch is a
  different conversation.

- **Distinguish a live collaborator from an abandoned edit.** `docs/agents/
  concurrent-sessions.md` rule 4 says "stop and report, do not touch it", which
  assumes someone is actively editing. Compare the file's mtime against the last
  commit touching it; if the edit is more than ~24h stale, the correct move is to
  escalate to the owner rather than block forever on a session that already
  ended.

- **Keep worktree directory names short.** Nesting under `.claude/worktrees/`
  costs ~30–40 characters of path. Measured longest path: 174 chars in the primary
  tree, 203 in `calib-wiring`, 214 in `agent-a34b4fb1390ff6e67` — against a 260
  limit, since `core.longpaths` is not set. All the length comes from
  `target/debug/incremental/…`.

- **Budget the disk before spawning worktrees.** Each gets its own `target/` and
  starts from a cold cargo cache: measured 6.0G + 2.0G + 1.2G = **9.2G across
  three checkouts**. Worktrees are cheap in git terms and expensive in Rust terms.
  (A shared `CARGO_TARGET_DIR` would fix the disk cost but reintroduce
  cross-session build contention — rejected for that reason.)

- **Don't reach for a worktree by default.** Commit directly when the target
  files are verifiably clean *and* the change is one slice you will finish now,
  protected by the HEAD-pinned atomic `git add … && git commit` from rules 1–2.
  Reserve worktrees for: a dirty target file, needing to run `cargo test --all`
  while someone else is mid-edit, work spanning sessions, or a background agent.

- **Prune stale worktree admin entries.** `git worktree prune` — a scratchpad
  worktree outlived its directory and its `.git/worktrees/` entry lingered,
  showing up as a phantom `MISSING` row in any cross-worktree scan.

## Related

- `hari:docs/agents/concurrent-sessions.md` — the canonical protocol; this doc is
  the incident record. **Rule 4 currently dead-ends at "stop and report, do not
  touch it" with no way to proceed. The worktree is the missing escape hatch:
  it converts "blocked, stop" into "proceed in isolation, merge later."**
  Amending rule 4 to reference this doc is an owner call.
- `hari:docs/solutions/workflow-issues/staged-commit-lost-to-concurrent-session-GitWorkflow-20260719.md`
  — the direct predecessor: same failure family (concurrent sessions contending
  for one working tree), but the *loss* branch, where a staged change was
  silently destroyed. This is the *prevention* branch, where git refused loudly
  and nothing was lost.
- `hari:docs/solutions/integration-issues/mcp-shared-browser-profile-lock-concurrent-sessions-20260723.md`
  — the generalisation: a shared single-instance resource under N concurrent
  sessions. There it was one Chrome `--user-data-dir`; here it is one working
  tree.
- `hari:docs/methodology/agentic-engineering.md:109` — "Move work AFK. Scope a
  task tightly, hand it to a sandboxed agent (a git worktree off `main` …)" —
  states the aspiration this doc turns into a procedure.
- `hari:docs/research/2026-07-22-multi-ai-collaboration.md:112` — cites
  Anthropic's "Worktrees: run separate CLI sessions in isolated git checkouts"
  as verified upstream guidance.
