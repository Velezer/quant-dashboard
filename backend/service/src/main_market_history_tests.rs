use super::*;
use crate::main_test_support::*;

#[test]
fn history_derived_stockbit_registrations_retire_without_deleting_bars_or_app_history() {
    let path = temporary_database_path("retire-history-registration");
    let connection = open_db_minimal(&path).expect("storage schema should initialize");
    connection
        .execute(
            "DELETE FROM instrument_registry_meta WHERE key='history_registration_retired_v1'",
            [],
        )
        .expect("simulate a pre-upgrade registry");
    connection
        .execute(
            "INSERT INTO instruments(instrument_id,exchange_id,asset_category_id,symbol,
                                     venue_symbol,underlying_symbol,storage_symbol,quote_currency,
                                     listing_status,registration_source,price_source_policy,
                                     registered_at,updated_at)
             VALUES('stockbit.idx_stock.legacy','stockbit','idx_stock','LEGACY',
                    'LEGACY','LEGACY','LEGACY.JK','IDR','history_observed',
                    'local_market_history','research_proxy',1,1)",
            [],
        )
        .expect("legacy registration should persist");
    connection
        .execute(
            "INSERT INTO bars(symbol,interval,source,ts,open,high,low,close,volume,ingested_at)
             VALUES('LEGACY.JK','1d','yfinance_fallback',1,100,101,99,100,10,'test')",
            [],
        )
        .expect("historical bar should persist");
    connection
        .execute(
            "INSERT INTO trade_plans(symbol,asset,strategy_id,direction,state,entry_price,
                                     stop_loss,tp1,created_at,is_active)
             VALUES('LEGACY.JK','LEGACY','idx_stockbit_day_trade','WAIT','WATCHLIST',
                    100,90,110,1,1)",
            [],
        )
        .expect("legacy plan should persist");
    instrument_registry::initialize(&connection).expect("retirement should complete");
    instrument_registry::initialize(&connection).expect("retirement should be idempotent");
    let registration_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM instruments WHERE symbol='LEGACY'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let bar_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM bars WHERE symbol='LEGACY.JK'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let active_plan_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM trade_plans WHERE symbol='LEGACY.JK' AND is_active=1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let retired_count: String = connection.query_row(
            "SELECT value FROM instrument_registry_meta WHERE key='history_registration_retired_v1'",
            [], |row| row.get(0)
        ).unwrap();
    assert_eq!(
        (registration_count, bar_count, active_plan_count),
        (0, 1, 1)
    );
    assert_eq!(retired_count, "1");
    drop(connection);
    remove_temporary_database(&path);
}

