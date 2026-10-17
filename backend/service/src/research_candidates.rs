use super::*;

#[path = "research_candidates/selection.rs"]
mod selection;
#[cfg(test)]
pub(super) use selection::normalize_research_symbol;
pub(super) use selection::{
    nwe_dss_research_selection, research_selection, research_symbols, ResearchSelection,
};
#[path = "research_candidates/discovery.rs"]
mod discovery;
#[path = "research_candidates/evidence.rs"]
mod evidence;
pub(super) use discovery::{multibagger_candidate_assessment, multibagger_price_screen};
pub(super) use evidence::{
    finite_json_number, has_validated_holdout_cagr, has_validated_holdout_calendar_month_return,
    research_source_label,
};
#[cfg(test)]
#[path = "research_candidates/evidence_tests.rs"]
mod evidence_tests;

pub(super) fn research_row(
    connection: &Connection,
    selection: ResearchSelection,
    symbol: &str,
) -> Result<Value, rusqlite::Error> {
    let metadata = load_series_metadata_for_exchange(
        connection,
        symbol,
        selection.timeframe,
        true,
        selection.exchange,
    )?;
    let exists = metadata
        .get("exists")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let history_bars = metadata.get("rows").and_then(Value::as_i64).unwrap_or(0);
    let history_target_days = metadata
        .get("history_target_days")
        .and_then(Value::as_i64)
        .unwrap_or(MIN_ENGINE_HISTORY_DAYS);
    let coverage_days = metadata
        .get("coverage_days")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let history_status = metadata
        .get("history_status")
        .and_then(Value::as_str)
        .unwrap_or("MISSING");
    let stale = metadata
        .get("is_stale")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let freshness = if !exists || history_bars <= 0 {
        "MISSING"
    } else if stale {
        "STALE"
    } else {
        "CURRENT"
    };
    let evidence = local_historical_evidence(
        connection,
        symbol,
        selection.strategy,
        Some(selection.exchange),
    )?;
    let source = metadata
        .get("source")
        .and_then(Value::as_str)
        .unwrap_or("unavailable");
    let instrument_kind = metadata
        .get("instrument_kind")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let source_label = evidence
        .as_ref()
        .and_then(|value| value.get("source_label"))
        .and_then(Value::as_str)
        .unwrap_or_else(|| research_source_label(source, instrument_kind));
    let execution_scope = evidence
        .as_ref()
        .and_then(|value| value.get("execution_scope"))
        .and_then(Value::as_str)
        .unwrap_or(if is_research_proxy_source(source) {
            "research_proxy"
        } else if is_native_candle_source_for_exchange(source, Some(selection.exchange)) {
            "venue_history"
        } else {
            "unverified_history"
        });
    let evidence_status = evidence
        .as_ref()
        .and_then(|value| value.get("evidence_status"))
        .and_then(Value::as_str)
        .unwrap_or(if !exists || history_bars <= 0 {
            "NO_HISTORY"
        } else {
            "INSUFFICIENT_HISTORY"
        });
    let research_status = if !exists || history_bars <= 0 {
        "NO_HISTORY"
    } else if stale {
        "STALE_HISTORY"
    } else if evidence.is_none() || evidence_status.eq_ignore_ascii_case("INSUFFICIENT_HISTORY") {
        "INSUFFICIENT_HISTORY"
    } else if execution_scope.eq_ignore_ascii_case("research_proxy") {
        "PROXY_ONLY"
    } else if !execution_scope.eq_ignore_ascii_case("venue_history") {
        "UNVERIFIED_SOURCE"
    } else {
        "READY"
    };
    let strategy_invention = if let Some(baseline) = evidence.as_ref() {
        let source_id = source.to_string();
        let candles = if selection.timeframe == "1d" {
            load_candles_from_source(
                connection,
                symbol,
                selection.timeframe,
                MAX_EVIDENCE_CANDLES,
                &source_id,
            )?
        } else {
            load_series_candles_from_source(
                connection,
                symbol,
                selection.timeframe,
                MAX_EVIDENCE_CANDLES,
                &source_id,
            )?
        };
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
        build_strategy_invention_report(&bars, selection.strategy, Some(baseline))
    } else {
        None
    };
    let read_stat = |key: &str| {
        evidence
            .as_ref()
            .and_then(|value| value.get(key))
            .cloned()
            .unwrap_or(Value::Null)
    };
    let last_ts = metadata.get("last_ts").cloned().unwrap_or(Value::Null);
    let mut row = serde_json::json!({
        "symbol": symbol,
        "underlying_symbol": (is_tokenized_stock(symbol)).then(|| underlying_yfinance_symbol(symbol)),
        "exchange": selection.exchange,
        "exchange_label": selection.exchange_label,
        "asset_class": selection.asset_class,
        "strategy": selection.strategy,
        "strategy_label": selection.strategy_label,
        "evidence_asset_class": selection.evidence_class,
        "timeframe": selection.timeframe,
        "history_bars": history_bars,
        "history_target_days": history_target_days,
        "coverage_days": coverage_days,
        "history_status": history_status,
        "first_bar_ts": metadata.get("first_ts").cloned().unwrap_or(Value::Null),
        "last_bar_ts": last_ts,
        "age_seconds": metadata.get("age_seconds").cloned().unwrap_or(Value::Null),
        "freshness": freshness,
        "source": source,
        "source_label": source_label,
        "instrument_kind": instrument_kind,
        "execution_scope": execution_scope,
        "evidence_status": evidence_status,
        "research_status": research_status,
        "warning": metadata.get("warning").cloned().unwrap_or(Value::Null),
        "history_warning": metadata.get("history_warning").cloned().unwrap_or(Value::Null),
        "reason": if research_status == "NO_HISTORY" {
            "No stored history is available for this app, category, and timeframe."
        } else if research_status == "STALE_HISTORY" {
            "Stored history exists but is older than the freshness threshold."
        } else if research_status == "INSUFFICIENT_HISTORY" {
            "Stored history is present, but it does not contain enough observed strategy evidence yet."
        } else if research_status == "PROXY_ONLY" {
            "Evidence is available from a research proxy; venue-native history is still required for execution validation."
        } else {
            "Stored history and the chronological strategy evaluation are available for research."
        },
        "read_only": true,
        "execution_status": "RESEARCH_ONLY",
        "trades": read_stat("trades"),
        "wins": read_stat("wins"),
        "losses": read_stat("losses"),
        "win_rate_pct": read_stat("win_rate_pct"),
        "minimum_win_rate_pct": read_stat("minimum_win_rate_pct"),
        "profit_factor": read_stat("profit_factor"),
        "total_pnl": read_stat("total_pnl"),
        "compounded_return_pct": read_stat("compounded_return_pct"),
        "quality_grade": read_stat("quality_grade"),
        "sample_days": read_stat("sample_days"),
        "cagr_pct": read_stat("cagr_pct"),
        "minimum_cagr_pct": strategy_min_cagr_pct(selection.strategy),
        "cagr_target_basis": if strategy_min_cagr_pct(selection.strategy) > 0.0 {
            "annualized untouched-holdout CAGR"
        } else {
            "not required"
        },
        "max_calendar_month_return_pct": read_stat("max_calendar_month_return_pct"),
        "completed_calendar_months": read_stat("completed_calendar_months"),
        "cagr_sample_sufficient": read_stat("cagr_sample_sufficient"),
        "cagr_validated": read_stat("cagr_validated"),
        "cagr_validation_status": read_stat("cagr_validation_status"),
        "calendar_month_return_validated": read_stat("calendar_month_return_validated"),
        "calendar_month_return_validation_status": read_stat("calendar_month_return_validation_status"),
        "holdout_max_calendar_month_return_pct": read_stat("holdout_max_calendar_month_return_pct"),
        "holdout_completed_calendar_months": read_stat("holdout_completed_calendar_months"),
        "cagr_basis": read_stat("cagr_basis"),
        "compounding_basis": read_stat("compounding_basis"),
        "risk_sized_trade_count": read_stat("risk_sized_trade_count"),
        "halted_trade_count": read_stat("halted_trade_count"),
        "max_drawdown_pct": read_stat("max_drawdown_pct"),
        "fee_model": read_stat("fee_model"),
        "max_hold_bars": read_stat("max_hold_bars"),
        "validation_basis": read_stat("validation_basis"),
        "strategy_invention": strategy_invention,
    });
    if let Some(object) = row.as_object_mut() {
        if object.get("underlying_symbol").is_some_and(Value::is_null) {
            object.remove("underlying_symbol");
        }
    }
    if selection.strategy == "multibagger" {
        let price_screen = multibagger_price_screen(connection, symbol)?;
        let (candidate_status, candidate_score, candidate_reason) =
            multibagger_candidate_assessment(&row, &price_screen);
        if let Some(object) = row.as_object_mut() {
            object.insert("price_screen".to_string(), price_screen);
            object.insert(
                "candidate_status".to_string(),
                serde_json::json!(candidate_status),
            );
            object.insert(
                "candidate_score".to_string(),
                serde_json::json!(candidate_score),
            );
            object.insert(
                "candidate_reason".to_string(),
                serde_json::json!(candidate_reason),
            );
        }
    }
    Ok(row)
}
