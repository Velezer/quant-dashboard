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
    let asset = values
        .get("asset")
        .filter(|value| !value.is_empty())
        .cloned()
        .unwrap_or_else(|| symbol.clone());
    let asset_class = values
        .get("asset_class")
        .filter(|value| !value.is_empty())
        .cloned()
        .unwrap_or_else(|| {
            if symbol.ends_with(".JK") {
                "idx".into()
            } else {
                "crypto_futures".into()
            }
        });
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
    let parse = |key: &str, default: f64| {
        values
            .get(key)
            .and_then(|value| value.parse::<f64>().ok())
            .filter(|value| value.is_finite())
            .unwrap_or(default)
    };
    let optional_positive = |key: &str| {
        values
            .get(key)
            .and_then(|value| value.parse::<f64>().ok())
            .filter(|value| value.is_finite() && *value > 0.0)
    };
    let current_price = parse("current_price", 0.0);
    let stop_loss = parse("stop_loss", 0.0);
    let tp1 = parse("tp1", 0.0);
    let direction = values
        .get("direction")
        .cloned()
        .unwrap_or_else(|| "LONG".into());
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
    let market_connection = match open_market_history_for_symbol(db, &storage_symbol, false) {
        Ok(connection) => connection,
        Err(error) => {
            api_error(stream, "503 Service Unavailable", error);
            return Ok(());
        }
    };
    let (daily, intraday) = if let Some(market_connection) = market_connection.as_ref() {
        let daily = match load_series_candles_for_exchange(
            market_connection,
            &storage_symbol,
            "1d",
            MAX_CANDLES,
            &exchange,
        ) {
            Ok(candles) => candles,
            Err(error) => {
                api_error(stream, "503 Service Unavailable", error);
                return Ok(());
            }
        };
        let intraday = match load_series_candles_for_exchange(
            market_connection,
            &storage_symbol,
            "15m",
            MAX_CANDLES,
            &exchange,
        ) {
            Ok(candles) => candles,
            Err(error) => {
                api_error(stream, "503 Service Unavailable", error);
                return Ok(());
            }
        };
        (daily, intraday)
    } else {
        (Vec::new(), Vec::new())
    };
    let to_bars = |candles: Vec<Candle>| {
        candles
            .into_iter()
            .map(|candle| StrategyBar {
                time: candle.time,
                open: candle.open,
                high: candle.high,
                low: candle.low,
                close: candle.close,
                volume: candle.volume,
            })
            .collect::<Vec<_>>()
    };
    let daily_bars = to_bars(daily);
    let intraday_bars = to_bars(intraday);
    let mut profile = compute_day_trade_profile(
        &asset,
        &asset_class,
        current_price,
        stop_loss,
        tp1,
        &direction,
        optional_positive("position_notional"),
        optional_positive("margin_allocated"),
        &daily_bars,
        &intraday_bars,
    );
    if asset_class == "idx" || symbol.ends_with(".JK") {
        if let Some(profile_object) = profile.as_mut().and_then(Value::as_object_mut) {
            if let Ok(flow) = crate::foreign_flow::summary_for_symbol(&connection, &storage_symbol)
            {
                profile_object.insert("stockbit_foreign_flow".to_string(), flow);
            }
        }
    }
    respond(
            stream,
            crate::wire::encode(&serde_json::json!({
                "status": "ok",
                "service": "rust_quant_engine",
                "version": SERVICE_VERSION,
                "operation": "day_trade_profile",
                "symbol": storage_symbol,
                "asset": asset,
                "asset_class": asset_class,
                "profile": profile,
                "explainability": [
                    "Stockbit profiles use bounded daily OHLCV, legal ARA/ARB limits, tick rounding, O=L/breakout/RVOL eligibility, and the same-day cutoff rule.",
                    "Stockbit day-trade profiles include imported session-based foreign-flow context when available; flow does not create a standalone entry or validate the win-rate goal.",
                    "Pintu profiles use the plan's actual levels for price change and optional margin-based ROI; funding is informational and does not create an expiry.",
                    "Missing bars or unsupported venues return a null profile rather than fabricating a setup."
                ],
            })).unwrap_or_default(),
            "200 OK",
        );
    return Ok(());
}
