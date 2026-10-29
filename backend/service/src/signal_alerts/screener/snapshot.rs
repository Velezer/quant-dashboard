use super::*;

#[cfg(test)]
pub(crate) fn replace_signal_screener_snapshot(
    connection: &Connection,
    signals: &[Value],
) -> Result<(), rusqlite::Error> {
    replace_signal_screener_snapshot_with_status(connection, signals, 0, 0)
}

#[cfg(test)]
pub(crate) fn replace_signal_screener_snapshot_with_status(
    connection: &Connection,
    signals: &[Value],
    price_data_unavailable_count: usize,
    registered_count: usize,
) -> Result<(), rusqlite::Error> {
    replace_signal_screener_snapshot_with_score_updates(
        connection,
        signals,
        &[],
        price_data_unavailable_count,
        registered_count,
    )
}

#[cfg(test)]
fn replace_signal_screener_snapshot_with_compact_score_updates(
    connection: &Connection,
    signals: &[Value],
    score_updates: &[StockbitSignalScoreUpdate],
    price_data_unavailable_count: usize,
    registered_count: usize,
) -> Result<(), rusqlite::Error> {
    replace_signal_screener_snapshot_with_readiness(
        connection,
        signals,
        score_updates,
        price_data_unavailable_count,
        registered_count,
        None,
    )
}

pub(crate) fn replace_signal_screener_snapshot_with_readiness(
    connection: &Connection,
    signals: &[Value],
    score_updates: &[StockbitSignalScoreUpdate],
    price_data_unavailable_count: usize,
    registered_count: usize,
    readiness: Option<&Value>,
) -> Result<(), rusqlite::Error> {
    crate::database::retry_sqlite_busy(|| {
        replace_signal_screener_snapshot_attempt(
            connection,
            signals,
            score_updates,
            price_data_unavailable_count,
            registered_count,
            readiness,
        )
    })
}

/// Publish a completed app batch while the full scan is still running. The
/// previous global success timestamp is kept until the complete scan commits.
pub(crate) fn publish_signal_screener_app_batch(
    connection: &Connection,
    exchange_id: &str,
    scanned_instruments: &[(String, String, String)],
    clear_app: bool,
    running_signals: &[Value],
    potential_signals: &[Value],
    standalone_signals: &[Value],
    score_updates: &[StockbitSignalScoreUpdate],
) -> Result<(), rusqlite::Error> {
    crate::database::retry_sqlite_busy(|| {
        publish_signal_screener_app_batch_attempt(
            connection,
            exchange_id,
            scanned_instruments,
            clear_app,
            running_signals,
            potential_signals,
            standalone_signals,
            score_updates,
        )
    })
}

