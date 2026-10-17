use super::common::{SignalConfluenceScore, StrategyBar, StrategySignal};
use super::dss_bressert::classify_latest_spread;
use crate::indicators::nadaraya_watson_envelope::calculate_nadaraya_watson_endpoint_only;
use crate::{calculate_dss, DssConfig, NadarayaWatsonEnvelopeConfig};

pub const DSS_BRESSERT_EXTREME_TURN_CONFIRMATION_ID: &str = "dss_bressert_extreme_turn";
pub const NADARAYA_WATSON_ENVELOPE_TOUCH_CONFIRMATION_ID: &str = "nadaraya_watson_envelope_touch";
pub const SIGNAL_CONFLUENCE_SCORING_MODEL_ID: &str = "dss_nadaraya_extreme_rank_v2";
pub const REQUIRED_SIGNAL_CONFIRMATION_IDS: &[&str] = &[
    DSS_BRESSERT_EXTREME_TURN_CONFIRMATION_ID,
    NADARAYA_WATSON_ENVELOPE_TOUCH_CONFIRMATION_ID,
];

pub const SIGNAL_STRENGTH_SCORE_SCALE_MAX: f64 = 10.0;
pub const SIGNAL_STRENGTH_STRATEGY_WEIGHT: f64 = 0.70;
pub const SIGNAL_STRENGTH_CONFLUENCE_WEIGHT: f64 = 0.30;
pub const NWE_DSS_COMPONENT_SCORE_SCALE_MAX: f64 = SIGNAL_STRENGTH_SCORE_SCALE_MAX;
pub const NWE_DSS_SIGNAL_SCORE_SCALE_MAX: f64 = NWE_DSS_COMPONENT_SCORE_SCALE_MAX * 2.0;
pub const NWE_DSS_SIGNAL_MINIMUM_SCORE_OUT_OF_20: f64 = 1.0;
pub const NWE_DSS_LEGACY_MINIMUM_SCORE_OUT_OF_10: f64 = 5.0;
const MAX_CONFLUENCE_POINTS: f64 = 11.0;
const DSS_BRESSERT_CROSS_POINTS: f64 = 3.0;
const DSS_BRESSERT_NEAR_CROSS_POINTS: f64 = 2.0;
const DSS_EXTREME_ZONE_POINTS: f64 = 3.0;
const STOCKBIT_NWE_DSS_CONFIRMATION_MAX_POINTS: f64 =
    DSS_BRESSERT_CROSS_POINTS + DSS_EXTREME_ZONE_POINTS;
const NADARAYA_BODY_TOUCH_POINTS: f64 = 3.0;
const NADARAYA_WICK_TOUCH_POINTS: f64 = 2.0;
const NADARAYA_NEAR_LINE_POINTS: f64 = 1.0;
const NADARAYA_RECLAIM_POINTS: f64 = 1.0;
const NADARAYA_NEAR_LINE_MAX_ENVELOPE_WIDTH_RATIO: f64 = 0.15;
const DSS_EXTREME_ZONE_LOOKBACK_BARS: usize = 3;

#[derive(Debug, Clone, Copy)]
pub(super) struct NadarayaConfirmation {
    pub state: &'static str,
    pub touch_points: f64,
    pub near_line: bool,
    pub reclaimed: bool,
}

