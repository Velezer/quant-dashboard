use super::common::{finite, SignalConfluenceScore, StrategyBar, StrategySignal};
use super::confirmations::{
    NWE_DSS_COMPONENT_SCORE_SCALE_MAX, NWE_DSS_SIGNAL_MINIMUM_SCORE_OUT_OF_20,
    NWE_DSS_SIGNAL_SCORE_SCALE_MAX, SIGNAL_STRENGTH_SCORE_SCALE_MAX,
};
use super::dss_bressert::classify_latest_spread;
use super::indicator_combination::{
    stockbit_nwe_4h_near_band_candidate, StockbitNwe4hSignalConfig,
};
use crate::indicators::nadaraya_watson_envelope::{
    calculate_nadaraya_watson_endpoint_only, NadarayaWatsonEnvelopeMode,
};
use crate::{calculate_dss, DssConfig, NadarayaWatsonEnvelopeConfig};

pub const STOCKBIT_DSS_BRESSERT_4H_SIGNAL_STRATEGY_ID: &str =
    "stockbit_dss_bressert_near_cross_4h_v1";
pub const STOCKBIT_DSS_BRESSERT_4H_SIGNAL_SCORE_MODEL_ID_LEGACY: &str =
    "dss_bressert_near_cross_nwe_rank_v2";
pub const STOCKBIT_DSS_BRESSERT_4H_SIGNAL_SCORE_MODEL_ID: &str =
    "dss_bressert_near_cross_nwe_volume_rank_v4";
pub const CRYPTO_DSS_BRESSERT_4H_SIGNAL_STRATEGY_ID: &str = "crypto_dss_bressert_near_cross_4h_v1";

/// Return the stable crypto DSS Bressert strategy identity for a supported screener frame.
pub fn crypto_dss_bressert_signal_strategy_id(timeframe: &str) -> Option<String> {
    super::SIGNAL_SCREENER_TIMEFRAMES
        .iter()
        .any(|supported| *supported == timeframe)
        .then(|| format!("crypto_dss_bressert_near_cross_{timeframe}_v1"))
}

/// Return the timeframe-specific Stockbit DSS Bressert screener identity.
pub fn stockbit_dss_bressert_signal_strategy_id(timeframe: &str) -> Option<String> {
    super::SIGNAL_SCREENER_TIMEFRAMES
        .iter()
        .any(|supported| *supported == timeframe)
        .then(|| {
            if timeframe == super::indicator_combination::STOCKBIT_NWE_4H_TIMEFRAME {
                STOCKBIT_DSS_BRESSERT_4H_SIGNAL_STRATEGY_ID.to_string()
            } else {
                format!("stockbit_dss_bressert_near_cross_{timeframe}_v1")
            }
        })
}

#[derive(Debug, Clone, Copy)]
pub struct StockbitDssBressert4hSignalConfig {
    pub minimum_score_out_of_20: f64,
}

pub const STOCKBIT_DSS_BRESSERT_4H_SIGNAL_CONFIG: StockbitDssBressert4hSignalConfig =
    StockbitDssBressert4hSignalConfig {
        minimum_score_out_of_20: NWE_DSS_SIGNAL_MINIMUM_SCORE_OUT_OF_20,
    };

impl Default for StockbitDssBressert4hSignalConfig {
    fn default() -> Self {
        STOCKBIT_DSS_BRESSERT_4H_SIGNAL_CONFIG
    }
}

impl StockbitDssBressert4hSignalConfig {
    fn is_valid(self) -> bool {
        self.minimum_score_out_of_20.is_finite()
            && self.minimum_score_out_of_20 > 0.0
            && self.minimum_score_out_of_20 <= NWE_DSS_SIGNAL_SCORE_SCALE_MAX
    }
}

