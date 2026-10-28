#![recursion_limit = "512"]

mod foreign_flow;
mod instrument_registry;
mod master_data;
mod pintu_listing;
mod pintu_storage;
mod public_http;
mod request_admission;
mod research_proxy;
mod routes;
mod stockbit_listing;
mod tokocrypto;
mod tokocrypto_storage;
mod wire;
#[cfg(test)]
use master_data::tokenized_stock_symbols;
use master_data::{
    catalog_string, category_by_id_or_research_alias, default_profile_for_exchange,
    exchange_by_alias, master_data_catalog, master_data_records, multibagger_seed_symbols,
    profile_by_id, signal_screener_master_data_for_connection,
};
use quant_core::{
    build_registered_plan, calculate_dss, calculate_dss_bressert_mtf,
    calculate_nadaraya_watson_envelope, calculate_position_size, compute_day_trade_profile,
    compute_liquidity_risk, day_trade_minimum_win_rate_pct, evaluate_historical_evidence_with_fees,
    generate_idx_first_half_hour_momentum_signals, generate_idx_opening_range_signals,
    generate_idx_signals, generate_idx_time_of_day_momentum_signals,
    generate_idx_time_series_momentum_signals, generate_pintu_golden_cross_signals,
    generate_pintu_signals, generate_tokocrypto_confluence_signals,
    historical_evidence_minimum_cagr_pct, historical_evidence_minimum_calendar_month_return_pct,
    historical_evidence_profile, is_pintu_golden_cross_strategy, is_tokocrypto_confluence_strategy,
    stockbit_candidate_price_eligible, DssBressertTimeframeInput, DssConfig,
    IdxIntradayMomentumConfig, IdxOpeningRangeConfig, IdxStrategyConfig,
    IdxTimeSeriesMomentumConfig, NadarayaWatsonEnvelopeConfig, NadarayaWatsonEnvelopeMode,
    PintuGoldenCrossConfig, PintuStrategyConfig, PositionSizeRequest, StrategyBar,
    TokocryptoConfluenceConfig, DSS_BRESSERT_MINIMUM_HISTORY_BARS, DSS_BRESSERT_MTF_TIMEFRAMES,
    MIN_DAY_TRADE_PROFIT_FACTOR, MIN_DAY_TRADE_WIN_RATE_PCT, MIN_EVIDENCE_SAMPLE_DAYS,
    MIN_EVIDENCE_SAMPLE_TRADES, MIN_MULTIBAGGER_CALENDAR_MONTH_RETURN_PCT,
    MIN_STOCKBIT_DAY_TRADE_CAGR_PCT, MIN_SWING_CAGR_PCT, NANOS_PER_DAY, NANOS_PER_SECOND,
    PINTU_GOLDEN_CROSS_STRATEGY_ID, PINTU_GOLDEN_CROSS_TIMEFRAME,
    PINTU_GOLDEN_CROSS_V2_EVIDENCE_ID, SECONDS_PER_DAY, TOKENIZED_STOCK_CONFLUENCE_STRATEGY_ID,
    TOKOCRYPTO_SPOT_CONFLUENCE_STRATEGY_ID,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::HashMap,
    env, fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering},
        Arc, Condvar, Mutex,
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const SERVICE_VERSION: &str = "0.2.0";
const DEFAULT_TIMEFRAME: &str = "15m";
const DEFAULT_LEGACY_MARKET_DB: &str = "data/market/market.sqlite3";
const DEFAULT_MARKET_DIR: &str = "data/market/tickers";
const DEFAULT_APP_DB: &str = "data/app/app.sqlite3";
const DEFAULT_SIGNAL_DB: &str = "data/signals/signals.sqlite3";
const SIGNAL_REFRESH_INTERVAL_SECONDS: u64 = 15 * quant_core::SECONDS_PER_MINUTE as u64;
const DEFAULT_SCAN_INTERVAL_MINUTES: u64 =
    SIGNAL_REFRESH_INTERVAL_SECONDS / quant_core::SECONDS_PER_MINUTE as u64;
