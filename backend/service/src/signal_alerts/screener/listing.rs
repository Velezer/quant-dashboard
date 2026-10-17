use super::*;
use rusqlite::OptionalExtension;

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
    connection.execute(
        &format!(
            "DELETE FROM signals.signal_screener_rows
              WHERE last_updated_at <= ?1 AND NOT ({})",
            stockbit_signal_score_row_scope_sql()
        ),
        [now.saturating_sub(SIGNAL_MAX_AGE_SECONDS)],
    )?;
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
    let count_query =
        format!("SELECT COUNT(*) FROM signals.signal_screener_rows WHERE 1=1{where_clause}");
    let matching_rows = connection.query_row(
        &count_query,
        rusqlite::params_from_iter(query_values.iter()),
        |row| row.get::<_, usize>(0),
    )?;
    let page_size = limit.clamp(1, MAX_SIGNAL_SCREENER_PAGE_SIZE);
    let page_offset = offset.min(matching_rows);
    let display_order = signal_screener_display_order_sql();
    let query = format!(
        "SELECT exchange_id,product_id,asset_category_id,symbol,storage_symbol,signal_strategy_id,
                signal_strategy_name,timeframe,direction,score,strategy_strength,confluence_score,
                score_basis_json,current_price,price_timeframe,price_bar_time,signal_bar_time,
                data_source,data_provider,data_quality,evidence_status,evidence_source,last_updated_at
           FROM signals.signal_screener_rows WHERE 1=1{where_clause}
          ORDER BY {display_order} LIMIT {page_size} OFFSET {page_offset}"
    );
    let mut statement = connection.prepare(&query)?;
    let row_mapper = |row: &rusqlite::Row<'_>| {
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
            "storage_symbol": row.get::<_, String>(4)?,
            "signal_strategy_id": row.get::<_, String>(5)?,
            "signal_strategy_name": row.get::<_, String>(6)?,
            "timeframe": row.get::<_, String>(7)?,
            "direction": row.get::<_, String>(8)?,
            "score": row.get::<_, f64>(9)?,
            "strategy_strength": row.get::<_, Option<f64>>(10)?,
            "confluence_score": row.get::<_, Option<f64>>(11)?,
            "score_basis": score_basis,
            "current_price": row.get::<_, f64>(13)?,
            "price_timeframe": row.get::<_, String>(14)?,
            "price_bar_time": row.get::<_, i64>(15)?,
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
    let rows = statement.query_map(rusqlite::params_from_iter(query_values.iter()), row_mapper)?;
    let rows = rows.collect::<Result<Vec<_>, _>>()?;
    let fetched_rows = rows.len();
    let now_ns = now.saturating_mul(crate::NANOS_PER_SECOND);
    let mut stale_price_signal_count = 0;
    let signals = rows
        .into_iter()
        .filter_map(|mut signal| {
            let symbol = signal["storage_symbol"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            let timeframe = signal["price_timeframe"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            let price_bar_time = signal["price_bar_time"].as_i64().unwrap_or_default();
            let signal_timeframe = signal["timeframe"].as_str().unwrap_or_default().to_string();
            let signal_bar_time = signal["signal_bar_time"].as_i64().unwrap_or_default();
            let is_stockbit_persistent = is_stockbit_signal_score_row(
                signal["exchange_id"].as_str().unwrap_or_default(),
                signal["product_id"].as_str().unwrap_or_default(),
                signal["asset_category_id"].as_str().unwrap_or_default(),
                signal["signal_strategy_id"].as_str().unwrap_or_default(),
                &signal_timeframe,
            );
            signal["persistent_score"] = json!(is_stockbit_persistent);
            let strategy_id = signal["signal_strategy_id"].as_str().unwrap_or_default();
            let stored_model_id = signal["score_basis"]["model_id"]
                .as_str()
                .unwrap_or_default();
            let current_model_id = crate::master_data::master_data_records("signal_strategies")
                .iter()
                .find(|model| crate::master_data::catalog_string(model, "id") == Some(strategy_id))
                .and_then(|model| crate::master_data::catalog_string(model, "score_model_id"));
            if current_model_id.is_some_and(|model_id| model_id != stored_model_id) {
                signal["score_status"] = json!("stale");
                if let Some(score_basis) = signal["score_basis"].as_object_mut() {
                    score_basis.insert("score_model_status".into(), json!("legacy"));
                    score_basis.insert("status".into(), json!("stale"));
                }
            }
            let score_bar_is_forming = signal["score_bar_is_forming"].as_bool().unwrap_or(false);
            let score_bar_time = signal["score_bar_time"].as_i64().unwrap_or(price_bar_time);
            let score_updated_at = signal["last_updated_at"].as_i64().unwrap_or_default();
            let score_is_recently_updated =
                now.saturating_sub(score_updated_at) < scan_interval_seconds;
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
                            &timeframe,
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
            if !is_current_price_fresh && !is_stockbit_persistent {
                stale_price_signal_count += 1;
            }
            let is_visible = is_stockbit_persistent
                || (!super::super::signal_candle_is_expired(
                    signal_bar_time,
                    &signal_timeframe,
                    now,
                ) && is_current_price_fresh);
            is_visible.then_some(signal)
        })
        .collect::<Vec<_>>();
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
            "status": status,
        }
    }))
}
