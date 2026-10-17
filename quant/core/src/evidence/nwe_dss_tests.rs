use super::{evaluate_nwe_dss_exit_selection, evaluate_window, NWE_DSS_EXIT_CANDIDATES_BARS};
use crate::strategies::{SignalConfluenceScore, StrategyBar, StrategySignal};

fn bar(time: i64, open: f64, close: f64) -> StrategyBar {
    StrategyBar {
        time,
        open,
        high: open.max(close) + 1.0,
        low: open.min(close) - 1.0,
        close,
        volume: 1.0,
    }
}

fn matching_long_signal() -> StrategySignal {
    StrategySignal {
        direction: "LONG".to_string(),
        strength: 12.0,
        strategy_strength: 10.0,
        confluence: Some(SignalConfluenceScore {
            score: 8.0,
            nadaraya_state: "NWE_LOWER_BAND_NEAR_LONG".to_string(),
            dss_state: "APPROACHING_BULLISH_NEAR".to_string(),
            dss_extreme_zone: false,
            dss_points: 0.0,
            dss_extreme_points: 0.0,
            nadaraya_touch_points: 0.0,
            nadaraya_near_line_points: 0.0,
            nadaraya_reclaim_points: 0.0,
        }),
        regime: "DSS_BULLISH_NEAR_CROSS_LONG".to_string(),
        suggested_sl: 0.0,
        suggested_tp1: 0.0,
        suggested_tp2: 0.0,
        exit_signal: None,
    }
}

fn matching_short_signal() -> StrategySignal {
    let mut signal = matching_long_signal();
    signal.direction = "SHORT".to_string();
    signal.confluence.as_mut().unwrap().nadaraya_state = "NWE_UPPER_BAND_NEAR_SHORT".to_string();
    signal
}

#[test]
fn exit_candidates_are_fixed_and_ordered_for_shorter_timeout_ties() {
    assert_eq!(NWE_DSS_EXIT_CANDIDATES_BARS, &[4, 8, 12]);
}

#[test]
fn fixed_timeout_enters_next_open_and_exits_at_timeout_close_after_fees() {
    let bars = [
        bar(0, 100.0, 100.0),
        bar(1, 101.0, 102.0),
        bar(2, 103.0, 104.0),
        bar(3, 105.0, 106.0),
        bar(4, 107.0, 108.0),
    ];
    let mut signals = vec![StrategySignal::default(); bars.len()];
    signals[1] = matching_long_signal();
    let summary = evaluate_window(&bars, &signals, 0, bars.len(), 2, 0.1, 0.2, true, false);
    assert_eq!(summary.trades, 1);
    assert_eq!(summary.wins, 1);
    let result = summary.to_json(true);
    assert_eq!(result["profit_factor_unbounded"], true);
    assert_eq!(result["profit_factor"], 99.0);
    let expected = (106.0 - 103.0) / 103.0 * 100.0 - 0.1 - 0.2 * (106.0 / 103.0);
    assert!((summary.mean_net().expect("one trade") - expected).abs() < 1e-10);
}

#[test]
fn opposing_nwe_direction_does_not_count_as_a_combined_signal() {
    let bars = [
        bar(0, 100.0, 100.0),
        bar(1, 101.0, 102.0),
        bar(2, 103.0, 104.0),
        bar(3, 105.0, 106.0),
        bar(4, 107.0, 108.0),
    ];
    let mut signals = vec![StrategySignal::default(); bars.len()];
    let mut signal = matching_long_signal();
    signal.confluence.as_mut().unwrap().nadaraya_state = "NWE_UPPER_BAND_NEAR_SHORT".to_string();
    signals[1] = signal;
    let summary = evaluate_window(&bars, &signals, 0, bars.len(), 2, 0.0, 0.0, true, false);
    assert_eq!(summary.trades, 0);
}

#[test]
fn exit_side_fees_use_the_exit_transaction_notional_for_both_directions() {
    let rising = [
        bar(0, 100.0, 100.0),
        bar(1, 100.0, 100.0),
        bar(2, 120.0, 120.0),
    ];
    let mut long_signals = vec![StrategySignal::default(); rising.len()];
    long_signals[0] = matching_long_signal();
    let long = evaluate_window(
        &rising,
        &long_signals,
        0,
        rising.len(),
        2,
        1.0,
        2.0,
        true,
        false,
    );
    let expected_long = 20.0 - 1.0 - 2.0 * 1.2;
    assert!((long.mean_net().expect("one long trade") - expected_long).abs() < 1e-10);

    let falling = [
        bar(0, 100.0, 100.0),
        bar(1, 100.0, 100.0),
        bar(2, 80.0, 80.0),
    ];
    let mut short_signals = vec![StrategySignal::default(); falling.len()];
    short_signals[0] = matching_short_signal();
    let short = evaluate_window(
        &falling,
        &short_signals,
        0,
        falling.len(),
        2,
        1.0,
        2.0,
        true,
        true,
    );
    let expected_short = 20.0 - 2.0 - 1.0 * 0.8;
    assert!((short.mean_net().expect("one short trade") - expected_short).abs() < 1e-10);
}

#[test]
fn short_history_never_emits_a_selected_rule() {
    let bars = (0..100)
        .map(|index| {
            let close = 100.0 + index as f64;
            bar(index as i64 * 86_400_000_000_000, close, close)
        })
        .collect::<Vec<_>>();
    let result = evaluate_nwe_dss_exit_selection(&bars, 0.15, 0.25, true, true, true, false);
    assert_eq!(result["status"], "INSUFFICIENT_HISTORY");
    assert!(result["rule_candidate"].is_null());
}
