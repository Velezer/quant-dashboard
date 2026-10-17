use crate::risk::{drawdown_policy, MAX_CAPITAL_PER_TRADE_PCT, MAX_RISK_PER_TRADE_PCT};
use crate::strategies::StrategyBar;
use serde_json::{json, Value};

use super::{
    calendar::maximum_completed_calendar_month_return, MIN_EVIDENCE_SAMPLE_DAYS,
    MIN_EVIDENCE_SAMPLE_TRADES,
};

const PROFIT_FACTOR_GRADE_A_MINIMUM: f64 = 1.5;
const PROFIT_FACTOR_GRADE_B_MINIMUM: f64 = 1.2;
const DEFAULT_QUALITY_GRADE_MINIMUM_WIN_RATE_PCT: f64 = 55.0;
const TOTAL_PNL_BASIS: &str =
    "sum of per-trade net price-return percentages; account growth is reported separately";
const COMPOUNDING_BASIS: &str =
    "sequential fixed-fractional sizing; 2% risk budget, 25% max capital per trade, 5% defensive drawdown, 10% trading halt";
const IN_SAMPLE_CAGR_BASIS: &str =
    "risk-sized in-sample estimate after reported venue fees; no untouched holdout supplied";
const CALENDAR_MONTH_RETURN_BASIS: &str =
    "maximum fee-adjusted risk-sized account-equity return for a complete local calendar month; leading and trailing partial months excluded";
const EXECUTION_MODEL: &str = "next_bar_open_stop_first";

#[derive(Clone, Copy)]
pub(super) struct EvaluatedTrade {
    pub(super) return_pct: f64,
    pub(super) stop_risk_fraction: f64,
    pub(super) entry_index: usize,
    pub(super) exit_index: usize,
    pub(super) entry_price: f64,
    pub(super) exit_price: f64,
    pub(super) is_long: bool,
}

struct PortfolioPerformance {
    trades: usize,
    wins: usize,
    losses: usize,
    flat_trades: usize,
    win_rate_pct: Option<f64>,
    profit_factor: Option<f64>,
    average_win_pct: Option<f64>,
    average_loss_pct: Option<f64>,
    observed_reward_risk_ratio: Option<f64>,
    break_even_win_rate_pct: Option<f64>,
    asymmetric_reward_risk: bool,
    is_day_trade_strategy: bool,
    buy_fee_pct: f64,
    sell_fee_pct: f64,
    minimum_win_rate_pct: Option<f64>,
    total_pnl: f64,
    equity: f64,
    max_drawdown_fraction: f64,
    risk_sized_trade_count: usize,
    halted_trade_count: usize,
    sample_days: i64,
    first_ts: i64,
    last_ts: i64,
    cagr_sample_sufficient: bool,
    cagr_pct: Option<f64>,
    max_calendar_month_return_pct: Option<f64>,
    completed_calendar_months: usize,
    quality_grade: &'static str,
}

