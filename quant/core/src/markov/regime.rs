//! Deterministic market-regime classification and Markov payload assembly.
//!
//! The implementation mirrors the historical Python ``MarkovRegimeEngine``
//! contract, but keeps all numerical work on bounded Rust slices.  Storage
//! and HTTP concerns remain in the sidecar crate.

use super::{DtmcModel, RegimeId};
use crate::indicators::nadaraya_watson_envelope::calculate_nadaraya_watson_endpoint_only;
use crate::{
    calculate_dss, finite, round_decimal, DssConfig, NadarayaWatsonEnvelopeConfig, StrategyBar,
};
use serde_json::{json, Value};

pub use super::dtmc::NUM_CANONICAL_REGIMES as REGIME_COUNT;
use super::dtmc::TRANSITION_DIRICHLET_ALPHA;

const MINIMUM_WALK_FORWARD_TRAINING_TRANSITIONS: usize = 30;
const MAX_REGIME_TREND_EMA_SPAN_BARS: usize = 500;
const EMA_CONTEXT_SPANS: usize = 20;

#[derive(Debug, Clone)]
pub struct MarkovAsset {
    pub storage_symbol: String,
    pub exchange_id: String,
    pub asset_category_id: String,
    pub bars: Vec<StrategyBar>,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct MarkovRegimeConfig {
    pub dss: DssConfig,
    pub nwe: NadarayaWatsonEnvelopeConfig,
    pub trend_ema_span_bars: usize,
    pub dss_oversold_max: f64,
    pub dss_overbought_min: f64,
    pub dss_neutral_level: f64,
    pub dss_bull_pullback_min: f64,
    pub dss_bull_pullback_max: f64,
    pub dss_bear_pullback_min: f64,
    pub dss_bear_pullback_max: f64,
    pub dss_near_cross_spread: f64,
    pub trend_ema_tolerance: f64,
    pub pullback_ema_tolerance: f64,
}

impl Default for MarkovRegimeConfig {
    fn default() -> Self {
        Self {
            dss: DssConfig::default(),
            nwe: NadarayaWatsonEnvelopeConfig::default(),
            trend_ema_span_bars: 50,
            dss_oversold_max: 30.0,
            dss_overbought_min: 70.0,
            dss_neutral_level: 50.0,
            dss_bull_pullback_min: 30.0,
            dss_bull_pullback_max: 65.0,
            dss_bear_pullback_min: 35.0,
            dss_bear_pullback_max: 70.0,
            dss_near_cross_spread: 10.0,
            trend_ema_tolerance: 0.01,
            pullback_ema_tolerance: 0.02,
        }
    }
}

impl MarkovRegimeConfig {
    pub fn validated(self) -> Self {
        let defaults = Self::default();
        let dss_is_valid = self.dss.pds > 0
            && self.dss.pds <= MAX_REGIME_TREND_EMA_SPAN_BARS
            && self.dss.ema_len > 0
            && self.dss.ema_len <= MAX_REGIME_TREND_EMA_SPAN_BARS
            && self.dss.trigger_len > 0
            && self.dss.trigger_len <= MAX_REGIME_TREND_EMA_SPAN_BARS;
        let dss = if dss_is_valid { self.dss } else { defaults.dss };
        let mut config = Self {
            dss,
            nwe: self.nwe.validated(),
            trend_ema_span_bars: if (1..=MAX_REGIME_TREND_EMA_SPAN_BARS)
                .contains(&self.trend_ema_span_bars)
            {
                self.trend_ema_span_bars
            } else {
                defaults.trend_ema_span_bars
            },
            dss_oversold_max: bounded_dss_level(self.dss_oversold_max, defaults.dss_oversold_max),
            dss_overbought_min: bounded_dss_level(
                self.dss_overbought_min,
                defaults.dss_overbought_min,
            ),
            dss_neutral_level: bounded_dss_level(
                self.dss_neutral_level,
                defaults.dss_neutral_level,
            ),
            dss_bull_pullback_min: bounded_dss_level(
                self.dss_bull_pullback_min,
                defaults.dss_bull_pullback_min,
            ),
            dss_bull_pullback_max: bounded_dss_level(
                self.dss_bull_pullback_max,
                defaults.dss_bull_pullback_max,
            ),
            dss_bear_pullback_min: bounded_dss_level(
                self.dss_bear_pullback_min,
                defaults.dss_bear_pullback_min,
            ),
            dss_bear_pullback_max: bounded_dss_level(
                self.dss_bear_pullback_max,
                defaults.dss_bear_pullback_max,
            ),
            dss_near_cross_spread: bounded_dss_level(
                self.dss_near_cross_spread,
                defaults.dss_near_cross_spread,
            ),
            trend_ema_tolerance: bounded_tolerance(
                self.trend_ema_tolerance,
                defaults.trend_ema_tolerance,
            ),
            pullback_ema_tolerance: bounded_tolerance(
                self.pullback_ema_tolerance,
                defaults.pullback_ema_tolerance,
            ),
        };
        if config.dss_oversold_max >= config.dss_overbought_min {
            config.dss_oversold_max = defaults.dss_oversold_max;
            config.dss_overbought_min = defaults.dss_overbought_min;
        }
        if config.dss_bull_pullback_min >= config.dss_bull_pullback_max {
            config.dss_bull_pullback_min = defaults.dss_bull_pullback_min;
            config.dss_bull_pullback_max = defaults.dss_bull_pullback_max;
        }
        if config.dss_bear_pullback_min >= config.dss_bear_pullback_max {
            config.dss_bear_pullback_min = defaults.dss_bear_pullback_min;
            config.dss_bear_pullback_max = defaults.dss_bear_pullback_max;
        }
        config
    }

