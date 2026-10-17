use super::{
    generate_stockbit_nwe_4h_signals, stockbit_nwe_4h_minimum_history_bars, StrategyBar,
    StrategySignal, NWE_DSS_SIGNAL_MINIMUM_SCORE_OUT_OF_20, NWE_DSS_SIGNAL_SCORE_SCALE_MAX,
    SIGNAL_STRENGTH_SCORE_SCALE_MAX, STOCKBIT_NWE_4H_SIGNAL_CONFIG,
};

const FOUR_HOUR_NANOSECONDS: i64 = 4 * 60 * 60 * 1_000_000_000;

fn flat_bars(count: usize) -> Vec<StrategyBar> {
    (0..count)
        .map(|index| StrategyBar {
            time: index as i64 * FOUR_HOUR_NANOSECONDS,
            open: 100.0,
            high: 101.0,
            low: 99.0,
            close: 100.0,
            volume: 1_000.0,
        })
        .collect()
}

fn alternating_bars(count: usize) -> Vec<StrategyBar> {
    (0..count)
        .map(|index| {
            let close: f64 = if index % 2 == 0 { 99.0 } else { 101.0 };
            StrategyBar {
                time: index as i64 * FOUR_HOUR_NANOSECONDS,
                open: 100.0,
                high: close.max(100.0) + 1.0,
                low: close.min(100.0) - 1.0,
                close,
                volume: 1_000.0,
            }
        })
        .collect()
}

fn set_final_close(bars: &mut [StrategyBar], close: f64) {
    let latest = bars.last_mut().expect("fixture has a final bar");
    latest.open = 100.0;
    latest.high = close.max(100.0) + 1.0;
    latest.low = close.min(100.0) - 1.0;
    latest.close = close;
}

#[test]
fn downward_cross_of_the_lower_band_generates_a_long_research_signal() {
    let mut bars = flat_bars(stockbit_nwe_4h_minimum_history_bars());
    set_final_close(&mut bars, 80.0);

    let signals = generate_stockbit_nwe_4h_signals(&bars);
    let latest_signal = signals.last().expect("signal series follows input bars");

    assert_eq!(latest_signal.direction, "LONG");
    assert!(latest_signal.strength >= NWE_DSS_SIGNAL_MINIMUM_SCORE_OUT_OF_20);
    assert!(latest_signal.strength <= NWE_DSS_SIGNAL_SCORE_SCALE_MAX);
    assert_eq!(
        latest_signal.strategy_strength,
        SIGNAL_STRENGTH_SCORE_SCALE_MAX
    );
    assert_score_sum(latest_signal);
    assert_eq!(latest_signal.regime, "NWE_LOWER_BAND_CROSS_LONG");
    assert!(latest_signal.confluence.is_some());
}

#[test]
fn upward_cross_of_the_upper_band_generates_a_short_research_signal() {
    let mut bars = flat_bars(stockbit_nwe_4h_minimum_history_bars());
    set_final_close(&mut bars, 120.0);

    let signals = generate_stockbit_nwe_4h_signals(&bars);
    let latest_signal = signals.last().expect("signal series follows input bars");

    assert_eq!(latest_signal.direction, "SHORT");
    assert!(latest_signal.strength >= NWE_DSS_SIGNAL_MINIMUM_SCORE_OUT_OF_20);
    assert!(latest_signal.strength <= NWE_DSS_SIGNAL_SCORE_SCALE_MAX);
    assert_score_sum(latest_signal);
    assert_eq!(latest_signal.regime, "NWE_UPPER_BAND_CROSS_SHORT");
}

fn assert_score_sum(signal: &StrategySignal) {
    let confluence = signal
        .confluence
        .as_ref()
        .expect("DSS score details are present");
    let expected = signal.strategy_strength + confluence.score;
    assert!((signal.strength - expected).abs() < 1e-10);
    assert!(signal.strategy_strength <= SIGNAL_STRENGTH_SCORE_SCALE_MAX);
    assert!(confluence.score <= SIGNAL_STRENGTH_SCORE_SCALE_MAX);
}

