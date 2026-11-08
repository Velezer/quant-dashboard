use super::*;

pub(super) fn append_timeframe_signal_candidate(
    signals: &mut Vec<Value>,
    input: &TimeframeScanInput<'_>,
    signal: &quant_core::StrategySignal,
    strategy_id: &str,
    score_model_id: &str,
    timeframe: &str,
    signal_bar: &quant_core::StrategyBar,
    score_bar: &quant_core::StrategyBar,
    current_price: &CurrentPrice,
    source: &str,
    provider: &str,
    score_components: Vec<Value>,
    four_factor_groups: Option<(f64, f64, f64)>,
    profile: Option<&crate::research_reports::ActiveScoringProfile>,
) {
    let data_quality = if is_research_proxy_source(source) {
        "RESEARCH_PROXY"
    } else {
        "APP_NATIVE"
    };
    let score_weights = score_components
        .iter()
        .map(|component| component.get("weight_points").cloned().unwrap_or(Value::Null))
        .collect::<Vec<_>>();
    let profile_version = score_components
        .first()
        .and_then(|component| component.get("scoring_profile_version"))
        .cloned()
        .unwrap_or(Value::Null);
    signals.push(serde_json::json!({
        "Exchange_ID": input.exchange_id,
        "Product_ID": input.product_id,
        "Asset_Category_ID": input.category_id,
        "Symbol": input.asset,
        "Data_Symbol": input.symbol,
        "Signal_Strategy_ID": strategy_id,
        "Signal_Direction": signal.direction,
        "Signal_Strength": four_factor_groups.map(|(_, _, total)| total).unwrap_or(signal.strength),
        "Strategy_Strength": four_factor_groups.map(|(price, _, _)| price).unwrap_or(signal.strategy_strength),
        "Signal_Confluence_Score": four_factor_groups.map(|(_, volume, _)| volume)
            .or_else(|| signal.confluence.as_ref().map(|score| score.score)),
        "Signal_Confluence": if four_factor_groups.is_some() { Value::Null } else {
            serde_json::to_value(signal.confluence.as_ref()).unwrap_or(Value::Null)
        },
        "Signal_Scoring_Model_ID": score_model_id,
        "Signal_Scoring_Weights": score_weights,
        "Signal_Scoring_Profile_Version": profile_version,
        "Signal_After_Fee_Gate": profile
            .map(|profile| profile.after_fee_gate.clone())
            .unwrap_or(Value::Null),
        "Signal_Requires_After_Fee_Gate": input.active_scoring_profiles.is_some(),
        "Signal_Score_Components": score_components,
        "timeframe": timeframe,
        "Current_Price": current_price.value,
        "Current_Price_Timeframe": "15m",
        "Current_Price_Bar_Time": current_price.signal_bar_time,
        "Signal_Bar_Time": signal_bar.time,
        "Signal_Score_Bar_Time": score_bar.time,
        "Signal_Score_Bar_Is_Forming": score_bar.time.saturating_add(
            timeframe_duration_ns(timeframe).unwrap_or_default(),
        ) > current_time_ns(),
        "Current_Price_Data_Source": current_price.source,
        "Current_Price_Data_Provider": current_price.provider,
        "Data_Source": source,
        "Data_Provider": provider,
        "Data_Quality": data_quality,
    }));
}

pub(super) fn four_indicator_score_outputs(
    bars: &[StrategyBar],
    signal: &quant_core::StrategySignal,
    lead_indicator: &str,
    profile: Option<&crate::research_reports::ActiveScoringProfile>,
) -> (Vec<Value>, Option<(f64, f64, f64)>) {
    let kind = if lead_indicator == "DSS Bressert" {
        quant_core::EntryModelKind::Dss
    } else {
        quant_core::EntryModelKind::Nwe
    };
    let weights = profile
        .map(|profile| profile.weights)
        .unwrap_or(quant_core::FOUR_INDICATOR_EQUAL_WEIGHTS);
    let Some(score) = quant_core::score_entry_signal_with_weights(bars, signal, kind, weights) else {
        return (Vec::new(), None);
    };
    let groups = Some((score.price_score, score.volume_score, score.total_score));
    let components = score
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
                "scoring_profile_version": profile.map(|profile| profile.version),
            })
        })
        .collect();
    (components, groups)
}

pub(super) fn active_scoring_profile<'a>(
    input: &'a TimeframeScanInput<'_>,
    score_model_id: &str,
    timeframe: &str,
    source: &str,
) -> Option<&'a crate::research_reports::ActiveScoringProfile> {
    let profiles = input.active_scoring_profiles?;
    let key = (
        score_model_id.to_string(),
        input.category_id.to_string(),
        timeframe.to_string(),
        source.to_string(),
    );
    profiles.get(&key)
}