fn publish_signal_screener_app_batch_attempt(
    connection: &Connection,
    exchange_id: &str,
    scanned_instruments: &[(String, String, String)],
    clear_app: bool,
    running_signals: &[Value],
    potential_signals: &[Value],
    standalone_signals: &[Value],
    score_updates: &[StockbitSignalScoreUpdate],
) -> Result<(), rusqlite::Error> {
    let now = now_secs() as i64;
    connection.execute_batch("BEGIN IMMEDIATE")?;
    let result = (|| {
        let persistent_row_scope = stockbit_signal_score_row_scope_sql();
        if clear_app {
            connection.execute(
                &format!(
                    "DELETE FROM signals.signal_screener_rows
                      WHERE exchange_id=?1 AND NOT ({persistent_row_scope})"
                ),
                [exchange_id],
            )?;
        } else {
            for (product_id, asset_category_id, symbol) in scanned_instruments {
                connection.execute(
                    &format!(
                        "DELETE FROM signals.signal_screener_rows
                          WHERE exchange_id=?1 AND product_id=?2 AND asset_category_id=?3
                            AND symbol=?4 AND NOT ({persistent_row_scope})"
                    ),
                    params![exchange_id, product_id, asset_category_id, symbol],
                )?;
            }
        }
        if exchange_id == quant_core::STOCKBIT_NWE_4H_EXCHANGE_ID {
            mark_stockbit_score_rows_stale(connection)?;
        }
        let app_signals = running_signals
            .iter()
            .chain(potential_signals)
            .chain(standalone_signals)
            .filter(|signal| signal["exchange_id"].as_str() == Some(exchange_id));
        insert_signal_screener_rows(connection, app_signals, now)?;
        apply_signal_score_updates(
            connection,
            score_updates
                .iter()
                .filter(|update| update.exchange_id == exchange_id),
            now,
        )?;
        let stored_signal_count: i64 = connection.query_row(
            "SELECT COUNT(*) FROM signals.signal_screener_rows",
            [],
            |row| row.get(0),
        )?;
        connection.execute(
            "INSERT INTO signals.signal_screener_status(
                 id,last_attempt_at,last_success_at,last_attempt_status,last_error,
                 signal_count,price_data_status,price_data_unavailable_count
             ) VALUES(1,?1,NULL,'scanning','',?2,'unknown',0)
             ON CONFLICT(id) DO UPDATE SET last_attempt_at=excluded.last_attempt_at,
                last_attempt_status='scanning',last_error='',signal_count=excluded.signal_count",
            params![now, stored_signal_count],
        )?;
        Ok::<(), rusqlite::Error>(())
    })();
    match result {
        Ok(()) => match connection.execute_batch("COMMIT") {
            Ok(()) => Ok(()),
            Err(error) => {
                let _ = connection.execute_batch("ROLLBACK");
                Err(error)
            }
        },
        Err(error) => {
            let _ = connection.execute_batch("ROLLBACK");
            Err(error)
        }
    }
}

fn replace_signal_screener_snapshot_attempt(
    connection: &Connection,
    signals: &[Value],
    score_updates: &[StockbitSignalScoreUpdate],
    price_data_unavailable_count: usize,
    registered_count: usize,
    readiness: Option<&Value>,
) -> Result<(), rusqlite::Error> {
    let now = now_secs() as i64;
    let price_data_status = if price_data_unavailable_count == 0 {
        "ready"
    } else if registered_count > 0 && price_data_unavailable_count >= registered_count {
        "unavailable"
    } else {
        "partial_stale"
    };
    connection.execute_batch("BEGIN IMMEDIATE")?;
    let result = (|| {
        connection.execute(
            &format!(
                "DELETE FROM signals.signal_screener_rows WHERE NOT ({})",
                stockbit_signal_score_row_scope_sql()
            ),
            [],
        )?;
        mark_stockbit_score_rows_stale(connection)?;
        insert_signal_screener_rows(connection, signals.iter(), now)?;
        apply_signal_score_updates(connection, score_updates.iter(), now)?;
        let stored_signal_count: i64 = connection.query_row(
            "SELECT COUNT(*) FROM signals.signal_screener_rows",
            [],
            |row| row.get(0),
        )?;
        connection.execute(
            "INSERT INTO signals.signal_screener_status(id,last_attempt_at,last_success_at,last_attempt_status,last_error,signal_count,price_data_status,price_data_unavailable_count)
             VALUES(1,?1,?1,'ready','',?2,?3,?4)
             ON CONFLICT(id) DO UPDATE SET last_attempt_at=excluded.last_attempt_at,
                last_success_at=excluded.last_success_at,last_attempt_status='ready',last_error='',
                signal_count=excluded.signal_count,price_data_status=excluded.price_data_status,
                price_data_unavailable_count=excluded.price_data_unavailable_count",
            params![
                now,
                stored_signal_count,
                price_data_status,
                price_data_unavailable_count as i64,
            ],
        )?;
        save_scan_readiness(connection, readiness)?;
        Ok::<(), rusqlite::Error>(())
    })();
    match result {
        Ok(()) => match connection.execute_batch("COMMIT") {
            Ok(()) => Ok(()),
            Err(error) => {
                let _ = connection.execute_batch("ROLLBACK");
                Err(error)
            }
        },
        Err(error) => {
            let _ = connection.execute_batch("ROLLBACK");
            Err(error)
        }
    }
}

