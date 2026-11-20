use super::*;
use crate::tokocrypto_storage::SOURCE;

pub(super) const FORECAST_SCAN_TIMEFRAME: &str = "4h";
pub(super) const TOKOCRYPTO_FORECAST_PROTOCOL: &str =
    "tokocrypto_native_momentum_next_24h_return_forecast_v1";
pub(super) const STOCKBIT_FORECAST_PROTOCOL: &str =
    "stockbit_yahoo_momentum_next_idx_session_return_forecast_v1";

const FORECAST_HORIZON_NS: i64 = 24 * 60 * 60 * NANOS_PER_SECOND;
const NATIVE_TIMEFRAME: &str = "1h";
const STOCKBIT_TIMEFRAME: &str = "1d";
const STOCKBIT_SOURCE: &str = "stockbit_regular_yfinance_fallback";
const STOCKBIT_MAX_BAR_AGE_NS: i64 = 5 * NANOS_PER_DAY;
const STOCKBIT_HISTORY_NS: i64 = 7 * NANOS_PER_DAY;
const UNIVERSE_LIMIT: usize = 1_000;
const MIN_COVERAGE_PCT: usize = 80;
const MIN_COVERAGE_PAIRS: usize = 10;
const SHORTLIST_LIMIT: usize = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ForecastScope {
    TokocryptoSpot,
    StockbitIdx,
}

impl ForecastScope {
    pub(crate) fn from_id(scope_id: &str) -> Option<Self> {
        match scope_id {
            "tokocrypto_spot" => Some(Self::TokocryptoSpot),
            "stockbit_idx" => Some(Self::StockbitIdx),
            _ => None,
        }
    }

    pub(super) fn id(self) -> &'static str {
        match self {
            Self::TokocryptoSpot => "tokocrypto_spot",
            Self::StockbitIdx => "stockbit_idx",
        }
    }

    pub(super) fn protocol(self) -> &'static str {
        match self {
            Self::TokocryptoSpot => TOKOCRYPTO_FORECAST_PROTOCOL,
            Self::StockbitIdx => STOCKBIT_FORECAST_PROTOCOL,
        }
    }

    pub(super) fn scan_interval_seconds(self) -> i64 {
        let timeframe = match self {
            Self::TokocryptoSpot => FORECAST_SCAN_TIMEFRAME,
            Self::StockbitIdx => STOCKBIT_TIMEFRAME,
        };
        timeframe_duration_ns(timeframe).expect("supported forecast interval") / NANOS_PER_SECOND
    }

    fn exchange_id(self) -> &'static str {
        match self {
            Self::TokocryptoSpot => "tokocrypto",
            Self::StockbitIdx => "stockbit",
        }
    }

    fn product_id(self) -> &'static str {
        match self {
            Self::TokocryptoSpot => "tokocrypto_spot",
            Self::StockbitIdx => "stockbit_regular",
        }
    }

    fn category_id(self) -> &'static str {
        match self {
            Self::TokocryptoSpot => "crypto_spot",
            Self::StockbitIdx => "idx_stock",
        }
    }

    fn source(self) -> &'static str {
        match self {
            Self::TokocryptoSpot => SOURCE,
            Self::StockbitIdx => STOCKBIT_SOURCE,
        }
    }

    fn history_interval(self) -> &'static str {
        match self {
            Self::TokocryptoSpot => NATIVE_TIMEFRAME,
            Self::StockbitIdx => STOCKBIT_TIMEFRAME,
        }
    }

    fn price_basis(self) -> &'static str {
        match self {
            Self::TokocryptoSpot => "venue_native",
            Self::StockbitIdx => "yahoo_unadjusted",
        }
    }

    fn provider(self) -> &'static str {
        match self {
            Self::TokocryptoSpot => "tokocrypto",
            Self::StockbitIdx => "yfinance",
        }
    }

    fn timezone(self) -> &'static str {
        match self {
            Self::TokocryptoSpot => "UTC",
            Self::StockbitIdx => "Asia/Jakarta",
        }
    }

    fn model(self) -> &'static str {
        match self {
            Self::TokocryptoSpot => "Naive 24h momentum extrapolation",
            Self::StockbitIdx => "Naive next-session momentum extrapolation",
        }
    }

    fn target(self) -> &'static str {
        match self {
            Self::TokocryptoSpot => "Next 24h close-to-close return (%)",
            Self::StockbitIdx => "Next available IDX session close-to-close return (%)",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::TokocryptoSpot => "Projects the previous 24h return as the next 24h return using native Tokocrypto Spot candles; unvalidated research.",
            Self::StockbitIdx => "Projects the latest completed session return into the next available IDX session. History is the Yahoo Finance unadjusted research proxy, not Stockbit broker prices; unvalidated research.",
        }
    }
}

