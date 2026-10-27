use super::{listing_status, REGISTRY_SCHEMA};
use rusqlite::Connection;
use serde_json::Value;

fn status_database() -> Connection {
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch("ATTACH DATABASE ':memory:' AS signals;")
        .unwrap();
    connection.execute_batch(REGISTRY_SCHEMA).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE series(
                symbol TEXT,interval TEXT,source TEXT,exchange_id TEXT,product_id TEXT,
                instrument_id TEXT,data_provider TEXT,timezone TEXT,price_basis TEXT,
                instrument_kind TEXT,last_success TEXT,last_attempt TEXT,last_error TEXT,
                PRIMARY KEY(symbol,interval,source));
             CREATE TABLE bars(
                symbol TEXT,interval TEXT,source TEXT,ts INTEGER,
                open REAL,high REAL,low REAL,close REAL,volume REAL,
                PRIMARY KEY(symbol,interval,source,ts));
             CREATE INDEX idx_bars_series_ts ON bars(symbol,interval,source,ts);
             CREATE TABLE signals.signal_screener_rows(exchange_id TEXT,product_id TEXT);",
        )
        .unwrap();
    connection
}

fn status_database_without_signal_store() -> Connection {
    let connection = Connection::open_in_memory().unwrap();
    connection.execute_batch(REGISTRY_SCHEMA).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE series(
                symbol TEXT,interval TEXT,source TEXT,exchange_id TEXT,product_id TEXT,
                instrument_id TEXT,data_provider TEXT,timezone TEXT,price_basis TEXT,
                instrument_kind TEXT,last_success TEXT,last_attempt TEXT,last_error TEXT,
                PRIMARY KEY(symbol,interval,source));
             CREATE TABLE bars(
                symbol TEXT,interval TEXT,source TEXT,ts INTEGER,
                open REAL,high REAL,low REAL,close REAL,volume REAL,
                PRIMARY KEY(symbol,interval,source,ts));
             CREATE INDEX idx_bars_series_ts ON bars(symbol,interval,source,ts);",
        )
        .unwrap();
    connection
}

fn insert_instrument(connection: &Connection, product: &str, instrument_id: &str) {
    let category = crate::research_proxy::shared::category_for_product("pintu", product).unwrap();
    connection
        .execute(
            "INSERT INTO instruments(
                instrument_id,exchange_id,product_id,asset_category_id,symbol,venue_symbol,
                underlying_symbol,storage_symbol,quote_currency,listing_status,venue_verified,
                execution_supported,registration_source,source_url,source_snapshot_at,
                price_source_policy,contract_metadata_json,registered_at,updated_at,is_active)
             VALUES(?1,'pintu',?2,?3,'BTC','BTCUSDT','BTC','BTC-USD','USDT',
                'venue_active',1,0,'app_listing','https://pintu.example/markets','2026-09-27',
                'research_proxy','{}',1,1,1)",
            rusqlite::params![instrument_id, product, category],
        )
        .unwrap();
}

#[test]
fn listing_status_counts_bars_and_screener_rows_per_exact_app_product() {
    let connection = status_database();
    insert_instrument(
        &connection,
        "pintu_pro_futures",
        "pintu.pintu_pro_futures.btc",
    );
    insert_instrument(&connection, "pintu_pro_spot", "pintu.pintu_pro_spot.btc");
    connection
        .execute_batch(
            "INSERT INTO series(symbol,interval,source,exchange_id,product_id,instrument_id,
                                data_provider,timezone,price_basis,instrument_kind)
             VALUES('BTC-USD','15m','proxy','pintu','pintu_pro_futures',
                    'pintu.pintu_pro_futures.btc','yfinance','UTC','research_proxy','crypto_perpetual'),
                   ('BTC-USD','1d','proxy','pintu','pintu_pro_spot',
                    'pintu.pintu_pro_spot.btc','yfinance','UTC','research_proxy','crypto_spot');
             INSERT INTO bars VALUES('BTC-USD','15m','proxy',1,10,11,9,10,100),
                                    ('BTC-USD','1d','proxy',2,10,12,8,11,200);
             INSERT INTO signals.signal_screener_rows(exchange_id,product_id)
             VALUES('pintu','pintu_pro_futures'),
                   ('pintu','pintu_pro_spot');",
        )
        .unwrap();

    let status = listing_status(&connection, ":memory:").unwrap();
    let products = status["products"].as_array().unwrap();
    let product = |id: &str| -> &Value {
        products
            .iter()
            .find(|row| row["product_id"] == id)
            .unwrap_or_else(|| panic!("missing product {id}"))
    };

    assert_eq!(product("pintu_pro_futures")["with_15m_candles"], 1);
    assert_eq!(product("pintu_pro_futures")["with_daily_candles"], 0);
    assert_eq!(product("pintu_pro_futures")["signals"], 1);
    assert_eq!(product("pintu_pro_spot")["with_15m_candles"], 0);
    assert_eq!(product("pintu_pro_spot")["with_daily_candles"], 1);
    assert_eq!(product("pintu_pro_spot")["signals"], 1);
}

