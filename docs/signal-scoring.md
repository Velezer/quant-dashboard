# Signal scoring rules

The in-app Scoring guide (`/scoring`) explains these rules on one page, with an
illustrative points example and the deployment's current model catalog.

This is a reader reference for the signal scores shown by the dashboard. The
versioned strategy and catalog definitions in the source code own runtime
values; update this explanation when those definitions change. A score ranks a
research setup. It is not win probability, validated performance, an execution
quote, or an order recommendation.

Compare rows only when their `score_model_id`, app/product/category, timeframe,
and data-source basis match. Keep legacy score bases at their recorded scale;
never convert an old `/10` or `/20` value into `/40` by multiplying it.

## Total-strength display distinction

For current four-indicator rows, sum the four unrounded numeric contributions.
A sum below 10 shows **Low total strength** with an amber label and left border;
exactly 10 or above shows the neutral **10+ combined points** label. Missing
numeric contributions show **Total strength unavailable**. An indicator with
recorded zero points and an unavailable state keeps that state visible. Stale
rows qualify the label as last recorded. This is a presentation distinction,
not a change to eligibility, ordering, evidence validation, or notifications.
Legacy score models keep their recorded scales and do not use this distinction.
The four separate /10 values and 25% weights remain the primary score display.

## Four indicator scores (`/10` each; 25% weight each)

The registered NWE/DSS score models
`nwe_dss_relative_volume_estimated_delta_rank_v6` and
`dss_bressert_near_cross_nwe_volume_rank_v4` use four factors. Each factor is
capped at 10 points. NWE and DSS form price evidence (0–20); relative volume
and the OHLCV delta estimate form volume evidence (0–20). The dashboard shows
those four values separately, each out of 10 with 25% maximum weight. Their sum
remains an internal ranking value. The current minimum accepted score is
1/40; the active catalog and strategy configuration own that floor.

| Factor | Rule |
| --- | --- |
| Nadaraya-Watson Envelope (NWE) | Uses the score supplied by the registered NWE setup for the selected timeframe and direction, bounded to 0–10. Cross and proximity rules belong to that strategy version. |
| DSS Bressert | Uses the score supplied by the registered DSS setup for the selected timeframe and direction, bounded to 0–10. Cross and near-cross rules belong to that strategy version. |
| Relative volume | Divide the current bar's volume by the mean volume of the preceding 20 bars for the same registered instrument, timeframe, and source. If the current candle direction agrees with the signal, score `10 × clamp((relative volume − 1) / (2 − 1), 0, 1)`; otherwise score zero. |
| Estimated Volume Delta (OHLCV proxy) | Compute close location as `(2 × close − high − low) / (high − low)`, clamp it to −1…1, then multiply by +1 for LONG or −1 for SHORT. Score `10 × max(aligned close location, 0)`. This estimates candle pressure; it is not aggressor-side trade delta. |

The two volume factors use related OHLCV observations, so they are not two
independent trade-flow measurements. A relative-volume factor is unavailable
when the current bar or any of its preceding 20 baseline bars has missing,
non-finite, or non-positive volume, or when there is insufficient baseline
history. The delta proxy is unavailable when its required current-bar values or
positive high-low range are invalid. An unavailable factor contributes zero
points and stays labeled unavailable; it is not treated as neutral agreement.
Stale, incomplete, or cross-source bars do not become current score inputs.

NWE and DSS calculations are deliberately not reduced to one shared formula:
the factor values come from each registered strategy version. The table's
`/40` arithmetic describes how the four values are combined after those
strategy-specific calculations.

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
not interchangeable with the four-factor `/40` score.

## Tokocrypto vote gate

Tokocrypto crypto-spot and tokenized-stock research profiles first apply a
separate long-only voting gate. The four potential votes are EMA 20/50 trend,
RSI momentum, range breakout, and relative volume. At least two available
indicators must vote LONG, and LONG votes must outnumber SHORT votes; a 2–2
tie does not qualify. Missing or neutral indicators do not vote. SHORT signal
emission is disabled for these categories. The vote count is an
eligibility rule, not a numeric subtotal. A signal that passes it still goes
through the shared required-confirmation path and receives the `/10`
strategy/confluence blend described above; it does not receive the four-factor
`/40` model. The profile's registered version and configuration define the
vote count and indicator parameters. Do not claim that all four indicators
agree.

## Retained score versions and status

Persisted score components retain the model identity and scale that produced
them. A retired `/10` or `/20` model remains on its recorded scale and is
marked legacy when it no longer matches the active catalog. Those same scales
may still be current for other registered models; the `score_model_id` decides
which rules apply. A fresh scan recomputes the active model; startup does not
silently rescale retained scores. When inputs are unavailable or a scan fails,
preserve the last recorded value only with its stale/unavailable status and
source provenance. A non-qualifying current strategy state is distinct from a
stale retained score.

Stale four-indicator rows display their recorded factor values as **Last recorded
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
