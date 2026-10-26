use super::nwe_dss::{wilson_interval, TradeSummary};
use crate::strategies::{score_entry_signal, visit_entry_signals, EntryModelKind, StrategyBar};
use serde_json::{json, Value};

const TRAINING_SHARE: usize = 60;
const VALIDATION_END_SHARE: usize = 80;
const TOTAL_SCORE_BAND_WIDTH: f64 = 10.0;
const FACTOR_SCORE_BAND_WIDTH: f64 = 2.5;
const BAND_COUNT: usize = 4;
const FACTOR_NAMES: [&str; 4] = [
    "Nadaraya-Watson Envelope", "DSS Bressert", "Relative Volume",
    "Estimated Volume Delta (OHLCV proxy)",
];
pub const ENTRY_SCORE_VALIDATION_PROTOCOL_ID: &str = "entry_four_factor_holdout_v2";

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
    buy_fee_pct: f64,
    sell_fee_pct: f64,
    fees_known: bool,
    allow_short: bool,
}

fn band_index(points: f64, width: f64) -> usize {
    ((points / width).floor() as usize).min(BAND_COUNT - 1)
}

fn evaluate_window(
    bars: &[StrategyBar], observations: &[Option<Observation>],
    start: usize, end: usize, policy: EvaluationPolicy,
) -> WindowSummary {
    let mut result = WindowSummary::default();
    let mut index = start;
    while index < end {
        let Some(observation) = observations[index] else { index += 1; continue; };
        if !observation.long && !policy.allow_short { index += 1; continue; }
        let entry = index + 1;
        let Some(timeout_exit) = entry.checked_add(policy.horizon.saturating_sub(1)) else { break; };
        if entry >= end { index += 1; continue; }
        let exit = if policy.exit_on_reverse_signal {
            let scan_end = timeout_exit.min(end - 1);
            let reverse_exit = (entry..=scan_end).find(|candidate| {
                observations[*candidate].is_some_and(|next| next.long != observation.long)
            });
            match reverse_exit {
                Some(candidate) => candidate,
                None if timeout_exit < end => timeout_exit,
                None => { index += 1; continue; }
            }
        } else {
            if timeout_exit >= end { index += 1; continue; }
            timeout_exit
        };
        let direction = if observation.long { "LONG" } else { "SHORT" };
        let add = |summary: &mut TradeSummary| summary.add(
            bars, entry, exit, direction, policy.buy_fee_pct, policy.sell_fee_pct, policy.fees_known,
        );
        add(&mut result.overall);
        add(&mut result.total_bands[band_index(observation.total, TOTAL_SCORE_BAND_WIDTH)]);
        for factor in 0..4 {
            if observation.available[factor] {
                add(&mut result.factor_bands[factor][band_index(observation.factors[factor], FACTOR_SCORE_BAND_WIDTH)]);
            } else {
                add(&mut result.unavailable_factors[factor]);
            }
        }
        index = exit + 1;
    }
    result
}

fn summary_json(summary: TradeSummary, fees_known: bool) -> Value {
    let mut value = summary.to_json(fees_known);
    if value["profit_factor_unbounded"] == true {
        value["profit_factor"] = Value::Null;
    }
    value
}

fn score_bands_json(bands: &[TradeSummary; BAND_COUNT], width: f64, fees_known: bool) -> Value {
    Value::Array(bands.iter().enumerate().map(|(index, summary)| {
        let mut value = summary_json(*summary, fees_known);
        value["label"] = Value::from(format!("{}–{}{}", index as f64 * width, (index + 1) as f64 * width,
            if index == BAND_COUNT - 1 { " inclusive" } else { " (upper exclusive)" }));
        value["min_score"] = Value::from(index as f64 * width);
        value["max_score"] = Value::from((index + 1) as f64 * width);
        value["max_inclusive"] = Value::from(index == BAND_COUNT - 1);
        value
    }).collect())
}

