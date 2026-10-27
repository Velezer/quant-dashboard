use super::*;

pub(crate) fn load_series_metadata(
    connection: &Connection,
    symbol: &str,
    requested_timeframe: &str,
    force_direct_storage: bool,
) -> Result<Value, rusqlite::Error> {
    let source =
        selected_candle_source(connection, symbol, None)?.unwrap_or_else(|| "unknown".to_string());
    load_series_metadata_from_source(
        connection,
        symbol,
        requested_timeframe,
        force_direct_storage,
        &source,
    )
}

pub(crate) fn load_series_metadata_for_exchange(
    connection: &Connection,
    symbol: &str,
    requested_timeframe: &str,
    force_direct_storage: bool,
    exchange: &str,
) -> Result<Value, rusqlite::Error> {
    let source = selected_candle_source_for_timeframe(
        connection,
        symbol,
        Some(exchange),
        requested_timeframe,
    )?
    .unwrap_or_else(|| "unknown".to_string());
    let mut metadata = load_series_metadata_from_source(
        connection,
        symbol,
        requested_timeframe,
        force_direct_storage,
        &source,
    )?;
    attach_requested_candle_context(connection, symbol, exchange, None, &mut metadata)?;
    Ok(metadata)
}

pub(crate) fn load_product_candle_metadata(
    connection: &Connection,
    symbol: &str,
    exchange: &str,
    product_id: &str,
    timeframe: &str,
) -> Result<Value, rusqlite::Error> {
    let Some((source, data_provider)) = selected_candle_source_metadata_for_product(
        connection, symbol, exchange, product_id, timeframe,
    )?
    else {
        return Ok(serde_json::json!({
            "exists": false,
            "requested_timeframe": timeframe,
            "source": "unknown",
            "data_provider": "unknown",
            "rows": 0,
            "age_seconds": null,
            "stale_after_seconds": freshness_threshold_seconds(symbol, timeframe),
            "is_stale": true,
        }));
    };
    let mut metadata =
        load_series_metadata_from_source(connection, symbol, timeframe, false, &source)?;
    metadata["data_provider"] = serde_json::json!(data_provider);
    attach_requested_candle_context(
        connection,
        symbol,
        exchange,
        Some(product_id),
        &mut metadata,
    )?;
    metadata["product_id"] = serde_json::json!(product_id);
    Ok(metadata)
}

fn attach_requested_candle_context(
    connection: &Connection,
    symbol: &str,
    exchange: &str,
    product: Option<&str>,
    metadata: &mut Value,
) -> Result<(), rusqlite::Error> {
    let app = candle_product_scope(exchange).map_or(exchange, |(app, _)| app);
    let product = product.or_else(|| candle_product_scope(exchange).map(|(_, product)| product));
    metadata["app_exchange_id"] = serde_json::json!(app);
    if metadata["source_is_proxy"] == true {
        let native_error = candle_source_for_exchange(Some(app)).0.map(|native| {
            connection.query_row(
                "SELECT last_error FROM series WHERE symbol=?1 AND exchange_id=?2 AND source=?3
                    AND (?4 IS NULL OR product_id=?4) AND instrument_kind=?5
                    AND last_error IS NOT NULL AND TRIM(last_error)<>''
                 ORDER BY COALESCE(last_attempt,'') DESC LIMIT 1",
                params![symbol,app,native,product,metadata["instrument_kind"].as_str().unwrap_or("")], |row| row.get::<_,String>(0),
            ).optional()
        }).transpose()?.flatten();
        metadata["native_fallback_reason"] = serde_json::json!(native_error);
    }
    Ok(())
}

#[derive(Clone)]
pub(crate) struct ProductCandleScanState {
    pub(crate) source: String,
    pub(crate) data_provider: String,
    pub(crate) is_stale: bool,
    pub(crate) latest_bar_time: Option<i64>,
}

pub(crate) fn product_candle_scan_state_at(
    connection: &Connection,
    symbol: &str,
    exchange: &str,
    product_id: &str,
    timeframe: &str,
    now_seconds: i64,
) -> Result<Option<ProductCandleScanState>, rusqlite::Error> {
    product_candle_scan_state_at_with_preferred_source(
        connection,
        symbol,
        exchange,
        product_id,
        timeframe,
        now_seconds,
        None,
    )
}

