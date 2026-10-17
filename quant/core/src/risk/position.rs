//! Deterministic venue-aware position sizing and liquidation safeguards.
//!
//! This module is intentionally pure: it consumes a validated request and
//! returns an explainable result without touching storage, clocks, or HTTP.
//! Keeping the calculation here gives plan sizing one
//! authoritative risk contract for the plan engine.

use super::policy::{
    drawdown_policy, DEFENSIVE_DRAWDOWN_PCT, HALT_DRAWDOWN_PCT, MAX_CAPITAL_PER_TRADE_PCT,
    MAX_RISK_PER_TRADE_PCT,
};
use crate::{finite, round_decimal, venue_rules::STOCKBIT_MIN_CANDIDATE_PRICE_IDR};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionSizeRequest {
    /// Canonical execution app. Venue rules are selected from this field first.
    pub venue: String,
    pub entry_price: f64,
    pub stop_loss: f64,
    pub asset_class: String,
    pub direction: String,
    pub equity: f64,
    pub available_cash: Option<f64>,
    pub atr: Option<f64>,
    pub leverage: Option<f64>,
    pub win_rate: Option<f64>,
    pub payoff: Option<f64>,
    pub sizing_model: Option<String>,
    pub current_equity: Option<f64>,
    pub peak_equity: Option<f64>,
}