#[test]
fn registered_daily_targets_are_due_once_per_day_per_app_exchange() {
    let path = temporary_database_path("daily-registered-targets");
    let connection = open_db_minimal(&path).expect("storage schema should initialize");
    for exchange in ["aaa_exchange", "bbb_exchange", "ccc_exchange"] {
        connection
            .execute(
                "INSERT INTO instruments (
                        instrument_id, exchange_id, asset_category_id, symbol,
                        venue_symbol, underlying_symbol, storage_symbol, quote_currency,
                        listing_status, venue_verified, execution_supported,
                        registration_source, price_source_policy, registered_at,
                        updated_at, is_active
                     ) VALUES (?1, ?2, 'crypto_spot', 'SAME', 'SAMEUSDT', 'SAME',
                               'SAME-USD', 'USDT', 'venue_active', 1, 0, 'test',
                               'research_proxy', unixepoch(), unixepoch(), 1)",
                params![format!("{exchange}.crypto_spot.same"), exchange],
            )
            .expect("test instrument should register");
    }
    connection
        .execute(
            "INSERT INTO ingestion_runs(
                    symbol, interval, source, exchange_id, data_provider,
                    attempted_at, status, rows_received
                 ) VALUES ('SAME-USD', '1d', 'daily_scheduler_', 'aaa_exchange', 'scheduler',
                           strftime('%Y-%m-%d %H:%M:%S','now') || ' UTC', 'success', 1)",
            [],
        )
        .expect("completed daily attempt should persist");
    connection
        .execute(
            "INSERT INTO ingestion_runs(
                    symbol, interval, source, exchange_id, data_provider,
                    attempted_at, status, rows_received
                 ) VALUES ('SAME-USD', '1d', 'daily_scheduler_', 'ccc_exchange', 'scheduler',
                           strftime('%Y-%m-%d %H:%M:%S','now','-1 day') || ' UTC', 'success', 1)",
            [],
        )
        .expect("previous-day attempt should persist");

    let due =
        registered_daily_targets(&connection, &path, None).expect("daily due targets should load");
    assert!(!due
        .iter()
        .any(|(exchange, _, symbol, _, _)| exchange == "aaa_exchange" && symbol == "SAME-USD"));
    assert!(due
        .iter()
        .any(|(exchange, _, symbol, _, _)| exchange == "bbb_exchange" && symbol == "SAME-USD"));
    assert!(due
        .iter()
        .any(|(exchange, _, symbol, _, _)| exchange == "ccc_exchange" && symbol == "SAME-USD"));

    connection
            .execute(
                "INSERT INTO ingestion_runs(
                    symbol, interval, source, exchange_id, data_provider,
                    attempted_at, status, rows_received, error
                 ) VALUES ('SAME-USD', '1d', 'daily_scheduler_', 'bbb_exchange', 'scheduler',
                           strftime('%Y-%m-%d %H:%M:%S','now') || ' UTC', 'error', 0, 'provider unavailable')",
                [],
            )
            .expect("failed daily attempt should persist");
    let due_after_failure = registered_daily_targets(&connection, &path, None)
        .expect("remaining daily due targets should load");
    assert!(!due_after_failure
        .iter()
        .any(|(exchange, _, symbol, _, _)| exchange == "bbb_exchange" && symbol == "SAME-USD"));
    assert!(due_after_failure
        .iter()
        .any(|(exchange, _, symbol, _, _)| exchange == "ccc_exchange" && symbol == "SAME-USD"));

    drop(connection);
    remove_temporary_database(&path);
}

#[test]
fn daily_queue_keeps_same_ticker_on_two_pintu_products() {
    let path = temporary_database_path("daily-product-identity");
    let connection = open_db_minimal(&path).unwrap();
    for product in ["pintu_retail_spot", "pintu_pro_spot"] {
        connection
            .execute(
                "INSERT INTO instruments(instrument_id,exchange_id,product_id,asset_category_id,
                   symbol,venue_symbol,underlying_symbol,storage_symbol,quote_currency,
                   listing_status,venue_verified,execution_supported,registration_source,
                   price_source_policy,registered_at,updated_at,is_active)
                 VALUES(?1,'pintu',?2,'crypto_spot','TEST',?3,'TEST','TEST-USD','IDR',
                   'venue_active',1,0,'test','research_proxy',unixepoch(),unixepoch(),1)",
                params![
                    format!("pintu.{product}.test"),
                    product,
                    format!("TEST/IDR")
                ],
            )
            .unwrap();
    }
    let due = registered_daily_targets(&connection, &path, None).unwrap();
    assert!(due
        .iter()
        .any(|row| row.1 == "pintu_retail_spot" && row.2 == "TEST-USD"));
    assert!(due
        .iter()
        .any(|row| row.1 == "pintu_pro_spot" && row.2 == "TEST-USD"));
    record_daily_instrument_attempt(
        &connection,
        "TEST-USD",
        "pintu",
        "pintu_retail_spot",
        "error",
        0,
        "provider unavailable",
    );
    let due = registered_daily_targets(&connection, &path, None).unwrap();
    assert!(!due
        .iter()
        .any(|row| row.1 == "pintu_retail_spot" && row.2 == "TEST-USD"));
    assert!(due
        .iter()
        .any(|row| row.1 == "pintu_pro_spot" && row.2 == "TEST-USD"));
    drop(connection);
    remove_temporary_database(&path);
}

