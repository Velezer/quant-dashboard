use super::*;

const BINANCE_INGESTION_PAGE_ROWS: usize = 1_500;
const BINANCE_HISTORY_PAGE_ROWS: usize = 1_500;
const BINANCE_PAGE_PAUSE_MILLISECONDS: u64 = 250;
const YAHOO_HISTORY_PAGE_ROWS: usize = 2_000;

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
    has_stored_history: bool,
) -> Result<Vec<Bar>, String> {
    let (provider_interval, default_range) = yahoo_request_window(kind, interval)?;
    let range = if has_stored_history && interval == "1d" {
        "5d"
    } else {
        default_range
    };
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
        "1d" => Ok((
            "1d",
            if kind == "idx_stock" { "1y" } else { "10y" },
        )),
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
    validate_yahoo_symbol(symbol)?;
    let url = format!(
        "https://query1.finance.yahoo.com/v8/finance/chart/{symbol}?interval={interval}&range={range}"
    );
    let body = fetch_bounded_url(&url)?;
    parse_yahoo_candles(&body, crate::now_secs() as i64, duration_seconds, maximum_rows)
        .map_err(|error| yahoo_chart_error(&body).unwrap_or(error))
}

/// Fetch one older Stockbit/Yahoo candle page. The cursor is the oldest stored
/// timestamp, so each scheduled sync commits a bounded page and resumes from
/// the new database minimum. Daily candles are paged back to first trade; Yahoo
/// itself limits 15m and 60m data to the latest 60 and 730 days respectively.
pub(super) fn fetch_yahoo_history_page(
    symbol: &str,
    interval: &str,
    duration_seconds: i64,
    earliest_ts: i64,
    now_seconds: i64,
) -> Result<(Vec<Bar>, bool), String> {
    validate_yahoo_symbol(symbol)?;
    if duration_seconds <= 0 || earliest_ts <= 0 || now_seconds <= 0 {
        return Err("invalid Yahoo history cursor".into());
    }
    let (provider_interval, page_days, provider_limit_days) = match interval {
        "15m" => ("15m", 14_i64, Some(60_i64)),
        "1h" => ("60m", 90_i64, Some(730_i64)),
        "1d" => ("1d", 365 * 3, None),
        _ => return Err("unsupported Yahoo history interval".into()),
    };
    let provider_floor = provider_limit_days.map(|days| {
        now_seconds.saturating_sub(days.saturating_mul(crate::SECONDS_PER_DAY))
    });
    if provider_floor
        .is_some_and(|floor| earliest_ts <= floor.saturating_add(duration_seconds))
    {
        return Ok((Vec::new(), true));
    }

    let earliest_request = earliest_ts.saturating_sub(1);
    let page_start = earliest_ts
        .saturating_sub(page_days.saturating_mul(crate::SECONDS_PER_DAY))
        .max(0)
        .max(provider_floor.unwrap_or(0));
    if page_start >= earliest_request {
        return Ok((Vec::new(), true));
    }

    let url = format!(
        "https://query1.finance.yahoo.com/v8/finance/chart/{symbol}?interval={provider_interval}&period1={page_start}&period2={earliest_ts}"
    );
    let body = match fetch_bounded_url(&url) {
        Ok(body) => body,
        Err(error) if is_yahoo_history_exhausted(&error) => return Ok((Vec::new(), true)),
        Err(error) => return Err(error),
    };
    let empty_result = serde_json::from_slice::<Value>(&body)
        .ok()
        .and_then(|root| {
            root.pointer("/chart/result/0/timestamp")
                .and_then(Value::as_array)
                .map(Vec::is_empty)
        })
        .unwrap_or(false);
    if empty_result {
        return Ok((Vec::new(), true));
    }
    let bars = match parse_yahoo_candles(
        &body,
        now_seconds,
        duration_seconds,
        YAHOO_HISTORY_PAGE_ROWS,
    ) {
        Ok(bars) => bars,
        Err(error) => {
            if let Some(provider_error) = yahoo_chart_error(&body) {
                if is_yahoo_history_exhausted(&provider_error) {
                    return Ok((Vec::new(), true));
                }
                return Err(provider_error);
            }
            return Err(error);
        }
    };
    if bars.iter().any(|bar| bar.ts >= earliest_ts) {
        return Err("Yahoo history page overlaps stored candles".into());
    }
    let first_trade = yahoo_first_trade_date(&body);
    let reached_first_trade = first_trade.is_some_and(|first_trade| {
        bars.first()
            .is_some_and(|bar| bar.ts <= first_trade.saturating_add(duration_seconds))
    });
    Ok((bars, reached_first_trade))
}

fn validate_yahoo_symbol(symbol: &str) -> Result<(), String> {
    if symbol.is_empty()
        || symbol.len() > 50
        || !symbol
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
    {
        return Err("invalid Yahoo research ticker".into());
    }
    Ok(())
}

fn yahoo_first_trade_date(body: &[u8]) -> Option<i64> {
    serde_json::from_slice::<Value>(body)
        .ok()?
        .pointer("/chart/result/0/meta/firstTradeDate")?
        .as_i64()
}

