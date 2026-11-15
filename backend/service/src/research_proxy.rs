//! Bounded public-market-data and Yahoo chart reads for research-only candles.
//! The selected app exchange stays attached to the registered instrument while
//! data_provider identifies the remote source that supplied the candle.
use crate::{Candle, NANOS_PER_SECOND};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;

#[path = "research_proxy/interval.rs"]
mod interval;
#[path = "research_proxy/binance_stream.rs"]
mod binance_stream;
#[path = "research_proxy_parsing.rs"]
mod parsing;
#[path = "research_proxy/providers.rs"]
mod providers;
pub(crate) mod shared;
#[path = "research_proxy_sources.rs"]
mod sources;
mod transport;
use interval::sync_interval_for_exchange;
#[cfg(test)]
use providers::yahoo_request_window;
use providers::{
    fetch_binance_chart_repair_interval, fetch_binance_history_page, fetch_binance_interval,
    fetch_chart_interval, fetch_yahoo_history_page, fetch_yahoo_interval, yahoo_base_asset, Bar,
    BinanceMarket,
};
#[path = "research_proxy_warmup.rs"]
mod warmup;
pub(crate) use transport::fetch_bounded_url;

use parsing::{
    parse_binance_candles, parse_yahoo_candles, validate_bar, FIFTEEN_MINUTE_INTERVAL_SECONDS,
    ONE_DAY_INTERVAL_SECONDS, ONE_HOUR_INTERVAL_SECONDS,
};

const MAX_RESEARCH_15M_CANDLES: usize = 96_000;
const MAX_RESEARCH_1H_CANDLES: usize = 24_000;
const RESEARCH_15M_REFRESH_INTERVAL_SECONDS: i64 = 10 * 60;
pub(crate) const RECENT_CHART_HISTORY_ROWS: usize = 500;
const MAX_CONTINUOUS_CHART_GAP_INTERVALS: i64 = 3;

pub(crate) fn chart_priority_history_rows(interval: &str) -> usize {
    let warmup_rows = quant_core::MarkovRegimeConfig::default()
        .chart_context_warmup_bars()
        .min(crate::MAX_CANDLES as usize) as i64;
    let calculation_rows = (RECENT_CHART_HISTORY_ROWS as i64)
        .saturating_add(warmup_rows)
        .min(crate::MAX_CANDLES);
    // Four-hour candles use the one-hour series, so repair enough source bars
    // for the requested chart window and its regime context.
    let source_rows = match interval {
        "15m" => calculation_rows,
        "1h" => crate::required_chart_source_candles("4h", calculation_rows)
            .unwrap_or(RECENT_CHART_HISTORY_ROWS as i64),
        _ => RECENT_CHART_HISTORY_ROWS as i64,
    };
    let maximum_rows = match interval {
        "15m" => MAX_RESEARCH_15M_CANDLES,
        "1h" => MAX_RESEARCH_1H_CANDLES,
        _ => RECENT_CHART_HISTORY_ROWS,
    };
    source_rows.clamp(1, maximum_rows as i64) as usize
}

/// Size the repair window for the source timeframe selected by the chart,
/// within one bounded chart-priority provider batch.
pub(crate) fn chart_priority_history_rows_for_timeframe(
    interval: &str,
    chart_timeframe: &str,
    source_timeframe: &str,
    history_bars: usize,
) -> Option<usize> {
    if interval != source_timeframe || !matches!(interval, "15m" | "1h" | "1d") {
        return None;
    }
    let requested_rows = crate::required_chart_source_candles_from_base(
        chart_timeframe,
        interval,
        i64::try_from(history_bars).unwrap_or(i64::MAX),
    )
    .and_then(|rows| usize::try_from(rows).ok())
    .unwrap_or_else(|| chart_priority_history_rows(interval));
    let interval_limit = match interval {
        "15m" => MAX_RESEARCH_15M_CANDLES,
        "1h" => MAX_RESEARCH_1H_CANDLES,
        "1d" => 4_000,
        _ => return None,
    };
    let bounded_provider_rows = BinanceMarket::Spot
        .chart_repair_row_limit()
        .min(interval_limit);
    Some(
        requested_rows
            .max(chart_priority_history_rows(interval))
            .clamp(1, bounded_provider_rows),
    )
}

