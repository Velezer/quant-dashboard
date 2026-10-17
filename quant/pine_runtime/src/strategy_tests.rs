use crate::{
    run_indicator, run_strategy, run_strategy_with_history_limit_and_magnifier,
    run_strategy_with_history_limit_and_magnifier_and_tick_size, run_strategy_with_inputs,
    strategy_uses_bar_magnifier, validate_source, Candle,
};
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

fn run_strategy_with_tick_size(
    source: &str,
    bars: &[Candle],
    tick_size: Option<f64>,
) -> Result<crate::StrategyOutput, Vec<crate::Diagnostic>> {
    run_strategy_with_history_limit_and_magnifier_and_tick_size(
        source,
        bars,
        "1d",
        &HashMap::new(),
        &[],
        None,
        bars.len().max(1),
        0.0,
        0.0,
        &HashMap::new(),
        tick_size,
    )
}

#[test]
fn validates_and_runs_a_pine_strategy_with_next_open_market_fills() {
    let source = "//@version=6\nstrategy(\"Next open\", default_qty_type=strategy.fixed, default_qty_value=1)\nbool enter = bar_index == 0\nif enter\n    strategy.entry(\"Long\", strategy.long)\nbool leave = strategy.position_size > 0 and bar_index - strategy.opentrades.entry_bar_index(0) >= 1\nif leave\n    strategy.close(\"Long\", immediately=true)\nplot(strategy.position_size, title=\"Position\")";
    let validation = validate_source(source);
    assert!(validation.valid, "{:?}", validation.diagnostics);
    assert_eq!(validation.name, "Next open");
    let output = run_strategy(
        source,
        &candles(&[
            (10.0, 10.5, 9.5, 10.0),
            (11.0, 12.0, 10.5, 12.0),
            (13.0, 14.0, 12.0, 14.0),
        ]),
        0.1,
        0.2,
    )
    .expect("bounded market strategy should execute");
    assert_eq!(output.closed_trades, 1);
    assert_eq!(output.trades[0].entry_bar, 1);
    assert_eq!(output.trades[0].exit_bar, 2);
    assert_eq!(output.trades[0].entry_price, 11.0);
    assert_eq!(output.trades[0].exit_price, 14.0);
    assert!((output.trades[0].net_pnl - 2.961).abs() < 1e-9);
}

#[test]
fn closed_trade_accessors_match_recorded_long_trade_fields() {
    let source = "//@version=6\nstrategy(\"Closed trade accessors\", default_qty_type=strategy.fixed, default_qty_value=2)\nif bar_index == 0\n    strategy.entry(\"Long\", strategy.long)\nif bar_index == 1\n    strategy.close_all(immediately=true)\nbool fields_match = strategy.closedtrades > 0 and strategy.closedtrades.entry_id(0) == \"Long\" and strategy.closedtrades.entry_bar_index(0) == 1 and strategy.closedtrades.exit_bar_index(0) == 1 and strategy.closedtrades.entry_price(0) == 11 and strategy.closedtrades.exit_price(0) == 12 and strategy.closedtrades.size(0) == 2 and strategy.closedtrades.profit(0) == 2 and strategy.closedtrades.commission(0) == 0\nplot(fields_match ? 1 : 0)";
    let output = run_strategy(
        source,
        &candles(&[
            (10.0, 10.5, 9.5, 10.0),
            (11.0, 12.0, 10.5, 12.0),
            (12.0, 12.5, 11.5, 12.0),
        ]),
        0.0,
        0.0,
    )
    .expect("closed trade fields should be available to Pine scripts");
    assert_eq!(output.closed_trades, 1);
    let matched = output.plots[0].values[2].expect("third bar should evaluate closed trade fields");
    assert_eq!(matched, 1.0);
    assert!((output.trades[0].net_return_pct - (2.0 / 22.0 * 100.0)).abs() < 1e-10);
}

#[test]
fn closed_trade_retains_runup_and_drawdown_excursions() {
    let source = "//@version=6\nstrategy(\"Closed trade excursions\", default_qty_type=strategy.fixed, default_qty_value=1)\nif bar_index == 0\n    strategy.entry(\"Long\", strategy.long)\nif bar_index == 2\n    strategy.close_all(immediately=true)\nbool excursions_match = strategy.closedtrades.max_runup(0) == 3 and strategy.closedtrades.max_drawdown(0) == 2 and strategy.closedtrades.max_runup_percent(0) == 30 and strategy.closedtrades.max_drawdown_percent(0) == 20\nplot(excursions_match ? 1 : 0)";
    let output = run_strategy(
        source,
        &candles(&[
            (10.0, 10.0, 10.0, 10.0),
            (10.0, 13.0, 8.0, 12.0),
            (12.0, 12.5, 11.5, 12.0),
            (12.0, 12.5, 11.5, 12.0),
        ]),
        0.0,
        0.0,
    )
    .expect("closed trades should retain path-based excursion values");
    assert_eq!(output.plots[0].values[3], Some(1.0));
    assert_eq!(output.trades[0].max_runup, 3.0);
    assert_eq!(output.trades[0].max_drawdown, 2.0);
    assert_eq!(output.trades[0].max_runup_percent, 30.0);
    assert_eq!(output.trades[0].max_drawdown_percent, 20.0);
}

#[test]
fn pine_time_and_closed_trade_times_use_milliseconds() {
    let source = "//@version=6\nstrategy(\"Millisecond timestamps\", default_qty_type=strategy.fixed, default_qty_value=1)\nif bar_index == 0\n    strategy.entry(\"Long\", strategy.long)\nif bar_index == 1\n    strategy.close_all(immediately=true)\nbool trade_times_match = strategy.closedtrades.entry_time(0) == 1700000001000 and strategy.closedtrades.exit_time(0) == 1700000001000\nplot(time, title=\"Pine time\")\nplot(trade_times_match ? 1 : 0, title=\"Closed trade time check\")";
    let mut bars = candles(&[
        (10.0, 10.5, 9.5, 10.0),
        (11.0, 12.0, 10.5, 12.0),
        (12.0, 12.5, 11.5, 12.0),
    ]);
    let timestamp_ns = 1_700_000_000_000_000_000_i64;
    for (index, bar) in bars.iter_mut().enumerate() {
        bar.time = Some(timestamp_ns + index as i64 * 1_000_000_000);
    }
    let output = run_strategy(source, &bars, 0.0, 0.0).expect(
        "Pine time values should be milliseconds even when candle storage uses nanoseconds",
    );
    assert_eq!(output.plots[0].values[2], Some(1_700_000_002_000.0));
    assert_eq!(output.plots[1].values[2], Some(1.0));
}

#[test]
fn open_trade_accessors_report_id_entry_price_time_and_signed_remaining_size() {
    let source = "//@version=6\nstrategy(\"Open trade accessors\", default_qty_type=strategy.fixed, default_qty_value=3, commission_type=strategy.commission.percent, commission_value=1)\nif bar_index == 0\n    strategy.entry(\"Short\", strategy.short)\nbool fields_match = strategy.opentrades > 0 and strategy.opentrades.entry_id(0) == \"Short\" and strategy.opentrades.entry_bar_index(0) == 1 and strategy.opentrades.entry_price(0) == 11 and strategy.opentrades.entry_time(0) == 1700000001000 and strategy.opentrades.size(0) == -3 and math.abs(strategy.opentrades.commission(0) - 0.33) < 0.000001 and math.abs(strategy.opentrades.profit(0) + 3.33) < 0.000001 and math.abs(strategy.opentrades.profit_percent(0) - (-3.33 / 33 * 100)) < 0.000001\nplot(fields_match ? 1 : 0, title=\"Open fields\")\nplot(strategy.opentrades.size(0), title=\"Signed open size\")\nif bar_index == 1\n    strategy.close_all(immediately=true)";
    let mut bars = candles(&[
        (10.0, 10.5, 9.5, 10.0),
        (11.0, 12.0, 10.5, 12.0),
        (12.0, 12.5, 11.5, 12.0),
    ]);
    let timestamp_ns = 1_700_000_000_000_000_000_i64;
    for (index, bar) in bars.iter_mut().enumerate() {
        bar.time = Some(timestamp_ns + index as i64 * 1_000_000_000);
    }
    let output = run_strategy(source, &bars, 0.0, 0.0)
        .expect("open trade accessors should expose the currently open broker position");
    assert_eq!(output.plots[0].values[1], Some(1.0));
    assert_eq!(output.plots[1].values[1], Some(-3.0));
}

#[test]
fn strategy_summary_variables_match_realized_and_open_equity_components() {
    let source = "//@version=6\nstrategy(\"Summary variables\", initial_capital=200, default_qty_type=strategy.fixed, default_qty_value=2, commission_type=strategy.commission.percent, commission_value=1)\nif bar_index == 0\n    strategy.entry(\"Long\", strategy.long)\nbool open_fields_match = strategy.initial_capital == 200 and strategy.position_entry_name == \"Long\" and math.abs(strategy.openprofit - 1.78) < 0.000001 and math.abs(strategy.openprofit_percent - 0.89) < 0.000001\nplot(open_fields_match ? 1 : 0, title=\"Open summary\")\nif bar_index == 2\n    strategy.close_all(immediately=true)\nbool closed_fields_match = math.abs(strategy.grossprofit_percent - 2) < 0.000001 and math.abs(strategy.grossloss_percent) < 0.000001 and math.abs(strategy.netprofit_percent - 1.76) < 0.000001 and strategy.openprofit == 0\nplot(closed_fields_match ? 1 : 0, title=\"Closed summary\")";
    let output = run_strategy(
        source,
        &candles(&[
            (10.0, 10.5, 9.5, 10.0),
            (11.0, 12.0, 10.5, 12.0),
            (12.0, 13.0, 11.5, 13.0),
            (13.0, 13.5, 12.5, 13.0),
        ]),
        0.0,
        0.0,
    )
    .expect("strategy summary variables should expose open and realized account values");
    assert_eq!(output.plots[0].values[1], Some(1.0));
    assert_eq!(output.plots[1].values[3], Some(1.0));
}

#[test]
fn strategy_max_runup_and_drawdown_use_intrabar_extremes_and_closed_equity_baselines() {
    let source = "//@version=6\nstrategy(\"Aggregate excursions\", initial_capital=100, default_qty_type=strategy.fixed, default_qty_value=1)\nif bar_index == 0\n    strategy.entry(\"Winner\", strategy.long)\nif bar_index == 1\n    strategy.close_all(immediately=true)\nif bar_index == 2\n    strategy.entry(\"Loser\", strategy.long)\nif bar_index == 3\n    strategy.close_all(immediately=true)\nbool metrics_match = strategy.max_runup == 2 and math.abs(strategy.max_runup_percent - 2) < 0.000001 and strategy.max_drawdown == 6 and math.abs(strategy.max_drawdown_percent - (6.0 / 101.0 * 100)) < 0.000001\nplot(metrics_match ? 1 : 0)";
    let output = run_strategy(
        source,
        &candles(&[
            (10.0, 10.0, 10.0, 10.0),
            (10.0, 12.0, 9.0, 11.0),
            (11.0, 11.0, 11.0, 11.0),
            (11.0, 11.0, 5.0, 6.0),
            (6.0, 6.5, 5.5, 6.0),
        ]),
        0.0,
        0.0,
    )
    .expect(
        "aggregate strategy excursion variables should use trade extrema and prior closed equity",
    );
    assert_eq!(output.plots[0].values[4], Some(1.0));
    assert_eq!(output.max_drawdown, 6.0);
    assert!((output.max_drawdown_percent - 6.0 / 101.0 * 100.0).abs() < 1e-9);
    assert_eq!(output.max_drawdown_initial_capital_percent, 6.0);
}

