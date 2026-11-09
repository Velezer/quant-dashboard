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
) -> Vec<ReadinessIssue> {
    let mut after_fee_diagnostics = Vec::new();
    let Some((source, provider)) = source_metadata else {
        return after_fee_diagnostics;
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
            if let Some(issue) = append_stockbit_model_result(
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
            ) {
                after_fee_diagnostics.push(issue);
            }
        } else {
            let crypto_context = CryptoSignalContext {
                input,
                timeframe,
                source,
                provider,
                bars,
                current_price,
            };
            if let Some(issue) = append_crypto_model_signal(
                &crypto_context,
                model,
                &signals,
                strategy_id,
                indicator,
                standalone_indicator_signals,
            ) {
                after_fee_diagnostics.push(issue);
            }
        }
    }
    after_fee_diagnostics
}

fn append_crypto_model_signal(
    context: &CryptoSignalContext<'_, '_>,
    model: quant_core::EntryModelKind,
    signals: &[quant_core::StrategySignal],
    strategy_id: Option<String>,
    indicator: &str,
    candidates: &mut Vec<Value>,
) -> Option<ReadinessIssue> {
    let Some(strategy_id) = strategy_id else {
        return None;
    };
    let Some((signal_index, signal)) = latest_crypto_signal_for_latest_bar(signals, context.bars)
    else {
        return None;
    };
    let Some(bar) = context
        .bars
        .get(signal_index)
        .filter(|bar| bar.close.is_finite() && bar.close > 0.0)
    else {
        return None;
    };
    let Some(current_price) = context
        .current_price
        .filter(|price| price.source == context.source)
    else {
        return None;
    };

    let profile = active_scoring_profile(
        context.input,
        model.score_model_id(),
        context.timeframe,
        context.source,
    );
    let (score_components, four_factor_groups) = four_indicator_score_outputs(
        &context.bars[..=signal_index],
        signal,
        indicator,
        profile,
    );
    let score = four_factor_groups.map(|(_, _, total)| total);
    let after_fee_issue = if profile.is_none_or(|profile| {
        score.is_none_or(|score| score < profile.minimum_score)
    }) {
        let (reason, explanation) = match (profile, score) {
            (None, _) => (
                "after_fee_profile_missing",
                "No active after-fee profile is available; equal-weight scoring is used and this setup is still shown.",
            ),
            (Some(profile), Some(score)) if score < profile.minimum_score => (
                "below_after_fee_score_cutoff",
                "The setup is below the historical after-fee score cutoff; the metric is informational and does not hide this signal.",
            ),
            _ => (
                "after_fee_score_unavailable",
                "The after-fee backtest metric is unavailable; this setup is still shown.",
            ),
        };
        Some(ReadinessIssue {
            symbol: context.input.asset.into(),
            reason: reason.into(),
            timeframe: context.timeframe.into(),
            source: Some(context.source.into()),
            data_provider: Some(context.provider.into()),
            last_bar_time: Some(bar.time),
            error: Some(explanation.into()),
            ..ReadinessIssue::default()
        })
    } else {
        None
    };
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
        profile,
    );
    after_fee_issue
}

fn latest_crypto_signal_for_latest_bar<'a>(
    signals: &'a [quant_core::StrategySignal],
    bars: &[StrategyBar],
) -> Option<(usize, &'a quant_core::StrategySignal)> {
    let latest_index = signals.len().min(bars.len()).checked_sub(1)?;
    let latest_signal = signals.get(latest_index)?;
    matches!(latest_signal.direction.as_str(), "LONG" | "SHORT")
        .then_some((latest_index, latest_signal))
}

#[cfg(test)]
#[path = "nwe_dss_models_tests.rs"]
mod tests;
