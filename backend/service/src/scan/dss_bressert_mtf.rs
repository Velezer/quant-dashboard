use super::*;

const DSS_BRESSERT_SCAN_HISTORY_BARS: i64 = 80;

pub(super) fn build_annotation(
    connection: &Connection,
    symbol: &str,
    exchange: &str,
    product_id: &str,
) -> Value {
    let now_ns = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .min(i64::MAX as u128) as i64;
    let mut frame_inputs = Vec::with_capacity(DSS_BRESSERT_MTF_TIMEFRAMES.len());

    for definition in DSS_BRESSERT_MTF_TIMEFRAMES {
        let (source, source_error) = match selected_candle_source_for_product(
            connection,
            symbol,
            exchange,
            product_id,
            definition.timeframe,
        ) {
            Ok(source) => (source, None),
            Err(error) => (None, Some(error.to_string())),
        };
        let mut load_error = false;
        let candles = source
            .as_deref()
            .and_then(|source| {
                load_series_candles_from_source(
                    connection,
                    symbol,
                    definition.timeframe,
                    DSS_BRESSERT_SCAN_HISTORY_BARS,
                    source,
                )
                .map_err(|_| load_error = true)
                .ok()
            })
            .unwrap_or_default();
        let completed_bars = timeframe_duration_ns(definition.timeframe)
            .map(|duration_ns| {
                candles
                    .iter()
                    .filter(|bar| bar.time.saturating_add(duration_ns) <= now_ns)
                    .map(|candle| StrategyBar {
                        time: candle.time,
                        open: candle.open,
                        high: candle.high,
                        low: candle.low,
                        close: candle.close,
                        volume: candle.volume,
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let metadata = source
            .as_deref()
            .map(|source| {
                load_series_metadata_from_source(
                    connection,
                    symbol,
                    definition.timeframe,
                    false,
                    source,
                )
                .unwrap_or_else(|error| {
                    serde_json::json!({
                        "exists": false,
                        "is_stale": true,
                        "warning": format!("Unable to read timeframe metadata: {error}"),
                    })
                })
            })
            .or_else(|| {
                source_error.as_ref().map(|error| {
                    serde_json::json!({
                        "exists": false,
                        "is_stale": true,
                        "warning": format!("Unable to select product history: {error}"),
                    })
                })
            });
        let freshness_available = metadata
            .as_ref()
            .and_then(|item| item.get("exists"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let is_stale = metadata
            .as_ref()
            .and_then(|item| item.get("is_stale"))
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let availability = if source_error.is_some() {
            "SOURCE_ERROR"
        } else if load_error {
            "LOAD_ERROR"
        } else if source.is_none() || completed_bars.is_empty() {
            "NO_HISTORY"
        } else if completed_bars.len() < DSS_BRESSERT_MINIMUM_HISTORY_BARS {
            "INSUFFICIENT_HISTORY"
        } else if !freshness_available {
            "METADATA_UNAVAILABLE"
        } else if is_stale {
            "STALE"
        } else {
            "READY"
        };
        let last_bar_time = completed_bars.last().map(|bar| bar.time);
        let age_seconds = last_bar_time.map(|timestamp| {
            now_ns
                .saturating_sub(timestamp)
                .max(0)
                .div_euclid(NANOS_PER_SECOND)
        });
        let source_label = metadata
            .as_ref()
            .and_then(|item| item.get("source_label"))
            .cloned()
            .or_else(|| source.as_deref().map(candle_source_label).map(Value::from));
        frame_inputs.push((
            definition.timeframe,
            completed_bars,
            availability,
            source,
            source_label,
            metadata,
            last_bar_time,
            age_seconds,
        ));
    }

    let inputs = frame_inputs
        .iter()
        .map(|(timeframe, bars, ..)| DssBressertTimeframeInput { timeframe, bars })
        .collect::<Vec<_>>();
    let result = calculate_dss_bressert_mtf(&inputs);
    let complete = result.complete
        && frame_inputs
            .iter()
            .all(|(_, _, availability, ..)| *availability == "READY");
    let stale = frame_inputs
        .iter()
        .any(|(_, _, availability, ..)| *availability == "STALE");
    let status = if complete {
        "CURRENT"
    } else if stale {
        "STALE"
    } else {
        "INCOMPLETE"
    };
    let frames = result
        .timeframes
        .iter()
        .zip(frame_inputs.iter())
        .map(
            |(
                frame,
                (_, bars, availability, source, source_label, metadata, last_bar_time, age_seconds),
            )| {
                let provider = metadata
                    .as_ref()
                    .and_then(|item| item.get("data_provider"))
                    .cloned();
                let source_is_proxy = metadata
                    .as_ref()
                    .and_then(|item| item.get("source_is_proxy"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let source_is_native = source.as_deref().is_some_and(|source| {
                    is_native_candle_source_for_exchange(source, Some(exchange))
                });
                let data_quality = if source_is_proxy {
                    "RESEARCH_PROXY"
                } else if source_is_native {
                    "APP_NATIVE"
                } else if source.is_some() {
                    "UNCLASSIFIED_SOURCE"
                } else {
                    "UNAVAILABLE"
                };
                let sync_warning = metadata
                    .as_ref()
                    .and_then(|item| item.get("warning"))
                    .cloned();
                serde_json::json!({
                    "timeframe": frame.timeframe,
                    "state": frame.state,
                    "score": frame.score,
                    "weight": frame.weight,
                    "weighted_score": frame.weighted_score,
                    "availability": availability,
                    "completed_bars": bars.len(),
                    "latest_bar_time": last_bar_time,
                    "age_seconds": age_seconds,
                    "source": source,
                    "source_label": source_label,
                    "provider": provider,
                    "data_quality": data_quality,
                    "sync_warning": sync_warning,
                })
            },
        )
        .collect::<Vec<_>>();
    serde_json::json!({
        "strategy_id": result.strategy_id,
        "status": status,
        "research_only": true,
        "diagnostic_only": true,
        "bias": if complete { result.bias } else { "UNAVAILABLE" },
        "score": if complete { result.score } else { None },
        "alignment_score": if complete { Some(result.alignment_score) } else { None },
        "timeframes": frames,
        "note": "DSS multi-timeframe bias is a research diagnostic. It does not change the plan decision or execution readiness.",
    })
}
