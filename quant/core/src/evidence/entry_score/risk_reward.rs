use super::super::nwe_dss::{wilson_interval, TradeSummary};
use super::{
    qualifies, summary_json, EvaluationPolicy, Observation, ENTRY_SCORE_MIN_HISTORY_DAYS,
    ENTRY_SCORE_TARGET_AFTER_FEE_WIN_RATE_PCT,
};
use crate::strategies::StrategyBar;
use serde_json::{json, Value};

const ATR_LOOKBACK_BARS: usize = 14;
const STOP_ATR_MULTIPLIERS: [f64; 3] = [0.75, 1.0, 1.5];
const TARGET_AFTER_FEE_R_MULTIPLES: [f64; 5] = [1.1, 1.25, 1.5, 2.0, 2.5];
const MAXIMUM_HOLD_BARS: [usize; 3] = [4, 8, 12];
const SCORE_FLOORS_OUT_OF_60: [f64; 3] = [
    crate::SIX_INDICATOR_MINIMUM_SCORE_OUT_OF_60,
    10.0,
    20.0,
];
const RISK_REWARD_INTERVAL_NS: i64 = 4 * crate::SECONDS_PER_HOUR * crate::NANOS_PER_SECOND;
const MAX_AUTOCORRELATION_RETURN_BARS: usize = 128;
const MIN_AUTOCORRELATION_RETURN_SAMPLES: usize = 30;
const HYPOTHETICAL_BASE_RISK_PCT: f64 = 1.0;
const MINIMUM_RISK_MODEL_TRADES: usize = 30;
pub(super) const PROTOCOL_ID: &str = "entry_score_signed_risk_reward_after_fee_v5";
pub(super) const RISK_SIZING_PROTOCOL_ID: &str = "causal_return_autocorrelation_horizon_variance_v1";

#[derive(Clone, Copy)]
struct BracketRule {
    stop_atr_multiple: f64,
    target_after_fee_r_multiple: f64,
    maximum_hold_bars: usize,
    minimum_score: f64,
}

#[derive(Clone, Copy)]
struct ValidationCandidate {
    rule: BracketRule,
    summary: TradeSummary,
    passes_win_rate_target: bool,
}

#[derive(Clone, Copy)]
struct RuleTrade {
    signal_index: usize,
    entry_price: f64,
    stop_price: f64,
    exit_price: f64,
    long: bool,
}

#[derive(Clone, Copy)]
struct AutocorrelationRiskEstimate {
    multiplier: f64,
    variance_ratio: f64,
    return_samples: usize,
}

#[derive(Clone, Copy)]
struct SimulatedEquity {
    current: f64,
    peak: f64,
    maximum_drawdown_pct: f64,
}

impl Default for SimulatedEquity {
    fn default() -> Self {
        Self {
            current: 1.0,
            peak: 1.0,
            maximum_drawdown_pct: 0.0,
        }
    }
}

impl SimulatedEquity {
    fn add_risk_unit(&mut self, risk_fraction: f64, realized_r: f64) {
        let change = risk_fraction * realized_r;
        if change.is_nan()
            || change <= -1.0
            || (change.is_infinite() && change.is_sign_negative())
        {
            self.current = 0.0;
        } else if change.is_infinite() {
            self.current = f64::MAX / 100.0;
        } else {
            let next = self.current * (1.0 + change);
            self.current = if next.is_finite() { next } else { f64::MAX / 100.0 };
        }
        self.peak = self.peak.max(self.current);
        let drawdown_pct = if self.peak > 0.0 {
            (self.peak - self.current) / self.peak * 100.0
        } else {
            100.0
        };
        self.maximum_drawdown_pct = self.maximum_drawdown_pct.max(drawdown_pct);
    }

    fn total_return_pct(self) -> f64 {
        (self.current - 1.0) * 100.0
    }
}

#[derive(Default)]
struct RiskSizingSummary {
    trades: usize,
    missing_estimates: usize,
    multipliers: Vec<f64>,
    fixed_risk_equity: SimulatedEquity,
    adjusted_risk_equity: SimulatedEquity,
}

