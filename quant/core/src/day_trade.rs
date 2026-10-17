//! Deterministic venue profile enrichment for the registered day-trade plans.
//!
//! The profile is returned as a structured value because Stockbit and Pintu
//! expose different fields.  The Python manager only assembles/persists this
//! DTO; all arithmetic and venue rules live here.

use crate::venue_rules::{
    stockbit_candidate_price_eligible, PINTU_MAKER_FEE_PCT, PINTU_TAKER_FEE_PCT,
    PINTU_TAKER_FEE_RATE, PINTU_TAKER_FEE_ROUND_TRIP_PCT, STOCKBIT_MIN_CANDIDATE_PRICE_IDR,
};
use crate::{finite, round_decimal, strategies::StrategyBar};
use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};

fn format_price(value: f64) -> String {
    if !finite(value) || value == 0.0 {
        return "0.00".into();
    }
    let abs = value.abs();
    if abs >= 100.0 {
        format!("{value:.2}")
    } else if abs >= 1.0 {
        format!("{value:.3}")
    } else if abs >= 0.01 {
        format!("{value:.4}")
    } else if abs >= 0.0001 {
        format!("{value:.6}")
    } else if abs >= 0.000001 {
        format!("{value:.8}")
    } else {
        format!("{value:.10}")
    }
}

fn grouped_integer(value: f64) -> String {
    let raw = format!("{:.0}", value.max(0.0));
    let mut grouped = String::with_capacity(raw.len() + raw.len() / 3);
    for (index, character) in raw.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(character);
    }
    grouped.chars().rev().collect()
}

fn trimmed_decimal(value: f64, decimals: usize) -> String {
    let mut text = format!("{:.*}", decimals, value);
    while text.contains('.') && text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.push('0');
    }
    text
}

fn tick_size(price: f64) -> f64 {
    if price < 200.0 {
        1.0
    } else if price < 500.0 {
        2.0
    } else if price < 2_000.0 {
        5.0
    } else if price < 5_000.0 {
        10.0
    } else {
        25.0
    }
}

fn round_tick(price: f64, mode: &str) -> f64 {
    if !finite(price) || price <= 0.0 {
        return 50.0;
    }
    let tick = tick_size(price);
    let units = match mode {
        "floor" => (price / tick).floor(),
        "ceil" => (price / tick).ceil(),
        _ => (price / tick).round_ties_even(),
    };
    (units * tick).max(50.0)
}

fn ara_arb(reference: f64) -> (f64, f64, f64, f64, f64, f64) {
    if !finite(reference) || reference <= 0.0 {
        return (50.0, 50.0, 0.0, 0.0, 25.0, 1.0);
    }
    let limit = if reference <= 200.0 {
        0.35
    } else if reference <= 5_000.0 {
        0.25
    } else {
        0.20
    };
    let ara = round_tick(reference * (1.0 + limit), "floor");
    let arb = round_tick(reference * (1.0 - limit), "ceil").max(50.0);
    (
        ara,
        arb,
        round_decimal((ara - reference) / reference * 100.0, 2),
        round_decimal((arb - reference) / reference * 100.0, 2),
        limit * 100.0,
        tick_size(reference),
    )
}

fn rolling_volume_mean(bars: &[StrategyBar]) -> f64 {
    if bars.len() < 5 {
        return f64::NAN;
    }
    let start = bars.len().saturating_sub(20);
    let values = &bars[start..];
    if values.iter().any(|bar| !finite(bar.volume)) {
        f64::NAN
    } else {
        values.iter().map(|bar| bar.volume).sum::<f64>() / values.len() as f64
    }
}

fn clean_ticker(asset: &str) -> String {
    asset
        .replace("-USD", "")
        .replace("USDT", "")
        .replace(".JK", "")
}

