//! Additional public crypto candle sources used only as labeled research data.
use super::parsing::{
    candle_interval_complete_at, numeric_value, FIFTEEN_MINUTE_INTERVAL_SECONDS,
    ONE_HOUR_INTERVAL_SECONDS,
};
use super::{validate_bar, Bar};
use crate::SECONDS_PER_DAY;
use serde_json::Value;

const MAX_COINBASE_ROWS: usize = 300;
const MAX_BYBIT_ROWS: usize = 1_000;
const MAX_OKX_ROWS: usize = 300;
const BYBIT_MAINNET_MARKET_API_BASE_URLS: [&str; 3] = [
    "https://api.bybit.id",
    "https://api.bytick.com",
    "https://api.bybit.com",
];

#[derive(Clone, Copy)]
enum MarketKind {
    Spot,
    Linear,
}

#[derive(Clone, Copy)]
enum ProviderKind {
    Coinbase,
    Bybit(MarketKind),
    Okx,
}

#[derive(Clone, Copy)]
pub(super) struct ResearchAdapter {
    provider: ProviderKind,
}

pub(super) const ADAPTERS: [ResearchAdapter; 4] = [
    ResearchAdapter {
        provider: ProviderKind::Coinbase,
    },
    ResearchAdapter {
        provider: ProviderKind::Bybit(MarketKind::Spot),
    },
    ResearchAdapter {
        provider: ProviderKind::Bybit(MarketKind::Linear),
    },
    ResearchAdapter {
        provider: ProviderKind::Okx,
    },
];

impl ResearchAdapter {
    pub(super) fn supports(self, category: &str) -> bool {
        match self.provider {
            ProviderKind::Coinbase | ProviderKind::Okx => category == "crypto_spot",
            ProviderKind::Bybit(MarketKind::Spot) => category == "crypto_spot",
            ProviderKind::Bybit(MarketKind::Linear) => {
                matches!(category, "crypto_perpetual" | "crypto_futures")
            }
        }
    }

    pub(super) fn provider_id(self) -> &'static str {
        match self.provider {
            ProviderKind::Coinbase => "coinbase",
            ProviderKind::Bybit(_) => "bybit",
            ProviderKind::Okx => "okx",
        }
    }

    pub(super) fn source_suffix(self) -> &'static str {
        match self.provider {
            ProviderKind::Coinbase => "coinbase_spot_proxy",
            ProviderKind::Bybit(MarketKind::Spot) => "bybit_spot_proxy",
            ProviderKind::Bybit(MarketKind::Linear) => "bybit_linear_proxy",
            ProviderKind::Okx => "okx_spot_proxy",
        }
    }

    fn pair(self, base: &str) -> String {
        match self.provider {
            ProviderKind::Coinbase => format!("{base}-USD"),
            ProviderKind::Bybit(_) => format!("{base}USDT"),
            ProviderKind::Okx => format!("{base}-USDT"),
        }
    }

    fn price_basis(self) -> &'static str {
        match self.provider {
            ProviderKind::Coinbase => "coinbase_usd_reference",
            ProviderKind::Bybit(_) => "bybit_usdt_reference",
            ProviderKind::Okx => "okx_usdt_reference",
        }
    }

    pub(super) fn fetch(
        self,
        base: &str,
        interval: &str,
        duration_seconds: i64,
        maximum_rows: usize,
    ) -> Result<(String, &'static str, Vec<Bar>), String> {
        if base.is_empty()
            || base.len() > 15
            || !base
                .bytes()
                .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
        {
            return Err("invalid crypto reference base asset".into());
        }
        if maximum_rows == 0 || duration_seconds <= 0 {
            return Err("invalid crypto reference candle bound".into());
        }
        let pair = self.pair(base);
        let now_seconds = i64::try_from(crate::now_secs())
            .map_err(|_| "system clock exceeds market-data timestamp bound")?;
        let candles = match self.provider {
            ProviderKind::Coinbase => fetch_coinbase(
                &pair,
                interval,
                duration_seconds,
                maximum_rows.min(MAX_COINBASE_ROWS),
                now_seconds,
            )?,
            ProviderKind::Bybit(market) => fetch_bybit(
                &pair,
                market,
                interval,
                duration_seconds,
                maximum_rows.min(MAX_BYBIT_ROWS),
                now_seconds,
            )?,
            ProviderKind::Okx => fetch_okx(
                &pair,
                interval,
                duration_seconds,
                maximum_rows.min(MAX_OKX_ROWS),
                now_seconds,
            )?,
        };
        Ok((pair, self.price_basis(), candles))
    }
}

