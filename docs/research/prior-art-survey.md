# Project Hari: Prior-Art and Novelty Survey

Date: 2026-04-29
Author: Research synthesizer (commissioned)
Audience: Project owner (senior engineer, exploratory research)

---

## 1. TL;DR

- **The 6-valued lattice is not original — it is independently re-derived.** Coniglio & Rodrigues (2022/2023) published a six-valued semantics for the logics LETK+/LETF+ that explicitly extends Belnap-Dunn with two additional values for "reliable" positive/negative evidence. Hari's `Probable`/`Doubtful` axis maps almost exactly to their reliability axis. This is closer to "rediscovery" than novelty, but Hari's *engineering* of `Contradictory` as an off-chain absorbing element is a defensible design choice.
- **Subjective Logic (Jøsang) and Dempster–Shafer already do most of what Hari's belief substrate aims to do, plus trust-weighted multi-agent fusion.** Both have decades of formal results, conflict-aware fusion operators, and explicit uncertainty mass — features Hari currently lacks.
- **Lie-algebra-on-cognitive-state is genuinely under-explored** as framed in Hari, but **Quantum Cognition (Busemeyer & Bruza)** already occupies the closest adjacent niche — Hilbert-space state evolution for belief and decision. This is the most important comparator the project has not yet cited; it should be addressed head-on.
- **Hari's IX integration target sits in a real gap.** Current AI-scientist systems (Sakana AI Scientist, FunSearch, AI-Researcher, Agent Laboratory) are documented as having weak-to-absent epistemic state tracking — they rely on LLM context windows or ad-hoc memory. A typed, contradiction-preserving claim layer is a credible wedge.
- **The `explorer/critic/integrator/guardian` swarm pattern is a re-skinning** of standard role-based multi-agent debate / deliberative collective intelligence. The minority-preserving consensus is well-aligned with current 2025 literature but is not novel by itself.

---

## 2. Cluster-by-cluster findings

### 2.1 Many-valued and paraconsistent logics for AI belief

**Prior art summary.** Belnap's four-valued logic (1977) — values {True, False, Both, Neither} — is the canonical paraconsistent/paracomplete framework for handling inconsistent and incomplete data, and is the direct ancestor of modern "evidence logics." Subjective Logic (Jøsang, 2001–2016) represents opinions as 4-tuples (belief, disbelief, uncertainty, base rate) with `b+d+u=1`, and is *the* formalism for trust-weighted reasoning under uncertainty. Dempster–Shafer evidence theory (1976) generalizes Bayesian probability to belief functions over power sets and supports explicit conflict measures. Probabilistic Logic Networks (Goertzel et al., 2008) represent truth as a second-order distribution.

Most directly relevant: **Coniglio, Rodrigues, et al. (2022, arXiv:2209.12337; Studia Logica 2023, doi 10.1007/s11225-023-10062-5)** introduce **LETK+ / LETF+**, six-valued logics of evidence and truth. Their lattice extends Belnap-Dunn's four values with two new ones interpreted as "reliable positive information" and "reliable negative information" — i.e., a probable/doubtful axis on top of true/false/both/neither.

**Hari's relationship.** Hari's `{T, P, U, D, F, C}` is structurally parallel to LETK+/LETF+: T/F/U/C ≈ Belnap (with `U` ≈ Neither, `C` ≈ Both rebadged as absorbing), and P/D ≈ the reliable-evidence axis. Hari's design choice that `Contradictory` sits **outside the chain** as an absorbing element is a real engineering departure (LETK+ keeps a uniform lattice), but the underlying ontology is convergent.

**Novelty verdict.** *Likely reinventing.* The hexavalent ontology has been formally published at least twice independently. Hari's design has practical merit but should be presented as a reimplementation/specialization, not a novel logic.

**Key references.**
- Belnap, "A Useful Four-Valued Logic" (1977)
- Coniglio & Rodrigues, "On six-valued logics of evidence and truth expanding Belnap-Dunn four-valued logic" (arXiv:2209.12337, 2022)
- Coniglio & Rodrigues, "From Belnap-Dunn Four-Valued Logic to Six-Valued Logics of Evidence and Truth" (Studia Logica, 2023)
- Jøsang, *Subjective Logic: A Formalism for Reasoning Under Uncertainty* (Springer, 2016)
- Dempster–Shafer theory (Shafer 1976; modern conflict-fusion surveys 2020+)
- Goertzel et al., *Probabilistic Logic Networks* (Springer, 2008)

