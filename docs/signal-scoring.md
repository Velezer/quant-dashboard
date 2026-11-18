# Signal scoring rules

The in-app Scoring guide (`/scoring`) explains these rules on one page, with an
illustrative points example and the deployment's current model catalog.

This is a reader reference for the signal scores shown by the dashboard. The
versioned strategy and catalog definitions in the source code own runtime
values; update this explanation when those definitions change. A score ranks a
research setup. It is not win probability, an execution quote, or an order
recommendation. After-fee cohort metrics report historical performance for a
score range, but they do not filter live signals. A promoted profile supplies
weights; if none exists, equal weights are used and the base technical score
floor still applies. The cohort metric is not a forecast for an individual
signal.

Compare rows only when their `score_model_id`, app/product/category, timeframe,
and data-source basis match. Keep legacy score bases at their recorded scale;
never convert an old `/10`, `/20`, `/40`, or `/50` value into `/60` by
multiplying it.

## Crypto signal refresh

Each scan calculates one combined entry score for each registered crypto or
Stockbit product, ticker and timeframe. A row requires sufficient history,
fresh matching reference price when a current market entry is claimed, and a
signed-total magnitude at or above the base floor. It does not require an active cohort profile
or a score above the backtested cutoff. Candidate weights are promoted only
when the cohort reaches at least 50% after-fee wins, a 95% Wilson lower bound
of at least 50% on untouched holdout data, with at least 30 trades. Each app/product slice must
pass independently. These metrics are displayed as historical results; they
do not withhold signals. A successful scan replaces rows that no longer
qualify. Notification history has a separate two-hour retention window based
on when the alert was created. A stale reference price can remain visible with
its freshness label, but it is not a current market entry.

## Signed total and magnitude

Every indicator is evaluated independently on the same causal candle history.
Bullish evidence is positive, bearish evidence negative, and neutral evidence
zero. The weighted total determines the side: positive means LONG, negative
means SHORT, and exactly zero means no directional entry. Opposing components
cancel within that calculation; no fixed ±50 vote is assigned afterward.

The total's absolute value is its magnitude. Apply the base floor and strength
filters to that magnitude so a strong negative total is not accidentally
filtered out. For the current six-factor budget the total ranges from −60 to
+60. A magnitude below 10 receives the low-strength display distinction; missing
numeric components remain unavailable. Stale rows show their last recorded
calculation and cannot claim a current market entry.

## After-fee historical metric

For each learned cohort, the service tests cumulative score cutoffs in
ascending order using candidate-weight outcomes. It selects the lowest cutoff
that reaches at least 50% after fees with a 95% Wilson lower bound of at
least 50% and at least 30 trades on validation data. The
frozen cutoff is then reported against the chronological holdout and separately
for each app/product slice. These metrics decide whether candidate weights
may be promoted; they never filter live signal rows. The screener shows an
available profile's holdout rate, confidence lower bound, sample count, and
cutoff as historical cohort evidence. Without a promoted profile, live scoring
uses equal weights and still shows setups that meet the base score floor.
Historical results do not estimate an individual signal's future win rate.
Spread, slippage, depth, market impact, funding, and borrow remain outside the
fee-adjusted return.

## Six signed indicator scores (−60…+60 total)

The combined entry model evaluates six factors, each on a −10…+10
scale. The first four share 40 nonnegative weight points; validated after-fee
profiles may redistribute them, while the baseline assigns 10 to each. Markov
and lag-one return autocorrelation each keep a fixed 10-point weight.
A component contributes `raw points × weight points / 10`. The six unrounded
contributions sum to the signed total. The base magnitude floor is 1 point.

| Factor | Signed meaning |
| --- | --- |
| Nadaraya-Watson Envelope (NWE) | Score the causal endpoint midpoint minus the close, divided by half the band width. Scale to ±10. The factor is unavailable before 501 bars. |
| DSS Bressert | Score the oscillator minus its trigger. A five-point gap in either direction reaches ±10. |
| Relative volume | Compare current volume with the preceding 20-bar mean. Rising candles contribute positive points and falling candles negative points. Magnitude is `10 × clamp(relative volume − 1, 0, 1)`; a flat candle contributes zero. |
| Estimated Volume Delta (OHLCV proxy) | `10 × clamp((2 × close − high − low) / (high − low), −1, 1)`. Positive close location is bullish and negative close location bearish. This is a candle-pressure estimate, not aggressor-side trade delta. |
| Markov Regime | Causal bull trend or accumulation contributes +10; bear trend or distribution contributes −10; neutral or unavailable state contributes zero. Its weight is fixed. |
| Lag-one Return Autocorrelation | Use a causal one-step AR(1) forecast from up to 128 same-timeframe log returns, requiring at least 30 adjacent pairs. Standardize the forecast by recent return deviation and map it to −10…+10. Positive forecast points are bullish, negative bearish, and zero neutral. The separately displayed Pearson correlation ρ describes dependence, not direction or win probability. |

