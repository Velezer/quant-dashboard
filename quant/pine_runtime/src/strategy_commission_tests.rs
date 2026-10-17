use crate::{run_strategy, Candle, StrategyCommissionType};

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
fn cash_per_contract_commission_is_charged_on_entry_and_exit() {
    let source = "//@version=6\nstrategy(\"Cash commission\", initial_capital=1000, default_qty_type=strategy.fixed, default_qty_value=3, process_orders_on_close=true, commission_type=strategy.commission.cash_per_contract, commission_value=2)\nif bar_index == 0\n    strategy.entry(\"Long\", strategy.long)\nif bar_index == 1\n    strategy.close_all(immediately=true)";
    let output = run_strategy(
        source,
        &candles(&[(10.0, 10.5, 9.5, 10.0), (11.0, 12.0, 10.5, 12.0)]),
        1.0,
        1.0,
    )
    .expect("cash commission should run without venue fee substitution");

    assert_eq!(
        output.commission.map(|value| value.commission_type),
        Some(StrategyCommissionType::CashPerContract)
    );
    assert_eq!(output.commission.map(|value| value.value), Some(2.0));
    assert_eq!(output.closed_trades, 1);
    assert_eq!(output.trades[0].gross_pnl, 6.0);
    assert_eq!(output.trades[0].net_pnl, -6.0);
    assert_eq!(output.net_profit, -6.0);
    assert_eq!(output.equity_curve.last(), Some(&994.0));
}

#[test]
fn cash_per_contract_commission_is_rejected_when_negative() {
    let source = "//@version=6\nstrategy(\"Invalid cash commission\", commission_type=strategy.commission.cash_per_contract, commission_value=-0.1)";
    let error = run_strategy(source, &candles(&[(10.0, 10.5, 9.5, 10.0)]), 0.0, 0.0)
        .expect_err("negative commission cannot be accepted");
    assert!(error
        .iter()
        .any(|item| item.message.contains("finite nonnegative amount")));
}

#[test]
fn cash_per_order_commission_is_charged_for_entry_and_exit_orders_once_each() {
    let source = "//@version=6\nstrategy(\"Cash per order\", initial_capital=1000, default_qty_type=strategy.fixed, default_qty_value=3, process_orders_on_close=true, commission_type=strategy.commission.cash_per_order, commission_value=2)\nif bar_index == 0\n    strategy.entry(\"Long\", strategy.long)\nif bar_index == 1\n    strategy.close_all(immediately=true)";
    let output = run_strategy(
        source,
        &candles(&[(10.0, 10.5, 9.5, 10.0), (11.0, 12.0, 10.5, 12.0)]),
        1.0,
        1.0,
    )
    .expect("cash-per-order commission should run without venue fee substitution");

    assert_eq!(
        output.commission.map(|value| value.commission_type),
        Some(StrategyCommissionType::CashPerOrder)
    );
    assert_eq!(output.commission.map(|value| value.value), Some(2.0));
    assert_eq!(output.closed_trades, 1);
    assert_eq!(output.trades[0].gross_pnl, 6.0);
    assert_eq!(output.trades[0].net_pnl, 2.0);
    assert_eq!(output.net_profit, 2.0);
    assert_eq!(output.equity_curve.last(), Some(&1002.0));
}
