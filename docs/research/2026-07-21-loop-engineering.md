# Loop engineering: deep research + ecosystem review (2026-07-21)

**Method.** Deep-research workflow (Fable 5 subagents): 5 search angles → 16 sources
fetched → 79 claims extracted → top 25 adversarially verified (3-vote refute panels).
**14 confirmed (0 refuted); 11 left unverified** because verifier agents hit session/spend
limits mid-run — those are marked ⚠ below and should be treated as plausible-but-unpinned,
not false. The synthesize agent also hit the limit, so this synthesis was done in the main
loop. Repo review: 3 parallel survey subagents over the 13 ecosystem repos
(inventory: session scratchpad `repo-loop-inventory.md`, summarized in §4).

## 1. What "loop engineering" is, and where it came from

**Definition (converged across sources):** stop being the person who prompts the agent;
design the *system* that prompts, verifies, and re-prompts the agent. Prompting is absorbed
as a substructure of a larger designed process — state management, verification, feedback
routing [Steinberger; secondary coverage, both 3-0].

Provenance timeline (confirmed where marked):

- **2025-02** — Claude Code ships; Simon Willison later names "designing agentic loops" a
  distinct new skill and defines an agent as "something that runs tools in a loop to
  achieve a goal" (fetched, not in verified top-25).
- **2025-09** — Willison, *Designing agentic loops*.
- **2025-11** — Anthropic, *Effective harnesses for long-running agents* [confirmed claims below].
- **2025-12→2026-02** — Every codifies **compound engineering** (Kieran Klaassen, GM of
  Cora) [3-0] — the "loops that learn" branch.
- **2026** — Steinberger: "you shouldn't be prompting coding agents anymore. You should be
  designing loops that prompt your agents" [3-0 — the most-cited seed of the term].
  ⚠ Boris Cherny saying his job is now "writing loops that prompt Claude" (Code with Claude
  conference) and ⚠ Addy Osmani's *Loop Engineering* essay (June 2026) both popularized it;
  ⚠ Anthropic's own *Getting started with loops* post (2026-06-30) adopted the term.

**Relation to prompt/context engineering:** prompt engineering optimizes a single
instruction; context engineering optimizes what the model sees; loop engineering optimizes
the *cycle* — what happens after the model acts: how work is verified, how feedback routes
back, when the loop stops, and what persists between iterations. Each subsumes the previous.

## 2. Confirmed engineering practices (the load-bearing findings)

**Harness design (Anthropic, effective-harnesses — all 3-0):**
1. **Compaction is not enough.** Even a frontier model in a plain loop across context
   windows fails to build production-quality software without harness scaffolding.
2. **One feature per session.** Constraining each session to exactly one feature from an
   initializer-written requirements file was *the* critical fix for one-shot-everything
   failure. (= our tracer-bullet/vertical-slice doctrine, independently confirmed.)
3. **Verify must be tool-enforced, not assumed.** Agents mark features complete without
   testing; explicit end-to-end verification with real tools (browser automation there;
   `cargo test`/`verify.ps1` here) largely eliminates it.
4. **Sessions are memoryless; git is the state/recovery substrate.** Design for discrete
   sessions: durable artifacts (requirements files, ledgers, digests) + git revert/recover.

**Verification ladder (Claude Code docs, 3-0):** escalating gate mechanisms —
in-prompt checks → a `/goal` condition re-evaluated every turn → a deterministic **Stop
hook** that blocks turn-ending until a script passes (overridden after 8 consecutive
blocks) → a **fresh-context verification subagent** so the agent doing the work isn't the
one grading it.

**Workflow shape (Claude Code docs, 3-0):** Explore (read-only) → Plan → Implement
(verify against plan) → Commit; *skip planning when the diff is describable in one
sentence* — right-size the loop to the task.

