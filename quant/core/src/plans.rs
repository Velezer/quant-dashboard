//! Registered day-trade plan generation.
//!
//! This module is intentionally pure: it receives the same bounded OHLCV
//! window used by the signal engine and returns the canonical
//! screener row/plan contract consumed by the existing UI.  Storage and HTTP
//! concerns stay in the sidecar binary.

use crate::finite;
use crate::strategies::{
    generate_idx_first_half_hour_momentum_signals, generate_idx_opening_range_signals,
    generate_idx_signals, generate_idx_time_of_day_momentum_signals,
    generate_idx_time_series_momentum_signals, generate_pintu_golden_cross_signals,
    generate_pintu_golden_cross_signals_with_context, generate_tokocrypto_confluence_signals,
    idx_tick_size, is_pintu_golden_cross_strategy, is_tokenized_stock_confluence_strategy,
    is_tokocrypto_spot_confluence_strategy, round_idx_tick_at, round_price, round_price_up,
    IdxIntradayMomentumConfig, IdxOpeningRangeConfig, IdxStrategyConfig, IdxTickRounding,
    IdxTimeSeriesMomentumConfig, PintuGoldenCrossConfig, PintuSignalTimeframeContext, StrategyBar,
    StrategySignal, TokocryptoConfluenceConfig, PINTU_GOLDEN_CROSS_CONTEXT_TIMEFRAME,
    PINTU_GOLDEN_CROSS_MINIMUM_PLAN_BARS, PINTU_GOLDEN_CROSS_MIN_R_MULTIPLE,
    PINTU_GOLDEN_CROSS_STRATEGY_ID, PINTU_GOLDEN_CROSS_TIMEFRAME,
    TOKENIZED_STOCK_CONFLUENCE_STRATEGY_ID, TOKOCRYPTO_SPOT_CONFLUENCE_STRATEGY_ID,
};
use crate::venue_rules::stockbit_candidate_price_eligible;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructureSnapshot {
    pub trend: String,
    pub bos: Option<String>,
    pub choch: Option<String>,
    pub range_high: f64,
    pub range_low: f64,
    pub eq: f64,
    pub pricing: String,
    pub percent_in_range: f64,
    pub atr: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisteredPlan {
    pub base: Value,
    pub plan: Value,
    pub signal: StrategySignal,
    pub structure: StructureSnapshot,
}

fn format_price(value: f64) -> String {
    if !finite(value) || value == 0.0 {
        return "0.00".into();
    }
    let abs = value.abs();
    if abs >= 50.0 && value.fract() == 0.0 {
        format!("{value:.0}")
    } else if abs >= 100.0 {
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

fn atr(bars: &[StrategyBar]) -> f64 {
    if bars.is_empty() {
        return 0.0;
    }
    let start = bars.len().saturating_sub(14);
    let mut total = 0.0;
    let mut count = 0usize;
    for index in start..bars.len() {
        let previous_close = index
            .checked_sub(1)
            .and_then(|i| bars.get(i))
            .map(|bar| bar.close)
            .unwrap_or(bars[index].close);
        let tr = (bars[index].high - bars[index].low)
            .max((bars[index].high - previous_close).abs())
            .max((bars[index].low - previous_close).abs());
        if finite(tr) {
            total += tr;
            count += 1;
        }
    }
    if count == 0 {
        bars.last().map(|bar| bar.close * 0.03).unwrap_or(0.0)
    } else {
        total / count as f64
    }
}

fn structure_snapshot(bars: &[StrategyBar]) -> StructureSnapshot {
    let n = bars.len();
    if n == 0 {
        return StructureSnapshot {
            trend: "RANGING".into(),
            bos: None,
            choch: None,
            range_high: 0.0,
            range_low: 0.0,
            eq: 0.0,
            pricing: "EQUILIBRIUM".into(),
            percent_in_range: 50.0,
            atr: 0.0,
        };
    }
    let high_all = bars
        .iter()
        .map(|bar| bar.high)
        .fold(f64::NEG_INFINITY, f64::max);
    let low_all = bars.iter().map(|bar| bar.low).fold(f64::INFINITY, f64::min);
    if n < 25 {
        let close = bars.last().unwrap().close;
        return StructureSnapshot {
            trend: "RANGING".into(),
            bos: None,
            choch: None,
            range_high: high_all,
            range_low: low_all,
            eq: (high_all + low_all) / 2.0,
            pricing: "EQUILIBRIUM".into(),
            percent_in_range: 50.0,
            atr: close * 0.03,
        };
    }

    let mut highs: Vec<(usize, f64)> = Vec::new();
    let mut lows: Vec<(usize, f64)> = Vec::new();
    let window = 3usize;
    for index in window..n - 1 {
        let high = bars[index].high;
        let low = bars[index].low;
        if (1..=window).all(|k| high >= bars[index - k].high)
            && (1..=window.min(n - index - 1)).all(|k| high >= bars[index + k].high)
        {
            highs.push((index, high));
        }
        if (1..=window).all(|k| low <= bars[index - k].low)
            && (1..=window.min(n - index - 1)).all(|k| low <= bars[index + k].low)
        {
            lows.push((index, low));
        }
    }
    if highs.is_empty() {
        highs.push((
            n.saturating_sub(10),
            bars[n.saturating_sub(10)..]
                .iter()
                .map(|b| b.high)
                .fold(f64::NEG_INFINITY, f64::max),
        ));
    }
    if lows.is_empty() {
        lows.push((
            n.saturating_sub(10),
            bars[n.saturating_sub(10)..]
                .iter()
                .map(|b| b.low)
                .fold(f64::INFINITY, f64::min),
        ));
    }
    let last_sh = highs.last().unwrap().1;
    let prev_sh = highs
        .get(highs.len().saturating_sub(2))
        .map(|v| v.1)
        .unwrap_or(last_sh);
    let last_sl = lows.last().unwrap().1;
    let prev_sl = lows
        .get(lows.len().saturating_sub(2))
        .map(|v| v.1)
        .unwrap_or(last_sl);
    let trend = if last_sh >= prev_sh * 0.998 && last_sl >= prev_sl * 0.998 {
        "UPTREND"
    } else if last_sh <= prev_sh * 1.002 && last_sl <= prev_sl * 1.002 {
        "DOWNTREND"
    } else {
        "RANGING"
    };
    let close = bars.last().unwrap().close;
    let (bos, choch) = match trend {
        "UPTREND" if close > last_sh => (Some("BULLISH_BOS"), None),
        "UPTREND" if close < last_sl => (None, Some("BEARISH_CHOCH")),
        "DOWNTREND" if close < last_sl => (Some("BEARISH_BOS"), None),
        "DOWNTREND" if close > last_sh => (None, Some("BULLISH_CHOCH")),
        _ if close > last_sh => (None, Some("BULLISH_CHOCH")),
        _ if close < last_sl => (None, Some("BEARISH_CHOCH")),
        _ => (None, None),
    };
    let start = n.saturating_sub(30);
    let range_high = bars[start..]
        .iter()
        .map(|bar| bar.high)
        .fold(f64::NEG_INFINITY, f64::max);
    let range_low = bars[start..]
        .iter()
        .map(|bar| bar.low)
        .fold(f64::INFINITY, f64::min);
    let range = (range_high - range_low).max(1e-6);
    let percent = ((close - range_low) / range) * 100.0;
    let pricing = if percent <= 45.0 {
        "DISCOUNT"
    } else if percent >= 55.0 {
        "PREMIUM"
    } else {
        "EQUILIBRIUM"
    };
    StructureSnapshot {
        trend: trend.into(),
        bos: bos.map(str::to_string),
        choch: choch.map(str::to_string),
        range_high,
        range_low,
        eq: range_low + range * 0.5,
        pricing: pricing.into(),
        percent_in_range: percent,
        atr: atr(bars),
    }
}

fn asset_name(symbol: &str) -> String {
    symbol
        .strip_suffix(".JK")
        .or_else(|| symbol.strip_suffix("-USD"))
        .unwrap_or(symbol)
        .to_string()
}

/// Build the canonical registered plan. `None` means the latest bar is neutral
/// or the strategy is not supported by this endpoint. Stockbit multibagger
/// uses the same long-only IDX signal engine over daily candles, then receives
/// its distinct identity and CAGR gate in the API layer.
pub fn build_registered_plan(
    bars: &[StrategyBar],
    symbol: &str,
    asset: Option<&str>,
    strategy: &str,
) -> Option<RegisteredPlan> {
    build_registered_plan_with_signal_context(bars, symbol, asset, strategy, None)
}

pub fn build_registered_plan_with_signal_context(
    bars: &[StrategyBar],
    symbol: &str,
    asset: Option<&str>,
    strategy: &str,
    signal_context: Option<PintuSignalTimeframeContext<'_>>,
) -> Option<RegisteredPlan> {
    if bars.is_empty() {
        return None;
    }
    let is_multibagger = matches!(strategy, "multibagger" | "stockbit_multibagger");
    let is_tsmom_swing = matches!(
        strategy,
        "idx_stockbit_time_series_momentum_swing" | "stockbit_tsmom_swing"
    );
    let is_orb_momentum = matches!(
        strategy,
        "idx_stockbit_opening_range_momentum" | "stockbit_orb_momentum"
    );
    let is_first_half_hour_momentum = matches!(
        strategy,
        "idx_stockbit_first_half_hour_momentum" | "stockbit_first_half_hour_momentum"
    );
    let is_time_of_day_momentum = matches!(
        strategy,
        "idx_stockbit_time_of_day_momentum" | "stockbit_time_of_day_momentum"
    );
    let is_stockbit_intraday_research =
        is_orb_momentum || is_first_half_hour_momentum || is_time_of_day_momentum;
    let is_swing = matches!(strategy, "idx_stockbit_swing" | "stockbit_swing") || is_tsmom_swing;
    let is_tokocrypto_spot = is_tokocrypto_spot_confluence_strategy(strategy);
    let is_tokenized_stock = is_tokenized_stock_confluence_strategy(strategy);
    let is_tokocrypto_confluence = is_tokocrypto_spot || is_tokenized_stock;
    let is_pintu_golden_cross = is_pintu_golden_cross_strategy(strategy);
    let pintu_signal_timeframe = signal_context
        .map(|context| context.signal_timeframe)
        .unwrap_or(PINTU_GOLDEN_CROSS_TIMEFRAME);
    let pintu_context_timeframe = signal_context
        .map(|context| context.context_timeframe)
        .unwrap_or(PINTU_GOLDEN_CROSS_CONTEXT_TIMEFRAME);
    let pintu_config = PintuGoldenCrossConfig::default();
    let tokocrypto_config = TokocryptoConfluenceConfig::default();
    let (signal, class, action, is_idx) = match strategy {
        _ if is_tokocrypto_confluence && bars.len() >= 60 => (
            generate_tokocrypto_confluence_signals(bars, tokocrypto_config).pop()?,
            if is_tokenized_stock {
                "TOKENIZED_STOCK"
            } else {
                "CRYPTO_SPOT"
            },
            if is_tokenized_stock {
                "TOKOCRYPTO_TOKENIZED_STOCK"
            } else {
                "TOKOCRYPTO_SPOT"
            },
            false,
        ),
        strategy
            if is_pintu_golden_cross_strategy(strategy)
                && bars.len() >= PINTU_GOLDEN_CROSS_MINIMUM_PLAN_BARS =>
        {
            let mut config = pintu_config;
            let signal = if let Some(context) = signal_context {
                config.signal_interval_ns = context.signal_interval_ns;
                config.context_interval_ns = context.context_interval_ns;
                generate_pintu_golden_cross_signals_with_context(bars, context.context_bars, config)
                    .pop()?
            } else {
                generate_pintu_golden_cross_signals(bars, config).pop()?
            };
            (signal, "CRYPTO_FUT", "PINTU_GOLDEN_CROSS", false)
        }
        "idx_stockbit_day_trade" | "stockbit" | "idx" if bars.len() >= 25 => (
            generate_idx_signals(bars, IdxStrategyConfig::default()).pop()?,
            "IDX_STOCK",
            "STOCKBIT_DAY_TRADE",
            true,
        ),
        "idx_stockbit_swing" | "stockbit_swing" if bars.len() >= 25 => (
            generate_idx_signals(bars, IdxStrategyConfig::swing()).pop()?,
            "IDX_STOCK",
            "STOCKBIT_SWING",
            true,
        ),
        "idx_stockbit_time_series_momentum_swing" | "stockbit_tsmom_swing" if bars.len() >= 253 => {
            (
                generate_idx_time_series_momentum_signals(
                    bars,
                    IdxTimeSeriesMomentumConfig::default(),
                )
                .pop()?,
                "IDX_STOCK",
                "STOCKBIT_TIME_SERIES_MOMENTUM_SWING",
                true,
            )
        }
        "idx_stockbit_opening_range_momentum" | "stockbit_orb_momentum" if bars.len() >= 8 => (
            generate_idx_opening_range_signals(bars, IdxOpeningRangeConfig::default()).pop()?,
            "IDX_STOCK",
            "STOCKBIT_ORB_MOMENTUM",
            true,
        ),
        "idx_stockbit_first_half_hour_momentum" | "stockbit_first_half_hour_momentum"
            if bars.len() >= 8 =>
        {
            (
                generate_idx_first_half_hour_momentum_signals(
                    bars,
                    IdxIntradayMomentumConfig::default(),
                )
                .pop()?,
                "IDX_STOCK",
                "STOCKBIT_FIRST_HALF_HOUR_MOMENTUM",
                true,
            )
        }
        "idx_stockbit_time_of_day_momentum" | "stockbit_time_of_day_momentum"
            if bars.len() >= 60 =>
        {
            (
                generate_idx_time_of_day_momentum_signals(
                    bars,
                    IdxIntradayMomentumConfig::default(),
                )
                .pop()?,
                "IDX_STOCK",
                "STOCKBIT_TIME_OF_DAY_MOMENTUM",
                true,
            )
        }
        "multibagger" | "stockbit_multibagger" if bars.len() >= 25 => (
            generate_idx_signals(bars, IdxStrategyConfig::default()).pop()?,
            "IDX_STOCK",
            "MULTIBAGGER_CANDIDATE",
            true,
        ),
        _ => return None,
    };
    let direction = signal.direction.as_str();
    if direction == "NEUTRAL" || (is_idx && direction != "LONG") {
        return None;
    }
    let close = bars.last()?.close;
    if is_idx && !stockbit_candidate_price_eligible(close) {
        return None;
    }
    let plan_timestamp = bars.last()?.time;
    let asset = asset.filter(|value| !value.is_empty()).unwrap_or("");
    let asset = if asset.is_empty() {
        asset_name(symbol)
    } else {
        asset.to_string()
    };
    let structure = structure_snapshot(bars);
    let (entry, sl, tp1, tp2, tp3, base) = if is_idx {
        let entry = round_idx_tick_at(close, IdxTickRounding::Nearest, plan_timestamp);
        let mut sl = round_idx_tick_at(signal.suggested_sl, IdxTickRounding::Floor, plan_timestamp);
        let mut tp1 =
            round_idx_tick_at(signal.suggested_tp1, IdxTickRounding::Ceil, plan_timestamp);
        let tp2_signal =
            round_idx_tick_at(signal.suggested_tp2, IdxTickRounding::Ceil, plan_timestamp);
        let tick = idx_tick_size(entry) as f64;
        if sl >= entry {
            sl = (entry - tick).max(50.0);
        }
        if tp1 <= entry {
            tp1 = entry + tick;
        }
        if entry <= 0.0 || sl <= 0.0 || tp1 <= entry || sl >= entry {
            return None;
        }
        let risk = (entry - sl).max(entry * 0.005);
        let tp2 = tp2_signal.max(round_idx_tick_at(
            entry + 2.0 * risk,
            IdxTickRounding::Ceil,
            plan_timestamp,
        ));
        let tp3 = round_idx_tick_at(entry + 3.0 * risk, IdxTickRounding::Ceil, plan_timestamp);
        let base = json!({
            "Class": class, "Asset": asset, "Direction": "🟢 LONG", "Action": action,
            "Compute_Engine": "rust_quant_engine",
            "Regime": signal.regime, "DSS_Regime": if is_multibagger { "STOCKBIT_DAILY_MULTIBAGGER" } else if is_tsmom_swing { "STOCKBIT_DAILY_TSMOM_RESEARCH" } else if is_swing { "STOCKBIT_DAILY_SWING" } else if is_orb_momentum { "STOCKBIT_ORB_15M_RESEARCH" } else if is_first_half_hour_momentum { "STOCKBIT_FIRST_HALF_HOUR_15M_RESEARCH" } else if is_time_of_day_momentum { "STOCKBIT_TIME_OF_DAY_15M_RESEARCH" } else { "STOCKBIT_15M_SETUP" },
            "Strategy_ID": if is_multibagger { "multibagger" } else if is_tsmom_swing { "idx_stockbit_time_series_momentum_swing" } else if is_swing { "idx_stockbit_swing" } else if is_orb_momentum { "idx_stockbit_opening_range_momentum" } else if is_first_half_hour_momentum { "idx_stockbit_first_half_hour_momentum" } else if is_time_of_day_momentum { "idx_stockbit_time_of_day_momentum" } else { "idx_stockbit_day_trade" }, "Strategy_Status": "RESEARCH_UNVALIDATED",
            "Strength": signal.strength, "Market_Structure_15m": structure.trend,
            "Structure_15m_BOS": structure.bos, "Structure_15m_CHoCH": structure.choch,
        });
        (entry, sl, tp1, tp2, tp3, base)
    } else {
        let entry = round_price(close);
        let sl = if is_pintu_golden_cross {
            round_price(entry * (1.0 - PintuGoldenCrossConfig::default().stop_loss_pct))
        } else {
            round_price(signal.suggested_sl)
        };
        let tp1 = if is_pintu_golden_cross {
            round_price_up(
                entry + (entry - sl) * PintuGoldenCrossConfig::default().target_r_multiple,
            )
        } else {
            round_price(signal.suggested_tp1)
        };
        let tp2 = if is_pintu_golden_cross {
            round_price_up(
                entry + (entry - sl) * PintuGoldenCrossConfig::default().target_r_multiple * 2.0,
            )
        } else {
            round_price(signal.suggested_tp2)
        };
        if entry <= 0.0 || sl <= 0.0 || tp1 <= 0.0 || tp2 <= 0.0 {
            return None;
        }
        if is_pintu_golden_cross
            && (sl >= entry
                || tp1 <= entry
                || (tp1 - entry) / (entry - sl) + f64::EPSILON < PINTU_GOLDEN_CROSS_MIN_R_MULTIPLE)
        {
            return None;
        }
        let tp3 = if is_pintu_golden_cross {
            round_price_up(
                entry + (entry - sl) * PintuGoldenCrossConfig::default().target_r_multiple * 3.0,
            )
        } else {
            round_price(if direction == "LONG" {
                tp2 + (tp2 - entry)
            } else {
                tp2 - (entry - tp2)
            })
        };
        let icon = if direction == "LONG" { "🟢" } else { "🔴" };
        let structure_15m = if signal.regime.contains("GOLDEN_CROSS_LONG") {
            "BULLISH_MSS"
        } else if signal.regime.contains("BREAKOUT_LONG") {
            "BULLISH_BOS"
        } else if signal.regime.contains("BREAKDOWN_SHORT") {
            "BEARISH_BOS"
        } else {
            "RANGING"
        };
        let base = json!({
            "Class": class, "Asset": asset, "Direction": format!("{icon} {direction}"), "Action": action,
            "Compute_Engine": "rust_quant_engine",
            "Regime": signal.regime,
            "DSS_Regime": if is_tokocrypto_confluence { format!("Tokocrypto confluence: {} of {} directional votes", tokocrypto_config.minimum_agreeing_indicators, crate::tokocrypto_confluence_indicator_ids().len()) } else { format!("{pintu_context_timeframe} DSS below {:.0} within {} bars · {pintu_signal_timeframe} DSS below {:.0} turn · bullish MSS", pintu_config.context_dss_oversold, pintu_config.context_dss_oversold_lookback_bars, pintu_config.max_signal_dss_extreme) },
            "Price": entry, "Current_Price": entry,
            "Strategy_ID": if is_tokocrypto_spot { TOKOCRYPTO_SPOT_CONFLUENCE_STRATEGY_ID } else if is_tokenized_stock { TOKENIZED_STOCK_CONFLUENCE_STRATEGY_ID } else { PINTU_GOLDEN_CROSS_STRATEGY_ID },
            "Strategy_Status": "RESEARCH_UNVALIDATED", "Market_Structure_15m": structure.trend,
            "Structure_15m_BOS": structure.bos, "Structure_15m_CHoCH": structure.choch,
            "Signal_Structure_15m": structure_15m,
        });
        (entry, sl, tp1, tp2, tp3, base)
    };

    let risk = (entry - sl).abs().max(1e-9);
    let tp1_r_multiple = (tp1 - entry).abs() / risk;
    let tp2_r_multiple = (tp2 - entry).abs() / risk;
    let direction_label = format!(
        "{} {}",
        if direction == "LONG" { "🟢" } else { "🔴" },
        direction
    );
    let risk_pct = (sl - entry).abs() / entry * 100.0;
    let mut plan = if is_idx {
        json!({
            "Class": class, "Asset": asset, "Direction": direction_label, "Action": action,
            "Compute_Engine": "rust_quant_engine",
            "Regime": signal.regime, "DSS_Regime": if is_multibagger { "STOCKBIT_DAILY_MULTIBAGGER" } else if is_tsmom_swing { "STOCKBIT_DAILY_TSMOM_RESEARCH" } else if is_swing { "STOCKBIT_DAILY_SWING" } else if is_orb_momentum { "STOCKBIT_ORB_15M_RESEARCH" } else if is_first_half_hour_momentum { "STOCKBIT_FIRST_HALF_HOUR_15M_RESEARCH" } else if is_time_of_day_momentum { "STOCKBIT_TIME_OF_DAY_15M_RESEARCH" } else { "STOCKBIT_15M_SETUP" }, "Strategy_ID": if is_multibagger { "multibagger" } else if is_tsmom_swing { "idx_stockbit_time_series_momentum_swing" } else if is_swing { "idx_stockbit_swing" } else if is_orb_momentum { "idx_stockbit_opening_range_momentum" } else if is_first_half_hour_momentum { "idx_stockbit_first_half_hour_momentum" } else if is_time_of_day_momentum { "idx_stockbit_time_of_day_momentum" } else { "idx_stockbit_day_trade" },
            "Strategy_Status": "RESEARCH_UNVALIDATED", "Is_Optimized_Edge": false,
            "timeframe": if is_multibagger || is_swing { "1d" } else { "15m" }, "is_day_trade": !is_multibagger && !is_swing,
            "Price": entry, "Current_Price": entry, "Price_Time_Zone": format!("{} - {}", format_price(entry), format_price(entry)),
            "Zone_Status": "TRIGGERED", "Time_Window": if is_multibagger { "Stockbit multi-year holding / daily close" } else if is_tsmom_swing { "Stockbit daily 12–1 month momentum · 200-session trend" } else if is_swing { "Stockbit swing — daily close" } else if is_orb_momentum { "Stockbit 15m ORB · after 09:30 WIB · flatten by 12:00 Mon–Thu / 11:30 Fri" } else if is_first_half_hour_momentum { "Stockbit first-half-hour momentum · enter after 09:30 WIB · flatten by morning-session end" } else if is_time_of_day_momentum { "Stockbit same-time-of-day momentum · morning only · flatten by morning-session end" } else { "Stockbit T+0 — exit before regular-session close" }, "Stop_Loss": sl,
            "Risk_%": format!("-{risk_pct:.2}%"), "TP1 (2.0R)": tp1, "TP2 (3.5R)": tp2, "TP3 (Runner)": tp3,
            "TP1_R_Multiple": tp1_r_multiple, "TP2_R_Multiple": tp2_r_multiple,
            "RRR": format!("1:{:.2}", (tp1 - entry) / (entry - sl).max(1e-9)), "Est_Hold": if is_multibagger { "Multi-year compounder candidate" } else if is_tsmom_swing { "Up to 63 daily bars; exit when the momentum/trend regime ends" } else if is_swing { "Several days to weeks" } else if is_stockbit_intraday_research { "Intraday only; next 15m open entry and forced flat by the morning-session end" } else { "Intraday — forced exit before regular-session close" },
            "Invalidation": if is_multibagger { format!("Daily close below {} or complete-month return proof failure", format_price(sl)) } else if is_swing { format!("Daily close below {} or momentum/trend regime exit", format_price(sl)) } else if is_stockbit_intraday_research { format!("15m close below {} or morning-session end", format_price(sl)) } else { format!("15m close below {} or Stockbit session cutoff", format_price(sl)) },
            "Lot_Size_Shares": 100,
            "Position_Sizing_Note": if is_stockbit_intraday_research { "Quantity must be entered in 100-share lots; this research plan does not infer account risk budget or order-book liquidity." } else { "Research sizing only; verify the current Stockbit order ticket." },
            "Catalyst_Thesis": if is_multibagger { "Stockbit multibagger candidate: daily IDX structure. Goal: validated fee-adjusted, risk-sized account return ≥100% in one complete calendar month; research status: unvalidated." } else if is_tsmom_swing { "Stockbit 12–1 month time-series momentum with a 200-session trend filter and ATR-based brackets. Swing goal: validated annualized CAGR ≥100%; research only, not validated for Stockbit execution." } else if is_swing { "Stockbit swing: daily IDX open-low or relative-volume confluence with wider multi-day levels. Goal: validated annualized CAGR ≥100%; research status: unvalidated." } else if is_orb_momentum { "Research candidate based on opening-range breakout and early-session momentum studies: positive first 30m return, breakout above range, and turnover/volume confirmation. Goal: ≥50% untouched-holdout win rate, ≥1.20 untouched-holdout profit factor, and ≥200% annualized untouched-holdout CAGR; execution remains unvalidated." } else if is_first_half_hour_momentum { "Research candidate based on early-session momentum: positive first-30m return with volume and turnover confirmation; entry follows the first half-hour close. Goal: ≥50% untouched-holdout win rate, ≥1.20 untouched-holdout profit factor, and ≥200% annualized untouched-holdout CAGR; execution remains unvalidated." } else if is_time_of_day_momentum { "Research candidate based on recurring intraday return patterns: positive historical mean and current return for the same 15m morning slot. Requires sufficient intraday history. Goal: ≥50% untouched-holdout win rate, ≥1.20 untouched-holdout profit factor, and ≥200% annualized untouched-holdout CAGR; execution remains unvalidated." } else { "Stockbit T+0: Open=Low/RVOL confluence with IDX tick-rounded levels. Goal: ≥50% untouched-holdout win rate, ≥1.20 untouched-holdout profit factor, and ≥200% annualized untouched-holdout CAGR; research status: unvalidated." }
        })
    } else {
        json!({
            "Class": class, "Asset": asset, "Direction": direction_label, "Action": action,
            "Compute_Engine": "rust_quant_engine",
            "Regime": signal.regime,
            "DSS_Regime": if is_tokocrypto_confluence { format!("Tokocrypto confluence: {} of {} directional votes", tokocrypto_config.minimum_agreeing_indicators, crate::tokocrypto_confluence_indicator_ids().len()) } else { "1h DSS <22 within 24h · 15m DSS <5 turn · bullish MSS".to_string() },
            "Price": entry, "Current_Price": entry, "Entry_Price": entry,
            "Strategy_ID": if is_tokocrypto_spot { TOKOCRYPTO_SPOT_CONFLUENCE_STRATEGY_ID } else if is_tokenized_stock { TOKENIZED_STOCK_CONFLUENCE_STRATEGY_ID } else { PINTU_GOLDEN_CROSS_STRATEGY_ID },
            "Strategy_Status": "RESEARCH_UNVALIDATED", "Is_Optimized_Edge": false,
            "timeframe": pintu_signal_timeframe, "is_day_trade": true,
            "Price_Time_Zone": format!("{} - {}", format_price(entry), format_price(entry)), "Zone_Status": "TRIGGERED",
            "Time_Window": if is_tokocrypto_spot { "Tokocrypto spot — 15m research signal" } else if is_tokenized_stock { "Tokocrypto tokenized stock — 15m proxy research signal" } else { "Pintu perpetual — no fixed expiry" },
            "Stop_Loss": sl, "Risk_%": format!("-{risk_pct:.2}%"),
            "TP1": tp1, "TP2": tp2, "TP3 (Runner)": tp3,
            "Take_Profit_1": tp1, "Take_Profit_2": tp2,
            "Risk_Reward": tp1_r_multiple,
            "TP1_R_Multiple": tp1_r_multiple, "TP2_R_Multiple": tp2_r_multiple,
            "TP3_R_Multiple": (tp3 - entry).abs() / risk,
            "RRR": format!("1:{tp1_r_multiple:.2}"),
            "Minimum_Reward_Risk_Multiple": if is_pintu_golden_cross { json!(PINTU_GOLDEN_CROSS_MIN_R_MULTIPLE) } else { Value::Null },
            "Risk_Reward_Basis": if is_pintu_golden_cross { "Gross price-distance before fees and slippage; realized net R can be lower." } else { "Price-distance from entry to target versus stop risk." },
            "Est_Hold": "Signal exit / no fixed expiry", "Invalidation": format!("{pintu_signal_timeframe} close through {}", format_price(sl)),
            "Catalyst_Thesis": if is_tokocrypto_spot {
                format!("Tokocrypto crypto spot, long-only: at least {} of {} trend, momentum, breakout, and relative-volume votes align. Relative volume threshold {:.1}×; {:.2}% target and {:.2}% stop. Research only; unvalidated; stored proxy candles are not venue-native execution evidence.", tokocrypto_config.minimum_agreeing_indicators, crate::tokocrypto_confluence_indicator_ids().len(), tokocrypto_config.indicator_config.relative_volume_min, tokocrypto_config.target_pct * 100.0, tokocrypto_config.stop_loss_pct * 100.0)
            } else if is_tokenized_stock {
                format!("Tokocrypto tokenized stock, long-only: at least {} of {} trend, momentum, breakout, and relative-volume votes align. Relative volume threshold {:.1}×; {:.2}% target and {:.2}% stop. Research only; unvalidated; underlying equity candles are proxies, not token execution evidence.", tokocrypto_config.minimum_agreeing_indicators, crate::tokocrypto_confluence_indicator_ids().len(), tokocrypto_config.indicator_config.relative_volume_min, tokocrypto_config.target_pct * 100.0, tokocrypto_config.stop_loss_pct * 100.0)
            } else {
                format!("Pintu Pro Futures EMA 20/50 golden cross on a closed {pintu_signal_timeframe} candle. Require at least one of the latest {} completed {pintu_context_timeframe} DSS Bressert readings to be below its configured oversold level ({:.0}); {pintu_signal_timeframe} DSS must be rising on the cross candle and have printed below {:.0} on that candle or one of the prior {} closed candles; and a fresh bullish {pintu_signal_timeframe} market-structure shift. The shift uses confirmed five-candle swings with lower highs and lower lows, followed by a close above the latest lower swing high on the cross candle or one of the prior {} candles; price must remain above that level. Static {:.1}% stop; TP1 at least {:.0}R gross price-distance before fees and slippage. Research only; unvalidated.", pintu_config.context_dss_oversold_lookback_bars, pintu_config.context_dss_oversold, pintu_config.max_signal_dss_extreme, pintu_config.dss_extreme_max_age_bars - 1, pintu_config.structure_max_age_bars, pintu_config.stop_loss_pct * 100.0, pintu_config.target_r_multiple)
            }
        })
    };
    plan["Signal_Bar_Time"] = json!(plan_timestamp);
    plan["Signal_Strength"] = json!(signal.strength);
    plan["Strategy_Strength"] = json!(signal.strategy_strength);
    if let Some(confluence) = &signal.confluence {
        plan["Signal_Confluence_Score"] = json!(confluence.score);
        plan["Signal_Confluence"] = json!(confluence);
        plan["Signal_Scoring_Model_ID"] =
            json!(crate::strategies::SIGNAL_CONFLUENCE_SCORING_MODEL_ID);
    }
    if !is_idx {
        let tp1_label = if is_tokocrypto_confluence {
            "TP1 (configured target)"
        } else if is_pintu_golden_cross {
            "TP1 (3.0R)"
        } else {
            "TP1 (2.0R)"
        };
        let tp2_label = if is_tokocrypto_confluence {
            "TP2 (configured target)"
        } else if is_pintu_golden_cross {
            "TP2 (6.0R)"
        } else {
            "TP2 (3.5R)"
        };
        plan[tp1_label] = json!(tp1);
        plan[tp2_label] = json!(tp2);
    }
    Some(RegisteredPlan {
        base,
        plan,
        signal,
        structure,
    })
}

#[cfg(test)]
#[path = "plans_tests.rs"]
mod tests;
