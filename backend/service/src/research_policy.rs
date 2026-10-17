use super::*;

#[derive(Clone, Copy)]
pub(super) struct StrategyInventionVariant {
    id: &'static str,
    label: &'static str,
    rationale: &'static str,
}

pub(super) const STOCKBIT_STRATEGY_INVENTION_VARIANTS: &[StrategyInventionVariant] = &[
    StrategyInventionVariant {
        id: "idx_stockbit_quality_day_trade_v1",
        label: "IDX Quality Breakout v1",
        rationale: "Adds close-above-trend and prior-high confirmation while tightening volume and risk levels.",
    },
    StrategyInventionVariant {
        id: "idx_stockbit_pullback_day_trade_v1",
        label: "IDX Pullback Reclaim v1",
        rationale: "Uses the same quality confirmation with a smaller target and tighter stop for pullback entries.",
    },
    StrategyInventionVariant {
        id: "idx_stockbit_momentum_day_trade_v1",
        label: "IDX Momentum Breakout v1",
        rationale: "Requires stronger relative volume and more room to the IDX rejection ceiling before entry.",
    },
];

pub(super) const PINTU_STRATEGY_INVENTION_VARIANTS: &[StrategyInventionVariant] = &[
    StrategyInventionVariant {
        id: "pintu_futures_quality_day_trade_v1",
        label: "Futures Volume Breakout v1",
        rationale: "Requires stronger relative volume with a tighter futures risk envelope.",
    },
    StrategyInventionVariant {
        id: "pintu_futures_confluence_quality_day_trade_v1",
        label: "Confluence Quality v1",
        rationale: "Raises volume confirmation and tightens the EMA/RSI breakout response.",
    },
];

pub(super) const TOKOCRYPTO_STRATEGY_INVENTION_VARIANTS: &[StrategyInventionVariant] =
    &[StrategyInventionVariant {
        id: "tokenized_stock_quality_day_trade_v1",
        label: "Tokenized Stock Quality v1",
        rationale: "Uses stronger confluence confirmation for the tokenized-stock research proxy.",
    }];

pub(super) const TOKOCRYPTO_SPOT_STRATEGY_INVENTION_VARIANTS: &[StrategyInventionVariant] =
    &[StrategyInventionVariant {
        id: "tokocrypto_spot_quality_day_trade_v1",
        label: "Crypto Spot Quality v1",
        rationale: "Uses stronger confluence confirmation for the crypto-spot research proxy.",
    }];

pub(super) fn strategy_invention_variants(strategy: &str) -> &'static [StrategyInventionVariant] {
    let Some(profile) = historical_evidence_profile(strategy) else {
        return &[];
    };
    let strategy_id = profile
        .strategy_id
        .strip_suffix(quant_core::REQUIRED_SIGNAL_CONFIRMATION_EVIDENCE_SUFFIX)
        .unwrap_or(profile.strategy_id);
    if strategy_id == PINTU_GOLDEN_CROSS_STRATEGY_ID {
        return PINTU_STRATEGY_INVENTION_VARIANTS;
    }
    match strategy_id {
        "stockbit_day_trade" => STOCKBIT_STRATEGY_INVENTION_VARIANTS,
        "pintu_futures_day_trade" => PINTU_STRATEGY_INVENTION_VARIANTS,
        "tokocrypto_spot_day_trade" => TOKOCRYPTO_SPOT_STRATEGY_INVENTION_VARIANTS,
        "tokenized_stock_day_trade" => TOKOCRYPTO_STRATEGY_INVENTION_VARIANTS,
        _ => &[],
    }
}

pub(super) fn strategy_invention_score(stats: &Value) -> f64 {
    let win_rate = finite_json_number(stats.get("win_rate_pct")).unwrap_or(0.0);
    let profit_factor = finite_json_number(stats.get("profit_factor")).unwrap_or(0.0);
    let reward_risk = finite_json_number(stats.get("observed_reward_risk_ratio")).unwrap_or(0.0);
    let trades = stats.get("trades").and_then(Value::as_u64).unwrap_or(0) as f64;
    let mut score = win_rate.clamp(0.0, 100.0) * 0.45
        + profit_factor.clamp(0.0, 3.0) * 12.0
        + trades.min(30.0) * 1.2
        + reward_risk.clamp(0.0, 3.0) * 4.0;
    // A one- or two-trade outlier must not win the lab just because its
    // observed percentage is high.
    if trades < 3.0 {
        score *= 0.25;
    }
    score.clamp(0.0, 100.0)
}