#[test]
fn reported_max_drawdown_preserves_intrabar_loss_that_recovers_by_close() {
    let source = "//@version=6\nstrategy(\"Intrabar drawdown report\", initial_capital=100, default_qty_type=strategy.fixed, default_qty_value=1)\nif bar_index == 0\n    strategy.entry(\"Long\", strategy.long)";
    let output = run_strategy(
        source,
        &candles(&[(10.0, 10.0, 10.0, 10.0), (10.0, 13.0, 8.0, 12.5)]),
        0.0,
        0.0,
    )
    .expect("intrabar drawdown metrics should be included in strategy output");
    assert_eq!(output.equity_curve, vec![100.0, 102.5]);
    assert_eq!(output.max_drawdown, 2.0);
    assert_eq!(output.max_drawdown_percent, 2.0);
    assert_eq!(output.max_drawdown_initial_capital_percent, 2.0);
}

#[test]
fn open_trade_excursions_follow_inferred_ohlc_path_and_intrabar_fill_time() {
    let source = "//@version=6\nstrategy(\"Open trade excursions\", default_qty_type=strategy.fixed, default_qty_value=1)\nif bar_index == 0\n    strategy.entry(\"Long\", strategy.long)\nbool excursions_match = strategy.opentrades.max_runup(0) == 3 and strategy.opentrades.max_drawdown(0) == 2 and strategy.opentrades.max_runup_percent(0) == 30 and strategy.opentrades.max_drawdown_percent(0) == 20\nplot(excursions_match ? 1 : 0)\nif bar_index == 2\n    strategy.close_all(immediately=true)";
    let output = run_strategy(
        source,
        &candles(&[
            (10.0, 10.0, 10.0, 10.0),
            (10.0, 13.0, 8.0, 12.0),
            (12.0, 12.5, 11.5, 12.0),
            (12.0, 12.5, 11.5, 12.0),
        ]),
        0.0,
        0.0,
    )
    .expect("open trade excursions should use the broker's inferred OHLC path");
    assert_eq!(output.plots[0].values[2], Some(1.0));
}

#[test]
fn intrabar_entry_excursions_exclude_prices_before_the_entry_fill() {
    let source = "//@version=6\nstrategy(\"Entry excursion boundary\", default_qty_type=strategy.fixed, default_qty_value=1)\nif bar_index == 0\n    strategy.entry(\"Long\", strategy.long, limit=9)\nbool excursions_match = strategy.opentrades.max_runup(0) == 3 and strategy.opentrades.max_drawdown(0) == 1\nplot(excursions_match ? 1 : 0)\nif bar_index == 2\n    strategy.close_all(immediately=true)";
    let output = run_strategy(
        source,
        &candles(&[
            (10.0, 11.0, 9.0, 10.0),
            (10.0, 12.0, 8.0, 11.0),
            (11.0, 11.5, 10.5, 11.0),
        ]),
        0.0,
        0.0,
    )
    .expect("intrabar fills should begin excursion tracking at the fill distance");
    assert_eq!(output.plots[0].values[1], Some(1.0));
}

#[test]
fn downward_first_path_entry_excludes_pre_fill_high_from_runup() {
    let source = "//@version=6\nstrategy(\"Downward path entry boundary\", default_qty_type=strategy.fixed, default_qty_value=1)\nif bar_index == 0\n    strategy.entry(\"Long\", strategy.long, limit=9)\nbool excursions_match = strategy.opentrades.max_runup(0) == 0.5 and strategy.opentrades.max_drawdown(0) == 1\nplot(strategy.opentrades.max_runup(0), title=\"Runup\")\nplot(strategy.opentrades.max_drawdown(0), title=\"Drawdown\")\nplot(excursions_match ? 1 : 0)\nif bar_index == 2\n    strategy.close_all(immediately=true)";
    let output = run_strategy(
        source,
        &candles(&[
            (10.0, 11.0, 9.0, 10.0),
            (11.0, 12.0, 8.0, 9.0),
            (9.0, 9.5, 8.5, 9.0),
            (9.0, 9.5, 8.5, 9.0),
        ]),
        0.0,
        0.0,
    )
    .expect("downward-first inferred paths should track only prices after the entry");
    assert_eq!(output.plots[0].values[2], Some(0.5));
    assert_eq!(output.plots[1].values[2], Some(1.0));
    assert_eq!(output.plots[2].values[2], Some(1.0));
}

#[test]
fn bar_magnifier_resolves_target_before_later_stop_within_chart_bar() {
    let source = "//@version=6\nstrategy(\"Magnifier sequence\", default_qty_type=strategy.fixed, default_qty_value=1, use_bar_magnifier=true)\nif bar_index == 0\n    strategy.entry(\"Long\", strategy.long)\nif bar_index == 1\n    strategy.exit(\"Bracket\", \"Long\", stop=9, limit=12)\nplot(strategy.closedtrades.exit_price(0), title=\"Exit\")";
    assert!(
        strategy_uses_bar_magnifier(source).expect("magnifier declaration should be recognized")
    );
    assert!(
        !strategy_uses_bar_magnifier("//@version=6\nstrategy(\"Default chart fills\")")
            .expect("default magnifier setting should be false")
    );
    let hour_ns = 3_600_000_000_000_i64;
    let minute_15_ns = 900_000_000_000_i64;
    let chart_bars = [
        (10.0, 10.0, 10.0, 10.0),
        (10.0, 10.0, 10.0, 10.0),
        (10.0, 13.0, 8.0, 12.0),
    ]
    .iter()
    .enumerate()
    .map(|(index, (open, high, low, close))| Candle {
        open: *open,
        high: *high,
        low: *low,
        close: *close,
        volume: 1.0,
        time: Some(index as i64 * hour_ns),
    })
    .collect::<Vec<_>>();
    let mut magnifier_bars = Vec::new();
    for chart_index in 0..3 {
        let start = chart_index * hour_ns;
        if chart_index == 2 {
            magnifier_bars.push(Candle {
                open: 10.0,
                high: 13.0,
                low: 10.0,
                close: 12.0,
                volume: 1.0,
                time: Some(start),
            });
            magnifier_bars.push(Candle {
                open: 12.0,
                high: 12.0,
                low: 8.0,
                close: 12.0,
                volume: 1.0,
                time: Some(start + 3 * minute_15_ns),
            });
        } else {
            magnifier_bars.push(Candle {
                open: 10.0,
                high: 10.0,
                low: 10.0,
                close: 10.0,
                volume: 1.0,
                time: Some(start),
            });
            magnifier_bars.push(Candle {
                open: 10.0,
                high: 10.0,
                low: 10.0,
                close: 10.0,
                volume: 1.0,
                time: Some(start + 3 * minute_15_ns),
            });
        }
    }
    let no_context = HashMap::new();
    let no_inputs = HashMap::new();
    let magnified = run_strategy_with_history_limit_and_magnifier(
        source,
        &chart_bars,
        "1h",
        &no_context,
        &magnifier_bars,
        Some("15m"),
        3,
        0.0,
        0.0,
        &no_inputs,
    )
    .expect("complete matching lower-timeframe bars should run through Bar Magnifier");
    assert_eq!(magnified.trades[0].exit_price, 12.0);
    assert!(magnified.diagnostics.iter().any(|item| item.message.contains("Bar Magnifier used lower-timeframe OHLC that matches chart-bar aggregate OHLC on 3 chart bars")));
    assert!(magnified.bar_magnifier_enabled);
    assert_eq!(magnified.bar_magnifier_covered_chart_bars, 3);
    assert_eq!(magnified.bar_magnifier_fallback_chart_bars, 0);
    let chart_only = run_strategy_with_history_limit_and_magnifier(
        source,
        &chart_bars,
        "1h",
        &no_context,
        &[],
        None,
        3,
        0.0,
        0.0,
        &no_inputs,
    )
    .expect("chart-only fallback should remain available when no lower-timeframe data is supplied");
    assert_eq!(chart_only.trades[0].exit_price, 9.0);
    assert!(chart_only.diagnostics.iter().any(|item| item.message.contains("Bar Magnifier is enabled but no aggregate-matching lower-timeframe candles were available")));
    let incomplete_magnifier = magnifier_bars
        .iter()
        .filter(|bar| bar.time.unwrap() < 2 * hour_ns)
        .cloned()
        .chain(std::iter::once(Candle {
            open: 10.0,
            high: 13.0,
            low: 10.0,
            close: 12.0,
            volume: 1.0,
            time: Some(2 * hour_ns),
        }))
        .collect::<Vec<_>>();
    let mismatch_fallback = run_strategy_with_history_limit_and_magnifier(
        source,
        &chart_bars,
        "1h",
        &no_context,
        &incomplete_magnifier,
        Some("15m"),
        3,
        0.0,
        0.0,
        &no_inputs,
    )
    .expect("aggregate-mismatched lower-timeframe bars should fall back to chart OHLC");
    assert_eq!(mismatch_fallback.trades[0].exit_price, 9.0);
    let disabled_source = source.replace("use_bar_magnifier=true", "use_bar_magnifier=false");
    let disabled = run_strategy_with_history_limit_and_magnifier(
        &disabled_source,
        &chart_bars,
        "1h",
        &no_context,
        &magnifier_bars,
        Some("15m"),
        3,
        0.0,
        0.0,
        &no_inputs,
    )
    .expect(
        "lower-timeframe candles should be ignored when the declaration disables Bar Magnifier",
    );
    assert_eq!(disabled.trades[0].exit_price, 9.0);
    assert!(disabled.diagnostics.iter().any(|item| item
        .message
        .contains("Bar Magnifier is disabled by the strategy declaration")));
}

#[test]
fn closed_trade_size_is_negative_for_short_trades() {
    let source = "//@version=6\nstrategy(\"Closed short\", default_qty_type=strategy.fixed, default_qty_value=3)\nif bar_index == 0\n    strategy.entry(\"Short\", strategy.short)\nif bar_index == 1\n    strategy.close_all(immediately=true)\nbool fields_match = strategy.closedtrades.size(0) == -3\nplot(fields_match ? 1 : 0)";
    let output = run_strategy(
        source,
        &candles(&[
            (10.0, 10.5, 9.5, 10.0),
            (9.0, 9.5, 8.0, 8.5),
            (8.0, 8.5, 7.5, 8.0),
        ]),
        0.0,
        0.0,
    )
    .expect("short direction and signed size should match Pine semantics");
    let matched = output.plots[0].values[2].expect("third bar should evaluate closed short fields");
    assert_eq!(matched, 1.0);
    assert_eq!(output.trades[0].quantity, 3.0);
}

