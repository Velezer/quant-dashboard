use super::*;
use crate::main_test_support::*;

#[test]
fn plan_semantics_keep_exchange_and_signal_model_separate() {
    let mut plan = serde_json::json!({
        "Strategy_ID": "idx_stockbit_swing",
        "Class": "IDX_STOCK",
        "Exchange": "STOCKBIT"
    });

    annotate_plan_identity(&mut plan, "TEST.JK");

    assert_eq!(plan["Exchange"], "STOCKBIT");
    assert_eq!(plan["Asset_Class"], "IDX_STOCK");
    assert_eq!(plan["Trade_Style"], "swing");
    assert_eq!(
        plan["Signal_Strategy_ID"],
        "idx_open_low_rvol_dss_extreme_nwe_touch_v2"
    );
    assert_eq!(
        plan["Signal_Strategy_Name"],
        "Open-Low + Relative Volume · DSS extreme + NWE touch"
    );
    assert_eq!(
        plan["Strategy_ID"],
        "stockbit_swing_dss_extreme_nwe_touch_v2"
    );
    assert_eq!(plan["Engine_Strategy_ID"], "idx_stockbit_swing");
    assert_eq!(plan["Execution_Profile_ID"], "idx_stockbit_swing");
}

#[test]
fn tokenized_stock_venues_normalize_to_tokocrypto() {
    for venue in [
        "tokocrypto",
        "tokocrypto_spot",
        "tokenized_stock",
        "tokenized_stock_spot",
    ] {
        assert_eq!(
            routes::canonical_asset_class(venue, "crypto_futures"),
            "tokocrypto"
        );
    }
}

#[test]
fn runtime_defaults_use_the_app_database_and_ticker_directory() {
    assert_eq!(DEFAULT_MARKET_DIR, "data/market/tickers");
    assert_eq!(DEFAULT_APP_DB, "data/app/app.sqlite3");
    assert_eq!(DEFAULT_SIGNAL_DB, "data/signals/signals.sqlite3");
}

#[test]
fn storage_schema_initializes_and_reads_local_history() {
    let path = temporary_database_path("storage");
    let connection = open_db_minimal(&path).expect("storage schema should initialize");

    let table_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('bars','series','ingestion_runs')",
                [],
                |row| row.get(0),
            )
            .expect("market tables should exist");
    assert_eq!(table_count, 3);

    connection
        .execute(
            "INSERT INTO series (symbol, interval, source, timezone, price_basis, instrument_kind)
                 VALUES ('LOCAL', '15m', 'test_fixture', 'UTC', 'unadjusted', 'test')",
            [],
        )
        .expect("series metadata should persist");
    for (ts, open, high, low, close) in [
        (1_i64, 10.0, 11.0, 9.0, 10.5),
        (2_i64, 10.5, 12.0, 10.0, 11.5),
    ] {
        connection
                .execute(
                    "INSERT INTO bars (symbol, interval, source, ts, open, high, low, close, volume, ingested_at)
                     VALUES ('LOCAL', '15m', 'test_fixture', ?1, ?2, ?3, ?4, ?5, 100.0, 'test')",
                    params![ts, open, high, low, close],
                )
                .expect("candle should persist");
    }

    let candles = load_candles(&connection, "LOCAL", "15m", 100).expect("history should load");
    assert_eq!(candles.len(), 2);
    assert_eq!(candles[0].time, 1);
    assert_eq!(candles[1].close, 11.5);

    drop(connection);
    remove_temporary_database(&path);
}

