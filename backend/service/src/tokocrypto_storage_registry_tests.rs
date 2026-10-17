use super::*;

fn registry_fixture() -> Connection {
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch(
            "CREATE TABLE instruments(
                instrument_id TEXT PRIMARY KEY,exchange_id TEXT,product_id TEXT,
                asset_category_id TEXT,symbol TEXT,venue_symbol TEXT,underlying_symbol TEXT,
                storage_symbol TEXT,quote_currency TEXT,listing_status TEXT,venue_verified INTEGER,
                execution_supported INTEGER,registration_source TEXT,source_url TEXT,
                source_snapshot_at TEXT,price_source_policy TEXT,contract_metadata_json TEXT,
                registered_at INTEGER,updated_at INTEGER,is_active INTEGER,
                UNIQUE(exchange_id,product_id,symbol));
             CREATE TABLE series(
                symbol TEXT NOT NULL,interval TEXT NOT NULL,source TEXT NOT NULL,
                exchange_id TEXT NOT NULL,product_id TEXT,instrument_id TEXT,
                data_provider TEXT NOT NULL,timezone TEXT NOT NULL,price_basis TEXT NOT NULL,
                instrument_kind TEXT NOT NULL,last_success TEXT,last_attempt TEXT,last_error TEXT,
                PRIMARY KEY(symbol,interval,source));
             CREATE TABLE bars(
                symbol TEXT NOT NULL,interval TEXT NOT NULL,source TEXT NOT NULL,ts INTEGER NOT NULL,
                open REAL NOT NULL,high REAL NOT NULL,low REAL NOT NULL,close REAL NOT NULL,
                volume REAL NOT NULL,adj_close REAL,dividends REAL,splits REAL,ingested_at TEXT NOT NULL,
                PRIMARY KEY(symbol,interval,source,ts));
             CREATE TABLE ingestion_runs(
                id INTEGER PRIMARY KEY,symbol TEXT,interval TEXT,source TEXT,exchange_id TEXT,
                product_id TEXT,data_provider TEXT,attempted_at TEXT,status TEXT,
                rows_received INTEGER,error TEXT);",
        )
        .unwrap();
    connection
}

fn sample_bar() -> MarketBar {
    MarketBar {
        open_time_ms: 1_700_000_000_000,
        open: 10.0,
        high: 11.0,
        low: 9.0,
        close: 10.5,
        volume: 3.0,
    }
}

#[test]
fn native_candle_persistence_reuses_the_existing_registered_instrument() {
    let connection = registry_fixture();
    connection
        .execute(
            "INSERT INTO instruments(
                instrument_id,exchange_id,product_id,asset_category_id,symbol,venue_symbol,
                underlying_symbol,storage_symbol,quote_currency,listing_status,venue_verified,
                execution_supported,registration_source,source_url,source_snapshot_at,
                price_source_policy,contract_metadata_json,registered_at,updated_at,is_active
             ) VALUES(
                'tokocrypto.crypto_spot.btc','tokocrypto','tokocrypto_spot','crypto_spot',
                'BTC','BTC_IDR','BTC','TOKOCRYPTO:SPOT:BTC_IDR','IDR','venue_active',1,0,
                'tokocrypto_live_symbols_api','','','tokocrypto_native','{}',1,1,1)",
            [],
        )
        .unwrap();

    persist_sync(
        &connection,
        "TOKOCRYPTO:SPOT:BTC_IDR",
        "crypto_spot",
        &[("15m", &[sample_bar()])],
    )
    .unwrap();

    let instrument_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM instruments", [], |row| row.get(0))
        .unwrap();
    let stored_identity: (String, String) = connection
        .query_row(
            "SELECT instrument_id,product_id FROM series
              WHERE symbol='TOKOCRYPTO:SPOT:BTC_IDR' AND interval='15m'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();

    assert_eq!(instrument_count, 1);
    assert_eq!(stored_identity.0, "tokocrypto.crypto_spot.btc");
    assert_eq!(stored_identity.1, "tokocrypto_spot");
}

#[test]
fn native_candle_persistence_does_not_register_unlisted_instruments() {
    let connection = registry_fixture();

    let result = persist_sync(
        &connection,
        "TOKOCRYPTO:SPOT:BTC_IDR",
        "crypto_spot",
        &[("15m", &[sample_bar()])],
    );

    assert!(result
        .unwrap_err()
        .contains("venue-verified registered instrument"));
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM instruments", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM bars", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        0
    );
}
