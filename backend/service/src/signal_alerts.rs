use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{params, Connection};
use serde_json::{json, Value};

use crate::now_secs;

mod readiness;
use readiness::{load_scan_readiness, save_scan_readiness};
pub(crate) use readiness::{
    InstrumentReadiness, ReadinessIssue, SignalReadiness, TimeframeReadiness,
};

const MAX_SIGNAL_ALERT_HISTORY: usize = 1_000;
const DEFAULT_SIGNAL_ALERT_LIST_LIMIT: i64 = 50;
const MAX_SIGNAL_ALERT_LIST_LIMIT: i64 = 100;
pub(super) const SIGNAL_MAX_AGE_SECONDS: i64 = 2 * quant_core::SECONDS_PER_HOUR;
pub(super) const SIGNAL_SCORE_SUM_TOLERANCE: f64 = 1e-10;
pub(super) const SIGNAL_LAYOUT_VERSION: &str = "1";
pub(super) const SIGNAL_SCORE_SCHEMA_VERSION: &str = "3";

pub(super) fn is_four_indicator_score_model(score_model_id: &str) -> bool {
    matches!(
        score_model_id,
        quant_core::STOCKBIT_NWE_4H_SIGNAL_SCORE_MODEL_ID
            | quant_core::STOCKBIT_DSS_BRESSERT_4H_SIGNAL_SCORE_MODEL_ID
    )
}

pub(super) fn score_components_match(
    components: &[Value],
    four_indicator_score_model: bool,
    component_points_max: f64,
    group_score_max: f64,
    strategy_strength: f64,
    confluence_score: f64,
    total_score: f64,
    group_tolerance_inclusive: bool,
) -> bool {
    if !strategy_strength.is_finite()
        || strategy_strength < 0.0
        || strategy_strength > group_score_max
        || !confluence_score.is_finite()
        || confluence_score < 0.0
        || confluence_score > group_score_max
        || (total_score - strategy_strength - confluence_score).abs() > SIGNAL_SCORE_SUM_TOLERANCE
    {
        return false;
    }
    let Some(component_points) = components
        .iter()
        .map(|component| {
            component
                .get("points")
                .and_then(Value::as_f64)
                .filter(|points| {
                    points.is_finite() && (0.0..=component_points_max).contains(points)
                })
        })
        .collect::<Option<Vec<_>>>()
    else {
        return false;
    };
    let groups_match = |difference: f64| {
        if group_tolerance_inclusive {
            difference.abs() <= SIGNAL_SCORE_SUM_TOLERANCE
        } else {
            difference.abs() < SIGNAL_SCORE_SUM_TOLERANCE
        }
    };
    let expected_component_count = if four_indicator_score_model { 4 } else { 2 };
    let component_groups_match = if four_indicator_score_model {
        component_points.len() == expected_component_count
            && groups_match(component_points[0] + component_points[1] - strategy_strength)
            && groups_match(component_points[2] + component_points[3] - confluence_score)
    } else {
        component_points.len() == expected_component_count
            && groups_match(component_points[0] - strategy_strength)
            && groups_match(component_points[1] - confluence_score)
    };
    component_groups_match
        && (component_points.iter().sum::<f64>() - total_score).abs() <= SIGNAL_SCORE_SUM_TOLERANCE
}

