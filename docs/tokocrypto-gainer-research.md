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
split. The shipped baseline uses fixed equal factor weights and a fixed ranking rule,
so neither training nor validation fits parameters. Report all three partitions
and evaluate the frozen rule on holdout. Purge observations whose forward outcome crosses a
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

## Implemented method and limits

The research UI uses completed 4h history through a single cutoff, shared by the
entire frozen Tokocrypto Spot registry and all retries. Sample entry timestamps
are a fixed four-day UTC epoch grid; each outcome is next-open to next-4h-open.
This samples across the full available history up to the existing 100,000-bar
read cap, rather than replaying recent prefixes. Recursive indicators reset at
history gaps. The shared causal strategy visitor computes indicators once per
segment; a regression compares sampled scores with live prefixes and verifies
that future candles do not change past scores. Positive candidates must meet
the strategy-owned signed-score eligibility floor.

The browser limits the scope to 1,000 instruments and 1,000,000 observations.
Exceeding either bound produces incomplete/unavailable evidence, never a partial
universe presented as complete. Requests run sequentially. Stop finishes the
current ticker; retry preserves the cutoff, registry and cost scenario. The
coverage disclosure shows gaps, stored dates, registration dates, native history
paging status, attempts and errors. Venue listing dates remain explicitly
unknown when the venue has not supplied them. No historical eligibility is
fabricated from current registration or candle timestamps.

Native hourly ingestion advances one page after committing latest prices and
continues past the 1,000-day qualification minimum until the provider exhausts
history. Exhaustion is rechecked weekly. Markers use existing ingestion metadata
and do not turn a successful latest-price refresh into a historical success.

Published standard quote-specific fee scenarios come from the official
[Tokocrypto fee page](https://support.tokocrypto.com/hc/id/articles/360004044591-Informasi-Biaya-Transaksi-di-Tokocrypto),
updated 18 June 2026 and checked 17 November 2026. The owning venue-policy module
keeps IDR taker and USDT/crypto totals distinct. Published current rates applied
to older returns are a scenario, not a historical tax reconstruction; account
discounts are not assumed. Other quotes and tokenized stocks remain unknown.
Manual fee, quoted spread and per-fill slippage scenarios are available and
labeled unverified. Net and gross top-five returns are both reported.

Paired cohort excess returns are compared with the equal-weight universe and
prior-4h momentum. The report also shows top-one positive-return frequency,
top-five gainer-decile precision and tie-aware Spearman correlation. Approximate
Newey–West mean intervals use four cohort lags and require 30 observations;
these do not remove survivorship bias or establish future prediction accuracy.

Save/Load stores only the latest bounded aggregate report in the existing app
SQLite database. It retains cutoff, source counts, input fingerprint, frozen fee
policies, splits, metrics and limitations. Individual outcomes are transient.
Server validation rejects raw outcome fields and forces saved status to
UNVALIDATED_RESEARCH: browser-computed reports never promote live profiles.

## Decision effects

1. Users can inspect a reproducible ranking experiment and recover missing inputs.
2. Historical ingestion advances independently of latest-price refreshes.
3. Retries reuse the original experiment identity instead of silently changing it.
4. Shared scoring, quote-specific fee ownership and source identity remain intact.
5. Only one aggregate snapshot is retained; observation and request work is bounded.
6. Gaps reset studies; missing fees, incomplete scopes and short samples remain visible.
7. Users can stop, retry, sync history, save and reload through the research page.
8. The existing research and SQLite boundaries remain; no execution workflow is added.
9. Hypothesis: longer history and paired evidence help assess ranking quality; that
   requires adequate untouched data, rather than a successful software build.
10. Reject or revise the protocol if causal tests fail, universe coverage is
    inadequate or resource checks regress. No result changes live eligibility.

## Scheduled next-gainer forecasts

The dedicated `/forecast` page has separate Tokocrypto Spot and Stockbit IDX
profiles. One background worker runs the profiles on their market cadence, and
the page loads the selected saved result automatically. Each profile has an
independent universe, protocol, data source, schedule and outcome history.

Tokocrypto scans after each aligned 4h close. It uses seven contiguous 4h
buckets assembled from exact native 1h candles and projects the previous 24h
close-to-close return into the next 24h. Its top-five positive projections are
withheld unless at least 80% of eligible pairs and at least 10 pairs have fresh,
contiguous history. Tokenized stocks are excluded by exact product/category.

Stockbit scans at 14:00 WIB from active, verified `stockbit_regular` IDX
listings, after the bounded daily history refresh has had time to run and before
the IDX close. The target is the next available session close-to-close return;
it is not labeled as a fixed 24-hour horizon.
The only current price history is `stockbit_regular_yfinance_fallback` from
Yahoo Finance, with `yahoo_unadjusted` prices. It is shown as a research proxy,
not a Stockbit broker price. Returns are raw unadjusted close returns. Current
listing snapshots do not provide a point-in-time historical universe, so
retrospective Stockbit tests would have survivorship bias.

Each scope freezes its eligible instrument snapshot and cutoff for coverage
retries. Results and top-five predictions are saved in the existing app SQLite
database. Once the relevant next candle is available, the worker stores the
realized gross return and the page reports the prospective sample count, mean
return and positive-return rate. Those observations remain descriptive; neither
baseline is promoted to validated status. Fee, spread, slippage and other costs
are not included.

The API selects a profile with `scope=tokocrypto_spot` or `scope=stockbit_idx`.
The page shows profile-specific source, price basis, target, cutoff, coverage,
schedule, recovery status and observed outcomes. It refreshes automatically;
scans do not require a page action. Verify exact source matching, frozen retry
universes, four-hour candle contiguity, daily-session target settlement,
weekend scheduling, stale and insufficient-coverage states, and separation of
each scope's forecast and outcome history.