pub(crate) fn recent_chart_window_needs_repair(
    connection: &Connection,
    symbol: &str,
    interval: &str,
    source: &str,
    now_ns: i64,
) -> Result<bool, rusqlite::Error> {
    recent_chart_window_needs_repair_with_rows(
        connection,
        symbol,
        interval,
        source,
        RECENT_CHART_HISTORY_ROWS,
        now_ns,
    )
}

pub(crate) fn recent_chart_window_needs_repair_with_rows(
    connection: &Connection,
    symbol: &str,
    interval: &str,
    source: &str,
    history_rows: usize,
    now_ns: i64,
) -> Result<bool, rusqlite::Error> {
    if !matches!(interval, "15m" | "1h" | "1d") {
        return Ok(false);
    }
    let duration_seconds = match interval {
        "15m" => FIFTEEN_MINUTE_INTERVAL_SECONDS,
        "1h" => ONE_HOUR_INTERVAL_SECONDS,
        "1d" => ONE_DAY_INTERVAL_SECONDS,
        _ => unreachable!(),
    };
    let candles = crate::load_candles_from_source(
        connection,
        symbol,
        interval,
        i64::try_from(history_rows).unwrap_or(i64::MAX),
        source,
    )?;
    Ok(chart_window_needs_repair(
        &candles,
        interval,
        duration_seconds,
        now_ns,
        symbol,
    ))
}

fn chart_window_needs_repair(
    candles: &[Candle],
    timeframe: &str,
    duration_seconds: i64,
    now_ns: i64,
    symbol: &str,
) -> bool {
    if candles.len() < 2 {
        return true;
    }
    let interval_ns = duration_seconds.saturating_mul(NANOS_PER_SECOND);
    let maximum_gap_ns = interval_ns.saturating_mul(MAX_CONTINUOUS_CHART_GAP_INTERVALS);
    if candles.windows(2).any(|pair| {
        let distance = pair[1].time.saturating_sub(pair[0].time);
        distance <= 0 || distance > maximum_gap_ns
    }) {
        return true;
    }
    let freshness_ns = crate::freshness_threshold_seconds(symbol, timeframe)
        .saturating_mul(NANOS_PER_SECOND);
    candles
        .last()
        .is_none_or(|candle| now_ns.saturating_sub(candle.time) > freshness_ns)
}

/// Prefer Binance for crypto references, trying the next eligible research
/// source only when the preferred source is unavailable. Eligible equities
/// and tokenized stocks use Yahoo. Each successful source remains a separately
/// labeled SQLite series.
pub fn sync_hourly_for_exchange(
    connection: &Connection,
    storage_symbol: &str,
    kind: &str,
    exchange: &str,
    product_id: &str,
    yahoo_symbol: &str,
) -> Result<usize, String> {
    sync_interval_for_exchange(
        connection,
        storage_symbol,
        kind,
        exchange,
        product_id,
        yahoo_symbol,
        "1h",
        false,
        None,
    )
}

/// Populate the completed 15-minute history required by registered day-trade
/// profiles. Results remain clearly labeled research-proxy series.
pub fn sync_15m_for_exchange(
    connection: &Connection,
    storage_symbol: &str,
    kind: &str,
    exchange: &str,
    product_id: &str,
    yahoo_symbol: &str,
) -> Result<usize, String> {
    sync_interval_for_exchange(
        connection,
        storage_symbol,
        kind,
        exchange,
        product_id,
        yahoo_symbol,
        "15m",
        false,
        None,
    )
}

/// Fetch and persist the completed daily candle window for one registered app
/// instrument. The app exchange remains distinct from the proxy provider.
pub fn sync_daily_for_exchange(
    connection: &Connection,
    storage_symbol: &str,
    kind: &str,
    exchange: &str,
    product_id: &str,
    yahoo_symbol: &str,
) -> Result<usize, String> {
    sync_interval_for_exchange(
        connection,
        storage_symbol,
        kind,
        exchange,
        product_id,
        yahoo_symbol,
        "1d",
        false,
        None,
    )
}

/// Refresh a chart-selected source series when its stored window has a gap.
pub fn sync_chart_interval_for_exchange(
    connection: &Connection,
    storage_symbol: &str,
    kind: &str,
    exchange: &str,
    product_id: &str,
    yahoo_symbol: &str,
    interval: &str,
    chart_history_rows: usize,
) -> Result<usize, String> {
    sync_interval_for_exchange(
        connection,
        storage_symbol,
        kind,
        exchange,
        product_id,
        yahoo_symbol,
        interval,
        true,
        Some(chart_history_rows),
    )
}

