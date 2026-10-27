use super::{
    parse_binance_candles, parse_yahoo_candles, recent_proxy_result, yahoo_request_window,
};
use rusqlite::Connection;

fn proxy_fixture() -> Connection {
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch(
            "CREATE TABLE series(
                symbol TEXT,interval TEXT,source TEXT,exchange_id TEXT,product_id TEXT,
                data_provider TEXT,timezone TEXT,price_basis TEXT,instrument_kind TEXT,
                last_success TEXT,last_attempt TEXT,last_error TEXT,
                PRIMARY KEY(symbol,interval,source));
             CREATE TABLE bars(symbol TEXT,interval TEXT,source TEXT,ts INTEGER,close REAL DEFAULT 100);
             CREATE TABLE ingestion_runs(symbol TEXT,interval TEXT,source TEXT,exchange_id TEXT,
                product_id TEXT,data_provider TEXT,attempted_at TEXT,status TEXT,
                rows_received INTEGER,error TEXT);",
        )
        .unwrap();
    connection
}

fn last_completed_open(duration_seconds: i64) -> i64 {
    (super::super::now_secs() as i64 / duration_seconds - 1) * duration_seconds
}

fn insert_completed_bars(
    connection: &Connection,
    symbol: &str,
    source: &str,
    interval: &str,
    duration_seconds: i64,
    rows: i64,
) {
    let latest = last_completed_open(duration_seconds);
    for index in 0..rows {
        connection
            .execute(
                "INSERT INTO bars(symbol,interval,source,ts) VALUES(?1,?2,?3,?4)",
                rusqlite::params![
                    symbol,
                    interval,
                    source,
                    (latest - index * duration_seconds) * crate::NANOS_PER_SECOND
                ],
            )
            .unwrap();
    }
}

fn shared_proxy_fixture(interval: &str, latest_open: i64) -> Connection {
    let connection = super::super::open_db_minimal(":memory:").unwrap();
    super::persist(
        &connection,
        "BTC-USD",
        "crypto_spot",
        "pintu",
        "binance",
        "shared_crypto_spot_binance_spot_proxy",
        "BTCUSDT",
        interval,
        "binance_usdt_reference",
        &[super::Bar {
            ts: latest_open,
            open: 100.0,
            high: 102.0,
            low: 99.0,
            close: 101.0,
            volume: 10.0,
        }],
    )
    .unwrap();
    connection
}

#[test]
fn shared_15m_history_satisfies_other_app_syncs_without_new_ingestion() {
    let connection = shared_proxy_fixture("15m", last_completed_open(900));
    for (app, product) in [
        ("pintu", "pintu_retail_spot"),
        ("pintu", "pintu_pro_spot"),
        ("pluang", "pluang_spot"),
        ("tokocrypto", "tokocrypto_spot"),
    ] {
        assert_eq!(
            super::sync_15m_for_exchange(
                &connection,
                "BTC-USD",
                "crypto_spot",
                app,
                product,
                "BTC-USD"
            ),
            Ok(1)
        );
    }
    let runs: i64 = connection
        .query_row("SELECT COUNT(*) FROM ingestion_runs", [], |row| row.get(0))
        .unwrap();
    assert_eq!(runs,1,"app requests must reuse the persisted reference instead of requesting another provider page");
}