#[test]
fn rejects_undocumented_closed_trade_direction_accessor() {
    let source = "//@version=6\nstrategy(\"Unknown trade accessor\")\nplot(strategy.closedtrades.direction(0))";
    let validation = validate_source(source);
    assert!(!validation.valid);
    assert!(validation
        .diagnostics
        .iter()
        .any(|item| item.message.contains("strategy.closedtrades.direction")));
}

#[test]
fn strategy_close_uses_fifo_by_default_even_when_it_names_a_newer_entry() {
    let source = "//@version=6\nstrategy(\"FIFO close\", pyramiding=2, default_qty_type=strategy.fixed, default_qty_value=1)\nif bar_index == 0\n    strategy.entry(\"A\", strategy.long, qty=5)\n    strategy.entry(\"B\", strategy.long, qty=10)\nif bar_index == 1\n    strategy.close(\"B\")\nif bar_index == 2\n    strategy.close_all(immediately=true)";
    let output = run_strategy(
        source,
        &candles(&[
            (10.0, 10.5, 9.5, 10.0),
            (11.0, 11.5, 10.5, 11.0),
            (12.0, 13.0, 11.5, 13.0),
        ]),
        0.0,
        0.0,
    )
    .expect("default close_entries_rule should use FIFO");
    assert_eq!(output.trades[0].entry_id, "A");
    assert_eq!(output.trades[0].quantity, 5.0);
    assert_eq!(output.trades[0].exit_bar, 2);
    assert_eq!(output.trades[1].entry_id, "B");
    assert_eq!(output.trades[1].quantity, 10.0);
    assert_eq!(output.trades[1].exit_price, 12.5);
}

#[test]
fn strategy_close_entries_rule_any_closes_the_named_entry_first() {
    let source = "//@version=6\nstrategy(\"Any close\", pyramiding=2, close_entries_rule=\"ANY\", default_qty_type=strategy.fixed, default_qty_value=1)\nif bar_index == 0\n    strategy.entry(\"A\", strategy.long, qty=5)\n    strategy.entry(\"B\", strategy.long, qty=10)\nif bar_index == 1\n    strategy.close(\"B\")\nif bar_index == 2\n    strategy.close_all(immediately=true)";
    let output = run_strategy(
        source,
        &candles(&[
            (10.0, 10.5, 9.5, 10.0),
            (11.0, 11.5, 10.5, 11.0),
            (12.0, 13.0, 11.5, 13.0),
        ]),
        0.0,
        0.0,
    )
    .expect("close_entries_rule ANY should close the requested entry");
    assert_eq!(output.trades[0].entry_id, "B");
    assert_eq!(output.trades[0].quantity, 10.0);
    assert_eq!(output.trades[0].exit_price, 12.0);
    assert_eq!(output.trades[1].entry_id, "A");
    assert_eq!(output.trades[1].quantity, 5.0);
}

#[test]
fn strategy_close_qty_partially_closes_matching_entry() {
    let source = "//@version=6\nstrategy(\"Close quantity\", default_qty_type=strategy.fixed, default_qty_value=4)\nif bar_index == 0\n    strategy.entry(\"Long\", strategy.long)\nif bar_index == 1\n    strategy.close(\"Long\", qty=1, qty_percent=75)\nif bar_index == 2\n    strategy.close_all(immediately=true)";
    let output = run_strategy(
        source,
        &candles(&[
            (10.0, 10.5, 9.5, 10.0),
            (11.0, 11.5, 10.5, 11.0),
            (12.0, 13.0, 11.5, 13.0),
        ]),
        0.0,
        0.0,
    )
    .expect("strategy.close qty should close only the requested amount");
    assert_eq!(output.trades.len(), 1);
    assert_eq!(output.trades[0].quantity, 4.0);
    assert_eq!(output.trades[0].exit_price, 12.75);
    assert_eq!(output.trades[0].gross_pnl, 7.0);
}

#[test]
fn strategy_close_qty_percent_partially_closes_matching_entry() {
    let source = "//@version=6\nstrategy(\"Close percentage\", default_qty_type=strategy.fixed, default_qty_value=4)\nif bar_index == 0\n    strategy.entry(\"Long\", strategy.long)\nif bar_index == 1\n    strategy.close(\"Long\", qty_percent=50)\nif bar_index == 2\n    strategy.close_all(immediately=true)";
    let output = run_strategy(
        source,
        &candles(&[
            (10.0, 10.5, 9.5, 10.0),
            (11.0, 11.5, 10.5, 11.0),
            (12.0, 13.0, 11.5, 13.0),
        ]),
        0.0,
        0.0,
    )
    .expect("strategy.close qty_percent should close the requested fraction");
    assert_eq!(output.trades.len(), 1);
    assert_eq!(output.trades[0].quantity, 4.0);
    assert_eq!(output.trades[0].exit_price, 12.5);
    assert_eq!(output.trades[0].gross_pnl, 6.0);
}

#[test]
fn limit_entry_fills_at_a_better_open_or_the_limit_on_intrabar_touch() {
    let source = "//@version=6\nstrategy(\"Limit entries\", default_qty_type=strategy.fixed, default_qty_value=1)\nif bar_index == 0\n    strategy.entry(\"Long\", strategy.long, limit=9)\nif bar_index == 1\n    strategy.close(\"Long\", immediately=true)";
    let touched = run_strategy(
        source,
        &candles(&[(10.0, 10.5, 9.5, 10.0), (10.0, 11.0, 8.0, 9.5)]),
        0.0,
        0.0,
    )
    .expect("limit entry should fill on an intrabar touch");
    assert_eq!(touched.trades[0].entry_price, 9.0);
    assert_eq!(touched.trades[0].entry_bar, 1);

    let marketable = run_strategy(
        source.replace("limit=9", "limit=11").as_str(),
        &candles(&[(10.0, 10.5, 9.5, 10.0), (10.5, 11.0, 10.0, 10.5)]),
        0.0,
        0.0,
    )
    .expect("marketable limit entry should fill at the next open");
    assert_eq!(marketable.trades[0].entry_price, 10.5);
}

#[test]
fn stop_limit_entry_waits_for_activation_then_limit_retrace() {
    let source = "//@version=6\nstrategy(\"Stop limit\", default_qty_type=strategy.fixed, default_qty_value=1)\nif bar_index == 0\n    strategy.entry(\"Long\", strategy.long, stop=13, limit=11)\nif bar_index == 2\n    strategy.close(\"Long\", immediately=true)";
    let bars = candles(&[
        (10.0, 10.5, 9.5, 10.0),
        (10.0, 14.0, 9.0, 13.0),
        (13.0, 13.5, 10.0, 10.5),
    ]);
    let output = run_strategy(source, &bars, 0.0, 0.0)
        .expect("stop-limit entry should activate then fill at its limit");
    assert_eq!(output.trades[0].entry_bar, 2);
    assert_eq!(output.trades[0].entry_price, 11.0);
}

#[test]
fn stop_limit_entry_fills_at_the_activation_price_when_limit_is_already_marketable() {
    let long_source = "//@version=6\nstrategy(\"Marketable long stop-limit\", default_qty_type=strategy.fixed, default_qty_value=1)\nif bar_index == 0\n    strategy.entry(\"Long\", strategy.long, stop=12, limit=13)\nif bar_index == 2\n    strategy.close_all(immediately=true)";
    let long = run_strategy(
        long_source,
        &candles(&[
            (10.0, 10.5, 9.5, 10.0),
            (10.0, 14.0, 8.0, 13.0),
            (13.0, 13.5, 12.5, 13.0),
        ]),
        0.0,
        0.0,
    )
    .expect("marketable long limit should fill when its stop activates");
    assert_eq!(long.closed_trades, 1);
    assert_eq!(long.trades[0].entry_price, 12.0);

    let short_source = "//@version=6\nstrategy(\"Marketable short stop-limit\", default_qty_type=strategy.fixed, default_qty_value=1)\nif bar_index == 0\n    strategy.entry(\"Short\", strategy.short, stop=8, limit=7)\nif bar_index == 2\n    strategy.close_all(immediately=true)";
    let short = run_strategy(
        short_source,
        &candles(&[
            (10.0, 10.5, 9.5, 10.0),
            (10.0, 12.0, 6.0, 7.0),
            (7.0, 7.5, 6.5, 7.0),
        ]),
        0.0,
        0.0,
    )
    .expect("marketable short limit should fill when its stop activates");
    assert_eq!(short.closed_trades, 1);
    assert_eq!(short.trades[0].entry_price, 8.0);
}

#[test]
fn stop_entry_gapped_through_fills_at_the_next_open() {
    let source = "//@version=6\nstrategy(\"Gap stop\", default_qty_type=strategy.fixed, default_qty_value=1)\nif bar_index == 0\n    strategy.entry(\"Long\", strategy.long, stop=12)\nif bar_index == 1\n    strategy.close(\"Long\", immediately=true)";
    let output = run_strategy(
        source,
        &candles(&[(10.0, 10.5, 9.5, 10.0), (13.0, 14.0, 12.5, 13.5)]),
        0.0,
        0.0,
    )
    .expect("stop entry should fill at the open after a gap through its trigger");
    assert_eq!(output.trades[0].entry_price, 13.0);
}

#[test]
fn bracket_exit_cannot_fill_before_an_intrabar_stop_entry_exists() {
    let source = "//@version=6\nstrategy(\"Bracket after stop entry\", default_qty_type=strategy.fixed, default_qty_value=1)\nif bar_index == 0\n    strategy.entry(\"Long\", strategy.long, stop=12)\n    strategy.exit(\"Protection\", \"Long\", stop=9)";
    let output = run_strategy(
        source,
        &candles(&[(10.0, 10.5, 9.5, 10.0), (10.0, 13.0, 8.0, 11.0)]),
        0.0,
        0.0,
    )
    .expect("stop entry should open after the bar visited the protective stop");
    assert_eq!(output.closed_trades, 0);
    assert_eq!(output.open_position_size, 1.0);

    let target_source = source.replace("stop=9", "limit=13");
    let target_output = run_strategy(
        &target_source,
        &candles(&[(10.0, 10.5, 9.5, 10.0), (10.0, 14.0, 8.0, 11.0)]),
        0.0,
        0.0,
    )
    .expect("a bracket target after the stop-entry fill should remain eligible");
    assert_eq!(target_output.closed_trades, 1);
    assert_eq!(target_output.trades[0].exit_price, 13.0);
}