/// Require a completed DSS extreme turn and a matching Nadaraya outer-band
/// touch, then blend strategy strength with a transparent confluence rank.
pub(super) fn apply_required_signal_confirmations(
    bars: &[StrategyBar],
    signals: &mut [StrategySignal],
) {
    if !signals.iter().any(|signal| {
        signal.direction.eq_ignore_ascii_case("LONG")
            || signal.direction.eq_ignore_ascii_case("SHORT")
    }) {
        return;
    }
    let closes = bars.iter().map(|bar| bar.close).collect::<Vec<_>>();
    let highs = bars.iter().map(|bar| bar.high).collect::<Vec<_>>();
    let lows = bars.iter().map(|bar| bar.low).collect::<Vec<_>>();
    let dss_config = DssConfig::default();
    let dss = calculate_dss(&highs, &lows, &closes, &dss_config);
    let nadaraya_config = NadarayaWatsonEnvelopeConfig::default();
    let nadaraya = calculate_nadaraya_watson_endpoint_only(&closes, &nadaraya_config);

    for (index, signal) in signals.iter_mut().enumerate() {
        let direction = match signal.direction.as_str() {
            "LONG" => 1,
            "SHORT" => -1,
            _ => continue,
        };
        let Some(bar) = bars.get(index) else {
            suppress_directional_entry(signal);
            continue;
        };
        let dss_confirmation = dss_confirmation_state(&dss.dss, &dss.trigger, index, direction);
        let dss_extreme_zone =
            has_recent_directional_extreme(&dss.dss, index, direction, &dss_config);
        let Some(nadaraya_confirmation) =
            nadaraya_confirmation_state(bar, &nadaraya, &closes, index, direction)
        else {
            suppress_directional_entry(signal);
            continue;
        };
        if !dss_extreme_zone {
            suppress_directional_entry(signal);
            continue;
        }

        let (dss_state, dss_points) = dss_confirmation.unwrap_or(("NO_CROSS", 0.0));
        let dss_extreme_points = DSS_EXTREME_ZONE_POINTS;
        let nadaraya_near_line_points = if nadaraya_confirmation.near_line {
            NADARAYA_NEAR_LINE_POINTS
        } else {
            0.0
        };
        let nadaraya_reclaim_points = if nadaraya_confirmation.reclaimed {
            NADARAYA_RECLAIM_POINTS
        } else {
            0.0
        };
        let confluence_points = dss_points
            + dss_extreme_points
            + nadaraya_confirmation.touch_points
            + nadaraya_near_line_points
            + nadaraya_reclaim_points;

        let confluence_score = (confluence_points / MAX_CONFLUENCE_POINTS
            * SIGNAL_STRENGTH_SCORE_SCALE_MAX)
            .clamp(0.0, SIGNAL_STRENGTH_SCORE_SCALE_MAX);
        let strategy_strength = if signal.strength.is_finite() {
            signal.strength.clamp(0.0, SIGNAL_STRENGTH_SCORE_SCALE_MAX)
        } else {
            0.0
        };
        signal.strategy_strength = strategy_strength;
        signal.strength = strategy_strength * SIGNAL_STRENGTH_STRATEGY_WEIGHT
            + confluence_score * SIGNAL_STRENGTH_CONFLUENCE_WEIGHT;
        signal.confluence = Some(SignalConfluenceScore {
            score: confluence_score,
            nadaraya_state: nadaraya_confirmation.state.to_string(),
            dss_state: dss_state.to_string(),
            dss_extreme_zone,
            dss_points,
            dss_extreme_points,
            nadaraya_touch_points: nadaraya_confirmation.touch_points,
            nadaraya_near_line_points,
            nadaraya_reclaim_points,
        });
    }
}

/// Add same-direction DSS ranking evidence to NWE crosses and near-band states.
/// The NWE and DSS components each retain their own 0–10 score, and missing
/// DSS evidence contributes zero without suppressing the NWE signal.
pub(super) fn apply_stockbit_nwe_proximity_confluence(
    bars: &[StrategyBar],
    signals: &mut [StrategySignal],
) {
    if !signals.iter().any(|signal| {
        signal.direction.eq_ignore_ascii_case("LONG")
            || signal.direction.eq_ignore_ascii_case("SHORT")
    }) {
        return;
    }
    let highs = bars.iter().map(|bar| bar.high).collect::<Vec<_>>();
    let lows = bars.iter().map(|bar| bar.low).collect::<Vec<_>>();
    let closes = bars.iter().map(|bar| bar.close).collect::<Vec<_>>();
    let dss_config = DssConfig::default();
    let dss = calculate_dss(&highs, &lows, &closes, &dss_config);

    for (index, signal) in signals.iter_mut().enumerate() {
        let direction = match signal.direction.as_str() {
            "LONG" => 1,
            "SHORT" => -1,
            _ => continue,
        };
        let dss_state = classify_latest_spread(&dss.dss[..=index], &dss.trigger[..=index])
            .map(|(_, state)| state)
            .unwrap_or("UNAVAILABLE");
        let dss_points = dss_confirmation_state(&dss.dss, &dss.trigger, index, direction)
            .map(|(_, points)| points)
            .unwrap_or(0.0);
        let dss_extreme_zone =
            has_recent_directional_extreme(&dss.dss, index, direction, &dss_config);
        let dss_extreme_points = if dss_extreme_zone {
            DSS_EXTREME_ZONE_POINTS
        } else {
            0.0
        };
        let confluence_score = stockbit_nwe_dss_confluence_score(dss_points + dss_extreme_points);
        let strategy_strength = if signal.strength.is_finite() {
            signal.strength.clamp(0.0, SIGNAL_STRENGTH_SCORE_SCALE_MAX)
        } else {
            0.0
        };
        signal.strategy_strength = strategy_strength;
        signal.strength = strategy_strength + confluence_score;
        signal.confluence = Some(SignalConfluenceScore {
            score: confluence_score,
            nadaraya_state: signal.regime.clone(),
            dss_state: dss_state.to_string(),
            dss_extreme_zone,
            dss_points,
            dss_extreme_points,
            nadaraya_touch_points: 0.0,
            nadaraya_near_line_points: 0.0,
            nadaraya_reclaim_points: 0.0,
        });
    }
}

pub(super) fn stockbit_nwe_dss_confluence_score(points: f64) -> f64 {
    if !points.is_finite() {
        return 0.0;
    }
    (points / STOCKBIT_NWE_DSS_CONFIRMATION_MAX_POINTS * SIGNAL_STRENGTH_SCORE_SCALE_MAX)
        .clamp(0.0, SIGNAL_STRENGTH_SCORE_SCALE_MAX)
}

