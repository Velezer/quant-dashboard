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
        &std::collections::BTreeSet::new(),
    )
}

pub(crate) fn replace_signal_screener_snapshot_with_readiness(
    connection: &Connection,
    signals: &[Value],
    score_updates: &[StockbitSignalScoreUpdate],
    price_data_unavailable_count: usize,
    registered_count: usize,
    readiness: Option<&Value>,
    preserved_stockbit: &std::collections::BTreeSet<(String, String, String)>,
) -> Result<(), rusqlite::Error> {
    crate::database::retry_sqlite_busy(|| {
        replace_signal_screener_snapshot_attempt(
            connection,
            signals,
            score_updates,
            price_data_unavailable_count,
            registered_count,
            readiness,
            preserved_stockbit,
        )
    })
}

pub(crate) fn mark_signal_screener_app_scanning(
    connection: &Connection,
    exchange_id: &str,
) -> Result<(), rusqlite::Error> {
    crate::database::retry_sqlite_busy(|| {
        let now = now_secs() as i64;
        connection.execute(
            "INSERT INTO signals.signal_screener_app_status(
                 exchange_id,last_attempt_at,last_success_at,last_attempt_status,last_error,
                 signal_count,price_data_status,price_data_unavailable_count,registered_count,readiness_json
             ) VALUES(?1,?2,NULL,'scanning','',0,'unknown',0,0,'null')
             ON CONFLICT(exchange_id) DO UPDATE SET
                 last_attempt_at=excluded.last_attempt_at,
                 last_attempt_status='scanning',last_error=''",
            params![exchange_id, now],
        )?;
        Ok(())
    })
}

/// Publish an app batch while this scan cycle is running.
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
            let scanned = scanned_instruments.iter().cloned().collect::<std::collections::BTreeSet<_>>();
            mark_stockbit_score_rows_stale(connection, if clear_app { None } else { Some(&scanned) }, false)?;
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
        let app_stored_signal_count: i64 = connection.query_row(
            "SELECT COUNT(*) FROM signals.signal_screener_rows WHERE exchange_id=?1",
            [exchange_id],
            |row| row.get(0),
        )?;
        connection.execute(
            "INSERT INTO signals.signal_screener_app_status(
                 exchange_id,last_attempt_at,last_success_at,last_attempt_status,last_error,
                 signal_count,price_data_status,price_data_unavailable_count,registered_count,readiness_json
             ) VALUES(?1,?2,NULL,'scanning','',?3,'unknown',0,0,'null')
             ON CONFLICT(exchange_id) DO UPDATE SET
                 last_attempt_at=excluded.last_attempt_at,
                 last_attempt_status='scanning',last_error=''",
            params![exchange_id, now, app_stored_signal_count],
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

