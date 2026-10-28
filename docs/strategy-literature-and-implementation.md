# NWE + DSS Bressert causal exit selection · research

Reviewed 11 October 2026. This bounded study combines the existing completed-bar
DSS Bressert cross or approaching near-cross with same-direction causal NWE
outer-band support. It enters at the next bar open, allows one position at a
time, and compares 4, 8, and 12 bar close-timeouts. The highest validation mean
return selects the timeout, with ties favoring the shorter one. Chronological
segments are 60% training, 20% validation, and a final untouched 20% holdout.
The holdout never selects the rule. The same completed-bar signal inputs are
used on the supported screener timeframes.

The dashboard displays each registered ticker separately with its selected
timeout, holdout sample, return basis, freshness, and candle-source identity.
App/category identities remain separate, including Tokocrypto tokenized
stocks. The only configured fees used for net-price returns are the existing
venue rules; unknown fees stay gross-only. The model omits slippage, order-book
depth, market impact, funding, and borrow. A candidate is marked validated only
with at least the shared minimum holdout trade count, at least the shared 730
day total-history rule, venue-native history, known fees and complete modeled
costs, positive holdout mean return, and profit factor above one. It is a
research result, not an order or execution recommendation.

The fresh-session 4h UI check on 11 October 2026 rendered ticker-specific
timeouts and holdouts for Stockbit IDX history. Sample rows included ACST at 4
bars (5 holdout trades, 80% win rate, +3.11% configured-fee mean, PF 12.41) and
ABBA at 8 bars (6 holdout trades, 33.33%, +0.10%, PF 1.03). These are small,
proxy-sourced samples and remain validation-pending; they do not establish a
winning live rule. Many rows had no eligible validation trades. Long daily
histories exist, but the observed selected 4h history is often under 730 days,
and current candles can be marked stale. The report correctly labels those
sources and does not promote them to validated candidates.

Decision and learning: retain this as a per-symbol, per-timeframe research
report. Consider larger / longer venue-native bar histories before setting a
rule in use. Reconsider the timeouts if a future chronological study with
enough native holdout trades changes the validation winner or fails the
holdout gate. Revert the feature if the displayed timeframe, source identity,
fee basis, or selected timeout differs from the inputs used by the evaluator.

## Stockbit day-trading research: opening range and momentum

Reviewed 24 September 2026. This is a research candidate for IDX regular-board
shares in Stockbit. It is long-only, research-only, and not validated for live
execution.

## Ten papers reviewed

