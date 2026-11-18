use crate::main_test_support::{remove_temporary_database, temporary_database_path};
use crate::market_diagnostics::load_markov_payload;
use crate::{now_secs, open_db_minimal, Connection, Value, NANOS_PER_SECOND};
use rusqlite::params;

const TICKER: &str = "MARKOVTEST";
const SYMBOL: &str = "MARKOVTEST-USD";
const PINTU_PRODUCT: &str = "pintu_pro_futures";
const TOKO_PRODUCT: &str = "tokocrypto_spot";
const BAR_COUNT: i64 = 1_200;
const FIFTEEN_MINUTES_NS: i64 = 900 * NANOS_PER_SECOND;

fn insert_instrument(connection: &Connection, exchange: &str, product: &str, category: &str) {
    connection
        .execute(
            "INSERT INTO app.instruments(
                 instrument_id,exchange_id,product_id,asset_category_id,symbol,
                 venue_symbol,underlying_symbol,storage_symbol,quote_currency,
                 listing_status,venue_verified,registration_source,price_source_policy,
                 registered_at,updated_at,is_active
            ) VALUES(?1,?2,?3,?4,?5,?6,?5,?7,'USDT',
                      'venue_active',1,'test','research_proxy',1,1,1)",
            params![
                format!("{exchange}.{product}.markov_test"),
                exchange,
                product,
                category,
                TICKER,
                format!("{TICKER}USDT"),
                SYMBOL
            ],
        )
        .expect("registered instrument should persist");
}

fn insert_product_history(
    connection: &Connection,
    exchange: &str,
    product: &str,
    source: &str,
    provider: &str,
    price_basis: &str,
    first_close: f64,
    last_success: &str,
) {
    connection
        .execute(
            "INSERT INTO series(
                 symbol,interval,source,exchange_id,product_id,data_provider,timezone,
                 price_basis,instrument_kind,last_success
             ) VALUES(?1,'15m',?2,?3,?4,?5,'UTC',?6,'spot',?7)",
            params![SYMBOL, source, exchange, product, provider, price_basis, last_success],
        )
        .expect("product history metadata should persist");
    let latest = (now_secs() as i64).div_euclid(900) * 900 - 900;
    let first = (latest * NANOS_PER_SECOND) - (BAR_COUNT - 1) * FIFTEEN_MINUTES_NS;
    for index in 0..BAR_COUNT {
        let close = first_close + (index as f64 * 0.01).sin();
        connection
            .execute(
                "INSERT INTO bars(
                     symbol,interval,source,ts,open,high,low,close,volume,ingested_at
                 ) VALUES(?1,'15m',?2,?3,?4,?5,?6,?4,1000,'test')",
                params![
                    SYMBOL,
                    source,
                    first + index * FIFTEEN_MINUTES_NS,
                    close,
                    close + 1.0,
                    close - 1.0
                ],
            )
            .expect("product candle should persist");
    }
}

fn scoped_markov_payload(
    connection: &Connection,
    database: &str,
    timeframe: &str,
    exchange: &str,
    category: &str,
) -> Value {
    load_markov_payload(
        connection,
        database,
        timeframe,
        1_000,
        Some(SYMBOL),
        Some(exchange),
        Some(category),
    )
    .expect("scoped Markov history should load")
}

#[test]
fn markov_reads_the_selected_app_product_source_for_a_shared_storage_symbol() {
    let path = temporary_database_path("markov-product-source");
    let connection = open_db_minimal(&path).expect("storage should initialize");
    insert_instrument(&connection, "pintu", PINTU_PRODUCT, "crypto_perpetual");
    insert_instrument(&connection, "tokocrypto", TOKO_PRODUCT, "crypto_spot");
    insert_product_history(
        &connection,
        "pintu",
        PINTU_PRODUCT,
        "pintu_futures_native",
        "pintu",
        "last_trade",
        200.0,
        "2000-01-01 00:00:00 UTC",
    );
    insert_product_history(
        &connection,
        "tokocrypto",
        TOKO_PRODUCT,
        "tokocrypto_native",
        "tokocrypto",
        "last_trade",
        100.0,
        "2099-01-01 00:00:00 UTC",
    );

    let payload = scoped_markov_payload(&connection, &path, "15m", "pintu", "crypto_perpetual");

    assert_eq!(payload["assets"].as_array().unwrap().len(), 1);
    assert_eq!(payload["assets"][0]["exchange_id"], "pintu");
    assert!(payload["assets"][0]["current_price"].as_f64().unwrap() > 190.0);
    assert!(payload["ticker_models"]["pintu:crypto_perpetual:MARKOVTEST-USD"].is_object());
    drop(connection);
    remove_temporary_database(&path);
}

#[test]
fn markov_uses_the_same_derived_timeframe_history_as_the_chart_loader() {
    let path = temporary_database_path("markov-derived-timeframe");
    let connection = open_db_minimal(&path).expect("storage should initialize");
    insert_instrument(&connection, "tokocrypto", TOKO_PRODUCT, "crypto_spot");
    insert_product_history(
        &connection,
        "tokocrypto",
        TOKO_PRODUCT,
        "tokocrypto_native",
        "tokocrypto",
        "last_trade",
        100.0,
        "2099-01-01 00:00:00 UTC",
    );

    let payload = scoped_markov_payload(&connection, &path, "30m", "tokocrypto", "crypto_spot");

    assert_eq!(payload["assets"].as_array().unwrap().len(), 1);
    assert!(payload["assets"][0]["available_bars"].as_u64().unwrap() >= 500);
    assert_eq!(payload["assets"][0]["exchange_id"], "tokocrypto");
    drop(connection);
    remove_temporary_database(&path);
}
