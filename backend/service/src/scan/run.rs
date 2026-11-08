use super::*;

#[cfg(test)]
#[path = "run/shared_market_history_tests.rs"]
mod shared_market_history_tests;

mod stockbit_close;
mod types;

use stockbit_close::{
    ensure_stockbit_close_checkpoint_schema, is_final_stockbit_close_bar,
    expected_stockbit_closed_session_day, save_stockbit_close_scan_checkpoint,
    stockbit_close_checkpoint_is_saved, stockbit_closed_session_checkpoint,
    StockbitCloseScanCheckpoint,
};
use types::{AppScanResult, RegisteredInstrument, ScanCycleOutput};

const SIGNAL_SCAN_APP_ORDER: [&str; 4] = ["pintu", "pluang", "tokocrypto", "stockbit"];
const SIGNAL_SCAN_PUBLISH_BATCH_SIZE: usize = 25;
#[cfg(not(test))]
type AppScanMetrics = (
    AppScanResult,
    Option<std::time::Duration>,
    Option<std::time::Duration>,
);

#[cfg(not(test))]
struct AppScanProgress {
    remaining: std::collections::BTreeMap<String, usize>,
    pending: std::collections::BTreeMap<String, ScanCycleOutput>,
}

pub(super) fn run_rust_scan_inner(
    app_connection: &Connection,
    market_database: &str,
    interval_min: u64,
    sync_state: Option<&AppState>,
    exchange_scope: Option<&str>,
    selected_signal_scope: Option<&SelectedSignalScanTarget>,
) -> Result<Value, rusqlite::Error> {
    #[cfg(not(test))]
    let active_scoring_profiles =
        crate::research_reports::entry_validation_active_profile_snapshot(app_connection)?;
    #[cfg(test)]
    let active_scoring_profiles = crate::research_reports::ActiveScoringProfiles::new();

    // Signal rows belong to an app registration. Stored candles only supply
    // prices; they cannot create a ticker or choose an app.
    let registered = {
        let mut statement = app_connection.prepare(
            "SELECT exchange_id, product_id, asset_category_id, storage_symbol, symbol,
                    underlying_symbol
               FROM instruments
              WHERE is_active=1 AND venue_verified=1
                AND listing_status IN ('venue_active','public_listed_snapshot')
              ORDER BY exchange_id, product_id, asset_category_id, storage_symbol",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?;
        rows.collect::<Result<Vec<_>, _>>()?
    }
    .into_iter()
    .filter(|instrument| {
        exchange_scope.is_none_or(|exchange| instrument.0 == exchange)
            && selected_signal_scope.is_none_or(|target| {
                instrument.0 == target.exchange_id
                    && instrument.1 == target.product_id
                    && instrument.2 == target.asset_category_id
                    && instrument.3 == target.storage_symbol
                    && instrument.4 == target.symbol
            })
    })
    .collect::<Vec<_>>();

    let registered_count = registered.len();
    if selected_signal_scope.is_some() && registered_count != 1 {
        return Err(rusqlite::Error::InvalidParameterName(
            "selected signal is no longer a verified registered instrument".into(),
        ));
    }
    let mut app_registrations =
        std::collections::BTreeMap::<String, Vec<RegisteredInstrument>>::new();
    for exchange_id in SIGNAL_SCAN_APP_ORDER {
        if exchange_scope.is_none_or(|scope| scope == exchange_id) {
            app_registrations.insert(exchange_id.to_string(), Vec::new());
        }
    }
    for instrument in registered {
        app_registrations
            .entry(instrument.0.clone())
            .or_default()
            .push(instrument);
    }

    let mut workers = Vec::new();
    for exchange_id in SIGNAL_SCAN_APP_ORDER {
        if let Some(instruments) = app_registrations.remove(exchange_id) {
            workers.push(ScanWorker::new(exchange_id.to_string(), instruments));
        }
    }
    // Preserve any additional registered apps without displacing the four
    // app rotation. Unsupported products still follow the existing scan rules.
    for (exchange_id, instruments) in app_registrations {
        if exchange_scope.is_none_or(|scope| scope == exchange_id) {
            workers.push(ScanWorker::new(exchange_id, instruments));
        }
    }
    #[cfg(not(test))]
    ensure_stockbit_close_checkpoint_schema(app_connection)?;
    let mut output = ScanCycleOutput::new();
    let scan_started = std::time::Instant::now();
    let completed_count = std::sync::atomic::AtomicUsize::new(0);

    #[cfg(test)]
    {
        // Keep tests deterministic and compatible with their in-memory app DB.
        let mut remaining: Vec<usize> = workers.iter().map(|worker| worker.total).collect();
        let mut cursor = 0usize;
        while let Some(index) = next_scan_worker(&remaining, &mut cursor) {
            let worker = &mut workers[index];
            let instrument = worker
                .instruments
                .next()
                .expect("remaining count matches scan queue");
            let started = std::time::Instant::now();
            let result = scan_registered_app(
                app_connection,
                market_database,
                sync_state,
                &worker.exchange_id,
                std::iter::once(instrument),
                &completed_count,
                registered_count,
                selected_signal_scope.is_some(),
                &active_scoring_profiles,
                &mut output,
            )?;
            worker.elapsed += started.elapsed();
            worker.result.fresh_price_count += result.fresh_price_count;
            worker.result.unavailable_price_count += result.unavailable_price_count;
            worker.result.generated_plan_count += result.generated_plan_count;
            worker.result.signal_row_count += result.signal_row_count;
            remaining[index] -= 1;
            if remaining[index] == 0 {
                eprintln!(
                    "[ScanWorker] Finished {}: {} fresh prices, {} signal rows, elapsed {}ms.",
                    worker.exchange_id,
                    worker.result.fresh_price_count,
                    worker.result.signal_row_count,
                    worker.elapsed.as_millis()
                );
            }
        }
        for worker in workers {
            output.app_workers.push(serde_json::json!({
                "exchange_id": worker.exchange_id,
                "status": "complete",
                "registered_instruments": worker.total,
                "fresh_price_instruments": worker.result.fresh_price_count,
                "unavailable_price_instruments": worker.result.unavailable_price_count,
                "generated_plans": worker.result.generated_plan_count,
                "signal_rows": worker.result.signal_row_count,
                "paused_market_closed_count": worker.result.paused_market_closed_count,
            }));
        }
    }

    #[cfg(not(test))]
    {
        use std::sync::Mutex;

        // Worker concurrency is per registered ticker, not per app. This
        // keeps a single large app parallel while the shared queue still
        // rotates fairly across apps.
        let worker_count = registered_count.min(SIGNAL_SCAN_DATABASE_RESERVE);
        let mut app_connections = Vec::with_capacity(worker_count);
        for _ in 0..worker_count {
            app_connections.push(open_app_db_for_market_directory(market_database)?);
        }
        let app_instrument_counts = workers
            .iter()
            .map(|worker| (worker.exchange_id.clone(), worker.total))
            .collect::<std::collections::BTreeMap<_, _>>();
        for worker in workers.iter().filter(|worker| worker.total == 0) {
            signal_alerts::publish_signal_screener_app_batch(
                app_connection,
                &worker.exchange_id,
                &[],
                true,
                &[],
                &[],
                &[],
                &[],
            )?;
        }
        let mut app_scan_results = app_instrument_counts
            .keys()
            .map(|exchange_id| {
                (
                    exchange_id.clone(),
                    (AppScanResult::default(), None, None),
                )
            })
            .collect::<std::collections::BTreeMap<String, AppScanMetrics>>();
        if selected_signal_scope.is_none() {
            for exchange_id in app_instrument_counts.keys() {
                signal_alerts::mark_signal_screener_app_scanning(app_connection, exchange_id)?;
            }
        }
        let app_scan_progress = Mutex::new(AppScanProgress {
            remaining: app_instrument_counts.clone(),
            pending: std::collections::BTreeMap::new(),
        });
        let queue = Mutex::new((workers, 0usize));
        std::thread::scope(|scope| -> Result<(), rusqlite::Error> {
            let mut handles = Vec::with_capacity(worker_count);
            for app_connection in app_connections {
                let queue = &queue;
                let progress = &completed_count;
                let app_scan_progress = &app_scan_progress;
                let active_profiles = &active_scoring_profiles;
                handles.push(scope.spawn(move || -> Result<_, rusqlite::Error> {
                    let mut worker_output = ScanCycleOutput::new();
                    let mut worker_app_results =
                        std::collections::BTreeMap::<String, AppScanMetrics>::new();
                    loop {
                        let next_task = {
                            let mut queue = queue
                                .lock()
                                .unwrap_or_else(|poisoned| poisoned.into_inner());
                            let (workers, cursor) = &mut *queue;
                            next_scan_task(workers, cursor)
                        };
                        let Some((exchange_id, instrument)) = next_task else {
                            break;
                        };
                        let mut instrument_output = ScanCycleOutput::new();
                        let started = std::time::Instant::now();
                        let result = scan_registered_app(
                            &app_connection,
                            market_database,
                            sync_state,
                            &exchange_id,
                            std::iter::once(instrument),
                            progress,
                            registered_count,
                            selected_signal_scope.is_some(),
                            active_profiles,
                            &mut instrument_output,
                        )?;
                        let elapsed = started.elapsed();
                        let started_at = started.duration_since(scan_started);
                        let finished_at = started_at.saturating_add(elapsed);
                        let app_result = worker_app_results
                            .entry(exchange_id.clone())
                            .or_default();
                        app_result.0.fresh_price_count += result.fresh_price_count;
                        app_result.0.unavailable_price_count += result.unavailable_price_count;
                        app_result.0.generated_plan_count += result.generated_plan_count;
                        app_result.0.signal_row_count += result.signal_row_count;
                        app_result.0.paused_market_closed_count += result.paused_market_closed_count;
                        app_result.1 = Some(
                            app_result
                                .1
                                .map_or(started_at, |first| first.min(started_at)),
                        );
                        app_result.2 = Some(
                            app_result
                                .2
                                .map_or(finished_at, |last| last.max(finished_at)),
                        );
                        let completed_batch = {
                            let mut scan_progress = app_scan_progress
                                .lock()
                                .unwrap_or_else(|poisoned| poisoned.into_inner());
                            let remaining = scan_progress
                                .remaining
                                .entry(exchange_id.clone())
                                .or_default();
                            *remaining = remaining.saturating_sub(1);
                            let app_finished = *remaining == 0;
                            let pending = scan_progress
                                .pending
                                .entry(exchange_id.clone())
                                .or_insert_with(ScanCycleOutput::new);
                            pending.merge(instrument_output);
                            let should_publish = pending.scanned_instruments.len()
                                >= SIGNAL_SCAN_PUBLISH_BATCH_SIZE
                                || app_finished;
                            if should_publish {
                                scan_progress.pending.remove(&exchange_id)
                            } else {
                                None
                            }
                        };
                        if let Some(batch) = completed_batch {
                            if selected_signal_scope.is_some() {
                                worker_output.merge(batch);
                            } else {
                                let batch_signal_count = batch.running_signal_rows.len()
                                    + batch.potential_signal_rows.len()
                                    + batch.standalone_signal_rows.len();
                                signal_alerts::publish_signal_screener_app_batch(
                                    &app_connection,
                                    &exchange_id,
                                    &batch.scanned_instruments,
                                    false,
                                    &batch.running_signal_rows,
                                    &batch.potential_signal_rows,
                                    &batch.standalone_signal_rows,
                                    &batch.score_updates,
                                )?;
                                for checkpoint in &batch.close_checkpoints {
                                    save_stockbit_close_scan_checkpoint(&app_connection, checkpoint)?;
                                }
                                eprintln!(
                                    "[ScanWorker] Published {exchange_id} batch: {} instruments, {batch_signal_count} signal rows.",
                                    batch.scanned_instruments.len()
                                );
                                worker_output.merge(batch);
                            }
                        }
                    }
                    Ok((worker_output, worker_app_results))
                }));
            }
            for handle in handles {
                match handle.join() {
                    Ok(Ok((worker_output, worker_results))) => {
                        output.merge(worker_output);
                        for (exchange_id, (result, started_at, finished_at)) in worker_results {
                            let aggregate = app_scan_results
                                .entry(exchange_id)
                                .or_insert_with(|| (AppScanResult::default(), None, None));
                            aggregate.0.fresh_price_count += result.fresh_price_count;
                            aggregate.0.unavailable_price_count += result.unavailable_price_count;
                            aggregate.0.generated_plan_count += result.generated_plan_count;
                            aggregate.0.signal_row_count += result.signal_row_count;
                            aggregate.0.paused_market_closed_count += result.paused_market_closed_count;
                            if let Some(started_at) = started_at {
                                aggregate.1 = Some(
                                    aggregate.1.map_or(started_at, |first| first.min(started_at)),
                                );
                            }
                            if let Some(finished_at) = finished_at {
                                aggregate.2 = Some(
                                    aggregate.2.map_or(finished_at, |last| last.max(finished_at)),
                                );
                            }
                        }
                    }
                    Ok(Err(error)) => return Err(error),
                    Err(_) => {
                        return Err(rusqlite::Error::InvalidParameterName(
                            "app signal scan worker panicked".into(),
                        ));
                    }
                }
            }
            Ok(())
        })?;
        for (exchange_id, registered_instruments) in app_instrument_counts {
            let (result, started_at, finished_at) = app_scan_results
                .remove(&exchange_id)
                .unwrap_or_default();
            let elapsed = match (started_at, finished_at) {
                (Some(started_at), Some(finished_at)) => finished_at.saturating_sub(started_at),
                _ => std::time::Duration::ZERO,
            };
            eprintln!(
                "[ScanWorker] Finished {exchange_id}: {registered_instruments} registered, {} fresh prices, {} signal rows, elapsed {}ms.",
                result.fresh_price_count,
                result.signal_row_count,
                elapsed.as_millis()
            );
            output.app_workers.push(serde_json::json!({
                "exchange_id": exchange_id,
                "status": "complete",
                "registered_instruments": registered_instruments,
                "fresh_price_instruments": result.fresh_price_count,
                "unavailable_price_instruments": result.unavailable_price_count,
                "generated_plans": result.generated_plan_count,
                "signal_rows": result.signal_row_count,
                "paused_market_closed_count": result.paused_market_closed_count,
                "elapsed_ms": elapsed.as_millis(),
            }));
        }
    }

    if let Some(state) = sync_state {
        state.update_scan_progress("", registered_count, registered_count);
    }

    let now_str = chrono_or_simple_timestamp();
    output
        .running_signal_rows
        .extend(output.potential_signal_rows);
    output
        .running_signal_rows
        .extend(output.standalone_signal_rows);
    let signals = output.running_signal_rows;
    let mut readiness = output.readiness.into_value();
    readiness["workers"] = serde_json::json!(output.app_workers.clone());
    eprintln!(
        "Signal scan completed: {registered_count} registered instruments, {} with fresh 15m prices, {} directional plans generated, {} screener rows accepted, {} Stockbit timeframe score updates prepared",
        registered_count.saturating_sub(output.price_data_unavailable_count),
        output.generated_plan_count,
        signals.len(),
        output.score_updates.len()
    );
    if let Some(target) = selected_signal_scope {
        signal_alerts::replace_signal_screener_instrument_snapshot(
            app_connection,
            &target.exchange_id,
            &target.product_id,
            &target.asset_category_id,
            &target.symbol,
            &signals,
            &output.score_updates,
        )?;
        #[cfg(not(test))]
        for checkpoint in &output.close_checkpoints {
            save_stockbit_close_scan_checkpoint(app_connection, checkpoint)?;
        }
    } else if let Some(exchange_id) = exchange_scope {
        signal_alerts::replace_signal_screener_app_snapshot_with_readiness(
            app_connection,
            exchange_id,
            &signals,
            &output.score_updates,
            output.price_data_unavailable_count,
            registered_count,
            Some(&readiness),
            &output.preserved_stockbit,
        )?;
    } else {
        signal_alerts::replace_signal_screener_snapshot_with_readiness(
            app_connection,
            &signals,
            &output.score_updates,
            output.price_data_unavailable_count,
            registered_count,
            Some(&readiness),
            &output.preserved_stockbit,
        )?;
    }
    let total_signals = signals.len();
    let signal_master_data = signal_screener_master_data_for_connection(app_connection)?;
    let mut payload = serde_json::json!({
        "summary": {
            "total_signals": total_signals,
            "generated_at": now_str,
            "interval_minutes": interval_min,
            "interval_seconds": interval_min * 60,
            "scan_duration_ms": scan_started.elapsed().as_millis(),
            "score_interpretation": "ranking_only_not_probability",
            "engine": "rust_native_zero_allocation",
            "workers": readiness["workers"].clone(),
            "readiness": readiness
        },
        "signals": [],
        "master_data": signal_master_data
    });

    let mut newly_detected_alerts = Vec::new();
    let mut alert_storage_available = true;
    match signal_alerts::record_scan_signal_alerts(app_connection, signals.iter()) {
        Ok(alerts) => newly_detected_alerts.extend(alerts),
        Err(error) => {
            eprintln!("[Alerts] Could not persist new signal alerts: {error}");
            alert_storage_available = false;
        }
    }
    match signal_alerts::record_indicator_combo_alerts(app_connection, output.user_indicator_alerts)
    {
        Ok(alerts) => newly_detected_alerts.extend(alerts),
        Err(error) => {
            eprintln!("[Alerts] Could not persist user indicator signals: {error}");
            alert_storage_available = false;
        }
    }
    if let Err(error) = signal_alerts::refresh_lifecycles(app_connection) {
        eprintln!("[Alerts] Could not refresh signal lifecycles: {error}");
        alert_storage_available = false;
    }
    payload["signals"] = Value::Array(signals);
    payload["new_signal_alerts"] = Value::Array(newly_detected_alerts);
    if !alert_storage_available {
        payload["signal_alert_status"] = serde_json::json!("unavailable");
    }

    Ok(payload)
}

#[cfg(test)]
#[path = "run/stockbit_close_tests.rs"]
mod stockbit_close_tests;

struct ScanWorker {
    exchange_id: String,
    total: usize,
    instruments: std::vec::IntoIter<RegisteredInstrument>,
    #[cfg(test)]
    result: AppScanResult,
    #[cfg(test)]
    elapsed: std::time::Duration,
}

struct ScanProgressStep<'a> {
    completed_count: &'a std::sync::atomic::AtomicUsize,
    sync_state: Option<&'a AppState>,
    label: String,
    total: usize,
}

