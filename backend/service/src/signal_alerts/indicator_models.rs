use super::*;

pub fn indicator_model_options(connection: &Connection) -> Result<Value, rusqlite::Error> {
    let registered_scopes = {
        let mut statement = connection.prepare(
            "SELECT DISTINCT exchange_id,product_id,asset_category_id
               FROM app.instruments
              WHERE is_active=1 AND venue_verified=1
                AND listing_status IN ('venue_active','public_listed_snapshot')
              ORDER BY exchange_id,asset_category_id,product_id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    let supported_ids = quant_core::indicator_combination_indicator_ids();
    let default_config = quant_core::IndicatorCombinationConfig::default();
    let mut scopes = Vec::new();
    for (exchange_id, product_id, asset_category_id) in registered_scopes {
        let canonical_exchange = crate::master_data::exchange_by_alias(&exchange_id)
            .and_then(|record| crate::master_data::catalog_string(record, "id"))
            .unwrap_or(&exchange_id);
        let exchange_label = crate::master_data::master_data_records("exchanges")
            .iter()
            .find(|record| {
                crate::master_data::catalog_string(record, "id") == Some(canonical_exchange)
            })
            .and_then(|record| crate::master_data::catalog_string(record, "label"))
            .unwrap_or(canonical_exchange);
        let category = crate::master_data::master_data_records("asset_categories")
            .iter()
            .find(|record| {
                crate::master_data::catalog_string(record, "id") == Some(asset_category_id.as_str())
            });
        let Some(category) = category else {
            continue;
        };
        let category_label =
            crate::master_data::catalog_string(category, "label").unwrap_or(&asset_category_id);
        let exchange_is_supported = category
            .get("exchange_ids")
            .and_then(Value::as_array)
            .is_some_and(|ids| ids.iter().any(|id| id.as_str() == Some(canonical_exchange)));
        if !exchange_is_supported {
            continue;
        }

        let mut indicators_by_timeframe: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for profile in crate::master_data::master_data_records("plan_profiles") {
            let profile_exchange = crate::master_data::catalog_string(profile, "exchange_id")
                .and_then(|id| crate::master_data::exchange_by_alias(id))
                .and_then(|record| crate::master_data::catalog_string(record, "id"));
            if profile_exchange != Some(canonical_exchange)
                || crate::master_data::catalog_string(profile, "asset_category_id")
                    != Some(asset_category_id.as_str())
            {
                continue;
            }
            let Some(timeframe) = crate::master_data::catalog_string(profile, "timeframe") else {
                continue;
            };
            let Some(strategy_id) =
                crate::master_data::catalog_string(profile, "signal_strategy_id")
            else {
                continue;
            };
            let strategy = crate::master_data::master_data_records("signal_strategies")
                .iter()
                .find(|record| {
                    crate::master_data::catalog_string(record, "id") == Some(strategy_id)
                });
            let Some(strategy) = strategy
                .filter(|record| record.get("implemented").and_then(Value::as_bool) == Some(true))
            else {
                continue;
            };
            let ids = strategy
                .get("indicator_ids")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .filter(|id| supported_ids.contains(id))
                .filter(|id| {
                    crate::master_data::master_data_records("indicators")
                        .iter()
                        .any(|indicator| {
                            crate::master_data::catalog_string(indicator, "id") == Some(*id)
                                && indicator.get("signal_strategy").and_then(Value::as_bool)
                                    == Some(true)
                        })
                });
            indicators_by_timeframe
                .entry(timeframe.to_string())
                .or_default()
                .extend(ids.map(ToOwned::to_owned));
        }

        for (timeframe, indicator_ids) in indicators_by_timeframe {
            if indicator_ids.is_empty() {
                continue;
            }
            let indicators = indicator_ids
                .iter()
                .map(|id| {
                    let label = crate::master_data::master_data_records("indicators")
                        .iter()
                        .find(|indicator| {
                            crate::master_data::catalog_string(indicator, "id") == Some(id.as_str())
                        })
                        .and_then(|indicator| {
                            crate::master_data::catalog_string(indicator, "label")
                        })
                        .unwrap_or(id);
                    json!({
                        "id": id,
                        "label": label,
                        "description": indicator_description(id, default_config),
                    })
                })
                .collect::<Vec<_>>();
            scopes.push(json!({
                "exchange_id": exchange_id,
                "exchange_label": exchange_label,
                "product_id": product_id,
                "asset_category_id": asset_category_id,
                "asset_category_label": category_label,
                "direction_policy": crate::master_data::catalog_string(category, "direction_policy").unwrap_or("long_only"),
                "timeframe": timeframe,
                "indicators": indicators,
            }));
        }
    }
    let mut statement = connection.prepare(
        "SELECT exchange_id,product_id,asset_category_id,timeframe,indicator_ids_json,updated_at
           FROM app.signal_alert_models
          ORDER BY exchange_id,asset_category_id,product_id,timeframe",
    )?;
    let rows = statement.query_map([], |row| {
        let indicators_json = row.get::<_, String>(4)?;
        Ok(json!({
            "exchange_id": row.get::<_, String>(0)?,
            "product_id": row.get::<_, String>(1)?,
            "asset_category_id": row.get::<_, String>(2)?,
            "timeframe": row.get::<_, String>(3)?,
            "indicator_ids": serde_json::from_str::<Vec<String>>(&indicators_json).unwrap_or_default(),
            "updated_at": row.get::<_, i64>(5)?,
        }))
    })?;
    let models = rows.collect::<Result<Vec<_>, _>>()?;
    Ok(json!({
        "scopes": scopes,
        "models": models,
        "minimum_indicators": 2,
        "maximum_indicators": quant_core::MAX_INDICATOR_COMBINATION_SIZE,
    }))
}

pub fn save_indicator_model(connection: &Connection, payload: &Value) -> Result<Value, String> {
    let exchange_id = required_payload_string(payload, "exchange_id")?;
    let product_id = required_payload_string(payload, "product_id")?;
    let asset_category_id = required_payload_string(payload, "asset_category_id")?;
    let timeframe = required_payload_string(payload, "timeframe")?;
    let indicator_ids = payload
        .get("indicator_ids")
        .and_then(Value::as_array)
        .ok_or_else(|| "indicator_ids must be an array".to_string())?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(ToOwned::to_owned)
                .ok_or_else(|| "indicator_ids must contain only strings".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let distinct = indicator_ids.iter().collect::<BTreeSet<_>>();
    if distinct.len() != indicator_ids.len() {
        return Err("remove duplicate indicators before saving".to_string());
    }

    let options = indicator_model_options(connection).map_err(|error| error.to_string())?;
    let scope = options["scopes"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|scope| {
            scope["exchange_id"].as_str() == Some(&exchange_id)
                && scope["product_id"].as_str() == Some(&product_id)
                && scope["asset_category_id"].as_str() == Some(&asset_category_id)
                && scope["timeframe"].as_str() == Some(&timeframe)
        })
        .ok_or_else(|| {
            "choose a currently registered app, product, category, and timeframe".to_string()
        })?;

    if indicator_ids.is_empty() {
        connection
            .execute(
                "DELETE FROM app.signal_alert_models
                  WHERE exchange_id=?1 AND product_id=?2 AND asset_category_id=?3 AND timeframe=?4",
                params![exchange_id, product_id, asset_category_id, timeframe],
            )
            .map_err(|error| error.to_string())?;
        return Ok(json!({"status":"ok","cleared":true}));
    }
    let min_count = options["minimum_indicators"].as_u64().unwrap_or(2) as usize;
    let max_count = options["maximum_indicators"].as_u64().unwrap_or(6) as usize;
    if !(min_count..=max_count).contains(&indicator_ids.len()) {
        return Err(format!(
            "choose between {min_count} and {max_count} indicators"
        ));
    }
    let available = scope["indicators"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|indicator| indicator["id"].as_str())
        .collect::<BTreeSet<_>>();
    if indicator_ids
        .iter()
        .any(|id| !available.contains(id.as_str()))
    {
        return Err(
            "one or more indicators are not available for this registered scope".to_string(),
        );
    }
    let mut sorted_ids = indicator_ids;
    sorted_ids.sort();
    let ids_json = serde_json::to_string(&sorted_ids).map_err(|error| error.to_string())?;
    connection
        .execute(
            "INSERT INTO app.signal_alert_models
                 (exchange_id,product_id,asset_category_id,timeframe,indicator_ids_json,updated_at)
             VALUES (?1,?2,?3,?4,?5,unixepoch())
             ON CONFLICT(exchange_id,product_id,asset_category_id,timeframe) DO UPDATE SET
                 indicator_ids_json=excluded.indicator_ids_json,updated_at=excluded.updated_at",
            params![
                exchange_id,
                product_id,
                asset_category_id,
                timeframe,
                ids_json
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(json!({"status":"ok","cleared":false,"indicator_ids":sorted_ids}))
}

pub fn evaluate_indicator_models_for_scan(
    connection: &Connection,
    exchange_id: &str,
    product_id: &str,
    asset_category_id: &str,
    symbol: &str,
    timeframe: &str,
    bars: &[quant_core::StrategyBar],
    data_source: &str,
    data_provider: &str,
) -> Result<Vec<Value>, rusqlite::Error> {
    let mut statement = connection.prepare(
        "SELECT indicator_ids_json FROM app.signal_alert_models
          WHERE exchange_id=?1 AND product_id=?2 AND asset_category_id=?3 AND timeframe=?4",
    )?;
    let models = statement.query_map(
        params![exchange_id, product_id, asset_category_id, timeframe],
        |row| row.get::<_, String>(0),
    )?;
    let category = crate::master_data::master_data_records("asset_categories")
        .iter()
        .find(|record| crate::master_data::catalog_string(record, "id") == Some(asset_category_id));
    let allow_short = category
        .and_then(|record| crate::master_data::catalog_string(record, "direction_policy"))
        == Some("long_and_short");
    let config = quant_core::IndicatorCombinationConfig::default();
    let mut alerts = Vec::new();
    for model in models {
        let ids_json = model?;
        let indicator_ids = serde_json::from_str::<Vec<String>>(&ids_json).unwrap_or_default();
        let Ok(signals) = quant_core::generate_indicator_combination_signals(
            bars,
            &indicator_ids,
            config,
            allow_short,
        ) else {
            continue;
        };
        let Some(last_signal) = signals.last() else {
            continue;
        };
        if !matches!(last_signal.direction.as_str(), "LONG" | "SHORT") {
            continue;
        }
        let indicator_labels = indicator_ids
            .iter()
            .map(|id| {
                let label = crate::master_data::master_data_records("indicators")
                    .iter()
                    .find(|indicator| {
                        crate::master_data::catalog_string(indicator, "id") == Some(id.as_str())
                    })
                    .and_then(|indicator| crate::master_data::catalog_string(indicator, "label"))
                    .unwrap_or(id);
                json!({"id":id,"label":label})
            })
            .collect::<Vec<_>>();
        let model_name = indicator_labels
            .iter()
            .filter_map(|indicator| indicator["label"].as_str())
            .collect::<Vec<_>>()
            .join(" + ");
        let direction = last_signal.direction.to_ascii_uppercase();
        let signal_strategy_id = format!("user_indicator_combo_v1:{}", indicator_ids.join("+"));
        alerts.push(json!({
            "exchange_id": canonical_exchange_id(exchange_id),
            "product_id": product_id,
            "asset_category_id": asset_category_id,
            "symbol": symbol,
            "signal_strategy_id": signal_strategy_id,
            "signal_strategy_name": format!("Custom indicators · {model_name}"),
            "indicator_ids": indicator_ids,
            "indicators": indicator_labels,
            "data_source": data_source,
            "data_provider": data_provider,
            "timeframe": timeframe,
            "direction": direction,
            "price": bars.last().map(|bar| bar.close),
            "signal_bar_time": bars.last().map(|bar| bar.time),
            "validation_status": "VALIDATION_PENDING",
        }));
    }
    Ok(alerts)
}