fn fetch_coinbase(
    product: &str,
    interval: &str,
    duration_seconds: i64,
    maximum_rows: usize,
    now_seconds: i64,
) -> Result<Vec<Bar>, String> {
    let granularity = match interval {
        "15m" => FIFTEEN_MINUTE_INTERVAL_SECONDS,
        "1h" => ONE_HOUR_INTERVAL_SECONDS,
        "1d" => SECONDS_PER_DAY,
        _ => return Err("unsupported Coinbase candle interval".into()),
    };
    let url = format!(
        "https://api.exchange.coinbase.com/products/{product}/candles?granularity={granularity}"
    );
    let body = super::fetch_bounded_url(&url)?;
    parse_coinbase_candles(&body, duration_seconds, maximum_rows, now_seconds)
}

fn parse_coinbase_candles(
    body: &[u8],
    duration_seconds: i64,
    maximum_rows: usize,
    now_seconds: i64,
) -> Result<Vec<Bar>, String> {
    let root: Value = serde_json::from_slice(&body).map_err(|_| "invalid Coinbase response")?;
    let rows = root
        .as_array()
        .ok_or("Coinbase returned a non-array candle response")?;
    if rows.len() > MAX_COINBASE_ROWS {
        return Err("Coinbase candle count out of bounds".into());
    }
    let mut bars = Vec::with_capacity(rows.len());
    for row in rows {
        let values = row.as_array().ok_or("invalid Coinbase candle row")?;
        if values.len() < 6 {
            return Err("incomplete Coinbase candle row".into());
        }
        let ts = integer_value(&values[0]).ok_or("invalid Coinbase candle timestamp")?;
        if ts <= 0 {
            return Err("invalid Coinbase candle timestamp".into());
        }
        let low = numeric_value(&values[1]).ok_or("invalid Coinbase low price")?;
        let high = numeric_value(&values[2]).ok_or("invalid Coinbase high price")?;
        let open = numeric_value(&values[3]).ok_or("invalid Coinbase open price")?;
        let close = numeric_value(&values[4]).ok_or("invalid Coinbase close price")?;
        let volume = numeric_value(&values[5]).ok_or("invalid Coinbase volume")?;
        validate_bar(open, high, low, close, volume)?;
        if candle_interval_complete_at(ts, duration_seconds, now_seconds) {
            bars.push(Bar {
                ts,
                open,
                high,
                low,
                close,
                volume,
            });
        }
    }
    let mut bars = finish_bars(bars, duration_seconds, "Coinbase")?;
    if bars.len() > maximum_rows {
        bars.drain(..bars.len() - maximum_rows);
    }
    Ok(bars)
}

fn fetch_bybit(
    symbol: &str,
    market: MarketKind,
    interval: &str,
    duration_seconds: i64,
    maximum_rows: usize,
    now_seconds: i64,
) -> Result<Vec<Bar>, String> {
    let provider_interval = match interval {
        "15m" => "15",
        "1h" => "60",
        "1d" => "D",
        _ => return Err("unsupported Bybit candle interval".into()),
    };
    let category = match market {
        MarketKind::Spot => "spot",
        MarketKind::Linear => "linear",
    };
    let limit = maximum_rows.clamp(1, MAX_BYBIT_ROWS);
    let mut failures = Vec::with_capacity(BYBIT_MAINNET_MARKET_API_BASE_URLS.len());
    for base_url in BYBIT_MAINNET_MARKET_API_BASE_URLS {
        let url = bybit_kline_url(base_url, category, symbol, provider_interval, limit);
        let body = match super::fetch_bounded_url(&url) {
            Ok(body) => body,
            Err(error) => {
                failures.push(format!("{base_url}: {error}"));
                continue;
            }
        };
        match parse_bybit_candles(&body, duration_seconds, limit, now_seconds) {
            Ok(bars) => return Ok(bars),
            Err(error) if error.starts_with("Bybit candle request failed:") => return Err(error),
            Err(error) => failures.push(format!("{base_url}: {error}")),
        }
    }
    Err(format!(
        "Bybit public market-data endpoints failed: {}",
        failures.join("; ")
    ))
}

fn bybit_kline_url(
    base_url: &str,
    category: &str,
    symbol: &str,
    interval: &str,
    limit: usize,
) -> String {
    format!(
        "{base_url}/v5/market/kline?category={category}&symbol={symbol}&interval={interval}&limit={limit}"
    )
}

