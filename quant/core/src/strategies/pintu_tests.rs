use super::*;

const QUARTER_HOUR_NS: i64 = PINTU_GOLDEN_CROSS_INTERVAL_NS;
const HOUR_NS: i64 = 4 * QUARTER_HOUR_NS;

fn bar(time: i64, open: f64, high: f64, low: f64, close: f64, volume: f64) -> StrategyBar {
    StrategyBar {
        time,
        open,
        high,
        low,
        close,
        volume,
    }
}

#[test]
fn hourly_aggregation_uses_only_complete_contiguous_utc_hours() {
    let bars = vec![
        bar(0, 10.0, 12.0, 9.0, 11.0, 1.0),
        bar(QUARTER_HOUR_NS, 11.0, 13.0, 10.0, 12.0, 2.0),
        bar(2 * QUARTER_HOUR_NS, 12.0, 14.0, 11.0, 13.0, 3.0),
        bar(3 * QUARTER_HOUR_NS, 13.0, 15.0, 12.0, 14.0, 4.0),
        bar(HOUR_NS, 14.0, 16.0, 13.0, 15.0, 5.0),
    ];

    let hourly = aggregate_complete_hourly_bars(&bars);

    assert_eq!(hourly.len(), 1);
    assert_eq!(hourly[0].time, 0);
    assert_eq!(hourly[0].open, 10.0);
    assert_eq!(hourly[0].high, 15.0);
    assert_eq!(hourly[0].low, 9.0);
    assert_eq!(hourly[0].close, 14.0);
    assert_eq!(hourly[0].volume, 10.0);
}

#[test]
fn configurable_context_aggregation_uses_the_requested_signal_and_context_frames() {
    let signal_interval = 10_i64;
    let context_interval = 30_i64;
    let bars = vec![
        bar(0, 10.0, 12.0, 9.0, 11.0, 1.0),
        bar(10, 11.0, 13.0, 10.0, 12.0, 2.0),
        bar(20, 12.0, 14.0, 11.0, 13.0, 3.0),
        bar(30, 13.0, 15.0, 12.0, 14.0, 4.0),
        bar(40, 14.0, 16.0, 13.0, 15.0, 5.0),
        bar(50, 15.0, 17.0, 14.0, 16.0, 6.0),
    ];

    let context = aggregate_complete_context_bars(&bars, signal_interval, context_interval);

    assert_eq!(context.len(), 2);
    assert_eq!(context[0].time, 0);
    assert_eq!(context[0].close, 13.0);
    assert_eq!(context[1].time, 30);
    assert_eq!(context[1].volume, 15.0);
    assert!(aggregate_complete_context_bars(&bars, signal_interval, 25).is_empty());
}

#[test]
fn hourly_context_uses_the_latest_closed_hour_and_rejects_stale_data() {
    let hourly = vec![
        bar(0, 10.0, 11.0, 9.0, 10.0, 1.0),
        bar(HOUR_NS, 10.0, 11.0, 9.0, 10.0, 1.0),
        bar(2 * HOUR_NS, 10.0, 11.0, 9.0, 10.0, 1.0),
    ];

    assert_eq!(latest_completed_hourly_index(&hourly, HOUR_NS - 1), None);
    assert_eq!(latest_completed_hourly_index(&hourly, HOUR_NS), Some(0));
    assert_eq!(latest_completed_hourly_index(&hourly, 2 * HOUR_NS), Some(1));
    assert_eq!(
        latest_completed_hourly_index(&hourly, 5 * HOUR_NS + 1),
        None
    );
}

