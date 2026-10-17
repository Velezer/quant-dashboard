//! # quant_core
#![recursion_limit = "512"]

//!
//! Institutional Quantitative Trading & Analytics Engine in Rust.
//! High-performance workspace crate providing:
//! - Indicator calculations (DSS Bressert double EMA smoothing).
//! - Discrete-Time Markov Chain (DTMC) transition probability solvers, persistence, and stationary eigenvectors.
//! - Multi-factor institutional tail risk scoring and cascade probability estimation.

pub mod day_trade;
pub mod evidence;
pub mod indicators;
pub mod markov;
pub mod plans;
pub mod risk;
pub mod strategies;
pub mod venue_rules;

/// Shared Unix time units used by the service and quantitative calculations.
pub const SECONDS_PER_DAY: i64 = 86_400;
pub const SECONDS_PER_MINUTE: i64 = 60;
pub const SECONDS_PER_HOUR: i64 = 60 * SECONDS_PER_MINUTE;
pub const MILLISECONDS_PER_SECOND: i64 = 1_000;
pub const NANOS_PER_MILLISECOND: i64 = 1_000_000;
pub const NANOS_PER_SECOND: i64 = 1_000_000_000;
pub const NANOS_PER_DAY: i64 = SECONDS_PER_DAY * NANOS_PER_SECOND;
/// UTC offset for Jakarta local trading dates, in seconds.
pub const JAKARTA_UTC_OFFSET_SECONDS: i64 = 7 * SECONDS_PER_HOUR;

pub(crate) fn round_decimal(value: f64, decimals: i32) -> f64 {
    let scale = 10f64.powi(decimals);
    (value * scale).round() / scale
}

pub(crate) fn finite(value: f64) -> bool {
    value.is_finite()
}

/// Convert a Unix-epoch day number to its Gregorian `(year, month, day)`.
pub fn gregorian_date_from_unix_days(unix_days: i64) -> (i64, u32, u32) {
    const DAYS_TO_CIVIL_EPOCH: i64 = 719_468;
    const DAYS_PER_GREGORIAN_ERA: i64 = 146_097;
    let shifted_days = unix_days + DAYS_TO_CIVIL_EPOCH;
    let era = shifted_days.div_euclid(DAYS_PER_GREGORIAN_ERA);
    let day_of_era = shifted_days - era * DAYS_PER_GREGORIAN_ERA;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    let year = year + i64::from(month <= 2);
    (year, month as u32, day as u32)
}

#[cfg(test)]
#[path = "venue_rules_tests.rs"]
mod venue_rules_tests;