**Compound engineering (Every — all 3-0 unless marked):**
- Core law: *each unit of engineering work should make subsequent units easier — not harder.*
- The mechanism is a **learning loop**: bugs, failed tests, and a-ha insights get documented
  and consumed by future agents (docs/solutions, CLAUDE.md learnings, review-agent lessons).
- Prescribed loop: **Plan → Work → Assess → Compound** (plugin variant: brainstorm → plan →
  work → simplify → review → compound, "repeat with better context" [2-0]).
- **Effort is 80% at the edges** (plan + review), 20% in the middle (work + compound).
- ⚠ Plugin mechanism: `/ce-compound` writes `docs/solutions/`, later `/ce-brainstorm` /
  `/ce-plan` read it as grounding context [1-0, limits].

**⚠ Unverified-by-limits but source-attributed (claude.com loops post / docs):**
- Loop taxonomy by trigger / stop condition / primitive / task type; four types:
  turn-based (manual), goal-based (`/goal` + deterministic success criteria), time-based
  (`/loop`, `/schedule`), proactive (composed, no real-time human).
- Encode manual verification steps as a SKILL.md; the more quantitative the check, the
  easier agent self-verification becomes.
- CLAUDE.md is *advisory* and should stay minimal (bloat measurably degrades
  instruction-following); hooks are *deterministic*; skills are *on-demand* — put each rule
  in the right layer.
- Close every loop with a machine-readable check (tests, exit codes, linters, diff
  scripts, screenshots) so the human isn't the verification step.
- Ralph loop (Geoffrey Huntley): minimal viable loop = bash `while`, prompt file piped into
  the agent; each iteration picks ONE task, implements, validates.

## 3. The operator's checklist (synthesis)

A repo is "loop-engineered" when:
1. **Entry point** — a minimal, accurate CLAUDE.md (advisory layer) that documents the
   verify command; no claims that drift from reality.
2. **Machine-readable verify** — one command (tests+lint+build) an agent can run locally,
   *and* CI that runs the same thing, so "looks done" is never the stop condition.
3. **Deterministic gates** — hooks/CI for rules that must always hold (the model never
   has to remember them).
4. **Durable state between sessions** — digests/ledgers/requirements files + git
   discipline (memoryless-session assumption).
5. **One-slice work units** — tracer-bullet vertical slices; issues sized to one session.
6. **Independent grading** — review/verification by a fresh context (subagent, CI verdict,
   or second model), not the author-agent.
7. **A compounding channel** — somewhere learnings land (solutions docs, session-learned
   rules, ADRs) that future sessions *actually read*.
