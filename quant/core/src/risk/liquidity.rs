//! Deterministic liquidity-zone discovery and trade liquidity audit.
//!
//! This is the Rust counterpart of the legacy Python liquidity calculator.
//! It intentionally accepts a bounded, chronological OHLCV slice and keeps
//! the field names used by the dashboard stable.  The function does not
//! use order-book data or fabricate venue liquidity; liquidation shelves and
//! FVGs are explicitly estimates derived from the supplied candles.

use serde::{Deserialize, Serialize};

use crate::{round_decimal, strategies::StrategyBar};

use super::tail_risk::{cascade_probability_for_count, compute_tail_risk, LiquidityZoneInput};
use super::MIN_RELATIVE_DISTANCE_PRICE_DENOMINATOR;

const ZONE_BSL: &str = "BUY_SIDE_LIQUIDITY";
const ZONE_SSL: &str = "SELL_SIDE_LIQUIDITY";
const ZONE_LIQ: &str = "LIQUIDATION_CLUSTER";
const ZONE_FVG: &str = "FAIR_VALUE_GAP";
const STOP_LOSS_POOL_PROXIMITY_RATIO: f64 = 0.008;
const RECOMMENDED_STOP_LOSS_BUFFER_RATIO: f64 = 0.008;
const TAKE_PROFIT_POOL_PROXIMITY_RATIO: f64 = 0.03;
const LIQUIDITY_ATR_PERIOD_BARS: usize = 14;
const LIQUIDITY_ATR_FALLBACK_RATIO: f64 = 0.02;
const MINIMUM_LIQUIDITY_ATR_RATIO: f64 = 0.005;
const PIVOT_CLUSTER_MAX_DISTANCE_ATR: f64 = 0.6;
const SWING_ZONE_PADDING_ATR: f64 = 0.5;
const LIQUIDATION_SHELF_HALF_WIDTH_ATR: f64 = 0.25;
const FAIR_VALUE_GAP_MINIMUM_WIDTH_ATR: f64 = 0.3;
const EQUAL_SWING_BASE_DENSITY_SCORE: i32 = 85;
const SINGLE_SWING_BASE_DENSITY_SCORE: i32 = 65;
const UNSWEPT_DENSITY_SCORE_BONUS: i32 = 10;
const SWEPT_DENSITY_SCORE_PENALTY: i32 = 25;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LiquidityZone {
    pub zone_type: String,
    pub price_low: f64,
    pub price_high: f64,
    pub mid_price: f64,
    pub distance_pct: f64,
    pub density_score: u32,
    pub source_pivot: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub leverage_tier: Option<f64>,
    pub tail_risk_score: u32,
    pub tail_risk_label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LiquidityRiskReport {
    pub stop_loss_safety: String,
    pub stop_hunt_hazard: String,
    pub nearest_bsl: Option<LiquidityZone>,
    pub nearest_ssl: Option<LiquidityZone>,
    pub nearest_liq_cluster: Option<LiquidityZone>,
    pub recommended_safe_sl: Option<f64>,
    pub tp_magnet_score: f64,
    pub liquidity_magnet_target: Option<String>,
    pub audit_summary: String,
    pub max_tail_risk_score: u32,
    pub max_tail_risk_label: String,
    pub cascade_probability: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LiquidityRiskResult {
    pub zones: Vec<LiquidityZone>,
    pub report: LiquidityRiskReport,
    pub current_price: f64,
    pub lookback_bars: usize,
}

#[allow(clippy::too_many_arguments)]
fn zone(
    zone_type: &str,
    price_low: f64,
    price_high: f64,
    mid_price: f64,
    current_price: f64,
    density_score: u32,
    source_pivot: String,
    status: &str,
    leverage_tier: Option<f64>,
) -> LiquidityZone {
    LiquidityZone {
        zone_type: zone_type.to_string(),
        price_low: round_decimal(price_low, 4),
        price_high: round_decimal(price_high, 4),
        mid_price: round_decimal(mid_price, 4),
        distance_pct: round_decimal(
            (mid_price - current_price) / current_price.max(1e-12) * 100.0,
            2,
        ),
        density_score,
        source_pivot,
        status: status.to_string(),
        leverage_tier,
        tail_risk_score: 0,
        tail_risk_label: "LOW".to_string(),
    }
}

fn swing_zone_density_score(equal_pivots: bool, status: &str) -> u32 {
    let base_score = if equal_pivots {
        EQUAL_SWING_BASE_DENSITY_SCORE
    } else {
        SINGLE_SWING_BASE_DENSITY_SCORE
    };
    let status_adjustment = match status {
        "UNSWEPT" => UNSWEPT_DENSITY_SCORE_BONUS,
        "SWEPT" => -SWEPT_DENSITY_SCORE_PENALTY,
        _ => 0,
    };
    (base_score + status_adjustment).clamp(10, 100) as u32
}

fn attach_tail_risk(zones: &mut [LiquidityZone], current_price: f64) -> (u32, String, String) {
    let inputs = zones
        .iter()
        .map(|z| LiquidityZoneInput {
            mid_price: z.mid_price,
            distance_pct: Some(z.distance_pct),
            density_score: z.density_score as f64,
            is_liquidation_cluster: z.zone_type == ZONE_LIQ,
            is_unswept: z.status == "UNSWEPT",
            is_partially_swept: z.status == "PARTIALLY_SWEPT",
        })
        .collect::<Vec<_>>();
    let report = compute_tail_risk(&inputs, current_price);
    for score in &report.zone_scores {
        if let Some(zone) = zones.get_mut(score.zone_index) {
            zone.tail_risk_score = score.tail_risk_score;
            zone.tail_risk_label = score.tail_risk_label.clone();
        }
    }
    (
        report.max_tail_risk_score,
        report.max_tail_risk_label,
        report.cascade_probability,
    )
}

/// Discover potential liquidity zones from a bounded candle slice.
pub fn discover_liquidity_zones(
    bars: &[StrategyBar],
    current_price: Option<f64>,
    asset_class: &str,
    fractal_window: usize,
    lookback_bars: usize,
    include_bsl_ssl: bool,
) -> Vec<LiquidityZone> {
    if bars.len() < 15 {
        return Vec::new();
    }
    let start = bars.len().saturating_sub(lookback_bars.max(1));
    let sub = &bars[start..];
    let n = sub.len();
    let curr = current_price
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or(sub[n - 1].close);
    if !curr.is_finite() || curr <= 0.0 {
        return Vec::new();
    }

    let tr = (1..n)
        .map(|i| {
            let tr1 = sub[i].high - sub[i].low;
            let tr2 = (sub[i].high - sub[i - 1].close).abs();
            let tr3 = (sub[i].low - sub[i - 1].close).abs();
            tr1.max(tr2).max(tr3)
        })
        .collect::<Vec<_>>();
    let atr = if tr.len() >= LIQUIDITY_ATR_PERIOD_BARS {
        let tail = &tr[tr.len() - LIQUIDITY_ATR_PERIOD_BARS..];
        tail.iter().sum::<f64>() / tail.len() as f64
    } else {
        curr * LIQUIDITY_ATR_FALLBACK_RATIO
    }
    .max(curr * MINIMUM_LIQUIDITY_ATR_RATIO);

    let highs = sub.iter().map(|bar| bar.high).collect::<Vec<_>>();
    let lows = sub.iter().map(|bar| bar.low).collect::<Vec<_>>();
    let mut zones = Vec::<LiquidityZone>::new();

    if include_bsl_ssl {
        let w = fractal_window.max(1);
        let mut swing_highs = Vec::<(usize, f64)>::new();
        let mut swing_lows = Vec::<(usize, f64)>::new();
        if n > w * 2 {
            for i in w..(n - w) {
                let high = highs[i];
                let low = lows[i];
                let high_ok = (1..=w).all(|k| high >= highs[i - k] && high >= highs[i + k]);
                let low_ok = (1..=w).all(|k| low <= lows[i - k] && low <= lows[i + k]);
                if high_ok {
                    swing_highs.push((i, high));
                }
                if low_ok {
                    swing_lows.push((i, low));
                }
            }
        }

        let mut used = vec![false; swing_highs.len()];
        for i in 0..swing_highs.len() {
            if used[i] {
                continue;
            }
            let (bar_idx, pivot) = swing_highs[i];
            let mut cluster = vec![pivot];
            used[i] = true;
            for j in (i + 1)..swing_highs.len() {
                if !used[j]
                    && (swing_highs[j].1 - pivot).abs() <= PIVOT_CLUSTER_MAX_DISTANCE_ATR * atr
                {
                    cluster.push(swing_highs[j].1);
                    used[j] = true;
                }
            }
            let equal = cluster.len() >= 2;
            let mean = cluster.iter().sum::<f64>() / cluster.len() as f64;
            let low = cluster.iter().copied().fold(f64::INFINITY, f64::min);
            let high = cluster.iter().copied().fold(f64::NEG_INFINITY, f64::max)
                + SWING_ZONE_PADDING_ATR * atr;
            let max_after = highs
                .get((bar_idx + 1)..)
                .and_then(|slice| slice.iter().copied().reduce(f64::max))
                .unwrap_or(pivot);
            let status = if curr > high {
                "SWEPT"
            } else if max_after > low {
                "PARTIALLY_SWEPT"
            } else {
                "UNSWEPT"
            };
            zones.push(zone(
                ZONE_BSL,
                low,
                high,
                (low + high) / 2.0,
                curr,
                swing_zone_density_score(equal, status),
                if equal {
                    format!("Equal Highs ({}x) at {:.2}", cluster.len(), mean)
                } else {
                    format!("Swing High at {:.2}", mean)
                },
                status,
                None,
            ));
        }

        let mut used = vec![false; swing_lows.len()];
        for i in 0..swing_lows.len() {
            if used[i] {
                continue;
            }
            let (bar_idx, pivot) = swing_lows[i];
            let mut cluster = vec![pivot];
            used[i] = true;
            for j in (i + 1)..swing_lows.len() {
                if !used[j]
                    && (swing_lows[j].1 - pivot).abs() <= PIVOT_CLUSTER_MAX_DISTANCE_ATR * atr
                {
                    cluster.push(swing_lows[j].1);
                    used[j] = true;
                }
            }
            let equal = cluster.len() >= 2;
            let mean = cluster.iter().sum::<f64>() / cluster.len() as f64;
            let high = cluster.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            let low = cluster.iter().copied().fold(f64::INFINITY, f64::min)
                - SWING_ZONE_PADDING_ATR * atr;
            let min_after = lows
                .get((bar_idx + 1)..)
                .and_then(|slice| slice.iter().copied().reduce(f64::min))
                .unwrap_or(pivot);
            let status = if curr < low {
                "SWEPT"
            } else if min_after < high {
                "PARTIALLY_SWEPT"
            } else {
                "UNSWEPT"
            };
            zones.push(zone(
                ZONE_SSL,
                low,
                high,
                (low + high) / 2.0,
                curr,
                swing_zone_density_score(equal, status),
                if equal {
                    format!("Equal Lows ({}x) at {:.2}", cluster.len(), mean)
                } else {
                    format!("Swing Low at {:.2}", mean)
                },
                status,
                None,
            ));
        }
    }

    // Preserve the Python behavior: positive-price assets receive estimated
    // liquidation shelves, while no exchange mark/funding data is assumed.
    let is_crypto = asset_class.to_ascii_lowercase().contains("crypto") || curr > 0.0;
    if is_crypto && n >= 15 {
        let recent_start = n.saturating_sub(40);
        let recent_high = highs[recent_start..]
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        let recent_low = lows[recent_start..]
            .iter()
            .copied()
            .fold(f64::INFINITY, f64::min);
        for leverage in [10.0_f64, 25.0_f64] {
            let long_liq = recent_low * (1.0 - 1.0 / leverage + 0.005);
            if long_liq > 0.0 {
                zones.push(zone(
                    ZONE_LIQ,
                    long_liq - LIQUIDATION_SHELF_HALF_WIDTH_ATR * atr,
                    long_liq + LIQUIDATION_SHELF_HALF_WIDTH_ATR * atr,
                    long_liq,
                    curr,
                    if leverage == 25.0 { 90 } else { 75 },
                    format!("Estimated {:.0}x Long Liquidation Shelf", leverage),
                    if curr > long_liq { "UNSWEPT" } else { "SWEPT" },
                    Some(leverage),
                ));
            }
            let short_liq = recent_high * (1.0 + 1.0 / leverage - 0.005);
            zones.push(zone(
                ZONE_LIQ,
                short_liq - LIQUIDATION_SHELF_HALF_WIDTH_ATR * atr,
                short_liq + LIQUIDATION_SHELF_HALF_WIDTH_ATR * atr,
                short_liq,
                curr,
                if leverage == 25.0 { 90 } else { 75 },
                format!("Estimated {:.0}x Short Liquidation Shelf", leverage),
                if curr < short_liq { "UNSWEPT" } else { "SWEPT" },
                Some(leverage),
            ));
        }
    }

    // Three-bar displacement gaps.  Only unfilled gaps above the minimum ATR
    // width are retained, matching the Python scanner.
    for i in 2..n {
        if lows[i] > highs[i - 2] {
            let low = highs[i - 2];
            let high = lows[i];
            if high - low >= FAIR_VALUE_GAP_MINIMUM_WIDTH_ATR * atr {
                let filled = lows
                    .get((i + 1)..)
                    .map(|slice| slice.iter().copied().fold(f64::INFINITY, f64::min) <= low)
                    .unwrap_or(false);
                if !filled {
                    zones.push(zone(
                        ZONE_FVG,
                        low,
                        high,
                        (low + high) / 2.0,
                        curr,
                        70,
                        "Bullish FVG Imbalance".to_string(),
                        "UNSWEPT",
                        None,
                    ));
                }
            }
        } else if highs[i] < lows[i - 2] {
            let low = highs[i];
            let high = lows[i - 2];
            if high - low >= FAIR_VALUE_GAP_MINIMUM_WIDTH_ATR * atr {
                let filled = highs
                    .get((i + 1)..)
                    .map(|slice| slice.iter().copied().fold(f64::NEG_INFINITY, f64::max) >= high)
                    .unwrap_or(false);
                if !filled {
                    zones.push(zone(
                        ZONE_FVG,
                        low,
                        high,
                        (low + high) / 2.0,
                        curr,
                        70,
                        "Bearish FVG Imbalance".to_string(),
                        "UNSWEPT",
                        None,
                    ));
                }
            }
        }
    }

    zones.sort_by(|a, b| {
        a.distance_pct
            .abs()
            .partial_cmp(&b.distance_pct.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    attach_tail_risk(&mut zones, curr);
    zones
}

fn nearest<F>(zones: &[LiquidityZone], predicate: F, price: f64) -> Option<&LiquidityZone>
where
    F: Fn(&LiquidityZone) -> bool,
{
    zones.iter().filter(|zone| predicate(zone)).min_by(|a, b| {
        (a.mid_price - price)
            .abs()
            .partial_cmp(&(b.mid_price - price).abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    })
}

fn target_pool<'a>(
    pools: &[&'a LiquidityZone],
    target_price: f64,
    entry_price: f64,
    preferred_pool: Option<&'a LiquidityZone>,
) -> (Option<&'a LiquidityZone>, bool) {
    let matched = pools.iter().copied().find(|pool| {
        (pool.price_low <= target_price && target_price <= pool.price_high)
            || (target_price - pool.mid_price).abs()
                / entry_price.max(MIN_RELATIVE_DISTANCE_PRICE_DENOMINATOR)
                <= TAKE_PROFIT_POOL_PROXIMITY_RATIO
    });
    let best = matched.or(preferred_pool).or_else(|| {
        pools.iter().copied().min_by(|left, right| {
            (left.mid_price - target_price)
                .abs()
                .partial_cmp(&(right.mid_price - target_price).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    });
    (best, matched.is_some())
}

fn stop_is_vulnerable_near_pool(
    stop_loss: f64,
    pool: &LiquidityZone,
    entry_price: f64,
    relevant_pool_edge: f64,
) -> bool {
    (pool.price_low <= stop_loss && stop_loss <= pool.price_high)
        || (stop_loss - relevant_pool_edge).abs()
            / entry_price.max(MIN_RELATIVE_DISTANCE_PRICE_DENOMINATOR)
            < STOP_LOSS_POOL_PROXIMITY_RATIO
}

fn recommended_safe_stop_loss(pool: &LiquidityZone, entry_price: f64, is_long: bool) -> f64 {
    let pool_buffer = (pool.price_high - pool.price_low).abs() * 0.4;
    let minimum_buffer = entry_price * RECOMMENDED_STOP_LOSS_BUFFER_RATIO;
    round_decimal(
        if is_long {
            pool.price_low - pool_buffer.max(minimum_buffer)
        } else {
            pool.price_high + pool_buffer.max(minimum_buffer)
        },
        4,
    )
}

fn clone_zone(zone: Option<&LiquidityZone>) -> Option<LiquidityZone> {
    zone.cloned()
}

/// Evaluate stop-hunt hazards and target liquidity attraction against zones.
pub fn evaluate_liquidity_risk(
    entry_price: f64,
    stop_loss: f64,
    targets: &[f64],
    direction: &str,
    zones: &[LiquidityZone],
) -> LiquidityRiskReport {
    let dir = direction.to_ascii_uppercase();
    let is_long = dir.contains("LONG");
    let is_short = dir.contains("SHORT");
    let is_selling_time = dir.contains("SELLING_TIME") || dir.contains("SELL_TIME");
    let bsl = zones
        .iter()
        .filter(|z| z.zone_type == ZONE_BSL)
        .collect::<Vec<_>>();
    let ssl = zones
        .iter()
        .filter(|z| z.zone_type == ZONE_SSL)
        .collect::<Vec<_>>();
    let liq = zones
        .iter()
        .filter(|z| z.zone_type == ZONE_LIQ)
        .collect::<Vec<_>>();

    let nearest_bsl_ref = nearest(
        zones,
        |z| z.zone_type == ZONE_BSL && z.mid_price >= entry_price * 0.99,
        entry_price,
    )
    .or_else(|| nearest(zones, |z| z.zone_type == ZONE_BSL, entry_price));
    let nearest_ssl_ref = nearest(
        zones,
        |z| z.zone_type == ZONE_SSL && z.mid_price <= entry_price * 1.01,
        entry_price,
    )
    .or_else(|| nearest(zones, |z| z.zone_type == ZONE_SSL, entry_price));
    let nearest_liq_ref = nearest(zones, |z| z.zone_type == ZONE_LIQ, entry_price);
    let downside = if !ssl.is_empty() {
        ssl.clone()
    } else {
        liq.iter()
            .filter(|zone| zone.mid_price <= entry_price)
            .copied()
            .collect()
    };
    let upside = if !bsl.is_empty() {
        bsl.clone()
    } else {
        liq.iter()
            .filter(|zone| zone.mid_price >= entry_price)
            .copied()
            .collect()
    };

    let mut stop_loss_safety = "SAFE_BEHIND_LIQUIDITY".to_string();
    let mut stop_hunt_hazard = "LOW".to_string();
    let mut recommended_safe_sl = None;
    let mut tp_magnet_score = 50.0;
    let mut liquidity_magnet_target = None;
    let mut narrative = Vec::<String>::new();

    if is_long {
        let vulnerable = downside.iter().find(|pool| {
            stop_is_vulnerable_near_pool(stop_loss, pool, entry_price, pool.price_high)
        });
        if let Some(pool) = vulnerable {
            stop_loss_safety = "VULNERABLE_IN_LIQUIDITY_POOL".to_string();
            stop_hunt_hazard = "HIGH".to_string();
            recommended_safe_sl = Some(recommended_safe_stop_loss(pool, entry_price, true));
            narrative.push(format!(
                "⚠️ STOP-HUNT RISK HIGH: Stop Loss ({stop_loss}) sits inside the {} liquidity zone [{:.2} - {:.2}]. Algorithms frequently sweep this zone before reversing. Recommended safe SL: {:.2}.",
                pool.source_pivot,
                pool.price_low,
                pool.price_high,
                recommended_safe_sl.unwrap_or(stop_loss)
            ));
        } else if let Some(pool) = nearest_ssl_ref {
            if stop_loss < pool.price_low {
                narrative.push(format!(
                    "✅ PROTECTED STOP LOSS: Stop Loss ({stop_loss}) is positioned safely behind the {} liquidity pool [{:.2} - {:.2}].",
                    pool.source_pivot, pool.price_low, pool.price_high
                ));
            } else {
                stop_loss_safety = "UNPROTECTED".to_string();
                stop_hunt_hazard = "MEDIUM".to_string();
                narrative.push(
                    "Stop loss is unanchored to major structural liquidity pools.".to_string(),
                );
            }
        } else if let Some(pool) = nearest_liq_ref {
            if pool.mid_price < entry_price && stop_loss < pool.price_low {
                narrative.push(format!(
                    "✅ PROTECTED STOP LOSS: Stop Loss ({stop_loss}) is positioned safely below the {} liquidation shelf [{:.2} - {:.2}].",
                    pool.source_pivot, pool.price_low, pool.price_high
                ));
            } else {
                stop_loss_safety = "UNPROTECTED".to_string();
                stop_hunt_hazard = "MEDIUM".to_string();
                narrative.push(
                    "Stop loss is unanchored to major structural liquidity pools.".to_string(),
                );
            }
        } else {
            stop_loss_safety = "UNPROTECTED".to_string();
            stop_hunt_hazard = "MEDIUM".to_string();
            narrative
                .push("Stop loss is unanchored to major structural liquidity pools.".to_string());
        }

        if let Some(&tp1) = targets.first() {
            if !upside.is_empty() {
                let (best, matched) = target_pool(&upside, tp1, entry_price, nearest_bsl_ref);
                if let Some(pool) = best {
                    if matched || tp1 >= pool.price_low * 0.98 {
                        tp_magnet_score = 90.0;
                        liquidity_magnet_target =
                            Some(format!("{} at {:.2}", pool.source_pivot, pool.mid_price));
                        narrative.push(format!(
                            "🎯 HIGH LIQUIDITY ATTRACTION: TP1 ({tp1}) directly targets the {}.",
                            pool.source_pivot
                        ));
                    } else {
                        tp_magnet_score = 65.0;
                    }
                }
            }
        }
    } else if is_short {
        let vulnerable = upside.iter().find(|pool| {
            stop_is_vulnerable_near_pool(stop_loss, pool, entry_price, pool.price_low)
        });
        if let Some(pool) = vulnerable {
            stop_loss_safety = "VULNERABLE_IN_LIQUIDITY_POOL".to_string();
            stop_hunt_hazard = "HIGH".to_string();
            recommended_safe_sl = Some(recommended_safe_stop_loss(pool, entry_price, false));
            narrative.push(format!(
                "⚠️ STOP-HUNT RISK HIGH: Stop Loss ({stop_loss}) sits inside the {} liquidity zone [{:.2} - {:.2}]. Recommended safe SL: {:.2}.",
                pool.source_pivot,
                pool.price_low,
                pool.price_high,
                recommended_safe_sl.unwrap_or(stop_loss)
            ));
        } else if let Some(pool) = nearest_bsl_ref {
            if stop_loss > pool.price_high {
                narrative.push(format!(
                    "✅ PROTECTED STOP LOSS: Stop Loss ({stop_loss}) is positioned safely above the {} BSL pool [{:.2} - {:.2}].",
                    pool.source_pivot, pool.price_low, pool.price_high
                ));
            } else {
                stop_loss_safety = "UNPROTECTED".to_string();
                stop_hunt_hazard = "MEDIUM".to_string();
            }
        } else if let Some(pool) = nearest_liq_ref {
            if pool.mid_price > entry_price && stop_loss > pool.price_high {
                narrative.push(format!(
                    "✅ PROTECTED STOP LOSS: Stop Loss ({stop_loss}) is positioned safely above the {} liquidation shelf [{:.2} - {:.2}].",
                    pool.source_pivot, pool.price_low, pool.price_high
                ));
            } else {
                stop_loss_safety = "UNPROTECTED".to_string();
                stop_hunt_hazard = "MEDIUM".to_string();
            }
        } else {
            stop_loss_safety = "UNPROTECTED".to_string();
            stop_hunt_hazard = "MEDIUM".to_string();
        }

        if let Some(&tp1) = targets.first() {
            if !downside.is_empty() {
                let (best, matched) = target_pool(&downside, tp1, entry_price, nearest_ssl_ref);
                if let Some(pool) = best {
                    if matched || tp1 <= pool.price_high * 1.02 {
                        tp_magnet_score = 90.0;
                        liquidity_magnet_target =
                            Some(format!("{} at {:.2}", pool.source_pivot, pool.mid_price));
                        narrative.push(format!(
                            "🎯 HIGH LIQUIDITY ATTRACTION: TP1 ({tp1}) targets the {}.",
                            pool.source_pivot
                        ));
                    }
                }
            }
        }
    } else if is_selling_time {
        let target = nearest_bsl_ref.or_else(|| {
            nearest(
                zones,
                |zone| zone.zone_type == ZONE_LIQ && zone.mid_price >= entry_price,
                entry_price,
            )
        });
        if let Some(pool) = target {
            if entry_price >= pool.price_low * 0.98 {
                tp_magnet_score = 95.0;
                liquidity_magnet_target = Some(format!(
                    "Selling into {} at {:.2}",
                    pool.source_pivot, pool.mid_price
                ));
                narrative.push(format!("🟠 OPTIMAL SELLING TIME: Market price is tapping the {}. Prime institutional liquidity window to liquidate long holdings into resting orders.", pool.source_pivot));
            }
        } else {
            narrative.push("Selling time exit approaching resistance liquidity.".to_string());
        }
    }

    let mut max_tail_score = 0_u32;
    let mut max_tail_label = "LOW".to_string();
    let mut cascade_count = 0_usize;
    for zone in zones {
        let dist = (zone.mid_price - entry_price).abs()
            / entry_price.max(MIN_RELATIVE_DISTANCE_PRICE_DENOMINATOR);
        if dist <= 0.05 && zone.tail_risk_score > max_tail_score {
            max_tail_score = zone.tail_risk_score;
            max_tail_label = zone.tail_risk_label.clone();
        }
        if dist <= 0.03 && zone.tail_risk_score > 50 {
            cascade_count += 1;
        }
    }
    let cascade_probability = cascade_probability_for_count(cascade_count).to_string();
    if max_tail_score > 50 {
        narrative.push(format!("🌪️ TAIL RISK {max_tail_label}: High density of clustered liquidity zones near entry. Cascade probability is {cascade_probability}."));
    }

    LiquidityRiskReport {
        stop_loss_safety,
        stop_hunt_hazard,
        nearest_bsl: clone_zone(nearest_bsl_ref),
        nearest_ssl: clone_zone(nearest_ssl_ref),
        nearest_liq_cluster: clone_zone(nearest_liq_ref),
        recommended_safe_sl,
        tp_magnet_score,
        liquidity_magnet_target,
        audit_summary: narrative.join(" "),
        max_tail_risk_score: max_tail_score,
        max_tail_risk_label: max_tail_label,
        cascade_probability,
    }
}

/// Combined discovery + report helper used by the HTTP sidecar.
#[allow(clippy::too_many_arguments)]
pub fn compute_liquidity_risk(
    bars: &[StrategyBar],
    current_price: Option<f64>,
    asset_class: &str,
    entry_price: f64,
    stop_loss: f64,
    targets: &[f64],
    direction: &str,
    fractal_window: usize,
    lookback_bars: usize,
    include_bsl_ssl: bool,
) -> LiquidityRiskResult {
    let curr = current_price
        .filter(|value| value.is_finite() && *value > 0.0)
        .or_else(|| bars.last().map(|bar| bar.close))
        .unwrap_or(0.0);
    let zones = discover_liquidity_zones(
        bars,
        Some(curr),
        asset_class,
        fractal_window,
        lookback_bars,
        include_bsl_ssl,
    );
    let report = evaluate_liquidity_risk(entry_price, stop_loss, targets, direction, &zones);
    LiquidityRiskResult {
        zones,
        report,
        current_price: curr,
        lookback_bars,
    }
}
