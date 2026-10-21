//! Deterministic historical evidence from the same signal engines used by the
//! live plan builder.
//!
//! This module is deliberately separate from order execution. It consumes a
//! bounded chronological candle slice, enters on the next bar, gives a stop
//! precedence when a bar crosses both levels, and reports observed evidence.
//! It does not make an unvalidated plan actionable.

mod calendar;
mod nwe_dss;
mod entry_score;
mod performance;
mod profile;

use crate::{finite, NANOS_PER_SECOND};

use crate::strategies::{
    round_price, round_price_up, PintuGoldenCrossConfig, StrategyBar,
    PINTU_GOLDEN_CROSS_MIN_R_MULTIPLE,
};
use calendar::local_trading_day;
#[cfg(test)]
use performance::quality_grade;
use performance::{calculate_performance, EvaluatedTrade};
use profile::{resolve_evidence_strategy, EvidenceStrategy};
use serde_json::{json, Value};

pub use entry_score::{evaluate_entry_score_backtest, ENTRY_SCORE_VALIDATION_PROTOCOL_ID};
pub use nwe_dss::{evaluate_nwe_dss_exit_selection, NWE_DSS_EXIT_CANDIDATES_BARS};
pub use profile::{
    historical_evidence_profile, HistoricalEvidenceProfile,
    REQUIRED_SIGNAL_CONFIRMATION_EVIDENCE_SUFFIX, RETIRED_SIGNAL_CONFIRMATION_EVIDENCE_SUFFIX,
};

pub const MIN_DAY_TRADE_WIN_RATE_PCT: f64 = 50.0;
pub const MIN_DAY_TRADE_PROFIT_FACTOR: f64 = 1.2;
pub const MIN_SWING_CAGR_PCT: f64 = 100.0;
pub const MIN_STOCKBIT_DAY_TRADE_CAGR_PCT: f64 = 200.0;
pub const MIN_MULTIBAGGER_CALENDAR_MONTH_RETURN_PCT: f64 = 100.0;
/// Compatibility name retained for consumers of the old asymmetric policy.
/// Both policies now use the user's 50% minimum win-rate goal.
pub const MIN_ASYMMETRIC_DAY_TRADE_WIN_RATE_PCT: f64 = 50.0;
/// A 1.5:1 observed average win/loss is the minimum payoff asymmetry needed
/// to use the lower day-trade win-rate floor.
pub const MIN_ASYMMETRIC_REWARD_RISK: f64 = 1.5;
/// Minimum chronological market-history span for a validated CAGR result.
///
/// The storage layer uses the same contract for every timeframe. Intraday
/// providers may not be able to supply this window in one request, so their
/// evidence remains explicitly insufficient until local/native history has
/// accumulated the full span.
pub const MIN_EVIDENCE_SAMPLE_DAYS: i64 = 730;
/// Minimum completed-trade count used by the shared historical evidence gate.
pub const MIN_EVIDENCE_SAMPLE_TRADES: usize = 10;
const FIFTEEN_MINUTE_INTERVAL_NS: i64 = 15 * crate::SECONDS_PER_MINUTE * NANOS_PER_SECOND;

/// Return the required observed win rate for a day-trade evidence set.
/// Profit factor, net returns, venue history, and sample requirements remain
/// separate checks; win rate alone does not establish a profitable strategy.
pub fn day_trade_minimum_win_rate_pct(_observed_reward_risk_ratio: Option<f64>) -> f64 {
    MIN_DAY_TRADE_WIN_RATE_PCT
}

/// Return the strategy-owned annualized CAGR promotion target. A zero target
/// means the profile does not require CAGR as a promotion criterion.
pub fn historical_evidence_minimum_cagr_pct(strategy: &str) -> f64 {
    resolve_evidence_strategy(strategy)
        .map(|strategy| strategy.minimum_cagr_pct())
        .unwrap_or(0.0)
}

/// Return the strategy-owned complete-calendar-month promotion target.
pub fn historical_evidence_minimum_calendar_month_return_pct(strategy: &str) -> f64 {
    resolve_evidence_strategy(strategy)
        .map(|strategy| strategy.minimum_calendar_month_return_pct())
        .unwrap_or(0.0)
}