#[test]
fn shared_provider_failure_cooldown_is_reused_across_app_callers() {
    let connection = super::super::open_db_minimal(":memory:").unwrap();
    let now = super::super::format_timestamp(super::super::now_secs());
    super::record_proxy_failure(
        &connection,
        "BTC-USD",
        "crypto_spot",
        "pintu",
        "binance",
        "shared_crypto_spot_binance_spot_proxy",
        "15m",
        &now,
        "reference provider throttled",
    );
    for (app, product) in [
        ("pintu", "pintu_pro_spot"),
        ("pluang", "pluang_spot"),
        ("tokocrypto", "tokocrypto_spot"),
    ] {
        let result = recent_proxy_result(
            &connection,
            "BTC-USD",
            app,
            product,
            "shared_crypto_spot_binance_spot_proxy",
            "15m",
            900,
        );
        assert!(
            matches!(result,Some(Err(ref error)) if error.contains("reference provider throttled")),
            "{app}: {result:?}"
        );
    }
    let metadata: (String, String) = connection
        .query_row("SELECT exchange_id,product_id FROM series", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .unwrap();
    assert_eq!(metadata, ("research_reference".into(), "".into()));
}

#[test]
fn recently_saved_stale_shared_prices_are_not_success_cache_hits() {
    let connection =
        shared_proxy_fixture("15m", last_completed_open(900) - 3 * crate::SECONDS_PER_DAY);
    assert_eq!(
        recent_proxy_result(
            &connection,
            "BTC-USD",
            "pluang",
            "pluang_spot",
            "shared_crypto_spot_binance_spot_proxy",
            "15m",
            900
        ),
        None
    );
}

#[test]
fn shared_failed_refresh_is_not_hidden_by_a_recent_success_timestamp() {
    let connection = shared_proxy_fixture("15m", last_completed_open(900));
    let now = super::super::format_timestamp(super::super::now_secs());
    super::record_proxy_failure(
        &connection,
        "BTC-USD",
        "crypto_spot",
        "pintu",
        "binance",
        "shared_crypto_spot_binance_spot_proxy",
        "15m",
        &now,
        "newer request failed",
    );
    let result = recent_proxy_result(
        &connection,
        "BTC-USD",
        "pluang",
        "pluang_spot",
        "shared_crypto_spot_binance_spot_proxy",
        "15m",
        900,
    );
    assert!(
        matches!(result,Some(Err(ref error)) if error.contains("newer request failed")),
        "{result:?}"
    );
}

#[test]
fn shared_success_cache_requires_a_positive_completed_candle() {
    let connection = shared_proxy_fixture("15m", last_completed_open(900));
    connection.execute("UPDATE bars SET close=0", []).unwrap();
    assert_eq!(
        recent_proxy_result(
            &connection,
            "BTC-USD",
            "pluang",
            "pluang_spot",
            "shared_crypto_spot_binance_spot_proxy",
            "15m",
            900
        ),
        None
    );
    connection
        .execute(
            "UPDATE bars SET close=101,ts=?1",
            [(super::super::now_secs() as i64) * crate::NANOS_PER_SECOND],
        )
        .unwrap();
    assert_eq!(
        recent_proxy_result(
            &connection,
            "BTC-USD",
            "pluang",
            "pluang_spot",
            "shared_crypto_spot_binance_spot_proxy",
            "15m",
            900
        ),
        None
    );
}

#[test]
fn shared_daily_reference_is_reused_across_app_syncs() {
    let connection = shared_proxy_fixture("1d", last_completed_open(crate::SECONDS_PER_DAY));
    for (app, product) in [("pluang", "pluang_spot"), ("pintu", "pintu_retail_spot")] {
        assert_eq!(
            super::sync_daily_for_exchange(
                &connection,
                "BTC-USD",
                "crypto_spot",
                app,
                product,
                "BTC-USD"
            ),
            Ok(1)
        );
    }
    let runs: i64 = connection
        .query_row("SELECT COUNT(*) FROM ingestion_runs", [], |row| row.get(0))
        .unwrap();
    assert_eq!(runs, 1);
}

#[test]
fn fresh_15m_proxy_history_is_reused_without_another_provider_request() {
    let connection = proxy_fixture();
    let now = super::super::format_timestamp(super::super::now_secs());
    connection
        .execute(
            "INSERT INTO series(symbol,interval,source,exchange_id,product_id,data_provider,
                last_success,last_attempt,last_error)
             VALUES('BBCA.JK','15m','stockbit_regular_yfinance_fallback','stockbit',
                'stockbit_regular','yfinance',?1,?1,NULL)",
            [&now],
        )
        .unwrap();
    insert_completed_bars(
        &connection,
        "BBCA.JK",
        "stockbit_regular_yfinance_fallback",
        "15m",
        900,
        30,
    );
    assert_eq!(
        recent_proxy_result(
            &connection,
            "BBCA.JK",
            "stockbit",
            "stockbit_regular",
            "stockbit_regular_yfinance_fallback",
            "15m",
            900,
        ),
        Some(Ok(30))
    );
}

#[test]
fn proxy_15m_history_is_refetched_before_a_full_sweep_can_skip_it() {
    let connection = proxy_fixture();
    let fourteen_minutes_ago =
        super::super::format_timestamp(super::super::now_secs().saturating_sub(14 * 60));
    connection
        .execute(
            "INSERT INTO series(symbol,interval,source,exchange_id,product_id,data_provider,
                last_success,last_attempt,last_error)
             VALUES('BTC-USD','15m','pintu_futures_bybit_linear_proxy','pintu',
                'pintu_pro_futures','bybit',?1,?1,NULL)",
            [&fourteen_minutes_ago],
        )
        .unwrap();

    assert_eq!(
        recent_proxy_result(
            &connection,
            "BTC-USD",
            "pintu",
            "pintu_pro_futures",
            "pintu_futures_bybit_linear_proxy",
            "15m",
            900,
        ),
        None
    );
}