fn recent_proxy_result(
    connection: &Connection,
    symbol: &str,
    exchange: &str,
    product_id: &str,
    source: &str,
    interval: &str,
    duration_seconds: i64,
) -> Option<Result<usize, String>> {
    let refresh_interval_seconds = match interval {
        "15m" => RESEARCH_15M_REFRESH_INTERVAL_SECONDS,
        "1h" | "1d" => duration_seconds,
        _ => return None,
    };
    let (exchange, product_id) = if shared::is_shared_source(source) {
        (shared::SOURCE_EXCHANGE_ID, "")
    } else {
        (exchange, product_id)
    };
    let now = i64::try_from(super::now_secs()).ok()?;
    let result: Option<(
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<i64>,
        i64,
    )> = connection
        .query_row(
            "SELECT s.source,s.data_provider,strftime('%s',replace(s.last_success,' UTC','')),s.last_error,
                strftime('%s',replace(s.last_attempt,' UTC','')),
                (SELECT COUNT(*) FROM bars b WHERE b.symbol=s.symbol AND b.interval=s.interval
                    AND b.source=s.source),
                (SELECT MIN(ts) FROM bars b WHERE b.symbol=s.symbol AND b.interval=s.interval
                    AND b.source=s.source)
           FROM series s WHERE s.symbol=?1 AND s.exchange_id=?2 AND s.product_id=?3
             AND s.source=?4 AND s.interval=?5
          LIMIT 1",
            params![symbol, exchange, product_id, source, interval],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            },
        )
        .optional()
        .ok()
        .flatten();
    let (_source, provider, last_success, last_error, last_attempt, _earliest_ts, rows) = result?;
    let success_epoch = last_success.and_then(|value| value.parse::<i64>().ok());
    let attempt_epoch = last_attempt.and_then(|value| value.parse::<i64>().ok());
    let latest_completed: Option<i64> = connection
        .query_row(
            "SELECT MAX(ts) FROM bars WHERE symbol=?1 AND interval=?2 AND source=?3
            AND close>0 AND ts<=?4",
            params![
                symbol,
                interval,
                source,
                now.saturating_sub(duration_seconds)
                    .saturating_mul(NANOS_PER_SECOND)
            ],
            |row| row.get(0),
        )
        .ok()
        .flatten();
    let fresh = latest_completed.is_some_and(|ts| {
        now.saturating_sub(ts.div_euclid(NANOS_PER_SECOND))
            <= crate::freshness_threshold_seconds(symbol, interval)
    });
    let backfill_required = (provider == "binance" && matches!(interval, "15m" | "1h" | "1d"))
        || (provider == "yfinance"
            && exchange == "stockbit"
            && source.starts_with("stockbit_regular_")
            && matches!(interval, "15m" | "1h" | "1d"));
    let history_complete = if backfill_required {
        provider_history_backfill_complete(connection, symbol, interval, source).ok()?
    } else {
        true
    };
    if rows > 0
        && fresh
        && history_complete
        && last_error.as_deref().is_none_or(str::is_empty)
        && success_epoch.is_some_and(|epoch| now.saturating_sub(epoch) < refresh_interval_seconds)
    {
        return Some(Ok(rows.max(0) as usize));
    }
    if attempt_epoch.is_some_and(|epoch| now.saturating_sub(epoch) < 300) {
        if let Some(error) = last_error.filter(|error| !error.is_empty()) {
            return Some(Err(format!(
                "recent {exchange} research-provider attempt failed: {error}"
            )));
        }
    }
    None
}

fn provider_history_backfill_complete(
    connection: &Connection,
    symbol: &str,
    interval: &str,
    source: &str,
) -> Result<bool, rusqlite::Error> {
    connection
        .query_row(
            "SELECT status='complete' FROM ingestion_runs
              WHERE symbol=?1 AND interval=?2 AND source=?3
              ORDER BY id DESC LIMIT 1",
            params![symbol, format!("{interval}_history_backfill"), source],
            |row| row.get(0),
        )
        .optional()
        .map(|complete| complete.unwrap_or(false))
}

