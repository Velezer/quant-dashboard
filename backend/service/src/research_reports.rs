use super::*;

mod entry_validation;
pub(super) use entry_validation::{entry_validation_list, entry_validation_saved, entry_validation_timeframe, run_entry_validation};

pub(super) fn build_nwe_dss_research_report(
    market_database: &str,
    selection: ResearchSelection,
    symbols: &[String],
) -> Result<Value, rusqlite::Error> {
    let timeframe = selection.timeframe;
    let fee_policy =
        quant_core::venue_fee_policy_for_category(selection.exchange, selection.asset_category_id);
    let (buy_fee_pct, sell_fee_pct, fees_known, fee_model) = match fee_policy {
        quant_core::VenueFeePolicy::Known {
            buy_fee_pct,
            sell_fee_pct,
            fee_model,
            ..
        } => (buy_fee_pct, sell_fee_pct, true, fee_model),
        quant_core::VenueFeePolicy::Unknown { fee_model, .. }
        | quant_core::VenueFeePolicy::Unsupported { fee_model, .. } => (0.0, 0.0, false, fee_model),
    };
    let costs_complete = fees_known && selection.exchange == "stockbit";
    let allow_short = selection.asset_category_id == "crypto_perpetual";
    let mut rows = Vec::with_capacity(symbols.len());
    for symbol in symbols.iter().take(MAX_RESEARCH_SYMBOLS) {
        let row = match open_market_history_for_symbol(market_database, symbol, false) {
            Ok(Some(connection)) => nwe_dss_research_row(
                &connection,
                selection,
                symbol,
                buy_fee_pct,
                sell_fee_pct,
                fees_known,
                costs_complete,
                allow_short,
            )?,
            Ok(None) => nwe_dss_missing_row(selection, symbol, fee_model),
            Err(error) => nwe_dss_unavailable_row(selection, symbol, fee_model, &error.to_string()),
        };
        rows.push(row);
    }
    let validated_candidates = rows
        .iter()
        .filter(|row| {
            row.pointer("/backtest/status").and_then(Value::as_str) == Some("VALIDATED_CANDIDATE")
        })
        .count();
    let current_rows = rows
        .iter()
        .filter(|row| row.get("freshness").and_then(Value::as_str) == Some("CURRENT"))
        .count();
    Ok(serde_json::json!({
        "status": "ok",
        "service": "rust_quant_engine",
        "version": SERVICE_VERSION,
        "selection": {
            "exchange": selection.exchange,
            "exchange_label": selection.exchange_label,
            "asset_class": selection.asset_class,
            "asset_category_id": selection.asset_category_id,
            "asset_category_label": selection.asset_category_label,
            "strategy_id": "nwe_dss_causal_bressert_hold_v1",
            "strategy_label": "NWE + DSS Bressert · chronological research",
            "timeframe": timeframe
        },
        "summary": {
            "symbols_total": rows.len(),
            "current_history": current_rows,
            "validated_candidate_rows": validated_candidates,
            "known_fee_model": fee_model
        },
        "model_selection": {
            "signal": "Completed-bar DSS Bressert cross/approaching near-cross plus same-direction causal NWE outer-band cross/near-band support.",
            "entry": "Next bar open",
            "exits_bars": quant_core::NWE_DSS_EXIT_CANDIDATES_BARS,
            "selection_metric": "Highest validation mean net price return; ties favor shorter timeout.",
            "chronology": "60% training · 20% validation for exit selection · final 20% untouched holdout.",
            "position_policy": "One position at a time; signals during an open position are ignored.",
            "short_policy": if allow_short { "LONG and SHORT" } else { "LONG only; SHORT signals are excluded" },
            "fee_model": fee_model,
            "execution_costs_complete": costs_complete,
            "limitations": ["No slippage, order-book depth, market impact, funding, or borrow model.", "A validated candidate requires venue-native history, known fees, complete modeled execution costs, at least the configured minimum holdout trades, at least the configured minimum total history, positive holdout mean net return, and holdout profit factor above 1.", "Research candidates are not orders or execution recommendations."]
        },
        "provider_sync_performed": false,
        "read_only": true,
        "execution_status": "RESEARCH_ONLY",
        "storage_mode": "sqlite_local_history",
        "rows": rows
    }))
}