#[test]
fn same_ticker_on_two_products_has_distinct_plan_identity() {
    let mut retail = serde_json::json!({"Exchange":"PINTU",
            "Product_ID":"pintu_retail_spot","Strategy_ID":"spot_research","Class":"CRYPTO_SPOT"});
    let mut pro = serde_json::json!({"Exchange":"PINTU",
            "Product_ID":"pintu_pro_spot","Strategy_ID":"spot_research","Class":"CRYPTO_SPOT"});
    annotate_plan_identity(&mut retail, "BTC-USD");
    annotate_plan_identity(&mut pro, "BTC-USD");
    assert_ne!(retail["Plan_ID"], pro["Plan_ID"]);
}

#[test]
fn storage_stats_returns_bounded_ticker_inventory_summary() {
    let path = temporary_database_path("storage-missing");
    let connection = open_db_minimal(&path).expect("storage schema should initialize");
    connection
        .execute(
            "INSERT INTO series (symbol, interval, source, timezone, price_basis, instrument_kind)
                 VALUES ('EMPTY', '1d', 'yfinance_fallback', 'UTC', 'unadjusted', 'test')",
            [],
        )
        .expect("empty series metadata should persist");

    let stats = load_storage_stats(&connection, &path, None).expect("storage stats should load");
    assert_eq!(stats["status"], "summary");
    assert_eq!(stats["market_directory"], path);
    assert_eq!(stats["ticker_database_count"], 0);
    assert_eq!(stats["series"], serde_json::json!([]));
    assert_eq!(stats["summary_truncated"], false);

    drop(connection);
    remove_temporary_database(&path);
}

#[test]
fn freshness_thresholds_match_market_cadence() {
    assert_eq!(freshness_threshold_seconds("MEDC.JK", "15m"), 30 * 60);
    assert_eq!(freshness_threshold_seconds("BTC-USD", "15m"), 30 * 60);
    assert_eq!(freshness_threshold_seconds("BTC-USD", "1h"), 6 * 3_600);
    assert_eq!(freshness_threshold_seconds("TRUK.JK", "1d"), 3 * 86_400);
    assert_eq!(freshness_threshold_seconds("TSLA", "1w"), 21 * 86_400);
}

#[test]
fn provider_sync_errors_have_actionable_reasons() {
    assert_eq!(
        sync_error_reason("curl exit code: Some(7)"),
        "Market-data provider could not connect"
    );
    assert_eq!(
        sync_error_reason("curl exit code: Some(28)"),
        "Market-data provider request timed out"
    );
    assert_eq!(
        sync_error_reason("Yahoo ingestion is disabled"),
        "Market-data provider sync is disabled"
    );
}

#[test]
fn history_contract_requires_two_calendar_years() {
    assert_eq!(MIN_ENGINE_HISTORY_DAYS, 730);
    assert_eq!(history_coverage_status(0, 0), "MISSING");
    assert_eq!(history_coverage_status(100, 729), "UNDER_TARGET");
    assert_eq!(history_coverage_status(100, 730), "TARGET_MET");
    assert!(history_coverage_warning("1d", 100, 729)
        .expect("daily coverage should explain the gap")
        .contains("729/730"));
    assert!(history_coverage_warning("15m", 100, 65)
        .expect("intraday coverage should explain the provider limit")
        .contains("provider-window limited"));
}

#[test]
fn five_minute_candles_are_removed_from_the_engine_contract() {
    assert!(is_removed_candle_timeframe("5m"));
    assert!(is_removed_candle_timeframe(" 5M "));
    assert!(!is_removed_candle_timeframe("15m"));
    assert_eq!(timeframe_duration_ns("5m"), None);
    assert!(lower_timeframe_candidates("1h")
        .iter()
        .all(|timeframe| *timeframe != "5m"));
}