#[derive(serde::Serialize)]
struct Candidate {
    instrument_id: String,
    symbol: String,
    venue_symbol: String,
    quote_currency: String,
    forecast_return_pct: f64,
    source_family: &'static str,
    data_provider: &'static str,
    price_basis: &'static str,
    reference_bar_time: i64,
    reference_close: f64,
}

#[derive(Default)]
struct Coverage {
    forecasted: usize,
    missing_or_stale: usize,
    invalid_history: usize,
    read_errors: usize,
}

struct ForecastValue {
    return_pct: f64,
    reference_bar_time: i64,
    reference_close: f64,
}

enum InstrumentForecast {
    Forecasted(ForecastValue),
    MissingOrStale,
    InvalidHistory,
}

fn minimum_forecast_count(eligible: usize) -> usize {
    MIN_COVERAGE_PAIRS.max((eligible * MIN_COVERAGE_PCT).div_ceil(100))
}

pub(super) fn forecast_universe(
    connection: &Connection,
    scope: ForecastScope,
) -> Result<Vec<Value>, rusqlite::Error> {
    match scope {
        ForecastScope::TokocryptoSpot => store::verified_gainer_universe(connection, UNIVERSE_LIMIT),
        ForecastScope::StockbitIdx => {
            store::verified_stockbit_gainer_universe(connection, UNIVERSE_LIMIT)
        }
    }
}

pub(super) fn gainer_forecast_for_scope_at(
    directory: &str,
    scope: ForecastScope,
    cutoff: i64,
    instruments: &[Value],
) -> Result<Value, rusqlite::Error> {
    let mut coverage = Coverage::default();
    let mut candidates = Vec::new();
    for instrument in instruments {
        match forecast_instrument(directory, scope, instrument, cutoff) {
            Ok(InstrumentForecast::Forecasted(value)) => {
                coverage.forecasted += 1;
                if value.return_pct > 0.0 {
                    candidates.push(candidate(scope, instrument, value));
                }
            }
            Ok(InstrumentForecast::MissingOrStale) => coverage.missing_or_stale += 1,
            Ok(InstrumentForecast::InvalidHistory) => coverage.invalid_history += 1,
            Err(_) => coverage.read_errors += 1,
        }
    }
    Ok(forecast_payload(scope, cutoff, instruments.len(), coverage, candidates))
}

pub(super) fn scheduled_payload(scope: ForecastScope) -> Value {
    json!({
        "scope_id": scope.id(),
        "exchange_id": scope.exchange_id(),
        "product_id": scope.product_id(),
        "asset_category_id": scope.category_id(),
        "protocol_id": scope.protocol(),
        "status": "SCHEDULED",
        "target_kind": if scope == ForecastScope::TokocryptoSpot { "calendar_24h" } else { "next_idx_session" },
        "forecast_target": scope.target(),
        "forecast_horizon_seconds": if scope == ForecastScope::TokocryptoSpot { Some(FORECAST_HORIZON_NS / NANOS_PER_SECOND) } else { None },
        "source_family": scope.source(),
        "data_provider": scope.provider(),
        "price_basis": scope.price_basis(),
        "model": scope.model(),
        "model_description": scope.description(),
        "validation_status": "UNVALIDATED_RESEARCH",
        "candidates": [],
    })
}

