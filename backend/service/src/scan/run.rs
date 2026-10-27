use super::*;

type RegisteredInstrument = (String, String, String, String, String);

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
            "SELECT exchange_id, product_id, asset_category_id, storage_symbol, symbol
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

    let mut output = ScanCycleOutput::new();
    let mut completed_count = 0usize;
    for (exchange_id, instruments) in app_registrations {
        let app_instrument_count = instruments.len();
        let app_started = std::time::Instant::now();
        eprintln!("[ScanWorker] Starting {exchange_id} scan worker for {app_instrument_count} registered instruments.");
        let app_result = scan_registered_app(
            app_connection,
            market_database,
            sync_state,
            &exchange_id,
            instruments,
            completed_count,
            registered_count,
            &mut output,
        )?;
        completed_count = completed_count.saturating_add(app_instrument_count);
        if let Some(state) = sync_state {
            state.update_scan_progress(
                &format!("{exchange_id} complete"),
                completed_count,
                registered_count,
            );
        }
        output.app_workers.push(serde_json::json!({
            "exchange_id": exchange_id.clone(),
            "status": "complete",
            "registered_instruments": app_instrument_count,
            "fresh_price_instruments": app_result.fresh_price_count,
            "unavailable_price_instruments": app_result.unavailable_price_count,
            "generated_plans": app_result.generated_plan_count,
            "signal_rows": app_result.signal_row_count,
        }));
        eprintln!(
            "[ScanWorker] Finished {exchange_id}: {} fresh prices, {} signal rows, elapsed {}ms.",
            app_result.fresh_price_count,
            app_result.signal_row_count,
            app_started.elapsed().as_millis()
        );
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
    match signal_alerts::record_indicator_combo_alerts(app_connection, output.user_indicator_alerts) {
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

struct AppScanResult {
    fresh_price_count: usize,
    unavailable_price_count: usize,
    generated_plan_count: usize,
    signal_row_count: usize,
}

#[allow(clippy::too_many_arguments)]
fn scan_registered_app(
    app_connection: &Connection,
    market_database: &str,
    sync_state: Option<&AppState>,
    exchange_id: &str,
    instruments: Vec<RegisteredInstrument>,
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
    for (index, (instrument_exchange, product_id, category_id, sym, asset)) in
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
        let market_connection =
            match open_market_history_for_signal_scan(market_database, &sym, false) {
                Ok(Some(connection)) => connection,
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
                symbol: &sym,
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
                &sym,
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
            &sym,
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
        output.user_indicator_alerts.append(&mut symbol_indicator_alerts);
        let elapsed = instrument_started.elapsed();
        if elapsed >= std::time::Duration::from_secs(2) {
            eprintln!("[ScanWorker] Slow instrument {exchange_id}/{product_id}/{sym}: {}ms", elapsed.as_millis());
        }
    }
    Ok(app_result)
}
