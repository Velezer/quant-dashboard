use super::*;
use rusqlite::OptionalExtension;

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

fn update_signal_market_entry_freshness(
    signal: &mut Value,
    now: i64,
    now_ns: i64,
    scan_interval_seconds: i64,
    strategies: &std::collections::HashMap<&str, &str>,
) -> (bool, bool) {
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
    let score_bar_is_forming = signal["score_bar_is_forming"].as_bool().unwrap_or(false);
    let score_bar_time = signal["score_bar_time"].as_i64().unwrap_or(price_bar_time);
    let score_updated_at = signal["last_updated_at"].as_i64().unwrap_or_default();
    let score_is_recently_updated = now.saturating_sub(score_updated_at) < scan_interval_seconds;
    let score_bar_duration_ns =
        crate::timeframe_duration_ns(&signal_timeframe).unwrap_or_default();
    let score_bar_is_current = score_bar_duration_ns > 0
        && score_bar_time <= now_ns
        && score_bar_time.saturating_add(score_bar_duration_ns) > now_ns;
    let is_current_price_fresh = if is_stockbit_persistent {
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
    if is_stockbit_persistent && !is_current_price_fresh {
        signal["score_status"] = json!("stale");
        if let Some(score_basis) = signal["score_basis"].as_object_mut() {
            score_basis.insert("status".into(), json!("stale"));
        }
    }
    signal["price_is_fresh"] = json!(is_current_price_fresh);
    let score_is_current = signal["score_status"].as_str() != Some("stale");
    (
        is_current_price_fresh && score_is_current,
        !is_current_price_fresh && !is_stockbit_persistent,
    )
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
    let now = now_secs() as i64;
    let mut query_values = Vec::new();
    let mut where_clause = String::new();
    if let Some(exchange) = exchange_filter {
        where_clause.push_str(&format!(" AND exchange_id=?{}", query_values.len() + 1));
        query_values.push(rusqlite::types::Value::Text(exchange.to_string()));
    }
    if let Some(timeframe) = timeframe_filter {
        where_clause.push_str(&format!(" AND timeframe=?{}", query_values.len() + 1));
        query_values.push(rusqlite::types::Value::Text(timeframe.to_string()));
    }
    if let Some(product) = product_filter {
        where_clause.push_str(&format!(" AND product_id=?{}", query_values.len() + 1));
        query_values.push(rusqlite::types::Value::Text(product.to_string()));
    }
    if let Some(search) = search_filter {
        where_clause.push_str(&format!(
            " AND (symbol LIKE ?{0} ESCAPE '\\' COLLATE NOCASE OR signal_strategy_name LIKE ?{0} ESCAPE '\\' COLLATE NOCASE OR signal_strategy_id LIKE ?{0} ESCAPE '\\' COLLATE NOCASE)",
            query_values.len() + 1,
        ));
        let escaped_search = search
            .trim()
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        query_values.push(rusqlite::types::Value::Text(format!("%{escaped_search}%")));
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
        Ok(json!({
            "exchange_id": row.get::<_, String>(0)?,
            "product_id": row.get::<_, String>(1)?,
            "asset_category_id": row.get::<_, String>(2)?,
            "symbol": row.get::<_, String>(3)?,
            "storage_symbol": storage_symbol,
            "signal_strategy_id": row.get::<_, String>(5)?,
            "signal_strategy_name": row.get::<_, String>(6)?,
            "timeframe": row.get::<_, String>(7)?,
            "direction": row.get::<_, String>(8)?,
            "score": row.get::<_, f64>(9)?,
            "strategy_strength": row.get::<_, Option<f64>>(10)?,
            "confluence_score": row.get::<_, Option<f64>>(11)?,
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
    let mut matching_rows = 0usize;
    let mut signals = Vec::with_capacity(page_size);
    while let Some(row) = query_rows.next()? {
        let strategy = row.get::<_, String>(5)?;
        let score = row.get::<_, f64>(9)?;
        let score_basis_json = row.get::<_, String>(12)?;
        if !score_meets_render_minimum(
            strategy,
            score,
            &score_basis_json,
            &score_model_by_strategy,
            &master_data["signal_score_models"],
        ) {
            continue;
        }
        let mut signal = row_mapper(row)?;
        let (is_current, is_stale_price) = update_signal_market_entry_freshness(
            &mut signal,
            now,
            now_ns,
            scan_interval_seconds,
            &score_model_by_strategy,
        );
        if is_stale_price {
            stale_price_signal_count = stale_price_signal_count.saturating_add(1);
        }
        if !is_current {
            continue;
        }
        if matching_rows >= offset && signals.len() < page_size {
            signals.push(signal);
        }
        matching_rows = matching_rows.saturating_add(1);
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
            "readiness": load_scan_readiness(connection)?,
            "status": status,
        }
    }))
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
        let mut statement = transaction.prepare(
            "SELECT exchange_id,product_id,asset_category_id,symbol,storage_symbol,
                    signal_strategy_id,timeframe,score,score_basis_json,price_timeframe,
                    price_bar_time,last_updated_at
               FROM signals.signal_screener_rows",
        )?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
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
                "score_status": score_basis_status(&score_basis_json),
                "score_bar_time": score_bar_time(&score_basis_json).unwrap_or(row.get::<_, i64>(10)?),
                "score_bar_is_forming": score_bar_is_forming(&score_basis_json),
                "price_timeframe": row.get::<_, String>(9)?,
                "price_bar_time": row.get::<_, i64>(10)?,
                "last_updated_at": row.get::<_, i64>(11)?,
            });
            let (is_current, _) = update_signal_market_entry_freshness(
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

fn score_basis_status(score_basis_json: &str) -> String {
    serde_json::from_str::<Value>(score_basis_json)
        .ok()
        .and_then(|basis| basis["status"].as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| "stale".to_string())
}

fn score_bar_time(score_basis_json: &str) -> Option<i64> {
    serde_json::from_str::<Value>(score_basis_json)
        .ok()
        .and_then(|basis| basis["score_bar_time"].as_i64())
}

fn score_bar_is_forming(score_basis_json: &str) -> bool {
    serde_json::from_str::<Value>(score_basis_json)
        .ok()
        .and_then(|basis| basis["score_bar_is_forming"].as_bool())
        .unwrap_or(false)
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
        Some(40.0) => "minimum_score_out_of_40",
        Some(20.0) => "minimum_score_out_of_20",
        _ => "minimum_score_out_of_10",
    };
    let stored_model_id = basis["model_id"].as_str();
    let current_model_id = model.and_then(|model| model["score_model_id"].as_str());
    let basis_is_current = basis["score_model_status"].as_str() != Some("legacy")
        && match (stored_model_id, current_model_id) {
            (Some(stored), Some(current)) => stored == current,
            _ => true,
        };
    let minimum_candidate = if scale.is_some_and(|scale| scale >= 20.0) {
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