pub(super) fn calculate_performance(
    bars: &[StrategyBar],
    evaluated_trades: &[EvaluatedTrade],
    strategy_id: &'static str,
    buy_fee_pct: f64,
    sell_fee_pct: f64,
    is_day_trade_strategy: bool,
) -> Value {
    let trades = evaluated_trades.len();
    let total_pnl: f64 = evaluated_trades.iter().map(|trade| trade.return_pct).sum();
    let mut equity = 1.0f64;
    let mut equity_marks = vec![1.0f64; bars.len()];
    let mut peak_equity = 1.0f64;
    let mut max_drawdown_fraction = 0.0f64;
    let mut halted_trade_count = 0usize;
    let mut risk_sized_trade_count = 0usize;
    let mut last_marked_index = 0usize;
    for trade in evaluated_trades {
        let Some(risk_policy) = drawdown_policy(equity, peak_equity) else {
            halted_trade_count += 1;
            continue;
        };
        if !risk_policy.allowed {
            halted_trade_count += 1;
            continue;
        }
        let exposure_fraction = (MAX_RISK_PER_TRADE_PCT * risk_policy.risk_scale
            / trade.stop_risk_fraction)
            .min(MAX_CAPITAL_PER_TRADE_PCT);
        for mark_index in last_marked_index..trade.entry_index {
            equity_marks[mark_index] = equity;
        }
        let entry_fee_pct = if trade.is_long {
            buy_fee_pct
        } else {
            sell_fee_pct
        };
        let exit_fee_pct = if trade.is_long {
            sell_fee_pct
        } else {
            buy_fee_pct
        };
        for mark_index in trade.entry_index..=trade.exit_index {
            let mark_price = if mark_index == trade.exit_index {
                trade.exit_price
            } else {
                bars[mark_index].close
            };
            let gross_mark_return_pct = if trade.is_long {
                (mark_price - trade.entry_price) / trade.entry_price * 100.0
            } else {
                (trade.entry_price - mark_price) / trade.entry_price * 100.0
            };
            let net_mark_return_pct = if mark_index == trade.exit_index {
                trade.return_pct
            } else {
                crate::venue_rules::net_return_after_transaction_fees(
                    gross_mark_return_pct,
                    mark_price / trade.entry_price,
                    entry_fee_pct,
                    exit_fee_pct,
                )
            };
            equity_marks[mark_index] =
                equity * (1.0 + exposure_fraction * net_mark_return_pct / 100.0);
        }
        let portfolio_return = exposure_fraction * trade.return_pct / 100.0;
        if !portfolio_return.is_finite() || 1.0 + portfolio_return <= 0.0 {
            equity = 0.0;
            equity_marks[trade.exit_index] = equity;
            last_marked_index = trade.exit_index.saturating_add(1);
            halted_trade_count += 1;
            continue;
        }
        equity *= 1.0 + portfolio_return;
        equity_marks[trade.exit_index] = equity;
        last_marked_index = trade.exit_index.saturating_add(1);
        risk_sized_trade_count += 1;
        peak_equity = peak_equity.max(equity);
        let realized_drawdown = if peak_equity > 0.0 {
            ((peak_equity - equity) / peak_equity).max(0.0)
        } else {
            1.0
        };
        max_drawdown_fraction = max_drawdown_fraction.max(realized_drawdown);
    }
    for mark_index in last_marked_index..equity_marks.len() {
        equity_marks[mark_index] = equity;
    }

    let (max_calendar_month_return_pct, completed_calendar_months) =
        maximum_completed_calendar_month_return(bars, &equity_marks);
    let first_ts = bars.first().map(|bar| bar.time).unwrap_or(0);
    let last_ts = bars.last().map(|bar| bar.time).unwrap_or(first_ts);
    let sample_days = (last_ts.saturating_sub(first_ts).max(0) / crate::NANOS_PER_DAY).max(0);
    let wins = evaluated_trades
        .iter()
        .filter(|trade| trade.return_pct > 0.0)
        .count();
    let losses = evaluated_trades
        .iter()
        .filter(|trade| trade.return_pct < 0.0)
        .count();
    let flat_trades = trades - wins - losses;
    let gross_win: f64 = evaluated_trades
        .iter()
        .filter(|trade| trade.return_pct > 0.0)
        .map(|trade| trade.return_pct)
        .sum();
    let gross_loss: f64 = evaluated_trades
        .iter()
        .filter(|trade| trade.return_pct < 0.0)
        .map(|trade| trade.return_pct.abs())
        .sum();
    let win_rate_pct = (trades > 0).then(|| (wins as f64 / trades as f64) * 100.0);
    let profit_factor = if gross_loss > 0.0 {
        Some(gross_win / gross_loss)
    } else if gross_win > 0.0 {
        Some(99.0)
    } else {
        None
    };
    let average_win_pct = (wins > 0).then(|| gross_win / wins as f64);
    let average_loss_pct = (losses > 0 && gross_loss > 0.0).then(|| gross_loss / losses as f64);
    let observed_reward_risk_ratio = average_win_pct
        .zip(average_loss_pct)
        .filter(|(win, loss)| win.is_finite() && loss.is_finite() && *win > 0.0 && *loss > 0.0)
        .map(|(win, loss)| win / loss);
    let break_even_win_rate_pct = observed_reward_risk_ratio.map(|ratio| 100.0 / (1.0 + ratio));
    let minimum_win_rate_pct = is_day_trade_strategy
        .then(|| super::day_trade_minimum_win_rate_pct(observed_reward_risk_ratio));
    let asymmetric_reward_risk =
        observed_reward_risk_ratio.is_some_and(|ratio| ratio >= super::MIN_ASYMMETRIC_REWARD_RISK);
    let cagr_sample_sufficient = sample_days >= MIN_EVIDENCE_SAMPLE_DAYS
        && trades >= MIN_EVIDENCE_SAMPLE_TRADES
        && equity.is_finite()
        && equity > 0.0;
    let cagr_pct =
        cagr_sample_sufficient.then(|| (equity.powf(365.0 / sample_days as f64) - 1.0) * 100.0);
    let quality_grade = quality_grade(
        trades,
        win_rate_pct.unwrap_or(0.0),
        profit_factor.unwrap_or(0.0),
        minimum_win_rate_pct.unwrap_or(DEFAULT_QUALITY_GRADE_MINIMUM_WIN_RATE_PCT),
    );

    PortfolioPerformance {
        trades,
        wins,
        losses,
        flat_trades,
        win_rate_pct,
        profit_factor,
        average_win_pct,
        average_loss_pct,
        observed_reward_risk_ratio,
        break_even_win_rate_pct,
        asymmetric_reward_risk,
        is_day_trade_strategy,
        buy_fee_pct,
        sell_fee_pct,
        minimum_win_rate_pct,
        total_pnl,
        equity,
        max_drawdown_fraction,
        risk_sized_trade_count,
        halted_trade_count,
        sample_days,
        first_ts,
        last_ts,
        cagr_sample_sufficient,
        cagr_pct,
        max_calendar_month_return_pct,
        completed_calendar_months,
        quality_grade,
    }
    .to_value(bars.len(), strategy_id, MIN_EVIDENCE_SAMPLE_DAYS)
}