#[test]
fn strategy_exit_profit_and_loss_place_entry_relative_tick_brackets() {
    let long_target_source = "//@version=6\nstrategy(\"Relative long target\", default_qty_type=strategy.fixed, default_qty_value=1)\nif bar_index == 0\n    strategy.entry(\"Long\", strategy.long)\n    strategy.exit(\"Target\", \"Long\", profit=4)";
    let long_target = run_strategy_with_tick_size(
        long_target_source,
        &candles(&[(10.0, 10.2, 9.8, 10.0), (10.0, 12.5, 9.5, 12.0)]),
        Some(0.5),
    )
    .expect("relative profit target should use entry price and instrument tick size");
    assert_eq!(long_target.closed_trades, 1);
    assert_eq!(long_target.trades[0].entry_price, 10.0);
    assert_eq!(long_target.trades[0].exit_price, 12.0);

    let short_loss_source = "//@version=6\nstrategy(\"Relative short stop\", default_qty_type=strategy.fixed, default_qty_value=1)\nif bar_index == 0\n    strategy.entry(\"Short\", strategy.short)\n    strategy.exit(\"Stop\", \"Short\", loss=2)";
    let short_loss = run_strategy_with_tick_size(
        short_loss_source,
        &candles(&[(10.0, 10.2, 9.8, 10.0), (10.0, 12.0, 9.5, 11.0)]),
        Some(0.5),
    )
    .expect("relative loss stop should be direction-aware for short positions");
    assert_eq!(short_loss.closed_trades, 1);
    assert_eq!(short_loss.trades[0].exit_price, 11.0);
}

#[test]
fn strategy_exit_relative_ticks_require_tick_metadata_and_integer_distances() {
    let no_tick = "//@version=6\nstrategy(\"Missing tick size\")\nstrategy.entry(\"Long\", strategy.long)\nstrategy.exit(\"Target\", \"Long\", profit=4)";
    let error = run_strategy_with_tick_size(no_tick, &candles(&[(10.0, 10.5, 9.5, 10.0)]), None)
        .expect_err("relative tick exits require authoritative tick metadata");
    assert!(error.iter().any(|diagnostic| diagnostic
        .message
        .contains("require authoritative instrument tick-size metadata")));

    let fractional = no_tick.replace("profit=4", "profit=1.5");
    let error =
        run_strategy_with_tick_size(&fractional, &candles(&[(10.0, 10.5, 9.5, 10.0)]), Some(0.5))
            .expect_err("profit must be an integer tick distance");
    assert!(error.iter().any(|diagnostic| diagnostic
        .message
        .contains("finite nonnegative tick distance")));
}

#[test]
fn trailing_exit_activates_ratchets_and_closes_for_long_and_short_positions() {
    let long_source = "//@version=6\nstrategy(\"Long trailing stop\", default_qty_type=strategy.fixed, default_qty_value=1)\nif bar_index == 0\n    strategy.entry(\"Long\", strategy.long)\n    strategy.exit(\"Trail\", \"Long\", trail_price=12, trail_offset=2)";
    let long_bars = candles(&[(10.0, 10.5, 9.5, 10.0), (10.0, 14.0, 8.0, 10.0)]);
    let long = run_strategy_with_tick_size(long_source, &long_bars, Some(0.5))
        .expect("absolute activation and tick offset should create a trailing exit");
    assert_eq!(long.closed_trades, 1);
    assert_eq!(long.trades[0].exit_price, 13.0);

    let short_source = "//@version=6\nstrategy(\"Short trailing stop\", default_qty_type=strategy.fixed, default_qty_value=1)\nif bar_index == 0\n    strategy.entry(\"Short\", strategy.short)\n    strategy.exit(\"Trail\", \"Short\", trail_points=4, trail_offset=2)";
    let short_bars = candles(&[(10.0, 10.5, 9.5, 10.0), (10.0, 12.0, 6.0, 10.0)]);
    let short = run_strategy_with_tick_size(short_source, &short_bars, Some(0.5))
        .expect("entry-relative activation should create a trailing exit for shorts");
    assert_eq!(short.closed_trades, 1);
    assert_eq!(short.trades[0].exit_price, 7.0);
}

#[test]
fn trailing_stop_keeps_its_best_price_across_bars() {
    let source = "//@version=6\nstrategy(\"Persistent trail\", default_qty_type=strategy.fixed, default_qty_value=1)\nif bar_index == 0\n    strategy.entry(\"Long\", strategy.long)\n    strategy.exit(\"Trail\", \"Long\", trail_price=12, trail_offset=2)";
    let bars = candles(&[
        (10.0, 10.5, 9.5, 10.0),
        (10.0, 14.0, 9.5, 13.5),
        (13.5, 13.8, 12.0, 13.0),
    ]);
    let output = run_strategy_with_tick_size(source, &bars, Some(0.5))
        .expect("an active trail should retain its high-water mark across historical bars");
    assert_eq!(output.closed_trades, 1);
    assert_eq!(output.trades[0].exit_price, 13.0);
    assert_eq!(output.trades[0].exit_bar, 2);
}

#[test]
fn trailing_stop_uses_matching_bar_magnifier_ohlc_sequence() {
    let source = "//@version=6\nstrategy(\"Magnified trailing stop\", default_qty_type=strategy.fixed, default_qty_value=1, use_bar_magnifier=true)\nif bar_index == 0\n    strategy.entry(\"Long\", strategy.long)\n    strategy.exit(\"Trail\", \"Long\", trail_price=12, trail_offset=2)";
    let hour_ns = 3_600_000_000_000_i64;
    let minute_15_ns = 900_000_000_000_i64;
    let chart_bars = vec![
        Candle {
            open: 10.0,
            high: 10.0,
            low: 10.0,
            close: 10.0,
            volume: 1.0,
            time: Some(0),
        },
        Candle {
            open: 10.0,
            high: 14.0,
            low: 10.0,
            close: 12.5,
            volume: 1.0,
            time: Some(hour_ns),
        },
    ];
    let magnifier_bars = vec![
        Candle {
            open: 10.0,
            high: 10.0,
            low: 10.0,
            close: 10.0,
            volume: 1.0,
            time: Some(0),
        },
        Candle {
            open: 10.0,
            high: 10.0,
            low: 10.0,
            close: 10.0,
            volume: 1.0,
            time: Some(3 * minute_15_ns),
        },
        Candle {
            open: 10.0,
            high: 14.0,
            low: 10.0,
            close: 14.0,
            volume: 1.0,
            time: Some(hour_ns),
        },
        Candle {
            open: 14.0,
            high: 14.0,
            low: 12.0,
            close: 12.5,
            volume: 1.0,
            time: Some(hour_ns + 3 * minute_15_ns),
        },
    ];
    let output = run_strategy_with_history_limit_and_magnifier_and_tick_size(
        source,
        &chart_bars,
        "1h",
        &HashMap::new(),
        &magnifier_bars,
        Some("15m"),
        4,
        0.0,
        0.0,
        &HashMap::new(),
        Some(0.5),
    )
    .expect("matching lower-timeframe bars should drive trailing activation and stop fills");
    assert_eq!(output.closed_trades, 1);
    assert_eq!(output.trades[0].exit_price, 13.0);
    assert!(
        output.diagnostics.iter().any(|item| item.message.contains(
            "Bar Magnifier used lower-timeframe OHLC that matches chart-bar aggregate OHLC"
        )),
        "diagnostics: {:?}",
        output.diagnostics
    );
}

#[test]
fn trailing_exit_uses_the_earlier_absolute_or_relative_activation() {
    let source = "//@version=6\nstrategy(\"Earlier trail activation\", default_qty_type=strategy.fixed, default_qty_value=1)\nif bar_index == 0\n    strategy.entry(\"Long\", strategy.long)\n    strategy.exit(\"Trail\", \"Long\", trail_price=12, trail_points=2, trail_offset=2)";
    let bars = candles(&[(10.0, 10.5, 9.5, 10.0), (10.0, 11.5, 9.0, 10.0)]);
    let output = run_strategy_with_tick_size(source, &bars, Some(0.5))
        .expect("both activation inputs select the earlier favorable trail level");
    assert_eq!(output.closed_trades, 1);
    assert_eq!(output.trades[0].exit_price, 10.5);
}

#[test]
fn trailing_exit_rejects_incomplete_pairs_and_missing_tick_metadata() {
    let bars = candles(&[(10.0, 10.5, 9.5, 10.0)]);
    let incomplete = "//@version=6\nstrategy(\"Incomplete trail\")\nstrategy.exit(\"Trail\", \"Long\", trail_price=12)";
    let error = run_strategy(incomplete, &bars, 0.0, 0.0)
        .expect_err("a trailing activation requires an offset");
    assert!(error.iter().any(|item| item
        .message
        .contains("trailing stops require `trail_offset`")));

    let no_tick = "//@version=6\nstrategy(\"Trail without tick metadata\")\nstrategy.exit(\"Trail\", \"Long\", trail_points=4, trail_offset=2)";
    let error = run_strategy(no_tick, &bars, 0.0, 0.0)
        .expect_err("trail tick distances require authoritative instrument tick metadata");
    assert!(error.iter().any(|item| item
        .message
        .contains("trailing tick distances require authoritative instrument tick-size metadata")));
}

#[test]
fn existing_position_bracket_fills_before_a_later_intrabar_reversal_entry() {
    let source = "//@version=6\nstrategy(\"Exit before reversal entry\", pyramiding=2, default_qty_type=strategy.fixed, default_qty_value=1)\nif bar_index == 0\n    strategy.entry(\"Long\", strategy.long)\n    strategy.exit(\"Protection\", \"Long\", stop=9)\n    strategy.entry(\"Short\", strategy.short, stop=8)";
    let output = run_strategy(
        source,
        &candles(&[(10.0, 10.5, 9.5, 10.0), (10.0, 13.0, 7.0, 11.0)]),
        0.0,
        0.0,
    )
    .expect("the existing position exit should precede the later reversal entry on the path");
    assert_eq!(output.closed_trades, 1);
    assert_eq!(output.trades[0].entry_id, "Long");
    assert_eq!(output.trades[0].exit_price, 9.0);
    assert_eq!(output.open_position_size, -1.0);
}

#[test]
fn cancel_by_id_and_cancel_all_remove_unfilled_entry_orders() {
    let bars = candles(&[
        (10.0, 10.5, 9.5, 10.0),
        (10.0, 10.5, 9.5, 10.0),
        (10.0, 11.0, 8.0, 9.0),
    ]);
    let cancel_id = "//@version=6\nstrategy(\"Cancel ID\")\nif bar_index == 0\n    strategy.entry(\"Long\", strategy.long, limit=9)\nif bar_index == 1\n    strategy.cancel(\"Long\")";
    let output = run_strategy(cancel_id, &bars, 0.0, 0.0)
        .expect("cancel by id should remove an unfilled limit entry");
    assert!(output.trades.is_empty());

    let cancel_all = "//@version=6\nstrategy(\"Cancel all\")\nif bar_index == 0\n    strategy.entry(\"Long\", strategy.long, limit=9)\n    strategy.entry(\"Long 2\", strategy.long, stop=12)\nif bar_index == 1\n    strategy.cancel_all()";
    let output = run_strategy(cancel_all, &bars, 0.0, 0.0)
        .expect("cancel_all should remove all unfilled price entries");
    assert!(output.trades.is_empty());
}