fn record_history_backfill_state(
    connection: &Connection,
    symbol: &str,
    interval: &str,
    exchange: &str,
    product_id: &str,
    provider: &str,
    source: &str,
    status: &str,
    rows_received: usize,
    error: &str,
) {
    let attempted_at = super::format_timestamp(super::now_secs());
    let marker_interval = format!("{interval}_history_backfill");
    let (recorded_exchange, recorded_product) = if shared::is_shared_source(source) {
        (shared::SOURCE_EXCHANGE_ID, "")
    } else {
        (exchange, product_id)
    };
    let updated = connection
        .execute(
            "UPDATE ingestion_runs SET exchange_id=?1,product_id=?2,data_provider=?3,
                attempted_at=?4,status=?5,rows_received=?6,error=?7
              WHERE id=(SELECT id FROM ingestion_runs WHERE symbol=?8 AND interval=?9
                AND source=?10 ORDER BY id DESC LIMIT 1)",
            params![
                recorded_exchange,
                recorded_product,
                provider,
                attempted_at,
                status,
                rows_received as i64,
                if error.is_empty() { None } else { Some(error) },
                symbol,
                marker_interval,
                source
            ],
        )
        .unwrap_or(0);
    if updated == 0 {
        let _ = connection.execute(
            "INSERT INTO ingestion_runs(symbol,interval,source,exchange_id,product_id,data_provider,
                attempted_at,status,rows_received,error)
              VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![
                symbol,
                marker_interval,
                source,
                recorded_exchange,
                recorded_product,
                provider,
                attempted_at,
                status,
                rows_received as i64,
                if error.is_empty() { None } else { Some(error) }
            ],
        );
    }
}

fn sync_with_binance(
    connection: &Connection,
    storage_symbol: &str,
    kind: &str,
    exchange: &str,
    pair: &str,
    market: BinanceMarket,
    source: &str,
    interval: &str,
    duration_seconds: i64,
    maximum_rows: usize,
    chart_repair_rows: usize,
) -> Result<usize, String> {
    // Candle timestamps are the durable cursor, scoped to the exact source series.
    // Replay the last closed candle so provider corrections remain idempotent.
    let (mut earliest_ts, mut latest_ts): (Option<i64>, Option<i64>) = connection
        .query_row(
            "SELECT MIN(ts),MAX(ts) FROM bars WHERE symbol=?1 AND interval=?2 AND source=?3",
            params![storage_symbol, interval, source],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|error| error.to_string())?;
    let now_seconds = i64::try_from(super::now_secs()).unwrap_or(i64::MAX);
    let recent_window_needs_repair = recent_chart_window_needs_repair_with_rows(
        connection,
        storage_symbol,
        interval,
        source,
        chart_repair_rows,
        now_seconds.saturating_mul(NANOS_PER_SECOND),
    )
    .map_err(|error| format!("chart history check failed: {error}"))?;
    let mut repaired_rows = 0;
    if recent_window_needs_repair {
        let repair_window_seconds = duration_seconds.saturating_mul(chart_repair_rows as i64);
        let recent_start_seconds = now_seconds.saturating_sub(repair_window_seconds);
        let save_page = |candles: &[Bar]| {
            persist(
                connection,
                storage_symbol,
                kind,
                exchange,
                "binance",
                source,
                pair,
                interval,
                "binance_usdt_reference",
                candles,
            )
        };
        let result = if chart_repair_rows > RECENT_CHART_HISTORY_ROWS {
            fetch_binance_chart_repair_interval(
                pair,
                market,
                interval,
                duration_seconds,
                chart_repair_rows,
                Some(recent_start_seconds),
                save_page,
            )
        } else {
            fetch_binance_interval(
                pair,
                market,
                interval,
                duration_seconds,
                chart_repair_rows,
                Some(recent_start_seconds),
                save_page,
            )
        };
        match result {
            Ok(rows) => repaired_rows = rows,
            Err(error) => {
                let now = super::format_timestamp(super::now_secs());
                record_proxy_failure(
                    connection,
                    storage_symbol,
                    kind,
                    exchange,
                    "binance",
                    source,
                    interval,
                    &now,
                    &error,
                );
                return Err(error);
            }
        }
        (earliest_ts, latest_ts) = connection
            .query_row(
                "SELECT MIN(ts),MAX(ts) FROM bars WHERE symbol=?1 AND interval=?2 AND source=?3",
                params![storage_symbol, interval, source],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|error| error.to_string())?;
    }
    let history_complete = provider_history_backfill_complete(
        connection,
        storage_symbol,
        interval,
        source,
    )
    .unwrap_or(false);
    if let Some(earliest_ts) = earliest_ts.filter(|_| !history_complete) {
        match fetch_binance_history_page(
            pair,
            market,
            interval,
            duration_seconds,
            earliest_ts.div_euclid(NANOS_PER_SECOND),
            |candles| {
                persist(
                    connection,
                    storage_symbol,
                    kind,
                    exchange,
                    "binance",
                    source,
                    pair,
                    interval,
                    "binance_usdt_reference",
                    candles,
                )
            },
        ) {
            Ok(rows) => {
                record_history_backfill_state(
                    connection,
                    storage_symbol,
                    interval,
                    exchange,
                    app_product_from_source(source).unwrap_or(""),
                    "binance",
                    source,
                    if rows == 0 { "complete" } else { "running" },
                    rows,
                    "",
                );
            }
            Err(error) => {
                record_history_backfill_state(
                    connection,
                    storage_symbol,
                    interval,
                    exchange,
                    app_product_from_source(source).unwrap_or(""),
                    "binance",
                    source,
                    "error",
                    0,
                    &error,
                );
                eprintln!(
                    "[ResearchProxy] Binance history backfill deferred for {storage_symbol} {interval}: {error}"
                );
            }
        }
    }

    // The latest completed candle remains a separate, small incremental read.
    // Historical pages advance backward from MIN(ts) and never replace recent
    // market refreshes with a multi-year bulk download.
    let resume_ts = latest_ts.map(|ts| ts.div_euclid(NANOS_PER_SECOND));
    let result = fetch_binance_interval(
        pair,
        market,
        interval,
        duration_seconds,
        maximum_rows,
        resume_ts,
        |candles| {
            persist(
                connection,
                storage_symbol,
                kind,
                exchange,
                "binance",
                source,
                pair,
                interval,
                "binance_usdt_reference",
                candles,
            )
        },
    );
    if let Err(error) = &result {
        let now = super::format_timestamp(super::now_secs());
        record_proxy_failure(
            connection,
            storage_symbol,
            kind,
            exchange,
            "binance",
            source,
            interval,
            &now,
            error,
        );
        if repaired_rows > 0 {
            return Ok(repaired_rows);
        }
    }
    result
}