pub(super) fn store_forecast_run(
    connection: &Connection,
    scope: ForecastScope,
    cutoff_seconds: i64,
    payload: &Value,
    computed_at: i64,
) -> Result<(), rusqlite::Error> {
    let encoded = serde_json::to_string(payload)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
    let status = payload.get("status").and_then(Value::as_str).unwrap_or("ERROR");
    connection.execute(
        "INSERT INTO app.gainer_forecast_runs
            (scope_id,protocol_id,forecast_cutoff,computed_at,status,result_json)
         VALUES(?1,?2,?3,?4,?5,?6)
         ON CONFLICT(scope_id,protocol_id,forecast_cutoff) DO UPDATE SET
            computed_at=excluded.computed_at,status=excluded.status,result_json=excluded.result_json
         WHERE gainer_forecast_runs.status='INSUFFICIENT_COVERAGE'
            OR excluded.status<>'INSUFFICIENT_COVERAGE'",
        params![scope.id(), scope.protocol(), cutoff_seconds, computed_at, status, encoded],
    )?;
    if status != "READY" {
        return Ok(());
    }
    let candidates = payload
        .get("candidates")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    for (index, candidate) in candidates.iter().enumerate() {
        let instrument_id = text(candidate, "instrument_id");
        let reference_bar_time = candidate
            .get("reference_bar_time")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let reference_close = candidate
            .get("reference_close")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let predicted_return_pct = candidate
            .get("forecast_return_pct")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        if instrument_id.is_empty()
            || reference_bar_time <= 0
            || !reference_close.is_finite()
            || reference_close <= 0.0
            || !predicted_return_pct.is_finite()
        {
            continue;
        }
        let target_bar_time = (scope == ForecastScope::TokocryptoSpot).then(|| {
            cutoff_seconds + FORECAST_HORIZON_NS / NANOS_PER_SECOND
                - timeframe_duration_ns(NATIVE_TIMEFRAME).unwrap_or(NANOS_PER_SECOND)
                    / NANOS_PER_SECOND
        });
        connection.execute(
            "INSERT OR IGNORE INTO app.gainer_forecast_predictions
                (scope_id,protocol_id,forecast_cutoff,instrument_id,rank,source,
                 reference_bar_time,reference_close,predicted_return_pct,target_bar_time,created_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            params![
                scope.id(), scope.protocol(), cutoff_seconds, instrument_id,
                i64::try_from(index + 1).unwrap_or(i64::MAX), scope.source(),
                reference_bar_time, reference_close, predicted_return_pct,
                target_bar_time, computed_at,
            ],
        )?;
    }
    Ok(())
}

fn next_stockbit_target_candle(
    market: &Connection,
    symbol: &str,
    reference_bar_time: i64,
    now: i64,
) -> Result<Option<(i64, f64)>, rusqlite::Error> {
    let from_time = reference_bar_time
        .saturating_mul(NANOS_PER_SECOND)
        .saturating_add(1);
    let to_time = now.saturating_mul(NANOS_PER_SECOND);
    let offset = quant_core::JAKARTA_UTC_OFFSET_SECONDS * NANOS_PER_SECOND;
    let target = market
        .query_row(
            "SELECT b.ts,b.open,b.high,b.low,b.close,b.volume FROM bars b
             WHERE b.symbol=?1 AND b.interval=?2 AND b.source=?3
               AND b.ts>?4 AND b.ts<?5 AND b.close>0
               AND NOT EXISTS (
                   SELECT 1 FROM bars newer
                   WHERE newer.symbol=b.symbol AND newer.interval=b.interval
                     AND newer.source=b.source AND newer.ts>b.ts
                     AND newer.ts<((b.ts+?6)/?7+1)*?7-?6
               )
             ORDER BY b.ts LIMIT 1",
            params![
                symbol,
                STOCKBIT_TIMEFRAME,
                STOCKBIT_SOURCE,
                from_time,
                to_time,
                offset,
                NANOS_PER_DAY,
            ],
            candle_from_row,
        )
        .optional()?;
    Ok(target
        .filter(valid_candle)
        .map(|bar| (bar.time, bar.close)))
}

pub(super) fn settle_forecast_outcomes(
    connection: &Connection,
    directory: &str,
    scope: ForecastScope,
    now: i64,
) -> Result<usize, rusqlite::Error> {
    const MAX_OUTCOMES_PER_TICK: usize = 100;
    let pending = {
        let mut statement = connection.prepare(
            "SELECT forecast_cutoff,instrument_id,reference_bar_time,reference_close
             FROM app.gainer_forecast_predictions
             WHERE scope_id=?1 AND protocol_id=?2 AND settled_at IS NULL
             ORDER BY forecast_cutoff,instrument_id LIMIT ?3",
        )?;
        let rows = statement
            .query_map(
                params![scope.id(), scope.protocol(), MAX_OUTCOMES_PER_TICK as i64],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, f64>(3)?,
                    ))
                },
            )?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    let mut settled = 0;
    for (forecast_cutoff, instrument_id, reference_bar_time, reference_close) in pending {
        let storage_symbol: Option<String> = connection
            .query_row(
                "SELECT storage_symbol FROM app.instruments WHERE instrument_id=?1",
                [&instrument_id],
                |row| row.get(0),
            )
            .optional()?;
        let Some(storage_symbol) = storage_symbol else {
            continue;
        };
        let Some(market) = open_market_history_for_symbol(directory, &storage_symbol, false)? else {
            continue;
        };
        let metadata_matches: bool = market.query_row(
            "SELECT EXISTS(SELECT 1 FROM series
             WHERE symbol=?1 AND interval=?2 AND source=?3 AND exchange_id=?4
               AND product_id=?5 AND instrument_id=?6 AND data_provider=?7
               AND price_basis=?8 AND timezone=?9)",
            params![
                storage_symbol, scope.history_interval(), scope.source(), scope.exchange_id(),
                scope.product_id(), instrument_id, scope.provider(), scope.price_basis(), scope.timezone(),
            ],
            |row| row.get(0),
        )?;
        if !metadata_matches {
            continue;
        }
        let row: Option<(i64, f64)> = match scope {
            ForecastScope::TokocryptoSpot => {
                let target_bar_time = forecast_cutoff
                    + FORECAST_HORIZON_NS / NANOS_PER_SECOND
                    - timeframe_duration_ns(NATIVE_TIMEFRAME).unwrap_or(NANOS_PER_SECOND)
                        / NANOS_PER_SECOND;
                market
                    .query_row(
                        "SELECT ts,close FROM bars WHERE symbol=?1 AND interval=?2 AND source=?3
                         AND ts=?4 AND close>0",
                        params![
                            storage_symbol, NATIVE_TIMEFRAME, scope.source(),
                            target_bar_time * NANOS_PER_SECOND,
                        ],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .optional()?
            }
            ForecastScope::StockbitIdx => next_stockbit_target_candle(
                &market,
                &storage_symbol,
                reference_bar_time,
                now,
            )?,
        };
        let Some((target_bar_time_ns, target_close)) = row else {
            continue;
        };
        let realized_return_pct = (target_close / reference_close - 1.0) * 100.0;
        if !realized_return_pct.is_finite() {
            continue;
        }
        connection.execute(
            "UPDATE app.gainer_forecast_predictions
             SET target_bar_time=?1,realized_return_pct=?2,settled_at=?3
             WHERE scope_id=?4 AND protocol_id=?5 AND forecast_cutoff=?6
               AND instrument_id=?7 AND settled_at IS NULL",
            params![
                target_bar_time_ns / NANOS_PER_SECOND, realized_return_pct, now,
                scope.id(), scope.protocol(), forecast_cutoff, instrument_id,
            ],
        )?;
        settled += 1;
    }
    Ok(settled)
}

