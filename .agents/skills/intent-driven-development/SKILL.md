---
name: intent-driven-development
description: Turn underspecified feature requests, bug reports, and refactors into the smallest completed change with explicit intent, constraints, and acceptance evidence. Use when the user describes a problem rather than a solution.
---

# Intent-driven development

Infer the outcome from the prompt, existing behavior, and established decisions. Read [AGENTS.md](../../../AGENTS.md); for interface work read [UI/UX-driven development](../../../docs/uiux-driven-development.md). These sources own current invariants; do not maintain a parallel product specification here.

## Form actionable intent

Identify internally the user's primary task and decision, affected surface, smallest improvement, constraints, acceptance/recovery signals, assumptions, and material uncertainty. Resolve low-risk ambiguity from existing conventions and proceed. Ask only when missing information affects correctness or authorization; continue independent work while waiting. Do not turn routine implementation choices into an approval flow.

A conflicting invariant requires explicit approval to change that rule; approval for a feature alone is insufficient. Architectural expansion requires approval before implementation, including a representative slice that crosses the boundary. Prepare the reason, affected boundaries, and migration impact while continuing work within approved boundaries.

## Implement and verify

1. Inspect owners, callers, contracts, and existing checks. Confirm that legacy behavior is permitted before preserving it.
2. Define observable acceptance and recovery. For UI work, specify loading, stale, empty, error, recovery, accessibility, and progressive disclosure before editing.
3. Record first- through tenth-order material effects before changing product behavior, architecture, persistence, financial meaning, reliability, or safety. Treat distant effects as hypotheses and keep reversible edits brief.
4. Implement the smallest coherent slice using owning APIs and existing helpers. Retire prohibited legacy paths rather than preserve aliases/fallbacks; migrate or discard data according to the storage contract.
5. Run checks appropriate to the change. New tests belong in dedicated files; move affected inline Rust tests into sibling test files. Respect the source line limit.
6. Verify changed interface flows in a fresh browser session with documented APIs, accessibility state, rendered output, selected context, applicable failure/recovery states, and console health. Use ui-tester when its workflow is needed.

Do not add persisted files, browser domain state, services, or cross-layer dependencies for implementation convenience. The R:R illustration is a narrowly approved assumption calculator, not permission for a simulated ledger.

Lead the final response with the outcome, meaningful checks, material assumptions, and remaining limitations. An edited file alone does not prove a flow works. Report unavailable verification explicitly.