#[test]
fn hourly_oversold_context_is_recent_contiguous_and_uses_closed_readings() {
    let mut config = PintuGoldenCrossConfig::default();
    config.context_dss_oversold_lookback_bars = 4;
    let hourly = vec![
        bar(0, 10.0, 11.0, 9.0, 10.0, 1.0),
        bar(HOUR_NS, 10.0, 11.0, 9.0, 10.0, 1.0),
        bar(2 * HOUR_NS, 10.0, 11.0, 9.0, 10.0, 1.0),
        bar(3 * HOUR_NS, 10.0, 11.0, 9.0, 10.0, 1.0),
    ];

    assert!(has_recent_context_dss_oversold(
        &hourly,
        &[5.0, 40.0, 41.0, 42.0],
        3,
        &config,
    ));
    config.context_dss_oversold_lookback_bars = 3;
    assert!(!has_recent_context_dss_oversold(
        &hourly,
        &[5.0, 40.0, 41.0, 42.0],
        3,
        &config,
    ));

    let gapped = vec![
        hourly[0],
        hourly[1],
        bar(3 * HOUR_NS, 10.0, 11.0, 9.0, 10.0, 1.0),
        hourly[3],
    ];
    config.context_dss_oversold_lookback_bars = 4;
    assert!(!has_recent_context_dss_oversold(
        &gapped,
        &[5.0, 40.0, 41.0, 42.0],
        3,
        &config,
    ));
}

#[test]
fn recent_dss_extreme_requires_a_rising_cross_candle_and_no_gap() {
    let config = PintuGoldenCrossConfig::default();
    let bars: Vec<_> = (0..6)
        .map(|index| bar(index * QUARTER_HOUR_NS, 10.0, 11.0, 9.0, 10.0, 1.0))
        .collect();

    assert!(has_recent_signal_dss_extreme_turn(
        &bars,
        &[50.0, 4.0, 6.0, 7.0, 8.0, 9.0],
        4,
        &config,
    ));
    assert!(!has_recent_signal_dss_extreme_turn(
        &bars,
        &[4.0, 6.0, 7.0, 8.0, 9.0, 10.0],
        4,
        &config,
    ));
    assert!(!has_recent_signal_dss_extreme_turn(
        &bars,
        &[50.0, 4.0, 6.0, 7.0, 6.0, 9.0],
        4,
        &config,
    ));

    let mut gap_bars = bars.clone();
    for bar in &mut gap_bars[3..] {
        bar.time += QUARTER_HOUR_NS;
    }
    assert!(!has_recent_signal_dss_extreme_turn(
        &gap_bars,
        &[50.0, 4.0, 6.0, 7.0, 8.0, 9.0],
        4,
        &config,
    ));
}

#[test]
fn bullish_mss_uses_confirmed_pivots_and_price_must_hold_the_break() {
    let mut config = PintuGoldenCrossConfig::default();
    config.structure_swing_width = 1;
    config.structure_lookback = 20;
    let highs = [
        14.0, 13.0, 12.0, 13.0, 16.0, 14.0, 11.0, 13.0, 15.0, 14.0, 16.0,
    ];
    let lows = [8.0, 9.0, 7.0, 9.0, 10.0, 9.0, 6.0, 9.0, 10.0, 12.0, 14.0];
    let mut bars: Vec<_> = highs
        .iter()
        .zip(lows)
        .enumerate()
        .map(|(index, (high, low))| {
            let close = (high + low) / 2.0;
            bar(
                index as i64 * QUARTER_HOUR_NS,
                close,
                *high,
                low,
                close,
                1.0,
            )
        })
        .collect();
    bars[10].open = 14.5;
    bars[10].close = 15.5;

    assert_eq!(bullish_market_structure_shift_at(&bars, 9, &config), None);
    assert_eq!(
        bullish_market_structure_shift_at(&bars, 10, &config),
        Some(15.0)
    );
    assert!(has_recent_bullish_market_structure_shift(
        &bars, 10, &config
    ));

    bars.push(bar(11 * QUARTER_HOUR_NS, 15.5, 15.8, 14.5, 14.9, 1.0));
    assert!(!has_recent_bullish_market_structure_shift(
        &bars, 11, &config
    ));
}

#[test]
fn only_the_current_version_uses_the_new_evidence_model() {
    assert!(is_pintu_golden_cross_strategy(
        PINTU_GOLDEN_CROSS_STRATEGY_ID
    ));
    assert!(!is_pintu_golden_cross_strategy(
        "pintu_futures_golden_cross_v4_mss_dss5_3r"
    ));
    assert!(!is_pintu_golden_cross_strategy(
        "pintu_futures_golden_cross_v5_h1_dss22_m15_extreme_turn_mss_3r"
    ));
}
