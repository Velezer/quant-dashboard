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
failure. The live progress bar refreshes every 15 seconds and shows processed
pairs and failed attempts. A waiting state means the worker yielded to a signal
scan or manual backtest; it resumes automatically. SQLite busy/locked errors
are retried in place before a pair is recorded as failed. A protocol change
restarts the cycle from the first pair so a profile evaluation never mixes
reports from different backtest rules. If a cycle takes more than 24 hours, the
next one starts as soon as the current one finishes.

This refresh updates the saved research reports and their training-selected
candidate weights. It uses stored matching candles and never fetches provider
data. After a complete cycle, the service groups reports by model, asset
category, timeframe, and candle source family. It can promote one exact vector
only when it is shared by at least five instruments and covers at least 80% of
the refreshed cohort and each app/product slice.

Live profile promotion requires known transaction fees, at least 730 days of
history, positive net expectancy and profit factor above 1, and both validation
and untouched holdout to meet the 50% after-fee win-rate target with a 95%
Wilson lower bound at or above 50%. It must also improve mean net return over
equal weights in both samples. If any requirement fails, the current active
profile remains; when no profile exists, scans use `[10, 10, 10, 10]`. A 50%
historical result is a target, not a future win guarantee.

The `/entry-validation` page shows active profile versions and the latest
cohort blockers. Each signal scan loads one profile snapshot at its start and
uses that version throughout the scan. Signal rows retain raw component points
out of 10, each component's weight and weighted contribution, and the profile
version. Newly promoted weights apply on the next scan.

## Reading a report

Each saved report identifies its registered instrument, model and score version,
timeframe, selected candle source, sample dates, candle fingerprint, and
protocol version. It separates the strategy warm-up, training, exit-selection,
and untouched holdout windows. The equal-weight baseline's four total-score
bands and each indicator's points out of 10 show observed holdout trades,
win-rate interval, returns, wins/losses, and profit factor. Scores rank signals;
they are not win probabilities.

The protocol replays the live lead signal and shared four component scores using
only candles available at that signal. The score floor remains 1 point out of
40. A qualifying signal enters at the next bar open. The nominal 60/20/20
chronological splits apply to bars after the model's indicator warm-up; no
position crosses a split boundary.

Weight learning tests 165 predeclared nonnegative weight vectors in steps of 5
that sum to 40. Missing factor values contribute zero and their weight is not
redistributed. Training candidates need at least 30 non-overlapping outcomes
and 50% winners after configured transaction fees using a fixed 8-bar timeout.
Among those candidates, the fitter maximizes the Wilson 95% lower bound for
after-fee win rate, then mean net return, then proximity to equal weights.
Unknown fees disable weight fitting; if no candidate reaches 50%, no candidate
vector is produced.

For the fixed equal-weight baseline and the training-selected candidate,
validation compares fixed 4, 8, and 12-bar timeouts with an exit on the first
opposite-direction signal that qualifies under that model's weights and score
floor. The opposite signal closes the position and does not open a reverse one.
Validation selects the exit rule and maximum hold independently for each model.
Each selected policy is evaluated once on the final chronological holdout; the
holdout does not select weights or exits. The separate cohort evaluator
combines only reports whose training-selected vector exactly matches, checks
the vector in validation and holdout, and leaves weights unchanged when the
evidence is short or inconclusive. Inspecting the holdout and then changing
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
execution-cost checks. Live scoring profile promotion is a separate gate; it
subtracts configured transaction fees but does not imply spread, slippage,
funding, borrow, or market impact are covered.

Research-proxy candles cannot qualify as venue-native evidence. Unknown fees are
shown as unknown and returns remain gross; weight fitting is disabled without
known fees. The current report does not model slippage, order-book depth, market
impact, spread, or applicable funding and borrow costs, so sizing eligibility is
always blocked by this version of the report.
Historical evidence can inform a later sizing review only after these gaps and
the separate sizing rule and risk limits have been addressed.

An unsupported app/product/category has no substitute strategy. Missing,
forming, malformed, stale, or insufficient candles stay visible as such.
Analysis reads at most 100,000 of the latest matching completed candles per run;
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

1. **Direct effect:** Saved per-ticker candidates refresh daily; shared live
   profiles are evaluated once successful jobs cover 80% of the cycle, then
   again at cycle end against the 50% after-fee target.
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
8. **Governance:** A candidate reaches live screener scoring only when its
   after-fee validation and untouched holdout both pass the 50% target, score
   cutoff, sample, and return checks. A failed or incomplete cohort leaves the
   existing active profile unchanged.
9. **Trajectory:** A long or repeatedly failing cycle can leave some reports
   older than a day; progress and completion timestamps expose that condition.
10. **Decision:** Keep the worker sequential and resumable; profile evaluation
    uses successful-job coverage, so one failed ticker cannot block all other
    cohorts from becoming eligible.

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