#[test]
fn strategy_history_range_reads_selected_sqlite_window_and_rolls_up_4h_bars() {
    let path = temporary_database_path("pine-history-range");
    let connection = open_db_minimal(&path).expect("storage schema should initialize");
    connection
        .execute(
            "INSERT INTO series(symbol,interval,source,timezone,price_basis,instrument_kind)
         VALUES('RANGE-USD','1h','yfinance_fallback','UTC','unadjusted','crypto')",
            [],
        )
        .expect("selected history metadata should persist");
    let hour_ns = 3_600_000_000_000_i64;
    for index in 0..8_i64 {
        let open = 10.0 + index as f64;
        connection
            .execute(
                "INSERT INTO bars(symbol,interval,source,ts,open,high,low,close,volume,ingested_at)
             VALUES('RANGE-USD','1h','yfinance_fallback',?1,?2,?3,?4,?5,1,'test')",
                params![index * hour_ns, open, open + 1.0, open - 1.0, open + 0.5],
            )
            .expect("hourly candle should persist");
    }
    let direct = load_series_candles_from_source_between_with_cap(
        &connection,
        "RANGE-USD",
        "1h",
        2,
        "yfinance_fallback",
        100,
        2 * hour_ns,
        6 * hour_ns,
    )
    .expect("bounded direct range should load");
    assert_eq!(
        direct.iter().map(|bar| bar.time).collect::<Vec<_>>(),
        vec![4 * hour_ns, 5 * hour_ns]
    );

    let rolled = load_series_candles_from_source_between_with_cap(
        &connection,
        "RANGE-USD",
        "4h",
        10,
        "yfinance_fallback",
        100,
        4 * hour_ns,
        8 * hour_ns,
    )
    .expect("bounded derived range should roll up hourly candles");
    assert_eq!(rolled.len(), 1);
    assert_eq!(rolled[0].time, 4 * hour_ns);
    assert_eq!(rolled[0].open, 14.0);
    assert_eq!(rolled[0].high, 18.0);
    assert_eq!(rolled[0].low, 13.0);
    assert_eq!(rolled[0].close, 17.5);
    connection
        .execute(
            "INSERT INTO series(symbol,interval,source,timezone,price_basis,instrument_kind)
         VALUES('RANGE.JK','1d','yfinance_fallback','Asia/Jakarta','unadjusted','equity')",
            [],
        )
        .expect("daily history metadata should persist");
    for (timestamp, close) in [(0_i64, 10.0_f64), (hour_ns, 12.0_f64)] {
        connection
            .execute(
                "INSERT INTO bars(symbol,interval,source,ts,open,high,low,close,volume,ingested_at)
             VALUES('RANGE.JK','1d','yfinance_fallback',?1,10,12,9,?2,1,'test')",
                params![timestamp, close],
            )
            .expect("daily candle version should persist");
    }
    let daily = load_series_candles_from_source_between_with_cap(
        &connection,
        "RANGE.JK",
        "1d",
        10,
        "yfinance_fallback",
        100,
        0,
        2 * hour_ns,
    )
    .expect("bounded daily history should load");
    assert_eq!(daily.len(), 1);
    assert_eq!(daily[0].time, hour_ns);
    assert_eq!(daily[0].close, 12.0);
    drop(connection);
    remove_temporary_database(&path);
}

#[test]
fn sync_diagnostics_keep_the_latest_unresolved_series_reason() {
    let path = temporary_database_path("sync-diagnostics");
    let connection = open_db_minimal(&path).expect("storage schema should initialize");
    connection
        .execute(
            "INSERT INTO series (symbol, interval, source, timezone, price_basis, instrument_kind,
                 last_success, last_attempt, last_error)
                 VALUES ('TEST-USD', '15m', 'yfinance_fallback', 'UTC', 'unadjusted', 'test',
                 '2026-09-22 14:00:00 UTC', '2026-09-22 15:00:00 UTC', 'curl exit code: Some(7)')",
            [],
        )
        .expect("failed series metadata should persist");
    connection
            .execute(
                "INSERT INTO ingestion_runs (symbol, interval, source, attempted_at, status, rows_received, error)
                 VALUES ('MISSING-USD', '15m', 'yfinance_fallback', '2026-09-23 10:46:43 UTC', 'error', 0, 'curl exit code: Some(7)')",
                [],
            )
            .expect("latest failed ingestion run should persist");
    connection
        .execute(
            "INSERT INTO series (symbol, interval, source, timezone, price_basis, instrument_kind,
                 last_success, last_attempt, last_error)
                 VALUES ('RESOLVED-USD', '15m', 'yfinance_fallback', 'UTC', 'unadjusted', 'test',
                 '2026-09-23 11:00:00 UTC', '2026-09-23 11:00:00 UTC', NULL)",
            [],
        )
        .expect("resolved series metadata should persist");
    connection
            .execute(
                "INSERT INTO ingestion_runs (symbol, interval, source, attempted_at, status, rows_received, error)
                 VALUES ('RESOLVED-USD', '15m', 'yfinance_fallback', '2026-09-23 10:50:00 UTC', 'error', 0, 'curl exit code: Some(7)')",
                [],
            )
            .expect("resolved failed ingestion run should persist");
    connection
            .execute(
                "INSERT INTO ingestion_runs (symbol, interval, source, attempted_at, status, rows_received, error)
                 VALUES ('RESOLVED-USD', '15m', 'yfinance_fallback', '2026-09-23 11:00:00 UTC', 'success', 100, NULL)",
                [],
            )
            .expect("successful recovery run should persist");
    let diagnostics = load_sync_diagnostics(&connection).expect("diagnostics should load");
    assert_eq!(diagnostics["status"], "degraded");
    assert_eq!(diagnostics["failed_series_count"], 1);
    assert_eq!(diagnostics["latest_failure"]["symbol"], "MISSING-USD");
    assert_eq!(diagnostics["latest_failure"]["interval"], "15m");
    assert_eq!(
        diagnostics["latest_failure"]["reason"],
        "Market-data provider could not connect"
    );
    drop(connection);
    remove_temporary_database(&path);
}