const MIN_ENGINE_HISTORY_DAYS: i64 = MIN_EVIDENCE_SAMPLE_DAYS;
const MARKET_SYNC_TICK_SECONDS: u64 = quant_core::SECONDS_PER_MINUTE as u64;
// The browser loads dashboard modules in parallel, so the bounded cap must
// allow that startup burst while rejecting excess connections under load.
const MAX_CONCURRENT_HTTP_REQUESTS: usize = 20;
// Match the Pine runtime candle cap (keeps heap allocation bounded per request).
const MAX_CANDLES: i64 = quant_pine_runtime::MAX_CANDLES as i64;
const MIN_CHART_CANDLES: usize = 2;
// A higher-timeframe chart may need more lower-timeframe rows than the normal
// response cap. The rollup is built in one bounded request and released before
// the response is assembled, keeping the 32 MB steady-state cap intact.
const MAX_AGGREGATION_SOURCE_CANDLES: i64 = 12_000;
// A 20,000-bar 4h strategy sample needs at most 80,000 hourly rows plus
// bounded rollup warm-up. This cap applies only to transient strategy reads.
const MAX_STRATEGY_AGGREGATION_SOURCE_CANDLES: i64 = 80_120;
// Evidence reads are bounded separately from chart reads. One symbol is
// evaluated at a time, so this keeps the sidecar within the 16 MB steady-state
// target. A future two-year 24/7 intraday store can exceed this cap; its
// evidence remains explicitly under-target until a streaming evaluator is used.
const MAX_EVIDENCE_CANDLES: i64 = 10_000;
const MAX_RESEARCH_SYMBOLS: usize = 100;
mod candle_history;
mod candles;
mod database;
mod market_diagnostics;
mod market_sync;
mod pine_registry;
mod plan_annotations;
mod research_candidates;
mod research_policy;
mod research_reports;
mod reward_risk_rule;
mod runtime;
mod scan;
mod signal_alerts;
use candle_history::*;
use candles::*;
use database::*;
use market_diagnostics::*;
use market_sync::*;
use plan_annotations::*;
use research_candidates::*;
use research_policy::*;
use research_reports::*;
use runtime::*;
use scan::*;

#[derive(Clone, Debug, Serialize)]
struct NadarayaWatsonEnvelopeModeCandleValue {
    #[serde(skip_serializing_if = "Option::is_none")]
    upper: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    lower: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    crossed_above_upper: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    crossed_below_lower: Option<bool>,
}

#[derive(Clone, Debug, Serialize)]
struct NadarayaWatsonEnvelopeCandleValue {
    repainting: NadarayaWatsonEnvelopeModeCandleValue,
    endpoint: NadarayaWatsonEnvelopeModeCandleValue,
}

#[derive(Clone, Debug, Default, Serialize)]
struct ChartIndicatorValues {
    #[serde(skip_serializing_if = "Option::is_none")]
    nadaraya_watson_envelope: Option<NadarayaWatsonEnvelopeCandleValue>,
}

#[derive(Clone, Debug, Serialize)]
struct Candle {
    time: i64,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    dss: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trigger: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    gradient: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    accel: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    indicators: Option<ChartIndicatorValues>,
}

