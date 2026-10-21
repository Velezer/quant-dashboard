//! Venue-aware day-trade signal generation.
//!
//! Public strategy types and functions stay re-exported here while the
//! venue-specific implementations live in focused modules.

pub(super) mod common;
mod confirmations;
mod dss_bressert;
mod dss_bressert_4h;
mod four_indicator_score;
mod entry_score;
mod idx;
mod indicator_combination;
mod pintu;
mod tokocrypto;

#[cfg(test)]
use common::{idx_ara_arb, stockbit_cutoff_due};
pub(crate) use common::{idx_tick_size, round_idx_tick_at, IdxTickRounding};
pub use common::{
    is_pintu_golden_cross_strategy, round_price, round_price_up, IdxIntradayMomentumConfig,
    IdxOpeningRangeConfig, IdxStrategyConfig, IdxTimeSeriesMomentumConfig, PintuConfluenceConfig,
    PintuGoldenCrossConfig, PintuSignalTimeframeContext, PintuStrategyConfig,
    SignalConfluenceScore, StrategyBar, StrategySignal, PINTU_GOLDEN_CROSS_CONTEXT_TIMEFRAME,
    PINTU_GOLDEN_CROSS_INTERVAL_NS, PINTU_GOLDEN_CROSS_LEGACY_SIGNAL_STRATEGY_IDS,
    PINTU_GOLDEN_CROSS_LEGACY_STRATEGY_IDS, PINTU_GOLDEN_CROSS_MINIMUM_PLAN_BARS,
    PINTU_GOLDEN_CROSS_MIN_R_MULTIPLE, PINTU_GOLDEN_CROSS_SIGNAL_STRATEGY_ID,
    PINTU_GOLDEN_CROSS_STRATEGY_ID, PINTU_GOLDEN_CROSS_TIMEFRAME,
    PINTU_GOLDEN_CROSS_V2_EVIDENCE_ID,
};
pub use confirmations::{
    DSS_BRESSERT_EXTREME_TURN_CONFIRMATION_ID, NADARAYA_WATSON_ENVELOPE_TOUCH_CONFIRMATION_ID,
    NWE_DSS_COMPONENT_SCORE_SCALE_MAX, NWE_DSS_LEGACY_MINIMUM_SCORE_OUT_OF_10,
    NWE_DSS_SIGNAL_MINIMUM_SCORE_OUT_OF_20, NWE_DSS_SIGNAL_SCORE_SCALE_MAX,
    REQUIRED_SIGNAL_CONFIRMATION_IDS, SIGNAL_CONFLUENCE_SCORING_MODEL_ID,
    SIGNAL_STRENGTH_CONFLUENCE_WEIGHT, SIGNAL_STRENGTH_SCORE_SCALE_MAX,
    SIGNAL_STRENGTH_STRATEGY_WEIGHT,
};
pub use dss_bressert::{
    calculate_dss_bressert_mtf, DssBressertFrameResult, DssBressertMtfResult, DssBressertTimeframe,
    DssBressertTimeframeInput, DSS_BRESSERT_MINIMUM_HISTORY_BARS, DSS_BRESSERT_MTF_STRATEGY_ID,
    DSS_BRESSERT_MTF_TIMEFRAMES, DSS_BRESSERT_NEAR_SPREAD_THRESHOLD,
};
pub use dss_bressert_4h::{
    crypto_dss_bressert_signal_strategy_id, generate_stockbit_dss_bressert_4h_signals,
    stockbit_dss_bressert_signal_strategy_id, StockbitDssBressert4hSignalConfig,
    CRYPTO_DSS_BRESSERT_4H_SIGNAL_STRATEGY_ID, STOCKBIT_DSS_BRESSERT_4H_SIGNAL_CONFIG,
    STOCKBIT_DSS_BRESSERT_4H_SIGNAL_SCORE_MODEL_ID,
    STOCKBIT_DSS_BRESSERT_4H_SIGNAL_SCORE_MODEL_ID_LEGACY,
    STOCKBIT_DSS_BRESSERT_4H_SIGNAL_STRATEGY_ID,
};
pub use entry_score::{score_entry_signal, EntryModelKind};
pub(crate) use entry_score::visit_entry_signals;
pub use four_indicator_score::{
    score_four_indicators, FourIndicatorScore, ScoreComponent,
    FOUR_INDICATOR_COMPONENT_SCORE_SCALE_MAX, FOUR_INDICATOR_GROUP_SCORE_SCALE_MAX,
    FOUR_INDICATOR_MAX_RELATIVE_VOLUME_MULTIPLE_FOR_FULL_SCORE,
    FOUR_INDICATOR_MINIMUM_SCORE_OUT_OF_40, FOUR_INDICATOR_MIN_RELATIVE_VOLUME_MULTIPLE,
    FOUR_INDICATOR_SCORE_SCALE_MAX, FOUR_INDICATOR_VOLUME_LOOKBACK_BARS,
};

