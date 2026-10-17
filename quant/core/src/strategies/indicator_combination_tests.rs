use super::{generate_indicator_combination_signals, IndicatorCombinationConfig, StrategyBar};

pub(super) fn bars_with_bullish_breakout() -> Vec<StrategyBar> {
    let mut bars = (0..59)
        .map(|index| StrategyBar {
            time: index as i64 * 900,
            open: 100.0,
            high: 101.0,
            low: 99.0,
            close: 100.0,
            volume: 1.0,
        })
        .collect::<Vec<_>>();
    bars.push(StrategyBar {
        time: 59 * 900,
        open: 100.0,
        high: 103.0,
        low: 99.0,
        close: 102.0,
        volume: 3.0,
    });
    bars
}

#[test]
fn ordinary_user_indicator_combinations_still_require_every_selected_vote() {
    let indicators = ["ema_20_50", "rsi_14", "range_breakout", "relative_volume"]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let signals = generate_indicator_combination_signals(
        &bars_with_bullish_breakout(),
        &indicators,
        IndicatorCombinationConfig::default(),
        false,
    )
    .expect("the supported four-indicator selection should validate");

    assert_eq!(signals.last().unwrap().direction, "NEUTRAL");
}

#[test]
fn minimum_vote_model_emits_long_when_one_selected_component_has_no_vote() {
    let indicators = ["ema_20_50", "rsi_14", "range_breakout", "relative_volume"]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let signals =
        super::indicator_combination::generate_indicator_combination_signals_with_minimum(
            &bars_with_bullish_breakout(),
            &indicators,
            IndicatorCombinationConfig {
                relative_volume_min: 1.8,
                ..IndicatorCombinationConfig::default()
            },
            false,
            2,
            "TOKOCRYPTO_CONFLUENCE",
        )
        .expect("the supported four-indicator selection should validate");
    let signal = signals.last().unwrap();

    assert_eq!(signal.direction, "LONG");
    assert_eq!(signal.regime, "TOKOCRYPTO_CONFLUENCE_3_OF_4_LONG");
    assert_eq!(signal.strength, 7.5);
}

#[test]
fn minimum_vote_model_never_emits_short_when_category_is_long_only() {
    let mut bars = bars_with_bullish_breakout();
    for (index, bar) in bars.iter_mut().enumerate().skip(40) {
        let close = 100.0 - (index - 39) as f64 * 0.2;
        bar.open = close + 0.1;
        bar.high = bar.open + 0.2;
        bar.low = close - 0.2;
        bar.close = close;
        bar.volume = if index == 59 { 3.0 } else { 1.0 };
    }
    let indicators = ["ema_20_50", "rsi_14", "range_breakout", "relative_volume"]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let signals =
        super::indicator_combination::generate_indicator_combination_signals_with_minimum(
            &bars,
            &indicators,
            IndicatorCombinationConfig::default(),
            false,
            2,
            "TOKOCRYPTO_CONFLUENCE",
        )
        .expect("the supported four-indicator selection should validate");

    assert_ne!(signals.last().unwrap().direction, "SHORT");
}