#[test]
fn one_hour_proxy_history_keeps_its_one_hour_refresh_interval() {
    let connection = proxy_fixture();
    let fourteen_minutes_ago =
        super::super::format_timestamp(super::super::now_secs().saturating_sub(14 * 60));
    connection
        .execute(
            "INSERT INTO series(symbol,interval,source,exchange_id,product_id,data_provider,
                last_success,last_attempt,last_error)
             VALUES('BTC-USD','1h','pintu_futures_bybit_linear_proxy','pintu',
                'pintu_pro_futures','bybit',?1,?1,NULL)",
            [&fourteen_minutes_ago],
        )
        .unwrap();
    insert_completed_bars(
        &connection,
        "BTC-USD",
        "pintu_futures_bybit_linear_proxy",
        "1h",
        3_600,
        30,
    );

    assert_eq!(
        recent_proxy_result(
            &connection,
            "BTC-USD",
            "pintu",
            "pintu_pro_futures",
            "pintu_futures_bybit_linear_proxy",
            "1h",
            3_600,
        ),
        Some(Ok(30))
    );
}

#[test]
fn recent_proxy_failure_is_returned_without_retrying_each_scheduler_minute() {
    let connection = proxy_fixture();
    let now = super::super::format_timestamp(super::super::now_secs());
    connection
        .execute(
            "INSERT INTO series(symbol,interval,source,exchange_id,product_id,data_provider,
                last_success,last_attempt,last_error)
             VALUES('BBCA.JK','15m','stockbit_regular_yfinance_fallback','stockbit',
                'stockbit_regular','yfinance',NULL,?1,'Yahoo request failed')",
            [&now],
        )
        .unwrap();
    let result = recent_proxy_result(
        &connection,
        "BBCA.JK",
        "stockbit",
        "stockbit_regular",
        "stockbit_regular_yfinance_fallback",
        "15m",
        900,
    );
    assert!(matches!(result, Some(Err(message)) if message.contains("Yahoo request failed")));
}

#[test]
fn first_proxy_failure_creates_a_throttled_diagnostic_series() {
    let connection = proxy_fixture();
    let now = super::super::format_timestamp(super::super::now_secs());
    super::record_proxy_failure(
        &connection,
        "BBCA.JK",
        "idx_stock",
        "stockbit",
        "yfinance",
        "stockbit_regular_yfinance_fallback",
        "15m",
        &now,
        "Yahoo temporarily unavailable",
    );
    let result = recent_proxy_result(
        &connection,
        "BBCA.JK",
        "stockbit",
        "stockbit_regular",
        "stockbit_regular_yfinance_fallback",
        "15m",
        900,
    );
    assert!(
        matches!(result, Some(Err(ref message)) if message.contains("temporarily unavailable")),
        "unexpected cooldown result: {result:?}"
    );
}

#[test]
fn yahoo_15m_request_windows_fit_the_parser_bound_by_asset_class() {
    assert_eq!(
        yahoo_request_window("idx_stock", "15m").unwrap(),
        ("15m", "30d")
    );
    assert_eq!(
        yahoo_request_window("crypto_spot", "15m").unwrap(),
        ("15m", "7d")
    );
    assert_eq!(
        yahoo_request_window("crypto_perpetual", "15m").unwrap(),
        ("15m", "7d")
    );
    assert_eq!(
        yahoo_request_window("crypto_futures", "15m").unwrap(),
        ("15m", "7d")
    );
}

#[test]
fn yahoo_non_15m_request_windows_keep_the_existing_interval_mapping() {
    assert_eq!(
        yahoo_request_window("idx_stock", "1h").unwrap(),
        ("60m", "5d")
    );
    assert_eq!(
        yahoo_request_window("crypto_spot", "1d").unwrap(),
        ("1d", "10y")
    );
    assert!(yahoo_request_window("idx_stock", "5m").is_err());
}

#[test]
fn only_completed_valid_yahoo_bars_survive() {
    let body = br#"{"chart":{"result":[{"timestamp":[3600,7200],"indicators":{"quote":[{"open":[10,11],"high":[12,12],"low":[9,10],"close":[11,11],"volume":[2,3]}]}}]}}"#;
    let bars = parse_yahoo_candles(body, 8_000, 3_600, 200).unwrap();
    assert_eq!(bars.len(), 1);
    assert_eq!(bars[0].ts, 3_600);
}

#[test]
fn binance_rows_are_typed_and_incomplete_candles_are_dropped() {
    let body = br#"[[3600000,"10","12","9","11","3",7199999,"33",2,"1","11","0"],[7200000,"11","13","10","12","4",10799999,"44",3,"2","22","0"]]"#;
    let bars = parse_binance_candles(body, 10_799_999, 3_600, 10).unwrap();
    assert_eq!(bars.len(), 1);
    assert_eq!(bars[0].ts, 3_600);
    assert_eq!(bars[0].volume, 3.0);
}
