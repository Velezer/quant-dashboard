use super::*;
use rusqlite::OptionalExtension;

/// Saved Pintu futures signals must still belong to an active verified app
/// contract. The registry may change between scans, so check it at read time.
const PINTU_FUTURES_REGISTRY_GUARD: &str = "(signal_screener_rows.exchange_id <> 'pintu'
    OR signal_screener_rows.product_id <> 'pintu_pro_futures'
    OR EXISTS (
        SELECT 1 FROM app.instruments AS registered
         WHERE registered.exchange_id = signal_screener_rows.exchange_id
           AND registered.product_id = signal_screener_rows.product_id
           AND registered.asset_category_id = signal_screener_rows.asset_category_id
           AND registered.symbol = signal_screener_rows.symbol
           AND registered.is_active = 1
           AND registered.venue_verified = 1
           AND registered.listing_status IN ('venue_active', 'public_listed_snapshot')
    ))";

// Apply official delisting notices even when the last registry refresh failed.
fn is_known_delisted_pintu_signal(row: &rusqlite::Row<'_>) -> Result<bool, rusqlite::Error> {
    let exchange = row.get::<_, String>(0)?;
    let product = row.get::<_, String>(1)?;
    if exchange != "pintu" || product != "pintu_pro_futures" {
        return Ok(false);
    }
    let symbol = row.get::<_, String>(3)?.trim().to_ascii_uppercase();
    let contract = if symbol.ends_with("USDT-PERP") {
        symbol
    } else {
        format!("{symbol}USDT-PERP")
    };
    Ok(crate::pintu_listing::is_announced_delisted_pintu_future(&contract))
}

fn score_model_by_strategy() -> std::collections::HashMap<&'static str, &'static str> {
    crate::master_data::master_data_records("signal_strategies")
        .iter()
        .filter_map(|model| Some((
            crate::master_data::catalog_string(model, "id")?,
            crate::master_data::catalog_string(model, "score_model_id")?,
        )))
        .collect()
}

fn mark_legacy_score(signal: &mut Value, models: &std::collections::HashMap<&str, &str>) {
    let strategy_id = signal["signal_strategy_id"].as_str().unwrap_or_default();
    let stored_model_id = signal["score_basis"]["model_id"].as_str().unwrap_or_default();
    if models.get(strategy_id).is_some_and(|model_id| *model_id != stored_model_id) {
        signal["score_status"] = json!("stale");
        if let Some(basis) = signal["score_basis"].as_object_mut() {
            basis.insert("score_model_status".into(), json!("legacy"));
            basis.insert("status".into(), json!("stale"));
        }
    }
}

fn expected_stockbit_closed_session_day(now: i64) -> Option<i64> {
    let local_seconds = now.saturating_add(quant_core::JAKARTA_UTC_OFFSET_SECONDS);
    let day = local_seconds.div_euclid(quant_core::SECONDS_PER_DAY);
    let weekday = (day + 4).rem_euclid(7); // Sunday = 0, Monday = 1.
    let second_of_day = local_seconds.rem_euclid(quant_core::SECONDS_PER_DAY);
    if (1..=5).contains(&weekday) {
        (second_of_day >= 16 * 60 * 60 + 15 * 60).then_some(day)
    } else if weekday == 6 {
        Some(day - 1)
    } else if weekday == 0 {
        Some(day - 2)
    } else {
        None
    }
}

fn has_verified_stockbit_close_reference(
    connection: &Connection,
    signal: &Value,
    now: i64,
) -> bool {
    let Some(session_day) = expected_stockbit_closed_session_day(now) else {
        return false;
    };
    let Some(product_id) = signal["product_id"].as_str() else {
        return false;
    };
    let Some(category_id) = signal["asset_category_id"].as_str() else {
        return false;
    };
    let Some(symbol) = signal["symbol"].as_str() else {
        return false;
    };
    let Some(price_bar_time) = signal["price_bar_time"].as_i64() else {
        return false;
    };
    let Some(source) = signal["data_source"].as_str() else {
        return false;
    };
    if source.is_empty() {
        return false;
    }

    let has_exact_close_checkpoint = connection
        .query_row(
            "SELECT 1 FROM signals.stockbit_close_scan_checkpoints
              WHERE product_id=?1 AND asset_category_id=?2 AND symbol=?3
                AND session_day=?4 AND final_bar_time=?5 AND source=?6
              LIMIT 1",
            rusqlite::params![
                product_id,
                category_id,
                symbol,
                session_day,
                price_bar_time,
                source
            ],
            |_| Ok(()),
        )
        .optional()
        .ok()
        .flatten()
        .is_some();
    if has_exact_close_checkpoint {
        return true;
    }

    // When a provider omits the exchange's final candle, an older completed
    // Stockbit bar is still a valid score reference after the session closes.
    // It can keep a signal visible, but the caller marks it as non-executable.
    let Some(_price) = signal["current_price"]
        .as_f64()
        .filter(|price| price.is_finite() && *price > 0.0)
    else {
        return false;
    };
    let Some(bar_time) = signal["price_bar_time"].as_i64() else {
        return false;
    };
    if bar_time <= 0 || signal["price_timeframe"].as_str() != Some("15m") {
        return false;
    }
    let now_ns = now.saturating_mul(crate::NANOS_PER_SECOND);
    let bar_interval_ns = crate::timeframe_duration_ns("15m").unwrap_or_default();
    let bar_local_day = bar_time
        .div_euclid(crate::NANOS_PER_SECOND)
        .saturating_add(quant_core::JAKARTA_UTC_OFFSET_SECONDS)
        .div_euclid(quant_core::SECONDS_PER_DAY);
    bar_interval_ns > 0
        && bar_time.saturating_add(bar_interval_ns) <= now_ns
        && bar_local_day <= session_day
}

