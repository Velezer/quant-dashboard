use super::{
    candle_instrument, candle_sync_worker_count, completed_bar_is_fresh_at,
    load_candles_from_source, open_market_history_for_symbol, record_app_sync_cursor,
    record_hourly_instrument_attempt, record_provider_scheduler_attempt,
    selected_candle_source_for_product, should_continue_candle_provider_chain, CandleSourceClass,
    CandleSyncRequest, CandleSyncWindow, PreparedCandleSyncTarget, RegisteredTokocryptoMarket,
    StaleSignalSyncPriority, CANDLE_SYNC_ADAPTERS, CANDLE_SYNC_WORKER_SQLITE_PAGE_CACHE_KIB,
    MAX_DAILY_REGISTERED_CANDLE_TARGETS_PER_TICK, STALE_DAILY_REFRESH_COOLDOWN_SECONDS,
};
use rusqlite::{Connection, OptionalExtension};

const CANDLE_SYNC_SWEEP_TICKS: usize = 12;
const INTRADAY_VOLUME_BASELINE_BAR_COUNT: usize = 20;
const MINIMUM_INTRADAY_VOLUME_BASELINE_BAR_COUNT: usize = 5;
const INTRADAY_VOLUME_HISTORY_READ_LIMIT: i64 = (INTRADAY_VOLUME_BASELINE_BAR_COUNT + 2) as i64;

pub(super) fn candle_sync_target_limit(registered_count: usize) -> usize {
    registered_count
        .saturating_add(CANDLE_SYNC_SWEEP_TICKS - 1)
        .checked_div(CANDLE_SYNC_SWEEP_TICKS)
        .unwrap_or(0)
}

#[derive(Default)]
pub(super) struct StorageSymbolSyncGate {
    active_symbols: std::sync::Mutex<std::collections::HashSet<String>>,
    available: std::sync::Condvar,
}

pub(super) struct StorageSymbolSyncGuard<'a> {
    gate: &'a StorageSymbolSyncGate,
    storage_symbol: &'a str,
}

impl StorageSymbolSyncGate {
    pub(super) fn lock<'a>(&'a self, storage_symbol: &'a str) -> StorageSymbolSyncGuard<'a> {
        let mut active_symbols = self
            .active_symbols
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        while active_symbols.contains(storage_symbol) {
            active_symbols = self
                .available
                .wait(active_symbols)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
        active_symbols.insert(storage_symbol.to_string());
        StorageSymbolSyncGuard {
            gate: self,
            storage_symbol,
        }
    }
}

impl Drop for StorageSymbolSyncGuard<'_> {
    fn drop(&mut self) {
        self.gate
            .active_symbols
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(self.storage_symbol);
        self.gate.available.notify_one();
    }
}

pub(super) fn sync_prepared_candle_target(
    target: &PreparedCandleSyncTarget,
    market_database: &str,
) -> Result<usize, String> {
    let market_connection =
        match open_market_history_for_symbol(market_database, &target.storage_symbol, true) {
            Ok(Some(connection)) => connection,
            Ok(None) => return Ok(0),
            Err(error) => return Err(error.to_string()),
        };
    let _ = market_connection.pragma_update(
        None,
        "cache_size",
        -CANDLE_SYNC_WORKER_SQLITE_PAGE_CACHE_KIB,
    );
    let tokocrypto_market =
        target
            .tokocrypto_market
            .as_ref()
            .map(|market| RegisteredTokocryptoMarket {
                venue_symbol: &market.venue_symbol,
                quote_currency: &market.quote_currency,
                contract_metadata_json: &market.contract_metadata_json,
            });
    let instrument = candle_instrument(
        &target.exchange_id,
        &target.storage_symbol,
        &target.product_id,
        &target.underlying_symbol,
        &target.asset_category_id,
        tokocrypto_market,
    );
    let request = CandleSyncRequest {
        instrument,
        window: CandleSyncWindow::Intraday,
    };
    let mut refreshed = 0;
    for adapter in &CANDLE_SYNC_ADAPTERS {
        if !adapter.supports(&request) {
            continue;
        }
        let report = adapter.sync(&market_connection, &request);
        let succeeded = report.all_succeeded();
        let error = report.error_summary();
        let continue_chain =
            should_continue_candle_provider_chain(adapter.source_class(), succeeded);
        if adapter.source_class() == CandleSourceClass::VenueNative {
            record_provider_scheduler_attempt(
                &market_connection,
                instrument,
                adapter.provider_id(),
                succeeded,
                &error,
            );
            refreshed += usize::from(succeeded);
        } else {
            refreshed += report
                .outcomes
                .iter()
                .filter(|outcome| outcome.result.is_ok())
                .count();
        }
        if !continue_chain {
            break;
        }
    }
    record_hourly_instrument_attempt(&market_connection, instrument);
    Ok(refreshed)
}