#[test]
fn newer_lower_fallback_history_replaces_stale_direct_chart_tail() {
    let path = temporary_database_path("stale-direct-rollup");
    let connection = open_db_minimal(&path).expect("storage schema should initialize");
    let base_ts = 1_700_000_000_i64 * 1_000_000_000;
    connection
        .execute(
            "INSERT INTO series (symbol, interval, source, timezone, price_basis, instrument_kind)
                 VALUES ('STALE-USD', '1h', 'yfinance_fallback', 'UTC', 'unadjusted', 'test')",
            [],
        )
        .expect("direct fallback metadata should persist");
    connection
        .execute(
            "INSERT INTO series (symbol, interval, source, timezone, price_basis, instrument_kind)
                 VALUES ('STALE-USD', '15m', 'yfinance_fallback', 'UTC', 'unadjusted', 'test')",
            [],
        )
        .expect("lower fallback metadata should persist");
    for index in 0..2_i64 {
        let ts = base_ts + index * 3_600 * 1_000_000_000;
        connection
                .execute(
                    "INSERT INTO bars (symbol, interval, source, ts, open, high, low, close, volume, ingested_at)
                     VALUES ('STALE-USD', '1h', 'yfinance_fallback', ?1, ?2, ?3, ?4, ?5, 10.0, 'test')",
                    params![
                        ts,
                        90.0 + index as f64,
                        92.0 + index as f64,
                        89.0 + index as f64,
                        91.0 + index as f64
                    ],
                )
                .expect("direct fallback candle should persist");
    }
    let lower_base = base_ts + 2 * 86_400 * 1_000_000_000;
    for index in 0..8_i64 {
        let open = 100.0 + index as f64;
        connection
                .execute(
                    "INSERT INTO bars (symbol, interval, source, ts, open, high, low, close, volume, ingested_at)
                     VALUES ('STALE-USD', '15m', 'yfinance_fallback', ?1, ?2, ?3, ?4, ?5, 10.0, 'test')",
                    params![
                        lower_base + index * 15 * 60 * 1_000_000_000,
                        open,
                        open + 2.0,
                        open - 1.0,
                        open + 0.5
                    ],
                )
                .expect("newer lower fallback candle should persist");
    }

    assert_eq!(
        preferred_storage_timeframe(&connection, "STALE-USD", "1h")
            .expect("storage source should resolve"),
        "15m"
    );
    let hourly = load_series_candles(&connection, "STALE-USD", "1h", 10)
        .expect("stale direct chart should roll up newer lower history");
    assert_eq!(
        hourly.last().expect("rollup should contain bars").close,
        107.5
    );
    assert!(hourly.last().expect("rollup should contain bars").time > base_ts);

    drop(connection);
    remove_temporary_database(&path);
}