8. **A/B doctrine** — every new behavior beats a simpler baseline (already ecosystem law;
   independently mirrored by Anthropic's harness-iteration methodology).

## 4. Ecosystem review (13 repos, surveyed 2026-07-21)

Tiers (full inventory in session scratchpad; keyed to checklist items 1–8):

| Repo | Tier | Weakest checklist items |
|---|---|---|
| ga | 1 (reference) | none structural — sprawl risk (item 1: 192-line CLAUDE.md tests the "minimal advisory" guidance) |
| Demerzel | 1 | none structural; stray crash artifact in tree |
| ix | 1 | no commands; no fleet/handoff layer (nice-to-have) |
| hari | 2 | 7 (CONTEXT.md stub, no ADRs despite convention); tree hygiene |
| agent-blackbox | 2 | 4 (no session digests — CI-only governance); 7 (no CONTEXT/ADR) |
| tars | 2 | 1 (**CLAUDE.md claims skills that aren't installed**), 3 (no gates), 6, 7 (empty learned-rules) |
| ga-org-github | 2 | 1 (**no CLAUDE.md at all** — no single entry point) |
| demerzel-bot | 3 | 1 (boilerplate pointing at nonexistent docs), 2 (**no CI, no test framework**), 4–7; `.env` in tree (verify ignored) |
| ga-godot | 3 | 1 (boilerplate, no Godot guidance), 2 (**no verification of any kind**) |
| fin | 3 | 2 (**documents `-D warnings` bar but has no CI enforcing it**) |
| guitaralchemist.github.io | 3 | 1, 2 (no CI, no checks) |
| afk-harness | 3 | 1 (**no CLAUDE.md/.claude at all** — ironic: it IS loop infra), 2 (`npm test` = exit-1 stub) |
| mergerisk | 3 | 1 (no CLAUDE.md), 2 (**CI self-scan diffs `HEAD..HEAD` = empty diff — gates nothing**) |

Cross-cutting findings:
- The doctrine layer (Karpathy rules, Cherny loops, tracer-bullets, A/B baselines,
  autocompact=40%) is uniform; the *tooling* layer is power-law distributed (3 repos have
  ~90% of it).
- **Anti-pattern found twice: verification theater** — mergerisk's empty-diff self-scan and
  fin's unenforced clippy bar look like loops but close nothing. Worse than no loop,
  because they read as "covered".
- **Anti-pattern: advisory drift** — tars claims uninstalled skills; demerzel-bot/ga-godot
  reference nonexistent CONTEXT/ADR files. The advisory layer must never lie (item 1).
- The ADR convention is documented everywhere and practiced almost nowhere (only
  ga/Demerzel/ix/tars have any).

## 5. Adoption plan (dogfood)

P0 — close fake/missing verify loops (checklist item 2):
1. mergerisk: fix CI self-scan to diff a real range; keeps dogfooding honest.
2. fin: add CI enforcing the already-documented fmt/clippy/test bar.
3. demerzel-bot: confirm `.env` ignored; add smallest real CI (node syntax + classifier test).

P1 — make every advisory layer minimal and TRUE (item 1):
4. tars: remove false skill claims from CLAUDE.md (or install the pack — owner call; default: fix the doc).
5. demerzel-bot, ga-godot: replace boilerplate CLAUDE.md with accurate minimal entry points
   (run/test commands, architecture in 3 lines, no dead references).
6. afk-harness: add CLAUDE.md + fix `npm test` stub (exit 0 no-op is still lying; wire the
   smallest real check).
7. ga-org-github: add CLAUDE.md entry point pointing at the root governance docs.
8. guitaralchemist.github.io: minimal accurate CLAUDE.md.
9. hari: gitignore `bash.exe.stackdump` + `target-test/`; (CONTEXT.md/ADR backfill = P2).

P2 — compounding channels (items 6–7), owner-review items:
- Backfill ADRs where decisions already exist in prose (hari substrate decision is a
  ready-made ADR-001). Grow CONTEXT.md stubs.
- Consider porting hari/Demerzel digest hooks to tars (complete its partial set) and
  agent-blackbox (currently CI-only).
- Loop-quality metric worth tracking (per claude.com taxonomy): % of merges that passed a
  machine-readable gate vs. human-eyeball; % of sessions ending with a digest written.

P0/P1 are mechanical and reversible; each lands as one guarded commit per repo, no pushes.

## 6. Execution status (cross-session coordination)

- **P1 #4 (tars) — DONE** by the tars session, 2026-07-21: CLAUDE.md advisory drift fixed
  (default option: doc corrected, skills not installed) — tars commit `79c71013` on
  `refactor/reason-feedback-seam`. Also relevant to §4's tars row: item 7 is no longer
  empty-channel — 12 verified research docs landed in tars `docs/research/` (`a55efaaf`)
  and seeded a round-3 self-improve backlog (`d258b4d1`). Tars digest hooks (P2 port
  candidate) are already partially present: 6 hooks incl. activity tracker + validate.
  Claimed via tars `state/digests/latest.md`; coordinate there before touching tars items.
- **Cross-session claim ledger now exists**: `~/.agents/claims.jsonl` (schema in
  `~/.agents/README.md`) — append a `claimed` line before starting any lane in §5;
  seeded 2026-07-21 by the tars session after the ix session confirmed no explicit
  channel exists. Latest line per (repo, lane) wins; append-only.