mod nwe_dss_rows;
use nwe_dss_rows::{nwe_dss_missing_row, nwe_dss_research_row, nwe_dss_unavailable_row};

mod builder;
pub(super) use builder::build_research_report;

pub(super) fn attach_backtest_stats(plan: &mut Value, stats: &HashMap<String, Value>) {
    let symbol = plan_backtest_symbol(plan);
    let class = plan_backtest_class(plan);
    let strategy = plan
        .get("Strategy_ID")
        .or_else(|| plan.get("strategy_id"))
        .or_else(|| plan.get("Action"))
        .or_else(|| plan.get("action"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let exact_key = strategy_backtest_key(&symbol, class, strategy);
    let stat = stats
        .get(&exact_key)
        // Keep the low-level gate compatible with direct unit fixtures. Live
        // callers only insert strategy-scoped evidence into the map.
        .or_else(|| stats.get(&backtest_key(&symbol, class)));
    if let Some(object) = plan.as_object_mut() {
        if let Some(stat) = stat {
            object.insert("Backtest_Stats".to_string(), stat.clone());
        } else {
            object.remove("Backtest_Stats");
        }
    }
}

pub(super) fn plan_signal_direction(plan: &Value) -> &'static str {
    let raw = plan
        .get("Signal_Direction")
        .or_else(|| plan.get("signal_direction"))
        .or_else(|| plan.get("Direction"))
        .or_else(|| plan.get("direction"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_ascii_uppercase();
    if raw.contains("SELLING_TIME") {
        "SELLING_TIME"
    } else if raw.contains("SHORT") {
        "SHORT"
    } else if raw.contains("LONG") {
        "LONG"
    } else {
        "WAIT"
    }
}

pub(super) fn plan_has_no_current_signal(plan: &Value) -> bool {
    let explicit_no_signal = ["Strategy_Status", "strategy_status", "Regime", "regime"]
        .iter()
        .any(|key| {
            plan.get(*key)
                .and_then(Value::as_str)
                .is_some_and(|value| value.eq_ignore_ascii_case("WATCHLIST_NO_CURRENT_SIGNAL"))
        });
    let validation_demoted = plan
        .get("Is_Watchlist")
        .or_else(|| plan.get("is_watchlist"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
        && ["Strategy_Status", "strategy_status"].iter().any(|key| {
            plan.get(*key)
                .and_then(Value::as_str)
                .is_some_and(|value| value.eq_ignore_ascii_case("BACKTEST_REQUIRED"))
        });
    explicit_no_signal || validation_demoted
}

/// Make backtest evidence a release gate for every generated trade plan.
///
/// A plan may remain visible as a watchlist candidate when evidence is absent,
/// but it must not retain an order action, RUNNING/IN_ZONE state, or a trigger
/// zone. This prevents the UI from turning an untested signal into a ticket.
pub(super) fn apply_backtest_gate(plan: &mut Value, stats: &HashMap<String, Value>) -> bool {
    let signal_direction = plan_signal_direction(plan);
    let no_current_signal = plan_has_no_current_signal(plan);
    let was_validation_demoted = plan
        .get("Is_Watchlist")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        && plan
            .get("Strategy_Status")
            .and_then(Value::as_str)
            .is_some_and(|value| value.eq_ignore_ascii_case("BACKTEST_REQUIRED"));
    attach_backtest_stats(plan, stats);
    let symbol = plan_backtest_symbol(plan);
    let class = plan_backtest_class(plan);
    let minimum_cagr_pct = plan_min_cagr_pct(plan);
    let minimum_calendar_month_return_pct = plan_min_calendar_month_return_pct(plan);
    let strategy_key = plan
        .get("Strategy_ID")
        .or_else(|| plan.get("strategy_id"))
        .or_else(|| plan.get("Action"))
        .or_else(|| plan.get("action"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_ascii_lowercase();
    let is_day_trade = plan
        .get("is_day_trade")
        .or_else(|| plan.get("Is_Day_Trade"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || strategy_key.contains("day_trade")
        || strategy_key.contains("day trade")
        || strategy_key.contains("confluence")
        || plan
            .get("Trade_Style")
            .or_else(|| plan.get("trade_style"))
            .and_then(Value::as_str)
            .is_some_and(|style| style.eq_ignore_ascii_case("day_trade"));
    let stat = plan.get("Backtest_Stats").cloned();
    let observed_cagr_pct = stat
        .as_ref()
        .and_then(|value| value.get("cagr_pct"))
        .and_then(Value::as_f64);
    let holdout_cagr_pct = stat
        .as_ref()
        .and_then(|value| value.get("holdout_cagr_pct"))
        .and_then(Value::as_f64);
    let observed_max_calendar_month_return_pct = stat
        .as_ref()
        .and_then(|value| value.get("max_calendar_month_return_pct"))
        .and_then(Value::as_f64);
    let holdout_max_calendar_month_return_pct = stat
        .as_ref()
        .and_then(|value| value.get("holdout_max_calendar_month_return_pct"))
        .and_then(Value::as_f64);
    let calendar_month_return_validated = stat
        .as_ref()
        .is_some_and(has_validated_holdout_calendar_month_return);
    let execution_scope = stat
        .as_ref()
        .and_then(|value| value.get("execution_scope"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let proxy_only = execution_scope.eq_ignore_ascii_case("research_proxy");
    let observed_win_rate_pct = stat
        .as_ref()
        .and_then(|value| value.get("win_rate_pct"))
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let holdout_win_rate_pct = stat
        .as_ref()
        .and_then(|value| value.get("holdout_win_rate_pct"))
        .and_then(Value::as_f64);
    let observed_profit_factor = stat
        .as_ref()
        .and_then(|value| value.get("profit_factor"))
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let holdout_profit_factor = stat
        .as_ref()
        .and_then(|value| value.get("holdout_profit_factor"))
        .and_then(Value::as_f64);
    let observed_reward_risk_ratio = stat
        .as_ref()
        .and_then(|value| value.get("observed_reward_risk_ratio"))
        .and_then(Value::as_f64);
    let trades = stat
        .as_ref()
        .and_then(|value| value.get("trades"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let sample_days = stat
        .as_ref()
        .and_then(|value| value.get("sample_days"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let declared_evidence_validated = stat.as_ref().is_some_and(has_validated_holdout_cagr);
    let simulated_strategy = stat
        .as_ref()
        .and_then(|value| value.get("strategy_id"))
        .and_then(Value::as_str);
    let strategy_matches = simulated_strategy
        .map(|value| {
            normalize_backtest_strategy(value) == normalize_backtest_strategy(&strategy_key)
        })
        .unwrap_or_else(|| plan.get("Strategy_ID").is_none() && plan.get("strategy_id").is_none());
    // Recheck the hard evidence contract at the gate. A stale or invalid
    // producer must not be able to promote a plan by setting a boolean flag.
    let evidence_validated = declared_evidence_validated
        && strategy_matches
        && trades >= MIN_EVIDENCE_SAMPLE_TRADES as u64
        && sample_days >= MIN_EVIDENCE_SAMPLE_DAYS;
    let venue_history = execution_scope.eq_ignore_ascii_case("venue_history");
    let minimum_day_trade_win_rate_pct = if is_day_trade {
        day_trade_minimum_win_rate_pct(observed_reward_risk_ratio)
    } else {
        0.0
    };
    let day_trade_win_rate_ok = !is_day_trade
        || holdout_win_rate_pct.is_some_and(|win_rate| win_rate >= minimum_day_trade_win_rate_pct);
    let day_trade_profit_factor_ok = !is_day_trade
        || holdout_profit_factor
            .is_some_and(|profit_factor| profit_factor >= MIN_DAY_TRADE_PROFIT_FACTOR);
    let cagr_target_ok =
        minimum_cagr_pct <= 0.0 || holdout_cagr_pct.is_some_and(|cagr| cagr >= minimum_cagr_pct);
    let calendar_month_target_ok = minimum_calendar_month_return_pct <= 0.0
        || (calendar_month_return_validated
            && holdout_max_calendar_month_return_pct
                .is_some_and(|value| value >= minimum_calendar_month_return_pct));
    let validated = evidence_validated
        && venue_history
        && day_trade_win_rate_ok
        && day_trade_profit_factor_ok
        && cagr_target_ok
        && calendar_month_target_ok;
    let evidence_source = stat
        .as_ref()
        .and_then(|value| value.get("evidence_source"))
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let source_label = stat
        .as_ref()
        .and_then(|value| value.get("source_label"))
        .and_then(Value::as_str)
        .unwrap_or("source unavailable");
    let reason = if validated {
        "Symbol/class-specific compounded backtest passed the minimum evidence rules."
    } else if stat.is_none() {
        "No symbol/class-specific backtest evidence is available."
    } else if !strategy_matches {
        "Historical simulation belongs to a different strategy profile."
    } else if proxy_only || !venue_history {
        "Historical evidence is not venue-native; venue-native history is required before execution validation."
    } else if is_day_trade && !day_trade_win_rate_ok {
        "Untouched-holdout Day Trade win rate has not met the 50% minimum win-rate goal."
    } else if is_day_trade && !day_trade_profit_factor_ok {
        "Untouched-holdout Day Trade evidence has not cleared the 1.20 profit-factor quality floor."
    } else if !evidence_validated {
        "An untouched, venue-native holdout with at least 730 days and 10 trades is required for validation."
    } else if !cagr_target_ok {
        "Untouched-holdout annualized CAGR has not met the strategy-specific minimum."
    } else if !calendar_month_target_ok {
        "An untouched holdout has not validated at least 100% account return in one complete calendar month."
    } else {
        "Backtest evidence exists but does not meet the minimum 730-day and 10-trade proof rules."
    };

    let decision = if validated && !no_current_signal {
        signal_direction
    } else {
        "WAIT"
    };
    let decision_reason = if !validated {
        reason.to_string()
    } else if no_current_signal {
        "No current strategy signal is active.".to_string()
    } else if decision == "WAIT" {
        "The current strategy did not emit a LONG or SHORT signal.".to_string()
    } else {
        format!("Current {decision} signal passed its matching historical simulation gate.")
    };

    if let Some(object) = plan.as_object_mut() {
        if validated && was_validation_demoted {
            object.insert(
                "Strategy_Status".to_string(),
                serde_json::json!("WATCHLIST_NO_CURRENT_SIGNAL"),
            );
            object.insert("Action".to_string(), serde_json::json!("MONITOR"));
            object.insert("State".to_string(), serde_json::json!("WATCHLIST"));
            object.insert("Zone_Status".to_string(), serde_json::json!("WATCHLIST"));
        }
        object.insert(
            "Signal_Direction".to_string(),
            serde_json::json!(signal_direction),
        );
        object.insert("Decision".to_string(), serde_json::json!(decision));
        object.insert(
            "Decision_Reason".to_string(),
            serde_json::json!(decision_reason),
        );
        object.insert("Direction".to_string(), serde_json::json!(decision));
        object.insert(
            "Backtest_Proof".to_string(),
            serde_json::json!({
                "status": if validated { "VALIDATED" } else { "REQUIRED" },
                "symbol": symbol,
                "asset_class": class,
                "minimum_trades": MIN_EVIDENCE_SAMPLE_TRADES,
                "minimum_sample_days": MIN_EVIDENCE_SAMPLE_DAYS,
                "minimum_win_rate_pct": if is_day_trade {
                    Some(minimum_day_trade_win_rate_pct)
                } else {
                    None
                },
                "minimum_profit_factor": if is_day_trade {
                    Some(MIN_DAY_TRADE_PROFIT_FACTOR)
                } else {
                    None
                },
                "observed_profit_factor": observed_profit_factor,
                "holdout_profit_factor": holdout_profit_factor,
                "holdout_win_rate_pct": holdout_win_rate_pct,
                "minimum_cagr_pct": minimum_cagr_pct,
                "observed_cagr_pct": observed_cagr_pct,
                "cagr_target_basis": if minimum_cagr_pct > 0.0 { "annualized untouched-holdout CAGR" } else { "not required" },
                "minimum_calendar_month_return_pct": minimum_calendar_month_return_pct,
                "observed_max_calendar_month_return_pct": observed_max_calendar_month_return_pct,
                "holdout_max_calendar_month_return_pct": holdout_max_calendar_month_return_pct,
                "observed_completed_calendar_months": stat
                    .as_ref()
                    .and_then(|value| value.get("completed_calendar_months"))
                    .and_then(Value::as_u64),
                "holdout_completed_calendar_months": stat
                    .as_ref()
                    .and_then(|value| value.get("holdout_completed_calendar_months"))
                    .and_then(Value::as_u64),
                "calendar_month_return_validated": calendar_month_return_validated,
                "calendar_month_return_validation_status": stat
                    .as_ref()
                    .and_then(|value| value.get("calendar_month_return_validation_status"))
                    .and_then(Value::as_str)
                    .unwrap_or("OUT_OF_SAMPLE_REQUIRED"),
                "cagr_validated": evidence_validated,
                "cagr_validation_status": stat
                    .as_ref()
                    .and_then(|value| value.get("cagr_validation_status"))
                    .and_then(Value::as_str)
                    .unwrap_or("OUT_OF_SAMPLE_REQUIRED"),
                "cagr_sample_sufficient": stat
                    .as_ref()
                    .and_then(|value| value.get("cagr_sample_sufficient"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                "holdout_cagr_pct": holdout_cagr_pct,
                "holdout_trades": stat
                    .as_ref()
                    .and_then(|value| value.get("holdout_trades"))
                    .and_then(Value::as_u64),
            "holdout_trade_span_days": stat
                    .as_ref()
                    .and_then(|value| value.get("trade_span_days"))
                    .and_then(Value::as_i64),
                "observed_win_rate_pct": observed_win_rate_pct,
                "observed_reward_risk_ratio": observed_reward_risk_ratio,
                "observed_trades": trades,
                "observed_sample_days": sample_days,
                "evidence_source": evidence_source,
                "source_label": source_label,
                "execution_scope": execution_scope,
                "reason": reason,
            }),
        );
        if !validated
            && !object
                .get("Is_Watchlist")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        {
            object.insert("Action".to_string(), serde_json::json!("MONITOR"));
            object.insert("State".to_string(), serde_json::json!("WATCHLIST"));
            object.insert("Zone_Status".to_string(), serde_json::json!("WATCHLIST"));
            object.insert("Is_Watchlist".to_string(), serde_json::json!(true));
            object.insert(
                "Strategy_Status".to_string(),
                serde_json::json!("BACKTEST_REQUIRED"),
            );
            object.insert(
                "Est_Hold".to_string(),
                serde_json::json!(
                    "No holding period is estimated without validated backtest evidence."
                ),
            );
            object.insert(
                "Invalidation".to_string(),
                serde_json::json!("No order until symbol/class-specific backtest proof passes"),
            );
            object.insert(
                "Catalyst_Thesis".to_string(),
                serde_json::json!(format!("Backtest required: {reason}")),
            );
        }
    }
    annotate_plan_decision_state(plan, validated);
    validated
}
