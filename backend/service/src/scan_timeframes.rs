use super::*;

#[path = "scan_timeframes/market_inputs.rs"]
mod market_inputs;
use market_inputs::{
    annotate_current_price_provenance, completed_bars, current_time_ns, fresh_current_price,
    load_stockbit_signal_candles, CurrentPrice,
};

#[path = "scan_timeframes/nwe_dss_models.rs"]
mod nwe_dss_models;
use nwe_dss_models::append_nwe_dss_model_signals;

pub(super) struct TimeframeScanInput<'a> {
    pub(super) app_connection: &'a Connection,
    pub(super) market_connection: &'a Connection,
    pub(super) sync_state: Option<&'a AppState>,
    pub(super) exchange_id: &'a str,
    pub(super) product_id: &'a str,
    pub(super) category_id: &'a str,
    pub(super) symbol: &'a str,
    pub(super) asset: &'a str,
    pub(super) strategy: &'a str,
    pub(super) evidence_class: &'a str,
    pub(super) class: &'a str,
    pub(super) exchange: &'a str,
    pub(super) time_window: &'a str,
    pub(super) base_timeframe: &'a str,
    pub(super) is_tokenized: bool,
    pub(super) is_multibagger: bool,
    pub(super) retained_stockbit_signal_rows: &'a std::collections::BTreeSet<(String, String)>,
}

