use super::nwe_dss::{wilson_interval, LagOneCorrelation, TradeSummary};
use crate::strategies::{score_entry_signal, visit_entry_signals, EntryModelKind, StrategyBar};
use serde_json::{json, Value};

const TRAINING_SHARE: usize = 60;
const VALIDATION_END_SHARE: usize = 80;
const TOTAL_SCORE_BAND_WIDTH: f64 = 10.0;
const FACTOR_SCORE_BAND_WIDTH: f64 = 2.5;
const BAND_COUNT: usize = 4;
const SCORE_WEIGHT_BUDGET: u8 = 40;
const SCORE_WEIGHT_STEP: u8 = 5;
const SCORE_WEIGHT_SCALE: f64 = 10.0;
const MIN_WEIGHT_FIT_TRADES: usize = 30;
pub const ENTRY_SCORE_TARGET_AFTER_FEE_WIN_RATE_PCT: f64 = 70.0;
const MIN_HOUR_SELECTION_TRADES: usize = crate::MIN_EVIDENCE_SAMPLE_TRADES;
const WEIGHT_FIT_EXIT_BARS: usize = 8;
const BASELINE_WEIGHTS: [u8; 4] = [10; 4];
const HOURS_PER_DAY: i64 = 24;
const TOP_HOUR_PAIRS_TO_REPORT: usize = 5;
const FACTOR_NAMES: [&str; 4] = [
    "Nadaraya-Watson Envelope",
    "DSS Bressert",
    "Relative Volume",
    "Estimated Volume Delta (OHLCV proxy)",
];
pub const ENTRY_SCORE_VALIDATION_PROTOCOL_ID: &str = "entry_four_factor_weight_fit_after_fee_target_70_v4";

#[derive(Debug, Clone, Copy, PartialEq)]
struct Observation {
    long: bool,
    total: f64,
    factors: [f64; 4],
    available: [bool; 4],
}

#[derive(Clone, Copy, Default)]
struct WindowSummary {
    overall: TradeSummary,
    total_bands: [TradeSummary; BAND_COUNT],
    factor_bands: [[TradeSummary; BAND_COUNT]; 4],
    unavailable_factors: [TradeSummary; 4],
}

#[derive(Clone, Copy)]
struct EvaluationPolicy {
    horizon: usize,
    exit_on_reverse_signal: bool,
    weights: [u8; 4],
    minimum_score: f64,
    buy_fee_pct: f64,
    sell_fee_pct: f64,
    fees_known: bool,
    allow_short: bool,
}

struct ExitSelection {
    selected_policy: Option<EvaluationPolicy>,
    validation: WindowSummary,
    candidates: Vec<Value>,
}

#[derive(Clone, Copy)]
struct HourPair {
    entry_hour: u8,
    exit_hour: u8,
    summary: TradeSummary,
}

struct HourSelection {
    selected: Option<HourPair>,
    eligible_pairs: usize,
    top_candidates: Vec<HourPair>,
}

fn band_index(points: f64, width: f64) -> usize {
    ((points / width).floor() as usize).min(BAND_COUNT - 1)
}

fn score_with_weights(observation: Observation, weights: [u8; 4]) -> f64 {
    observation
        .factors
        .iter()
        .zip(weights)
        .map(|(points, weight)| points * f64::from(weight) / SCORE_WEIGHT_SCALE)
        .sum()
}

fn qualifies(observation: Observation, policy: EvaluationPolicy) -> bool {
    (observation.long || policy.allow_short)
        && score_with_weights(observation, policy.weights) >= policy.minimum_score
}

fn evaluate_window(
    bars: &[StrategyBar],
    observations: &[Option<Observation>],
    start: usize,
    end: usize,
    policy: EvaluationPolicy,
) -> WindowSummary {
    let mut result = WindowSummary::default();
    let mut index = start;
    while index < end {
        let Some(observation) =
            observations[index].filter(|observation| qualifies(*observation, policy))
        else {
            index += 1;
            continue;
        };
        let entry = index + 1;
        let Some(timeout_exit) = entry.checked_add(policy.horizon.saturating_sub(1)) else {
            break;
        };
        if entry >= end {
            index += 1;
            continue;
        }
        let exit = if policy.exit_on_reverse_signal {
            let scan_end = timeout_exit.min(end - 1);
            let reverse_exit = (entry..=scan_end).find(|candidate| {
                observations[*candidate]
                    .is_some_and(|next| next.long != observation.long && qualifies(next, policy))
            });
            match reverse_exit {
                Some(candidate) => candidate,
                None if timeout_exit < end => timeout_exit,
                None => {
                    index += 1;
                    continue;
                }
            }
        } else {
            if timeout_exit >= end {
                index += 1;
                continue;
            }
            timeout_exit
        };
        let direction = if observation.long { "LONG" } else { "SHORT" };
        let add = |summary: &mut TradeSummary| {
            summary.add(
                bars,
                entry,
                exit,
                direction,
                policy.buy_fee_pct,
                policy.sell_fee_pct,
                policy.fees_known,
            )
        };
        add(&mut result.overall);
        add(&mut result.total_bands[band_index(observation.total, TOTAL_SCORE_BAND_WIDTH)]);
        for factor in 0..4 {
            if observation.available[factor] {
                add(&mut result.factor_bands[factor]
                    [band_index(observation.factors[factor], FACTOR_SCORE_BAND_WIDTH)]);
            } else {
                add(&mut result.unavailable_factors[factor]);
            }
        }
        index = exit + 1;
    }
    result
}

