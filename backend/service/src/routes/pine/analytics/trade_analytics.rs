use super::*;

pub(crate) fn summarize_strategy_trades(
    trades: &[StrategyTrade],
    direction: Option<&str>,
    use_net: bool,
) -> serde_json::Value {
    let selected = trades
        .iter()
        .filter(|trade| direction.is_none_or(|side| trade.direction == side))
        .collect::<Vec<_>>();
    let pnl = |trade: &StrategyTrade| {
        if use_net {
            trade.net_pnl
        } else {
            trade.gross_pnl
        }
    };
    let winners = selected
        .iter()
        .filter(|trade| pnl(trade) > 0.0)
        .collect::<Vec<_>>();
    let losers = selected
        .iter()
        .filter(|trade| pnl(trade) < 0.0)
        .collect::<Vec<_>>();
    let total = selected.len();
    let average_return = |items: &[&&StrategyTrade]| {
        (!items.is_empty()).then(|| {
            items
                .iter()
                .map(|trade| {
                    if use_net {
                        trade.net_return_pct
                    } else {
                        trade.gross_return_pct
                    }
                })
                .sum::<f64>()
                / items.len() as f64
        })
    };
    let excursion_summary = |field: fn(&StrategyTrade) -> f64, items: &[&StrategyTrade]| {
        let values = items
            .iter()
            .map(|trade| field(trade))
            .filter(|value| value.is_finite())
            .collect::<Vec<_>>();
        serde_json::json!({
            "average": (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64),
            "maximum": values.iter().copied().max_by(f64::total_cmp),
            "sample_trades": values.len()
        })
    };
    let max_favorable_excursion = excursion_summary(|trade| trade.max_runup, &selected);
    let max_adverse_excursion = excursion_summary(|trade| trade.max_drawdown, &selected);
    let average_favorable_excursion_pct =
        excursion_summary(|trade| trade.max_runup_percent, &selected);
    let average_adverse_excursion_pct =
        excursion_summary(|trade| trade.max_drawdown_percent, &selected);
    serde_json::json!({
        "trades": total,
        "wins": winners.len(),
        "losses": losers.len(),
        "breakeven": total.saturating_sub(winners.len() + losers.len()),
        "win_rate_pct": (total > 0).then_some(winners.len() as f64 / total as f64 * 100.0),
        "average_win_return_pct": average_return(&winners),
        "average_loss_return_pct": average_return(&losers),
        "largest_win_pnl": winners.iter().map(|trade| pnl(trade)).max_by(f64::total_cmp),
        "largest_loss_pnl": losers.iter().map(|trade| pnl(trade)).min_by(f64::total_cmp),
        "max_favorable_excursion": max_favorable_excursion,
        "max_adverse_excursion": max_adverse_excursion,
        "favorable_excursion_pct": average_favorable_excursion_pct,
        "adverse_excursion_pct": average_adverse_excursion_pct
    })
}

pub(crate) fn summarize_trade_return_distribution(
    trades: &[StrategyTrade],
    use_net: bool,
) -> serde_json::Value {
    let returns = trades
        .iter()
        .map(|trade| {
            if use_net {
                trade.net_return_pct
            } else {
                trade.gross_return_pct
            }
        })
        .filter(|value| value.is_finite())
        .collect::<Vec<_>>();
    if returns.is_empty() {
        return serde_json::json!({
            "available": false,
            "sample_trades": 0,
            "excluded_nonfinite_returns": trades.len(),
            "basis": if use_net { "net after configured costs" } else { "gross; venue fees unknown" },
            "bins": []
        });
    }
    let minimum = returns
        .iter()
        .copied()
        .min_by(f64::total_cmp)
        .unwrap_or_default();
    let maximum = returns
        .iter()
        .copied()
        .max_by(f64::total_cmp)
        .unwrap_or_default();
    if !(maximum - minimum).is_finite() {
        return serde_json::json!({
            "available": false,
            "sample_trades": returns.len(),
            "excluded_nonfinite_returns": trades.len() - returns.len(),
            "basis": if use_net { "net after configured costs" } else { "gross; venue fees unknown" },
            "reason": "Finite return range exceeds the supported histogram numeric range",
            "bins": []
        });
    }
    let bin_count = if minimum == maximum {
        1
    } else {
        (returns.len() as f64).sqrt().ceil().clamp(1.0, 12.0) as usize
    };
    let width = (maximum - minimum) / bin_count as f64;
    let mut counts = vec![0usize; bin_count];
    for value in &returns {
        let index = if width == 0.0 {
            0
        } else {
            (((value - minimum) / width).floor() as usize).min(bin_count - 1)
        };
        counts[index] += 1;
    }
    let bins = counts.into_iter().enumerate().map(|(index, count)| {
        let lower = if width == 0.0 { minimum } else { minimum + width * index as f64 };
        let upper = if width == 0.0 || index + 1 == bin_count { maximum } else { minimum + width * (index + 1) as f64 };
        serde_json::json!({"lower_return_pct": lower, "upper_return_pct": upper, "count": count, "includes_upper_bound": index + 1 == bin_count})
    }).collect::<Vec<_>>();
    serde_json::json!({
        "available": true,
        "sample_trades": returns.len(),
        "excluded_nonfinite_returns": trades.len() - returns.len(),
        "minimum_return_pct": minimum,
        "maximum_return_pct": maximum,
        "basis": if use_net { "net after configured costs" } else { "gross; venue fees unknown" },
        "binning": "equal-width return bins; bin count is ceil(sqrt(sample size)), bounded to 1 through 12",
        "bins": bins
    })
}

