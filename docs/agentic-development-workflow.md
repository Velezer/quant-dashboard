# Agentic development workflow

Use this workflow for Codex work in this repository. It coordinates code and
research tasks; it does not create an automated trading, portfolio, or order
execution workflow.

## Roles

- **Coordinator:** owns the intent, chooses the relevant playbooks, integrates
  work, and reports only verified outcomes.
- **Explorer (optional, read-only):** traces the current behavior and returns
  relevant files, contracts, risks, and acceptance checks.
- **Implementer (optional):** owns one named vertical slice and its files.
- **Reviewer (optional):** checks the resulting diff against the project
  contracts and reports findings without taking over implementation.

For a small change, the coordinator performs the needed roles in sequence.

## Operating loop

1. **Frame the outcome.** Apply
   [intent-driven development](../.agents/skills/intent-driven-development/SKILL.md).
   Identify the user outcome, affected surface, observable acceptance checks,
   project constraints, and any assumption that materially shapes the change.
2. **Route by domain.** Read only the relevant specialist playbooks:

   | Change area | Playbook and required focus |
   | --- | --- |
   | UI or chart | [UI/UX contract](uiux-driven-development.md); use [ux-reviewer](../.agents/skills/ux-reviewer/SKILL.md) for hierarchy and financial meaning and [ui-tester](../.agents/skills/ui-tester/SKILL.md) for browser behavior and accessibility |
   | Signals, risk, strategies, or evidence | [institutional-trading](../.agents/skills/institutional-trading/SKILL.md); preserve venue rules, source and validation labels, chronological evaluation, and explainable risk |
   | Python-to-Rust or service ownership | [python-to-rust-migration](../.agents/skills/python-to-rust-migration/SKILL.md) and its `references/project-boundaries.md`; migrate one path at a time and establish parity before cutover |
   | SQLite, API payloads, market history, or instruments | [root AGENTS.md](../AGENTS.md) storage invariant and [market storage contract](market-data-storage.md); use migratable SQLite schemas, JSON messages, bounded chronological reads, and labeled sources |
   | Other code | Current source map in [architecture](../ARCHITECTURE.md), then the smallest owning module |

   Existing skills are the specialist checklists. Do not fork their guidance
   into another copy or apply a specialist review to unrelated work.
3. **Trace before editing.** Follow the changed path across browser, HTTP,
   service, quant core, and SQLite as applicable. Check current working-tree
   changes first, preserve them, and identify generated or durable data that
   must remain untouched.
4. **Choose one vertical slice.** Name the owning files, acceptance checks,
   failure/recovery behavior, and rollback signal. Prefer one end-to-end path
   over a broad refactor. Keep API compatibility unless the request explicitly
   changes it.
5. **Delegate only bounded work.** Keep one coordinator responsible for the
   intent, integration, and final report. Use a specialist agent only when
   delegation is authorized in the current task and the work is independent.
   Assign explicit file or review ownership; avoid concurrent edits to the same
   code. Treat delegated findings as review input and verify them against the
   repository before integrating.
6. **Implement within project boundaries.** Keep SQLite as the canonical store
   for durable runtime state, JSON for structured messages, and browser storage
   limited to disposable presentation preferences. Preserve bounded reads and the 16 MB steady-state memory
   target. Put new or changed test code in separate test files, never inline in
   production source files; extract existing inline tests when changing them.
   Provider sync remains disabled until a compatible adapter exists; keep
   existing proxy-origin rows labeled.
7. **Verify the changed path.** Use the applicable checks in
   [harness engineering](harness-engineering.md). For interface changes, use a
   fresh browser session and cover selected, loading, stale/empty, error, and
   recovery states. For quantitative changes, retain reproducible inputs,
   venue rules, sample and validation status; never promote a research result
   or infer CAGR from another metric. Do not place live orders or trigger an
   unrelated scan as a check.
8. **Review and report.** Inspect the final diff against the acceptance checks
   and project invariants. Report changed files, checks actually performed,
   evidence and data-source limits, and any unresolved assumption. Do not call
   the task complete when the observable acceptance condition is still unmet.

## Always-preserved product boundaries

- The selected app exchange determines eligible instruments. Tokenized stocks
  remain a distinct category under Tokocrypto.
- Research plans remain read-only and visibly separate from execution. No
  paper-trading or order-placement flow belongs in the dashboard.
- Win rate, CAGR, profit factor, sample size, and validation status remain
  separate claims tied to matching evidence.
- Missing or stale history stays visible; do not fabricate candles or hide
  recovery states.
- Do not add JSON/XML payloads, CSV application data, or file-backed runtime
  state. Make durable SQLite schema changes migratable.
