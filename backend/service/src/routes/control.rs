use super::{
    api_error, query_values, read_post_body, redirect, respond, respond_html, respond_status,
    same_origin_request, serve_static, RequestContext,
};
use crate::*;

pub(super) fn handle<'a>(request: RequestContext<'a>) -> Result<(), RequestContext<'a>> {
    let RequestContext {
        stream,
        db,
        state,
        remote_is_loopback,
        request_text,
        method,
        path,
        route,
    } = request;
    let signal_count_request = route == "/api/signals/count" && method == "GET";
    if (route == "/api/signals" && method == "GET") || signal_count_request {
        let values = query_values(path);
        let optional_filter = |key: &str| {
            values
                .get(key)
                .map(String::as_str)
                .filter(|value| !value.is_empty() && !value.eq_ignore_ascii_case("all"))
        };
        let exchange_filter = optional_filter("exchange");
        let timeframe_filter = optional_filter("timeframe");
        let product_filter = optional_filter("product");
        let search_filter = values
            .get("search")
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(|value| value.chars().take(80).collect::<String>());
        let offset = values
            .get("offset")
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or_default();
        let limit = values
            .get("limit")
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(100);
        let mut connection = match open_db_minimal(db) {
            Ok(connection) => connection,
            Err(error) => {
                api_error(stream, "500 Internal Server Error", error);
                return Ok(());
            }
        };
        let scan_interval_seconds =
            i64::try_from(state.scan_interval_sec.load(Ordering::Relaxed)).unwrap_or(i64::MAX);
        if signal_count_request {
            match crate::signal_alerts::count_current_signal_screener_rows(
                &mut connection,
                scan_interval_seconds,
            ) {
                Ok(response) => respond(
                    stream,
                    crate::wire::encode(&response).unwrap_or_default(),
                    "200 OK",
                ),
                Err(error) => api_error(stream, "500 Internal Server Error", error),
            }
            return Ok(());
        }
        match crate::signal_alerts::list_signal_screener_page(
            &connection,
            scan_interval_seconds,
            exchange_filter,
            timeframe_filter,
            product_filter,
            search_filter.as_deref(),
            offset,
            limit,
        ) {
            Ok(mut payload) => {
                if let Some(object) = payload.as_object_mut() {
                    let master_data = match signal_screener_master_data_for_connection(&connection)
                    {
                        Ok(master_data) => master_data,
                        Err(error) => {
                            api_error(stream, "500 Internal Server Error", error);
                            return Ok(());
                        }
                    };
                    object.insert("master_data".into(), master_data);
                    object.insert(
                        "available_timeframes".into(),
                        serde_json::json!(DSS_BRESSERT_MTF_TIMEFRAMES
                            .iter()
                            .map(|definition| definition.timeframe)
                            .collect::<Vec<_>>()),
                    );
                }
                respond(
                    stream,
                    crate::wire::encode(&payload).unwrap_or_default(),
                    "200 OK",
                )
            }
            Err(error) => api_error(stream, "500 Internal Server Error", error),
        }
        return Ok(());
    }

    if route == "/api/signal-alerts" && method == "GET" {
        let values = query_values(path);
        let requested_limit = values
            .get("limit")
            .and_then(|value| value.parse::<i64>().ok());
        let connection = match open_db_minimal(db) {
            Ok(connection) => connection,
            Err(error) => {
                api_error(stream, "500 Internal Server Error", error);
                return Ok(());
            }
        };
        match crate::signal_alerts::list_alerts(&connection, requested_limit) {
            Ok(payload) => respond(
                stream,
                crate::wire::encode(&payload).unwrap_or_default(),
                "200 OK",
            ),
            Err(error) => api_error(stream, "500 Internal Server Error", error),
        }
        return Ok(());
    }

    if route == "/api/signal-alerts/ack" && method == "POST" {
        if !same_origin_request(request_text) {
            api_error(
                stream,
                "403 Forbidden",
                "signal alert updates require a same-origin dashboard request",
            );
            return Ok(());
        }
        let connection = match open_db_minimal(db) {
            Ok(connection) => connection,
            Err(error) => {
                api_error(stream, "500 Internal Server Error", error);
                return Ok(());
            }
        };
        match crate::signal_alerts::mark_all_read(&connection) {
            Ok(updated) => respond(
                stream,
                crate::wire::encode(&serde_json::json!({
                    "status": "ok",
                    "marked_read": updated,
                }))
                .unwrap_or_default(),
                "200 OK",
            ),
            Err(error) => api_error(stream, "500 Internal Server Error", error),
        }
        return Ok(());
    }

    if route == "/api/signal-alert-models" && method == "GET" {
        let connection = match open_db_minimal(db) {
            Ok(connection) => connection,
            Err(error) => {
                api_error(stream, "500 Internal Server Error", error);
                return Ok(());
            }
        };
        match crate::signal_alerts::indicator_model_options(&connection) {
            Ok(payload) => respond(
                stream,
                crate::wire::encode(&payload).unwrap_or_default(),
                "200 OK",
            ),
            Err(error) => api_error(stream, "500 Internal Server Error", error),
        }
        return Ok(());
    }

    if route == "/api/signal-alert-models" && method == "POST" {
        if !remote_is_loopback {
            api_error(
                stream,
                "403 Forbidden",
                "signal model settings are restricted to the local dashboard",
            );
            return Ok(());
        }
        let body = read_post_body(&request_text);
        let payload = match crate::wire::decode::<serde_json::Value>(&body) {
            Ok(payload) => payload,
            Err(error) => {
                api_error(stream, "400 Bad Request", error);
                return Ok(());
            }
        };
        let connection = match open_db_minimal(db) {
            Ok(connection) => connection,
            Err(error) => {
                api_error(stream, "500 Internal Server Error", error);
                return Ok(());
            }
        };
        match crate::signal_alerts::save_indicator_model(&connection, &payload) {
            Ok(result) => respond(
                stream,
                crate::wire::encode(&result).unwrap_or_default(),
                "200 OK",
            ),
            Err(error) => api_error(stream, "400 Bad Request", error),
        }
        return Ok(());
    }

    if route == "/api/stockbit/foreign-flow" && method == "GET" {
        let values = query_values(path);
        let Some(symbol) = values
            .get("symbol")
            .filter(|value| !value.trim().is_empty())
        else {
            api_error(stream, "400 Bad Request", "symbol is required");
            return Ok(());
        };
        if let Err(error) = crate::foreign_flow::normalize_symbol(symbol) {
            api_error(stream, "400 Bad Request", error);
            return Ok(());
        }
        let connection = match open_db_minimal(db) {
            Ok(connection) => connection,
            Err(error) => {
                api_error(stream, "500 Internal Server Error", error);
                return Ok(());
            }
        };
        match crate::foreign_flow::summary_for_symbol(&connection, symbol) {
            Ok(summary) => respond(
                stream,
                crate::wire::encode(&summary).unwrap_or_default(),
                "200 OK",
            ),
            Err(error) => api_error(stream, "500 Internal Server Error", error),
        }
        return Ok(());
    }

    if route == "/api/stockbit/foreign-flow/import" && method == "POST" {
        let origin = request_text.lines().find_map(|line| {
            line.split_once(':')
                .filter(|(name, _)| name.trim().eq_ignore_ascii_case("origin"))
                .map(|(_, value)| value.trim().trim_end_matches('/').to_ascii_lowercase())
        });
        let local_origin = origin.as_deref().map_or(true, |value| {
            ["http://127.0.0.1:", "http://localhost:", "http://[::1]:"]
                .iter()
                .any(|prefix| {
                    value
                        .strip_prefix(prefix)
                        .is_some_and(|port| port.parse::<u16>().is_ok())
                })
        });
        if !remote_is_loopback || !local_origin {
            api_error(
                stream,
                "403 Forbidden",
                "Stockbit flow imports are restricted to the local dashboard",
            );
            return Ok(());
        }
        let body = read_post_body(&request_text);
        let payload = match crate::wire::decode::<serde_json::Value>(&body) {
            Ok(payload) => payload,
            Err(error) => {
                api_error(stream, "400 Bad Request", error);
                return Ok(());
            }
        };
        let connection = match open_db_minimal(db) {
            Ok(connection) => connection,
            Err(error) => {
                api_error(stream, "500 Internal Server Error", error);
                return Ok(());
            }
        };
        match crate::foreign_flow::import_payload(&connection, &payload) {
            Ok(result) => respond(
                stream,
                crate::wire::encode(&result).unwrap_or_default(),
                "200 OK",
            ),
            Err(error) => api_error(stream, "400 Bad Request", error),
        }
        return Ok(());
    }

    // ─── Static / Dashboard routes ──────────────────────────────────────────
    if route == "/entry-validation" && method == "GET" {
        serve_static(stream, "frontend/dashboard/templates/entry-validation.html");
        return Ok(());
    }

    if route == "/scoring" && method == "GET" {
        serve_static(stream, "frontend/dashboard/templates/scoring.html");
        return Ok(());
    }

    if route == "/" || route == "/index.html" {
        match fs::read_to_string("frontend/dashboard/templates/dashboard.html") {
            Ok(html) => respond_html(stream, html, "200 OK"),
            Err(_) => {
                respond_html(
                    stream,
                    "<h1>Dashboard not found</h1><p>frontend/dashboard/templates/dashboard.html missing.</p>"
                        .to_string(),
                    "404 Not Found",
                );
            }
        }
        return Ok(());
    }

    if route == "/backtest" || route == "/backtest/" || route == "/backtest.html" {
        redirect(stream, "/");
        return Ok(());
    }

    if route == "/backtest_visualizer.html" {
        redirect(stream, "/");
        return Ok(());
    }

    if route == "/regime" || route == "/regimes" || route == "/markov" || route == "/regime.html" {
        redirect(stream, "/?tab=plans");
        return Ok(());
    }

    // Keep the public /web/ asset URLs while storing the dashboard under frontend/.
    if route.starts_with("/web/") {
        let safe_path = route.trim_start_matches('/').replace("..", "");
        let asset_path = if route == "/web/wasm/quant_pine_runtime.wasm" {
            "target/wasm32-unknown-unknown/release/quant_pine_runtime.wasm".to_string()
        } else {
            safe_path.replacen("web/", "frontend/dashboard/", 1)
        };
        serve_static(stream, &asset_path);
        return Ok(());
    }

    // ─── SSE stream (must be checked before route guard — it's a special long-lived connection) ──
    if route == "/api/events" {
        let state_clone = Arc::clone(state);
        // Set TCP_NODELAY and let the SSE handler run on a new thread so it
        // doesn't block the main serial accept loop.
        let _ = stream.set_nodelay(true);
        thread::spawn(move || handle_sse(stream, state_clone));
        return Ok(());
    }

    if route == "/health" || route == "/api/health" {
        let payload = serde_json::json!({
            "status": "ok",
            "service": "rust_quant_engine",
            "version": SERVICE_VERSION,
        "compute": ["bounded_candles", "dss_bressert", "nadaraya_watson_envelope", "chart_bundle", "historical_research_engine", "idx_stockbit_day_trade", "idx_stockbit_opening_range_momentum", "idx_stockbit_first_half_hour_momentum", "idx_stockbit_time_of_day_momentum", "idx_stockbit_swing", "idx_stockbit_time_series_momentum_swing_research", "stockbit_multibagger", "pintu_futures_day_trade", PINTU_GOLDEN_CROSS_STRATEGY_ID, TOKOCRYPTO_SPOT_CONFLUENCE_STRATEGY_ID, TOKENIZED_STOCK_CONFLUENCE_STRATEGY_ID, "registered_plan_generation", "markov_regime_models", "liquidity_zone_risk", "venue_risk_position_sizing", "day_trade_profile", "storage_stats"],
        });
        respond(
            stream,
            crate::wire::encode(&payload).unwrap_or_default(),
            "200 OK",
        );
        return Ok(());
    }

    // ─── New Phase-3 routes (no route guard needed — handled before guard) ───

    // GET /api/status
    if route == "/api/status" && method == "GET" {
        let is_scanning = state.is_scanning.load(Ordering::Relaxed);
        let last_hb = state.last_heartbeat.load(Ordering::Relaxed);
        let now = now_millis() / 1000;
        let tab_active = last_hb > 0 && (now - last_hb) < 120;
        let last_scan = state
            .last_scan_time
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let next_scan = state
            .next_scan_time
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let next_scan_epoch = state.next_scan_epoch.load(Ordering::Relaxed);
        let pct = *state
            .scan_progress_pct
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let current_sym = state
            .current_scan_symbol
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let interval_min = state.scan_interval_sec.load(Ordering::Relaxed) / 60;
        let info = serde_json::json!({
            "server_time": format_timestamp(now as u64),
            "tab_active": tab_active,
            "is_scanning": is_scanning,
            "current_scan_symbol": current_sym,
            "scan_progress_pct": pct,
            "last_scan_time": last_scan,
            "next_scan_time": next_scan,
            "next_scan_epoch": next_scan_epoch,
            "scan_interval_minutes": interval_min,
            "seconds_since_heartbeat": now - last_hb,
            "active_sse_clients": state.sse_clients.lock().unwrap_or_else(|e| e.into_inner()).len(),
        });
        respond(
            stream,
            crate::wire::encode(&info).unwrap_or_default(),
            "200 OK",
        );
        return Ok(());
    }

    // POST /api/heartbeat
    if route == "/api/heartbeat" && method == "POST" {
        // A dashboard heartbeat only marks the read-only view as active. Users
        // start scans explicitly, and the scheduler owns interval-based scans.
        state
            .last_heartbeat
            .store(now_millis() / 1000, Ordering::Relaxed);
        let payload = serde_json::json!({"status":"alive","timestamp":now_millis()});
        respond(
            stream,
            crate::wire::encode(&payload).unwrap_or_default(),
            "200 OK",
        );
        return Ok(());
    }

    // POST /api/settings/scan-interval
    if route == "/api/settings/scan-interval" && method == "POST" {
        let body_bytes = read_post_body(&request_text);
        match crate::wire::decode::<serde_json::Value>(&body_bytes) {
            Ok(body) => {
                let minutes = body.get("minutes").and_then(|v| v.as_u64()).unwrap_or(0);
                if minutes != crate::DEFAULT_SCAN_INTERVAL_MINUTES {
                    let payload = serde_json::json!({"status":"error","error":"signal generation runs on a fixed 15-minute cycle"});
                    respond_status(
                        stream,
                        crate::wire::encode(&payload).unwrap_or_default(),
                        "400 Bad Request",
                    );
                } else {
                    state
                        .scan_interval_sec
                        .store(minutes * 60, Ordering::Relaxed);
                    let next_epoch = now_secs().saturating_add(minutes * 60);
                    state.next_scan_epoch.store(next_epoch, Ordering::Relaxed);
                    *state
                        .next_scan_time
                        .lock()
                        .unwrap_or_else(|e| e.into_inner()) = format_timestamp(next_epoch);
                    let payload = serde_json::json!({
                        "status":"ok",
                        "scan_interval_minutes":minutes,
                        "next_scan_epoch":next_epoch,
                        "next_scan_time":format_timestamp(next_epoch),
                    });
                    respond(
                        stream,
                        crate::wire::encode(&payload).unwrap_or_default(),
                        "200 OK",
                    );
                }
            }
            Err(e) => {
                let payload = serde_json::json!({"status":"error","error":e.to_string()});
                respond_status(
                    stream,
                    crate::wire::encode(&payload).unwrap_or_default(),
                    "400 Bad Request",
                );
            }
        }
        return Ok(());
    }

    Err(RequestContext {
        stream,
        db,
        state,
        remote_is_loopback,
        request_text,
        method,
        path,
        route,
    })
}