pub(super) fn observed_outcomes(
    connection: &Connection,
    scope: ForecastScope,
) -> Result<Value, rusqlite::Error> {
    let (count, average_return, positive_count, latest_settled_at): (i64, Option<f64>, i64, Option<i64>) =
        connection.query_row(
            "SELECT COUNT(*),AVG(realized_return_pct),
                    SUM(CASE WHEN realized_return_pct>0 THEN 1 ELSE 0 END),MAX(settled_at)
             FROM app.gainer_forecast_predictions
             WHERE scope_id=?1 AND protocol_id=?2 AND settled_at IS NOT NULL",
            params![scope.id(), scope.protocol()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get::<_, Option<i64>>(2)?.unwrap_or(0), row.get(3)?)),
        )?;
    Ok(json!({
        "sample_count": count,
        "mean_realized_return_pct": average_return,
        "positive_return_rate_pct": (count > 0).then_some(positive_count as f64 * 100.0 / count as f64),
        "latest_settled_at": latest_settled_at,
        "status": "PROSPECTIVE_RESEARCH",
    }))
}

fn candidate(scope: ForecastScope, instrument: &Value, value: ForecastValue) -> Candidate {
    Candidate {
        instrument_id: text(instrument, "instrument_id").to_owned(),
        symbol: text(instrument, "symbol").to_owned(),
        venue_symbol: text(instrument, "venue_symbol").to_owned(),
        quote_currency: text(instrument, "quote_currency").to_owned(),
        forecast_return_pct: value.return_pct,
        source_family: scope.source(),
        data_provider: scope.provider(),
        price_basis: scope.price_basis(),
        reference_bar_time: value.reference_bar_time / NANOS_PER_SECOND,
        reference_close: value.reference_close,
    }
}

