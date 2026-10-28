use super::*;

pub fn list_alerts(
    connection: &Connection,
    requested_limit: Option<i64>,
) -> Result<Value, rusqlite::Error> {
    refresh_lifecycles(connection)?;
    let limit = requested_limit
        .unwrap_or(DEFAULT_SIGNAL_ALERT_LIST_LIMIT)
        .clamp(1, MAX_SIGNAL_ALERT_LIST_LIMIT);
    let unread_count = connection.query_row(
        "SELECT COUNT(*) FROM app.signal_alerts WHERE read_at IS NULL",
        [],
        |row| row.get::<_, i64>(0),
    )?;
    let mut statement = connection.prepare(
        "SELECT alert_id,exchange_id,product_id,asset_category_id,symbol,
                signal_strategy_id,signal_strategy_name,indicator_ids_json,data_source,
                data_provider,timeframe,direction,price,signal_bar_time,validation_status,
                created_at,read_at
           FROM app.signal_alerts
          ORDER BY created_at DESC,alert_id DESC LIMIT ?1",
    )?;
    let rows = statement.query_map([limit], |row| {
        let indicator_json = row.get::<_, String>(7)?;
        let indicator_ids =
            serde_json::from_str::<Vec<String>>(&indicator_json).unwrap_or_default();
        let indicators = indicator_ids
            .iter()
            .map(|id| {
                let label = crate::master_data::master_data_records("indicators")
                    .iter()
                    .find(|indicator| {
                        crate::master_data::catalog_string(indicator, "id") == Some(id.as_str())
                    })
                    .and_then(|indicator| crate::master_data::catalog_string(indicator, "label"))
                    .unwrap_or(id);
                json!({"id": id, "label": label})
            })
            .collect::<Vec<_>>();
        let read_at = row.get::<_, Option<i64>>(16)?;
        let strategy_id = row.get::<_, String>(5)?;
        let alert_kind = if strategy_id.starts_with("user_indicator_combo_v1:") {
            "custom_indicator"
        } else {
            "screener_signal"
        };
        Ok(json!({
            "alert_id": row.get::<_, i64>(0)?,
            "exchange_id": row.get::<_, String>(1)?,
            "product_id": row.get::<_, String>(2)?,
            "asset_category_id": row.get::<_, String>(3)?,
            "symbol": row.get::<_, String>(4)?,
            "signal_strategy_id": strategy_id,
            "alert_kind": alert_kind,
            "signal_strategy_name": row.get::<_, String>(6)?,
            "indicator_ids": indicator_ids,
            "indicators": indicators,
            "data_source": row.get::<_, String>(8)?,
            "data_provider": row.get::<_, String>(9)?,
            "timeframe": row.get::<_, String>(10)?,
            "direction": row.get::<_, String>(11)?,
            "price": row.get::<_, Option<f64>>(12)?,
            "signal_bar_time": row.get::<_, i64>(13)?,
            "validation_status": row.get::<_, String>(14)?,
            "created_at": row.get::<_, i64>(15)?,
            "is_read": read_at.is_some(),
        }))
    })?;
    let alerts = rows.collect::<Result<Vec<_>, _>>()?;
    Ok(json!({"alerts": alerts, "unread_count": unread_count, "limit": limit}))
}

pub fn mark_all_read(connection: &Connection) -> Result<usize, rusqlite::Error> {
    connection.execute(
        "UPDATE app.signal_alerts SET read_at=unixepoch() WHERE read_at IS NULL",
        [],
    )
}