impl RiskSizingSummary {
    fn add_trade(
        &mut self,
        bars: &[StrategyBar],
        trade: RuleTrade,
        maximum_hold_bars: usize,
        policy: EvaluationPolicy,
    ) {
        self.trades += 1;
        let (entry_fee_pct, exit_fee_pct) = if trade.long {
            (policy.buy_fee_pct, policy.sell_fee_pct)
        } else {
            (policy.sell_fee_pct, policy.buy_fee_pct)
        };
        let net_return_pct = |exit_price: f64| {
            let gross_return_pct = if trade.long {
                (exit_price - trade.entry_price) / trade.entry_price * 100.0
            } else {
                (trade.entry_price - exit_price) / trade.entry_price * 100.0
            };
            crate::venue_rules::net_return_after_transaction_fees(
                gross_return_pct,
                exit_price / trade.entry_price,
                entry_fee_pct,
                exit_fee_pct,
            )
        };
        let planned_stop_loss_pct = -net_return_pct(trade.stop_price);
        let realized_return_pct = net_return_pct(trade.exit_price);
        if !planned_stop_loss_pct.is_finite()
            || planned_stop_loss_pct <= 0.0
            || !realized_return_pct.is_finite()
        {
            self.missing_estimates += 1;
            return;
        }
        let realized_r = realized_return_pct / planned_stop_loss_pct;
        if !realized_r.is_finite() {
            self.missing_estimates += 1;
            return;
        }
        self.fixed_risk_equity
            .add_risk_unit(HYPOTHETICAL_BASE_RISK_PCT / 100.0, realized_r);
        if let Some(estimate) = autocorrelation_risk_multiplier(
            bars,
            trade.signal_index,
            maximum_hold_bars,
        ) {
            self.multipliers.push(estimate.multiplier);
            self.adjusted_risk_equity.add_risk_unit(
                HYPOTHETICAL_BASE_RISK_PCT / 100.0 * estimate.multiplier,
                realized_r,
            );
        } else {
            self.missing_estimates += 1;
        }
    }

    fn to_json(&self) -> Value {
        let mut multipliers = self.multipliers.clone();
        multipliers.sort_by(f64::total_cmp);
        let coverage_pct = if self.trades > 0 {
            multipliers.len() as f64 / self.trades as f64 * 100.0
        } else {
            0.0
        };
        let complete = self.trades > 0 && self.missing_estimates == 0;
        let sufficient_sample = self.trades >= MINIMUM_RISK_MODEL_TRADES;
        let multiplier_mean = (!multipliers.is_empty()).then(|| {
            multipliers.iter().sum::<f64>() / multipliers.len() as f64
        });
        let multiplier_median = if multipliers.is_empty() {
            None
        } else {
            let middle = multipliers.len() / 2;
            Some(if multipliers.len() % 2 == 0 {
                (multipliers[middle - 1] + multipliers[middle]) / 2.0
            } else {
                multipliers[middle]
            })
        };
        json!({
            "status": if self.trades == 0 { "NO_TRADES" } else if !complete { "INCOMPLETE_AUTOCORRELATION_COVERAGE" } else if !sufficient_sample { "INSUFFICIENT_SAMPLE" } else { "RESEARCH_ONLY" },
            "trades": self.trades,
            "autocorrelation_estimates": multipliers.len(),
            "unavailable_estimates": self.missing_estimates,
            "coverage_pct": coverage_pct,
            "mean_size_multiplier": multiplier_mean,
            "median_size_multiplier": multiplier_median,
            "base_risk_per_trade_pct": HYPOTHETICAL_BASE_RISK_PCT,
            "fixed_risk_equity_return_pct": self.fixed_risk_equity.total_return_pct(),
            "autocorrelation_adjusted_equity_return_pct": complete.then(|| self.adjusted_risk_equity.total_return_pct()),
            "fixed_risk_max_drawdown_pct": self.fixed_risk_equity.maximum_drawdown_pct,
            "autocorrelation_adjusted_max_drawdown_pct": complete.then_some(self.adjusted_risk_equity.maximum_drawdown_pct),
            "minimum_trades": MINIMUM_RISK_MODEL_TRADES,
        })
    }
}

