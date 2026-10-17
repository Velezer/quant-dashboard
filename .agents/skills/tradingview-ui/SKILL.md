---
name: tradingview-ui
description: Build or refine financial chart interactions, including zoom, pan, axes, crosshairs, and indicator rendering. Use for chart behavior and layout; ordinary dashboard changes do not require this skill.
---

# Financial chart interactions

Respect the chosen chart library and existing architecture. TradingView is an interaction reference, not an implied official integration. Read [AGENTS.md](../../../AGENTS.md) and [UI/UX-driven development](../../../docs/uiux-driven-development.md). The screener chart displays stored candles and permitted studies without entry, stop, target, or position overlays.

## Find the owner and outcome

Inspect `frontend/dashboard/templates/dashboard.html`, relevant styles, and `frontend/dashboard/scripts/modules/chart-data.js`, `chart-math.js`, `chart-renderer.js`, `chart-interactions.js`, and `chart-inspection.js`. Trace imports before editing. Use the existing chart route/storage interface; do not create generated dashboard copies or duplicate candle caches.

Define the user's chart task and acceptance signal. Keep app, symbol, source, freshness, OHLC, chart timeframe, and signal-generation timeframe understandable. Resolve app/product eligibility through the registry, not a hard-coded app list.

## Preserve coherence

- Use one viewport model for drawing and pointer math. Anchor continuous wheel zoom to the cursor candle, support fractional pan, and batch rapid input with animation frames.
- Resize the drawing buffer only when dimensions/device pixel ratio change. Refresh pointer bounds after symbol, timeframe, viewport, or layout changes.
- Use the existing stable signal identity, including app/product, symbol, strategy, and timeframe as required by the contract. Apply async responses only to the current request/selection. Do not coerce tokenized-stock symbols into IDX symbols.
- Preserve viewport on refresh of the same selection; reset intentionally on changed identity or fit/reset. Avoid detached canvas references.
- Keep candle/study values and metadata consistent with typed backend responses. Indicator defaults belong to the core owner, not another frontend definition.

## Render, recover, and verify

Expose loading, stale, empty, failed-request, and failed-render states. Preserve usable last-good context with a stale label when appropriate. Distinguish absent bars from renderer failure and keep retry/later selection working. Bound history requests and disclose diagnostics progressively.

Provide accessible keyboard zoom, pan, selection, and fit controls. Keep candles, wicks, axes, and crosshair readouts legible at compact/desktop sizes using established layout dimensions. Color needs a non-color alternative.

Verify in a fresh browser session: candles/wicks, OHLC/app context, cursor zoom, pointer/keyboard pan, fit/reset, timeframe changes, rapid switching, refresh during loading, responsive rendering, recovery, and console health as relevant. A populated response with a blank canvas fails acceptance. Separate interaction evidence from measured latency/frame rate.