fn forecast_payload(
    scope: ForecastScope,
    cutoff: i64,
    eligible: usize,
    coverage: Coverage,
    mut candidates: Vec<Candidate>,
) -> Value {
    candidates.retain(|candidate| {
        candidate.forecast_return_pct.is_finite() && candidate.forecast_return_pct > 0.0
    });
    let minimum_forecasted = minimum_forecast_count(eligible);
    let status = if coverage.forecasted < minimum_forecasted {
        candidates.clear();
        "INSUFFICIENT_COVERAGE"
    } else if candidates.is_empty() {
        "NO_POSITIVE_FORECASTS"
    } else {
        candidates.sort_by(|a, b| {
            b.forecast_return_pct
                .total_cmp(&a.forecast_return_pct)
                .then_with(|| a.symbol.cmp(&b.symbol))
                .then_with(|| a.instrument_id.cmp(&b.instrument_id))
        });
        candidates.truncate(SHORTLIST_LIMIT);
        "READY"
    };
    let horizon_seconds = match scope {
        ForecastScope::TokocryptoSpot => Some(FORECAST_HORIZON_NS / NANOS_PER_SECOND),
        ForecastScope::StockbitIdx => None,
    };
    let forecast_until = (scope == ForecastScope::TokocryptoSpot)
        .then_some((cutoff + FORECAST_HORIZON_NS) / NANOS_PER_SECOND);
    json!({
        "scope_id": scope.id(),
        "exchange_id": scope.exchange_id(),
        "product_id": scope.product_id(),
        "asset_category_id": scope.category_id(),
        "protocol_id": scope.protocol(),
        "status": status,
        "as_of": cutoff / NANOS_PER_SECOND,
        "forecast_until": forecast_until,
        "target_kind": if scope == ForecastScope::TokocryptoSpot { "calendar_24h" } else { "next_idx_session" },
        "eligible_count": eligible,
        "forecasted_count": coverage.forecasted,
        "minimum_forecasted_count": minimum_forecasted,
        "missing_or_stale_count": coverage.missing_or_stale,
        "invalid_history_count": coverage.invalid_history,
        "read_error_count": coverage.read_errors,
        "model": scope.model(),
        "forecast_target": scope.target(),
        "forecast_horizon_seconds": horizon_seconds,
        "source_family": scope.source(),
        "data_provider": scope.provider(),
        "price_basis": scope.price_basis(),
        "model_description": scope.description(),
        "validation_status": "UNVALIDATED_RESEARCH",
        "candidates": candidates,
    })
}

