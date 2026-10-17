use crate::strategies::{generate_stockbit_dss_bressert_4h_signals, StrategyBar};
use serde_json::{json, Value};

/// Fixed-bar exit candidates compared on validation data only.
pub const NWE_DSS_EXIT_CANDIDATES_BARS: &[usize] = &[4, 8, 12];
const TRAINING_FRACTION: f64 = 0.60;
const VALIDATION_FRACTION: f64 = 0.20;

#[derive(Clone, Copy, Default)]
struct TradeSummary {
    trades: usize,
    wins: usize,
    net_sum_pct: f64,
    gross_sum_pct: f64,
    positive_returns_pct: f64,
    negative_returns_pct: f64,
    first_time: Option<i64>,
    last_time: Option<i64>,
}

impl TradeSummary {
    fn add(
        &mut self,
        bars: &[StrategyBar],
        entry: usize,
        exit: usize,
        direction: &str,
        buy_fee_pct: f64,
        sell_fee_pct: f64,
        fees_known: bool,
    ) {
        let entry_price = bars[entry].open;
        let exit_price = bars[exit].close;
        if !entry_price.is_finite() || !exit_price.is_finite() || entry_price <= 0.0 {
            return;
        }
        let long = direction == "LONG";
        let gross = if long {
            (exit_price - entry_price) / entry_price * 100.0
        } else {
            (entry_price - exit_price) / entry_price * 100.0
        };
        if !gross.is_finite() {
            return;
        }
        let (entry_fee_pct, exit_fee_pct) = if long {
            (buy_fee_pct, sell_fee_pct)
        } else {
            (sell_fee_pct, buy_fee_pct)
        };
        let net = if fees_known {
            crate::venue_rules::net_return_after_transaction_fees(
                gross,
                exit_price / entry_price,
                entry_fee_pct,
                exit_fee_pct,
            )
        } else {
            gross
        };
        self.trades += 1;
        self.wins += usize::from(net > 0.0);
        self.net_sum_pct += net;
        self.gross_sum_pct += gross;
        if net > 0.0 {
            self.positive_returns_pct += net;
        }
        if net < 0.0 {
            self.negative_returns_pct += net.abs();
        }
        self.first_time.get_or_insert(bars[entry].time);
        self.last_time = Some(bars[exit].time);
    }

    fn mean_net(self) -> Option<f64> {
        (self.trades > 0).then(|| self.net_sum_pct / self.trades as f64)
    }

    fn to_json(self, fees_known: bool) -> Value {
        let sample_days = self
            .first_time
            .zip(self.last_time)
            .map(|(first, last)| last.saturating_sub(first).max(0) / crate::NANOS_PER_DAY)
            .unwrap_or(0);
        let confidence_interval = wilson_interval(self.wins, self.trades);
        let profit_factor_unbounded =
            self.positive_returns_pct > 0.0 && self.negative_returns_pct == 0.0;
        json!({
            "trades": self.trades,
            "wins": self.wins,
            "win_rate_pct": (self.trades > 0).then(|| self.wins as f64 / self.trades as f64 * 100.0),
            "win_rate_wilson_95_pct": confidence_interval,
            "mean_return_pct": self.mean_net(),
            "mean_net_return_pct": fees_known.then(|| self.mean_net()).flatten(),
            "mean_gross_return_pct": (self.trades > 0).then(|| self.gross_sum_pct / self.trades as f64),
            "return_basis": if fees_known { "after configured fees" } else { "gross; fees unknown" },
            "profit_factor": if self.negative_returns_pct > 0.0 { Some(self.positive_returns_pct / self.negative_returns_pct) } else if profit_factor_unbounded { Some(99.0) } else { None },
            "profit_factor_unbounded": profit_factor_unbounded,
            "trade_span_days": sample_days,
            "fees_applied": fees_known
        })
    }
}

