---
name: ux-reviewer
description: Review financial dashboard usability, hierarchy, accessibility, chart ergonomics, and evidence clarity. Use for UI/UX audits and design recommendations; use ui-tester for behavioral verification and tradingview-ui for chart implementation.
---

# Financial dashboard UX review

Read [AGENTS.md](../../../AGENTS.md) and [UI/UX-driven development](../../../docs/uiux-driven-development.md). Review the informational screener against its primary task: find ranked signals and decide which merits inspection. Do not evaluate it as an execution terminal or recommend account/order features.

## Gather evidence

Inspect the requested screen in a fresh browser session when available. Record task, decision, viewport, data state, and success condition. Use fresh DOM/accessibility state for labels, screenshots for hierarchy/chart rendering, and console evidence for failures. Screenshot-only reviews cannot establish keyboard behavior, recovery, or performance; state those limits. For interface edits, write the UX contract before implementation and complete project verification. Review authorization does not expand product scope.

## Evaluate the relevant dimensions

- **Hierarchy:** Keep app/product/category, signal timeframe, direction, score basis, price, and freshness near the selected signal/chart. Disclose deeper diagnostics unless they affect immediate interpretation.
- **Identity/evidence:** Keep Tokocrypto tokenized stocks categorized and signal/chart timeframes distinguishable. Show source, quality, sample, and validation separately. Missing evidence is not agreement; score is not probability or validated return.
- **State/recovery:** Distinguish loading, stale, empty filter, no signal, failed fetch, and failed render. Offer the appropriate retry and preserve last-good context with stale status. Selection stays coherent during async updates.
- **Chart ergonomics:** Candles, wicks, OHLC, axes, crosshair, zoom, pan, and fit remain usable at relevant sizes. Review existing gestures rather than mandate unsupported momentum/axis behavior. Studies must not hide price action; no trade-level/position overlays.
- **Financial numbers/color:** Use existing tokens, owning formatters, and instrument precision. Avoid fixed crypto decimals that erase small prices. Tabular numerals prevent jitter without requiring a specific font. Unknown differs from zero; raw price change differs from margin ROI. Preserve established financial colors with non-color cues.
- **Accessibility/feedback:** Check names, keyboard paths, focus, dialog dismissal, contrast, and clipping. Use applicable accessibility standards and actual measurements rather than asserting contrast from color names. Acknowledge refresh promptly without implying success before completion.
- **Product boundary:** No execution or simulated ledger flow. The approved assumption-based R:R illustration explains assumptions/uncertainty without account, position, order, or trade-history state.

## Deliver findings

Prioritize observed defects by user impact. Give affected control/state, evidence/reproduction, consequence for the decision, smallest correction, and acceptance check. Distinguish defects, hypotheses, and preferences; avoid unmeasured timing claims and default perfect ratings.

For implemented corrections, reuse existing owners/tokens, record proportionate ten-order reasoning, and verify in a fresh session. Use ui-tester for browser verification when needed. Report verified improvements and limits without turning an audit into an architectural rewrite.