    pub fn minimum_history_bars(self) -> usize {
        let config = self.validated();
        config
            .nwe
            .minimum_endpoint_envelope_bars()
            .max(config.dss.pds.saturating_mul(2))
    }

    /// Recent pre-chart bars needed to warm finite windows and recursive EMA
    /// state before calculating the visible chart regime labels.
    pub fn chart_context_warmup_bars(self) -> usize {
        let config = self.validated();
        let recursive_span = config
            .trend_ema_span_bars
            .max(config.dss.ema_len)
            .max(config.dss.trigger_len);
        let recursive_context = recursive_span.saturating_mul(EMA_CONTEXT_SPANS);
        let envelope_context = config.nwe.window_bars.saturating_mul(2);
        let stochastic_context = config.dss.pds.saturating_mul(2);
        recursive_context
            .max(envelope_context)
            .saturating_add(stochastic_context)
    }
}

fn bounded_dss_level(value: f64, fallback: f64) -> f64 {
    if value.is_finite() && (0.0..=100.0).contains(&value) {
        value
    } else {
        fallback
    }
}

fn bounded_tolerance(value: f64, fallback: f64) -> f64 {
    if value.is_finite() && (0.0..=0.5).contains(&value) {
        value
    } else {
        fallback
    }
}

fn ema(values: &[f64], span: usize) -> Vec<f64> {
    if values.is_empty() || span == 0 {
        return vec![f64::NAN; values.len()];
    }
    let alpha = 2.0 / (span as f64 + 1.0);
    let mut result = vec![f64::NAN; values.len()];
    let mut previous = f64::NAN;
    for (index, value) in values.iter().copied().enumerate() {
        if !finite(value) {
            result[index] = previous;
            continue;
        }
        previous = if finite(previous) {
            alpha * value + (1.0 - alpha) * previous
        } else {
            value
        };
        result[index] = previous;
    }
    result
}

/// Classify completed bars; `None` marks indicator warm-up or invalid inputs.
pub fn classify_regimes(
    bars: &[StrategyBar],
    config: &MarkovRegimeConfig,
) -> Vec<Option<RegimeId>> {
    let config = config.validated();
    classify_regimes_with_validated_config(bars, &config)
}

fn classify_regimes_with_validated_config(
    bars: &[StrategyBar],
    config: &MarkovRegimeConfig,
) -> Vec<Option<RegimeId>> {
    let n = bars.len();
    let mut regimes = vec![None; n];
    if n < config.minimum_history_bars() {
        return regimes;
    }
    let highs: Vec<f64> = bars.iter().map(|bar| bar.high).collect();
    let lows: Vec<f64> = bars.iter().map(|bar| bar.low).collect();
    let closes: Vec<f64> = bars.iter().map(|bar| bar.close).collect();
    let dss = calculate_dss(&highs, &lows, &closes, &config.dss);
    let trend_ema = ema(&closes, config.trend_ema_span_bars);
    let nwe = calculate_nadaraya_watson_endpoint_only(&closes, &config.nwe);

    for index in 1..n {
        let d_now = dss.dss[index];
        let d_prev = dss.dss[index - 1];
        let trigger_now = dss.trigger[index];
        let trigger_prev = dss.trigger[index - 1];
        if !finite(d_now) || !finite(d_prev) || !finite(trigger_now) || !finite(trigger_prev) {
            continue;
        }
        let spread = d_now - trigger_now;
        let spread_prev = d_prev - trigger_prev;
        let bull_cross = spread_prev <= 0.0 && spread > 0.0;
        let bear_cross = spread_prev >= 0.0 && spread < 0.0;
        let close = closes[index];
        let ema_value = trend_ema[index];
        let upper = nwe.upper[index];
        let lower = nwe.lower[index];
        let midline = nwe.midline[index];
        if ![close, ema_value, upper, lower, midline]
            .iter()
            .all(|value| finite(*value))
        {
            continue;
        }
        let above_midline = close >= midline;
        let below_midline = close <= midline;
        let lower_band_turn = close <= lower || nwe.crossed_below_lower[index];
        let upper_band_turn = close >= upper || nwe.crossed_above_upper[index];

        regimes[index] = Some(
            if lower_band_turn
                && d_now <= config.dss_oversold_max
                && (bull_cross || (d_now > d_prev && spread > -config.dss_near_cross_spread))
            {
                RegimeId::Accumulation
            } else if upper_band_turn
                && d_now >= config.dss_overbought_min
                && (bear_cross || (d_now < d_prev && spread < config.dss_near_cross_spread))
            {
                RegimeId::Distribution
            } else if (bull_cross
                && d_now > config.dss_bull_pullback_min
                && d_now < config.dss_bull_pullback_max
                && above_midline
                && close >= ema_value * (1.0 - config.pullback_ema_tolerance))
                || (bear_cross
                    && d_now > config.dss_bear_pullback_min
                    && d_now < config.dss_bear_pullback_max
                    && below_midline
                    && close <= ema_value * (1.0 + config.pullback_ema_tolerance))
            {
                RegimeId::PullbackReset
            } else if d_now > trigger_now
                && d_now >= config.dss_neutral_level
                && above_midline
                && close >= ema_value * (1.0 - config.trend_ema_tolerance)
            {
                RegimeId::BullTrend
            } else if d_now < trigger_now
                && d_now <= config.dss_neutral_level
                && below_midline
                && close <= ema_value * (1.0 + config.trend_ema_tolerance)
            {
                RegimeId::BearTrend
            } else {
                RegimeId::ConsolidationChop
            },
        );
    }
    regimes
}

fn model_value(model: &DtmcModel) -> Value {
    let percent_matrix = model
        .transition_matrix
        .iter()
        .map(|row| {
            Value::Array(
                row.iter()
                    .map(|v| json!(round_decimal(v * 100.0, 1)))
                    .collect(),
            )
        })
        .collect::<Vec<_>>();
    json!({
        "transition_matrix": percent_matrix,
        "persistence_pct": model.persistence.iter().map(|v| json!(round_decimal(v * 100.0, 1))).collect::<Vec<_>>(),
        "expected_duration_bars": model.expected_duration.iter().map(|v| json!(round_decimal(*v, 1))).collect::<Vec<_>>(),
        "stationary_distribution_pct": model.stationary_distribution.iter().map(|v| json!(round_decimal(v * 100.0, 1))).collect::<Vec<_>>(),
        "transition_row_support": model.transition_row_support,
        "transition_prior_alpha_per_destination": TRANSITION_DIRICHLET_ALPHA,
    })
}

fn regime_keys() -> Vec<&'static str> {
    (0..REGIME_COUNT)
        .map(|index| RegimeId::from_u8(index as u8).name())
        .collect()
}

