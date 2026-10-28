use super::market_inputs::CurrentPrice;
use super::*;

pub(super) fn append_nwe_dss_model_signals(
    input: &TimeframeScanInput<'_>,
    is_stockbit_signal_scope: bool,
    timeframe: &str,
    source_metadata: Option<&(String, String)>,
    bars: &[StrategyBar],
    current_price: Option<&CurrentPrice>,
    standalone_indicator_signals: &mut Vec<Value>,
    standalone_score_updates: &mut Vec<Value>,
) {
    let Some((source, provider)) = source_metadata else {
        return;
    };

    if bars.len() >= quant_core::stockbit_nwe_4h_minimum_history_bars() {
        let signals = quant_core::generate_stockbit_nwe_4h_signals(bars);
        if is_stockbit_signal_scope {
            let Some(strategy_id) = quant_core::stockbit_nwe_signal_strategy_id(timeframe) else {
                return;
            };
            append_stockbit_model_result(
                standalone_indicator_signals,
                standalone_score_updates,
                input,
                &signals,
                bars,
                &strategy_id,
                quant_core::STOCKBIT_NWE_4H_SIGNAL_SCORE_MODEL_ID,
                timeframe,
                source,
                provider,
                quant_core::NWE_METHOD_LABEL,
                current_price,
            );
        } else if let Some((signal_index, signal)) = latest_active_crypto_signal(
            &signals,
            bars,
            timeframe,
            current_time_ns().div_euclid(NANOS_PER_SECOND),
        ) {
            if let (Some(bar), Some(current_price)) = (
                bars.get(signal_index)
                    .filter(|bar| bar.close.is_finite() && bar.close > 0.0),
                current_price.filter(|price| price.source == *source),
            ) {
                if let Some(strategy_id) = quant_core::crypto_nwe_signal_strategy_id(timeframe) {
                    let (score_components, four_factor_groups) =
                        four_indicator_score_outputs(&bars[..=signal_index], signal, "NWE");
                    append_timeframe_signal_candidate(
                        standalone_indicator_signals,
                        input,
                        signal,
                        &strategy_id,
                        quant_core::STOCKBIT_NWE_4H_SIGNAL_SCORE_MODEL_ID,
                        timeframe,
                        bar,
                        bar,
                        current_price,
                        source,
                        provider,
                        score_components,
                        four_factor_groups,
                    );
                }
            }
        }
    }

    if bars.len() < quant_core::DSS_BRESSERT_MINIMUM_HISTORY_BARS {
        return;
    }
    let signals = quant_core::generate_stockbit_dss_bressert_4h_signals(bars);
    if is_stockbit_signal_scope {
        let Some(strategy_id) = quant_core::stockbit_dss_bressert_signal_strategy_id(timeframe)
        else {
            return;
        };
        append_stockbit_model_result(
            standalone_indicator_signals,
            standalone_score_updates,
            input,
            &signals,
            bars,
            &strategy_id,
            quant_core::STOCKBIT_DSS_BRESSERT_4H_SIGNAL_SCORE_MODEL_ID,
            timeframe,
            source,
            provider,
            "DSS Bressert",
            current_price,
        );
    } else if let Some((signal_index, signal)) = latest_active_crypto_signal(
        &signals,
        bars,
        timeframe,
        current_time_ns().div_euclid(NANOS_PER_SECOND),
    ) {
        if let (Some(bar), Some(current_price)) = (
            bars.get(signal_index),
            current_price.filter(|price| price.source == *source),
        ) {
            if bar.close.is_finite() && bar.close > 0.0 {
                let (score_components, four_factor_groups) =
                    four_indicator_score_outputs(&bars[..=signal_index], signal, "DSS Bressert");
                if let Some(strategy_id) =
                    quant_core::crypto_dss_bressert_signal_strategy_id(timeframe)
                {
                    append_timeframe_signal_candidate(
                        standalone_indicator_signals,
                        input,
                        signal,
                        &strategy_id,
                        quant_core::STOCKBIT_DSS_BRESSERT_4H_SIGNAL_SCORE_MODEL_ID,
                        timeframe,
                        bar,
                        bar,
                        current_price,
                        source,
                        provider,
                        score_components,
                        four_factor_groups,
                    );
                }
            }
        }
    }
}

fn latest_active_crypto_signal<'a>(
    signals: &'a [quant_core::StrategySignal],
    bars: &[StrategyBar],
    timeframe: &str,
    now_seconds: i64,
) -> Option<(usize, &'a quant_core::StrategySignal)> {
    signals
        .iter()
        .enumerate()
        .rev()
        .find(|(index, signal)| {
            matches!(signal.direction.as_str(), "LONG" | "SHORT")
                && bars.get(*index).is_some_and(|bar| {
                    !signal_alerts::signal_candle_is_expired(bar.time, timeframe, now_seconds)
                })
        })
}

#[cfg(test)]
#[path = "nwe_dss_models_tests.rs"]
mod tests;
