use super::super::now_secs;
use super::MAX_SIGNAL_ALERT_HISTORY;
use rusqlite::{params, Connection};
use serde_json::{json, Value};

pub(crate) fn refresh_lifecycles(connection: &Connection) -> Result<(), rusqlite::Error> {
    delete_expired_alerts(connection, now_secs() as i64)
}

fn delete_expired_alerts(connection: &Connection, now: i64) -> Result<(), rusqlite::Error> {
    connection.execute(
        "DELETE FROM app.signal_alerts WHERE created_at <= ?1",
        [now.saturating_sub(super::SIGNAL_ALERT_HISTORY_RETENTION_SECONDS)],
    )?;
    Ok(())
}

pub(crate) fn persist_alerts(
    connection: &Connection,
    alerts: impl Iterator<Item = Value>,
) -> Result<Vec<Value>, rusqlite::Error> {
    let alerts = alerts.collect::<Vec<_>>();
    crate::database::retry_sqlite_busy(|| persist_alerts_once(connection, &alerts))
}

fn persist_alerts_once(
    connection: &Connection,
    alerts: &[Value],
) -> Result<Vec<Value>, rusqlite::Error> {
    connection.execute_batch("SAVEPOINT persist_signal_alerts")?;
    let result = (|| {
        let mut new_alerts = Vec::new();
        for alert in alerts.iter().cloned() {
            let timeframe = alert["timeframe"].as_str().unwrap_or_default();
            let signal_bar_time = alert["signal_bar_time"].as_i64().unwrap_or_default();
            let indicator_ids = alert["indicator_ids"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let indicators_json =
                serde_json::to_string(&indicator_ids).unwrap_or_else(|_| "[]".into());
            let changed = connection.execute(
                "INSERT OR IGNORE INTO app.signal_alerts (
                     exchange_id,product_id,asset_category_id,symbol,signal_strategy_id,
                     signal_strategy_name,indicator_ids_json,data_source,data_provider,
                     timeframe,direction,price,signal_bar_time,validation_status
                 ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
                params![
                    alert["exchange_id"].as_str().unwrap_or_default(),
                    alert["product_id"].as_str().unwrap_or_default(),
                    alert["asset_category_id"].as_str().unwrap_or_default(),
                    alert["symbol"].as_str().unwrap_or_default(),
                    alert["signal_strategy_id"].as_str().unwrap_or_default(),
                    alert["signal_strategy_name"].as_str().unwrap_or_default(),
                    indicators_json,
                    alert["data_source"].as_str().unwrap_or("unknown"),
                    alert["data_provider"].as_str().unwrap_or("unknown"),
                    timeframe,
                    alert["direction"].as_str().unwrap_or_default(),
                    alert["price"].as_f64(),
                    signal_bar_time,
                    alert["validation_status"]
                        .as_str()
                        .unwrap_or("VALIDATION_PENDING"),
                ],
            )?;
            if changed == 0 {
                continue;
            }
            let alert_id = connection.last_insert_rowid();
            let created_at = connection.query_row(
                "SELECT created_at FROM app.signal_alerts WHERE alert_id=?1",
                [alert_id],
                |row| row.get::<_, i64>(0),
            )?;
            let mut new_alert = alert;
            new_alert["alert_id"] = json!(alert_id);
            new_alert["created_at"] = json!(created_at);
            new_alert["is_read"] = json!(false);
            new_alerts.push(new_alert);
        }

        delete_expired_alerts(connection, now_secs() as i64)?;
        connection.execute(
            "DELETE FROM app.signal_alerts WHERE alert_id NOT IN (
                 SELECT alert_id FROM app.signal_alerts
                 ORDER BY created_at DESC,alert_id DESC LIMIT ?1
             )",
            [MAX_SIGNAL_ALERT_HISTORY as i64],
        )?;
        Ok::<_, rusqlite::Error>(new_alerts)
    })();
    match result {
        Ok(new_alerts) => {
            match connection.execute_batch("RELEASE SAVEPOINT persist_signal_alerts") {
                Ok(()) => Ok(new_alerts),
                Err(error) => {
                    let _ = connection.execute_batch(
                        "ROLLBACK TO SAVEPOINT persist_signal_alerts;
                         RELEASE SAVEPOINT persist_signal_alerts;",
                    );
                    Err(error)
                }
            }
        }
        Err(error) => {
            let _ = connection.execute_batch(
                "ROLLBACK TO SAVEPOINT persist_signal_alerts;
                 RELEASE SAVEPOINT persist_signal_alerts;",
            );
            Err(error)
        }
    }
}

#[cfg(test)]
#[path = "signal_alerts_lifecycle_tests.rs"]
mod lifecycle_tests;
