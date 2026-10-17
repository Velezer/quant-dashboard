//! Bounded Rust adaptation of the LuxAlgo Nadaraya-Watson Envelope reference.
//!
//! Based on “Nadaraya-Watson Envelope [LuxAlgo]”, © LuxAlgo.
//! Licensed under CC BY-NC-SA 4.0:
//! https://creativecommons.org/licenses/by-nc-sa/4.0/
//!
//! Repainting mode centers each estimate across the latest available window,
//! so historical values use later bars. Endpoint mode uses only current and
//! earlier closes and stays stable as future bars arrive.

use serde::{Deserialize, Serialize};

pub const NWE_METHOD_ID: &str = "luxalgo_nadaraya_watson_envelope";
pub const NWE_METHOD_LABEL: &str = "Nadaraya-Watson Envelope [LuxAlgo]";
pub const NWE_ATTRIBUTION: &str = "© LuxAlgo";
pub const NWE_LICENSE: &str = "CC BY-NC-SA 4.0";
pub const NWE_LICENSE_URL: &str = "https://creativecommons.org/licenses/by-nc-sa/4.0/";

pub const DAY_TRADE_NWE_WINDOW_BARS: usize = 500;
pub const DAY_TRADE_NWE_BANDWIDTH_BARS: f64 = 8.0;
pub const DAY_TRADE_NWE_ENVELOPE_MULTIPLIER: f64 = 3.0;

const MIN_NWE_WINDOW_BARS: usize = 2;
const MAX_NWE_WINDOW_BARS: usize = 500;
const MAX_NWE_BANDWIDTH_BARS: f64 = 500.0;
const MAX_NWE_ENVELOPE_MULTIPLIER: f64 = 100.0;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NadarayaWatsonEnvelopeConfig {
    /// Maximum trailing sample window, matching the reference's 500-bar cap.
    pub window_bars: usize,
    /// Gaussian kernel width, measured in bars.
    pub bandwidth_bars: f64,
    /// Distance multiplier applied to mean absolute error.
    pub envelope_multiplier: f64,
}

impl Default for NadarayaWatsonEnvelopeConfig {
    fn default() -> Self {
        Self {
            window_bars: DAY_TRADE_NWE_WINDOW_BARS,
            bandwidth_bars: DAY_TRADE_NWE_BANDWIDTH_BARS,
            envelope_multiplier: DAY_TRADE_NWE_ENVELOPE_MULTIPLIER,
        }
    }
}

impl NadarayaWatsonEnvelopeConfig {
    /// Keeps public configurations valid and bounds per-series work.
    pub fn validated(self) -> Self {
        let defaults = Self::default();
        Self {
            window_bars: if self.window_bars < MIN_NWE_WINDOW_BARS {
                defaults.window_bars
            } else {
                self.window_bars.min(MAX_NWE_WINDOW_BARS)
            },
            bandwidth_bars: if self.bandwidth_bars.is_finite() && self.bandwidth_bars > 0.0 {
                self.bandwidth_bars.min(MAX_NWE_BANDWIDTH_BARS)
            } else {
                defaults.bandwidth_bars
            },
            envelope_multiplier: if self.envelope_multiplier.is_finite()
                && self.envelope_multiplier >= 0.0
            {
                self.envelope_multiplier.min(MAX_NWE_ENVELOPE_MULTIPLIER)
            } else {
                defaults.envelope_multiplier
            },
        }
    }

    /// Returns the published parameters and attribution used by the chart UI.
    pub fn metadata(self) -> NadarayaWatsonEnvelopeMetadata {
        let config = self.validated();
        NadarayaWatsonEnvelopeMetadata {
            method: NWE_METHOD_ID,
            method_label: NWE_METHOD_LABEL,
            attribution: NWE_ATTRIBUTION,
            license: NWE_LICENSE,
            license_url: NWE_LICENSE_URL,
            window_bars: config.window_bars,
            mae_period_bars: config.window_bars - 1,
            endpoint_envelope_warmup_bars: config.minimum_endpoint_envelope_bars(),
            bandwidth_bars: config.bandwidth_bars,
            envelope_multiplier: config.envelope_multiplier,
            repainting_default: true,
        }
    }