---

### 2.2 Multi-agent belief revision, consensus, and trust

**Prior art summary.** Opinion dynamics has a strong literature: DeGroot (1974), Friedkin–Johnsen, and Hegselmann–Krause bounded confidence (2002), with a recent comprehensive survey (Bernardo, Vasca, Iervolino, *Automatica* 2023, doi:10.1016/j.automatica.2023.111271). AGM belief revision (Alchourrón–Gärdenfors–Makinson, 1985) and its multi-agent extensions handle revision under new information. BDI (Rao & Georgeff, 1995) gives belief-desire-intention semantics for agent updates. Subjective Logic includes transitivity and fusion operators for trust networks. Dempster–Shafer has well-known multi-source conflict-fusion variants (PCR rules, Yager's rule). 2025 LLM literature has converged on **deliberative collective intelligence** with explicit minority-report packets (e.g., "From Debate to Deliberation," arXiv:2603.11781).

**Hari's relationship.** Hari's role-based agents (`explorer/critic/integrator/guardian`) with `self_trust`/`message_trust` parameters are a sensible operational design but are not formally distinct from trust-weighted Subjective Logic fusion. The minority-preserving consensus is well-aligned with current 2025 multi-agent LLM work but is not a new insight.

**Novelty verdict.** *Partial overlap, leaning reinventing.* The role taxonomy is engineering flavor; the underlying mechanism (trust-weighted aggregation of opinions over a many-valued space) is squarely in subjective-logic / DS-fusion territory.

**Key references.**
- Hegselmann & Krause, "Opinion dynamics and bounded confidence" (*JASSS*, 2002)
- Bernardo, Vasca, Iervolino, "Bounded confidence opinion dynamics: A survey" (*Automatica*, 2023)
- Jøsang, *Subjective Logic* (2016) — chapters on trust networks and fusion
- "From Debate to Deliberation: Structured Collective Reasoning with Typed Epistemic Acts" (arXiv:2603.11781, 2026)
- Rao & Georgeff, "BDI Agents: From Theory to Practice" (ICMAS 1995)

---

### 2.3 Geometric / Lie-algebra approaches to cognition or reasoning

**Prior art summary.** Lie groups and algebras are pervasive in **geometric deep learning** — Bronstein et al. (2021), Cohen & Welling on equivariant CNNs, Lie Algebra Convolutional Networks, neural ODEs on manifolds — but these are about *symmetry-respecting representation learning over physical/spatial data*, not over belief or epistemic state. In **cognitive science**, the dynamical-systems hypothesis (van Gelder 1998; Spivey 2007 *The Continuity of Mind*) treats cognition as continuous trajectories in state space, but with no Lie-algebraic structure imposed. **Quantum Cognition** (Busemeyer & Bruza, *Quantum Models of Cognition and Decision*, Cambridge 2012) is the strongest direct comparator: it represents beliefs as vectors in Hilbert space, models judgments as projection operators, and exploits non-commutativity to capture order effects. The unitary evolution `U = exp(-iHt)` is structurally identical to Hari's matrix-exponential evolution under a cognitive Hamiltonian — Quantum Cognition has been doing this since the early 2000s, with experimental support for human order-effects, conjunction fallacies, and preference reversals.

**Hari's relationship.** Hari's Lie-algebra framing — generators, commutators, structure constants, `dψ/dt = H(ψ)`, evolution via `exp` — is mathematically a near-restatement of the Quantum Cognition apparatus, minus the explicit Hilbert-space inner product and probability axioms. The *application* (composing belief-revision and goal operations and analyzing order via commutators) is novel relative to Quantum Cognition's typical experimental-psychology focus. It is not novel relative to "use Lie-algebraic dynamics to model cognitive state."

**Coherence flag.** No source flags the framing as mathematically incoherent. The pragmatic concern is that Hari's `Hamiltonian` is currently hard-coded (per CLAUDE.md), so the mathematical structure isn't yet bearing weight. Without a learned or principled `H` and meaningful invariants, the Lie-algebra layer risks being expensive ornamentation over what reduces to "matrix-exponential-weighted attention vector."

**Novelty verdict.** *Partial novelty.* The exact framing — Lie-algebra generators acting on a state vector that includes hexavalent belief context, with structure-constant analysis applied to autoresearch decision-making — is, to my reading, not published. But the *mathematical core* is well-trodden by Quantum Cognition and by neural-ODE work. Speculative novelty is fine for a research sandbox, but the project should explicitly cite Quantum Cognition or risk being framed as unaware of it.

**Key references.**
- Busemeyer & Bruza, *Quantum Models of Cognition and Decision* (Cambridge, 2012)
- van Gelder, "The dynamical hypothesis in cognitive science" (*BBS*, 1998)
- Spivey, *The Continuity of Mind* (Oxford, 2007)
- Bronstein, Bruna, Cohen, Veličković, *Geometric Deep Learning* (2021)
- "Geometric Deep Learning and Equivariant Neural Networks" (Gerken et al., *Artificial Intelligence Review*, 2023; arXiv:2105.13926)
- Chen et al., "Neural Ordinary Differential Equations" (NeurIPS 2018) and Bayesian Neural ODEs (arXiv:2012.07244)

---

### 2.4 Cognitive architectures and neuro-symbolic systems

**Prior art summary.** SOAR (Laird) and ACT-R (Anderson) are the canonical symbolic cognitive architectures; both have decades of perceive–think–act loop work and explicit working/declarative/procedural memory. CLARION integrates symbolic and sub-symbolic processing. OpenCog/Hyperon (Goertzel) is the closest *spiritual* cousin: it has PLN for uncertain inference, an AtomSpace knowledge representation, and explicit AGI ambitions. LIDA implements a global-workspace cognitive cycle. A 40-year survey (Kotseruba & Tsotsos, *AI Review* 2018, doi:10.1007/s10462-018-9646-y) catalogs ~50 cognitive architectures.

**Hari's relationship.** Hari's `Perceive → Think → Act` cycle with belief network + goals + attention is *structurally* a small cognitive architecture — closest to a stripped-down OpenCog with PLN replaced by hexavalent lattice and AtomSpace replaced by `BeliefNetwork`. Hari is much smaller scope (no procedural memory, no learning, no language) and does not aim to be a general cognitive architecture per the README's narrowed hypothesis.

**Novelty verdict.** *Partial overlap.* The architectural skeleton is generic; the substrate choices (hexavalent + Lie-algebra + swarm) are the differentiator, not the loop itself. As long as Hari does not market itself as a cognitive architecture, this overlap is not a problem.

**Key references.**
- Anderson, *How Can the Human Mind Occur in the Physical Universe?* (2007) — ACT-R
- Laird, *The Soar Cognitive Architecture* (MIT Press, 2012)
- Goertzel et al., *Engineering General Intelligence* (Atlantis, 2014) — OpenCog/PLN
- Kotseruba & Tsotsos, "40 years of cognitive architectures" (*AI Review*, 2018)
- "An Analysis and Comparison of ACT-R and Soar" (arXiv:2201.09305, 2022)

---

### 2.5 Autoresearch / AI-scientist systems

**Prior art summary.** Sakana **AI Scientist** v1/v2 (Lu et al., 2024–2025) automates idea generation, code, experiments, and paper writing; independent evaluation (Beel et al., arXiv:2502.14297) found weak literature review, ~42% experiment failure, and routinely-misclassified novelty. **FunSearch** (Romera-Paredes et al., *Nature* 2024) couples LLM with evolutionary search for code; epistemic state is the program-pool fitness, nothing richer. **AI-Researcher** (arXiv:2505.18705) and **Agent Laboratory** (2025) follow the same Lit Review → Idea → Code → Paper pipeline. The 2025 survey "From AI for Science to Agentic Science" (arXiv:2508.14111) and "Agentic AI for Scientific Discovery" (arXiv:2503.08979) confirm: literature review and claim verification are the dominant failure modes, and **none of these systems use a typed, contradiction-preserving epistemic store** — most rely on LLM context, RAG over papers, or scratchpad memory. The 2025 "Memory in the Age of AI Agents" survey (arXiv:2512.13564) discusses memory but not principled belief logics.

**Hari's relationship.** This is Hari's strongest positioning. The *target consumer* (autoresearch loops like IX) has a documented gap that matches what Hari claims to provide: a typed claim store with contradiction detection, evidence provenance, consensus across agent votes, and a small action vocabulary (`Investigate / Retry / Accept / Escalate / Wait`). Whether the substrate needs to be hexavalent-plus-Lie-algebra rather than, say, Subjective Logic, is the open empirical question.

**Novelty verdict.** *Novel as positioning, partial overlap on substrate.* The *role* — epistemic substrate for an autoresearch loop — is currently underserved. The *implementation* could be done with off-the-shelf Subjective Logic and look very similar from the outside.

**Key references.**
- Lu et al., "The AI Scientist" (Sakana AI, 2024) and v2 (2025)
- Romera-Paredes et al., "Mathematical discoveries from program search with large language models" (FunSearch, *Nature* 2024)
- Beel et al., "Evaluating Sakana's AI Scientist" (arXiv:2502.14297, 2025)
- "From AI for Science to Agentic Science: A Survey" (arXiv:2508.14111, 2025)
- "Agentic AI for Scientific Discovery: A Survey" (arXiv:2503.08979, 2025)
- "Memory in the Age of AI Agents" (arXiv:2512.13564, 2025)

---

## 3. Where Hari is most novel

Two specific claims worth defending. Caveat: novelty is not utility — these are the parts least covered by prior art, but they still need to beat baselines.

1. **Hexavalent + Lie-algebra + swarm-consensus, *as an integrated substrate for autoresearch claim tracking*.** Each piece has prior art separately; the specific composition for IX-style loops is, to my reading, unpublished. The autoresearch-substrate framing is the load-bearing novelty, not any single layer.
2. **`Contradictory` as an off-chain absorbing fixed point.** LETK+ keeps a uniform 6-element lattice. Hari's choice — that contradictions sit outside the truth chain and resist ordinary join/meet collapse — is an engineering-meaningful departure from the published six-valued logics, and it operationalizes the "preserve contradictions" philosophy more aggressively than the academic versions.

Anything else (the 6 values themselves, role-based agents, matrix-exponential evolution, perceive/think/act loop, minority-preserving consensus) has substantial prior art.

---

## 4. Where Hari is at risk of reinventing

Stated bluntly, since the project owner asked for it.

1. **The hexavalent lattice itself is independently published (Coniglio & Rodrigues, 2022/2023).** Hari's introduction reads "extends Demerzel's tetravalent logic" and describes the 6 values as if they are a fresh design choice. They are not. The Probable/Doubtful axis is exactly the "reliable evidence" axis from LETK+/LETF+. The project should cite this work and reframe Hari's contribution as an *engineering specialization* (off-chain `C`, fixed action recommendations, integration with swarm) rather than a novel logic.
2. **Subjective Logic (Jøsang) does most of what the swarm + lattice combination is trying to do, with 25 years of formal results.** Opinions `(b, d, u, base_rate)`, transitivity for trust chains, fusion operators for combining sources, and explicit Beta/Dirichlet PDF semantics. If the project owner has not seriously evaluated SL as a baseline, they probably should — it could be a more defensible substrate or a strong baseline to beat. The fact that the roadmap's Phase 3 baseline list is "flat confidence / lattice-only / lattice+swarm" but does not include "Subjective Logic" is a notable gap.
3. **Quantum Cognition (Busemeyer & Bruza, 2012) is the closest direct competitor for the Lie-algebra-on-belief idea**, and it is mathematically more developed (proper Hilbert space, projection-valued measures, experimental validation on order effects). Hari's Lie-algebra layer needs to either (a) cite QC and explain how it differs, or (b) be reframed. Right now `hari-cognition` reads as "physics intuitions imported into AI" — exactly what QC has been doing for 20 years.
4. **Role-based debate agents (`explorer/critic/integrator/guardian`) with minority-preserving consensus are the standard 2025 multi-agent-LLM playbook.** "From Debate to Deliberation" (arXiv:2603.11781) and the broader deliberative-collective-intelligence literature describe nearly-identical mechanisms with typed epistemic acts and minority-report packets. The role taxonomy has marketing value but no formal novelty.
5. **The `Perceive → Think → Act` loop is a 50-year-old cognitive-architecture pattern.** Acceptable as scaffolding, but should not be presented as a contribution. SOAR/ACT-R/CLARION/LIDA/OpenCog all do this with more cognitive depth.

The honest summary: **Hari's most defensible novelty is its niche (epistemic substrate for autoresearch loops), not its mathematical machinery.** If the substrate were Subjective Logic instead of hexavalent-plus-Lie-algebra, the autoresearch-substrate value proposition would be approximately the same, possibly stronger. Phase 3's baseline-vs-experiment evaluation needs to address this directly, or risk reviewers asking "why not just SL?"

---

## 5. Recommended next reads

Ranked by ROI for the project owner.

1. **Coniglio & Rodrigues, "On six-valued logics of evidence and truth..." (arXiv:2209.12337, 2022)** — the prior published 6-valued logic. Read first; you need to know what they did and how Hari differs.
2. **Jøsang, *Subjective Logic: A Formalism for Reasoning Under Uncertainty* (Springer, 2016)** — chapters 1–4 (opinions, fusion, trust networks). The most likely "should we just use this?" question.
3. **Busemeyer & Bruza, *Quantum Models of Cognition and Decision* (Cambridge, 2012)** — the strongest existing analog of Hari's Lie-algebra-on-cognition framing. Read to either validate or rescope `hari-cognition`.
4. **Beel et al., "Evaluating Sakana's AI Scientist" (arXiv:2502.14297, 2025)** — direct evidence that current AI-scientist systems have weak claim tracking. Best ammunition for the IX-substrate positioning.
5. **"From AI for Science to Agentic Science" survey (arXiv:2508.14111, 2025)** — comprehensive landscape map of autoresearch systems; identifies where Hari's wedge fits.
6. **"From Debate to Deliberation" (arXiv:2603.11781, 2026)** — current state-of-the-art for typed multi-agent epistemic acts and minority-preserving consensus. Calibrates how Hari's swarm compares.
7. **Bernardo, Vasca, Iervolino, "Bounded confidence opinion dynamics: A survey" (*Automatica* 2023)** — modern opinion-dynamics landscape; useful for the consensus-stability metric in Phase 3.
8. **Goertzel et al., *Probabilistic Logic Networks* (Springer, 2008)** — the closest spiritual cousin. Contrast PLN's second-order distributions with Hari's discrete lattice.
9. **Bronstein et al., *Geometric Deep Learning* (2021)** + **Gerken et al. "Geometric Deep Learning and Equivariant Neural Networks" (*AI Review* 2023)** — to understand what Lie-group methods *currently* do in ML and where Hari does not overlap.
10. **Dempster–Shafer modern fusion review** — e.g., "Conflict Data Fusion in a Multi-Agent System" (PMC8308205) — for the conflict-detection metrics in Phase 3.

---

## 6. Open questions for the project owner

1. **Has Subjective Logic been considered as a baseline, or as the substrate itself?** The roadmap's Phase 3 baselines do not include SL. Adding it would make the experimental evaluation considerably more credible, and might force a real conversation about whether the hexavalent lattice is doing work SL doesn't already do.
2. **What is the experimental claim that distinguishes the Lie-algebra path from a learned-RNN or neural-ODE path on the same state vector?** The roadmap says "beat a simpler baseline." Is the baseline "no state evolution" or "non-Lie state evolution (e.g., a learned linear layer)"? The latter is harder and more honest.
3. **Is the off-chain `Contradictory` value load-bearing in any concrete IX scenario, or is it currently aesthetic?** A worked example where the off-chain treatment changes a recommendation (vs. LETK+'s in-lattice treatment) would be the cleanest defense of the design choice.
4. **What is the relationship between `hari-cognition`'s state vector and the belief network's propositions?** CLAUDE.md says the integration is WIP. Without a concrete mapping, Lie-algebra evolution is operating on a vector with no semantic grounding, which makes the "useful patterns in belief revision" claim hard to test.
5. **For the IX integration, who else is in the running?** If IX could plug in Subjective Logic, Dempster–Shafer, or PLN as the substrate, what's the case for Hari? Cost, latency, expressiveness, action vocabulary, audit trails — the wedge needs to be concrete.

---

*End of survey.*