pub(super) fn sync_prepared_candle_targets(
    connection: &Connection,
    targets: &[super::PreparedCandleSyncTarget],
    market_database: &str,
    storage_symbol_gate: &StorageSymbolSyncGate,
) -> (usize, usize) {
    let worker_count = candle_sync_worker_count(targets.len());
    let next_target = std::sync::atomic::AtomicUsize::new(0);
    let refreshed = std::sync::atomic::AtomicUsize::new(0);
    let open_failures = std::sync::Mutex::new(Vec::<(usize, String)>::new());
    std::thread::scope(|scope| {
        let mut workers = Vec::with_capacity(worker_count);
        for _ in 0..worker_count {
            let next_target = &next_target;
            let refreshed = &refreshed;
            let open_failures = &open_failures;
            let storage_symbol_gate = storage_symbol_gate;
            workers.push(scope.spawn(move || loop {
                let index = next_target.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let Some(target) = targets.get(index) else {
                    break;
                };
                let _storage_symbol_guard = storage_symbol_gate.lock(&target.storage_symbol);
                match sync_prepared_candle_target(target, market_database) {
                    Ok(count) => {
                        refreshed.fetch_add(count, std::sync::atomic::Ordering::Relaxed);
                    }
                    Err(error) => open_failures
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .push((index, error)),
                }
            }));
        }
        for worker in workers {
            let _ = worker.join();
        }
    });

    for (index, error) in open_failures
        .into_inner()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
    {
        let target = &targets[index];
        record_app_sync_cursor(
            connection,
            &target.exchange_id,
            &target.product_id,
            &target.storage_symbol,
            "hourly",
            "error",
            &error,
        );
    }
    (
        refreshed.load(std::sync::atomic::Ordering::Relaxed),
        worker_count,
    )
}

pub(crate) fn registered_hourly_targets(
    connection: &Connection,
    priority_symbol: Option<&str>,
    stale_priorities: &[StaleSignalSyncPriority],
    target_limit: i64,
) -> Result<Vec<(String, String, String, String, String)>, rusqlite::Error> {
    let target_limit = usize::try_from(target_limit.max(1)).unwrap_or(usize::MAX);
    let mut targets = Vec::with_capacity(target_limit);
    let mut seen = std::collections::HashSet::with_capacity(target_limit);
    let mut priority_statement = connection.prepare(
        "SELECT exchange_id, storage_symbol, product_id, underlying_symbol, asset_category_id
           FROM app.instruments
          WHERE is_active=1 AND venue_verified=1 AND storage_symbol <> ''
            AND listing_status IN ('venue_active','public_listed_snapshot')
            AND exchange_id=?1 AND storage_symbol=?2 AND product_id=?3
          GROUP BY exchange_id, product_id, storage_symbol
          LIMIT 1",
    )?;
    for priority in stale_priorities {
        let target = priority_statement
            .query_row(
                rusqlite::params![
                    priority.exchange_id,
                    priority.symbol,
                    priority.product_id
                ],
                registered_hourly_target_from_row,
            )
            .optional()?;
        if let Some(target) = target {
            let identity = registered_hourly_target_identity(&target);
            if seen.insert(identity) {
                targets.push(target);
            }
        }
        if targets.len() >= target_limit {
            return Ok(targets);
        }
    }

    let normal_limit = target_limit
        .saturating_add(stale_priorities.len())
        .min(i64::MAX as usize) as i64;
    let mut statement = connection.prepare(
        "SELECT i.exchange_id, i.storage_symbol, i.product_id, i.underlying_symbol, i.asset_category_id
           FROM app.instruments AS i
           LEFT JOIN app.market_sync_cursor AS c ON c.exchange_id=i.exchange_id
             AND c.product_id=i.product_id AND c.storage_symbol=i.storage_symbol
             AND c.interval='hourly'
          WHERE i.is_active=1 AND i.venue_verified=1 AND i.storage_symbol <> ''
            AND i.listing_status IN ('venue_active','public_listed_snapshot')
          GROUP BY i.exchange_id, i.product_id, i.storage_symbol
          ORDER BY MAX(CASE WHEN ?1 IS NOT NULL AND
                                  (i.storage_symbol=?1 OR i.venue_symbol=?1 OR i.symbol=?1)
                             THEN 1 ELSE 0 END) DESC,
                   COALESCE(MIN(c.attempted_at), '') ASC,
                   i.exchange_id ASC, i.product_id ASC, i.storage_symbol ASC
          LIMIT ?2",
    )?;
    let normal_targets = statement
        .query_map(
            rusqlite::params![priority_symbol, normal_limit],
            registered_hourly_target_from_row,
        )?
        .collect::<Result<Vec<_>, _>>()?;
    for target in normal_targets {
        let identity = registered_hourly_target_identity(&target);
        if seen.insert(identity) {
            targets.push(target);
        }
        if targets.len() >= target_limit {
            break;
        }
    }
    Ok(targets)
}

