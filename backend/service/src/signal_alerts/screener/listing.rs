use super::ticker_holdout::{
    has_score_matched_win_probability, load_ticker_holdout_win_rates, ticker_holdout_lookup_key,
};
use super::timeframe_signals::{ScreenerMatch, ScreenerMatches};
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

struct SignalScreenerRowSnapshot {
    exchange_id: String,
    product_id: String,
    asset_category_id: String,
    symbol: String,
    storage_symbol: String,
    signal_strategy_id: String,
    signal_strategy_name: String,
    timeframe: String,
    direction: String,
    score: f64,
    strategy_strength: Option<f64>,
    confluence_score: Option<f64>,
    score_basis_json: String,
    current_price: f64,
    price_timeframe: String,
    price_bar_time: i64,
    signal_bar_time: i64,
    data_source: String,
    data_provider: String,
    data_quality: String,
    evidence_status: String,
    evidence_source: String,
    last_updated_at: i64,
}

impl SignalScreenerRowSnapshot {
    fn from_row(row: &rusqlite::Row<'_>) -> Result<Self, rusqlite::Error> {
        Ok(Self {
            exchange_id: row.get(0)?,
            product_id: row.get(1)?,
            asset_category_id: row.get(2)?,
            symbol: row.get(3)?,
            storage_symbol: row.get(4)?,
            signal_strategy_id: row.get(5)?,
            signal_strategy_name: row.get(6)?,
            timeframe: row.get(7)?,
            direction: row.get(8)?,
            score: row.get(9)?,
            strategy_strength: row.get(10)?,
            confluence_score: row.get(11)?,
            score_basis_json: row.get(12)?,
            current_price: row.get(13)?,
            price_timeframe: row.get(14)?,
            price_bar_time: row.get(15)?,
            signal_bar_time: row.get(16)?,
            data_source: row.get(17)?,
            data_provider: row.get(18)?,
            data_quality: row.get(19)?,
            evidence_status: row.get(20)?,
            evidence_source: row.get(21)?,
            last_updated_at: row.get(22)?,
        })
    }
}

// Apply official delisting notices even when the last registry refresh failed.
fn is_unavailable_pintu_signal(row: &SignalScreenerRowSnapshot) -> bool {
    if row.exchange_id != "pintu" || row.product_id != "pintu_pro_futures" {
        return false;
    }
    let symbol = row.symbol.trim().to_ascii_uppercase();
    let symbol = symbol.strip_suffix("-USD").unwrap_or(&symbol).to_string();
    let contract = if symbol.ends_with("USDT-PERP") {
        symbol
    } else {
        format!("{symbol}USDT-PERP")
    };
    crate::pintu_listing::is_unavailable_pintu_future(&contract)
}

fn score_model_by_strategy() -> std::collections::HashMap<&'static str, &'static str> {
    crate::master_data::master_data_records("signal_strategies")
        .iter()
        .filter_map(|model| {
            Some((
                crate::master_data::catalog_string(model, "id")?,
                crate::master_data::catalog_string(model, "score_model_id")?,
            ))
        })
        .collect()
}

fn mark_legacy_score(signal: &mut Value, models: &std::collections::HashMap<&str, &str>) {
    let strategy_id = signal["signal_strategy_id"].as_str().unwrap_or_default();
    let stored_model_id = signal["score_basis"]["model_id"]
        .as_str()
        .unwrap_or_default();
    if models
        .get(strategy_id)
        .is_some_and(|model_id| *model_id != stored_model_id)
    {
        signal["score_status"] = json!("stale");
        signal["market_entry_status"] = json!("NO_MARKET_ENTRY");
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

type StockbitCloseReferenceKey = (String, String, String, i64, String);

fn load_stockbit_close_references(
    connection: &Connection,
    session_day: Option<i64>,
) -> std::collections::HashSet<StockbitCloseReferenceKey> {
    let Some(session_day) = session_day else {
        return std::collections::HashSet::new();
    };
    let Ok(mut statement) = connection.prepare(
        "SELECT product_id,asset_category_id,symbol,final_bar_time,source
           FROM signals.stockbit_close_scan_checkpoints
          WHERE session_day=?1",
    ) else {
        return std::collections::HashSet::new();
    };
    let Ok(rows) = statement.query_map([session_day], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, String>(4)?,
        ))
    }) else {
        return std::collections::HashSet::new();
    };
    rows.filter_map(Result::ok).collect()
}