pub(super) fn initialize_schema(connection: &Connection) -> Result<(), rusqlite::Error> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS app.signal_alerts (
             alert_id INTEGER PRIMARY KEY AUTOINCREMENT,
             exchange_id TEXT NOT NULL,
             product_id TEXT NOT NULL DEFAULT '',
             asset_category_id TEXT NOT NULL,
             symbol TEXT NOT NULL,
             storage_symbol TEXT NOT NULL DEFAULT '',
             signal_strategy_id TEXT NOT NULL,
             signal_strategy_name TEXT NOT NULL,
             indicator_ids_json TEXT NOT NULL,
             data_source TEXT NOT NULL DEFAULT 'unknown',
             data_provider TEXT NOT NULL DEFAULT 'unknown',
             timeframe TEXT NOT NULL,
             direction TEXT NOT NULL CHECK(direction IN ('LONG','SHORT')),
             price REAL,
             signal_bar_time INTEGER NOT NULL,
             validation_status TEXT NOT NULL,
             created_at INTEGER NOT NULL DEFAULT (unixepoch()),
             read_at INTEGER,
             UNIQUE(exchange_id,product_id,asset_category_id,symbol,signal_strategy_id,timeframe,signal_bar_time,direction)
         );
         CREATE INDEX IF NOT EXISTS app.idx_signal_alerts_unread_created
             ON signal_alerts(read_at,created_at DESC,alert_id DESC);
         ",
    )?;
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS app.signal_alert_models (
             exchange_id TEXT NOT NULL,
             product_id TEXT NOT NULL,
             asset_category_id TEXT NOT NULL,
             timeframe TEXT NOT NULL,
             indicator_ids_json TEXT NOT NULL,
             updated_at INTEGER NOT NULL DEFAULT (unixepoch()),
             PRIMARY KEY(exchange_id,product_id,asset_category_id,timeframe)
         );",
    )?;
    for (column, definition) in [
        (
            "data_source",
            "ALTER TABLE app.signal_alerts ADD COLUMN data_source TEXT NOT NULL DEFAULT 'unknown'",
        ),
        (
            "data_provider",
            "ALTER TABLE app.signal_alerts ADD COLUMN data_provider TEXT NOT NULL DEFAULT 'unknown'",
        ),
    ] {
        let exists = {
            let mut statement = connection.prepare("PRAGMA app.table_info('signal_alerts')")?;
            let columns = statement.query_map([], |row| row.get::<_, String>(1))?;
            columns
                .collect::<Result<Vec<_>, _>>()?
                .iter()
                .any(|name| name == column)
        };
        if !exists {
            connection.execute_batch(definition)?;
        }
    }
    connection.execute_batch(
        "CREATE INDEX IF NOT EXISTS app.idx_signal_alerts_expiry
             ON signal_alerts(created_at);",
    )?;
    Ok(())
}

#[path = "signal_alerts_lifecycle.rs"]
mod lifecycle;
pub(super) use lifecycle::{persist_alerts, refresh_lifecycles, signal_candle_is_expired};

#[path = "signal_alerts/screener.rs"]
mod screener;
pub(crate) use screener::create_signal_screener_rows_table;
#[cfg(test)]
pub(super) use screener::signal_screener_score_update;
pub(super) use screener::{
    compact_stockbit_signal_score_update, count_current_signal_screener_rows,
    initialize_screener_schema, list_signal_screener_page, record_signal_screener_failure,
    replace_signal_screener_snapshot_with_readiness, signal_screener_row,
    stockbit_signal_score_rows_for_symbol,
};

pub(super) fn reject_legacy_screener_tables(
    connection: &Connection,
) -> Result<(), rusqlite::Error> {
    let legacy_tables_exist = connection.query_row(
        "SELECT EXISTS(
             SELECT 1 FROM app.sqlite_schema
              WHERE type='table' AND name IN ('signal_screener_rows','signal_screener_status')
         )",
        [],
        |row| row.get::<_, bool>(0),
    )?;
    if legacy_tables_exist {
        return Err(rusqlite::Error::InvalidParameterName(
            "signal snapshot tables remain in QUANT_APP_DB; stop the service and run `tooling\\migrate-signal-screener.bat`".into(),
        ));
    }
    Ok(())
}

pub(super) fn record_scan_signal_alerts(
    connection: &Connection,
    candidates: impl Iterator<Item = Value>,
) -> Result<Vec<Value>, rusqlite::Error> {
    persist_alerts(
        connection,
        candidates.filter_map(|candidate| signal_alert_from_candidate(&candidate)),
    )
}

pub(super) fn record_indicator_combo_alerts(
    connection: &Connection,
    alerts: impl IntoIterator<Item = Value>,
) -> Result<Vec<Value>, rusqlite::Error> {
    persist_alerts(connection, alerts.into_iter())
}

mod indicator_models;
pub(super) use indicator_models::{
    evaluate_indicator_models_for_scan, indicator_model_options, save_indicator_model,
};

mod payloads;
use payloads::{
    canonical_exchange_id, first_candidate_string, indicator_description, required_payload_string,
    signal_alert_from_candidate,
};

#[cfg(test)]
#[path = "signal_alerts_compact_candidate_tests.rs"]
mod compact_candidate_tests;

mod history;
pub(super) use history::{list_alerts, mark_all_read};