fn sync_with_yahoo(
    connection: &Connection,
    storage_symbol: &str,
    kind: &str,
    exchange: &str,
    yahoo_symbol: &str,
    source: &str,
    interval: &str,
    duration_seconds: i64,
    maximum_rows: usize,
) -> Result<usize, String> {
    let mut history_error = None;
    let has_stored_history: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM bars WHERE symbol=?1 AND interval=?2 AND source=?3)",
            params![storage_symbol, interval, source],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if kind == "idx_stock"
        && exchange == "stockbit"
        && source.starts_with("stockbit_regular_")
        && matches!(interval, "15m" | "1h" | "1d")
    {
        let earliest_ts: Option<i64> = connection
            .query_row(
                "SELECT MIN(ts) FROM bars WHERE symbol=?1 AND interval=?2 AND source=?3",
                params![storage_symbol, interval, source],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        let complete =
            provider_history_backfill_complete(connection, storage_symbol, interval, source)
                .unwrap_or(false);
        if let Some(earliest_ts) = earliest_ts.filter(|_| !complete) {
            match fetch_yahoo_history_page(
                yahoo_symbol,
                interval,
                duration_seconds,
                earliest_ts.div_euclid(NANOS_PER_SECOND),
                i64::try_from(super::now_secs()).unwrap_or(i64::MAX),
            ) {
                Ok((bars, reached_provider_start)) => {
                    let rows = bars.len();
                    if !bars.is_empty() {
                        if let Err(error) = persist(
                            connection,
                            storage_symbol,
                            kind,
                            exchange,
                            "yfinance",
                            source,
                            yahoo_symbol,
                            interval,
                            "yahoo_unadjusted",
                            &bars,
                        ) {
                            history_error = Some(error);
                        }
                    }
                    if history_error.is_none() {
                        record_history_backfill_state(
                            connection,
                            storage_symbol,
                            interval,
                            exchange,
                            app_product_from_source(source).unwrap_or(""),
                            "yfinance",
                            source,
                            if reached_provider_start || rows == 0 {
                                "complete"
                            } else {
                                "running"
                            },
                            rows,
                            "",
                        );
                    }
                }
                Err(error) => {
                    record_history_backfill_state(
                        connection,
                        storage_symbol,
                        interval,
                        exchange,
                        app_product_from_source(source).unwrap_or(""),
                        "yfinance",
                        source,
                        "error",
                        0,
                        &error,
                    );
                    history_error = Some(error);
                }
            }
        }
    }

    let result = fetch_yahoo_interval(
        yahoo_symbol,
        kind,
        interval,
        duration_seconds,
        maximum_rows,
        has_stored_history,
    )
    .and_then(|bars| {
        persist(
            connection,
            storage_symbol,
            kind,
            exchange,
            "yfinance",
            source,
            yahoo_symbol,
            interval,
            "yahoo_unadjusted",
            &bars,
        )?;
        Ok(bars.len())
    });
    if let Err(error) = &result {
        let now = super::format_timestamp(super::now_secs());
        record_proxy_failure(
            connection,
            storage_symbol,
            kind,
            exchange,
            "yfinance",
            source,
            interval,
            &now,
            error,
        );
    }
    if let Some(error) = history_error {
        eprintln!(
            "[ResearchProxy] Yahoo history backfill deferred for {storage_symbol} {interval}: {error}"
        );
    }
    result
}