#[test]
fn cancel_on_the_placement_execution_removes_a_market_entry() {
    let source = "//@version=6\nstrategy(\"Cancel market\")\nstrategy.entry(\"Long\", strategy.long)\nstrategy.cancel(\"Long\")";
    let output = run_strategy(
        source,
        &candles(&[(10.0, 10.5, 9.5, 10.0), (11.0, 12.0, 10.5, 12.0)]),
        0.0,
        0.0,
    )
    .expect("same-execution market cancellation should work");
    assert_eq!(output.open_position_size, 0.0);
}

#[test]
fn repeated_entry_id_modifies_pending_market_entry_instead_of_adding_another() {
    let source = "//@version=6\nstrategy(\"Modify market ID\", pyramiding=5, default_qty_type=strategy.fixed, default_qty_value=1)\nif bar_index == 0\n    strategy.entry(\"Long\", strategy.long)\nif bar_index == 1\n    strategy.entry(\"Long\", strategy.long, qty=2)\nif bar_index == 2\n    strategy.close_all()";
    let output = run_strategy(
        source,
        &candles(&[
            (10.0, 10.5, 9.5, 10.0),
            (11.0, 11.5, 10.5, 11.0),
            (12.0, 12.5, 11.5, 12.0),
            (13.0, 13.5, 12.5, 13.0),
        ]),
        0.0,
        0.0,
    )
    .expect("same-ID market entry should replace the pending order");
    assert_eq!(output.trades.len(), 2);
    assert_eq!(output.trades[0].quantity, 1.0);
    assert_eq!(output.trades[1].quantity, 2.0);
}

#[test]
fn same_id_market_order_submitted_before_pending_fill_replaces_original() {
    let source = "//@version=6\nstrategy(\"Replace pending market ID\", default_qty_type=strategy.fixed, default_qty_value=1)\nif bar_index == 0\n    strategy.entry(\"Long\", strategy.long)\n    strategy.entry(\"Long\", strategy.long, qty=2)\nif bar_index == 1\n    strategy.close_all()";
    let output = run_strategy(
        source,
        &candles(&[
            (10.0, 10.5, 9.5, 10.0),
            (11.0, 11.5, 10.5, 11.0),
            (12.0, 12.5, 11.5, 12.0),
        ]),
        0.0,
        0.0,
    )
    .expect("same-ID market entries from one calculation should leave one pending entry");
    assert_eq!(output.trades.len(), 1);
    assert_eq!(output.trades[0].quantity, 2.0);
    assert_eq!(output.trades[0].entry_bar, 1);
}

#[test]
fn repeated_entry_and_order_ids_share_one_pending_order_namespace() {
    let source = "//@version=6\nstrategy(\"Shared order IDs\", pyramiding=5, default_qty_type=strategy.fixed, default_qty_value=1)\nif bar_index == 0\n    strategy.entry(\"Shared\", strategy.long, qty=1, limit=9)\nif bar_index == 1\n    strategy.order(\"Shared\", strategy.long, qty=2, limit=8)\nif bar_index == 2\n    strategy.close_all()";
    let output = run_strategy(
        source,
        &candles(&[
            (10.0, 10.5, 9.5, 10.0),
            (10.0, 10.5, 7.5, 10.0),
            (8.0, 8.5, 7.5, 8.0),
            (9.0, 9.5, 8.5, 9.0),
        ]),
        0.0,
        0.0,
    )
    .expect("entry and order commands with the same ID should replace one another");
    assert_eq!(output.trades.len(), 2);
    assert!(output
        .trades
        .iter()
        .any(|trade| trade.quantity == 2.0 && trade.entry_price == 8.0));
}

#[test]
fn repeated_same_id_price_entry_replaces_price_and_keeps_other_ids() {
    let source = "//@version=6\nstrategy(\"Modify price IDs\", pyramiding=5, default_qty_type=strategy.fixed, default_qty_value=1)\nif bar_index == 0\n    strategy.entry(\"Long\", strategy.long, limit=9)\n    strategy.entry(\"Other\", strategy.long, limit=8)\nif bar_index == 1\n    strategy.entry(\"Long\", strategy.long, qty=2, limit=7)\nif bar_index == 2\n    strategy.close_all()";
    let output = run_strategy(
        source,
        &candles(&[
            (10.0, 10.5, 9.5, 10.0),
            (10.0, 10.5, 7.5, 10.0),
            (8.0, 8.5, 6.5, 8.0),
            (9.0, 9.5, 8.5, 9.0),
        ]),
        0.0,
        0.0,
    )
    .expect("same-ID price entry should be modified while a different ID remains pending");
    assert_eq!(output.trades.len(), 3);
    assert!(output
        .trades
        .iter()
        .any(|trade| trade.entry_id == "Other" && trade.entry_price == 8.0));
    assert!(output.trades.iter().any(|trade| trade.entry_id == "Long"
        && trade.quantity == 2.0
        && trade.entry_price == 7.0));
}

#[test]
fn process_orders_on_close_applies_only_to_price_orders_marketable_at_the_close() {
    let marketable = "//@version=6\nstrategy(\"Close limit\", process_orders_on_close=true)\nstrategy.entry(\"Long\", strategy.long, limit=11)";
    let filled = run_strategy(marketable, &candles(&[(10.0, 12.0, 8.0, 10.0)]), 0.0, 0.0)
        .expect("marketable limit should fill at the processing close");
    assert_eq!(filled.open_position_size, 1.0);

    let not_marketable = "//@version=6\nstrategy(\"Close limit pending\", process_orders_on_close=true)\nstrategy.entry(\"Long\", strategy.long, limit=9)";
    let pending = run_strategy(
        not_marketable,
        &candles(&[(10.0, 12.0, 8.0, 10.0)]),
        0.0,
        0.0,
    )
    .expect("earlier intrabar touch must not fill an order submitted at the close");
    assert_eq!(pending.open_position_size, 0.0);
}

#[test]
fn process_orders_on_close_defers_oca_updates_until_all_close_tick_fills() {
    let bars = candles(&[(10.0, 10.5, 9.5, 10.0)]);
    for oca_type in ["strategy.oca.cancel", "strategy.oca.reduce"] {
        let same_tick = format!("//@version=6\nstrategy(\"Close tick OCA batch\", process_orders_on_close=true, pyramiding=5, default_qty_type=strategy.fixed, default_qty_value=1)\nif bar_index == 0\n    strategy.entry(\"First\", strategy.long, limit=10, oca_name=\"close\", oca_type={oca_type})\n    strategy.entry(\"Second\", strategy.long, limit=10, oca_name=\"close\", oca_type={oca_type})");
        let output = run_strategy(&same_tick, &bars, 0.0, 0.0)
            .expect("same-close-tick price orders should execute before OCA cancel/reduce updates");
        assert_eq!(output.open_position_size, 2.0);
    }

    let later_sibling = "//@version=6\nstrategy(\"Close tick later OCA\", process_orders_on_close=true, pyramiding=5, default_qty_type=strategy.fixed, default_qty_value=1)\nif bar_index == 0\n    strategy.entry(\"Marketable limit\", strategy.long, limit=10, oca_name=\"close\", oca_type=strategy.oca.cancel)\n    strategy.entry(\"Later stop\", strategy.long, stop=12, oca_name=\"close\", oca_type=strategy.oca.cancel)";
    let bars = candles(&[(10.0, 13.0, 9.5, 10.0)]);
    let output = run_strategy(later_sibling, &bars, 0.0, 0.0)
        .expect("the close fill should cancel an OCA sibling that did not execute at the close");
    assert_eq!(output.open_position_size, 1.0);
}

#[test]
fn process_orders_on_close_fills_new_bracket_only_when_close_reaches_its_level() {
    let source = "//@version=6\nstrategy(\"Close bracket\", process_orders_on_close=true, default_qty_type=strategy.fixed, default_qty_value=1)\nif bar_index == 0\n    strategy.entry(\"Long\", strategy.long)\nif bar_index == 1\n    strategy.exit(\"Close target\", \"Long\", limit=12)";
    let output = run_strategy(
        source,
        &candles(&[(10.0, 10.5, 9.5, 10.0), (11.0, 14.0, 9.0, 13.0)]),
        0.0,
        0.0,
    )
    .expect("new close-tick bracket should only see the closing price");
    assert_eq!(output.closed_trades, 1);
    assert_eq!(output.trades[0].exit_price, 13.0);
    assert_eq!(output.trades[0].exit_bar, 1);
}

#[test]
fn strategy_order_nets_against_the_open_position_and_can_reverse_it() {
    let source = "//@version=6\nstrategy(\"Net orders\", default_qty_type=strategy.fixed, default_qty_value=1)\nif bar_index == 0\n    strategy.order(\"Long\", strategy.long, qty=2)\nif bar_index == 1\n    strategy.order(\"Reverse\", strategy.short, qty=3)\nif bar_index == 2\n    strategy.close_all(immediately=true)";
    let output = run_strategy(
        source,
        &candles(&[
            (10.0, 10.5, 9.5, 10.0),
            (11.0, 12.0, 10.5, 11.5),
            (13.0, 14.0, 12.5, 13.5),
            (14.0, 14.5, 13.5, 14.0),
        ]),
        0.0,
        0.0,
    )
    .expect("strategy.order should net the existing position");

    assert_eq!(output.closed_trades, 2);
    assert_eq!(output.trades[0].entry_id, "Long");
    assert_eq!(output.trades[0].quantity, 2.0);
    assert_eq!(output.trades[0].exit_price, 13.0);
    assert_eq!(output.trades[1].entry_id, "Reverse");
    assert_eq!(output.trades[1].quantity, 1.0);
    assert_eq!(output.trades[1].direction, "SHORT");
    assert_eq!(output.trades[1].entry_price, 13.0);
}

#[test]
fn strategy_order_accepts_positional_limit_price() {
    let source = "//@version=6\nstrategy(\"Positional net order\", default_qty_type=strategy.fixed, default_qty_value=1)\nif bar_index == 0\n    strategy.order(\"Limit long\", strategy.long, 1, 9)\nif bar_index == 2\n    strategy.close_all(immediately=true)";
    let output = run_strategy(
        source,
        &candles(&[
            (10.0, 10.5, 9.5, 10.0),
            (10.0, 10.5, 8.5, 9.0),
            (9.0, 10.0, 8.5, 9.5),
        ]),
        0.0,
        0.0,
    )
    .expect("the fourth positional argument is the order limit price");
    assert_eq!(output.closed_trades, 1);
    assert_eq!(output.trades[0].entry_price, 9.0);
    assert_eq!(output.trades[0].quantity, 1.0);
}

