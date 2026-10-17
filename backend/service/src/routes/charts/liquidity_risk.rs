use super::super::{api_error, query_values, respond, RequestContext};
use crate::*;

pub(super) fn handle<'a>(request: RequestContext<'a>) -> Result<(), RequestContext<'a>> {
    let RequestContext {
        stream, db, path, ..
    } = request;
    let values = query_values(path);
    let symbol = values
        .get("symbol")
        .cloned()
        .unwrap_or_default()
        .to_uppercase();
    if symbol.is_empty() {
        api_error(stream, "400 Bad Request", "symbol is required");
        return Ok(());
    }
    let timeframe = values
        .get("timeframe")
        .filter(|value| !value.is_empty())
        .cloned()
        .unwrap_or_else(|| "1d".to_string());
    let exchange = values
        .get("exchange")
        .or_else(|| values.get("venue"))
        .map(|value| value.trim().to_ascii_lowercase())
        .unwrap_or_else(|| {
            if symbol.ends_with(".JK") {
                "stockbit".into()
            } else {
                "pintu_futures".into()
            }
        });
    let parse_positive = |key: &str| {
        values
            .get(key)
            .and_then(|value| value.parse::<f64>().ok())
            .filter(|value| value.is_finite() && *value > 0.0)
    };
    let entry_price = match parse_positive("entry_price") {
        Some(value) => value,
        None => {
            api_error(
                stream,
                "400 Bad Request",
                "entry_price is required and must be positive",
            );
            return Ok(());
        }
    };
    let stop_loss = match parse_positive("stop_loss") {
        Some(value) => value,
        None => {
            api_error(
                stream,
                "400 Bad Request",
                "stop_loss is required and must be positive",
            );
            return Ok(());
        }
    };
    let current_price = parse_positive("current_price");
    let asset_class = values
        .get("asset_class")
        .filter(|value| !value.is_empty())
        .cloned()
        .unwrap_or_else(|| {
            if symbol.ends_with(".JK") {
                "idx".to_string()
            } else {
                "crypto_futures".to_string()
            }
        });
    let direction = values
        .get("direction")
        .filter(|value| !value.is_empty())
        .cloned()
        .unwrap_or_else(|| "LONG".to_string());
    let lookback_bars = values
        .get("lookback_bars")
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(100)
        .min(MAX_CANDLES as usize);
    let fractal_window = values
        .get("fractal_window")
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(2)
        .min(20);
    let include_bsl_ssl = values
        .get("include_bsl_ssl")
        .map(|value| matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false);
    let targets = values
        .get("targets")
        .map(|raw| {
            raw.split(',')
                .filter_map(|value| value.trim().parse::<f64>().ok())
                .filter(|value| value.is_finite() && *value > 0.0)
                .take(8)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let connection = match open_db_minimal(db) {
        Ok(connection) => connection,
        Err(error) => {
            api_error(stream, "500 Internal Server Error", error);
            return Ok(());
        }
    };
    let storage_symbol =
        match resolve_candle_storage_symbol(&connection, &exchange, &symbol, Some(&asset_class)) {
            Ok(storage_symbol) => storage_symbol,
            Err(error) => {
                api_error(stream, "400 Bad Request", error);
                return Ok(());
            }
        };
    let source_limit = (lookback_bars as i64 + 120).clamp(15, MAX_CANDLES);
    let market_connection = match open_market_history_for_symbol(db, &storage_symbol, false) {
        Ok(connection) => connection,
        Err(error) => {
            api_error(stream, "503 Service Unavailable", error);
            return Ok(());
        }
    };
    let candles = if let Some(market_connection) = market_connection.as_ref() {
        match load_series_candles_for_exchange(
            market_connection,
            &storage_symbol,
            &timeframe,
            source_limit,
            &exchange,
        ) {
            Ok(candles) => candles,
            Err(error) => {
                api_error(stream, "503 Service Unavailable", error);
                return Ok(());
            }
        }
    } else {
        Vec::new()
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
    if bars.len() < 15 {
        respond(
            stream,
            crate::wire::encode(&serde_json::json!({
                "status": "insufficient_data",
                "service": "rust_quant_engine",
                "version": SERVICE_VERSION,
                "symbol": storage_symbol,
                "timeframe": timeframe,
                "zones": [],
                "report": null,
                "error": "at least 15 chronological candles are required for liquidity analysis"
            }))
            .unwrap_or_default(),
            "422 Unprocessable Entity",
        );
        return Ok(());
    }
    let result = compute_liquidity_risk(
        &bars,
        current_price,
        &asset_class,
        entry_price,
        stop_loss,
        &targets,
        &direction,
        fractal_window,
        lookback_bars,
        include_bsl_ssl,
    );
    respond(
            stream,
            crate::wire::encode(&serde_json::json!({
                "status": "ok",
                "service": "rust_quant_engine",
                "version": SERVICE_VERSION,
                "symbol": storage_symbol,
                "timeframe": timeframe,
                "asset_class": asset_class,
                "direction": direction,
                "entry_price": entry_price,
                "stop_loss": stop_loss,
                "targets": targets,
                "current_price": result.current_price,
                "lookback_bars": result.lookback_bars,
                "zones": result.zones,
                "report": result.report,
                "explainability": [
                    "Liquidity zones are estimated from bounded OHLCV candles; no order-book or funding data is fabricated.",
                    "Liquidation shelves use the configured 10x and 25x stress tiers anchored to the recent 40-bar range.",
                    "Stop-hunt and target-magnet decisions preserve the venue-aware plan audit contract."
                ]
            })).unwrap_or_default(),
            "200 OK",
        );
    return Ok(());
}