fn average_true_range(bars: &[StrategyBar], signal_index: usize) -> Option<f64> {
    let first_index = signal_index.checked_add(1)?.checked_sub(ATR_LOOKBACK_BARS)?;
    let mut total = 0.0;
    for index in first_index..=signal_index {
        let bar = bars.get(index)?;
        let true_range = match index.checked_sub(1).and_then(|previous| bars.get(previous)) {
            Some(previous) => (bar.high - bar.low)
                .max((bar.high - previous.close).abs())
                .max((bar.low - previous.close).abs()),
            None => bar.high - bar.low,
        };
        if !true_range.is_finite() || true_range <= 0.0 {
            return None;
        }
        total += true_range;
    }
    let value = total / ATR_LOOKBACK_BARS as f64;
    (value.is_finite() && value > 0.0).then_some(value)
}

fn autocorrelation_risk_multiplier(
    bars: &[StrategyBar],
    signal_index: usize,
    horizon_bars: usize,
) -> Option<AutocorrelationRiskEstimate> {
    let mut returns = Vec::with_capacity(MAX_AUTOCORRELATION_RETURN_BARS);
    let mut current = signal_index;
    while current > 0 && returns.len() < MAX_AUTOCORRELATION_RETURN_BARS {
        let previous = current - 1;
        let current_bar = bars.get(current)?;
        let previous_bar = bars.get(previous)?;
        if current_bar.time.saturating_sub(previous_bar.time) != RISK_REWARD_INTERVAL_NS {
            break;
        }
        if !current_bar.close.is_finite()
            || !previous_bar.close.is_finite()
            || current_bar.close <= 0.0
            || previous_bar.close <= 0.0
        {
            break;
        }
        returns.push((current_bar.close / previous_bar.close).ln());
        current = previous;
    }
    if returns.len() < MIN_AUTOCORRELATION_RETURN_SAMPLES || horizon_bars <= 1 {
        return None;
    }
    returns.reverse();
    let mean = returns.iter().sum::<f64>() / returns.len() as f64;
    let centered = returns
        .iter()
        .map(|value| value - mean)
        .collect::<Vec<_>>();
    let variance = centered.iter().map(|value| value * value).sum::<f64>();
    if !variance.is_finite() || variance <= 0.0 {
        return None;
    }
    let maximum_lag = horizon_bars.saturating_sub(1).min(centered.len() - 1);
    let variance_ratio = 1.0
        + (1..=maximum_lag)
            .map(|lag| {
                let covariance = centered[lag..]
                    .iter()
                    .zip(&centered[..centered.len() - lag])
                    .map(|(left, right)| left * right)
                    .sum::<f64>();
                let rho = covariance / variance;
                2.0 * (1.0 - lag as f64 / horizon_bars as f64) * rho
            })
            .sum::<f64>();
    if !variance_ratio.is_finite() {
        return None;
    }
    let conservative_variance_ratio = variance_ratio.max(1.0);
    let multiplier = conservative_variance_ratio.sqrt().recip().clamp(0.25, 1.0);
    multiplier
        .is_finite()
        .then_some(AutocorrelationRiskEstimate {
            multiplier,
            variance_ratio: conservative_variance_ratio,
            return_samples: returns.len(),
        })
}

fn bracket_exit(
    bars: &[StrategyBar],
    entry_index: usize,
    last_index: usize,
    long: bool,
    stop_price: f64,
    target_price: f64,
) -> (usize, f64) {
    for index in entry_index..=last_index {
        let bar = bars[index];
        if long {
            if bar.open <= stop_price {
                return (index, bar.open);
            }
            if bar.open >= target_price {
                return (index, target_price);
            }
            // If both levels fall inside one OHLC bar, assume the stop was hit first.
            if bar.low <= stop_price {
                return (index, stop_price);
            }
            if bar.high >= target_price {
                return (index, target_price);
            }
        } else {
            if bar.open >= stop_price {
                return (index, bar.open);
            }
            if bar.open <= target_price {
                return (index, target_price);
            }
            if bar.high >= stop_price {
                return (index, stop_price);
            }
            if bar.low <= target_price {
                return (index, target_price);
            }
        }
    }
    (last_index, bars[last_index].close)
}