pub(super) fn append_timeframe_signal_plans(
    input: TimeframeScanInput<'_>,
    running_plans: &mut Vec<Value>,
    potential_plans: &mut Vec<Value>,
    cagr_stats: &mut HashMap<String, Value>,
    user_indicator_alerts: &mut Vec<Value>,
    standalone_indicator_signals: &mut Vec<Value>,
    standalone_score_updates: &mut Vec<Value>,
) -> Result<bool, rusqlite::Error> {
    let now_ns = current_time_ns();
    let is_stockbit_signal_scope = input.exchange_id == quant_core::STOCKBIT_NWE_4H_EXCHANGE_ID
        && input.product_id == quant_core::STOCKBIT_NWE_4H_PRODUCT_ID
        && input.category_id == quant_core::STOCKBIT_NWE_4H_ASSET_CATEGORY_ID;
    let stockbit_intraday_state = if is_stockbit_signal_scope {
        product_candle_scan_state_at(
            input.market_connection,
            input.symbol,
            input.exchange_id,
            input.product_id,
            "15m",
            now_ns.div_euclid(NANOS_PER_SECOND),
        )
        .ok()
        .flatten()
    } else {
        None
    };
    let current_price = fresh_current_price(&input, stockbit_intraday_state.as_ref(), now_ns);
    if current_price.is_none() {
        if let Some(state) = input.sync_state {
            state.prioritize_stale_signal_sync(
                input.symbol,
                input.exchange_id,
                input.product_id,
                "15m",
            );
        }
    }

    let required_history_bars =
        quant_core::NadarayaWatsonEnvelopeConfig::default().minimum_endpoint_envelope_bars();
    let history_limit = required_history_bars.max(720).min(MAX_CANDLES as usize) as i64;
    let stockbit_intraday_is_fresh = stockbit_intraday_state
        .as_ref()
        .is_some_and(|metadata| !metadata.is_stale);
    for (frame_index, definition) in DSS_BRESSERT_MTF_TIMEFRAMES.iter().enumerate() {
        let timeframe = definition.timeframe;
        let is_crypto_signal_scope = input.category_id.starts_with("crypto_");
        let is_nwe_dss_signal_scope = is_stockbit_signal_scope || is_crypto_signal_scope;
        let timeframe_required_history_bars = if is_nwe_dss_signal_scope {
            quant_core::stockbit_nwe_4h_minimum_history_bars()
        } else {
            required_history_bars
        };
        // Most strategies use the latest fresh 15m price. Stockbit NWE/DSS scores
        // the matching timeframe bar, using a forming bar when fresh 15m input is
        // available and the latest stored bar otherwise; crypto uses closed bars.
        // Keep the daily recovery check without loading unused daily history.
        if current_price.is_none() && timeframe != "1d" && !is_nwe_dss_signal_scope {
            continue;
        }
        let timeframe_state = product_candle_scan_state_at(
            input.market_connection,
            input.symbol,
            input.exchange_id,
            input.product_id,
            timeframe,
            now_ns.div_euclid(NANOS_PER_SECOND),
        )
        .ok()
        .flatten();
        let stale = timeframe_state
            .as_ref()
            .map(|metadata| metadata.is_stale)
            .unwrap_or(true);
        if timeframe == "1d" && stale {
            if let Some(state) = input.sync_state {
                state.prioritize_stale_signal_sync(
                    input.symbol,
                    input.exchange_id,
                    input.product_id,
                    "1d",
                );
            }
        }
        if current_price.is_none() && !is_nwe_dss_signal_scope {
            continue;
        }
        let duration_ns = timeframe_duration_ns(timeframe).unwrap_or_default();
        let use_stockbit_timeframe_history = is_stockbit_signal_scope && !stale;
        if (stale && !stockbit_intraday_is_fresh) || duration_ns <= 0 {
            continue;
        }
        let source_metadata = if is_stockbit_signal_scope {
            match (
                timeframe_state.as_ref(),
                stockbit_intraday_state.as_ref(),
                current_price.as_ref(),
            ) {
                (Some(timeframe_source), Some(intraday), Some(_))
                    if timeframe_source.source == intraday.source && stockbit_intraday_is_fresh =>
                {
                    Some((intraday.source.clone(), intraday.data_provider.clone()))
                }
                (Some(timeframe_source), _, _) if use_stockbit_timeframe_history => Some((
                    timeframe_source.source.clone(),
                    timeframe_source.data_provider.clone(),
                )),
                _ => None,
            }
        } else {
            timeframe_state
                .as_ref()
                .map(|metadata| (metadata.source.clone(), metadata.data_provider.clone()))
        };
        let candles = if is_stockbit_signal_scope {
            match (source_metadata.as_ref(), current_price.as_ref()) {
                (Some((source, _)), Some(current_price)) => load_stockbit_signal_candles(
                    &input,
                    timeframe,
                    history_limit,
                    source,
                    current_price.signal_bar_time,
                ),
                (Some((source, _)), None) => load_series_candles_from_source(
                    input.market_connection,
                    input.symbol,
                    timeframe,
                    history_limit,
                    source,
                )
                .unwrap_or_default(),
                _ => Vec::new(),
            }
        } else {
            source_metadata
                .as_ref()
                .map(|(source, _)| {
                    load_series_candles_from_source(
                        input.market_connection,
                        input.symbol,
                        timeframe,
                        history_limit,
                        source,
                    )
                    .unwrap_or_default()
                })
                .unwrap_or_default()
        };
        let completed_strategy_bars = completed_bars(&candles, duration_ns, now_ns);
        let bars = if is_stockbit_signal_scope {
            candles
                .iter()
                .map(|bar| StrategyBar {
                    time: bar.time,
                    open: bar.open,
                    high: bar.high,
                    low: bar.low,
                    close: bar.close,
                    volume: bar.volume,
                })
                .collect::<Vec<_>>()
        } else {
            completed_strategy_bars.clone()
        };
        let minimum_available_history_bars = if is_stockbit_signal_scope || is_crypto_signal_scope {
            quant_core::DSS_BRESSERT_MINIMUM_HISTORY_BARS
        } else {
            timeframe_required_history_bars
        };
        if bars.len() < minimum_available_history_bars {
            continue;
        }

        if is_nwe_dss_signal_scope {
            append_nwe_dss_model_signals(
                &input,
                is_stockbit_signal_scope,
                timeframe,
                source_metadata.as_ref(),
                &bars,
                current_price.as_ref(),
                standalone_indicator_signals,
                standalone_score_updates,
            );
        }

        if completed_strategy_bars.len() < timeframe_required_history_bars {
            continue;
        }

        if current_price.is_some() {
            if let Some((source, provider)) = source_metadata.as_ref() {
                if let Ok(alerts) = signal_alerts::evaluate_indicator_models_for_scan(
                    input.app_connection,
                    input.exchange_id,
                    input.product_id,
                    input.category_id,
                    input.symbol,
                    timeframe,
                    &completed_strategy_bars,
                    source,
                    provider,
                ) {
                    user_indicator_alerts.extend(alerts);
                }
            }
        }

        let Some(current_price) = current_price.as_ref() else {
            continue;
        };

        let is_pintu_golden_cross = is_pintu_golden_cross_strategy(input.strategy);
        let next = DSS_BRESSERT_MTF_TIMEFRAMES
            .get(frame_index + 1)
            .unwrap_or(&DSS_BRESSERT_MTF_TIMEFRAMES[frame_index]);
        let context_timeframe = if is_pintu_golden_cross {
            next.timeframe
        } else {
            timeframe
        };
        let context_interval_ns = timeframe_duration_ns(context_timeframe).unwrap_or_default();
        let mut context_bars = None;
        if is_pintu_golden_cross && context_timeframe != timeframe {
            let context_state = product_candle_scan_state_at(
                input.market_connection,
                input.symbol,
                input.exchange_id,
                input.product_id,
                context_timeframe,
                now_ns.div_euclid(NANOS_PER_SECOND),
            )
            .ok()
            .flatten();
            if let Some(context_state) = context_state.filter(|state| !state.is_stale) {
                let context_candles = load_series_candles_from_source(
                    input.market_connection,
                    input.symbol,
                    context_timeframe,
                    720,
                    &context_state.source,
                )
                .unwrap_or_default();
                context_bars = Some(completed_bars(
                    &context_candles,
                    context_interval_ns,
                    now_ns,
                ));
            }
        }
        let context_slice = if context_timeframe == timeframe {
            Some(completed_strategy_bars.as_slice())
        } else {
            context_bars
                .as_deref()
                .filter(|context| !context.is_empty())
        };
        let signal_context = if is_pintu_golden_cross {
            context_slice.map(|context_bars| quant_core::PintuSignalTimeframeContext {
                signal_timeframe: timeframe,
                signal_interval_ns: duration_ns,
                context_timeframe,
                context_bars,
                context_interval_ns,
            })
        } else {
            None
        };
        if is_pintu_golden_cross && signal_context.is_none() {
            continue;
        }
        let isolate_evidence = !timeframe.eq_ignore_ascii_case(input.base_timeframe);
        let options = ScanPlanOptions {
            timeframe: Some(timeframe),
            signal_context,
            current_price: Some(current_price.value),
            isolate_evidence,
            suffix_plan_id: !timeframe.eq_ignore_ascii_case(input.base_timeframe),
            signals_only: true,
        };
        // The screener stores directional signals only; empty watchlist plans
        // were discarded later and needlessly accumulated across the universe.
        let allow_watchlist = false;
        let running_start = running_plans.len();
        let potential_start = potential_plans.len();
        append_scan_plan_with_options(
            input.market_connection,
            input.app_connection,
            running_plans,
            potential_plans,
            &completed_strategy_bars,
            input.symbol,
            input.asset,
            input.strategy,
            input.evidence_class,
            input.class,
            input.exchange,
            input.time_window,
            input.is_tokenized,
            input.is_multibagger,
            allow_watchlist,
            cagr_stats,
            input.product_id,
            options,
        );
        annotate_current_price_provenance(
            running_plans,
            potential_plans,
            current_price,
            timeframe,
            running_start,
            potential_start,
        );
    }
    Ok(current_price.is_some())
}