pub(crate) fn summarize_close_drawdown(
    initial_capital: f64,
    equity: &[f64],
    times: &[i64],
) -> serde_json::Value {
    if !initial_capital.is_finite()
        || initial_capital <= 0.0
        || equity.is_empty()
        || equity.len() != times.len()
        || equity.iter().any(|value| !value.is_finite())
    {
        return serde_json::json!({"available": false, "series": []});
    }
    let mut peak_equity = initial_capital;
    let mut peak_index = 0usize;
    let mut max_drawdown_pct = 0.0f64;
    let mut max_drawdown_amount = 0.0f64;
    let mut max_peak_index = 0usize;
    let mut max_trough_index = 0usize;
    let mut max_recovery_index = None;
    let mut current_drawdown_pct = 0.0f64;
    let mut current_duration_bars = 0usize;
    let mut drawdown_active = false;
    let mut completed_drawdown_periods = 0usize;
    let mut completed_drawdown_duration_bars = 0usize;
    let sample_indexes = backtest_curve_sample_indexes(equity.len());
    let mut sample_indexes = sample_indexes.into_iter().peekable();
    let mut series = Vec::with_capacity(sample_indexes.len());

    for (index, value) in equity.iter().copied().enumerate() {
        if value >= peak_equity {
            if drawdown_active {
                completed_drawdown_periods += 1;
                completed_drawdown_duration_bars += index.saturating_sub(peak_index);
                drawdown_active = false;
            }
            if max_recovery_index.is_none()
                && peak_index == max_peak_index
                && index > max_peak_index
                && value >= peak_equity
                && max_drawdown_pct > 0.0
            {
                max_recovery_index = Some(index);
            }
            peak_equity = value;
            peak_index = index;
            current_drawdown_pct = 0.0;
            current_duration_bars = 0;
        } else {
            drawdown_active = true;
            let amount = peak_equity - value;
            let drawdown_pct = amount / peak_equity * 100.0;
            current_drawdown_pct = drawdown_pct;
            current_duration_bars = index.saturating_sub(peak_index);
            if drawdown_pct > max_drawdown_pct {
                max_drawdown_pct = drawdown_pct;
                max_drawdown_amount = amount;
                max_peak_index = peak_index;
                max_trough_index = index;
                max_recovery_index = None;
            }
        }
        if sample_indexes.peek() == Some(&index) {
            series.push(
                serde_json::json!({"time": times[index], "drawdown_pct": current_drawdown_pct}),
            );
            sample_indexes.next();
        }
    }

    serde_json::json!({
        "available": true,
        "sampling": "candle-close equity versus the prior close-to-close peak, including initial capital",
        "max_drawdown_pct": max_drawdown_pct,
        "max_drawdown_amount": max_drawdown_amount,
        "peak_time": times[max_peak_index],
        "trough_time": times[max_trough_index],
        "recovery_time": max_recovery_index.map(|index| times[index]),
        "duration_bars": max_trough_index.saturating_sub(max_peak_index),
        "recovery_bars": max_recovery_index.map(|index| index.saturating_sub(max_trough_index)),
        "max_period_duration_bars": max_recovery_index.unwrap_or(equity.len() - 1).saturating_sub(max_peak_index),
        "completed_drawdown_periods": completed_drawdown_periods,
        "average_recovered_drawdown_duration_bars": (completed_drawdown_periods > 0).then(|| completed_drawdown_duration_bars as f64 / completed_drawdown_periods as f64),
        "current_drawdown_pct": current_drawdown_pct,
        "current_duration_bars": current_duration_bars,
        "series": series
    })
}