fn parse_bybit_candles(
    body: &[u8],
    duration_seconds: i64,
    limit: usize,
    now_seconds: i64,
) -> Result<Vec<Bar>, String> {
    let root: Value = serde_json::from_slice(&body).map_err(|_| "invalid Bybit response")?;
    if root.get("retCode").and_then(Value::as_i64) != Some(0) {
        let message = root
            .get("retMsg")
            .and_then(Value::as_str)
            .filter(|message| !message.is_empty())
            .unwrap_or("Bybit rejected the candle request");
        return Err(format!("Bybit candle request failed: {message}"));
    }
    let rows = root
        .pointer("/result/list")
        .and_then(Value::as_array)
        .ok_or("Bybit candle rows missing")?;
    if rows.len() > limit {
        return Err("Bybit candle count out of bounds".into());
    }
    let mut bars = Vec::with_capacity(rows.len());
    for row in rows {
        let values = row.as_array().ok_or("invalid Bybit candle row")?;
        if values.len() < 6 {
            return Err("incomplete Bybit candle row".into());
        }
        let timestamp_ms = integer_value(&values[0]).ok_or("invalid Bybit candle timestamp")?;
        if timestamp_ms <= 0 {
            return Err("invalid Bybit candle timestamp".into());
        }
        let ts = timestamp_ms.div_euclid(quant_core::MILLISECONDS_PER_SECOND);
        let open = numeric_value(&values[1]).ok_or("invalid Bybit open price")?;
        let high = numeric_value(&values[2]).ok_or("invalid Bybit high price")?;
        let low = numeric_value(&values[3]).ok_or("invalid Bybit low price")?;
        let close = numeric_value(&values[4]).ok_or("invalid Bybit close price")?;
        let volume = numeric_value(&values[5]).ok_or("invalid Bybit volume")?;
        validate_bar(open, high, low, close, volume)?;
        if candle_interval_complete_at(ts, duration_seconds, now_seconds) {
            bars.push(Bar {
                ts,
                open,
                high,
                low,
                close,
                volume,
            });
        }
    }
    finish_bars(bars, duration_seconds, "Bybit")
}

fn fetch_okx(
    instrument: &str,
    interval: &str,
    duration_seconds: i64,
    maximum_rows: usize,
    now_seconds: i64,
) -> Result<Vec<Bar>, String> {
    let provider_interval = match interval {
        "15m" => "15m",
        "1h" => "1H",
        "1d" => "1Dutc",
        _ => return Err("unsupported OKX candle interval".into()),
    };
    let limit = maximum_rows.clamp(1, MAX_OKX_ROWS);
    let url = format!(
        "https://www.okx.com/api/v5/market/candles?instId={instrument}&bar={provider_interval}&limit={limit}"
    );
    let body = super::fetch_bounded_url(&url)?;
    parse_okx_candles(&body, duration_seconds, limit, now_seconds)
}

fn parse_okx_candles(
    body: &[u8],
    duration_seconds: i64,
    limit: usize,
    now_seconds: i64,
) -> Result<Vec<Bar>, String> {
    let root: Value = serde_json::from_slice(&body).map_err(|_| "invalid OKX response")?;
    if root.get("code").and_then(Value::as_str) != Some("0") {
        let message = root
            .get("msg")
            .and_then(Value::as_str)
            .filter(|message| !message.is_empty())
            .unwrap_or("OKX rejected the candle request");
        return Err(format!("OKX candle request failed: {message}"));
    }
    let rows = root
        .get("data")
        .and_then(Value::as_array)
        .ok_or("OKX candle rows missing")?;
    if rows.len() > limit {
        return Err("OKX candle count out of bounds".into());
    }
    let mut bars = Vec::with_capacity(rows.len());
    for row in rows {
        let values = row.as_array().ok_or("invalid OKX candle row")?;
        if values.len() < 9 {
            return Err("incomplete OKX candle row".into());
        }
        if values[8].as_str() != Some("1") {
            continue;
        }
        let timestamp_ms = integer_value(&values[0]).ok_or("invalid OKX candle timestamp")?;
        if timestamp_ms <= 0 {
            return Err("invalid OKX candle timestamp".into());
        }
        let ts = timestamp_ms.div_euclid(quant_core::MILLISECONDS_PER_SECOND);
        let open = numeric_value(&values[1]).ok_or("invalid OKX open price")?;
        let high = numeric_value(&values[2]).ok_or("invalid OKX high price")?;
        let low = numeric_value(&values[3]).ok_or("invalid OKX low price")?;
        let close = numeric_value(&values[4]).ok_or("invalid OKX close price")?;
        let volume = numeric_value(&values[5]).ok_or("invalid OKX volume")?;
        validate_bar(open, high, low, close, volume)?;
        if candle_interval_complete_at(ts, duration_seconds, now_seconds) {
            bars.push(Bar {
                ts,
                open,
                high,
                low,
                close,
                volume,
            });
        }
    }
    finish_bars(bars, duration_seconds, "OKX")
}

fn finish_bars(
    mut bars: Vec<Bar>,
    duration_seconds: i64,
    provider: &str,
) -> Result<Vec<Bar>, String> {
    bars.sort_by_key(|bar| bar.ts);
    if bars.len() > 1 && bars.windows(2).any(|pair| pair[0].ts >= pair[1].ts) {
        return Err(format!(
            "{provider} timestamps are not unique and chronological"
        ));
    }
    if bars.is_empty() {
        return Err(format!(
            "{provider} returned no completed {duration_seconds}-second candles"
        ));
    }
    Ok(bars)
}

fn integer_value(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_str().and_then(|text| text.parse::<i64>().ok()))
}

#[cfg(test)]
#[path = "research_proxy_sources_tests.rs"]
mod tests;
