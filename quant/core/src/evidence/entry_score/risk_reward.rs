use super::nwe_dss::{wilson_interval, TradeSummary};
use super::{
    qualifies, summary_json, EvaluationPolicy, Observation, ENTRY_SCORE_MIN_HISTORY_DAYS,
    ENTRY_SCORE_TARGET_AFTER_FEE_WIN_RATE_PCT,
};
use crate::strategies::StrategyBar;
use serde_json::{json, Value};

const ATR_LOOKBACK_BARS: usize = 14;
const STOP_ATR_MULTIPLIERS: [f64; 3] = [0.75, 1.0, 1.5];
const TARGET_R_MULTIPLES: [f64; 3] = [1.5, 2.0, 2.5];
const MAXIMUM_HOLD_BARS: [usize; 3] = [4, 8, 12];
const SCORE_FLOORS_OUT_OF_50: [f64; 3] = [
    crate::FIVE_INDICATOR_MINIMUM_SCORE_OUT_OF_50,
    10.0,
    20.0,
];
const RISK_REWARD_INTERVAL_NS: i64 = 4 * crate::SECONDS_PER_HOUR * crate::NANOS_PER_SECOND;

#[derive(Clone, Copy)]
struct BracketRule {
    stop_atr_multiple: f64,
    target_r_multiple: f64,
    maximum_hold_bars: usize,
    minimum_score: f64,
}

#[derive(Clone, Copy)]
struct ValidationCandidate {
    rule: BracketRule,
    summary: TradeSummary,
    passes_target: bool,
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
        let (stop_price, target_price) = if observation.long {
            (
                entry_price - risk_distance,
                entry_price + risk_distance * rule.target_r_multiple,
            )
        } else {
            (
                entry_price + risk_distance,
                entry_price - risk_distance * rule.target_r_multiple,
            )
        };
        if stop_price <= 0.0 || target_price <= 0.0 {
            continue;
        }
        let (exit_index, exit_price) = bracket_exit(
            bars,
            entry_index,
            last_index,
            observation.long,
            stop_price,
            target_price,
        );
        summary.add_prices(
            bars[entry_index].time,
            bars[exit_index].time,
            entry_price,
            exit_price,
            if observation.long { "LONG" } else { "SHORT" },
            policy.buy_fee_pct,
            policy.sell_fee_pct,
            policy.fees_known,
        );
        cursor = signal_indices.partition_point(|index| *index <= exit_index);
    }
    summary
}

fn validation_passes(summary: TradeSummary, fees_known: bool) -> bool {
    summary.trades >= crate::MIN_EVIDENCE_SAMPLE_TRADES
        && fees_known
        && summary.win_rate().is_some_and(|rate| {
            rate >= ENTRY_SCORE_TARGET_AFTER_FEE_WIN_RATE_PCT
        })
        && wilson_interval(summary.wins, summary.trades)
            .is_some_and(|interval| interval[0] >= ENTRY_SCORE_TARGET_AFTER_FEE_WIN_RATE_PCT)
        && summary
            .payoff_ratio()
            .is_some_and(|ratio| ratio >= crate::MIN_ASYMMETRIC_REWARD_RISK)
        && summary.mean_net().is_some_and(|mean| mean > 0.0)
        && summary
            .profit_factor()
            .is_some_and(|factor| factor > 1.0)
}

