use serde_json::{json, Value};

pub(super) fn ensure_crypto_signal_catalog(catalog: &mut Value) -> bool {
    let categories = catalog
        .get("asset_categories")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|category| {
            let id = category.get("id")?.as_str()?;
            id.starts_with("crypto_").then(|| id.to_string())
        })
        .collect::<Vec<_>>();
    let exchanges = catalog
        .get("exchanges")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|exchange| {
            exchange
                .get("asset_category_ids")
                .and_then(Value::as_array)
                .is_some_and(|ids| {
                    ids.iter().any(|id| {
                        id.as_str()
                            .is_some_and(|id| categories.iter().any(|category| category == id))
                    })
                })
        })
        .filter_map(|exchange| {
            exchange
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .collect::<Vec<_>>();
    let products = catalog
        .get("plan_profiles")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|profile| {
            profile
                .get("asset_category_id")
                .and_then(Value::as_str)
                .is_some_and(|id| categories.iter().any(|category| category == id))
        })
        .filter_map(|profile| {
            profile
                .get("product_id")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .collect::<Vec<_>>();

    let minimum_score = quant_core::FOUR_INDICATOR_MINIMUM_SCORE_OUT_OF_40;
    let nwe_minimum_history = quant_core::stockbit_nwe_4h_minimum_history_bars();
    let dss_minimum_history = quant_core::DSS_BRESSERT_MINIMUM_HISTORY_BARS;
    let score_scale_max = quant_core::FOUR_INDICATOR_SCORE_SCALE_MAX;
    let component_score_scale_max = quant_core::FOUR_INDICATOR_GROUP_SCORE_SCALE_MAX;
    let indicator_component_score_scale_max = quant_core::FOUR_INDICATOR_COMPONENT_SCORE_SCALE_MAX;
    let mut changed = false;

    for timeframe in quant_core::SIGNAL_SCREENER_TIMEFRAMES.iter().copied() {
        let Some(nwe_strategy_id) = quant_core::crypto_nwe_signal_strategy_id(timeframe) else {
            continue;
        };
        let Some(dss_strategy_id) = quant_core::crypto_dss_bressert_signal_strategy_id(timeframe)
        else {
            continue;
        };
        let shared_scope = json!({
            "asset_category_ids": categories.clone(),
            "exchange_ids": exchanges.clone(),
            "product_ids": products.clone(),
            "timeframe": timeframe,
            "score_scale_max": score_scale_max,
            "component_score_scale_max": component_score_scale_max,
            "indicator_component_score_scale_max": indicator_component_score_scale_max,
            "research_only": true,
            "implemented": true,
            "status": "research_only",
            "validation_status": "UNVALIDATED",
            "icon": "📈",
        });

        let nwe_description = crypto_score_description(
            timeframe,
            nwe_minimum_history,
            minimum_score,
            score_scale_max,
            component_score_scale_max,
            indicator_component_score_scale_max,
        );
        let nwe_strategy = with_crypto_scope(
            json!({
                "id": nwe_strategy_id,
                "label": format!("Crypto {timeframe} Nadaraya-Watson Band Proximity · Research"),
                "short_label": format!("{timeframe} NWE Band Proximity"),
                "description": nwe_description.clone(),
                "indicator_ids": [quant_core::NWE_METHOD_ID, "dss_bressert", "relative_volume", "estimated_volume_delta_ohlcv"],
                "score_model_id": quant_core::STOCKBIT_NWE_4H_SIGNAL_SCORE_MODEL_ID,
                "minimum_score_out_of_40": minimum_score,
                "minimum_history_bars": nwe_minimum_history,
                "strategy_score_label": "Price evidence (NWE + DSS)",
                "confluence_score_label": "Volume evidence (relative volume + delta proxy)",
                "score_basis_description": nwe_description,
            }),
            &shared_scope,
        );
        changed |= super::upsert_catalog_record(catalog, "signal_strategies", nwe_strategy);

        let dss_description = crypto_score_description(
            timeframe,
            dss_minimum_history,
            minimum_score,
            score_scale_max,
            component_score_scale_max,
            indicator_component_score_scale_max,
        );
        let dss_strategy = with_crypto_scope(
            json!({
                "id": dss_strategy_id,
                "label": format!("Crypto {timeframe} DSS Bressert Near-Cross · Research"),
                "short_label": format!("{timeframe} DSS Near-Cross"),
                "description": dss_description.clone(),
                "indicator_ids": ["dss_bressert", quant_core::NWE_METHOD_ID, "relative_volume", "estimated_volume_delta_ohlcv"],
                "score_model_id": quant_core::STOCKBIT_DSS_BRESSERT_4H_SIGNAL_SCORE_MODEL_ID,
                "minimum_score_out_of_40": minimum_score,
                "minimum_history_bars": dss_minimum_history,
                "strategy_score_label": "Price evidence (NWE + DSS)",
                "confluence_score_label": "Volume evidence (relative volume + delta proxy)",
                "score_basis_description": dss_description,
            }),
            &shared_scope,
        );
        changed |= super::upsert_catalog_record(catalog, "signal_strategies", dss_strategy);
    }
    changed
}

fn crypto_score_description(
    timeframe: &str,
    minimum_history_bars: usize,
    minimum_score: f64,
    score_scale_max: f64,
    component_score_scale_max: f64,
    indicator_component_score_scale_max: f64,
) -> String {
    let volume_lookback_bars = quant_core::FOUR_INDICATOR_VOLUME_LOOKBACK_BARS;
    let timeframe_source_note = if timeframe == "30m" {
        " 30m setup candles are aggregated locally from two adjacent, completed same-source 15m candles; no separate 30m provider request is made."
    } else {
        ""
    };
    format!(
        "Crypto research market-entry signals are recalculated each scan from the latest directional setup on a completed {timeframe} bar. The strategy setup and score floor determine whether a row qualifies; there is no separate signal-age expiry. The latest same-source 15m reference price is shown when available; a stale reference remains labeled and is not a market entry. Setup-strength score has four equal-weight factors: NWE, DSS Bressert, {volume_lookback_bars}-bar relative volume, and estimated volume delta from OHLCV close location. Each factor contributes 0–{indicator_component_score_scale_max:.0}; the two price factors and two volume factors form groups capped at {component_score_scale_max:.0} each, for {score_scale_max:.0} total. Relative volume compares the setup bar to the preceding {volume_lookback_bars} same-source bars; estimated delta is a candle proxy, not true aggressor-side trade delta. Unavailable volume evidence contributes zero and remains labeled unavailable. Scores below {minimum_score:.1}/{score_scale_max:.0} are withheld. Requires at least {minimum_history_bars} completed {timeframe} bars.{timeframe_source_note} The 15m price is a candle close, not a live bid/ask. Source and quality stay attached; proxy candles are not venue-native execution evidence. The score is not win probability or execution availability, and no order is placed."
    )
}

fn with_crypto_scope(mut strategy: Value, scope: &Value) -> Value {
    let Some(scope) = scope.as_object() else {
        return strategy;
    };
    if let Some(strategy_object) = strategy.as_object_mut() {
        for (key, value) in scope {
            strategy_object.insert(key.clone(), value.clone());
        }
    }
    strategy
}
