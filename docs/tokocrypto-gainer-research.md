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

## Current 4h forecast

The forecast applies the fixed equal-weight six-factor combined entry score to
the latest completed 4h UTC window for verified Tokocrypto Spot pairs. It reads
up to 1,000 consecutive 4h candles from native hourly data. Every bucket
requires four consecutive hours, and a gap resets the available history. The
top five positive candidates are ordered by signed score. A pair whose latest
completed bar is older than the shared cutoff is excluded.

1. A user opens the dedicated `/forecast` page from the screener. The screener
   link requests the latest forecast on arrival; manual refresh remains on page.
2. Each result expires at the end of the interval it forecasts.
3. Users can recheck after history sync. A single request at a time bounds
   competing research work.
4. Forecasts reuse the causal score and keep the exact Spot pair and source.
5. Results are computed on demand and remain transient.
6. Candidates are withheld when fewer than 10 pairs or 80% of the eligible
   universe can be scored. A covered universe can have no positive candidate.
7. The page reports the cutoff, coverage, excluded inputs and retry path.
8. The ranking carries an unvalidated research label. The historical study
   and its fee scenarios provide separate evidence.
9. Hypothesis: the ranking may help find short-term winners. Untouched
   after-cost results are needed to establish that it improves on momentum.
10. Check exact timestamps, incomplete buckets, deterministic ranking and
    coverage in tests. Revisit the read limit if observations show the bounded
    prefix changes ranks materially. Historical replay can use longer prefixes
    and proxy data, so its results do not establish this forecast's accuracy.