fn sync_with_research_adapter(
    connection: &Connection,
    storage_symbol: &str,
    kind: &str,
    exchange: &str,
    yahoo_symbol: &str,
    source: &str,
    interval: &str,
    duration_seconds: i64,
    maximum_rows: usize,
    adapter: sources::ResearchAdapter,
) -> Result<usize, String> {
    let provider = adapter.provider_id();
    let result = yahoo_base_asset(yahoo_symbol)
        .and_then(|base| adapter.fetch(&base, interval, duration_seconds, maximum_rows))
        .and_then(|(provider_symbol, price_basis, bars)| {
            persist(
                connection,
                storage_symbol,
                kind,
                exchange,
                provider,
                source,
                &provider_symbol,
                interval,
                price_basis,
                &bars,
            )?;
            Ok(bars.len())
        });
    if let Err(error) = &result {
        let now = super::format_timestamp(super::now_secs());
        record_proxy_failure(
            connection,
            storage_symbol,
            kind,
            exchange,
            provider,
            source,
            interval,
            &now,
            error,
        );
    }
    result
}

fn record_proxy_failure(
    connection: &Connection,
    symbol: &str,
    kind: &str,
    exchange: &str,
    provider: &str,
    source: &str,
    interval: &str,
    now: &str,
    error: &str,
) {
    let (exchange, product_id) = if shared::is_shared_source(source) {
        (shared::SOURCE_EXCHANGE_ID, "")
    } else {
        (exchange, app_product_from_source(source).unwrap_or(""))
    };
    let _ = connection.execute(
        "INSERT INTO ingestion_runs(symbol,interval,source,exchange_id,product_id,data_provider,attempted_at,status,rows_received,error) VALUES (?1,?2,?3,?4,?5,?6,?7,'error',0,?8)",
        params![symbol, interval, source, exchange, product_id, provider, now, error],
    );
    let _ = connection.execute(
        "INSERT INTO series(symbol,interval,source,exchange_id,product_id,data_provider,
            timezone,price_basis,instrument_kind,last_attempt,last_error)
         VALUES(?1,?2,?3,?4,?5,?6,?7,'research_proxy',?8,?9,?10)
         ON CONFLICT(symbol,interval,source) DO UPDATE SET exchange_id=excluded.exchange_id,
            product_id=excluded.product_id,data_provider=excluded.data_provider,
            last_attempt=excluded.last_attempt,last_error=excluded.last_error",
        params![
            symbol,
            interval,
            source,
            exchange,
            product_id,
            provider,
            if kind == "idx_stock" {
                "Asia/Jakarta"
            } else {
                "UTC"
            },
            kind,
            now,
            error
        ],
    );
}

fn app_product_from_source(source: &str) -> Option<&'static str> {
    [
        "stockbit_regular",
        "tokocrypto_spot",
        "tokocrypto_tokenized_stock",
        "pintu_retail_spot",
        "pintu_pro_spot",
        "pintu_futures",
        "pluang_spot",
        "pluang_perpetual",
    ]
    .into_iter()
    .find(|product| source == *product || source.starts_with(&format!("{product}_")))
    .map(|product| {
        if product == "pintu_futures" {
            "pintu_pro_futures"
        } else {
            product
        }
    })
}