fn has_verified_stockbit_close_reference(
    signal: &Value,
    session_day: Option<i64>,
    checkpoints: &std::collections::HashSet<StockbitCloseReferenceKey>,
    now: i64,
) -> bool {
    let Some(closed_session_day) = session_day else {
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

    let has_exact_close_checkpoint = checkpoints.contains(&(
        product_id.to_string(),
        category_id.to_string(),
        symbol.to_string(),
        price_bar_time,
        source.to_string(),
    ));
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
        && bar_local_day <= closed_session_day
}

fn update_signal_market_entry_freshness(
    signal: &mut Value,
    now: i64,
    now_ns: i64,
    scan_interval_seconds: i64,
    strategies: &std::collections::HashMap<&str, &str>,
    stockbit_close_session_day: Option<i64>,
    stockbit_close_checkpoints: &std::collections::HashSet<StockbitCloseReferenceKey>,
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
    let signal_timeframe = signal["timeframe"].as_str().unwrap_or_default().to_string();
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
        && has_verified_stockbit_close_reference(
            signal,
            stockbit_close_session_day,
            stockbit_close_checkpoints,
            now,
        );
    let score_bar_is_forming = signal["score_bar_is_forming"].as_bool().unwrap_or(false);
    let price_reference_only = signal["score_basis"]["price_reference_only"]
        .as_bool()
        .unwrap_or(false);
    let score_bar_time = signal["score_bar_time"].as_i64().unwrap_or(price_bar_time);
    let score_updated_at = signal["last_updated_at"].as_i64().unwrap_or_default();
    let score_is_recently_updated = now.saturating_sub(score_updated_at) < scan_interval_seconds;
    let score_bar_duration_ns = crate::timeframe_duration_ns(&signal_timeframe).unwrap_or_default();
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
                crate::completed_bar_is_fresh_at(&symbol, &signal_timeframe, score_bar_time, now_ns)
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
        signal["market_entry_status"] = json!("NO_MARKET_ENTRY");
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
    if is_stockbit_persistent && !score_is_current {
        signal["market_entry_status"] = json!("NO_MARKET_ENTRY");
    }
    (
        (is_current_price_fresh || is_verified_closed_reference) && score_is_current,
        is_verified_closed_reference || (!is_current_price_fresh && !is_stockbit_persistent),
        !score_is_current,
    )
}

pub(super) fn score_indicator_name(filter: &str) -> Option<&'static str> {
    match filter {
        "nwe" => Some("Nadaraya-Watson Envelope"),
        "dss_bressert" => Some("DSS Bressert"),
        "relative_volume" => Some("Relative Volume"),
        "delta_proxy" => Some("Estimated Volume Delta (OHLCV proxy)"),
        "markov_regime" => Some("Markov Regime"),
        "lag_one_return_autocorrelation" => Some("Lag-one Return Autocorrelation"),
        _ => None,
    }
}

fn normalized_indicator_identity(value: &str) -> String {
    value
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_lowercase)
        .collect()
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