fn funding_info() -> Value {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);
    let day = crate::SECONDS_PER_DAY;
    let hour = crate::SECONDS_PER_HOUR;
    let day_start = now.div_euclid(day) * day;
    let mut next = day_start;
    for offset in [0i64, 8 * hour, 16 * hour, day] {
        let candidate = day_start + offset;
        if candidate > now {
            next = candidate;
            break;
        }
    }
    if next <= now {
        next = day_start + day;
    }
    let minutes = (next - now).max(0) / crate::SECONDS_PER_MINUTE;
    let hours = minutes / crate::SECONDS_PER_MINUTE;
    let remainder = minutes % crate::SECONDS_PER_MINUTE;
    // Funding cuts are 00:00/08:00/16:00 UTC => 07:00/15:00/23:00 WIB.
    let wib_hour = ((next - day_start) / hour + 7) % 24;
    let wib_time = format!("{wib_hour:02}:00 WIB");
    json!({
        "next_cut_utc": format!("unix:{next}"),
        "wib_time": wib_time,
        "hours_remaining": hours,
        "minutes_remaining": remainder,
        "total_minutes": minutes,
        "countdown_str": format!("{hours}h {remainder:02}m to {wib_hour:02}:00 WIB"),
        "countdown_short": format!("{hours}h {remainder:02}m"),
        "is_urgent": (next - now) <= 1_800,
        "funding_interval": "8 Hours",
    })
}

fn return_metrics(
    entry: f64,
    target: f64,
    direction: &str,
    notional: Option<f64>,
    margin: Option<f64>,
) -> Value {
    if !finite(entry)
        || !finite(target)
        || entry <= 0.0
        || target <= 0.0
        || !matches!(direction, "LONG" | "SHORT")
    {
        return json!({"price_change_pct": 0.0, "price_return_pct": 0.0, "net_return_pct": 0.0, "roi_pct": null, "leverage": null, "roi_basis": "Set position notional and allocated margin to estimate ROI"});
    }
    let change = target / entry - 1.0;
    let gross = change * if direction == "LONG" { 1.0 } else { -1.0 };
    let fee = PINTU_TAKER_FEE_RATE * (1.0 + target / entry);
    let net = gross - fee;
    let valid_size = notional
        .zip(margin)
        .filter(|(n, m)| finite(*n) && finite(*m) && *n > 0.0 && *m > 0.0);
    let leverage = valid_size.map(|(n, m)| n / m);
    let roi = leverage.map(|value| net * value * 100.0);
    json!({
        "price_change_pct": round_decimal(change * 100.0, 6),
        "price_return_pct": round_decimal(gross * 100.0, 6),
        "net_return_pct": round_decimal(net * 100.0, 6),
        "roi_pct": roi.map(|value| round_decimal(value, 6)),
        "leverage": leverage,
        "roi_basis": if leverage.is_some() { "Estimated net PnL / allocated initial margin; excludes funding and slippage" } else { "Set position notional and allocated margin to estimate ROI" },
    })
}

