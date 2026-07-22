# Multi-AI collaboration: Claude Code ↔ Codex / Gemini / local models (2026-07-22)

**Method.** Deep-research workflow (Fable 5 subagents): 5 angles → 22 sources (10
primary) → 109 claims → top 25 sent to 3-vote adversarial verification. The original run
was crippled by spend limits (70/104 verifier agents killed); a cached resume then
verified most, and the last 8 (official-vendor-docs claims) were cross-checked directly
against the source docs. **Final status (2026-07-22): 21 confirmed, 4 refuted, 0
unverified — see §7.** Per-claim markers: ✓ = confirmed; ✗ = refuted on adversarial
re-check; no ⚠ (unverified) claims remain.

## 1. The headline result: cross-vendor review is asymmetric

- ✓ **Direction matters, a lot.** Claude reviewing Codex-written code raised
  LiveCodeBench pass rate 71.6% → 89.7% (+18.1 pts)
  [[ResearchGate 407032793](https://www.researchgate.net/publication/407032793_Cross-Model_LLM_Code_Review_Should_you_use_Claude_to_review_Codex_or_vice_versa)].
- ⚠ The reverse direction was actively harmful in the same study: Codex reviewing
  Claude-written code *lowered* pass rate 91.4% → 82.8%. A weaker cross-vendor reviewer
  can degrade a stronger writer's output.

**Ecosystem implication (direct):** ga's merge flow treats Codex P0/P1 PR comments as a
gate on Claude-authored PRs — per this study that is the *harmful* direction, at least
when review feedback is applied unfiltered. Don't rip it out on one unverified number:
**measure it**. The G2 pr-grade machinery can grade Codex-review comments on
Claude-authored PRs for precision (how many flagged P0/P1s were real?). If precision is
low, demote Codex from gate to advisory on Claude-authored PRs, keep Claude-reviews-Codex
as a gate. That's the A/B doctrine applied to the reviewer itself.

## 2. Why cross-vendor review works — and its sharp limits

- ✓ **Self-preference is a familiarity effect.** LLM judges score lower-perplexity
  (more familiar) text higher regardless of authorship
  [[arXiv:2410.21819](https://arxiv.org/pdf/2410.21819)]. ⚠ Models have lower perplexity
  on their own outputs — the proposed mechanism for same-model review leniency, and the
  reason cross-vendor review reduces collusion risk.
- ⚠ **But cross-vendor ≠ independent.** Judge error correlations are barely lower
  cross-family (0.389) than within-family (OpenAI 0.437, Meta 0.435); a panel of 9
  frontier judges carried ~2.18 independent votes' worth of information
  [[arXiv:2605.29800](https://arxiv.org/html/2605.29800)]. Multiplying judges does not
  multiply reliability.
- ⚠ **Judges are swayed by framing, not just code.** Injected prompt biases flip
  pairwise code judgments even when the code is byte-identical; labeling a candidate as
  coming from the judge's own model family shifts preferences (self-enhancement via
  metadata) [[arXiv:2604.16790](https://arxiv.org/pdf/2604.16790)]. Prescribed
  mitigations: A/B position swapping, controlled prompt perturbations, bias-sensitivity
  metrics reported alongside accuracy.
- ⚠ **Panels of small judges are viable.** Composed judge pipelines
  (verify→debate→aggregate) make small models competitive with much larger judges;
  Panel-of-LLM-evaluators (PoLL) with several small models is a citable alternative to
  one frontier reviewer [[arXiv:2502.18018](https://arxiv.org/pdf/2502.18018)].

**Ecosystem implications:**
1. **Strip author-model metadata from review inputs** in cross-model-review CI
   (Demerzel) and any judge panel — the self-enhancement finding says the label alone
   biases the verdict. Present diffs identically regardless of author.
2. **Add position-swap A/B to any pairwise judging** (qa-tribunal, adversarial panels).
3. **Stop counting judge votes as independent.** One cross-vendor reviewer + the
   deterministic gates (tests, clippy, verify.ps1 — zero error-correlation with any LLM)
   beats a 5-judge panel that is really ~2 votes. This *confirms* the ecosystem's
   deterministic-first doctrine from the loop-engineering research.
4. **tars's local qwen hermetic gates are the right shape** — PoLL says cheap local
   panels can do triage; reserve frontier reviewers for the final pass.

## 3. Multi-agent skepticism (the counterweight)

- ⚠ Multi-agent systems often fail to beat simpler setups; failures cluster into **14
  modes across 3 categories** — system design, inter-agent misalignment, task
  verification (MAST taxonomy) [[arXiv:2503.13657](https://arxiv.org/abs/2503.13657)].
- ⚠ Stanford (Tran/Kiela): single agents match multi-agent architectures at the *same
  thinking-token budget* — reported multi-agent gains are often just extra compute
  [[VentureBeat](https://venturebeat.com/orchestration/are-you-paying-an-ai-swarm-tax-why-single-agents-often-beat-complex-systems)].
- ⚠ The decision boundary is **context degradation, not task complexity**: split work
  across agents when one context would be too long/noisy/corrupted, not because the task
  is hard. Handoffs are information-loss channels (Data Processing Inequality): every
  summarize-and-pass step loses data and adds a compounding-error site.

**Ecosystem implications:** hand off **artifacts, not summaries** (the JSON-contract
discipline the ecosystem already runs is the right mitigation); default to one agent
with a clean context; fan out only when context degradation is the binding constraint.
The MAST categories map onto existing infra: duplicated work → claims ledger;
verification → deterministic gates; misalignment → lane agreements in handoff notes.

## 4. Coordination mechanics between agents that can't message each other

- ⚠ **TASKS.md** ([github.com/tasksmd/tasks.md](https://github.com/tasksmd/tasks.md)):
  vendor-neutral work-queue convention claimed to work across Claude Code, Cursor,
  Devin, Codex, Gemini CLI, Windsurf. File backend = append your agent ID to a task
  line, commit+push immediately — explicitly **non-atomic best-effort**. For
  multi-writer fleets it offers a **git-native backend using git's atomic ref
  compare-and-swap**, demoting the file to a generated snapshot.

**Ecosystem implication:** our `~/.agents/claims.jsonl` (born 2026-07-21) is exactly the
file-backend pattern with the same known race. It worked for a 4-session day. If claim
collisions ever appear, the researched upgrade path is git-atomic-ref CAS claims with
the JSONL as a snapshot — don't invent a lock server. Also worth evaluating: moving the
ledger into a git repo at all (it's currently untracked homedir state — one machine
only, no history).

## 5. What the vendors officially support (the lingua-franca surfaces)

- ✓ **OpenAI/Codex**: PR comments are the coordination channel (`@codex review`
  trigger, or auto-review on PR open); behavior is steered by **AGENTS.md** files with a
  `## Code Review Rules` section, root-level + nested per-service. Confirmed verbatim
  against the official doc (2026-07-22): *"mention `@codex review`"* / *"turn on
  **Automatic reviews**"*; *"Codex searches your repository for `AGENTS.md` files and
  follows the applicable code review rules"*; *"Add a `## Code Review Rules` section to
  the file closest to the code the rules govern"*; *"Put repository-wide rules in the
  root `AGENTS.md` and service-specific rules in a nested file"*
  [[learn.chatgpt.com](https://learn.chatgpt.com/docs/third-party/github)] (was
  developers.openai.com; 308-redirected).
- ✓ **Anthropic**: Writer/Reviewer via **separate fresh-context sessions** (reviewer
  independence through context isolation, not necessarily a different vendor); **git
  worktrees** for parallel agents on one repo; **headless mode with JSON/stream-JSON
  output** as the official interface for external pipelines and other orchestrators. All
  three confirmed verbatim (2026-07-22): the Writer/Reviewer session table (*"A fresh
  context improves code review since Claude won't be biased toward code it just wrote"*),
  *"Worktrees: run separate CLI sessions in isolated git checkouts"*, and `claude -p …
  --output-format json` / `stream-json`
  [[code.claude.com/docs](https://code.claude.com/docs/en/best-practices)].
- (Fetched but low-claim-yield: A2A protocol adoption remains platform-level, not a
  coding-agent handoff standard in practice; MCP is the shared *tool* layer, not an
  agent-to-agent channel; AGENTS.md is the de-facto cross-vendor repo convention.)

**Ecosystem implications:**
1. **Add `## Code Review Rules` to AGENTS.md** in repos where Codex reviews (ga at
   minimum) — steer its P0/P1 bar there instead of hoping; ga already auto-syncs
   AGENTS.md from CLAUDE.md, so the section rides the existing sync.
2. **GitHub stays the lingua franca**: bots coordinate via comments/labels/checks, not
   private channels — the ecosystem's gemini/jules/codex workflows are already shaped
   right.
3. When gemini-dispatch or jules needs Claude Code output, call it headless with
   `--output-format json` rather than scraping text.

## 6. Recommendations ranked (for this ecosystem)

| # | Action | Cost | Grounding |
|---|---|---|---|
| 1 | Strip author-model metadata + add position-swap to cross-model-review CI and tribunals | Low | ⚠ 2604.16790 (bias flips on framing/labels) |
| 2 | Grade the Codex→Claude review gate's precision via pr-grades; demote to advisory if low | Low | ✓/⚠ 407032793 (direction asymmetry) |
| 3 | Add `## Code Review Rules` to ga's AGENTS.md (root + nest per-service) | Trivial | ✓ official Codex docs (confirmed verbatim) — **shipped** ga `39159837` |
| 4 | Keep deterministic gates primary; treat any judge panel as ~2 effective votes | Doctrine | ⚠ 2605.29800 |
| 5 | Claims ledger: keep JSONL; upgrade path = git atomic-ref CAS (TASKS.md pattern) if collisions appear; consider versioning the ledger | Low | ⚠ tasksmd |
| 6 | Hand off artifacts (JSON contracts), never summaries; single agent until context degrades | Doctrine | ⚠ DPI/Stanford |
| 7 | ~~Local-model PoLL panels for cheap triage~~ **RETRACTED** — supporting claims refuted 0-3 on re-verification (see §7); tars qwen gates stay valid as deterministic gates only | — | ✗ 2502.18018 refuted |

## 7. Verification debt — PAID (2026-07-22)

Original run: 23 of 25 claims ⚠ (spend limit killed the verifiers). The cached resume
re-ran adversarial verification: **13 confirmed 3-0, 4 REFUTED 0-3, 8 left unverified**
(limit hit again mid-run). Those last 8 — the Codex-docs and Anthropic-guidance claims —
were then verified **directly against the official docs** (low-budget path: two targeted
doc fetches, no workflow fan-out) on 2026-07-22 and all 8 came back **CONFIRMED verbatim**
(see §5, now ✓). Final tally: **21 confirmed, 4 refuted, 0 unverified.** No ⚠ claims
remain in this report.

One redirect of note: `developers.openai.com/codex/integrations/github` now 308-redirects
to `learn.chatgpt.com/docs/third-party/github` — citations updated.

**Upgraded to confirmed (selection):** the 9-judge-panel ≈ 2.18 effective votes result;
cross-vendor correlation — with harder numbers than the original extract: **Claude×Gemini
φ=0.603 and GPT-4o×Claude φ=0.588 are among the MOST correlated judge pairs**, so vendor
mixing buys even less independence than §2 stated; prompt-framing bias on code judges,
plus its prescribed guardrails (treat repeated-run disagreement, A/B-swap disagreement,
or bias-vs-no-bias shifts as escalation triggers; prefer presentation-invariant evidence
— compilation, tests, static analysis — over forcing an LLM verdict); GPT-4's 0.520
self-preference bias score; perplexity-aware vote down-weighting + multi-model ensembles
as mitigations; both TASKS.md claims (6-vendor support; git ref compare-and-swap
backend); MAST (14 failure modes / 3 categories, 1600+ annotated traces).

**REFUTED (0-3) — corrections to this report:**
1. *"Small-judge ensembles rival a single frontier reviewer"* and *"modular judge
   pipelines make small judges competitive with much larger ones"* (both from
   arXiv:2502.18018 as extracted) did not survive adversarial checking. **Recommendation
   #6/#7 (local-model PoLL panels for triage) is hereby RETRACTED** — tars's local qwen
   gates remain sound, but as *deterministic hermetic gates*, not as LLM judge panels;
   don't build local judge ensembles expecting frontier-reviewer substitution.
2. *"Adding judges gives negligible-or-negative returns vs the best single judge"* — the
   specific SNLI/MNLI numbers as extracted were also refuted; the confirmed, weaker form
   is the effective-sample-size result (panels ≈ 2 independent votes), which still
   supports "don't multiply judges," just without the stronger single-judge-dominance
   claim.
3. *"Self-preference transfers within a model family (same-family models favor each
   other)"* — refuted as stated; the confirmed core is judges over-rating low-perplexity
   text. Same-vendor review is still suspect (GPT-4's 0.520 self-bias is confirmed) but
   the family-transfer mechanism is NOT established.

**A2A correction to §5:** stronger than originally stated — confirmed 3-0: 150+
supporting orgs (AWS, Microsoft, IBM, Salesforce among them), stable 1.0 spec
(multi-tenancy, security flows, migration path), Linux Foundation governance,
explicitly complementary to MCP. Still no confirmed evidence of *coding-agent handoff*
usage specifically, but "platform-level only, low traction" undersold it — worth
re-checking in 6 months for coding-agent tooling.

Remaining 8 ⚠ claims: **now verified** — see the confirmation block at the top of §7.
The resume path (`resumeFromRunId: wf_18c02537-fbe`) is preserved for provenance but is no
longer needed; the official-docs cross-check settled all eight without another workflow run.
