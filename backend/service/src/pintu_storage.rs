//! Pintu Pro Futures' public candle adapter and the SQLite ingestion boundary.
//!
//! Public venue candles are decoded and validated in memory. Only completed,
//! typed OHLCV bars are persisted in the registered instrument's market store.

use rusqlite::{params, Connection};
use serde_json::Value;
use std::collections::BTreeMap;

const SOURCE: &str = "pintu_futures_native";
const CANDLES_URL: &str = "https://api.pintu.pro/perps/v1/public/get-candles";
const MAX_CANDLES: usize = 1_000;
const FIVE_MINUTE_MS: i64 = 5 * 60 * 1_000;
const FIFTEEN_MINUTE_MS: i64 = 15 * 60 * 1_000;
const HOUR_MS: i64 = 60 * 60 * 1_000;
const DAY_MS: i64 = 24 * HOUR_MS;

#[derive(Debug, Clone, PartialEq)]
struct PintuCandle {
    open_time_ms: i64,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: f64,
}

/// Download recent venue candles for the active Pintu Pro Futures instrument.
/// Pintu serves five minute bars, so the 15 minute interval is aggregated from
/// three complete, consecutive venue bars. The one hour history includes the
/// latest 30 closed bars for the futures entry filters.
pub(super) fn sync_intraday_asset(
    connection: &Connection,
    storage_symbol: &str,
    product_id: &str,
    asset_category_id: &str,
) -> Result<(usize, usize), String> {
    let venue_symbol =
        registered_venue_symbol(connection, storage_symbol, product_id, asset_category_id)?;
    let now_ms = super::now_millis();
    let five_minute_end = now_ms.div_euclid(FIVE_MINUTE_MS) * FIVE_MINUTE_MS;
    let five_minute_start = five_minute_end
        .saturating_sub(FIVE_MINUTE_MS.saturating_mul(MAX_CANDLES.saturating_sub(1) as i64));
    let five_minute = fetch_completed_candles(
        &venue_symbol,
        "5m",
        five_minute_start,
        now_ms,
        FIVE_MINUTE_MS,
    )
    .map_err(|error| {
        record_failure(connection, storage_symbol, product_id, "15m", &error);
        error
    })?;
    let bars_15m = aggregate_five_minute_to_fifteen(&five_minute, now_ms);
    if bars_15m.is_empty() {
        let error = "Pintu returned no complete 15-minute candle groups";
        record_failure(connection, storage_symbol, product_id, "15m", error);
        return Err(error.into());
    }

    let hour_start = now_ms.div_euclid(HOUR_MS) * HOUR_MS;
    let bars_1h = fetch_completed_candles(
        &venue_symbol,
        "1h",
        hour_start.saturating_sub(30 * HOUR_MS),
        now_ms,
        HOUR_MS,
    )
    .map_err(|error| {
        record_failure(connection, storage_symbol, product_id, "1h", &error);
        error
    })?;
    if bars_1h.is_empty() {
        let error = "Pintu returned no complete one-hour candles";
        record_failure(connection, storage_symbol, product_id, "1h", error);
        return Err(error.into());
    }

    persist_sync(
        connection,
        storage_symbol,
        product_id,
        asset_category_id,
        &[("15m", &bars_15m), ("1h", &bars_1h)],
    )
    .map_err(|error| {
        record_failure(connection, storage_symbol, product_id, "intraday", &error);
        error
    })?;
    Ok((bars_15m.len(), bars_1h.len()))
}

