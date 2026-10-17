use super::{catalog_array_contains, catalog_string, master_data_catalog};
use rusqlite::Connection;
use serde_json::Value;
use std::collections::HashSet;

pub(crate) fn signal_screener_master_data_for_connection(
    connection: &Connection,
) -> Result<Value, rusqlite::Error> {
    signal_screener_master_data_with_registered_products(connection, master_data_catalog())
}

pub(crate) fn signal_screener_master_data_with_registered_products(
    connection: &Connection,
    catalog: &Value,
) -> Result<Value, rusqlite::Error> {
    let mut reference_data = signal_screener_master_data_from_catalog(catalog);
    let mut statement = connection.prepare(
        "SELECT DISTINCT exchange_id,product_id,asset_category_id
           FROM app.exchange_products
          WHERE status='enabled' AND product_id<>'' AND asset_category_id<>''",
    )?;
    let registered_products = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let Some(models) = reference_data
        .get_mut("signal_score_models")
        .and_then(Value::as_array_mut)
    else {
        return Ok(reference_data);
    };
    for model in models {
        let categories = model
            .get("asset_category_ids")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect::<Vec<_>>();
        if !categories
            .iter()
            .any(|category| category.starts_with("crypto_"))
        {
            continue;
        }
        let exchanges = model
            .get("exchange_ids")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let Some(product_ids) = model.get_mut("product_ids").and_then(Value::as_array_mut) else {
            continue;
        };
        let mut seen_products = HashSet::new();
        product_ids.retain(|product| {
            product
                .as_str()
                .is_some_and(|product| seen_products.insert(product.to_owned()))
        });
        for (exchange_id, product_id, category_id) in &registered_products {
            let exchange_matches = exchanges.iter().any(|exchange| exchange == exchange_id);
            let category_matches = categories.iter().any(|category| category == category_id);
            if exchange_matches && category_matches && seen_products.insert(product_id.clone()) {
                product_ids.push(Value::from(product_id.clone()));
            }
        }
    }
    Ok(reference_data)
}

pub(crate) fn signal_screener_master_data_from_catalog(catalog: &Value) -> Value {
    let mut reference_data = catalog.clone();
    if let Some(object) = reference_data.as_object_mut() {
        object.retain(|key, _| matches!(key.as_str(), "exchanges" | "asset_categories"));
        let score_models = catalog
            .get("signal_strategies")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|model| {
                let is_current_model = model.get("research_only").and_then(Value::as_bool)
                    == Some(true)
                    && model.get("implemented").and_then(Value::as_bool) == Some(true);
                let is_stockbit_scope = catalog_array_contains(
                    model,
                    "exchange_ids",
                    quant_core::STOCKBIT_NWE_4H_EXCHANGE_ID,
                ) && catalog_array_contains(
                    model,
                    "product_ids",
                    quant_core::STOCKBIT_NWE_4H_PRODUCT_ID,
                ) && catalog_array_contains(
                    model,
                    "asset_category_ids",
                    quant_core::STOCKBIT_NWE_4H_ASSET_CATEGORY_ID,
                );
                let is_crypto_scope = model
                    .get("asset_category_ids")
                    .and_then(Value::as_array)
                    .is_some_and(|categories| {
                        categories.iter().any(|category| {
                            category
                                .as_str()
                                .is_some_and(|category| category.starts_with("crypto_"))
                        })
                    });
                let timeframe = catalog_string(model, "timeframe");
                let is_supported_crypto_timeframe = is_crypto_scope
                    && timeframe.is_some_and(|timeframe| {
                        quant_core::DSS_BRESSERT_MTF_TIMEFRAMES
                            .iter()
                            .any(|definition| definition.timeframe == timeframe)
                    });
                let is_supported_stockbit_timeframe = is_stockbit_scope
                    && timeframe.is_some_and(|timeframe| {
                        model.get("id").and_then(Value::as_str)
                            == quant_core::stockbit_nwe_signal_strategy_id(timeframe).as_deref()
                            || model.get("id").and_then(Value::as_str)
                                == quant_core::stockbit_dss_bressert_signal_strategy_id(timeframe)
                                    .as_deref()
                    });
                is_current_model
                    && (is_supported_stockbit_timeframe || is_supported_crypto_timeframe)
                    && catalog_string(model, "score_model_id").is_some()
            })
            .map(|model| {
                serde_json::json!({
                    "id": model["id"],
                    "label": model["label"],
                    "short_label": model["short_label"],
                    "exchange_ids": model["exchange_ids"],
                    "product_ids": model["product_ids"],
                    "asset_category_ids": model["asset_category_ids"],
                    "timeframe": model["timeframe"],
                    "score_model_id": model["score_model_id"],
                    "score_scale_max": model["score_scale_max"],
                    "minimum_score_out_of_20": model["minimum_score_out_of_20"],
                    "minimum_score_out_of_40": model["minimum_score_out_of_40"],
                    "component_score_scale_max": model["component_score_scale_max"],
                    "indicator_component_score_scale_max": model["indicator_component_score_scale_max"],
                    "strategy_score_label": model["strategy_score_label"],
                    "confluence_score_label": model["confluence_score_label"],
                    "minimum_history_bars": model["minimum_history_bars"],
                    "score_basis_description": model["score_basis_description"],
                })
            })
            .collect::<Vec<_>>();
        object.insert(
            "signal_score_models".to_string(),
            Value::Array(score_models),
        );
    }
    reference_data
}