fn forecast_instrument(
    directory: &str,
    scope: ForecastScope,
    instrument: &Value,
    cutoff: i64,
) -> Result<InstrumentForecast, rusqlite::Error> {
    let symbol = text(instrument, "storage_symbol");
    let Some(connection) = open_market_history_for_symbol(directory, symbol, false)? else {
        return Ok(InstrumentForecast::MissingOrStale);
    };
    let market = connection.unchecked_transaction()?;
    let matches_instrument: bool = market.query_row(
        "SELECT EXISTS(SELECT 1 FROM series
         WHERE symbol=?1 AND interval=?2 AND source=?3 AND exchange_id=?4
           AND product_id=?5 AND instrument_id=?6 AND data_provider=?7
           AND price_basis=?8 AND timezone=?9)",
        params![
            symbol,
            scope.history_interval(),
            scope.source(),
            scope.exchange_id(),
            scope.product_id(),
            text(instrument, "instrument_id"),
            scope.provider(),
            scope.price_basis(),
            scope.timezone(),
        ],
        |row| row.get(0),
    )?;
    if !matches_instrument {
        return Ok(InstrumentForecast::MissingOrStale);
    }
    let result = match scope {
        ForecastScope::TokocryptoSpot => forecast_tokocrypto_history(&market, symbol, cutoff),
        ForecastScope::StockbitIdx => forecast_stockbit_history(&market, symbol, cutoff),
    };
    market.commit()?;
    drop(connection);
    result
}

fn forecast_tokocrypto_history(
    market: &Connection,
    symbol: &str,
    cutoff: i64,
) -> Result<InstrumentForecast, rusqlite::Error> {
    let scan_duration_ns = timeframe_duration_ns(FORECAST_SCAN_TIMEFRAME)
        .expect("supported forecast scan timeframe");
    let hour_ns = timeframe_duration_ns(NATIVE_TIMEFRAME).expect("supported native timeframe");
    let required_history_ns = FORECAST_HORIZON_NS + scan_duration_ns;
    let hourly_limit = required_history_ns / hour_ns;
    let hourly = load_candles_from_source_between(
        market,
        symbol,
        NATIVE_TIMEFRAME,
        hourly_limit,
        SOURCE,
        cutoff - required_history_ns,
        cutoff,
    )?;
    if hourly.last().is_none_or(|bar| bar.time != cutoff - hour_ns) {
        return Ok(InstrumentForecast::MissingOrStale);
    }
    let bars = complete_four_hour_suffix(&hourly, cutoff, hour_ns, scan_duration_ns);
    let horizon_bars = usize::try_from(FORECAST_HORIZON_NS / scan_duration_ns)
        .expect("forecast horizon fits in memory");
    if bars.len() <= horizon_bars {
        return Ok(InstrumentForecast::MissingOrStale);
    }
    if bars.iter().any(|bar| !valid_ohlcv(bar)) {
        return Ok(InstrumentForecast::InvalidHistory);
    }
    let end = bars.len() - 1;
    let reference = bars[end];
    let return_pct = forecast_return_pct(&bars, horizon_bars);
    Ok(return_pct.map_or(
        InstrumentForecast::InvalidHistory,
        |return_pct| {
            InstrumentForecast::Forecasted(ForecastValue {
                return_pct,
                reference_bar_time: reference.time,
                reference_close: reference.close,
            })
        },
    ))
}

