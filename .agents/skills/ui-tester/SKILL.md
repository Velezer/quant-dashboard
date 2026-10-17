---
name: ui-tester
description: Verify rendered web flows, reproduce interface bugs, and check accessibility, responsive behavior, and failure recovery through browser interaction. Use for UI verification, not unrelated implementation.
---

# UI tester

Read [AGENTS.md](../../../AGENTS.md) and [UI/UX-driven development](../../../docs/uiux-driven-development.md). Derive checks from changed behavior and the primary user task. Use documented browser APIs; inspect fresh state before choosing controls and after meaningful actions.

## Establish the surface

Confirm served workspace, URL, startup command, and whether assets are disk-served or embedded. Inspect `tooling/start.bat` before starting/restarting. Reuse a suitable service and never stop an unrelated process. Do not assume retired Python previews exist. Inspect existing fixture facilities when needed and label synthetic data.

Open a fresh browser session for the changed flow, not just a hard reload. If tools cannot isolate session state, report the limitation. Domain state comes from the backend; browser storage holds only disposable presentation preferences. Do not alter SQLite history or seed data incidentally to make a check pass.

## Verify the decision and recovery

Check app/product/category filters, selected state, Tokocrypto tokenized-stock categorization, timeframe, and search where affected. App identity remains authoritative. Distinguish valid empty results from failed fetches/stale scans. Retry preserves last-good context and exposes queued/running/failed/completed status when invoking the existing refresh path.

For selection/chart work, check that selected row, symbol, app, OHLC, and timeframe context update together. Exercise repeated symbols across timeframes, rapid switching, and refresh during loading. Inspect actual storage symbols and responses; never assume all tokenized stocks use IDX `.JK` symbols.

Check actual candles/wicks, studies, crosshair, cursor zoom, drag/keyboard pan, fit/reset, and compact/desktop rendering as relevant. A blank canvas after a populated response is a defect. Distinguish no bars from rendering failure; verify retry and later selection.

For evidence, compare source, sample, quality, validation, and unknown states with responses. Scores rank signals, not probability or guaranteed returns. The screener exposes no trade-plan controls or account, position, order, or trade-history state. The R:R illustration stays limited to assumptions.

## Scope interactions and report evidence

Use DOM/accessibility observations for exact labels and selection, screenshots for canvas/clipping/hierarchy, and console output for runtime failures. Exercise applicable loading/stale/empty/error/recovery states. Prefer existing fixtures or controlled failures over durable data edits.

Classify refresh/sync or other durable/external actions before clicking. Perform them only when needed and authorized by the requested flow. Do not run unrelated scans, migrations, seeding, or destructive actions. Prohibited execution actions never become permitted by exhaustive coverage requests.

For an explicitly exhaustive review, inventory controls including disclosed menus/dialogs. Exercise distinct behavior and representative generated controls; test every instance when requested or when instance-specific state matters. Track stable names/IDs, verified results, and skipped controls with reasons.

Run meaningful existing checks without equating them with browser verification. Report URL/session, exercised flows, reproducible failures, and unverified limits. Claim performance only when measured. Stop only temporary processes/tabs created for the test after capturing evidence.