impl Default for PositionSizeRequest {
    fn default() -> Self {
        Self {
            // Empty means legacy caller: derive from asset_class until venue is supplied.
            venue: String::new(),
            entry_price: 0.0,
            stop_loss: 0.0,
            asset_class: "crypto_futures".to_string(),
            direction: "LONG".to_string(),
            equity: 10_000.0,
            available_cash: None,
            atr: None,
            leverage: None,
            win_rate: None,
            payoff: None,
            sizing_model: None,
            current_equity: None,
            peak_equity: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionSizeResult {
    pub approved: bool,
    pub status_badge: String,
    pub rejection_reasons: Vec<String>,
    pub shares: f64,
    pub lots: Option<i64>,
    pub unit_label: String,
    pub notional_value: f64,
    pub margin_required: f64,
    pub effective_leverage: f64,
    pub risk_dollars: f64,
    pub effective_risk_pct: f64,
    pub risk_per_share: f64,
    pub liquidation_price: Option<f64>,
    pub dist_to_liquidation_pct: Option<f64>,
    pub liq_safety_buffer_pct: Option<f64>,
    pub is_liquidation_safe: bool,
    pub risk_scale: f64,
    pub drawdown_pct: f64,
    pub venue_rule: String,
}

const MAX_LEVERAGE: f64 = 25.0;
const DEFAULT_LEVERAGE: f64 = 5.0;
const MMR: f64 = 0.005;
const MIN_LIQ_BUFFER: f64 = 0.20;
const IDX_LOT_SIZE: i64 = 100;
const MIN_NOTIONAL: f64 = 10.0;
const VOL_TARGET_RISK: f64 = 0.015;

fn is_idx(asset: &str) -> bool {
    matches!(asset, "idx" | "idx_stocks" | "stockbit" | "stockbit_idx")
}

fn is_spot(asset: &str) -> bool {
    matches!(
        asset,
        "crypto_spot"
            | "tokocrypto"
            | "pintu_spot"
            | "pluang_spot"
            | "tokenized_stock"
            | "tokocrypto_tokenized_stock"
    )
}

fn is_futures(asset: &str) -> bool {
    !is_idx(asset) && !is_spot(asset)
}

fn asset_class_for_venue(venue: &str, requested: &str) -> String {
    match venue.trim().to_lowercase().as_str() {
        "stockbit" | "stockbit_idx" | "idx" => "idx".into(),
        "tokocrypto" | "tokocrypto_spot" => {
            if matches!(
                requested.trim().to_lowercase().as_str(),
                "tokenized_stock" | "tokocrypto_tokenized_stock"
            ) {
                "tokenized_stock".into()
            } else {
                "tokocrypto".into()
            }
        }
        "pintu_spot" => "pintu_spot".into(),
        "pintu" | "pintu_futures" | "pintu_perpetual" => "crypto_futures".into(),
        _ => requested.to_lowercase(),
    }
}

fn round_units(value: f64, price: f64) -> f64 {
    if price < 100.0 {
        round_decimal(value, 4)
    } else {
        round_decimal(value, 2)
    }
}

fn liquidation_price(entry: f64, direction: &str, leverage: f64) -> f64 {
    let lev = leverage.max(1.0);
    let value = if direction == "LONG" {
        entry * (1.0 - (1.0 / lev) + MMR)
    } else {
        entry * (1.0 + (1.0 / lev) - MMR)
    };
    value.max(0.0)
}

fn empty_result(
    request: &PositionSizeRequest,
    badge: &str,
    reason: String,
    risk_per_share: f64,
) -> PositionSizeResult {
    let unit_label = if is_idx(&request.asset_class.to_lowercase()) {
        "Lots"
    } else {
        "Contracts"
    };
    PositionSizeResult {
        approved: false,
        status_badge: badge.to_string(),
        rejection_reasons: vec![reason],
        shares: 0.0,
        lots: (unit_label == "Lots").then_some(0),
        unit_label: unit_label.to_string(),
        notional_value: 0.0,
        margin_required: 0.0,
        effective_leverage: 0.0,
        risk_dollars: 0.0,
        effective_risk_pct: 0.0,
        risk_per_share,
        liquidation_price: None,
        dist_to_liquidation_pct: None,
        liq_safety_buffer_pct: None,
        is_liquidation_safe: true,
        risk_scale: 1.0,
        drawdown_pct: 0.0,
        venue_rule: venue_rule(&request.asset_class),
    }
}

fn venue_rule(asset: &str) -> String {
    match asset {
        "idx" | "idx_stocks" | "stockbit" | "stockbit_idx" =>
            "Stockbit IDX: long-only, 100-share lots, local plan floor Rp60, same-day exit cutoff enforced by holding policy.".to_string(),
        "tokocrypto" =>
            "Tokocrypto spot: long-only cash purchase; venue minimums and fees must be confirmed from live market metadata.".to_string(),
        "tokenized_stock" | "tokocrypto_tokenized_stock" =>
            "Tokocrypto tokenized stock: long-only spot category; instrument fees and trading session must be confirmed from live market metadata.".to_string(),
        "pintu_spot" =>
            "Pintu spot: long-only cash purchase, fractional coins, minimum notional 10 quote units.".to_string(),
        "pluang_spot" =>
            "Pluang spot: route disabled until instrument, order, fee, and holding rules are configured.".to_string(),
        "crypto_spot" =>
            "Generic crypto spot: long-only cash purchase; venue-specific minimums and fees are unavailable.".to_string(),
        _ => "Pintu perpetuals: long/short, leverage capped at 25x, isolated liquidation buffer checked before approval; funding is not fabricated.".to_string(),
    }
}

/// Calculate an explainable, exchange-aware position size.
pub fn calculate_position_size(request: &PositionSizeRequest) -> PositionSizeResult {
    let asset = asset_class_for_venue(&request.venue, &request.asset_class);
    let direction = request.direction.to_uppercase();
    let configured_equity = if finite(request.equity) && request.equity > 0.0 {
        request.equity
    } else {
        0.0
    };
    let equity = match request.current_equity {
        Some(value) if finite(value) && value >= 0.0 => value,
        Some(_) => 0.0,
        None => configured_equity,
    };
    let cash = match request.available_cash {
        Some(value) if finite(value) && value >= 0.0 => value,
        Some(_) => 0.0,
        None => equity,
    };
    let risk_per_share = (request.entry_price - request.stop_loss).abs();
    let selling_time = matches!(
        direction.as_str(),
        "SELLING_TIME" | "SELL_TIME" | "EXIT" | "SELL"
    );
    if !selling_time && direction != "LONG" && direction != "SHORT" {
        return empty_result(
            request,
            "REJECTED_INVALID_DIRECTION",
            "Direction must be LONG, SHORT, or a supported selling-time exit.".into(),
            risk_per_share,
        );
    }
    if !finite(request.entry_price) || !finite(request.stop_loss) || request.entry_price <= 0.0 {
        return empty_result(
            request,
            "REJECTED_INVALID_GEOMETRY",
            "Stop loss is identical to or invalid relative to entry price.".to_string(),
            risk_per_share,
        );
    }
    if (is_idx(&asset) || is_spot(&asset)) && direction == "SHORT" && !selling_time {
        return empty_result(request, "REJECTED_SHORT_NOT_ALLOWED", format!("{} is strictly LONG ONLY. Short selling prohibited. Use SELLING_TIME for exit plans.", asset.to_uppercase()), risk_per_share);
    }
    if is_idx(&asset) && request.entry_price < STOCKBIT_MIN_CANDIDATE_PRICE_IDR {
        return empty_result(
            request,
            "REJECTED_PENNY_STOCK",
            format!(
                "Stock price {} IDR is below minimum threshold of {} IDR.",
                request.entry_price, STOCKBIT_MIN_CANDIDATE_PRICE_IDR
            ),
            risk_per_share,
        );
    }
    if !finite(risk_per_share) || risk_per_share <= 0.0 {
        return empty_result(
            request,
            "REJECTED_INVALID_GEOMETRY",
            "Stop loss is identical to or invalid relative to entry price.".to_string(),
            risk_per_share,
        );
    }
    if !selling_time
        && ((direction == "LONG" && request.stop_loss >= request.entry_price)
            || (direction == "SHORT" && request.stop_loss <= request.entry_price))
    {
        return empty_result(
            request,
            "REJECTED_INVALID_GEOMETRY",
            "Stop loss must be on the loss side of entry for the selected direction.".into(),
            risk_per_share,
        );
    }

    if equity <= 0.0 || !finite(cash) || cash <= 0.0 {
        return empty_result(
            request,
            "REJECTED_INVALID_CAPITAL",
            "Positive current equity and available cash are required for risk sizing.".into(),
            risk_per_share,
        );
    }

    let peak = request.peak_equity.unwrap_or(configured_equity.max(equity));
    let Some(drawdown_state) = drawdown_policy(equity, peak) else {
        return empty_result(
            request,
            "REJECTED_INVALID_EQUITY_HISTORY",
            "Peak and current equity must be finite and peak equity must be positive.".into(),
            risk_per_share,
        );
    };
    let drawdown = drawdown_state.drawdown_fraction;
    let risk_scale = drawdown_state.risk_scale;
    let (allowed, circuit_message) = if !drawdown_state.allowed {
        (
            false,
            format!(
                "CIRCUIT BREAKER LEVEL 2: Drawdown {:.1}% >= {:.0}%. Trading Halted.",
                drawdown * 100.0,
                HALT_DRAWDOWN_PCT * 100.0
            ),
        )
    } else if drawdown >= DEFENSIVE_DRAWDOWN_PCT {
        (
            true,
            format!(
                "CIRCUIT BREAKER LEVEL 1: Drawdown {:.1}% >= {:.0}%. Risk scaled down 50%.",
                drawdown * 100.0,
                DEFENSIVE_DRAWDOWN_PCT * 100.0
            ),
        )
    } else {
        (
            true,
            format!(
                "NORMAL OPERATION: Drawdown {:.1}% within limits.",
                drawdown * 100.0
            ),
        )
    };
    if !allowed {
        let mut result = empty_result(
            request,
            "REJECTED_CIRCUIT_BREAKER",
            circuit_message,
            risk_per_share,
        );
        result.risk_scale = risk_scale;
        result.drawdown_pct = round_decimal(drawdown * 100.0, 2);
        return result;
    }

    let model = request
        .sizing_model
        .as_deref()
        .unwrap_or("FIXED_FRACTIONAL")
        .to_uppercase();
    let base_risk = match model.as_str() {
        "VOLATILITY_PARITY" if request.atr.unwrap_or(0.0) > 0.0 => {
            let target = equity * VOL_TARGET_RISK * risk_scale;
            (equity * MAX_RISK_PER_TRADE_PCT * risk_scale)
                .min(target / (request.atr.unwrap_or(0.0) / request.entry_price).max(0.01))
        }
        "HALF_KELLY" => {
            let valid_win_rate = request
                .win_rate
                .filter(|value| finite(*value) && *value > 0.0 && *value < 1.0);
            let valid_payoff = request
                .payoff
                .filter(|value| finite(*value) && *value > 0.0);
            let (Some(win_rate), Some(payoff)) = (valid_win_rate, valid_payoff) else {
                return empty_result(
                    request,
                    "REJECTED_KELLY_EVIDENCE",
                    "HALF_KELLY requires an observed win rate in (0, 1) and a positive observed payoff ratio.".into(),
                    risk_per_share,
                );
            };
            let kelly = win_rate - ((1.0 - win_rate) / payoff);
            if !finite(kelly) || kelly <= 0.0 {
                return empty_result(
                    request,
                    "REJECTED_KELLY_NO_EDGE",
                    "Observed win rate and payoff do not produce a positive Kelly fraction.".into(),
                    risk_per_share,
                );
            }
            equity * (0.5 * kelly).min(MAX_RISK_PER_TRADE_PCT) * risk_scale
        }
        _ => equity * MAX_RISK_PER_TRADE_PCT * risk_scale,
    };
    let max_notional = equity * MAX_CAPITAL_PER_TRADE_PCT;
    let raw_shares = (base_risk / risk_per_share).min(max_notional / request.entry_price);
    let mut reasons = Vec::new();
    if base_risk / risk_per_share > max_notional / request.entry_price {
        reasons.push(format!(
            "Position capped at single-asset allocation limit ({:.0}% of equity).",
            MAX_CAPITAL_PER_TRADE_PCT * 100.0
        ));
    }

    let effective_lev = if is_futures(&asset) {
        request
            .leverage
            .unwrap_or(DEFAULT_LEVERAGE)
            .clamp(1.0, MAX_LEVERAGE)
    } else {
        1.0
    };
    let mut lots = None;
    let (mut shares, unit_label) = if is_idx(&asset) {
        let count = (raw_shares / IDX_LOT_SIZE as f64).floor() as i64;
        lots = Some(count);
        (count as f64 * IDX_LOT_SIZE as f64, "Lots")
    } else if is_spot(&asset) {
        (round_units(raw_shares, request.entry_price), "Coins")
    } else {
        (round_units(raw_shares, request.entry_price), "Contracts")
    };
    if is_idx(&asset) && shares < IDX_LOT_SIZE as f64 {
        reasons.push(format!("Position size ({:.1} shares) is below IDX minimum of 1 Lot (100 shares). Required capital exceeds risk allocation.", raw_shares));
    }
    let mut notional = shares * request.entry_price;
    let mut margin = notional / effective_lev;
    if !selling_time && margin > cash * 0.95 {
        if effective_lev == 1.0 {
            let affordable = cash * 0.95;
            if is_idx(&asset) {
                let affordable_lots =
                    (affordable / request.entry_price / IDX_LOT_SIZE as f64).floor() as i64;
                if affordable_lots < 1 {
                    reasons.push(
                        "Insufficient available cash to purchase minimum 1 Lot (100 shares)."
                            .to_string(),
                    );
                } else {
                    lots = Some(affordable_lots);
                    shares = affordable_lots as f64 * IDX_LOT_SIZE as f64;
                    notional = shares * request.entry_price;
                    margin = notional;
                    reasons.push("Position downscaled to fit available cash buffer.".to_string());
                }
            } else {
                shares = round_units(affordable / request.entry_price, request.entry_price);
                notional = shares * request.entry_price;
                margin = notional;
                reasons.push("Position downscaled to fit available cash buffer.".to_string());
            }
        } else {
            reasons.push(format!(
                "Required margin ({:.2}) exceeds 95% available cash buffer ({:.2}).",
                margin,
                cash * 0.95
            ));
        }
    }
    if is_spot(&asset) && notional < MIN_NOTIONAL {
        reasons.push(format!(
            "Notional value ({:.2}) is below minimum order threshold of {:.2}.",
            notional, MIN_NOTIONAL
        ));
    }

    let mut approved = reasons.iter().all(|reason| {
        !reason.starts_with("Position size")
            && !reason.starts_with("Insufficient")
            && !reason.starts_with("Required margin")
            && !reason.starts_with("Notional value")
    });
    let mut liq = None;
    let mut liq_dist = None;
    let mut liq_buffer = None;
    let mut liq_safe = true;
    if is_futures(&asset) && effective_lev > 1.0 {
        let liq_price = liquidation_price(request.entry_price, &direction, effective_lev);
        liq = Some(liq_price);
        liq_dist = Some((request.entry_price - liq_price).abs() / request.entry_price * 100.0);
        let safe = if direction == "LONG" {
            liq_price < request.stop_loss
        } else {
            liq_price > request.stop_loss
        };
        if !safe {
            liq_safe = false;
            approved = false;
            reasons.push(format!("CRITICAL: Liquidation price ({:.2}) triggers before or equal to Stop Loss ({:.2})! Reduce leverage.", liq_price, request.stop_loss));
        } else {
            let cushion = if direction == "LONG" {
                request.stop_loss - liq_price
            } else {
                liq_price - request.stop_loss
            };
            liq_buffer = Some(cushion / risk_per_share * 100.0);
            if cushion / risk_per_share < MIN_LIQ_BUFFER {
                reasons.push(format!("WARNING: Liquidation buffer ({:.1}%) is below recommended safety margin of 20%.", cushion / risk_per_share * 100.0));
            }
        }
    }
    let badge = if !approved {
        "REJECTED"
    } else if selling_time {
        "APPROVED_SELLING_TIME"
    } else if reasons.iter().any(|r| r.starts_with("WARNING")) {
        "APPROVED_WITH_WARNING"
    } else {
        "APPROVED"
    };
    if approved && reasons.is_empty() {
        reasons.push("Risk parameters and physical lot constraints verified.".to_string());
    }
    let actual_risk = shares * risk_per_share;
    PositionSizeResult {
        approved,
        status_badge: badge.to_string(),
        rejection_reasons: reasons,
        shares,
        lots,
        unit_label: unit_label.to_string(),
        notional_value: round_decimal(notional, 2),
        margin_required: round_decimal(margin, 2),
        effective_leverage: round_decimal(notional / equity.max(1.0), 2),
        risk_dollars: round_decimal(actual_risk, 2),
        effective_risk_pct: round_decimal(actual_risk / equity.max(1.0) * 100.0, 2),
        risk_per_share: round_decimal(risk_per_share, 4),
        liquidation_price: liq.map(|v| round_decimal(v, 4)),
        dist_to_liquidation_pct: liq_dist.map(|v| round_decimal(v, 2)),
        liq_safety_buffer_pct: liq_buffer.map(|v| round_decimal(v, 1)),
        is_liquidation_safe: liq_safe,
        risk_scale,
        drawdown_pct: round_decimal(drawdown * 100.0, 2),
        venue_rule: venue_rule(&asset),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idx_quantizes_to_lots() {
        let result = calculate_position_size(&PositionSizeRequest {
            entry_price: 1_000.0,
            stop_loss: 950.0,
            asset_class: "idx".into(),
            direction: "LONG".into(),
            equity: 100_000_000.0,
            ..Default::default()
        });
        assert_eq!(result.lots, Some(250));
        assert_eq!(result.shares, 25_000.0);
    }

    #[test]
    fn stockbit_position_sizing_rejects_prices_below_rp60() {
        let result = calculate_position_size(&PositionSizeRequest {
            venue: "stockbit".into(),
            entry_price: 59.0,
            stop_loss: 55.0,
            asset_class: "idx".into(),
            direction: "LONG".into(),
            equity: 100_000_000.0,
            ..Default::default()
        });
        assert!(!result.approved);
        assert_eq!(result.status_badge, "REJECTED_PENNY_STOCK");
        assert!(result.rejection_reasons[0].contains("60 IDR"));
    }

    #[test]
    fn futures_liquidation_is_checked() {
        let result = calculate_position_size(&PositionSizeRequest {
            entry_price: 100.0,
            stop_loss: 95.0,
            asset_class: "crypto_futures".into(),
            direction: "LONG".into(),
            equity: 10_000.0,
            leverage: Some(25.0),
            ..Default::default()
        });
        assert!(result.liquidation_price.is_some());
        assert!(!result.is_liquidation_safe);
        assert!(!result.approved);
    }
}
