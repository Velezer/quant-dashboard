use std::collections::HashSet;

use super::common::{
    finite, IdxStrategyConfig, IdxTimeSeriesMomentumConfig, StrategyBar, StrategySignal,
};
use super::confirmations::{
    apply_stockbit_nwe_proximity_confluence, NWE_DSS_SIGNAL_MINIMUM_SCORE_OUT_OF_20,
    NWE_DSS_SIGNAL_SCORE_SCALE_MAX, SIGNAL_STRENGTH_SCORE_SCALE_MAX,
};
use crate::indicators::nadaraya_watson_envelope::calculate_nadaraya_watson_endpoint_only;
use crate::NadarayaWatsonEnvelopeConfig;

pub const MAX_INDICATOR_COMBINATION_SIZE: usize = 6;
pub const STOCKBIT_NWE_4H_PREVIOUS_CROSS_STRATEGY_ID: &str =
    "stockbit_nwe_upper_band_breakout_4h_v1";
pub const STOCKBIT_NWE_4H_PREVIOUS_SCORE_STRATEGY_ID: &str =
    "stockbit_nwe_band_position_score_4h_v1";
pub const STOCKBIT_NWE_4H_PREVIOUS_BINARY_CROSS_STRATEGY_ID: &str = "stockbit_nwe_band_cross_4h_v1";
pub const STOCKBIT_NWE_4H_PREVIOUS_RANKED_CROSS_STRATEGY_ID: &str = "stockbit_nwe_band_cross_4h_v2";
pub const STOCKBIT_NWE_4H_SIGNAL_STRATEGY_ID: &str = "stockbit_nwe_band_proximity_4h_v3";
pub const STOCKBIT_NWE_4H_SIGNAL_SCORE_MODEL_ID_LEGACY: &str =
    "nwe_endpoint_band_proximity_dss_rank_v4";
pub const STOCKBIT_NWE_4H_SIGNAL_SCORE_MODEL_ID: &str =
    "nwe_dss_relative_volume_estimated_delta_rank_v6";
pub const CRYPTO_NWE_4H_SIGNAL_STRATEGY_ID: &str = "crypto_nwe_band_proximity_4h_v1";
pub const STOCKBIT_NWE_4H_PRODUCT_ID: &str = "stockbit_regular";
pub const STOCKBIT_NWE_4H_EXCHANGE_ID: &str = "stockbit";
pub const STOCKBIT_NWE_4H_ASSET_CATEGORY_ID: &str = "idx_stock";
pub const STOCKBIT_NWE_4H_TIMEFRAME: &str = "4h";

/// Return the stable crypto NWE strategy identity for a supported screener frame.
pub fn crypto_nwe_signal_strategy_id(timeframe: &str) -> Option<String> {
    super::DSS_BRESSERT_MTF_TIMEFRAMES
        .iter()
        .any(|definition| definition.timeframe == timeframe)
        .then(|| format!("crypto_nwe_band_proximity_{timeframe}_v1"))
}

/// Return the timeframe-specific Stockbit NWE screener identity.
pub fn stockbit_nwe_signal_strategy_id(timeframe: &str) -> Option<String> {
    super::DSS_BRESSERT_MTF_TIMEFRAMES
        .iter()
        .any(|definition| definition.timeframe == timeframe)
        .then(|| {
            if timeframe == STOCKBIT_NWE_4H_TIMEFRAME {
                STOCKBIT_NWE_4H_SIGNAL_STRATEGY_ID.to_string()
            } else {
                format!("stockbit_nwe_band_proximity_{timeframe}_v1")
            }
        })
}

#[derive(Debug, Clone, Copy)]
pub struct StockbitNwe4hSignalConfig {
    pub minimum_score_out_of_20: f64,
    pub maximum_band_distance_ratio: f64,
}

pub const STOCKBIT_NWE_4H_SIGNAL_CONFIG: StockbitNwe4hSignalConfig = StockbitNwe4hSignalConfig {
    minimum_score_out_of_20: NWE_DSS_SIGNAL_MINIMUM_SCORE_OUT_OF_20,
    maximum_band_distance_ratio: 0.5,
};

impl Default for StockbitNwe4hSignalConfig {
    fn default() -> Self {
        STOCKBIT_NWE_4H_SIGNAL_CONFIG
    }
}