fn after_fee_target_price(
    entry_price: f64,
    stop_price: f64,
    long: bool,
    target_after_fee_r_multiple: f64,
    policy: EvaluationPolicy,
) -> Option<f64> {
    // The R grid is net reward / net stop loss; solve for the price target after both fees.
    if !entry_price.is_finite()
        || entry_price <= 0.0
        || !stop_price.is_finite()
        || stop_price <= 0.0
        || !target_after_fee_r_multiple.is_finite()
        || target_after_fee_r_multiple <= 0.0
        || !policy.buy_fee_pct.is_finite()
        || !policy.sell_fee_pct.is_finite()
        || policy.buy_fee_pct < 0.0
        || policy.sell_fee_pct < 0.0
        || policy.buy_fee_pct >= 100.0
        || policy.sell_fee_pct >= 100.0
    {
        return None;
    }
    let (entry_fee_pct, exit_fee_pct) = if long {
        (policy.buy_fee_pct, policy.sell_fee_pct)
    } else {
        (policy.sell_fee_pct, policy.buy_fee_pct)
    };
    let stop_gross_return_pct = if long {
        (stop_price - entry_price) / entry_price * 100.0
    } else {
        (entry_price - stop_price) / entry_price * 100.0
    };
    let stop_net_return_pct = crate::venue_rules::net_return_after_transaction_fees(
        stop_gross_return_pct,
        stop_price / entry_price,
        entry_fee_pct,
        exit_fee_pct,
    );
    let net_risk_pct = -stop_net_return_pct;
    if !net_risk_pct.is_finite() || net_risk_pct <= 0.0 {
        return None;
    }

    let target_net_return_pct = net_risk_pct * target_after_fee_r_multiple;
    let exit_price_multiple = if long {
        let denominator = 100.0 - exit_fee_pct;
        if denominator <= 0.0 {
            return None;
        }
        (100.0 + entry_fee_pct + target_net_return_pct) / denominator
    } else {
        let numerator = 100.0 - entry_fee_pct - target_net_return_pct;
        let denominator = 100.0 + exit_fee_pct;
        if numerator <= 0.0 || denominator <= 0.0 {
            return None;
        }
        numerator / denominator
    };
    let target_price = entry_price * exit_price_multiple;
    (target_price.is_finite()
        && target_price > 0.0
        && if long {
            target_price > entry_price
        } else {
            target_price < entry_price
        })
    .then_some(target_price)
}

fn evaluate_rule_with_observer(
    bars: &[StrategyBar],
    observations: &[Option<Observation>],
    signal_indices: &[usize],
    signal_atrs: &[Option<f64>],
    start: usize,
    end: usize,
    weights: [u8; 4],
    template: EvaluationPolicy,
    rule: BracketRule,
    mut on_trade: impl FnMut(RuleTrade),
) -> TradeSummary {
    let mut summary = TradeSummary::default();
    let policy = EvaluationPolicy {
        weights,
        minimum_score: rule.minimum_score,
        ..template
    };
    let mut cursor = signal_indices.partition_point(|index| *index < start);
    while cursor < signal_indices.len() {
        let signal_index = signal_indices[cursor];
        if signal_index >= end {
            break;
        }
        let atr = signal_atrs.get(cursor).copied().flatten();
        cursor += 1;
        let Some(observation) = observations
            .get(signal_index)
            .copied()
            .flatten()
            .filter(|observation| qualifies(*observation, policy))
        else {
            continue;
        };
        let long = super::score_with_weights(observation, policy.weights) > 0.0;
        let entry_index = signal_index.saturating_add(1);
        let last_index = entry_index
            .saturating_add(rule.maximum_hold_bars.saturating_sub(1));
        if entry_index >= end || last_index >= end {
            continue;
        }
        let Some(atr) = atr else {
            continue;
        };
        let entry_price = bars[entry_index].open;
        let risk_distance = atr * rule.stop_atr_multiple;
        if !entry_price.is_finite() || entry_price <= 0.0 || !risk_distance.is_finite() {
            continue;
        }
        let stop_price = if long {
            entry_price - risk_distance
        } else {
            entry_price + risk_distance
        };
        if stop_price <= 0.0 {
            continue;
        }
        let Some(target_price) = after_fee_target_price(
            entry_price,
            stop_price,
            long,
            rule.target_after_fee_r_multiple,
            policy,
        ) else {
            continue;
        };
        let (exit_index, exit_price) = bracket_exit(
            bars,
            entry_index,
            last_index,
            long,
            stop_price,
            target_price,
        );
        summary.add_prices(
            bars[entry_index].time,
            bars[exit_index].time,
            entry_price,
            exit_price,
            if long { "LONG" } else { "SHORT" },
            policy.buy_fee_pct,
            policy.sell_fee_pct,
            policy.fees_known,
        );
        on_trade(RuleTrade {
            signal_index,
            entry_price,
            stop_price,
            exit_price,
            long,
        });
        cursor = signal_indices.partition_point(|index| *index <= exit_index);
    }
    summary
}