fn insert_signal_screener_rows<'a>(
    connection: &Connection,
    signals: impl Iterator<Item = &'a Value>,
    now: i64,
) -> Result<(), rusqlite::Error> {
    for signal in signals {
        let score_basis = json!({
            "model_id": signal["score_model_id"],
            "score_scale_max": signal["score_scale_max"],
            "minimum_score_out_of_10": signal["minimum_score_out_of_10"],
            "minimum_score_out_of_20": signal["minimum_score_out_of_20"],
            "minimum_score_out_of_40": signal["minimum_score_out_of_40"],
            "component_score_scale_max": signal["component_score_scale_max"],
            "score_model_status": "current",
            "strategy_strength": signal["strategy_strength"],
            "confluence_score": signal["confluence_score"],
            "components": signal["score_components"],
            "status": "qualifying",
            "score_bar_time": signal["score_bar_time"],
            "score_bar_is_forming": signal["score_bar_is_forming"],
        });
        connection.execute(
            "INSERT OR REPLACE INTO signals.signal_screener_rows (
                exchange_id,product_id,asset_category_id,symbol,storage_symbol,signal_strategy_id,
                signal_strategy_name,timeframe,direction,score,strategy_strength,
                confluence_score,score_basis_json,current_price,price_timeframe,
                price_bar_time,signal_bar_time,data_source,data_provider,data_quality,
                evidence_status,evidence_source,last_updated_at
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23)",
            params![
                signal["exchange_id"].as_str().unwrap_or_default(),
                signal["product_id"].as_str().unwrap_or_default(),
                signal["asset_category_id"].as_str().unwrap_or_default(),
                signal["symbol"].as_str().unwrap_or_default(),
                signal["storage_symbol"].as_str().unwrap_or_default(),
                signal["signal_strategy_id"].as_str().unwrap_or_default(),
                signal["signal_strategy_name"].as_str().unwrap_or_default(),
                signal["timeframe"].as_str().unwrap_or_default(),
                signal["direction"].as_str().unwrap_or_default(),
                signal["score"].as_f64().unwrap_or_default(),
                signal["strategy_strength"].as_f64(),
                signal["confluence_score"].as_f64(),
                serde_json::to_string(&score_basis).unwrap_or_else(|_| "{}".to_string()),
                signal["current_price"].as_f64().unwrap_or_default(),
                signal["price_timeframe"].as_str().unwrap_or_default(),
                signal["price_bar_time"].as_i64().unwrap_or_default(),
                signal["signal_bar_time"].as_i64().unwrap_or_default(),
                signal["data_source"].as_str().unwrap_or("unknown"),
                signal["data_provider"].as_str().unwrap_or("unknown"),
                signal["data_quality"].as_str().unwrap_or("unknown"),
                signal["evidence_status"].as_str().unwrap_or("VALIDATION_PENDING"),
                signal["evidence_source"].as_str().unwrap_or_default(),
                now,
            ],
        )?;
    }
    Ok(())
}

