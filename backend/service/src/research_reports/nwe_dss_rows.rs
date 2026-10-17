use super::*;

pub(super) fn nwe_dss_research_row(
    connection: &Connection,
    selection: ResearchSelection,
    symbol: &str,
    buy_fee_pct: f64,
    sell_fee_pct: f64,
    fees_known: bool,
    costs_complete: bool,
    allow_short: bool,
) -> Result<Value, rusqlite::Error> {
    let metadata = load_series_metadata_for_exchange(
        connection,
        symbol,
        selection.timeframe,
        false,
        selection.exchange,
    )?;
    let selected_source = selected_candle_source_for_timeframe(
        connection,
        symbol,
        Some(selection.exchange),
        selection.timeframe,
    )?
    .unwrap_or_else(|| "unknown".to_string());
    let source = metadata
        .get("source")
        .and_then(Value::as_str)
        .unwrap_or(&selected_source)
        .to_string();
    let storage_timeframe = metadata
        .get("storage_timeframe")
        .and_then(Value::as_str)
        .unwrap_or(selection.timeframe);
    let candles = if selection.timeframe == "1d" {
        load_candles_from_source(
            connection,
            symbol,
            selection.timeframe,
            MAX_EVIDENCE_CANDLES,
            &source,
        )?
    } else {
        load_series_candles_from_source(
            connection,
            symbol,
            selection.timeframe,
            MAX_EVIDENCE_CANDLES,
            &source,
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
    let (data_source, instrument_kind) = connection.query_row(
        "SELECT source,instrument_kind FROM series WHERE symbol=?1 AND interval=?2 AND source=?3",
        params![symbol, storage_timeframe, source],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    ).optional()?.unwrap_or_else(|| (source.clone(), "unknown".to_string()));
    let is_proxy = is_research_proxy_source(&data_source) || instrument_kind.contains("proxy");
    let venue_native = is_native_candle_source_for_exchange(&data_source, Some(selection.exchange));
    let execution_scope = if is_proxy {
        "research_proxy"
    } else if venue_native {
        "venue_history"
    } else {
        "unverified_history"
    };
    let is_stale = metadata
        .get("is_stale")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let freshness = if bars.is_empty() {
        "MISSING"
    } else if is_stale {
        "STALE"
    } else {
        "CURRENT"
    };
    let mut backtest = quant_core::evaluate_nwe_dss_exit_selection(
        &bars,
        buy_fee_pct,
        sell_fee_pct,
        fees_known,
        costs_complete,
        venue_native,
        allow_short,
    );
    if let Some(object) = backtest.as_object_mut() {
        object.insert("execution_scope".to_string(), Value::from(execution_scope));
        object.insert("data_source".to_string(), Value::from(data_source.clone()));
        object.insert(
            "fee_model".to_string(),
            Value::from(if fees_known {
                "configured venue fee policy"
            } else {
                "venue fees unknown; gross returns only"
            }),
        );
    }
    let source_label = research_source_label(&data_source, &instrument_kind);
    Ok(serde_json::json!({
        "symbol": symbol,
        "exchange": selection.exchange,
        "exchange_label": selection.exchange_label,
        "asset_category_id": selection.asset_category_id,
        "timeframe": selection.timeframe,
        "strategy_id": "nwe_dss_causal_bressert_hold_v1",
        "history_bars": bars.len(),
        "coverage_days": metadata.get("coverage_days").cloned().unwrap_or(Value::from(0)),
        "history_start_ts": metadata.get("first_ts").cloned().unwrap_or(Value::Null),
        "history_end_ts": metadata.get("last_ts").cloned().unwrap_or(Value::Null),
        "freshness": freshness,
        "source": data_source,
        "source_label": source_label,
        "instrument_kind": instrument_kind,
        "execution_scope": execution_scope,
        "fee_model": if fees_known { "configured venue fee policy" } else { "venue fees unknown; gross returns only" },
        "backtest": backtest
    }))
}

pub(super) fn nwe_dss_missing_row(
    selection: ResearchSelection,
    symbol: &str,
    fee_model: &str,
) -> Value {
    serde_json::json!({"symbol":symbol,"exchange":selection.exchange,"asset_category_id":selection.asset_category_id,"timeframe":selection.timeframe,"freshness":"MISSING","history_bars":0,"source":"unavailable","execution_scope":"unverified_history","fee_model":fee_model,"backtest":{"status":"NO_HISTORY","rule_candidate":null}})
}

pub(super) fn nwe_dss_unavailable_row(
    selection: ResearchSelection,
    symbol: &str,
    fee_model: &str,
    error: &str,
) -> Value {
    serde_json::json!({"symbol":symbol,"exchange":selection.exchange,"asset_category_id":selection.asset_category_id,"timeframe":selection.timeframe,"freshness":"UNAVAILABLE","history_bars":0,"source":"unavailable","execution_scope":"unverified_history","fee_model":fee_model,"error":error,"recovery_status":"retry_available","backtest":{"status":"UNAVAILABLE","rule_candidate":null}})
}
