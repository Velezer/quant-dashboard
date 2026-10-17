use super::*;

pub fn build_research_report(
    market_database: &str,
    selection: ResearchSelection,
    symbols: &[String],
) -> Result<Value, rusqlite::Error> {
    let mut rows = Vec::with_capacity(symbols.len());
    for symbol in symbols.iter().take(MAX_RESEARCH_SYMBOLS) {
        let row = match open_market_history_for_symbol(market_database, symbol, false) {
            Ok(Some(connection)) => match research_row(&connection, selection, symbol) {
                Ok(row) => row,
                Err(error) => unavailable_research_row(selection, symbol, &error.to_string())?,
            },
            Ok(None) => missing_research_row(selection, symbol)?,
            Err(error) => unavailable_research_row(selection, symbol, &error.to_string())?,
        };
        rows.push(row);
    }
    if selection.strategy == "multibagger" {
        rows.sort_by(|left, right| {
            let left_score = finite_json_number(left.get("candidate_score")).unwrap_or(0.0);
            let right_score = finite_json_number(right.get("candidate_score")).unwrap_or(0.0);
            right_score
                .partial_cmp(&left_score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    left.get("symbol")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .cmp(right.get("symbol").and_then(Value::as_str).unwrap_or(""))
                })
        });
        for (index, row) in rows.iter_mut().enumerate() {
            if let Some(object) = row.as_object_mut() {
                object.insert("candidate_rank".to_string(), serde_json::json!(index + 1));
            }
        }
    }
    let count = |field: &str, value: &str| {
        rows.iter()
            .filter(|row| row.get(field).and_then(Value::as_str) == Some(value))
            .count()
    };
    let sum_trades: u64 = rows
        .iter()
        .filter_map(|row| row.get("trades").and_then(Value::as_u64))
        .sum();
    let evidence_rows = rows
        .iter()
        .filter(|row| {
            row.get("evidence_status")
                .and_then(Value::as_str)
                .is_some_and(|status| {
                    !matches!(
                        status,
                        "NO_HISTORY" | "INSUFFICIENT_HISTORY" | "UNSUPPORTED_STRATEGY"
                    )
                })
        })
        .count();
    let validated_cagr = rows
        .iter()
        .filter(|row| has_validated_holdout_cagr(row))
        .count();
    let validated_monthly_target = rows
        .iter()
        .filter(|row| {
            row.get("candidate_status").and_then(Value::as_str) == Some("MONTHLY_RETURN_TARGET_MET")
        })
        .count();
    let potential_candidates = rows
        .iter()
        .filter(|row| {
            row.get("candidate_status").and_then(Value::as_str) == Some("POTENTIAL_CANDIDATE")
        })
        .count();
    let momentum_watchlist = rows
        .iter()
        .filter(|row| {
            row.get("candidate_status").and_then(Value::as_str) == Some("MOMENTUM_WATCHLIST")
        })
        .count();
    let discovery = if selection.strategy == "multibagger" {
        let status = if validated_monthly_target > 0 {
            "MONTHLY_RETURN_TARGETS_FOUND"
        } else if potential_candidates > 0 {
            "POTENTIAL_CANDIDATES_FOUND"
        } else if momentum_watchlist > 0 {
            "WATCHLIST_ONLY"
        } else {
            "NO_POTENTIAL_CANDIDATE"
        };
        let message = match status {
            "MONTHLY_RETURN_TARGETS_FOUND" =>
                "At least one stored candidate meets the 100% calendar-month account-return evidence goal; venue-native history is still required before execution.",
            "POTENTIAL_CANDIDATES_FOUND" =>
                "Candidates pass the price and evidence screen, but none has validated 100% return in a complete calendar month yet.",
            "WATCHLIST_ONLY" =>
                "Momentum exists in the stored history, but no candidate passes the complete compounder screen.",
            _ =>
                "No stored candidate passes the current compounder screen. Refresh history or widen the reviewed universe before drawing a conclusion.",
        };
        let shortlist = rows
            .iter()
            .take(10)
            .map(|row| {
                serde_json::json!({
                    "rank": row.get("candidate_rank").cloned().unwrap_or(Value::Null),
                    "symbol": row.get("symbol").cloned().unwrap_or(Value::Null),
                    "status": row.get("candidate_status").cloned().unwrap_or(Value::Null),
                    "score": row.get("candidate_score").cloned().unwrap_or(Value::Null)
                })
            })
            .collect::<Vec<_>>();
        serde_json::json!({
            "status": status,
            "message": message,
            "universe": "Stockbit IDX symbols with stored 1d history",
            "screened_symbols": rows.len(),
            "target_met_count": validated_monthly_target,
            "potential_candidate_count": potential_candidates,
            "momentum_watchlist_count": momentum_watchlist,
            "shortlist_count": shortlist.len(),
            "minimum_calendar_month_return_pct": MIN_MULTIBAGGER_CALENDAR_MONTH_RETURN_PCT,
            "shortlist": shortlist,
            "criteria": [
                "At least 730 calendar days and 10 observed trades for untouched-holdout validation",
                "Positive one-year price trend and close above the 200-day average for potential status",
                "Profit factor of at least 1.0 for potential status",
                "At least 100% fee-adjusted, risk-sized account return in one complete calendar month is required for the target-met status"
            ],
            "score_note": "Candidate score ranks stored evidence and price-screen facts; it is not a return forecast or order permission."
        })
    } else {
        Value::Null
    };
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
            "profile_id": selection.profile_id,
            "profile_label": selection.profile_label,
            "trade_style": selection.trade_style,
            "trade_style_label": selection.trade_style_label,
            "signal_strategy_id": selection.signal_strategy_id,
            "signal_strategy_label": selection.signal_strategy_label,
            "strategy": selection.strategy,
            "strategy_label": selection.strategy_label,
            "timeframe": selection.timeframe,
            "evidence_asset_class": selection.evidence_class,
        },
        "summary": {
            "symbols_total": rows.len(),
            "current_history": count("freshness", "CURRENT"),
            "stale_history": count("freshness", "STALE"),
            "missing_history": count("freshness", "MISSING"),
            "unavailable_history": count("freshness", "UNAVAILABLE"),
            "observed_evidence_rows": evidence_rows,
            "observed_trades": sum_trades,
            "validated_cagr_rows": validated_cagr,
            "validated_monthly_return_target_rows": validated_monthly_target,
            "proxy_rows": rows.iter().filter(|row| row.get("execution_scope").and_then(Value::as_str) == Some("research_proxy")).count(),
        },
        "discovery": discovery,
        "storage_mode": "sqlite_local_history",
        "history_contract": {
            "minimum_calendar_days": MIN_ENGINE_HISTORY_DAYS,
            "daily_weekly_request_range": "10y",
            "intraday_policy": "provider_window_plus_local_accumulation"
        },
        "provider_sync_performed": false,
        "read_only": true,
        "execution_status": "RESEARCH_ONLY",
        "limitations": [
            "Research reads durable local SQLite candles and does not fetch a provider.",
            "Provider proxy rows remain labeled research_proxy and do not validate venue execution.",
            "Annualized CAGR and completed calendar-month returns are displayed as observed evidence; targets require untouched-holdout validation.",
            "Observed OHLC strategy results do not model order-book depth, funding, slippage, or fills beyond the documented fee model."
        ],
        "rows": rows,
    }))
}

