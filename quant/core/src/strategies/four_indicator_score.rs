use super::common::StrategyBar;

pub const FOUR_INDICATOR_SCORE_SCALE_MAX: f64 = 40.0;
pub const FOUR_INDICATOR_EQUAL_WEIGHTS: [u8; 4] = [10; 4];
pub const MARKOV_REGIME_COMPONENT_WEIGHT: u8 = 10;
pub const STRATEGY_RETURN_AUTOCORRELATION_COMPONENT_WEIGHT: u8 = 10;
pub const STRATEGY_RETURN_AUTOCORRELATION_LOOKBACK_RETURNS: usize = 128;
pub const STRATEGY_RETURN_AUTOCORRELATION_MINIMUM_PAIRS: usize = 30;
pub const SIX_INDICATOR_SCORE_SCALE_MAX: f64 = 60.0;
pub const SIX_INDICATOR_MINIMUM_SCORE_OUT_OF_60: f64 = 1.0;
/// A price or volume subtotal may receive the full score budget under a
/// learned profile, so its valid range is the complete model scale.
pub const FOUR_INDICATOR_GROUP_SCORE_SCALE_MAX: f64 = FOUR_INDICATOR_SCORE_SCALE_MAX;
pub const FOUR_INDICATOR_COMPONENT_SCORE_SCALE_MAX: f64 = 10.0;
pub const FOUR_INDICATOR_MINIMUM_SCORE_OUT_OF_40: f64 =
    super::confirmations::NWE_DSS_SIGNAL_MINIMUM_SCORE_OUT_OF_20;
pub const FOUR_INDICATOR_VOLUME_LOOKBACK_BARS: usize = 20;
pub const FOUR_INDICATOR_MIN_RELATIVE_VOLUME_MULTIPLE: f64 = 1.0;
pub const FOUR_INDICATOR_MAX_RELATIVE_VOLUME_MULTIPLE_FOR_FULL_SCORE: f64 = 2.0;

#[derive(Debug, Clone, PartialEq)]
pub struct ScoreComponent {
    pub indicator: &'static str,
    pub state: String,
    pub points: f64,
    pub weight_points: u8,
    pub weighted_points: f64,
    pub value: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FourIndicatorScore {
    pub price_score: f64,
    pub volume_score: f64,
    pub total_score: f64,
    pub weights: [u8; 4],
    pub components: [ScoreComponent; 4],
}

#[derive(Debug, Clone, PartialEq)]
pub struct SixIndicatorScore {
    pub price_score: f64,
    pub volume_score: f64,
    pub markov_regime_score: f64,
    pub autocorrelation_score: f64,
    pub total_score: f64,
    pub weights: [u8; 4],
    pub components: [ScoreComponent; 6],
}

impl SixIndicatorScore {
    /// Direction follows the signed weighted total; an exact zero is neutral.
    pub fn direction(&self) -> Option<&'static str> {
        super::entry_score_direction(self.total_score, 0.0, true)
    }
}

/// Add fixed-weight Markov and lag-one return-autocorrelation readings to the
/// four learned score factors. Both additions are causal and each has a fixed
/// 10-point weight; neither is a win-probability estimate.
pub(super) fn add_entry_model_indicators(
    score: FourIndicatorScore,
    regime: Option<crate::RegimeId>,
    bars: &[StrategyBar],
) -> SixIndicatorScore {
    let markov_component = markov_regime_component(regime);
    let markov_regime_score = markov_component.weighted_points;
    let autocorrelation_component =
        strategy_return_autocorrelation_component(bars);
    let autocorrelation_score = autocorrelation_component.weighted_points;
    let weighted_points = score.components.each_ref().map(|component| component.weighted_points);
    let total_score = entry_score_total(weighted_points, markov_regime_score, autocorrelation_score);
    let components = score.components;
    let components = [
        components[0].clone(),
        components[1].clone(),
        components[2].clone(),
        components[3].clone(),
        markov_component,
        autocorrelation_component,
    ];
    SixIndicatorScore {
        price_score: score.price_score,
        volume_score: score.volume_score,
        markov_regime_score,
        autocorrelation_score,
        total_score,
        weights: score.weights,
        components,
    }
}

