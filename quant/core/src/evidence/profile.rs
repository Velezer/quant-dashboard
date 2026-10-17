use crate::strategies::{
    generate_idx_first_half_hour_momentum_signals, generate_idx_opening_range_signals,
    generate_idx_quality_signals, generate_idx_signals, generate_idx_time_of_day_momentum_signals,
    generate_idx_time_series_momentum_signals, generate_pintu_confluence_signals,
    generate_pintu_golden_cross_signals, generate_pintu_signals,
    generate_tokocrypto_confluence_signals, is_pintu_golden_cross_strategy,
    IdxIntradayMomentumConfig, IdxOpeningRangeConfig, IdxStrategyConfig,
    IdxTimeSeriesMomentumConfig, PintuConfluenceConfig, PintuGoldenCrossConfig,
    PintuStrategyConfig, StrategyBar, StrategySignal, TokocryptoConfluenceConfig,
    PINTU_GOLDEN_CROSS_TIMEFRAME, TOKENIZED_STOCK_CONFLUENCE_STRATEGY_ID,
    TOKOCRYPTO_SPOT_CONFLUENCE_STRATEGY_ID,
};
use crate::venue_rules::{
    PINTU_TAKER_FEE_ASSUMPTION_LABEL, PINTU_TAKER_FEE_PCT, STOCKBIT_BUY_FEE_PCT,
    STOCKBIT_FEE_ASSUMPTION_LABEL, STOCKBIT_SELL_FEE_PCT, TOKOCRYPTO_SPOT_UNKNOWN_FEE_LABEL,
    TOKOCRYPTO_TOKENIZED_STOCK_UNKNOWN_FEE_LABEL,
};

const FIFTEEN_MINUTE_EVIDENCE_INTERVAL: &str = "15m";
const STOCKBIT_DAILY_EVIDENCE_TIMEFRAME: &str = "1d";
const STOCKBIT_DAY_TRADE_MAX_HOLD_BARS: usize = 25;
const STOCKBIT_SWING_MAX_HOLD_BARS: usize = 20;
const STOCKBIT_TSMOM_MAX_HOLD_BARS: usize = 63;
const STOCKBIT_MULTIBAGGER_MAX_HOLD_BARS: usize = 60;
const PINTU_FUTURES_DAY_TRADE_MAX_HOLD_BARS: usize = 96;
const TOKOCRYPTO_SPOT_DAY_TRADE_MAX_HOLD_BARS: usize = 96;
const TOKENIZED_STOCK_DAY_TRADE_MAX_HOLD_BARS: usize = 96;
pub const REQUIRED_SIGNAL_CONFIRMATION_EVIDENCE_SUFFIX: &str = "_dss_extreme_nwe_touch_v2";
pub const RETIRED_SIGNAL_CONFIRMATION_EVIDENCE_SUFFIX: &str = "_dss_bressert_nadaraya_watson_v1";

macro_rules! confirmed_evidence_id {
    ($base:literal) => {
        concat!($base, "_dss_extreme_nwe_touch_v2")
    };
}

/// The local history and venue assumptions required to build a stored
/// performance observation for one supported strategy identity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HistoricalEvidenceProfile {
    pub strategy_id: &'static str,
    pub evidence_class: &'static str,
    pub history_interval: &'static str,
    pub max_hold_bars: usize,
    pub buy_fee_pct: f64,
    pub sell_fee_pct: f64,
    pub fee_model: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EvidenceStrategy {
    StockbitDayTrade,
    StockbitSwing,
    StockbitTimeSeriesMomentumSwing,
    StockbitOpeningRangeMomentum,
    StockbitFirstHalfHourMomentum,
    StockbitTimeOfDayMomentum,
    StockbitQualityDayTradeV1,
    StockbitPullbackDayTradeV1,
    StockbitMomentumDayTradeV1,
    StockbitMultibagger,
    PintuFuturesDayTrade,
    PintuFuturesQualityDayTradeV1,
    PintuFuturesConfluenceQualityDayTradeV1,
    PintuGoldenCross,
    TokocryptoSpotDayTrade,
    TokocryptoSpotConfluence2Of4V1,
    TokocryptoSpotQualityDayTradeV1,
    TokenizedStockDayTrade,
    TokenizedStockConfluence2Of4V1,
    TokenizedStockQualityDayTradeV1,
}