impl StockbitNwe4hSignalConfig {
    fn is_valid(self) -> bool {
        self.minimum_score_out_of_20.is_finite()
            && self.minimum_score_out_of_20 > 0.0
            && self.minimum_score_out_of_20 <= NWE_DSS_SIGNAL_SCORE_SCALE_MAX
            && self.maximum_band_distance_ratio.is_finite()
            && self.maximum_band_distance_ratio > 0.0
            && self.maximum_band_distance_ratio <= 1.0
    }
}

/// Return enough completed bars for the current endpoint band and its prior value.
pub fn stockbit_nwe_4h_minimum_history_bars() -> usize {
    NadarayaWatsonEnvelopeConfig::default()
        .minimum_endpoint_envelope_bars()
        .saturating_add(1)
}

/// Generate causal Stockbit 4h NWE cross and near-band research directions.
/// A lower-band cross or near-band state is LONG; an upper-band equivalent is SHORT.
pub fn generate_stockbit_nwe_4h_signals(bars: &[StrategyBar]) -> Vec<StrategySignal> {
    let config = NadarayaWatsonEnvelopeConfig::default();
    let signal_config = StockbitNwe4hSignalConfig::default();
    let required_cross_bars = stockbit_nwe_4h_minimum_history_bars();
    let mut signals = vec![StrategySignal::default(); bars.len()];
    if !signal_config.is_valid() || bars.len() < required_cross_bars {
        return signals;
    }
    let first_index = bars.len().saturating_sub(required_cross_bars);
    let closes = bars[first_index..]
        .iter()
        .map(|bar| bar.close)
        .collect::<Vec<_>>();
    let envelope = calculate_nadaraya_watson_endpoint_only(&closes, &config);
    for (index, signal) in signals.iter_mut().enumerate().skip(first_index) {
        let envelope_index = index - first_index;
        let Some(bar) = bars.get(index) else {
            continue;
        };
        let Some(crossed_above_upper) = envelope.crossed_above_upper.get(envelope_index).copied()
        else {
            continue;
        };
        let Some(crossed_below_lower) = envelope.crossed_below_lower.get(envelope_index).copied()
        else {
            continue;
        };
        let Some(upper) = envelope.upper.get(envelope_index).copied() else {
            continue;
        };
        let Some(lower) = envelope.lower.get(envelope_index).copied() else {
            continue;
        };
        if !finite(bar.close) || bar.close <= 0.0 {
            continue;
        }
        let (direction, regime, strategy_strength) =
            match (crossed_below_lower, crossed_above_upper) {
                (true, false) => (
                    "LONG",
                    "NWE_LOWER_BAND_CROSS_LONG",
                    SIGNAL_STRENGTH_SCORE_SCALE_MAX,
                ),
                (false, true) => (
                    "SHORT",
                    "NWE_UPPER_BAND_CROSS_SHORT",
                    SIGNAL_STRENGTH_SCORE_SCALE_MAX,
                ),
                (false, false) => {
                    let Some(candidate) =
                        stockbit_nwe_4h_near_band_candidate(bar.close, lower, upper, signal_config)
                    else {
                        continue;
                    };
                    candidate
                }
                (true, true) => continue,
            };
        signal.direction = direction.to_string();
        signal.strength = strategy_strength;
        signal.strategy_strength = strategy_strength;
        signal.regime = regime.to_string();
    }
    apply_stockbit_nwe_proximity_confluence(bars, &mut signals);
    let score_rounding_tolerance = 1e-12;
    for signal in &mut signals {
        if matches!(signal.direction.as_str(), "LONG" | "SHORT")
            && signal.strength + score_rounding_tolerance < signal_config.minimum_score_out_of_20
        {
            *signal = StrategySignal::default();
        } else if matches!(signal.direction.as_str(), "LONG" | "SHORT") {
            signal.strength = signal.strength.clamp(0.0, NWE_DSS_SIGNAL_SCORE_SCALE_MAX);
        }
    }
    signals
}

