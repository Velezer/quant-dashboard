use super::*;
mod adapters;
mod contract;
use adapters::{PINTU_NATIVE_ADAPTER, RESEARCH_PROXY_ADAPTER, TOKOCRYPTO_NATIVE_ADAPTER};
use contract::{
    should_continue_candle_provider_chain, CandleProviderCapabilities, CandleProviderFailure,
    CandleProviderReport, CandleSourceClass, CandleSyncAdapter, CandleSyncRequest,
    CandleSyncWindow, PreparedCandleSyncTarget, PreparedTokocryptoMarket,
    RegisteredCandleInstrument, RegisteredTokocryptoMarket,
};

// This is the explicit source policy: native data is attempted first, then a
// separately labeled research proxy where the registered instrument permits it.
static CANDLE_SYNC_ADAPTERS: [&dyn CandleSyncAdapter; 3] = [
    &TOKOCRYPTO_NATIVE_ADAPTER,
    &PINTU_NATIVE_ADAPTER,
    &RESEARCH_PROXY_ADAPTER,
];

#[path = "market_sync_targets.rs"]
mod targets;
use targets::{candle_sync_target_limit, registered_hourly_target_count};
pub(super) use targets::{registered_daily_targets, registered_hourly_targets};

const MAX_REGISTERED_CANDLE_TARGETS_PER_TICK: usize = 8;
const MAX_DAILY_REGISTERED_CANDLE_TARGETS_PER_TICK: i64 = 12;
// Match the existing regular history lease allowance while preserving the
// two scan leases and eleven-connection cap.
const MAX_PARALLEL_CANDLE_SYNC_WORKERS: usize = 9;
const DAILY_CANDLE_SYNC_MINIMUM_INTERVAL_SECONDS: u64 = 5 * 60;
const CANDLE_SYNC_WORKER_SQLITE_PAGE_CACHE_KIB: i64 = 64;
const CANDLE_SYNC_WAVE_PAUSE_MILLISECONDS: u64 = 250;
const REGATHER_BATCH_PAUSE_SECONDS: u64 = 1;
const STALE_DAILY_REFRESH_COOLDOWN_SECONDS: i64 = quant_core::SECONDS_PER_HOUR;

fn candle_sync_worker_count(target_count: usize) -> usize {
    target_count.min(MAX_PARALLEL_CANDLE_SYNC_WORKERS)
}

fn daily_candle_sync_due(now: u64, last_sync: u64) -> bool {
    last_sync == 0 || now.saturating_sub(last_sync) >= DAILY_CANDLE_SYNC_MINIMUM_INTERVAL_SECONDS
}

fn candle_instrument<'a>(
    exchange_id: &'a str,
    storage_symbol: &'a str,
    product_id: &'a str,
    underlying_symbol: &'a str,
    asset_category_id: &'a str,
    tokocrypto_market: Option<RegisteredTokocryptoMarket<'a>>,
) -> RegisteredCandleInstrument<'a> {
    RegisteredCandleInstrument {
        exchange_id,
        storage_symbol,
        product_id,
        underlying_symbol,
        asset_category_id,
        tokocrypto_market,
    }
}

fn record_provider_scheduler_attempt(
    connection: &Connection,
    instrument: RegisteredCandleInstrument<'_>,
    provider_id: &str,
    succeeded: bool,
    error: &str,
) {
    let attempted_at = format_timestamp(now_secs());
    let source = format!("{provider_id}_native");
    let _ = connection.execute(
        "INSERT INTO ingestion_runs(symbol,interval,source,exchange_id,data_provider,attempted_at,status,rows_received,error) VALUES (?1,'scheduler',?2,?3,?4,?5,?6,0,?7)",
        params![
            instrument.storage_symbol,
            source,
            instrument.exchange_id,
            provider_id,
            attempted_at,
            if succeeded { "success" } else { "error" },
            if error.is_empty() { None } else { Some(error) },
        ],
    );
}

fn record_hourly_instrument_attempt(
    connection: &Connection,
    instrument: RegisteredCandleInstrument<'_>,
) {
    let attempted_at = format_timestamp(now_secs());
    let source = format!("hourly_scheduler_{}", instrument.product_id);
    let _ = connection.execute(
        "INSERT INTO ingestion_runs(symbol,interval,source,exchange_id,product_id,data_provider,attempted_at,status,rows_received,error)
         VALUES(?1,'scheduler',?2,?3,?4,'scheduler',?5,'attempted',0,NULL)",
        params![
            instrument.storage_symbol,
            source,
            instrument.exchange_id,
            instrument.product_id,
            attempted_at,
        ],
    );
    record_app_sync_cursor(
        connection,
        instrument.exchange_id,
        instrument.product_id,
        instrument.storage_symbol,
        "hourly",
        "attempted",
        "",
    );
}

