use super::*;

pub(super) fn summarize_chronological_validation(
    source: &str,
    training_percent: u8,
    candles: &[Candle],
    runtime_candles: &[quant_pine_runtime::Candle],
    timeframe: &str,
    timeframe_candles: &HashMap<String, Vec<quant_pine_runtime::Candle>>,
    magnifier_bars: &[quant_pine_runtime::Candle],
    magnifier_timeframe: Option<&str>,
    history_limit: usize,
    buy_fee_pct: f64,
    sell_fee_pct: f64,
    inputs: &HashMap<String, serde_json::Value>,
    tick_size: Option<f64>,
    fees_known: bool,
    full_output: &quant_pine_runtime::StrategyOutput,
    progress: Option<&(dyn Fn(usize, usize) -> bool + Sync)>,
) -> serde_json::Value {
    let split_index = candles.len().saturating_mul(training_percent as usize) / 100;
    if split_index == 0 || split_index >= candles.len() {
        return serde_json::json!({
            "available": false,
            "training_percent": training_percent,
            "reason": "The selected history is too short to create both training and validation bars."
        });
    }
    let training_curve = &full_output.equity_curve[..split_index];
    let training_times = candles[..split_index]
        .iter()
        .map(|candle| candle.time)
        .collect::<Vec<_>>();
    let training_trades = full_output
        .trades
        .iter()
        .filter(|trade| trade.exit_bar < split_index)
        .cloned()
        .collect::<Vec<_>>();
    let training_wins = training_trades
        .iter()
        .filter(|trade| {
            if fees_known {
                trade.net_pnl > 0.0
            } else {
                trade.gross_pnl > 0.0
            }
        })
        .count();
    let training_losses = training_trades
        .iter()
        .filter(|trade| {
            if fees_known {
                trade.net_pnl < 0.0
            } else {
                trade.gross_pnl < 0.0
            }
        })
        .count();
    let training_pnl = training_trades
        .iter()
        .map(|trade| {
            if fees_known {
                trade.net_pnl
            } else {
                trade.gross_pnl
            }
        })
        .sum::<f64>();
    let training_gross_profit = training_trades
        .iter()
        .map(|trade| trade.gross_pnl.max(0.0))
        .sum::<f64>();
    let training_gross_loss = training_trades
        .iter()
        .map(|trade| trade.gross_pnl.min(0.0))
        .sum::<f64>();
    let training_profit_factor = if training_gross_loss == 0.0 {
        None
    } else {
        Some(training_gross_profit / training_gross_loss.abs())
    };
    let training_end_equity = training_curve
        .last()
        .copied()
        .unwrap_or(full_output.initial_capital);
    let training_close_drawdown =
        summarize_close_drawdown(full_output.initial_capital, training_curve, &training_times);
    let training_monthly_returns = summarize_monthly_marked_returns(
        full_output.initial_capital,
        training_curve,
        &training_times,
    );
    let cross_boundary_trades = full_output
        .trades
        .iter()
        .filter(|trade| trade.entry_bar < split_index && trade.exit_bar >= split_index)
        .count();
    let validation_output = match quant_pine_runtime::run_strategy_with_history_limit_and_magnifier_and_tick_size_and_evaluation_start_and_progress(
        source,
        runtime_candles,
        timeframe,
        timeframe_candles,
        magnifier_bars,
        magnifier_timeframe,
        history_limit,
        buy_fee_pct,
        sell_fee_pct,
        inputs,
        tick_size,
        split_index,
        progress,
    ) {
        Ok(output) => output,
        Err(diagnostics) => return serde_json::json!({
            "available": false,
            "training_percent": training_percent,
            "training_bars": split_index,
            "validation_bars": candles.len() - split_index,
            "validation_start_time": candles[split_index].time,
            "reason": diagnostics.into_iter().map(|item| item.message).collect::<Vec<_>>().join(" ")
        }),
    };
    let validation_curve = &validation_output.equity_curve[split_index..];
    let validation_times = candles[split_index..]
        .iter()
        .map(|candle| candle.time)
        .collect::<Vec<_>>();
    let validation_close_drawdown = summarize_close_drawdown(
        validation_output.initial_capital,
        validation_curve,
        &validation_times,
    );
    let validation_monthly_returns = summarize_monthly_marked_returns(
        validation_output.initial_capital,
        validation_curve,
        &validation_times,
    );
    let validation_end_equity = validation_curve
        .last()
        .copied()
        .unwrap_or(validation_output.initial_capital);
    let validation_return_pct =
        (validation_end_equity / validation_output.initial_capital - 1.0) * 100.0;
    let validation_pnl = if fees_known {
        validation_output.net_profit
    } else {
        validation_output
            .trades
            .iter()
            .map(|trade| trade.gross_pnl)
            .sum::<f64>()
    };
    serde_json::json!({
        "available": true,
        "training_percent": training_percent,
        "validation_percent": 100 - training_percent,
        "split_index": split_index,
        "training_bars": split_index,
        "validation_bars": candles.len() - split_index,
        "training_start_time": candles.first().map(|candle| candle.time),
        "training_end_time": candles.get(split_index - 1).map(|candle| candle.time),
        "validation_start_time": candles.get(split_index).map(|candle| candle.time),
        "validation_end_time": candles.last().map(|candle| candle.time),
        "pnl_basis": if fees_known { "net after configured costs" } else { "gross; venue fees unknown" },
        "training": {
            "closed_trades": training_trades.len(),
            "wins": training_wins,
            "losses": training_losses,
            "win_rate_pct": (!training_trades.is_empty()).then_some(training_wins as f64 / training_trades.len() as f64 * 100.0),
            "win_rate_interval_95_pct": wilson_interval_95_pct(training_wins, training_trades.len()),
            "closed_trade_pnl": training_pnl,
            "marked_return_pct": (training_end_equity / full_output.initial_capital - 1.0) * 100.0,
            "end_equity": training_end_equity,
            "profit_factor": training_profit_factor,
            "profit_factor_unbounded": training_gross_profit > 0.0 && training_gross_loss == 0.0,
            "close_drawdown": training_close_drawdown,
            "monthly_marked_returns": training_monthly_returns,
            "cross_boundary_trades_excluded": cross_boundary_trades
        },
        "validation": {
            "closed_trades": validation_output.closed_trades,
            "wins": validation_output.wins,
            "win_rate_pct": validation_output.win_rate_pct,
            "win_rate_interval_95_pct": wilson_interval_95_pct(validation_output.wins, validation_output.closed_trades),
            "closed_trade_pnl": validation_pnl,
            "marked_return_pct": validation_return_pct,
            "end_equity": validation_end_equity,
            "profit_factor": validation_output.profit_factor,
            "profit_factor_unbounded": validation_output.profit_factor_unbounded,
            "max_drawdown_pct": validation_output.max_drawdown_initial_capital_percent,
            "close_drawdown": validation_close_drawdown,
            "monthly_marked_returns": validation_monthly_returns,
            "diagnostics": validation_output.diagnostics
        },
        "method": "The same source and inputs run chronologically over the full sample for Pine warm-up. Orders are suppressed before the split; validation starts flat with initial capital. Training trades crossing the split are excluded from training closed-trade statistics.",
        "warning": "This is a single chronological tail check, not an untouched holdout guarantee. Reusing these results to select source code or inputs contaminates the validation period; no execution validation is performed."
    })
}

