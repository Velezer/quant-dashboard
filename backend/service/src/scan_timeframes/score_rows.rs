use super::*;

fn win_probability_interval_95_pct(wins: u64, trades: u64) -> Option<[f64; 2]> {
    if trades == 0 || wins > trades {
        return None;
    }
    let z = 1.959_963_984_540_054;
    let n = trades as f64;
    let p = wins as f64 / n;
    let denominator = 1.0 + z * z / n;
    let center = (p + z * z / (2.0 * n)) / denominator;
    let half_width = z * (p * (1.0 - p) / n + z * z / (4.0 * n * n)).sqrt() / denominator;
    Some([
        (center - half_width).max(0.0) * 100.0,
        (center + half_width).min(1.0) * 100.0,
    ])
}

fn win_probability_for_score(
    score: Option<f64>,
    profile: Option<&crate::research_reports::ActiveScoringProfile>,
    exchange_id: &str,
    product_id: &str,
) -> Value {
    let Some(profile) = profile else {
        return serde_json::json!({
            "status": "UNAVAILABLE",
            "reason": "No active validated after-fee scoring profile is available for this signal.",
        });
    };
    let Some(score) = score.filter(|value| value.is_finite()) else {
        return serde_json::json!({
            "status": "UNAVAILABLE",
            "reason": "The signal's weighted score is unavailable.",
            "profile_version": profile.version,
        });
    };
    let product_slice = profile.after_fee_gate["product_slices"]
        .as_array()
        .and_then(|slices| {
            slices.iter().find(|slice| {
                slice["exchange_id"].as_str() == Some(exchange_id)
                    && slice["product_id"].as_str() == Some(product_id)
            })
        });
    let Some(product_slice) = product_slice else {
        return serde_json::json!({
            "status": "UNAVAILABLE",
            "reason": "No after-fee holdout calibration exists for this app and product.",
            "profile_version": profile.version,
            "exchange_id": exchange_id,
            "product_id": product_id,
        });
    };
    let bands = product_slice["holdout_score_bands"].as_array();
    let band = bands.and_then(|bands| {
        bands.iter().find(|band| {
            let minimum = band["minimum_score_out_of_60"]
                .as_f64()
                .unwrap_or(f64::INFINITY);
            let maximum = band["maximum_score_out_of_60"]
                .as_f64()
                .unwrap_or(f64::NEG_INFINITY);
            score >= minimum
                && (score < maximum || (band["maximum_inclusive"] == true && score <= maximum))
        })
    });
    if let Some(band) = band {
        let sample_trades = band["sample_trades"].as_u64().unwrap_or(0);
        let win_probability = band["win_probability_pct"].as_f64();
        if sample_trades >= 30 && win_probability.is_some_and(f64::is_finite) {
            return serde_json::json!({
                "status": "ESTIMATED",
                "win_probability_pct": win_probability,
                "confidence_interval_95_pct": band["confidence_interval_95_pct"],
                "sample_trades": sample_trades,
                "wins": band["wins"],
                "score_band_minimum_out_of_60": band["minimum_score_out_of_60"],
                "score_band_maximum_out_of_60": band["maximum_score_out_of_60"],
                "score_band_maximum_inclusive": band["maximum_inclusive"],
                "basis": "app/product cohort chronological holdout after configured venue fees, matched score band; long and short outcomes pooled; Wilson interval does not adjust for correlated outcomes; spread, slippage, market impact, funding and borrow excluded",
                "fees_applied": true,
                "direction_conditioned": false,
                "profile_version": profile.version,
                "exchange_id": exchange_id,
                "product_id": product_id,
            });
        }
    }
    if score.abs() < profile.minimum_score {
        return serde_json::json!({
            "status": "BELOW_PROFILE_GATE",
            "reason": "This score is below the validated profile's score gate, so that profile's win estimate does not apply.",
            "minimum_score_out_of_60": profile.minimum_score,
            "profile_version": profile.version,
        });
    }
    let holdout = &product_slice["signal_gate_holdout"];
    let sample_trades = holdout["trades"].as_u64().unwrap_or(0);
    let wins = holdout["wins"].as_u64().unwrap_or(0);
    let win_probability = holdout["win_rate_after_fees_pct"].as_f64();
    let score_band_sample_trades = band
        .and_then(|band| band["sample_trades"].as_u64())
        .map(|sample| serde_json::json!(sample))
        .unwrap_or(Value::Null);
    if sample_trades >= 30 && win_probability.is_some_and(f64::is_finite) {
        serde_json::json!({
            "status": "PROFILE_GATE_ESTIMATE",
            "win_probability_pct": win_probability,
            "confidence_interval_95_pct": holdout
                .get("win_rate_after_fees_wilson_95_pct")
                .filter(|interval| interval.is_array())
                .cloned()
                .or_else(|| {
                    win_probability_interval_95_pct(wins, sample_trades)
                        .map(|interval| serde_json::json!(interval))
                }),
            "sample_trades": sample_trades,
            "wins": wins,
            "minimum_score_out_of_60": profile.minimum_score,
            "score_band_sample_trades": score_band_sample_trades,
            "minimum_score_band_sample_trades": 30,
            "basis": "app/product cohort chronological holdout after configured venue fees above the validated profile cutoff; score band unavailable; long and short outcomes pooled; Wilson interval does not adjust for correlated outcomes; spread, slippage, market impact, funding and borrow excluded",
            "fees_applied": true,
            "direction_conditioned": false,
            "profile_version": profile.version,
            "exchange_id": exchange_id,
            "product_id": product_id,
        })
    } else {
        serde_json::json!({
            "status": "INSUFFICIENT_SAMPLE",
            "reason": "The matching after-fee holdout has fewer than 30 trades for a probability estimate.",
            "sample_trades": sample_trades,
            "minimum_sample_trades": 30,
            "profile_version": profile.version,
        })
    }
}

