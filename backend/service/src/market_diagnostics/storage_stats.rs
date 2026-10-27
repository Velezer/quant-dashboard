use super::*;

pub const STORAGE_SUMMARY_SYMBOL_LIMIT: usize = 50;

pub fn summarized_market_inventory<'a>(
    inventory: &'a [(String, String)],
) -> impl Iterator<Item = &'a (String, String)> {
    inventory.iter().take(STORAGE_SUMMARY_SYMBOL_LIMIT)
}

#[cfg(not(test))]
fn stored_market_inventory(
    connection: &Connection,
) -> Result<Vec<(String, String)>, rusqlite::Error> {
    let mut statement = connection.prepare(
        "SELECT storage_symbol,database_filename FROM app.market_store_inventory
         ORDER BY upper(storage_symbol),storage_symbol",
    )?;
    let inventory = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect();
    inventory
}

/// Return durable market-store metadata without loading candle values into memory.
#[cfg(not(test))]
pub fn load_storage_stats(
    connection: &Connection,
    market_directory: &str,
    storage_symbol: Option<&str>,
) -> Result<Value, rusqlite::Error> {
    let app_database = std::env::var_os("QUANT_APP_DB")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(DEFAULT_APP_DB));
    let inventory = stored_market_inventory(connection)?;
    let ticker_database_count = i64::try_from(inventory.len()).unwrap_or(i64::MAX);
    let Some(storage_symbol) = storage_symbol else {
        let summary_limit = STORAGE_SUMMARY_SYMBOL_LIMIT as i64;
        let mut total_series = 0_i64;
        let mut market_size_bytes = 0_u64;
        for (symbol, _) in &inventory {
            let path = ticker_database_path(std::path::Path::new(market_directory), symbol)
                .map_err(|error| rusqlite::Error::InvalidParameterName(error.to_string()))?;
            market_size_bytes = market_size_bytes.saturating_add(
                std::fs::metadata(&path)
                    .map(|metadata| metadata.len())
                    .unwrap_or(0),
            );
        }
        let mut summaries = Vec::with_capacity(STORAGE_SUMMARY_SYMBOL_LIMIT);
        for (symbol, inventory_filename) in summarized_market_inventory(&inventory) {
            let path = ticker_database_path(std::path::Path::new(market_directory), symbol)
                .map_err(|error| rusqlite::Error::InvalidParameterName(error.to_string()))?;
            let database_exists = path.is_file();
            let expected_filename = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("");
            let mut series_count = None;
            let status = if inventory_filename != expected_filename {
                "error"
            } else if !database_exists {
                "missing"
            } else {
                match open_market_history_for_symbol(market_directory, symbol, false) {
                    Ok(Some(market_connection)) => match market_connection.query_row(
                        "SELECT COUNT(*) FROM main.series",
                        [],
                        |row| row.get::<_, i64>(0),
                    ) {
                        Ok(count) => {
                            total_series += count;
                            series_count = Some(count);
                            "available"
                        }
                        Err(_) => "error",
                    },
                    Ok(None) => "missing",
                    Err(_) => "error",
                }
            };
            summaries.push(serde_json::json!({
                "storage_symbol": symbol,
                "ticker_database": path.to_string_lossy(),
                "database_exists": database_exists,
                "series_count": series_count,
                "status": status,
                "recovery": match status {
                    "missing" => Some("sync the registered instrument to restore chart history"),
                    "error" => Some("repair or restore this ticker database, then retry the chart"),
                    _ => None,
                },
            }));
        }
        let market_size_mb = round_size_mb(market_size_bytes as f64 / (1024.0 * 1024.0));
        let app_size_mb = round_size_mb(database_size_mb(&app_database));
        let total_size_mb = round_size_mb(market_size_mb + app_size_mb);
        let summary_truncated = ticker_database_count > summary_limit;
        let total_charts = (!summary_truncated).then_some(total_series);
        return Ok(serde_json::json!({
            "status": "summary",
            "service": "rust_quant_engine",
            "version": SERVICE_VERSION,
            "database": market_directory,
            "market_database": market_directory,
            "market_directory": market_directory,
            "app_database": app_database,
            "market_database_count": ticker_database_count,
            "ticker_database_count": ticker_database_count,
            "summary_limit": summary_limit,
            "summary_truncated": summary_truncated,
            "series_summary": summaries,
            "source": "sqlite_local_history",
            "provider": "tokocrypto_native_and_labeled_history",
            "storage_mode": "app_sqlite_plus_per_symbol_market_sqlite",
            "provider_sync_allowed": provider_sync_enabled(),
            "provider_sync_by_exchange": provider_sync_by_exchange(),
            "provider_fallback_by_exchange": provider_fallback_by_exchange(),
            "sync": {"status":"summary_only","latest_failure":null},
            "series": [],
            "recent_runs": [],
            "total_unique_symbols": ticker_database_count,
            "unique_symbols": ticker_database_count,
            "total_charts": total_charts,
            "summary_series_count": total_series,
            "history_coverage": {"target_met_series":0,"under_target_series":0,"missing_series":0},
            "total_size_mb": total_size_mb,
            "total_disk_usage_mb": total_size_mb,
            "primary_storage_mb": total_size_mb,
            "market_database_mb": market_size_mb,
            "app_database_mb": app_size_mb,
            "offline_ready": ticker_database_count > 0,
        }));
    };

    let ticker_path = ticker_database_path(std::path::Path::new(market_directory), storage_symbol)
        .map_err(|error| rusqlite::Error::InvalidParameterName(error.to_string()))?;
    let expected_filename = ticker_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    let inventory_filename = inventory
        .iter()
        .find(|(symbol, _)| symbol == storage_symbol)
        .map(|(_, filename)| filename.as_str());
    if inventory_filename.is_some_and(|filename| filename != expected_filename) {
        return Ok(unavailable_storage_stats(
            "error",
            storage_symbol,
            market_directory,
            &app_database,
            ticker_database_count,
            &ticker_path,
            ticker_path.is_file(),
            Some("ticker inventory filename does not match its canonical storage symbol"),
            "repair the app inventory entry before retrying this ticker",
        ));
    }
    if inventory_filename.is_none() && ticker_path.is_file() {
        return Ok(unavailable_storage_stats(
            "error",
            storage_symbol,
            market_directory,
            &app_database,
            ticker_database_count,
            &ticker_path,
            true,
            Some("ticker database exists without an app inventory entry"),
            "restore the ticker inventory entry or rerun the explicit migration",
        ));
    }
    if inventory_filename.is_none() {
        return Ok(unavailable_storage_stats(
            "missing",
            storage_symbol,
            market_directory,
            &app_database,
            ticker_database_count,
            &ticker_path,
            false,
            None,
            "sync the registered instrument to populate chart history",
        ));
    }
    let Some(market_connection) =
        open_market_history_for_symbol(market_directory, storage_symbol, false)?
    else {
        return Ok(serde_json::json!({
            "status": "missing",
            "service": "rust_quant_engine",
            "version": SERVICE_VERSION,
            "database": market_directory,
            "market_database": market_directory,
            "market_directory": market_directory,
            "app_database": app_database,
            "market_database_count": ticker_database_count,
            "ticker_database_count": ticker_database_count,
            "symbol": storage_symbol,
            "database_exists": false,
            "series": [],
            "recent_runs": [],
            "recovery": "sync the registered instrument to populate chart history",
        }));
    };
    let market_size_mb = round_size_mb(database_size_mb(&ticker_path));
    let app_size_mb = round_size_mb(database_size_mb(&app_database));
    let mut payload = load_market_storage_details(
        &market_connection,
        market_directory,
        storage_symbol,
        &app_database,
        market_size_mb,
        app_size_mb,
    )?;
    if let Some(object) = payload.as_object_mut() {
        let ticker_database = ticker_path.to_string_lossy().into_owned();
        object.insert(
            "market_directory".into(),
            Value::String(market_directory.into()),
        );
        object.insert(
            "ticker_database".into(),
            Value::String(ticker_database.clone()),
        );
        object.insert(
            "ticker_database_count".into(),
            Value::from(ticker_database_count),
        );
        object.insert(
            "market_database_count".into(),
            Value::from(ticker_database_count),
        );
        object.insert("database".into(), Value::String(ticker_database.clone()));
        object.insert("market_database".into(), Value::String(ticker_database));
    }
    Ok(payload)
}

