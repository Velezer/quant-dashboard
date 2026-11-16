use super::{score_four_indicators_with_weights, StrategyBar};

fn bars() -> Vec<StrategyBar> {
    (0..21).map(|i| StrategyBar { time: i, open: 100.0, high: 101.0,
        low: 99.0, close: 100.0, volume: 100.0 }).collect()
}

#[test]
fn opposing_indicators_cancel_and_weights_can_reverse_direction() {
    let bars = bars();
    let score = |weights| score_four_indicators_with_weights(
        &bars, "LONG", "SIGNED", 8.0, "SIGNED", -8.0, weights).unwrap();
    assert_eq!(score([10,10,10,10]).total_score, 0.0);
    assert_eq!(score([20,0,10,10]).total_score, 16.0);
    assert_eq!(score([0,20,10,10]).total_score, -16.0);
}

#[test]
fn bearish_volume_and_delta_subtract_even_from_bullish_price_inputs() {
    let mut bars = bars();
    let last = bars.last_mut().unwrap();
    last.open = 101.0;
    last.close = 99.0;
    last.volume = 200.0;
    let score = score_four_indicators_with_weights(
        &bars, "LONG", "SIGNED", 8.0, "SIGNED", 6.0, [10;4]).unwrap();
    assert_eq!(score.components[2].points, -10.0);
    assert_eq!(score.components[3].points, -10.0);
    assert_eq!(score.total_score, -6.0);
}