fn registered_hourly_target_from_row(
    row: &rusqlite::Row<'_>,
) -> Result<(String, String, String, String, String), rusqlite::Error> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
    ))
}

fn registered_hourly_target_identity(
    target: &(String, String, String, String, String),
) -> (String, String, String) {
    (target.0.clone(), target.2.clone(), target.1.clone())
}

pub(super) fn rank_intraday_targets_by_recent_volume(
    targets: Vec<(String, String, String, String, String)>,
    market_directory: &str,
    priority_symbol: Option<&str>,
    stale_priorities: &[StaleSignalSyncPriority],
    now_ns: i64,
) -> Vec<(String, String, String, String, String)> {
    let mut ranked = targets
        .into_iter()
        .enumerate()
        .map(|(queue_position, target)| {
            let stale_priority_rank = stale_priorities
                .iter()
                .position(|priority| {
                    priority.exchange_id == target.0
                        && priority.product_id == target.2
                        && priority.symbol == target.1
                })
                .unwrap_or(stale_priorities.len());
            let chart_priority_rank = u8::from(
                !priority_symbol.is_some_and(|symbol| symbol == target.1 || symbol == target.3),
            );
            let volume_score = latest_relative_quote_volume(
                market_directory,
                &target.0,
                &target.1,
                &target.2,
                now_ns,
            )
            .unwrap_or(1.0);
            (
                stale_priority_rank,
                chart_priority_rank,
                volume_score,
                queue_position,
                target,
            )
        })
        .collect::<Vec<_>>();

    ranked.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| left.1.cmp(&right.1))
            .then_with(|| right.2.total_cmp(&left.2))
            .then_with(|| left.3.cmp(&right.3))
    });

    ranked
        .into_iter()
        .map(|(_, _, _, _, target)| target)
        .collect()
}

fn latest_relative_quote_volume(
    market_directory: &str,
    exchange_id: &str,
    storage_symbol: &str,
    product_id: &str,
    now_ns: i64,
) -> Option<f64> {
    let connection =
        open_market_history_for_symbol(market_directory, storage_symbol, false).ok()??;
    let _ = connection.pragma_update(
        None,
        "cache_size",
        -CANDLE_SYNC_WORKER_SQLITE_PAGE_CACHE_KIB,
    );
    let source = selected_candle_source_for_product(
        &connection,
        storage_symbol,
        exchange_id,
        product_id,
        "15m",
    )
    .ok()??;
    let candles = load_candles_from_source(
        &connection,
        storage_symbol,
        "15m",
        INTRADAY_VOLUME_HISTORY_READ_LIMIT,
        &source,
    )
    .ok()?;
    let bar_duration_ns = super::timeframe_duration_ns("15m")?;
    let latest = candles
        .iter()
        .rev()
        .find(|candle| completed_bar_is_fresh_at(storage_symbol, "15m", candle.time, now_ns))?;
    let latest_turnover = candle_quote_turnover(latest.close, latest.volume)?;
    let baseline_window_ns =
        bar_duration_ns.saturating_mul((INTRADAY_VOLUME_BASELINE_BAR_COUNT + 1) as i64);
    let mut prior_turnover = candles
        .iter()
        .rev()
        .filter(|candle| {
            candle.time < latest.time
                && latest.time.saturating_sub(candle.time) <= baseline_window_ns
                && candle.time.saturating_add(bar_duration_ns) <= now_ns
        })
        .filter_map(|candle| candle_quote_turnover(candle.close, candle.volume))
        .take(INTRADAY_VOLUME_BASELINE_BAR_COUNT)
        .collect::<Vec<_>>();
    let baseline = median(&mut prior_turnover)?;
    (prior_turnover.len() >= MINIMUM_INTRADAY_VOLUME_BASELINE_BAR_COUNT)
        .then(|| latest_turnover / baseline)
        .filter(|score| score.is_finite())
}