fn apply_signal_score_updates<'a>(
    connection: &Connection,
    updates: impl Iterator<Item = &'a StockbitSignalScoreUpdate>,
    now: i64,
) -> Result<(), rusqlite::Error> {
    for update in updates {
        connection.execute(
            "UPDATE signals.signal_screener_rows
                SET direction=COALESCE(?1,direction),signal_bar_time=COALESCE(?2,signal_bar_time),
                    score=?3,strategy_strength=?4,confluence_score=?5,score_basis_json=?6,
                    current_price=?7,price_timeframe=?8,price_bar_time=?9,
                    data_source=?10,data_provider=?11,data_quality=?12,last_updated_at=?13
              WHERE exchange_id=?14 AND product_id=?15 AND asset_category_id=?16
                AND symbol=?17 AND signal_strategy_id=?18 AND timeframe=?19",
            params![
                update.direction.as_deref(),
                update.signal_bar_time,
                update.score,
                update.strategy_strength,
                update.confluence_score,
                update.score_basis_json,
                update.current_price,
                update.price_timeframe,
                update.price_bar_time,
                update.data_source,
                update.data_provider,
                update.data_quality,
                now,
                update.exchange_id,
                update.product_id,
                update.asset_category_id,
                update.symbol,
                update.signal_strategy_id,
                update.timeframe,
            ],
        )?;
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn replace_signal_screener_snapshot_with_score_updates(
    connection: &Connection,
    signals: &[Value],
    score_updates: &[Value],
    price_data_unavailable_count: usize,
    registered_count: usize,
) -> Result<(), rusqlite::Error> {
    let compact_updates = score_updates
        .iter()
        .map(StockbitSignalScoreUpdate::from_test_value)
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| {
            rusqlite::Error::InvalidParameterName("invalid score update row".to_string())
        })?;
    replace_signal_screener_snapshot_with_compact_score_updates(
        connection,
        signals,
        &compact_updates,
        price_data_unavailable_count,
        registered_count,
    )
}

fn mark_stockbit_score_rows_stale(connection: &Connection) -> Result<(), rusqlite::Error> {
    let rows = {
        let mut statement = connection.prepare(&format!(
            "SELECT exchange_id,product_id,asset_category_id,symbol,signal_strategy_id,timeframe,
                    score_basis_json
               FROM signals.signal_screener_rows WHERE {}",
            stockbit_signal_score_row_scope_sql()
        ))?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };
    for (exchange, product, category, symbol, strategy, timeframe, basis_json) in rows {
        let Ok(mut basis) = serde_json::from_str::<Value>(&basis_json) else {
            continue;
        };
        let Some(basis) = basis.as_object_mut() else {
            continue;
        };
        if basis.get("status").and_then(Value::as_str) == Some("stale")
            && basis.get("score_bar_is_forming").and_then(Value::as_bool) != Some(true)
        {
            continue;
        }
        basis.insert("status".into(), json!("stale"));
        basis.insert("score_bar_is_forming".into(), json!(false));
        let updated_basis = serde_json::to_string(&basis).unwrap_or(basis_json);
        connection.execute(
            "UPDATE signals.signal_screener_rows SET score_basis_json=?1
              WHERE exchange_id=?2 AND product_id=?3 AND asset_category_id=?4
                AND symbol=?5 AND signal_strategy_id=?6 AND timeframe=?7",
            params![
                updated_basis,
                exchange,
                product,
                category,
                symbol,
                strategy,
                timeframe
            ],
        )?;
    }
    Ok(())
}

pub(crate) fn record_signal_screener_failure(
    connection: &Connection,
    error: &str,
) -> Result<(), rusqlite::Error> {
    crate::database::retry_sqlite_busy(|| record_signal_screener_failure_attempt(connection, error))
}

fn record_signal_screener_failure_attempt(
    connection: &Connection,
    error: &str,
) -> Result<(), rusqlite::Error> {
    connection.execute_batch("BEGIN IMMEDIATE")?;
    let result = (|| {
        mark_stockbit_score_rows_stale(connection)?;
        connection.execute(
            "INSERT INTO signals.signal_screener_status(id,last_attempt_at,last_success_at,last_attempt_status,last_error,signal_count,price_data_status,price_data_unavailable_count)
             VALUES(1,?1,NULL,'error',?2,0,'unknown',0)
             ON CONFLICT(id) DO UPDATE SET last_attempt_at=excluded.last_attempt_at,
                last_attempt_status='error',last_error=excluded.last_error",
            params![now_secs() as i64, error.chars().take(500).collect::<String>()],
        )?;
        Ok::<(), rusqlite::Error>(())
    })();
    match result {
        Ok(()) => match connection.execute_batch("COMMIT") {
            Ok(()) => Ok(()),
            Err(error) => {
                let _ = connection.execute_batch("ROLLBACK");
                Err(error)
            }
        },
        Err(error) => {
            let _ = connection.execute_batch("ROLLBACK");
            Err(error)
        }
    }
}
