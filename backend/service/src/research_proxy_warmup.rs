use super::*;

const STOCKBIT_4H_SOURCE: &str = "stockbit_regular_yfinance_fallback";
const STOCKBIT_4H_WARMUP_INTERVAL: &str = "1h_history_for_4h";
const STOCKBIT_4H_HISTORY_RANGE: &str = "2y";
const MAX_STOCKBIT_4H_HISTORY_RESPONSE_ROWS: usize = 4_000;
const STOCKBIT_4H_WARMUP_FAILURE_RETRY_SECONDS: i64 = quant_core::SECONDS_PER_HOUR;
const STOCKBIT_4H_WARMUP_INSUFFICIENT_RETRY_SECONDS: i64 = 30 * quant_core::SECONDS_PER_DAY;
const STOCKBIT_4H_WARMUP_ERROR_PREFIX: &str = "Stockbit 4h Envelope history warm-up failed:";
const STOCKBIT_4H_HISTORY_SHORT_PREFIX: &str = "Stockbit 4h Envelope history is insufficient:";

static STOCKBIT_4H_HISTORY_GATE: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub(super) fn sync_stockbit_4h_history_if_required(
    connection: &Connection,
    storage_symbol: &str,
    kind: &str,
    exchange: &str,
    product_id: &str,
    yahoo_symbol: &str,
    interval: &str,
) -> Option<Result<usize, String>> {
    if kind != "idx_stock"
        || exchange != "stockbit"
        || product_id != quant_core::STOCKBIT_NWE_4H_PRODUCT_ID
        || interval != "1h"
        || !yahoo_symbol.ends_with(".JK")
    {
        return None;
    }

    let required_bars = quant_core::stockbit_nwe_4h_minimum_history_bars();
    let required_limit = match i64::try_from(required_bars) {
        Ok(limit) => limit,
        Err(_) => return Some(Err("Stockbit 4h history limit exceeds its bound".into())),
    };
    let current_4h_bars = match super::super::load_series_candles_from_source(
        connection,
        storage_symbol,
        "4h",
        required_limit,
        STOCKBIT_4H_SOURCE,
    ) {
        Ok(bars) => bars.len(),
        Err(error) => return Some(Err(format!("Stockbit 4h history check failed: {error}"))),
    };
    if current_4h_bars >= required_bars || warmup_cooldown_active(connection, storage_symbol) {
        return None;
    }

    Some(sync_stockbit_4h_history(
        connection,
        storage_symbol,
        kind,
        exchange,
        yahoo_symbol,
        required_bars,
        required_limit,
    ))
}

fn sync_stockbit_4h_history(
    connection: &Connection,
    storage_symbol: &str,
    kind: &str,
    exchange: &str,
    yahoo_symbol: &str,
    required_bars: usize,
    required_limit: i64,
) -> Result<usize, String> {
    let _history_guard = STOCKBIT_4H_HISTORY_GATE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let result = fetch_stockbit_4h_history(yahoo_symbol).and_then(|bars| {
        let received = bars.len();
        super::persist(
            connection,
            storage_symbol,
            kind,
            exchange,
            "yfinance",
            STOCKBIT_4H_SOURCE,
            yahoo_symbol,
            "1h",
            "yahoo_unadjusted",
            &bars,
        )?;
        let derived = super::super::load_series_candles_from_source(
            connection,
            storage_symbol,
            "4h",
            required_limit,
            STOCKBIT_4H_SOURCE,
        )
        .map_err(|error| format!("Stockbit 4h history validation failed: {error}"))?;
        if derived.len() < required_bars {
            let error = format!(
                "{STOCKBIT_4H_HISTORY_SHORT_PREFIX} {} completed 4h bars are available; {required_bars} are required",
                derived.len()
            );
            record_insufficient_history(connection, storage_symbol, received, &error)?;
        }
        Ok(received)
    });

    if let Err(error) = &result {
        let now = super::super::format_timestamp(super::super::now_secs());
        super::record_proxy_failure(
            connection,
            storage_symbol,
            kind,
            exchange,
            "yfinance",
            STOCKBIT_4H_SOURCE,
            "1h",
            &now,
            &format!("{STOCKBIT_4H_WARMUP_ERROR_PREFIX} {error}"),
        );
    }
    result
}

