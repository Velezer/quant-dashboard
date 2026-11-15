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
        .and_then(|slices| slices.iter().find(|slice| {
            slice["exchange_id"].as_str() == Some(exchange_id)
                && slice["product_id"].as_str() == Some(product_id)
        }));
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
            let minimum = band["minimum_score_out_of_60"].as_f64().unwrap_or(f64::INFINITY);
            let maximum = band["maximum_score_out_of_60"].as_f64().unwrap_or(f64::NEG_INFINITY);
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
    if score < profile.minimum_score {
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
        .and_then(|slices| slices.iter().find(|slice| {
            slice["exchange_id"].as_str() == Some(exchange_id)
                && slice["product_id"].as_str() == Some(product_id)
        }));
    let Some(product_slice) = product_slice else {
        return Value::Null;
    };
    serde_json::json!({
        "status": profile.after_fee_gate.get("status").cloned().unwrap_or(Value::Null),
        "minimum_score_out_of_60": profile.minimum_score,
        "target_after_fee_win_rate_pct": profile.after_fee_gate
            .get("target_after_fee_win_rate_pct").cloned().unwrap_or(Value::Null),
        "fees_applied": true,
        "validation": product_slice.get("signal_gate_validation").cloned().unwrap_or(Value::Null),
        "holdout": product_slice.get("signal_gate_holdout").cloned().unwrap_or(Value::Null),
        "exchange_id": exchange_id,
        "product_id": product_id,
    })
}

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
    price_timeframe: &str,
    price_reference_only: bool,
    source: &str,
    provider: &str,
    score_components: Vec<Value>,
    score_groups: Option<(f64, f64, f64, f64, f64)>,
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
    let win_probability = win_probability_for_score(
        score_groups.map(|groups| groups.4),
        profile,
        input.exchange_id,
        input.product_id,
    );
    signals.push(serde_json::json!({
        "Exchange_ID": input.exchange_id,
        "Product_ID": input.product_id,
        "Asset_Category_ID": input.category_id,
        "Symbol": input.asset,
        "Data_Symbol": input.symbol,
        "Signal_Strategy_ID": strategy_id,
        "Signal_Direction": signal.direction,
        "Signal_Strength": score_groups.map(|(_, _, _, _, total)| total).unwrap_or(signal.strength),
        "Strategy_Strength": score_groups.map(|(price, _, _, _, _)| price).unwrap_or(signal.strategy_strength),
        "Signal_Confluence_Score": score_groups.map(|(_, volume, _, _, _)| volume)
            .or_else(|| signal.confluence.as_ref().map(|score| score.score)),
        "Signal_Markov_Regime_Score": score_groups.map(|(_, _, markov, _, _)| markov),
        "Signal_Autocorrelation_Score": score_groups.map(|(_, _, _, autocorrelation, _)| autocorrelation),
        "Signal_Markov_Regime": score_components.iter()
            .find(|component| component.get("indicator").and_then(Value::as_str) == Some("Markov Regime"))
            .and_then(|component| component.get("state"))
            .cloned()
            .unwrap_or(Value::Null),
        "Signal_Autocorrelation_State": score_components.iter()
            .find(|component| component.get("indicator").and_then(Value::as_str) == Some("Lag-one Return Autocorrelation"))
            .and_then(|component| component.get("state"))
            .cloned()
            .unwrap_or(Value::Null),
        "Signal_Confluence": if score_groups.is_some() { Value::Null } else {
            serde_json::to_value(signal.confluence.as_ref()).unwrap_or(Value::Null)
        },
        "Signal_Scoring_Model_ID": score_model_id,
        "Signal_Scoring_Weights": score_weights,
        "Signal_Scoring_Profile_Version": profile_version,
        "Signal_After_Fee_Gate": profile
            .map(|profile| after_fee_gate_for_product(profile, input.exchange_id, input.product_id))
            .unwrap_or(Value::Null),
        "Signal_Win_Probability": win_probability,
        "Signal_Score_Components": score_components,
        "timeframe": timeframe,
        "Current_Price": current_price.value,
        "Current_Price_Timeframe": price_timeframe,
        "Current_Price_Bar_Time": current_price.signal_bar_time,
        "Current_Price_Reference_Only": price_reference_only,
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

pub(super) fn six_indicator_score_outputs(
    bars: &[StrategyBar],
    signal: &quant_core::StrategySignal,
    lead_indicator: &str,
    profile: Option<&crate::research_reports::ActiveScoringProfile>,
    markov_regime: Option<quant_core::RegimeId>,
) -> (Vec<Value>, Option<(f64, f64, f64, f64, f64)>) {
    let kind = if lead_indicator == "DSS Bressert" {
        quant_core::EntryModelKind::Dss
    } else {
        quant_core::EntryModelKind::Nwe
    };
    let weights = profile
        .map(|profile| profile.weights)
        .unwrap_or(quant_core::FOUR_INDICATOR_EQUAL_WEIGHTS);
    let Some(score) = quant_core::score_entry_signal_with_markov(
        bars,
        signal,
        kind,
        weights,
        markov_regime,
    ) else {
        return (Vec::new(), None);
    };
    let groups = Some((
        score.price_score,
        score.volume_score,
        score.markov_regime_score,
        score.autocorrelation_score,
        score.total_score,
    ));
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
    markov_regime: Option<quant_core::RegimeId>,
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
    let (score_components, score_groups) = latest_signal
        .map(|signal| six_indicator_score_outputs(bars, signal, indicator, profile, markov_regime))
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
                    serde_json::json!({"indicator": "Markov Regime", "state": "UNAVAILABLE", "points": 0.0,"weight_points":quant_core::MARKOV_REGIME_COMPONENT_WEIGHT,"weighted_points":0.0,"scoring_profile_version":version}),
                    serde_json::json!({"indicator": "Lag-one Return Autocorrelation", "state": "UNAVAILABLE", "points": 0.0,"weight_points":quant_core::STRATEGY_RETURN_AUTOCORRELATION_COMPONENT_WEIGHT,"weighted_points":0.0,"scoring_profile_version":version}),
                ],
                Some((0.0, 0.0, 0.0, 0.0, 0.0)),
            )
        });
    let has_after_fee_setup = latest_signal.is_some();
    let after_fee_issue = if has_after_fee_setup
        && profile.is_none_or(|profile| {
            score_groups.is_none_or(|(_, _, _, _, total)| total < profile.minimum_score)
        })
    {
        let (reason, explanation) = match (profile, score_groups) {
            (None, _) => (
                "after_fee_profile_missing",
                "No active after-fee profile is available; equal-weight scoring is used and this setup is still shown.",
            ),
            (Some(profile), Some((_, _, _, _, score))) if score < profile.minimum_score => (
                "below_after_fee_score_cutoff",
                "The setup is below the historical after-fee score cutoff; the metric is informational and does not hide this signal.",
            ),
            _ => (
                "after_fee_score_unavailable",
                "The after-fee backtest metric is unavailable; this setup is still shown.",
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
                "15m",
                false,
                source,
                provider,
                score_components.clone(),
                score_groups,
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
    let win_probability = win_probability_for_score(
        score_groups.map(|groups| groups.4),
        profile,
        input.exchange_id,
        input.product_id,
    );
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
            "Signal_After_Fee_Gate": profile.map(|profile| {
                after_fee_gate_for_product(profile, input.exchange_id, input.product_id)
            }),
            "Signal_Win_Probability": win_probability,
            "timeframe": timeframe,
            "Signal_Direction": latest_signal.map(|signal| signal.direction.as_str()),
            "Signal_Bar_Time": latest_signal.map(|_| score_bar.time),
            "Signal_Strength": score_groups.map(|(_, _, _, _, total)| total).unwrap_or(0.0),
            "Strategy_Strength": score_groups.map(|(price, _, _, _, _)| price).unwrap_or(0.0),
            "Signal_Confluence_Score": score_groups.map(|(_, volume, _, _, _)| volume),
            "Signal_Markov_Regime_Score": score_groups.map(|(_, _, markov, _, _)| markov),
            "Signal_Autocorrelation_Score": score_groups.map(|(_, _, _, autocorrelation, _)| autocorrelation),
            "Signal_Markov_Regime": score_components.iter()
                .find(|component| component.get("indicator").and_then(Value::as_str) == Some("Markov Regime"))
                .and_then(|component| component.get("state"))
                .cloned()
                .unwrap_or(Value::Null),
            "Signal_Autocorrelation_State": score_components.iter()
                .find(|component| component.get("indicator").and_then(Value::as_str) == Some("Lag-one Return Autocorrelation"))
                .and_then(|component| component.get("state"))
                .cloned()
                .unwrap_or(Value::Null),
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
    after_fee_issue
}