pub(super) fn stockbit_nwe_4h_near_band_candidate(
    close: f64,
    lower: f64,
    upper: f64,
    config: StockbitNwe4hSignalConfig,
) -> Option<(&'static str, &'static str, f64)> {
    let envelope_width = upper - lower;
    if ![close, lower, upper, envelope_width]
        .iter()
        .all(|value| finite(*value))
        || envelope_width <= 0.0
        || !config.is_valid()
    {
        return None;
    }

    let maximum_distance = envelope_width * config.maximum_band_distance_ratio;
    if !finite(maximum_distance) || maximum_distance <= 0.0 {
        return None;
    }
    let lower_distance = (close - lower).abs();
    let upper_distance = (upper - close).abs();
    let (direction, regime, distance) = if lower_distance < upper_distance {
        ("LONG", "NWE_LOWER_BAND_NEAR_LONG", lower_distance)
    } else if upper_distance < lower_distance {
        ("SHORT", "NWE_UPPER_BAND_NEAR_SHORT", upper_distance)
    } else {
        return None;
    };
    if distance > maximum_distance {
        return None;
    }

    let proximity = (distance / maximum_distance).clamp(0.0, 1.0);
    let strategy_strength = SIGNAL_STRENGTH_SCORE_SCALE_MAX * (1.0 - proximity);
    if !finite(strategy_strength) {
        return None;
    }
    Some((direction, regime, strategy_strength))
}

const SUPPORTED_INDICATOR_IDS: [&str; 8] = [
    "ema_20_50",
    "rsi_14",
    "relative_volume",
    "open_low_structure",
    "range_breakout",
    "wick_rejection",
    "momentum_12_1_month",
    "sma_200_session",
];

/// Defaults for user-composed indicator alerts. These alerts remain separate
/// from registered plan strategies and never provide execution evidence.
#[derive(Debug, Clone, Copy)]
pub struct IndicatorCombinationConfig {
    pub fast_ema_period: usize,
    pub slow_ema_period: usize,
    pub rsi_period: usize,
    pub rsi_long_min: f64,
    pub rsi_long_max: f64,
    pub rsi_short_min: f64,
    pub rsi_short_max: f64,
    pub relative_volume_period: usize,
    pub relative_volume_min: f64,
    pub breakout_lookback: usize,
    pub wick_fraction_min: f64,
}

impl Default for IndicatorCombinationConfig {
    fn default() -> Self {
        Self {
            fast_ema_period: 20,
            slow_ema_period: 50,
            rsi_period: 14,
            rsi_long_min: 55.0,
            rsi_long_max: 78.0,
            rsi_short_min: 22.0,
            rsi_short_max: 45.0,
            relative_volume_period: 20,
            relative_volume_min: 1.5,
            breakout_lookback: 20,
            wick_fraction_min: 0.45,
        }
    }
}

impl IndicatorCombinationConfig {
    fn is_valid(self) -> bool {
        self.fast_ema_period > 0
            && self.fast_ema_period < self.slow_ema_period
            && self.rsi_period > 0
            && self.rsi_long_min < self.rsi_long_max
            && self.rsi_short_min < self.rsi_short_max
            && self.rsi_short_max < self.rsi_long_min
            && self.relative_volume_period > 0
            && finite(self.relative_volume_min)
            && self.relative_volume_min > 0.0
            && self.breakout_lookback > 0
            && finite(self.wick_fraction_min)
            && self.wick_fraction_min > 0.0
            && self.wick_fraction_min <= 1.0
    }
}

pub fn indicator_combination_indicator_ids() -> &'static [&'static str] {
    &SUPPORTED_INDICATOR_IDS
}

/// Generate alert-only signals when every selected indicator agrees on the
/// current candle. Callers validate product/category/timeframe availability.
pub fn generate_indicator_combination_signals(
    bars: &[StrategyBar],
    indicator_ids: &[String],
    config: IndicatorCombinationConfig,
    allow_short: bool,
) -> Result<Vec<StrategySignal>, &'static str> {
    generate_indicator_combination_signals_with_minimum(
        bars,
        indicator_ids,
        config,
        allow_short,
        indicator_ids.len(),
        "USER_INDICATOR_COMBINATION",
    )
}