pub(in crate::research_proxy) fn persist(
    connection: &Connection,
    symbol: &str,
    kind: &str,
    exchange: &str,
    provider: &str,
    source: &str,
    request_symbol: &str,
    interval: &str,
    price_basis: &str,
    bars: &[Bar],
) -> Result<(), String> {
    crate::database::retry_sqlite_busy_message(|| {
        persist_once(
            connection,
            symbol,
            kind,
            exchange,
            provider,
            source,
            request_symbol,
            interval,
            price_basis,
            bars,
        )
    })
}

fn persist_once(
    connection: &Connection,
    symbol: &str,
    kind: &str,
    exchange: &str,
    provider: &str,
    source: &str,
    request_symbol: &str,
    interval: &str,
    price_basis: &str,
    bars: &[Bar],
) -> Result<(), String> {
    let now = super::format_timestamp(super::now_secs());
    let timezone = if request_symbol.ends_with(".JK") {
        "Asia/Jakarta"
    } else {
        "UTC"
    };
    let product_id = app_product_from_source(source).unwrap_or("");
    let app_id = if shared::is_shared_source(source) {
        shared::SOURCE_EXCHANGE_ID
    } else if exchange == "pintu_futures" {
        "pintu"
    } else {
        exchange
    };
    let instrument_id: Option<String> = if product_id.is_empty() {
        None
    } else {
        connection
            .query_row(
                "SELECT instrument_id FROM instruments WHERE exchange_id=?1 AND product_id=?2
               AND (storage_symbol=?3 OR symbol=?3) LIMIT 1",
                params![app_id, product_id, symbol],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?
    };
    // This connection also attaches the shared app store. Reserve only the
    // ticker database on its first write, rather than every attached database.
    connection
        .execute_batch("BEGIN DEFERRED")
        .map_err(|e| e.to_string())?;
    let result = (|| {
        connection.execute(
            "INSERT INTO main.series(symbol,interval,source,exchange_id,product_id,instrument_id,data_provider,timezone,price_basis,instrument_kind,last_success,last_attempt,last_error)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?11,NULL)
             ON CONFLICT(symbol,interval,source) DO UPDATE SET
                exchange_id=excluded.exchange_id,product_id=excluded.product_id,
                instrument_id=COALESCE(excluded.instrument_id,series.instrument_id),
                data_provider=excluded.data_provider,
                timezone=excluded.timezone,price_basis=excluded.price_basis,
                instrument_kind=excluded.instrument_kind,last_success=excluded.last_success,
                last_attempt=excluded.last_attempt,last_error=NULL",
            params![symbol,interval,source,app_id,product_id,instrument_id,provider,timezone,price_basis,kind,now],
        ).map_err(|e| e.to_string())?;
        for bar in bars {
            let ts = bar
                .ts
                .checked_mul(NANOS_PER_SECOND)
                .ok_or("market-data timestamp overflow")?;
            connection.execute(
                "INSERT INTO main.bars(symbol,interval,source,ts,open,high,low,close,volume,ingested_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)
                 ON CONFLICT(symbol,interval,source,ts) DO UPDATE SET
                    open=excluded.open,high=excluded.high,low=excluded.low,
                    close=excluded.close,volume=excluded.volume,ingested_at=excluded.ingested_at",
                params![symbol, interval, source, ts, bar.open, bar.high, bar.low, bar.close, bar.volume, now],
            ).map_err(|e| e.to_string())?;
        }
        connection.execute(
            "INSERT INTO main.ingestion_runs(symbol,interval,source,exchange_id,product_id,data_provider,attempted_at,status,rows_received,error)
             VALUES (?1,?2,?3,?4,?5,?6,?7,'success',?8,NULL)",
            params![symbol,interval,source,app_id,product_id,provider,now,bars.len() as i64],
        ).map_err(|e| e.to_string())?;
        Ok::<(), String>(())
    })();
    let result = result.and_then(|()| {
        connection
            .execute_batch("COMMIT")
            .map_err(|error| error.to_string())
    });
    if result.is_err() {
        let _ = connection.execute_batch("ROLLBACK");
    }
    result
}

pub(crate) fn background_binance_kline_stream_loop(market_directory: String) {
    binance_stream::run_loop(market_directory);
}

#[cfg(test)]
#[path = "research_proxy_tests.rs"]
mod recovery_tests;

#[cfg(test)]
#[path = "research_proxy_persistence_tests.rs"]
mod persistence_tests;

#[cfg(test)]
#[path = "research_proxy_chart_gap_tests.rs"]
mod chart_gap_tests;