fn net_return_pct(
    entry: f64,
    exit: f64,
    direction: &str,
    buy_fee_pct: f64,
    sell_fee_pct: f64,
) -> f64 {
    if !finite(entry) || !finite(exit) || entry <= 0.0 || exit <= 0.0 {
        return f64::NAN;
    }
    let gross = if direction == "SHORT" {
        (entry - exit) / entry
    } else {
        (exit - entry) / entry
    };
    let (entry_fee_pct, exit_fee_pct) = if direction == "SHORT" {
        (sell_fee_pct, buy_fee_pct)
    } else {
        (buy_fee_pct, sell_fee_pct)
    };
    crate::venue_rules::net_return_after_transaction_fees(
        gross * 100.0,
        exit / entry,
        entry_fee_pct,
        exit_fee_pct,
    )
}

/// Evaluate a historical candle window using a registered strategy.
///
/// `max_hold_bars` is an explicit research timeout. It bounds the simulation
/// and makes the result reproducible for strategies whose live exit is signal
/// driven. `fee_per_side_pct` is the documented venue estimate, not a claim
/// about provider depth, funding, or fills.
pub fn evaluate_historical_evidence(
    bars: &[StrategyBar],
    strategy: &str,
    max_hold_bars: usize,
    fee_per_side_pct: f64,
) -> Value {
    evaluate_historical_evidence_with_fees(
        bars,
        strategy,
        max_hold_bars,
        fee_per_side_pct,
        fee_per_side_pct,
    )
}