pub(super) fn wilson_interval_95_pct(wins: usize, trades: usize) -> Option<[f64; 2]> {
    if trades == 0 || wins > trades {
        return None;
    }
    let sample_size = trades as f64;
    let observed_rate = wins as f64 / sample_size;
    let z_score = 1.959_963_984_540_054_f64;
    let z_squared = z_score * z_score;
    let denominator = 1.0 + z_squared / sample_size;
    let center = (observed_rate + z_squared / (2.0 * sample_size)) / denominator;
    let margin = z_score
        * ((observed_rate * (1.0 - observed_rate) / sample_size
            + z_squared / (4.0 * sample_size * sample_size))
            .sqrt())
        / denominator;
    Some([
        (center - margin).max(0.0) * 100.0,
        (center + margin).min(1.0) * 100.0,
    ])
}

pub(super) fn summarize_monthly_marked_returns(
    initial_capital: f64,
    equity: &[f64],
    times: &[i64],
) -> Vec<serde_json::Value> {
    if !initial_capital.is_finite() || initial_capital <= 0.0 || equity.len() != times.len() {
        return Vec::new();
    }
    let mut months = Vec::new();
    let mut active_month: Option<String> = None;
    let mut month_start_equity = initial_capital;
    let mut month_end_equity = initial_capital;
    let mut prior_equity = initial_capital;
    for (equity, time) in equity.iter().zip(times.iter()) {
        if !equity.is_finite() {
            return Vec::new();
        }
        let month = utc_month_key(*time);
        if active_month
            .as_ref()
            .is_some_and(|current| current != &month)
        {
            months.push(serde_json::json!({
                "month": active_month.take(),
                "return_pct": if month_start_equity > 0.0 { Some((month_end_equity / month_start_equity - 1.0) * 100.0) } else { None }
            }));
            month_start_equity = prior_equity;
        }
        if active_month.is_none() {
            active_month = Some(month);
        }
        month_end_equity = *equity;
        prior_equity = *equity;
    }
    if let Some(month) = active_month {
        months.push(serde_json::json!({
            "month": month,
            "return_pct": if month_start_equity > 0.0 { Some((month_end_equity / month_start_equity - 1.0) * 100.0) } else { None }
        }));
    }
    months
}

pub(super) fn utc_month_key(timestamp_ns: i64) -> String {
    let unix_days = timestamp_ns
        .div_euclid(crate::NANOS_PER_SECOND)
        .div_euclid(crate::SECONDS_PER_DAY);
    let (year, month, _) = quant_core::gregorian_date_from_unix_days(unix_days);
    format!("{year:04}-{month:02}")
}

pub(super) fn backtest_curve_sample_indexes(length: usize) -> Vec<usize> {
    if length <= MAX_BACKTEST_CURVE_POINTS {
        return (0..length).collect();
    }
    let last = length - 1;
    (0..MAX_BACKTEST_CURVE_POINTS)
        .map(|point| point * last / (MAX_BACKTEST_CURVE_POINTS - 1))
        .collect()
}

pub(super) fn buy_and_hold_benchmark(initial_capital: f64, closes: &[f64]) -> Option<(f64, f64)> {
    let start_close = *closes.first()?;
    let end_close = *closes.last()?;
    if !initial_capital.is_finite()
        || initial_capital <= 0.0
        || closes
            .iter()
            .any(|close| !close.is_finite() || *close <= 0.0)
        || closes
            .iter()
            .any(|close| !(initial_capital * close / start_close).is_finite())
    {
        return None;
    }
    let return_pct = (end_close / start_close - 1.0) * 100.0;
    if !return_pct.is_finite() {
        return None;
    }
    (initial_capital * end_close / start_close)
        .is_finite()
        .then_some((return_pct, start_close))
}
#[path = "analytics/trade_analytics.rs"]
mod trade_analytics;
pub(super) use trade_analytics::{
    summarize_close_drawdown, summarize_strategy_trades, summarize_trade_return_distribution,
};
