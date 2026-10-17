use crate::{
    run_strategy_with_history_limit_and_magnifier_and_tick_size_and_evaluation_start, Candle,
};
use std::collections::HashMap;

fn candle(time: i64, close: f64) -> Candle {
    Candle {
        open: close,
        high: close + 1.0,
        low: close - 1.0,
        close,
        volume: 1.0,
        time: Some(time),
    }
}

#[test]
fn evaluation_start_preserves_indicator_warmup_but_suppresses_pre_split_orders() {
    let source = "//@version=6\nstrategy(\"Chronological tail\", default_qty_type=strategy.fixed, default_qty_value=1)\nplot(ta.sma(close, 3), title=\"Warmup SMA\")\nif bar_index == 0\n    strategy.entry(\"Warmup order\", strategy.long)\nif bar_index == 2\n    strategy.entry(\"Tail order\", strategy.long)\nif bar_index == 3\n    strategy.close_all(immediately=true)";
    let bars = [10.0, 11.0, 12.0, 13.0].map(|close| {
        candle(
            1_700_000_000_000_000_000 + close as i64 * 60_000_000_000,
            close,
        )
    });
    let output = run_strategy_with_history_limit_and_magnifier_and_tick_size_and_evaluation_start(
        source,
        &bars,
        "1h",
        &HashMap::new(),
        &[],
        None,
        bars.len(),
        0.0,
        0.0,
        &HashMap::new(),
        None,
        2,
    )
    .expect("a valid chronological evaluation start should run");

    assert_eq!(output.equity_curve[0], output.initial_capital);
    assert_eq!(output.equity_curve[1], output.initial_capital);
    assert_eq!(output.closed_trades, 1);
    assert_eq!(output.trades.len(), 1);
    assert_eq!(output.trades[0].entry_bar, 3);
    assert_eq!(output.trades[0].exit_bar, 3);
    assert_eq!(output.trades[0].entry_id, "Tail order");
    assert_eq!(output.plots[0].values[0], None);
    assert_eq!(output.plots[0].values[1], None);
    assert_eq!(output.plots[0].values[2], Some(11.0));
}

#[test]
fn evaluation_start_must_be_inside_the_candle_history() {
    let bars = [candle(1, 10.0), candle(2, 11.0)];
    let error = run_strategy_with_history_limit_and_magnifier_and_tick_size_and_evaluation_start(
        "//@version=6\nstrategy(\"Invalid start\")",
        &bars,
        "1h",
        &HashMap::new(),
        &[],
        None,
        bars.len(),
        0.0,
        0.0,
        &HashMap::new(),
        None,
        bars.len(),
    )
    .expect_err("an evaluation start after the final bar must be rejected");

    assert!(error[0].message.contains("evaluation start bar"));
}
