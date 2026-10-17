use super::*;

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

    let mut user_indicator_alerts = Vec::new();
    let mut running_signal_rows = Vec::new();
    let mut potential_signal_rows = Vec::new();
    let mut standalone_signal_rows = Vec::new();
    let mut score_updates = Vec::new();
    let mut generated_plan_count = 0usize;
    let registered_count = registered.len();
    let mut price_data_unavailable_count = 0;
    for (index, (exchange_id, product_id, category_id, sym, asset)) in
        registered.into_iter().enumerate()
    {
        if let Some(state) = sync_state {
            state.update_scan_progress(&sym, index, registered_count);
        }
        let mut cagr_stats = HashMap::new();
        let market_connection =
            match open_market_history_for_signal_scan(market_database, &sym, false) {
                Ok(Some(connection)) => connection,
                Ok(None) => {
                    price_data_unavailable_count += 1;
                    continue;
                }
                Err(error) => {
                    eprintln!("scan skipped unavailable market history for {sym}: {error}");
                    price_data_unavailable_count += 1;
                    continue;
                }
            };
        let connection = &market_connection;
        let is_idx = exchange_id == "stockbit" && category_id == "idx_stock";
        let is_tokenized = exchange_id == "tokocrypto" && category_id == "tokenized_stock";
        let retained_stockbit_signal_rows = if is_idx {
            signal_alerts::stockbit_signal_score_rows_for_symbol(app_connection, &asset)?
        } else {
            std::collections::BTreeSet::new()
        };
        let is_multibagger = is_idx && is_multibagger_symbol(&sym);
        let profile_id = match (
            exchange_id.as_str(),
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
        let has_fresh_price = scan_timeframes::append_timeframe_signal_plans(
            scan_timeframes::TimeframeScanInput {
                app_connection,
                market_connection: connection,
                sync_state,
                exchange_id: &exchange_id,
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
        )?;
        if !has_fresh_price {
            price_data_unavailable_count += 1;
        }
        let new_directional_signal = running_plans[running_start..]
            .iter()
            .chain(potential_plans[potential_start..].iter())
            .any(|plan| signal_alerts::signal_screener_row(plan).is_some());
        if new_directional_signal {
            let diagnostic =
                build_dss_bressert_mtf_annotation(connection, &sym, &exchange_id, &product_id);
            scan_timeframes::annotate_new_dss_screener_rows(
                &mut running_plans,
                &mut potential_plans,
                running_start,
                potential_start,
                scan_interval,
                &diagnostic,
            );
        }
        generated_plan_count += running_plans.len() + potential_plans.len();
        running_signal_rows.extend(
            running_plans
                .iter()
                .filter_map(signal_alerts::signal_screener_row),
        );
        potential_signal_rows.extend(
            potential_plans
                .iter()
                .filter_map(signal_alerts::signal_screener_row),
        );
        standalone_signal_rows.extend(
            standalone_screener_signals
                .iter()
                .filter_map(signal_alerts::signal_screener_row),
        );
        score_updates.extend(
            symbol_score_updates
                .iter()
                .filter_map(signal_alerts::compact_stockbit_signal_score_update),
        );
        user_indicator_alerts.append(&mut symbol_indicator_alerts);
    }
    if let Some(state) = sync_state {
        state.update_scan_progress("", registered_count, registered_count);
    }

    let now_str = chrono_or_simple_timestamp();
    running_signal_rows.extend(potential_signal_rows);
    running_signal_rows.extend(standalone_signal_rows);
    let signals = running_signal_rows;
    eprintln!(
        "Signal scan completed: {registered_count} registered instruments, {} with fresh 15m prices, {} directional plans generated, {} screener rows accepted, {} Stockbit timeframe score updates prepared",
        registered_count.saturating_sub(price_data_unavailable_count),
        generated_plan_count,
        signals.len(),
        score_updates.len()
    );
    signal_alerts::replace_signal_screener_snapshot_with_compact_score_updates(
        app_connection,
        &signals,
        &score_updates,
        price_data_unavailable_count,
        registered_count,
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
            "engine": "rust_native_zero_allocation"
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
    match signal_alerts::record_indicator_combo_alerts(app_connection, user_indicator_alerts) {
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