#[test]
fn listing_status_without_signal_database_still_reports_app_and_candle_readiness() {
    let connection = status_database_without_signal_store();
    insert_instrument(
        &connection,
        "pintu_pro_futures",
        "pintu.pintu_pro_futures.btc",
    );
    connection
        .execute_batch(
            "INSERT INTO series(symbol,interval,source,exchange_id,product_id,instrument_id,
                                data_provider,timezone,price_basis,instrument_kind)
             VALUES('BTC-USD','15m','proxy','pintu','pintu_pro_futures',
                    'pintu.pintu_pro_futures.btc','yfinance','UTC','research_proxy','crypto_perpetual');
             INSERT INTO bars VALUES('BTC-USD','15m','proxy',1,10,11,9,10,100);",
        )
        .unwrap();

    let status = super::listing_status_for_exchange(&connection, ":memory:", Some("pintu"))
        .expect("listing status must not require the signal database");
    let product = status["products"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["product_id"] == "pintu_pro_futures")
        .unwrap();

    assert_eq!(product["with_15m_candles"], 1);
    assert_eq!(product["signals"], Value::Null);
}

#[test]
fn shared_research_history_counts_for_matching_products_without_crossing_categories() {
    let connection = status_database();
    for product in ["pintu_retail_spot", "pintu_pro_spot", "pintu_pro_futures"] {
        insert_instrument(&connection, product, &format!("pintu.{product}.btc"));
    }
    connection
        .execute_batch(
            "INSERT INTO series(symbol,interval,source,exchange_id,product_id,
                                data_provider,timezone,price_basis,instrument_kind)
             VALUES('BTC-USD','15m','shared_crypto_spot_binance_spot_proxy',
                    'research_reference','','binance','UTC','binance_usdt_reference','crypto_spot'),
                   ('BTC-USD','1h','shared_crypto_spot_binance_spot_proxy',
                    'research_reference','','binance','UTC','binance_usdt_reference','crypto_spot'),
                   ('BTC-USD','1d','shared_crypto_perpetual_binance_usdm_futures_proxy',
                    'research_reference','','binance','UTC','binance_usdt_reference','crypto_perpetual');
             INSERT INTO bars VALUES
                ('BTC-USD','15m','shared_crypto_spot_binance_spot_proxy',1,10,11,9,10,100),
                ('BTC-USD','1h','shared_crypto_spot_binance_spot_proxy',2,10,11,9,10,100),
                ('BTC-USD','1d','shared_crypto_perpetual_binance_usdm_futures_proxy',3,20,21,19,20,100);",
        )
        .unwrap();

    let status = listing_status(&connection, ":memory:").unwrap();
    let products = status["products"].as_array().unwrap();
    for product in ["pintu_retail_spot", "pintu_pro_spot"] {
        let row = products
            .iter()
            .find(|row| row["product_id"] == product)
            .unwrap();
        assert_eq!(row["with_15m_candles"], 1);
        assert_eq!(row["with_daily_candles"], 0);
    }
    let perpetual = products
        .iter()
        .find(|row| row["product_id"] == "pintu_pro_futures")
        .unwrap();
    assert_eq!(perpetual["with_15m_candles"], 0);
    assert_eq!(perpetual["with_daily_candles"], 1);
}