The model does not preselect a direction. Opposing regime and forecast points
subtract from the other contributions.
Volume and delta use related OHLCV observations and are not independent trade
flow measurements. Missing, non-finite, or invalid factor inputs contribute
zero while retaining an unavailable label; their weights are not redistributed.
Scoring starts after 50 bars. Venue direction policy and data freshness govern
entry status. Backtests use completed bars and simulate shorts only for perpetuals.
The screener can show provisional scores on forming bars.

Changing weights can change the total's sign. The backtest therefore determines
entry and reverse-exit directions from the weighted signed total for every
candidate profile rather than replaying a side chosen before weighting.
Old positive-support model rows and learned profiles retain their model and
protocol identities and cannot be silently reused as signed-model evidence.

## Strategy/confluence blend (`/10`)

Strategies routed through the shared required-confirmation scorer combine a
strategy-strength value and a confluence value, each on a 0–10 scale. The
strategy component has 70% weight and confluence has 30% weight. Confluence raw
points are normalized from the implementation's 11-point maximum onto 0–10.
The total is:

`0.70 × strategy strength + 0.30 × normalized confluence`

Confluence's raw components are:

- DSS cross in the signal direction: 3 points; approaching near-cross: 2 points.
- A same-direction DSS extreme in the latest 3 bars, with DSS turning in the
  signal direction on the current bar: 3 points and required for eligibility.
- NWE outer-band touch: 3 points for a body touch or 2 for a wick touch; the
  signal candle must close inside the envelope.
- NWE near-line close: 1 point when the close is within 15% of full envelope
  width of the direction-matched band.
- NWE reclaim: 1 point when the prior close was outside that band and the
  current close returns inside the envelope.

The directional NWE touch and DSS extreme are eligibility conditions; the
remaining points rank qualifying setups. The maximum is 11 raw confluence
points before normalization.

The component states are presented as separate evidence. This `/10` blend is
not interchangeable with the six-factor `/60` score.

## Tokocrypto vote gate

Tokocrypto crypto-spot and tokenized-stock research profiles first apply a
separate long-only voting gate. The four potential votes are EMA 20/50 trend,
RSI momentum, range breakout, and relative volume. At least two available
indicators must vote LONG, and LONG votes must outnumber SHORT votes; a 2–2
tie does not qualify. Missing or neutral indicators do not vote. SHORT signal
emission is disabled for these categories. The vote count is an
eligibility rule, not a numeric subtotal. A signal that passes it still goes
through the shared required-confirmation path and receives the `/10`
strategy/confluence blend described above. The combined screener score is
a separate six-factor `/60` research model. The profile's registered version and configuration define the
vote count and indicator parameters. Do not claim that all four indicators
agree.

## Retained score versions and status

Persisted score components retain the model identity and scale that produced
them. A retired `/10`, `/20`, `/40`, or `/50` model remains on its recorded scale and is
marked legacy when it no longer matches the active catalog. Those same scales
may still be current for other registered models; the `score_model_id` decides
which rules apply. A fresh scan recomputes the active model; startup does not
silently rescale retained scores. When inputs are unavailable or a scan fails,
preserve the last recorded value only with its stale/unavailable status and
source provenance. A non-qualifying current strategy state is distinct from a
stale retained score.

Stale prior-model rows display their recorded factor values as **Last recorded
indicators**, beside the stale/no-current-entry status. Staleness does not replace that recorded value
with zero. A fresh `not_qualifying` state still displays zero; missing numeric
scores display unavailable rather than an invented zero.

A completed score bar is checked for freshness using its signal timeframe,
including hourly, daily, and weekly rows. The separate current-price check
keeps its own price timeframe; it must not make a completed higher-timeframe
score bar stale merely because that bar opened more than 15 minutes ago.

The signal-card presentation reads the active catalog and persisted score
basis. It must keep scale, component names, status, source, and timeframe
visible together so unlike models are not mistaken for comparable values.