fn score_weight_candidates() -> Vec<[u8; 4]> {
    let mut candidates = Vec::new();
    for nwe in (0..=SCORE_WEIGHT_BUDGET).step_by(usize::from(SCORE_WEIGHT_STEP)) {
        for dss in (0..=SCORE_WEIGHT_BUDGET - nwe).step_by(usize::from(SCORE_WEIGHT_STEP)) {
            for relative_volume in
                (0..=SCORE_WEIGHT_BUDGET - nwe - dss).step_by(usize::from(SCORE_WEIGHT_STEP))
            {
                let estimated_delta = SCORE_WEIGHT_BUDGET - nwe - dss - relative_volume;
                if estimated_delta % SCORE_WEIGHT_STEP == 0 {
                    candidates.push([nwe, dss, relative_volume, estimated_delta]);
                }
            }
        }
    }
    candidates
}

fn weight_distance_from_baseline(weights: [u8; 4]) -> u16 {
    weights
        .iter()
        .zip(BASELINE_WEIGHTS)
        .map(|(weight, baseline)| u16::from((*weight).abs_diff(baseline)))
        .sum()
}

fn fit_weights(
    bars: &[StrategyBar],
    observations: &[Option<Observation>],
    start: usize,
    end: usize,
    policy: EvaluationPolicy,
) -> (Option<[u8; 4]>, usize, TradeSummary) {
    let mut selected = None;
    let mut selected_summary = TradeSummary::default();
    let mut eligible_candidates = 0;
    for weights in score_weight_candidates() {
        let candidate_policy = EvaluationPolicy {
            weights,
            horizon: WEIGHT_FIT_EXIT_BARS,
            exit_on_reverse_signal: false,
            ..policy
        };
        let summary = evaluate_window(bars, observations, start, end, candidate_policy).overall;
        if summary.trades < MIN_WEIGHT_FIT_TRADES {
            continue;
        }
        eligible_candidates += 1;
        let win_rate = summary.wins as f64 / summary.trades as f64 * 100.0;
        if win_rate + f64::EPSILON < ENTRY_SCORE_TARGET_AFTER_FEE_WIN_RATE_PCT {
            continue;
        }
        let lower_bound = wilson_interval(summary.wins, summary.trades)
            .map(|interval| interval[0])
            .unwrap_or_default();
        let mean = summary.mean_net().unwrap_or_default();
        let best_lower_bound = wilson_interval(selected_summary.wins, selected_summary.trades)
            .map(|interval| interval[0]);
        let best_mean = selected_summary.mean_net();
        let replace = best_lower_bound.is_none_or(|best| {
            lower_bound > best + f64::EPSILON
                || ((lower_bound - best).abs() <= f64::EPSILON
                    && (best_mean.is_none_or(|best_mean| mean > best_mean + f64::EPSILON)
                        || (best_mean.is_some_and(|best_mean| {
                            (mean - best_mean).abs() <= f64::EPSILON
                        }) && weight_distance_from_baseline(weights)
                            < weight_distance_from_baseline(
                                selected.unwrap_or(BASELINE_WEIGHTS),
                            ))))
        });
        if replace {
            selected = Some(weights);
            selected_summary = summary;
        }
    }
    (selected, eligible_candidates, selected_summary)
}