fn fetch_stockbit_4h_history(yahoo_symbol: &str) -> Result<Vec<Bar>, String> {
    // The provider response is bounded; retain all accepted candles, including
    // history beyond the current strategy's transient aggregation window.
    super::fetch_chart_interval(
        yahoo_symbol,
        "60m",
        STOCKBIT_4H_HISTORY_RANGE,
        super::ONE_HOUR_INTERVAL_SECONDS,
        MAX_STOCKBIT_4H_HISTORY_RESPONSE_ROWS,
    )
}

fn record_insufficient_history(
    connection: &Connection,
    storage_symbol: &str,
    rows_received: usize,
    error: &str,
) -> Result<(), String> {
    let now = super::super::format_timestamp(super::super::now_secs());
    connection
        .execute(
            "INSERT INTO ingestion_runs(symbol,interval,source,exchange_id,product_id,data_provider,attempted_at,status,rows_received,error)
             VALUES (?1,?2,?3,'stockbit','stockbit_regular','yfinance',?4,'insufficient_history',?5,?6)",
            params![
                storage_symbol,
                STOCKBIT_4H_WARMUP_INTERVAL,
                STOCKBIT_4H_SOURCE,
                now,
                i64::try_from(rows_received).unwrap_or(i64::MAX),
                error
            ],
        )
        .map_err(|db_error| db_error.to_string())?;
    Ok(())
}

fn warmup_cooldown_active(connection: &Connection, storage_symbol: &str) -> bool {
    let insufficient_attempt: Option<Option<i64>> = connection
        .query_row(
            "SELECT CAST(strftime('%s',replace(attempted_at,' UTC','')) AS INTEGER)
               FROM ingestion_runs
              WHERE symbol=?1 AND interval=?2 AND source=?3
                AND status='insufficient_history'
              ORDER BY id DESC LIMIT 1",
            params![
                storage_symbol,
                STOCKBIT_4H_WARMUP_INTERVAL,
                STOCKBIT_4H_SOURCE
            ],
            |row| row.get(0),
        )
        .optional()
        .ok()
        .flatten();
    let failure: Option<Option<i64>> = connection
        .query_row(
            "SELECT CAST(strftime('%s',replace(attempted_at,' UTC','')) AS INTEGER)
               FROM ingestion_runs
              WHERE symbol=?1 AND interval='1h' AND source=?2 AND status='error'
                AND error LIKE ?3
              ORDER BY id DESC LIMIT 1",
            params![
                storage_symbol,
                STOCKBIT_4H_SOURCE,
                format!("{STOCKBIT_4H_WARMUP_ERROR_PREFIX}%")
            ],
            |row| row.get(0),
        )
        .optional()
        .ok()
        .flatten();
    let latest_hourly_attempt: Option<(Option<i64>, String)> = connection
        .query_row(
            "SELECT CAST(strftime('%s',replace(attempted_at,' UTC','')) AS INTEGER),status
               FROM ingestion_runs
              WHERE symbol=?1 AND interval='1h' AND source=?2
              ORDER BY id DESC LIMIT 1",
            params![storage_symbol, STOCKBIT_4H_SOURCE],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .ok()
        .flatten();
    let now = i64::try_from(super::super::now_secs()).unwrap_or(i64::MAX);
    let insufficient_is_active = insufficient_attempt.flatten().is_some_and(|attempted_at| {
        now.saturating_sub(attempted_at) < STOCKBIT_4H_WARMUP_INSUFFICIENT_RETRY_SECONDS
    });
    let failure_is_active = failure.flatten().is_some_and(|failed_at| {
        let provider_recovered =
            latest_hourly_attempt
                .as_ref()
                .is_some_and(|(attempted_at, status)| {
                    status == "success"
                        && attempted_at.is_some_and(|success_at| success_at > failed_at)
                });
        now.saturating_sub(failed_at) < STOCKBIT_4H_WARMUP_FAILURE_RETRY_SECONDS
            && !provider_recovered
    });
    insufficient_is_active || failure_is_active
}

#[cfg(test)]
#[path = "research_proxy_warmup_tests.rs"]
mod tests;