1. **Syamni, Wardhiah, Abd Madjid, Faisal & Musnadi, “Do Domestic Investors
   with the Smallest Trading Orders Use Momentum Strategies in The IDX?”**
   *International Research Journal of Business Studies*, 13/14(1), 35–47.
   Studies small-order trades in LQ45 shares from March–May 2017. This is the
   closest local intraday evidence, but its short sample does not test this
   opening-range rule. [DOAJ record](https://doaj.org/article/f3ec3aedae32492d8f831338da4140cc)
   · [paper PDF](https://irjbs.prasetiyamulya.ac.id/index.php/jurnalirjbs/article/download/298/58).
2. **Gao, Han, Li & Zhou (2018), “Market intraday momentum,”** *Journal of
   Financial Economics*, 129(2), 394–414. Finds that the first half-hour
   market return predicts the last half-hour ETF return in studied markets.
   This motivates a first-half-hour direction filter; it is not IDX stock
   evidence. [Publisher](https://www.sciencedirect.com/science/article/pii/S0304405X18301351).
3. **Heston, Korajczyk & Sadka (2010), “Intraday Patterns in the
   Cross-section of Stock Returns,”** *The Journal of Finance*, 65(4),
   1369–1407. Finds return continuation at recurring half-hour periods and
   discusses liquidity effects. [Publisher](https://onlinelibrary.wiley.com/doi/abs/10.1111/j.1540-6261.2010.01573.x).
4. **Holmberg, Lönnbark & Lundström (2013), “Assessing the profitability of
   intraday opening range breakout strategies,”** *Finance Research Letters*,
   10(1), 27–33. Tests crude-oil futures ORB rules; profitability is not
   robust across subperiods. This supports testing ORB as a candidate, not
   assuming it works for IDX equities. [Publisher](https://www.sciencedirect.com/science/article/pii/S1544612312000438).
5. **Zhang, Ma & Zhu (2019), “Intraday momentum and stock return
   predictability: Evidence from China,”** *Economic Modelling*, 76, 319–329.
   Reports predictive first/seventh half-hour returns for the final half-hour
   in Chinese indexes, including a lunch-break effect. [Publisher](https://www.sciencedirect.com/science/article/pii/S0264999318306692).
6. **Limkriangkrai, Chai & Zheng (2023), “Market intraday momentum: APAC
   evidence,”** *Pacific-Basin Finance Journal*, 80, 102086. Finds the effect
   varies across APAC markets, and is weaker or absent in some. This is a
   reason to validate on IDX data rather than transfer foreign results.
   [Publisher](https://doi.org/10.1016/J.PACFIN.2023.102086).
7. **Ho, Lv & Schultz (2021), “Market intraday momentum in Australia,”**
   *Pacific-Basin Finance Journal*, 65, 101499. Finds little evidence of the
   effect in Australia in its sample, another transferability warning.
   [ANU research portal](https://researchportalplus.anu.edu.au/en/publications/market-intraday-momentum-in-australia/).
8. **Li, Sakkas & Urquhart (2022), “Intraday time-series momentum: Global
   evidence and links to market characteristics,”** *Journal of Financial
   Markets*, 57, 100619. Studies 16 developed markets and links effect
   strength to liquidity, volatility, and information characteristics.
   [Publisher](https://www.sciencedirect.com/science/article/pii/S138641812100001X).
9. **Barber, Lee, Liu & Odean (2014), “The Cross-Section of Speculator Skill:
   Evidence from Day Trading,”** *Journal of Financial Markets*, 18, 1–24.
   In Taiwan, fewer than 1% of day traders were predicted to earn positive
   abnormal returns net of costs consistently. [Publisher](https://www.sciencedirect.com/science/article/pii/S1386418113000190).
10. **Chague, De-Losso & Giovannetti (2020), “Day Trading for a Living?”**
    FGV/USP working paper 525. Reports high net-loss rates among persistent
    Brazilian equity-index futures day traders. The market and instrument
    differ from IDX cash equities, so it is a caution about costs and
    persistence rather than a direct estimate for Stockbit users.
    [FGV working paper](https://repositorio.fgv.br/bitstreams/0f00dd78-6909-4f91-84d5-aef8803e6436/download)
    · [RePEc record](https://ideas.repec.org/p/fgv/eesptd/525.html).

The research does not identify a proven IDX opening-range edge. The chosen
rule combines opening-range breakout and early-session momentum findings into
a falsifiable candidate; it does not reproduce any paper's exact strategy.

## Selectable Stockbit day-trade models

The Trade Plans Strategy filter offers four distinct Stockbit → IDX Stock →
Day Trade models. Every research-derived model is marked research-only,
unvalidated; the ten papers do not each supply an executable
signal rule.

Trade Plans now generates a separate row for each of the three research-derived
models alongside the existing Open-Low + Relative Volume baseline. The Strategy
filter shows each model's available plan count. When a model has no active
signal, its row remains monitor-only; any fallback-price evidence stays labeled
as a research proxy and cannot make the candidate validated for execution.

1. **Opening Range + Intraday Momentum · Research** — combines the falsifiable
   opening-range breakout idea in paper 4 with early-session direction evidence
   in papers 2 and 5. No cited study validates this exact rule on IDX shares.
2. **First-Half-Hour Momentum · Research** — inspired by the first-half-hour
   return finding in papers 2 and 5. The Stockbit adaptation tests a same-day
   morning entry and is not a direct replication of either paper's final
   half-hour market prediction.
3. **Same-Time-of-Day Momentum · Research** — inspired by paper 3's recurring
   intraday continuation result. This adaptation compares the current 15-minute
   morning slot with the mean return for that slot across prior sessions.
4. **Open-Low + Relative Volume · Day Trade** — the application's existing
   baseline; it is included for comparison and is not presented as a rule from
   the reviewed papers.

The remaining papers help bound transferability and execution expectations;
they are not additional entry strategies. The APAC and Australia results vary,
and the day-trader studies emphasize persistent costs and the rarity of
consistent net profitability. The UI keeps those findings in validation
guidance rather than turning them into invented signal rules.

All three research candidates use chronological 15-minute OHLCV candles
stamped at the candle open in UTC. They share Stockbit eligibility filters:
long-only direction, relative volume, minimum signal-bar turnover, IDX
price-limit and tick checks, at most 3.5% signal-close stop risk, next-bar
entries, Stockbit's separate buy/sell fees, and a forced morning-session exit.
They skip a missing next 15-minute candle and remain unvalidated until stored
history supports a suitable out-of-sample review.

### Opening-range rule

- Use the first two regular-session bars stamped 09:00 and 09:15 WIB to set
  the 09:00–09:30 opening high and low. Require the 09:15 bar close to exceed
  the prior session close.
- Starting with the 09:30 bar, require a close at least one IDX tick above
  the opening high, relative volume of at least 1.5 versus up to the prior 20
  bars (minimum five observations), and at least Rp500 million of signal-bar
  turnover.
- Require at least two ticks of headroom to ARA. Place the candidate stop at
  the opening-range low, bounded by ARB. Skip a signal if close-to-stop risk
  exceeds 3.5% of the signal close. Targets are 1.5R and 2.5R, capped at ARA.
- Emit no more than one long signal per share each day. Simulate entry at the
  next 15-minute open, stop-first if one candle crosses both stop and target,
  evaluate a full exit at TP1, and close at the last candle before the lunch
  break. TP2 is a plan reference, not a partial-fill leg in the evidence
  replay. There are no afternoon entries and no overnight positions.
  Monday–Thursday's morning session ends at 12:00 WIB; Friday's ends at
  11:30 WIB.
- Model Stockbit's published 0.15% buy and 0.25% sell charges separately.
  This does not include spread, market impact, queue position, or order-book
  depth. Paper fills from OHLCV cannot establish executable liquidity.
- Restrict intended use to ordinary continuous regular-board stocks. The
  current market-history schema does not identify board type, so eligibility
  is not verified by the engine. Watchlist and Acceleration Board rules are
  outside this model.

### First-half-hour rule

- After the 09:15 candle closes at 09:30 WIB, require the first-half-hour
  close above the prior session close and the 09:15 candle to close at or above
  its open. Enter only at the next contiguous 15-minute candle in evidence.
- Use the opening-range low as the stop reference, then apply the shared
  Stockbit price-limit, liquidity proxy, stop-risk, target, fee, and session
  constraints above. The code does not require price to break the opening
  range high.

### Same-time-of-day rule

- On eligible morning candles from 09:30 WIB onward, require a positive candle
  and a positive mean return for that same 15-minute slot over at least five
  of the preceding 20 observed sessions.
- Use the signal candle low as the stop reference, then apply the same shared
  Stockbit constraints. Missing slot observations are skipped; no return is
  inferred for a session without a stored candle.

## Fixed-clock Stockbit session experiment

Reviewed 28 September 2026. This is a retrospective research comparison, not a
registered or execution-ready strategy.

### Tested schedule and rules

- Morning baseline: buy at the 09:00 15-minute bar open and sell at the 11:45
  bar open Monday–Thursday. IDX regular trading ends at 11:30 on Friday, so
  Friday trades exit at the 11:30 session close, approximated by the 11:15
  bar close. There is no regular-market 11:45 Friday fill.
- Afternoon baseline: buy at the 14:00 bar open and sell at the 15:45 bar
  open. Both times are within the current IDX regular session.
- Skip a ticker-session when an exact entry or exit bar is absent. The 15-minute
  bar timestamps represent bar opens. Entry and exit are modeled at those
  opens; the Friday morning exception uses the final morning bar close.
- Fees are modeled as Stockbit's published 0.15% buy fee and 0.25% sell fee.
  Net return is gross price return less 0.40 percentage points. Spread,
  slippage, market impact, queue position, price-limit execution, and board
  eligibility are not modeled.

Two simple filter sets were specified before checking the later observations:

- Morning: previous close above its prior five-session mean, prior five-session
  mean turnover at least Rp1 billion, and a nonnegative 09:00 opening gap no
  greater than 2% versus the previous close.
- Afternoon: the same prior-close trend and liquidity gates, entry at or above
  the previous close, and positive pre-entry momentum. For Monday–Thursday,
  momentum is measured from the 13:30 open through the 13:45 close; on Friday,
  it is measured from the 09:00 open through the 11:15 close because the second
  session opens at 14:00.

### Results

The read-only SQLite sample contained 33,246 yfinance-derived 15-minute bars
for 74 active registered Stockbit tickers, dated 2 July–25 September 2026.
These are research proxies, not Stockbit-native candles. Returns use exact
window timestamps and the fee estimate above. Each ticker's own history was
split chronologically, using its first 70% of observed sessions for rule
selection and its last 30% as a holdout. Results below are averages per trade;
the overlapping ticker trades are not independent observations.

| Window / rule | Earlier trades | Earlier win rate | Earlier mean net return | Earlier profit factor | Holdout trades | Holdout win rate | Holdout mean net return | Holdout profit factor |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 09:00–11:45 baseline | 973 | 31.6% | -0.357% | 0.637 | 449 | 18.0% | -1.041% | 0.303 |
| Morning trend + liquidity + gap | 234 | 32.9% | -0.331% | 0.643 | 65 | 7.7% | -1.148% | 0.305 |
| 14:00–15:45 baseline | 944 | 19.1% | -0.554% | 0.277 | 448 | 27.0% | -0.512% | 0.356 |
| Afternoon trend + liquidity + momentum | 78 | 17.9% | -0.766% | 0.123 | 15 | 26.7% | -1.057% | 0.247 |

None of the tested schedules or filters had positive mean net return or a
profit factor above 1 in either sample. The morning rule's apparent small
improvement in its earlier sample did not persist in the holdout. The
afternoon momentum filter also failed. These results reject the tested
fixed-clock rules on this proxy sample; they do not establish that every IDX
intraday approach is unprofitable. The schedule-only holdout's largest single
loss was -14.686% for the morning window and -8.295% for the afternoon window;
no stop was applied to those baseline returns.

Do not promote these schedules to Stockbit's default plan or label them
validated. A new test needs longer, complete Stockbit-native intraday history,
including realistic entry/exit fills and eligible-board metadata. The sample
is short and cross-ticker returns are correlated, so the trade count overstates
the amount of independent evidence.

### Fixed-clock 1% stop and 3R net target sensitivity

Reviewed 28 September 2026. The same clock entries and exit times were tested
with a fixed 1% stop and a target 4.6% above entry. With the 0.40% round-trip
fee estimate, a stopped trade loses 1.4% net and a target win gains 4.2% net,
or 3R before slippage. Stop-first ordering was used when one candle crossed
both levels. Time exits, missing-bar skips, and the filters are as defined
above. Friday's 11:30 session close is modeled at the 11:15 bar close. No
spread, slippage, price-limit fills, or board eligibility is modeled.

The sample used the largest available yfinance-derived 15-minute series for
each of 74 active registered Stockbit tickers (62 `yfinance_fallback` and 12
`stockbit_regular_yfinance_fallback` series). Each ticker's observed dates were
split chronologically 70/30. These are Stockbit research proxies, not native
venue candles. The later block is retrospective and has already been viewed;
it is not an untouched validation set.

| Window / rule | Earlier trades | Earlier win rate | Earlier mean net return | Earlier profit factor | Later trades | Later win rate | Later mean net return | Later profit factor |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 09:00 entry, 1% stop / 3R net target | 689 | 25.7% | -0.416% | 0.518 | 445 | 12.6% | -0.819% | 0.225 |
| Morning trend + liquidity + gap | 220 | 26.4% | -0.439% | 0.511 | 62 | 9.7% | -1.000% | 0.118 |
| 14:00 entry, 1% stop / 3R net target | 671 | 16.8% | -0.588% | 0.196 | 444 | 20.5% | -0.580% | 0.244 |
| Afternoon trend + liquidity + momentum | 81 | 17.3% | -0.656% | 0.128 | 16 | 12.5% | -0.938% | 0.036 |

The 1% stop bounded the modeled loss, but neither clock window nor filter had
positive expectancy. The net-3R bracket did not improve the results enough to
produce a profit factor above 1. Do not deploy either schedule on this evidence.

### Look-ahead oracle demonstration

To show how an apparent 100% win rate can be manufactured, each eligible clock
entry was retained only when the later OHLC path hit the +4.6% target before
the -1% stop. The decision therefore uses future candles that were not
available at entry. Stop-first ordering resolves same-candle ties. Friday
morning's 11:30 session close is modeled at the 11:15 bar close.

| Window / period | Eligible entries | Future target hits kept | Kept-entry win rate | Opportunity coverage |
| --- | ---: | ---: | ---: | ---: |
| Morning, earlier | 689 | 38 | 100% | 5.52% |
| Morning, later | 445 | 16 | 100% | 3.60% |
| Afternoon, earlier | 671 | 5 | 100% | 0.75% |
| Afternoon, later | 444 | 6 | 100% | 1.35% |

The 100% is guaranteed by the future-outcome filter; it is not a predictive
rule and cannot decide which live entries to take. It retained 65 of 2,249
eligible ticker-session entries (2.89%) in this proxy sample. Keep this result
as an overfitting demonstration, not a strategy or validation claim.

### Existing Stockbit signal-candidate screen

The existing Rust evidence engine was also applied to all selected proxy
series with at least 25 bars. It entered at the next bar open, used the
candidate's configured stop and first target, exited on the same session, and
charged 0.15% buy / 0.25% sell fees. These are full-sample retrospective
screening results, not an independent validation. The trade count pools
correlated ticker-session observations.

| Candidate | Trades | Win rate | Mean net return | Profit factor |
| --- | ---: | ---: | ---: | ---: |
| Stockbit open-low + RVOL baseline | 2,915 | 24.9% | -0.526% | 0.312 |
| Opening-range momentum | 68 | 20.6% | -0.429% | 0.264 |
| First-half-hour momentum | 113 | 20.4% | -0.673% | 0.189 |
| Same-time-of-day momentum | 140 | 25.7% | -0.532% | 0.288 |
| Quality day-trade filter | 1,684 | 22.8% | -0.525% | 0.269 |
| Pullback day-trade filter | 2,004 | 23.2% | -0.539% | 0.243 |
| Momentum day-trade filter | 1,447 | 22.1% | -0.508% | 0.296 |

No tested Stockbit setup in the current proxy sample has positive net
expectancy. Keep the profiles research-only and gather a longer, complete
Stockbit-native history before adopting a rule or attempting further parameter
selection. A positive backtest alone would still require realistic fill and
walk-forward validation.

## Pintu Futures EMA 20/50 Golden Cross v1 · Research

This version is the default Pintu Futures perpetual-futures plan profile. It
is a new research identity so its results are not mixed with the prior
EMA/RSI/breakout profile. It remains research-only until matching evidence
passes the existing validation gate.

- Use a closed, contiguous 15-minute candle as the signal bar. Emit a long
  signal only when EMA 20 crosses from at-or-below EMA 50 to above EMA 50.
  There are no short signals.
- Keep the existing confluence filters: RSI 14 between 55 and 78, signal-bar
  volume at least 1.8 times the mean of the prior 20 completed bars, a bullish
  candle, a close above EMA 20, and a close above the prior 20-bar high.
- Place the plan stop 1% below the signal close. In historical evidence, enter
  at the next contiguous 15-minute open and recalculate the same fixed 1% stop
  from that fill. The existing 1.4% and 2.8% profit levels remain references;
  evidence evaluates a full exit at the first target, with stop precedence if
  a bar crosses both levels, and a 96-bar timeout.
- Apply the configured 0.2865% taker fee per side. Stored proxy candles,
  funding, spread, slippage, and venue depth do not establish live execution
  quality. Performance remains unknown until the strategy has validated
  out-of-sample evidence from the required source and window.

### IDX constraints encoded

Rules were checked on 24 September 2026. Regular-session hours are
09:00–12:00 and 13:30–15:49:59 Monday–Thursday, and 09:00–11:30 and
14:00–15:49:59 on Friday. Pre-closing begins at 15:50. The round lot is 100
shares. Price ticks are Rp1 below Rp200, Rp2 from Rp200 to below Rp500, Rp5
from Rp500 to below Rp2,000, Rp10 from Rp2,000 to below Rp5,000, and Rp25 at
Rp5,000 or more. Current regular-board ARA/ARB percentages are tiered ARA
35%/25%/20% and ARB 15%.

The code applies the announced IDX rule phases by Jakarta timestamp:

- From 28 September 2026, minimum price becomes Rp1; prices Rp1–Rp10 have
  fixed Rp1 ARA/ARB moves, while higher price tiers use the announced ARA
  percentages and 15% ARB.
- From 1 January 2027, ARB percentages become symmetric to ARA for the
  percentage-based tiers; the Rp1 fixed move remains for Rp1–Rp10.

Sources: [IDX trading hours, ticks, and auto-rejection rules](https://www.idx.id/en/products-services/trading-hours-and-mechanism/),
[Stockbit published stock fees](https://help.stockbit.com/id/article/transaksi-saham-berapa-biaya-trading-di-stockbit-sekuritas-1lbkyq9/),
[IDX press-release listing for the September 2026 decree](https://block.idx.id/en/news/press-release/),
and [ANTARA's effective-date summary](https://www.antaranews.com/berita/5751471/bei-harga-saham-terendah-rp1-berlaku-efektif-mulai-28-september-2026).

## Evidence and limits

The strategies are marked **Research / Unvalidated**. The dashboard has no
execution workflow. Historical evaluation uses stored 15-minute bars only; it
does not trigger a provider sync. As of 28 September 2026, the active market
database contains proxy 15-minute history for 74 registered Stockbit tickers,
with uneven coverage from 2 July through 25 September. All bars used in the
fixed-clock experiment are yfinance-derived; no Stockbit-native intraday
history was available to validate execution. Proxy data remains research-only.
Promotion requires adequate history and trade count, positive net performance
after costs, and venue-native execution evidence. The candidates do not place
orders.

## DSS Bressert multi-timeframe screen · Research diagnostic

The supplied screener contributes a per-timeframe DSS/trigger cross state and
a weighted alignment score. The project computes it from the canonical SQLite
history for each active, app-registered instrument already present in the
market scan. It does not import the pasted notebook's Yahoo downloader or
Parquet cache. The model uses the existing DSS defaults (PDS 10, EMA 9,
trigger 5), requires 50 completed bars per frame, and scores the six active
frames `15m`, `1h`, `2h`, `4h`, `1d`, and `1w`. The disabled `1M` frame is not
included. A `2h` frame is derived from that app product's hourly history when
no compatible direct series exists; it is never substituted with raw 1h bars.

Per-frame scores follow the reference states: cross `+3/-3`, near approach
`+2/-2`, approach `+1/-1`, and neutral `0`. A spread within five DSS points
counts as near. The raw score multiplies each state by its screener weight, then
adds `+2/-2` for unanimous alignment or `+1/-1` for a majority. The combined
bias is withheld unless all six frames have enough fresh, completed history.
Each frame retains its provider, source label, proxy/native quality, freshness,
and any last-sync warning. Proxy results stay visibly research-only and cannot
count as venue execution evidence.

The dashboard shows the details in a collapsed section of the selected plan.
It is a ranking diagnostic only: it does not replace the selected plan's
strategy, change `WAIT`, create stops or targets, pass historical validation,
or place orders. Promotion to a signal model requires a separately versioned
strategy identity and matching, chronological walk-forward evidence. The
acceptance signal is full six-frame coverage with source labels and current
freshness shown; missing or stale frames must leave the combined bias
unavailable. The rollback signal is any DSS score that changes plan readiness
or obscures a timeframe's source or stale state; remove the screener annotation
and detail panel while retaining the canonical history and source metadata.

## Required DSS and Nadaraya confirmations · Signal model v2

Every directional candidate entering the screener must have a recent DSS
Bressert reading in the directionally relevant extreme zone and a current turn
back. The completed signal candle must also touch the matching causal,
non-repainting Nadaraya-Watson outer envelope with its wick or real body: LONG
uses the lower band and SHORT uses the upper band. The close must finish inside
the envelope on the signal side. DSS cross/near-cross and Nadaraya near-line or
reclaim add ranking points, but do not substitute for either required
condition. The separate multi-timeframe DSS panel remains diagnostic only.

The causal endpoint needs one full configured 500-bar window before its
midline and bands can qualify a signal. To supply the 499 historical residuals
for its MAE band without waiting for a second window, early residual estimates
use only data available at each bar; the visible midline remains hidden until
its full window is available. Missing, stale, malformed, non-finite, or
insufficient history removes the candidate from the screener. Signal-model and
evidence identities are versioned for v2 so earlier evidence cannot validate
it. Proxy rows retain their source labels and do not become venue-native
evidence through this change.

The ten-order decision trail and rollback conditions for this change appear in
the next section, “Fifteen-minute signal refresh and confluence ranking.”

## Fifteen-minute signal refresh and confluence ranking

The background scanner runs on a fixed 15-minute schedule anchored to each
scan start, without requiring a dashboard heartbeat. Successful completion
does not advance or shorten the next interval. The market-data scheduler sizes
rotating batches to cover the active registered product set in twelve nominal
one-minute ticks, subject to the bounded batch and worker limits documented in
`exchange-data-providers.md`. Live logs showed a 246-target batch taking 153
seconds, extending the full intraday pass to about 30 minutes.
The market-sync pool is now capped at nine workers, matching the database
gate's nine regular history leases; two leases remain reserved for signal scans.
A 15-minute candle is stale after 30 minutes from its open timestamp. Stale
products cannot emit strategy or indicator signals; their existing strategy
plans are retired until a fresh candle is stored.
Provider errors stay visible in ingestion diagnostics, and a proxy success
remains proxy-origin research data.

Production evidence showed the four-worker cap let 246-target batches take
153 seconds across a twelve-batch rotation, putting a complete pass near 30
minutes. The earlier eight-worker run completed 2,911 targets in 13m22 with a
14.4 MiB peak. Nine workers use the full regular history lease allowance; the
eight-target wave size and connection reservations remain unchanged. The
steady-state memory ceiling is now 32 MB. Production must confirm a full pass
within 900 seconds and private memory at or below 32 MB.


Screener signals expire after the two-hour lifetime and are cleaned during
scheduled scans. This is a lifecycle limit, not a historical performance
claim. The scan publication target is 15 minutes; the dashboard displays that
target without allowing it to be changed. The scheduler uses the last measured
scan duration and a two-minute safety margin to place the next start, and never
overlaps scans. A stale-candle sync keeps its product priority but does not
launch another full-universe scan when it succeeds. The next scheduled scan
consumes refreshed candles; the explicit Refresh signals action remains
available for an immediate user-requested scan.

Ten-order decision trail for duration-aware scan scheduling and recovery:

1. **Direct effect:** successful scans schedule the next start from the
   measured duration and two-minute lead against the 15-minute publication
   target; a failed scan still retries after one minute.
2. **Immediate consequence:** a fresh candle produced by a stale-priority sync
   appears in the next scheduled screener snapshot unless the user explicitly
   requests an earlier refresh.
3. **Response:** repeated recovery of stale products no longer starts repeated
   whole-universe scans; the market-data queue continues its bounded rotation.
4. **Interactions:** market sync owns candle acquisition and priority, while
   the scan scheduler owns generation timing. Manual refresh remains a separate
   explicit control.
5. **Accumulation:** duration-aware scheduling reduces avoidable gaps between
   completed snapshots while keeping one scan cycle active at a time and
   retaining the bounded database and memory work.
6. **Failure boundary:** missing or failed provider data remains stale and
   cannot create a current signal. A scan that exceeds its interval finishes
   before the serial scheduler starts its next cycle.
7. **Operational recovery:** `/api/status`, the signal snapshot status, and
   stored candle timestamps expose scan progress and freshness. A user can use
   Refresh signals to request an immediate scan after market data recovers.
8. **Governance and precedent:** the change does not alter source selection,
   app identity, the SQLite ownership model, or the read-only execution rule.
9. **System trajectory (hypothesis):** a duration-aware start target should
   publish snapshots within the 15-minute target when each scan completes
   within the target; live scan timing and memory observations will test this.
10. **Decision and learning:** schedule from the previous completion using the
    measured scan duration and bounded two-minute lead; remove automatic
    sync-triggered scans. Reconsider this if snapshots still miss the target,
    or CPU/memory rises above project limits.

For the active screener score, directional candidates must have a recent DSS
Bressert extreme that has turned back and a completed candle whose wick or body
touches the matching causal Nadaraya-Watson outer envelope: the lower band for
LONG and the upper band for SHORT. The close must finish back inside the
envelope on the signal side. Body touches receive 3 points and wick-only
touches receive 2. DSS cross (3) or near-cross (2), the directional extreme
(3), a close within 15% of the full envelope width from the touched band (1),
and a reclaim from beyond that band on the prior completed candle (1) form an
11-point maximum confluence subtotal. It is normalized to 0–10 and blended
with the existing strategy strength at 30% confluence and 70% strategy
strength. DSS near-cross, Nadaraya near-line, and Nadaraya reclaim distinguish
ranking quality; they do not replace the required extreme and band-touch
conditions. The screener sorts by the resulting score. The score is a ranking
aid, not a probability, and has no claim of predictive quality until
version-matched chronological evidence validates it. The version is
`dss_nadaraya_extreme_rank_v2`; no evidence from v1 is inherited.

Ten-order decision trail for the envelope-touch scoring change:

1. **Direct effect:** only candidates with a directional DSS extreme turn and
   a matching outer-band candle touch enter the screener; the candle's body or
   wick and each score contribution are visible.
2. **Immediate consequence:** long candidates touch the lower envelope and
   short candidates touch the upper envelope, then close back inside the
   signal side; the scorer gives a stronger point value to body contact than
   wick-only contact.
3. **Response:** DSS cross/near-cross and Nadaraya near-line/reclaim award
   ranking points; absent optional confluences lower rank without removing an
   otherwise eligible signal.
4. **Interactions:** the strategy owns thresholds and scoring; candidates use
   completed canonical SQLite bars and causal endpoint values, and API rows
   keep provider/source/quality labels unchanged.
5. **Accumulation:** two-hour screener expiry bounds active signal rows; each
   completed 15-minute scan recomputes the score, while historical candles and
   provenance remain in their existing SQLite owners.
6. **Failure boundary:** missing/non-finite bands, invalid OHLC, an unavailable
   endpoint warmup, a non-turning extreme, or a candle that misses the
   directionally matching band suppresses the candidate; no midline fallback
   is permitted. Early residuals use only data available at that candle while
   the visible endpoint midline waits for its full window. This lets the
   envelope warm up after one configured window instead of two.
7. **Operational recovery:** scan status, candle freshness, score components,
   and source diagnostics remain observable; the previous model can be
   restored by reverting the score implementation and version constant while
   retaining canonical candle history.
8. **Governance and precedent:** the change affects screener eligibility and
   ranking only; it adds no account, position, execution, paper-trading, or
   persisted-store ownership.
9. **System trajectory (hypothesis):** making the qualifying setup explicit
   may reduce signal count and improve operator understanding; compare the
   before/after candidate counts and version-matched chronological results to
   evaluate that hypothesis.
10. **Decision and learning:** use the smallest rule change that implements
    the requested extreme-plus-touch condition; reconsider it if live scans
    emit candidates without both conditions, completed prices grow stale, or
    version-matched evidence shows the quality score does not rank outcomes.

Rollback triggers are any emitted directional signal without both mandatory
conditions, any stale or malformed input passing the gate, a missing score
breakdown, or v1 evidence being associated with v2. Revert the screener scoring
version while preserving stored candle provenance.

## Independent signals on enabled DSS timeframes

The scanner evaluates each registered strategy independently on the enabled
DSS frames: 15m, 1h, 2h, 4h, 1d, and 1w. Every evaluation uses that frame's own
completed candle history and the same DSS Bressert, causal Nadaraya-Watson, and
directional extreme scoring and confirmation rules. Pintu futures keeps its
higher-frame DSS context by using the next enabled DSS frame; the 1w signal
uses its own completed bars as context because no higher enabled frame exists.
Alert identity and SQLite plan replacement are scoped by timeframe. The primary
profile timeframe keeps its existing `Plan_ID` so saved reviews remain stable;
added timeframe plans append their frame to that ID.
The full six-frame screener diagnostic is attached once per product to its
profile timeframe plan; other frame plans keep only their own DSS diagnostic,
so multi-frame output does not multiply the full diagnostic payload.

The scan still runs every 15 minutes. Each emitted plan uses the latest fresh,
completed 15m close for its displayed current price, while its entry structure,
signal candle, source label, and chart timeframe stay tied to its own strategy
frame. If the current 15m price bar is stale or unavailable, all six strategy
frames are withheld and the scanner requests an intraday refresh. If another
frame is stale or lacks enough history, only that frame is withheld and its
existing active plan is retired. A later successful scan regenerates the frame
from canonical SQLite candles.

Each frame's old plan is retired independently before its current result is
stored. The existing two-hour signal lifetime is measured from the completed
signal candle close; repeated 15-minute scans do not extend an expired signal.
Secondary-timeframe plans do not borrow historical validation evidence from a
different interval, so they remain `WAIT` until matching evidence exists.

Seven-order decision trail:

1. **Direct effect:** one registered strategy can produce distinct plans for
   all six enabled DSS timeframes, with one shared confirmation and ranking
   model.
2. **Immediate consequence:** each plan is calculated from its own completed
   candles and displays the latest completed 15m price with separate source
   metadata.
3. **Response:** the 15-minute scanner refreshes each frame; if 15m price data
   is unavailable it queues that product for intraday refresh and withholds all
   timeframe plans.
4. **Interactions:** timeframe-specific alert keys, SQLite plan rows, and
   chart zones remain separated; secondary plan IDs are distinct while the
   primary ID stays stable. App/product identity and provider source labels
   stay owned by existing modules.
5. **Accumulation:** each plan is replaced only by the same strategy and frame.
   Different-frame performance evidence is not reused, and all signal rows
   continue to follow the two-hour cleanup rule. Frame-local diagnostics avoid
   repeating the full screener payload across every plan.
6. **Failure boundary:** stale, partial, or insufficient history retires only
   the affected timeframe, except that stale 15m price history suppresses all
   frames so the UI cannot imply a current price.
7. **Decision and learning:** keep the six configured DSS frames as the sole
   timeframe list and preserve unvalidated secondary signals as `WAIT`.
   Reconsider the Pintu higher-frame context mapping if per-frame validation
   evidence or completed-candle diagnostics show it does not match the intended
   strategy.

The recovery trigger is any timeframe sharing another frame's signal bar,
score/evidence identity, or active-plan lifecycle; any plan showing a stale
15m current price; or any alert remaining active beyond its two-hour expiry.

## Tokocrypto confluence 2-of-4 v1 · Research

The user requested that Tokocrypto not depend on the EMA + RSI + Breakout
Volume combination. The current Tokocrypto profiles require EMA 20/50 trend,
RSI range, a 20-bar range breakout, and relative-volume direction to agree.
The selected interpretation keeps those four factors as optional votes and
emits a long-only signal when at least two valid votes are long and outnumber
short votes. The new strategy has a distinct evidence identity. The existing
all-four model remains unchanged for other profiles that still reference it;
Tokocrypto's former evidence identity remains separately reproducible. The new
Tokocrypto model remains research-only until its matching evidence passes the
existing validation gate.

1. **Direct effect:** Tokocrypto crypto-spot and tokenized-stock signals no
   longer require all four components to agree. Acceptance: two or more long
   votes, greater than the opposing vote count, can produce a signal; fewer
   votes or a tie stay neutral.
2. **Immediate consequence:** More candles can qualify and signal counts may
   rise. Preserve existing 15-minute stops and targets; the screener remains
   informational and places no orders.
3. **Response:** A user may see more candidate rows. Keep the `2-of-4`
   requirement in the strategy name and description so the user can interpret
   the broader gate before inspecting a chart.
4. **Interactions:** Apply the new vote rule only to Tokocrypto crypto spot
   and tokenized stock. Preserve each app/category, provider, and validation
   label. Keep Pintu Futures strategy and evidence behavior unchanged.
5. **Accumulation:** Store the 2-of-4 model under new signal, engine, and
   evidence IDs. Do not combine its results with the retired all-four model,
   the spot fee assumptions, or tokenized-stock proxy evidence.
6. **Failure boundary:** Invalid or unavailable components cast no vote.
   Missing/stale candles still produce no current signal. If fewer than two
   usable votes remain, or opposing votes tie the long count, emit no signal.
7. **Recovery:** Existing all-four evidence remains reproducible by its exact
   old strategy ID. Existing SQLite catalog rows are updated only by the
   explicit command `tooling\\migrate-tokocrypto-confluence.bat`; it stores
   source/destination hashes and changed-row counts in SQLite. Revert the
   active profile IDs if 2-of-4 routing or its source labels fail; do not
   migrate or rewrite stored evidence.
8. **Governance:** This changes a research signal rule only. It does not
   authorize paper trading, order placement, or venue execution claims.
9. **System trajectory (hypothesis):** The explicit vote count may make the
   signal easier to compare with future models. Reconsider if fresh-session
   screener use does not make the rule and evidence status clear.
10. **Decision and learning:** Use the smallest versioned change: a 2-of-4
    Tokocrypto model with long-only output, separate history identities, and
    unchanged provider and risk boundaries. Review chronological evidence and
    fresh-session signal counts; revert if opposing or missing votes are
    represented as agreement, or if Tokocrypto rows expose the Pintu model.

The recovery trigger is a Tokocrypto row generated by the retired all-four
rule, a short signal for a long-only category, mixed evidence IDs, or a row
whose displayed `2-of-4` model does not match the votes used to generate it.

For an existing `QUANT_APP_DB`, stop the service and run
`tooling\\migrate-tokocrypto-confluence.bat` once. Fresh app databases receive
the current Tokocrypto profiles during reference-catalog seeding. The migration
updates only reference-catalog records in one SQLite transaction; it does not
rewrite stored research or evidence rows.

## 2026-10-15 — NWE/DSS + Volume + Estimated Volume Delta v6

The user requested two additional scoring indicators alongside the existing
NWE and DSS components, and then requested a `/40` scale. NWE, DSS Bressert,
20-bar relative volume, and a close-location OHLCV estimate named
`Estimated Volume Delta (OHLCV proxy)` each contribute up to 10 points. NWE and
DSS form the price group (0–20); relative volume and the delta proxy form the
volume group (0–20). The minimum render floor is 1/40, preserving the existing
minimum raw price-evidence contribution. The score model IDs advance so
retained earlier scores are visibly legacy/stale until a current four-factor
scan replaces them.
Tokocrypto's separate 2-of-4 vote
strategy remains unchanged.

The exact factor calculations, availability rules, related model distinctions,
and presentation requirements are documented in the central
[signal scoring rules](signal-scoring.md) reference. This decision record
retains the rationale, schema history, and recovery requirements for the v6
four-factor model.

1. **Direct effect:** NWE and DSS screener rows now show four score components
   and a `/40` ranking total. Acceptance: all four components are named and
   visible, each is within 0–10, group subtotals are within 0–20, and the four
   points sum to the displayed total.
2. **Immediate consequence:** Price and volume each contribute half of the
   total. Missing volume history can lower the score but cannot fabricate a
   point or suppress an otherwise valid price setup by itself.
3. **Response:** Users can see relative participation and the candle-based
   directional estimate beside NWE/DSS. The UI labels the latter as a proxy
   and shows unavailable or opposing states separately.
4. **Interactions:** Calculate every factor from the existing same-source,
   same-timeframe bars. Preserve all app/product/category IDs, registered
   instrument rules, source/provider quality, per-timeframe identity, and
   research-only status. Do not change Tokocrypto's existing 2-of-4 votes.
5. **Accumulation:** Bump the signal score schema to v3 with a `/40` SQLite
   constraint. Existing v2 `/20` rows keep their original values and are
   marked legacy/stale by the explicit maintainer command
   `tooling/migrate-signal-score-scale.bat`; startup never migrates them.
   Preserve the JSON component envelope and version the score model.
6. **Failure boundary:** No prior 20-bar baseline, non-finite or non-positive
   current/baseline volume, or zero-range candle makes the relevant factor
   unavailable with zero points. Opposing direction gives zero aligned volume
   points. Stale/incomplete timeframe history remains subject to existing
   signal freshness and source checks.
7. **Recovery:** Use the existing signal refresh and stale-row status. A fresh
   same-source scan recomputes all four factors; if unavailable inputs persist,
   the UI keeps those components unavailable and retains the last good row as
   stale for the existing Stockbit lifecycle.
8. **Governance:** This changes research ranking only. It adds no account,
   position, execution evidence, paper trade, or order state, and it makes no
   predictive-validity claim.
9. **System trajectory (hypothesis):** Equal factor caps may make price and
   volume contributions easier to compare, though the two volume measures may
   be correlated. Reconsider only with chronological out-of-sample evidence or
   observed confusion about the `/40`, `/20`, and `/10` scales.
10. **Decision and learning:** Use the requested `/40` total, allocate equal
    `/10` contributions, use same-source relative volume plus an explicitly
    labeled OHLCV delta proxy, and version model IDs. Reconsider if a displayed
    total fails to equal its four factors, source/timeframe inputs diverge, or
    users mistake the proxy for true trade delta.

Recovery trigger: any NWE/DSS screener row with fewer or more than four factors,
an incorrect subtotal/total, a legacy score shown as current, or an unlabeled
OHLCV proxy presented as true buy/sell volume delta.
