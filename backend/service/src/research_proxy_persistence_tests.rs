use super::{persist, Bar};
use crate::main_test_support::{remove_temporary_database, temporary_database_path};

#[test]
fn repeated_product_proxy_refresh_does_not_reinsert_known_provider() {
    let path = temporary_database_path("proxy-provider-upsert");
    let connection =
        super::super::open_db_minimal(&path).expect("temporary market database should initialize");
    let bars = [Bar {
        ts: 1_700_000_000,
        open: 100.0,
        high: 102.0,
        low: 99.0,
        close: 101.0,
        volume: 10.0,
    }];

    for _ in 0..2 {
        persist(
            &connection,
            "BTC-USD",
            "crypto_spot",
            "pluang",
            "binance",
            "pluang_spot_binance_spot_proxy",
            "BTCUSDT",
            "15m",
            "binance_usdt_reference",
            &bars,
        )
        .expect("repeated candle refresh should upsert existing proxy rows");
    }

    let providers: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM data_providers WHERE provider_id='binance'",
            [],
            |row| row.get(0),
        )
        .expect("provider catalog should remain readable");
    let candles: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM bars WHERE symbol='BTC-USD' AND interval='15m'
                AND source='pluang_spot_binance_spot_proxy'",
            [],
            |row| row.get(0),
        )
        .expect("candle history should remain readable");

    assert_eq!(providers, 1);
    assert_eq!(candles, 1);
    drop(connection);
    remove_temporary_database(&path);
}

#[test]
fn proxy_storage_keeps_app_exchange_and_data_provider_separate() {
    let connection = super::super::open_db_minimal(":memory:")
        .expect("in-memory market database should initialize");
    let bars = [Bar {
        ts: 1_700_000_000,
        open: 100.0,
        high: 102.0,
        low: 99.0,
        close: 101.0,
        volume: 10.0,
    }];
    persist(
        &connection,
        "BTC-USD",
        "crypto_spot",
        "pluang",
        "binance",
        "pluang_binance_spot_proxy",
        "BTCUSDT",
        "1h",
        "binance_usdt_reference",
        &bars,
    )
    .expect("typed proxy bars and provenance should persist");

    let metadata = super::super::load_series_metadata_for_exchange(
        &connection,
        "BTC-USD",
        "1h",
        true,
        "pluang",
    )
    .expect("chart metadata should load from the app-scoped series");
    assert_eq!(metadata["exchange_id"], "pluang");
    assert_eq!(metadata["data_provider"], "binance");
    assert_eq!(metadata["source"], "pluang_binance_spot_proxy");
    assert_eq!(metadata["source_is_proxy"], true);

    persist(
        &connection,
        "BTC-USD",
        "crypto_spot",
        "pluang",
        "binance",
        "pluang_binance_spot_proxy",
        "BTCUSDT",
        "1d",
        "binance_usdt_reference",
        &bars,
    )
    .expect("daily proxy bars should persist to canonical history");
    let daily_metadata = super::super::load_series_metadata_for_exchange(
        &connection,
        "BTC-USD",
        "1d",
        true,
        "pluang",
    )
    .expect("daily proxy metadata should load for the app exchange");
    assert_eq!(daily_metadata["data_provider"], "binance");
    let daily_rows: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM bars WHERE symbol='BTC-USD' AND interval='1d' AND source='pluang_binance_spot_proxy'",
            [],
            |row| row.get(0),
        )
        .expect("daily bars should be stored");
    assert_eq!(daily_rows, 1);
}
