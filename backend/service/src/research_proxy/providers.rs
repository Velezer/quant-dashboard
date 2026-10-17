use super::*;

#[derive(Clone, Copy)]
pub(super) enum BinanceMarket {
    Spot,
    UsdmFutures,
}

impl BinanceMarket {
    pub(super) fn source_suffix(self) -> &'static str {
        match self {
            Self::Spot => "binance_spot_proxy",
            Self::UsdmFutures => "binance_usdm_futures_proxy",
        }
    }

    fn endpoint(self) -> &'static str {
        match self {
            Self::Spot => "https://data-api.binance.vision/api/v3/klines",
            Self::UsdmFutures => "https://fapi.binance.com/fapi/v1/klines",
        }
    }

    fn max_page_size(self) -> usize {
        match self {
            Self::Spot => 1_000,
            Self::UsdmFutures => 1_500,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct Bar {
    pub(super) ts: i64,
    pub(super) open: f64,
    pub(super) high: f64,
    pub(super) low: f64,
    pub(super) close: f64,
    pub(super) volume: f64,
}

pub(super) fn fetch_yahoo_interval(
    symbol: &str,
    kind: &str,
    interval: &str,
    duration_seconds: i64,
    maximum_rows: usize,
) -> Result<Vec<Bar>, String> {
    let (provider_interval, range) = yahoo_request_window(kind, interval)?;
    fetch_chart_interval(
        symbol,
        provider_interval,
        range,
        duration_seconds,
        maximum_rows,
    )
}

pub(super) fn yahoo_request_window(
    kind: &str,
    interval: &str,
) -> Result<(&'static str, &'static str), String> {
    match interval {
        "15m" => Ok((
            "15m",
            match kind {
                "crypto_spot" | "crypto_perpetual" | "crypto_futures" => "7d",
                _ => "30d",
            },
        )),
        "1h" => Ok(("60m", "5d")),
        "1d" => Ok(("1d", "10y")),
        _ => Err("unsupported Yahoo candle interval".into()),
    }
}

pub(super) fn fetch_chart_interval(
    symbol: &str,
    interval: &str,
    range: &str,
    duration_seconds: i64,
    maximum_rows: usize,
) -> Result<Vec<Bar>, String> {
    if symbol.is_empty()
        || symbol.len() > 50
        || !symbol
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
    {
        return Err("invalid Yahoo research ticker".into());
    }
    let url = format!(
        "https://query1.finance.yahoo.com/v8/finance/chart/{symbol}?interval={interval}&range={range}"
    );
    let body = fetch_bounded_url(&url)?;
    parse_yahoo_candles(
        &body,
        crate::now_secs() as i64,
        duration_seconds,
        maximum_rows,
    )
}

pub(super) fn fetch_binance_interval(
    pair: &str,
    market: BinanceMarket,
    interval: &str,
    duration_seconds: i64,
    maximum_rows: usize,
) -> Result<Vec<Bar>, String> {
    if pair.is_empty()
        || pair.len() > 24
        || !pair
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        || !matches!(interval, "15m" | "1h" | "1d")
        || maximum_rows == 0
    {
        return Err("invalid Binance research request".into());
    }

    let now_ms = i64::try_from(crate::now_secs())
        .map_err(|_| "system clock exceeds Binance timestamp bound")?
        .saturating_mul(quant_core::MILLISECONDS_PER_SECOND);
    let duration_ms = duration_seconds
        .checked_mul(quant_core::MILLISECONDS_PER_SECOND)
        .ok_or("Binance interval overflow")?;
    let start_ms = now_ms.saturating_sub(duration_ms.saturating_mul(maximum_rows as i64));
    let mut cursor_ms = start_ms;
    let mut bars = Vec::with_capacity(maximum_rows.min(4_000));

    for _ in 0..MAX_BINANCE_PAGES {
        if cursor_ms >= now_ms || bars.len() >= maximum_rows {
            break;
        }
        let limit = (maximum_rows - bars.len()).min(market.max_page_size());
        let url = format!(
            "{}?symbol={pair}&interval={interval}&startTime={cursor_ms}&endTime={now_ms}&limit={limit}",
            market.endpoint()
        );
        let body = fetch_bounded_url(&url)?;
        let root: Value =
            serde_json::from_slice(&body).map_err(|_| "invalid Binance candle response")?;
        let rows = root
            .as_array()
            .ok_or("Binance returned a non-array candle response")?;
        if rows.is_empty() {
            break;
        }
        if rows.len() > limit {
            return Err("Binance candle count exceeded request bound".into());
        }
        let last_open_ms = rows
            .last()
            .and_then(Value::as_array)
            .and_then(|row| row.first())
            .and_then(Value::as_i64)
            .ok_or("Binance candle timestamp missing")?;
        let parsed = parse_binance_candles(&body, now_ms, duration_seconds, limit)?;
        for bar in parsed {
            if bars
                .last()
                .is_some_and(|previous: &Bar| previous.ts >= bar.ts)
            {
                if bars.last().is_some_and(|previous| previous.ts == bar.ts) {
                    continue;
                }
                return Err("Binance timestamps are not chronological".into());
            }
            bars.push(bar);
        }
        let next_cursor = last_open_ms.saturating_add(duration_ms);
        if next_cursor <= cursor_ms {
            return Err("Binance pagination did not advance".into());
        }
        cursor_ms = next_cursor;
        if rows.len() < limit {
            break;
        }
    }
    if bars.is_empty() {
        return Err("Binance returned no completed candles".into());
    }
    Ok(bars)
}

pub(super) fn yahoo_base_asset(symbol: &str) -> Result<String, String> {
    let normalized = symbol.trim().to_ascii_uppercase();
    let base = normalized
        .strip_suffix("-USD")
        .or_else(|| normalized.strip_suffix("-USDT"))
        .or_else(|| normalized.strip_suffix("USDT"))
        .unwrap_or(&normalized);
    if base.is_empty()
        || base.len() > 15
        || !base
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
    {
        return Err("invalid crypto underlying for Binance reference market".into());
    }
    Ok(base.to_string())
}
