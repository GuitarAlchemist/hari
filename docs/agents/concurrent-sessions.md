# Concurrent-session commit protocol

Multiple Claude Code sessions (typically ~4) run simultaneously across
the sibling repos (`hari`, `ix`, `ga`, `Demerzel`, …). Another session —
or the user — may commit, reset, or check out **while your edit is in
flight**. This protocol exists because a mid-air collision on
2026-07-19 silently discarded a staged change (`git commit` reported
"no changes added" after a concurrent reset; the work vanished without
an error).

Rules, in order of importance:

1. **Stage and commit in ONE shell invocation.** The gap between
   `git add` and `git commit` is where work is lost. Chain them:
   `git add <files> && git commit -m ...` — never two separate calls.
2. **Pin HEAD before editing; re-check immediately before committing.**
   ```bash
   PIN=$(git rev-parse HEAD)
   # ... edits ...
   NOW=$(git rev-parse HEAD)
   [ "$PIN" = "$NOW" ] || { echo "ABORT: HEAD moved"; exit 1; }
   git add <files> && git commit -m "..."
   ```
   If HEAD moved: stop, re-read the files you edited, reconcile, then
   retry. Never force through.
3. **Verify the commit landed** — `git log --grep '<distinctive
   phrase>'` — rather than trusting the exit code. The 2026-07-19
   failure also "succeeded" by exit code.
4. **Before editing a file in a sibling repo**, check
   `git status --short <area>` and the file's last commit. Uncommitted
   local modifications in your target area mean another session has a
   live edit: **stop and report, do not touch it.**
5. **Never push.** Pushes are the owner's call, always.
6. **Prefer narrow, additive edits** over rewrites, so a collision is
   recoverable.
7. **If your change disappears, surface it and re-read** — never
   blindly re-apply a stale edit over someone else's newer work (the
   newer work may have obsoleted yours: the 07-19 collision's
   replacement commit made the re-applied version *better*).

Subagent authors: include a pointer to this file in any prompt that
authorizes commits in a shared repo.
