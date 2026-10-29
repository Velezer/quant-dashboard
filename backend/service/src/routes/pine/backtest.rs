use super::*;

pub(super) fn run_registered_pine_strategy(
    db: &str,
    request: StrategyBacktestRequest,
    job: &PineBacktestJob,
) -> Result<serde_json::Value, (&'static str, String)> {
    let max_history_bars = request
        .max_history_bars
        .unwrap_or(quant_pine_runtime::MAX_STRATEGY_CANDLES);
    if max_history_bars == 0 || max_history_bars > quant_pine_runtime::MAX_STRATEGY_CANDLES {
        return Err((
            "400 Bad Request",
            format!(
                "Deep backtest history must be between 1 and {} candles.",
                quant_pine_runtime::MAX_STRATEGY_CANDLES
            ),
        ));
    }
    let walk_forward_training_percent = request.walk_forward_training_percent.unwrap_or(70);
    if !(50..=90).contains(&walk_forward_training_percent)
        || walk_forward_training_percent % 10 != 0
    {
        return Err((
            "400 Bad Request",
            "Chronological split training share must be 50, 60, 70, 80, or 90 percent.".to_string(),
        ));
    }
    let requested_range = match (request.from_time_ms, request.to_time_exclusive_ms) {
        (None, None) => None,
        (Some(from), Some(to)) if from < to => Some((
            from.checked_mul(quant_core::NANOS_PER_MILLISECOND)
                .ok_or_else(|| {
                    (
                        "400 Bad Request",
                        "Backtest start time is outside the supported timestamp range.".to_string(),
                    )
                })?,
            to.checked_mul(quant_core::NANOS_PER_MILLISECOND)
                .ok_or_else(|| {
                    (
                        "400 Bad Request",
                        "Backtest end time is outside the supported timestamp range.".to_string(),
                    )
                })?,
        )),
        (Some(_), Some(_)) => {
            return Err((
                "400 Bad Request",
                "Backtest start date must be before the exclusive end date.".to_string(),
            ))
        }
        _ => {
            return Err((
                "400 Bad Request",
                "Backtest date range requires both a start date and an end date.".to_string(),
            ))
        }
    };
    let app =
        open_db_minimal(db).map_err(|error| ("500 Internal Server Error", error.to_string()))?;
    let exchange_record = exchange_by_alias(&request.exchange_id)
        .filter(|record| catalog_string(record, "status") == Some("enabled"))
        .ok_or_else(|| {
            (
                "400 Bad Request",
                "Pine backtests require an enabled app exchange.".to_string(),
            )
        })?;
    let exchange_id = catalog_string(exchange_record, "id").ok_or_else(|| {
        (
            "400 Bad Request",
            "Enabled exchange is missing its canonical id.".to_string(),
        )
    })?;
    let category_record = category_by_id_or_research_alias(&request.asset_category_id)
        .filter(|record| {
            record
                .get("exchange_ids")
                .and_then(serde_json::Value::as_array)
                .is_some_and(|ids| ids.iter().any(|id| id.as_str() == Some(exchange_id)))
        })
        .ok_or_else(|| {
            (
                "400 Bad Request",
                "Asset category does not belong to the selected app.".to_string(),
            )
        })?;
    let category_id = catalog_string(category_record, "id").ok_or_else(|| {
        (
            "400 Bad Request",
            "Category is missing its canonical id.".to_string(),
        )
    })?;
    if !SIGNAL_SCREENER_TIMEFRAMES
        .iter()
        .any(|item| *item == request.timeframe)
    {
        return Err((
            "400 Bad Request",
            "Timeframe is not supported by the bounded Pine backtest runtime.".to_string(),
        ));
    }
    let strategies = pine_registry::list_strategies(&app, Some(request.strategy_id))
        .map_err(|error| ("500 Internal Server Error", error.to_string()))?;
    let strategy = strategies.first().ok_or_else(|| {
        (
            "404 Not Found",
            "Registered Pine strategy was not found.".to_string(),
        )
    })?;
    let storage_symbol = instrument_registry::storage_symbol_for_exchange_category(
        &app,
        exchange_id,
        category_id,
        &request.symbol,
    )
    .map_err(|error| ("500 Internal Server Error", error.to_string()))?
    .ok_or_else(|| {
        (
            "404 Not Found",
            "Ticker is not an active registered instrument in this app/category.".to_string(),
        )
    })?;
    let product_matches = app.query_row(
        "SELECT 1 FROM app.instruments WHERE exchange_id=?1 AND asset_category_id=?2 AND product_id=?3 AND storage_symbol=?4 AND is_active=1 AND listing_status <> 'venue_delisted' LIMIT 1",
        params![exchange_id, category_id, request.product_id, storage_symbol], |_| Ok(true),
    ).optional().map_err(|error| ("500 Internal Server Error", error.to_string()))?.unwrap_or(false);
    if !product_matches {
        return Err((
            "404 Not Found",
            "Ticker is not registered in the selected product.".to_string(),
        ));
    }
    let contract_metadata_json = app.query_row(
        "SELECT contract_metadata_json FROM app.instruments WHERE exchange_id=?1 AND product_id=?2 AND storage_symbol=?3 AND is_active=1 ORDER BY instrument_id LIMIT 1",
        params![exchange_id, request.product_id, storage_symbol],
        |row| row.get::<_, String>(0),
    ).optional().map_err(|error| ("500 Internal Server Error", error.to_string()))?;
    let instrument_tick_size = contract_metadata_json
        .map(|encoded| {
            let metadata: serde_json::Value = serde_json::from_str(&encoded).map_err(|error| {
                (
                    "500 Internal Server Error",
                    format!("Registered instrument contract metadata is invalid JSON: {error}"),
                )
            })?;
            Ok::<_, (&str, String)>(
                metadata
                    .get("price_tick")
                    .and_then(serde_json::Value::as_f64)
                    .filter(|tick| tick.is_finite() && *tick > 0.0),
            )
        })
        .transpose()?
        .flatten();

    let history = open_market_history_for_symbol(db, &storage_symbol, false)
        .map_err(|error| ("500 Internal Server Error", error.to_string()))?
        .ok_or_else(|| {
            (
                "404 Not Found",
                "No stored market history exists for this ticker.".to_string(),
            )
        })?;
    let source = selected_candle_source_for_product(
        &history,
        &storage_symbol,
        exchange_id,
        &request.product_id,
        &request.timeframe,
    )
    .map_err(|error| ("500 Internal Server Error", error.to_string()))?
    .ok_or_else(|| {
        (
            "404 Not Found",
            "No selected-app history exists for this product and timeframe.".to_string(),
        )
    })?;
    let metadata = load_product_candle_metadata(
        &history,
        &storage_symbol,
        exchange_id,
        &request.product_id,
        &request.timeframe,
    )
    .map_err(|error| ("500 Internal Server Error", error.to_string()))?;
    let load_window = |timeframe: &str, limit: i64| match requested_range {
        Some((from_time, to_time)) => load_series_candles_from_source_between_with_cap(
            &history,
            &storage_symbol,
            timeframe,
            limit,
            &source,
            MAX_STRATEGY_AGGREGATION_SOURCE_CANDLES,
            from_time,
            to_time,
        ),
        None if timeframe == "1d" => {
            load_candles_from_source(&history, &storage_symbol, timeframe, limit, &source)
        }
        None => load_strategy_history(&history, &storage_symbol, timeframe, limit, &source),
    };
    let candles = load_window(&request.timeframe, max_history_bars as i64)
        .map_err(|error| ("500 Internal Server Error", error.to_string()))?;
    if candles.is_empty() {
        let message = if requested_range.is_some() {
            "No stored candles fall inside the selected UTC date range for this strategy timeframe."
        } else {
            "Selected app history has no candles for this strategy timeframe."
        };
        return Err(("422 Unprocessable Content", message.to_string()));
    }
    let (instrument_kind, data_provider) = history.query_row(
        "SELECT instrument_kind,COALESCE(data_provider,'unknown') FROM series WHERE symbol=?1 AND interval=?2 AND source=?3 ORDER BY CASE WHEN exchange_id=?4 AND product_id=?5 THEN 0 ELSE 1 END LIMIT 1",
        params![storage_symbol, request.timeframe, source, exchange_id, request.product_id],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    ).optional().map_err(|error| ("500 Internal Server Error", error.to_string()))?.unwrap_or_else(|| ("unknown".to_string(), "unknown".to_string()));
    let fee_policy = quant_core::venue_fee_policy_for_category(exchange_id, category_id);
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
    let convert_candles = |bars: &[Candle]| {
        bars.iter()
            .map(|bar| quant_pine_runtime::Candle {
                open: bar.open,
                high: bar.high,
                low: bar.low,
                close: bar.close,
                volume: bar.volume,
                time: Some(bar.time),
            })
            .collect::<Vec<_>>()
    };
    let mut timeframe_candles = HashMap::new();
    let chart_duration_ns = timeframe_duration_ns(&request.timeframe).unwrap_or(i64::MAX);
    for timeframe in ["15m", "30m", "1h", "4h", "1d"] {
        if timeframe == request.timeframe {
            continue;
        }
        if timeframe_duration_ns(timeframe).unwrap_or_default() < chart_duration_ns {
            continue;
        }
        let bars = load_window(timeframe, max_history_bars as i64)
            .map_err(|error| ("500 Internal Server Error", error.to_string()))?;
        if !bars.is_empty() {
            timeframe_candles.insert(timeframe.to_string(), convert_candles(&bars));
        }
    }
    let runtime_candles = convert_candles(&candles);
    let uses_bar_magnifier = quant_pine_runtime::strategy_uses_bar_magnifier(&strategy.source)
        .map_err(|diagnostics| {
            (
                "422 Unprocessable Content",
                diagnostics
                    .into_iter()
                    .map(|item| item.message)
                    .collect::<Vec<_>>()
                    .join(" "),
            )
        })?;
    let magnifier_timeframe = (uses_bar_magnifier && request.timeframe != "15m").then_some("15m");
    let magnifier_bars = match magnifier_timeframe {
        Some(timeframe) => load_series_candles_from_source_between_with_cap(
            &history,
            &storage_symbol,
            timeframe,
            quant_pine_runtime::MAX_STRATEGY_MAGNIFIER_CANDLES as i64,
            &source,
            MAX_STRATEGY_AGGREGATION_SOURCE_CANDLES,
            candles
                .first()
                .expect("nonempty chart history was checked")
                .time,
            candles
                .last()
                .expect("nonempty chart history was checked")
                .time
                .saturating_add(timeframe_duration_ns(&request.timeframe).unwrap_or_default()),
        )
        .map_err(|error| ("500 Internal Server Error", error.to_string()))?,
        None => Vec::new(),
    };
    let runtime_magnifier_bars = convert_candles(&magnifier_bars);
    job.set_phase("evaluating full history");
    let full_history_progress =
        |completed, total| job.set_progress(completed, total, "evaluating full history", 0, 7_000);
    let output = quant_pine_runtime::run_strategy_with_history_limit_and_magnifier_and_tick_size_and_evaluation_start_and_progress(
        &strategy.source,
        &runtime_candles,
        &request.timeframe,
        &timeframe_candles,
        &runtime_magnifier_bars,
        magnifier_timeframe,
        max_history_bars,
        buy_fee_pct,
        sell_fee_pct,
        &request.inputs,
        instrument_tick_size,
        0,
        Some(&full_history_progress),
    )
        .map_err(|diagnostics| ("422 Unprocessable Content", diagnostics.into_iter().map(|item| item.message).collect::<Vec<_>>().join(" ")))?;
    let commission_return_basis =
        output
            .commission
            .map(|commission| match commission.commission_type {
                quant_pine_runtime::StrategyCommissionType::Percent => {
                    "after Pine strategy.commission.percent"
                }
                quant_pine_runtime::StrategyCommissionType::CashPerOrder => {
                    "after Pine strategy.commission.cash_per_order in chart currency"
                }
                quant_pine_runtime::StrategyCommissionType::CashPerContract => {
                    "after Pine strategy.commission.cash_per_contract in chart currency"
                }
            });
    let (fees_known, fee_model) = output.commission
        .map(|commission| {
            let description = match commission.commission_type {
                quant_pine_runtime::StrategyCommissionType::Percent => format!("Pine strategy commission: {}% per transaction; replaces venue fee estimate", commission.value),
                quant_pine_runtime::StrategyCommissionType::CashPerOrder => format!("Pine strategy commission: {} chart-currency units per filled order; replaces venue fee estimate", commission.value),
                quant_pine_runtime::StrategyCommissionType::CashPerContract => format!("Pine strategy commission: {} chart-currency units per contract traded; replaces venue fee estimate", commission.value),
            };
            (true, description)
        })
        .unwrap_or((fees_known, fee_model.to_string()));
    let data_source = metadata
        .get("source")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(&source);
    let proxy = is_research_proxy_source(data_source) || instrument_kind.contains("proxy");
    let venue_native = is_native_candle_source_for_exchange(data_source, Some(exchange_id));
    let execution_scope = if proxy {
        "research_proxy"
    } else if venue_native {
        "venue_history"
    } else {
        "unverified_history"
    };
    let sample_trades = output.trades.len();
    let trade_analysis = serde_json::json!({
        "pnl_basis": if fees_known { "net after configured costs" } else { "gross; venue fees unknown" },
        "all": summarize_strategy_trades(&output.trades, None, fees_known),
        "long": summarize_strategy_trades(&output.trades, Some("LONG"), fees_known),
        "short": summarize_strategy_trades(&output.trades, Some("SHORT"), fees_known)
    });
    let trade_return_distribution = summarize_trade_return_distribution(&output.trades, fees_known);
    let return_total = output
        .trades
        .iter()
        .map(|trade| {
            if fees_known {
                trade.net_return_pct
            } else {
                trade.gross_return_pct
            }
        })
        .sum::<f64>();
    let mean_return_pct = (sample_trades > 0).then_some(return_total / sample_trades as f64);
    let benchmark = buy_and_hold_benchmark(
        output.initial_capital,
        &candles
            .iter()
            .map(|candle| candle.close)
            .collect::<Vec<_>>(),
    );
    let buy_and_hold_return_pct = benchmark.as_ref().map(|(return_pct, _)| *return_pct);
    let close_drawdown = summarize_close_drawdown(
        output.initial_capital,
        &output.equity_curve,
        &candles.iter().map(|candle| candle.time).collect::<Vec<_>>(),
    );
    job.set_phase("evaluating chronological validation");
    let validation_progress = |completed, total| {
        job.set_progress(
            completed,
            total,
            "evaluating chronological validation",
            7_000,
            2_500,
        )
    };
    let chronological_validation = summarize_chronological_validation(
        &strategy.source,
        walk_forward_training_percent,
        &candles,
        &runtime_candles,
        &request.timeframe,
        &timeframe_candles,
        &runtime_magnifier_bars,
        magnifier_timeframe,
        max_history_bars,
        buy_fee_pct,
        sell_fee_pct,
        &request.inputs,
        instrument_tick_size,
        fees_known,
        &output,
        Some(&validation_progress),
    );
    job.set_phase("assembling bounded report");
    job.set_progress(1, 1, "assembling bounded report", 9_500, 450);
    let backtest = serde_json::json!({
        "closed_trades": output.closed_trades,
        "history_limit_bars": max_history_bars,
        "wins": output.wins,
        "win_rate_pct": output.win_rate_pct,
        "mean_trade_return_pct": mean_return_pct,
        "return_basis": commission_return_basis.unwrap_or(if fees_known { "after configured venue fees" } else { "gross; venue fees unknown" }),
        "profit_factor": output.profit_factor,
        "profit_factor_unbounded": output.profit_factor_unbounded,
        "profit_factor_basis": "gross trade P&L; fees are reported separately",
        "trade_analysis": trade_analysis,
        "trade_return_distribution": trade_return_distribution,
        "fill_assumptions": {
            "slippage_ticks": output.slippage_ticks,
            "calc_on_order_fills": output.calc_on_order_fills,
            "instrument_tick_size": output.instrument_tick_size,
            "tick_size_source": output.instrument_tick_size.map(|_| "registered_instrument_contract_metadata").unwrap_or("unavailable"),
            "backtest_fill_limits_assumption_ticks": output.backtest_fill_limits_assumption_ticks,
            "bar_magnifier_enabled": output.bar_magnifier_enabled,
            "bar_magnifier_timeframe": magnifier_timeframe,
            "bar_magnifier_covered_chart_bars": output.bar_magnifier_covered_chart_bars,
            "bar_magnifier_fallback_chart_bars": output.bar_magnifier_fallback_chart_bars,
            "bar_magnifier_chart_bars": candles.len()
        },
        "gross_profit": output.gross_profit,
        "gross_loss": output.gross_loss,
        "net_profit": output.net_profit,
        "initial_capital": output.initial_capital,
        "max_runup": output.max_runup,
        "max_runup_pct": output.max_runup_percent,
        "max_drawdown": output.max_drawdown,
        "max_drawdown_pct": output.max_drawdown_initial_capital_percent,
        "strategy_max_drawdown_percent": output.max_drawdown_percent,
        "buy_and_hold_return_pct": buy_and_hold_return_pct,
        "buy_and_hold_basis": "raw underlying price change over the same sample; no fees, orders, or execution assumptions",
        "close_drawdown": close_drawdown,
        "chronological_validation": chronological_validation,
        "equity_curve_points_returned": backtest_curve_sample_indexes(candles.len()).len(),
        "equity_curve_is_sampled": candles.len() > MAX_BACKTEST_CURVE_POINTS,
        "equity_curve_sampling": "evenly spaced candle-close points including both sample endpoints; exact summary metrics use every candle",
        "equity_curve": backtest_curve_sample_indexes(candles.len()).into_iter().map(|index| {
            let candle = &candles[index];
            let equity = output.equity_curve[index];
            let buy_and_hold_equity = benchmark.as_ref().map(|(_, start_close)| output.initial_capital * candle.close / start_close);
            serde_json::json!({"time": candle.time, "equity": equity, "buy_and_hold_equity": buy_and_hold_equity})
        }).collect::<Vec<_>>(),
        "trades_returned": sample_trades.min(MAX_BACKTEST_REPORT_TRADES),
        "trades_limit": MAX_BACKTEST_REPORT_TRADES,
        "trades": output.trades.iter().rev().take(MAX_BACKTEST_REPORT_TRADES).rev().cloned().collect::<Vec<_>>(),
        "input_overrides": output.input_overrides,
        "diagnostics": output.diagnostics
    });
    Ok(serde_json::json!({
        "status": "RESEARCH_ONLY_UNVALIDATED",
        "read_only": true,
        "strategy": {"id": strategy.id, "name": strategy.name, "author": strategy.author, "source_reference": strategy.source_reference, "rights_basis": strategy.rights_basis, "license_or_permission": strategy.license_or_permission, "attribution": strategy.attribution},
        "instrument": {"exchange_id": exchange_id, "asset_category_id": category_id, "product_id": request.product_id, "symbol": request.symbol, "storage_symbol": storage_symbol, "timeframe": request.timeframe},
        "history": {"bars": candles.len(), "start_time": candles.first().map(|bar| bar.time), "end_time": candles.last().map(|bar| bar.time), "requested_from_time_ms": request.from_time_ms, "requested_to_time_exclusive_ms": request.to_time_exclusive_ms, "freshness": if metadata.get("is_stale").and_then(serde_json::Value::as_bool).unwrap_or(true) { "STALE" } else { "CURRENT" }, "source": data_source, "source_label": research_source_label(data_source, &instrument_kind), "data_provider": data_provider, "execution_scope": execution_scope},
        "fees": {"known": fees_known, "model": fee_model, "return_basis": commission_return_basis.unwrap_or(if fees_known { "after configured venue fees" } else { "gross; venue fees unknown" })},
        "validation": {"venue_native_history": venue_native, "holdout_validated": false, "note": "The chronological tail is a single research check, not an untouched holdout if source code or inputs are tuned against it. This run reports the source-defined strategy over one bounded historical sample. It models declared fixed-tick slippage only when registered tick metadata is available; it does not model spread, order-book depth, market impact, funding, or borrow and does not qualify execution evidence."},
        "backtest": backtest
    }))
}

pub(super) fn load_strategy_history(
    connection: &rusqlite::Connection,
    symbol: &str,
    timeframe: &str,
    limit: i64,
    source: &str,
) -> Result<Vec<Candle>, rusqlite::Error> {
    load_series_candles_from_source_with_cap(
        connection,
        symbol,
        timeframe,
        limit,
        source,
        MAX_STRATEGY_AGGREGATION_SOURCE_CANDLES,
    )
}
