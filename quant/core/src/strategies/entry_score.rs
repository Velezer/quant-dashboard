use super::four_indicator_score::add_entry_model_indicators;
use super::{
    score_four_indicators_with_weights, FourIndicatorScore, SignalConfluenceScore,
    SixIndicatorScore, StrategyBar, StrategySignal, FOUR_INDICATOR_EQUAL_WEIGHTS,
    stockbit_nwe_4h_minimum_history_bars, DSS_BRESSERT_MINIMUM_HISTORY_BARS,
    STOCKBIT_DSS_BRESSERT_4H_SIGNAL_SCORE_MODEL_ID, STOCKBIT_NWE_4H_SIGNAL_SCORE_MODEL_ID,
};
use crate::indicators::nadaraya_watson_envelope::calculate_nadaraya_watson_endpoint_only;

const SIGNED_INPUT_STATE: &str = "SIGNED_INDICATOR_INPUTS";
const DSS_FULL_SCORE_SPREAD: f64 = super::DSS_BRESSERT_NEAR_SPREAD_THRESHOLD;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryModelKind { Nwe, Dss }

impl EntryModelKind {
    pub fn score_model_id(self) -> &'static str {
        match self {
            Self::Nwe => STOCKBIT_NWE_4H_SIGNAL_SCORE_MODEL_ID,
            Self::Dss => STOCKBIT_DSS_BRESSERT_4H_SIGNAL_SCORE_MODEL_ID,
        }
    }

    pub fn minimum_history_bars(self) -> usize {
        match self {
            Self::Nwe => stockbit_nwe_4h_minimum_history_bars(),
            Self::Dss => DSS_BRESSERT_MINIMUM_HISTORY_BARS,
        }
    }
}

/// Signed scores use independent indicator directions. A candidate's old
/// LONG/SHORT classification does not constrain or clip the observations.
pub fn score_entry_signal(
    bars: &[StrategyBar], signal: &StrategySignal, kind: EntryModelKind,
) -> Option<SixIndicatorScore> {
    score_entry_signal_with_weights(bars, signal, kind, FOUR_INDICATOR_EQUAL_WEIGHTS)
}

pub fn score_entry_signal_with_weights(
    bars: &[StrategyBar], signal: &StrategySignal, kind: EntryModelKind, weights: [u8; 4],
) -> Option<SixIndicatorScore> {
    let markov = crate::classify_regimes(bars, &crate::MarkovRegimeConfig::default())
        .last().copied().flatten();
    score_entry_signal_with_markov(bars, signal, kind, weights, markov)
}

fn price_inputs(bars: &[StrategyBar]) -> Option<(Vec<f64>, crate::DssResult, crate::NadarayaWatsonEnvelopeMode)> {
    let closes: Vec<_> = bars.iter().map(|bar| bar.close).collect();
    let highs: Vec<_> = bars.iter().map(|bar| bar.high).collect();
    let lows: Vec<_> = bars.iter().map(|bar| bar.low).collect();
    if closes.is_empty() { return None; }
    let dss = crate::calculate_dss(&highs, &lows, &closes, &crate::DssConfig::default());
    let nwe = calculate_nadaraya_watson_endpoint_only(&closes, &crate::NadarayaWatsonEnvelopeConfig::default());
    Some((closes, dss, nwe))
}