fn main() {
    let address = env::var("QUANT_RUST_ADDR").unwrap_or_else(|_| {
        env::var("PORT")
            .map(|port| format!("0.0.0.0:{port}"))
            .unwrap_or_else(|_| "127.0.0.1:7777".into())
    });
    let market_dir = env::var("QUANT_MARKET_DIR").unwrap_or_else(|_| DEFAULT_MARKET_DIR.into());
    let app_database = env::var("QUANT_APP_DB").unwrap_or_else(|_| DEFAULT_APP_DB.into());
    let signal_database = database::configured_signal_database_path()
        .to_string_lossy()
        .into_owned();
    if env::args().any(|arg| arg == "--split-market-database") {
        let market_source = migration_market_database_path();
        match split_market_database(&market_source, &app_database) {
            Ok(summary) => println!("database split {summary}"),
            Err(error) => {
                eprintln!("database split failed: {error}");
                std::process::exit(1);
            }
        }
        return;
    }
    if env::args().any(|arg| arg == "--shard-market-database") {
        let market_source = migration_market_database_path();
        match shard_market_database(&market_source, &market_dir, &app_database, &signal_database) {
            Ok(summary) => println!("market sharding {summary}"),
            Err(error) => {
                eprintln!("market sharding failed: {error}");
                std::process::exit(1);
            }
        }
        return;
    }
    if env::args().any(|arg| arg == "--share-crypto-candles") {
        match database::share_crypto_candles(&market_dir, &app_database) {
            Ok(summary) => println!("shared crypto candle migration {summary}"),
            Err(error) => {
                eprintln!("shared crypto candle migration failed: {error}");
                std::process::exit(1);
            }
        }
        return;
    }
    if env::args().any(|arg| arg == "--migrate-signal-snapshot") {
        match migrate_signal_screener_snapshot(&app_database, &signal_database) {
            Ok(summary) => println!("signal snapshot migration {summary}"),
            Err(error) => {
                eprintln!("signal snapshot migration failed: {error}");
                std::process::exit(1);
            }
        }
        return;
    }
    if env::args().any(|arg| arg == "--migrate-signal-score-scale") {
        match migrate_signal_score_scale(&signal_database) {
            Ok(summary) => println!("signal score scale migration {summary}"),
            Err(error) => {
                eprintln!("signal score scale migration failed: {error}");
                std::process::exit(1);
            }
        }
        return;
    }
    if let Some(symbol) = env::args().skip(1).find_map(|arg| {
        arg.strip_prefix("--sync-research-15m=")
            .map(str::to_ascii_uppercase)
    }) {
        let connection = open_app_db_for_market_directory(&market_dir)
            .expect("open app database for targeted research sync");
        let instrument: Option<(String, String, String, String)> = connection
            .query_row(
                "SELECT storage_symbol,exchange_id,product_id,asset_category_id
                   FROM app.instruments
                  WHERE symbol=?1 AND exchange_id='stockbit' AND product_id='stockbit_regular'
                    AND asset_category_id='idx_stock' AND is_active=1 AND venue_verified=1
                    AND listing_status='public_listed_snapshot'
                  LIMIT 1",
                [&symbol],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()
            .expect("query registered Stockbit instrument");
        let Some((storage_symbol, exchange_id, product_id, category_id)) = instrument else {
            eprintln!("no active, verified Stockbit listing found for {symbol}");
            std::process::exit(2);
        };
        let market_connection = open_market_history_for_symbol(&market_dir, &storage_symbol, true)
            .expect("open registered ticker SQLite database")
            .expect("create registered ticker SQLite database");
        match research_proxy::sync_15m_for_exchange(
            &market_connection,
            &storage_symbol,
            &category_id,
            &exchange_id,
            &product_id,
            &format!("{symbol}.JK"),
        ) {
            Ok(rows) => println!("synced {rows} labeled research-proxy 15m candles for {symbol}"),
            Err(error) => {
                eprintln!("research candle sync failed for {symbol}: {error}");
                std::process::exit(1);
            }
        }
        return;
    }
    for database in [&market_dir, &app_database, &signal_database] {
        if let Some(parent) = std::path::Path::new(database)
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent).expect("create SQLite storage directory");
        }
    }
    if env::args().any(|arg| arg == "--regather-market-data") {
        let conn = open_app_db_for_market_directory(&market_dir)
            .expect("open active SQLite database for regather");
        let registry_results = [
            instrument_registry::sync_stockbit(&conn),
            instrument_registry::sync_tokocrypto(&conn),
            instrument_registry::sync_pintu(&conn),
        ];
        let mut registry_failed = false;
        for result in registry_results {
            match result.error {
                Some(error) => {
                    registry_failed = true;
                    eprintln!("{} registry refresh failed: {error}", result.exchange_id);
                }
                None => println!(
                    "{} registry refresh complete: {} records reconciled, {} delisted",
                    result.exchange_id, result.inserted, result.deactivated
                ),
            }
        }
        if !provider_sync_enabled() {
            eprintln!("Market-data providers are disabled; no candles were gathered.");
            std::process::exit(1);
        }
        let (daily_attempted, hourly_series_refreshed) =
            regather_registered_market_history(&conn, &market_dir)
                .expect("regather registered market history into SQLite");
        println!(
            "market-data regather complete: {daily_attempted} daily instruments attempted, {hourly_series_refreshed} hourly series refreshed"
        );
        drop(conn);
        if registry_failed {
            std::process::exit(1);
        }
        return;
    }
    if let Err(error) = validate_runtime_market_directory(&market_dir) {
        eprintln!("market storage layout refused: {error}");
        std::process::exit(1);
    }
    if env::args().any(|arg| arg == "--migrate-tokocrypto-confluence") {
        if !std::path::Path::new(&app_database).is_file() {
            eprintln!(
                "Tokocrypto catalog migration requires an existing QUANT_APP_DB SQLite database"
            );
            std::process::exit(1);
        }
        let conn = open_app_db_for_market_directory(&market_dir)
            .expect("open active SQLite database for Tokocrypto catalog migration");
        match master_data::migrate_tokocrypto_confluence_catalog(&conn) {
            Ok(rows) => println!(
                "Tokocrypto confluence catalog migration complete: {rows} reference rows updated"
            ),
            Err(error) => {
                eprintln!("Tokocrypto confluence catalog migration failed: {error}");
                std::process::exit(1);
            }
        }
        return;
    }
    if env::args().any(|arg| arg == "--migrate-app-products") {
        let conn = open_app_db_for_market_directory(&market_dir)
            .expect("open active SQLite database for migration");
        let migrated = instrument_registry::migrate_app_products(&conn)
            .expect("migrate app/product registry transactionally");
        println!("app/product migration complete: {migrated} registry rows migrated");
        return;
    }
    if env::args().any(|arg| arg == "--refresh-pintu-listings") {
        let conn = open_app_db_for_market_directory(&market_dir)
            .expect("open active SQLite database for Pintu refresh");
        let result = instrument_registry::sync_pintu(&conn);
        if let Some(error) = result.error {
            panic!("{error}");
        }
        println!(
            "Pintu listing refresh complete: {} records reconciled, {} delisted",
            result.inserted, result.deactivated
        );
        return;
    }
    if env::args().any(|arg| arg == "--refresh-public-registries") {
        let conn = open_app_db_for_market_directory(&market_dir)
            .expect("open active SQLite database for public registry refresh");
        let results = [
            instrument_registry::sync_stockbit(&conn),
            instrument_registry::sync_tokocrypto(&conn),
        ];
        let mut failed = false;
        for result in results {
            match result.error {
                Some(error) => {
                    failed = true;
                    eprintln!("{} registry refresh failed: {error}", result.exchange_id);
                }
                None => println!(
                    "{} registry refresh complete: {} records reconciled, {} delisted",
                    result.exchange_id, result.inserted, result.deactivated
                ),
            }
        }
        drop(conn);
        if failed {
            std::process::exit(1);
        }
        return;
    }
    if env::args().any(|arg| arg == "--scan-once") {
        let conn = open_app_db_for_market_directory(&market_dir)
            .expect("open active SQLite database for scan");
        let result = run_rust_scan(&conn, &market_dir, DEFAULT_SCAN_INTERVAL_MINUTES)
            .expect("scan registered app instruments");
        println!("scan complete: {} plans", result["summary"]["total_plans"]);
        return;
    }
    drop(
        open_app_db_for_market_directory(&market_dir)
            .expect("initialize the active SQLite database"),
    );
    let scan_interval_min = DEFAULT_SCAN_INTERVAL_MINUTES;

    let state = AppState::new(market_dir.clone(), scan_interval_min * 60);
    let background_work_enabled =
        !env::var("QUANT_DISABLE_BACKGROUND_WORK")
            .ok()
            .is_some_and(|value| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes"
                )
            });

    if background_work_enabled {
        // Spawn background scan scheduler daemon
        {
            let state_clone = Arc::clone(&state);
            thread::Builder::new()
                .name("scan-daemon".to_string())
                .spawn(move || background_scan_loop(state_clone))
                .expect("spawn scan daemon");
        }

        // Spawn daily registry sync daemon — refreshes instrument tickers per exchange
        {
            let state_clone = Arc::clone(&state);
            thread::Builder::new()
                .name("registry-sync-daemon".to_string())
                .spawn(move || background_registry_sync_loop(state_clone))
                .expect("spawn registry sync daemon");
        }

        // Keep local hourly history current for the 4H chart even when the
        // dashboard is closed or idle. This worker has its own bounded sync batch
        // and does not generate signal results.
        {
            let state_clone = Arc::clone(&state);
            thread::Builder::new()
                .name("market-sync-daemon".to_string())
                .spawn(move || background_market_sync_loop(state_clone))
                .expect("spawn market sync daemon");
        }

        // Binance's public kline WebSocket keeps shared crypto references
        // current between the bounded REST history sweeps.
        let stream_market_directory = market_dir.clone();
        thread::Builder::new()
            .name("binance-kline-daemon".to_string())
            .spawn(move || {
                research_proxy::background_binance_kline_stream_loop(stream_market_directory)
            })
            .expect("spawn Binance kline stream daemon");
    } else {
        eprintln!("[Daemon] Background work disabled by QUANT_DISABLE_BACKGROUND_WORK.");
    }

    let listener = TcpListener::bind(&address).expect("bind Rust quant service");
    eprintln!("================================================================");
    eprintln!("  QUANT SYSTEM DASHBOARD SERVER RUNNING (NATIVE RUST)");
    eprintln!("  Local Address: http://{address}");
    eprintln!("  SSE Event Stream: http://{address}/api/events");
    eprintln!("  Scan Interval: {scan_interval_min} min");
    eprintln!("  Provider sync: Tokocrypto native first; Binance, Coinbase, Bybit, OKX, and Yahoo research proxies");
    if background_work_enabled {
        eprintln!("  Binance public kline streams: completed 15m, 1h, and 1d shared crypto references");
    }
    eprintln!("  Market directory: {market_dir}");
    eprintln!("  App database: {app_database}");
    eprintln!("  Signal database: {signal_database}");
    eprintln!("  Version: {SERVICE_VERSION}");
    eprintln!("================================================================");

    let request_admission = Arc::new(request_admission::RequestAdmission::new(
        MAX_CONCURRENT_HTTP_REQUESTS,
    ));
    for stream in listener.incoming().flatten() {
        let Some(permit) = request_admission.clone().try_acquire() else {
            routes::reject_service_unavailable(stream);
            continue;
        };

        let market_dir = market_dir.clone();
        let state = Arc::clone(&state);
        if let Err(error) = thread::Builder::new()
            .name("http-request".to_string())
            .spawn(move || {
                let _permit = permit;
                routes::handle(stream, &market_dir, &state);
            })
        {
            eprintln!("[HTTP] Failed to start bounded request handler: {error}");
        }
    }
}