pub(crate) fn replace_signal_screener_instrument_snapshot(
    connection: &Connection,
    exchange_id: &str,
    product_id: &str,
    asset_category_id: &str,
    symbol: &str,
    signals: &[Value],
    score_updates: &[StockbitSignalScoreUpdate],
) -> Result<(), rusqlite::Error> {
    crate::database::retry_sqlite_busy(|| {
        connection.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| {
            let persistent_row_scope = stockbit_signal_score_row_scope_sql();
            connection.execute(
                &format!(
                    "DELETE FROM signals.signal_screener_rows
                      WHERE exchange_id=?1 AND product_id=?2 AND asset_category_id=?3
                        AND symbol=?4 AND NOT ({persistent_row_scope})"
                ),
                params![exchange_id, product_id, asset_category_id, symbol],
            )?;
            if exchange_id == quant_core::STOCKBIT_NWE_4H_EXCHANGE_ID {
                let scanned = std::collections::BTreeSet::from([(
                    product_id.to_string(),
                    asset_category_id.to_string(),
                    symbol.to_string(),
                )]);
                mark_stockbit_score_rows_stale(connection, Some(&scanned), false)?;
            }
            insert_signal_screener_rows(
                connection,
                signals.iter().filter(|signal| {
                    signal["exchange_id"].as_str() == Some(exchange_id)
                        && signal["product_id"].as_str() == Some(product_id)
                        && signal["asset_category_id"].as_str() == Some(asset_category_id)
                        && signal["symbol"].as_str() == Some(symbol)
                }),
                now_secs() as i64,
            )?;
            apply_signal_score_updates(
                connection,
                score_updates.iter().filter(|update| {
                    update.exchange_id == exchange_id
                        && update.product_id == product_id
                        && update.asset_category_id == asset_category_id
                        && update.symbol == symbol
                }),
                now_secs() as i64,
            )?;
            connection.execute(
                "UPDATE signals.signal_screener_status
                    SET signal_count=(SELECT COUNT(*) FROM signals.signal_screener_rows)
                  WHERE id=1",
                [],
            )?;
            connection.execute(
                "UPDATE signals.signal_screener_app_status
                    SET signal_count=(
                        SELECT COUNT(*) FROM signals.signal_screener_rows WHERE exchange_id=?1
                    )
                  WHERE exchange_id=?1",
                [exchange_id],
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
    })
}

pub(crate) fn replace_signal_screener_app_snapshot_with_readiness(
    connection: &Connection,
    exchange_id: &str,
    signals: &[Value],
    score_updates: &[StockbitSignalScoreUpdate],
    price_data_unavailable_count: usize,
    registered_count: usize,
    readiness: Option<&Value>,
    preserved_stockbit: &std::collections::BTreeSet<(String, String, String)>,
) -> Result<(), rusqlite::Error> {
    crate::database::retry_sqlite_busy(|| {
        replace_signal_screener_app_snapshot_attempt(
            connection,
            exchange_id,
            signals,
            score_updates,
            price_data_unavailable_count,
            registered_count,
            readiness,
            preserved_stockbit,
        )
    })
}

fn replace_signal_screener_app_snapshot_attempt(
    connection: &Connection,
    exchange_id: &str,
    signals: &[Value],
    score_updates: &[StockbitSignalScoreUpdate],
    price_data_unavailable_count: usize,
    registered_count: usize,
    readiness: Option<&Value>,
    preserved_stockbit: &std::collections::BTreeSet<(String, String, String)>,
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
        let persistent_row_scope = stockbit_signal_score_row_scope_sql();
        connection.execute(
            &format!(
                "DELETE FROM signals.signal_screener_rows
                  WHERE exchange_id=?1 AND NOT ({persistent_row_scope})"
            ),
            [exchange_id],
        )?;
        if exchange_id == quant_core::STOCKBIT_NWE_4H_EXCHANGE_ID {
            mark_stockbit_score_rows_stale(connection, None, false)?;
            if !preserved_stockbit.is_empty() {
                mark_stockbit_score_rows_stale(connection, Some(preserved_stockbit), true)?;
            }
        }
        insert_signal_screener_rows(connection, signals.iter(), now)?;
        apply_signal_score_updates(
            connection,
            score_updates.iter().filter(|update| update.exchange_id == exchange_id),
            now,
        )?;
        let app_signal_count: i64 = connection.query_row(
            "SELECT COUNT(*) FROM signals.signal_screener_rows WHERE exchange_id=?1",
            [exchange_id],
            |row| row.get(0),
        )?;
        let readiness_value = readiness.cloned().unwrap_or(Value::Null);
        let readiness_json = serde_json::to_string(&readiness_value)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        connection.execute(
            "INSERT INTO signals.signal_screener_app_status(
                 exchange_id,last_attempt_at,last_success_at,last_attempt_status,last_error,
                 signal_count,price_data_status,price_data_unavailable_count,registered_count,readiness_json
             ) VALUES(?1,?2,?2,'ready','',?3,?4,?5,?6,?7)
             ON CONFLICT(exchange_id) DO UPDATE SET
                 last_attempt_at=excluded.last_attempt_at,
                 last_success_at=excluded.last_success_at,
                 last_attempt_status='ready',last_error='',
                 signal_count=excluded.signal_count,
                 price_data_status=excluded.price_data_status,
                 price_data_unavailable_count=excluded.price_data_unavailable_count,
                 registered_count=excluded.registered_count,
                 readiness_json=excluded.readiness_json",
            params![
                exchange_id,
                now,
                app_signal_count,
                price_data_status,
                price_data_unavailable_count as i64,
                registered_count as i64,
                readiness_json
            ],
        )?;
        connection.execute(
            "UPDATE signals.signal_screener_status
                SET signal_count=(SELECT COUNT(*) FROM signals.signal_screener_rows)
              WHERE id=1",
            [],
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

fn save_completed_app_scan_statuses(
    connection: &Connection,
    readiness: Option<&Value>,
    now: i64,
) -> Result<(), rusqlite::Error> {
    let Some(workers) = readiness.and_then(|value| value["workers"].as_array()) else {
        return Ok(());
    };
    for worker in workers {
        let Some(exchange_id) = worker["exchange_id"].as_str() else {
            continue;
        };
        let registered = worker["registered_instruments"].as_u64().unwrap_or(0) as usize;
        let fresh = worker["fresh_price_instruments"].as_u64().unwrap_or(0) as usize;
        let unavailable = registered.saturating_sub(fresh);
        let price_status = if unavailable == 0 {
            "ready"
        } else if registered > 0 && unavailable >= registered {
            "unavailable"
        } else {
            "partial_stale"
        };
        let signal_count: i64 = connection.query_row(
            "SELECT COUNT(*) FROM signals.signal_screener_rows WHERE exchange_id=?1",
            [exchange_id],
            |row| row.get(0),
        )?;
        let mut scoped = readiness.cloned().unwrap_or(Value::Null);
        if let Some(products) = scoped["products"].as_array_mut() {
            products.retain(|product| product["exchange_id"].as_str() == Some(exchange_id));
        }
        if let Some(app_workers) = scoped["workers"].as_array_mut() {
            app_workers.retain(|app_worker| app_worker["exchange_id"].as_str() == Some(exchange_id));
        }
        if exchange_id != quant_core::STOCKBIT_NWE_4H_EXCHANGE_ID {
            scoped.as_object_mut().map(|object| object.remove("retained_stockbit_readiness"));
        }
        let readiness_json = serde_json::to_string(&scoped)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        connection.execute(
            "INSERT INTO signals.signal_screener_app_status(
                 exchange_id,last_attempt_at,last_success_at,last_attempt_status,last_error,
                 signal_count,price_data_status,price_data_unavailable_count,registered_count,readiness_json
             ) VALUES(?1,?2,?2,'ready','',?3,?4,?5,?6,?7)
             ON CONFLICT(exchange_id) DO UPDATE SET
                 last_attempt_at=excluded.last_attempt_at,
                 last_success_at=excluded.last_success_at,
                 last_attempt_status='ready',last_error='',
                 signal_count=excluded.signal_count,
                 price_data_status=excluded.price_data_status,
                 price_data_unavailable_count=excluded.price_data_unavailable_count,
                 registered_count=excluded.registered_count,
                 readiness_json=excluded.readiness_json",
            params![
                exchange_id,
                now,
                signal_count,
                price_status,
                unavailable as i64,
                registered as i64,
                readiness_json
            ],
        )?;
    }
    Ok(())
}

fn replace_signal_screener_snapshot_attempt(
    connection: &Connection,
    signals: &[Value],
    score_updates: &[StockbitSignalScoreUpdate],
    price_data_unavailable_count: usize,
    registered_count: usize,
    readiness: Option<&Value>,
    preserved_stockbit: &std::collections::BTreeSet<(String, String, String)>,
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
        if !preserved_stockbit.is_empty() {
            connection.execute_batch(
                "CREATE TEMP TABLE IF NOT EXISTS preserved_stockbit_scan_rows (
                    product_id TEXT NOT NULL, asset_category_id TEXT NOT NULL, symbol TEXT NOT NULL,
                    PRIMARY KEY(product_id, asset_category_id, symbol)
                 ) WITHOUT ROWID;
                 DELETE FROM temp.preserved_stockbit_scan_rows;",
            )?;
            for (product, category, symbol) in preserved_stockbit {
                connection.execute(
                    "INSERT INTO temp.preserved_stockbit_scan_rows(product_id,asset_category_id,symbol)
                     VALUES(?1,?2,?3)",
                    params![product, category, symbol],
                )?;
            }
        }
        let keep_preserved = if preserved_stockbit.is_empty() {
            String::new()
        } else {
            " AND NOT (exchange_id='stockbit' AND EXISTS (
                SELECT 1 FROM temp.preserved_stockbit_scan_rows AS p
                 WHERE p.product_id=signal_screener_rows.product_id
                   AND p.asset_category_id=signal_screener_rows.asset_category_id
                   AND p.symbol=signal_screener_rows.symbol
            ))".to_string()
        };
        connection.execute(
            &format!(
                "DELETE FROM signals.signal_screener_rows WHERE NOT ({}){}",
                stockbit_signal_score_row_scope_sql(), keep_preserved
            ),
            [],
        )?;
        // Preserve values and timestamps, but never expose a closed-market
        // retained row as an active entry while its scan is paused.
        mark_stockbit_score_rows_stale(connection, None, false)?;
        if !preserved_stockbit.is_empty() {
            mark_stockbit_score_rows_stale(connection, Some(preserved_stockbit), true)?;
        }
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
        let merged_readiness = readiness_with_historical_stockbit(connection, readiness, preserved_stockbit)?;
        save_scan_readiness(connection, merged_readiness.as_ref())?;
        save_completed_app_scan_statuses(connection, merged_readiness.as_ref(), now)?;
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

