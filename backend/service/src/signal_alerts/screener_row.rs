use super::super::{canonical_exchange_id, first_candidate_string, signal_candle_is_expired};
use super::{
    is_crypto_nwe_dss_signal_score_row, is_stockbit_signal_score_row, score_components_match,
};
use crate::now_secs;
use serde_json::{json, Value};

pub(crate) fn signal_screener_row(candidate: &Value) -> Option<Value> {
    let direction = [
        "Signal_Direction",
        "signal_direction",
        "Direction",
        "direction",
    ]
    .iter()
    .find_map(|key| candidate.get(*key).and_then(Value::as_str))?
    .trim()
    .to_ascii_uppercase();
    if !matches!(direction.as_str(), "LONG" | "SHORT") {
        return None;
    }

    let exchange = first_candidate_string(
        candidate,
        &["Exchange_ID", "exchange_id", "Exchange", "exchange"],
    )?;
    let exchange_id = canonical_exchange_id(&exchange);
    let product_id = first_candidate_string(candidate, &["Product_ID", "product_id"])?;
    let raw_category = first_candidate_string(
        candidate,
        &[
            "Asset_Category_ID",
            "asset_category_id",
            "Asset_Class",
            "Class",
        ],
    )?;
    let asset_category_id = crate::master_data::category_by_id_or_research_alias(&raw_category)
        .and_then(|category| crate::master_data::catalog_string(category, "id"))
        .unwrap_or(&raw_category)
        .to_string();
    let symbol = first_candidate_string(candidate, &["Symbol", "symbol", "Asset", "asset"])?;
    let signal_strategy_id = first_candidate_string(
        candidate,
        &[
            "Signal_Strategy_ID",
            "signal_strategy_id",
            "Strategy_ID",
            "strategy_id",
        ],
    )?;
    let timeframe = first_candidate_string(candidate, &["timeframe", "Zone_Timeframe"])?;
    let current_price = candidate
        .get("Current_Price")
        .or_else(|| candidate.get("current_price"))
        .and_then(Value::as_f64)
        .filter(|price| price.is_finite() && *price > 0.0)?;
    let score = candidate
        .get("Signal_Strength")
        .or_else(|| candidate.get("signal_strength"))
        .or_else(|| candidate.get("Strategy_Strength"))
        .or_else(|| candidate.get("strategy_strength"))
        .and_then(Value::as_f64)
        .filter(|score| score.is_finite())?;
    let signal_bar_time = candidate
        .get("Signal_Bar_Time")
        .or_else(|| candidate.get("signal_bar_time"))
        .and_then(Value::as_i64)?;
    let price_bar_time = candidate
        .get("Current_Price_Bar_Time")
        .and_then(Value::as_i64)
        .unwrap_or(signal_bar_time);
    let storage_symbol = first_candidate_string(candidate, &["Data_Symbol", "data_symbol"])
        .unwrap_or_else(|| symbol.clone());
    let price_timeframe = first_candidate_string(candidate, &["Current_Price_Timeframe"])
        .unwrap_or_else(|| timeframe.clone());
    let is_stockbit_signal_score = is_stockbit_signal_score_row(
        &exchange_id,
        &product_id,
        &asset_category_id,
        &signal_strategy_id,
        &timeframe,
    );
    let is_crypto_nwe_dss_signal_score =
        is_crypto_nwe_dss_signal_score_row(&asset_category_id, &signal_strategy_id, &timeframe);
    let score_bar_time = candidate
        .get("Signal_Score_Bar_Time")
        .and_then(Value::as_i64)
        .unwrap_or(price_bar_time);
    let score_bar_is_forming = candidate
        .get("Signal_Score_Bar_Is_Forming")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let now_ns = i64::try_from(now_secs())
        .unwrap_or(i64::MAX)
        .saturating_mul(crate::NANOS_PER_SECOND);
    if !is_stockbit_signal_score
        && signal_candle_is_expired(signal_bar_time, &timeframe, now_secs() as i64)
    {
        return None;
    }
    let forming_bar_is_valid = is_stockbit_signal_score
        && score_bar_is_forming
        && score_bar_time <= now_ns
        && score_bar_time
            .saturating_add(crate::timeframe_duration_ns(&timeframe).unwrap_or_default())
            > now_ns;
    if !forming_bar_is_valid
        && !crate::completed_bar_is_fresh_at(
            &storage_symbol,
            &price_timeframe,
            price_bar_time,
            now_ns,
        )
    {
        return None;
    }
    let strategy = crate::master_data::master_data_records("signal_strategies")
        .iter()
        .find(|model| {
            crate::master_data::catalog_string(model, "id") == Some(signal_strategy_id.as_str())
        });
    let score_model_id = first_candidate_string(candidate, &["Signal_Scoring_Model_ID"])
        .unwrap_or_else(|| quant_core::SIGNAL_CONFLUENCE_SCORING_MODEL_ID.to_string());
    let four_indicator_score_model = super::super::is_four_indicator_score_model(&score_model_id);
    let score_scale_max = if four_indicator_score_model {
        quant_core::FOUR_INDICATOR_SCORE_SCALE_MAX
    } else if is_crypto_nwe_dss_signal_score {
        quant_core::NWE_DSS_SIGNAL_SCORE_SCALE_MAX
    } else {
        strategy
            .and_then(|model| model.get("score_scale_max"))
            .and_then(Value::as_f64)
            .unwrap_or(quant_core::SIGNAL_STRENGTH_SCORE_SCALE_MAX)
    };
    if !score_scale_max.is_finite()
        || score_scale_max <= 0.0
        || score_scale_max > quant_core::FOUR_INDICATOR_SCORE_SCALE_MAX
        || score < 0.0
        || score > score_scale_max
    {
        return None;
    }
    let minimum_score_field = if four_indicator_score_model {
        "minimum_score_out_of_40"
    } else if score_scale_max == quant_core::NWE_DSS_SIGNAL_SCORE_SCALE_MAX {
        "minimum_score_out_of_20"
    } else {
        "minimum_score_out_of_10"
    };
    let minimum_score = if four_indicator_score_model {
        Some(quant_core::FOUR_INDICATOR_MINIMUM_SCORE_OUT_OF_40)
    } else if is_crypto_nwe_dss_signal_score {
        Some(quant_core::NWE_DSS_SIGNAL_MINIMUM_SCORE_OUT_OF_20)
    } else {
        strategy
            .and_then(|model| model.get(minimum_score_field))
            .and_then(Value::as_f64)
    };
    if score_scale_max >= quant_core::NWE_DSS_SIGNAL_SCORE_SCALE_MAX && minimum_score.is_none() {
        return None;
    }
    if let Some(minimum_score) = minimum_score {
        if !minimum_score.is_finite() || minimum_score < 0.0 || minimum_score > score_scale_max {
            return None;
        }
        if score < minimum_score {
            return None;
        }
    }
    if strategy
        .and_then(|model| crate::master_data::catalog_string(model, "score_model_id"))
        .is_some_and(|expected| expected != score_model_id)
    {
        return None;
    }
    let signal_strategy_name = strategy
        .and_then(|model| crate::master_data::catalog_string(model, "label"))
        .map(ToOwned::to_owned)
        .or_else(|| {
            first_candidate_string(candidate, &["Signal_Strategy_Name", "signal_strategy_name"])
        })
        .unwrap_or_else(|| signal_strategy_id.clone());
    let confluence = candidate
        .get("Signal_Confluence")
        .cloned()
        .unwrap_or(Value::Null);
    let strategy_strength = candidate
        .get("Strategy_Strength")
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite());
    let confluence_score = candidate
        .get("Signal_Confluence_Score")
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite());
    let component_score_scale_max = strategy
        .and_then(|model| model.get("component_score_scale_max"))
        .and_then(Value::as_f64)
        .unwrap_or(quant_core::SIGNAL_STRENGTH_SCORE_SCALE_MAX);
    let indicator_component_score_scale_max = strategy
        .and_then(|model| model.get("indicator_component_score_scale_max"))
        .and_then(Value::as_f64)
        .unwrap_or(component_score_scale_max);
    let group_score_scale_max = if four_indicator_score_model {
        quant_core::FOUR_INDICATOR_GROUP_SCORE_SCALE_MAX
    } else {
        quant_core::NWE_DSS_COMPONENT_SCORE_SCALE_MAX
    };
    if !component_score_scale_max.is_finite()
        || component_score_scale_max <= 0.0
        || component_score_scale_max > group_score_scale_max
        || !indicator_component_score_scale_max.is_finite()
        || indicator_component_score_scale_max <= 0.0
        || indicator_component_score_scale_max > component_score_scale_max
        || (score_scale_max < quant_core::NWE_DSS_SIGNAL_SCORE_SCALE_MAX
            && (strategy_strength
                .is_some_and(|value| value < 0.0 || value > component_score_scale_max)
                || confluence_score
                    .is_some_and(|value| value < 0.0 || value > component_score_scale_max)))
    {
        return None;
    }
    let (strategy_strength, confluence_score) =
        if score_scale_max >= quant_core::NWE_DSS_SIGNAL_SCORE_SCALE_MAX {
            let (Some(strategy_strength), Some(confluence_score)) = (
                strategy_strength,
                confluence_score.or_else(|| {
                    (candidate
                        .get("Signal_Confluence")
                        .is_none_or(Value::is_null))
                    .then_some(0.0)
                }),
            ) else {
                return None;
            };
            (Some(strategy_strength), Some(confluence_score))
        } else {
            (strategy_strength, confluence_score)
        };
    let dss_state = confluence
        .get("dss_state")
        .and_then(Value::as_str)
        .unwrap_or("UNAVAILABLE");
    let nadaraya_state = confluence
        .get("nadaraya_state")
        .and_then(Value::as_str)
        .unwrap_or("UNAVAILABLE");
    let dss_extreme_zone = confluence
        .get("dss_extreme_zone")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let dss_points = confluence
        .get("dss_points")
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
        .unwrap_or_default();
    let dss_extreme_points = confluence
        .get("dss_extreme_points")
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
        .unwrap_or_default();
    let nadaraya_touch_points = confluence
        .get("nadaraya_touch_points")
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
        .unwrap_or_default();
    let nadaraya_near_line_points = confluence
        .get("nadaraya_near_line_points")
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
        .unwrap_or_default();
    let nadaraya_reclaim_points = confluence
        .get("nadaraya_reclaim_points")
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
        .unwrap_or_default();
    let mut score_components = candidate
        .get("Signal_Score_Components")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_else(|| {
            let mut components = Vec::new();
            if dss_points > 0.0 {
                components.push(
                    json!({"indicator":"DSS Bressert","state":dss_state,"points":dss_points}),
                );
            }
            if dss_extreme_zone {
                components.push(json!({"indicator":"DSS Bressert","state":"EXTREME_ZONE_TURN","points":dss_extreme_points}));
            }
            if nadaraya_state != "UNAVAILABLE" {
                components.push(json!({"indicator":"Nadaraya-Watson","state":nadaraya_state,"points":nadaraya_touch_points}));
            }
            if nadaraya_near_line_points > 0.0 {
                components.push(json!({"indicator":"Nadaraya-Watson","state":"NEAR_ENVELOPE_LINE","points":nadaraya_near_line_points}));
            }
            if nadaraya_reclaim_points > 0.0 {
                components.push(json!({"indicator":"Nadaraya-Watson","state":"ENVELOPE_RECLAIM","points":nadaraya_reclaim_points}));
            }
            components
        });
    if score_scale_max >= quant_core::NWE_DSS_SIGNAL_SCORE_SCALE_MAX {
        let (Some(strategy_strength), Some(confluence_score)) =
            (strategy_strength, confluence_score)
        else {
            return None;
        };
        if score_components.is_empty() && confluence_score == 0.0 {
            score_components.push(json!({
                "indicator": "Nadaraya-Watson",
                "state": "UNAVAILABLE",
                "points": 0.0
            }));
        }
        let component_points_max = if four_indicator_score_model {
            indicator_component_score_scale_max
        } else {
            component_score_scale_max
        };
        if !score_components_match(
            &score_components,
            four_indicator_score_model,
            component_points_max,
            group_score_scale_max,
            strategy_strength,
            confluence_score,
            score,
            true,
        ) {
            return None;
        }
    }
    let evidence_status = candidate
        .get("Backtest_Proof")
        .and_then(|proof| proof.get("status"))
        .or_else(|| candidate.get("Execution_Status"))
        .and_then(Value::as_str)
        .unwrap_or("VALIDATION_PENDING");
    let evidence_source = candidate
        .get("Backtest_Proof")
        .and_then(|proof| proof.get("evidence_source"))
        .or_else(|| {
            candidate
                .get("Backtest_Stats")
                .and_then(|stats| stats.get("evidence_source"))
        })
        .and_then(Value::as_str)
        .unwrap_or("");
    let data_source = first_candidate_string(
        candidate,
        &["Current_Price_Data_Source", "Data_Source", "data_source"],
    )
    .unwrap_or_else(|| "unknown".to_string());
    let data_provider = first_candidate_string(
        candidate,
        &[
            "Current_Price_Data_Provider",
            "Data_Provider",
            "data_provider",
        ],
    )
    .unwrap_or_else(|| "unknown".to_string());
    let data_quality = first_candidate_string(candidate, &["Data_Quality", "data_quality"])
        .unwrap_or_else(|| "unknown".to_string());

    Some(json!({
        "exchange_id": exchange_id,
        "product_id": product_id,
        "asset_category_id": asset_category_id,
        "symbol": symbol,
        "storage_symbol": storage_symbol,
        "signal_strategy_id": signal_strategy_id,
        "signal_strategy_name": signal_strategy_name,
        "timeframe": timeframe,
        "direction": direction,
        "score": score,
        "strategy_strength": strategy_strength,
        "confluence_score": confluence_score,
        "score_model_id": score_model_id,
        "score_components": score_components,
        "score_scale_max": score_scale_max,
        "minimum_score": minimum_score,
        "minimum_score_out_of_10": if score_scale_max == quant_core::SIGNAL_STRENGTH_SCORE_SCALE_MAX {
            minimum_score
        } else {
            None
        },
        "minimum_score_out_of_20": if score_scale_max == quant_core::NWE_DSS_SIGNAL_SCORE_SCALE_MAX {
            minimum_score
        } else {
            None
        },
        "minimum_score_out_of_40": if four_indicator_score_model { minimum_score } else { None },
        "component_score_scale_max": component_score_scale_max,
        "indicator_component_score_scale_max": indicator_component_score_scale_max,
        "current_price": current_price,
        "price_timeframe": price_timeframe,
        "price_bar_time": price_bar_time,
        "signal_bar_time": signal_bar_time,
        "score_bar_time": score_bar_time,
        "score_bar_is_forming": score_bar_is_forming,
        "score_status": "qualifying",
        "data_source": data_source,
        "data_provider": data_provider,
        "data_quality": data_quality,
        "evidence_status": evidence_status,
        "evidence_source": evidence_source,
    }))
}