#[test]
fn strategy_order_oca_cancel_keeps_only_the_first_filled_group_order() {
    let source = "//@version=6\nstrategy(\"OCA cancel\", default_qty_type=strategy.fixed, default_qty_value=1)\nif bar_index == 0\n    strategy.order(\"First\", strategy.long, qty=1, limit=9, oca_name=\"entries\", oca_type=strategy.oca.cancel)\n    strategy.order(\"Second\", strategy.long, qty=1, limit=8, oca_name=\"entries\", oca_type=strategy.oca.cancel)\nif bar_index == 2\n    strategy.close_all(immediately=true)";
    let output = run_strategy(
        source,
        &candles(&[
            (10.0, 10.5, 9.5, 10.0),
            (10.0, 10.5, 7.0, 9.0),
            (9.0, 9.5, 8.5, 9.0),
        ]),
        0.0,
        0.0,
    )
    .expect("OCA cancel should remove sibling pending orders");

    assert_eq!(output.closed_trades, 1);
    assert_eq!(output.trades[0].entry_id, "First");
    assert_eq!(output.trades[0].quantity, 1.0);
    assert_eq!(output.trades[0].entry_price, 9.0);
}

#[test]
fn strategy_entry_oca_cancel_removes_sibling_price_entries() {
    let source = "//@version=6\nstrategy(\"Entry OCA cancel\", pyramiding=5, default_qty_type=strategy.fixed, default_qty_value=1)\nif bar_index == 0\n    strategy.entry(\"First\", strategy.long, qty=1, limit=9, oca_name=\"entries\", oca_type=strategy.oca.cancel)\n    strategy.entry(\"Second\", strategy.long, qty=1, limit=8, oca_name=\"entries\", oca_type=strategy.oca.cancel)";
    let output = run_strategy(
        source,
        &candles(&[(10.0, 10.5, 9.5, 10.0), (10.0, 10.5, 7.0, 9.0)]),
        0.0,
        0.0,
    )
    .expect("strategy.entry OCA cancel should remove sibling price entries");
    assert_eq!(output.open_position_size, 1.0);
}

#[test]
fn same_tick_price_orders_defer_oca_cancel_and_reduce_updates() {
    let bars = candles(&[(11.0, 12.0, 10.5, 11.0), (11.0, 12.0, 9.0, 10.5)]);
    for oca_type in ["strategy.oca.cancel", "strategy.oca.reduce"] {
        let source = format!("//@version=6\nstrategy(\"Same-tick OCA\", default_qty_type=strategy.fixed, default_qty_value=1)\nif bar_index == 0\n    strategy.order(\"Buy limit\", strategy.long, limit=10, oca_name=\"Entry\", oca_type={oca_type})\n    strategy.order(\"Sell stop\", strategy.short, stop=10, oca_name=\"Entry\", oca_type={oca_type})");
        let output = run_strategy(&source, &bars, 0.0, 0.0)
            .expect("same-tick execution must precede OCA cancel/reduce updates");
        assert_eq!(output.closed_trades, 1);
        assert_eq!(output.open_position_size, 0.0);
        assert_eq!(output.trades[0].entry_id, "Buy limit");
    }
}

#[test]
fn strategy_entry_oca_reduce_subtracts_filled_quantity_from_siblings() {
    let source = "//@version=6\nstrategy(\"Entry OCA reduce\", pyramiding=5, default_qty_type=strategy.fixed, default_qty_value=1)\nif bar_index == 0\n    strategy.entry(\"First\", strategy.long, qty=2, limit=9, oca_name=\"entries\", oca_type=strategy.oca.reduce)\n    strategy.entry(\"Second\", strategy.long, qty=3, limit=8, oca_name=\"entries\", oca_type=strategy.oca.reduce)";
    let output = run_strategy(
        source,
        &candles(&[(10.0, 10.5, 9.5, 10.0), (10.0, 10.5, 7.0, 9.0)]),
        0.0,
        0.0,
    )
    .expect("strategy.entry OCA reduce should reduce sibling quantity");
    assert_eq!(output.open_position_size, 3.0);
}

#[test]
fn strategy_entry_accepts_positional_oca_arguments() {
    let source = "//@version=6\nstrategy(\"Entry positional OCA\", pyramiding=5, default_qty_type=strategy.fixed, default_qty_value=1)\nif bar_index == 0\n    strategy.entry(\"First\", strategy.long, 1, 9, na, \"entries\", strategy.oca.cancel)\n    strategy.entry(\"Second\", strategy.long, 1, 8, na, \"entries\", strategy.oca.cancel)";
    let output = run_strategy(
        source,
        &candles(&[(10.0, 10.5, 9.5, 10.0), (10.0, 10.5, 7.0, 9.0)]),
        0.0,
        0.0,
    )
    .expect("strategy.entry should accept positional OCA name and type");
    assert_eq!(output.open_position_size, 1.0);
}

#[test]
fn market_orders_in_same_oca_group_execute_before_cancel_or_reduce_updates() {
    for oca_type in ["strategy.oca.cancel", "strategy.oca.reduce"] {
        let source = format!("//@version=6\nstrategy(\"Entry market OCA\", pyramiding=5, default_qty_type=strategy.fixed, default_qty_value=1)\nif bar_index == 0\n    strategy.entry(\"First\", strategy.long, qty=1, oca_name=\"entries\", oca_type={oca_type})\n    strategy.entry(\"Second\", strategy.long, qty=1, oca_name=\"entries\", oca_type={oca_type})");
        let output = run_strategy(
            &source,
            &candles(&[(10.0, 10.5, 9.5, 10.0), (11.0, 11.5, 10.5, 11.0)]),
            0.0,
            0.0,
        )
        .expect("same-opening-tick market orders should execute before OCA updates");
        assert_eq!(output.open_position_size, 2.0);
    }
}

#[test]
fn next_open_market_fill_still_cancels_a_later_price_order_sibling() {
    let source = "//@version=6\nstrategy(\"Market then stop OCA\", pyramiding=5, default_qty_type=strategy.fixed, default_qty_value=1)\nif bar_index == 0\n    strategy.entry(\"Market\", strategy.long, oca_name=\"entries\", oca_type=strategy.oca.cancel)\n    strategy.entry(\"Later stop\", strategy.long, stop=12, oca_name=\"entries\", oca_type=strategy.oca.cancel)";
    let output = run_strategy(
        source,
        &candles(&[(10.0, 10.5, 9.5, 10.0), (11.0, 13.0, 10.5, 12.5)]),
        0.0,
        0.0,
    )
    .expect("the market fill should cancel the later, non-same-tick stop");
    assert_eq!(output.open_position_size, 1.0);
}

#[test]
fn strategy_entry_rejects_oca_cancel_without_group_name() {
    let source = "//@version=6\nstrategy(\"Entry unnamed OCA\")\nstrategy.entry(\"Long\", strategy.long, oca_type=strategy.oca.cancel)";
    let result = run_strategy(source, &candles(&[(10.0, 10.5, 9.5, 10.0)]), 0.0, 0.0)
        .expect_err("OCA cancel requires a group name");
    assert!(result
        .iter()
        .any(|diagnostic| diagnostic.message.contains("requires a nonempty oca_name")));
}

#[test]
fn same_bar_oca_cancel_uses_inferred_price_path_before_submission_order() {
    let source = "//@version=6\nstrategy(\"OCA path order\", default_qty_type=strategy.fixed, default_qty_value=1)\nif bar_index == 0\n    strategy.order(\"Long stop\", strategy.long, qty=1, stop=11, oca_name=\"entry\", oca_type=strategy.oca.cancel)\n    strategy.order(\"Short stop\", strategy.short, qty=1, stop=9, oca_name=\"entry\", oca_type=strategy.oca.cancel)";
    let output = run_strategy(
        source,
        &candles(&[(10.0, 10.5, 9.5, 10.0), (10.0, 12.0, 8.0, 11.0)]),
        0.0,
        0.0,
    )
    .expect("same-bar order fills should follow the inferred OHLC path");
    assert_eq!(output.open_position_size, -1.0);
}

#[test]
fn strategy_order_oca_reduce_subtracts_filled_quantity_from_siblings() {
    let source = "//@version=6\nstrategy(\"OCA reduce\", default_qty_type=strategy.fixed, default_qty_value=1)\nif bar_index == 0\n    strategy.order(\"First\", strategy.long, qty=2, limit=9, oca_name=\"entries\", oca_type=strategy.oca.reduce)\n    strategy.order(\"Second\", strategy.long, qty=3, limit=8, oca_name=\"entries\", oca_type=strategy.oca.reduce)\nif bar_index == 2\n    strategy.close_all(immediately=true)";
    let output = run_strategy(
        source,
        &candles(&[
            (10.0, 10.5, 9.5, 10.0),
            (10.0, 10.5, 7.0, 9.0),
            (9.0, 9.5, 8.5, 9.0),
        ]),
        0.0,
        0.0,
    )
    .expect("OCA reduce should bound combined filled quantity");

    assert_eq!(output.closed_trades, 2);
    assert_eq!(output.trades[0].entry_id, "First");
    assert_eq!(output.trades[0].quantity, 2.0);
    assert_eq!(output.trades[1].entry_id, "Second");
    assert_eq!(output.trades[1].quantity, 1.0);
}

#[test]
fn same_oca_name_with_different_types_defines_distinct_groups() {
    let source = "//@version=6\nstrategy(\"Separate OCA types\", default_qty_type=strategy.fixed, default_qty_value=1)\nif bar_index == 0\n    strategy.order(\"Cancel group\", strategy.long, qty=1, oca_name=\"shared\", oca_type=strategy.oca.cancel)\n    strategy.order(\"Reduce group\", strategy.long, qty=1, oca_name=\"shared\", oca_type=strategy.oca.reduce)";
    let output = run_strategy(
        source,
        &candles(&[(10.0, 10.5, 9.5, 10.0), (11.0, 11.5, 10.5, 11.0)]),
        0.0,
        0.0,
    )
    .expect("same names with distinct OCA types should belong to separate groups");
    assert_eq!(output.open_position_size, 2.0);
}

#[test]
fn strategy_cancel_removes_a_pending_strategy_order_by_id() {
    let source = "//@version=6\nstrategy(\"Cancel net order\", default_qty_type=strategy.fixed, default_qty_value=2)\nif bar_index == 0\n    strategy.order(\"To cancel\", strategy.long, qty=2)\n    strategy.cancel(\"To cancel\")";
    let output = run_strategy(
        source,
        &candles(&[(10.0, 10.5, 9.5, 10.0), (11.0, 11.5, 10.5, 11.0)]),
        0.0,
        0.0,
    )
    .expect("strategy.cancel should cancel pending strategy.order IDs");
    assert_eq!(output.open_position_size, 0.0);
}

#[test]
fn rejects_wrong_declaration_and_unsupported_pyramiding() {
    let candles = candles(&[(10.0, 10.5, 9.5, 10.0)]);
    let indicator = run_strategy(
        "//@version=6\nindicator(\"Not a strategy\")\nplot(close)",
        &candles,
        0.0,
        0.0,
    )
    .expect_err("indicator source must not enter the strategy engine");
    assert!(indicator
        .iter()
        .any(|diagnostic| diagnostic.message.contains("must declare `strategy()`")));
    let strategy = "//@version=6\nstrategy(\"Pyramiding\", pyramiding=101)\nstrategy.entry(\"Long\", strategy.long)";
    let result = run_strategy(strategy, &candles, 0.0, 0.0)
        .expect_err("pyramiding above the bounded broker limit must be explicit");
    assert!(result
        .iter()
        .any(|diagnostic| diagnostic.message.contains("through 100")));
}

