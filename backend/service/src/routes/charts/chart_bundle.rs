use super::super::{api_error, query_values, respond, RequestContext};
use crate::*;

pub(super) fn handle<'a>(request: RequestContext<'a>) -> Result<(), RequestContext<'a>> {
    let RequestContext {
        stream,
        db,
        state,
        path,
        ..
    } = request;
    let values = query_values(path);
    let symbol = values
        .get("symbol")
        .cloned()
        .unwrap_or_default()
        .to_uppercase();
    let exchange = values
        .get("exchange")
        .map(|value| value.trim().to_ascii_lowercase())
        .unwrap_or_default();
    let product_id = values
        .get("product_id")
        .map(|value| value.trim())
        .unwrap_or("");
    let asset_class = values
        .get("asset_class")
        .map(|value| value.trim().to_ascii_lowercase())
        .unwrap_or_default();
    if symbol.is_empty() {
        api_error(stream, "400 Bad Request", "symbol is required");
        return Ok(());
    }
    let limit = values
        .get("limit")
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(500)
        .clamp(1, MAX_CANDLES);
    let connection = match open_db_minimal(db) {
        Ok(connection) => connection,
        Err(error) => {
            api_error(stream, "500 Internal Server Error", error);
            return Ok(());
        }
    };
    let source_exchange = match (exchange.as_str(), product_id) {
        ("pintu", "pintu_pro_futures") => "pintu_futures",
        ("pintu", "pintu_retail_spot" | "pintu_pro_spot") => product_id,
        ("pluang", "pluang_spot" | "pluang_perpetual") => product_id,
        ("tokocrypto", "tokocrypto_spot" | "tokocrypto_tokenized_stock") => product_id,
        (_, "") => exchange.as_str(),
        _ => {
            let product_is_enabled = match connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM exchange_products
                      WHERE exchange_id=?1 AND product_id=?2 AND status='enabled')",
                params![exchange, product_id],
                |row| row.get::<_, bool>(0),
            ) {
                Ok(enabled) => enabled,
                Err(error) => {
                    api_error(stream, "500 Internal Server Error", error);
                    return Ok(());
                }
            };
            if product_is_enabled {
                exchange.as_str()
            } else {
                api_error(stream, "400 Bad Request", "product does not belong to app");
                return Ok(());
            }
        }
    };
    let storage_symbol = if let Some(requested_storage_symbol) = values
        .get("storage_symbol")
        .filter(|value| !value.trim().is_empty())
    {
        match resolve_registered_candle_storage_symbol(
            &connection,
            &exchange,
            product_id,
            requested_storage_symbol,
        ) {
            Ok(Some(resolved)) => resolved,
            Ok(None) => {
                api_error(
                    stream,
                    "400 Bad Request",
                    "storage_symbol is not an active registered ticker for this app",
                );
                return Ok(());
            }
            Err(error) => {
                api_error(stream, "500 Internal Server Error", error);
                return Ok(());
            }
        }
    } else if exchange.is_empty() {
        match resolve_candle_storage_symbol_any_exchange(&connection, &symbol, Some(&asset_class)) {
            Ok(resolved) => resolved,
            Err(error) => {
                api_error(stream, "400 Bad Request", error);
                return Ok(());
            }
        }
    } else {
        match resolve_candle_storage_symbol(&connection, &exchange, &symbol, Some(&asset_class)) {
            Ok(resolved) => resolved,
            Err(error) => {
                api_error(stream, "500 Internal Server Error", error);
                return Ok(());
            }
        }
    };
    state.prioritize_chart_sync(&storage_symbol);
    let market_connection = match open_market_history_for_symbol(db, &storage_symbol, false) {
        Ok(Some(connection)) => connection,
        Ok(None) => {
            api_error(
                stream,
                "503 Service Unavailable",
                "ticker history is not available yet",
            );
            return Ok(());
        }
        Err(error) => {
            api_error(stream, "503 Service Unavailable", error);
            return Ok(());
        }
    };
    let market_connection = &market_connection;
    // Intraday context includes 15m and 1h bars for chart and Pine
    // higher-timeframe requests. Higher views keep their existing direct
    // or lower-timeframe storage resolution. Metadata retains each source.
    let timeframes = DSS_BRESSERT_MTF_TIMEFRAMES
        .iter()
        .map(|definition| (definition.timeframe, 120_i64))
        .collect::<Vec<_>>();
    let mut result = serde_json::Map::new();
    let mut markov_regimes = serde_json::Map::new();
    let mut freshness = serde_json::Map::new();
    let mut bundle_degraded = false;
    let regime_config = quant_core::MarkovRegimeConfig::default();
    for (timeframe, timeframe_limit) in timeframes {
        let requested = timeframe_limit.min(limit);
        let calculation_limit = requested
            .saturating_add(regime_config.chart_context_warmup_bars() as i64)
            .min(MAX_CANDLES);
        let mut metadata = if exchange.is_empty() {
            load_series_metadata(market_connection, &storage_symbol, timeframe, false)
        } else {
            load_series_metadata_for_exchange(
                market_connection,
                &storage_symbol,
                timeframe,
                false,
                source_exchange,
            )
        }
        .unwrap_or_else(|error| {
            serde_json::json!({
                "exists": false,
                "requested_timeframe": timeframe,
                "storage_timeframe": timeframe,
                "is_stale": true,
                "warning": format!("Unable to read local history metadata: {error}"),
            })
        });
        let candle_result = if exchange.is_empty() {
            load_series_candles(
                market_connection,
                &storage_symbol,
                timeframe,
                calculation_limit,
            )
        } else {
            load_series_candles_for_exchange(
                market_connection,
                &storage_symbol,
                timeframe,
                calculation_limit,
                source_exchange,
            )
        };
        match candle_result {
            Ok(mut candles) => {
                let chart_status = if candles.len() < MIN_CHART_CANDLES {
                    bundle_degraded = true;
                    "insufficient_data"
                } else {
                    "ready"
                };
                if let Some(object) = metadata.as_object_mut() {
                    object.insert("chart_status".into(), serde_json::json!(chart_status));
                    object.insert(
                        "markov_calculation_bars".into(),
                        serde_json::json!(candles.len()),
                    );
                    object.insert(
                        "chart_min_bars".into(),
                        serde_json::json!(MIN_CHART_CANDLES),
                    );
                    if chart_status == "insufficient_data" {
                        object.insert(
                                "chart_warning".into(),
                                serde_json::json!(format!(
                                    "At least {MIN_CHART_CANDLES} chronological candles are required to render this chart"
                                )),
                            );
                    }
                }
                freshness.insert(timeframe.to_string(), metadata);
                let markov_bars = candles
                    .iter()
                    .map(|candle| quant_core::StrategyBar {
                        time: candle.time,
                        open: candle.open,
                        high: candle.high,
                        low: candle.low,
                        close: candle.close,
                        volume: candle.volume,
                    })
                    .collect::<Vec<_>>();
                let regimes = quant_core::classify_regimes(&markov_bars, &regime_config);
                let regime_start = candles.len().saturating_sub(requested as usize);
                markov_regimes.insert(
                    timeframe.to_string(),
                    serde_json::Value::Array(
                        candles
                            .iter()
                            .zip(regimes.iter())
                            .skip(regime_start)
                            .map(|(candle, regime)| {
                                serde_json::json!({
                                    "time": candle.time,
                                    "state": regime.map(|value| value.name()),
                                })
                            })
                            .collect(),
                    ),
                );
                trim_candles(&mut candles, requested);
                result.insert(
                    timeframe.to_string(),
                    serde_json::Value::Array(super::super::chart_market_bars(&candles)),
                );
            }
            Err(error) => {
                api_error(stream, "500 Internal Server Error", error);
                return Ok(());
            }
        }
    }
    respond(
        stream,
        crate::wire::encode(&serde_json::json!({
            "status": "ok",
            "chart_status": if bundle_degraded { "degraded" } else { "ready" },
            "service": "rust_quant_engine",
            "version": SERVICE_VERSION,
            "symbol": symbol,
            "storage_symbol": storage_symbol,
            "exchange": exchange,
            "storage_mode": "sqlite_local_history",
            "provider_sync_allowed": provider_sync_enabled(),
            "provider_sync_by_exchange": provider_sync_by_exchange(),
            "provider_fallback_by_exchange": provider_fallback_by_exchange(),
            "series": freshness,
            "timeframes": result,
            "markov_regimes": markov_regimes,
        }))
        .unwrap_or_default(),
        "200 OK",
    );
    return Ok(());
}
