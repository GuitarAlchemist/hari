---
review_agents: [code-simplicity-reviewer, security-sentinel, performance-oracle, architecture-strategist]
plan_review_agents: [code-simplicity-reviewer]
---

# Review Context

Add project-specific review instructions here.
These notes are passed to all review agents during /workflows:review and /workflows:work.

- Rust workspace, 4 crates with a strict dependency hierarchy (lattice → cognition → swarm → core); flag any edit that makes hari-lattice or hari-cognition depend on higher crates.
- Defaults are owner-pinned: `PriorityModel::RecencyDecay` and Lie tunables are pinned by tests (see docs/adr/0001) — any change to defaults or tunables must be called out as a project-direction change, not a refactor.
- A/B doctrine: new behaviors must be comparable against a simpler baseline in the same run; reviews should ask "where's the baseline?" for new decision logic.
- `HexValue::Contradictory` must never be collapsed into the F<D<U<P<T chain in join/meet — irreconcilable evidence is preserved by design.
- Verification bar: cargo fmt --check, clippy -D warnings, cargo test --all (CI enforces all three).