/// Generate a directional signal when a configured number of selected
/// indicators agree. The ordinary user-composed alert path keeps its
/// all-selected-indicators rule through `generate_indicator_combination_signals`.
pub(super) fn generate_indicator_combination_signals_with_minimum(
    bars: &[StrategyBar],
    indicator_ids: &[String],
    config: IndicatorCombinationConfig,
    allow_short: bool,
    minimum_agreements: usize,
    regime_prefix: &str,
) -> Result<Vec<StrategySignal>, &'static str> {
    if !config.is_valid() {
        return Err("indicator combination defaults are invalid");
    }
    if !(2..=MAX_INDICATOR_COMBINATION_SIZE).contains(&indicator_ids.len()) {
        return Err("choose between two and six indicators");
    }
    let supported = indicator_combination_indicator_ids();
    let unique = indicator_ids.iter().collect::<HashSet<_>>();
    if unique.len() != indicator_ids.len()
        || indicator_ids
            .iter()
            .any(|indicator_id| !supported.contains(&indicator_id.as_str()))
    {
        return Err("indicator selection contains duplicates or unsupported indicators");
    }
    if minimum_agreements == 0 || minimum_agreements > indicator_ids.len() {
        return Err("minimum agreement count must fit the selected indicators");
    }

    let n = bars.len();
    let mut signals = vec![StrategySignal::default(); n];
    if n == 0 {
        return Ok(signals);
    }

    let closes = bars.iter().map(|bar| bar.close).collect::<Vec<_>>();
    let volumes = bars.iter().map(|bar| bar.volume).collect::<Vec<_>>();
    let fast_ema = ema_series(&closes, config.fast_ema_period);
    let slow_ema = ema_series(&closes, config.slow_ema_period);
    let rsi = rsi_series(&closes, config.rsi_period);
    let momentum_config = IdxTimeSeriesMomentumConfig::default();
    let open_low_config = IdxStrategyConfig::default();

    for index in 0..n {
        let mut long_votes = 0;
        let mut short_votes = 0;
        for indicator_id in indicator_ids {
            let direction = indicator_direction(
                indicator_id,
                bars,
                index,
                &closes,
                &volumes,
                &fast_ema,
                &slow_ema,
                &rsi,
                config,
                momentum_config,
                open_low_config,
            );
            if direction > 0 {
                long_votes += 1;
            } else if direction < 0 {
                short_votes += 1;
            }
        }
        let direction = if long_votes >= minimum_agreements && long_votes > short_votes {
            Some(("LONG", long_votes))
        } else if allow_short && short_votes >= minimum_agreements && short_votes > long_votes {
            Some(("SHORT", short_votes))
        } else {
            None
        };
        if let Some((label, matching_votes)) = direction {
            signals[index].direction = label.to_string();
            signals[index].strength = 10.0 * matching_votes as f64 / indicator_ids.len() as f64;
            signals[index].regime = if minimum_agreements < indicator_ids.len() {
                format!(
                    "{regime_prefix}_{matching_votes}_OF_{}_{}",
                    indicator_ids.len(),
                    label
                )
            } else {
                format!("{regime_prefix}_{label}")
            };
        }
    }
    Ok(signals)
}