fn contiguous_regime_sequences(observations: &[Option<RegimeId>]) -> Vec<Vec<usize>> {
    let mut sequences = Vec::new();
    let mut current = Vec::new();
    for observation in observations {
        if let Some(regime) = observation {
            current.push(*regime as usize);
        } else if !current.is_empty() {
            sequences.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        sequences.push(current);
    }
    sequences
}

fn walk_forward_evaluation(sequences: &[Vec<usize>]) -> Value {
    let state_count = REGIME_COUNT;
    let mut counts = vec![vec![0u64; state_count]; state_count];
    let mut scored = 0u64;
    let mut correct = 0u64;
    let mut persistence_correct = 0u64;
    let mut brier_total = 0.0;

    for sequence in sequences {
        if sequence.len() < MINIMUM_WALK_FORWARD_TRAINING_TRANSITIONS + 2 {
            continue;
        }
        counts.fill(vec![0; state_count]);
        let first_test_index = MINIMUM_WALK_FORWARD_TRAINING_TRANSITIONS + 1;
        for index in 1..sequence.len() {
            let previous = sequence[index - 1];
            let actual = sequence[index];
            if index >= first_test_index && previous < state_count && actual < state_count {
                let row_total: u64 = counts[previous].iter().sum();
                if row_total > 0 {
                    let denominator =
                        row_total as f64 + TRANSITION_DIRICHLET_ALPHA * state_count as f64;
                    let probabilities: Vec<f64> = counts[previous]
                        .iter()
                        .map(|count| (*count as f64 + TRANSITION_DIRICHLET_ALPHA) / denominator)
                        .collect();
                    let predicted = probabilities
                        .iter()
                        .enumerate()
                        .max_by(|left, right| left.1.total_cmp(right.1))
                        .map(|(state, _)| state)
                        .unwrap_or(previous);
                    correct += u64::from(predicted == actual);
                    persistence_correct += u64::from(previous == actual);
                    for (state, probability) in probabilities.iter().enumerate() {
                        let observed = f64::from(state == actual);
                        brier_total += (probability - observed).powi(2);
                    }
                    scored += 1;
                }
            }
            if previous < state_count && actual < state_count {
                counts[previous][actual] += 1;
            }
        }
    }

    if scored == 0 {
        return json!({
            "status": "insufficient_history",
            "minimum_training_transitions": MINIMUM_WALK_FORWARD_TRAINING_TRANSITIONS,
            "evaluated_transitions": 0
        });
    }
    json!({
        "status": "evaluated",
        "method": "expanding_window_one_step_ahead",
        "minimum_training_transitions": MINIMUM_WALK_FORWARD_TRAINING_TRANSITIONS,
        "evaluated_transitions": scored,
        "accuracy_pct": round_decimal(correct as f64 / scored as f64 * 100.0, 1),
        "stay_in_current_state_baseline_accuracy_pct": round_decimal(persistence_correct as f64 / scored as f64 * 100.0, 1),
        "multiclass_brier_score": round_decimal(brier_total / scored as f64, 4),
        "interpretation": "Chronological within-sample walk-forward diagnostic; not an untouched holdout or a guarantee of future performance."
    })
}

fn regime_definitions() -> Value {
    json!([
        {"id":"BULL_TREND","name":"Bull Trend Context","short_name":"BULL CONTEXT","color":"#10b981","bg_color":"rgba(16, 185, 129, 0.15)","border_color":"#059669","icon":"🚀","description":"DSS is at or above its midpoint and trigger, while price is above the causal NWE midline and 50-bar EMA context."},
        {"id":"BEAR_TREND","name":"Bear Trend Context","short_name":"BEAR CONTEXT","color":"#ef4444","bg_color":"rgba(239, 68, 68, 0.15)","border_color":"#dc2626","icon":"🔻","description":"DSS is at or below its midpoint and trigger, while price is below the causal NWE midline and 50-bar EMA context."},
        {"id":"ACCUMULATION","name":"Lower-Band Turn Context","short_name":"LOWER TURN","color":"#06b6d4","bg_color":"rgba(6, 182, 212, 0.15)","border_color":"#0891b2","icon":"💎","description":"DSS is low and turning upward near or beyond the causal NWE lower band. This label does not infer participant intent."},
        {"id":"DISTRIBUTION","name":"Upper-Band Turn Context","short_name":"UPPER TURN","color":"#f59e0b","bg_color":"rgba(245, 158, 11, 0.15)","border_color":"#d97706","icon":"⚠️","description":"DSS is high and turning downward near or beyond the causal NWE upper band. This label does not infer participant intent."},
        {"id":"PULLBACK_RESET","name":"Midrange DSS Cross Context","short_name":"MIDRANGE CROSS","color":"#8b5cf6","bg_color":"rgba(139, 92, 246, 0.15)","border_color":"#7c3aed","icon":"🎯","description":"A midrange DSS/trigger cross aligned with price relative to causal NWE midline and 50-bar EMA."},
        {"id":"CONSOLIDATION_CHOP","name":"Mixed Indicator Context","short_name":"MIXED CONTEXT","color":"#6b7280","bg_color":"rgba(107, 114, 128, 0.15)","border_color":"#4b5563","icon":"💤","description":"DSS, causal NWE position, and trend context do not meet another named state rule. This does not assert low volatility."}
    ])
}

fn classify_universe(exchange_id: &str, asset_category_id: &str) -> Option<&'static str> {
    match (exchange_id, asset_category_id) {
        ("stockbit", "idx_stock") => Some("idx"),
        ("pintu_futures", "crypto_perpetual") => Some("crypto_futures"),
        ("tokocrypto", "crypto_spot") => Some("crypto_spot"),
        ("tokocrypto", "tokenized_stock") => Some("tokenized_stock"),
        _ => None,
    }
}