impl PortfolioPerformance {
    fn to_value(
        self,
        history_bars: usize,
        strategy_id: &'static str,
        minimum_sample_days: i64,
    ) -> Value {
        json!({
            "trades": self.trades,
            "wins": self.wins,
            "losses": self.losses,
            "flat_trades": self.flat_trades,
            "win_rate_pct": self.win_rate_pct,
            "profit_factor": self.profit_factor,
            "minimum_win_rate_pct": self.minimum_win_rate_pct,
            "average_win_pct": self.average_win_pct,
            "average_loss_pct": self.average_loss_pct,
            "observed_reward_risk_ratio": self.observed_reward_risk_ratio,
            "break_even_win_rate_pct": self.break_even_win_rate_pct,
            "asymmetric_reward_risk": self.asymmetric_reward_risk,
            "risk_reward_policy": if self.is_day_trade_strategy {
                "MINIMUM_50_PCT_WR"
            } else {
                "NOT_APPLICABLE"
            },
            "total_pnl": self.total_pnl,
            "total_pnl_basis": TOTAL_PNL_BASIS,
            "compounded_return_pct": (self.equity - 1.0) * 100.0,
            "compounding_basis": COMPOUNDING_BASIS,
            "risk_sized_trade_count": self.risk_sized_trade_count,
            "halted_trade_count": self.halted_trade_count,
            "max_drawdown_pct": self.max_drawdown_fraction * 100.0,
            "quality_grade": self.quality_grade,
            "history_bars": history_bars,
            "sample_days": self.sample_days,
            "minimum_sample_days": minimum_sample_days,
            "history_start_ts": self.first_ts,
            "history_end_ts": self.last_ts,
            "cagr_pct": self.cagr_pct,
            "cagr_sample_sufficient": self.cagr_sample_sufficient,
            "cagr_validated": false,
            "cagr_validation_status": "OUT_OF_SAMPLE_REQUIRED",
            "cagr_basis": IN_SAMPLE_CAGR_BASIS,
            "max_calendar_month_return_pct": self.max_calendar_month_return_pct,
            "completed_calendar_months": self.completed_calendar_months,
            "calendar_month_return_validated": false,
            "calendar_month_return_validation_status": "OUT_OF_SAMPLE_REQUIRED",
            "calendar_month_return_basis": CALENDAR_MONTH_RETURN_BASIS,
            "evidence_status": if self.sample_days < minimum_sample_days {
                "INSUFFICIENT_HISTORY"
            } else if self.trades > 0 {
                "OBSERVED"
            } else {
                "NO_STRATEGY_TRADES"
            },
            "strategy": strategy_id,
            "fee_per_side_pct": self.buy_fee_pct.max(self.sell_fee_pct),
            "buy_fee_pct": self.buy_fee_pct,
            "sell_fee_pct": self.sell_fee_pct,
            "round_trip_fee_pct": self.buy_fee_pct + self.sell_fee_pct,
            "execution_model": EXECUTION_MODEL
        })
    }
}

pub(super) fn quality_grade(
    trades: usize,
    win_rate_pct: f64,
    profit_factor: f64,
    minimum_win_rate_pct: f64,
) -> &'static str {
    if trades >= 3
        && win_rate_pct >= minimum_win_rate_pct
        && profit_factor >= PROFIT_FACTOR_GRADE_A_MINIMUM
    {
        "A"
    } else if trades >= 3
        && win_rate_pct >= minimum_win_rate_pct
        && profit_factor >= PROFIT_FACTOR_GRADE_B_MINIMUM
    {
        "B"
    } else {
        "C"
    }
}