pub(super) fn strategy_invention_promotion_status(
    stats: &Value,
    minimum_cagr_pct: f64,
) -> &'static str {
    let scope = stats
        .get("execution_scope")
        .and_then(Value::as_str)
        .unwrap_or("");
    if !scope.eq_ignore_ascii_case("venue_history") {
        return "VENUE_HISTORY_REQUIRED";
    }
    let win_rate = finite_json_number(stats.get("holdout_win_rate_pct")).unwrap_or(0.0);
    let required_win_rate = finite_json_number(stats.get("minimum_win_rate_pct")).unwrap_or(50.0);
    let profit_factor = finite_json_number(stats.get("holdout_profit_factor")).unwrap_or(0.0);
    let trades = stats
        .get("holdout_trades")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let sample_days = stats
        .get("holdout_sample_days")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let cagr_validated = has_validated_holdout_cagr(stats);
    let cagr_meets_minimum = minimum_cagr_pct <= 0.0
        || finite_json_number(stats.get("holdout_cagr_pct"))
            .is_some_and(|cagr| cagr >= minimum_cagr_pct);
    if win_rate >= required_win_rate
        && profit_factor >= MIN_DAY_TRADE_PROFIT_FACTOR
        && trades >= MIN_EVIDENCE_SAMPLE_TRADES as u64
        && sample_days >= MIN_EVIDENCE_SAMPLE_DAYS
        && cagr_validated
        && cagr_meets_minimum
    {
        "VALIDATED_CANDIDATE"
    } else {
        "VALIDATION_PENDING"
    }
}