#[test]
fn daily_candles_keep_latest_row_per_jakarta_session_before_limiting() {
    let path = temporary_database_path("daily-session-duplicates");
    let connection = open_db_minimal(&path).expect("storage schema should initialize");
    connection
        .execute(
            "INSERT INTO series (symbol, interval, source, timezone, price_basis, instrument_kind,
                 last_error)
                 VALUES ('TEST.JK', '1d', 'yfinance_fallback', 'Asia/Jakarta', 'unadjusted',
                 'idx_stock', 'upstream unavailable')",
            [],
        )
        .expect("daily series metadata should persist");

    let hour_ns = 3_600_i64 * 1_000_000_000;
    let utc_day = 1_700_000_000_i64.div_euclid(86_400) * NANOS_PER_DAY;
    let local_midnight = utc_day - 7 * hour_ns;
    for day in 0..3_i64 {
        let first_ts = local_midnight + day * NANOS_PER_DAY;
        let later_ts = first_ts + 9 * hour_ns;
        let price = 100.0 + day as f64;
        for (ts, open, high, low, close, volume) in [
            (first_ts, price, price + 50.0, price - 50.0, price, 100.0),
            (
                later_ts,
                price + 1.0,
                price + 2.0,
                price - 1.0,
                price + 1.5,
                200.0,
            ),
        ] {
            connection
                    .execute(
                        "INSERT INTO bars (symbol, interval, source, ts, open, high, low, close, volume,
                         ingested_at)
                         VALUES ('TEST.JK', '1d', 'yfinance_fallback', ?1, ?2, ?3, ?4, ?5, ?6, 'test')",
                        params![ts, open, high, low, close, volume],
                    )
                    .expect("daily version should persist");
        }
    }

    let raw_rows: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM bars WHERE symbol='TEST.JK' AND interval='1d'",
            [],
            |row| row.get(0),
        )
        .expect("raw history should remain available");
    assert_eq!(raw_rows, 6);

    let candles =
        load_candles(&connection, "TEST.JK", "1d", 2).expect("bounded daily history should load");
    assert_eq!(candles.len(), 2);
    assert_eq!(
        candles[0].time,
        local_midnight + NANOS_PER_DAY + 9 * hour_ns
    );
    assert_eq!(
        candles[1].time,
        local_midnight + 2 * NANOS_PER_DAY + 9 * hour_ns
    );
    assert_eq!(candles[1].open, 103.0);
    assert_eq!(candles[1].high, 104.0);
    assert_eq!(candles[1].low, 101.0);
    assert_eq!(candles[1].close, 103.5);
    assert_eq!(candles[1].volume, 200.0);
    let chart = load_series_candles(&connection, "TEST.JK", "1d", 2)
        .expect("chart should use the same daily sessions");
    assert_eq!(
        chart.iter().map(|candle| candle.time).collect::<Vec<_>>(),
        candles.iter().map(|candle| candle.time).collect::<Vec<_>>()
    );

    for force_direct in [false, true] {
        let metadata = load_series_metadata(&connection, "TEST.JK", "1d", force_direct)
            .expect("daily metadata should load");
        assert_eq!(metadata["rows"], 3);
        assert_eq!(metadata["first_ts"], local_midnight + 9 * hour_ns);
        assert_eq!(metadata["last_ts"], candles[1].time);
        assert_eq!(metadata["coverage_days"], 2);
        assert_eq!(metadata["source"], "yfinance_fallback");
        assert_eq!(metadata["last_error"], "upstream unavailable");
        assert_eq!(metadata["is_stale"], true);
    }

    drop(connection);
    remove_temporary_database(&path);
}

#[test]
fn daily_metadata_coverage_uses_local_session_dates() {
    let path = temporary_database_path("daily-session-coverage");
    let connection = open_db_minimal(&path).expect("storage schema should initialize");
    connection
            .execute(
                "INSERT INTO series (symbol, interval, source, timezone, price_basis, instrument_kind)
                 VALUES ('COVER.JK', '1d', 'test_fixture', 'Asia/Jakarta', 'unadjusted', 'idx_stock')",
                [],
            )
            .expect("daily series metadata should persist");

    let hour_ns = 3_600_i64 * 1_000_000_000;
    let utc_day = 1_700_000_000_i64.div_euclid(86_400) * NANOS_PER_DAY;
    let first_ts = utc_day + 2 * hour_ns; // 09:00 Jakarta
    let last_ts = utc_day + 17 * hour_ns; // 00:00 Jakarta, next session
    for ts in [first_ts, last_ts] {
        connection
                .execute(
                    "INSERT INTO bars (symbol, interval, source, ts, open, high, low, close, volume,
                     ingested_at)
                     VALUES ('COVER.JK', '1d', 'test_fixture', ?1, 100.0, 101.0, 99.0, 100.5, 10.0, 'test')",
                    params![ts],
                )
                .expect("daily candle should persist");
    }

    assert_eq!(history_coverage_days(Some(first_ts), Some(last_ts)), 0);
    for force_direct in [false, true] {
        let metadata = load_series_metadata(&connection, "COVER.JK", "1d", force_direct)
            .expect("daily metadata should load");
        assert_eq!(metadata["rows"], 2);
        assert_eq!(metadata["first_ts"], first_ts);
        assert_eq!(metadata["last_ts"], last_ts);
        assert_eq!(metadata["coverage_days"], 1);
        assert_eq!(metadata["timezone"], "Asia/Jakarta");
    }

    drop(connection);
    remove_temporary_database(&path);
}