fn suppress_directional_entry(signal: &mut StrategySignal) {
    let exit_signal = signal.exit_signal.take();
    *signal = StrategySignal::default();
    signal.exit_signal = exit_signal;
}

fn dss_confirmation_state(
    dss: &[f64],
    trigger: &[f64],
    index: usize,
    direction: i8,
) -> Option<(&'static str, f64)> {
    if direction == 0 || index >= dss.len() || index >= trigger.len() {
        return None;
    }
    let (score, state) = classify_latest_spread(&dss[..=index], &trigger[..=index])?;
    if score.signum() != direction || score.abs() < 2 {
        return None;
    }
    if state.ends_with("_CROSS") {
        Some(("CROSS", DSS_BRESSERT_CROSS_POINTS))
    } else if state.ends_with("_NEAR") {
        Some(("NEAR_CROSS", DSS_BRESSERT_NEAR_CROSS_POINTS))
    } else {
        None
    }
}

pub(super) fn nadaraya_confirmation_state(
    bar: &StrategyBar,
    endpoint: &crate::NadarayaWatsonEnvelopeMode,
    closes: &[f64],
    index: usize,
    direction: i8,
) -> Option<NadarayaConfirmation> {
    if direction == 0 || index >= closes.len() {
        return None;
    }
    let upper = *endpoint.upper.get(index)?;
    let lower = *endpoint.lower.get(index)?;
    if ![upper, lower, bar.open, bar.close, bar.high, bar.low]
        .into_iter()
        .all(f64::is_finite)
        || upper < lower
        || bar.low > bar.high
        || bar.low > bar.open.min(bar.close)
        || bar.high < bar.open.max(bar.close)
    {
        return None;
    }
    let touched_band = if direction > 0 { lower } else { upper };
    let inside_envelope = lower <= bar.close && bar.close <= upper;
    let candle_touches_band = bar.low <= touched_band && touched_band <= bar.high;
    if !inside_envelope || !candle_touches_band {
        return None;
    }
    let body_touches_band =
        bar.open.min(bar.close) <= touched_band && touched_band <= bar.open.max(bar.close);
    let state = match (direction > 0, body_touches_band) {
        (true, true) => "LOWER_ENVELOPE_BODY_TOUCH",
        (true, false) => "LOWER_ENVELOPE_WICK_TOUCH",
        (false, true) => "UPPER_ENVELOPE_BODY_TOUCH",
        (false, false) => "UPPER_ENVELOPE_WICK_TOUCH",
    };
    let envelope_width = upper - lower;
    let near_line = envelope_width > 0.0
        && (bar.close - touched_band).abs()
            <= envelope_width * NADARAYA_NEAR_LINE_MAX_ENVELOPE_WIDTH_RATIO;
    let reclaimed = index > 0
        && closes
            .get(index - 1)
            .copied()
            .zip(endpoint.lower.get(index - 1).copied())
            .zip(endpoint.upper.get(index - 1).copied())
            .is_some_and(|((previous_close, previous_lower), previous_upper)| {
                previous_close.is_finite()
                    && previous_lower.is_finite()
                    && previous_upper.is_finite()
                    && if direction > 0 {
                        previous_close < previous_lower && bar.close >= lower
                    } else {
                        previous_close > previous_upper && bar.close <= upper
                    }
            });
    Some(NadarayaConfirmation {
        state,
        touch_points: if body_touches_band {
            NADARAYA_BODY_TOUCH_POINTS
        } else {
            NADARAYA_WICK_TOUCH_POINTS
        },
        near_line,
        reclaimed,
    })
}

pub(super) fn has_recent_directional_extreme(
    dss: &[f64],
    index: usize,
    direction: i8,
    config: &DssConfig,
) -> bool {
    if direction == 0 || index == 0 || index >= dss.len() {
        return false;
    }
    let Some(current) = dss.get(index).copied().filter(|value| value.is_finite()) else {
        return false;
    };
    let Some(previous) = dss
        .get(index - 1)
        .copied()
        .filter(|value| value.is_finite())
    else {
        return false;
    };
    let recent_start = index
        .saturating_add(1)
        .saturating_sub(DSS_EXTREME_ZONE_LOOKBACK_BARS);
    let reached_extreme = dss[recent_start..=index].iter().any(|value| {
        value.is_finite()
            && if direction > 0 {
                *value <= config.oversold
            } else {
                *value >= config.overbought
            }
    });
    reached_extreme
        && if direction > 0 {
            current > previous
        } else {
            current < previous
        }
}

#[cfg(test)]
pub(super) fn dss_confirms_direction(
    dss: &[f64],
    trigger: &[f64],
    index: usize,
    direction: i8,
) -> bool {
    dss_confirmation_state(dss, trigger, index, direction).is_some()
}