// Readiness is aggregated by product, so previous Stockbit totals cannot be
// mixed with this scan's partial counts. Retain them separately as historical.
fn readiness_with_historical_stockbit(
    connection: &Connection,
    readiness: Option<&Value>,
    preserved_stockbit: &std::collections::BTreeSet<(String, String, String)>,
) -> Result<Option<Value>, rusqlite::Error> {
    let Some(mut current) = readiness.cloned() else { return Ok(None); };
    if preserved_stockbit.is_empty() || !current.is_object() {
        return Ok(Some(current));
    }
    let previous = load_scan_readiness(connection)?;
    let mut historical = previous.get("retained_stockbit_readiness")
        .filter(|value| value.is_object())
        .cloned()
        .unwrap_or_else(|| {
            let products = previous["products"].as_array().map(|products| {
                products.iter().filter(|product| product["exchange_id"] == "stockbit")
                    .cloned().collect::<Vec<_>>()
            }).unwrap_or_default();
            json!({
                "state": "historical_market_closed",
                "scan_started_at": previous["scan_started_at"],
                "checked_at": previous["checked_at"],
                "products": products,
            })
        });
    historical["state"] = json!("historical_market_closed");
    historical["preserved_instrument_count"] = json!(preserved_stockbit.len());
    current["retained_stockbit_readiness"] = historical;
    Ok(Some(current))
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

fn mark_stockbit_score_rows_stale(
    connection: &Connection,
    instruments: Option<&std::collections::BTreeSet<(String, String, String)>>,
    include_nonpersistent: bool,
) -> Result<(), rusqlite::Error> {
    let scope = if include_nonpersistent {
        "exchange_id='stockbit'".to_string()
    } else {
        stockbit_signal_score_row_scope_sql()
    };
    let rows = {
        let mut statement = connection.prepare(&format!(
            "SELECT exchange_id,product_id,asset_category_id,symbol,signal_strategy_id,timeframe,
                    score_basis_json
               FROM signals.signal_screener_rows WHERE {}",
            scope
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
        if instruments.is_some_and(|items| !items.contains(&(product.clone(), category.clone(), symbol.clone()))) {
            continue;
        }
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

pub(crate) fn record_signal_screener_app_failure(
    connection: &Connection,
    exchange_id: &str,
    error: &str,
) -> Result<(), rusqlite::Error> {
    crate::database::retry_sqlite_busy(|| {
        let now = now_secs() as i64;
        connection.execute(
            "INSERT INTO signals.signal_screener_app_status(
                 exchange_id,last_attempt_at,last_success_at,last_attempt_status,last_error,
                 signal_count,price_data_status,price_data_unavailable_count,registered_count,readiness_json
             ) VALUES(?1,?2,NULL,'error',?3,0,'unknown',0,0,'null')
             ON CONFLICT(exchange_id) DO UPDATE SET
                 last_attempt_at=excluded.last_attempt_at,
                 last_attempt_status='error',last_error=excluded.last_error",
            params![exchange_id, now, error.chars().take(500).collect::<String>()],
        )?;
        Ok(())
    })
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
        mark_stockbit_score_rows_stale(connection, None, false)?;
        let now = now_secs() as i64;
        connection.execute(
            "INSERT INTO signals.signal_screener_status(id,last_attempt_at,last_success_at,last_attempt_status,last_error,signal_count,price_data_status,price_data_unavailable_count)
             VALUES(1,?1,NULL,'error',?2,0,'unknown',0)
             ON CONFLICT(id) DO UPDATE SET last_attempt_at=excluded.last_attempt_at,
                last_attempt_status='error',last_error=excluded.last_error",
            params![now, error.chars().take(500).collect::<String>()],
        )?;
        connection.execute(
            "UPDATE signals.signal_screener_app_status
                SET last_attempt_at=?1,last_attempt_status='error',last_error=?2",
            params![now, error.chars().take(500).collect::<String>()],
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
