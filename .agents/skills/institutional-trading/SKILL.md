---
name: institutional-trading
description: Review quantitative strategy design, venue-aware risk math, market-data provenance, and backtest evidence. Use for financial meaning and performance validation; ordinary chart styling does not require this skill.
---

# Institutional trading research

Read [AGENTS.md](../../../AGENTS.md) and the applicable [UI/product contract](../../../docs/uiux-driven-development.md). For provider work, read [exchange data providers](../../../docs/exchange-data-providers.md). These sources own the product and approval boundaries. This skill supports research and informational signals, not execution or paper trading.

## Establish the question and owner

Identify app, product/category, registered instrument, timeframe, strategy version, source, and evaluation window. Resolve eligibility, fees, and strategy parameters through existing owning APIs rather than another allowlist or copied thresholds. Tokocrypto tokenized stocks retain their category and exact storage identity. Proxy candles cannot establish exchange depth, executable liquidity, or venue-native execution evidence.

Inspect relevant owners in `quant/core/src/strategies/`, `venue_rules.rs`, `risk/`, and `evidence.rs`, plus their backend consumers. Keep algorithms, provider transport, registry identity, and evidence ownership separate. Retained plan history does not imply an active plan lifecycle.

## Validate financial meaning

- Distinguish raw price change, direction-adjusted return, net return after costs, margin ROI, funding, and liquidation estimates. Unknown notional, margin, spread, or venue inputs remain unknown.
- Use completed candles and chronological inputs. Historical execution models disclose fill timing, fees, funding, slippage, timeouts, and ambiguous intrabar ordering. If stop and target cross in one bar, use conservative stop precedence unless finer evidence proves the sequence.
- Evaluate loss, exposure, correlation, and adverse execution assumptions before optimizing return. Research calculations do not create account, position, order, or trade-history state in the dashboard.
- Preserve strategy version, source, sample, and validation identity. Equal-looking values from different policies or providers are not interchangeable evidence.

## Review bounded Pine strategy backtests

- Keep this capability a historical research calculation. Its transient equity curve and closed-trade rows do not represent an account, position, order, execution evidence, or paper-trading state; do not persist them as such or infer authorization for order placement.
- Confirm the backtest uses the registered instrument identity and bounded SQLite candle history, and reports actual sample bounds, candle source/freshness, truncation, and any fee or fill-model limitations. Unknown costs stay explicitly unknown; Pine-declared commissions replace the venue fee estimate only when the report states that basis.
- Treat the selectable chronological training/validation tail as a diagnostic, not an untouched holdout. Pine calculations may run over the full sequence for warm-up, but validation must suppress pre-split broker side effects and restart from flat with initial capital. Exclude training trades that cross the boundary from training closed-trade metrics.
- Read win-rate intervals as descriptive uncertainty only: Wilson intervals assume independent closed trades, while serial dependence can make them too narrow. Monthly marked returns and train/validation comparisons are descriptive, not predictive evidence. Show sample counts and omit statistics when their inputs are insufficient.
- Inspect the actual fill approximation: chart-OHLC inferred paths, Bar Magnifier coverage/fallback, order processing, slippage, commissions, and unsupported semantics can materially change results. Do not call runtime behavior TradingView-equivalent without differential evidence on fixed histories.
- Keep jobs, result retention, history, series memory, strings, request caches, and returned rows/curve bounded. The normal UI should expose progress, cancellation, expiration, errors, provenance, limitations, and a recovery action; direct API access alone is not a complete product workflow.
- Reproduce a finding with the same registered source/version, declared input overrides, instrument, timeframe, source identity, date range, and sample bounds. Reconsider conclusions when fresh/native history or fixed-history differential checks materially change the result.

## Validate evidence

Use chronological training, validation, and untouched holdout windows. Predictions precede outcomes; adaptive updates follow resolved outcomes. Do not repeatedly tune against the holdout. Preserve reproducible parameters/model versions, timestamps, and evaluation windows in authorized SQLite evidence storage when runtime persistence is needed.

Report sample count, win rate with uncertainty such as a Wilson interval, net return, profit factor, drawdown, exposure, and timeout treatment where applicable. Identify missing costs and insufficient samples. CAGR requires matching capital and duration evidence; never derive it from win rate. Ranking scores are neither probability nor validated return. Inspect their actual components rather than assuming a fixed formula.

Record the required ten-order reasoning before material financial changes. Keep chart/evidence reads bounded and measure resource claims against the 16 MB steady-state target. Report verified evidence, limitations, and invalidation signals. Verify external financial facts and current venue rules against primary sources rather than presenting remembered rules as current facts.
