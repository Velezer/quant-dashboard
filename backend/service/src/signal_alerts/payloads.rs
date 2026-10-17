use super::*;

pub(super) fn required_payload_string(payload: &Value, key: &str) -> Result<String, String> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| format!("{key} is required"))
}

pub(super) fn indicator_description(
    id: &str,
    config: quant_core::IndicatorCombinationConfig,
) -> String {
    match id {
        "ema_20_50" => format!(
            "LONG when close > EMA {} > EMA {}; SHORT uses the inverse.",
            config.fast_ema_period, config.slow_ema_period
        ),
        "rsi_14" => format!(
            "LONG when RSI {} is {:.0}–{:.0}; SHORT when it is {:.0}–{:.0}.",
            config.rsi_period,
            config.rsi_long_min,
            config.rsi_long_max,
            config.rsi_short_min,
            config.rsi_short_max
        ),
        "relative_volume" => format!(
            "Volume is at least {:.1}× its {}-bar average and candle direction agrees.",
            config.relative_volume_min, config.relative_volume_period
        ),
        "open_low_structure" => format!(
            "LONG only when open is within {:.1}% of the candle low and close is at or above open.",
            quant_core::IdxStrategyConfig::default().open_equal_low_tolerance * 100.0
        ),
        "range_breakout" => format!(
            "Close breaks the previous {}-bar high or low with matching candle direction.",
            config.breakout_lookback
        ),
        "wick_rejection" => format!(
            "Bullish lower wick or bearish upper wick is at least {:.0}% of candle range.",
            config.wick_fraction_min * 100.0
        ),
        "momentum_12_1_month" => {
            let momentum = quant_core::IdxTimeSeriesMomentumConfig::default();
            format!(
                "LONG when return from {} bars ago to {} bars ago is positive; SHORT when negative.",
                momentum.lookback_bars, momentum.skip_recent_bars
            )
        }
        "sma_200_session" => format!(
            "LONG above the {}-bar simple moving average; SHORT below it.",
            quant_core::IdxTimeSeriesMomentumConfig::default().trend_period
        ),
        _ => "Indicator rule unavailable.".to_string(),
    }
}

pub(super) fn signal_alert_from_candidate(candidate: &Value) -> Option<Value> {
    let direction = ["Signal_Direction", "signal_direction", "direction"]
        .iter()
        .find_map(|key| candidate.get(*key).and_then(Value::as_str))
        .unwrap_or("")
        .to_ascii_uppercase();
    if !matches!(direction.as_str(), "LONG" | "SHORT") {
        return None;
    }

    let signal_strategy_id =
        first_candidate_string(candidate, &["Signal_Strategy_ID", "signal_strategy_id"])?;
    let strategy = crate::master_data::master_data_records("signal_strategies")
        .iter()
        .find(|model| {
            crate::master_data::catalog_string(model, "id") == Some(signal_strategy_id.as_str())
        });
    let indicator_ids = strategy
        .and_then(|model| model.get("indicator_ids"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if indicator_ids.is_empty() {
        return None;
    }

    let signal_bar_time = candidate
        .get("Signal_Bar_Time")
        .or_else(|| candidate.get("signal_bar_time"))
        .and_then(Value::as_i64)?;
    let validation_status = candidate
        .get("Backtest_Proof")
        .and_then(|proof| proof.get("status"))
        .and_then(Value::as_str)
        .or_else(|| candidate.get("evidence_status").and_then(Value::as_str))
        .map(|status| {
            if status.eq_ignore_ascii_case("VALIDATED") {
                "VALIDATED"
            } else {
                "VALIDATION_PENDING"
            }
        })
        .unwrap_or("VALIDATION_PENDING");
    let model_name = strategy
        .and_then(|model| crate::master_data::catalog_string(model, "label"))
        .map(ToOwned::to_owned)
        .or_else(|| {
            first_candidate_string(candidate, &["Signal_Strategy_Name", "signal_strategy_name"])
        })
        .unwrap_or_else(|| "Indicator-based signal model".to_string());
    let raw_exchange = first_candidate_string(
        candidate,
        &["Exchange_ID", "exchange_id", "Exchange", "exchange"],
    )
    .unwrap_or_default();
    let exchange_id = canonical_exchange_id(&raw_exchange);
    let indicators = indicator_ids
        .iter()
        .filter_map(Value::as_str)
        .map(|id| {
            let label = crate::master_data::master_data_records("indicators")
                .iter()
                .find(|indicator| crate::master_data::catalog_string(indicator, "id") == Some(id))
                .and_then(|indicator| crate::master_data::catalog_string(indicator, "label"))
                .unwrap_or(id);
            json!({"id": id, "label": label})
        })
        .collect::<Vec<_>>();

    Some(json!({
        "exchange_id": exchange_id,
        "product_id": first_candidate_string(candidate, &["Product_ID", "product_id"]).unwrap_or_default(),
        "asset_category_id": first_candidate_string(candidate, &["Asset_Category_ID", "asset_category_id", "Asset_Class", "Class"]).unwrap_or_default(),
        "symbol": first_candidate_string(candidate, &["Symbol", "symbol", "Asset", "asset"]).unwrap_or_default(),
        "signal_strategy_id": signal_strategy_id,
        "signal_strategy_name": model_name,
        "indicator_ids": indicator_ids,
        "indicators": indicators,
        "data_source": first_candidate_string(candidate, &["Data_Source", "data_source"]).unwrap_or_else(|| "unknown".to_string()),
        "data_provider": first_candidate_string(candidate, &["Data_Provider", "data_provider"]).unwrap_or_else(|| "unknown".to_string()),
        "timeframe": first_candidate_string(candidate, &["timeframe", "Zone_Timeframe"]).unwrap_or_else(|| "unknown".to_string()),
        "direction": direction,
        "price": candidate.get("Current_Price").or_else(|| candidate.get("Price")).or_else(|| candidate.get("current_price")).and_then(Value::as_f64),
        "signal_bar_time": signal_bar_time,
        "validation_status": validation_status,
    }))
}

pub(super) fn first_candidate_string(candidate: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        candidate
            .get(*key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    })
}

pub(super) fn canonical_exchange_id(raw_exchange: &str) -> String {
    crate::master_data::exchange_by_alias(raw_exchange)
        .and_then(|exchange| crate::master_data::catalog_string(exchange, "id"))
        .unwrap_or(raw_exchange)
        .to_string()
}