#[path = "scan_timeframes/score_rows.rs"]
mod score_rows;
use score_rows::{
    append_stockbit_model_result, append_timeframe_signal_candidate, four_indicator_score_outputs,
};

pub(super) fn append_timeframe_to_plan_id(plan: &mut Value, timeframe: &str) {
    if let Some(plan_id) = plan
        .get("Plan_ID")
        .and_then(Value::as_str)
        .map(str::to_string)
    {
        if !plan_id.ends_with(&format!("::{timeframe}")) {
            if let Some(object) = plan.as_object_mut() {
                object.insert(
                    "Plan_ID".to_string(),
                    serde_json::json!(format!("{plan_id}::{timeframe}")),
                );
            }
        }
    }
}

pub(super) fn annotate_plan_signal_timeframe(plan: &mut Value, timeframe: &str) {
    plan["timeframe"] = serde_json::json!(timeframe);
    plan["Zone_Timeframe"] = serde_json::json!(timeframe);
    for key in ["Invalidation", "Catalyst_Thesis", "DSS_Regime"] {
        let Some(description) = plan.get(key).and_then(Value::as_str).map(str::to_string) else {
            continue;
        };
        if description.contains("15m") {
            plan[key] = serde_json::json!(description.replace("15m", timeframe));
        }
    }
}

pub(super) fn annotate_new_dss_screener_rows(
    running_plans: &mut [Value],
    potential_plans: &mut [Value],
    running_start: usize,
    potential_start: usize,
    full_diagnostic_timeframe: &str,
    diagnostic: &Value,
) {
    for plan in running_plans[running_start..]
        .iter_mut()
        .chain(potential_plans[potential_start..].iter_mut())
    {
        let timeframe = plan
            .get("timeframe")
            .or_else(|| plan.get("Zone_Timeframe"))
            .and_then(Value::as_str)
            .unwrap_or("15m");
        let frame = diagnostic
            .get("timeframes")
            .and_then(Value::as_array)
            .and_then(|frames| {
                frames.iter().find(|frame| {
                    frame
                        .get("timeframe")
                        .and_then(Value::as_str)
                        .is_some_and(|value| value.eq_ignore_ascii_case(timeframe))
                })
            });
        let use_full_diagnostic = timeframe.eq_ignore_ascii_case(full_diagnostic_timeframe);
        if let Some(object) = plan.as_object_mut() {
            if use_full_diagnostic {
                object.insert("DSS_Bressert_MTF".to_string(), diagnostic.clone());
            } else if let Some(frame) = frame {
                object.insert("DSS_Bressert_Frame".to_string(), frame.clone());
            }
        }
    }
}

#[cfg(test)]
#[path = "scan_timeframe_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "scan_timeframe_nwe_tests.rs"]
mod nwe_tests;

#[cfg(test)]
#[path = "scan_timeframe_nwe_matrix_tests.rs"]
mod nwe_matrix_tests;

#[cfg(test)]
#[path = "scan_timeframe_dss_bressert_tests.rs"]
mod dss_bressert_tests;

#[cfg(test)]
#[path = "scan_timeframe_crypto_4h_tests.rs"]
mod crypto_4h_tests;