fn candle_quote_turnover(close: f64, volume: f64) -> Option<f64> {
    let turnover = close * volume;
    (close > 0.0 && volume > 0.0 && turnover.is_finite() && turnover > 0.0).then_some(turnover)
}

fn median(values: &mut [f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    let midpoint = values.len() / 2;
    if values.len() % 2 == 0 {
        Some((values[midpoint - 1] + values[midpoint]) / 2.0)
    } else {
        Some(values[midpoint])
    }
}

pub(crate) fn registered_daily_targets(
    connection: &Connection,
    market_directory: &str,
    stale_priority: Option<&StaleSignalSyncPriority>,
) -> Result<Vec<(String, String, String, String, String)>, rusqlite::Error> {
    let mut statement = connection.prepare(
        "SELECT i.exchange_id, i.product_id, i.storage_symbol, i.underlying_symbol, i.asset_category_id
           FROM app.instruments AS i
          WHERE i.is_active=1 AND i.venue_verified=1 AND i.storage_symbol <> ''
            AND i.listing_status IN ('venue_active','public_listed_snapshot')
            AND (
                NOT EXISTS (
                    SELECT 1 FROM app.market_sync_cursor AS r
                     WHERE r.storage_symbol=i.storage_symbol
                       AND r.exchange_id=i.exchange_id
                       AND r.product_id=i.product_id
                       AND r.interval='1d'
                       AND r.status IN ('success','error')
                       AND substr(r.attempted_at,1,10)=date('now')
                )
                OR (
                    ?1 IS NOT NULL
                    AND i.exchange_id=?1
                    AND i.product_id=?2
                    AND i.storage_symbol=?3
                    AND NOT EXISTS (
                        SELECT 1 FROM app.market_sync_cursor AS prior
                         WHERE prior.storage_symbol=i.storage_symbol
                           AND prior.exchange_id=i.exchange_id
                           AND prior.product_id=i.product_id
                           AND prior.interval='1d'
                           AND COALESCE(unixepoch(substr(prior.attempted_at,1,19)),0)
                               >= unixepoch('now') - ?4
                    )
                )
            )
          ORDER BY CASE WHEN ?1 IS NOT NULL
                             AND i.exchange_id=?1
                             AND i.product_id=?2
                             AND i.storage_symbol=?3
                        THEN 0 ELSE 1 END,
                   i.exchange_id ASC, i.product_id ASC, i.storage_symbol ASC
          LIMIT ?5",
    )?;
    let candidates: Vec<(String, String, String, String, String)> = statement
        .query_map(
            rusqlite::params![
                stale_priority.map(|priority| priority.exchange_id.as_str()),
                stale_priority.map(|priority| priority.product_id.as_str()),
                stale_priority.map(|priority| priority.symbol.as_str()),
                STALE_DAILY_REFRESH_COOLDOWN_SECONDS,
                MAX_DAILY_REGISTERED_CANDLE_TARGETS_PER_TICK
            ],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);

    let mut due_targets = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        let (exchange_id, product_id, storage_symbol, _, _) = &candidate;
        let Some(market_connection) =
            open_market_history_for_symbol(market_directory, storage_symbol, false)?
        else {
            due_targets.push(candidate);
            continue;
        };
        let source = format!("daily_scheduler_{product_id}");
        let attempted_today: bool = market_connection.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM main.ingestion_runs
                 WHERE symbol=?1 AND interval='1d' AND source=?2
                   AND exchange_id=?3 AND product_id=?4
                   AND substr(attempted_at,1,10)=date('now')
            )",
            rusqlite::params![storage_symbol, source, exchange_id, product_id],
            |row| row.get(0),
        )?;
        drop(market_connection);
        let is_priority = stale_priority.is_some_and(|priority| {
            priority.exchange_id == exchange_id.as_str()
                && priority.product_id == product_id.as_str()
                && priority.symbol == storage_symbol.as_str()
        });
        if !attempted_today || is_priority {
            due_targets.push(candidate);
        }
    }
    Ok(due_targets)
}

pub(super) fn registered_hourly_target_count(
    connection: &Connection,
) -> Result<usize, rusqlite::Error> {
    let count: i64 = connection.query_row(
        "SELECT COUNT(*) FROM (
            SELECT 1 FROM app.instruments
             WHERE is_active=1 AND venue_verified=1 AND storage_symbol <> ''
               AND listing_status IN ('venue_active','public_listed_snapshot')
             GROUP BY exchange_id, product_id, storage_symbol
         )",
        [],
        |row| row.get(0),
    )?;
    Ok(count.max(0) as usize)
}

#[cfg(test)]
#[path = "market_sync_targets_tests.rs"]
mod tests;