#[cfg(test)]
pub fn load_storage_stats(
    connection: &Connection,
    market_directory: &str,
    storage_symbol: Option<&str>,
) -> Result<Value, rusqlite::Error> {
    let app_database = std::env::var_os("QUANT_APP_DB")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(DEFAULT_APP_DB));
    let app_size_mb = database_size_mb(&app_database);
    let ticker_database_count: i64 = connection.query_row(
        "SELECT COUNT(*) FROM app.market_store_inventory",
        [],
        |row| row.get(0),
    )?;
    let market_dir = std::path::Path::new(market_directory);

    if let Some(storage_symbol) = storage_symbol {
        let ticker_path = ticker_database_path(market_dir, storage_symbol)?;
        let expected_name = ticker_path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        let inventory_name: Option<String> = connection
            .query_row(
                "SELECT database_filename FROM app.market_store_inventory
                 WHERE storage_symbol=?1",
                [storage_symbol],
                |row| row.get(0),
            )
            .optional()?;
        let database_exists = ticker_path.is_file();
        if inventory_name
            .as_deref()
            .is_some_and(|name| name != expected_name)
        {
            return Ok(unavailable_storage_stats(
                "error",
                storage_symbol,
                market_directory,
                &app_database,
                ticker_database_count,
                &ticker_path,
                database_exists,
                Some("ticker inventory filename does not match its canonical storage symbol"),
                "repair the app inventory entry before retrying this ticker",
            ));
        }
        if inventory_name.is_none() && database_exists {
            return Ok(unavailable_storage_stats(
                "error",
                storage_symbol,
                market_directory,
                &app_database,
                ticker_database_count,
                &ticker_path,
                true,
                Some("ticker database exists without an app inventory entry"),
                "restore the ticker inventory entry or rerun the explicit migration",
            ));
        }
        if inventory_name.is_none() || !database_exists {
            return Ok(unavailable_storage_stats(
                "missing",
                storage_symbol,
                market_directory,
                &app_database,
                ticker_database_count,
                &ticker_path,
                false,
                None,
                "sync the registered instrument to populate chart history",
            ));
        }
        let market_size_mb = database_size_mb(&ticker_path);
        let ticker_connection =
            match open_market_history_for_symbol(market_directory, storage_symbol, false) {
                Ok(Some(connection)) => connection,
                Ok(None) => {
                    return Ok(unavailable_storage_stats(
                        "missing",
                        storage_symbol,
                        market_directory,
                        &app_database,
                        ticker_database_count,
                        &ticker_path,
                        false,
                        None,
                        "sync the registered instrument to populate chart history",
                    ))
                }
                Err(error) => {
                    return Ok(unavailable_storage_stats(
                        "error",
                        storage_symbol,
                        market_directory,
                        &app_database,
                        ticker_database_count,
                        &ticker_path,
                        true,
                        Some(&error.to_string()),
                        "repair or restore this ticker database, then retry the chart",
                    ))
                }
            };
        let mut payload = load_market_storage_details(
            &ticker_connection,
            market_directory,
            storage_symbol,
            &app_database,
            market_size_mb,
            app_size_mb,
        )?;
        if let Some(object) = payload.as_object_mut() {
            object.insert(
                "market_directory".into(),
                Value::String(market_directory.into()),
            );
            object.insert(
                "ticker_database".into(),
                Value::String(ticker_path.to_string_lossy().into_owned()),
            );
            object.insert(
                "ticker_database_count".into(),
                Value::from(ticker_database_count),
            );
            object.insert(
                "database".into(),
                Value::String(ticker_path.to_string_lossy().into_owned()),
            );
            object.insert(
                "market_database".into(),
                Value::String(ticker_path.to_string_lossy().into_owned()),
            );
        }
        return Ok(payload);
    }

    let summary_limit = STORAGE_SUMMARY_SYMBOL_LIMIT as i64;
    let mut statement = connection.prepare(
        "SELECT storage_symbol,database_filename FROM app.market_store_inventory
         ORDER BY upper(storage_symbol),storage_symbol",
    )?;
    let mut rows = statement.query([])?;
    let mut series_summary = Vec::new();
    let mut market_size_mb = 0.0;
    while let Some(row) = rows.next()? {
        let symbol = row.get::<_, String>(0)?;
        let database_filename = row.get::<_, String>(1)?;
        let ticker_path = ticker_database_path(market_dir, &symbol)?;
        let expected_name = ticker_path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        let database_exists = ticker_path.is_file();
        market_size_mb += database_size_mb(&ticker_path);
        if series_summary.len() < summary_limit as usize {
            let status = if database_filename != expected_name {
                "error"
            } else if database_exists {
                "available"
            } else {
                "missing"
            };
            series_summary.push(serde_json::json!({
                "storage_symbol": symbol,
                "ticker_database": ticker_path.to_string_lossy(),
                "database_exists": database_exists,
                "status": status,
                "recovery": (status == "missing").then_some("sync the registered instrument to restore chart history"),
            }));
        }
    }
    let market_size_mb = round_size_mb(market_size_mb);
    let app_size_mb = round_size_mb(app_size_mb);
    let total_size_mb = round_size_mb(market_size_mb + app_size_mb);
    Ok(serde_json::json!({
        "status": "summary",
        "service": "rust_quant_engine",
        "version": SERVICE_VERSION,
        "database": market_directory,
        "market_database": market_directory,
        "market_directory": market_directory,
        "app_database": app_database,
        "ticker_database_count": ticker_database_count,
        "summary_limit": summary_limit,
        "summary_truncated": ticker_database_count > summary_limit,
        "series_summary": series_summary,
        "source": "sqlite_local_history",
        "provider": "tokocrypto_native_and_labeled_history",
        "storage_mode": "sqlite_ticker_databases",
        "provider_sync_allowed": provider_sync_enabled(),
        "provider_sync_by_exchange": provider_sync_by_exchange(),
        "provider_fallback_by_exchange": provider_fallback_by_exchange(),
        "sync": {"status":"summary_only","latest_failure":null},
        "series": [],
        "recent_runs": [],
        "total_unique_symbols": ticker_database_count,
        "unique_symbols": ticker_database_count,
        "history_coverage": {"target_met_series":0,"under_target_series":0,"missing_series":0},
        "total_size_mb": total_size_mb,
        "total_disk_usage_mb": total_size_mb,
        "primary_storage_mb": total_size_mb,
        "market_database_mb": market_size_mb,
        "app_database_mb": app_size_mb,
        "offline_ready": ticker_database_count > 0,
    }))
}