fn migration_market_database_path() -> String {
    env::var("QUANT_MARKET_DB").unwrap_or_else(|_| DEFAULT_LEGACY_MARKET_DB.into())
}

#[cfg(test)]
#[path = "chart_indicators_tests.rs"]
mod chart_indicators_tests;
#[cfg(test)]
#[path = "main_candle_scope_tests.rs"]
mod main_candle_scope_tests;
#[cfg(test)]
#[path = "main_shared_candle_tests.rs"]
mod main_shared_candle_tests;
#[cfg(test)]
#[path = "main_dss_bressert_tests.rs"]
mod main_dss_bressert_tests;
#[cfg(test)]
#[path = "main_market_history_tests.rs"]
mod main_market_history_tests;
#[cfg(test)]
#[path = "main_research_tests.rs"]
mod main_research_tests;
#[cfg(test)]
#[path = "main_storage_tests.rs"]
mod main_storage_tests;
#[cfg(test)]
#[path = "main_test_support.rs"]
mod main_test_support;
#[cfg(test)]
#[path = "plan_identity_tests.rs"]
mod plan_identity_tests;
#[cfg(test)]
#[path = "request_admission_tests.rs"]
mod request_admission_tests;
#[cfg(test)]
#[path = "reward_risk_rule_tests.rs"]
mod reward_risk_rule_tests;
#[cfg(test)]
#[path = "scan_staleness_tests.rs"]
mod scan_staleness_tests;
