use super::confirmations::{
    apply_required_signal_confirmations, dss_confirms_direction, has_recent_directional_extreme,
    nadaraya_confirmation_state,
};
use super::{StrategyBar, StrategySignal};
use crate::indicators::nadaraya_watson_envelope::{
    calculate_nadaraya_watson_endpoint_midline_at, NadarayaWatsonEnvelopeMode,
};
use crate::{calculate_nadaraya_watson_envelope, DssConfig, NadarayaWatsonEnvelopeConfig};

fn bar(index: usize, close: f64) -> StrategyBar {
    StrategyBar {
        time: index as i64,
        open: close,
        high: close + 1.0,
        low: close - 1.0,
        close,
        volume: 1_000.0,
    }
}

fn envelope(upper: &[f64], lower: &[f64]) -> NadarayaWatsonEnvelopeMode {
    NadarayaWatsonEnvelopeMode {
        midline: vec![100.0; upper.len()],
        upper: upper.to_vec(),
        lower: lower.to_vec(),
        crossed_above_upper: vec![false; upper.len()],
        crossed_below_lower: vec![false; upper.len()],
    }
}

#[test]
fn dss_crosses_and_near_approaches_must_match_the_strategy_direction() {
    let trigger = [50.0, 50.0];
    let near_bullish = [45.0, 47.0];
    let far_bullish = [30.0, 40.0];
    let crossed_bearish = [55.0, 48.0];

    assert!(dss_confirms_direction(&near_bullish, &trigger, 1, 1));
    assert!(!dss_confirms_direction(&near_bullish, &trigger, 1, -1));
    assert!(!dss_confirms_direction(&far_bullish, &trigger, 1, 1));
    assert!(dss_confirms_direction(&crossed_bearish, &trigger, 1, -1));
}

#[test]
fn nadaraya_requires_a_directional_outer_band_touch_by_body_or_wick() {
    let endpoint = envelope(&[110.0, 110.0, 110.0], &[90.0, 90.0, 90.0]);
    let bullish_body_touch = StrategyBar {
        open: 89.0,
        high: 98.0,
        low: 88.0,
        close: 95.0,
        ..bar(0, 95.0)
    };
    let bullish_wick_touch = StrategyBar {
        open: 94.0,
        high: 98.0,
        low: 90.0,
        close: 96.0,
        ..bar(1, 96.0)
    };
    let bearish_wick_touch = StrategyBar {
        open: 106.0,
        high: 110.0,
        low: 102.0,
        close: 104.0,
        ..bar(2, 104.0)
    };
    let no_lower_touch = StrategyBar {
        low: 91.0,
        high: 98.0,
        close: 96.0,
        ..bar(0, 96.0)
    };
    let malformed_ohlc = StrategyBar {
        low: 97.0,
        high: 96.0,
        close: 95.0,
        ..bar(0, 95.0)
    };

    assert_eq!(
        nadaraya_confirmation_state(&bullish_body_touch, &endpoint, &[95.0, 96.0, 104.0], 0, 1)
            .map(|confirmation| (confirmation.state, confirmation.touch_points)),
        Some(("LOWER_ENVELOPE_BODY_TOUCH", 3.0))
    );
    assert_eq!(
        nadaraya_confirmation_state(&bullish_wick_touch, &endpoint, &[95.0, 96.0, 104.0], 1, 1)
            .map(|confirmation| (confirmation.state, confirmation.touch_points)),
        Some(("LOWER_ENVELOPE_WICK_TOUCH", 2.0))
    );
    assert_eq!(
        nadaraya_confirmation_state(&bearish_wick_touch, &endpoint, &[95.0, 96.0, 104.0], 2, -1)
            .map(|confirmation| (confirmation.state, confirmation.touch_points)),
        Some(("UPPER_ENVELOPE_WICK_TOUCH", 2.0))
    );
    assert!(nadaraya_confirmation_state(&no_lower_touch, &endpoint, &[95.0], 0, 1).is_none());
    assert!(nadaraya_confirmation_state(&bullish_body_touch, &endpoint, &[95.0], 0, -1).is_none());
    assert!(nadaraya_confirmation_state(&malformed_ohlc, &endpoint, &[95.0], 0, 1).is_none());
}

#[test]
fn endpoint_midline_matches_the_causal_series_at_the_requested_bar() {
    let closes = (0..520)
        .map(|index| 100.0 + index as f64 * 0.01 + (index as f64 * 0.2).sin())
        .collect::<Vec<_>>();
    let config = NadarayaWatsonEnvelopeConfig::default();
    let endpoint = calculate_nadaraya_watson_envelope(&closes, &config).endpoint;
    let index = closes.len() - 1;
    let targeted = calculate_nadaraya_watson_endpoint_midline_at(&closes, index, &config);

    assert!((targeted - endpoint.midline[index]).abs() < 1e-10);
}

