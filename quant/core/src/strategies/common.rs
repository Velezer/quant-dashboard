//! Venue-aware 15-minute day-trade signal generation.
//!
//! These functions intentionally contain no storage or HTTP concerns. They
//! accept a bounded, chronological OHLCV slice and return one deterministic
//! signal per bar so Python and Rust implementations can be compared on the
//! same fixture against the Python reference implementation.

pub(super) use crate::finite;
use crate::venue_rules::STOCKBIT_MIN_CANDIDATE_PRICE_IDR;
use crate::DssConfig;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct StrategyBar {
    pub time: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct PintuSignalTimeframeContext<'a> {
    pub signal_timeframe: &'a str,
    pub signal_interval_ns: i64,
    pub context_timeframe: &'a str,
    pub context_bars: &'a [StrategyBar],
    pub context_interval_ns: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StrategySignal {
    pub direction: String,
    pub strength: f64,
    #[serde(default)]
    pub strategy_strength: f64,
    #[serde(default)]
    pub confluence: Option<SignalConfluenceScore>,
    pub regime: String,
    pub suggested_sl: f64,
    pub suggested_tp1: f64,
    pub suggested_tp2: f64,
    pub exit_signal: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SignalConfluenceScore {
    /// Indicator-confirmation score normalized to the same 0–10 scale as the
    /// owning strategy strength. This ranks signals; it is not a win probability.
    pub score: f64,
    pub nadaraya_state: String,
    pub dss_state: String,
    pub dss_extreme_zone: bool,
    #[serde(default)]
    pub dss_points: f64,
    #[serde(default)]
    pub dss_extreme_points: f64,
    #[serde(default)]
    pub nadaraya_touch_points: f64,
    #[serde(default)]
    pub nadaraya_near_line_points: f64,
    #[serde(default)]
    pub nadaraya_reclaim_points: f64,
}

impl Default for StrategySignal {
    fn default() -> Self {
        Self {
            direction: "NEUTRAL".to_string(),
            strength: 0.0,
            strategy_strength: 0.0,
            confluence: None,
            regime: "NONE".to_string(),
            suggested_sl: 0.0,
            suggested_tp1: 0.0,
            suggested_tp2: 0.0,
            exit_signal: None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PintuStrategyConfig {
    pub min_rvol: f64,
    pub tp1_underlying_pct: f64,
    pub tp2_underlying_pct: f64,
    pub sl_underlying_pct: f64,
    pub volume_ma_period: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct PintuConfluenceConfig {
    pub volume_ratio: f64,
    pub tp_pct: f64,
    pub sl_pct: f64,
}

/// Versioned behavior parameters for the registered Pintu golden-cross model.
/// Update the strategy identity and its profile copy when changing defaults.
#[derive(Debug, Clone, Copy)]
pub struct PintuGoldenCrossConfig {
    pub fast_ema_period: usize,
    pub slow_ema_period: usize,
    pub signal_interval_ns: i64,
    pub context_interval_ns: i64,
    pub context_dss_oversold: f64,
    /// Number of contiguous, completed context bars searched for the oversold print.
    pub context_dss_oversold_lookback_bars: usize,
    pub max_signal_dss_extreme: f64,
    pub dss_extreme_max_age_bars: usize,
    pub structure_swing_width: usize,
    pub structure_lookback: usize,
    pub structure_max_age_bars: usize,
    pub target_r_multiple: f64,
    pub stop_loss_pct: f64,
    pub signal_strength: f64,
}

pub const PINTU_GOLDEN_CROSS_STRATEGY_ID: &str =
    "pintu_futures_golden_cross_v5_h1_dss22_recent24h_m15_extreme_turn_mss_3r";
pub const PINTU_GOLDEN_CROSS_SIGNAL_STRATEGY_ID: &str =
    "ema_20_50_golden_cross_h1_dss22_recent24h_m15_extreme_turn_mss_3r_long";
pub const PINTU_GOLDEN_CROSS_V2_EVIDENCE_ID: &str = "pintu_futures_golden_cross_v2_3r";
pub const PINTU_GOLDEN_CROSS_MIN_R_MULTIPLE: f64 = 3.0;
pub const PINTU_GOLDEN_CROSS_TIMEFRAME: &str = "15m";
pub const PINTU_GOLDEN_CROSS_CONTEXT_TIMEFRAME: &str = "1h";
pub const PINTU_GOLDEN_CROSS_INTERVAL_NS: i64 =
    15 * crate::SECONDS_PER_MINUTE * crate::NANOS_PER_SECOND;
/// Default trailing-bar window for Stockbit IDX relative-volume filters.
pub(super) const IDX_VOLUME_LOOKBACK_BARS: usize = 20;
pub const PINTU_GOLDEN_CROSS_MINIMUM_PLAN_BARS: usize = 60;
pub const PINTU_GOLDEN_CROSS_LEGACY_STRATEGY_IDS: &[&str] = &[
    "pintu_futures_confluence",
    "pintu_confluence",
    "pintu_futures_golden_cross_v1",
    PINTU_GOLDEN_CROSS_V2_EVIDENCE_ID,
    "pintu_futures_golden_cross_v3_mss_3r",
    "pintu_futures_golden_cross_v4_mss_dss5_3r",
    "pintu_futures_golden_cross_v5_h1_dss22_m15_extreme_turn_mss_3r",
];
pub const PINTU_GOLDEN_CROSS_LEGACY_SIGNAL_STRATEGY_IDS: &[&str] = &[
    "ema_20_50_golden_cross_long",
    "ema_20_50_golden_cross_3r_long",
    "ema_20_50_golden_cross_mss_3r_long",
    "ema_20_50_golden_cross_mss_dss5_3r_long",
    "ema_20_50_golden_cross_h1_dss22_m15_extreme_turn_mss_3r_long",
];

/// Recognize only the active, evidence-compatible golden-cross strategy version.
pub fn is_pintu_golden_cross_strategy(strategy_id: &str) -> bool {
    strategy_id
        .trim()
        .eq_ignore_ascii_case(PINTU_GOLDEN_CROSS_STRATEGY_ID)
}

impl Default for PintuGoldenCrossConfig {
    fn default() -> Self {
        Self {
            fast_ema_period: 20,
            slow_ema_period: 50,
            signal_interval_ns: PINTU_GOLDEN_CROSS_INTERVAL_NS,
            context_interval_ns: 4 * PINTU_GOLDEN_CROSS_INTERVAL_NS,
            context_dss_oversold: DssConfig::default().oversold,
            context_dss_oversold_lookback_bars: 24,
            max_signal_dss_extreme: 5.0,
            dss_extreme_max_age_bars: 3,
            structure_swing_width: 2,
            structure_lookback: 80,
            structure_max_age_bars: 3,
            target_r_multiple: PINTU_GOLDEN_CROSS_MIN_R_MULTIPLE,
            stop_loss_pct: 0.01,
            signal_strength: 9.0,
        }
    }
}

impl Default for PintuConfluenceConfig {
    fn default() -> Self {
        Self {
            volume_ratio: 1.8,
            tp_pct: 0.014,
            sl_pct: 0.009,
        }
    }
}

impl PintuConfluenceConfig {
    /// Versioned higher-confirmation confluence settings used by the research
    /// candidate. The published strategy remains unchanged.
    pub fn quality_day_trade_v1() -> Self {
        Self {
            volume_ratio: 2.2,
            tp_pct: 0.012,
            sl_pct: 0.008,
        }
    }
}

impl Default for PintuStrategyConfig {
    fn default() -> Self {
        Self {
            min_rvol: 1.5,
            tp1_underlying_pct: 0.018,
            tp2_underlying_pct: 0.035,
            sl_underlying_pct: 0.012,
            volume_ma_period: IDX_VOLUME_LOOKBACK_BARS,
        }
    }
}

impl PintuStrategyConfig {
    /// Versioned higher-volume, tighter-risk settings for the research
    /// candidate. The published strategy remains unchanged.
    pub fn quality_day_trade_v1() -> Self {
        Self {
            min_rvol: 2.2,
            tp1_underlying_pct: 0.014,
            tp2_underlying_pct: 0.028,
            sl_underlying_pct: 0.008,
            volume_ma_period: 24,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct IdxStrategyConfig {
    pub open_equal_low_tolerance: f64,
    pub min_rvol: f64,
    pub min_turnover_idr: f64,
    pub min_price: f64,
    pub min_ticks_to_ara: i64,
    pub tp1_pct: f64,
    pub tp2_pct: f64,
    pub sl_pct: f64,
    pub volume_ma_period: usize,
}

impl Default for IdxStrategyConfig {
    fn default() -> Self {
        Self {
            open_equal_low_tolerance: 0.005,
            min_rvol: 1.5,
            min_turnover_idr: 500_000_000.0,
            min_price: STOCKBIT_MIN_CANDIDATE_PRICE_IDR,
            min_ticks_to_ara: 2,
            tp1_pct: 0.018,
            tp2_pct: 0.035,
            sl_pct: 0.015,
            volume_ma_period: IDX_VOLUME_LOOKBACK_BARS,
        }
    }
}

impl IdxStrategyConfig {
    /// Daily Stockbit swing settings. The signal rules stay venue-specific,
    /// while the wider levels and lower intraday volume threshold fit a
    /// multi-day holding window.
    pub fn swing() -> Self {
        Self {
            open_equal_low_tolerance: 0.008,
            min_rvol: 1.2,
            min_turnover_idr: 250_000_000.0,
            min_price: STOCKBIT_MIN_CANDIDATE_PRICE_IDR,
            min_ticks_to_ara: 1,
            tp1_pct: 0.05,
            tp2_pct: 0.10,
            sl_pct: 0.04,
            volume_ma_period: IDX_VOLUME_LOOKBACK_BARS,
        }
    }

    /// Versioned IDX quality breakout settings used by the research
    /// candidate. Each variant keeps its own strategy identity and defaults.
    pub fn quality_day_trade_v1() -> Self {
        Self {
            open_equal_low_tolerance: 0.003,
            min_rvol: 2.0,
            min_turnover_idr: 500_000_000.0,
            min_price: STOCKBIT_MIN_CANDIDATE_PRICE_IDR,
            min_ticks_to_ara: 2,
            tp1_pct: 0.014,
            tp2_pct: 0.028,
            sl_pct: 0.010,
            volume_ma_period: IDX_VOLUME_LOOKBACK_BARS,
        }
    }

    /// Versioned IDX pullback reclaim settings used by the research
    /// candidate. Each variant keeps its own strategy identity and defaults.
    pub fn pullback_day_trade_v1() -> Self {
        Self {
            open_equal_low_tolerance: 0.006,
            min_rvol: 1.7,
            min_turnover_idr: 500_000_000.0,
            min_price: STOCKBIT_MIN_CANDIDATE_PRICE_IDR,
            min_ticks_to_ara: 1,
            tp1_pct: 0.012,
            tp2_pct: 0.024,
            sl_pct: 0.009,
            volume_ma_period: IDX_VOLUME_LOOKBACK_BARS,
        }
    }

    /// Versioned IDX momentum breakout settings used by the research
    /// candidate. Each variant keeps its own strategy identity and defaults.
    pub fn momentum_day_trade_v1() -> Self {
        Self {
            open_equal_low_tolerance: 0.002,
            min_rvol: 2.5,
            min_turnover_idr: 500_000_000.0,
            min_price: STOCKBIT_MIN_CANDIDATE_PRICE_IDR,
            min_ticks_to_ara: 3,
            tp1_pct: 0.018,
            tp2_pct: 0.032,
            sl_pct: 0.012,
            volume_ma_period: IDX_VOLUME_LOOKBACK_BARS,
        }
    }
}

/// Daily Stockbit time-series momentum research settings. The signal uses the
/// return from 12 months ago through one month ago, requires price above its
/// 200-session average, and sizes protective levels from recent true range.
#[derive(Debug, Clone, Copy)]
pub struct IdxTimeSeriesMomentumConfig {
    pub lookback_bars: usize,
    pub skip_recent_bars: usize,
    pub trend_period: usize,
    pub atr_period: usize,
    pub stop_atr_multiple: f64,
    pub target_atr_multiple: f64,
}

/// Research-only IDX opening-range momentum settings. The first two 15m bars
/// establish the 09:00–09:30 WIB opening range. Signals require a positive
/// first-half-hour return, a close above that range, relative volume and
/// turnover confirmation. The candidate is long-only and exits by the end of
/// the morning session; it never models a short sale or an overnight hold.
#[derive(Debug, Clone, Copy)]
pub struct IdxOpeningRangeConfig {
    pub opening_range_bars: usize,
    pub min_rvol: f64,
    pub min_turnover_idr: f64,
    pub min_price: f64,
    pub min_ticks_to_ara: i64,
    pub max_stop_pct: f64,
    pub tp1_r_multiple: f64,
    pub tp2_r_multiple: f64,
    pub volume_lookback: usize,
}

impl Default for IdxOpeningRangeConfig {
    fn default() -> Self {
        Self {
            opening_range_bars: 2,
            min_rvol: 1.5,
            min_turnover_idr: 500_000_000.0,
            min_price: STOCKBIT_MIN_CANDIDATE_PRICE_IDR,
            min_ticks_to_ara: 2,
            max_stop_pct: 0.035,
            tp1_r_multiple: 1.5,
            tp2_r_multiple: 2.5,
            volume_lookback: IDX_VOLUME_LOOKBACK_BARS,
        }
    }
}

/// Shared execution filters for additional research-derived intraday hypotheses.
/// Each model keeps its own signal rule while reusing Stockbit's turnover,
/// relative-volume, price-limit, stop-risk, and exit constraints.
#[derive(Debug, Clone, Copy)]
pub struct IdxIntradayMomentumConfig {
    pub min_rvol: f64,
    pub min_turnover_idr: f64,
    pub min_price: f64,
    pub min_ticks_to_ara: i64,
    pub max_stop_pct: f64,
    pub tp1_r_multiple: f64,
    pub tp2_r_multiple: f64,
    pub volume_lookback: usize,
    pub time_of_day_lookback_sessions: usize,
    pub minimum_time_of_day_observations: usize,
}

impl Default for IdxIntradayMomentumConfig {
    fn default() -> Self {
        Self {
            min_rvol: 1.5,
            min_turnover_idr: 500_000_000.0,
            min_price: STOCKBIT_MIN_CANDIDATE_PRICE_IDR,
            min_ticks_to_ara: 2,
            max_stop_pct: 0.035,
            tp1_r_multiple: 1.5,
            tp2_r_multiple: 2.5,
            volume_lookback: IDX_VOLUME_LOOKBACK_BARS,
            time_of_day_lookback_sessions: 20,
            minimum_time_of_day_observations: 5,
        }
    }
}

impl Default for IdxTimeSeriesMomentumConfig {
    fn default() -> Self {
        Self {
            lookback_bars: 252,
            skip_recent_bars: 21,
            trend_period: 200,
            atr_period: 20,
            stop_atr_multiple: 2.0,
            target_atr_multiple: 4.0,
        }
    }
}

pub(super) fn rolling_mean(values: &[f64], index: usize, period: usize) -> f64 {
    // pandas rolling(window=period, min_periods=5) uses the available
    // prefix once five bars exist; it does not wait for a full window.
    if period == 0 || index + 1 < 5 {
        return f64::NAN;
    }
    let count = period.min(index + 1);
    let start = index + 1 - count;
    let window = &values[start..=index];
    if !window.iter().all(|value| finite(*value)) {
        return f64::NAN;
    }
    window.iter().sum::<f64>() / count as f64
}

/// Match the Python `core.utils.round_price` display/risk rounding tiers.
pub fn round_price(value: f64) -> f64 {
    if !finite(value) {
        return 0.0;
    }
    let magnitude = value.abs();
    if magnitude >= 100.0 {
        crate::round_decimal(value, 2)
    } else if magnitude >= 1.0 {
        crate::round_decimal(value, 3)
    } else if magnitude >= 0.01 {
        crate::round_decimal(value, 4)
    } else if magnitude >= 0.0001 {
        crate::round_decimal(value, 6)
    } else if magnitude >= 0.000001 {
        crate::round_decimal(value, 8)
    } else {
        crate::round_decimal(value, 10)
    }
}

/// Round a target upward using the same display/risk precision tiers as
/// `round_price`, so a minimum reward multiple is not rounded below its floor.
pub fn round_price_up(value: f64) -> f64 {
    if !finite(value) {
        return 0.0;
    }
    let magnitude = value.abs();
    let decimals = if magnitude >= 100.0 {
        2
    } else if magnitude >= 1.0 {
        3
    } else if magnitude >= 0.01 {
        4
    } else if magnitude >= 0.0001 {
        6
    } else if magnitude >= 0.000001 {
        8
    } else {
        10
    };
    let scale = 10f64.powi(decimals);
    (value * scale).ceil() / scale
}

pub(crate) fn idx_tick_size(price: f64) -> i64 {
    if !finite(price) || price <= 0.0 {
        return 1;
    }
    if price < 200.0 {
        1
    } else if price < 500.0 {
        2
    } else if price < 2_000.0 {
        5
    } else if price < 5_000.0 {
        10
    } else {
        25
    }
}

pub(crate) fn idx_min_price_at(timestamp_ns: i64) -> f64 {
    // IDX Decree Kep-00136/BEI/09-2026 lowers the minimum regular-market
    // equity price to Rp1 effective at 00:00 WIB on 28 September 2026.
    if timestamp_ns >= 1_790_614_800_i64.saturating_mul(crate::NANOS_PER_SECOND) {
        1.0
    } else {
        50.0
    }
}

#[derive(Clone, Copy)]
pub(crate) enum IdxTickRounding {
    Floor,
    Ceil,
    Nearest,
}

pub(crate) fn round_idx_tick_at(price: f64, rounding: IdxTickRounding, timestamp_ns: i64) -> f64 {
    if !finite(price) || price <= 0.0 {
        return idx_min_price_at(timestamp_ns);
    }
    let tick = idx_tick_size(price) as f64;
    let units = match rounding {
        IdxTickRounding::Floor => (price / tick).floor(),
        IdxTickRounding::Ceil => (price / tick).ceil(),
        IdxTickRounding::Nearest => (price / tick).round(),
    };
    (units * tick).max(idx_min_price_at(timestamp_ns))
}

pub(super) fn idx_ara_arb(reference: f64, timestamp_ns: i64) -> (f64, f64, f64) {
    if !finite(reference) || reference <= 0.0 {
        let floor = idx_min_price_at(timestamp_ns);
        return (floor, floor, 1.0);
    }
    let price_floor = idx_min_price_at(timestamp_ns);
    let new_low_price_rule = timestamp_ns
        >= 1_790_614_800_i64.saturating_mul(crate::NANOS_PER_SECOND)
        && reference <= 10.0;
    let ara_pct = if reference <= 200.0 {
        0.35
    } else if reference <= 5_000.0 {
        0.25
    } else {
        0.20
    };
    let arb_pct = if timestamp_ns >= 1_798_736_400_i64.saturating_mul(crate::NANOS_PER_SECOND) {
        // The next IDX phase makes the percentage ARB symmetric from
        // 1 January 2027; Rp1–Rp10 remains a fixed Rp1 move.
        ara_pct
    } else {
        0.15
    };
    let (ara_raw, arb_raw) = if new_low_price_rule {
        (reference + 1.0, reference - 1.0)
    } else {
        (reference * (1.0 + ara_pct), reference * (1.0 - arb_pct))
    };
    let ara = round_idx_tick_at(ara_raw, IdxTickRounding::Floor, timestamp_ns);
    let arb = round_idx_tick_at(arb_raw, IdxTickRounding::Ceil, timestamp_ns).max(price_floor);
    (ara, arb, idx_tick_size(reference) as f64)
}

pub(super) fn local_day_seconds(timestamp_ns: i64) -> i64 {
    timestamp_ns
        .div_euclid(crate::NANOS_PER_SECOND)
        .saturating_add(crate::JAKARTA_UTC_OFFSET_SECONDS)
}

pub(super) fn idx_local_day(timestamp_ns: i64) -> i64 {
    local_day_seconds(timestamp_ns).div_euclid(crate::SECONDS_PER_DAY)
}

pub(super) fn idx_local_seconds_in_day(timestamp_ns: i64) -> i64 {
    local_day_seconds(timestamp_ns).rem_euclid(crate::SECONDS_PER_DAY)
}

pub(super) fn idx_weekday(timestamp_ns: i64) -> i64 {
    // Monday = 0; Unix day zero (1970-01-01) was Thursday.
    (idx_local_day(timestamp_ns) + 3).rem_euclid(7)
}

pub(super) fn idx_morning_session_end(timestamp_ns: i64) -> Option<i64> {
    match idx_weekday(timestamp_ns) {
        0..=3 => Some(12 * crate::SECONDS_PER_HOUR),
        4 => Some(11 * crate::SECONDS_PER_HOUR + 30 * crate::SECONDS_PER_MINUTE),
        _ => None,
    }
}

pub(super) fn idx_regular_morning_bar(timestamp_ns: i64) -> bool {
    let seconds = idx_local_seconds_in_day(timestamp_ns);
    idx_morning_session_end(timestamp_ns)
        .is_some_and(|end| seconds >= 9 * crate::SECONDS_PER_HOUR && seconds < end)
}

pub(super) fn idx_orb_signal_window(timestamp_ns: i64) -> bool {
    let seconds = idx_local_seconds_in_day(timestamp_ns);
    idx_morning_session_end(timestamp_ns).is_some_and(|end| {
        // Allow the signal bar to close, the next bar to open, and a final
        // 15m bar to resolve before the morning session ends.
        seconds >= 9 * crate::SECONDS_PER_HOUR + 30 * crate::SECONDS_PER_MINUTE
            && seconds + 30 * crate::SECONDS_PER_MINUTE <= end
    })
}

pub(super) fn idx_reference_closes(bars: &[StrategyBar]) -> Vec<f64> {
    let mut references = vec![f64::NAN; bars.len()];
    let mut previous_day_close = f64::NAN;
    let mut start = 0usize;
    while start < bars.len() {
        let day = idx_local_day(bars[start].time);
        let mut end = start + 1;
        while end < bars.len() && idx_local_day(bars[end].time) == day {
            end += 1;
        }
        let fallback = start.checked_sub(1).map(|index| bars[index].close);
        let reference = if finite(previous_day_close) && previous_day_close > 0.0 {
            previous_day_close
        } else {
            fallback
                .filter(|value| finite(*value) && *value > 0.0)
                .unwrap_or(f64::NAN)
        };
        references[start..end].fill(reference);
        previous_day_close = bars[end - 1].close;
        start = end;
    }
    references
}

pub(super) fn stockbit_cutoff_due(timestamp_ns: i64) -> bool {
    // With start-stamped 15m candles, the 15:30 candle closes at about 15:45.
    // Emit the forced-exit marker there, ahead of the IDX 15:49:59 continuous
    // session close and before the pre-closing auction begins at 15:50.
    local_day_seconds(timestamp_ns).rem_euclid(crate::SECONDS_PER_DAY)
        >= 15 * crate::SECONDS_PER_HOUR + 30 * crate::SECONDS_PER_MINUTE
}

pub(super) fn stockbit_session_exit_due(bars: &[StrategyBar], index: usize) -> bool {
    if index + 1 >= bars.len() {
        return false;
    }
    let current = bars[index].time;
    let next = bars[index + 1].time;
    let current_day = idx_local_day(current);
    let next_day = idx_local_day(next);
    if next_day != current_day {
        return true;
    }
    let current_seconds = idx_local_seconds_in_day(current);
    let next_seconds = idx_local_seconds_in_day(next);
    idx_morning_session_end(current).is_some_and(|end| current_seconds < end && next_seconds >= end)
}