fn update_signal_market_entry_freshness(
    connection: &Connection,
    signal: &mut Value,
    now: i64,
    now_ns: i64,
    scan_interval_seconds: i64,
    strategies: &std::collections::HashMap<&str, &str>,
) -> (bool, bool, bool) {
    let symbol = signal["storage_symbol"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let timeframe = signal["price_timeframe"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let price_bar_time = signal["price_bar_time"].as_i64().unwrap_or_default();
    let signal_timeframe = signal["timeframe"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let is_stockbit_persistent = is_stockbit_signal_score_row(
        signal["exchange_id"].as_str().unwrap_or_default(),
        signal["product_id"].as_str().unwrap_or_default(),
        signal["asset_category_id"].as_str().unwrap_or_default(),
        signal["signal_strategy_id"].as_str().unwrap_or_default(),
        &signal_timeframe,
    );
    signal["persistent_score"] = json!(is_stockbit_persistent);
    mark_legacy_score(signal, strategies);
    let is_verified_closed_reference = is_stockbit_persistent
        && signal["score_basis"]["score_model_status"].as_str() != Some("legacy")
        && has_verified_stockbit_close_reference(connection, signal, now);
    let score_bar_is_forming = signal["score_bar_is_forming"].as_bool().unwrap_or(false);
    let price_reference_only = signal["score_basis"]["price_reference_only"]
        .as_bool()
        .unwrap_or(false);
    let score_bar_time = signal["score_bar_time"].as_i64().unwrap_or(price_bar_time);
    let score_updated_at = signal["last_updated_at"].as_i64().unwrap_or_default();
    let score_is_recently_updated = now.saturating_sub(score_updated_at) < scan_interval_seconds;
    let score_bar_duration_ns =
        crate::timeframe_duration_ns(&signal_timeframe).unwrap_or_default();
    let score_bar_is_current = score_bar_duration_ns > 0
        && score_bar_time <= now_ns
        && score_bar_time.saturating_add(score_bar_duration_ns) > now_ns;
    let is_current_price_fresh = if price_reference_only {
        false
    } else if is_stockbit_persistent {
        score_is_recently_updated
            && crate::completed_bar_is_fresh_at(&symbol, &timeframe, price_bar_time, now_ns)
            && if score_bar_is_forming {
                score_bar_is_current
            } else {
                crate::completed_bar_is_fresh_at(
                    &symbol,
                    &signal_timeframe,
                    score_bar_time,
                    now_ns,
                )
            }
    } else {
        crate::completed_bar_is_fresh_at(&symbol, &timeframe, price_bar_time, now_ns)
    };
    if price_reference_only {
        signal["price_reference_status"] = json!("same_source_timeframe_candle");
        signal["market_entry_status"] = json!("NO_MARKET_ENTRY");
    }
    if is_stockbit_persistent && !is_current_price_fresh && !is_verified_closed_reference {
        signal["score_status"] = json!("stale");
        if let Some(score_basis) = signal["score_basis"].as_object_mut() {
            score_basis.insert("status".into(), json!("stale"));
        }
    }
    signal["price_is_fresh"] = json!(is_current_price_fresh && !is_verified_closed_reference);
    if is_verified_closed_reference {
        signal["market_entry_status"] = json!("NO_MARKET_ENTRY");
        signal["price_reference_status"] = json!("market_closed_reference");
        if signal["score_status"].as_str() == Some("stale") {
            signal["score_status"] = json!("current");
            if signal["score_basis"]["status"].as_str() == Some("stale") {
                signal["score_basis"]["status"] = json!("current");
            }
        }
    }
    let score_is_current = signal["score_status"].as_str() != Some("stale");
    (
        (is_current_price_fresh || is_verified_closed_reference) && score_is_current,
        is_verified_closed_reference
            || (!is_current_price_fresh && !is_stockbit_persistent),
        !score_is_current,
    )
}

fn score_indicator_name(filter: &str) -> Option<&'static str> {
    match filter {
        "nwe" => Some("Nadaraya-Watson Envelope"),
        "dss_bressert" => Some("DSS Bressert"),
        "relative_volume" => Some("Relative Volume"),
        "delta_proxy" => Some("Estimated Volume Delta (OHLCV proxy)"),
        _ => None,
    }
}

fn direction_score_key(signal: &Value) -> (String, String, String, String) {
    (
        signal["exchange_id"]
            .as_str()
            .unwrap_or_default()
            .to_ascii_lowercase(),
        signal["asset_category_id"]
            .as_str()
            .unwrap_or_default()
            .to_ascii_lowercase(),
        signal["symbol"]
            .as_str()
            .unwrap_or_default()
            .trim()
            .to_ascii_uppercase(),
        signal["timeframe"]
            .as_str()
            .unwrap_or_default()
            .to_ascii_lowercase(),
    )
}

fn win_rate_confidence_interval_95_pct(wins: u64, trades: u64) -> Option<[f64; 2]> {
    if trades == 0 || wins > trades {
        return None;
    }
    let z = 1.959_963_984_540_054;
    let n = trades as f64;
    let p = wins as f64 / n;
    let denominator = 1.0 + z * z / n;
    let center = (p + z * z / (2.0 * n)) / denominator;
    let half_width = z * (p * (1.0 - p) / n + z * z / (4.0 * n * n)).sqrt() / denominator;
    Some([
        (center - half_width).max(0.0) * 100.0,
        (center + half_width).min(1.0) * 100.0,
    ])
}

fn ticker_holdout_win_rate(report: Option<&str>) -> Option<Value> {
    let report = serde_json::from_str::<Value>(report?).ok()?;
    let rate = report["win_rate_after_fees_pct"]
        .as_f64()
        .filter(|rate| rate.is_finite() && (0.0..=100.0).contains(rate))?;
    let trades = report["trades"].as_u64().filter(|trades| *trades > 0)?;
    let wins = report["wins"].as_u64().filter(|wins| *wins <= trades)?;
    Some(json!({
        "status": "TICKER_HOLDOUT_RATE",
        "metric_type": "historical_win_rate",
        "win_probability_pct": rate,
        "confidence_interval_95_pct": win_rate_confidence_interval_95_pct(wins, trades),
        "sample_trades": trades,
        "wins": wins,
        "basis": "Exact registered ticker, model and timeframe holdout after configured venue fees; historical win rate, not matched to this signal's score; long and short outcomes pooled; interval does not adjust for correlated outcomes; spread, slippage, market impact, funding and borrow excluded",
        "fees_applied": true,
        "direction_conditioned": false,
        "score_conditioned": false,
        "computed_at": report["computed_at"],
        "source_family": report["source_family"],
    }))
}

fn has_score_matched_win_probability(value: &Value) -> bool {
    matches!(
        value["status"].as_str(),
        Some("ESTIMATED" | "PROFILE_GATE_ESTIMATE")
    ) && value["win_probability_pct"]
        .as_f64()
        .is_some_and(|rate| rate.is_finite() && (0.0..=100.0).contains(&rate))
        && value["sample_trades"].as_u64().is_some_and(|trades| trades > 0)
}

fn load_ticker_holdout_win_rate(
    connection: &Connection,
    signal: &Value,
    score_model_id: &str,
) -> Result<Option<Value>, rusqlite::Error> {
    let report = connection
        .query_row(
            "SELECT json_object(
                'win_rate_after_fees_pct',json_extract(model.value,'$.backtest.holdout.win_rate_after_fees_pct'),
                'trades',json_extract(model.value,'$.backtest.holdout.trades'),
                'wins',json_extract(model.value,'$.backtest.holdout.wins'),
                'computed_at',validation_report.computed_at,
                'source_family',json_extract(validation_report.report_json,'$.source.id'))
               FROM app.instruments AS validation_instrument
               JOIN app.entry_model_validation_reports AS validation_report
                 ON validation_report.instrument_id=validation_instrument.instrument_id
                AND validation_report.timeframe=?5
                AND validation_report.protocol_id=?6
               JOIN json_each(
                    CASE WHEN json_valid(validation_report.report_json)
                         THEN validation_report.report_json ELSE '{\"models\":[]}' END,
                    '$.models') AS model
                 ON json_extract(model.value,'$.score_model_id')=?7
                AND json_extract(model.value,'$.backtest.holdout.fees_applied')=1
              WHERE validation_instrument.exchange_id=?1
                AND validation_instrument.product_id=?2
                AND validation_instrument.asset_category_id=?3
                AND validation_instrument.symbol=?4
                AND validation_instrument.is_active=1
              LIMIT 1",
            rusqlite::params![
                signal["exchange_id"].as_str().unwrap_or_default(),
                signal["product_id"].as_str().unwrap_or_default(),
                signal["asset_category_id"].as_str().unwrap_or_default(),
                signal["symbol"].as_str().unwrap_or_default(),
                signal["timeframe"].as_str().unwrap_or_default(),
                quant_core::ENTRY_SCORE_VALIDATION_PROTOCOL_ID,
                score_model_id,
            ],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    Ok(ticker_holdout_win_rate(report.as_deref()))
}

fn score_basis_matches_indicator_filter(
    score_basis: &Value,
    indicator_filter: Option<&str>,
    minimum_score: Option<f64>,
) -> bool {
    let Some(indicator) = indicator_filter.and_then(score_indicator_name) else {
        return true;
    };
    if score_basis["score_model_status"].as_str() == Some("legacy") {
        return false;
    }
    let Some(component) = score_basis["components"]
        .as_array()
        .and_then(|components| components.iter().find(|item| item["indicator"].as_str() == Some(indicator)))
    else {
        return false;
    };
    let state = component["state"].as_str().unwrap_or_default();
    if state.is_empty() || state.eq_ignore_ascii_case("UNAVAILABLE") {
        return false;
    }
    let Some(points) = component["points"]
        .as_f64()
        .filter(|points| points.is_finite() && (-10.0..=10.0).contains(points))
    else {
        return false;
    };
    points.abs() >= minimum_score.unwrap_or(0.0).clamp(0.0, 10.0)
}

fn score_meets_render_minimum(
    strategy: String,
    score: f64,
    basis_json: &str,
    strategies: &std::collections::HashMap<&str, &str>,
    models: &Value,
) -> bool {
    let basis = serde_json::from_str::<Value>(basis_json).unwrap_or(Value::Null);
    let mut signal = json!({"signal_strategy_id": strategy, "score": score, "score_basis": basis});
    mark_legacy_score(&mut signal, strategies);
    signal_meets_render_minimum(&signal, models)
}

#[cfg(test)]
pub(super) fn list_signal_screener(
    connection: &Connection,
    scan_interval_seconds: i64,
    exchange_filter: Option<&str>,
    timeframe_filter: Option<&str>,
) -> Result<Value, rusqlite::Error> {
    list_signal_screener_page(
        connection,
        scan_interval_seconds,
        exchange_filter,
        timeframe_filter,
        None,
        None,
        0,
        MAX_SIGNAL_SCREENER_ROWS,
    )
}

#[cfg(test)]
pub(crate) fn list_signal_screener_page(
    connection: &Connection,
    scan_interval_seconds: i64,
    exchange_filter: Option<&str>,
    timeframe_filter: Option<&str>,
    product_filter: Option<&str>,
    search_filter: Option<&str>,
    offset: usize,
    limit: usize,
) -> Result<Value, rusqlite::Error> {
    list_signal_screener_page_with_filters(
        connection,
        scan_interval_seconds,
        exchange_filter,
        timeframe_filter,
        product_filter,
        search_filter,
        None,
        None,
        offset,
        limit,
    )
}

#[cfg(test)]
fn list_signal_screener_page_with_filters(
    connection: &Connection,
    scan_interval_seconds: i64,
    exchange_filter: Option<&str>,
    timeframe_filter: Option<&str>,
    product_filter: Option<&str>,
    search_filter: Option<&str>,
    indicator_filter: Option<&str>,
    indicator_min_score: Option<f64>,
    offset: usize,
    limit: usize,
) -> Result<Value, rusqlite::Error> {
    list_signal_screener_page_with_minimum_total_score(
        connection,
        scan_interval_seconds,
        exchange_filter,
        timeframe_filter,
        product_filter,
        search_filter,
        indicator_filter,
        indicator_min_score,
        None,
        offset,
        limit,
    )
}

pub(crate) fn list_signal_screener_page_with_minimum_total_score(
    connection: &Connection,
    scan_interval_seconds: i64,
    exchange_filter: Option<&str>,
    timeframe_filter: Option<&str>,
    product_filter: Option<&str>,
    search_filter: Option<&str>,
    indicator_filter: Option<&str>,
    indicator_min_score: Option<f64>,
    minimum_total_score: Option<f64>,
    offset: usize,
    limit: usize,
) -> Result<Value, rusqlite::Error> {
    let now = now_secs() as i64;
    let mut query_values = Vec::new();
    let mut where_clause = String::new();
    if let Some(exchange) = exchange_filter {
        where_clause.push_str(&format!(" AND exchange_id=?{}", query_values.len() + 1));
        query_values.push(rusqlite::types::Value::Text(exchange.to_string()));
    }
    let page_size = limit.clamp(1, MAX_SIGNAL_SCREENER_PAGE_SIZE);
    let display_order = signal_screener_display_order_sql();
    let score_model_by_strategy = score_model_by_strategy();
    let master_data = crate::master_data::signal_screener_master_data_for_connection(connection)?;
    let query = format!(
        "SELECT exchange_id,product_id,asset_category_id,symbol,storage_symbol,signal_strategy_id,
                signal_strategy_name,timeframe,direction,score,strategy_strength,confluence_score,
                score_basis_json,current_price,price_timeframe,price_bar_time,signal_bar_time,
                data_source,data_provider,data_quality,evidence_status,evidence_source,last_updated_at
           FROM signals.signal_screener_rows WHERE 1=1{where_clause}
            AND {PINTU_FUTURES_REGISTRY_GUARD}
          ORDER BY {display_order}"
    );
    let mut statement = connection.prepare(&query)?;
    let row_mapper = |row: &rusqlite::Row<'_>| -> Result<Value, rusqlite::Error> {
        let storage_symbol = row.get::<_, String>(4)?;
        let price_timeframe = row.get::<_, String>(14)?;
        let price_bar_time = row.get::<_, i64>(15)?;
        let price_fresh_until_ms = price_bar_time
            .div_euclid(quant_core::NANOS_PER_MILLISECOND)
            .saturating_add(
                crate::freshness_threshold_seconds(&storage_symbol, &price_timeframe)
                    .max(0)
                    .saturating_mul(quant_core::MILLISECONDS_PER_SECOND),
            );
        let score_basis_json = row.get::<_, String>(12)?;
        let score_basis = serde_json::from_str::<Value>(&score_basis_json).unwrap_or(Value::Null);
        let score_status = score_basis["status"].as_str().unwrap_or("stale");
        let score_bar_time = score_basis["score_bar_time"]
            .as_i64()
            .unwrap_or(row.get::<_, i64>(15)?);
        let score_bar_is_forming = score_basis["score_bar_is_forming"]
            .as_bool()
            .unwrap_or(false);
        let direction = row.get::<_, String>(8)?;
        let setup_strength = row.get::<_, f64>(9)?;
        let setup_strength_scale_max = score_basis["score_scale_max"].clone();
        // Keep the saved sign on retained qualifying rows; the live net below
        // is reserved for fresh market entries.
        let last_direction_score = match score_status {
            "qualifying" | "stale" => Some(if direction.eq_ignore_ascii_case("SHORT") { -setup_strength } else { setup_strength }),
            _ => None,
        };
        Ok(json!({
            "exchange_id": row.get::<_, String>(0)?,
            "product_id": row.get::<_, String>(1)?,
            "asset_category_id": row.get::<_, String>(2)?,
            "symbol": row.get::<_, String>(3)?,
            "storage_symbol": storage_symbol,
            "signal_strategy_id": row.get::<_, String>(5)?,
            "signal_strategy_name": row.get::<_, String>(6)?,
            "timeframe": row.get::<_, String>(7)?,
            "direction": direction,
            "last_direction_score": last_direction_score,
            // `score` is the public signed vote; the stored positive model
            // value is exposed separately as `setup_strength`.
            "score": last_direction_score,
            "score_contract": "signed_indicator_sum_v1",
            "score_scale_max": setup_strength_scale_max,
            "setup_strength": setup_strength,
            "setup_strength_scale_max": setup_strength_scale_max,
            "strategy_strength": row.get::<_, Option<f64>>(10)?,
            "confluence_score": row.get::<_, Option<f64>>(11)?,
            "win_probability": score_basis["win_probability"].clone(),
            "score_basis": score_basis,
            "current_price": row.get::<_, f64>(13)?,
            "price_timeframe": price_timeframe,
            "price_bar_time": price_bar_time,
            "price_fresh_until_ms": price_fresh_until_ms,
            "signal_bar_time": row.get::<_, i64>(16)?,
            "data_source": row.get::<_, String>(17)?,
            "data_provider": row.get::<_, String>(18)?,
            "data_quality": row.get::<_, String>(19)?,
            "evidence_status": row.get::<_, String>(20)?,
            "evidence_source": row.get::<_, String>(21)?,
            "last_updated_at": row.get::<_, i64>(22)?,
            "score_status": score_status,
            "score_bar_time": score_bar_time,
            "score_bar_is_forming": score_bar_is_forming,
        }))
    };
    let mut query_rows = statement.query(rusqlite::params_from_iter(query_values.iter()))?;
    let now_ns = now.saturating_mul(crate::NANOS_PER_SECOND);
    let mut stale_price_signal_count = 0usize;
    let mut stale_score_signal_count = 0usize;
    let mut matching_rows = 0usize;
    let mut signals = Vec::with_capacity(page_size);
    let mut direction_score_counts =
        std::collections::HashMap::<(String, String, String, String), (usize, usize, f64)>::new();
    while let Some(row) = query_rows.next()? {
        if is_known_delisted_pintu_signal(row)? { continue; }
        let strategy = row.get::<_, String>(5)?;
        let score = row.get::<_, f64>(9)?;
        let timeframe = row.get::<_, String>(7)?;
        let score_basis_json = row.get::<_, String>(12)?;
        let score_basis = serde_json::from_str::<Value>(&score_basis_json).unwrap_or(Value::Null);
        let is_stockbit_no_entry = is_stockbit_signal_score_row(
            &row.get::<_, String>(0)?,
            &row.get::<_, String>(1)?,
            &row.get::<_, String>(2)?,
            &strategy,
            &timeframe,
        ) && score_basis["status"].as_str() == Some("not_qualifying");
        if !is_stockbit_no_entry
            && !score_meets_render_minimum(
                strategy,
                score,
                &score_basis_json,
                &score_model_by_strategy,
                &master_data["signal_score_models"],
            )
        {
            continue;
        }
        let mut signal = row_mapper(row)?;
        let (is_current, is_stale_price, is_stale_score) = update_signal_market_entry_freshness(
            connection,
            &mut signal,
            now,
            now_ns,
            scan_interval_seconds,
            &score_model_by_strategy,
        );
        let is_current_market_entry = is_current
            && signal["market_entry_status"].as_str() != Some("NO_MARKET_ENTRY")
            && signal["score_basis"]["status"].as_str() != Some("not_qualifying");
        if is_current_market_entry {
            let direction = signal["direction"].as_str().unwrap_or_default();
            let counts = direction_score_counts
                .entry(direction_score_key(&signal))
                .or_default();
            if direction.eq_ignore_ascii_case("LONG") {
                counts.0 = counts.0.saturating_add(1);
            } else if direction.eq_ignore_ascii_case("SHORT") {
                counts.1 = counts.1.saturating_add(1);
            }
            counts.2 += signal["score"].as_f64().unwrap_or_default();
        }
        // Direction scores summarize all current entries for this app and
        // ticker/timeframe, independent of the screener's visible filters.
        if product_filter.is_some_and(|product| {
            !signal["product_id"]
                .as_str()
                .is_some_and(|row_product| row_product.eq_ignore_ascii_case(product))
        }) || timeframe_filter.is_some_and(|timeframe| {
            !signal["timeframe"]
                .as_str()
                .is_some_and(|row_timeframe| row_timeframe.eq_ignore_ascii_case(timeframe))
        }) || search_filter.is_some_and(|search| {
            let needle = search.trim().to_lowercase();
            ![
                signal["symbol"].as_str().unwrap_or_default(),
                signal["signal_strategy_name"].as_str().unwrap_or_default(),
                signal["signal_strategy_id"].as_str().unwrap_or_default(),
            ]
            .into_iter()
            .any(|value| value.to_lowercase().contains(&needle))
        }) {
            continue;
        }
        if !score_basis_matches_indicator_filter(
            &signal["score_basis"],
            indicator_filter,
            indicator_min_score,
        ) {
            continue;
        }
        if is_stale_price {
            stale_price_signal_count = stale_price_signal_count.saturating_add(1);
        }
        if is_stale_score {
            stale_score_signal_count = stale_score_signal_count.saturating_add(1);
        }
        let retain_stale_crypto_setup = is_stale_price
            && !is_stale_score
            && is_crypto_nwe_dss_signal_score_row(
                signal["asset_category_id"].as_str().unwrap_or_default(),
                signal["signal_strategy_id"].as_str().unwrap_or_default(),
                signal["timeframe"].as_str().unwrap_or_default(),
            );
        if !is_current && !retain_stale_crypto_setup {
            continue;
        }
        if minimum_total_score.is_some_and(|minimum| {
            normalized_total_score_out_of_ten(&signal, &master_data["signal_score_models"])
                .is_none_or(|score| score < minimum)
        }) {
            continue;
        }
        if matching_rows >= offset && signals.len() < page_size {
            if !has_score_matched_win_probability(&signal["win_probability"]) {
                let score_model_id = signal["score_basis"]["model_id"]
                    .as_str()
                    .or_else(|| {
                        score_model_by_strategy
                            .get(signal["signal_strategy_id"].as_str().unwrap_or_default())
                            .copied()
                    });
                if let Some(score_model_id) = score_model_id {
                    if let Some(win_rate) =
                        load_ticker_holdout_win_rate(connection, &signal, score_model_id)?
                    {
                        signal["win_probability"] = win_rate;
                    }
                }
            }
            signals.push(signal);
        }
        matching_rows = matching_rows.saturating_add(1);
    }
    for signal in &mut signals {
        // A matching current row may share this ticker/timeframe key with a
        // retained row. Never attach its live net to the stale/held row.
        if signal["price_is_fresh"].as_bool() != Some(true)
            || signal["market_entry_status"].as_str() == Some("NO_MARKET_ENTRY")
            || signal["score_status"].as_str() == Some("stale")
            || signal["score_basis"]["status"].as_str() == Some("not_qualifying")
        {
            continue;
        }
        let Some((long_count, short_count, net_score)) = direction_score_counts
            .get(&direction_score_key(signal))
            .copied()
        else {
            continue;
        };
        let Some(signal_vote) = signal["score"].as_f64() else { continue; };
        signal["direction_score"] = json!({
            "score": signal_vote,
            "net_score": net_score,
            "long_count": long_count,
            "short_count": short_count,
            "scope": "all current qualifying signals for this ticker and timeframe in the selected app",
        });
    }
    let page_offset = offset.min(matching_rows);
    let fetched_rows = signals.len();
    let signal_count = signals.len();
    let status = connection
        .query_row(
            "SELECT last_attempt_at,last_success_at,last_attempt_status,last_error,signal_count,
                    price_data_status,price_data_unavailable_count
               FROM signals.signal_screener_status WHERE id=1",
            [],
            |row| {
                Ok(json!({
                    "last_attempt_at": row.get::<_, i64>(0)?,
                    "last_success_at": row.get::<_, Option<i64>>(1)?,
                    "last_attempt_status": row.get::<_, String>(2)?,
                    "last_error": row.get::<_, String>(3)?,
                    "signal_count": row.get::<_, i64>(4)?,
                    "price_data_status": row.get::<_, String>(5)?,
                    "price_data_unavailable_count": row.get::<_, i64>(6)?,
                }))
            },
        )
        .optional()?
        .unwrap_or_else(|| {
            json!({"last_attempt_status":"waiting","last_error":"","signal_count":0,
                "price_data_status":"unknown","price_data_unavailable_count":0})
        });
    let status = if let Some(exchange_id) = exchange_filter {
        super::app_signal_screener_status(connection, exchange_id)?.unwrap_or(status)
    } else {
        status
    };
    let readiness = match status.get("readiness").filter(|value| !value.is_null()) {
        Some(readiness) => readiness.clone(),
        None => load_scan_readiness(connection)?,
    };
    let last_success = status["last_success_at"].as_i64();
    let stale =
        last_success.is_none_or(|timestamp| now.saturating_sub(timestamp) > scan_interval_seconds);
    Ok(json!({
            "signals": signals,
        "summary": {
            "total_signals": signal_count,
            "matching_rows": matching_rows,
            "offset": page_offset,
            "next_offset": page_offset.saturating_add(fetched_rows),
            "limit": page_size,
            "has_more": page_offset.saturating_add(fetched_rows) < matching_rows,
            "generated_at": last_success,
            "interval_seconds": scan_interval_seconds,
            "interval_minutes": scan_interval_seconds / 60,
            "stale": stale,
            "stale_price_signal_count": stale_price_signal_count,
            "stale_score_signal_count": stale_score_signal_count,
            "readiness": readiness,
            "status": status,
        }
    }))
}

fn normalized_total_score_out_of_ten(signal: &Value, models: &Value) -> Option<f64> {
    let score = signal["setup_strength"]
        .as_f64()
        .filter(|score| score.is_finite())?;
    let strategy_id = signal["signal_strategy_id"].as_str()?;
    let model = models
        .as_array()
        .into_iter()
        .flatten()
        .find(|model| model["id"].as_str() == Some(strategy_id));
    let scale = signal["score_basis"]["score_scale_max"]
        .as_f64()
        .or_else(|| signal["score_scale_max"].as_f64())
        .or_else(|| model.and_then(|model| model["score_scale_max"].as_f64()))
        .filter(|scale| scale.is_finite() && *scale > 0.0)
        .unwrap_or(10.0);
    Some((score / scale * 10.0).clamp(0.0, 10.0))
}

pub(crate) fn count_current_signal_screener_rows(
    connection: &mut Connection,
    scan_interval_seconds: i64,
) -> Result<Value, rusqlite::Error> {
    let transaction = connection.transaction()?;
    let now = now_secs() as i64;
    let now_ns = now.saturating_mul(crate::NANOS_PER_SECOND);
    let score_models = score_model_by_strategy();
    let master_data = crate::master_data::signal_screener_master_data_for_connection(&transaction)?;
    let mut signal_count = 0usize;
    {
        let mut statement = transaction.prepare(&format!(
            "SELECT exchange_id,product_id,asset_category_id,symbol,storage_symbol,
                    signal_strategy_id,timeframe,score,score_basis_json,price_timeframe,
                    price_bar_time,last_updated_at,data_source,current_price
               FROM signals.signal_screener_rows
              WHERE {PINTU_FUTURES_REGISTRY_GUARD}",
        ))?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            if is_known_delisted_pintu_signal(row)? { continue; }
            let strategy = row.get::<_, String>(5)?;
            let score = row.get::<_, f64>(7)?;
            let score_basis_json = row.get::<_, String>(8)?;
            if !score_meets_render_minimum(
                strategy.clone(),
                score,
                &score_basis_json,
                &score_models,
                &master_data["signal_score_models"],
            ) {
                continue;
            }
            let score_basis =
                serde_json::from_str::<Value>(&score_basis_json).unwrap_or(Value::Null);
            let score_status = score_basis["status"].as_str().unwrap_or("stale");
            let score_bar_time = score_basis["score_bar_time"]
                .as_i64()
                .unwrap_or(row.get::<_, i64>(10)?);
            let score_bar_is_forming = score_basis["score_bar_is_forming"]
                .as_bool()
                .unwrap_or(false);
            let mut signal = json!({
                "exchange_id": row.get::<_, String>(0)?,
                "product_id": row.get::<_, String>(1)?,
                "asset_category_id": row.get::<_, String>(2)?,
                "symbol": row.get::<_, String>(3)?,
                "storage_symbol": row.get::<_, String>(4)?,
                "signal_strategy_id": strategy,
                "timeframe": row.get::<_, String>(6)?,
                "score": score,
                "score_basis": score_basis,
                "score_status": score_status,
                "score_bar_time": score_bar_time,
                "score_bar_is_forming": score_bar_is_forming,
                "price_timeframe": row.get::<_, String>(9)?,
                "price_bar_time": row.get::<_, i64>(10)?,
                "last_updated_at": row.get::<_, i64>(11)?,
                "data_source": row.get::<_, String>(12)?,
                "current_price": row.get::<_, f64>(13)?,
            });
            let (is_current, _, _) = update_signal_market_entry_freshness(
                &transaction,
                &mut signal,
                now,
                now_ns,
                scan_interval_seconds,
                &score_models,
            );
            if is_current {
                signal_count = signal_count.saturating_add(1);
            }
        }
    }
    let generated_at = transaction.query_row(
        "SELECT last_success_at FROM signals.signal_screener_status WHERE id=1",
        [], |row| row.get::<_, Option<i64>>(0),
    ).optional()?.flatten();
    let stale = generated_at.is_none_or(|timestamp| now.saturating_sub(timestamp) > scan_interval_seconds);

    transaction.commit()?;

    Ok(json!({
        "signal_count": signal_count,
        "stale": stale,
        "generated_at": generated_at,
    }))
}