#[cfg(test)]
#[path = "strategies/four_indicator_score_tests.rs"]
mod four_indicator_score_tests;
pub use idx::{
    generate_idx_first_half_hour_momentum_signals, generate_idx_opening_range_signals,
    generate_idx_quality_signals, generate_idx_signals, generate_idx_time_of_day_momentum_signals,
    generate_idx_time_series_momentum_signals,
};
pub use indicator_combination::{
    crypto_nwe_signal_strategy_id, generate_indicator_combination_signals,
    generate_stockbit_nwe_4h_signals, indicator_combination_indicator_ids,
    stockbit_nwe_4h_minimum_history_bars, stockbit_nwe_signal_strategy_id,
    IndicatorCombinationConfig, StockbitNwe4hSignalConfig, CRYPTO_NWE_4H_SIGNAL_STRATEGY_ID,
    MAX_INDICATOR_COMBINATION_SIZE, STOCKBIT_NWE_4H_ASSET_CATEGORY_ID, STOCKBIT_NWE_4H_EXCHANGE_ID,
    STOCKBIT_NWE_4H_PREVIOUS_BINARY_CROSS_STRATEGY_ID, STOCKBIT_NWE_4H_PREVIOUS_CROSS_STRATEGY_ID,
    STOCKBIT_NWE_4H_PREVIOUS_RANKED_CROSS_STRATEGY_ID, STOCKBIT_NWE_4H_PREVIOUS_SCORE_STRATEGY_ID,
    STOCKBIT_NWE_4H_PRODUCT_ID, STOCKBIT_NWE_4H_SIGNAL_CONFIG,
    STOCKBIT_NWE_4H_SIGNAL_SCORE_MODEL_ID, STOCKBIT_NWE_4H_SIGNAL_SCORE_MODEL_ID_LEGACY,
    STOCKBIT_NWE_4H_SIGNAL_STRATEGY_ID, STOCKBIT_NWE_4H_TIMEFRAME,
};
pub use pintu::{
    generate_pintu_confluence_signals, generate_pintu_golden_cross_signals,
    generate_pintu_golden_cross_signals_with_context, generate_pintu_signals,
};
pub use tokocrypto::{
    generate_tokocrypto_confluence_signals, is_tokenized_stock_confluence_strategy,
    is_tokocrypto_confluence_strategy, is_tokocrypto_spot_confluence_strategy,
    tokocrypto_confluence_indicator_ids, TokocryptoConfluenceConfig,
    TOKENIZED_STOCK_CONFLUENCE_STRATEGY_ID, TOKOCRYPTO_CONFLUENCE_SIGNAL_STRATEGY_ID,
    TOKOCRYPTO_SPOT_CONFLUENCE_STRATEGY_ID,
};

#[cfg(test)]
#[path = "strategies_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "strategies/dss_bressert_tests.rs"]
mod dss_bressert_tests;

#[cfg(test)]
#[path = "strategies/stockbit_dss_bressert_4h_tests.rs"]
mod stockbit_dss_bressert_4h_tests;

#[cfg(test)]
#[path = "strategies/confirmations_tests.rs"]
mod confirmations_tests;

#[cfg(test)]
#[path = "strategies/indicator_combination_tests.rs"]
mod indicator_combination_tests;

#[cfg(test)]
#[path = "strategies/stockbit_nwe_4h_tests.rs"]
mod stockbit_nwe_4h_tests;

#[cfg(test)]
#[path = "strategies/tokocrypto_tests.rs"]
mod tokocrypto_tests;