    /// Number of completed causal bars required before the endpoint has both
    /// a full-width midline and a full rolling residual sample for its bands.
    pub fn minimum_endpoint_envelope_bars(self) -> usize {
        self.validated().window_bars
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct NadarayaWatsonEnvelopeMetadata {
    pub method: &'static str,
    pub method_label: &'static str,
    pub attribution: &'static str,
    pub license: &'static str,
    pub license_url: &'static str,
    pub window_bars: usize,
    pub mae_period_bars: usize,
    pub endpoint_envelope_warmup_bars: usize,
    pub bandwidth_bars: f64,
    pub envelope_multiplier: f64,
    pub repainting_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NadarayaWatsonEnvelopeMode {
    pub midline: Vec<f64>,
    pub upper: Vec<f64>,
    pub lower: Vec<f64>,
    pub crossed_above_upper: Vec<bool>,
    pub crossed_below_lower: Vec<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NadarayaWatsonEnvelopeResult {
    pub repainting: NadarayaWatsonEnvelopeMode,
    pub endpoint: NadarayaWatsonEnvelopeMode,
}

/// Calculates both LuxAlgo display modes with a maximum 500-bar window.
pub fn calculate_nadaraya_watson_envelope(
    closes: &[f64],
    config: &NadarayaWatsonEnvelopeConfig,
) -> NadarayaWatsonEnvelopeResult {
    let config = config.validated();
    let weights: Vec<f64> = (0..config.window_bars)
        .map(|age| gaussian(age as f64, config.bandwidth_bars))
        .collect();

    NadarayaWatsonEnvelopeResult {
        repainting: calculate_repainting(closes, &config),
        endpoint: calculate_endpoint(closes, &config, &weights),
    }
}

/// Calculates only the causal endpoint envelope used by signal confirmations.
/// This avoids computing the display-only repainting mode during screening.
pub(crate) fn calculate_nadaraya_watson_endpoint_only(
    closes: &[f64],
    config: &NadarayaWatsonEnvelopeConfig,
) -> NadarayaWatsonEnvelopeMode {
    let config = config.validated();
    let weights: Vec<f64> = (0..config.window_bars)
        .map(|age| gaussian(age as f64, config.bandwidth_bars))
        .collect();
    calculate_endpoint(closes, &config, &weights)
}

/// Calculates one causal endpoint midline value without allocating chart
/// bands or evaluating bars that have no strategy entry candidate.
#[cfg(test)]
pub(crate) fn calculate_nadaraya_watson_endpoint_midline_at(
    closes: &[f64],
    index: usize,
    config: &NadarayaWatsonEnvelopeConfig,
) -> f64 {
    let config = config.validated();
    if index >= closes.len() || index + 1 < config.window_bars {
        return f64::NAN;
    }
    let mut weighted_close = 0.0;
    let mut weight_total = 0.0;
    for age in 0..config.window_bars {
        let close = closes[index - age];
        if !close.is_finite() {
            return f64::NAN;
        }
        let weight = gaussian(age as f64, config.bandwidth_bars);
        weighted_close += close * weight;
        weight_total += weight;
    }
    if weight_total > 0.0 {
        weighted_close / weight_total
    } else {
        f64::NAN
    }
}

fn empty_mode(count: usize) -> NadarayaWatsonEnvelopeMode {
    NadarayaWatsonEnvelopeMode {
        midline: vec![f64::NAN; count],
        upper: vec![f64::NAN; count],
        lower: vec![f64::NAN; count],
        crossed_above_upper: vec![false; count],
        crossed_below_lower: vec![false; count],
    }
}

fn gaussian(distance: f64, bandwidth_bars: f64) -> f64 {
    (-(distance * distance) / (bandwidth_bars * bandwidth_bars * 2.0)).exp()
}

fn calculate_repainting(
    closes: &[f64],
    config: &NadarayaWatsonEnvelopeConfig,
) -> NadarayaWatsonEnvelopeMode {
    let count = closes.len();
    let mut result = empty_mode(count);
    let window_count = count.min(config.window_bars);
    if window_count == 0 {
        return result;
    }
    let first_index = count - window_count;
    let mut absolute_error_total = 0.0;

    for target_index in first_index..count {
        let mut weighted_close = 0.0;
        let mut weight_total = 0.0;
        let mut complete_window = true;
        for sample_index in first_index..count {
            let close = closes[sample_index];
            if !close.is_finite() {
                complete_window = false;
                break;
            }
            let distance = target_index.abs_diff(sample_index) as f64;
            let weight = gaussian(distance, config.bandwidth_bars);
            weighted_close += close * weight;
            weight_total += weight;
        }
        if complete_window && weight_total > 0.0 {
            let midline = weighted_close / weight_total;
            result.midline[target_index] = midline;
            absolute_error_total += (closes[target_index] - midline).abs();
        }
    }

    if window_count < 2 || !(first_index..count).all(|index| result.midline[index].is_finite()) {
        return result;
    }

    // The supplied Pine reference divides its accumulated error by max age
    // (window_count - 1), then applies the selected multiplier.
    let sae = absolute_error_total / (window_count - 1) as f64 * config.envelope_multiplier;
    for index in first_index..count {
        result.upper[index] = result.midline[index] + sae;
        result.lower[index] = result.midline[index] - sae;
    }
    for index in (first_index + 1)..count {
        result.crossed_above_upper[index] =
            closes[index] > result.upper[index] && closes[index - 1] < result.upper[index];
        result.crossed_below_lower[index] =
            closes[index] < result.lower[index] && closes[index - 1] > result.lower[index];
    }
    result
}

fn calculate_endpoint(
    closes: &[f64],
    config: &NadarayaWatsonEnvelopeConfig,
    weights: &[f64],
) -> NadarayaWatsonEnvelopeMode {
    let count = closes.len();
    let mut result = empty_mode(count);
    let mut absolute_errors = vec![f64::NAN; count];
    let mut cumulative_weights = Vec::with_capacity(weights.len());
    let mut cumulative_weight = 0.0;
    for weight in weights {
        cumulative_weight += *weight;
        cumulative_weights.push(cumulative_weight);
    }

    for index in 0..count {
        let sample_count = (index + 1).min(config.window_bars);
        let partial_weight_total = cumulative_weights[sample_count - 1];
        let mut weighted_close = 0.0;
        let mut complete_window = true;
        for (age, weight) in weights.iter().take(sample_count).enumerate() {
            let close = closes[index - age];
            if !close.is_finite() {
                complete_window = false;
                break;
            }
            weighted_close += close * weight;
        }
        if complete_window && partial_weight_total > 0.0 {
            let midline = weighted_close / partial_weight_total;
            if sample_count == config.window_bars {
                result.midline[index] = midline;
            }
            absolute_errors[index] = (closes[index] - midline).abs();
        }
    }

    let error_period = config.window_bars - 1;
    let mut rolling_error_total = 0.0;
    let mut valid_error_count = 0;
    for index in 0..count {
        if index >= error_period {
            let expired_error = absolute_errors[index - error_period];
            if expired_error.is_finite() {
                rolling_error_total -= expired_error;
                valid_error_count -= 1;
            }
        }
        let error = absolute_errors[index];
        if error.is_finite() {
            rolling_error_total += error;
            valid_error_count += 1;
        }
        if index + 1 >= config.window_bars && valid_error_count == error_period {
            let mae = rolling_error_total / error_period as f64 * config.envelope_multiplier;
            result.upper[index] = result.midline[index] + mae;
            result.lower[index] = result.midline[index] - mae;
        }
    }

    for index in 1..count {
        if ![result.upper[index - 1], result.upper[index]]
            .iter()
            .all(|v| v.is_finite())
            || ![result.lower[index - 1], result.lower[index]]
                .iter()
                .all(|v| v.is_finite())
        {
            continue;
        }
        result.crossed_above_upper[index] =
            closes[index] > result.upper[index] && closes[index - 1] <= result.upper[index - 1];
        result.crossed_below_lower[index] =
            closes[index] < result.lower[index] && closes[index - 1] >= result.lower[index - 1];
    }
    result
}

#[cfg(test)]
#[path = "nadaraya_watson_envelope_tests.rs"]
mod tests;
