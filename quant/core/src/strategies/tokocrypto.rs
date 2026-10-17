use super::common::{finite, round_price, StrategyBar, StrategySignal};
use super::confirmations::apply_required_signal_confirmations;
use super::indicator_combination::{
    generate_indicator_combination_signals_with_minimum, IndicatorCombinationConfig,
};

pub const TOKOCRYPTO_CONFLUENCE_SIGNAL_STRATEGY_ID: &str = "tokocrypto_confluence_2_of_4_v1";
pub const TOKOCRYPTO_SPOT_CONFLUENCE_STRATEGY_ID: &str = "tokocrypto_spot_confluence_2_of_4_v1";
pub const TOKENIZED_STOCK_CONFLUENCE_STRATEGY_ID: &str = "tokenized_stock_confluence_2_of_4_v1";

const TOKOCRYPTO_CONFLUENCE_INDICATOR_IDS: [&str; 4] =
    ["ema_20_50", "rsi_14", "range_breakout", "relative_volume"];

/// Tunable defaults for the Tokocrypto spot and tokenized-stock research model.
/// The required vote count is part of this strategy version's identity.
#[derive(Debug, Clone, Copy)]
pub struct TokocryptoConfluenceConfig {
    pub indicator_config: IndicatorCombinationConfig,
    pub minimum_agreeing_indicators: usize,
    pub target_pct: f64,
    pub stop_loss_pct: f64,
}

impl Default for TokocryptoConfluenceConfig {
    fn default() -> Self {
        Self {
            indicator_config: IndicatorCombinationConfig {
                relative_volume_min: 1.8,
                ..IndicatorCombinationConfig::default()
            },
            minimum_agreeing_indicators: 2,
            target_pct: 0.014,
            stop_loss_pct: 0.009,
        }
    }
}

impl TokocryptoConfluenceConfig {
    fn is_valid(self) -> bool {
        self.minimum_agreeing_indicators >= 2
            && self.minimum_agreeing_indicators <= TOKOCRYPTO_CONFLUENCE_INDICATOR_IDS.len()
            && finite(self.target_pct)
            && self.target_pct > 0.0
            && finite(self.stop_loss_pct)
            && self.stop_loss_pct > 0.0
            && self.stop_loss_pct < 1.0
    }
}

pub fn tokocrypto_confluence_indicator_ids() -> &'static [&'static str] {
    &TOKOCRYPTO_CONFLUENCE_INDICATOR_IDS
}

pub fn is_tokocrypto_confluence_strategy(strategy: &str) -> bool {
    is_tokocrypto_spot_confluence_strategy(strategy)
        || is_tokenized_stock_confluence_strategy(strategy)
}

pub fn is_tokocrypto_spot_confluence_strategy(strategy: &str) -> bool {
    matches!(
        strategy,
        TOKOCRYPTO_SPOT_CONFLUENCE_STRATEGY_ID | "tokocrypto_spot"
    )
}

pub fn is_tokenized_stock_confluence_strategy(strategy: &str) -> bool {
    matches!(
        strategy,
        TOKENIZED_STOCK_CONFLUENCE_STRATEGY_ID | "tokenized_stock"
    )
}

/// Generate long-only Tokocrypto research signals when enough of the four
/// optional trend, momentum, breakout, and relative-volume votes agree.
pub fn generate_tokocrypto_confluence_signals(
    bars: &[StrategyBar],
    config: TokocryptoConfluenceConfig,
) -> Vec<StrategySignal> {
    let signals = vec![StrategySignal::default(); bars.len()];
    if !config.is_valid() {
        return signals;
    }
    let indicator_ids = TOKOCRYPTO_CONFLUENCE_INDICATOR_IDS
        .iter()
        .map(|id| (*id).to_string())
        .collect::<Vec<_>>();
    let Ok(mut generated) = generate_indicator_combination_signals_with_minimum(
        bars,
        &indicator_ids,
        config.indicator_config,
        false,
        config.minimum_agreeing_indicators,
        "TOKOCRYPTO_CONFLUENCE",
    ) else {
        return signals;
    };

    for (index, signal) in generated.iter_mut().enumerate() {
        if signal.direction != "LONG" {
            continue;
        }
        let close = bars[index].close;
        if !finite(close) || close <= 0.0 {
            *signal = StrategySignal::default();
            continue;
        }
        signal.suggested_sl = round_price(close * (1.0 - config.stop_loss_pct));
        signal.suggested_tp1 = round_price(close * (1.0 + config.target_pct));
        signal.suggested_tp2 = round_price(close * (1.0 + config.target_pct * 2.0));
    }
    apply_required_signal_confirmations(bars, &mut generated);
    generated
}