/// Build a Stockbit or Pintu day-trade profile. `daily_bars` are used for
/// Stockbit and `intraday_bars` for Pintu volume/ATR context.
#[allow(clippy::too_many_arguments)]
pub fn compute_day_trade_profile(
    asset: &str,
    asset_class: &str,
    current_price: f64,
    stop_loss: f64,
    tp1: f64,
    direction: &str,
    position_notional: Option<f64>,
    margin_allocated: Option<f64>,
    daily_bars: &[StrategyBar],
    intraday_bars: &[StrategyBar],
) -> Option<Value> {
    let is_idx = asset_class == "idx" || asset.ends_with(".JK");
    let is_futures = asset_class == "crypto_futures" || asset_class.to_lowercase().contains("fut");
    if !is_idx && !is_futures {
        return None;
    }
    if is_idx {
        if direction != "LONG" || daily_bars.len() < 5 {
            return None;
        }
        let last = daily_bars.last()?;
        let previous = daily_bars
            .get(daily_bars.len().saturating_sub(2))
            .unwrap_or(last);
        let open = last.open;
        let low = last.low;
        let close = last.close;
        let high = last.high;
        let average = rolling_volume_mean(daily_bars);
        let rvol = if finite(average) && average > 0.0 {
            round_decimal(last.volume / average, 1)
        } else {
            1.0
        };
        let turnover = close * last.volume;
        let is_ol = ((open - low).abs() / open.max(1.0) <= 0.005) && close >= open;
        let breakout = high >= previous.high || close >= previous.high;
        let (ara, arb, _ara_pct, _arb_pct, _max_limit, tick) = ara_arb(current_price);
        let ticks_to_ara = ((ara - current_price) / tick).round_ties_even();
        let price_eligible = stockbit_candidate_price_eligible(current_price);
        let eligible = price_eligible
            && ticks_to_ara >= 3.0
            && (is_ol || breakout || rvol >= 1.5 || turnover >= 1_000_000_000.0);
        let setup = if is_ol {
            "⚡ O=L BUYER DEFENSE"
        } else if breakout {
            "⚡ BREAKOUT R1 MOMENTUM"
        } else if rvol >= 2.0 {
            "⚡ RVOL VOLUME SPIKE"
        } else {
            "⚡ LIQUIDITY SCALP (LQ45)"
        };
        let tp_raw = ara.min(current_price * 1.018);
        let mut tp_scalp = round_tick(tp_raw, "ceil");
        if tp_scalp <= current_price {
            tp_scalp = current_price + tick * 2.0;
        }
        let sl_raw = arb.max(current_price * 0.985);
        let mut sl_scalp = round_tick(sl_raw, "floor");
        if sl_scalp >= current_price {
            sl_scalp = current_price - tick * 2.0;
        }
        let tp_gain = round_decimal((tp_scalp - current_price) / current_price * 100.0, 2);
        let sl_loss = round_decimal((current_price - sl_scalp) / current_price * 100.0, 2);
        let score = ((if is_ol { 35.0 } else { 10.0 })
            + (rvol * 12.0).trunc()
            + if breakout { 25.0 } else { 0.0 })
        .clamp(50.0, 98.0) as i64;
        let turnover_formatted = if turnover >= 1_000_000_000.0 {
            format!("Rp {:.1}B", turnover / 1e9)
        } else {
            format!("Rp {:.0}M", turnover / 1e6)
        };
        return Some(json!({
            "is_eligible": eligible,
            "candidate_price_eligible": price_eligible,
            "minimum_candidate_price_idr": STOCKBIT_MIN_CANDIDATE_PRICE_IDR,
            "exchange": "IDX_STOCKBIT",
            "direction": "LONG",
            "direction_badge": "🟢 LONG (1.0x)",
            "leverage": 1.0,
            "leverage_label": "1.0x Cash (T+0)",
            "score": score,
            "is_open_equal_low": is_ol,
            "is_breakout_r1": breakout,
            "rvol": rvol,
            "turnover_idr": turnover,
            "turnover_formatted": turnover_formatted,
            "setup_label": setup,
            "ticks_to_ara": ticks_to_ara,
            "tp1_scalp": tp_scalp,
            "tp1_scalp_pct": format!("+{}%", trimmed_decimal(tp_gain, 2)),
            "sl_scalp": sl_scalp,
            "sl_scalp_pct": format!("-{}%", trimmed_decimal(sl_loss, 2)),
            "net_profit_pct": format!("+{}% (Net)", trimmed_decimal(round_decimal(tp_gain - 0.40, 2), 2)),
            "net_loss_pct": format!("-{}% (Net w/ fee)", trimmed_decimal(round_decimal(sl_loss + 0.40, 2), 2)),
            "target_win_rate": "50% minimum win-rate goal (unvalidated)",
            "intraday_exit_rule": "15:40 WIB (Stockbit Day Trade facility deadline)",
            "holding_policy": "stockbit_day_trade",
            "order_instruction": format!("Buy: HAKA / Best Bid Rp {} | Take Profit: Rp {} (+{}%) | Cut Loss: Rp {} (-{}% / -{}% Net)", current_price as i64, tp_scalp as i64, trimmed_decimal(tp_gain, 2), sl_scalp as i64, trimmed_decimal(sl_loss, 2), trimmed_decimal(round_decimal(sl_loss + 0.40, 2), 2)),
        }));
    }

    let direction = direction.to_uppercase();
    if !matches!(direction.as_str(), "LONG" | "SHORT")
        || !finite(current_price)
        || current_price <= 0.0
        || !finite(stop_loss)
        || !finite(tp1)
        || stop_loss <= 0.0
        || tp1 <= 0.0
    {
        return None;
    }
    if (direction == "LONG" && !(stop_loss < current_price && current_price < tp1))
        || (direction == "SHORT" && !(tp1 < current_price && current_price < stop_loss))
    {
        return None;
    }
    let tp_metrics = return_metrics(
        current_price,
        tp1,
        &direction,
        position_notional,
        margin_allocated,
    );
    let sl_metrics = return_metrics(
        current_price,
        stop_loss,
        &direction,
        position_notional,
        margin_allocated,
    );
    let leverage = tp_metrics.get("leverage").cloned().unwrap_or(Value::Null);
    let roi = tp_metrics.get("roi_pct").cloned().unwrap_or(Value::Null);
    let price_change_tp = tp_metrics["price_change_pct"].as_f64().unwrap_or(0.0);
    let price_change_sl = sl_metrics["price_change_pct"].as_f64().unwrap_or(0.0);
    let net_tp = tp_metrics["net_return_pct"].as_f64().unwrap_or(0.0);
    let net_sl = sl_metrics["net_return_pct"].as_f64().unwrap_or(0.0);
    let rvol_turnover = if intraday_bars.len() >= 5 {
        let last = intraday_bars.last()?;
        let average = rolling_volume_mean(intraday_bars);
        if finite(average) && average > 0.0 {
            Some((
                round_decimal(last.volume / average, 2),
                current_price * last.volume,
            ))
        } else {
            None
        }
    } else {
        None
    };
    let atr_pct = if intraday_bars.len() >= 5 {
        let count = 14.min(intraday_bars.len());
        let start = intraday_bars.len() - count;
        let atr = intraday_bars[start..]
            .iter()
            .map(|bar| bar.high - bar.low)
            .sum::<f64>()
            / count as f64;
        if finite(atr) {
            Some(round_decimal(atr / current_price * 100.0, 2))
        } else {
            None
        }
    } else {
        None
    };
    let clean = clean_ticker(asset);
    let funding = funding_info();
    let funding_short = funding["countdown_short"].as_str().unwrap_or("");
    let funding_wib = funding["wib_time"].as_str().unwrap_or("");
    let eligible = sl_metrics["price_return_pct"].as_f64().unwrap_or(0.0) >= -4.0 && net_tp > 0.0;
    Some(json!({
        "is_eligible": eligible,
        "exchange": "PINTU_FUTURES",
        "contract_symbol": format!("{clean}USDT-PERP"),
        "direction": direction,
        "direction_badge": if direction == "LONG" { "🟢 LONG" } else { "🔴 SHORT" },
        "leverage": leverage,
        "leverage_label": leverage.as_f64().map(|value| format!("{value}x from position margin")).unwrap_or_else(|| "Position margin required".into()),
        "position_notional": if leverage.is_null() { Value::Null } else { position_notional.map(Value::from).unwrap_or(Value::Null) },
        "margin_allocated": if leverage.is_null() { Value::Null } else { margin_allocated.map(Value::from).unwrap_or(Value::Null) },
        "score": Value::Null,
        "rvol": rvol_turnover.map(|value| value.0),
        "turnover_usd": rvol_turnover.map(|value| value.1),
        "turnover_formatted": rvol_turnover.map(|value| format!("$ {}", grouped_integer(value.1))).unwrap_or_else(|| "Unavailable".into()),
        "atr_pct": atr_pct,
        "setup_label": format!("PINTU: {direction} PLAN"),
        "entry_price": current_price,
        "tp1_scalp": tp1,
        "sl_scalp": stop_loss,
        "tp1_scalp_pct": format!("{price_change_tp:+.2}%"),
        "sl_scalp_pct": format!("{price_change_sl:+.2}%"),
        "price_change_tp_pct": price_change_tp,
        "price_change_sl_pct": price_change_sl,
        "price_return_tp_pct": tp_metrics["price_return_pct"],
        "price_return_sl_pct": sl_metrics["price_return_pct"],
        "net_return_tp_pct": tp_metrics["net_return_pct"],
        "net_return_sl_pct": sl_metrics["net_return_pct"],
        "roi_tp_pct": roi,
        "roi_sl_pct": sl_metrics["roi_pct"],
        "roi_basis": tp_metrics["roi_basis"],
        "net_profit_pct": format!("{net_tp:+.2}% of notional (est. after fees)"),
        "net_loss_pct": format!("{net_sl:+.2}% of notional (est. after fees)"),
        "fee_maker_pct": PINTU_MAKER_FEE_PCT,
        "fee_taker_pct": PINTU_TAKER_FEE_PCT,
        "fee_roundtrip_taker_pct": PINTU_TAKER_FEE_ROUND_TRIP_PCT,
        "estimated_liquidation_price": Value::Null,
        "dist_to_liquidation_pct": Value::Null,
        "liquidation_buffer_pct": Value::Null,
        "liquidation_safety_status": "Requires exchange margin data",
        "funding_countdown_str": funding["countdown_str"],
        "funding_countdown_short": funding_short,
        "funding_wib_time": funding_wib,
        "funding_urgent": funding["is_urgent"],
        "target_win_rate": "50% minimum win-rate goal (unvalidated)",
        "intraday_exit_rule": "No fixed expiry; SL/TP or setup invalidation. Funding is not an exit deadline.",
        "holding_policy": "pintu_perpetual",
        "order_instruction": format!("{direction} {clean} | Entry: ${} | TP1: ${} ({price_change_tp:+.2}% price change; {}) | SL: ${} ({price_change_sl:+.2}% price change) | Funding at {funding_wib} (payment event; no mandatory exit)", format_price(current_price), format_price(tp1), if roi.is_null() { "ROI: margin required".into() } else { format!("{:+.2}% estimated ROI", roi.as_f64().unwrap_or(0.0)) }, format_price(stop_loss)),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(close: f64, volume: f64) -> StrategyBar {
        StrategyBar {
            time: 0,
            open: close,
            high: close * 1.01,
            low: close * 0.99,
            close,
            volume,
        }
    }

    #[test]
    fn pintu_profile_uses_actual_levels_and_optional_margin_roi() {
        let profile = compute_day_trade_profile(
            "BTC-USD",
            "crypto_futures",
            100.0,
            96.0,
            112.0,
            "LONG",
            Some(1_000.0),
            Some(250.0),
            &[],
            &[bar(100.0, 10.0); 5],
        )
        .expect("profile");
        assert_eq!(profile["exchange"], "PINTU_FUTURES");
        assert_eq!(profile["price_change_tp_pct"], 12.0);
        assert_eq!(profile["roi_tp_pct"], 45.57048);
    }

    #[test]
    fn stockbit_profile_requires_five_daily_bars_and_returns_legal_levels() {
        let profile = compute_day_trade_profile(
            "BBCA.JK",
            "idx",
            10_000.0,
            9_850.0,
            10_200.0,
            "LONG",
            None,
            None,
            &[bar(10_000.0, 100.0); 5],
            &[],
        )
        .expect("profile");
        assert_eq!(profile["exchange"], "IDX_STOCKBIT");
        assert_eq!(profile["holding_policy"], "stockbit_day_trade");
        assert!(profile["tp1_scalp"].as_f64().unwrap() >= 10_000.0);
    }

    #[test]
    fn stockbit_profile_marks_prices_below_rp60_ineligible() {
        let profile = compute_day_trade_profile(
            "ACRO.JK",
            "idx",
            59.0,
            55.0,
            62.0,
            "LONG",
            None,
            None,
            &[bar(59.0, 100.0); 5],
            &[],
        )
        .expect("profile remains available for explanation");
        assert_eq!(profile["candidate_price_eligible"], false);
        assert_eq!(profile["minimum_candidate_price_idr"], 60.0);
        assert_eq!(profile["is_eligible"], false);
    }
}
