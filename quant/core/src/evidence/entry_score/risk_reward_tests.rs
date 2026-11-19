use super::*;

fn policy() -> EvaluationPolicy {
    EvaluationPolicy {
        horizon: 1,
        exit_on_reverse_signal: false,
        weights: [10; 4],
        minimum_score: 0.0,
        buy_fee_pct: 0.0,
        sell_fee_pct: 0.0,
        fees_known: true,
        allow_short: true,
    }
}

fn observation() -> Observation {
    Observation {
        factors: [10.0; 4],
        available: [true; 4],
        markov_score: 0.0,
        markov_available: true,
        autocorrelation_score: 0.0,
        autocorrelation_available: true,
    }
}

fn bars() -> Vec<StrategyBar> {
    (0..3)
        .map(|index| StrategyBar {
            time: index as i64 * RISK_REWARD_INTERVAL_NS,
            open: 100.0,
            high: 101.0,
            low: 99.0,
            close: 100.0,
            volume: 1_000.0,
        })
        .collect()
}

fn rule(target_after_fee_r_multiple: f64) -> BracketRule {
    BracketRule {
        stop_atr_multiple: 1.0,
        target_after_fee_r_multiple,
        maximum_hold_bars: 1,
        minimum_score: 0.0,
    }
}

#[test]
fn bracket_backtest_does_not_open_a_trade_below_the_minimum_after_fee_r_multiple() {
    let candles = bars();
    let observations = [Some(observation()), None, None];
    let mut opened = 0;
    let below_minimum = evaluate_rule_with_observer(
        &candles,
        &observations,
        &[0],
        &[Some(1.0)],
        0,
        candles.len(),
        [10; 4],
        policy(),
        rule(MINIMUM_TARGET_AFTER_FEE_R_MULTIPLE - 0.01),
        |_| opened += 1,
    );
    assert_eq!(below_minimum.trades, 0);
    assert_eq!(opened, 0);
}

#[test]
fn bracket_backtest_allows_a_trade_at_the_minimum_after_fee_r_multiple() {
    let candles = bars();
    let observations = [Some(observation()), None, None];
    let mut opened = 0;
    let at_minimum = evaluate_rule_with_observer(
        &candles,
        &observations,
        &[0],
        &[Some(1.0)],
        0,
        candles.len(),
        [10; 4],
        policy(),
        rule(MINIMUM_TARGET_AFTER_FEE_R_MULTIPLE),
        |_| opened += 1,
    );
    assert_eq!(at_minimum.trades, 1);
    assert_eq!(opened, 1);
}