impl Drop for ScanProgressStep<'_> {
    fn drop(&mut self) {
        let completed = self
            .completed_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            .saturating_add(1);
        if let Some(state) = self.sync_state {
            state.finish_scan_symbol(&self.label, completed, self.total);
        }
    }
}

impl ScanWorker {
    fn new(exchange_id: String, instruments: Vec<RegisteredInstrument>) -> Self {
        let total = instruments.len();
        eprintln!(
            "[ScanWorker] Starting {exchange_id} scan worker for {total} registered instruments."
        );
        Self {
            exchange_id,
            total,
            instruments: instruments.into_iter(),
            #[cfg(test)]
            result: AppScanResult::default(),
            #[cfg(test)]
            elapsed: std::time::Duration::ZERO,
        }
    }
}

/// Cycle across nonempty app queues; exhausted apps do not block other apps.
pub(super) fn next_scan_worker(remaining: &[usize], cursor: &mut usize) -> Option<usize> {
    if remaining.is_empty() {
        return None;
    }
    for offset in 0..remaining.len() {
        let index = (*cursor + offset) % remaining.len();
        if remaining[index] > 0 {
            *cursor = (index + 1) % remaining.len();
            return Some(index);
        }
    }
    None
}

#[cfg(not(test))]
fn next_scan_task(
    workers: &mut [ScanWorker],
    cursor: &mut usize,
) -> Option<(String, RegisteredInstrument)> {
    let worker_count = workers.len();
    if worker_count == 0 {
        return None;
    }
    for offset in 0..worker_count {
        let index = (*cursor + offset) % worker_count;
        if workers[index].instruments.len() == 0 {
            continue;
        }
        *cursor = (index + 1) % worker_count;
        let worker = &mut workers[index];
        return Some((worker.exchange_id.clone(), worker.instruments.next()?));
    }
    None
}

