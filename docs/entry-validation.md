# Per-ticker entry model validation

Open **Backtest entry models per ticker** from the scoring guide, or choose
**Validate entry models** on the screener. The `/entry-validation` page runs the
current four-indicator NWE-led and DSS-led entry models for the selected app,
product, ticker, and timeframe. Choose one ticker or run every eligible ticker
in the selected scope. Scope runs follow every registry page, process tickers
sequentially, save each completed result, and offer stop-after-current and
retry-failed controls.

## Daily candidate refresh

The service automatically reruns both entry models for every eligible
registered ticker and supported signal timeframe once per 24 hours. The first
cycle starts after the initial registry sync; later cycles are due 24 hours
after the previous cycle starts. It processes one ticker/timeframe pair at a
time and yields between pairs. It checks for signal scans between pairs; a
pair already underway can finish. It shares a lock with manual backtests and
resumes from its registered ticker/timeframe after a restart. Per-pair
failures are recorded and retried in the next daily cycle. The page shows
cycle status, progress, last completion, next due time, and the latest
failure. If a cycle takes more than 24 hours, the next one starts as soon as
the current one finishes.

This refresh updates the saved research reports and their training-selected
candidate weights. It uses stored matching candles and never fetches provider
data. A candidate remains research-only; daily retraining does not change live
screener weights or signal qualification. Reports without sufficient training
trades or known fees do not produce candidate weights.

## Reading a report

Each saved report identifies its registered instrument, model and score version,
timeframe, selected candle source, sample dates, candle fingerprint, and
protocol version. It separates the strategy warm-up, training, exit-selection,
and untouched holdout windows. The equal-weight baseline's four total-score
bands and each indicator's points out of 10 show observed holdout trades,
win-rate interval, returns, wins/losses, and profit factor. Scores rank signals;
they are not win probabilities.

The protocol replays the live lead signal and shared four component scores using
only candles available at that signal. The score floor remains unchanged at 20
out of 40. A qualifying signal enters at the next bar open. The nominal 60/20/20
chronological splits apply to bars after the model's indicator warm-up; no
position crosses a split boundary.

Weight learning tests 165 predeclared nonnegative weight vectors in steps of 5
that sum to 40. Missing factor values contribute zero and their weight is not
redistributed. Training selects the vector with the highest mean return after
known transaction fees using a fixed 8-bar timeout, and requires at least 30
non-overlapping training outcomes for each candidate. Equal weights are the
baseline, and ties favor the vector closest to equal weights. Unknown fees or
too few training outcomes disable weight fitting.

For the fixed equal-weight baseline and the training-selected candidate,
validation compares fixed 4, 8, and 12-bar timeouts with an exit on the first
opposite-direction signal that qualifies under that model's weights and score
floor. The opposite signal closes the position and does not open a reverse one.
Validation selects the exit rule and maximum hold independently for each model.
Each selected policy is evaluated once on the final chronological holdout; the
holdout does not select weights or exits. This evidence remains exploratory and
does not change live screener scores. Inspecting the holdout and then changing
weights, floors, or rules contaminates it as independent evidence.

After weights are fitted, an intraday-only rule searches 576 entry-hour/exit-hour
pairs in validation using that fixed weight vector. Hours use WIB (Asia/Jakarta,
UTC+07:00). Entry time is the next bar's open; exit time is the first bar close
at the selected hour, capped by the longest configured fixed-exit horizon. If
the exit hour does not occur before that cap, the rule exits at the cap. A pair
needs at least 10 validation trades; ties favor more trades, then earlier entry
and exit hours. The selected pair is evaluated once on holdout. Daily and weekly
candles do not support intraday hour selection. One page shows the selected
rule, top validation pairs, weights, data source, and holdout result.

The report also shows lag-1 autocorrelation for the fitted weighted signal-score
series and its fixed-8-bar training trade returns. Signal scores include every
causal lead event, including events below the entry floor; trade returns are
consecutive non-overlapping outcomes. At least 10 lagged pairs and non-constant
values are required to report a coefficient. This describes linear dependence,
not predictive accuracy; trade spacing is irregular and is not treated as an
equal-time series.

**Backtest complete** means the evaluator produced a report. **Validated
holdout evidence** has additional minimum sample and calendar-span, positive
holdout return and profit factor, separated score-band evidence, verified
registered identity, venue-native candles, known product fees, and complete
execution-cost checks. A training-selected weight vector is a candidate, not a
validated replacement for the live equal weights; it does not stand in for
these checks.

Research-proxy candles cannot qualify as venue-native evidence. Unknown fees are
shown as unknown and returns remain gross; weight fitting is disabled without
known fees. The current report does not model slippage, order-book depth, market
impact, spread, or applicable funding and borrow costs, so sizing eligibility is
always blocked by this version of the report.
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

### Daily refresh operating contract

1. **Direct effect:** Saved per-ticker candidate reports and weights are
   refreshed on a daily cycle.
2. **Immediate consequence:** A full pass can contain thousands of pairs, so
   it processes one pair at a time and yields briefly between reports.
3. **Repeated operation:** The next cycle becomes due 24 hours after start;
   an unfinished cycle resumes after restart rather than starting a duplicate.
4. **Interactions:** It shares the manual-backtest lock, checks for signal
   scans between pairs, and reads the same per-symbol stored candle sources. A
   scan that starts during a backtest pair can overlap that one pair.
5. **Accumulation:** Each pass replaces the latest aggregate report per
   ticker/timeframe; it stores no individual simulated trades.
6. **Boundary conditions:** Unsupported, unverified, and excluded instruments
   are skipped. A pair failure is recorded and does not block later pairs.
7. **Recovery:** The page shows cycle progress and the latest failed pair; item
   failures retry in the next daily pass and worker-level failures retry after
   15 minutes.
8. **Governance:** Daily training refreshes research candidates only. It does
   not promote a weight candidate into live screener scoring.
9. **Trajectory:** A long or repeatedly failing cycle can leave some reports
   older than a day; progress and completion timestamps expose that condition.
10. **Decision:** Keep the worker sequential and resumable; revise its cadence
    only if deployed duration and memory measurements support a change.

## Conditional exit cues on the screener

A qualifying SHORT entry signal is also an exit cue for a LONG position; a
qualifying LONG entry signal is an exit cue for a SHORT position. This assumes
the position uses the same app, product, ticker, entry model, and timeframe as
the signal. The cue uses the existing entry signal and its indicator scores; it
does not open a reverse position or place an order.

The dashboard does not know whether a position exists, its entry time, or its
entry model. A cue therefore asks the user to check that match. A forming bar is
an exit watch until it closes. Stale or non-qualifying signals produce no exit
cue. The live cue does not enforce the backtest's selected timeout because that
requires a recorded position and entry bar. A ticker's validation report must be
read separately; an exit cue is not evidence that the policy is validated.