#[test]
fn lower_timeframe_history_builds_higher_chart_bars() {
    let path = temporary_database_path("timeframe-rollup");
    let connection = open_db_minimal(&path).expect("storage schema should initialize");
    let base_ts = (1_700_000_000_i64 + 12 * 3_600) * 1_000_000_000;
    connection
        .execute(
            "INSERT INTO series (symbol, interval, source, timezone, price_basis, instrument_kind)
                 VALUES ('LOWER-USD', '15m', 'yfinance_fallback', 'UTC', 'unadjusted', 'test')",
            [],
        )
        .expect("lower timeframe metadata should persist");
    for index in 0..8_i64 {
        let open = 100.0 + index as f64;
        let close = open + 0.5;
        connection
                .execute(
                    "INSERT INTO bars (symbol, interval, source, ts, open, high, low, close, volume, ingested_at)
                     VALUES ('LOWER-USD', '15m', 'yfinance_fallback', ?1, ?2, ?3, ?4, ?5, 10.0, 'test')",
                    params![
                        base_ts + index * 15 * 60 * 1_000_000_000,
                        open,
                        open + 2.0,
                        open - 1.0,
                        close
                    ],
                )
                .expect("lower timeframe candle should persist");
    }

    let hourly =
        load_series_candles(&connection, "LOWER-USD", "1h", 10).expect("hourly rollup should load");
    assert_eq!(hourly.len(), 2);
    assert_eq!(hourly[0].open, 100.0);
    assert_eq!(hourly[0].high, 105.0);
    assert_eq!(hourly[0].low, 99.0);
    assert_eq!(hourly[0].close, 103.5);
    assert_eq!(hourly[0].volume, 40.0);
    assert_eq!(hourly[1].open, 104.0);
    assert_eq!(hourly[1].close, 107.5);

    let daily =
        load_series_candles(&connection, "LOWER-USD", "1d", 10).expect("daily rollup should load");
    assert_eq!(daily.len(), 1);
    assert_eq!(daily[0].open, 100.0);
    assert_eq!(daily[0].high, 109.0);
    assert_eq!(daily[0].low, 99.0);
    assert_eq!(daily[0].close, 107.5);
    assert_eq!(daily[0].volume, 80.0);

    let metadata = load_series_metadata(&connection, "LOWER-USD", "1h", false)
        .expect("derived metadata should load");
    assert_eq!(metadata["exists"], true);
    assert_eq!(metadata["storage_timeframe"], "15m");
    assert_eq!(metadata["derived_from_timeframe"], "15m");

    drop(connection);
    remove_temporary_database(&path);
}

#[test]
fn research_metadata_keeps_daily_history_direct() {
    let path = temporary_database_path("research-daily-direct");
    let connection = open_db_minimal(&path).expect("storage schema should initialize");
    let daily_start = 1_700_000_000_i64 * 1_000_000_000;
    let hourly_last = daily_start + 2 * NANOS_PER_DAY + 3 * 3_600 * 1_000_000_000;
    for interval in ["1d", "1h"] {
        connection
                .execute(
                    "INSERT INTO series (symbol, interval, source, timezone, price_basis, instrument_kind)
                     VALUES ('RESEARCH-USD', ?1, 'yfinance_fallback', 'UTC', 'unadjusted', 'test')",
                    params![interval],
                )
                .expect("series metadata should persist");
    }
    for (interval, ts) in [("1d", daily_start), ("1d", daily_start + NANOS_PER_DAY)] {
        connection
                .execute(
                    "INSERT INTO bars (symbol, interval, source, ts, open, high, low, close, volume, ingested_at)
                     VALUES ('RESEARCH-USD', ?1, 'yfinance_fallback', ?2, 100.0, 101.0, 99.0, 100.0, 10.0, 'test')",
                    params![interval, ts],
                )
                .expect("daily candle should persist");
    }
    connection
            .execute(
                "INSERT INTO bars (symbol, interval, source, ts, open, high, low, close, volume, ingested_at)
                 VALUES ('RESEARCH-USD', '1h', 'yfinance_fallback', ?1, 100.0, 101.0, 99.0, 100.0, 10.0, 'test')",
                params![hourly_last],
            )
            .expect("hourly candle should persist");

    let chart_metadata = load_series_metadata(&connection, "RESEARCH-USD", "1d", false)
        .expect("chart metadata should load");
    assert_eq!(chart_metadata["storage_timeframe"], "1h");
    let research_metadata = load_series_metadata(&connection, "RESEARCH-USD", "1d", true)
        .expect("research metadata should load");
    assert_eq!(research_metadata["storage_timeframe"], "1d");
    assert_eq!(research_metadata["rows"], 2);
    assert_eq!(research_metadata["history_status"], "UNDER_TARGET");

    drop(connection);
    remove_temporary_database(&path);
}

