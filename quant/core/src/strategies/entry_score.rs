use super::four_indicator_score::{add_entry_model_indicators, score_signed_four_indicators};
use super::{
    SixIndicatorScore, StrategyBar, DSS_BRESSERT_NEAR_SPREAD_THRESHOLD,
    FOUR_INDICATOR_COMPONENT_SCORE_SCALE_MAX,
    FOUR_INDICATOR_EQUAL_WEIGHTS,
};
use crate::indicators::nadaraya_watson_envelope::calculate_nadaraya_watson_endpoint_only;

pub const ENTRY_SCORE_MODEL_ID: &str = "signed_six_indicator_combined_v1";
/// Model warm-up. Each factor can become available later on its own history.
pub const ENTRY_SCORE_MINIMUM_HISTORY_BARS: usize = 50;

/// Apply the score floor and venue direction policy to the weighted total.
pub fn entry_score_direction(
    total: f64,
    minimum_score: f64,
    allow_short: bool,
) -> Option<&'static str> {
    if !total.is_finite() || !minimum_score.is_finite() || total.abs() < minimum_score {
        return None;
    }
    if total > 0.0 {
        Some("LONG")
    } else if total < 0.0 && allow_short {
        Some("SHORT")
    } else {
        None
    }
}

struct PriceFactors {
    dss: crate::DssResult,
    nwe: crate::NadarayaWatsonEnvelopeMode,
}

impl PriceFactors {
    fn calculate(bars: &[StrategyBar]) -> Self {
        let closes: Vec<_> = bars.iter().map(|bar| bar.close).collect();
        let highs: Vec<_> = bars.iter().map(|bar| bar.high).collect();
        let lows: Vec<_> = bars.iter().map(|bar| bar.low).collect();
        Self {
            dss: crate::calculate_dss(&highs, &lows, &closes, &crate::DssConfig::default()),
            nwe: calculate_nadaraya_watson_endpoint_only(
                &closes,
                &crate::NadarayaWatsonEnvelopeConfig::default(),
            ),
        }
    }

    fn score(
        &self,
        bars: &[StrategyBar],
        weights: [u8; 4],
        regime: Option<crate::RegimeId>,
    ) -> Option<SixIndicatorScore> {
        let index = bars.len().checked_sub(1)?;
        let close = bars[index].close;
        let scale = FOUR_INDICATOR_COMPONENT_SCORE_SCALE_MAX;
        let nwe = self
            .nwe
            .lower
            .get(index)
            .zip(self.nwe.upper.get(index))
            .filter(|(lower, upper)| lower.is_finite() && upper.is_finite() && upper > lower)
            .map(|(lower, upper)| {
                (((lower + upper) / 2.0 - close) / ((upper - lower) / 2.0) * scale)
                    .clamp(-scale, scale)
            });
        let dss = self
            .dss
            .dss
            .get(index)
            .zip(self.dss.trigger.get(index))
            .filter(|(value, trigger)| value.is_finite() && trigger.is_finite())
            .map(|(value, trigger)| {
                ((value - trigger) / DSS_BRESSERT_NEAR_SPREAD_THRESHOLD * scale)
                    .clamp(-scale, scale)
            });
        let score = score_signed_four_indicators(
            bars,
            if nwe.is_some() {
                "SIGNED_BAND_POSITION"
            } else {
                "UNAVAILABLE"
            },
            nwe.unwrap_or(0.0),
            if dss.is_some() {
                "SIGNED_DSS_SPREAD"
            } else {
                "UNAVAILABLE"
            },
            dss.unwrap_or(0.0),
            weights,
        )?;
        Some(add_entry_model_indicators(score, regime, bars))
    }
}

/// Compute the live score from the last bar in the supplied causal history.
pub fn combined_entry_score(bars: &[StrategyBar], weights: [u8; 4]) -> Option<SixIndicatorScore> {
    if bars.len() < ENTRY_SCORE_MINIMUM_HISTORY_BARS {
        return None;
    }
    let regimes = crate::classify_regimes(bars, &crate::MarkovRegimeConfig::default());
    PriceFactors::calculate(bars).score(bars, weights, regimes.last().copied().flatten())
}

/// Visit the same causal scores as the live model while computing price studies once.
pub(crate) fn visit_combined_entry_scores(
    bars: &[StrategyBar],
    sample: impl Fn(usize) -> bool,
    mut visitor: impl FnMut(usize, SixIndicatorScore),
) {
    if bars.len() < ENTRY_SCORE_MINIMUM_HISTORY_BARS {
        return;
    }
    let prices = PriceFactors::calculate(bars);
    let regimes = crate::classify_regimes(bars, &crate::MarkovRegimeConfig::default());
    for (index, regime) in regimes
        .into_iter()
        .enumerate()
        .skip(ENTRY_SCORE_MINIMUM_HISTORY_BARS - 1)
    {
        if sample(index) {
            if let Some(score) = prices.score(&bars[..=index], FOUR_INDICATOR_EQUAL_WEIGHTS, regime)
            {
                visitor(index, score);
            }
        }
    }
}

/// Sample causal scores at timestamps selected before looking at forward outcomes.
pub fn visit_sampled_entry_scores(
    bars: &[StrategyBar],
    sample: impl Fn(usize) -> bool,
    mut visitor: impl FnMut(usize, f64),
) {
    visit_combined_entry_scores(bars, sample, |index, score| {
        if score.total_score.is_finite() {
            visitor(index, score.total_score);
        }
    });
}