pub use day_trade::compute_day_trade_profile;
pub use evidence::{
    day_trade_minimum_win_rate_pct, evaluate_historical_evidence,
    evaluate_historical_evidence_with_fees, evaluate_nwe_dss_exit_selection,
    historical_evidence_minimum_cagr_pct, historical_evidence_minimum_calendar_month_return_pct,
    historical_evidence_profile, HistoricalEvidenceProfile, MIN_ASYMMETRIC_DAY_TRADE_WIN_RATE_PCT,
    MIN_ASYMMETRIC_REWARD_RISK, MIN_DAY_TRADE_PROFIT_FACTOR, MIN_DAY_TRADE_WIN_RATE_PCT,
    MIN_EVIDENCE_SAMPLE_DAYS, MIN_EVIDENCE_SAMPLE_TRADES,
    MIN_MULTIBAGGER_CALENDAR_MONTH_RETURN_PCT, MIN_STOCKBIT_DAY_TRADE_CAGR_PCT, MIN_SWING_CAGR_PCT,
    NWE_DSS_EXIT_CANDIDATES_BARS, REQUIRED_SIGNAL_CONFIRMATION_EVIDENCE_SUFFIX,
    RETIRED_SIGNAL_CONFIRMATION_EVIDENCE_SUFFIX,
};
pub use indicators::{
    calculate_dss, calculate_nadaraya_watson_envelope, DssConfig, DssResult,
    NadarayaWatsonEnvelopeConfig, NadarayaWatsonEnvelopeMetadata, NadarayaWatsonEnvelopeMode,
    NadarayaWatsonEnvelopeResult, DAY_TRADE_NWE_BANDWIDTH_BARS, DAY_TRADE_NWE_ENVELOPE_MULTIPLIER,
    DAY_TRADE_NWE_WINDOW_BARS, NWE_ATTRIBUTION, NWE_LICENSE, NWE_LICENSE_URL, NWE_METHOD_ID,
    NWE_METHOD_LABEL,
};
pub use markov::{
    build_markov_payload, build_markov_payload_for_timeframe, classify_regimes, DtmcModel,
    MarkovAsset, MarkovRegimeConfig, RegimeId,
};
pub use plans::{
    build_registered_plan, build_registered_plan_with_signal_context, RegisteredPlan,
    StructureSnapshot,
};
pub use risk::{
    calculate_position_size, compute_liquidity_risk, compute_tail_risk, discover_liquidity_zones,
    evaluate_liquidity_risk, project_reward_risk_account_growth, simulate_reward_risk_rule,
    LiquidityRiskReport, LiquidityRiskResult, LiquidityZone, LiquidityZoneInput,
    PositionSizeRequest, PositionSizeResult, RewardRiskAccountGrowthProjection,
    RewardRiskGrowthPoint, RewardRiskGrowthProjectionConfig, RewardRiskRule, RewardRiskSimulation,
    TailRiskScoreResult, TradeDirection, DEFAULT_GROWTH_STARTING_BALANCE_USD,
    DEFAULT_GROWTH_TRADE_SIZE_PCT_OF_EQUITY, DEFAULT_GROWTH_TRADING_DAYS,
    GROWTH_TRADING_DAYS_PER_YEAR, MAX_GROWTH_TRADE_SIZE_PCT_OF_EQUITY, MAX_GROWTH_TRADING_DAYS,
    MAX_SETUP_RISK_REWARD_MOVE_PCT, MAX_SETUP_TRADES_PER_DAY, MAX_SETUP_WIN_RATE_PCT,
    MIN_GROWTH_STARTING_BALANCE_UNITS, MIN_GROWTH_TRADE_SIZE_PCT_OF_EQUITY,
    MIN_GROWTH_TRADING_DAYS, MIN_SETUP_RISK_REWARD_MOVE_PCT, MIN_SETUP_TRADES_PER_DAY,
    MIN_SETUP_WIN_RATE_PCT,
};
pub use strategies::{
    calculate_dss_bressert_mtf, crypto_dss_bressert_signal_strategy_id,
    crypto_nwe_signal_strategy_id, generate_idx_first_half_hour_momentum_signals,
    generate_idx_opening_range_signals, generate_idx_quality_signals, generate_idx_signals,
    generate_idx_time_of_day_momentum_signals, generate_idx_time_series_momentum_signals,
    generate_indicator_combination_signals, generate_pintu_confluence_signals,
    generate_pintu_golden_cross_signals, generate_pintu_signals,
    generate_stockbit_dss_bressert_4h_signals, generate_stockbit_nwe_4h_signals,
    generate_tokocrypto_confluence_signals, indicator_combination_indicator_ids,
    is_pintu_golden_cross_strategy, is_tokenized_stock_confluence_strategy,
    is_tokocrypto_confluence_strategy, is_tokocrypto_spot_confluence_strategy, round_price_up,
    score_four_indicators, stockbit_dss_bressert_signal_strategy_id,
    stockbit_nwe_4h_minimum_history_bars, stockbit_nwe_signal_strategy_id,
    tokocrypto_confluence_indicator_ids, DssBressertFrameResult, DssBressertMtfResult,
    DssBressertTimeframe, DssBressertTimeframeInput, FourIndicatorScore, IdxIntradayMomentumConfig,
    IdxOpeningRangeConfig, IdxStrategyConfig, IdxTimeSeriesMomentumConfig,
    IndicatorCombinationConfig, PintuConfluenceConfig, PintuGoldenCrossConfig,
    PintuSignalTimeframeContext, PintuStrategyConfig, ScoreComponent, SignalConfluenceScore,
    StockbitDssBressert4hSignalConfig, StockbitNwe4hSignalConfig, StrategyBar, StrategySignal,
    TokocryptoConfluenceConfig, CRYPTO_DSS_BRESSERT_4H_SIGNAL_STRATEGY_ID,
    CRYPTO_NWE_4H_SIGNAL_STRATEGY_ID, DSS_BRESSERT_EXTREME_TURN_CONFIRMATION_ID,
    DSS_BRESSERT_MINIMUM_HISTORY_BARS, DSS_BRESSERT_MTF_STRATEGY_ID, DSS_BRESSERT_MTF_TIMEFRAMES,
    DSS_BRESSERT_NEAR_SPREAD_THRESHOLD, FOUR_INDICATOR_COMPONENT_SCORE_SCALE_MAX,
    FOUR_INDICATOR_GROUP_SCORE_SCALE_MAX,
    FOUR_INDICATOR_MAX_RELATIVE_VOLUME_MULTIPLE_FOR_FULL_SCORE,
    FOUR_INDICATOR_MINIMUM_SCORE_OUT_OF_40, FOUR_INDICATOR_MIN_RELATIVE_VOLUME_MULTIPLE,
    FOUR_INDICATOR_SCORE_SCALE_MAX, FOUR_INDICATOR_VOLUME_LOOKBACK_BARS,
    MAX_INDICATOR_COMBINATION_SIZE, NADARAYA_WATSON_ENVELOPE_TOUCH_CONFIRMATION_ID,
    NWE_DSS_COMPONENT_SCORE_SCALE_MAX, NWE_DSS_LEGACY_MINIMUM_SCORE_OUT_OF_10,
    NWE_DSS_SIGNAL_MINIMUM_SCORE_OUT_OF_20, NWE_DSS_SIGNAL_SCORE_SCALE_MAX,
    PINTU_GOLDEN_CROSS_CONTEXT_TIMEFRAME, PINTU_GOLDEN_CROSS_INTERVAL_NS,
    PINTU_GOLDEN_CROSS_LEGACY_SIGNAL_STRATEGY_IDS, PINTU_GOLDEN_CROSS_LEGACY_STRATEGY_IDS,
    PINTU_GOLDEN_CROSS_MINIMUM_PLAN_BARS, PINTU_GOLDEN_CROSS_MIN_R_MULTIPLE,
    PINTU_GOLDEN_CROSS_SIGNAL_STRATEGY_ID, PINTU_GOLDEN_CROSS_STRATEGY_ID,
    PINTU_GOLDEN_CROSS_TIMEFRAME, PINTU_GOLDEN_CROSS_V2_EVIDENCE_ID,
    REQUIRED_SIGNAL_CONFIRMATION_IDS, SIGNAL_CONFLUENCE_SCORING_MODEL_ID,
    SIGNAL_STRENGTH_CONFLUENCE_WEIGHT, SIGNAL_STRENGTH_SCORE_SCALE_MAX,
    SIGNAL_STRENGTH_STRATEGY_WEIGHT, STOCKBIT_DSS_BRESSERT_4H_SIGNAL_CONFIG,
    STOCKBIT_DSS_BRESSERT_4H_SIGNAL_SCORE_MODEL_ID,
    STOCKBIT_DSS_BRESSERT_4H_SIGNAL_SCORE_MODEL_ID_LEGACY,
    STOCKBIT_DSS_BRESSERT_4H_SIGNAL_STRATEGY_ID, STOCKBIT_NWE_4H_ASSET_CATEGORY_ID,
    STOCKBIT_NWE_4H_EXCHANGE_ID, STOCKBIT_NWE_4H_PREVIOUS_BINARY_CROSS_STRATEGY_ID,
    STOCKBIT_NWE_4H_PREVIOUS_CROSS_STRATEGY_ID, STOCKBIT_NWE_4H_PREVIOUS_RANKED_CROSS_STRATEGY_ID,
    STOCKBIT_NWE_4H_PREVIOUS_SCORE_STRATEGY_ID, STOCKBIT_NWE_4H_PRODUCT_ID,
    STOCKBIT_NWE_4H_SIGNAL_CONFIG, STOCKBIT_NWE_4H_SIGNAL_SCORE_MODEL_ID,
    STOCKBIT_NWE_4H_SIGNAL_SCORE_MODEL_ID_LEGACY, STOCKBIT_NWE_4H_SIGNAL_STRATEGY_ID,
    STOCKBIT_NWE_4H_TIMEFRAME, TOKENIZED_STOCK_CONFLUENCE_STRATEGY_ID,
    TOKOCRYPTO_CONFLUENCE_SIGNAL_STRATEGY_ID, TOKOCRYPTO_SPOT_CONFLUENCE_STRATEGY_ID,
};
pub use venue_rules::{
    stockbit_candidate_price_eligible, venue_fee_policy, venue_fee_policy_for_category,
    VenueFeePolicy, PINTU_MAKER_FEE_PCT, PINTU_MAKER_FEE_RATE, PINTU_TAKER_FEE_ASSUMPTION_LABEL,
    PINTU_TAKER_FEE_PCT, PINTU_TAKER_FEE_RATE, PINTU_TAKER_FEE_ROUND_TRIP_PCT,
    STOCKBIT_BUY_FEE_PCT, STOCKBIT_FEE_ASSUMPTION_LABEL, STOCKBIT_MIN_CANDIDATE_PRICE_IDR,
    STOCKBIT_SELL_FEE_PCT, TOKOCRYPTO_SPOT_UNKNOWN_FEE_LABEL,
    TOKOCRYPTO_TOKENIZED_STOCK_UNKNOWN_FEE_LABEL,
};