/// Refresh a rolling 1,000 completed daily candles. This keeps the Pintu
/// futures history beyond the dashboard's 730-day evidence window.
pub(super) fn sync_daily_asset(
    connection: &Connection,
    storage_symbol: &str,
    product_id: &str,
    asset_category_id: &str,
) -> Result<usize, String> {
    let venue_symbol =
        registered_venue_symbol(connection, storage_symbol, product_id, asset_category_id)?;
    let now_ms = super::now_millis();
    let day_start = now_ms.div_euclid(DAY_MS) * DAY_MS;
    let start =
        day_start.saturating_sub(DAY_MS.saturating_mul(MAX_CANDLES.saturating_sub(1) as i64));
    let bars =
        fetch_completed_candles(&venue_symbol, "1d", start, now_ms, DAY_MS).map_err(|error| {
            record_failure(connection, storage_symbol, product_id, "1d", &error);
            error
        })?;
    if bars.len() < 731 {
        let error = format!(
            "Pintu returned {} completed daily candles; at least 731 are required",
            bars.len()
        );
        record_failure(connection, storage_symbol, product_id, "1d", &error);
        return Err(error);
    }
    persist_sync(
        connection,
        storage_symbol,
        product_id,
        asset_category_id,
        &[("1d", &bars)],
    )
    .map_err(|error| {
        record_failure(connection, storage_symbol, product_id, "1d", &error);
        error
    })?;
    Ok(bars.len())
}

fn registered_venue_symbol(
    connection: &Connection,
    storage_symbol: &str,
    product_id: &str,
    asset_category_id: &str,
) -> Result<String, String> {
    let mut statement = connection
        .prepare(
            "SELECT venue_symbol FROM app.instruments
              WHERE exchange_id='pintu' AND product_id=?1 AND asset_category_id=?2
                AND storage_symbol=?3 AND listing_status='venue_active'
                AND venue_verified=1 AND is_active=1",
        )
        .map_err(|error| error.to_string())?;
    let symbols = statement
        .query_map(
            params![product_id, asset_category_id, storage_symbol],
            |row| row.get::<_, String>(0),
        )
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    if symbols.len() != 1 {
        return Err(format!(
            "Pintu native candles require one active verified listing for {storage_symbol}; found {}",
            symbols.len()
        ));
    }
    let venue_symbol = symbols.into_iter().next().unwrap_or_default();
    if !valid_venue_symbol(&venue_symbol) {
        return Err("registered Pintu futures symbol is invalid".into());
    }
    Ok(venue_symbol)
}

fn valid_venue_symbol(symbol: &str) -> bool {
    symbol.ends_with("USDT-PERP")
        && symbol.len() <= 40
        && symbol
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'-')
}

fn fetch_completed_candles(
    venue_symbol: &str,
    interval: &str,
    from_ms: i64,
    to_ms: i64,
    duration_ms: i64,
) -> Result<Vec<PintuCandle>, String> {
    if !valid_venue_symbol(venue_symbol)
        || !matches!(interval, "5m" | "1h" | "1d")
        || from_ms <= 0
        || to_ms <= from_ms
        || duration_ms <= 0
    {
        return Err("invalid Pintu candle request".into());
    }
    let url = format!(
        "{CANDLES_URL}?symbol={venue_symbol}&interval={interval}&from={from_ms}&to={to_ms}"
    );
    let body = crate::research_proxy::fetch_bounded_url(&url)?;
    let root: Value = serde_json::from_slice(&body)
        .map_err(|_| "Pintu candle response was invalid JSON".to_string())?;
    if root.get("code").and_then(Value::as_i64) != Some(0) {
        let reason = root
            .get("reason")
            .and_then(Value::as_str)
            .or_else(|| root.get("message").and_then(Value::as_str))
            .unwrap_or("Pintu rejected the candle request");
        return Err(format!("Pintu candle request failed: {reason}"));
    }
    if root.pointer("/data/symbol").and_then(Value::as_str) != Some(venue_symbol)
        || root.pointer("/data/interval").and_then(Value::as_str) != Some(interval)
    {
        return Err("Pintu candle response does not match the requested market".into());
    }
    let rows = root
        .pointer("/data/candles")
        .and_then(Value::as_array)
        .ok_or_else(|| "Pintu candle rows are missing".to_string())?;
    if rows.is_empty() || rows.len() > MAX_CANDLES {
        return Err(format!(
            "Pintu returned {} candles outside the allowed range 1..={MAX_CANDLES}",
            rows.len()
        ));
    }

    let now_ms = super::now_millis();
    let mut completed = Vec::with_capacity(rows.len());
    let mut previous_time = None;
    for (index, row) in rows.iter().enumerate() {
        let candle = parse_candle(row, index)?;
        if candle.open_time_ms < from_ms || candle.open_time_ms > to_ms {
            return Err("Pintu returned a candle outside the requested time range".into());
        }
        if previous_time.is_some_and(|time| candle.open_time_ms <= time) {
            return Err("Pintu candle timestamps are not strictly chronological".into());
        }
        previous_time = Some(candle.open_time_ms);
        if candle.open_time_ms.saturating_add(duration_ms) <= now_ms {
            completed.push(candle);
        }
    }
    if completed.is_empty() {
        return Err(format!("Pintu returned no completed {interval} candles"));
    }
    Ok(completed)
}