pub(super) fn score_basis_matches_indicator_filter(
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
    let accepted = [
        normalized_indicator_identity(indicator),
        normalized_indicator_identity(indicator_filter.unwrap_or_default()),
    ];
    let Some(component) = score_basis["components"].as_array().and_then(|components| {
        components.iter().find(|item| {
            ["indicator", "indicator_id", "id", "key"]
                .into_iter()
                .filter_map(|key| item.get(key).and_then(Value::as_str))
                .any(|identity| {
                    let normalized = normalized_indicator_identity(identity);
                    accepted
                        .iter()
                        .any(|candidate| candidate.as_str() == normalized)
                })
        })
    }) else {
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
    strategy: &str,
    score: f64,
    basis: &Value,
    strategies: &std::collections::HashMap<&str, &str>,
    models: &std::collections::HashMap<&str, &Value>,
) -> bool {
    score_basis_meets_render_minimum(strategy, Some(score), basis, strategies, models)
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
    // Keep rows and readiness on one scan snapshot, then release SQLite before
    // per-row JSON, freshness, and pagination work. This gives batch publishers
    // a short read-lock window while preserving a coherent set of saved rows.
    let transaction = connection.unchecked_transaction()?;
    let snapshot_connection = &transaction;
    let now = now_secs() as i64;
    let stockbit_close_session_day = expected_stockbit_closed_session_day(now);
    // Load close checkpoints with the screener snapshot. The previous per-row
    // lookup ran one SQLite query for every persisted Stockbit score, which
    // made the full Stockbit screener request stall on large histories.
    let stockbit_close_checkpoints = if exchange_filter.is_none_or(|exchange| {
        exchange.eq_ignore_ascii_case("stockbit")
    }) {
        load_stockbit_close_references(snapshot_connection, stockbit_close_session_day)
    } else {
        std::collections::HashSet::new()
    };
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
    let page_size = limit.clamp(1, MAX_SIGNAL_SCREENER_PAGE_SIZE);
    let display_order = signal_screener_display_order_sql();
    let score_model_by_strategy = score_model_by_strategy();
    let master_data =
        crate::master_data::signal_screener_master_data_for_connection(snapshot_connection)?;
    let signal_score_models = signal_score_model_index(&master_data["signal_score_models"]);
    let columns = "exchange_id,product_id,asset_category_id,symbol,storage_symbol,signal_strategy_id,
                signal_strategy_name,timeframe,direction,score,strategy_strength,confluence_score,
                score_basis_json,current_price,price_timeframe,price_bar_time,signal_bar_time,
                data_source,data_provider,data_quality,evidence_status,evidence_source,last_updated_at";
    let query = format!(
        "SELECT {columns}
           FROM signals.signal_screener_rows WHERE 1=1{where_clause}
            AND {PINTU_FUTURES_REGISTRY_GUARD}
          ORDER BY {display_order}"
    );
    let mut statement = snapshot_connection.prepare(&query)?;
    let row_mapper = |row: &SignalScreenerRowSnapshot, score_basis: Value| -> Value {
        let price_fresh_until_ms = row
            .price_bar_time
            .div_euclid(quant_core::NANOS_PER_MILLISECOND)
            .saturating_add(
                crate::freshness_threshold_seconds(&row.storage_symbol, &row.price_timeframe)
                    .max(0)
                    .saturating_mul(quant_core::MILLISECONDS_PER_SECOND),
            );
        let score_status = score_basis["status"].as_str().unwrap_or("stale");
        let score_bar_time = score_basis["score_bar_time"]
            .as_i64()
            .unwrap_or(row.price_bar_time);
        let score_bar_is_forming = score_basis["score_bar_is_forming"]
            .as_bool()
            .unwrap_or(false);
        let setup_strength = row.score;
        let setup_strength_scale_max = score_basis["score_scale_max"].clone();
        // Keep the saved sign on retained qualifying rows; the live net below
        // is reserved for fresh market entries.
        let last_direction_score = match score_status {
            "qualifying" | "stale" => Some(if row.direction.eq_ignore_ascii_case("SHORT") {
                -setup_strength
            } else {
                setup_strength
            }),
            _ => None,
        };
        json!({
            "exchange_id": row.exchange_id,
            "product_id": row.product_id,
            "asset_category_id": row.asset_category_id,
            "symbol": row.symbol,
            "storage_symbol": row.storage_symbol,
            "signal_strategy_id": row.signal_strategy_id,
            "signal_strategy_name": row.signal_strategy_name,
            "timeframe": row.timeframe,
            "direction": row.direction,
            "last_direction_score": last_direction_score,
            // The persisted magnitude and sum-derived direction reconstruct the
            // signed indicator total without changing SQLite score constraints.
            "score": last_direction_score,
            "score_contract": "signed_indicator_sum_v1",
            "score_scale_max": setup_strength_scale_max,
            "setup_strength": setup_strength,
            "setup_strength_scale_max": setup_strength_scale_max,
            "strategy_strength": row.strategy_strength,
            "confluence_score": row.confluence_score,
            "win_probability": score_basis["win_probability"].clone(),
            "risk_reward_plan": score_basis.get("risk_reward_plan").cloned().unwrap_or_else(|| json!({
                "status": "UNAVAILABLE",
                "reason": "Refresh signals to calculate a 1:4 plan with the current configured fee schedule.",
                "minimum_target_after_fee_r_multiple": quant_core::ENTRY_SCORE_MINIMUM_TARGET_AFTER_FEE_R_MULTIPLE,
            })),
            "score_basis": score_basis,
            "current_price": row.current_price,
            "price_timeframe": row.price_timeframe,
            "price_bar_time": row.price_bar_time,
            "price_fresh_until_ms": price_fresh_until_ms,
            "signal_bar_time": row.signal_bar_time,
            "data_source": row.data_source,
            "data_provider": row.data_provider,
            "data_quality": row.data_quality,
            "evidence_status": row.evidence_status,
            "evidence_source": row.evidence_source,
            "last_updated_at": row.last_updated_at,
            "score_status": score_status,
            "score_bar_time": score_bar_time,
            "score_bar_is_forming": score_bar_is_forming,
        })
    };
    let mut query_rows = statement.query(rusqlite::params_from_iter(query_values.iter()))?;
    let mut snapshot_rows = Vec::new();
    while let Some(row) = query_rows.next()? {
        snapshot_rows.push(SignalScreenerRowSnapshot::from_row(row)?);
    }
    drop(query_rows);
    drop(statement);

    let mut status = snapshot_connection
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
    if let Some(exchange_id) = exchange_filter {
        status =
            super::app_signal_screener_status(snapshot_connection, exchange_id)?.unwrap_or(status);
    }
    let readiness = match status.get("readiness").filter(|value| !value.is_null()) {
        Some(readiness) => readiness.clone(),
        None => load_scan_readiness(snapshot_connection)?,
    };
    transaction.commit()?;

    let now_ns = now.saturating_mul(crate::NANOS_PER_SECOND);
    let mut stale_price_signal_count = 0usize;
    let mut stale_score_signal_count = 0usize;
    let mut matches = ScreenerMatches::default();
    let mut direction_score_counts =
        std::collections::HashMap::<(String, String, String, String), (usize, usize, f64)>::new();
    for row in snapshot_rows {
        if is_unavailable_pintu_signal(&row) {
            continue;
        }
        let strategy = row.signal_strategy_id.as_str();
        if crate::master_data::is_retired_entry_strategy(strategy) {
            continue;
        }
        let score = row.score;
        let timeframe = row.timeframe.as_str();
        let score_basis =
            serde_json::from_str::<Value>(&row.score_basis_json).unwrap_or(Value::Null);
        let is_stockbit_no_entry = is_stockbit_signal_score_row(
            &row.exchange_id,
            &row.product_id,
            &row.asset_category_id,
            strategy,
            timeframe,
        ) && score_basis["status"].as_str() == Some("not_qualifying");
        if !is_stockbit_no_entry
            && !score_meets_render_minimum(
                strategy,
                score,
                &score_basis,
                &score_model_by_strategy,
                &signal_score_models,
            )
        {
            continue;
        }
        let mut signal = row_mapper(&row, score_basis);
        let (is_current, is_stale_price, is_stale_score) = update_signal_market_entry_freshness(
            &mut signal,
            now,
            now_ns,
            scan_interval_seconds,
            &score_model_by_strategy,
            stockbit_close_session_day,
            &stockbit_close_checkpoints,
        );
        let is_crypto_timeframe_signal = is_crypto_combined_entry_score_row(
            signal["asset_category_id"].as_str().unwrap_or_default(),
            strategy,
            timeframe,
        );
        let is_pluang_commodities_timeframe_signal = is_pluang_commodities_combined_entry_score_row(
            signal["exchange_id"].as_str().unwrap_or_default(),
            signal["product_id"].as_str().unwrap_or_default(),
            signal["asset_category_id"].as_str().unwrap_or_default(),
            strategy,
            timeframe,
        );
        let is_combined_timeframe_signal =
            is_crypto_timeframe_signal || is_pluang_commodities_timeframe_signal;
        let group_by_timeframe = signal["persistent_score"] == true || is_combined_timeframe_signal;
        let is_current_market_entry = is_current
            && signal["market_entry_status"].as_str() != Some("NO_MARKET_ENTRY")
            && signal["score_basis"]["status"].as_str() != Some("not_qualifying");
        if is_current_market_entry && !group_by_timeframe {
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
        // Independent model votes are counted above, regardless of visible filters.
        if product_filter.is_some_and(|product| {
            !signal["product_id"]
                .as_str()
                .is_some_and(|row_product| row_product.eq_ignore_ascii_case(product))
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
        if is_stale_price && !group_by_timeframe {
            stale_price_signal_count = stale_price_signal_count.saturating_add(1);
        }
        if is_stale_score && !group_by_timeframe {
            stale_score_signal_count = stale_score_signal_count.saturating_add(1);
        }
        let retain_stale_combined_setup =
            is_stale_price && !is_stale_score && is_combined_timeframe_signal;
        let retain_stockbit_setup = signal["persistent_score"] == true
            && signal["score_basis"]["score_model_status"].as_str() != Some("legacy");
        if !is_current && !retain_stale_combined_setup && !retain_stockbit_setup {
            continue;
        }
        if minimum_total_score.is_some_and(|minimum| {
            normalized_total_score_out_of_ten(&signal, &master_data["signal_score_models"])
                .is_none_or(|score| score < minimum)
        }) {
            continue;
        }
        matches.push(
            ScreenerMatch {
                market_entry: is_current_market_entry,
                stale_score: group_by_timeframe && is_stale_score,
                stale_price: group_by_timeframe && is_stale_price,
                signal,
            },
            group_by_timeframe,
        );
    }
    let mut matching_rows = 0usize;
    let mut signals = Vec::with_capacity(page_size);
    let score_models = &master_data["signal_score_models"];
    let mut ordered_matches = matches.into_ordered().collect::<Vec<_>>();
    ordered_matches.sort_by(|left, right| {
        right
            .market_entry
            .cmp(&left.market_entry)
            .then_with(|| {
                let left_strength =
                    normalized_total_score_out_of_ten(&left.signal, score_models).unwrap_or(-1.0);
                let right_strength =
                    normalized_total_score_out_of_ten(&right.signal, score_models).unwrap_or(-1.0);
                right_strength.total_cmp(&left_strength)
            })
            .then_with(|| {
                right.signal["last_updated_at"]
                    .as_i64()
                    .unwrap_or_default()
                    .cmp(&left.signal["last_updated_at"].as_i64().unwrap_or_default())
            })
            .then_with(|| {
                left.signal["symbol"]
                    .as_str()
                    .unwrap_or_default()
                    .cmp(right.signal["symbol"].as_str().unwrap_or_default())
            })
            .then_with(|| {
                left.signal["timeframe"]
                    .as_str()
                    .unwrap_or_default()
                    .cmp(right.signal["timeframe"].as_str().unwrap_or_default())
            })
    });
    for candidate in ordered_matches {
        stale_price_signal_count += usize::from(candidate.stale_price);
        stale_score_signal_count += usize::from(candidate.stale_score);
        if matching_rows >= offset && signals.len() < page_size {
            signals.push(candidate.signal);
        }
        matching_rows = matching_rows.saturating_add(1);
    }
    let holdout_lookup_keys = signals
        .iter()
        .filter(|signal| !has_score_matched_win_probability(&signal["win_probability"]))
        .filter_map(|signal| ticker_holdout_lookup_key(signal, &score_model_by_strategy))
        .collect::<std::collections::BTreeSet<_>>();
    let ticker_holdout_win_rates = load_ticker_holdout_win_rates(connection, &holdout_lookup_keys)?;
    for signal in &mut signals {
        if has_score_matched_win_probability(&signal["win_probability"]) {
            continue;
        }
        let Some(key) = ticker_holdout_lookup_key(signal, &score_model_by_strategy) else {
            continue;
        };
        if let Some(win_rate) = ticker_holdout_win_rates.get(&key) {
            signal["win_probability"] = win_rate.clone();
        }
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
        let Some(signal_vote) = signal["score"].as_f64() else {
            continue;
        };
        let is_single_combined_score = signal["persistent_score"] == true
            || is_crypto_combined_entry_score_row(
                signal["asset_category_id"].as_str().unwrap_or_default(),
                signal["signal_strategy_id"].as_str().unwrap_or_default(),
                signal["timeframe"].as_str().unwrap_or_default(),
            )
            || is_pluang_commodities_combined_entry_score_row(
                signal["exchange_id"].as_str().unwrap_or_default(),
                signal["product_id"].as_str().unwrap_or_default(),
                signal["asset_category_id"].as_str().unwrap_or_default(),
                signal["signal_strategy_id"].as_str().unwrap_or_default(),
                signal["timeframe"].as_str().unwrap_or_default(),
            );
        let (long_count, short_count, net_score) = if is_single_combined_score {
            (
                usize::from(signal_vote > 0.0),
                usize::from(signal_vote < 0.0),
                signal_vote,
            )
        } else {
            let Some(counts) = direction_score_counts.get(&direction_score_key(signal)) else {
                continue;
            };
            *counts
        };
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
    let last_success = status["last_success_at"].as_i64();
    let stale =
        last_success.is_none_or(|timestamp| now.saturating_sub(timestamp) > scan_interval_seconds);
    let mut response = json!({
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
    });
    if offset == 0 {
        response["master_data"] = master_data;
    }
    Ok(response)
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

fn signal_score_model_index(models: &Value) -> std::collections::HashMap<&str, &Value> {
    models
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|model| Some((model["id"].as_str()?, model)))
        .collect()
}

fn score_basis_meets_render_minimum(
    strategy: &str,
    score: Option<f64>,
    basis: &Value,
    strategies: &std::collections::HashMap<&str, &str>,
    models: &std::collections::HashMap<&str, &Value>,
) -> bool {
    let model = models.get(strategy).copied();
    let scale = basis["score_scale_max"]
        .as_f64()
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
    let legacy_model_id = strategies.get(strategy).copied();
    let is_six_indicator_score_model = stored_model_id
        .or(current_model_id)
        .is_some_and(super::super::is_six_indicator_score_model);
    let is_legacy = basis["score_model_status"].as_str() == Some("legacy")
        || legacy_model_id.is_some_and(|current| current != stored_model_id.unwrap_or_default());
    let basis_is_current = !is_legacy
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
