# Per-ticker entry model validation

Open **Backtest entry models per ticker** from the scoring guide, or choose
**Validate entry models** on the screener. The `/entry-validation` page runs the
current four-indicator NWE-led and DSS-led entry models for the selected app,
product, ticker, and timeframe. Choose one ticker or run every eligible ticker
in the selected scope. Scope runs follow every registry page, process tickers
sequentially, save each completed result, and offer stop-after-current and
retry-failed controls.

## Reading a report

Each saved report identifies its registered instrument, model and score version,
timeframe, selected candle source, sample dates, candle fingerprint, and
protocol version. It separates the training, exit-selection, and untouched
holdout windows. The four total-score bands and each indicator's points out of
10 show the observed holdout trades, win-rate interval, returns, wins/losses,
and profit factor. Scores rank signals; they are not win probabilities.

The protocol replays the live lead signal and shared score calculation using
only candles available at that signal. A qualifying signal enters at the next
bar open and exits at the close after one of the fixed 4, 8, or 12-bar horizons.
The horizon is selected by mean return on validation data; the final chronological
20% holdout does not select it. Trades do not overlap or cross a split boundary.
This gives a consistent per-ticker comparison, but manually changing the model,
score floor, or rules after inspecting the holdout makes that holdout no longer
independent evidence.

**Backtest complete** means the evaluator produced a report. **Validated
holdout evidence** has additional minimum sample and calendar-span, positive
holdout return and profit factor, separated score-band evidence, verified
registered identity, venue-native candles, known product fees, and complete
execution-cost checks. The indicator points and their weights remain descriptive;
they do not stand in for these checks.

Research-proxy candles cannot qualify as venue-native evidence. Unknown fees are
shown as unknown and returns remain gross. The current report does not model
slippage, order-book depth, market impact, or applicable funding and borrow
costs, so sizing eligibility is always blocked by this version of the report.
Historical evidence can inform a later sizing review only after these gaps and
the separate sizing rule and risk limits have been addressed.

An unsupported app/product/category has no substitute strategy. Missing,
forming, malformed, stale, or insufficient candles stay visible as such.
Analysis reads at most 10,000 of the latest matching completed candles per run;
this is an analysis bound, not a storage or candle-age limit. Check the sample
dates and read-limit notice before interpreting a report. A saved report is an
aggregate snapshot, not live evidence; rerun it after history or protocol changes.
Only aggregate summaries and a change-detection fingerprint are saved in app
SQLite. Individual simulated trades and account or equity history are not saved.

For missing history, use **Sync Market** on the screener and rerun the ticker.
For pending app verification, resolve listing readiness first. A failed request
can be retried from the page; completed ticker snapshots remain available after
reload. An old protocol snapshot stays visible but must be rerun before current
comparison.