fn select_exit_on_validation(
    bars: &[StrategyBar],
    observations: &[Option<Observation>],
    start: usize,
    end: usize,
    policy: EvaluationPolicy,
) -> ExitSelection {
    let mut result = ExitSelection {
        selected_policy: None,
        validation: WindowSummary::default(),
        candidates: Vec::new(),
    };
    for horizon in super::NWE_DSS_EXIT_CANDIDATES_BARS {
        for exit_on_reverse_signal in [false, true] {
            let candidate_policy = EvaluationPolicy {
                horizon: *horizon,
                exit_on_reverse_signal,
                ..policy
            };
            let candidate = evaluate_window(bars, observations, start, end, candidate_policy);
            let mut candidate_json = summary_json(candidate.overall, policy.fees_known);
            candidate_json["exit_bars"] = Value::from(*horizon);
            candidate_json["exit_rule"] = Value::from(if exit_on_reverse_signal {
                "opposite_entry_signal_or_timeout"
            } else {
                "fixed_timeout"
            });
            result.candidates.push(candidate_json);
            if candidate.overall.trades >= crate::MIN_EVIDENCE_SAMPLE_TRADES
                && candidate.overall.mean_net().is_some_and(|score| {
                    result
                        .validation
                        .overall
                        .mean_net()
                        .is_none_or(|best| score > best)
                })
            {
                result.selected_policy = Some(candidate_policy);
                result.validation = candidate;
            }
        }
    }
    result
}

fn evaluate_selected_policy(
    bars: &[StrategyBar],
    observations: &[Option<Observation>],
    policy: Option<EvaluationPolicy>,
    start: usize,
    end: usize,
) -> WindowSummary {
    policy
        .map(|selected| evaluate_window(bars, observations, start, end, selected))
        .unwrap_or_default()
}

fn hour_of_wib(timestamp_ns: i64) -> u8 {
    let hour_ns = crate::SECONDS_PER_HOUR * crate::NANOS_PER_SECOND;
    let local_ns =
        timestamp_ns.saturating_add(crate::JAKARTA_UTC_OFFSET_SECONDS * crate::NANOS_PER_SECOND);
    local_ns.div_euclid(hour_ns).rem_euclid(HOURS_PER_DAY) as u8
}

fn evaluate_hour_window(
    bars: &[StrategyBar],
    observations: &[Option<Observation>],
    start: usize,
    end: usize,
    bar_interval_ns: i64,
    entry_hour: u8,
    exit_hour: u8,
    policy: EvaluationPolicy,
) -> TradeSummary {
    let mut summary = TradeSummary::default();
    let maximum_hold = super::NWE_DSS_EXIT_CANDIDATES_BARS
        .iter()
        .copied()
        .max()
        .unwrap_or(WEIGHT_FIT_EXIT_BARS);
    let mut index = start;
    while index < end {
        let Some(observation) =
            observations[index].filter(|observation| qualifies(*observation, policy))
        else {
            index += 1;
            continue;
        };
        let entry = index + 1;
        let Some(timeout_exit) = entry.checked_add(maximum_hold.saturating_sub(1)) else {
            break;
        };
        if entry >= end || hour_of_wib(bars[entry].time) != entry_hour || timeout_exit >= end {
            index += 1;
            continue;
        }
        let exit = (entry..=timeout_exit)
            .find(|candidate| {
                hour_of_wib(bars[*candidate].time.saturating_add(bar_interval_ns)) == exit_hour
            })
            .unwrap_or(timeout_exit);
        let direction = if observation.long { "LONG" } else { "SHORT" };
        summary.add(
            bars,
            entry,
            exit,
            direction,
            policy.buy_fee_pct,
            policy.sell_fee_pct,
            policy.fees_known,
        );
        index = exit + 1;
    }
    summary
}

fn select_hour_pair_on_validation(
    bars: &[StrategyBar],
    observations: &[Option<Observation>],
    start: usize,
    end: usize,
    bar_interval_ns: i64,
    policy: EvaluationPolicy,
) -> HourSelection {
    let mut eligible = Vec::new();
    for entry_hour in 0..HOURS_PER_DAY as u8 {
        for exit_hour in 0..HOURS_PER_DAY as u8 {
            let summary = evaluate_hour_window(
                bars,
                observations,
                start,
                end,
                bar_interval_ns,
                entry_hour,
                exit_hour,
                policy,
            );
            if summary.trades >= MIN_HOUR_SELECTION_TRADES {
                eligible.push(HourPair {
                    entry_hour,
                    exit_hour,
                    summary,
                });
            }
        }
    }
    eligible.sort_by(|left, right| {
        let mean_order = right
            .summary
            .mean_net()
            .unwrap_or_default()
            .partial_cmp(&left.summary.mean_net().unwrap_or_default())
            .unwrap_or(std::cmp::Ordering::Equal);
        mean_order
            .then_with(|| right.summary.trades.cmp(&left.summary.trades))
            .then_with(|| left.entry_hour.cmp(&right.entry_hour))
            .then_with(|| left.exit_hour.cmp(&right.exit_hour))
    });
    let eligible_pairs = eligible.len();
    eligible.truncate(TOP_HOUR_PAIRS_TO_REPORT);
    HourSelection {
        selected: eligible.first().copied(),
        eligible_pairs,
        top_candidates: eligible,
    }
}