fn input_signal(close: f64, index: usize, dss: &crate::DssResult, nwe: &crate::NadarayaWatsonEnvelopeMode) -> StrategySignal {
    let nwe_points = nwe.lower.get(index).zip(nwe.upper.get(index))
        .filter(|(lower, upper)| lower.is_finite() && upper.is_finite() && upper > lower)
        .map(|(lower, upper)| (((lower + upper) / 2.0 - close) / ((upper - lower) / 2.0) * 10.0).clamp(-10.0, 10.0));
    let dss_points = dss.dss.get(index).zip(dss.trigger.get(index))
        .filter(|(value, trigger)| value.is_finite() && trigger.is_finite())
        .map(|(value, trigger)| ((value - trigger) / DSS_FULL_SCORE_SPREAD * 10.0).clamp(-10.0, 10.0));
    StrategySignal {
        // This is a calculation hint, not an entry decision. The six-factor
        // weighted sum supplies the eventual direction.
        direction: "LONG".into(),
        regime: SIGNED_INPUT_STATE.into(),
        strategy_strength: nwe_points.unwrap_or(0.0),
        confluence: Some(SignalConfluenceScore {
            score: dss_points.unwrap_or(0.0),
            nadaraya_state: if nwe_points.is_some() { "SIGNED_BAND_POSITION" } else { "UNAVAILABLE" }.into(),
            dss_state: if dss_points.is_some() { "SIGNED_DSS_SPREAD" } else { "UNAVAILABLE" }.into(),
            dss_extreme_zone: false, dss_points: 0.0, dss_extreme_points: 0.0,
            nadaraya_touch_points: 0.0, nadaraya_near_line_points: 0.0, nadaraya_reclaim_points: 0.0
        }),
        ..StrategySignal::default()
    }
}

/// Prepare the latest independent price-factor readings even when the old
/// NWE-led or DSS-led strategy would emit no directional candidate.
pub fn generate_signed_entry_signal(bars: &[StrategyBar], kind: EntryModelKind) -> Option<StrategySignal> {
    if bars.len() < kind.minimum_history_bars() { return None; }
    let (closes, dss, nwe) = price_inputs(bars)?;
    Some(input_signal(*closes.last()?, closes.len() - 1, &dss, &nwe))
}

fn score_price_inputs(
    bars: &[StrategyBar], signal: &StrategySignal, weights: [u8; 4],
) -> Option<FourIndicatorScore> {
    let support = signal.confluence.as_ref()?;
    score_four_indicators_with_weights(
        bars, "LONG", &support.nadaraya_state, signal.strategy_strength,
        &support.dss_state, support.score, weights,
    )
}

/// Use only the causal prefix and its matching Markov state. Historical replay
/// supplies precomputed price-factor hints to avoid repeated prefix replays.
pub fn score_entry_signal_with_markov(
    bars: &[StrategyBar], signal: &StrategySignal, kind: EntryModelKind,
    weights: [u8; 4], markov_regime: Option<crate::RegimeId>,
) -> Option<SixIndicatorScore> {
    let generated;
    let inputs = if signal.regime == SIGNED_INPUT_STATE { signal } else {
        generated = generate_signed_entry_signal(bars, kind)?;
        &generated
    };
    let score = score_price_inputs(bars, inputs, weights)?;
    Some(add_entry_model_indicators(score, "LONG", markov_regime, bars))
}

/// Visit every eligible bar, including neutral and opposing price readings.
/// Direction and qualification are determined after applying the chosen weights.
pub(crate) fn visit_entry_signals(
    bars: &[StrategyBar], kind: EntryModelKind, mut visitor: impl FnMut(usize, &StrategySignal),
) {
    if bars.len() < kind.minimum_history_bars() { return; }
    let Some((closes, dss, nwe)) = price_inputs(bars) else { return; };
    for index in kind.minimum_history_bars().saturating_sub(1)..bars.len() {
        visitor(index, &input_signal(closes[index], index, &dss, &nwe));
    }
}

/// Visit sampled, causal equal-weight signed scores without replaying indicators.
/// Callers choose timestamps before looking at forward outcomes.
pub fn visit_sampled_entry_scores(
    bars: &[StrategyBar], kind: EntryModelKind,
    sample: impl Fn(usize) -> bool, mut visitor: impl FnMut(usize, f64),
) {
    let regimes = crate::classify_regimes(bars, &crate::MarkovRegimeConfig::default());
    visit_entry_signals(bars, kind, |index, signal| {
        if !sample(index) { return; }
        if let Some(score) = score_entry_signal_with_markov(
            &bars[..=index], signal, kind, FOUR_INDICATOR_EQUAL_WEIGHTS,
            regimes.get(index).copied().flatten(),
        ) {
            if score.total_score.is_finite() { visitor(index, score.total_score); }
        }
    });
}