/// Score DSS Bressert crosses and approaching near-crosses on the supplied frame.
/// Same-direction NWE support adds a separate 0–10 component to the DSS score.
pub fn generate_stockbit_dss_bressert_4h_signals(bars: &[StrategyBar]) -> Vec<StrategySignal> {
    let mut signals = vec![StrategySignal::default(); bars.len()];
    let config = STOCKBIT_DSS_BRESSERT_4H_SIGNAL_CONFIG;
    if bars.len() < super::dss_bressert::DSS_BRESSERT_MINIMUM_HISTORY_BARS
        || !config.is_valid()
        || !valid_completed_history(bars)
    {
        return signals;
    }

    let highs = bars.iter().map(|bar| bar.high).collect::<Vec<_>>();
    let lows = bars.iter().map(|bar| bar.low).collect::<Vec<_>>();
    let closes = bars.iter().map(|bar| bar.close).collect::<Vec<_>>();
    let dss_config = DssConfig::default();
    let dss = calculate_dss(&highs, &lows, &closes, &dss_config);
    let nwe_config = NadarayaWatsonEnvelopeConfig::default();
    let nwe = calculate_nadaraya_watson_endpoint_only(&closes, &nwe_config);
    let nwe_signal_config = StockbitNwe4hSignalConfig::default();
    let minimum_dss_history_bars = super::dss_bressert::DSS_BRESSERT_MINIMUM_HISTORY_BARS;

    for (index, signal) in signals
        .iter_mut()
        .enumerate()
        .skip(minimum_dss_history_bars - 1)
    {
        let Some((dss_score, dss_state)) =
            classify_latest_spread(&dss.dss[..=index], &dss.trigger[..=index])
        else {
            continue;
        };
        let Some(spread) = dss
            .dss
            .get(index)
            .copied()
            .zip(dss.trigger.get(index).copied())
            .map(|(dss_value, trigger_value)| dss_value - trigger_value)
            .filter(|spread| finite(*spread))
        else {
            continue;
        };
        let Some((direction, regime, strategy_strength)) =
            stockbit_dss_bressert_4h_candidate(spread, dss_score, dss_state, config)
        else {
            continue;
        };
        let Some(bar) = bars.get(index) else {
            continue;
        };
        if !finite(bar.close) || bar.close <= 0.0 {
            continue;
        }

        let (nwe_state, confluence_score) =
            stockbit_nwe_4h_support(bar.close, index, &nwe, direction, nwe_signal_config);
        let score = strategy_strength + confluence_score;
        if !score.is_finite()
            || score + 1e-12 < config.minimum_score_out_of_20
            || strategy_strength > NWE_DSS_COMPONENT_SCORE_SCALE_MAX
            || confluence_score > NWE_DSS_COMPONENT_SCORE_SCALE_MAX
        {
            continue;
        }

        signal.direction = direction.to_string();
        signal.strength = score.clamp(0.0, NWE_DSS_SIGNAL_SCORE_SCALE_MAX);
        signal.strategy_strength = strategy_strength;
        signal.regime = regime.to_string();
        signal.confluence = Some(SignalConfluenceScore {
            score: confluence_score,
            nadaraya_state: nwe_state.to_string(),
            dss_state: dss_state.to_string(),
            dss_extreme_zone: false,
            dss_points: 0.0,
            dss_extreme_points: 0.0,
            nadaraya_touch_points: 0.0,
            nadaraya_near_line_points: 0.0,
            nadaraya_reclaim_points: 0.0,
        });
    }
    signals
}

fn valid_completed_history(bars: &[StrategyBar]) -> bool {
    bars.iter().all(|bar| {
        [bar.open, bar.high, bar.low, bar.close]
            .into_iter()
            .all(finite)
            && bar.close > 0.0
            && bar.low <= bar.open.min(bar.close)
            && bar.high >= bar.open.max(bar.close)
            && bar.low <= bar.high
    }) && bars
        .windows(2)
        .all(|window| window[0].time < window[1].time)
}