fn yahoo_chart_error(body: &[u8]) -> Option<String> {
    serde_json::from_slice::<Value>(body)
        .ok()?
        .pointer("/chart/error/description")?
        .as_str()
        .map(str::to_owned)
}

fn is_yahoo_history_exhausted(error: &str) -> bool {
    let error = error.to_ascii_lowercase();
    error.contains("data doesn't exist")
        || error.contains("no data found")
        || error.contains("no price data found")
        || error.contains("not available for starttime")
}

pub(super) fn fetch_binance_interval(
    pair: &str,
    market: BinanceMarket,
    interval: &str,
    duration_seconds: i64,
    maximum_rows: usize,
    resume_ts: Option<i64>,
    mut save_page: impl FnMut(&[Bar]) -> Result<(), String>,
) -> Result<usize, String> {
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
    let mut cursor_ms = resume_ts.map(|ts| ts.saturating_mul(quant_core::MILLISECONDS_PER_SECOND))
        .unwrap_or(start_ms);
    let mut saved_rows = 0usize;
    let mut last_saved_ts = None;

    for _ in 0..MAX_BINANCE_PAGES {
        if cursor_ms >= now_ms || saved_rows >= maximum_rows {
            break;
        }
        let limit = (maximum_rows - saved_rows).min(market.max_page_size()).min(BINANCE_INGESTION_PAGE_ROWS);
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
        let next_cursor = last_open_ms.saturating_add(duration_ms);
        if next_cursor <= cursor_ms {
            return Err("Binance pagination did not advance".into());
        }
        let parsed = parse_binance_candles(&body, now_ms, duration_seconds, limit)?;
        if parsed.windows(2).any(|pair| pair[0].ts >= pair[1].ts)
            || parsed.first().zip(last_saved_ts).is_some_and(|(first, previous)| first.ts <= previous)
        {
            return Err("Binance timestamps are not chronological".into());
        }
        if !parsed.is_empty() {
            // Commit before advancing. A later request failure cannot discard this page.
            save_page(&parsed)?;
            saved_rows += parsed.len();
            last_saved_ts = parsed.last().map(|bar| bar.ts);
        }
        cursor_ms = next_cursor;
        if rows.len() < limit {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(BINANCE_PAGE_PAUSE_MILLISECONDS));
    }
    if saved_rows == 0 {
        return Err("Binance returned no completed candles".into());
    }
    Ok(saved_rows)
}

/// Extend an existing Binance series by one bounded page before its oldest
/// stored candle. The database's minimum timestamp is the durable cursor, so a
/// restart resumes at the next missing range without a separate queue.
pub(super) fn fetch_binance_history_page(
    pair: &str,
    market: BinanceMarket,
    interval: &str,
    duration_seconds: i64,
    earliest_ts: i64,
    mut save_page: impl FnMut(&[Bar]) -> Result<(), String>,
) -> Result<usize, String> {
    if pair.is_empty()
        || pair.len() > 24
        || !pair
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        || !matches!(interval, "15m" | "1h" | "1d")
        || duration_seconds <= 0
        || earliest_ts <= 0
    {
        return Err("invalid Binance history backfill request".into());
    }
    let now_ms = i64::try_from(crate::now_secs())
        .map_err(|_| "system clock exceeds Binance timestamp bound")?
        .saturating_mul(quant_core::MILLISECONDS_PER_SECOND);
    let interval_ms = duration_seconds
        .checked_mul(quant_core::MILLISECONDS_PER_SECOND)
        .ok_or("Binance interval overflow")?;
    let earliest_ms = earliest_ts.saturating_mul(quant_core::MILLISECONDS_PER_SECOND);
    let end_ms = earliest_ms.saturating_sub(1);
    let page_start_ms = earliest_ms
        .saturating_sub(interval_ms.saturating_mul(BINANCE_HISTORY_PAGE_ROWS as i64))
        .max(0);
    if page_start_ms >= earliest_ms {
        return Ok(0);
    }
    let requested_rows = usize::try_from(
        end_ms
            .saturating_sub(page_start_ms)
            .div_euclid(interval_ms)
            .saturating_add(1),
    )
    .unwrap_or(BINANCE_INGESTION_PAGE_ROWS)
    .clamp(1, BINANCE_HISTORY_PAGE_ROWS.min(market.max_page_size()));
    let url = format!(
        "{}?symbol={pair}&interval={interval}&startTime={page_start_ms}&endTime={end_ms}&limit={requested_rows}",
        market.endpoint()
    );
    let body = fetch_bounded_url(&url)?;
    let root: Value =
        serde_json::from_slice(&body).map_err(|_| "invalid Binance history response")?;
    let rows = root
        .as_array()
        .ok_or("Binance returned a non-array history response")?;
    if rows.len() > requested_rows {
        return Err("Binance history count exceeded request bound".into());
    }
    if rows.is_empty() {
        return Ok(0);
    }
    let parsed = parse_binance_candles(&body, now_ms, duration_seconds, requested_rows)?;
    if parsed.iter().any(|bar| bar.ts >= earliest_ts)
        || parsed.windows(2).any(|pair| pair[0].ts >= pair[1].ts)
    {
        return Err("Binance history page overlaps stored candles or is unordered".into());
    }
    if !parsed.is_empty() {
        save_page(&parsed)?;
    }
    Ok(parsed.len())
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
