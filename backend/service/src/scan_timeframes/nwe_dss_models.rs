use super::market_inputs::CurrentPrice;
use super::*;

struct CryptoSignalContext<'scan, 'data> {
    input: &'scan TimeframeScanInput<'data>,
    timeframe: &'scan str,
    source: &'scan str,
    provider: &'scan str,
    bars: &'scan [StrategyBar],
    current_price: Option<&'scan CurrentPrice>,
}

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

    for model in [quant_core::EntryModelKind::Nwe, quant_core::EntryModelKind::Dss] {
        if bars.len() < model.minimum_history_bars() {
            continue;
        }
        let (signals, strategy_id, indicator) = match model {
            quant_core::EntryModelKind::Nwe => (
                quant_core::generate_stockbit_nwe_4h_signals(bars),
                if is_stockbit_signal_scope {
                    quant_core::stockbit_nwe_signal_strategy_id(timeframe)
                } else {
                    quant_core::crypto_nwe_signal_strategy_id(timeframe)
                },
                if is_stockbit_signal_scope {
                    quant_core::NWE_METHOD_LABEL
                } else {
                    "NWE"
                },
            ),
            quant_core::EntryModelKind::Dss => (
                quant_core::generate_stockbit_dss_bressert_4h_signals(bars),
                if is_stockbit_signal_scope {
                    quant_core::stockbit_dss_bressert_signal_strategy_id(timeframe)
                } else {
                    quant_core::crypto_dss_bressert_signal_strategy_id(timeframe)
                },
                "DSS Bressert",
            ),
        };

        if is_stockbit_signal_scope {
            let Some(strategy_id) = strategy_id else {
                continue;
            };
            append_stockbit_model_result(
                standalone_indicator_signals,
                standalone_score_updates,
                input,
                &signals,
                bars,
                &strategy_id,
                model.score_model_id(),
                timeframe,
                source,
                provider,
                indicator,
                current_price,
            );
        } else {
            let crypto_context = CryptoSignalContext {
                input,
                timeframe,
                source,
                provider,
                bars,
                current_price,
            };
            append_crypto_model_signal(
                &crypto_context,
                model,
                &signals,
                strategy_id,
                indicator,
                standalone_indicator_signals,
            );
        }
    }
}

fn append_crypto_model_signal(
    context: &CryptoSignalContext<'_, '_>,
    model: quant_core::EntryModelKind,
    signals: &[quant_core::StrategySignal],
    strategy_id: Option<String>,
    indicator: &str,
    candidates: &mut Vec<Value>,
) {
    let Some(strategy_id) = strategy_id else {
        return;
    };
    let Some((signal_index, signal)) =
        latest_active_crypto_signal(signals, context.bars, context.timeframe)
    else {
        return;
    };
    let Some(bar) = context
        .bars
        .get(signal_index)
        .filter(|bar| bar.close.is_finite() && bar.close > 0.0)
    else {
        return;
    };
    let Some(current_price) = context
        .current_price
        .filter(|price| price.source == context.source)
    else {
        return;
    };

    let (score_components, four_factor_groups) = four_indicator_score_outputs(
        &context.bars[..=signal_index],
        signal,
        indicator,
    );
    append_timeframe_signal_candidate(
        candidates,
        context.input,
        signal,
        &strategy_id,
        model.score_model_id(),
        context.timeframe,
        bar,
        bar,
        current_price,
        context.source,
        context.provider,
        score_components,
        four_factor_groups,
    );
}

fn latest_active_crypto_signal<'a>(
    signals: &'a [quant_core::StrategySignal],
    bars: &[StrategyBar],
    timeframe: &str,
) -> Option<(usize, &'a quant_core::StrategySignal)> {
    let now_seconds = current_time_ns().div_euclid(NANOS_PER_SECOND);
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