fn forecast_stockbit_history(
    market: &Connection,
    symbol: &str,
    cutoff: i64,
) -> Result<InstrumentForecast, rusqlite::Error> {
    let bars = load_candles_from_source_between(
        market,
        symbol,
        STOCKBIT_TIMEFRAME,
        3,
        STOCKBIT_SOURCE,
        cutoff - STOCKBIT_HISTORY_NS,
        cutoff,
    )?;
    if bars.len() < 2 {
        return Ok(InstrumentForecast::MissingOrStale);
    }
    let Some(latest) = bars.last() else {
        return Ok(InstrumentForecast::MissingOrStale);
    };
    if cutoff.saturating_sub(latest.time) > STOCKBIT_MAX_BAR_AGE_NS {
        return Ok(InstrumentForecast::MissingOrStale);
    }
    if bars.iter().any(|bar| !valid_candle(bar)) {
        return Ok(InstrumentForecast::InvalidHistory);
    }
    let previous = &bars[bars.len() - 2];
    let return_pct = (latest.close / previous.close - 1.0) * 100.0;
    if !return_pct.is_finite() {
        return Ok(InstrumentForecast::InvalidHistory);
    }
    Ok(InstrumentForecast::Forecasted(ForecastValue {
        return_pct,
        reference_bar_time: latest.time,
        reference_close: latest.close,
    }))
}

fn valid_ohlcv(bar: &StrategyBar) -> bool {
    [bar.open, bar.high, bar.low, bar.close, bar.volume]
        .into_iter()
        .all(f64::is_finite)
        && [bar.open, bar.high, bar.low, bar.close]
            .into_iter()
            .all(|price| price > 0.0)
        && bar.high >= bar.open.max(bar.close)
        && bar.low <= bar.open.min(bar.close)
        && bar.volume >= 0.0
}

fn valid_candle(bar: &Candle) -> bool {
    [bar.open, bar.high, bar.low, bar.close, bar.volume]
        .into_iter()
        .all(f64::is_finite)
        && [bar.open, bar.high, bar.low, bar.close]
            .into_iter()
            .all(|price| price > 0.0)
        && bar.high >= bar.open.max(bar.close)
        && bar.low <= bar.open.min(bar.close)
        && bar.volume >= 0.0
}

fn forecast_return_pct(bars: &[StrategyBar], horizon_bars: usize) -> Option<f64> {
    let end = bars.len().checked_sub(1)?;
    let start = end.checked_sub(horizon_bars)?;
    let previous_close = bars.get(start)?.close;
    let latest_close = bars.get(end)?.close;
    if !previous_close.is_finite()
        || previous_close <= 0.0
        || !latest_close.is_finite()
        || latest_close <= 0.0
    {
        return None;
    }
    let return_pct = (latest_close / previous_close - 1.0) * 100.0;
    return_pct.is_finite().then_some(return_pct)
}

fn complete_four_hour_suffix(
    hourly: &[Candle],
    cutoff: i64,
    hour_ns: i64,
    duration_ns: i64,
) -> Vec<StrategyBar> {
    let mut bars = Vec::new();
    let mut end = cutoff;
    for hours in hourly.rchunks_exact((duration_ns / hour_ns) as usize) {
        let start = end - duration_ns;
        if !hours.iter().enumerate().all(|(index, bar)| {
            bar.time == start + index as i64 * hour_ns && valid_candle(bar)
        }) {
            break;
        }
        let volume = hours.iter().map(|bar| bar.volume).sum::<f64>();
        if !volume.is_finite() {
            break;
        }
        bars.push(StrategyBar {
            time: start,
            open: hours[0].open,
            high: hours.iter().map(|bar| bar.high).fold(f64::NEG_INFINITY, f64::max),
            low: hours.iter().map(|bar| bar.low).fold(f64::INFINITY, f64::min),
            close: hours[hours.len() - 1].close,
            volume,
        });
        end = start;
    }
    bars.reverse();
    bars
}

#[cfg(test)]
#[path = "gainer_forecast_tests.rs"]
mod gainer_tests;
