use super::common::StrategyBar;

pub const FOUR_INDICATOR_SCORE_SCALE_MAX: f64 = 40.0;
pub const FOUR_INDICATOR_GROUP_SCORE_SCALE_MAX: f64 = 20.0;
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
    pub value: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FourIndicatorScore {
    pub price_score: f64,
    pub volume_score: f64,
    pub total_score: f64,
    pub components: [ScoreComponent; 4],
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
    let bar = bars.last()?;
    if !matches!(direction, "LONG" | "SHORT") {
        return None;
    }
    let direction_sign = if direction == "LONG" { 1.0 } else { -1.0 };
    let nwe_points = bounded_price_component(nwe_points_out_of_ten);
    let dss_points = bounded_price_component(dss_points_out_of_ten);
    let volume = relative_volume_component(bars, bar, direction_sign);
    let delta = estimated_delta_component(bar, direction_sign);
    let price_score = nwe_points + dss_points;
    let volume_score = volume.points + delta.points;
    let total_score = price_score + volume_score;
    Some(FourIndicatorScore {
        price_score,
        volume_score,
        total_score,
        components: [
            ScoreComponent {
                indicator: "Nadaraya-Watson Envelope",
                state: nwe_state.to_string(),
                points: nwe_points,
                value: None,
            },
            ScoreComponent {
                indicator: "DSS Bressert",
                state: dss_state.to_string(),
                points: dss_points,
                value: None,
            },
            volume,
            delta,
        ],
    })
}

fn bounded_price_component(value: f64) -> f64 {
    if value.is_finite() {
        value.clamp(0.0, FOUR_INDICATOR_COMPONENT_SCORE_SCALE_MAX)
    } else {
        0.0
    }
}

fn relative_volume_component(
    bars: &[StrategyBar],
    current: &StrategyBar,
    direction_sign: f64,
) -> ScoreComponent {
    let unavailable = || ScoreComponent {
        indicator: "Relative Volume",
        state: "UNAVAILABLE".to_string(),
        points: 0.0,
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
    let candle_direction = (current.close - current.open).signum();
    let state = if candle_direction == 0.0 {
        "NEUTRAL"
    } else if candle_direction == direction_sign {
        "ALIGNED"
    } else {
        "OPPOSED"
    };
    let points = if state == "ALIGNED" {
        ((relative_volume - FOUR_INDICATOR_MIN_RELATIVE_VOLUME_MULTIPLE)
            / (FOUR_INDICATOR_MAX_RELATIVE_VOLUME_MULTIPLE_FOR_FULL_SCORE
                - FOUR_INDICATOR_MIN_RELATIVE_VOLUME_MULTIPLE))
            .clamp(0.0, 1.0)
            * FOUR_INDICATOR_COMPONENT_SCORE_SCALE_MAX
    } else {
        0.0
    };
    ScoreComponent {
        indicator: "Relative Volume",
        state: state.to_string(),
        points,
        value: Some(relative_volume),
    }
}

fn estimated_delta_component(current: &StrategyBar, direction_sign: f64) -> ScoreComponent {
    let unavailable = || ScoreComponent {
        indicator: "Estimated Volume Delta (OHLCV proxy)",
        state: "UNAVAILABLE".to_string(),
        points: 0.0,
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
    let aligned_pressure = direction_sign * close_location;
    let state = if aligned_pressure > 0.0 {
        "ALIGNED_PROXY"
    } else if aligned_pressure < 0.0 {
        "OPPOSED_PROXY"
    } else {
        "NEUTRAL_PROXY"
    };
    ScoreComponent {
        indicator: "Estimated Volume Delta (OHLCV proxy)",
        state: state.to_string(),
        points: aligned_pressure.max(0.0) * FOUR_INDICATOR_COMPONENT_SCORE_SCALE_MAX,
        value: Some(close_location),
    }
}

fn valid_volume(volume: f64) -> bool {
    volume.is_finite() && volume > 0.0
}