/// Select registered instruments independently of exchange, then dispatch the
/// native feed first and a bounded international-exchange/Yahoo research proxy
/// when a native feed is unavailable. The viewed chart goes first; other
/// symbols rotate by last attempt.
fn sync_hourly_chart_targets(
    connection: &Connection,
    market_database: &str,
    priority_symbol: Option<&str>,
    sync_state: Option<&AppState>,
    storage_symbol_gate: &targets::StorageSymbolSyncGate,
) -> Result<usize, rusqlite::Error> {
    let batch_started = std::time::Instant::now();
    if !provider_sync_enabled() {
        return Ok(0);
    }
    let registered_count = registered_hourly_target_count(connection)?;
    let target_limit = candle_sync_target_limit(registered_count);
    if target_limit == 0 {
        return Ok(0);
    }
    let stale_priorities = sync_state
        .map(|state| state.take_stale_signal_sync_priorities("15m", target_limit))
        .unwrap_or_default();
    if !stale_priorities.is_empty() {
        eprintln!(
            "[Daemon] Prioritizing {} stale 15m signal prices in the next market batch.",
            stale_priorities.len()
        );
    }
    let selected_targets = registered_hourly_targets(
        connection,
        priority_symbol,
        &stale_priorities,
        target_limit as i64,
    )?;
    let mut pending_targets = selected_targets
        .into_iter()
        .collect::<std::collections::VecDeque<_>>();
    let mut attempted_targets = 0usize;
    let mut refreshed = 0usize;
    let mut worker_count = 0usize;
    while !pending_targets.is_empty() {
        let wave_limit = pending_targets.len().min(MAX_REGISTERED_CANDLE_TARGETS_PER_TICK);
        let target_rows = pending_targets.drain(..wave_limit).collect::<Vec<_>>();
        let target_rows = targets::rank_intraday_targets_by_recent_volume(
            target_rows,
            market_database,
            priority_symbol,
            &stale_priorities,
            now_millis().saturating_mul(quant_core::NANOS_PER_MILLISECOND),
        );
        let targets = target_rows
            .into_iter()
            .map(|(
                exchange_id,
                storage_symbol,
                product_id,
                underlying_symbol,
                asset_category_id,
            )| -> Result<PreparedCandleSyncTarget, rusqlite::Error> {
                let shared_reference_symbols = targets::shared_reference_storage_symbols(
                    connection,
                    &exchange_id,
                    &storage_symbol,
                    &underlying_symbol,
                    &asset_category_id,
                )?;
                let tokocrypto_market = if exchange_id == "tokocrypto" {
                    match instrument_registry::lookup_tokocrypto_market(
                        connection,
                        &product_id,
                        &storage_symbol,
                    ) {
                        Ok(Some((venue_symbol, quote_currency, contract_metadata_json))) => {
                            Some(PreparedTokocryptoMarket {
                                venue_symbol,
                                quote_currency,
                                contract_metadata_json,
                            })
                        }
                        Ok(None) => None,
                        Err(error) => {
                            eprintln!(
                                "[Daemon] Tokocrypto registration unavailable for {storage_symbol}: {error}"
                            );
                            None
                        }
                    }
                } else {
                    None
                };
                Ok(PreparedCandleSyncTarget {
                    exchange_id,
                    storage_symbol,
                    product_id,
                    underlying_symbol,
                    asset_category_id,
                    shared_reference_symbols,
                    tokocrypto_market,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let wave_size = targets.len();
        let (wave_refreshed, wave_workers) = targets::sync_prepared_candle_targets(
            connection,
            &targets,
            market_database,
            storage_symbol_gate,
        );
        attempted_targets += wave_size;
        refreshed += wave_refreshed;
        worker_count = worker_count.max(wave_workers);
        if !pending_targets.is_empty() {
            thread::sleep(Duration::from_millis(CANDLE_SYNC_WAVE_PAUSE_MILLISECONDS));
        }
    }
    eprintln!(
        "[Daemon] 15m history batch attempted {attempted_targets} registered products with up to {worker_count} workers; {refreshed} interval syncs succeeded in {}ms.",
        batch_started.elapsed().as_millis(),
    );
    Ok(refreshed)
}

/// Refresh one completed daily candle window for each active registered app
/// instrument once per UTC day. Attempts, including failures, are recorded in
/// ingestion_runs so a bad ticker cannot starve the remaining daily batch.
fn sync_daily_chart_targets(
    connection: &Connection,
    market_database: &str,
    stale_priority: Option<&StaleSignalSyncPriority>,
    storage_symbol_gate: &targets::StorageSymbolSyncGate,
) -> Result<usize, rusqlite::Error> {
    if !provider_sync_enabled() {
        return Ok(0);
    }
    let targets = registered_daily_targets(connection, market_database, stale_priority)?;
    let mut attempted = 0usize;
    for (exchange_id, product_id, storage_symbol, underlying_symbol, asset_category_id) in targets {
        let _storage_symbol_guard = storage_symbol_gate.lock(&storage_symbol);
        let registration = if exchange_id == "tokocrypto" {
            instrument_registry::lookup_tokocrypto_market(connection, &product_id, &storage_symbol)?
        } else {
            None
        };
        let tokocrypto_market = registration
            .as_ref()
            .map(
                |(venue_symbol, quote, metadata)| RegisteredTokocryptoMarket {
                    venue_symbol,
                    quote_currency: quote,
                    contract_metadata_json: metadata,
                },
            );
        record_app_sync_cursor(
            connection,
            &exchange_id,
            &product_id,
            &storage_symbol,
            "1d",
            "attempted",
            "",
        );
        let instrument = candle_instrument(
            &exchange_id,
            &storage_symbol,
            &product_id,
            &underlying_symbol,
            &asset_category_id,
            tokocrypto_market,
        );
        let request = CandleSyncRequest {
            instrument,
            window: CandleSyncWindow::Daily,
        };
        let shared_reference_symbols = targets::shared_reference_storage_symbols(
            connection,
            &exchange_id,
            &storage_symbol,
            &underlying_symbol,
            &asset_category_id,
        )?;
        let reusable_shared_intervals = targets::fresh_shared_reference_intervals(
            market_database,
            instrument,
            &shared_reference_symbols,
            request.window.intervals(),
        );
        let market_connection =
            match open_market_history_for_symbol(market_database, &storage_symbol, true) {
                Ok(Some(connection)) => connection,
                Ok(None) => continue,
                Err(error) => {
                    record_app_sync_cursor(
                        connection,
                        &exchange_id,
                        &product_id,
                        &storage_symbol,
                        "1d",
                        "error",
                        &error.to_string(),
                    );
                    eprintln!(
                        "[Daemon] daily market database unavailable for {storage_symbol}: {error}"
                    );
                    attempted += 1;
                    continue;
                }
            };
        let market_connection = &market_connection;
        record_daily_instrument_attempt(
            market_connection,
            &storage_symbol,
            &exchange_id,
            &product_id,
            "attempted",
            0,
            "",
        );
        let research_proxy_available = CANDLE_SYNC_ADAPTERS.iter().any(|adapter| {
            adapter.source_class() == CandleSourceClass::ResearchProxy && adapter.supports(&request)
        });
        let mut any_provider_succeeded = false;
        let mut rows_received = 0usize;
        let mut errors = Vec::new();
        for adapter in &CANDLE_SYNC_ADAPTERS {
            if !adapter.supports(&request) {
                continue;
            }
            if adapter.source_class() == CandleSourceClass::ResearchProxy
                && reusable_shared_intervals.contains("1d")
            {
                any_provider_succeeded = true;
                eprintln!(
                    "[Daemon] Daily research sync reused a fresh shared reference for {storage_symbol}"
                );
                break;
            }
            let report = adapter.sync(market_connection, &request);
            let (interval, result) = report
                .outcomes
                .into_iter()
                .next()
                .map(|outcome| (outcome.interval, outcome.result))
                .unwrap_or_else(|| {
                    (
                        "1d",
                        Err(CandleProviderFailure {
                            message: "provider returned no sync outcomes".to_string(),
                        }),
                    )
                });
            let continue_chain =
                should_continue_candle_provider_chain(adapter.source_class(), result.is_ok());
            match result {
                Ok(rows) => {
                    any_provider_succeeded = true;
                    rows_received += rows;
                    let source_kind = if adapter.source_class() == CandleSourceClass::VenueNative {
                        "native"
                    } else {
                        "research"
                    };
                    eprintln!(
                        "[Daemon] Daily {source_kind} sync saved {storage_symbol} ({rows} {interval} bars)"
                    );
                }
                Err(error) => match adapter.source_class() {
                    CandleSourceClass::VenueNative => {
                        let source = format!("{}_native", adapter.provider_id());
                        record_daily_sync_failure(
                            market_connection,
                            &storage_symbol,
                            &exchange_id,
                            &product_id,
                            adapter.provider_id(),
                            &source,
                            &error.message,
                        );
                        errors.push(format!("{}: {}", adapter.provider_id(), error.message));
                        eprintln!(
                            "[Daemon] {} daily sync failed for {storage_symbol}: {}",
                            adapter.provider_id(),
                            error.message
                        );
                    }
                    CandleSourceClass::ResearchProxy => {
                        errors.push(error.message.clone());
                        eprintln!(
                            "[Daemon] Daily research sync failed for {storage_symbol}: {}",
                            error.message
                        );
                    }
                },
            }
            if !continue_chain {
                break;
            }
        }
        if !research_proxy_available {
            if exchange_id != "tokocrypto" {
                record_daily_sync_failure(
                    market_connection,
                    &storage_symbol,
                    &exchange_id,
                    &product_id,
                    "none",
                    "daily_provider_unavailable",
                    "no approved candle provider mapping for this registered instrument",
                );
            }
            errors.push("no approved candle provider mapping".to_string());
        }
        let attempt_error = if any_provider_succeeded {
            String::new()
        } else {
            errors.join("; ")
        };
        record_daily_instrument_attempt(
            market_connection,
            &storage_symbol,
            &exchange_id,
            &product_id,
            if any_provider_succeeded {
                "success"
            } else {
                "error"
            },
            rows_received,
            &attempt_error,
        );
        attempted += 1;
    }
    let _ = record_daily_backlog(connection);
    Ok(attempted)
}

/// Explicit maintainer backfill used by a fresh-clone regather command. The
/// normal service keeps its bounded scheduler cadence; this routine drains the
/// same bounded batches into the active SQLite database.
pub(super) fn regather_registered_market_history(
    connection: &Connection,
    market_database: &str,
) -> Result<(usize, usize), rusqlite::Error> {
    let storage_symbol_gate = targets::StorageSymbolSyncGate::default();
    let mut daily_attempted = 0usize;
    loop {
        let attempted =
            sync_daily_chart_targets(connection, market_database, None, &storage_symbol_gate)?;
        if attempted == 0 {
            break;
        }
        daily_attempted += attempted;
        if registered_daily_targets(connection, market_database, None)?.is_empty() {
            break;
        }
        thread::sleep(Duration::from_secs(REGATHER_BATCH_PAUSE_SECONDS));
    }

    let hourly_targets = registered_hourly_target_count(connection)?;
    let batch_size = MAX_REGISTERED_CANDLE_TARGETS_PER_TICK as usize;
    let hourly_batches = hourly_targets.div_ceil(batch_size);
    let mut hourly_series_refreshed = 0usize;
    for batch in 0..hourly_batches {
        hourly_series_refreshed += sync_hourly_chart_targets(
            connection,
            market_database,
            None,
            None,
            &storage_symbol_gate,
        )?;
        if batch + 1 < hourly_batches {
            thread::sleep(Duration::from_secs(REGATHER_BATCH_PAUSE_SECONDS));
        }
    }
    Ok((daily_attempted, hourly_series_refreshed))
}

pub(super) fn record_daily_backlog(connection: &Connection) -> Result<(), rusqlite::Error> {
    let remaining: i64 = connection.query_row(
        "SELECT COUNT(*) FROM app.instruments AS i
          WHERE i.is_active=1 AND i.venue_verified=1
            AND i.listing_status IN ('venue_active','public_listed_snapshot')
            AND NOT EXISTS (SELECT 1 FROM app.market_sync_cursor AS r
                 WHERE r.storage_symbol=i.storage_symbol AND r.exchange_id=i.exchange_id
                   AND r.product_id=i.product_id AND r.interval='1d'
                   AND substr(r.attempted_at,1,10)=date('now'))",
        [],
        |row| row.get(0),
    )?;
    connection.execute(
        "INSERT INTO app.instrument_registry_meta(key,value)
       VALUES('daily_backlog_remaining',?1)
       ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        [remaining.to_string()],
    )?;
    Ok(())
}

pub(super) fn record_daily_instrument_attempt(
    connection: &Connection,
    symbol: &str,
    exchange: &str,
    product: &str,
    status: &str,
    rows: usize,
    error: &str,
) {
    let now = format_timestamp(now_secs());
    record_app_sync_cursor(connection, exchange, product, symbol, "1d", status, error);
    let source = format!("daily_scheduler_{product}");
    let updated = connection
        .execute(
            "UPDATE ingestion_runs SET status=?1,rows_received=?2,error=?3
         WHERE id=(SELECT id FROM ingestion_runs WHERE symbol=?4 AND interval='1d'
             AND source=?5 AND exchange_id=?6 AND product_id=?7
             AND status='attempted' ORDER BY id DESC LIMIT 1)",
            params![
                status,
                rows as i64,
                error,
                symbol,
                source,
                exchange,
                product
            ],
        )
        .unwrap_or(0);
    if updated == 0 {
        let _ = connection.execute(
            "INSERT INTO ingestion_runs(symbol,interval,source,exchange_id,product_id,data_provider,attempted_at,status,rows_received,error)
             VALUES(?1,'1d',?2,?3,?4,'scheduler',?5,?6,?7,?8)",
            params![symbol, source, exchange, product, now, status, rows as i64, error],
        );
    }
}

fn record_app_sync_cursor(
    connection: &Connection,
    exchange: &str,
    product: &str,
    symbol: &str,
    interval: &str,
    status: &str,
    error: &str,
) {
    let now = format_timestamp(now_secs());
    let _ = connection.execute(
        "INSERT INTO app.market_sync_cursor(exchange_id,product_id,storage_symbol,interval,
            attempted_at,status,error) VALUES(?1,?2,?3,?4,?5,?6,?7)
         ON CONFLICT(exchange_id,product_id,storage_symbol,interval) DO UPDATE SET
            attempted_at=excluded.attempted_at,status=excluded.status,error=excluded.error",
        params![exchange, product, symbol, interval, now, status, error],
    );
}

pub(super) fn record_daily_sync_failure(
    connection: &Connection,
    symbol: &str,
    exchange: &str,
    product: &str,
    provider: &str,
    source: &str,
    error: &str,
) {
    let now = format_timestamp(now_secs());
    let _ = connection.execute(
        "INSERT INTO ingestion_runs(symbol,interval,source,exchange_id,product_id,data_provider,attempted_at,status,rows_received,error) VALUES (?1,'1d',?2,?3,?4,?5,?6,'error',0,?7)",
        params![symbol, source, exchange, product, provider, now, error],
    );
}

pub(super) fn yahoo_fallback_symbol(
    exchange: &str,
    underlying_symbol: &str,
    category: &str,
) -> Option<String> {
    let mut symbol = underlying_symbol.trim().to_ascii_uppercase();
    if symbol.is_empty() {
        return None;
    }
    match (exchange, category) {
        ("stockbit", "idx_stock") | ("pluang", "idx_stock") => {
            if !symbol.ends_with(".JK") {
                symbol.push_str(".JK");
            }
        }
        ("tokocrypto", "tokenized_stock") => {
            // The underlying ticker is an explicit registry field. It is used
            // only for proxy research; the Tokocrypto instrument identity stays
            // attached to the stored series and is never treated as native.
        }
        ("tokocrypto", "crypto_spot")
        | ("pintu", "crypto_spot")
        | ("pintu_futures", "crypto_futures")
        | ("pintu_futures", "crypto_perpetual")
        | ("pintu", "crypto_futures")
        | ("pintu", "crypto_perpetual")
        | ("pluang", "crypto_spot")
        | ("pluang", "crypto_perpetual") => {
            for suffix in [
                "USDT-PERP",
                "-USDT",
                "/USDT",
                "_USDT",
                "USDT",
                "-IDR",
                "/IDR",
                "_IDR",
            ] {
                if let Some(base) = symbol.strip_suffix(suffix) {
                    symbol = base.to_string();
                    break;
                }
            }
            if !symbol.ends_with("-USD") && !symbol.ends_with(".JK") {
                symbol.push_str("-USD");
            }
        }
        _ => return None,
    }
    Some(symbol)
}

#[path = "market_sync_worker.rs"]
mod worker;

pub(super) fn background_market_sync_loop(state: Arc<AppState>) {
    worker::background_market_sync_loop(state);
}

#[cfg(test)]
#[path = "market_sync_provider_tests.rs"]
mod provider_chain_tests;