fn signal_meets_render_minimum(signal: &Value, models: &Value) -> bool {
    let basis = &signal["score_basis"];
    let score = signal["score"].as_f64();
    let model = models
        .as_array()
        .into_iter()
        .flatten()
        .find(|model| model["id"] == signal["signal_strategy_id"]);
    let scale = basis["score_scale_max"]
        .as_f64()
        .or_else(|| signal["score_scale_max"].as_f64())
        .or_else(|| model.and_then(|model| model["score_scale_max"].as_f64()));
    let minimum_key = match scale {
        Some(60.0) => "minimum_score_out_of_60",
        Some(50.0) => "minimum_score_out_of_50",
        Some(40.0) => "minimum_score_out_of_40",
        Some(20.0) => "minimum_score_out_of_20",
        _ => "minimum_score_out_of_10",
    };
    let stored_model_id = basis["model_id"].as_str();
    let current_model_id = model.and_then(|model| model["score_model_id"].as_str());
    let is_six_indicator_score_model = stored_model_id
        .or(current_model_id)
        .is_some_and(super::super::is_six_indicator_score_model);
    let basis_is_current = basis["score_model_status"].as_str() != Some("legacy")
        && match (stored_model_id, current_model_id) {
            (Some(stored), Some(current)) => stored == current,
            _ => true,
        };
    let minimum_candidate = if is_six_indicator_score_model {
        Some(quant_core::SIX_INDICATOR_MINIMUM_SCORE_OUT_OF_60)
    } else if scale.is_some_and(|scale| scale >= 20.0) {
        if basis_is_current {
            model
                .and_then(|model| model[minimum_key].as_f64())
                .or_else(|| basis[minimum_key].as_f64())
        } else {
            basis[minimum_key].as_f64()
        }
    } else {
        basis["minimum_score_out_of_10"]
            .as_f64()
            .or_else(|| model.and_then(|model| model["minimum_score_out_of_10"].as_f64()))
    };
    let minimum = minimum_candidate.or_else(|| basis["minimum_score"].as_f64());
    match (score, minimum) {
        (Some(score), Some(minimum)) => score >= minimum,
        (Some(_), None) => true,
        (None, _) => false,
    }
}
