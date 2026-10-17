use super::{api_error, parse_query, respond, RequestContext};
use crate::*;

pub(super) fn handle<'a>(request: RequestContext<'a>) -> Result<(), RequestContext<'a>> {
    let RequestContext {
        stream,
        db,
        path,
        route,
        ..
    } = request;
    let query = parse_query(path);
    if route == "/api/compute/risk" {
        let (entry_price, stop_loss) = match (query.entry_price, query.stop_loss) {
            (Some(entry), Some(stop)) if entry > 0.0 && stop > 0.0 => (entry, stop),
            _ => {
                api_error(
                    stream,
                    "400 Bad Request",
                    "entry_price and stop_loss are required and must be positive",
                );
                return Ok(());
            }
        };
        let request = PositionSizeRequest {
            venue: query.venue.clone(),
            entry_price,
            stop_loss,
            asset_class: query.asset_class.clone(),
            direction: query.direction.clone(),
            equity: query.initial_equity,
            available_cash: query.available_cash,
            atr: query.atr,
            leverage: query.leverage,
            win_rate: query.win_rate,
            payoff: query.payoff,
            sizing_model: query.sizing_model.clone(),
            current_equity: query.current_equity,
            peak_equity: query.peak_equity,
        };
        let result = calculate_position_size(&request);
        respond(
            stream,
            crate::wire::encode(&serde_json::json!({
                "status": "ok",
                "service": "rust_quant_engine",
                "version": SERVICE_VERSION,
                "symbol": query.symbol,
                "venue": query.venue,
                "asset_class": query.asset_class,
                "request": request,
                "position_size": result,
                "explainability": [
                    "Risk budget is computed from equity and the configured sizing model.",
                    "Physical quantity is quantized to venue rules before final approval.",
                    "Futures liquidation price and stop-loss ordering are checked before approval.",
                    "This endpoint uses no fabricated funding, mark price, or order-book data."
                ]
            }))
            .unwrap_or_default(),
            "200 OK",
        );
        return Ok(());
    }
    if query.symbol.is_empty() {
        api_error(stream, "400 Bad Request", "symbol is required");
        return Ok(());
    }
    let connection = match open_db_minimal(db) {
        Ok(connection) => connection,
        Err(error) => {
            api_error(stream, "400 Bad Request", error);
            return Ok(());
        }
    };
    let storage_symbol = match resolve_candle_storage_symbol(
        &connection,
        &query.venue,
        &query.symbol,
        Some(&query.asset_class),
    ) {
        Ok(symbol) => symbol,
        Err(error) => {
            api_error(stream, "500 Internal Server Error", error);
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
    let market_connection = market_connection.as_ref();
    let is_daily_tsmom = matches!(
        query.strategy.as_str(),
        "idx_stockbit_time_series_momentum_swing" | "stockbit_tsmom_swing"
    );
    let minimum_signal_history_bars = quant_core::NadarayaWatsonEnvelopeConfig::default()
        .minimum_endpoint_envelope_bars()
        .max(if is_daily_tsmom { 253 } else { 0 }) as i64;
    let load_limit = if route == "/api/chart/candles" || route == "/api/compute/indicators" {
        MAX_CANDLES
    } else if route == "/api/compute/signals" {
        query
            .limit
            .max(minimum_signal_history_bars)
            .min(MAX_CANDLES)
    } else {
        query.limit
    };

    // Chart reads are local-only. Provider access belongs to the explicit
    // scan/refresh flow so a chart cannot block on a network retry or mutate
    // its own history while the user is changing timeframes. The returned
    // series metadata exposes the precise last bar, age, and sync failure.
    let mut candles = if let Some(market_connection) = market_connection {
        match load_series_candles_for_exchange(
            market_connection,
            &storage_symbol,
            &query.timeframe,
            load_limit,
            &query.venue,
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
    let freshness = match market_connection {
        Some(market_connection) => load_series_metadata_for_exchange(
            market_connection,
            &storage_symbol,
            &query.timeframe,
            false,
            &query.venue,
        )
        .unwrap_or_else(|error| {
            serde_json::json!({
                "exists": false,
                "requested_timeframe": query.timeframe,
                "storage_timeframe": query.timeframe,
                "is_stale": true,
                "warning": format!("Unable to read local history metadata: {error}"),
            })
        }),
        None => serde_json::json!({
            "exists": false,
            "requested_timeframe": query.timeframe,
            "storage_timeframe": query.timeframe,
            "is_stale": true,
            "warning": "Ticker history database is missing",
            "recovery": "Use the retry action after market history is restored",
        }),
    };

    if route == "/api/compute/signals" {
        if matches!(
            query.strategy.as_str(),
            "idx_stockbit_time_series_momentum_swing" | "stockbit_tsmom_swing"
        ) && query.timeframe != "1d"
        {
            api_error(
                stream,
                "400 Bad Request",
                "Stockbit 12–1 month momentum research requires daily candles",
            );
            return Ok(());
        }
        if matches!(
            query.strategy.as_str(),
            "idx_stockbit_opening_range_momentum" | "stockbit_orb_momentum"
        ) && query.timeframe != "15m"
        {
            api_error(
                stream,
                "400 Bad Request",
                "Stockbit opening-range momentum research requires 15m candles",
            );
            return Ok(());
        }
        if matches!(
            query.strategy.as_str(),
            "idx_stockbit_first_half_hour_momentum"
                | "stockbit_first_half_hour_momentum"
                | "idx_stockbit_time_of_day_momentum"
                | "stockbit_time_of_day_momentum"
        ) && query.timeframe != "15m"
        {
            api_error(
                stream,
                "400 Bad Request",
                "Stockbit intraday momentum research requires 15m candles",
            );
            return Ok(());
        }
        if is_pintu_golden_cross_strategy(&query.strategy)
            && query.timeframe != PINTU_GOLDEN_CROSS_TIMEFRAME
        {
            api_error(
                stream,
                "400 Bad Request",
                "Pintu Futures 15m golden-cross research with 1h DSS oversold in the prior 24 hours, recent 15m DSS extreme turn, bullish MSS, and 3R minimum requires 15m candles",
            );
            return Ok(());
        }
        let bars: Vec<_> = candles
            .iter()
            .map(|candle| StrategyBar {
                time: candle.time,
                open: candle.open,
                high: candle.high,
                low: candle.low,
                close: candle.close,
                volume: candle.volume,
            })
            .collect();
        let signals = match query.strategy.as_str() {
            "pintu_futures_day_trade" | "pintu" | "pintu_futures" => {
                generate_pintu_signals(&bars, PintuStrategyConfig::default())
            }
            strategy if is_pintu_golden_cross_strategy(strategy) => {
                generate_pintu_golden_cross_signals(&bars, PintuGoldenCrossConfig::default())
            }
            strategy if is_tokocrypto_confluence_strategy(strategy) => {
                generate_tokocrypto_confluence_signals(&bars, TokocryptoConfluenceConfig::default())
            }
            "idx_stockbit_day_trade" | "stockbit" | "idx" => {
                generate_idx_signals(&bars, IdxStrategyConfig::default())
            }
            "idx_stockbit_swing" | "stockbit_swing" => {
                generate_idx_signals(&bars, IdxStrategyConfig::swing())
            }
            "idx_stockbit_time_series_momentum_swing" | "stockbit_tsmom_swing" => {
                generate_idx_time_series_momentum_signals(
                    &bars,
                    IdxTimeSeriesMomentumConfig::default(),
                )
            }
            "idx_stockbit_opening_range_momentum" | "stockbit_orb_momentum" => {
                generate_idx_opening_range_signals(&bars, IdxOpeningRangeConfig::default())
            }
            "idx_stockbit_first_half_hour_momentum" | "stockbit_first_half_hour_momentum" => {
                generate_idx_first_half_hour_momentum_signals(
                    &bars,
                    IdxIntradayMomentumConfig::default(),
                )
            }
            "idx_stockbit_time_of_day_momentum" | "stockbit_time_of_day_momentum" => {
                generate_idx_time_of_day_momentum_signals(
                    &bars,
                    IdxIntradayMomentumConfig::default(),
                )
            }
            "multibagger" | "stockbit_multibagger" => {
                generate_idx_signals(&bars, IdxStrategyConfig::default())
            }
            strategy => {
                api_error(
                    stream,
                    "400 Bad Request",
                    format!("unsupported strategy: {strategy}"),
                );
                return Ok(());
            }
        };
        respond(
            stream,
            crate::wire::encode(&serde_json::json!({
                "service": "rust_quant_engine",
                "version": SERVICE_VERSION,
                "symbol": query.symbol,
                "timeframe": query.timeframe,
                "strategy": query.strategy,
                "count": signals.len(),
                "signals": signals,
            }))
            .unwrap_or_default(),
            "200 OK",
        );
        return Ok(());
    }
    let indicator_settings = ChartIndicatorSettings::default();
    if route == "/api/compute/indicators" {
        attach_dss(&mut candles, &query.dss);
        indicator_settings.attach(&mut candles);
    }
    trim_candles(&mut candles, query.limit);

    if route == "/api/chart/candles" && candles.len() < MIN_CHART_CANDLES {
        let mut response = freshness;
        if let Some(object) = response.as_object_mut() {
            object.insert("status".into(), serde_json::json!("insufficient_data"));
            object.insert(
                "warning".into(),
                serde_json::json!(format!(
                    "At least {MIN_CHART_CANDLES} chronological candles are required to render a chart"
                )),
            );
        }
        respond(
            stream,
            crate::wire::encode(&serde_json::json!({
                "service": "rust_quant_engine",
                "version": SERVICE_VERSION,
                "symbol": query.symbol,
                "timeframe": query.timeframe,
                "storage_mode": "sqlite_local_history",
                "status": "insufficient_data",
                "provider_sync_allowed": provider_sync_enabled(),
                "provider_sync_by_exchange": provider_sync_by_exchange(),
                "provider_fallback_by_exchange": provider_fallback_by_exchange(),
                "series": response,
                "count": candles.len(),
                "candles": super::chart_market_bars(&candles),
            }))
            .unwrap_or_default(),
            "200 OK",
        );
        return Ok(());
    }

    if route == "/api/compute/indicators" {
        let points: Vec<_> = candles
            .iter()
            .map(|candle| {
                serde_json::json!({
                    "time": candle.time,
                    "dss": candle.dss,
                    "trigger": candle.trigger,
                    "gradient": candle.gradient,
                    "indicators": &candle.indicators,
                })
            })
            .collect();
        respond(
            stream,
            crate::wire::encode(&serde_json::json!({
                "service": "rust_quant_engine",
                "version": SERVICE_VERSION,
                "symbol": query.symbol,
                "timeframe": query.timeframe,
                "storage_mode": "sqlite_local_history",
                "provider_sync_allowed": provider_sync_enabled(),
                "provider_sync_by_exchange": provider_sync_by_exchange(),
                "provider_fallback_by_exchange": provider_fallback_by_exchange(),
                "series": freshness,
                "count": points.len(),
                "config": query.dss,
                "indicator_config": indicator_settings.metadata(),
                "points": points,
            }))
            .unwrap_or_default(),
            "200 OK",
        );
    } else {
        respond(
            stream,
            crate::wire::encode(&serde_json::json!({
                "service": "rust_quant_engine",
                "version": SERVICE_VERSION,
                "symbol": query.symbol,
                "timeframe": query.timeframe,
                "storage_mode": "sqlite_local_history",
                "provider_sync_allowed": provider_sync_enabled(),
                "provider_sync_by_exchange": provider_sync_by_exchange(),
                "provider_fallback_by_exchange": provider_fallback_by_exchange(),
                "series": freshness,
                "count": candles.len(),
                "candles": super::chart_market_bars(&candles),
            }))
            .unwrap_or_default(),
            "200 OK",
        );
    }
    Ok(())
}