fn candidate_is_better(candidate: ValidationCandidate, current: ValidationCandidate) -> bool {
    candidate
        .passes_target
        .cmp(&current.passes_target)
        .then_with(|| {
            candidate
                .summary
                .mean_net()
                .unwrap_or_default()
                .total_cmp(&current.summary.mean_net().unwrap_or_default())
        })
        .then_with(|| {
            candidate
                .summary
                .payoff_ratio()
                .unwrap_or_default()
                .total_cmp(&current.summary.payoff_ratio().unwrap_or_default())
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
        "entry_rule": "Completed NWE/DSS directional signal with the selected /50 score floor; enter at the next 4h bar open.",
        "weights": weights,
        "minimum_score_out_of_50": candidate.rule.minimum_score,
        "atr_lookback_bars": ATR_LOOKBACK_BARS,
        "stop_distance": format!("{} × 4h ATR", candidate.rule.stop_atr_multiple),
        "stop_atr_multiple": candidate.rule.stop_atr_multiple,
        "target_r_multiple": candidate.rule.target_r_multiple,
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
        * TARGET_R_MULTIPLES.len()
        * MAXIMUM_HOLD_BARS.len()
        * SCORE_FLOORS_OUT_OF_50.len();
    let empty = |status: &str, reason: &str| {
        json!({
            "status": status,
            "reason": reason,
            "preferred_timeframe": "4h",
            "target_after_fee_win_rate_pct": ENTRY_SCORE_TARGET_AFTER_FEE_WIN_RATE_PCT,
            "minimum_after_fee_payoff_ratio": crate::MIN_ASYMMETRIC_REWARD_RISK,
            "minimum_history_days": ENTRY_SCORE_MIN_HISTORY_DAYS,
            "candidate_count": candidate_count,
            "minimum_validation_trades": crate::MIN_EVIDENCE_SAMPLE_TRADES,
            "minimum_holdout_trades": crate::MIN_EVIDENCE_SAMPLE_TRADES,
            "search_space": {
                "atr14_stop_multiples": STOP_ATR_MULTIPLIERS,
                "target_r_multiples": TARGET_R_MULTIPLES,
                "maximum_hold_bars": MAXIMUM_HOLD_BARS,
                "score_floors_out_of_50": SCORE_FLOORS_OUT_OF_50,
            },
            "fees_applied": template.fees_known,
            "candidate": Value::Null,
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
        for target_r_multiple in TARGET_R_MULTIPLES {
            for maximum_hold_bars in MAXIMUM_HOLD_BARS {
                for minimum_score in SCORE_FLOORS_OUT_OF_50 {
                    let rule = BracketRule {
                        stop_atr_multiple,
                        target_r_multiple,
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
                        passes_target: validation_passes(summary, template.fees_known),
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
    let validation_passed = selected.passes_target;
    let holdout_passed = validation_passes(holdout, template.fees_known);
    let history_span_passed = history_days >= ENTRY_SCORE_MIN_HISTORY_DAYS;
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
        "NO_VALIDATION_RULE" => "No candidate rule met the after-fee win rate and 95% lower-bound target, asymmetric payoff, and positive expectancy checks on validation data.",
        "HOLDOUT_FAILED" => "The validation-selected rule did not meet every after-fee confidence, payoff, and expectancy check on the untouched holdout.",
        _ => "The frozen 4h rule met the after-fee confidence, payoff, and expectancy targets on validation and untouched holdout data.",
    };
    json!({
        "status": status,
        "reason": reason,
        "preferred_timeframe": "4h",
        "target_after_fee_win_rate_pct": ENTRY_SCORE_TARGET_AFTER_FEE_WIN_RATE_PCT,
        "minimum_after_fee_payoff_ratio": crate::MIN_ASYMMETRIC_REWARD_RISK,
        "minimum_history_days": ENTRY_SCORE_MIN_HISTORY_DAYS,
        "search_space": {
            "atr14_stop_multiples": STOP_ATR_MULTIPLIERS,
            "target_r_multiples": TARGET_R_MULTIPLES,
            "maximum_hold_bars": MAXIMUM_HOLD_BARS,
            "score_floors_out_of_50": SCORE_FLOORS_OUT_OF_50,
        },
        "candidate_count": candidate_count,
        "eligible_validation_candidate_count": eligible_candidates,
        "minimum_validation_trades": crate::MIN_EVIDENCE_SAMPLE_TRADES,
        "minimum_holdout_trades": crate::MIN_EVIDENCE_SAMPLE_TRADES,
        "selection_method": "Predeclared ATR stop, R-multiple target, max-hold, and score-floor grid; selection uses validation net return among candidates whose after-fee win rate and 95% Wilson lower bound both reach the target, with the required asymmetric payoff and positive expectancy. The selected rule is evaluated once on untouched holdout data.",
        "fill_assumptions": "Enter at the next 4h open. Stops fill at the stop or worse after a gap. Targets fill at the target. If both stop and target are inside one bar, assume the stop fills first. Untriggered rules exit at the max-hold bar close.",
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
        "selection_uses_holdout": false,
        "live_weights_changed": false,
    })
}