#[test]
fn strategy_entry_rejects_invalid_explicit_quantity_instead_of_using_default_size() {
    for quantity in ["0", "-1", "\"invalid\""] {
        let source = format!("//@version=6\nstrategy(\"Invalid entry size\", default_qty_type=strategy.fixed, default_qty_value=3)\nstrategy.entry(\"Long\", strategy.long, qty={quantity})");
        let diagnostics = run_strategy(&source, &candles(&[(10.0, 10.5, 9.5, 10.0)]), 0.0, 0.0)
            .expect_err("invalid explicit strategy.entry quantity must fail validation");
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("strategy.entry()` qty must be a finite positive number or `na`")));
    }
}

#[test]
fn strategy_entry_na_quantity_keeps_the_declared_default_size() {
    let source = "//@version=6\nstrategy(\"NA entry size\", default_qty_type=strategy.fixed, default_qty_value=3)\nstrategy.entry(\"Long\", strategy.long, qty=na)";
    let output = run_strategy(
        &source,
        &candles(&[(10.0, 10.5, 9.5, 10.0), (11.0, 11.5, 10.5, 11.0)]),
        0.0,
        0.0,
    )
    .expect("na quantity should defer to strategy declaration sizing");
    assert_eq!(output.open_position_size, 3.0);
}

#[test]
fn fills_bracket_stop_using_the_inferred_ohlc_path_and_rejects_unsupported_costs() {
    let source = "//@version=6\nstrategy(\"Bracket exit\", default_qty_type=strategy.fixed, default_qty_value=1)\nif bar_index == 0\n    strategy.entry(\"Long\", strategy.long)\nstrategy.exit(\"Bracket\", \"Long\", stop=9, limit=13)";
    let bars = candles(&[(10.0, 11.0, 9.5, 10.0), (10.0, 14.0, 8.0, 12.0)]);
    let output = run_strategy(source, &bars, 0.0, 0.0)
        .expect("supported stop and limit brackets should execute");
    assert_eq!(output.closed_trades, 1);
    assert_eq!(output.trades[0].entry_bar, 1);
    assert_eq!(output.trades[0].exit_bar, 1);
    assert_eq!(output.trades[0].exit_price, 9.0);
    assert!(output.diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("Bar Magnifier is disabled by the strategy declaration")));

    let bars = candles(&[(10.0, 10.5, 9.5, 10.0)]);
    let property = run_strategy(
        "//@version=6\nstrategy(\"Slippage\", slippage=2)",
        &bars,
        0.0,
        0.0,
    )
    .expect_err("nonzero slippage without instrument tick metadata must fail");
    assert!(property.iter().any(|diagnostic| diagnostic
        .message
        .contains("no authoritative tick-size metadata")));
}

#[test]
fn strategy_slippage_moves_entry_and_exit_by_instrument_ticks() {
    let source = "//@version=6\nstrategy(\"Tick slippage\", default_qty_type=strategy.fixed, default_qty_value=1, slippage=2)\nif bar_index == 0\n    strategy.entry(\"Long\", strategy.long)\nif bar_index == 1\n    strategy.close_all(immediately=true)";
    let bars = candles(&[(10.0, 10.5, 9.5, 10.0), (11.0, 12.0, 10.5, 12.0)]);
    let output = run_strategy_with_history_limit_and_magnifier_and_tick_size(
        source,
        &bars,
        "1d",
        &HashMap::new(),
        &[],
        None,
        10,
        0.0,
        0.0,
        &HashMap::new(),
        Some(0.5),
    )
    .expect("registered tick size should enable tick slippage");
    assert_eq!(output.closed_trades, 1);
    assert_eq!(output.trades[0].entry_price, 12.0);
    assert_eq!(output.trades[0].exit_price, 11.0);
    assert_eq!(output.trades[0].gross_pnl, -1.0);
    assert!(output.diagnostics.iter().any(|item| item
        .message
        .contains("2 ticks at a registered price tick of 0.5")));
}

#[test]
fn slippage_that_produces_nonpositive_fills_rejects_the_backtest() {
    let bars = candles(&[(1.0, 1.1, 0.9, 1.0), (1.0, 1.1, 0.4, 1.0)]);
    let sources = [
        "//@version=6\nstrategy(\"Invalid market exit\", default_qty_type=strategy.fixed, default_qty_value=1, slippage=30)\nif bar_index == 0\n    strategy.entry(\"Long\", strategy.long)\nif bar_index == 1\n    strategy.close_all(immediately=true)",
        "//@version=6\nstrategy(\"Invalid stop exit\", default_qty_type=strategy.fixed, default_qty_value=1, slippage=30)\nif bar_index == 0\n    strategy.entry(\"Long\", strategy.long)\n    strategy.exit(\"Stop\", \"Long\", stop=0.5)",
    ];
    for source in sources {
        let diagnostics = run_strategy_with_history_limit_and_magnifier_and_tick_size(
            source,
            &bars,
            "1d",
            &HashMap::new(),
            &[],
            None,
            10,
            0.0,
            0.0,
            &HashMap::new(),
            Some(0.1),
        )
        .expect_err("nonpositive slippage-adjusted exits cannot produce complete accounting");
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("non-finite or nonpositive simulated fill price")));
    }
}

#[test]
fn price_entries_filling_at_the_same_tick_can_exceed_pyramiding() {
    let source = "//@version=6\nstrategy(\"Same-tick price entries\", default_qty_type=strategy.fixed, default_qty_value=1, pyramiding=1)\nif bar_index == 0\n    strategy.entry(\"First\", strategy.long, stop=11)\n    strategy.entry(\"Second\", strategy.long, stop=11)";
    let bars = candles(&[(10.0, 10.5, 9.5, 10.0), (10.0, 12.0, 9.8, 11.5)]);
    let output = run_strategy(source, &bars, 0.0, 0.0)
        .expect("same-tick price entries use TradingView's pyramiding exception");
    assert_eq!(output.open_position_size, 2.0);
}

#[test]
fn strategy_slippage_is_adverse_for_short_bracket_fills() {
    let source = "//@version=6\nstrategy(\"Short tick slippage\", default_qty_type=strategy.fixed, default_qty_value=1, slippage=1)\nif bar_index == 0\n    strategy.entry(\"Short\", strategy.short)\n    strategy.exit(\"Stop\", \"Short\", stop=12)";
    let bars = candles(&[(10.0, 10.5, 9.5, 10.0), (11.0, 13.0, 10.5, 12.5)]);
    let output = run_strategy_with_history_limit_and_magnifier_and_tick_size(
        source,
        &bars,
        "1d",
        &HashMap::new(),
        &[],
        None,
        10,
        0.0,
        0.0,
        &HashMap::new(),
        Some(0.5),
    )
    .expect("short stop should use registered tick metadata");
    assert_eq!(output.closed_trades, 1);
    assert_eq!(output.trades[0].entry_price, 10.5);
    assert_eq!(output.trades[0].exit_price, 12.5);
    assert_eq!(output.trades[0].gross_pnl, -2.0);
}

#[test]
fn strategy_slippage_does_not_move_limit_exit_fills() {
    let source = "//@version=6\nstrategy(\"Limit without slippage\", default_qty_type=strategy.fixed, default_qty_value=1, slippage=1, backtest_fill_limits_assumption=1)\nif bar_index == 0\n    strategy.entry(\"Long\", strategy.long)\n    strategy.exit(\"Target\", \"Long\", limit=12)";
    let bars = candles(&[(10.0, 10.5, 9.5, 10.0), (11.0, 12.5, 10.5, 12.0)]);
    let output = run_strategy_with_history_limit_and_magnifier_and_tick_size(
        source,
        &bars,
        "1d",
        &HashMap::new(),
        &[],
        None,
        10,
        0.0,
        0.0,
        &HashMap::new(),
        Some(0.5),
    )
    .expect("limit exit should preserve its requested fill price");
    assert_eq!(output.closed_trades, 1);
    assert_eq!(output.trades[0].entry_price, 11.5);
    assert_eq!(output.trades[0].exit_price, 12.0);
    assert_eq!(output.trades[0].gross_pnl, 0.5);

    let insufficient_move = candles(&[(10.0, 10.5, 9.5, 10.0), (11.0, 12.4, 10.5, 12.0)]);
    let output = run_strategy_with_history_limit_and_magnifier_and_tick_size(
        source,
        &insufficient_move,
        "1d",
        &HashMap::new(),
        &[],
        None,
        10,
        0.0,
        0.0,
        &HashMap::new(),
        Some(0.5),
    )
    .expect("a touch without the favorable verification move must not fill the target");
    assert_eq!(output.closed_trades, 0);
    assert_eq!(output.open_position_size, 1.0);
}

#[test]
fn backtest_limit_verification_requires_favorable_ticks_but_fills_at_the_limit() {
    let bars = candles(&[(10.0, 10.5, 9.5, 10.0), (10.0, 10.5, 9.0, 9.4)]);
    let verified_source = "//@version=6\nstrategy(\"Verified limit\", default_qty_type=strategy.fixed, default_qty_value=1, backtest_fill_limits_assumption=1)\nif bar_index == 0\n    strategy.entry(\"Long\", strategy.long, limit=9.5)\nplot(strategy.position_avg_price)";
    let missing_tick = run_strategy(verified_source, &bars, 0.0, 0.0)
        .expect_err("nonzero limit verification requires registered instrument ticks");
    assert!(missing_tick.iter().any(|item| item
        .message
        .contains("backtest_fill_limits_assumption cannot be simulated")));
    let verified = run_strategy_with_history_limit_and_magnifier_and_tick_size(
        verified_source,
        &bars,
        "1d",
        &HashMap::new(),
        &[],
        None,
        10,
        0.0,
        0.0,
        &HashMap::new(),
        Some(0.5),
    )
    .expect("one-tick favorable movement should verify the limit");
    assert_eq!(verified.open_position_size, 1.0);
    assert_eq!(verified.equity_curve.len(), bars.len());
    assert_eq!(verified.plots[0].values[1], Some(9.5));
    assert!(verified.diagnostics.iter().any(|item| item
        .message
        .contains("requires 1 favorable ticks at a registered price tick of 0.5")));

    let unverified_source = verified_source.replace("assumption=1", "assumption=2");
    let unverified = run_strategy_with_history_limit_and_magnifier_and_tick_size(
        &unverified_source,
        &bars,
        "1d",
        &HashMap::new(),
        &[],
        None,
        10,
        0.0,
        0.0,
        &HashMap::new(),
        Some(0.5),
    )
    .expect("insufficient favorable movement should leave the limit unfilled");
    assert_eq!(unverified.open_position_size, 0.0);
}

