use super::dss_bressert_4h::{near_cross_component_score, stockbit_dss_bressert_4h_candidate};
use super::{
    generate_stockbit_dss_bressert_4h_signals, StrategyBar, DSS_BRESSERT_MINIMUM_HISTORY_BARS,
    DSS_BRESSERT_NEAR_SPREAD_THRESHOLD, NWE_DSS_COMPONENT_SCORE_SCALE_MAX,
    NWE_DSS_SIGNAL_MINIMUM_SCORE_OUT_OF_20, NWE_DSS_SIGNAL_SCORE_SCALE_MAX,
    STOCKBIT_DSS_BRESSERT_4H_SIGNAL_CONFIG,
};

fn alternating_bars(count: usize) -> Vec<StrategyBar> {
    (0..count)
        .map(|index| {
            let close: f64 = if index % 2 == 0 { 99.0 } else { 101.0 };
            StrategyBar {
                time: index as i64,
                open: 100.0,
                high: close.max(100.0) + 1.0,
                low: close.min(100.0) - 1.0,
                close,
                volume: 1_000.0,
            }
        })
        .collect()
}

#[test]
fn near_cross_component_scores_rise_to_ten_as_the_spread_approaches_zero() {
    let config = STOCKBIT_DSS_BRESSERT_4H_SIGNAL_CONFIG;
    let at_threshold = near_cross_component_score(DSS_BRESSERT_NEAR_SPREAD_THRESHOLD, config)
        .expect("the inclusive near-spread boundary qualifies");
    let close_to_cross = near_cross_component_score(0.5, config)
        .expect("a close near-cross remains a near-cross score");
    assert_eq!(at_threshold, 0.0);
    assert!(close_to_cross > at_threshold);
    assert!(close_to_cross <= NWE_DSS_COMPONENT_SCORE_SCALE_MAX);
    assert_eq!(
        near_cross_component_score(DSS_BRESSERT_NEAR_SPREAD_THRESHOLD + 0.01, config),
        None
    );
    assert_eq!(near_cross_component_score(f64::NAN, config), None);
}

#[test]
fn dss_bullish_and_bearish_near_crosses_keep_direction_and_score_basis() {
    let config = STOCKBIT_DSS_BRESSERT_4H_SIGNAL_CONFIG;
    let bullish = stockbit_dss_bressert_4h_candidate(-2.5, 2, "APPROACHING_BULLISH_NEAR", config)
        .expect("a bullish near-cross should qualify");
    let bearish = stockbit_dss_bressert_4h_candidate(2.5, -2, "APPROACHING_BEARISH_NEAR", config)
        .expect("a bearish near-cross should qualify");
    assert_eq!(bullish.0, "LONG");
    assert_eq!(bullish.1, "DSS_BULLISH_NEAR_CROSS_LONG");
    assert_eq!(bearish.0, "SHORT");
    assert_eq!(bearish.1, "DSS_BEARISH_NEAR_CROSS_SHORT");
    assert!(bullish.2 > 0.0 && bullish.2 <= NWE_DSS_COMPONENT_SCORE_SCALE_MAX);
    assert_eq!(
        stockbit_dss_bressert_4h_candidate(-2.5, -2, "APPROACHING_BULLISH_NEAR", config),
        None,
        "a conflicting classifier direction must not produce a row"
    );
}

#[test]
fn confirmed_dss_crosses_start_with_ten_component_points() {
    let bullish = stockbit_dss_bressert_4h_candidate(
        1.0,
        3,
        "BULLISH_CROSS",
        STOCKBIT_DSS_BRESSERT_4H_SIGNAL_CONFIG,
    )
    .expect("a confirmed bullish cross should qualify");
    let bearish = stockbit_dss_bressert_4h_candidate(
        -1.0,
        -3,
        "BEARISH_CROSS",
        STOCKBIT_DSS_BRESSERT_4H_SIGNAL_CONFIG,
    )
    .expect("a confirmed bearish cross should qualify");
    assert_eq!(bullish.0, "LONG");
    assert_eq!(bearish.0, "SHORT");
    assert_eq!(bullish.2, NWE_DSS_COMPONENT_SCORE_SCALE_MAX);
    assert_eq!(bearish.2, NWE_DSS_COMPONENT_SCORE_SCALE_MAX);
}

#[test]
fn generated_series_uses_completed_dss_near_crosses_and_withholds_bad_history() {
    let history = alternating_bars(DSS_BRESSERT_MINIMUM_HISTORY_BARS + 20);
    let signals = generate_stockbit_dss_bressert_4h_signals(&history);
    assert_eq!(signals.len(), history.len());
    assert!(signals.iter().any(|signal| {
        matches!(signal.direction.as_str(), "LONG" | "SHORT")
            && signal.strength >= NWE_DSS_SIGNAL_MINIMUM_SCORE_OUT_OF_20
            && signal.strength <= NWE_DSS_SIGNAL_SCORE_SCALE_MAX
            && (signal.strength
                - signal.strategy_strength
                - signal.confluence.as_ref().unwrap().score)
                .abs()
                < 1e-10
    }));

    let mut unordered = history.clone();
    unordered.swap(4, 5);
    assert!(generate_stockbit_dss_bressert_4h_signals(&unordered)
        .iter()
        .all(|signal| signal.direction == "NEUTRAL"));

    let insufficient = alternating_bars(DSS_BRESSERT_MINIMUM_HISTORY_BARS - 1);
    assert!(generate_stockbit_dss_bressert_4h_signals(&insufficient)
        .iter()
        .all(|signal| signal.direction == "NEUTRAL"));
}
