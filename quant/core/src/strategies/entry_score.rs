use super::{
    add_markov_regime_score, score_four_indicators_with_weights, FOUR_INDICATOR_EQUAL_WEIGHTS,
    stockbit_nwe_4h_minimum_history_bars, FiveIndicatorScore, FourIndicatorScore, StrategyBar,
    StrategySignal, DSS_BRESSERT_MINIMUM_HISTORY_BARS,
    STOCKBIT_DSS_BRESSERT_4H_SIGNAL_SCORE_MODEL_ID, STOCKBIT_NWE_4H_SIGNAL_SCORE_MODEL_ID,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryModelKind {
    Nwe,
    Dss,
}

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

/// Score the supplied latest signal using the screener's five-factor model.
/// The caller must pass only history available at signal time.
pub fn score_entry_signal(
    bars: &[StrategyBar],
    signal: &StrategySignal,
    kind: EntryModelKind,
) -> Option<FiveIndicatorScore> {
    score_entry_signal_with_weights(bars, signal, kind, FOUR_INDICATOR_EQUAL_WEIGHTS)
}

/// Apply the selected live scoring profile and latest causal Markov regime to
/// a signal. Invalid profile weights fail closed to no score.
pub fn score_entry_signal_with_weights(
    bars: &[StrategyBar],
    signal: &StrategySignal,
    kind: EntryModelKind,
    weights: [u8; 4],
) -> Option<FiveIndicatorScore> {
    let score = score_entry_signal_four_indicators_with_weights(bars, signal, kind, weights)?;
    let markov_regime = crate::classify_regimes(bars, &crate::MarkovRegimeConfig::default())
        .last()
        .copied()
        .flatten();
    Some(add_markov_regime_score(score, &signal.direction, markov_regime))
}

fn score_entry_signal_four_indicators_with_weights(
    bars: &[StrategyBar],
    signal: &StrategySignal,
    kind: EntryModelKind,
    weights: [u8; 4],
) -> Option<FourIndicatorScore> {
    let confluence = signal.confluence.as_ref();
    let (nwe_points, nwe_state, dss_points, dss_state) = match kind {
        EntryModelKind::Dss => (
            confluence.map(|score| score.score).unwrap_or(0.0),
            confluence.map(|score| score.nadaraya_state.as_str()).unwrap_or("UNAVAILABLE"),
            signal.strategy_strength,
            signal.regime.as_str(),
        ),
        EntryModelKind::Nwe => (
            signal.strategy_strength,
            signal.regime.as_str(),
            confluence.map(|score| score.score).unwrap_or(0.0),
            confluence.map(|score| score.dss_state.as_str()).unwrap_or("UNAVAILABLE"),
        ),
    };
    score_four_indicators_with_weights(
        bars, &signal.direction, nwe_state, nwe_points, dss_state, dss_points,
        weights,
    )
}

/// Score one entry with the causal Markov state already classified at the
/// signal bar. Callers that process histories should classify all bars once
/// and pass the matching state to avoid repeated historical calculations.
pub fn score_entry_signal_with_markov(
    bars: &[StrategyBar],
    signal: &StrategySignal,
    kind: EntryModelKind,
    weights: [u8; 4],
    markov_regime: Option<crate::RegimeId>,
) -> Option<FiveIndicatorScore> {
    let score = score_entry_signal_four_indicators_with_weights(bars, signal, kind, weights)?;
    Some(add_markov_regime_score(
        score,
        &signal.direction,
        markov_regime,
    ))
}

/// Replay the causal entry directions without retaining a full strategy-signal
/// object per candle. The model indicators are calculated once per history.
pub(crate) fn visit_entry_signals(
    bars: &[StrategyBar],
    kind: EntryModelKind,
    visitor: impl FnMut(usize, &StrategySignal),
) {
    match kind {
        EntryModelKind::Dss => {
            super::dss_bressert_4h::visit_stockbit_dss_bressert_4h_signals(bars, visitor);
        }
        EntryModelKind::Nwe => {
            let highs: Vec<_> = bars.iter().map(|bar| bar.high).collect();
            let lows: Vec<_> = bars.iter().map(|bar| bar.low).collect();
            let closes: Vec<_> = bars.iter().map(|bar| bar.close).collect();
            let dss = crate::calculate_dss(&highs, &lows, &closes, &crate::DssConfig::default());
            super::indicator_combination::visit_stockbit_nwe_4h_signals_with_dss(
                bars, &dss, visitor,
            );
        }
    }
}