fn clean_symbol(symbol: &str) -> String {
    symbol
        .strip_suffix(".JK")
        .or_else(|| symbol.strip_suffix("-USD"))
        .unwrap_or(symbol)
        .to_string()
}

fn current_price(close: f64) -> f64 {
    if close < 1.0 {
        round_decimal(close, 4)
    } else {
        round_decimal(close, 2)
    }
}

fn date_string(timestamp_ns: i64) -> String {
    // Timestamp values are UTC nanoseconds in the canonical SQLite store.
    // Convert days since Unix epoch to an ISO date without a heavyweight
    // datetime dependency in the sidecar.
    let days = timestamp_ns.div_euclid(crate::NANOS_PER_DAY);
    let (year, month, day) = crate::gregorian_date_from_unix_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

/// Build the dashboard-compatible payload from bounded per-asset bars.
/// Each asset identity comes from the registered exchange and category.
pub fn build_payload(assets: &[MarkovAsset], config: &MarkovRegimeConfig) -> Value {
    build_payload_for_timeframe(assets, config, "1d")
}

pub fn build_payload_for_timeframe(
    assets: &[MarkovAsset],
    config: &MarkovRegimeConfig,
    timeframe: &str,
) -> Value {
    let config = config.validated();
    let mut details: Vec<(Value, Vec<usize>, Vec<Vec<usize>>, &'static str)> = Vec::new();
    let mut all_sequences: Vec<Vec<usize>> = Vec::new();
    let mut universe_sequences: [Vec<Vec<usize>>; 4] =
        [Vec::new(), Vec::new(), Vec::new(), Vec::new()];

    for asset in assets {
        let Some(class) = classify_universe(&asset.exchange_id, &asset.asset_category_id) else {
            continue;
        };
        let symbol = &asset.storage_symbol;
        let bars = &asset.bars;
        if bars.len() < config.minimum_history_bars() {
            continue;
        }
        let observations = classify_regimes_with_validated_config(bars, &config);
        let sequence: Vec<usize> = observations
            .iter()
            .flatten()
            .map(|regime| *regime as usize)
            .collect();
        let Some(current) = observations.last().copied().flatten() else {
            continue;
        };
        if sequence.len() < 2 {
            continue;
        }
        let contiguous_sequences = contiguous_regime_sequences(&observations);
        all_sequences.extend(contiguous_sequences.iter().cloned());
        let class_index = match class {
            "crypto_futures" => 0,
            "crypto_spot" => 1,
            "idx" => 2,
            "tokenized_stock" => 3,
            _ => unreachable!("registered Markov universe class is exhaustive"),
        };
        universe_sequences[class_index].extend(contiguous_sequences);
        let current = current as usize;
        let dwell = sequence
            .iter()
            .rev()
            .take_while(|value| **value == current)
            .count();
        let last = bars.last().unwrap();
        details.push((
            json!({
                "symbol": symbol,
                "exchange_id": asset.exchange_id,
                "asset_category_id": asset.asset_category_id,
                "model_key": format!("{}:{}:{}", asset.exchange_id, asset.asset_category_id, symbol),
                "clean_symbol": clean_symbol(symbol),
                "class": class,
                "total_bars": sequence.len(),
                "available_bars": bars.len(),
                "classified_bars": sequence.len(),
                "warmup_bars": bars.len().saturating_sub(sequence.len()),
                "classification_coverage_pct": round_decimal(sequence.len() as f64 / bars.len() as f64 * 100.0, 1),
                "current_price": current_price(last.close),
                "last_date": date_string(last.time),
                "current_regime": RegimeId::from_u8(current as u8).name(),
                "dwell_bars": dwell,
                "recent_timeline": sequence.iter().rev().take(30).rev().map(|v| RegimeId::from_u8(*v as u8).name()).collect::<Vec<_>>()
            }),
            sequence,
            contiguous_regime_sequences(&observations),
            class,
        ));
    }

    let global_model = DtmcModel::from_sequences(&all_sequences, REGIME_COUNT);
    let universe_models: Vec<(&str, Value)> = [
        ("crypto_futures", &universe_sequences[0]),
        ("crypto_spot", &universe_sequences[1]),
        ("idx", &universe_sequences[2]),
        ("tokenized_stock", &universe_sequences[3]),
    ]
    .iter()
    .filter(|(_, sequences)| !sequences.is_empty())
    .map(|(class, sequences)| {
        (
            *class,
            model_value(&DtmcModel::from_sequences(sequences, REGIME_COUNT)),
        )
    })
    .collect();

    let mut universe_map = serde_json::Map::new();
    for (class, model) in universe_models {
        universe_map.insert(class.to_string(), model);
    }
    let mut ticker_map = serde_json::Map::new();
    let mut assets_out = Vec::with_capacity(details.len());
    for (mut detail, _sequence, transition_sequences, _class) in details {
        let model = DtmcModel::from_sequences(&transition_sequences, REGIME_COUNT);
        let current = detail["current_regime"]
            .as_str()
            .unwrap_or("CONSOLIDATION_CHOP");
        let current_index = regime_keys()
            .iter()
            .position(|key| *key == current)
            .unwrap_or(5);
        let (next_index, next_probability) = model.predict_next_state(current_index);
        let expected_duration = model
            .expected_duration
            .get(current_index)
            .copied()
            .unwrap_or(1.0);
        let dwell = detail["dwell_bars"].as_u64().unwrap_or(1) as f64;
        let current_transition_support = model.transition_row_support[current_index];
        detail["current_transition_support"] = json!(current_transition_support);
        detail["current_transition_row_supported"] = json!(current_transition_support > 0);
        detail["expected_duration_bars"] = json!(round_decimal(expected_duration, 1));
        detail["stability_pct"] = json!(round_decimal(
            model.persistence.get(current_index).copied().unwrap_or(0.0) * 100.0,
            1
        ));
        detail["most_frequent_next_regime"] = json!(RegimeId::from_u8(next_index as u8).name());
        detail["next_transition_frequency_pct"] = json!(round_decimal(next_probability * 100.0, 1));
        detail["is_overdue"] = json!(dwell > expected_duration);
        let model_key = detail["model_key"].as_str().unwrap_or_default().to_string();
        ticker_map.insert(model_key.clone(), json!({
            "model_key": model_key, "symbol": detail["symbol"], "exchange_id": detail["exchange_id"],
            "asset_category_id": detail["asset_category_id"], "clean_symbol": detail["clean_symbol"], "class": detail["class"],
            "total_bars": detail["total_bars"], "transition_matrix": model_value(&model)["transition_matrix"],
            "persistence_pct": model_value(&model)["persistence_pct"], "expected_duration_bars": model_value(&model)["expected_duration_bars"],
            "stationary_distribution_pct": model_value(&model)["stationary_distribution_pct"],
            "transition_row_support": model.transition_row_support,
            "walk_forward_evaluation": walk_forward_evaluation(&transition_sequences)
        }));
        assets_out.push(detail);
    }
    assets_out.sort_by(|a, b| {
        let overdue_a = a["is_overdue"].as_bool().unwrap_or(false);
        let overdue_b = b["is_overdue"].as_bool().unwrap_or(false);
        overdue_b
            .cmp(&overdue_a)
            .then_with(|| b["dwell_bars"].as_u64().cmp(&a["dwell_bars"].as_u64()))
    });

    json!({
        "generated_at": "rust",
        "model_type": "observed_state_discrete_time_markov_chain",
        "classification_inputs": ["DSS Bressert", "Nadaraya-Watson Envelope endpoint", format!("{}-bar EMA", config.trend_ema_span_bars)],
        "classification_config": config,
        "classification_mode": "causal_completed_bars",
        "transition_estimator": "row_normalized_dirichlet_smoothed_empirical_counts",
        "transition_prior_alpha_per_destination": TRANSITION_DIRICHLET_ALPHA,
        "timeframe": timeframe,
        "probability_note": "Empirical historical transition frequencies; descriptive estimates, not validated forecasts or trade probabilities.",
        "regimes": regime_definitions(),
        "regime_keys": regime_keys(),
        "total_assets": assets_out.len(),
        "global_model": model_value(&global_model),
        "universe_models": Value::Object(universe_map),
        "ticker_models": Value::Object(ticker_map),
        "assets": assets_out
    })
}

#[cfg(test)]
#[path = "regime_tests.rs"]
mod tests;