fn after_fee_gate_for_product(
    profile: &crate::research_reports::ActiveScoringProfile,
    exchange_id: &str,
    product_id: &str,
) -> Value {
    let product_slice = profile.after_fee_gate["product_slices"]
        .as_array()
        .and_then(|slices| {
            slices.iter().find(|slice| {
                slice["exchange_id"].as_str() == Some(exchange_id)
                    && slice["product_id"].as_str() == Some(product_id)
            })
        });
    let Some(product_slice) = product_slice else {
        return Value::Null;
    };
    serde_json::json!({
        "status": profile.after_fee_gate.get("status").cloned().unwrap_or(Value::Null),
        "minimum_score_out_of_60": profile.minimum_score,
        "minimum_mean_net_return_after_fees_pct": profile.after_fee_gate
            .get("minimum_mean_net_return_after_fees_pct").cloned().unwrap_or(Value::Null),
        "minimum_profit_factor_after_fees": profile.after_fee_gate
            .get("minimum_profit_factor_after_fees").cloned().unwrap_or(Value::Null),
        "fees_applied": true,
        "validation": product_slice.get("signal_gate_validation").cloned().unwrap_or(Value::Null),
        "holdout": product_slice.get("signal_gate_holdout").cloned().unwrap_or(Value::Null),
        "exchange_id": exchange_id,
        "product_id": product_id,
    })
}

