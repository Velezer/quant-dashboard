use crate::{run_strategy_with_history_limit_and_magnifier_and_tick_size, Candle};
use std::collections::HashMap;

#[test]
fn strategy_exit_uses_the_closer_relative_or_absolute_profit_level() {
    let source = "//@version=6\nstrategy(\"Nearest target\", default_qty_type=strategy.fixed, default_qty_value=1)\nif bar_index == 0\n    strategy.entry(\"Long\", strategy.long)\n    strategy.exit(\"Target\", \"Long\", limit=12.5, profit=4)";
    let bars = vec![
        Candle {
            open: 10.0,
            high: 10.2,
            low: 9.8,
            close: 10.0,
            volume: 1.0,
            time: Some(0),
        },
        Candle {
            open: 10.0,
            high: 13.0,
            low: 9.5,
            close: 12.0,
            volume: 1.0,
            time: Some(1),
        },
    ];
    let output = run_strategy_with_history_limit_and_magnifier_and_tick_size(
        source,
        &bars,
        "1d",
        &HashMap::new(),
        &[],
        None,
        bars.len(),
        0.0,
        0.0,
        &HashMap::new(),
        Some(0.5),
    )
    .expect("Pine v6 exit should use the first reached relative or absolute target");
    assert_eq!(output.closed_trades, 1);
    assert_eq!(output.trades[0].exit_price, 12.0);
}