fn hour_pair_json(pair: HourPair, fees_known: bool) -> Value {
    let mut value = summary_json(pair.summary, fees_known);
    value["entry_hour_wib"] = Value::from(pair.entry_hour);
    value["exit_hour_wib"] = Value::from(pair.exit_hour);
    value
}

fn weighted_score_autocorrelation(
    observations: &[Option<Observation>],
    start: usize,
    end: usize,
    weights: [u8; 4],
) -> Value {
    let mut correlation = LagOneCorrelation::default();
    for observation in observations
        .get(start..end)
        .unwrap_or_default()
        .iter()
        .flatten()
    {
        correlation.add(score_with_weights(*observation, weights));
    }
    correlation.to_json("Consecutive causal lead-model score events in chronological training, including events below the entry score floor.")
}

fn hour_rule_report(
    bars: &[StrategyBar],
    observations: &[Option<Observation>],
    validation_start: usize,
    validation_end: usize,
    holdout_end: usize,
    bar_interval_ns: i64,
    weights: Option<[u8; 4]>,
    policy: EvaluationPolicy,
) -> Value {
    let unavailable = |status: &str, reason: &str| {
        json!({
            "status":status,"reason":reason,"timezone":"Asia/Jakarta (WIB; UTC+07:00)",
            "candidate_pair_count":HOURS_PER_DAY * HOURS_PER_DAY,
            "minimum_validation_trades":MIN_HOUR_SELECTION_TRADES,
            "maximum_hold_bars":super::NWE_DSS_EXIT_CANDIDATES_BARS.iter().copied().max().unwrap_or(WEIGHT_FIT_EXIT_BARS),
            "candidate":Value::Null,"top_validation_pairs":[],"live_weights_changed":false,
        })
    };
    if bar_interval_ns <= 0 {
        return unavailable("INVALID_BAR_INTERVAL", "The candle interval is unavailable, so local entry and close hours cannot be calculated.");
    }
    if bar_interval_ns >= crate::NANOS_PER_DAY {
        return unavailable("INTRADAY_HOURS_NOT_APPLICABLE", "Daily and weekly bars do not contain enough intraday timing detail to learn entry or exit hours.");
    }
    let Some(weights) = weights else {
        return unavailable("WEIGHTS_UNAVAILABLE", "A fee-aware training weight candidate is required before searching entry and exit hours.");
    };
    let hour_policy = EvaluationPolicy { weights, ..policy };
    let selection = select_hour_pair_on_validation(
        bars,
        observations,
        validation_start,
        validation_end,
        bar_interval_ns,
        hour_policy,
    );
    let Some(selected) = selection.selected else {
        return json!({
            "status":"INSUFFICIENT_VALIDATION_SAMPLE",
            "reason":"No entry-hour and exit-hour pair reached the minimum validation trade count.",
            "timezone":"Asia/Jakarta (WIB; UTC+07:00)","candidate_pair_count":HOURS_PER_DAY * HOURS_PER_DAY,
            "eligible_pair_count":selection.eligible_pairs,"minimum_validation_trades":MIN_HOUR_SELECTION_TRADES,
            "maximum_hold_bars":super::NWE_DSS_EXIT_CANDIDATES_BARS.iter().copied().max().unwrap_or(WEIGHT_FIT_EXIT_BARS),
            "weights":weights,"candidate":Value::Null,
            "top_validation_pairs":selection.top_candidates.iter().map(|pair| hour_pair_json(*pair, policy.fees_known)).collect::<Vec<_>>(),
            "live_weights_changed":false,
        });
    };
    let holdout = evaluate_hour_window(
        bars,
        observations,
        validation_end,
        holdout_end,
        bar_interval_ns,
        selected.entry_hour,
        selected.exit_hour,
        hour_policy,
    );
    let maximum_hold_bars = super::NWE_DSS_EXIT_CANDIDATES_BARS
        .iter()
        .copied()
        .max()
        .unwrap_or(WEIGHT_FIT_EXIT_BARS);
    json!({
        "status":"HOUR_PAIR_SELECTED",
        "reason":"The pair was selected on validation data and then evaluated on the untouched holdout.",
        "timezone":"Asia/Jakarta (WIB; UTC+07:00)","candidate_pair_count":HOURS_PER_DAY * HOURS_PER_DAY,
        "eligible_pair_count":selection.eligible_pairs,"minimum_validation_trades":MIN_HOUR_SELECTION_TRADES,
        "maximum_hold_bars":maximum_hold_bars,"weights":weights,
        "entry_hour_basis":"Open time of the bar after the qualifying signal bar.",
        "exit_hour_basis":"Close time of the first bar at the selected hour; if it is not reached within the maximum hold, exit at the maximum-hold close.",
        "selection_metric":"Highest mean return after configured transaction fees; ties favor more validation trades, then earlier entry hour, then earlier exit hour.",
        "candidate":{"entry_hour_wib":selected.entry_hour,"exit_hour_wib":selected.exit_hour,
            "validation":summary_json(selected.summary, policy.fees_known),
            "holdout":summary_json(holdout, policy.fees_known)},
        "top_validation_pairs":selection.top_candidates.iter().map(|pair| hour_pair_json(*pair, policy.fees_known)).collect::<Vec<_>>(),
        "live_weights_changed":false,
    })
}