fn strategy_return_autocorrelation_component(bars: &[StrategyBar]) -> ScoreComponent {
    let unavailable = || ScoreComponent {
        indicator: "Lag-one Return Autocorrelation",
        state: "UNAVAILABLE".to_string(),
        points: 0.0,
        weight_points: STRATEGY_RETURN_AUTOCORRELATION_COMPONENT_WEIGHT,
        weighted_points: 0.0,
        value: None,
    };
    let return_count = bars
        .len()
        .saturating_sub(1)
        .min(STRATEGY_RETURN_AUTOCORRELATION_LOOKBACK_RETURNS);
    if return_count <= STRATEGY_RETURN_AUTOCORRELATION_MINIMUM_PAIRS {
        return unavailable();
    }
    let pair_count = return_count.saturating_sub(1);
    if pair_count < STRATEGY_RETURN_AUTOCORRELATION_MINIMUM_PAIRS {
        return unavailable();
    }
    let first_bar = bars.len().saturating_sub(return_count + 1);
    let mut returns = [0.0; STRATEGY_RETURN_AUTOCORRELATION_LOOKBACK_RETURNS];
    for (index, pair) in bars[first_bar..].windows(2).enumerate() {
        returns[index] = pair[1].close.ln() - pair[0].close.ln();
        if !returns[index].is_finite() {
            return unavailable();
        }
    }
    let pair_count_f64 = pair_count as f64;
    let mut previous_sum = 0.0;
    let mut next_sum = 0.0;
    for index in 0..pair_count {
        previous_sum += returns[index];
        next_sum += returns[index + 1];
    }
    let mean_previous = previous_sum / pair_count_f64;
    let mean_next = next_sum / pair_count_f64;
    let mut covariance = 0.0;
    let mut previous_variance = 0.0;
    let mut next_variance = 0.0;
    for index in 0..pair_count {
        let previous_delta = returns[index] - mean_previous;
        let next_delta = returns[index + 1] - mean_next;
        covariance += previous_delta * next_delta;
        previous_variance += previous_delta * previous_delta;
        next_variance += next_delta * next_delta;
    }
    covariance /= pair_count_f64;
    previous_variance /= pair_count_f64;
    next_variance /= pair_count_f64;
    if !covariance.is_finite()
        || !previous_variance.is_finite()
        || !next_variance.is_finite()
        || previous_variance <= f64::EPSILON
        || next_variance <= f64::EPSILON
    {
        return unavailable();
    }
    let deviation = next_variance.sqrt();
    let correlation = (covariance / (previous_variance.sqrt() * deviation)).clamp(-1.0, 1.0);
    let slope = covariance / previous_variance;
    let forecast = mean_next + slope * (returns[return_count - 1] - mean_previous);
    let directional_forecast_z = forecast / deviation;
    if !correlation.is_finite() || !directional_forecast_z.is_finite() {
        return unavailable();
    }
    let points = FOUR_INDICATOR_COMPONENT_SCORE_SCALE_MAX
        * directional_forecast_z.clamp(-1.0, 1.0);
    let alignment = if directional_forecast_z > 0.05 {
        "BULLISH"
    } else if directional_forecast_z < -0.05 {
        "BEARISH"
    } else {
        "NO_DIRECTIONAL_EDGE"
    };
    let regime = if correlation.abs() < 0.05 {
        "LOW_AUTOCORRELATION"
    } else if correlation > 0.0 {
        "CONTINUATION"
    } else {
        "MEAN_REVERSION"
    };
    ScoreComponent {
        indicator: "Lag-one Return Autocorrelation",
        state: format!("{alignment}_{regime}"),
        points,
        weight_points: STRATEGY_RETURN_AUTOCORRELATION_COMPONENT_WEIGHT,
        weighted_points: points,
        value: Some(correlation),
    }
}

fn markov_regime_component(regime: Option<crate::RegimeId>) -> ScoreComponent {
    let state = regime.map(|regime| regime.name()).unwrap_or("UNAVAILABLE");
    let aligned_direction = match regime {
        Some(crate::RegimeId::BullTrend | crate::RegimeId::Accumulation) => Some("LONG"),
        Some(crate::RegimeId::BearTrend | crate::RegimeId::Distribution) => Some("SHORT"),
        _ => None,
    };
    let points = match aligned_direction {
        Some("LONG") => FOUR_INDICATOR_COMPONENT_SCORE_SCALE_MAX,
        Some("SHORT") => -FOUR_INDICATOR_COMPONENT_SCORE_SCALE_MAX,
        _ => 0.0,
    };
    let component_state = match (regime, aligned_direction) {
        (Some(_), Some("LONG")) => format!("BULLISH_{state}"),
        (Some(_), Some("SHORT")) => format!("BEARISH_{state}"),
        (Some(crate::RegimeId::PullbackReset), None) => "PULLBACK_RESET_NO_DIRECTION".to_string(),
        (Some(crate::RegimeId::ConsolidationChop), None) => "CONSOLIDATION_CHOP".to_string(),
        _ => state.to_string(),
    };
    ScoreComponent {
        indicator: "Markov Regime",
        state: component_state,
        points,
        weight_points: MARKOV_REGIME_COMPONENT_WEIGHT,
        weighted_points: points,
        value: None,
    }
}

