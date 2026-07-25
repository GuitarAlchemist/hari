---
module: MCP Browser-Automation Integration
date: 2026-07-23
problem_type: integration_issue
component: notebooklm_mcp_server
symptoms:
  - "MCP browser tool fails with 'browserType.launchPersistentContext: Target page, context or browser has been closed'"
  - "browser log shows 'Opening in existing browser session' then the launched pid is immediately killed"
  - "the same query alternates between a launch-lock error and a 'Browser page unresponsive: health check timed out'"
  - "killing the Chrome child processes does not stick — they respawn within seconds"
root_cause: shared_single_instance_resource_contention
resolution_type: process_lifecycle_fix
severity: high
tags: [mcp, playwright, patchright, browser-profile, concurrent-sessions, multi-agent, notebooklm, windows, chrome]
---

# MCP browser tool blocked by a shared Chrome profile across concurrent sessions

## Problem

With ~4 Claude Code sessions running across sibling repos, the `notebooklm`
MCP server's `ask_question` tool could not drive its browser. `get_health`
reported `authenticated: true` and the notebook was registered and active, yet
every query failed at the point of launching the browser. The auth was fine; the
**browser profile was contended**.

## Symptoms (exact)

```text
browserType.launchPersistentContext: Target page, context or browser has been closed
<launched> pid=82988
[pid=82988][out] Opening in existing browser session.   <-- the tell
[pid=82988] <kill> ... <process did exit: exitCode=0>
```

And, on the attempts that got further:

```text
{ "success": false, "error": "Browser page unresponsive: health check timed out" }
```

Two distinct failure shapes alternated: a **launch-lock** (`Opening in existing
browser session` → immediate self-kill) and a **page-unresponsive timeout** when
the box was under heavy CPU load from other sessions.

## Investigation

1. **First read — "another Chrome is on the profile."** The `Opening in existing
   browser session` line means Chrome found a live instance already using the
   `--user-data-dir` and handed off to it, so Playwright's own launch exited and
   closed the context. Confirmed by enumerating processes on the exact profile
   path:
   ```powershell
   Get-CimInstance Win32_Process -Filter "Name='chrome.exe'" |
     Where-Object { $_.CommandLine -like '*notebooklm-mcp*accounts*main*chrome_profile*' }
   ```
   → 9 Chrome processes, all launched ~2h earlier (a leftover login window that
   never self-closed).
2. **`Stop-Process` on those PIDs did NOT stick** — a re-count still showed 9.
   The MCP-server-owned parent kept respawning the children as fast as they were
   killed.
3. **`close_session` cleared the logical session but not the OS processes.** The
   server reported the session closed, but the orphaned Chrome tree survived and
   kept the profile locked.
4. **The real scope: too many servers, one profile.** Counting the MCP server
   processes revealed the actual cause:
   ```powershell
   Get-CimInstance Win32_Process -Filter "Name='node.exe'" |
     Where-Object { $_.CommandLine -like '*notebooklm-mcp*' }
   ```
   → **10 `notebooklm-mcp` node servers** (ages 1–11 min), accumulated from `/mcp`
   reconnects and `/reload-plugins` across sessions, **all pointing at the single
   shared `accounts/main` Chrome profile**.

## Root cause

A persistent-context browser profile (`--user-data-dir`) is a **single-instance
resource**: exactly one live Chrome may own it. The setup violated that two ways
at once:

- **Many-to-one contention.** 10 duplicate MCP server processes (one per session
  reconnect) all tried to drive one shared profile. Every query raced the others;
  the loser got `Opening in existing browser session` and died.
- **Leaked subprocess on failure.** When a query failed, the server did **not**
  reap its Chrome subprocess, so the orphaned tree kept the profile locked and
  re-blocked the next attempt — a self-perpetuating lock.

`Stop-Process` fought a losing battle because it killed *children* while the
server-owned *parent* respawned them. The fix is to cut from the root of the
process tree, and — durably — to stop running N servers against one profile.

## Solution

**Immediate unblock — tree-kill from the root pid** (whose parent is the node MCP
server), not the children:

```powershell
$p = Get-CimInstance Win32_Process -Filter "Name='chrome.exe'" |
  Where-Object { $_.CommandLine -like '*notebooklm-mcp*accounts*main*chrome_profile*' }
foreach ($proc in $p) { & taskkill /F /T /PID $proc.ProcessId }   # /T = whole tree
```

`taskkill /F /T` from the root terminates the parent-plus-children atomically, so
nothing respawns. After this the profile lock clears (`0 remaining`) and the
server launches a fresh browser it fully owns.

**Use the server's own lever for a live, server-owned session** — don't process-kill
it:

```
close_session(session_id)   # correct for a session the server is managing
```

Process-kill is only for *orphaned* trees the server has already lost track of.

**Durable fix — one profile, one driver.** The root problem is N servers on one
profile. Quiesce the duplicates: close the other Claude sessions that loaded the
same MCP server (each `/quit` takes its server down), leaving ~1 server for the
one profile. Then run the query once, clean.

## Prevention

- **Never point multiple MCP-server instances at one persistent browser profile.**
  If several sessions need the tool concurrently, give each its own profile
  (e.g. per-account or per-session `--user-data-dir`), or serialize access.
- **Before driving a shared-profile browser tool, check for contention first:**
  count `node.exe` server processes and `chrome.exe` processes on the profile
  path. More than one server = expect collisions.
- **Reap on failure.** A browser-automation MCP server should kill its own Chrome
  subprocess when a query errors; a leaked tree re-locks the profile. (Server-side
  bug worth filing upstream — worked around here by manual tree-kill.)
- **Prefer the server's `close_session` / `reset_session` over OS kills** for
  live sessions; reserve `taskkill /F /T` for orphaned trees, and always kill from
  the **root** pid, never the leaves.
- **A stale login window counts as contention.** A one-shot login helper that
  "self-closes on success" may not; verify the profile is actually free before the
  first automated query.
- **Ownership boundary (multi-session):** do not blind-kill browser processes that
  another live session may be mid-query on. Identify the owning server first; if
  the profile is shared, quiesce by asking the owner to close, don't force it.

## Related

- `hari:docs/solutions/workflow-issues/staged-commit-lost-to-concurrent-session-GitWorkflow-20260719.md`
  — same theme (multi-session interference on a shared resource that reports
  success-shaped failures); different resource (git tree vs. browser profile).
- `hari:docs/agents/concurrent-sessions.md` — the canonical cross-session
  discipline this incident applied (identify the owner, quiesce don't force).
- `~/.agents/claims.jsonl` — the cross-session claim ledger; the NotebookLM lane
  was released here (ownership reassigned to demerzel) once the contention was
  understood.
