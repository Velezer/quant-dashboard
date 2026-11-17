# Tokocrypto next-gainer research acceptance contract

The primary task is to rank Tokocrypto Spot instruments for the next completed
4h interval and determine whether that ranking has predictive value after costs.
Tokenized stocks remain a separate product universe.

## History and coverage

Reuse the existing per-symbol SQLite stores and bounded ingestion worker.
Expose listing age, earliest/latest completed candle, contiguous bar count,
source, coverage days, provider exhaustion, latest attempt and observed error.
Continue historical pages independently of recent-price refreshes. A newly listed
instrument remains insufficient-history; never invent pre-listing candles or
relax the existing 1,000-day validation minimum. Provide retry/sync in the UI.

## Costs

Keep verified venue fees in the existing fee-policy owner, with official source,
effective date and exact app/product/quote scope. Unknown fees remain unknown.
Research inputs for spread and slippage must be explicit, validated, labeled
assumptions and included on both entry and exit. Display gross and configured
cost-adjusted results together; missing costs cannot produce an after-cost pass.

## Ranking protocol

Freeze a versioned protocol before evaluating results. Use a 4h horizon by
default, causal observations through the completed signal bar and next-open
entry. Deduplicate NWE/DSS rows by exact instrument; shared factors are not
independent votes. Use point-in-time listing eligibility and disclose historical
universe coverage and survivorship bias when delisted instruments are absent.

Evaluate synchronized cross-sectional cohorts with a chronological 60/20/20
split. Fit on training, choose ranking settings on validation, and evaluate the
frozen rule once on holdout. Purge observations whose forward outcome crosses a
split boundary. Never fill gaps or align prices by array index. Require complete
same-source entry and horizon-exit observations. Exclude stale, forming,
zero-volume and unsupported instruments with explicit exclusion counts.

Report top-1 and top-5 mean net return, positive-return frequency, top-decile
gainer precision, rank correlation and sample counts. Compare the same dates
and eligible universe against equal-weight universe returns and a causal
previous-4h momentum ranking. Include paired return differences and uncertainty;
win rate alone does not establish profitable or superior ranking. Repeated
holdout inspection must not drive tuning without a newly reserved sample.

Process sequentially with bounded memory and reuse existing research modules,
controls and SQLite aggregate storage. Show run, progress, stop after current,
retry, history recovery, source, costs, protocol and unavailable states in the
research UI. Persist aggregate evidence only, without account or order state.

## Screener presentation

Show signed setup score, timeframe, source, forming/completed state, price age
and validation availability beside each candidate. Scores never imply a win
probability. A next-gainer label requires passing ranking evidence; otherwise
label the result an unvalidated research ranking. Keep proxy research results
explicit and distinct from venue-native execution evidence.

## Verification

Check causal timestamp alignment, split-boundary purging, duplicate models,
missing/short history, listing changes, fees, source mismatches and baselines.
Verify a fresh browser flow from Tokocrypto screener through research, run,
completion/unavailable state, retry and source/cost review. Complete both release
builds and the required GitHub workflow before reporting a code PR verified.