fn selected_exit_rule(policy: Option<EvaluationPolicy>) -> Option<&'static str> {
    policy.map(|selected| {
        if selected.exit_on_reverse_signal {
            "opposite_entry_signal_or_timeout"
        } else {
            "fixed_timeout"
        }
    })
}

fn policy_comparison_json(
    weights: [u8; 4],
    training_fit: TradeSummary,
    selection: &ExitSelection,
    holdout: WindowSummary,
    fees_known: bool,
) -> Value {
    json!({
        "weights": weights,
        "training_fit": summary_json(training_fit, fees_known),
        "selected_exit_bars": selection.selected_policy.map(|selected| selected.horizon),
        "selected_exit_rule": selected_exit_rule(selection.selected_policy),
        "validation": summary_json(selection.validation.overall, fees_known),
        "validation_exit_candidates": selection.candidates,
        "holdout": summary_json(holdout.overall, fees_known),
    })
}

fn summary_json(summary: TradeSummary, fees_known: bool) -> Value {
    let mut value = summary.to_json(fees_known);
    if value["profit_factor_unbounded"] == true {
        value["profit_factor"] = Value::Null;
    }
    value
}

fn score_bands_json(bands: &[TradeSummary; BAND_COUNT], width: f64, fees_known: bool) -> Value {
    Value::Array(
        bands
            .iter()
            .enumerate()
            .map(|(index, summary)| {
                let mut value = summary_json(*summary, fees_known);
                value["label"] = Value::from(format!(
                    "{}–{}{}",
                    index as f64 * width,
                    (index + 1) as f64 * width,
                    if index == BAND_COUNT - 1 {
                        " inclusive"
                    } else {
                        " (upper exclusive)"
                    }
                ));
                value["min_score"] = Value::from(index as f64 * width);
                value["max_score"] = Value::from((index + 1) as f64 * width);
                value["max_inclusive"] = Value::from(index == BAND_COUNT - 1);
                value
            })
            .collect(),
    )
}

fn score_discrimination_supported(bands: &[TradeSummary; BAND_COUNT]) -> bool {
    let populated: Vec<_> = bands
        .iter()
        .filter(|summary| summary.trades >= crate::MIN_EVIDENCE_SAMPLE_TRADES)
        .collect();
    let (Some(bottom), Some(top)) = (populated.first(), populated.last()) else {
        return false;
    };
    if populated.len() < 2 {
        return false;
    }
    let (Some(bottom_interval), Some(top_interval)) = (
        wilson_interval(bottom.wins, bottom.trades),
        wilson_interval(top.wins, top.trades),
    ) else {
        return false;
    };
    top_interval[0] > bottom_interval[1]
        && top
            .mean_net()
            .zip(bottom.mean_net())
            .is_some_and(|(high, low)| high > low)
}

fn valid_history(bars: &[StrategyBar]) -> bool {
    bars.iter().all(|bar| {
        [bar.open, bar.high, bar.low, bar.close, bar.volume]
            .into_iter()
            .all(f64::is_finite)
            && bar.open > 0.0
            && bar.close > 0.0
            && bar.low > 0.0
            && bar.volume >= 0.0
            && bar.low <= bar.open.min(bar.close)
            && bar.high >= bar.open.max(bar.close)
    }) && bars.windows(2).all(|pair| pair[0].time < pair[1].time)
}