#[test]
fn nadaraya_scores_near_line_and_reclaim_after_an_outer_band_touch() {
    let endpoint = envelope(&[110.0, 110.0], &[90.0, 90.0]);
    let near_line = StrategyBar {
        open: 90.0,
        high: 94.0,
        low: 90.0,
        close: 92.0,
        ..bar(1, 92.0)
    };
    let reclaimed = StrategyBar {
        open: 89.0,
        high: 96.0,
        low: 88.0,
        close: 95.0,
        ..bar(1, 95.0)
    };

    let near = nadaraya_confirmation_state(&near_line, &endpoint, &[92.0, 92.0], 1, 1)
        .expect("lower band contact should qualify");
    assert_eq!(near.state, "LOWER_ENVELOPE_BODY_TOUCH");
    assert!(near.near_line);
    assert!(!near.reclaimed);

    let reclaim = nadaraya_confirmation_state(&reclaimed, &endpoint, &[89.0, 95.0], 1, 1)
        .expect("reclaim candle should qualify");
    assert!(reclaim.reclaimed);
    assert!(!reclaim.near_line);
}

#[test]
fn dss_extreme_zone_score_requires_a_recent_extreme_and_a_directional_turn() {
    let config = DssConfig::default();

    assert!(has_recent_directional_extreme(
        &[10.0, 18.0, 22.0, 25.0],
        3,
        1,
        &config
    ));
    assert!(has_recent_directional_extreme(
        &[90.0, 85.0, 78.0, 74.0],
        3,
        -1,
        &config
    ));
    assert!(!has_recent_directional_extreme(
        &[40.0, 45.0, 50.0],
        2,
        1,
        &config
    ));
}

#[test]
fn missing_endpoint_warmup_removes_entries_but_keeps_exit_markers() {
    let bars = (0..499)
        .map(|index| bar(index, 100.0 + (index as f64 * 0.1).sin()))
        .collect::<Vec<_>>();
    let mut signals = vec![StrategySignal::default(); bars.len()];
    signals[498].direction = "LONG".to_string();
    signals[1].exit_signal = Some("SESSION_EXIT".to_string());
    signals[497].direction = "LONG".to_string();
    signals[497].exit_signal = Some("SIMULTANEOUS_EXIT".to_string());

    apply_required_signal_confirmations(&bars, &mut signals);

    assert_eq!(signals[498].direction, "NEUTRAL");
    assert_eq!(signals[1].exit_signal.as_deref(), Some("SESSION_EXIT"));
    assert_eq!(signals[497].direction, "NEUTRAL");
    assert_eq!(
        signals[497].exit_signal.as_deref(),
        Some("SIMULTANEOUS_EXIT")
    );
}

#[test]
fn recent_dss_extreme_turn_and_lower_envelope_touch_keep_a_long_entry() {
    let mut closes = vec![100.0; 490];
    closes.extend([91.0, 82.0, 73.0, 64.0, 55.0, 46.0, 37.0, 28.0, 19.0, 62.0]);
    let endpoint =
        calculate_nadaraya_watson_envelope(&closes, &NadarayaWatsonEnvelopeConfig::default())
            .endpoint;
    let lower = endpoint.lower[closes.len() - 1];
    assert!(
        lower.is_finite() && lower <= 62.0,
        "lower={lower}, midline={}, upper={}",
        endpoint.midline[closes.len() - 1],
        endpoint.upper[closes.len() - 1]
    );
    let mut bars = closes
        .iter()
        .enumerate()
        .map(|(index, close)| StrategyBar {
            time: index as i64,
            open: *close,
            high: *close + 0.1,
            low: *close - 0.1,
            close: *close,
            volume: 1_000.0,
        })
        .collect::<Vec<_>>();
    bars[499].low = lower;
    let mut signals = vec![StrategySignal::default(); bars.len()];
    signals[499].direction = "LONG".to_string();
    signals[499].strength = 8.0;

    apply_required_signal_confirmations(&bars, &mut signals);

    assert_eq!(signals[499].direction, "LONG");
    let confluence = signals[499]
        .confluence
        .as_ref()
        .expect("passing entries retain their indicator score details");
    assert!((0.0..=10.0).contains(&confluence.score));
    assert_eq!(confluence.dss_extreme_zone, true);
    assert!(confluence.dss_extreme_points > 0.0);
    assert!(confluence.nadaraya_touch_points > 0.0);
    assert!(confluence.nadaraya_state.starts_with("LOWER_ENVELOPE_"));
    assert_eq!(signals[499].strategy_strength, 8.0);
    assert!((signals[499].strength - (8.0 * 0.70 + confluence.score * 0.30)).abs() < 1e-10);
}

#[test]
fn complete_history_without_a_directional_extreme_does_not_emit() {
    let bars = (0..500).map(|index| bar(index, 100.0)).collect::<Vec<_>>();
    let mut signals = vec![StrategySignal::default(); bars.len()];
    signals[499].direction = "LONG".to_string();
    signals[499].strength = 8.0;

    apply_required_signal_confirmations(&bars, &mut signals);

    assert_eq!(signals[499].direction, "NEUTRAL");
    assert!(signals[499].confluence.is_none());
}