/// Resolve an app ticker to its own fresh series first. If that history is
/// stale, a verified peer in the same crypto asset category may supply a
/// canonical shared research reference without copying candle rows.
fn open_scan_market_history(
    app_connection: &Connection,
    market_database: &str,
    exchange: &str,
    product: &str,
    category: &str,
    storage_symbol: &str,
    underlying_symbol: &str,
) -> Result<Option<(SignalScanConnection, String)>, rusqlite::Error> {
    let now_seconds = now_secs().min(i64::MAX as u64) as i64;
    let now_ns = now_seconds.saturating_mul(NANOS_PER_SECOND);
    let mut own_database_exists = false;
    let mut own_open_error = None;
    match open_market_history_for_signal_scan(market_database, storage_symbol, false) {
        Ok(Some(connection)) => {
            own_database_exists = true;
            let state = product_candle_scan_state_at(
                &connection,
                storage_symbol,
                exchange,
                product,
                "15m",
                now_seconds,
            );
            match state {
                Ok(state)
                    if scan_state_has_fresh_price(
                        &state,
                        storage_symbol,
                        category,
                        now_ns,
                        false,
                    ) =>
                {
                    return Ok(Some((connection, storage_symbol.to_string())));
                }
                Ok(_) => {}
                Err(error) => own_open_error = Some(error),
            }
        }
        Ok(None) => {}
        Err(error) => own_open_error = Some(error),
    }

    if matches!(category, "crypto_spot" | "crypto_perpetual" | "crypto_futures")
        && !underlying_symbol.trim().is_empty()
    {
        let candidates = {
            let mut statement = app_connection.prepare(
                "SELECT DISTINCT candidate.storage_symbol
                   FROM instruments AS candidate
                   JOIN market_store_inventory AS inventory
                     ON inventory.storage_symbol=candidate.storage_symbol
                  WHERE candidate.is_active=1 AND candidate.venue_verified=1
                    AND candidate.listing_status IN ('venue_active','public_listed_snapshot')
                    AND candidate.asset_category_id=?1
                    AND upper(trim(candidate.underlying_symbol))=upper(trim(?2))
                    AND candidate.exchange_id<>?3
                    AND candidate.storage_symbol<>?4
                  ORDER BY candidate.storage_symbol",
            )?;
            let rows = statement
                .query_map(
                    params![category, underlying_symbol, exchange, storage_symbol],
                    |row| row.get::<_, String>(0),
                )?
                .collect::<Result<Vec<_>, _>>()?;
            rows
        };
        for candidate_symbol in candidates {
            let Ok(Some(connection)) = open_market_history_for_signal_scan(
                market_database,
                &candidate_symbol,
                false,
            ) else {
                continue;
            };
            if let Ok(state) = product_candle_scan_state_at(
                &connection,
                &candidate_symbol,
                exchange,
                product,
                "15m",
                now_seconds,
            ) {
                if scan_state_has_fresh_price(
                    &state,
                    &candidate_symbol,
                    category,
                    now_ns,
                    true,
                ) {
                    return Ok(Some((connection, candidate_symbol)));
                }
            }
        }
    }

    if own_database_exists {
        if let Some(error) = own_open_error {
            return Err(error);
        }
        return open_market_history_for_signal_scan(market_database, storage_symbol, false)
            .map(|connection| connection.map(|connection| (connection, storage_symbol.to_string())));
    }
    if let Some(error) = own_open_error {
        return Err(error);
    }
    Ok(None)
}