fn parse_candle(row: &Value, index: usize) -> Result<PintuCandle, String> {
    let timestamp = row
        .get("t_start")
        .and_then(parse_integer)
        .filter(|timestamp| *timestamp > 0)
        .ok_or_else(|| format!("Pintu candle {index} has an invalid timestamp"))?;
    let number = |key: &str| {
        row.get(key)
            .and_then(parse_number)
            .filter(|number| number.is_finite())
            .ok_or_else(|| format!("Pintu candle {index} has an invalid {key}"))
    };
    let open = number("o")?;
    let high = number("h")?;
    let low = number("l")?;
    let close = number("c")?;
    let volume = number("v")?;
    if open <= 0.0
        || close <= 0.0
        || high < open.max(close)
        || low > open.min(close)
        || high < low
        || volume < 0.0
    {
        return Err(format!(
            "Pintu candle {index} has inconsistent OHLCV values"
        ));
    }
    Ok(PintuCandle {
        open_time_ms: timestamp,
        open,
        high,
        low,
        close,
        volume,
    })
}

fn parse_integer(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_str().and_then(|text| text.parse::<i64>().ok()))
}

fn parse_number(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|text| text.parse::<f64>().ok()))
}

fn aggregate_five_minute_to_fifteen(candles: &[PintuCandle], now_ms: i64) -> Vec<PintuCandle> {
    let mut groups = BTreeMap::<i64, Vec<&PintuCandle>>::new();
    for candle in candles {
        let bucket = candle.open_time_ms.div_euclid(FIFTEEN_MINUTE_MS) * FIFTEEN_MINUTE_MS;
        groups.entry(bucket).or_default().push(candle);
    }
    let mut result = Vec::with_capacity(groups.len());
    for (bucket, group) in groups {
        if group.len() != 3
            || group.iter().enumerate().any(|(index, candle)| {
                candle.open_time_ms != bucket + index as i64 * FIVE_MINUTE_MS
            })
            || bucket.saturating_add(FIFTEEN_MINUTE_MS) > now_ms
        {
            continue;
        }
        let Some(first) = group.first() else {
            continue;
        };
        let Some(last) = group.last() else {
            continue;
        };
        let volume = group.iter().map(|candle| candle.volume).sum::<f64>();
        if !volume.is_finite() {
            continue;
        }
        result.push(PintuCandle {
            open_time_ms: bucket,
            open: first.open,
            high: group
                .iter()
                .map(|candle| candle.high)
                .fold(f64::NEG_INFINITY, f64::max),
            low: group
                .iter()
                .map(|candle| candle.low)
                .fold(f64::INFINITY, f64::min),
            close: last.close,
            volume,
        });
    }
    result
}