impl EvidenceStrategy {
    pub(super) fn minimum_cagr_pct(self) -> f64 {
        match self {
            Self::StockbitSwing | Self::StockbitTimeSeriesMomentumSwing => {
                super::MIN_SWING_CAGR_PCT
            }
            Self::StockbitDayTrade
            | Self::StockbitOpeningRangeMomentum
            | Self::StockbitFirstHalfHourMomentum
            | Self::StockbitTimeOfDayMomentum
            | Self::StockbitQualityDayTradeV1
            | Self::StockbitPullbackDayTradeV1
            | Self::StockbitMomentumDayTradeV1 => super::MIN_STOCKBIT_DAY_TRADE_CAGR_PCT,
            _ => 0.0,
        }
    }

    pub(super) fn minimum_calendar_month_return_pct(self) -> f64 {
        if self == Self::StockbitMultibagger {
            super::MIN_MULTIBAGGER_CALENDAR_MONTH_RETURN_PCT
        } else {
            0.0
        }
    }

    pub(super) fn canonical_id(self) -> &'static str {
        match self {
            Self::StockbitDayTrade => confirmed_evidence_id!("stockbit_day_trade"),
            Self::StockbitSwing => confirmed_evidence_id!("stockbit_swing"),
            Self::StockbitTimeSeriesMomentumSwing => {
                confirmed_evidence_id!("idx_stockbit_time_series_momentum_swing")
            }
            Self::StockbitOpeningRangeMomentum => {
                confirmed_evidence_id!("idx_stockbit_opening_range_momentum")
            }
            Self::StockbitFirstHalfHourMomentum => {
                confirmed_evidence_id!("idx_stockbit_first_half_hour_momentum")
            }
            Self::StockbitTimeOfDayMomentum => {
                confirmed_evidence_id!("idx_stockbit_time_of_day_momentum")
            }
            Self::StockbitQualityDayTradeV1 => {
                confirmed_evidence_id!("idx_stockbit_quality_day_trade_v1")
            }
            Self::StockbitPullbackDayTradeV1 => {
                confirmed_evidence_id!("idx_stockbit_pullback_day_trade_v1")
            }
            Self::StockbitMomentumDayTradeV1 => {
                confirmed_evidence_id!("idx_stockbit_momentum_day_trade_v1")
            }
            Self::StockbitMultibagger => confirmed_evidence_id!("stockbit_multibagger"),
            Self::PintuFuturesDayTrade => confirmed_evidence_id!("pintu_futures_day_trade"),
            Self::PintuFuturesQualityDayTradeV1 => {
                confirmed_evidence_id!("pintu_futures_quality_day_trade_v1")
            }
            Self::PintuFuturesConfluenceQualityDayTradeV1 => {
                confirmed_evidence_id!("pintu_futures_confluence_quality_day_trade_v1")
            }
            Self::PintuGoldenCross => confirmed_evidence_id!(
                "pintu_futures_golden_cross_v5_h1_dss22_recent24h_m15_extreme_turn_mss_3r"
            ),
            Self::TokocryptoSpotDayTrade => confirmed_evidence_id!("tokocrypto_spot_day_trade"),
            Self::TokocryptoSpotConfluence2Of4V1 => {
                confirmed_evidence_id!("tokocrypto_spot_confluence_2_of_4_v1")
            }
            Self::TokocryptoSpotQualityDayTradeV1 => {
                confirmed_evidence_id!("tokocrypto_spot_quality_day_trade_v1")
            }
            Self::TokenizedStockDayTrade => {
                confirmed_evidence_id!("tokenized_stock_day_trade")
            }
            Self::TokenizedStockConfluence2Of4V1 => {
                confirmed_evidence_id!("tokenized_stock_confluence_2_of_4_v1")
            }
            Self::TokenizedStockQualityDayTradeV1 => {
                confirmed_evidence_id!("tokenized_stock_quality_day_trade_v1")
            }
        }
    }

    pub(super) fn generate_signals(self, bars: &[StrategyBar]) -> Vec<StrategySignal> {
        match self {
            Self::StockbitDayTrade | Self::StockbitMultibagger => {
                generate_idx_signals(bars, IdxStrategyConfig::default())
            }
            Self::StockbitSwing => generate_idx_signals(bars, IdxStrategyConfig::swing()),
            Self::StockbitTimeSeriesMomentumSwing => generate_idx_time_series_momentum_signals(
                bars,
                IdxTimeSeriesMomentumConfig::default(),
            ),
            Self::StockbitOpeningRangeMomentum => {
                generate_idx_opening_range_signals(bars, IdxOpeningRangeConfig::default())
            }
            Self::StockbitFirstHalfHourMomentum => generate_idx_first_half_hour_momentum_signals(
                bars,
                IdxIntradayMomentumConfig::default(),
            ),
            Self::StockbitTimeOfDayMomentum => generate_idx_time_of_day_momentum_signals(
                bars,
                IdxIntradayMomentumConfig::default(),
            ),
            Self::StockbitQualityDayTradeV1 => {
                generate_idx_quality_signals(bars, IdxStrategyConfig::quality_day_trade_v1())
            }
            Self::StockbitPullbackDayTradeV1 => {
                generate_idx_quality_signals(bars, IdxStrategyConfig::pullback_day_trade_v1())
            }
            Self::StockbitMomentumDayTradeV1 => {
                generate_idx_quality_signals(bars, IdxStrategyConfig::momentum_day_trade_v1())
            }
            Self::PintuFuturesDayTrade => {
                generate_pintu_signals(bars, PintuStrategyConfig::default())
            }
            Self::PintuFuturesQualityDayTradeV1 => {
                generate_pintu_signals(bars, PintuStrategyConfig::quality_day_trade_v1())
            }
            Self::PintuFuturesConfluenceQualityDayTradeV1 => generate_pintu_confluence_signals(
                bars,
                PintuConfluenceConfig::quality_day_trade_v1(),
            ),
            Self::PintuGoldenCross => {
                generate_pintu_golden_cross_signals(bars, PintuGoldenCrossConfig::default())
            }
            Self::TokocryptoSpotDayTrade | Self::TokenizedStockDayTrade => {
                generate_pintu_confluence_signals(bars, PintuConfluenceConfig::default())
            }
            Self::TokocryptoSpotConfluence2Of4V1 | Self::TokenizedStockConfluence2Of4V1 => {
                generate_tokocrypto_confluence_signals(bars, TokocryptoConfluenceConfig::default())
            }
            Self::TokocryptoSpotQualityDayTradeV1 => generate_pintu_confluence_signals(
                bars,
                PintuConfluenceConfig::quality_day_trade_v1(),
            ),
            Self::TokenizedStockQualityDayTradeV1 => generate_pintu_confluence_signals(
                bars,
                PintuConfluenceConfig::quality_day_trade_v1(),
            ),
        }
    }

    pub(super) fn is_stockbit_intraday(self) -> bool {
        matches!(
            self,
            Self::StockbitDayTrade
                | Self::StockbitOpeningRangeMomentum
                | Self::StockbitFirstHalfHourMomentum
                | Self::StockbitTimeOfDayMomentum
                | Self::StockbitQualityDayTradeV1
                | Self::StockbitPullbackDayTradeV1
                | Self::StockbitMomentumDayTradeV1
                | Self::PintuFuturesQualityDayTradeV1
                | Self::PintuFuturesConfluenceQualityDayTradeV1
                | Self::TokocryptoSpotQualityDayTradeV1
                | Self::TokenizedStockQualityDayTradeV1
        )
    }

    pub(super) fn requires_contiguous_intraday_entry(self) -> bool {
        matches!(
            self,
            Self::StockbitOpeningRangeMomentum
                | Self::StockbitFirstHalfHourMomentum
                | Self::StockbitTimeOfDayMomentum
                | Self::PintuGoldenCross
        )
    }

    pub(super) fn has_tsmom_exit(self) -> bool {
        self == Self::StockbitTimeSeriesMomentumSwing
    }

    pub(super) fn is_day_trade(self) -> bool {
        let canonical_id = self.canonical_id();
        let strategy_id = canonical_id
            .strip_suffix(REQUIRED_SIGNAL_CONFIRMATION_EVIDENCE_SUFFIX)
            .unwrap_or(canonical_id);
        self.is_stockbit_intraday_research()
            || strategy_id.ends_with("_day_trade")
            || strategy_id.contains("confluence")
            || strategy_id.contains("opening_range")
    }

    pub(super) fn is_stockbit_intraday_research(self) -> bool {
        matches!(
            self,
            Self::StockbitOpeningRangeMomentum
                | Self::StockbitFirstHalfHourMomentum
                | Self::StockbitTimeOfDayMomentum
        )
    }

    pub(super) fn historical_profile(self) -> Option<HistoricalEvidenceProfile> {
        let profile = match self {
            Self::StockbitMultibagger => HistoricalEvidenceProfile {
                strategy_id: self.canonical_id(),
                evidence_class: "idx",
                history_interval: STOCKBIT_DAILY_EVIDENCE_TIMEFRAME,
                max_hold_bars: STOCKBIT_MULTIBAGGER_MAX_HOLD_BARS,
                buy_fee_pct: STOCKBIT_BUY_FEE_PCT,
                sell_fee_pct: STOCKBIT_SELL_FEE_PCT,
                fee_model: STOCKBIT_FEE_ASSUMPTION_LABEL,
            },
            Self::StockbitSwing => HistoricalEvidenceProfile {
                strategy_id: self.canonical_id(),
                evidence_class: "idx",
                history_interval: STOCKBIT_DAILY_EVIDENCE_TIMEFRAME,
                max_hold_bars: STOCKBIT_SWING_MAX_HOLD_BARS,
                buy_fee_pct: STOCKBIT_BUY_FEE_PCT,
                sell_fee_pct: STOCKBIT_SELL_FEE_PCT,
                fee_model: STOCKBIT_FEE_ASSUMPTION_LABEL,
            },
            Self::StockbitTimeSeriesMomentumSwing => HistoricalEvidenceProfile {
                strategy_id: self.canonical_id(),
                evidence_class: "idx",
                history_interval: STOCKBIT_DAILY_EVIDENCE_TIMEFRAME,
                max_hold_bars: STOCKBIT_TSMOM_MAX_HOLD_BARS,
                buy_fee_pct: STOCKBIT_BUY_FEE_PCT,
                sell_fee_pct: STOCKBIT_SELL_FEE_PCT,
                fee_model: STOCKBIT_FEE_ASSUMPTION_LABEL,
            },
            Self::StockbitDayTrade
            | Self::StockbitOpeningRangeMomentum
            | Self::StockbitFirstHalfHourMomentum
            | Self::StockbitTimeOfDayMomentum => HistoricalEvidenceProfile {
                strategy_id: self.canonical_id(),
                evidence_class: "idx",
                history_interval: FIFTEEN_MINUTE_EVIDENCE_INTERVAL,
                max_hold_bars: STOCKBIT_DAY_TRADE_MAX_HOLD_BARS,
                buy_fee_pct: STOCKBIT_BUY_FEE_PCT,
                sell_fee_pct: STOCKBIT_SELL_FEE_PCT,
                fee_model: STOCKBIT_FEE_ASSUMPTION_LABEL,
            },
            Self::PintuFuturesDayTrade => HistoricalEvidenceProfile {
                strategy_id: self.canonical_id(),
                evidence_class: "crypto_futures",
                history_interval: FIFTEEN_MINUTE_EVIDENCE_INTERVAL,
                max_hold_bars: PINTU_FUTURES_DAY_TRADE_MAX_HOLD_BARS,
                buy_fee_pct: PINTU_TAKER_FEE_PCT,
                sell_fee_pct: PINTU_TAKER_FEE_PCT,
                fee_model: PINTU_TAKER_FEE_ASSUMPTION_LABEL,
            },
            Self::PintuGoldenCross => HistoricalEvidenceProfile {
                strategy_id: self.canonical_id(),
                evidence_class: "crypto_futures",
                history_interval: PINTU_GOLDEN_CROSS_TIMEFRAME,
                max_hold_bars: PINTU_FUTURES_DAY_TRADE_MAX_HOLD_BARS,
                buy_fee_pct: PINTU_TAKER_FEE_PCT,
                sell_fee_pct: PINTU_TAKER_FEE_PCT,
                fee_model: PINTU_TAKER_FEE_ASSUMPTION_LABEL,
            },
            Self::TokocryptoSpotDayTrade | Self::TokocryptoSpotConfluence2Of4V1 => {
                HistoricalEvidenceProfile {
                    strategy_id: self.canonical_id(),
                    evidence_class: "crypto_spot",
                    history_interval: FIFTEEN_MINUTE_EVIDENCE_INTERVAL,
                    max_hold_bars: TOKOCRYPTO_SPOT_DAY_TRADE_MAX_HOLD_BARS,
                    buy_fee_pct: 0.0,
                    sell_fee_pct: 0.0,
                    fee_model: TOKOCRYPTO_SPOT_UNKNOWN_FEE_LABEL,
                }
            }
            Self::TokenizedStockDayTrade | Self::TokenizedStockConfluence2Of4V1 => {
                HistoricalEvidenceProfile {
                    strategy_id: self.canonical_id(),
                    evidence_class: "crypto_spot",
                    history_interval: FIFTEEN_MINUTE_EVIDENCE_INTERVAL,
                    max_hold_bars: TOKENIZED_STOCK_DAY_TRADE_MAX_HOLD_BARS,
                    buy_fee_pct: 0.0,
                    sell_fee_pct: 0.0,
                    fee_model: TOKOCRYPTO_TOKENIZED_STOCK_UNKNOWN_FEE_LABEL,
                }
            }
            Self::StockbitQualityDayTradeV1
            | Self::StockbitPullbackDayTradeV1
            | Self::StockbitMomentumDayTradeV1
            | Self::PintuFuturesQualityDayTradeV1
            | Self::PintuFuturesConfluenceQualityDayTradeV1
            | Self::TokocryptoSpotQualityDayTradeV1
            | Self::TokenizedStockQualityDayTradeV1 => return None,
        };
        Some(profile)
    }
}