pub(super) fn stockbit_dss_bressert_4h_candidate(
    spread: f64,
    classification_score: i8,
    state: &str,
    config: StockbitDssBressert4hSignalConfig,
) -> Option<(&'static str, &'static str, f64)> {
    if !finite(spread) || !config.is_valid() {
        return None;
    }
    let (direction, regime, strategy_strength) = match state {
        "BULLISH_CROSS" if classification_score > 0 => (
            "LONG",
            "DSS_BULLISH_CROSS_LONG",
            NWE_DSS_COMPONENT_SCORE_SCALE_MAX,
        ),
        "BEARISH_CROSS" if classification_score < 0 => (
            "SHORT",
            "DSS_BEARISH_CROSS_SHORT",
            NWE_DSS_COMPONENT_SCORE_SCALE_MAX,
        ),
        "APPROACHING_BULLISH_NEAR" if classification_score > 0 => (
            "LONG",
            "DSS_BULLISH_NEAR_CROSS_LONG",
            near_cross_component_score(spread.abs(), config)?,
        ),
        "APPROACHING_BEARISH_NEAR" if classification_score < 0 => (
            "SHORT",
            "DSS_BEARISH_NEAR_CROSS_SHORT",
            near_cross_component_score(spread.abs(), config)?,
        ),
        _ => return None,
    };
    strategy_strength
        .is_finite()
        .then_some((direction, regime, strategy_strength))
}

pub(super) fn near_cross_component_score(
    absolute_spread: f64,
    config: StockbitDssBressert4hSignalConfig,
) -> Option<f64> {
    let spread_threshold = super::dss_bressert::DSS_BRESSERT_NEAR_SPREAD_THRESHOLD;
    if !finite(absolute_spread) || !config.is_valid() || absolute_spread > spread_threshold {
        return None;
    }
    let proximity = absolute_spread / spread_threshold;
    let score = NWE_DSS_COMPONENT_SCORE_SCALE_MAX * (1.0 - proximity);
    score.is_finite().then_some(score)
}

fn stockbit_nwe_4h_support(
    close: f64,
    index: usize,
    nwe: &NadarayaWatsonEnvelopeMode,
    dss_direction: &str,
    config: StockbitNwe4hSignalConfig,
) -> (&'static str, f64) {
    if index.saturating_add(1)
        < super::indicator_combination::stockbit_nwe_4h_minimum_history_bars()
    {
        return ("NWE_UNAVAILABLE", 0.0);
    }
    let Some(crossed_above_upper) = nwe.crossed_above_upper.get(index).copied() else {
        return ("NWE_UNAVAILABLE", 0.0);
    };
    let Some(crossed_below_lower) = nwe.crossed_below_lower.get(index).copied() else {
        return ("NWE_UNAVAILABLE", 0.0);
    };
    let Some(upper) = nwe.upper.get(index).copied() else {
        return ("NWE_UNAVAILABLE", 0.0);
    };
    let Some(lower) = nwe.lower.get(index).copied() else {
        return ("NWE_UNAVAILABLE", 0.0);
    };
    let support = match (crossed_below_lower, crossed_above_upper) {
        (true, false) => Some((
            "NWE_LOWER_BAND_CROSS_LONG",
            "LONG",
            SIGNAL_STRENGTH_SCORE_SCALE_MAX,
        )),
        (false, true) => Some((
            "NWE_UPPER_BAND_CROSS_SHORT",
            "SHORT",
            SIGNAL_STRENGTH_SCORE_SCALE_MAX,
        )),
        (true, true) => return ("AMBIGUOUS_NWE_CROSS", 0.0),
        (false, false) => stockbit_nwe_4h_near_band_candidate(close, lower, upper, config)
            .map(|(direction, state, score)| (state, direction, score)),
    };
    let Some((state, direction, score)) = support else {
        return ("NO_DIRECTIONAL_NWE_SUPPORT", 0.0);
    };
    if direction == dss_direction && score.is_finite() {
        (state, score.clamp(0.0, SIGNAL_STRENGTH_SCORE_SCALE_MAX))
    } else {
        ("OPPOSING_NWE_DIRECTION", 0.0)
    }
}