fn evaluate_rule(
    bars: &[StrategyBar],
    observations: &[Option<Observation>],
    signal_indices: &[usize],
    signal_atrs: &[Option<f64>],
    start: usize,
    end: usize,
    weights: [u8; 4],
    template: EvaluationPolicy,
    rule: BracketRule,
) -> TradeSummary {
    evaluate_rule_with_observer(
        bars,
        observations,
        signal_indices,
        signal_atrs,
        start,
        end,
        weights,
        template,
        rule,
        |_| {},
    )
}

fn validation_passes(summary: TradeSummary, fees_known: bool) -> bool {
    summary.trades >= crate::MIN_EVIDENCE_SAMPLE_TRADES
        && fees_known
        && summary.win_rate().is_some_and(|rate| {
            rate >= ENTRY_SCORE_TARGET_AFTER_FEE_WIN_RATE_PCT
        })
        && wilson_interval(summary.wins, summary.trades)
            .is_some_and(|interval| interval[0] >= ENTRY_SCORE_TARGET_AFTER_FEE_WIN_RATE_PCT)
}

fn candidate_is_better(candidate: ValidationCandidate, current: ValidationCandidate) -> bool {
    candidate
        .passes_win_rate_target
        .cmp(&current.passes_win_rate_target)
        .then_with(|| {
            candidate
                .summary
                .win_rate()
                .unwrap_or_default()
                .total_cmp(&current.summary.win_rate().unwrap_or_default())
        })
        .then_with(|| {
            let candidate_lower =
                wilson_interval(candidate.summary.wins, candidate.summary.trades)
                    .map(|interval| interval[0])
                    .unwrap_or_default();
            let current_lower = wilson_interval(current.summary.wins, current.summary.trades)
                .map(|interval| interval[0])
                .unwrap_or_default();
            candidate_lower.total_cmp(&current_lower)
        })
        .then_with(|| candidate.summary.trades.cmp(&current.summary.trades))
        .is_gt()
}