fn split_json(bars: &[StrategyBar], start: usize, end: usize) -> Value {
    json!({ "bars": end.saturating_sub(start),
        "start_ts": bars.get(start).filter(|_| start < end).map(|bar| bar.time),
        "end_ts": end.checked_sub(1).and_then(|index| bars.get(index)).filter(|_| start < end).map(|bar| bar.time) })
}

/// Evaluate the actual live entry score at signal time and compare fixed-time
/// exits with exits on the first qualifying opposite entry signal. This is
/// research evidence and never grants sizing readiness.
pub fn evaluate_entry_score_backtest(
    bars: &[StrategyBar],
    kind: EntryModelKind,
    bar_interval_ns: i64,
    buy_fee_pct: f64,
    sell_fee_pct: f64,
    fees_known: bool,
    allow_short: bool,
) -> Value {
    let minimum_history = kind.minimum_history_bars();
    let warmup_end = minimum_history.saturating_sub(1).min(bars.len());
    let scored_bars = bars.len().saturating_sub(warmup_end);
    let train_end = warmup_end + scored_bars * TRAINING_SHARE / 100;
    let validation_end = warmup_end + scored_bars * VALIDATION_END_SHARE / 100;
    let history_days = bars
        .first()
        .zip(bars.last())
        .map(|(first, last)| last.time.saturating_sub(first.time).max(0) / crate::NANOS_PER_DAY)
        .unwrap_or(0);
    let holdout_window_days = bars
        .get(validation_end)
        .zip(bars.last())
        .map(|(first, last)| last.time.saturating_sub(first.time).max(0) / crate::NANOS_PER_DAY)
        .unwrap_or(0);
    let invalid = !valid_history(bars)
        || !buy_fee_pct.is_finite()
        || !sell_fee_pct.is_finite()
        || buy_fee_pct < 0.0
        || sell_fee_pct < 0.0;
    let mut observations = vec![None; bars.len()];
    if !invalid && bars.len() >= minimum_history {
        visit_entry_signals(bars, kind, |index, signal| {
            if let Some(score) = score_entry_signal(&bars[..=index], signal, kind) {
                observations[index] = Some(Observation {
                    long: signal.direction == "LONG",
                    total: score.total_score,
                    factors: std::array::from_fn(|factor| score.components[factor].points),
                    available: std::array::from_fn(|factor| {
                        !score.components[factor].state.ends_with("UNAVAILABLE")
                    }),
                });
            }
        });
    }
    let baseline_policy = EvaluationPolicy {
        horizon: WEIGHT_FIT_EXIT_BARS,
        exit_on_reverse_signal: false,
        weights: BASELINE_WEIGHTS,
        minimum_score: crate::FOUR_INDICATOR_MINIMUM_SCORE_OUT_OF_40,
        buy_fee_pct,
        sell_fee_pct,
        fees_known,
        allow_short,
    };
    let empty_selection = || ExitSelection {
        selected_policy: None,
        validation: WindowSummary::default(),
        candidates: Vec::new(),
    };
    let baseline_training_fit = if invalid {
        TradeSummary::default()
    } else {
        evaluate_window(bars, &observations, warmup_end, train_end, baseline_policy).overall
    };
    let baseline_selection = if invalid {
        empty_selection()
    } else {
        select_exit_on_validation(
            bars,
            &observations,
            train_end,
            validation_end,
            baseline_policy,
        )
    };
    let training = evaluate_selected_policy(
        bars,
        &observations,
        baseline_selection.selected_policy,
        warmup_end,
        train_end,
    );
    let holdout = evaluate_selected_policy(
        bars,
        &observations,
        baseline_selection.selected_policy,
        validation_end,
        bars.len(),
    );

    let (selected_weights, eligible_weight_candidates, selected_training_fit) =
        if invalid || !fees_known {
            (None, 0, TradeSummary::default())
        } else {
            fit_weights(bars, &observations, warmup_end, train_end, baseline_policy)
        };
    let learned_selection = selected_weights.map(|weights| {
        select_exit_on_validation(
            bars,
            &observations,
            train_end,
            validation_end,
            EvaluationPolicy {
                weights,
                ..baseline_policy
            },
        )
    });
    let learned_holdout = learned_selection
        .as_ref()
        .map(|selection| {
            evaluate_selected_policy(
                bars,
                &observations,
                selection.selected_policy,
                validation_end,
                bars.len(),
            )
        })
        .unwrap_or_default();
    let learned_validation = learned_selection
        .as_ref()
        .map(|selection| selection.validation.overall)
        .unwrap_or_default();
    let after_fee_target_met = |summary: TradeSummary| {
        fees_known
            && summary.trades >= MIN_WEIGHT_FIT_TRADES
            && summary.wins as f64 / summary.trades as f64 * 100.0
                >= ENTRY_SCORE_TARGET_AFTER_FEE_WIN_RATE_PCT
            && wilson_interval(summary.wins, summary.trades)
                .is_some_and(|interval| interval[0] >= ENTRY_SCORE_TARGET_AFTER_FEE_WIN_RATE_PCT)
    };
    let weight_learning_status = if invalid {
        "INVALID_HISTORY"
    } else if bars.len() < minimum_history {
        "INSUFFICIENT_HISTORY"
    } else if !fees_known {
        "FEES_UNKNOWN"
    } else if selected_weights.is_none() {
        if eligible_weight_candidates > 0 {
            "AFTER_FEE_WIN_RATE_TARGET_NOT_REACHED"
        } else {
            "INSUFFICIENT_TRAINING_SAMPLE"
        }
    } else if learned_selection
        .as_ref()
        .is_some_and(|selection| selection.selected_policy.is_none())
    {
        "NO_VALIDATION_EXIT"
    } else {
        "CANDIDATE_READY"
    };
    let weight_learning_reason = match weight_learning_status {
        "INVALID_HISTORY" => "The candle history or fee inputs are invalid; no candidate weights were fitted.",
        "INSUFFICIENT_HISTORY" => "The strategy warm-up is incomplete; no candidate weights were fitted.",
        "FEES_UNKNOWN" => "Weight fitting requires known transaction fees; the equal-weight gross-only backtest remains available.",
        "INSUFFICIENT_TRAINING_SAMPLE" => "No candidate weight vector produced the minimum 30 non-overlapping training outcomes at the existing score floor.",
        "AFTER_FEE_WIN_RATE_TARGET_NOT_REACHED" => "Training candidates had enough outcomes, but none reached the 70% after-fee win-rate target; live weights stay at the active validated profile.",
        "NO_VALIDATION_EXIT" => "Weights were selected on training data, but no exit candidate met the minimum validation sample.",
        _ => "A training-only weight candidate and validation-selected exit are available for untouched holdout comparison.",
    };
    let baseline_weight_comparison = policy_comparison_json(
        BASELINE_WEIGHTS,
        baseline_training_fit,
        &baseline_selection,
        holdout,
        fees_known,
    );
    let learned_weight_comparison =
        selected_weights
            .zip(learned_selection.as_ref())
            .map(|(weights, selection)| {
                policy_comparison_json(
                    weights,
                    selected_training_fit,
                    selection,
                    learned_holdout,
                    fees_known,
                )
            });
    let weight_learning = json!({
        "status": weight_learning_status,
        "reason": weight_learning_reason,
        "score_formula": "sum(component_points * weight_points / 10); unavailable factors contribute zero without redistributing their weight",
        "score_floor_out_of_40": crate::FOUR_INDICATOR_MINIMUM_SCORE_OUT_OF_40,
        "weight_budget": SCORE_WEIGHT_BUDGET,
        "weight_step": SCORE_WEIGHT_STEP,
        "candidate_count": score_weight_candidates().len(),
        "eligible_candidate_count": eligible_weight_candidates,
        "minimum_training_trades": MIN_WEIGHT_FIT_TRADES,
        "training_exit_bars": WEIGHT_FIT_EXIT_BARS,
        "target_after_fee_win_rate_pct": ENTRY_SCORE_TARGET_AFTER_FEE_WIN_RATE_PCT,
        "selection_metric": "Among candidates with at least 30 non-overlapping training outcomes and at least 70% wins after configured transaction fees, maximize the Wilson 95% lower bound for after-fee win rate; tie-break by mean net return, then proximity to equal weights.",
        "validation_after_fee_target_met": after_fee_target_met(learned_validation),
        "holdout_after_fee_target_met": after_fee_target_met(learned_holdout.overall),
        "selected_weights": selected_weights,
        "weighted_score_autocorrelation": selected_weights.map(|weights|
            weighted_score_autocorrelation(&observations, warmup_end, train_end, weights)),
        "trade_return_autocorrelation": selected_training_fit.return_autocorrelation_json(),
        "baseline": baseline_weight_comparison,
        "candidate": learned_weight_comparison,
        "hour_rule": hour_rule_report(&bars, &observations, train_end, validation_end, bars.len(),
            bar_interval_ns, selected_weights, baseline_policy),
        "live_weights_changed": false,
    });
    let profitable = holdout.overall.mean_net().is_some_and(|mean| mean > 0.0) && {
        let value = summary_json(holdout.overall, fees_known);
        value["profit_factor_unbounded"] == true
            || value["profit_factor"]
                .as_f64()
                .is_some_and(|factor| factor > 1.0)
    };
    json!({
        "status": if invalid { "INVALID_HISTORY" } else if bars.len() < minimum_history { "INSUFFICIENT_HISTORY" }
            else if baseline_selection.selected_policy.is_none() { "NO_VALIDATION_TRADES" } else { "HOLDOUT_RESEARCH_COMPLETE" },
        "protocol_id": ENTRY_SCORE_VALIDATION_PROTOCOL_ID,
        "entry_model_kind": match kind { EntryModelKind::Nwe => "nwe", EntryModelKind::Dss => "dss" },
        "score_model_id": kind.score_model_id(),
        "history_bars": bars.len(), "minimum_history_bars": minimum_history,
        "history_days": history_days, "holdout_window_days": holdout_window_days,
        "selected_exit_bars": baseline_selection.selected_policy.map(|selected| selected.horizon),
        "selected_exit_rule": selected_exit_rule(baseline_selection.selected_policy),
        "exit_candidates_bars": super::NWE_DSS_EXIT_CANDIDATES_BARS,
        "exit_candidates": ["fixed_timeout", "opposite_entry_signal_or_timeout"],
        "splits": { "warmup": split_json(bars, 0, warmup_end),
            "training": split_json(bars, warmup_end, train_end),
            "validation": split_json(bars, train_end, validation_end),
            "holdout": split_json(bars, validation_end, bars.len()) },
        "validation_exit_candidates": baseline_selection.candidates,
        "training": summary_json(training.overall, fees_known),
        "validation": summary_json(baseline_selection.validation.overall, fees_known),
        "holdout": summary_json(holdout.overall, fees_known),
        "holdout_score_bands": score_bands_json(&holdout.total_bands, TOTAL_SCORE_BAND_WIDTH, fees_known),
        "holdout_factor_bands": FACTOR_NAMES.iter().enumerate().map(|(index, name)| json!({
            "indicator": name, "score_scale_max": crate::FOUR_INDICATOR_COMPONENT_SCORE_SCALE_MAX,
            "bands": score_bands_json(&holdout.factor_bands[index], FACTOR_SCORE_BAND_WIDTH, fees_known),
            "unavailable": summary_json(holdout.unavailable_factors[index], fees_known)
        })).collect::<Vec<_>>(),
        "weight_learning": weight_learning,
        "checks": { "holdout_sample_sufficient": holdout.overall.trades >= crate::MIN_EVIDENCE_SAMPLE_TRADES,
            "holdout_span_sufficient": !invalid && holdout_window_days >= crate::MIN_EVIDENCE_SAMPLE_DAYS,
            "profitable_holdout": profitable,
            "score_discrimination_supported": score_discrimination_supported(&holdout.total_bands) },
        "minimum_holdout_trades": crate::MIN_EVIDENCE_SAMPLE_TRADES,
        "minimum_holdout_days": crate::MIN_EVIDENCE_SAMPLE_DAYS,
        "fees_known": fees_known,
        "weight_promotion_target_after_fee_win_rate_pct": ENTRY_SCORE_TARGET_AFTER_FEE_WIN_RATE_PCT,
        "selection_metric": "Equal-weight baseline exit: highest validation mean return; ties favor the shorter maximum hold, then fixed timeout. Fee-unknown results are gross only.",
        "entry_rule": "Completed-bar live lead model; enter next bar open; no overlapping trades or trades crossing split boundaries.",
        "exit_rule": "Validation compares fixed bar-close timeouts with a close on the first opposite-direction entry signal that qualifies under the tested weights and existing score floor, capped by the same maximum hold. Each selected policy is evaluated once on the untouched holdout.",
        "discrimination_note": "Exploratory conservative screen: at least two ordered score bands each meet the minimum trade count; the highest eligible band's lower Wilson 95% win bound exceeds the lowest eligible band's upper bound and its mean return is higher. This is not independent statistical proof.",
        "validation_note": "Historical research only. The bounded weight grid is fitted on training data using a fixed 8-bar timeout; validation selects the exit rule and maximum hold for the equal-weight baseline and fitted candidate independently, and selects the WIB entry/exit-hour pair for the fitted candidate. The final chronological tail evaluates each selected policy once and never selects weights, exits, or hours. Reusing the tail for model or threshold tuning contaminates it. No slippage, spread, depth, market impact, funding, or borrow is modeled."
    })
}

#[cfg(test)]
#[path = "entry_score_tests.rs"]
mod tests;