/// Evaluate history with separate published buy-side and sell-side charges.
/// Existing callers can keep using the symmetric compatibility wrapper above.
pub fn evaluate_historical_evidence_with_fees(
    bars: &[StrategyBar],
    strategy: &str,
    max_hold_bars: usize,
    buy_fee_pct: f64,
    sell_fee_pct: f64,
) -> Value {
    if bars.len() < 2 {
        return json!({
            "trades": 0,
            "wins": 0,
            "losses": 0,
            "win_rate_pct": null,
            "profit_factor": null,
            "minimum_win_rate_pct": null,
            "average_win_pct": null,
            "average_loss_pct": null,
            "observed_reward_risk_ratio": null,
            "break_even_win_rate_pct": null,
            "asymmetric_reward_risk": false,
            "total_pnl": 0.0,
            "compounded_return_pct": 0.0,
            "quality_grade": "C",
            "history_bars": bars.len(),
            "sample_days": 0,
            "minimum_sample_days": MIN_EVIDENCE_SAMPLE_DAYS,
            "cagr_pct": null,
            "cagr_sample_sufficient": false,
            "cagr_validated": false,
            "cagr_validation_status": "OUT_OF_SAMPLE_REQUIRED",
            "evidence_status": "INSUFFICIENT_HISTORY",
            "execution_model": "next_bar_open_stop_first"
        });
    }

    let Some(evidence_strategy) = resolve_evidence_strategy(strategy) else {
        return json!({
            "trades": 0,
            "wins": 0,
            "losses": 0,
            "win_rate_pct": null,
            "profit_factor": null,
            "minimum_win_rate_pct": null,
            "average_win_pct": null,
            "average_loss_pct": null,
            "observed_reward_risk_ratio": null,
            "break_even_win_rate_pct": null,
            "asymmetric_reward_risk": false,
            "total_pnl": 0.0,
            "compounded_return_pct": 0.0,
            "quality_grade": "C",
            "history_bars": bars.len(),
            "sample_days": 0,
            "minimum_sample_days": MIN_EVIDENCE_SAMPLE_DAYS,
            "cagr_pct": null,
            "cagr_sample_sufficient": false,
            "cagr_validated": false,
            "cagr_validation_status": "OUT_OF_SAMPLE_REQUIRED",
            "evidence_status": "UNSUPPORTED_STRATEGY",
            "execution_model": "next_bar_open_stop_first"
        });
    };
    let signals = evidence_strategy.generate_signals(bars);
    let strategy_key = evidence_strategy.canonical_id();
    let timeout = max_hold_bars.max(1);
    let is_pintu_golden_cross = evidence_strategy == EvidenceStrategy::PintuGoldenCross;
    let golden_cross_config = PintuGoldenCrossConfig::default();
    let mut index = 0usize;
    // Store the trade-level return and stop distance separately. Compounding
    // then sizes each sequential trade with the same fixed-fractional policy
    // used by an execution simulator instead of treating each price move as a full
    // account return.
    let mut returns = Vec::new();

    while index + 1 < bars.len() && index < signals.len() {
        let signal = &signals[index];
        if signal.direction == "NEUTRAL"
            || !finite(signal.suggested_sl)
            || !finite(signal.suggested_tp1)
            || signal.suggested_sl <= 0.0
            || signal.suggested_tp1 <= 0.0
        {
            index += 1;
            continue;
        }

        let entry_index = index + 1;
        let signal_day = local_trading_day(bars[index].time);
        let entry_day = local_trading_day(bars[entry_index].time);
        if evidence_strategy.is_stockbit_intraday() && entry_day != signal_day {
            index += 1;
            continue;
        }
        if evidence_strategy.requires_contiguous_intraday_entry()
            && bars[entry_index].time.saturating_sub(bars[index].time) != FIFTEEN_MINUTE_INTERVAL_NS
        {
            // These research candidates require the next contiguous 15-minute
            // candle. Never assume a fill through a missing bar or session gap.
            index += 1;
            continue;
        }
        let entry = bars[entry_index].open;
        let long = signal.direction == "LONG";
        let stop_price = if is_pintu_golden_cross {
            round_price(entry * (1.0 - golden_cross_config.stop_loss_pct))
        } else {
            signal.suggested_sl
        };
        let target_price = if is_pintu_golden_cross {
            round_price_up(
                entry
                    + (entry - stop_price)
                        * golden_cross_config
                            .target_r_multiple
                            .max(PINTU_GOLDEN_CROSS_MIN_R_MULTIPLE),
            )
        } else {
            signal.suggested_tp1
        };
        let valid_levels = if long {
            stop_price < entry && target_price > entry
        } else {
            stop_price > entry && target_price < entry
        };
        if !finite(entry) || entry <= 0.0 || !valid_levels {
            index += 1;
            continue;
        }
        if evidence_strategy.is_stockbit_intraday_research()
            && (entry - signal.suggested_sl) / entry > 0.035
        {
            // Reject opening gaps that make the executed stop distance exceed
            // the strategy's maximum risk even when the signal-close risk fit.
            index += 1;
            continue;
        }

        let final_index = (entry_index + timeout).min(bars.len() - 1);
        let mut exit_index = final_index;
        let mut exit_price = bars[final_index].close;

        for (bar_index, bar) in bars
            .iter()
            .enumerate()
            .take(final_index + 1)
            .skip(entry_index)
        {
            if evidence_strategy.is_stockbit_intraday() && local_trading_day(bar.time) != signal_day
            {
                exit_index = bar_index.saturating_sub(1);
                exit_price = bars[exit_index].close;
                break;
            }
            let stop_hit = if long {
                bar.low <= stop_price
            } else {
                bar.high >= stop_price
            };
            let target_hit = if long {
                bar.high >= target_price
            } else {
                bar.low <= target_price
            };
            // Conservative bar-only ordering: stop wins when both are crossed.
            if stop_hit {
                exit_index = bar_index;
                exit_price = stop_price;
                break;
            }
            if target_hit {
                exit_index = bar_index;
                exit_price = target_price;
                break;
            }
            if evidence_strategy.has_tsmom_exit()
                && signals[bar_index].exit_signal.as_deref() == Some("TSMOM_TREND_EXIT")
            {
                exit_index = bar_index;
                exit_price = bar.close;
                break;
            }
            if evidence_strategy.is_stockbit_intraday()
                && signals[bar_index]
                    .exit_signal
                    .as_deref()
                    .is_some_and(|value| {
                        value.contains("TIME_STOP") || value.contains("SESSION_EXIT")
                    })
            {
                exit_index = bar_index;
                exit_price = bar.close;
                break;
            }
        }

        let result = net_return_pct(
            entry,
            exit_price,
            &signal.direction,
            buy_fee_pct,
            sell_fee_pct,
        );
        if finite(result) {
            returns.push(EvaluatedTrade {
                return_pct: result,
                stop_risk_fraction: ((entry - stop_price).abs() / entry).max(f64::EPSILON),
                entry_index,
                exit_index,
                entry_price: entry,
                exit_price,
                is_long: long,
            });
        }

        // A timeout closes on the final observed bar. A resolved trade closes
        // on the first stop/target bar. Both preserve chronological ordering.
        index = exit_index.saturating_add(1);
    }

    calculate_performance(
        bars,
        &returns,
        strategy_key,
        buy_fee_pct,
        sell_fee_pct,
        evidence_strategy.is_day_trade(),
    )
}
#[cfg(test)]
#[path = "evidence_tests.rs"]
mod tests;