pub(super) fn append_stockbit_model_result(
    candidates: &mut Vec<Value>,
    score_updates: &mut Vec<Value>,
    input: &TimeframeScanInput<'_>,
    model_signals: &[quant_core::StrategySignal],
    bars: &[StrategyBar],
    strategy_id: &str,
    score_model_id: &str,
    timeframe: &str,
    source: &str,
    provider: &str,
    indicator: &str,
    current_price: Option<&CurrentPrice>,
) -> Option<ReadinessIssue> {
    let Some(current_price) = current_price.filter(|price| price.source == source) else {
        return None;
    };
    let Some(score_bar) = bars
        .last()
        .filter(|bar| bar.close.is_finite() && bar.close > 0.0)
    else {
        return None;
    };
    let latest_signal = model_signals
        .last()
        .filter(|signal| matches!(signal.direction.as_str(), "LONG" | "SHORT"));
    let profile = active_scoring_profile(input, score_model_id, timeframe, source);
    let (score_components, four_factor_groups) = latest_signal
        .map(|signal| four_indicator_score_outputs(bars, signal, indicator, profile))
        .unwrap_or_else(|| {
            let weights = profile.map(|profile| profile.weights)
                .unwrap_or(quant_core::FOUR_INDICATOR_EQUAL_WEIGHTS);
            let version = profile.map(|profile| profile.version);
            (
                vec![
                    serde_json::json!({"indicator": "Nadaraya-Watson Envelope", "state": "NO_QUALIFYING_SETUP", "points": 0.0,"weight_points":weights[0],"weighted_points":0.0,"scoring_profile_version":version}),
                    serde_json::json!({"indicator": "DSS Bressert", "state": "NO_QUALIFYING_SETUP", "points": 0.0,"weight_points":weights[1],"weighted_points":0.0,"scoring_profile_version":version}),
                    serde_json::json!({"indicator": "Relative Volume", "state": "UNAVAILABLE", "points": 0.0,"weight_points":weights[2],"weighted_points":0.0,"scoring_profile_version":version}),
                    serde_json::json!({"indicator": "Estimated Volume Delta (OHLCV proxy)", "state": "UNAVAILABLE", "points": 0.0,"weight_points":weights[3],"weighted_points":0.0,"scoring_profile_version":version}),
                ],
                Some((0.0, 0.0, 0.0)),
            )
        });
    let gate_required = input.active_scoring_profiles.is_some();
    let has_after_fee_setup = latest_signal.is_some()
        && (!gate_required
            || profile.is_some_and(|profile| {
                four_factor_groups.is_some_and(|(_, _, total)| total >= profile.minimum_score)
            }));
    let blocked_issue = if gate_required && latest_signal.is_some() && !has_after_fee_setup {
        let (reason, explanation) = match (profile, four_factor_groups) {
            (None, _) => (
                "after_fee_profile_missing",
                "No active backtested score range passed the after-fee signal gate.",
            ),
            (Some(profile), Some((_, _, score))) if score < profile.minimum_score => (
                "below_after_fee_score_cutoff",
                "The live score is below this cohort's backtested after-fee score cutoff.",
            ),
            _ => (
                "after_fee_score_unavailable",
                "The current setup score could not be checked against after-fee backtest evidence.",
            ),
        };
        Some(ReadinessIssue {
            symbol: input.asset.into(),
            reason: reason.into(),
            timeframe: timeframe.into(),
            source: Some(source.into()),
            data_provider: Some(provider.into()),
            last_bar_time: Some(score_bar.time),
            error: Some(explanation.into()),
            ..ReadinessIssue::default()
        })
    } else {
        None
    };
    if has_after_fee_setup {
        if let Some(signal) = latest_signal {
            append_timeframe_signal_candidate(
                candidates,
                input,
                signal,
                strategy_id,
                score_model_id,
                timeframe,
                score_bar,
                score_bar,
                current_price,
                source,
                provider,
                score_components.clone(),
                four_factor_groups,
                profile,
            );
        }
    }
    let score_weights = score_components
        .iter()
        .map(|component| component.get("weight_points").cloned().unwrap_or(Value::Null))
        .collect::<Vec<_>>();
    let profile_version = score_components
        .first()
        .and_then(|component| component.get("scoring_profile_version"))
        .cloned()
        .unwrap_or(Value::Null);
    let retained_score_row_exists = input.retained_stockbit_signal_rows.iter().any(
        |(retained_strategy_id, retained_timeframe)| {
            retained_strategy_id == strategy_id && retained_timeframe == timeframe
        },
    );
    if retained_score_row_exists {
        score_updates.push(serde_json::json!({
            "Score_Update_Only": true,
            "Exchange_ID": input.exchange_id,
            "Product_ID": input.product_id,
            "Asset_Category_ID": input.category_id,
            "Symbol": input.asset,
            "Data_Symbol": input.symbol,
            "Signal_Strategy_ID": strategy_id,
            "Signal_Strategy_Name": strategy_id,
            "Signal_Scoring_Model_ID": score_model_id,
            "Signal_Scoring_Weights": score_weights,
            "Signal_Scoring_Profile_Version": profile_version,
            "Signal_After_Fee_Gate": profile.map(|profile| profile.after_fee_gate.clone()),
            "timeframe": timeframe,
            "Signal_Direction": latest_signal.map(|signal| signal.direction.as_str()),
            "Signal_Bar_Time": latest_signal.map(|_| score_bar.time),
            "Signal_Strength": four_factor_groups.map(|(_, _, total)| total).unwrap_or(0.0),
            "Strategy_Strength": four_factor_groups.map(|(price, _, _)| price).unwrap_or(0.0),
            "Signal_Confluence_Score": four_factor_groups.map(|(_, volume, _)| volume),
            "Signal_Score_Components": score_components,
            "Score_Update_Status": if has_after_fee_setup {
                "qualifying"
            } else {
                "not_qualifying"
            },
            "Current_Price": current_price.value,
            "Current_Price_Timeframe": "15m",
            "Current_Price_Bar_Time": current_price.signal_bar_time,
            "Signal_Score_Bar_Time": score_bar.time,
            "Signal_Score_Bar_Is_Forming": score_bar.time.saturating_add(
                timeframe_duration_ns(timeframe).unwrap_or_default(),
            ) > current_time_ns(),
            "Current_Price_Data_Source": current_price.source,
            "Current_Price_Data_Provider": current_price.provider,
            "Data_Source": source,
            "Data_Provider": provider,
            "Data_Quality": if is_research_proxy_source(source) {
                "RESEARCH_PROXY"
            } else {
                "APP_NATIVE"
            },
        }));
    }
    blocked_issue
}