#[test]
fn strategy_exit_qty_reserves_partial_quantity_and_aggregates_closed_entry() {
    let source = "//@version=6\nstrategy(\"Partial target\", default_qty_type=strategy.fixed, default_qty_value=4)\nif bar_index == 0\n    strategy.entry(\"Long\", strategy.long)\n    strategy.exit(\"First target\", \"Long\", limit=12, qty=1)\n    strategy.exit(\"Second target\", \"Long\", limit=14, qty=1)\nif bar_index == 3\n    strategy.close(\"Long\", immediately=true)";
    let bars = candles(&[
        (10.0, 11.0, 9.0, 10.0),
        (11.0, 13.0, 10.5, 12.5),
        (13.0, 15.0, 12.5, 14.5),
        (14.5, 15.0, 13.5, 14.0),
    ]);
    let output = run_strategy(source, &bars, 0.0, 0.0)
        .expect("bounded partial exit quantities should execute");
    assert_eq!(output.closed_trades, 1);
    assert_eq!(output.trades.len(), 1);
    assert_eq!(output.trades[0].quantity, 4.0);
    assert_eq!(output.trades[0].exit_price, 13.5);
    assert_eq!(output.trades[0].gross_pnl, 10.0);
}

#[test]
fn strategy_exit_qty_percent_closes_only_the_requested_fraction() {
    let source = "//@version=6\nstrategy(\"Partial percent\", default_qty_type=strategy.fixed, default_qty_value=4)\nif bar_index == 0\n    strategy.entry(\"Long\", strategy.long)\n    strategy.exit(\"Half\", \"Long\", limit=12, qty_percent=50)\nif bar_index == 2\n    strategy.close(\"Long\", immediately=true)";
    let bars = candles(&[
        (10.0, 11.0, 9.0, 10.0),
        (11.0, 13.0, 10.5, 12.5),
        (13.0, 14.0, 12.5, 13.5),
    ]);
    let output = run_strategy(source, &bars, 0.0, 0.0)
        .expect("bounded partial exit percentages should execute");
    assert_eq!(output.closed_trades, 1);
    assert_eq!(output.trades[0].quantity, 4.0);
    assert_eq!(output.trades[0].exit_price, 12.75);
    assert_eq!(output.trades[0].gross_pnl, 7.0);
}

#[test]
fn from_entry_exit_does_not_attach_to_later_entries_with_the_same_id() {
    let source = "//@version=6\nstrategy(\"Exit entry cutoff\", pyramiding=2, default_qty_type=strategy.fixed, default_qty_value=1)\nif bar_index == 0\n    strategy.entry(\"A\", strategy.long)\n    strategy.exit(\"Old protection\", \"A\", stop=8)\nif bar_index == 2\n    strategy.entry(\"A\", strategy.long)\nif bar_index == 4\n    strategy.close_all(immediately=true)";
    let output = run_strategy(
        source,
        &candles(&[
            (10.0, 10.5, 9.5, 10.0),
            (10.0, 10.5, 9.5, 10.0),
            (10.0, 10.5, 9.5, 10.0),
            (10.0, 11.0, 7.0, 9.0),
            (9.0, 9.5, 8.5, 9.0),
        ]),
        0.0,
        0.0,
    )
    .expect("from_entry exits should respect their entry creation cutoff");
    assert_eq!(output.closed_trades, 2);
    assert_eq!(output.trades[0].entry_bar, 1);
    assert_eq!(output.trades[0].exit_bar, 3);
    assert_eq!(output.trades[0].exit_price, 8.0);
    assert_eq!(output.trades[1].entry_bar, 3);
    assert_eq!(output.trades[1].exit_bar, 4);
    assert_eq!(output.trades[1].exit_price, 9.0);
}

#[test]
fn partial_exit_updates_open_size_realized_profit_and_marked_equity() {
    let source = "//@version=6\nstrategy(\"Partial open size\", default_qty_type=strategy.fixed, default_qty_value=4)\nif bar_index == 0\n    strategy.entry(\"Long\", strategy.long)\n    strategy.exit(\"Half\", \"Long\", limit=12, qty=2)\nplot(strategy.position_size)";
    let output = run_strategy(
        source,
        &candles(&[(10.0, 11.0, 9.0, 10.0), (11.0, 13.0, 10.5, 13.0)]),
        0.0,
        0.0,
    )
    .expect("partial exit leaves the remaining position marked to market");
    assert_eq!(output.closed_trades, 0);
    assert_eq!(output.open_position_size, 2.0);
    assert_eq!(output.net_profit, 2.0);
    assert_eq!(output.plots[0].values, vec![Some(0.0), Some(2.0)]);
    assert_eq!(*output.equity_curve.last().unwrap(), 1_000_006.0);
}

#[test]
fn validates_candle_order_and_consistency_before_execution() {
    let unordered = vec![
        Candle {
            time: Some(2),
            ..candles(&[(10.0, 10.5, 9.5, 10.0)])[0]
        },
        Candle {
            time: Some(1),
            ..candles(&[(10.0, 10.5, 9.5, 10.0)])[0]
        },
    ];
    let result = run_strategy("//@version=6\nstrategy(\"Time\")", &unordered, 0.0, 0.0)
        .expect_err("historical order must be chronological");
    assert!(result
        .iter()
        .any(|diagnostic| diagnostic.message.contains("strictly increasing")));
    let inconsistent = vec![Candle {
        high: 9.0,
        ..candles(&[(10.0, 10.5, 9.5, 10.0)])[0]
    }];
    let result = run_strategy("//@version=6\nstrategy(\"OHLC\")", &inconsistent, 0.0, 0.0)
        .expect_err("OHLC ranges must be internally consistent");
    assert!(result
        .iter()
        .any(|diagnostic| diagnostic.message.contains("inconsistent OHLCV")));
}

#[test]
fn marks_open_positions_to_market_in_the_equity_curve() {
    let source = "//@version=6\nstrategy(\"Mark to market\", default_qty_type=strategy.fixed, default_qty_value=1)\nif bar_index == 0\n    strategy.entry(\"Long\", strategy.long)";
    let output = run_strategy(
        source,
        &candles(&[(10.0, 10.5, 9.5, 10.0), (11.0, 12.0, 10.5, 12.0)]),
        0.0,
        0.0,
    )
    .expect("valid strategy should run");
    assert_eq!(output.closed_trades, 0);
    assert_eq!(output.open_position_size, 1.0);
    assert_eq!(*output.equity_curve.last().unwrap(), 1_000_001.0);
    assert!(output
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("leaves an open position")));
}

#[test]
fn applies_direction_aware_fees_and_validated_input_overrides() {
    let source = "//@version=6\nstrategy(\"Short costs\", default_qty_type=strategy.fixed, default_qty_value=1)\nint hold = input.int(1, \"Hold bars\", minval=1, maxval=3)\nif bar_index == 0\n    strategy.entry(\"Short\", strategy.short)\nif strategy.position_size < 0 and bar_index - strategy.opentrades.entry_bar_index(0) >= hold\n    strategy.close(\"Short\", immediately=true)";
    let inputs = HashMap::from([("Hold bars".to_string(), serde_json::json!(1))]);
    let output = run_strategy_with_inputs(
        source,
        &candles(&[
            (10.0, 10.5, 9.5, 10.0),
            (11.0, 12.0, 10.5, 11.5),
            (12.0, 13.0, 11.5, 12.0),
        ]),
        0.1,
        0.2,
        &inputs,
    )
    .expect("supported short and input controls should run");
    assert_eq!(output.closed_trades, 1);
    assert_eq!(output.trades[0].direction, "SHORT");
    assert!((output.trades[0].net_pnl + 1.034).abs() < 1e-9);
    assert_eq!(output.input_overrides, inputs);

    let invalid = HashMap::from([("Hold bars".to_string(), serde_json::json!(4))]);
    let result = run_strategy_with_inputs(
        source,
        &candles(&[(10.0, 10.5, 9.5, 10.0)]),
        0.0,
        0.0,
        &invalid,
    )
    .expect_err("out-of-range input overrides must be rejected");
    assert!(result
        .iter()
        .any(|diagnostic| diagnostic.message.contains("outside its declared range")));
}

#[test]
fn carries_var_assignments_declared_inside_conditional_blocks() {
    let source = "//@version=6\nstrategy(\"Local var\")\nif bar_index == 0\n    var int count = 0\n    count += 1\nplot(count)";
    let output = run_strategy(
        source,
        &candles(&[(10.0, 10.5, 9.5, 10.0), (11.0, 12.0, 10.5, 12.0)]),
        0.0,
        0.0,
    )
    .expect("nested persistent state should run");
    assert_eq!(output.plots[0].values, vec![Some(1.0), Some(1.0)]);
}

#[test]
fn var_and_varip_carry_between_historical_chart_bars() {
    let source = "//@version=6\nstrategy(\"Declaration modes\")\nvar int retained = 0\nvarip int refreshed = 0\nretained += 1\nrefreshed += 1\nplot(retained, title=\"var\")\nplot(refreshed, title=\"varip\")";
    let output = run_strategy(
        source,
        &candles(&[
            (10.0, 10.5, 9.5, 10.0),
            (11.0, 12.0, 10.5, 12.0),
            (12.0, 12.5, 11.5, 12.0),
        ]),
        0.0,
        0.0,
    )
    .expect("Pine variable declaration modes should remain distinct");
    assert_eq!(
        output.plots[0].values,
        vec![Some(1.0), Some(2.0), Some(3.0)]
    );
    assert_eq!(
        output.plots[1].values,
        vec![Some(1.0), Some(2.0), Some(3.0)]
    );
}

#[test]
fn indicator_runtime_refuses_strategy_declarations() {
    let result = run_indicator(
        "//@version=6\nstrategy(\"Orders\")",
        &candles(&[(10.0, 10.5, 9.5, 10.0)]),
    );
    assert!(result
        .expect_err("indicator runtime must not execute orders")
        .iter()
        .any(|diagnostic| diagnostic
            .message
            .contains("run it with the Pine backtest runtime")));
}

#[test]
fn shipped_nwe_dss_strategy_parses_and_executes_with_bounded_history() {
    let source = include_str!("../examples/nwe_dss_bressert_backtest.pine");
    let validation = validate_source(source);
    assert!(validation.valid, "{:?}", validation.diagnostics);
    let bars = (0..650)
        .map(|index| {
            let close = 100.0 + (index as f64 * 0.07).sin() * 6.0 + (index as f64) * 0.01;
            let open = close - 0.15;
            Candle {
                open,
                high: close + 0.5,
                low: open - 0.5,
                close,
                volume: 1.0,
                time: Some(index as i64),
            }
        })
        .collect::<Vec<_>>();
    let result = run_strategy(source, &bars, 0.0, 0.0).expect("bounded example should execute");
    assert!(result.valid);
    assert_eq!(result.equity_curve.len(), bars.len());
    assert!(result.diagnostics.iter().any(|item| item.message.contains(
        "`calc_on_order_fills` is disabled; fill-triggered recalculation is not modeled."
    )));
}