fn persist_sync(
    connection: &Connection,
    storage_symbol: &str,
    product_id: &str,
    asset_category_id: &str,
    intervals: &[(&str, &[PintuCandle])],
) -> Result<(), String> {
    let now = super::format_timestamp(super::now_secs());
    connection
        .execute_batch("BEGIN IMMEDIATE")
        .map_err(|error| error.to_string())?;
    let result = (|| {
        let mut statement = connection
            .prepare(
                "SELECT instrument_id FROM app.instruments
              WHERE exchange_id='pintu' AND product_id=?1 AND asset_category_id=?2
                AND storage_symbol=?3 AND listing_status='venue_active'
                AND venue_verified=1 AND is_active=1",
            )
            .map_err(|error| error.to_string())?;
        let instrument_ids = statement
            .query_map(
                params![product_id, asset_category_id, storage_symbol],
                |row| row.get::<_, String>(0),
            )
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        if instrument_ids.len() != 1 {
            return Err(format!(
                "Pintu native candles require one active verified listing for {storage_symbol}; found {}",
                instrument_ids.len()
            ));
        }
        let instrument_id = &instrument_ids[0];
        for (interval, candles) in intervals {
            persist_series(
                connection,
                storage_symbol,
                product_id,
                instrument_id,
                asset_category_id,
                interval,
                candles,
                &now,
            )?;
        }
        Ok::<(), String>(())
    })();
    match result {
        Ok(()) => connection
            .execute_batch("COMMIT")
            .map_err(|error| error.to_string()),
        Err(error) => {
            let _ = connection.execute_batch("ROLLBACK");
            Err(error)
        }
    }
}

fn persist_series(
    connection: &Connection,
    storage_symbol: &str,
    product_id: &str,
    instrument_id: &str,
    asset_category_id: &str,
    interval: &str,
    candles: &[PintuCandle],
    now: &str,
) -> Result<(), String> {
    connection
        .execute(
            "INSERT INTO series (
                symbol,interval,source,exchange_id,product_id,instrument_id,data_provider,
                timezone,price_basis,instrument_kind,last_success,last_attempt,last_error
             ) VALUES (?1,?2,?3,'pintu',?4,?5,'pintu','UTC','venue_native',?6,?7,?7,NULL)
             ON CONFLICT(symbol,interval,source) DO UPDATE SET
                exchange_id=excluded.exchange_id,product_id=excluded.product_id,
                instrument_id=excluded.instrument_id,data_provider=excluded.data_provider,
                timezone=excluded.timezone,price_basis=excluded.price_basis,
                instrument_kind=excluded.instrument_kind,last_success=excluded.last_success,
                last_attempt=excluded.last_attempt,last_error=NULL",
            params![
                storage_symbol,
                interval,
                SOURCE,
                product_id,
                instrument_id,
                asset_category_id,
                now
            ],
        )
        .map_err(|error| error.to_string())?;
    for candle in candles {
        let timestamp = candle
            .open_time_ms
            .checked_mul(quant_core::NANOS_PER_MILLISECOND)
            .ok_or_else(|| "Pintu candle timestamp overflow".to_string())?;
        connection
            .execute(
                "INSERT INTO bars (
                    symbol,interval,source,ts,open,high,low,close,volume,
                    adj_close,dividends,splits,ingested_at
                 ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,NULL,NULL,NULL,?10)
                 ON CONFLICT(symbol,interval,source,ts) DO UPDATE SET
                    open=excluded.open,high=excluded.high,low=excluded.low,
                    close=excluded.close,volume=excluded.volume,ingested_at=excluded.ingested_at",
                params![
                    storage_symbol,
                    interval,
                    SOURCE,
                    timestamp,
                    candle.open,
                    candle.high,
                    candle.low,
                    candle.close,
                    candle.volume,
                    now
                ],
            )
            .map_err(|error| error.to_string())?;
    }
    connection
        .execute(
            "INSERT INTO ingestion_runs (
                symbol,interval,source,exchange_id,product_id,data_provider,
                attempted_at,status,rows_received,error
             ) VALUES (?1,?2,?3,'pintu',?4,'pintu',?5,'success',?6,NULL)",
            params![
                storage_symbol,
                interval,
                SOURCE,
                product_id,
                now,
                candles.len() as i64
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn record_failure(
    connection: &Connection,
    storage_symbol: &str,
    product_id: &str,
    interval: &str,
    error: &str,
) {
    let now = super::format_timestamp(super::now_secs());
    let _ = connection.execute(
        "INSERT INTO ingestion_runs (
            symbol,interval,source,exchange_id,product_id,data_provider,
            attempted_at,status,rows_received,error
         ) VALUES (?1,?2,?3,'pintu',?4,'pintu',?5,'error',0,?6)",
        params![storage_symbol, interval, SOURCE, product_id, now, error],
    );
}
