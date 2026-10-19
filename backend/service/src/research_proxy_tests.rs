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
             CREATE TABLE bars(symbol TEXT,interval TEXT,source TEXT,ts INTEGER);
             CREATE TABLE ingestion_runs(symbol TEXT,interval TEXT,source TEXT,exchange_id TEXT,
                product_id TEXT,data_provider TEXT,attempted_at TEXT,status TEXT,
                rows_received INTEGER,error TEXT);",
        )
        .unwrap();
    connection
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
    for index in 0..30 {
        connection
            .execute(
                "INSERT INTO bars(symbol,interval,source,ts) VALUES('BBCA.JK','15m',
                    'stockbit_regular_yfinance_fallback',?1)",
                [index],
            )
            .unwrap();
    }
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
    for index in 0..30 {
        connection
            .execute(
                "INSERT INTO bars(symbol,interval,source,ts) VALUES('BTC-USD','1h',
                    'pintu_futures_bybit_linear_proxy',?1)",
                [index],
            )
            .unwrap();
    }

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