fn score_discrimination_supported(bands: &[TradeSummary; BAND_COUNT]) -> bool {
    let populated: Vec<_> = bands.iter()
        .filter(|summary| summary.trades >= crate::MIN_EVIDENCE_SAMPLE_TRADES).collect();
    let (Some(bottom), Some(top)) = (populated.first(), populated.last()) else { return false; };
    if populated.len() < 2 { return false; }
    let (Some(bottom_interval), Some(top_interval)) = (
        wilson_interval(bottom.wins, bottom.trades), wilson_interval(top.wins, top.trades),
    ) else { return false; };
    top_interval[0] > bottom_interval[1]
        && top.mean_net().zip(bottom.mean_net()).is_some_and(|(high, low)| high > low)
}

fn valid_history(bars: &[StrategyBar]) -> bool {
    bars.iter().all(|bar| {
        [bar.open, bar.high, bar.low, bar.close, bar.volume].into_iter().all(f64::is_finite)
            && bar.open > 0.0 && bar.close > 0.0 && bar.low > 0.0 && bar.volume >= 0.0
            && bar.low <= bar.open.min(bar.close) && bar.high >= bar.open.max(bar.close)
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
    bars: &[StrategyBar], kind: EntryModelKind,
    buy_fee_pct: f64, sell_fee_pct: f64, fees_known: bool, allow_short: bool,
) -> Value {
    let minimum_history = kind.minimum_history_bars();
    let train_end = (bars.len() * TRAINING_SHARE / 100).max(minimum_history).min(bars.len());
    let validation_end = (bars.len() * VALIDATION_END_SHARE / 100).max(train_end).min(bars.len());
    let history_days = bars.first().zip(bars.last()).map(|(first, last)|
        last.time.saturating_sub(first.time).max(0) / crate::NANOS_PER_DAY).unwrap_or(0);
    let holdout_window_days = bars.get(validation_end).zip(bars.last()).map(|(first, last)|
        last.time.saturating_sub(first.time).max(0) / crate::NANOS_PER_DAY).unwrap_or(0);
    let invalid = !valid_history(bars) || !buy_fee_pct.is_finite() || !sell_fee_pct.is_finite()
        || buy_fee_pct < 0.0 || sell_fee_pct < 0.0;
    let mut observations = vec![None; bars.len()];
    if !invalid && bars.len() >= minimum_history {
        visit_entry_signals(bars, kind, |index, signal| {
            if let Some(score) = score_entry_signal(&bars[..=index], signal, kind)
                .filter(|score| score.total_score >= crate::FOUR_INDICATOR_MINIMUM_SCORE_OUT_OF_40)
            {
                observations[index] = Some(Observation {
                    long: signal.direction == "LONG", total: score.total_score,
                    factors: std::array::from_fn(|factor| score.components[factor].points),
                    available: std::array::from_fn(|factor| !score.components[factor].state.ends_with("UNAVAILABLE")),
                });
            }
        });
    }
    let policy = EvaluationPolicy { horizon: 0, exit_on_reverse_signal: false,
        buy_fee_pct, sell_fee_pct, fees_known, allow_short };
    let mut selected_policy = None;
    let mut validation = WindowSummary::default();
    let mut validation_candidates = Vec::new();
    if !invalid {
        for horizon in super::NWE_DSS_EXIT_CANDIDATES_BARS {
            for exit_on_reverse_signal in [false, true] {
                let candidate_policy = EvaluationPolicy { horizon: *horizon,
                    exit_on_reverse_signal, ..policy };
                let candidate = evaluate_window(bars, &observations, train_end, validation_end,
                    candidate_policy);
                let mut candidate_json = summary_json(candidate.overall, fees_known);
                candidate_json["exit_bars"] = Value::from(*horizon);
                candidate_json["exit_rule"] = Value::from(if exit_on_reverse_signal {
                    "opposite_entry_signal_or_timeout"
                } else { "fixed_timeout" });
                validation_candidates.push(candidate_json);
                if candidate.overall.trades >= crate::MIN_EVIDENCE_SAMPLE_TRADES
                    && candidate.overall.mean_net().is_some_and(|score|
                        validation.overall.mean_net().is_none_or(|best| score > best))
                {
                    selected_policy = Some(candidate_policy);
                    validation = candidate;
                }
            }
        }
    }
    let training = selected_policy.map(|selected| evaluate_window(bars, &observations,
        minimum_history.saturating_sub(1).min(train_end), train_end,
        selected)).unwrap_or_default();
    let holdout = selected_policy.map(|selected| evaluate_window(bars, &observations,
        validation_end, bars.len(), selected)).unwrap_or_default();
    let profitable = holdout.overall.mean_net().is_some_and(|mean| mean > 0.0)
        && { let value = summary_json(holdout.overall, fees_known);
            value["profit_factor_unbounded"] == true || value["profit_factor"].as_f64().is_some_and(|factor| factor > 1.0) };
    json!({
        "status": if invalid { "INVALID_HISTORY" } else if bars.len() < minimum_history { "INSUFFICIENT_HISTORY" }
            else if selected_policy.is_none() { "NO_VALIDATION_TRADES" } else { "HOLDOUT_RESEARCH_COMPLETE" },
        "protocol_id": ENTRY_SCORE_VALIDATION_PROTOCOL_ID,
        "entry_model_kind": match kind { EntryModelKind::Nwe => "nwe", EntryModelKind::Dss => "dss" },
        "score_model_id": kind.score_model_id(),
        "history_bars": bars.len(), "minimum_history_bars": minimum_history,
        "history_days": history_days, "holdout_window_days": holdout_window_days,
        "selected_exit_bars": selected_policy.map(|selected| selected.horizon),
        "selected_exit_rule": selected_policy.map(|selected| if selected.exit_on_reverse_signal {
            "opposite_entry_signal_or_timeout"
        } else { "fixed_timeout" }),
        "exit_candidates_bars": super::NWE_DSS_EXIT_CANDIDATES_BARS,
        "exit_candidates": ["fixed_timeout", "opposite_entry_signal_or_timeout"],
        "splits": { "training": split_json(bars, 0, train_end),
            "validation": split_json(bars, train_end, validation_end),
            "holdout": split_json(bars, validation_end, bars.len()) },
        "validation_exit_candidates": validation_candidates,
        "training": summary_json(training.overall, fees_known),
        "validation": summary_json(validation.overall, fees_known),
        "holdout": summary_json(holdout.overall, fees_known),
        "holdout_score_bands": score_bands_json(&holdout.total_bands, TOTAL_SCORE_BAND_WIDTH, fees_known),
        "holdout_factor_bands": FACTOR_NAMES.iter().enumerate().map(|(index, name)| json!({
            "indicator": name, "score_scale_max": crate::FOUR_INDICATOR_COMPONENT_SCORE_SCALE_MAX,
            "bands": score_bands_json(&holdout.factor_bands[index], FACTOR_SCORE_BAND_WIDTH, fees_known),
            "unavailable": summary_json(holdout.unavailable_factors[index], fees_known)
        })).collect::<Vec<_>>(),
        "checks": { "holdout_sample_sufficient": holdout.overall.trades >= crate::MIN_EVIDENCE_SAMPLE_TRADES,
            "holdout_span_sufficient": !invalid && holdout_window_days >= crate::MIN_EVIDENCE_SAMPLE_DAYS,
            "profitable_holdout": profitable,
            "score_discrimination_supported": score_discrimination_supported(&holdout.total_bands) },
        "minimum_holdout_trades": crate::MIN_EVIDENCE_SAMPLE_TRADES,
        "minimum_holdout_days": crate::MIN_EVIDENCE_SAMPLE_DAYS,
        "fees_known": fees_known,
        "selection_metric": "Highest validation mean return; ties favor the shorter maximum hold, then fixed timeout. Fee-unknown results are gross only.",
        "entry_rule": "Completed-bar live lead model; enter next bar open; no overlapping trades or trades crossing split boundaries.",
        "exit_rule": "Validation compares fixed bar-close timeouts with a close on the first qualifying opposite-direction entry signal, capped by the same maximum hold. The selected rule is evaluated once on the untouched holdout.",
        "discrimination_note": "Exploratory conservative screen: at least two ordered score bands each meet the minimum trade count; the highest eligible band's lower Wilson 95% win bound exceeds the lowest eligible band's upper bound and its mean return is higher. This is not independent statistical proof.",
        "validation_note": "Historical research only. Validation selects the exit rule and maximum hold; the final chronological tail does not select them. Reusing the tail for model or threshold tuning contaminates it. No slippage, spread, depth, market impact, funding, or borrow is modeled."
    })
}

#[cfg(test)]
#[path = "entry_score_tests.rs"]
mod tests;
