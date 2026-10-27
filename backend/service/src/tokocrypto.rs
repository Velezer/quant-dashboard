//! Bounded, public Tokocrypto market-data reads. No account or order endpoints.

use serde_json::Value;
use std::{
    collections::HashSet,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub(super) const SYMBOLS_URL: &str = "https://www.tokocrypto.com/open/v1/common/symbols";
pub(super) const MAIN_KLINES_URL: &str = "https://www.tokocrypto.site/api/v3/klines";
pub(super) const NEXT_KLINES_URL: &str = "https://cloudme-toko.2meta.app/api/v1/klines";
const MAX_BODY_BYTES: usize = 4 * 1024 * 1024;
const MAX_SYMBOLS: usize = 20_000;
const MAX_KLINES: usize = 1_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VenueSymbol {
    pub symbol: String,
    pub symbol_type: u8,
    pub base_asset: String,
    pub quote_asset: String,
    pub spot_trading_enabled: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MarketBar {
    pub open_time_ms: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

/// Read the venue's current supported-symbol list. Unsupported symbol types are
/// retained in the result so callers can distinguish them from missing symbols.
pub fn fetch_symbols() -> Result<Vec<VenueSymbol>, String> {
    let body = get_public_json(SYMBOLS_URL, 45)?;
    parse_symbols(&body)
}

/// Read the most recent *completed* candles in ascending order. A symbol must
/// come from venue discovery; type 1 and type 3 use distinct documented hosts.
pub fn fetch_klines(
    symbol: &VenueSymbol,
    interval: &str,
    limit: usize,
) -> Result<Vec<MarketBar>, String> {
    let url = kline_url(symbol, interval, limit)?;
    let body = get_public_json(&url, 12)?;
    let now_ms = now_millis()?;
    require_completed_candles(parse_klines(&body, limit, now_ms)?)
}

fn require_completed_candles(bars: Vec<MarketBar>) -> Result<Vec<MarketBar>, String> {
    if bars.is_empty() {
        Err("Tokocrypto returned no completed candles".into())
    } else {
        Ok(bars)
    }
}

fn kline_url(symbol: &VenueSymbol, interval: &str, limit: usize) -> Result<String, String> {
    if !(1..=MAX_KLINES).contains(&limit) {
        return Err("Tokocrypto kline limit must be between 1 and 1000".into());
    }
    if !matches!(
        interval,
        "1m" | "3m"
            | "5m"
            | "15m"
            | "30m"
            | "1h"
            | "2h"
            | "4h"
            | "6h"
            | "8h"
            | "12h"
            | "1d"
            | "3d"
            | "1w"
            | "1M"
    ) {
        return Err("unsupported Tokocrypto kline interval".into());
    }
    if !valid_symbol_part(&symbol.symbol, true) {
        return Err("invalid Tokocrypto symbol".into());
    }
    let (host, query_symbol) = match symbol.symbol_type {
        1 => (MAIN_KLINES_URL, symbol.symbol.replace('_', "")),
        3 => (NEXT_KLINES_URL, symbol.symbol.clone()),
        _ => return Err("Tokocrypto symbol type has no documented kline route".into()),
    };
    Ok(format!(
        "{host}?symbol={query_symbol}&interval={interval}&limit={limit}"
    ))
}

fn valid_symbol_part(value: &str, allow_underscore: bool) -> bool {
    !value.is_empty()
        && value.len() <= 50
        && value.bytes().all(|byte| {
            byte.is_ascii_uppercase() || byte.is_ascii_digit() || (allow_underscore && byte == b'_')
        })
}

fn valid_listing_identifier(value: &str, allow_underscore: bool) -> bool {
    !value.is_empty()
        && value.len() <= 50
        && value
            .chars()
            .all(|character| character.is_alphanumeric() || (allow_underscore && character == '_'))
}

pub(super) fn venue_symbol_from_registration(
    symbol: &str,
    quote_asset: &str,
    contract_metadata_json: &str,
) -> Result<VenueSymbol, String> {
    if !valid_symbol_part(symbol, true) {
        return Err("registered Tokocrypto venue symbol is invalid".into());
    }
    let metadata: Value = serde_json::from_str(contract_metadata_json)
        .map_err(|_| "registered Tokocrypto contract metadata is invalid JSON")?;
    if metadata.get("data_provider").and_then(Value::as_str) != Some("tokocrypto") {
        return Err("registered Tokocrypto contract metadata has the wrong provider".into());
    }
    let symbol_type = metadata
        .get("symbol_type")
        .and_then(Value::as_u64)
        .and_then(|value| u8::try_from(value).ok())
        .ok_or("registered Tokocrypto symbol type is invalid")?;
    if !matches!(symbol_type, 1 | 3) {
        return Err("registered Tokocrypto symbol type has no documented kline route".into());
    }
    let quote_asset = quote_asset.trim().to_ascii_uppercase();
    let metadata_quote = metadata
        .get("quote_asset")
        .and_then(Value::as_str)
        .map(str::trim)
        .map(str::to_ascii_uppercase)
        .ok_or("registered Tokocrypto quote asset is missing")?;
    if quote_asset != metadata_quote || !matches!(quote_asset.as_str(), "IDR" | "USDT") {
        return Err("registered Tokocrypto quote asset does not match its metadata".into());
    }
    let compact_symbol = symbol.replace('_', "");
    let base_asset = compact_symbol
        .strip_suffix(&quote_asset)
        .filter(|base| !base.is_empty())
        .ok_or("registered Tokocrypto symbol does not match its quote asset")?;
    if !valid_symbol_part(base_asset, false) {
        return Err("registered Tokocrypto base asset is invalid".into());
    }
    Ok(VenueSymbol {
        symbol: symbol.to_string(),
        symbol_type,
        base_asset: base_asset.to_string(),
        quote_asset,
        spot_trading_enabled: true,
    })
}

fn now_millis() -> Result<i64, String> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_millis();
    i64::try_from(millis).map_err(|_| "system time is outside i64 milliseconds".into())
}

pub(super) fn get_public_json(url: &str, max_time_seconds: u64) -> Result<Vec<u8>, String> {
    get_public_document(url, max_time_seconds, "application/json")
}

pub(super) fn get_public_document(
    url: &str,
    max_time_seconds: u64,
    accept: &str,
) -> Result<Vec<u8>, String> {
    let response = super::public_http::get_bounded(
        url,
        super::public_http::RequestOptions {
            connect_timeout: Duration::from_secs(5),
            total_timeout: Duration::from_secs(max_time_seconds),
            max_body_bytes: MAX_BODY_BYTES,
            max_redirects: 0,
            accept,
            user_agent: None,
            accept_encoding: Some("gzip, br"),
        },
    )?;
    if let Some(error) = public_provider_status_error(response.status, response.retry_after_seconds)
    {
        return Err(error);
    }
    Ok(response.body)
}

fn public_provider_status_error(status: u16, retry_after: Option<u64>) -> Option<String> {
    if status == 429 || status == 418 {
        let wait = retry_after
            .map(|seconds| format!("; Retry-After: {seconds} seconds"))
            .unwrap_or_default();
        Some(format!("Public provider HTTP {status} rate limit{wait}"))
    } else if status != 200 {
        Some(format!("Public provider HTTP {status}"))
    } else {
        None
    }
}

fn parse_payload(body: &[u8]) -> Result<Value, String> {
    let root: Value = serde_json::from_slice(body)
        .map_err(|error| format!("Tokocrypto JSON was invalid: {error}"))?;
    if let Some(code) = root.get("code") {
        if code.as_i64() != Some(0) {
            return Err(format!("Tokocrypto API rejected request (code {code})"));
        }
        return root
            .get("data")
            .cloned()
            .ok_or_else(|| "Tokocrypto API response omitted data".into());
    }
    Ok(root)
}

fn parse_symbols(body: &[u8]) -> Result<Vec<VenueSymbol>, String> {
    let payload = parse_payload(body)?;
    let entries = payload
        .get("list")
        .unwrap_or(&payload)
        .as_array()
        .ok_or_else(|| "Tokocrypto symbols response was not an array".to_string())?;
    if entries.is_empty() {
        return Err("Tokocrypto symbols response was empty".into());
    }
    if entries.len() > MAX_SYMBOLS {
        return Err("Tokocrypto symbols response exceeded 20000 entries".into());
    }
    let mut symbols = Vec::with_capacity(entries.len());
    let mut seen = HashSet::with_capacity(entries.len());
    for (index, entry) in entries.iter().enumerate() {
        let object = entry
            .as_object()
            .ok_or_else(|| format!("Tokocrypto symbol {index} was not an object"))?;
        let string = |key: &str| -> Result<String, String> {
            object
                .get(key)
                .and_then(Value::as_str)
                .map(str::to_string)
                .ok_or_else(|| format!("Tokocrypto symbol {index} missing {key}"))
        };
        let symbol = string("symbol")?;
        let base_asset = string("baseAsset")?;
        let quote_asset = string("quoteAsset")?;
        if !valid_listing_identifier(&symbol, true)
            || !valid_listing_identifier(&base_asset, false)
            || !valid_listing_identifier(&quote_asset, false)
        {
            return Err(format!("Tokocrypto symbol {index} has invalid identifier"));
        }
        let symbol_type = object
            .get("type")
            .and_then(|value| {
                value
                    .as_u64()
                    .or_else(|| value.as_str().and_then(|text| text.parse::<u64>().ok()))
            })
            .and_then(|value| u8::try_from(value).ok())
            .ok_or_else(|| format!("Tokocrypto symbol {index} missing valid type"))?;
        if !seen.insert(symbol.clone()) {
            return Err(format!("Tokocrypto symbols response repeats {symbol}"));
        }
        let spot_trading_enabled = match object.get("spotTradingEnable") {
            Some(Value::Bool(enabled)) => *enabled,
            Some(Value::Number(number)) if number.as_i64() == Some(1) => true,
            Some(Value::Number(number)) if number.as_i64() == Some(0) => false,
            Some(Value::String(value)) if value == "1" => true,
            Some(Value::String(value)) if value == "0" => false,
            None if symbol_type == 3 => {
                // Tokocrypto's NextMe (type 3) listings have historically
                // omitted this field; the all-supported-symbols response is
                // the app listing source for that product.
                true
            }
            None => false,
            Some(_) => {
                return Err(format!(
                    "Tokocrypto symbol {index} has invalid spotTradingEnable"
                ))
            }
        };
        symbols.push(VenueSymbol {
            symbol,
            symbol_type,
            base_asset,
            quote_asset,
            spot_trading_enabled,
        });
    }
    Ok(symbols)
}

fn number(value: &Value, name: &str) -> Result<f64, String> {
    let parsed = match value {
        Value::String(text) => text.parse::<f64>().ok(),
        Value::Number(value) => value.as_f64(),
        _ => None,
    };
    parsed
        .filter(|number| number.is_finite())
        .ok_or_else(|| format!("Tokocrypto candle has invalid {name}"))
}

fn parse_klines(body: &[u8], limit: usize, now_ms: i64) -> Result<Vec<MarketBar>, String> {
    let payload = parse_payload(body)?;
    let rows = payload
        .as_array()
        .ok_or_else(|| "Tokocrypto klines response was not an array".to_string())?;
    if rows.len() > limit || rows.len() > MAX_KLINES {
        return Err("Tokocrypto klines response exceeded requested limit".into());
    }
    let mut bars = Vec::with_capacity(rows.len());
    let mut previous_open = None;
    for (index, row) in rows.iter().enumerate() {
        let fields = row
            .as_array()
            .filter(|fields| fields.len() == 12)
            .ok_or_else(|| format!("Tokocrypto candle {index} was not a 12-field array"))?;
        let open_time_ms = fields[0]
            .as_i64()
            .filter(|time| *time > 0)
            .ok_or_else(|| format!("Tokocrypto candle {index} has invalid open time"))?;
        let close_time_ms = fields[6]
            .as_i64()
            .filter(|time| *time >= open_time_ms)
            .ok_or_else(|| format!("Tokocrypto candle {index} has invalid close time"))?;
        if previous_open.is_some_and(|previous| open_time_ms <= previous) {
            return Err("Tokocrypto candles were not strictly chronological".into());
        }
        previous_open = Some(open_time_ms);
        let open = number(&fields[1], "open")?;
        let high = number(&fields[2], "high")?;
        let low = number(&fields[3], "low")?;
        let close = number(&fields[4], "close")?;
        let volume = number(&fields[5], "volume")?;
        if open <= 0.0 || high <= 0.0 || low <= 0.0 || close <= 0.0 {
            return Err(format!("Tokocrypto candle {index} has nonpositive price"));
        }
        if volume < 0.0 || high < open.max(close) || low > open.min(close) || high < low {
            return Err(format!("Tokocrypto candle {index} has inconsistent OHLCV"));
        }
        if close_time_ms < now_ms {
            bars.push(MarketBar {
                open_time_ms,
                open,
                high,
                low,
                close,
                volume,
            });
        }
    }
    Ok(bars)
}

#[cfg(test)]
#[path = "tokocrypto_tests.rs"]
mod tests;