#[test]
fn intraday_candles_on_one_local_date_remain_distinct() {
    let path = temporary_database_path("intraday-same-date");
    let connection = open_db_minimal(&path).expect("storage schema should initialize");
    connection
            .execute(
                "INSERT INTO series (symbol, interval, source, timezone, price_basis, instrument_kind)
                 VALUES ('TEST.JK', '1h', 'test_fixture', 'Asia/Jakarta', 'unadjusted', 'idx_stock')",
                [],
            )
            .expect("hourly series metadata should persist");
    let hour_ns = 3_600_i64 * 1_000_000_000;
    let first_ts = 1_700_000_000_i64.div_euclid(86_400) * NANOS_PER_DAY;
    for (index, ts) in [first_ts, first_ts + hour_ns].into_iter().enumerate() {
        connection
                .execute(
                    "INSERT INTO bars (symbol, interval, source, ts, open, high, low, close, volume,
                     ingested_at)
                     VALUES ('TEST.JK', '1h', 'test_fixture', ?1, 100.0, 101.0, 99.0, ?2, 10.0, 'test')",
                    params![ts, 100.0 + index as f64],
                )
                .expect("hourly candle should persist");
    }

    let candles =
        load_candles(&connection, "TEST.JK", "1h", 2).expect("hourly history should load");
    assert_eq!(candles.len(), 2);
    assert_eq!(candles[0].time, first_ts);
    assert_eq!(candles[1].time, first_ts + hour_ns);
    assert_eq!(candles[1].close, 101.0);

    drop(connection);
    remove_temporary_database(&path);
}

#[test]
fn scan_does_not_fetch_providers_and_registered_selection_is_exchange_agnostic() {
    let path = temporary_database_path("scan-without-provider");
    let connection = open_db_minimal(&path).expect("storage schema should initialize");
    connection
        .execute(
            "INSERT INTO series (symbol, interval, source, timezone, price_basis, instrument_kind)
                 VALUES ('BTC-USD', '15m', 'yfinance_fallback', 'UTC', 'unadjusted', 'crypto')",
            [],
        )
        .expect("fallback series should persist");

    run_rust_scan(&connection, &path, DEFAULT_SCAN_INTERVAL_MINUTES)
        .expect("scan should use stored history");
    let targets = registered_hourly_targets(&connection, Some("BTC-USD"), &[], 512)
        .expect("registered hourly targets should be selectable without provider calls");
    assert_eq!(
        targets.first().map(|target| target.1.as_str()),
        Some("BTC-USD")
    );
    assert!(targets.iter().any(|target| target.0 != "tokocrypto"));
    let attempts: i64 = connection
        .query_row("SELECT COUNT(*) FROM ingestion_runs", [], |row| row.get(0))
        .expect("ingestion runs should be readable");
    assert_eq!(attempts, 0);
    let metadata: (Option<String>, Option<String>) = connection
        .query_row(
            "SELECT last_attempt, last_error FROM series WHERE symbol='BTC-USD' AND interval='15m'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("series metadata should remain readable");
    assert_eq!(metadata, (None, None));

    drop(connection);
    remove_temporary_database(&path);
}

#[test]
fn price_history_does_not_register_an_app_ticker_or_screener_signal() {
    let path = temporary_database_path("unregistered-price-only");
    let connection = open_db_minimal(&path).expect("storage schema should initialize");
    connection
        .execute(
            "INSERT INTO series(symbol, interval, source, exchange_id, data_provider,
                                timezone, price_basis, instrument_kind)
             VALUES('UNREG.JK','15m','stockbit_yfinance_fallback','stockbit','yfinance',
                    'Asia/Jakarta','unadjusted','idx_stock')",
            [],
        )
        .expect("price source should persist");
    for index in 0..40_i64 {
        connection
            .execute(
                "INSERT INTO bars(symbol,interval,source,ts,open,high,low,close,volume,ingested_at)
                 VALUES('UNREG.JK','15m','stockbit_yfinance_fallback',?1,
                        100.0,102.0,99.0,101.0,1000.0,'test')",
                [1_700_000_000_000_000_000_i64 + index * 900_000_000_000],
            )
            .expect("price bar should persist");
    }
    assert!(
        instrument_registry::lookup_for_exchange(&connection, "stockbit", "UNREG.JK").is_none()
    );
    let result = run_rust_scan(&connection, &path, DEFAULT_SCAN_INTERVAL_MINUTES)
        .expect("scan should use app registrations");
    assert!(result["signals"].as_array().unwrap().is_empty());
    assert!(result.get("running_plans").is_none());
    assert!(result.get("potential_plans").is_none());
    let bars: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM bars WHERE symbol='UNREG.JK'",
            [],
            |row| row.get(0),
        )
        .expect("price history should remain");
    assert_eq!(bars, 40);
    drop(connection);
    remove_temporary_database(&path);
}
