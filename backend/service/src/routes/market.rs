use super::{api_error, query_values, read_post_body, respond, respond_status, RequestContext};
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
    // POST /api/refresh or /api/scan — trigger background scan immediately
    if (route == "/api/refresh" || route == "/api/scan" || route == "/api/scan-cycle")
        && method == "POST"
    {
        if state.is_scanning.load(Ordering::Relaxed) {
            let current_sym = state
                .current_scan_symbol
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone();
            let pct = *state
                .scan_progress_pct
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            respond_status(stream, crate::wire::encode(&serde_json::json!({"status":"already_scanning","message":"Scan cycle is currently in progress","current_symbol":current_sym,"progress_pct":pct})).unwrap_or_default(), "409 Conflict");
            return Ok(());
        }
        state.request_scan();
        respond_status(stream, crate::wire::encode(&serde_json::json!({"status":"queued","message":"Asynchronous scan cycle event triggered successfully"})).unwrap_or_default(), "202 Accepted");
        return Ok(());
    }

    // GET /api/markov-regimes
    if route == "/api/markov-regimes" && method == "GET" {
        let values = query_values(path);
        let timeframe = values.get("timeframe").map(String::as_str).unwrap_or("1d");
        let selected_symbol = values.get("symbol").map(String::as_str);
        let selected_exchange = values.get("exchange_id").map(String::as_str);
        let selected_category = values.get("asset_category_id").map(String::as_str);
        let limit = values
            .get("limit")
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(MAX_CANDLES)
            .clamp(30, MAX_CANDLES);
        match open_db_minimal(db) {
            Ok(connection) => {
                match load_markov_payload(
                    &connection,
                    db,
                    timeframe,
                    limit,
                    selected_symbol,
                    selected_exchange,
                    selected_category,
                ) {
                    Ok(payload) => {
                        let response = match payload {
                            Value::Object(mut object) => {
                                object.insert("status".into(), Value::String("ok".into()));
                                object.insert(
                                    "service".into(),
                                    Value::String("rust_quant_engine".into()),
                                );
                                Value::Object(object)
                            }
                            other => {
                                serde_json::json!({"status":"ok","service":"rust_quant_engine","payload":other})
                            }
                        };
                        respond(
                            stream,
                            crate::wire::encode(&response).unwrap_or_default(),
                            "200 OK",
                        );
                    }
                    Err(e) => api_error(stream, "500 Internal Server Error", e),
                }
            }
            Err(e) => api_error(stream, "500 Internal Server Error", e),
        }
        return Ok(());
    }

    if route == "/api/listing-status" && method == "GET" {
        let exchange_filter = query_values(path)
            .get("exchange")
            .map(|value| normalize_exchange_id(value));
        let exchange_filter = match exchange_filter {
            Some(exchange) => match master_data::exchange_by_alias(&exchange)
                .and_then(|record| master_data::catalog_string(record, "id"))
            {
                Some(exchange_id) => Some(exchange_id.to_string()),
                None => {
                    api_error(
                        stream,
                        "400 Bad Request",
                        "exchange must identify an app in the canonical catalog",
                    );
                    return Ok(());
                }
            },
            None => None,
        };
        match open_app_db_without_signal_store_for_market_directory(db).and_then(|connection| {
            instrument_registry::listing_status_for_exchange(
                &connection,
                db,
                exchange_filter.as_deref(),
            )
        }) {
            Ok(payload) => respond(
                stream,
                crate::wire::encode(&payload).unwrap_or_default(),
                "200 OK",
            ),
            Err(error) => api_error(stream, "500 Internal Server Error", error),
        }
        return Ok(());
    }
    if route == "/api/listing-status/refresh" && method == "POST" {
        let exchange = query_values(path)
            .get("exchange")
            .cloned()
            .unwrap_or_else(|| "pintu".into());
        match open_db_minimal(db) {
            Ok(connection) => {
                let result = match exchange.as_str() {
                    "pintu" => instrument_registry::sync_pintu(&connection),
                    "stockbit" => instrument_registry::sync_stockbit(&connection),
                    "tokocrypto" => instrument_registry::sync_tokocrypto(&connection),
                    _ => {
                        api_error(
                            stream,
                            "400 Bad Request",
                            "exchange must be stockbit, tokocrypto, or pintu",
                        );
                        return Ok(());
                    }
                };
                let status = if result.error.is_some() {
                    "503 Service Unavailable"
                } else {
                    "200 OK"
                };
                let payload = serde_json::json!({
                    "exchange_id": result.exchange_id,
                    "reconciled": result.inserted,
                    "delisted": result.deactivated,
                    "error": result.error,
                });
                respond(
                    stream,
                    crate::wire::encode(&payload).unwrap_or_default(),
                    status,
                );
            }
            Err(error) => api_error(stream, "500 Internal Server Error", error),
        }
        return Ok(());
    }

    if route == "/api/instruments" {
        let connection = match open_db_minimal(db) {
            Ok(connection) => connection,
            Err(error) => {
                api_error(stream, "500 Internal Server Error", error);
                return Ok(());
            }
        };
        match method {
            "GET" => {
                let values = query_values(path);
                let optional_filter = |key: &str| {
                    values
                        .get(key)
                        .map(String::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty() && !value.eq_ignore_ascii_case("all"))
                };
                let offset = values
                    .get("offset")
                    .and_then(|value| value.parse::<usize>().ok())
                    .unwrap_or_default();
                let limit = values
                    .get("limit")
                    .and_then(|value| value.parse::<usize>().ok())
                    .unwrap_or(instrument_registry::DEFAULT_PAGE_SIZE_ROWS);
                match instrument_registry::list_page(
                    &connection,
                    optional_filter("exchange"),
                    optional_filter("product"),
                    optional_filter("asset_category"),
                    optional_filter("symbol"),
                    optional_filter("search"),
                    offset,
                    limit,
                ) {
                    Ok(payload) => respond(
                        stream,
                        crate::wire::encode(&payload).unwrap_or_default(),
                        "200 OK",
                    ),
                    Err(error) => api_error(stream, "500 Internal Server Error", error),
                }
            }
            "POST" => {
                let body_text = read_post_body(&request_text);
                let body = match crate::wire::decode::<Value>(&body_text) {
                    Ok(body) => body,
                    Err(error) => {
                        api_error(stream, "400 Bad Request", error);
                        return Ok(());
                    }
                };
                match instrument_registry::register_manual(&connection, &body) {
                    Ok(payload) => {
                        let created = payload
                            .get("created")
                            .and_then(Value::as_bool)
                            .unwrap_or(false);
                        respond(
                            stream,
                            crate::wire::encode(&payload).unwrap_or_default(),
                            if created { "201 Created" } else { "200 OK" },
                        );
                    }
                    Err(error) => api_error(stream, "400 Bad Request", error),
                }
            }
            _ => api_error(stream, "405 Method Not Allowed", "use GET or POST"),
        }
        return Ok(());
    }

    if route == "/api/master-data" && method == "GET" {
        respond(
            stream,
            crate::wire::encode(master_data_catalog()).unwrap_or_default(),
            "200 OK",
        );
        return Ok(());
    }

    // GET /api/research/nwe-dss — per-symbol, per-timeframe causal signal study.
    if route == "/api/research/nwe-dss" && method == "GET" {
        let values = query_values(path);
        let exchange = values
            .get("exchange")
            .map(String::as_str)
            .unwrap_or("stockbit");
        let asset_class = values
            .get("asset_class")
            .map(String::as_str)
            .unwrap_or("idx_stock");
        let timeframe = values.get("timeframe").map(String::as_str).unwrap_or("4h");
        let selection = match nwe_dss_research_selection(exchange, asset_class, timeframe) {
            Ok(selection) => selection,
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
        let symbols = match research_symbols(
            &connection,
            selection,
            values.get("symbols").map(String::as_str),
        ) {
            Ok(symbols) => symbols,
            Err(error) => {
                api_error(stream, "400 Bad Request", error);
                return Ok(());
            }
        };
        match build_nwe_dss_research_report(db, selection, &symbols) {
            Ok(payload) => respond(
                stream,
                crate::wire::encode(&payload).unwrap_or_default(),
                "200 OK",
            ),
            Err(error) => api_error(stream, "500 Internal Server Error", error),
        }
        return Ok(());
    }

    // GET /api/research — read-only, exchange-first historical evidence report.
    // This route deliberately never triggers provider sync or plan generation.
    if route == "/api/research" && method == "GET" {
        let values = query_values(path);
        let exchange = values
            .get("exchange")
            .map(String::as_str)
            .unwrap_or("stockbit");
        let exchange_record = match exchange_by_alias(exchange)
            .filter(|record| catalog_string(record, "status") == Some("enabled"))
        {
            Some(record) => record,
            None => {
                api_error(
                    stream,
                    "400 Bad Request",
                    "exchange is not enabled in canonical master data",
                );
                return Ok(());
            }
        };
        let exchange_id = catalog_string(exchange_record, "id").unwrap_or("stockbit");
        let default_profile = default_profile_for_exchange(exchange_id);
        let requested_category = values
            .get("asset_class")
            .and_then(|category| category_by_id_or_research_alias(category));
        let category_id = requested_category
            .and_then(|category| catalog_string(category, "id"))
            .or_else(|| {
                default_profile.and_then(|profile| catalog_string(profile, "asset_category_id"))
            })
            .or_else(|| catalog_string(exchange_record, "default_asset_category_id"))
            .unwrap_or("idx_stock");
        let matching_default_profile = default_profile
            .filter(|profile| catalog_string(profile, "asset_category_id") == Some(category_id))
            .or_else(|| {
                master_data_records("plan_profiles").iter().find(|profile| {
                    catalog_string(profile, "exchange_id") == Some(exchange_id)
                        && catalog_string(profile, "asset_category_id") == Some(category_id)
                        && profile
                            .get("research_enabled")
                            .and_then(Value::as_bool)
                            .unwrap_or(false)
                })
            });
        let category_record =
            requested_category.or_else(|| category_by_id_or_research_alias(category_id));
        let default_asset_class = category_record
            .and_then(|category| category.get("research_aliases"))
            .and_then(Value::as_array)
            .and_then(|aliases| aliases.first())
            .and_then(Value::as_str)
            .unwrap_or(category_id);
        let asset_class = values
            .get("asset_class")
            .map(String::as_str)
            .unwrap_or(default_asset_class);
        let strategy = values
            .get("strategy")
            .map(String::as_str)
            .or_else(|| matching_default_profile.and_then(|profile| catalog_string(profile, "id")))
            .unwrap_or("");
        if strategy.is_empty() {
            api_error(
                stream,
                "400 Bad Request",
                "no research profile is configured for this app and asset category",
            );
            return Ok(());
        }
        let selection = match research_selection(exchange, asset_class, strategy) {
            Ok(selection) => selection,
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
        let requested_symbols = values.get("symbols").map(String::as_str);
        let symbols = match research_symbols(&connection, selection, requested_symbols) {
            Ok(symbols) => symbols,
            Err(error) => {
                api_error(stream, "400 Bad Request", error);
                return Ok(());
            }
        };
        match build_research_report(db, selection, &symbols) {
            Ok(payload) => respond(
                stream,
                crate::wire::encode(&payload).unwrap_or_default(),
                "200 OK",
            ),
            Err(error) => api_error(stream, "500 Internal Server Error", error),
        }
        return Ok(());
    }

    // Route guard — reject unknown paths
    if route != "/api/chart/candles"
        && route != "/api/compute/chart-bundle"
        && route != "/api/compute/indicators"
        && route != "/api/compute/signals"
        && route != "/api/compute/markov"
        && route != "/api/compute/liquidity-risk"
        && route != "/api/compute/risk"
        && route != "/api/compute/day-trade-profile"
        && route != "/api/chart/storage/stats"
        && route != "/api/master-data"
        && route != "/api/strategies"
        && route != "/api/research"
        && route != "/api/scan"
        && route != "/api/refresh"
    {
        api_error(stream, "404 Not Found", "route not found");
        return Ok(());
    }

    if query_values(path)
        .get("timeframe")
        .is_some_and(|timeframe| is_removed_candle_timeframe(timeframe))
    {
        api_error(
            stream,
            "410 Gone",
            "5m candle timeframe is no longer supported; use 15m or higher",
        );
        return Ok(());
    }

    if route == "/api/strategies" {
        let strategies = master_data_records("signal_strategies")
            .iter()
            .map(|model| {
                let mut strategy = model.clone();
                if let Some(object) = strategy.as_object_mut() {
                    let id = object.get("id").cloned().unwrap_or(Value::Null);
                    let label = object.get("label").cloned().unwrap_or(Value::Null);
                    object.insert("strategy_id".to_string(), id);
                    object.insert("name".to_string(), label);
                }
                strategy
            })
            .collect::<Vec<_>>();
        let payload = serde_json::json!({
            "total": strategies.len(),
            "total_plan_profiles": master_data_records("plan_profiles").len(),
            "exchanges": master_data_records("exchanges"),
            "asset_categories": master_data_records("asset_categories"),
            "trade_styles": master_data_records("trade_styles"),
            "signal_strategies": strategies,
            "plan_profiles": master_data_records("plan_profiles"),
            "strategies": strategies
        });
        respond(
            stream,
            crate::wire::encode(&payload).unwrap_or_default(),
            "200 OK",
        );
        return Ok(());
    }

    if route == "/api/scan" || route == "/api/refresh" {
        let connection = match open_app_db_for_market_directory(db) {
            Ok(connection) => connection,
            Err(error) => {
                api_error(stream, "500 Internal Server Error", error);
                return Ok(());
            }
        };
        match run_rust_scan_with_state(
            &connection,
            db,
            state.scan_interval_sec.load(Ordering::Relaxed) / 60,
            state,
        ) {
            Ok(payload) => respond(
                stream,
                crate::wire::encode(&payload).unwrap_or_default(),
                "200 OK",
            ),
            Err(error) => api_error(stream, "500 Internal Server Error", error),
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
