use super::{
    generate_stockbit_dss_bressert_4h_signals, score_four_indicators,
    stockbit_nwe_4h_minimum_history_bars, FourIndicatorScore, StrategyBar, StrategySignal,
    DSS_BRESSERT_MINIMUM_HISTORY_BARS, STOCKBIT_DSS_BRESSERT_4H_SIGNAL_SCORE_MODEL_ID,
    STOCKBIT_NWE_4H_SIGNAL_SCORE_MODEL_ID,
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

/// Score the supplied latest signal using the same lead/confluence mapping as
/// the screener. The caller must pass only history available at signal time.
pub fn score_entry_signal(
    bars: &[StrategyBar],
    signal: &StrategySignal,
    kind: EntryModelKind,
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
    score_four_indicators(
        bars, &signal.direction, nwe_state, nwe_points, dss_state, dss_points,
    )
}

/// Replay exact latest NWE outputs on causal prefixes. DSS smoothing is causal
/// and shared across the replay instead of being recomputed for every prefix.
pub(crate) fn visit_entry_signals(
    bars: &[StrategyBar],
    kind: EntryModelKind,
    mut visitor: impl FnMut(usize, &StrategySignal),
) {
    match kind {
        EntryModelKind::Dss => {
            let signals = generate_stockbit_dss_bressert_4h_signals(bars);
            for (index, signal) in signals.iter().enumerate() {
                visitor(index, signal);
            }
        }
        EntryModelKind::Nwe => {
            let highs: Vec<_> = bars.iter().map(|bar| bar.high).collect();
            let lows: Vec<_> = bars.iter().map(|bar| bar.low).collect();
            let closes: Vec<_> = bars.iter().map(|bar| bar.close).collect();
            let dss = crate::calculate_dss(&highs, &lows, &closes, &crate::DssConfig::default());
            for index in kind.minimum_history_bars().saturating_sub(1)..bars.len() {
                let signals = super::indicator_combination::generate_stockbit_nwe_4h_signals_with_dss(
                    &bars[..=index], &dss,
                );
                if let Some(signal) = signals.last() {
                    visitor(index, signal);
                }
            }
        }
    }
}