#[test]
fn stockbit_nwe_dss_confluence_normalizes_the_available_confirmation_points() {
    use super::confirmations::stockbit_nwe_dss_confluence_score;

    assert_eq!(stockbit_nwe_dss_confluence_score(0.0), 0.0);
    assert_eq!(stockbit_nwe_dss_confluence_score(3.0), 5.0);
    assert_eq!(stockbit_nwe_dss_confluence_score(6.0), 10.0);
}

#[test]
fn a_near_lower_band_position_without_a_cross_generates_a_long_signal() {
    let mut bars = alternating_bars(stockbit_nwe_4h_minimum_history_bars());
    set_final_close(&mut bars, 97.5);

    let signals = generate_stockbit_nwe_4h_signals(&bars);
    let signal = signals.last().unwrap();

    assert_eq!(signal.direction, "LONG");
    assert_eq!(signal.regime, "NWE_LOWER_BAND_NEAR_LONG");
    assert!(signal.strength >= NWE_DSS_SIGNAL_MINIMUM_SCORE_OUT_OF_20);
    assert!(signal.strength <= NWE_DSS_SIGNAL_SCORE_SCALE_MAX);
}

#[test]
fn near_band_scoring_covers_both_sides_of_a_band_and_withholds_the_midline() {
    use super::indicator_combination::stockbit_nwe_4h_near_band_candidate;

    let config = STOCKBIT_NWE_4H_SIGNAL_CONFIG;
    for close in [101.0, 99.0] {
        let candidate = stockbit_nwe_4h_near_band_candidate(close, 100.0, 110.0, config)
            .expect("both sides of the lower band qualify");
        assert_eq!(candidate.0, "LONG");
        assert_eq!(candidate.1, "NWE_LOWER_BAND_NEAR_LONG");
    }
    for close in [109.0, 111.0] {
        let candidate = stockbit_nwe_4h_near_band_candidate(close, 100.0, 110.0, config)
            .expect("both sides of the upper band qualify");
        assert_eq!(candidate.0, "SHORT");
        assert_eq!(candidate.1, "NWE_UPPER_BAND_NEAR_SHORT");
    }

    let edge = stockbit_nwe_4h_near_band_candidate(95.0, 100.0, 110.0, config)
        .expect("the inclusive half-width boundary qualifies");
    let line = stockbit_nwe_4h_near_band_candidate(100.0, 100.0, 110.0, config)
        .expect("the band line receives the strongest proximity score");
    assert_eq!(edge.2, 0.0);
    assert_eq!(line.2, SIGNAL_STRENGTH_SCORE_SCALE_MAX);
    assert!(stockbit_nwe_4h_near_band_candidate(105.0, 100.0, 110.0, config).is_none());
    assert!(stockbit_nwe_4h_near_band_candidate(94.0, 100.0, 110.0, config).is_none());
    assert!(stockbit_nwe_4h_near_band_candidate(100.0, 110.0, 100.0, config).is_none());
}

#[test]
fn a_band_break_emits_only_on_the_crossing_bar() {
    let required_bars = stockbit_nwe_4h_minimum_history_bars();
    let mut bars = flat_bars(required_bars);
    set_final_close(&mut bars, 120.0);
    let crossing_signals = generate_stockbit_nwe_4h_signals(&bars);
    assert_eq!(crossing_signals.last().unwrap().direction, "SHORT");

    bars.push(StrategyBar {
        time: bars.last().unwrap().time + FOUR_HOUR_NANOSECONDS,
        open: 120.0,
        high: 121.0,
        low: 119.0,
        close: 120.0,
        volume: 1_000.0,
    });

    let signals = generate_stockbit_nwe_4h_signals(&bars);

    assert_eq!(signals.last().unwrap().direction, "NEUTRAL");
}

#[test]
fn insufficient_envelope_history_does_not_generate_a_signal() {
    let required_bars = stockbit_nwe_4h_minimum_history_bars();
    let mut bars = flat_bars(required_bars - 1);
    set_final_close(&mut bars, 120.0);

    let signals = generate_stockbit_nwe_4h_signals(&bars);

    assert_eq!(signals.last().unwrap().direction, "NEUTRAL");
}