/// Measure the setup strength of a qualifying NWE/DSS entry on its latest
/// timeframe bar. The volume-delta factor is an OHLCV proxy, not aggressor-side
/// trade data; this score is neither a win probability nor an execution quote.
pub fn score_four_indicators(
    bars: &[StrategyBar],
    direction: &str,
    nwe_state: &str,
    nwe_points_out_of_ten: f64,
    dss_state: &str,
    dss_points_out_of_ten: f64,
) -> Option<FourIndicatorScore> {
    score_four_indicators_with_weights(
        bars,
        direction,
        nwe_state,
        nwe_points_out_of_ten,
        dss_state,
        dss_points_out_of_ten,
        FOUR_INDICATOR_EQUAL_WEIGHTS,
    )
}

/// Score the same raw signed −10–10 indicator readings with a validated 40-point
/// weight vector. Raw indicator points remain visible; weights affect only
/// their contribution to the total score.
pub fn score_four_indicators_with_weights(
    bars: &[StrategyBar],
    direction: &str,
    nwe_state: &str,
    nwe_points_out_of_ten: f64,
    dss_state: &str,
    dss_points_out_of_ten: f64,
    weights: [u8; 4],
) -> Option<FourIndicatorScore> {
    if !matches!(direction, "LONG" | "SHORT") {
        return None;
    }
    score_signed_four_indicators(
        bars, nwe_state, nwe_points_out_of_ten, dss_state, dss_points_out_of_ten, weights,
    )
}

pub(super) fn score_signed_four_indicators(
    bars: &[StrategyBar],
    nwe_state: &str,
    nwe_points_out_of_ten: f64,
    dss_state: &str,
    dss_points_out_of_ten: f64,
    weights: [u8; 4],
) -> Option<FourIndicatorScore> {
    let bar = bars.last()?;
    if !valid_four_indicator_weights(weights) {
        return None;
    }
    let nwe_points = bounded_price_component(nwe_points_out_of_ten);
    let dss_points = bounded_price_component(dss_points_out_of_ten);
    let volume = relative_volume_component(bars, bar);
    let delta = estimated_delta_component(bar);
    let mut components = [
        ScoreComponent {
            indicator: "Nadaraya-Watson Envelope",
            state: nwe_state.to_string(),
            points: nwe_points,
            weight_points: FOUR_INDICATOR_EQUAL_WEIGHTS[0],
            weighted_points: nwe_points,
            value: None,
        },
        ScoreComponent {
            indicator: "DSS Bressert",
            state: dss_state.to_string(),
            points: dss_points,
            weight_points: FOUR_INDICATOR_EQUAL_WEIGHTS[1],
            weighted_points: dss_points,
            value: None,
        },
        volume,
        delta,
    ];
    for (component, weight) in components.iter_mut().zip(weights) {
        component.weight_points = weight;
        component.weighted_points = weighted_factor_points(component.points, weight);
    }
    let price_score = components[0].weighted_points + components[1].weighted_points;
    let volume_score = components[2].weighted_points + components[3].weighted_points;
    let total_score = price_score + volume_score;
    Some(FourIndicatorScore {
        price_score,
        volume_score,
        total_score,
        weights,
        components,
    })
}

pub(crate) fn weighted_factor_points(points: f64, weight: u8) -> f64 {
    points * f64::from(weight) / FOUR_INDICATOR_COMPONENT_SCORE_SCALE_MAX
}

pub(crate) fn entry_score_total(
    [nwe, dss, relative_volume, estimated_delta]: [f64; 4],
    markov: f64,
    autocorrelation: f64,
) -> f64 {
    (nwe + dss) + (relative_volume + estimated_delta) + markov + autocorrelation
}

fn valid_four_indicator_weights(weights: [u8; 4]) -> bool {
    weights.iter().all(|weight| weight % 5 == 0)
        && weights.iter().map(|weight| u16::from(*weight)).sum::<u16>() == 40
}