/// Test a bounded set of deterministic alternatives after a low-win-rate
/// result. This is research discovery, not automatic strategy promotion.
pub(super) fn build_strategy_invention_report(
    bars: &[StrategyBar],
    strategy: &str,
    baseline: Option<&Value>,
) -> Option<Value> {
    let variants = strategy_invention_variants(strategy);
    if variants.is_empty() {
        return None;
    }

    let baseline_win_rate =
        baseline.and_then(|value| finite_json_number(value.get("win_rate_pct")));
    let baseline_trades = baseline
        .and_then(|value| value.get("trades"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let baseline_summary = serde_json::json!({
        "strategy_id": strategy,
        "win_rate_pct": baseline_win_rate,
        "trades": baseline_trades,
    });
    let minimum_cagr_pct = strategy_min_cagr_pct(strategy);

    let Some(baseline_win_rate) = baseline_win_rate.filter(|value| value.is_finite()) else {
        return Some(serde_json::json!({
            "status": "NO_BASELINE",
            "trigger": "BASELINE_EVIDENCE_UNAVAILABLE",
            "baseline": baseline_summary,
            "variants_tested": 0,
            "candidates": [],
            "selected": null,
            "execution_status": "RESEARCH_ONLY",
        }));
    };
    if baseline_trades == 0 {
        return Some(serde_json::json!({
            "status": "NO_BASELINE",
            "trigger": "BASELINE_HAS_NO_COMPLETED_TRADES",
            "baseline": baseline_summary,
            "variants_tested": 0,
            "candidates": [],
            "selected": null,
            "execution_status": "RESEARCH_ONLY",
        }));
    }
    if baseline_win_rate >= MIN_DAY_TRADE_WIN_RATE_PCT {
        return Some(serde_json::json!({
            "status": "NOT_TRIGGERED",
            "trigger": "BASELINE_MEETS_WIN_RATE_FLOOR",
            "baseline": baseline_summary,
            "trigger_threshold_pct": MIN_DAY_TRADE_WIN_RATE_PCT,
            "variants_tested": 0,
            "candidates": [],
            "selected": null,
            "execution_status": "RESEARCH_ONLY",
        }));
    }

    let evidence_profile = historical_evidence_profile(strategy)?;
    let baseline_score = baseline.map(strategy_invention_score).unwrap_or(0.0);
    let mut candidates = variants
        .iter()
        .map(|variant| {
            let mut stats = evaluate_historical_evidence_with_fees(
                bars,
                variant.id,
                evidence_profile.max_hold_bars,
                evidence_profile.buy_fee_pct,
                evidence_profile.sell_fee_pct,
            );
            if let (Some(base), Some(object)) = (baseline, stats.as_object_mut()) {
                for key in [
                    "evidence_source",
                    "data_source",
                    "source_label",
                    "instrument_kind",
                    "execution_scope",
                    "fee_model",
                    "history_interval",
                    "validation_basis",
                ] {
                    if let Some(value) = base.get(key) {
                        object.insert(key.to_string(), value.clone());
                    }
                }
                object.insert(
                    "strategy_variant_id".to_string(),
                    serde_json::json!(variant.id),
                );
                object.insert(
                    "strategy_variant_label".to_string(),
                    serde_json::json!(variant.label),
                );
            }
            let score = strategy_invention_score(&stats);
            let improves_baseline = score > baseline_score + 0.01;
            let promotion_status = if improves_baseline {
                strategy_invention_promotion_status(&stats, minimum_cagr_pct)
            } else {
                "NO_IMPROVEMENT"
            };
            serde_json::json!({
                "strategy_id": variant.id,
                "label": variant.label,
                "rationale": variant.rationale,
                "minimum_cagr_pct": minimum_cagr_pct,
                "score": score,
                "improves_baseline": improves_baseline,
                "promotion_status": promotion_status,
                "execution_status": "RESEARCH_ONLY",
                "backtest_stats": stats,
            })
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        let left_score = finite_json_number(left.get("score")).unwrap_or(0.0);
        let right_score = finite_json_number(right.get("score")).unwrap_or(0.0);
        right_score
            .partial_cmp(&left_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let selected = candidates.first().cloned().unwrap_or(Value::Null);
    let selected_score = finite_json_number(selected.get("score")).unwrap_or(0.0);
    let improvement_status = if selected_score > baseline_score + 0.01 {
        "IMPROVED_BASELINE"
    } else {
        "NO_IMPROVEMENT"
    };

    Some(serde_json::json!({
        "status": "LOW_WR_TRIGGERED",
        "trigger": "BASELINE_BELOW_WIN_RATE_FLOOR",
        "baseline": baseline_summary,
        "baseline_score": baseline_score,
        "trigger_threshold_pct": MIN_DAY_TRADE_WIN_RATE_PCT,
        "variants_tested": candidates.len(),
        "selected": selected,
        "selected_score": selected_score,
        "improvement_status": improvement_status,
        "candidates": candidates,
        "selection_method": "bounded chronological backtests ranked by observed win rate, profit factor, reward/risk, and trade count, then compared with the registered baseline",
        "promotion_rule": "A candidate must improve the registered baseline and remain research-only until the existing 730-day, 10-trade, win-rate, profit-factor, CAGR, and venue-history gates pass.",
        "execution_status": "RESEARCH_ONLY",
    }))
}

pub(super) fn attach_strategy_invention(
    plan: &mut Value,
    bars: &[StrategyBar],
    strategy: &str,
    baseline: Option<&Value>,
) {
    if let Some(report) = build_strategy_invention_report(bars, strategy, baseline) {
        if let Some(object) = plan.as_object_mut() {
            object.insert("Strategy_Invention".to_string(), report);
        }
    }
}

pub(super) fn normalize_backtest_symbol(value: &str) -> String {
    value
        .trim()
        .to_uppercase()
        .replace(".JK", "")
        .replace("_FUT", "")
        .replace("_SPOT", "")
        .replace("-USD", "")
        .replace("USDT-PERP", "")
}

pub(super) fn normalize_backtest_class(value: &str) -> String {
    match value.trim().to_lowercase().as_str() {
        "idx" | "idx_stock" | "stockbit" => "idx".to_string(),
        "crypto_futures" | "crypto_fut" | "pintu_futures" | "pintu" => "crypto_futures".to_string(),
        "crypto_spot" | "tokenized_stock" | "tokocrypto" => "crypto_spot".to_string(),
        other => other.to_string(),
    }
}

pub(super) fn backtest_key(symbol: &str, class: &str) -> String {
    format!(
        "{}|{}",
        normalize_backtest_symbol(symbol),
        normalize_backtest_class(class)
    )
}

pub(super) fn normalize_backtest_strategy(strategy: &str) -> String {
    match strategy.trim().to_ascii_lowercase().as_str() {
        "stockbit" | "idx" => "idx_stockbit_day_trade".to_string(),
        "stockbit_swing" => "idx_stockbit_swing".to_string(),
        "stockbit_tsmom_swing" => "idx_stockbit_time_series_momentum_swing".to_string(),
        "stockbit_orb_momentum" => "idx_stockbit_opening_range_momentum".to_string(),
        "stockbit_first_half_hour_momentum" => "idx_stockbit_first_half_hour_momentum".to_string(),
        "stockbit_time_of_day_momentum" => "idx_stockbit_time_of_day_momentum".to_string(),
        "pintu_confluence" | "pintu_futures_confluence" => {
            PINTU_GOLDEN_CROSS_V2_EVIDENCE_ID.to_string()
        }
        "tokocrypto_spot" => quant_core::TOKOCRYPTO_SPOT_CONFLUENCE_STRATEGY_ID.to_string(),
        "tokenized_stock" => quant_core::TOKENIZED_STOCK_CONFLUENCE_STRATEGY_ID.to_string(),
        "stockbit_multibagger" => "multibagger".to_string(),
        normalized => normalized.to_string(),
    }
}

pub(super) fn strategy_backtest_key(symbol: &str, class: &str, strategy: &str) -> String {
    let canonical_strategy = crate::master_data::profile_for_strategy(strategy)
        .and_then(|profile| crate::master_data::catalog_string(profile, "evidence_strategy_id"))
        .or_else(|| {
            quant_core::historical_evidence_profile(strategy).map(|profile| profile.strategy_id)
        })
        .unwrap_or(strategy);
    format!(
        "{}|{}",
        backtest_key(symbol, class),
        normalize_backtest_strategy(canonical_strategy)
    )
}

pub(super) fn plan_backtest_class(plan: &Value) -> &'static str {
    let class = plan.get("Class").and_then(Value::as_str).unwrap_or("");
    let exchange = plan.get("Exchange").and_then(Value::as_str).unwrap_or("");
    let strategy = plan
        .get("Strategy_ID")
        .and_then(Value::as_str)
        .unwrap_or("");
    if class.eq_ignore_ascii_case("IDX_STOCK") || exchange.eq_ignore_ascii_case("STOCKBIT") {
        "idx"
    } else if class.eq_ignore_ascii_case("TOKENIZED_STOCK")
        || exchange.eq_ignore_ascii_case("TOKOCRYPTO")
        || strategy.eq_ignore_ascii_case("tokenized_stock_day_trade")
    {
        "crypto_spot"
    } else {
        "crypto_futures"
    }
}

pub(super) fn plan_backtest_symbol(plan: &Value) -> String {
    plan.get("Symbol")
        .and_then(Value::as_str)
        .filter(|symbol| !symbol.trim().is_empty())
        .or_else(|| plan.get("Asset").and_then(Value::as_str))
        .unwrap_or("")
        .to_string()
}

pub(super) fn plan_min_cagr_pct(plan: &Value) -> f64 {
    let strategy = plan
        .get("Strategy_ID")
        .and_then(Value::as_str)
        .unwrap_or("");
    let style = plan
        .get("Trade_Style")
        .or_else(|| plan.get("trade_style"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let class = plan
        .get("Class")
        .or_else(|| plan.get("class"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let exchange = plan
        .get("Exchange")
        .or_else(|| plan.get("exchange"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let is_stockbit_day_trade = strategy_min_cagr_pct(strategy) > 0.0
        || ((class.eq_ignore_ascii_case("IDX_STOCK")
            || class.eq_ignore_ascii_case("IDX")
            || exchange.eq_ignore_ascii_case("STOCKBIT"))
            && (style.eq_ignore_ascii_case("day_trade")
                || plan
                    .get("is_day_trade")
                    .or_else(|| plan.get("Is_Day_Trade"))
                    .and_then(Value::as_bool)
                    == Some(true)));
    if strategy.eq_ignore_ascii_case("multibagger") {
        0.0
    } else if style.eq_ignore_ascii_case("swing") || strategy.to_ascii_lowercase().contains("swing")
    {
        MIN_SWING_CAGR_PCT
    } else if is_stockbit_day_trade {
        MIN_STOCKBIT_DAY_TRADE_CAGR_PCT
    } else {
        0.0
    }
}

pub(super) fn strategy_min_cagr_pct(strategy: &str) -> f64 {
    historical_evidence_minimum_cagr_pct(strategy)
}

pub(super) fn plan_min_calendar_month_return_pct(plan: &Value) -> f64 {
    let strategy = plan
        .get("Strategy_ID")
        .or_else(|| plan.get("strategy_id"))
        .and_then(Value::as_str)
        .unwrap_or("");
    historical_evidence_minimum_calendar_month_return_pct(strategy)
}

pub(super) fn plan_number(plan: &Value, keys: &[&str]) -> Option<f64> {
    keys.iter().find_map(|key| {
        plan.get(*key)
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite() && *value > 0.0)
    })
}

pub(super) fn plan_zone_timeframe(plan: &Value) -> String {
    ["Zone_Timeframe", "Entry_Timeframe", "timeframe"]
        .iter()
        .find_map(|key| {
            plan.get(*key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| value.to_ascii_lowercase())
        })
        .unwrap_or_else(|| "unknown".to_string())
}

pub(super) fn plan_price_zone(plan: &Value) -> Option<(f64, f64)> {
    if let Some(raw_zone) = plan.get("Price_Time_Zone").and_then(Value::as_str) {
        let mut values = raw_zone.split('-').filter_map(|part| {
            part.trim()
                .replace(',', "")
                .parse::<f64>()
                .ok()
                .filter(|value| value.is_finite() && *value > 0.0)
        });
        if let (Some(first), Some(second)) = (values.next(), values.next()) {
            return Some((first.min(second), first.max(second)));
        }
    }

    let low = plan_number(plan, &["Entry_Zone_Low", "Zone_Min", "zone_min"]);
    let high = plan_number(plan, &["Entry_Zone_High", "Zone_Max", "zone_max"]);
    match (low, high) {
        (Some(first), Some(second)) => Some((first.min(second), first.max(second))),
        _ => plan_number(plan, &["Entry_Price", "Price"]).map(|entry| (entry, entry)),
    }
}

/// Keep market location separate from execution readiness. A price can be in
/// the zone while the evidence gate still blocks an execution action.
pub(super) fn plan_price_position(plan: &Value) -> &'static str {
    let is_watchlist = plan
        .get("Is_Watchlist")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let no_current_signal = ["Strategy_Status", "Regime"].iter().any(|key| {
        plan.get(*key)
            .and_then(Value::as_str)
            .is_some_and(|value| value.eq_ignore_ascii_case("WATCHLIST_NO_CURRENT_SIGNAL"))
    });
    if is_watchlist && no_current_signal {
        return "REFERENCE_ONLY";
    }

    let Some(current) = plan_number(plan, &["Current_Price", "Price", "Entry_Price"]) else {
        return "UNAVAILABLE";
    };
    let Some((low, high)) = plan_price_zone(plan) else {
        return "UNAVAILABLE";
    };
    if current >= low - 1e-9 && current <= high + 1e-9 {
        "IN_ZONE"
    } else {
        "OUTSIDE_ZONE"
    }
}

pub(super) fn annotate_plan_decision_state(plan: &mut Value, validated: bool) {
    let price_position = plan_price_position(plan);
    let zone_timeframe = plan_zone_timeframe(plan);
    let state = plan.get("State").and_then(Value::as_str).unwrap_or("");
    let no_current_signal = ["Strategy_Status", "Regime"].iter().any(|key| {
        plan.get(*key)
            .and_then(Value::as_str)
            .is_some_and(|value| value.eq_ignore_ascii_case("WATCHLIST_NO_CURRENT_SIGNAL"))
    });
    let execution_status = if no_current_signal {
        "MONITOR_ONLY"
    } else if !validated {
        "VALIDATION_PENDING"
    } else if state.eq_ignore_ascii_case("RUNNING") || state.eq_ignore_ascii_case("IN_ZONE") {
        "READY_FOR_REVIEW"
    } else {
        "ENTRY_PENDING"
    };

    if let Some(object) = plan.as_object_mut() {
        object.insert(
            "Price_Position".to_string(),
            serde_json::json!(price_position),
        );
        object.insert(
            "Execution_Status".to_string(),
            serde_json::json!(execution_status),
        );
        object.insert(
            "Zone_Timeframe".to_string(),
            serde_json::json!(zone_timeframe),
        );
    }
}

pub(super) fn historical_evidence_class(strategy: &str) -> Option<&'static str> {
    historical_evidence_profile(strategy).map(|profile| profile.evidence_class)
}
/// Calculate and label evidence from the durable local market store.
///
/// This intentionally does not call a provider. Provider sync belongs to the
/// ingestion path; evidence and charts must remain readable when a provider
/// is down. A yfinance-backed row is useful observed research evidence, but
/// its proxy scope is recorded so it cannot be mistaken for venue execution
/// history.
pub(super) fn local_historical_evidence(
    connection: &Connection,
    symbol: &str,
    strategy: &str,
    exchange: Option<&str>,
) -> Result<Option<Value>, rusqlite::Error> {
    let Some(profile) = historical_evidence_profile(strategy) else {
        return Ok(None);
    };
    let timeframe = profile.history_interval;
    let source = selected_candle_source_for_timeframe(connection, symbol, exchange, timeframe)?
        .unwrap_or_else(|| "unknown".to_string());
    let candles = if timeframe == "1d" {
        // Long-horizon research must use the stored daily series directly.
        // Chart reads may merge a newer partial day from lower intervals, but
        // that rollup must never replace the daily evidence window.
        load_candles_from_source(connection, symbol, timeframe, MAX_EVIDENCE_CANDLES, &source)?
    } else {
        load_series_candles_from_source(
            connection,
            symbol,
            timeframe,
            MAX_EVIDENCE_CANDLES,
            &source,
        )?
    };
    if candles.len() < 2 {
        return Ok(None);
    }
    let bars = candles
        .iter()
        .map(|candle| StrategyBar {
            time: candle.time,
            open: candle.open,
            high: candle.high,
            low: candle.low,
            close: candle.close,
            volume: candle.volume,
        })
        .collect::<Vec<_>>();
    let mut stats = evaluate_historical_evidence_with_fees(
        &bars,
        profile.strategy_id,
        profile.max_hold_bars,
        profile.buy_fee_pct,
        profile.sell_fee_pct,
    );

    let provenance = connection
        .query_row(
            "SELECT source, instrument_kind FROM series WHERE symbol=?1 AND interval=?2 AND source=?3",
            params![symbol, timeframe, source],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;
    let (data_source, instrument_kind) =
        provenance.unwrap_or_else(|| ("sqlite_local_history".to_string(), "unknown".to_string()));
    let is_proxy = data_source.eq_ignore_ascii_case("yfinance")
        || is_research_proxy_source(&data_source)
        || instrument_kind.contains("proxy");
    let is_native = is_native_candle_source_for_exchange(&data_source, exchange);
    let source_label = if instrument_kind.contains("tokenized_stock") && is_proxy {
        "yfinance fallback proxy · Tokocrypto tokenized-stock identity".to_string()
    } else if is_proxy {
        candle_source_label(&data_source)
    } else if data_source == "tokocrypto_native" || data_source == "tokocrypto" {
        "Tokocrypto native history".to_string()
    } else if data_source == "pintu_futures_native" || data_source == "pintu_futures" {
        "Pintu native history".to_string()
    } else if data_source == "stockbit_native" || data_source == "stockbit" {
        "Stockbit native history".to_string()
    } else if data_source == "pluang_native" || data_source == "pluang" {
        "Pluang native history".to_string()
    } else {
        "local stored history".to_string()
    };

    if let Some(object) = stats.as_object_mut() {
        object.insert(
            "strategy_id".to_string(),
            serde_json::json!(profile.strategy_id),
        );
        object.insert(
            "evidence_source".to_string(),
            serde_json::json!("sqlite_local_history"),
        );
        object.insert("data_source".to_string(), serde_json::json!(data_source));
        object.insert("source_label".to_string(), serde_json::json!(source_label));
        object.insert(
            "instrument_kind".to_string(),
            serde_json::json!(instrument_kind),
        );
        object.insert("history_interval".to_string(), serde_json::json!(timeframe));
        object.insert(
            "max_hold_bars".to_string(),
            serde_json::json!(profile.max_hold_bars),
        );
        object.insert(
            "fee_model".to_string(),
            serde_json::json!(profile.fee_model),
        );
        object.insert(
            "execution_scope".to_string(),
            serde_json::json!(if is_proxy {
                "research_proxy"
            } else if is_native {
                "venue_history"
            } else {
                "unverified_history"
            }),
        );
        object.insert(
            "validation_basis".to_string(),
            serde_json::json!("same Rust strategy generator with required DSS Bressert and non-repainting Nadaraya confirmations over chronological stored OHLCV"),
        );
    }
    Ok(Some(stats))
}

pub(super) fn merge_local_historical_evidence(
    connection: &Connection,
    stats: &mut HashMap<String, Value>,
    symbol: &str,
    strategy: &str,
    exchange: Option<&str>,
) -> Result<(), rusqlite::Error> {
    let Some(class) = historical_evidence_class(strategy) else {
        return Ok(());
    };
    if let Some(evidence) = local_historical_evidence(connection, symbol, strategy, exchange)? {
        stats.insert(strategy_backtest_key(symbol, class, strategy), evidence);
    }
    Ok(())
}
