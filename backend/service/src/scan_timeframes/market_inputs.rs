use super::*;

pub(super) struct CurrentPrice {
    pub(super) value: f64,
    pub(super) signal_bar_time: i64,
    pub(super) source: String,
    pub(super) provider: String,
}

pub(super) fn load_stockbit_signal_candles(
    input: &TimeframeScanInput<'_>,
    timeframe: &str,
    limit: i64,
    source: &str,
    latest_15m_bar_time: i64,
) -> Vec<Candle> {
    let stored_history = load_series_candles_from_source(
        input.market_connection,
        input.symbol,
        timeframe,
        limit,
        source,
    )
    .unwrap_or_default();
    let Some(timeframe_duration_ns) = timeframe_duration_ns(timeframe) else {
        return Vec::new();
    };
    if timeframe == "15m" {
        let Some(latest_bar_time) = stored_history.last().map(|bar| bar.time) else {
            return Vec::new();
        };
        return if latest_15m_bar_time >= latest_bar_time
            && latest_15m_bar_time < latest_bar_time.saturating_add(timeframe_duration_ns)
        {
            stored_history
        } else {
            Vec::new()
        };
    }
    let intraday_rollup = aggregate_candles_from_base_source(
        input.market_connection,
        input.symbol,
        timeframe,
        "15m",
        limit,
        source,
    )
    .unwrap_or_default();
    let stored_history_is_newer = stored_history.last().map(|bar| bar.time);
    let intraday_is_newer = intraday_rollup.last().map(|bar| bar.time);
    if intraday_rollup.is_empty()
        || intraday_is_newer < stored_history_is_newer
        || intraday_is_newer.is_none()
    {
        return Vec::new();
    }
    let Some(first_intraday_time) = intraday_rollup.first().map(|bar| bar.time) else {
        return Vec::new();
    };
    let Some(latest_intraday_time) = intraday_rollup.last().map(|bar| bar.time) else {
        return Vec::new();
    };
    if latest_15m_bar_time < latest_intraday_time
        || latest_15m_bar_time >= latest_intraday_time.saturating_add(timeframe_duration_ns)
    {
        return Vec::new();
    }
    let mut merged = stored_history
        .into_iter()
        .filter(|bar| bar.time < first_intraday_time)
        .collect::<Vec<_>>();
    merged.extend(intraday_rollup);
    if merged.len() > limit as usize {
        let start = merged.len() - limit as usize;
        merged.drain(..start);
    }
    merged
}

pub(super) fn fresh_current_price(
    input: &TimeframeScanInput<'_>,
    known_15m_state: Option<&ProductCandleScanState>,
    now_ns: i64,
) -> Option<CurrentPrice> {
    let source_state = match known_15m_state {
        Some(state) => state.clone(),
        None => product_candle_scan_state_at(
            input.market_connection,
            input.symbol,
            input.exchange_id,
            input.product_id,
            "15m",
            now_ns.div_euclid(NANOS_PER_SECOND),
        )
        .ok()??,
    };
    if source_state.is_stale {
        return None;
    }
    let candles = load_series_candles_from_source(
        input.market_connection,
        input.symbol,
        "15m",
        4,
        &source_state.source,
    )
    .ok()?;
    let latest = candles
        .iter()
        .rev()
        .find(|bar| completed_bar_is_fresh_at(input.symbol, "15m", bar.time, now_ns))?;
    if !latest.close.is_finite() || latest.close <= 0.0 {
        return None;
    }
    Some(CurrentPrice {
        value: latest.close,
        signal_bar_time: latest.time,
        source: source_state.source,
        provider: source_state.data_provider,
    })
}

pub(super) fn completed_bars(
    candles: &[Candle],
    duration_ns: i64,
    now_ns: i64,
) -> Vec<StrategyBar> {
    candles
        .iter()
        .filter(|bar| bar.time.saturating_add(duration_ns) <= now_ns)
        .map(|bar| StrategyBar {
            time: bar.time,
            open: bar.open,
            high: bar.high,
            low: bar.low,
            close: bar.close,
            volume: bar.volume,
        })
        .collect()
}

pub(super) fn current_time_ns() -> i64 {
    i64::try_from(now_secs())
        .unwrap_or(i64::MAX)
        .saturating_mul(NANOS_PER_SECOND)
}

pub(super) fn annotate_current_price_provenance(
    running_plans: &mut [Value],
    potential_plans: &mut [Value],
    current_price: &CurrentPrice,
    timeframe: &str,
    running_start: usize,
    potential_start: usize,
) {
    for plan in running_plans
        .iter_mut()
        .skip(running_start)
        .chain(potential_plans.iter_mut().skip(potential_start))
    {
        if plan.get("timeframe").and_then(Value::as_str) != Some(timeframe) {
            continue;
        }
        if plan.get("Current_Price").and_then(Value::as_f64) != Some(current_price.value) {
            continue;
        }
        plan["Current_Price_Timeframe"] = serde_json::json!("15m");
        plan["Current_Price_Bar_Time"] = serde_json::json!(current_price.signal_bar_time);
        plan["Current_Price_Data_Source"] = serde_json::json!(current_price.source);
        plan["Current_Price_Data_Provider"] = serde_json::json!(current_price.provider);
    }
}
