use super::Bar;
use serde_json::Value;

pub(super) const FIFTEEN_MINUTE_INTERVAL_SECONDS: i64 = 15 * quant_core::SECONDS_PER_MINUTE;
pub(super) const ONE_HOUR_INTERVAL_SECONDS: i64 = quant_core::SECONDS_PER_HOUR;
pub(super) const ONE_DAY_INTERVAL_SECONDS: i64 = crate::SECONDS_PER_DAY;

const YAHOO_HOURLY_SESSION_OFFSET_SECONDS: i64 = 30 * quant_core::SECONDS_PER_MINUTE;

pub(super) fn parse_yahoo_candles(
    body: &[u8],
    now: i64,
    duration_seconds: i64,
    maximum_rows: usize,
) -> Result<Vec<Bar>, String> {
    let root: Value = serde_json::from_slice(body).map_err(|_| "invalid Yahoo response")?;
    let result = root
        .pointer("/chart/result/0")
        .ok_or("Yahoo returned no chart")?;
    let timestamps = result
        .get("timestamp")
        .and_then(Value::as_array)
        .ok_or("Yahoo timestamps missing")?;
    let quote = result
        .pointer("/indicators/quote/0")
        .ok_or("Yahoo OHLCV missing")?;
    if timestamps.len() > maximum_rows || timestamps.is_empty() {
        return Err("Yahoo candle count out of bounds".into());
    }
    let column = |name: &str| -> Result<&Vec<Value>, String> {
        let values = quote
            .get(name)
            .and_then(Value::as_array)
            .ok_or(format!("Yahoo {name} missing"))?;
        if values.len() != timestamps.len() {
            return Err(format!("Yahoo {name} length mismatch"));
        }
        Ok(values)
    };
    let (opens, highs, lows, closes, volumes) = (
        column("open")?,
        column("high")?,
        column("low")?,
        column("close")?,
        column("volume")?,
    );
    let mut bars = Vec::new();
    let mut previous = 0;
    for index in 0..timestamps.len() {
        let ts = timestamps[index]
            .as_i64()
            .ok_or("invalid Yahoo timestamp")?;
        if ts <= previous {
            return Err("Yahoo timestamps are not chronological".into());
        }
        previous = ts;
        if !candle_interval_complete_at(ts, duration_seconds, now) {
            continue;
        }
        if !yahoo_timestamp_matches_interval(ts, duration_seconds) {
            return Err(format!(
                "Yahoo timestamp {ts} is not aligned to the requested candle interval"
            ));
        }
        let values = [
            opens[index].as_f64(),
            highs[index].as_f64(),
            lows[index].as_f64(),
            closes[index].as_f64(),
        ];
        let [Some(open), Some(high), Some(low), Some(close)] = values else {
            continue;
        };
        let volume = volumes[index].as_f64().unwrap_or(0.0);
        validate_bar(open, high, low, close, volume)?;
        bars.push(Bar {
            ts,
            open,
            high,
            low,
            close,
            volume,
        });
    }
    if bars.is_empty() {
        return Err("Yahoo returned no completed candles".into());
    }
    Ok(bars)
}

fn yahoo_timestamp_matches_interval(timestamp: i64, duration_seconds: i64) -> bool {
    if timestamp.rem_euclid(quant_core::SECONDS_PER_MINUTE) != 0 {
        return false;
    }

    match duration_seconds {
        FIFTEEN_MINUTE_INTERVAL_SECONDS => timestamp.rem_euclid(duration_seconds) == 0,
        // US-listed Yahoo equities anchor hourly bars at :30 UTC.
        ONE_HOUR_INTERVAL_SECONDS => matches!(
            timestamp.rem_euclid(duration_seconds),
            0 | YAHOO_HOURLY_SESSION_OFFSET_SECONDS
        ),
        // Daily bars use exchange-local opens rather than UTC midnight.
        _ => true,
    }
}

pub(super) fn parse_binance_candles(
    body: &[u8],
    now_ms: i64,
    duration_seconds: i64,
    maximum_rows: usize,
) -> Result<Vec<Bar>, String> {
    let root: Value =
        serde_json::from_slice(body).map_err(|_| "invalid Binance candle response")?;
    let rows = root
        .as_array()
        .ok_or("Binance returned a non-array candle response")?;
    if rows.len() > maximum_rows {
        return Err("Binance candle count out of bounds".into());
    }
    let mut bars = Vec::with_capacity(rows.len());
    let mut previous_ms = 0;
    for row in rows {
        let values = row.as_array().ok_or("invalid Binance candle row")?;
        if values.len() < 7 {
            return Err("incomplete Binance candle row".into());
        }
        let open_time_ms = values[0].as_i64().ok_or("invalid Binance open timestamp")?;
        let close_time_ms = values[6]
            .as_i64()
            .ok_or("invalid Binance close timestamp")?;
        if open_time_ms <= previous_ms {
            return Err("Binance timestamps are not chronological".into());
        }
        previous_ms = open_time_ms;
        if close_time_ms >= now_ms {
            continue;
        }
        let open = numeric_value(&values[1]).ok_or("invalid Binance open price")?;
        let high = numeric_value(&values[2]).ok_or("invalid Binance high price")?;
        let low = numeric_value(&values[3]).ok_or("invalid Binance low price")?;
        let close = numeric_value(&values[4]).ok_or("invalid Binance close price")?;
        let volume = numeric_value(&values[5]).ok_or("invalid Binance volume")?;
        validate_bar(open, high, low, close, volume)?;
        bars.push(Bar {
            ts: open_time_ms.div_euclid(quant_core::MILLISECONDS_PER_SECOND),
            open,
            high,
            low,
            close,
            volume,
        });
    }
    if bars.is_empty() {
        return Err(format!(
            "Binance returned no completed {duration_seconds}-second candles"
        ));
    }
    Ok(bars)
}

pub(super) fn numeric_value(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|text| text.parse::<f64>().ok()))
}

pub(super) fn candle_interval_complete_at(
    open_time_seconds: i64,
    interval_seconds: i64,
    now_seconds: i64,
) -> bool {
    open_time_seconds.saturating_add(interval_seconds) <= now_seconds
}

pub(super) fn validate_bar(
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: f64,
) -> Result<(), String> {
    if ![open, high, low, close, volume]
        .iter()
        .all(|value| value.is_finite())
        || open <= 0.0
        || close <= 0.0
        || high < open.max(close)
        || low > open.min(close)
        || low <= 0.0
        || volume < 0.0
    {
        return Err("invalid market-data OHLCV".into());
    }
    Ok(())
}

#[cfg(test)]
#[path = "research_proxy_parsing_tests.rs"]
mod tests;
