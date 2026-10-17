use crate::{run_strategy, Candle};

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
fn market_fill_recalculation_can_submit_a_same_bar_next_tick_close() {
    let source = "//@version=6\nstrategy(\"Fill recalculation\", default_qty_type=strategy.fixed, default_qty_value=1, calc_on_order_fills=true)\nif bar_index == 0\n    strategy.entry(\"Long\", strategy.long)\nif strategy.position_size > 0\n    strategy.close_all()";
    let output = run_strategy(
        source,
        &candles(&[(10.0, 10.0, 10.0, 10.0), (10.0, 12.0, 8.0, 11.0)]),
        0.0,
        0.0,
    )
    .expect("market-only fill recalculation should run");

    assert_eq!(output.closed_trades, 1);
    assert_eq!(output.trades[0].entry_bar, 1);
    assert_eq!(output.trades[0].exit_bar, 1);
    assert_eq!(output.trades[0].entry_price, 10.0);
    assert_eq!(output.trades[0].exit_price, 8.0);
    assert!(output
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("final OHLC values")));
}

#[test]
fn immediate_close_after_fill_uses_the_current_ohlc_point() {
    let source = "//@version=6\nstrategy(\"Immediate fill recalculation\", default_qty_type=strategy.fixed, default_qty_value=1, calc_on_order_fills=true)\nif bar_index == 0\n    strategy.entry(\"Long\", strategy.long)\nif strategy.position_size > 0\n    strategy.close_all(immediately=true)";
    let output = run_strategy(
        source,
        &candles(&[(10.0, 10.0, 10.0, 10.0), (10.0, 12.0, 8.0, 11.0)]),
        0.0,
        0.0,
    )
    .expect("immediate market close should fill on the current tick");

    assert_eq!(output.closed_trades, 1);
    assert_eq!(output.trades[0].entry_bar, 1);
    assert_eq!(output.trades[0].exit_bar, 1);
    assert_eq!(output.trades[0].entry_price, 10.0);
    assert_eq!(output.trades[0].exit_price, 10.0);
}

#[test]
fn fill_recalculation_rolls_back_var_and_preserves_varip() {
    let source = "//@version=6\nstrategy(\"Rollback counters\", default_qty_type=strategy.fixed, default_qty_value=1, calc_on_order_fills=true)\nvar int committedExecutions = 0\nvarip int intrabarExecutions = 0\ncommittedExecutions += 1\nintrabarExecutions += 1\nif bar_index == 0\n    strategy.entry(\"Long\", strategy.long)\nif strategy.position_size > 0\n    strategy.close_all()\nplot(committedExecutions, title=\"var executions\")\nplot(intrabarExecutions, title=\"varip executions\")";
    let output = run_strategy(
        source,
        &candles(&[(10.0, 10.0, 10.0, 10.0), (10.0, 12.0, 8.0, 11.0)]),
        0.0,
        0.0,
    )
    .expect("historical rollback should preserve varip values only");

    assert_eq!(output.plots[0].values[1], Some(2.0));
    assert_eq!(output.plots[1].values[1], Some(4.0));
}

#[test]
fn fill_recalculation_rejects_unsupported_price_orders_and_modes() {
    let bars = candles(&[(10.0, 12.0, 8.0, 11.0)]);
    let sources = [
        "//@version=6\nstrategy(\"Stop order\", calc_on_order_fills=true)\nstrategy.entry(\"Long\", strategy.long, stop=11)",
        "//@version=6\nstrategy(\"Limit order\", calc_on_order_fills=true)\nstrategy.order(\"Long\", strategy.long, limit=9)",
        "//@version=6\nstrategy(\"Bracket exit\", calc_on_order_fills=true)\nstrategy.exit(\"Exit\", \"Long\", stop=9)",
        "//@version=6\nstrategy(\"Close processing\", calc_on_order_fills=true, process_orders_on_close=true)",
        "//@version=6\nstrategy(\"Magnifier\", calc_on_order_fills=true, use_bar_magnifier=true)",
    ];

    for source in sources {
        let error = run_strategy(source, &bars, 0.0, 0.0)
            .expect_err("unsupported fill model must be rejected");
        assert!(
            !error.is_empty(),
            "source should have a diagnostic: {source}"
        );
    }
}

#[test]
fn realtime_tick_setting_is_reported_but_does_not_change_historical_ohlc_replay() {
    let source = "//@version=6\nstrategy(\"Realtime tick declaration\", calc_on_every_tick=true)";
    let output = run_strategy(source, &candles(&[(10.0, 12.0, 8.0, 11.0)]), 0.0, 0.0)
        .expect("realtime-only tick setting should be accepted for historical reporting");
    assert!(output
        .diagnostics
        .iter()
        .any(|item| item.message.contains("realtime-bar setting")
            && item.message.contains("does not change candle replay")));

    let historical_ticks =
        "//@version=6\nstrategy(\"Unavailable historical ticks\", calc_on_every_history_tick=true)";
    let error = run_strategy(
        historical_ticks,
        &candles(&[(10.0, 12.0, 8.0, 11.0)]),
        0.0,
        0.0,
    )
    .expect_err("historical tick mode cannot be simulated from candle OHLC");
    assert!(error
        .iter()
        .any(|item| item.message.contains("requires historical tick data")));
}