fn scan_state_has_fresh_price(
    state: &Option<ProductCandleScanState>,
    symbol: &str,
    category: &str,
    now_ns: i64,
    require_shared_reference: bool,
) -> bool {
    let Some(state) = state.as_ref() else {
        return false;
    };
    !state.is_stale
        && (!require_shared_reference
            || research_proxy::shared::is_shared_source_for_category(&state.source, category))
        && state.latest_bar_time.is_some_and(|bar_time| {
            completed_bar_is_fresh_at(symbol, "15m", bar_time, now_ns)
        })
}

#[allow(clippy::too_many_arguments)]
fn scan_registered_app(
    app_connection: &Connection,
    market_database: &str,
    sync_state: Option<&AppState>,
    exchange_id: &str,
    instruments: impl IntoIterator<Item = RegisteredInstrument>,
    completed_count: &std::sync::atomic::AtomicUsize,
    total_instruments: usize,
    force_selected_refresh: bool,
    active_scoring_profiles: &crate::research_reports::ActiveScoringProfiles,
    output: &mut ScanCycleOutput,
) -> Result<AppScanResult, rusqlite::Error> {
    let mut app_result = AppScanResult {
        fresh_price_count: 0,
        unavailable_price_count: 0,
        generated_plan_count: 0,
        signal_row_count: 0,
        paused_market_closed_count: 0,
    };
    for (instrument_exchange, product_id, category_id, sym, asset, underlying) in instruments {
        if instrument_exchange != exchange_id {
            continue;
        }
        output.scanned_instruments.push((
            product_id.clone(),
            category_id.clone(),
            asset.clone(),
        ));
        let instrument_started = std::time::Instant::now();
        let scan_label = format!("{exchange_id} · {product_id} · {sym}");
        if let Some(state) = sync_state {
            state.begin_scan_symbol(&scan_label);
        }
        let _progress_step = ScanProgressStep {
            completed_count,
            sync_state,
            label: scan_label,
            total: total_instruments,
        };
        let mut cagr_stats = HashMap::new();
        let market_selection = match open_scan_market_history(
            app_connection,
            market_database,
            &instrument_exchange,
            &product_id,
            &category_id,
            &sym,
            &underlying,
        ) {
            Ok(Some(selection)) => selection,
            Ok(None) => {
                app_result.unavailable_price_count += 1;
                output.price_data_unavailable_count += 1;
                let mut diagnosis = signal_alerts::InstrumentReadiness::default();
                diagnosis.issue(signal_alerts::ReadinessIssue {
                    symbol: asset.clone(),
                    reason: "missing_history".into(),
                    timeframe: "15m".into(),
                    ..signal_alerts::ReadinessIssue::default()
                });
                output.readiness.record(
                    &instrument_exchange,
                    &product_id,
                    &sym,
                    diagnosis,
                    &Default::default(),
                    None,
                );
                continue;
            }
            Err(error) => {
                eprintln!("scan skipped unavailable market history for {sym}: {error}");
                app_result.unavailable_price_count += 1;
                output.price_data_unavailable_count += 1;
                let mut diagnosis = signal_alerts::InstrumentReadiness {
                    price_status: "error",
                    ..signal_alerts::InstrumentReadiness::default()
                };
                diagnosis.issue(signal_alerts::ReadinessIssue {
                    symbol: asset.clone(),
                    reason: "data_error".into(),
                    timeframe: "15m".into(),
                    error: Some(error.to_string()),
                    ..signal_alerts::ReadinessIssue::default()
                });
                output.readiness.record(
                    &instrument_exchange,
                    &product_id,
                    &sym,
                    diagnosis,
                    &Default::default(),
                    None,
                );
                continue;
            }
        };
        let (market_connection, market_symbol) = market_selection;
        let connection = &market_connection;
        let is_idx = instrument_exchange == "stockbit" && category_id == "idx_stock";
        // After the IDX session closes, keep evaluating the latest completed
        // close when the provider's final intraday price is stale or missing.
        let allow_stockbit_reference_price = is_idx
            && expected_stockbit_closed_session_day(
                now_secs().min(i64::MAX as u64) as i64,
            )
            .is_some();
        #[cfg(not(test))]
        if is_idx {
            match stockbit_closed_session_checkpoint(
                connection,
                &market_symbol,
                &instrument_exchange,
                &product_id,
                &category_id,
                &asset,
                now_secs().min(i64::MAX as u64) as i64,
            ) {
                Ok(Some(checkpoint)) => {
                    let already_scanned = match stockbit_close_checkpoint_is_saved(app_connection, &checkpoint) {
                        Ok(saved) => saved,
                        Err(error) => {
                            eprintln!("[ScanWorker] Could not read closed-market checkpoint for {sym}: {error}");
                            false
                        }
                    };
                    if already_scanned && !force_selected_refresh {
                        output.scanned_instruments.pop();
                        output.preserved_stockbit.insert((
                            product_id.clone(),
                            category_id.clone(),
                            asset.clone(),
                        ));
                        app_result.paused_market_closed_count += 1;
                        continue;
                    }
                    output.close_checkpoints.push(checkpoint);
                }
                Ok(None) => {}
                Err(error) => {
                    eprintln!("[ScanWorker] Could not verify final Stockbit candle for {sym}: {error}");
                }
            }
        }
        let is_tokenized = instrument_exchange == "tokocrypto" && category_id == "tokenized_stock";
        let retained_stockbit_signal_rows = if is_idx {
            signal_alerts::stockbit_signal_score_rows_for_symbol(app_connection, &asset)?
        } else {
            std::collections::BTreeSet::new()
        };
        let is_multibagger = is_idx && is_multibagger_symbol(&sym);
        let profile_id = match (
            instrument_exchange.as_str(),
            product_id.as_str(),
            category_id.as_str(),
        ) {
            ("stockbit", _, "idx_stock") if is_multibagger => "stockbit_multibagger",
            ("stockbit", _, "idx_stock") => "stockbit_day_trade",
            ("pintu", "pintu_retail_spot", "crypto_spot") => "pintu_retail_spot",
            ("pintu", "pintu_pro_spot", "crypto_spot") => "pintu_pro_spot",
            ("pintu", "pintu_pro_futures", "crypto_perpetual") => "pintu_futures_confluence",
            ("pluang", "pluang_spot", "crypto_spot") => "pluang_spot",
            ("pluang", "pluang_perpetual", "crypto_perpetual") => "pluang_perpetual",
            ("tokocrypto", _, "crypto_spot") => "tokocrypto_spot_day_trade",
            ("tokocrypto", _, "tokenized_stock") => "tokocrypto_tokenized_stock_day_trade",
            _ => continue,
        };
        let profile = profile_by_id(profile_id).expect("scan profile must exist in master data");
        let strategy = catalog_string(profile, "engine_strategy_id").unwrap_or("plan");
        let evidence_class = catalog_string(profile, "evidence_class").unwrap_or("unknown");
        let scan_interval = catalog_string(profile, "timeframe").unwrap_or("15m");
        let exchange = catalog_string(profile, "exchange_code").unwrap_or("UNKNOWN");
        let class = catalog_string(profile, "asset_class_code").unwrap_or("UNKNOWN");
        let time_window =
            catalog_string(profile, "time_window").unwrap_or("App profile unavailable");
        let mut running_plans = Vec::new();
        let mut potential_plans = Vec::new();
        let mut symbol_indicator_alerts = Vec::new();
        let mut standalone_screener_signals = Vec::new();
        let mut symbol_score_updates = Vec::new();
        let running_start = running_plans.len();
        let potential_start = potential_plans.len();
        let mut instrument_readiness = signal_alerts::InstrumentReadiness::default();
        let has_fresh_price = scan_timeframes::append_timeframe_signal_plans_with_closed_market_reference(
            scan_timeframes::TimeframeScanInput {
                app_connection,
                market_connection: connection,
                sync_state,
                exchange_id: &instrument_exchange,
                product_id: &product_id,
                category_id: &category_id,
                symbol: &market_symbol,
                asset: &asset,
                strategy,
                evidence_class,
                class,
                exchange,
                time_window,
                base_timeframe: scan_interval,
                is_tokenized,
                is_multibagger,
                retained_stockbit_signal_rows: &retained_stockbit_signal_rows,
                active_scoring_profiles: Some(active_scoring_profiles),
            },
            &mut running_plans,
            &mut potential_plans,
            &mut cagr_stats,
            &mut symbol_indicator_alerts,
            &mut standalone_screener_signals,
            &mut symbol_score_updates,
            &mut instrument_readiness,
            allow_stockbit_reference_price,
        )?;
        if has_fresh_price {
            app_result.fresh_price_count += 1;
        } else {
            app_result.unavailable_price_count += 1;
            output.price_data_unavailable_count += 1;
        }
        let new_directional_signal = running_plans[running_start..]
            .iter()
            .chain(potential_plans[potential_start..].iter())
            .any(|plan| signal_alerts::signal_screener_row(plan).is_some());
        if new_directional_signal {
            let diagnostic = build_dss_bressert_mtf_annotation(
                connection,
                &market_symbol,
                &instrument_exchange,
                &product_id,
            );
            scan_timeframes::annotate_new_dss_screener_rows(
                &mut running_plans,
                &mut potential_plans,
                running_start,
                potential_start,
                scan_interval,
                &diagnostic,
            );
        }
        let generated_count = running_plans.len() + potential_plans.len();
        output.generated_plan_count += generated_count;
        app_result.generated_plan_count += generated_count;
        let running_rows = running_plans
            .iter()
            .filter_map(signal_alerts::signal_screener_row)
            .collect::<Vec<_>>();
        let potential_rows = potential_plans
            .iter()
            .filter_map(signal_alerts::signal_screener_row)
            .collect::<Vec<_>>();
        let standalone_rows = standalone_screener_signals
            .iter()
            .filter_map(signal_alerts::signal_screener_row)
            .collect::<Vec<_>>();
        let mut accepted_by_timeframe = std::collections::BTreeMap::<String, usize>::new();
        for row in running_rows
            .iter()
            .chain(potential_rows.iter())
            .chain(standalone_rows.iter())
        {
            if let Some(timeframe) = row["timeframe"].as_str() {
                *accepted_by_timeframe.entry(timeframe.into()).or_default() += 1;
            }
        }
        app_result.signal_row_count +=
            running_rows.len() + potential_rows.len() + standalone_rows.len();
        output.readiness.record(
            &instrument_exchange,
            &product_id,
            &market_symbol,
            instrument_readiness,
            &accepted_by_timeframe,
            Some(connection),
        );
        output.running_signal_rows.extend(running_rows);
        output.potential_signal_rows.extend(potential_rows);
        output.standalone_signal_rows.extend(standalone_rows);
        output.score_updates.extend(
            symbol_score_updates
                .iter()
                .filter_map(signal_alerts::compact_stockbit_signal_score_update),
        );
        output
            .user_indicator_alerts
            .append(&mut symbol_indicator_alerts);
        let elapsed = instrument_started.elapsed();
        if elapsed >= std::time::Duration::from_secs(2) {
            eprintln!(
                "[ScanWorker] Slow instrument {exchange_id}/{product_id}/{sym}: {}ms",
                elapsed.as_millis()
            );
        }
    }
    Ok(app_result)
}
