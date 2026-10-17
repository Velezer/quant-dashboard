use crate::{run_strategy_with_history_limit_and_magnifier_and_tick_size, Candle};
use std::collections::HashMap;

fn candles(values: &[(f64, f64, f64, f64)]) -> Vec<Candle> {
    values
        .iter()
        .enumerate()
        .map(|(index, (open, high, low, close))| Candle {
            open: *open,
            high: *high,
            low: *low,
            close: *close,
            volume: 1.0,
            time: Some(index as i64),
        })
        .collect()
}

#[test]
fn trailing_stop_waits_for_its_path_position_after_an_earlier_reversal_entry() {
    let source = "//@version=6\nstrategy(\"Trailing and reversal order\", pyramiding=2, default_qty_type=strategy.fixed, default_qty_value=1, process_orders_on_close=true)\nif bar_index == 0\n    strategy.entry(\"Base\", strategy.long)\n    strategy.exit(\"Trail\", \"Base\", trail_price=12, trail_offset=2)\n    strategy.entry(\"Flip\", strategy.short, limit=12.5)";
    let bars = candles(&[(10.0, 10.5, 9.5, 10.0), (13.0, 13.5, 9.0, 10.0)]);
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
    .expect("intrabar entry and trailing-stop events should be replayed in path order");
    assert_eq!(output.closed_trades, 1);
    assert_eq!(output.trades[0].exit_price, 13.0, "{:?}", output.trades);
    assert_eq!(output.open_position_size, -1.0);
}