fn wilson_interval(wins: usize, trades: usize) -> Option<[f64; 2]> {
    if trades == 0 {
        return None;
    }
    let z = 1.959_963_984_540_054;
    let n = trades as f64;
    let p = wins as f64 / n;
    let denominator = 1.0 + z * z / n;
    let center = (p + z * z / (2.0 * n)) / denominator;
    let half_width = z * ((p * (1.0 - p) / n + z * z / (4.0 * n * n)).sqrt()) / denominator;
    Some([
        (center - half_width).max(0.0) * 100.0,
        (center + half_width).min(1.0) * 100.0,
    ])
}

fn eligible_signal(signal: &crate::strategies::StrategySignal, allow_short: bool) -> bool {
    if signal.direction != "LONG" && !(allow_short && signal.direction == "SHORT") {
        return false;
    }
    let Some(confluence) = signal.confluence.as_ref() else {
        return false;
    };
    let nwe_direction_matches = match signal.direction.as_str() {
        "LONG" => confluence.nadaraya_state.starts_with("NWE_LOWER_BAND_"),
        "SHORT" => confluence.nadaraya_state.starts_with("NWE_UPPER_BAND_"),
        _ => false,
    };
    nwe_direction_matches && confluence.dss_state != "NEUTRAL"
}

fn evaluate_window(
    bars: &[StrategyBar],
    signals: &[crate::strategies::StrategySignal],
    start: usize,
    end: usize,
    horizon: usize,
    buy_fee_pct: f64,
    sell_fee_pct: f64,
    fees_known: bool,
    allow_short: bool,
) -> TradeSummary {
    let mut summary = TradeSummary::default();
    let mut signal_index = start;
    while signal_index < end {
        if !eligible_signal(&signals[signal_index], allow_short) {
            signal_index += 1;
            continue;
        }
        let entry = signal_index + 1;
        let Some(exit) = entry.checked_add(horizon.saturating_sub(1)) else {
            break;
        };
        if entry >= end || exit >= end {
            signal_index += 1;
            continue;
        }
        summary.add(
            bars,
            entry,
            exit,
            &signals[signal_index].direction,
            buy_fee_pct,
            sell_fee_pct,
            fees_known,
        );
        signal_index = exit + 1;
    }
    summary
}

#[cfg(test)]
#[path = "nwe_dss_tests.rs"]
mod tests;