Implementation references: [four-factor combination](../quant/core/src/strategies/four_indicator_score.rs),
[Stockbit NWE setup scoring](../quant/core/src/strategies/indicator_combination.rs),
[DSS setup scoring](../quant/core/src/strategies/dss_bressert_4h.rs),
[shared strategy confirmations](../quant/core/src/strategies/confirmations.rs),
[Tokocrypto voting](../quant/core/src/strategies/tokocrypto.rs),
[Stockbit score catalog](../backend/service/src/master_data/stockbit_signal_catalog.rs),
[crypto score catalog](../backend/service/src/master_data/crypto_signal_catalog.rs),
and [score presentation](../frontend/dashboard/scripts/modules/signal-screener-model.js).

## Decision record: preserve retained values and timeframe freshness

1. **Direct effect:** A stale retained score remains visible with its original
   scale. Acceptance: stored 12/40 displays as last recorded 12/40, not 0/40.
2. **Immediate consequence:** Cards and selected-chart summaries describe the
   same stored value and retain their stale/no-current-entry warning.
3. **Response:** Refresh still requests fresh inputs; viewing a held score does
   not make it a current qualifying setup.
4. **Interactions:** Keep model identity, component arithmetic, timeframe,
   provider provenance, render floors, and current-first ordering unchanged.
   Check completed score-bar freshness using its signal timeframe, separately
   from the current-price timeframe.
5. **Accumulation:** No persisted scores or migrations change; repeated reads
   continue to show the authoritative stored snapshot.
6. **Failure boundaries:** Genuine non-qualifying zeros remain zero. Missing or
   invalid values remain unavailable and never become current evidence.
7. **Recovery:** The existing stale status and Retry action explain recovery;
   regression checks cover both cards and selected summaries.
8. **Governance:** This repairs presentation under the existing retained-score
   contract without changing scoring policy or execution boundaries.
9. **Trajectory:** Hypothesis: showing held values explicitly reduces confusion
   between unavailable fresh inputs and a freshly computed zero.
10. **Decision:** Correct the shared display helper and labels. Reconsider if a
    stale row loses its warning, changes scale, or changes its stored score.

## Decision record: signed contributions determine entry direction

1. Direct effect: opposite indicator contributions cancel; total sign sets the side.
2. Immediate consequence: filters compare magnitude and retain strong SHORT rows.
3. Response: new scans recompute the signed model from causal history.
4. Interactions: live and backtest policies share factors; each weight vector derives its own side.
5. Accumulation: model/protocol versions isolate historical positive-support evidence.
6. Failure boundaries: zero is neutral; unavailable factors keep their labels and zero contribution.
7. Recovery: stale inputs retain warnings, and the existing scan/backtest controls recalculate data.
8. Governance: scoring stays read-only with no execution or position state.
9. Trajectory: hypothesis: explicit arithmetic makes indicator disagreement easier to interpret.
10. Decision: use one signed calculation and revisit if displayed contributions do not reproduce its total or a backtest reuses a preselected side.

## Directional imbalance and cancellation

The screener and selected chart show bullish contribution, bearish contribution,
and cancellation: `100 × (1 − |net| / (bullish + bearish))`. A zero-contribution
row shows zero cancellation and remains neutral. Missing factors leave this
diagnostic unavailable. Cancellation describes disagreement, not win probability.
The combined score replaced the previous NWE-led and DSS-led identities, which
computed the same factors on different warm-up windows. Rerun historical
reports to see evidence for the current model.

The backtest page partitions the selected baseline exit policy's original
non-overlapping holdout trade sequence by LONG and SHORT. Each side shows sample
size, win rate after configured fees, and mean net return. Old snapshots need a
rerun. These diagnostics do not change weights or qualification thresholds.

## Decision record for the combined entry model

1. Publish one score for each app, product, ticker and timeframe. The total's
   magnitude qualifies the row and its sign determines direction.
2. Evaluate the six factors once instead of publishing two rows for the same
   calculation. Show available factors after the 50-bar warm-up.
3. Keep the existing Refresh signals and Backtest actions. Reports using the
   prior model protocol require a rerun for current evidence.
4. Use identical factor observations and weight arithmetic for the screener
   and chronological replay. Validate using completed candles.
5. Keep the new model and protocol identities separate from saved historical
   alerts, reports and weight profiles.
6. Treat zero as neutral and mark unavailable inputs. Maintain freshness
   checks and keep forming scores labeled provisional.
7. Show the score and its factors on the screener. Use the backtest's data
   coverage and validation status to diagnose missing evidence.
8. Preserve read-only research, same-source price attribution and venue
   direction rules for the historical trade simulation.
9. Track scan time and qualifying coverage after release. A single evaluation
   should reduce duplicate work, with the magnitude to be measured.
10. Verify one current row per scope and compare weighted replay with a live
    candle prefix. Rerun the BTC 4h archive with the same configured fees.