pub(super) fn resolve_evidence_strategy(strategy: &str) -> Option<EvidenceStrategy> {
    let normalized = strategy.trim().to_ascii_lowercase();
    let strategy = normalized
        .strip_suffix(REQUIRED_SIGNAL_CONFIRMATION_EVIDENCE_SUFFIX)
        .unwrap_or(&normalized)
        .to_string();
    if is_pintu_golden_cross_strategy(&strategy) {
        return Some(EvidenceStrategy::PintuGoldenCross);
    }
    Some(match strategy.as_str() {
        "idx_stockbit_day_trade" | "stockbit_day_trade" | "stockbit" | "idx" => {
            EvidenceStrategy::StockbitDayTrade
        }
        "idx_stockbit_swing" | "stockbit_swing" => EvidenceStrategy::StockbitSwing,
        "idx_stockbit_time_series_momentum_swing" | "stockbit_tsmom_swing" => {
            EvidenceStrategy::StockbitTimeSeriesMomentumSwing
        }
        "idx_stockbit_opening_range_momentum" | "stockbit_orb_momentum" => {
            EvidenceStrategy::StockbitOpeningRangeMomentum
        }
        "idx_stockbit_first_half_hour_momentum" | "stockbit_first_half_hour_momentum" => {
            EvidenceStrategy::StockbitFirstHalfHourMomentum
        }
        "idx_stockbit_time_of_day_momentum" | "stockbit_time_of_day_momentum" => {
            EvidenceStrategy::StockbitTimeOfDayMomentum
        }
        "idx_stockbit_quality_day_trade_v1" => EvidenceStrategy::StockbitQualityDayTradeV1,
        "idx_stockbit_pullback_day_trade_v1" => EvidenceStrategy::StockbitPullbackDayTradeV1,
        "idx_stockbit_momentum_day_trade_v1" => EvidenceStrategy::StockbitMomentumDayTradeV1,
        "multibagger" | "stockbit_multibagger" => EvidenceStrategy::StockbitMultibagger,
        "pintu_futures_day_trade" | "pintu" | "pintu_futures" => {
            EvidenceStrategy::PintuFuturesDayTrade
        }
        "pintu_futures_quality_day_trade_v1" => EvidenceStrategy::PintuFuturesQualityDayTradeV1,
        "pintu_futures_confluence_quality_day_trade_v1" => {
            EvidenceStrategy::PintuFuturesConfluenceQualityDayTradeV1
        }
        "tokocrypto_spot_day_trade" => EvidenceStrategy::TokocryptoSpotDayTrade,
        TOKOCRYPTO_SPOT_CONFLUENCE_STRATEGY_ID => EvidenceStrategy::TokocryptoSpotConfluence2Of4V1,
        "tokocrypto_spot" => EvidenceStrategy::TokocryptoSpotConfluence2Of4V1,
        "tokocrypto_spot_quality_day_trade_v1" => EvidenceStrategy::TokocryptoSpotQualityDayTradeV1,
        "tokenized_stock_day_trade" => EvidenceStrategy::TokenizedStockDayTrade,
        TOKENIZED_STOCK_CONFLUENCE_STRATEGY_ID => EvidenceStrategy::TokenizedStockConfluence2Of4V1,
        "tokenized_stock" => EvidenceStrategy::TokenizedStockConfluence2Of4V1,
        "tokenized_stock_quality_day_trade_v1" => EvidenceStrategy::TokenizedStockQualityDayTradeV1,
        _ => return None,
    })
}

/// Return the venue, interval, holding and fee policy for stored evidence.
/// The strategy registry is also the sole owner of the evaluator's supported
/// aliases, so service routing cannot drift from core evaluation.
pub fn historical_evidence_profile(strategy: &str) -> Option<HistoricalEvidenceProfile> {
    resolve_evidence_strategy(strategy)?.historical_profile()
}
