//! Bounded public-market-data and Yahoo chart reads for research-only candles.
//! The selected app exchange stays attached to the registered instrument while
//! data_provider identifies the remote source that supplied the candle.
use crate::NANOS_PER_SECOND;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;

#[path = "research_proxy/interval.rs"]
mod interval;
#[path = "research_proxy_parsing.rs"]
mod parsing;
#[path = "research_proxy/providers.rs"]
mod providers;
#[path = "research_proxy_sources.rs"]
mod sources;
mod transport;
use interval::sync_interval_for_exchange;
#[cfg(test)]
use providers::yahoo_request_window;
use providers::{
    fetch_binance_interval, fetch_chart_interval, fetch_yahoo_interval, yahoo_base_asset, Bar,
    BinanceMarket,
};
#[path = "research_proxy_warmup.rs"]
mod warmup;
pub(crate) use transport::fetch_bounded_url;

use parsing::{
    parse_binance_candles, parse_yahoo_candles, validate_bar, FIFTEEN_MINUTE_INTERVAL_SECONDS,
    ONE_DAY_INTERVAL_SECONDS, ONE_HOUR_INTERVAL_SECONDS,
};

const MAX_BINANCE_PAGES: usize = 4;
const MAX_RESEARCH_15M_CANDLES: usize = 1_000;
const RESEARCH_15M_REFRESH_INTERVAL_SECONDS: i64 = 10 * 60;

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
        "1h" => duration_seconds,
        _ => return None,
    };
    let now = i64::try_from(super::now_secs()).ok()?;
    let result: Option<(String, Option<String>, Option<String>, Option<String>, i64)> = connection
        .query_row(
            "SELECT s.source,strftime('%s',replace(s.last_success,' UTC','')),s.last_error,
                strftime('%s',replace(s.last_attempt,' UTC','')),
                (SELECT COUNT(*) FROM bars b WHERE b.symbol=s.symbol AND b.interval=s.interval
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
                ))
            },
        )
        .optional()
        .ok()
        .flatten();
    let (_source, last_success, last_error, last_attempt, rows) = result?;
    let success_epoch = last_success.and_then(|value| value.parse::<i64>().ok());
    let attempt_epoch = last_attempt.and_then(|value| value.parse::<i64>().ok());
    if success_epoch.is_some_and(|epoch| now.saturating_sub(epoch) < refresh_interval_seconds) {
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
) -> Result<usize, String> {
    let result = fetch_binance_interval(pair, market, interval, duration_seconds, maximum_rows)
        .and_then(|candles| {
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
                &candles,
            )?;
            Ok(candles.len())
        });
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
    let result = fetch_yahoo_interval(yahoo_symbol, kind, interval, duration_seconds, maximum_rows)
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
    let product_id = app_product_from_source(source).unwrap_or("");
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

fn persist(
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
    let app_id = if exchange == "pintu_futures" {
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
    connection
        .execute_batch("BEGIN IMMEDIATE")
        .map_err(|e| e.to_string())?;
    let result = (|| {
        connection.execute(
            "INSERT INTO series(symbol,interval,source,exchange_id,product_id,instrument_id,data_provider,timezone,price_basis,instrument_kind,last_success,last_attempt,last_error)
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
                "INSERT INTO bars(symbol,interval,source,ts,open,high,low,close,volume,ingested_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)
                 ON CONFLICT(symbol,interval,source,ts) DO UPDATE SET
                    open=excluded.open,high=excluded.high,low=excluded.low,
                    close=excluded.close,volume=excluded.volume,ingested_at=excluded.ingested_at",
                params![symbol, interval, source, ts, bar.open, bar.high, bar.low, bar.close, bar.volume, now],
            ).map_err(|e| e.to_string())?;
        }
        connection.execute(
            "INSERT INTO ingestion_runs(symbol,interval,source,exchange_id,product_id,data_provider,attempted_at,status,rows_received,error)
             VALUES (?1,?2,?3,?4,?5,?6,?7,'success',?8,NULL)",
            params![symbol,interval,source,app_id,product_id,provider,now,bars.len() as i64],
        ).map_err(|e| e.to_string())?;
        Ok::<(), String>(())
    })();
    match result {
        Ok(()) => connection
            .execute_batch("COMMIT")
            .map_err(|e| e.to_string()),
        Err(error) => {
            let _ = connection.execute_batch("ROLLBACK");
            Err(error)
        }
    }
}

#[cfg(test)]
#[path = "research_proxy_tests.rs"]
mod recovery_tests;

#[cfg(test)]
#[path = "research_proxy_persistence_tests.rs"]
mod persistence_tests;
