use super::*;
use crate::{
    database::open_db_minimal,
    pine_registry::{self, RegisterRequest, RegistryError},
};
use quant_pine_runtime::StrategyTrade;
use rusqlite::{params, OptionalExtension};
use serde::Deserialize;
use std::collections::HashMap;

const MAX_BACKTEST_REPORT_TRADES: usize = 200;
const MAX_BACKTEST_CURVE_POINTS: usize = 2_000;

#[cfg(test)]
#[path = "pine_tests.rs"]
mod tests;

#[derive(Debug, Deserialize)]
struct StrategyBacktestRequest {
    strategy_id: i64,
    exchange_id: String,
    asset_category_id: String,
    product_id: String,
    symbol: String,
    timeframe: String,
    #[serde(default)]
    inputs: HashMap<String, serde_json::Value>,
    #[serde(default)]
    max_history_bars: Option<usize>,
    #[serde(default)]
    from_time_ms: Option<i64>,
    #[serde(default)]
    to_time_exclusive_ms: Option<i64>,
    #[serde(default)]
    walk_forward_training_percent: Option<u8>,
}

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
    if route == "/api/pine/capabilities" && method == "GET" {
        respond(
            stream,
            serde_json::to_string(&quant_pine_runtime::capabilities())
                .unwrap_or_else(|_| "{}".to_string()),
            "200 OK",
        );
        return Ok(());
    }
    if route == "/api/pine/strategies" && method == "GET" {
        let id = query_values(path)
            .get("id")
            .and_then(|value| value.parse::<i64>().ok());
        let connection = match open_db_minimal(db) {
            Ok(connection) => connection,
            Err(error) => {
                respond_status(
                    stream,
                    serde_json::json!({"error": error.to_string()}).to_string(),
                    "500 Internal Server Error",
                );
                return Ok(());
            }
        };
        let response = if let Some(id) = id {
            pine_registry::list_strategies(&connection, Some(id)).and_then(|strategies| {
                let strategies = strategies
                    .into_iter()
                    .map(|strategy| {
                        let inputs =
                            quant_pine_runtime::strategy_input_definitions(&strategy.source)
                                .map_err(|diagnostics| {
                                    rusqlite::Error::InvalidParameterName(
                                        diagnostics
                                            .into_iter()
                                            .map(|item| item.message)
                                            .collect::<Vec<_>>()
                                            .join(" "),
                                    )
                                })?;
                        Ok(serde_json::json!({"strategy": strategy, "inputs": inputs}))
                    })
                    .collect::<Result<Vec<_>, rusqlite::Error>>()?;
                Ok(serde_json::json!({"strategies": strategies}))
            })
        } else {
            pine_registry::list_strategy_summaries(&connection)
                .map(|strategies| serde_json::json!({"strategies": strategies}))
        };
        match response {
            Ok(payload) => respond(
                stream,
                serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string()),
                "200 OK",
            ),
            Err(error) => respond_status(
                stream,
                serde_json::json!({"error": error.to_string()}).to_string(),
                "500 Internal Server Error",
            ),
        }
        return Ok(());
    }
    if route == "/api/pine/strategies" && method == "POST" {
        if !remote_is_loopback {
            respond_status(stream, serde_json::json!({"error": "Pine strategy registration is restricted to the local dashboard."}).to_string(), "403 Forbidden");
            return Ok(());
        }
        if !has_json_content_type(request_text) {
            respond_status(
                stream,
                serde_json::json!({"error": "Pine API requests require application/json."})
                    .to_string(),
                "415 Unsupported Media Type",
            );
            return Ok(());
        }
        let request: RegisterRequest = match serde_json::from_str(&read_post_body(request_text)) {
            Ok(request) => request,
            Err(error) => {
                respond_status(
                    stream,
                    serde_json::json!({"error": format!("Invalid registration payload: {error}")})
                        .to_string(),
                    "400 Bad Request",
                );
                return Ok(());
            }
        };
        let connection = match open_db_minimal(db) {
            Ok(connection) => connection,
            Err(error) => {
                respond_status(
                    stream,
                    serde_json::json!({"error": error.to_string()}).to_string(),
                    "500 Internal Server Error",
                );
                return Ok(());
            }
        };
        match pine_registry::register_strategy(&connection, request) {
            Ok(strategy) => respond_status(
                stream,
                serde_json::to_string(&serde_json::json!({"strategy": strategy}))
                    .unwrap_or_else(|_| "{}".to_string()),
                "201 Created",
            ),
            Err(RegistryError::Invalid(message)) => respond_status(
                stream,
                serde_json::json!({"error": message}).to_string(),
                "422 Unprocessable Content",
            ),
            Err(RegistryError::Storage(error)) => respond_status(
                stream,
                serde_json::json!({"error": error.to_string()}).to_string(),
                "500 Internal Server Error",
            ),
        }
        return Ok(());
    }
    if let Some(job_route) = route.strip_prefix("/api/pine/backtest/jobs/") {
        if !remote_is_loopback {
            respond_status(stream, serde_json::json!({"error": "Pine backtest jobs are restricted to the local dashboard."}).to_string(), "403 Forbidden");
            return Ok(());
        }
        if let Some(job_id) = job_route.strip_suffix("/cancel") {
            if method == "POST" {
                if !has_json_content_type(request_text) {
                    respond_status(
                        stream,
                        serde_json::json!({"error": "Pine API requests require application/json."})
                            .to_string(),
                        "415 Unsupported Media Type",
                    );
                    return Ok(());
                }
                match state.pine_backtest_jobs.get(job_id) {
                    Some(job) => {
                        let state_name = job.request_cancel();
                        respond_status(stream, serde_json::json!({"job_id": job_id, "state": state_name}).to_string(), "200 OK");
                    }
                    None => respond_status(stream, serde_json::json!({"error": "Pine backtest job was not found or has expired."}).to_string(), "404 Not Found"),
                }
                return Ok(());
            }
        } else if method == "GET" {
            match state.pine_backtest_jobs.get(job_route) {
                Some(job) => respond_status(stream, job.response().to_string(), "200 OK"),
                None => respond_status(
                    stream,
                    serde_json::json!({"error": "Pine backtest job was not found or has expired."})
                        .to_string(),
                    "404 Not Found",
                ),
            }
            return Ok(());
        }
    }
    if route == "/api/pine/backtest" && method == "POST" {
        if !remote_is_loopback {
            respond_status(stream, serde_json::json!({"error": "Pine backtests are restricted to the local dashboard."}).to_string(), "403 Forbidden");
            return Ok(());
        }
        if !has_json_content_type(request_text) {
            respond_status(
                stream,
                serde_json::json!({"error": "Pine API requests require application/json."})
                    .to_string(),
                "415 Unsupported Media Type",
            );
            return Ok(());
        }
        let request: StrategyBacktestRequest =
            match serde_json::from_str(&read_post_body(request_text)) {
                Ok(request) => request,
                Err(error) => {
                    respond_status(
                        stream,
                        serde_json::json!({"error": format!("Invalid backtest request: {error}")})
                            .to_string(),
                        "400 Bad Request",
                    );
                    return Ok(());
                }
            };
        let job = match state.pine_backtest_jobs.start() {
            Ok(job) => job,
            Err(error) => {
                respond_status(
                    stream,
                    serde_json::json!({"error": error}).to_string(),
                    "409 Conflict",
                );
                return Ok(());
            }
        };
        let job_for_thread = Arc::clone(&job);
        let database = db.to_string();
        if let Err(error) = std::thread::Builder::new().name("pine-backtest-job".to_string()).spawn(move || {
            job_for_thread.set_running();
            if job_for_thread.is_cancel_requested() { job_for_thread.cancelled(); return; }
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                run_registered_pine_strategy(&database, request, &job_for_thread)
            }));
            let result = match result {
                Ok(result) => result,
                Err(_) => { job_for_thread.fail("The Pine backtest worker stopped unexpectedly. Retry after the service recovers.".to_string()); return; }
            };
            if job_for_thread.is_cancel_requested() {
                job_for_thread.cancelled();
            } else {
                match result {
                    Ok(payload) => { let _ = job_for_thread.complete(payload); }
                    Err((_, error)) => job_for_thread.fail(error),
                }
            }
        }) {
            job.fail(format!("Could not start the bounded backtest worker: {error}"));
            respond_status(stream, serde_json::json!({"error": "Could not start the Pine backtest worker."}).to_string(), "500 Internal Server Error");
            return Ok(());
        }
        respond_status(
            stream,
            serde_json::json!({"job_id": job.id}).to_string(),
            "202 Accepted",
        );
        return Ok(());
    }
    if route == "/api/pine/validate" && method == "POST" {
        if !has_json_content_type(request_text) {
            respond_status(
                stream,
                serde_json::json!({"error": "Pine API requests require application/json."})
                    .to_string(),
                "415 Unsupported Media Type",
            );
            return Ok(());
        }
        let payload: serde_json::Value = match serde_json::from_str(&read_post_body(request_text)) {
            Ok(payload) => payload,
            Err(error) => {
                respond_status(
                    stream,
                    serde_json::json!({"error": format!("Invalid JSON: {error}")}).to_string(),
                    "400 Bad Request",
                );
                return Ok(());
            }
        };
        let source = payload
            .get("source")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let validation = pine_registry::validate_source(source);
        respond(
            stream,
            serde_json::to_string(&validation).unwrap_or_else(|_| "{}".to_string()),
            "200 OK",
        );
        return Ok(());
    }
    if route == "/api/pine/indicators" && method == "GET" {
        let id = query_values(path)
            .get("id")
            .and_then(|value| value.parse::<i64>().ok());
        let connection = match open_db_minimal(db) {
            Ok(connection) => connection,
            Err(error) => {
                respond_status(
                    stream,
                    serde_json::json!({"error": error.to_string()}).to_string(),
                    "500 Internal Server Error",
                );
                return Ok(());
            }
        };
        let response = if id.is_some() {
            pine_registry::list(&connection, id)
                .map(|indicators| serde_json::json!({"indicators": indicators}))
        } else {
            pine_registry::list_summaries(&connection)
                .map(|indicators| serde_json::json!({"indicators": indicators}))
        };
        match response {
            Ok(payload) => respond(
                stream,
                serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string()),
                "200 OK",
            ),
            Err(error) => respond_status(
                stream,
                serde_json::json!({"error": error.to_string()}).to_string(),
                "500 Internal Server Error",
            ),
        }
        return Ok(());
    }
    if route == "/api/pine/indicators" && method == "POST" {
        if !remote_is_loopback {
            respond_status(stream, serde_json::json!({"error": "Pine source registration is restricted to the local dashboard."}).to_string(), "403 Forbidden");
            return Ok(());
        }
        if !has_json_content_type(request_text) {
            respond_status(
                stream,
                serde_json::json!({"error": "Pine API requests require application/json."})
                    .to_string(),
                "415 Unsupported Media Type",
            );
            return Ok(());
        }
        let request: RegisterRequest = match serde_json::from_str(&read_post_body(request_text)) {
            Ok(request) => request,
            Err(error) => {
                respond_status(
                    stream,
                    serde_json::json!({"error": format!("Invalid registration payload: {error}")})
                        .to_string(),
                    "400 Bad Request",
                );
                return Ok(());
            }
        };
        let connection = match open_db_minimal(db) {
            Ok(connection) => connection,
            Err(error) => {
                respond_status(
                    stream,
                    serde_json::json!({"error": error.to_string()}).to_string(),
                    "500 Internal Server Error",
                );
                return Ok(());
            }
        };
        match pine_registry::register(&connection, request) {
            Ok(indicator) => respond_status(
                stream,
                serde_json::to_string(&serde_json::json!({"indicator": indicator}))
                    .unwrap_or_else(|_| "{}".to_string()),
                "201 Created",
            ),
            Err(RegistryError::Invalid(message)) => respond_status(
                stream,
                serde_json::json!({"error": message}).to_string(),
                "422 Unprocessable Content",
            ),
            Err(RegistryError::Storage(error)) => respond_status(
                stream,
                serde_json::json!({"error": error.to_string()}).to_string(),
                "500 Internal Server Error",
            ),
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

mod analytics;
mod backtest;
use analytics::{
    backtest_curve_sample_indexes, buy_and_hold_benchmark, summarize_chronological_validation,
    summarize_close_drawdown, summarize_strategy_trades, summarize_trade_return_distribution,
};
#[cfg(test)]
use analytics::{summarize_monthly_marked_returns, utc_month_key, wilson_interval_95_pct};
use backtest::run_registered_pine_strategy;

fn has_json_content_type(request_text: &str) -> bool {
    request_text
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.trim()
                .eq_ignore_ascii_case("content-type")
                .then(|| value.trim().to_ascii_lowercase())
        })
        .is_some_and(|content_type| content_type.starts_with("application/json"))
}