pub(crate) fn product_candle_scan_state_at_with_preferred_source(
    connection: &Connection,
    symbol: &str,
    exchange: &str,
    product_id: &str,
    timeframe: &str,
    now_seconds: i64,
    preferred_source: Option<&str>,
) -> Result<Option<ProductCandleScanState>, rusqlite::Error> {
    let Some((source, data_provider)) =
        selected_candle_source_metadata_for_product_with_preferred_source(
            connection,
            symbol,
            exchange,
            product_id,
            timeframe,
            preferred_source,
        )?
    else {
        return Ok(None);
    };
    let latest_bar_time =
        load_series_candles_from_source(connection, symbol, timeframe, 1, &source)?
            .last()
            .map(|bar| bar.time);
    let is_stale = latest_bar_time.map_or(true, |latest_bar_time| {
        let age_seconds = now_seconds
            .saturating_sub(latest_bar_time.div_euclid(NANOS_PER_SECOND))
            .max(0);
        age_seconds > freshness_threshold_seconds(symbol, timeframe)
    });
    Ok(Some(ProductCandleScanState {
        source,
        data_provider,
        is_stale,
        latest_bar_time,
    }))
}

pub(crate) fn load_series_metadata_from_source(
    connection: &Connection,
    symbol: &str,
    requested_timeframe: &str,
    force_direct_storage: bool,
    source: &str,
) -> Result<Value, rusqlite::Error> {
    let storage_timeframe = if force_direct_storage {
        requested_timeframe.to_string()
    } else {
        preferred_storage_timeframe_from_source(connection, symbol, requested_timeframe, source)?
    };
    let derived_from_timeframe =
        (storage_timeframe != requested_timeframe).then_some(storage_timeframe.clone());
    let row = connection
        .query_row(
            "SELECT s.source, s.timezone, s.price_basis, s.instrument_kind,
                    s.last_success, s.last_attempt, s.last_error,
                    COUNT(b.ts), MIN(b.ts), MAX(b.ts), s.exchange_id, s.data_provider
             FROM series s
             LEFT JOIN bars b ON b.symbol=s.symbol AND b.interval=s.interval AND b.source=s.source
             WHERE s.symbol=?1 AND s.interval=?2 AND s.source=?3
             GROUP BY s.symbol, s.interval, s.source",
            params![symbol, storage_timeframe, source],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, Option<i64>>(8)?,
                    row.get::<_, Option<i64>>(9)?,
                    row.get::<_, String>(10)?,
                    row.get::<_, String>(11)?,
                ))
            },
        )
        .optional()?;

    let Some((
        source,
        timezone,
        price_basis,
        instrument_kind,
        last_success,
        last_attempt,
        last_error,
        mut rows,
        mut first_ts,
        mut last_ts,
        exchange_id,
        data_provider,
    )) = row
    else {
        return Ok(serde_json::json!({
            "exists": false,
            "requested_timeframe": requested_timeframe,
            "storage_timeframe": storage_timeframe,
            "derived_from_timeframe": derived_from_timeframe,
            "rows": 0,
            "history_target_days": history_target_days(&storage_timeframe),
            "coverage_days": 0,
            "history_status": "MISSING",
            "history_policy": history_acquisition_policy(requested_timeframe),
            "history_warning": "No locally stored history for this timeframe",
            "is_stale": true,
            "warning": "No locally stored history for this timeframe"
        }));
    };

    if storage_timeframe == "1d" {
        (rows, first_ts, last_ts) = daily_series_span_from_source(connection, symbol, &source)?;
    }

    // Daily chart reads may append a newer partial day derived from lower
    // intervals. Report the effective span of the merged chart while keeping
    // research on the direct daily series via `force_direct_storage`.
    let mut effective_rows = rows;
    let mut effective_first_ts = first_ts;
    let mut effective_last_ts = last_ts;
    if !force_direct_storage && storage_timeframe != requested_timeframe {
        let direct_span = if requested_timeframe == "1d" {
            Some(daily_series_span_from_source(connection, symbol, &source)?)
        } else {
            connection
                .query_row(
                    "SELECT COUNT(ts), MIN(ts), MAX(ts) FROM bars
                 WHERE symbol=?1 AND interval=?2 AND source=?3",
                    params![symbol, requested_timeframe, source],
                    |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, Option<i64>>(1)?,
                            row.get::<_, Option<i64>>(2)?,
                        ))
                    },
                )
                .optional()?
        };
        if let Some((direct_rows, direct_first_ts, direct_last_ts)) = direct_span {
            if direct_rows > 0 {
                effective_rows = direct_rows;
                effective_first_ts = match (direct_first_ts, effective_first_ts) {
                    (Some(direct), Some(lower)) => Some(direct.min(lower)),
                    (Some(direct), None) => Some(direct),
                    (None, lower) => lower,
                };
                effective_last_ts = match (direct_last_ts, effective_last_ts) {
                    (Some(direct), Some(lower)) => Some(direct.max(lower)),
                    (Some(direct), None) => Some(direct),
                    (None, lower) => lower,
                };
            }
        }
        // Metadata must describe the same merged candle stream the chart uses,
        // not only the direct rows in SQLite. This keeps count/coverage and
        // empty-state decisions aligned with the rendered payload.
        if let Ok(merged) = load_series_candles_from_source(
            connection,
            symbol,
            requested_timeframe,
            MAX_CANDLES,
            &source,
        ) {
            effective_rows = merged.len() as i64;
            effective_first_ts = merged.first().map(|bar| bar.time);
            effective_last_ts = merged.last().map(|bar| bar.time);
        }
    }

    let age_seconds = effective_last_ts.map(|timestamp| {
        (now_secs() as i64)
            .saturating_sub(timestamp.div_euclid(NANOS_PER_SECOND))
            .max(0)
    });
    let stale_after_seconds = freshness_threshold_seconds(symbol, requested_timeframe);
    let is_stale = age_seconds
        .map(|age| age > stale_after_seconds)
        .unwrap_or(true);
    let coverage_days = if requested_timeframe == "1d" {
        daily_history_coverage_days(effective_first_ts, effective_last_ts, &timezone)
    } else {
        history_coverage_days(effective_first_ts, effective_last_ts)
    };
    let history_target_days = history_target_days(requested_timeframe);
    let history_status = history_coverage_status(effective_rows, coverage_days);
    let history_warning =
        history_coverage_warning(requested_timeframe, effective_rows, coverage_days);
    let error_reason = last_error.as_deref().map(sync_error_reason);
    let warning = if effective_rows == 0 {
        Some("No locally stored history for this timeframe".to_string())
    } else if let Some(reason) = error_reason {
        Some(format!(
            "Last provider sync failed: {reason}; showing stored history"
        ))
    } else if is_stale {
        Some("Stored history is older than the freshness threshold".to_string())
    } else {
        None
    };
    let native_fallback_reason = if is_research_proxy_source(&source) {
        candle_source_for_exchange(Some(&exchange_id))
            .0
            .and_then(|native_source| {
                connection
                    .query_row(
                        "SELECT last_error FROM series
                      WHERE symbol=?1 AND exchange_id=?2 AND source=?3
                        AND last_error IS NOT NULL AND TRIM(last_error) <> ''
                      ORDER BY COALESCE(last_attempt, '') DESC LIMIT 1",
                        params![symbol, exchange_id, native_source],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()
                    .ok()
                    .flatten()
            })
    } else {
        None
    };

    Ok(serde_json::json!({
        "exists": true,
        "requested_timeframe": requested_timeframe,
        "storage_timeframe": storage_timeframe,
        "derived_from_timeframe": derived_from_timeframe,
        "source": source,
        "source_label": candle_source_label(&source),
        "source_is_proxy": is_research_proxy_source(&source),
        "shared_reference": research_proxy::shared::is_shared_source(&source),
        "exchange_id": exchange_id,
        "data_provider": data_provider,
        "native_fallback_reason": native_fallback_reason,
        "timezone": timezone,
        "price_basis": price_basis,
        "instrument_kind": instrument_kind,
        "last_success": last_success,
        "last_attempt": last_attempt,
        "last_error": last_error,
        "error_reason": error_reason,
        "rows": effective_rows,
        "first_ts": effective_first_ts,
        "last_ts": effective_last_ts,
        "history_target_days": history_target_days,
        "coverage_days": coverage_days,
        "history_status": history_status,
        "history_policy": history_acquisition_policy(requested_timeframe),
        "history_warning": history_warning,
        "age_seconds": age_seconds,
        "market_data_age_seconds": age_seconds,
        "market_data_age_basis": "latest_candle_timestamp",
        "sync_attempt_at": last_attempt,
        "sync_success_at": last_success,
        "stale_after_seconds": stale_after_seconds,
        "is_stale": is_stale,
        "warning": warning,
    }))
}

pub(crate) fn trim_candles(candles: &mut Vec<Candle>, limit: i64) {
    let keep = limit.clamp(1, MAX_CANDLES) as usize;
    if candles.len() > keep {
        let start = candles.len() - keep;
        candles.drain(..start);
    }
}