fn unavailable_research_row(
    selection: ResearchSelection,
    symbol: &str,
    error: &str,
) -> Result<Value, rusqlite::Error> {
    let empty_market = empty_market_history()?;
    let mut row = research_row(&empty_market, selection, symbol)?;
    if let Some(object) = row.as_object_mut() {
        object.insert("freshness".to_string(), serde_json::json!("UNAVAILABLE"));
        object.insert(
            "history_status".to_string(),
            serde_json::json!("UNAVAILABLE"),
        );
        object.insert(
            "research_status".to_string(),
            serde_json::json!("UNAVAILABLE"),
        );
        object.insert(
            "recovery_status".to_string(),
            serde_json::json!("retry_available"),
        );
        object.insert(
            "reason".to_string(),
            serde_json::json!(
                "Ticker history could not be read; other registered tickers remain available."
            ),
        );
        object.insert("error".to_string(), serde_json::json!(error));
    }
    Ok(row)
}

fn missing_research_row(
    selection: ResearchSelection,
    symbol: &str,
) -> Result<Value, rusqlite::Error> {
    let empty_market = empty_market_history()?;
    research_row(&empty_market, selection, symbol)
}

fn empty_market_history() -> Result<Connection, rusqlite::Error> {
    let connection = Connection::open_in_memory()?;
    initialize_empty_market_schema(&connection)?;
    Ok(connection)
}