pub(super) fn append_combined_entry(
    input: &TimeframeScanInput<'_>,
    is_stockbit: bool,
    timeframe: &str,
    source_metadata: Option<&(String, String)>,
    bars: &[StrategyBar],
    current_price: Option<&CurrentPrice>,
    candidates: &mut Vec<Value>,
    score_updates: &mut Vec<Value>,
) -> Option<ReadinessIssue> {
    let (source, provider) = source_metadata?;
    let strategy_id = crate::master_data::combined_entry_strategy_id(timeframe, is_stockbit)?;
    let bar = bars
        .last()
        .filter(|bar| bar.close.is_finite() && bar.close > 0.0)?;
    let profile = active_scoring_profile(input, timeframe, source);
    let weights = profile
        .map(|profile| profile.weights)
        .unwrap_or(quant_core::FOUR_INDICATOR_EQUAL_WEIGHTS);
    let score = quant_core::combined_entry_score(bars, weights)?;
    let strength = score.total_score.abs();
    let direction = quant_core::entry_score_direction(
        score.total_score,
        quant_core::SIX_INDICATOR_MINIMUM_SCORE_OUT_OF_60,
        true, // A negative score is also visible as research on long-only products.
    );
    let retained = is_stockbit
        && input
            .retained_stockbit_signal_rows
            .contains(&(strategy_id.clone(), timeframe.to_string()));
    if direction.is_none() && !retained {
        return None;
    }
    let fallback_price = CurrentPrice {
        value: bar.close,
        signal_bar_time: bar.time,
        source: source.clone(),
        provider: provider.clone(),
    };
    let (price, price_timeframe, reference_only) = current_price
        .filter(|price| price.source == *source)
        .map(|price| {
            let reference_only = is_stockbit
                && !crate::completed_bar_is_fresh_at(
                    input.symbol,
                    "15m",
                    price.signal_bar_time,
                    current_time_ns(),
                );
            (price, "15m", reference_only)
        })
        .unwrap_or((&fallback_price, timeframe, true));
    let profile_version = profile.map(|profile| profile.version);
    let score_components = score
        .components
        .iter()
        .map(|component| {
            serde_json::json!({
                "indicator": component.indicator,
                "state": component.state,
                "points": component.points,
                "weight_points": component.weight_points,
                "weighted_points": component.weighted_points,
                "value": component.value,
                "scoring_profile_version": profile_version,
            })
        })
        .collect::<Vec<_>>();
    let score_weights = score
        .components
        .each_ref()
        .map(|component| component.weight_points);
    let [_, _, _, _, markov, autocorrelation] = &score.components;
    let candidate = serde_json::json!({
        "Exchange_ID": input.exchange_id,
        "Product_ID": input.product_id,
        "Asset_Category_ID": input.category_id,
        "Symbol": input.asset,
        "Data_Symbol": input.symbol,
        "Signal_Strategy_ID": strategy_id,
        "Signal_Scoring_Model_ID": quant_core::ENTRY_SCORE_MODEL_ID,
        "Signal_Direction": direction,
        "Signal_Strength": strength,
        "Strategy_Strength": score.price_score,
        "Signal_Confluence_Score": score.volume_score,
        "Signal_Markov_Regime_Score": score.markov_regime_score,
        "Signal_Autocorrelation_Score": score.autocorrelation_score,
        "Signal_Markov_Regime": markov.state,
        "Signal_Autocorrelation_State": autocorrelation.state,
        "Signal_Scoring_Weights": score_weights,
        "Signal_Scoring_Profile_Version": profile_version,
        "Signal_After_Fee_Gate": profile.map(|profile| {
            after_fee_gate_for_product(profile, input.exchange_id, input.product_id)
        }),
        "Signal_Win_Probability": win_probability_for_score(
            direction.map(|_| strength), profile, input.exchange_id, input.product_id,
        ),
        "Signal_Score_Components": score_components,
        "timeframe": timeframe,
        "Current_Price": price.value,
        "Current_Price_Timeframe": price_timeframe,
        "Current_Price_Bar_Time": price.signal_bar_time,
        "Current_Price_Reference_Only": reference_only,
        "Signal_Bar_Time": direction.map(|_| bar.time),
        "Signal_Score_Bar_Time": bar.time,
        "Signal_Score_Bar_Is_Forming": bar.time.saturating_add(
            timeframe_duration_ns(timeframe).unwrap_or_default(),
        ) > current_time_ns(),
        "Current_Price_Data_Source": price.source,
        "Current_Price_Data_Provider": price.provider,
        "Data_Source": source,
        "Data_Provider": provider,
        "Data_Quality": if is_research_proxy_source(source) { "RESEARCH_PROXY" } else { "APP_NATIVE" },
    });
    if retained {
        let mut update = candidate.clone();
        update["Score_Update_Only"] = serde_json::Value::Bool(true);
        update["Score_Update_Status"] = serde_json::json!(if direction.is_some() {
            "qualifying"
        } else {
            "not_qualifying"
        });
        // A retained score can stay visible below the floor without presenting
        // its prior direction or magnitude as a current entry.
        if direction.is_none() {
            update["Signal_Strength"] = serde_json::json!(0.0);
        }
        score_updates.push(update);
    }
    if direction.is_some() {
        candidates.push(candidate);
        after_fee_issue(
            input, timeframe, source, provider, bar.time, profile, strength,
        )
    } else {
        None
    }
}

fn active_scoring_profile<'a>(
    input: &'a TimeframeScanInput<'_>,
    timeframe: &str,
    source: &str,
) -> Option<&'a crate::research_reports::ActiveScoringProfile> {
    let profiles = input.active_scoring_profiles?;
    let key = (
        quant_core::ENTRY_SCORE_MODEL_ID.to_string(),
        input.category_id.to_string(),
        timeframe.to_string(),
        source.to_string(),
    );
    profiles.get(&key)
}

fn after_fee_issue(
    input: &TimeframeScanInput<'_>,
    timeframe: &str,
    source: &str,
    provider: &str,
    bar_time: i64,
    profile: Option<&crate::research_reports::ActiveScoringProfile>,
    strength: f64,
) -> Option<ReadinessIssue> {
    let (reason, explanation) = match profile {
        None => (
            "after_fee_profile_missing",
            "No active after-fee profile is available. This setup uses equal weights.",
        ),
        Some(profile) if strength < profile.minimum_score => (
            "below_after_fee_score_cutoff",
            "The setup is below the historical after-fee score cutoff. The cutoff is informational.",
        ),
        _ => return None,
    };
    Some(ReadinessIssue {
        symbol: input.asset.into(),
        reason: reason.into(),
        timeframe: timeframe.into(),
        source: Some(source.into()),
        data_provider: Some(provider.into()),
        last_bar_time: Some(bar_time),
        error: Some(explanation.into()),
        ..ReadinessIssue::default()
    })
}

#[cfg(test)]
#[path = "combined_entry_tests.rs"]
mod tests;
