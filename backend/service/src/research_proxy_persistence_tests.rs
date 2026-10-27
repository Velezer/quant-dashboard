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

#[test]
fn shared_proxy_refreshes_from_different_apps_keep_one_owner_and_candle_history() {
    let connection = super::super::open_db_minimal(":memory:")
        .expect("full market and app schemas should initialize");
    let source = super::shared::canonical_source("crypto_spot", "binance_spot_proxy")
        .expect("Binance spot has a canonical shared source");
    let first = [
        Bar {
            ts: 1_700_000_000,
            open: 100.0,
            high: 102.0,
            low: 99.0,
            close: 101.0,
            volume: 10.0,
        },
        Bar {
            ts: 1_700_000_900,
            open: 101.0,
            high: 103.0,
            low: 100.0,
            close: 102.0,
            volume: 11.0,
        },
    ];
    persist(
        &connection,
        "BTC-USD",
        "crypto_spot",
        "pintu",
        "binance",
        &source,
        "BTCUSDT",
        "15m",
        "binance_usdt_reference",
        &first,
    )
    .expect("the first app should save the canonical provider series");
    let correction = [
        Bar {
            ts: 1_700_000_900,
            open: 102.0,
            high: 105.0,
            low: 100.5,
            close: 104.0,
            volume: 15.0,
        },
        Bar {
            ts: 1_700_001_800,
            open: 104.0,
            high: 107.0,
            low: 103.0,
            close: 106.0,
            volume: 16.0,
        },
    ];
    for app in ["pluang", "tokocrypto"] {
        persist(
            &connection,
            "BTC-USD",
            "crypto_spot",
            app,
            "binance",
            &source,
            "BTCUSDT",
            "15m",
            "binance_usdt_reference",
            &correction,
        )
        .expect("other apps should upsert the same canonical series");
    }

    let series_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM series WHERE symbol='BTC-USD' AND interval='15m'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(series_count, 1, "the apps must share one provider series");
    let metadata: (String, String, Option<String>, String, String, String) = connection
        .query_row(
            "SELECT exchange_id,product_id,instrument_id,data_provider,instrument_kind,price_basis
         FROM series WHERE source=?1",
            [&source],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(
        metadata,
        (
            super::shared::SOURCE_EXCHANGE_ID.into(),
            "".into(),
            None,
            "binance".into(),
            "crypto_spot".into(),
            "binance_usdt_reference".into()
        )
    );

    let candles = connection
        .prepare("SELECT ts,open,high,low,close,volume FROM bars WHERE source=?1 ORDER BY ts")
        .unwrap()
        .query_map([&source], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, f64>(1)?,
                row.get::<_, f64>(2)?,
                row.get::<_, f64>(3)?,
                row.get::<_, f64>(4)?,
                row.get::<_, f64>(5)?,
            ))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(
        candles,
        vec![
            (
                1_700_000_000 * crate::NANOS_PER_SECOND,
                100.0,
                102.0,
                99.0,
                101.0,
                10.0
            ),
            (
                1_700_000_900 * crate::NANOS_PER_SECOND,
                102.0,
                105.0,
                100.5,
                104.0,
                15.0
            ),
            (
                1_700_001_800 * crate::NANOS_PER_SECOND,
                104.0,
                107.0,
                103.0,
                106.0,
                16.0
            ),
        ],
        "provider corrections should update the existing candle and retain earlier history"
    );
    let runs: (i64, i64) = connection
        .query_row(
            "SELECT COUNT(*),SUM(exchange_id=?2 AND product_id='' AND data_provider='binance')
         FROM ingestion_runs WHERE source=?1",
            [&source, &super::shared::SOURCE_EXCHANGE_ID.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        runs,
        (3, 3),
        "ingestion provenance must retain the shared research owner across callers"
    );
}