fn bounded_price_component(value: f64) -> f64 {
    if value.is_finite() {
        value.clamp(-FOUR_INDICATOR_COMPONENT_SCORE_SCALE_MAX, FOUR_INDICATOR_COMPONENT_SCORE_SCALE_MAX)
    } else {
        0.0
    }
}

fn relative_volume_component(
    bars: &[StrategyBar],
    current: &StrategyBar,
) -> ScoreComponent {
    let unavailable = || ScoreComponent {
        indicator: "Relative Volume",
        state: "UNAVAILABLE".to_string(),
        points: 0.0,
        weight_points: FOUR_INDICATOR_EQUAL_WEIGHTS[2],
        weighted_points: 0.0,
        value: None,
    };
    if bars.len() <= FOUR_INDICATOR_VOLUME_LOOKBACK_BARS
        || !valid_volume(current.volume)
        || !current.open.is_finite()
        || !current.close.is_finite()
    {
        return unavailable();
    }
    let baseline_start = bars.len() - FOUR_INDICATOR_VOLUME_LOOKBACK_BARS - 1;
    let baseline_end = bars.len() - 1;
    let baseline = &bars[baseline_start..baseline_end];
    if baseline.len() != FOUR_INDICATOR_VOLUME_LOOKBACK_BARS
        || baseline.iter().any(|bar| !valid_volume(bar.volume))
    {
        return unavailable();
    }
    let average = baseline.iter().map(|bar| bar.volume).sum::<f64>()
        / FOUR_INDICATOR_VOLUME_LOOKBACK_BARS as f64;
    if !average.is_finite() || average <= 0.0 {
        return unavailable();
    }
    let relative_volume = current.volume / average;
    if !relative_volume.is_finite() {
        return unavailable();
    }
    let candle_direction = if current.close > current.open { 1.0 } else if current.close < current.open { -1.0 } else { 0.0 };
    let state = if candle_direction == 0.0 {
        "NEUTRAL"
    } else if candle_direction > 0.0 {
        "BULLISH"
    } else {
        "BEARISH"
    };
    let points = if candle_direction != 0.0 {
        ((relative_volume - FOUR_INDICATOR_MIN_RELATIVE_VOLUME_MULTIPLE)
            / (FOUR_INDICATOR_MAX_RELATIVE_VOLUME_MULTIPLE_FOR_FULL_SCORE
                - FOUR_INDICATOR_MIN_RELATIVE_VOLUME_MULTIPLE))
            .clamp(0.0, 1.0)
            * FOUR_INDICATOR_COMPONENT_SCORE_SCALE_MAX * candle_direction
    } else {
        0.0
    };
    ScoreComponent {
        indicator: "Relative Volume",
        state: state.to_string(),
        points,
        weight_points: FOUR_INDICATOR_EQUAL_WEIGHTS[2],
        weighted_points: points,
        value: Some(relative_volume),
    }
}

fn estimated_delta_component(current: &StrategyBar) -> ScoreComponent {
    let unavailable = || ScoreComponent {
        indicator: "Estimated Volume Delta (OHLCV proxy)",
        state: "UNAVAILABLE".to_string(),
        points: 0.0,
        weight_points: FOUR_INDICATOR_EQUAL_WEIGHTS[3],
        weighted_points: 0.0,
        value: None,
    };
    let range = current.high - current.low;
    if !valid_volume(current.volume)
        || !current.high.is_finite()
        || !current.low.is_finite()
        || !current.close.is_finite()
        || !range.is_finite()
        || range <= 0.0
    {
        return unavailable();
    }
    // Close-location value estimates candle pressure; it is not true buy/sell delta.
    let close_location =
        ((2.0 * current.close - current.high - current.low) / range).clamp(-1.0, 1.0);
    let aligned_pressure = close_location;
    let state = if aligned_pressure > 0.0 {
        "BULLISH_PROXY"
    } else if aligned_pressure < 0.0 {
        "BEARISH_PROXY"
    } else {
        "NEUTRAL_PROXY"
    };
    ScoreComponent {
        indicator: "Estimated Volume Delta (OHLCV proxy)",
        state: state.to_string(),
        points: close_location * FOUR_INDICATOR_COMPONENT_SCORE_SCALE_MAX,
        weight_points: FOUR_INDICATOR_EQUAL_WEIGHTS[3],
        weighted_points: close_location * FOUR_INDICATOR_COMPONENT_SCORE_SCALE_MAX,
        value: Some(close_location),
    }
}

fn valid_volume(volume: f64) -> bool {
    volume.is_finite() && volume > 0.0
}