fn candidate_json(
    candidate: ValidationCandidate,
    validation: TradeSummary,
    holdout: TradeSummary,
    weights: [u8; 4],
    history_days: i64,
    fees_known: bool,
) -> Value {
    let validation_passed = validation_passes(validation, fees_known);
    let holdout_passed = validation_passes(holdout, fees_known);
    json!({
        "entry_rule": "Use the signed six-factor total and the selected /60 score floor on a completed bar. Enter at the next 4h bar open.",
        "weights": weights,
        "minimum_score_out_of_60": candidate.rule.minimum_score,
        "atr_lookback_bars": ATR_LOOKBACK_BARS,
        "stop_distance": format!("{} × 4h ATR", candidate.rule.stop_atr_multiple),
        "stop_atr_multiple": candidate.rule.stop_atr_multiple,
        "target_after_fee_r_multiple": candidate.rule.target_after_fee_r_multiple,
        "maximum_hold_bars": candidate.rule.maximum_hold_bars,
        "validation": summary_json(validation, fees_known),
        "holdout": summary_json(holdout, fees_known),
        "validation_passed": validation_passed,
        "holdout_passed": holdout_passed,
        "history_span_passed": history_days >= ENTRY_SCORE_MIN_HISTORY_DAYS,
        "qualified": validation_passed && holdout_passed && history_days >= ENTRY_SCORE_MIN_HISTORY_DAYS,
    })
}
pub(super) fn report(
    bars: &[StrategyBar],
    observations: &[Option<Observation>],
    weights: Option<[u8; 4]>,
    template: EvaluationPolicy,
    validation_start: usize,
    holdout_start: usize,
    history_days: i64,
    valid_history: bool,
    bar_interval_ns: i64,
) -> Value {
    let candidate_count = STOP_ATR_MULTIPLIERS.len()
        * TARGET_AFTER_FEE_R_MULTIPLES.len()
        * MAXIMUM_HOLD_BARS.len()
        * SCORE_FLOORS_OUT_OF_60.len();
    let empty = |status: &str, reason: &str| {
        json!({
            "protocol_id": PROTOCOL_ID,
            "status": status,
            "reason": reason,
            "preferred_timeframe": "4h",
            "target_after_fee_win_rate_pct": ENTRY_SCORE_TARGET_AFTER_FEE_WIN_RATE_PCT,
            "minimum_history_days": ENTRY_SCORE_MIN_HISTORY_DAYS,
            "candidate_count": candidate_count,
            "minimum_validation_trades": crate::MIN_EVIDENCE_SAMPLE_TRADES,
            "minimum_holdout_trades": crate::MIN_EVIDENCE_SAMPLE_TRADES,
            "search_space": {
                "atr14_stop_multiples": STOP_ATR_MULTIPLIERS,
                "target_after_fee_r_multiples": TARGET_AFTER_FEE_R_MULTIPLES,
                "maximum_hold_bars": MAXIMUM_HOLD_BARS,
                "score_floors_out_of_60": SCORE_FLOORS_OUT_OF_60,
            },
            "target_r_basis": "after configured entry and exit fees",
            "fees_applied": template.fees_known,
            "candidate": Value::Null,
            "risk_sizing": {
                "protocol_id": RISK_SIZING_PROTOCOL_ID,
                "status": "UNAVAILABLE",
                "reason": reason,
                "live_sizing_enabled": false,
            },
            "selection_uses_holdout": false,
            "live_weights_changed": false,
        })
    };
    if bar_interval_ns != RISK_REWARD_INTERVAL_NS {
        return empty(
            "PREFERRED_TIMEFRAME_IS_4H",
            "The risk/reward setup search is focused on 4h entries. Select 4h to compare bracket rules.",
        );
    }
    if !valid_history {
        return empty("INVALID_HISTORY", "The candle history is invalid or not chronological.");
    }
    if !template.fees_known {
        return empty(
            "FEES_UNKNOWN",
            "Fee-aware risk/reward selection requires a known buy and sell fee policy.",
        );
    }
    let Some(weights) = weights else {
        return empty(
            "WEIGHTS_UNAVAILABLE",
            "No training weight vector is available for this rule search.",
        );
    };
    let signal_indices = observations
        .iter()
        .enumerate()
        .filter_map(|(index, observation)| observation.is_some().then_some(index))
        .collect::<Vec<_>>();
    let signal_atrs = signal_indices
        .iter()
        .map(|index| average_true_range(bars, *index))
        .collect::<Vec<_>>();
    let mut selected: Option<ValidationCandidate> = None;
    let mut eligible_candidates = 0;
    for stop_atr_multiple in STOP_ATR_MULTIPLIERS {
        for target_after_fee_r_multiple in TARGET_AFTER_FEE_R_MULTIPLES {
            for maximum_hold_bars in MAXIMUM_HOLD_BARS {
                for minimum_score in SCORE_FLOORS_OUT_OF_60 {
                    let rule = BracketRule {
                        stop_atr_multiple,
                        target_after_fee_r_multiple,
                        maximum_hold_bars,
                        minimum_score,
                    };
                    let summary = evaluate_rule(
                        bars,
                        observations,
                        &signal_indices,
                        &signal_atrs,
                        validation_start,
                        holdout_start,
                        weights,
                        template,
                        rule,
                    );
                    if summary.trades < crate::MIN_EVIDENCE_SAMPLE_TRADES {
                        continue;
                    }
                    eligible_candidates += 1;
                    let candidate = ValidationCandidate {
                        rule,
                        summary,
                        passes_win_rate_target: validation_passes(summary, template.fees_known),
                    };
                    if selected
                        .is_none_or(|current| candidate_is_better(candidate, current))
                    {
                        selected = Some(candidate);
                    }
                }
            }
        }
    }
    let Some(selected) = selected else {
        let mut result = empty(
            "INSUFFICIENT_VALIDATION_SAMPLE",
            "No 4h stop/target/score rule reached the minimum validation trade count.",
        );
        result["eligible_validation_candidate_count"] = json!(eligible_candidates);
        return result;
    };
    let holdout = evaluate_rule(
        bars,
        observations,
        &signal_indices,
        &signal_atrs,
        holdout_start,
        bars.len(),
        weights,
        template,
        selected.rule,
    );
    let mut validation_sizing = RiskSizingSummary::default();
    evaluate_rule_with_observer(
        bars,
        observations,
        &signal_indices,
        &signal_atrs,
        validation_start,
        holdout_start,
        weights,
        template,
        selected.rule,
        |trade| {
            validation_sizing.add_trade(
                bars,
                trade,
                selected.rule.maximum_hold_bars,
                template,
            )
        },
    );
    let mut holdout_sizing = RiskSizingSummary::default();
    evaluate_rule_with_observer(
        bars,
        observations,
        &signal_indices,
        &signal_atrs,
        holdout_start,
        bars.len(),
        weights,
        template,
        selected.rule,
        |trade| {
            holdout_sizing.add_trade(
                bars,
                trade,
                selected.rule.maximum_hold_bars,
                template,
            )
        },
    );
    let validation_risk_json = validation_sizing.to_json();
    let holdout_risk_json = holdout_sizing.to_json();
    let risk_sizing_ready = validation_risk_json["status"] == "RESEARCH_ONLY"
        && holdout_risk_json["status"] == "RESEARCH_ONLY";
    let validation_passed = selected.passes_win_rate_target;
    let holdout_passed = validation_passes(holdout, template.fees_known);
    let history_span_passed = history_days >= ENTRY_SCORE_MIN_HISTORY_DAYS;
    let entry_rule_qualified = validation_passed && holdout_passed && history_span_passed;
    let risk_sizing_status = if !risk_sizing_ready {
        "INSUFFICIENT_EVIDENCE"
    } else if !entry_rule_qualified {
        "EXPLORATORY_ONLY"
    } else {
        "RESEARCH_ONLY"
    };
    let risk_sizing_reason = if !risk_sizing_ready {
        "The validation or untouched holdout sample lacks 30 trades with complete causal autocorrelation estimates."
    } else if !entry_rule_qualified {
        "The selected entry rule did not pass its 50% after-fee win-rate, confidence, and history gates; sizing scenarios remain exploratory."
    } else {
        "The frozen entry rule passed its existing validation and untouched holdout gates. The size model remains research-only and is not activated for live positions."
    };
    let latest_estimate = bars
        .len()
        .checked_sub(1)
        .and_then(|index| {
            autocorrelation_risk_multiplier(
                bars,
                index,
                selected.rule.maximum_hold_bars,
            )
            .map(|estimate| {
                json!({
                    "bar_time": bars[index].time,
                    "multiplier": estimate.multiplier,
                    "horizon_variance_ratio": estimate.variance_ratio,
                    "return_samples": estimate.return_samples,
                })
            })
        });
    let risk_sizing = json!({
        "protocol_id": RISK_SIZING_PROTOCOL_ID,
        "status": risk_sizing_status,
        "reason": risk_sizing_reason,
        "method": "At each qualifying 4h signal, estimate the variance ratio over the selected maximum hold from up to 128 contiguous completed-bar log returns. Set the risk multiplier to min(1, 1/sqrt(max(1, variance_ratio))). Positive serial dependence may reduce risk; negative dependence never increases it.",
        "minimum_return_samples": MIN_AUTOCORRELATION_RETURN_SAMPLES,
        "lookback_return_bars": MAX_AUTOCORRELATION_RETURN_BARS,
        "maximum_risk_multiplier": 1.0,
        "minimum_risk_multiplier": 0.25,
        "base_risk_per_trade_pct": HYPOTHETICAL_BASE_RISK_PCT,
        "maximum_base_risk_per_trade_pct": crate::risk::MAX_RISK_PER_TRADE_PCT * 100.0,
        "selected_maximum_hold_bars": selected.rule.maximum_hold_bars,
        "latest_causal_estimate": latest_estimate,
        "validation": validation_risk_json,
        "holdout": holdout_risk_json,
        "live_sizing_enabled": false,
        "qualification_target": "Research diagnostics only; this sizing comparison does not alter the after-fee entry win-rate gate or promote a live position-size profile.",
    });
    let status = if !history_span_passed {
        "INSUFFICIENT_HISTORY"
    } else if !validation_passed {
        "NO_VALIDATION_RULE"
    } else if !holdout_passed {
        "HOLDOUT_FAILED"
    } else {
        "QUALIFIED"
    };
    let reason = match status {
        "INSUFFICIENT_HISTORY" => "The selected rule is exploratory; qualification needs at least 1,000 calendar days. This is a minimum, not a maximum lookback.",
        "NO_VALIDATION_RULE" => "No candidate rule met the 50% after-fee win-rate target and its 95% Wilson lower bound on validation data.",
        "HOLDOUT_FAILED" => "The validation-selected rule did not meet the 50% after-fee win-rate target and its 95% Wilson lower bound on the untouched holdout.",
        _ => "The frozen 4h rule met the 50% after-fee win-rate target and its 95% Wilson lower bound on validation and untouched holdout data.",
    };
    json!({
        "protocol_id": PROTOCOL_ID,
        "status": status,
        "reason": reason,
        "preferred_timeframe": "4h",
        "target_after_fee_win_rate_pct": ENTRY_SCORE_TARGET_AFTER_FEE_WIN_RATE_PCT,
        "minimum_history_days": ENTRY_SCORE_MIN_HISTORY_DAYS,
        "search_space": {
            "atr14_stop_multiples": STOP_ATR_MULTIPLIERS,
            "target_after_fee_r_multiples": TARGET_AFTER_FEE_R_MULTIPLES,
            "maximum_hold_bars": MAXIMUM_HOLD_BARS,
            "score_floors_out_of_60": SCORE_FLOORS_OUT_OF_60,
        },
        "candidate_count": candidate_count,
        "eligible_validation_candidate_count": eligible_candidates,
        "minimum_validation_trades": crate::MIN_EVIDENCE_SAMPLE_TRADES,
        "minimum_holdout_trades": crate::MIN_EVIDENCE_SAMPLE_TRADES,
        "selection_method": "Predeclared ATR stop, target-to-stop net R after configured entry and exit fees, max-hold, and score-floor grid; among candidates meeting the 50% after-fee win-rate target and its 95% Wilson lower bound on validation, select the highest validation win rate, then the stronger lower bound and larger sample. Payoff and net return are reported as diagnostics, not qualification targets. The selected rule is evaluated once on untouched holdout data.",
        "fill_assumptions": "Enter at the next 4h open. The target price is solved so planned target reward divided by planned stop loss is the selected R multiple after configured entry and exit fees. Stops fill at the stop or worse after a gap, so gaps can reduce realized payoff. Targets fill at the target. If both stop and target are inside one bar, assume the stop fills first. Untriggered rules exit at the max-hold bar close. Observed validation and holdout payoff uses realized net trade returns after fees.",
        "target_r_basis": "after configured entry and exit fees",
        "fees_applied": true,
        "excluded_costs": ["Spread", "Slippage", "Market impact", "Funding or borrow costs"],
        "selected_score_weights": weights,
        "candidate": candidate_json(
            selected,
            selected.summary,
            holdout,
            weights,
            history_days,
            template.fees_known,
        ),
        "risk_sizing": risk_sizing,
        "selection_uses_holdout": false,
        "live_weights_changed": false,
    })
}