/// Compare declared fixed timeout exits, select solely on validation mean net
/// return, and keep the final chronological segment untouched for reporting.
pub fn evaluate_nwe_dss_exit_selection(
    bars: &[StrategyBar],
    buy_fee_pct: f64,
    sell_fee_pct: f64,
    fees_known: bool,
    execution_costs_complete: bool,
    venue_native_history: bool,
    allow_short: bool,
) -> Value {
    let minimum_history = crate::stockbit_nwe_4h_minimum_history_bars();
    if bars.len() < minimum_history
        || !buy_fee_pct.is_finite()
        || !sell_fee_pct.is_finite()
        || buy_fee_pct < 0.0
        || sell_fee_pct < 0.0
    {
        return json!({
            "status": "INSUFFICIENT_HISTORY",
            "strategy_id": "nwe_dss_causal_bressert_hold_v1",
            "history_bars": bars.len(),
            "minimum_history_bars": minimum_history,
            "exit_candidates_bars": NWE_DSS_EXIT_CANDIDATES_BARS
        });
    }
    let signals = generate_stockbit_dss_bressert_4h_signals(bars);
    let train_end = ((bars.len() as f64 * TRAINING_FRACTION).floor() as usize).max(minimum_history);
    let validation_end = ((bars.len() as f64 * (TRAINING_FRACTION + VALIDATION_FRACTION)).floor()
        as usize)
        .max(train_end);
    let holdout_end = bars.len();
    let allow_short_in_history = allow_short;
    let mut selected_horizon = None;
    let mut selected_validation = TradeSummary::default();
    for horizon in NWE_DSS_EXIT_CANDIDATES_BARS {
        let candidate = evaluate_window(
            bars,
            &signals,
            train_end,
            validation_end,
            *horizon,
            buy_fee_pct,
            sell_fee_pct,
            fees_known,
            allow_short_in_history,
        );
        let improves = candidate.trades >= crate::MIN_EVIDENCE_SAMPLE_TRADES
            && match (candidate.mean_net(), selected_validation.mean_net()) {
                (Some(score), Some(best)) => score > best,
                (Some(_), None) => true,
                _ => false,
            };
        if improves {
            selected_horizon = Some(*horizon);
            selected_validation = candidate;
        }
    }
    let training = selected_horizon
        .map(|horizon| {
            evaluate_window(
                bars,
                &signals,
                minimum_history.saturating_sub(1),
                train_end,
                horizon,
                buy_fee_pct,
                sell_fee_pct,
                fees_known,
                allow_short_in_history,
            )
        })
        .unwrap_or_default();
    let holdout = selected_horizon.map(|horizon| {
        evaluate_window(
            bars,
            &signals,
            validation_end,
            holdout_end,
            horizon,
            buy_fee_pct,
            sell_fee_pct,
            fees_known,
            allow_short_in_history,
        )
    });
    let holdout = holdout.unwrap_or_default();
    let holdout_window_days = bars
        .get(validation_end)
        .zip(bars.last())
        .map(|(first, last)| last.time.saturating_sub(first.time).max(0) / crate::NANOS_PER_DAY)
        .unwrap_or(0);
    let history_days = bars
        .first()
        .zip(bars.last())
        .map(|(first, last)| last.time.saturating_sub(first.time).max(0) / crate::NANOS_PER_DAY)
        .unwrap_or(0);
    let profitable_holdout = holdout.mean_net().is_some_and(|mean| mean > 0.0)
        && (holdout.negative_returns_pct == 0.0
            || holdout.positive_returns_pct / holdout.negative_returns_pct > 1.0);
    let validated = selected_horizon.is_some()
        && fees_known
        && execution_costs_complete
        && venue_native_history
        && holdout.trades >= crate::MIN_EVIDENCE_SAMPLE_TRADES
        && history_days >= crate::MIN_EVIDENCE_SAMPLE_DAYS
        && profitable_holdout;
    let rule = selected_horizon.map(|horizon| format!(
        "On a completed bar, require a directional DSS Bressert cross or approaching near-cross and matching causal NWE lower/upper band cross or near-band state. Enter at the next bar open; exit at that bar's close after {horizon} bars. No overlapping positions."
    ));
    json!({
        "status": if selected_horizon.is_none() { "NO_VALIDATION_TRADES" } else if validated { "VALIDATED_CANDIDATE" } else { "VALIDATION_PENDING" },
        "strategy_id": "nwe_dss_causal_bressert_hold_v1",
        "score_model_id": crate::STOCKBIT_DSS_BRESSERT_4H_SIGNAL_SCORE_MODEL_ID,
        "history_bars": bars.len(),
        "training": training.to_json(fees_known),
        "validation": selected_validation.to_json(fees_known),
        "holdout": holdout.to_json(fees_known),
        "history_days": history_days,
        "holdout_window_days": holdout_window_days,
        "selected_exit_bars": selected_horizon,
        "exit_candidates_bars": NWE_DSS_EXIT_CANDIDATES_BARS,
        "selection_metric": if fees_known { "highest validation mean return after configured fees; tie favors shorter timeout" } else { "highest validation gross mean return; tie favors shorter timeout; fee-unknown results cannot validate" },
        "training_fraction": TRAINING_FRACTION,
        "validation_fraction": VALIDATION_FRACTION,
        "holdout_fraction": 1.0 - TRAINING_FRACTION - VALIDATION_FRACTION,
        "minimum_holdout_trades": crate::MIN_EVIDENCE_SAMPLE_TRADES,
        "minimum_history_days": crate::MIN_EVIDENCE_SAMPLE_DAYS,
        "minimum_validation_trades": crate::MIN_EVIDENCE_SAMPLE_TRADES,
        "venue_native_history": venue_native_history,
        "fees_known": fees_known,
        "execution_costs_complete": execution_costs_complete,
        "rule_candidate": rule,
        "candidate_is_validated": validated,
        "validation_note": "Historical research only. Validation selects the timeout; holdout is evaluated once and never selects it. Slippage, depth, market impact, funding, and borrow are not modeled."
    })
}
