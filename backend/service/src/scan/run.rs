use super::*;

#[cfg(test)]
#[path = "run/shared_market_history_tests.rs"]
mod shared_market_history_tests;

type RegisteredInstrument = (String, String, String, String, String, String);

struct ScanCycleOutput {
    user_indicator_alerts: Vec<Value>,
    running_signal_rows: Vec<Value>,
    potential_signal_rows: Vec<Value>,
    standalone_signal_rows: Vec<Value>,
    readiness: signal_alerts::SignalReadiness,
    score_updates: Vec<signal_alerts::StockbitSignalScoreUpdate>,
    generated_plan_count: usize,
    price_data_unavailable_count: usize,
    app_workers: Vec<Value>,
}

impl ScanCycleOutput {
    fn new() -> Self {
        Self {
            user_indicator_alerts: Vec::new(),
            running_signal_rows: Vec::new(),
            potential_signal_rows: Vec::new(),
            standalone_signal_rows: Vec::new(),
            readiness: signal_alerts::SignalReadiness::new(),
            score_updates: Vec::new(),
            generated_plan_count: 0,
            price_data_unavailable_count: 0,
            app_workers: Vec::new(),
        }
    }
}

pub(super) fn run_rust_scan_inner(
    app_connection: &Connection,
    market_database: &str,
    interval_min: u64,
    sync_state: Option<&AppState>,
) -> Result<Value, rusqlite::Error> {
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
    };

    let registered_count = registered.len();
    let mut app_registrations =
        std::collections::BTreeMap::<String, Vec<RegisteredInstrument>>::new();
    for exchange_id in ["stockbit", "pintu", "pluang", "tokocrypto"] {
        app_registrations.insert(exchange_id.to_string(), Vec::new());
    }
    for instrument in registered {
        app_registrations
            .entry(instrument.0.clone())
            .or_default()
            .push(instrument);
    }

    let mut workers = Vec::new();
    for exchange_id in ["stockbit", "pintu", "pluang", "tokocrypto"] {
        if let Some(instruments) = app_registrations.remove(exchange_id) {
            workers.push(ScanWorker::new(exchange_id.to_string(), instruments));
        }
    }
    // Preserve any additional registered apps without displacing the four
    // app rotation. Unsupported products still follow the existing scan rules.
    for (exchange_id, instruments) in app_registrations {
        workers.push(ScanWorker::new(exchange_id, instruments));
    }
    let mut remaining: Vec<usize> = workers.iter().map(|worker| worker.total).collect();
    let mut cursor = 0usize;
    let mut output = ScanCycleOutput::new();
    let mut completed_count = 0usize;
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
            completed_count,
            registered_count,
            &mut output,
        )?;
        worker.elapsed += started.elapsed();
        worker.result.fresh_price_count += result.fresh_price_count;
        worker.result.unavailable_price_count += result.unavailable_price_count;
        worker.result.generated_plan_count += result.generated_plan_count;
        worker.result.signal_row_count += result.signal_row_count;
        remaining[index] -= 1;
        completed_count += 1;
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
        }));
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
    signal_alerts::replace_signal_screener_snapshot_with_readiness(
        app_connection,
        &signals,
        &output.score_updates,
        output.price_data_unavailable_count,
        registered_count,
        Some(&readiness),
    )?;
    let total_signals = signals.len();
    let signal_master_data = signal_screener_master_data_for_connection(app_connection)?;
    let mut payload = serde_json::json!({
        "summary": {
            "total_signals": total_signals,
            "generated_at": now_str,
            "interval_minutes": interval_min,
            "interval_seconds": interval_min * 60,
            "score_interpretation": "ranking_only_not_probability",
            "engine": "rust_native_zero_allocation",
            "workers": readiness["workers"].clone(),
            "readiness": readiness
        },
        "signals": signals,
        "master_data": signal_master_data
    });

    let mut newly_detected_alerts = Vec::new();
    let mut alert_storage_available = true;
    match signal_alerts::record_scan_signal_alerts(app_connection, signals.iter().cloned()) {
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
    payload["new_signal_alerts"] = serde_json::json!(newly_detected_alerts);
    if !alert_storage_available {
        payload["signal_alert_status"] = serde_json::json!("unavailable");
    }

    Ok(payload)
}

#[derive(Default)]
struct AppScanResult {
    fresh_price_count: usize,
    unavailable_price_count: usize,
    generated_plan_count: usize,
    signal_row_count: usize,
}

struct ScanWorker {
    exchange_id: String,
    total: usize,
    instruments: std::vec::IntoIter<RegisteredInstrument>,
    result: AppScanResult,
    elapsed: std::time::Duration,
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
            result: AppScanResult::default(),
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
    progress_offset: usize,
    total_instruments: usize,
    output: &mut ScanCycleOutput,
) -> Result<AppScanResult, rusqlite::Error> {
    let mut app_result = AppScanResult {
        fresh_price_count: 0,
        unavailable_price_count: 0,
        generated_plan_count: 0,
        signal_row_count: 0,
    };
    for (index, (instrument_exchange, product_id, category_id, sym, asset, underlying)) in
        instruments.into_iter().enumerate()
    {
        if instrument_exchange != exchange_id {
            continue;
        }
        let instrument_started = std::time::Instant::now();
        if let Some(state) = sync_state {
            state.update_scan_progress(
                &format!("{exchange_id} · {sym}"),
                progress_offset.saturating_add(index),
                total_instruments,
            );
        }
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
        let has_fresh_price = scan_timeframes::append_timeframe_signal_plans_with_readiness(
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
            },
            &mut running_plans,
            &mut potential_plans,
            &mut cagr_stats,
            &mut symbol_indicator_alerts,
            &mut standalone_screener_signals,
            &mut symbol_score_updates,
            &mut instrument_readiness,
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