#[allow(clippy::too_many_arguments)]
fn indicator_direction(
    indicator_id: &str,
    bars: &[StrategyBar],
    index: usize,
    closes: &[f64],
    volumes: &[f64],
    fast_ema: &[f64],
    slow_ema: &[f64],
    rsi: &[f64],
    config: IndicatorCombinationConfig,
    momentum_config: IdxTimeSeriesMomentumConfig,
    open_low_config: IdxStrategyConfig,
) -> i8 {
    let bar = bars[index];
    match indicator_id {
        "ema_20_50" if finite(bar.close) && finite(fast_ema[index]) && finite(slow_ema[index]) => {
            if bar.close > fast_ema[index] && fast_ema[index] > slow_ema[index] {
                1
            } else if bar.close < fast_ema[index] && fast_ema[index] < slow_ema[index] {
                -1
            } else {
                0
            }
        }
        "rsi_14" if finite(rsi[index]) => {
            if (config.rsi_long_min..=config.rsi_long_max).contains(&rsi[index]) {
                1
            } else if (config.rsi_short_min..=config.rsi_short_max).contains(&rsi[index]) {
                -1
            } else {
                0
            }
        }
        "relative_volume" => {
            let average = strict_rolling_mean(volumes, index, config.relative_volume_period);
            if !finite(average)
                || average <= 0.0
                || !finite(bar.volume)
                || bar.volume / average < config.relative_volume_min
            {
                0
            } else if bar.close > bar.open {
                1
            } else if bar.close < bar.open {
                -1
            } else {
                0
            }
        }
        "open_low_structure" => {
            let tolerance = open_low_config.open_equal_low_tolerance;
            if finite(bar.open)
                && bar.open > 0.0
                && finite(bar.low)
                && finite(bar.close)
                && bar.low <= bar.open
                && (bar.open - bar.low) / bar.open <= tolerance
                && bar.close >= bar.open
            {
                1
            } else {
                0
            }
        }
        "range_breakout" if index >= config.breakout_lookback => {
            let previous = &bars[index - config.breakout_lookback..index];
            let prior_high = previous
                .iter()
                .map(|item| item.high)
                .fold(f64::NEG_INFINITY, f64::max);
            let prior_low = previous
                .iter()
                .map(|item| item.low)
                .fold(f64::INFINITY, f64::min);
            if finite(bar.close)
                && finite(bar.open)
                && bar.close > prior_high
                && bar.close >= bar.open
            {
                1
            } else if finite(bar.close)
                && finite(bar.open)
                && bar.close < prior_low
                && bar.close <= bar.open
            {
                -1
            } else {
                0
            }
        }
        "wick_rejection" => {
            let candle_range = bar.high - bar.low;
            if !finite(candle_range) || candle_range <= 0.0 {
                return 0;
            }
            let lower_wick = (bar.open.min(bar.close) - bar.low) / candle_range;
            let upper_wick = (bar.high - bar.open.max(bar.close)) / candle_range;
            if lower_wick >= config.wick_fraction_min && bar.close >= bar.open {
                1
            } else if upper_wick >= config.wick_fraction_min && bar.close <= bar.open {
                -1
            } else {
                0
            }
        }
        "momentum_12_1_month" => {
            if index < momentum_config.lookback_bars
                || momentum_config.skip_recent_bars >= momentum_config.lookback_bars
            {
                return 0;
            }
            let start = index - momentum_config.lookback_bars;
            let end = index - momentum_config.skip_recent_bars;
            let base = closes[start];
            let delayed = closes[end];
            if !finite(base) || base <= 0.0 || !finite(delayed) {
                0
            } else if delayed > base {
                1
            } else if delayed < base {
                -1
            } else {
                0
            }
        }
        "sma_200_session" => {
            let average = strict_rolling_mean(closes, index, momentum_config.trend_period);
            if !finite(bar.close) || !finite(average) {
                0
            } else if bar.close > average {
                1
            } else if bar.close < average {
                -1
            } else {
                0
            }
        }
        _ => 0,
    }
}

pub(super) fn ema_series(values: &[f64], span: usize) -> Vec<f64> {
    if values.is_empty() || span == 0 {
        return vec![f64::NAN; values.len()];
    }
    let alpha = 2.0 / (span as f64 + 1.0);
    let mut output = vec![f64::NAN; values.len()];
    let mut previous = f64::NAN;
    for (index, value) in values.iter().copied().enumerate() {
        if !finite(value) {
            continue;
        }
        previous = if finite(previous) {
            alpha * value + (1.0 - alpha) * previous
        } else {
            value
        };
        output[index] = previous;
    }
    output
}

pub(super) fn strict_rolling_mean(values: &[f64], index: usize, period: usize) -> f64 {
    if period == 0 || index + 1 < period {
        return f64::NAN;
    }
    let window = &values[index + 1 - period..=index];
    if window.iter().all(|value| finite(*value)) {
        window.iter().sum::<f64>() / period as f64
    } else {
        f64::NAN
    }
}

pub(super) fn rsi_series(closes: &[f64], period: usize) -> Vec<f64> {
    let mut gains = vec![f64::NAN; closes.len()];
    let mut losses = vec![f64::NAN; closes.len()];
    for index in 1..closes.len() {
        let delta = closes[index] - closes[index - 1];
        gains[index] = delta.max(0.0);
        losses[index] = (-delta).max(0.0);
    }
    let mut rsi = vec![f64::NAN; closes.len()];
    for (index, value) in rsi.iter_mut().enumerate() {
        let gain = strict_rolling_mean(&gains, index, period);
        let loss = strict_rolling_mean(&losses, index, period);
        if finite(gain) && finite(loss) && loss > 0.0 {
            *value = 100.0 - 100.0 / (1.0 + gain / loss);
        }
    }
    rsi
}