#[test]
fn query_defaults_are_bounded() {
    let query = routes::parse_query("/api/chart/candles?symbol=btc-usd&limit=999999&pds=0");
    assert_eq!(query.symbol, "BTC-USD");
    assert_eq!(query.limit, MAX_CANDLES);
    assert_eq!(query.dss.pds, 10);
}

#[test]
fn default_scan_interval_matches_the_fifteen_minute_signal_cycle() {
    assert_eq!(DEFAULT_SCAN_INTERVAL_MINUTES, 15);
}

#[test]
fn scan_freshness_matches_chart_metadata_for_direct_and_derived_history() {
    let path = temporary_database_path("scan-freshness-metadata");
    let connection = open_db_minimal(&path).expect("storage schema should initialize");
    let now_seconds = now_secs() as i64;
    let now_ns = now_seconds * 1_000_000_000;
    let cases = [
        (
            "DIRECT-USD",
            "15m",
            vec![
                now_ns - 45 * 60 * 1_000_000_000,
                now_ns - 15 * 60 * 1_000_000_000,
            ],
        ),
        (
            "STALE-USD",
            "15m",
            vec![
                now_ns - 55 * 60 * 1_000_000_000,
                now_ns - 40 * 60 * 1_000_000_000,
            ],
        ),
        (
            "DERIVED-USD",
            "15m",
            vec![
                now_ns - 3 * 60 * 60 * 1_000_000_000,
                now_ns - 2 * 60 * 60 * 1_000_000_000,
                now_ns - 60 * 60 * 1_000_000_000,
                now_ns - 15 * 60 * 1_000_000_000,
            ],
        ),
    ];
    for (symbol, interval, timestamps) in &cases {
        connection
            .execute(
                "INSERT INTO series(
                     symbol,interval,source,timezone,price_basis,instrument_kind,
                     exchange_id,data_provider,product_id
                 ) VALUES(?1,?2,'pintu_native','UTC','venue_native','spot',
                          'pintu','pintu','pintu_pro_spot')",
                params![symbol, interval],
            )
            .expect("test source should persist");
        for (index, timestamp) in timestamps.iter().enumerate() {
            let price = 100.0 + index as f64;
            connection
                .execute(
                    "INSERT INTO bars(
                         symbol,interval,source,ts,open,high,low,close,volume,ingested_at
                     ) VALUES(?1,?2,'pintu_native',?3,?4,?5,?6,?7,10,'test')",
                    params![
                        symbol,
                        interval,
                        timestamp,
                        price,
                        price + 1.0,
                        price - 1.0,
                        price
                    ],
                )
                .expect("test candle should persist");
        }
    }

    for (symbol, timeframe, expected_stale) in [
        ("DIRECT-USD", "15m", false),
        ("STALE-USD", "15m", true),
        ("DERIVED-USD", "4h", false),
    ] {
        let chart_metadata =
            load_product_candle_metadata(&connection, symbol, "pintu", "pintu_pro_spot", timeframe)
                .expect("chart metadata should load");
        let scan_state = product_candle_scan_state_at(
            &connection,
            symbol,
            "pintu",
            "pintu_pro_spot",
            timeframe,
            now_seconds,
        )
        .expect("scan freshness should load")
        .expect("registered test source should be available");
        assert_eq!(chart_metadata["is_stale"], expected_stale);
        assert_eq!(scan_state.is_stale, expected_stale);
    }

    drop(connection);
    remove_temporary_database(&path);
}