fn unavailable_storage_stats(
    status: &str,
    storage_symbol: &str,
    market_directory: &str,
    app_database: &std::path::Path,
    ticker_database_count: i64,
    ticker_path: &std::path::Path,
    database_exists: bool,
    message: Option<&str>,
    recovery: &str,
) -> Value {
    serde_json::json!({
        "status": status,
        "service": "rust_quant_engine",
        "version": SERVICE_VERSION,
        "database": ticker_path.to_string_lossy(),
        "market_database": ticker_path.to_string_lossy(),
        "market_directory": market_directory,
        "app_database": app_database,
        "ticker_database": ticker_path.to_string_lossy(),
        "ticker_database_count": ticker_database_count,
        "market_database_count": ticker_database_count,
        "symbol": storage_symbol,
        "database_exists": database_exists,
        "series": [],
        "recent_runs": [],
        "message": message,
        "recovery": recovery,
    })
}

fn load_market_storage_details(
    connection: &Connection,
    market_database: &str,
    storage_symbol: &str,
    app_database: &std::path::Path,
    market_size_mb: f64,
    app_size_mb: f64,
) -> Result<Value, rusqlite::Error> {
    let mut series_statement = connection.prepare(
        "SELECT s.symbol, s.interval, s.source, s.timezone, s.price_basis, \
         s.instrument_kind, s.last_success, s.last_attempt, s.last_error, \
         COUNT(b.ts) AS rows, MIN(b.ts) AS first_ts, MAX(b.ts) AS last_ts, \
         s.exchange_id, s.data_provider \
         FROM main.series s LEFT JOIN main.bars b \
           ON b.symbol=s.symbol AND b.interval=s.interval AND b.source=s.source \
         WHERE s.symbol=?1 \
         GROUP BY s.symbol, s.interval, s.source ORDER BY s.interval, s.source",
    )?;
    let series = series_statement
        .query_map([storage_symbol], |row| {
            let symbol = row.get::<_, String>(0)?;
            let interval = row.get::<_, String>(1)?;
            let source = row.get::<_, String>(2)?;
            let timezone = row.get::<_, String>(3)?;
            let price_basis = row.get::<_, String>(4)?;
            let instrument_kind = row.get::<_, String>(5)?;
            let last_success = row.get::<_, Option<String>>(6)?;
            let last_attempt = row.get::<_, Option<String>>(7)?;
            let last_error = row.get::<_, Option<String>>(8)?;
            let rows = row.get::<_, i64>(9)?;
            let first_ts = row.get::<_, Option<i64>>(10)?;
            let last_ts = row.get::<_, Option<i64>>(11)?;
            let exchange_id = row.get::<_, String>(12)?;
            let data_provider = row.get::<_, String>(13)?;
            let coverage_days = history_coverage_days(first_ts, last_ts);
            Ok(serde_json::json!({
                "symbol": symbol,
                "interval": interval,
                "source": source,
                "source_label": candle_source_label(&source),
                "source_is_proxy": is_research_proxy_source(&source),
                "shared_reference": research_proxy::shared::is_shared_source(&source),
                "exchange_id": exchange_id,
                "data_provider": data_provider,
                "timezone": timezone,
                "price_basis": price_basis,
                "instrument_kind": instrument_kind,
                "last_success": last_success,
                "last_attempt": last_attempt,
                "last_error": last_error,
                "error_reason": last_error.as_deref().map(sync_error_reason),
                "rows": rows,
                "first_ts": first_ts,
                "last_ts": last_ts,
                "history_target_days": history_target_days(&interval),
                "coverage_days": coverage_days,
                "history_status": history_coverage_status(rows, coverage_days),
                "history_policy": history_acquisition_policy(&interval),
                "history_warning": history_coverage_warning(&interval, rows, coverage_days),
            }))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let mut unique_symbols = std::collections::HashSet::new();
    let mut timeframe_coverage: HashMap<String, usize> = HashMap::new();
    let mut history_target_met_series = 0usize;
    let mut history_under_target_series = 0usize;
    let mut history_missing_series = 0usize;
    for item in &series {
        if let Some(symbol) = item.get("symbol").and_then(Value::as_str) {
            unique_symbols.insert(symbol.to_string());
        }
        if let Some(interval) = item.get("interval").and_then(Value::as_str) {
            *timeframe_coverage.entry(interval.to_string()).or_insert(0) += 1;
        }
        match item.get("history_status").and_then(Value::as_str) {
            Some("TARGET_MET") => history_target_met_series += 1,
            Some("UNDER_TARGET") => history_under_target_series += 1,
            _ => history_missing_series += 1,
        }
    }

    let mut runs_statement = connection.prepare(
        "SELECT id, symbol, interval, source, attempted_at, status, rows_received, error, \
                exchange_id, data_provider \
         FROM main.ingestion_runs WHERE symbol=?1 ORDER BY id DESC LIMIT 20",
    )?;
    let recent_runs = runs_statement
        .query_map([storage_symbol], |row| {
            let error = row.get::<_, Option<String>>(7)?;
            let exchange_id = row.get::<_, String>(8)?;
            let data_provider = row.get::<_, String>(9)?;
            Ok(serde_json::json!({
                "id": row.get::<_, i64>(0)?,
                "symbol": row.get::<_, Option<String>>(1)?,
                "interval": row.get::<_, Option<String>>(2)?,
                "source": row.get::<_, Option<String>>(3)?,
                "source_label": row.get::<_, Option<String>>(3)?.map(|value| candle_source_label(&value)),
                "exchange_id": exchange_id,
                "data_provider": data_provider,
                "attempted_at": row.get::<_, Option<String>>(4)?,
                "status": row.get::<_, Option<String>>(5)?,
                "rows_received": row.get::<_, Option<i64>>(6)?,
                "error": error,
                "error_reason": error.as_deref().map(sync_error_reason),
            }))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let sync = load_sync_diagnostics(connection)?;
    let size_mb = round_size_mb(market_size_mb + app_size_mb);
    Ok(serde_json::json!({
        "status": "operational",
        "service": "rust_quant_engine",
        "version": SERVICE_VERSION,
        "database": market_database,
        "market_database": market_database,
        "app_database": app_database.to_string_lossy(),
        "symbol": storage_symbol,
        "source": "sqlite_local_history",
        "provider": "tokocrypto_native_and_labeled_history",
        "storage_mode": "sqlite_ticker_databases",
        "provider_sync_allowed": provider_sync_enabled(),
        "provider_sync_by_exchange": provider_sync_by_exchange(),
        "provider_fallback_by_exchange": provider_fallback_by_exchange(),
        "sync": sync,
        "series": series,
        "recent_runs": recent_runs,
        "total_unique_symbols": unique_symbols.len(),
        "unique_symbols": unique_symbols.len(),
        "total_charts": series.len(),
        "timeframe_coverage": timeframe_coverage,
        "history_contract": {
            "minimum_calendar_days": MIN_ENGINE_HISTORY_DAYS,
            "daily_weekly_request_range": "10y",
            "intraday_policy": "provider_window_plus_local_accumulation",
            "validated_evidence_requires": "minimum_calendar_days_and_trade_count"
        },
        "history_coverage": {
            "target_met_series": history_target_met_series,
            "under_target_series": history_under_target_series,
            "missing_series": history_missing_series
        },
        "total_size_mb": size_mb,
        "total_disk_usage_mb": size_mb,
        "primary_storage_mb": size_mb,
        "market_database_mb": round_size_mb(market_size_mb),
        "app_database_mb": round_size_mb(app_size_mb),
        "offline_ready": !series.is_empty(),
    }))
}

fn database_size_mb(path: &std::path::Path) -> f64 {
    std::fs::metadata(path)
        .map(|metadata| metadata.len() as f64 / (1024.0 * 1024.0))
        .unwrap_or(0.0)
}

fn round_size_mb(size_mb: f64) -> f64 {
    (size_mb * 100.0).round() / 100.0
}
