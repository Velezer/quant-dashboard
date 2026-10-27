use super::*;
use crate::main_test_support::{remove_temporary_database, temporary_database_path};

const SHARED_SPOT_SOURCE: &str = "shared_crypto_spot_binance_spot_proxy";

fn add_registration(
    connection: &Connection,
    exchange: &str,
    product: &str,
    category: &str,
    symbol: &str,
    storage_symbol: &str,
) {
    connection
        .execute(
            "INSERT INTO instruments(
                 instrument_id,exchange_id,product_id,asset_category_id,symbol,
                 venue_symbol,underlying_symbol,storage_symbol,quote_currency,
                 listing_status,venue_verified,execution_supported,registration_source,
                 price_source_policy,registered_at,updated_at,is_active
             ) VALUES(?1,?2,?3,?4,'0G','0G','0G',?5,'USDT',
                      'public_listed_snapshot',1,0,'test','research_proxy',
                      unixepoch(),unixepoch(),1)",
            params![
                format!("{exchange}.{product}.{symbol}"),
                exchange,
                product,
                category,
                storage_symbol,
            ],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO market_store_inventory(storage_symbol,database_filename,updated_at)
             VALUES(?1,?2,unixepoch())",
            params![storage_symbol, format!("{storage_symbol}.sqlite3")],
        )
        .unwrap();
}

fn add_fresh_shared_spot_candles(connection: &Connection, storage_symbol: &str) {
    let duration = timeframe_duration_ns("15m").unwrap();
    let now_ns = i64::try_from(now_secs()).unwrap() * NANOS_PER_SECOND;
    let latest = now_ns.div_euclid(duration) * duration - duration;
    connection
        .execute(
            "INSERT INTO series(
                 symbol,interval,source,exchange_id,product_id,instrument_id,data_provider,
                 timezone,price_basis,instrument_kind,last_success
             ) VALUES(?1,'15m',?2,'research_reference','',NULL,'binance','UTC',
                      'binance_usdt_reference','crypto_spot',datetime('now'))",
            params![storage_symbol, SHARED_SPOT_SOURCE],
        )
        .unwrap();
    for index in 0..52_i64 {
        let time = latest - (51 - index) * duration;
        connection
            .execute(
                "INSERT INTO bars(
                     symbol,interval,source,ts,open,high,low,close,volume,ingested_at
                 ) VALUES(?1,'15m',?2,?3,1,1.1,0.9,1,100,'test')",
                params![storage_symbol, SHARED_SPOT_SOURCE, time],
            )
            .unwrap();
    }
}

#[test]
fn scan_uses_fresh_shared_candles_from_verified_peer_in_same_category() {
    let path = temporary_database_path("scan-peer-shared-spot");
    let connection = open_db_minimal(&path).unwrap();
    add_registration(
        &connection,
        "tokocrypto",
        "tokocrypto_spot",
        "crypto_spot",
        "0g_usdt",
        "TOKOCRYPTO:SPOT:0G_USDT",
    );
    add_registration(
        &connection,
        "pintu",
        "pintu_retail_spot",
        "crypto_spot",
        "0g_usd",
        "0G-USD",
    );
    add_fresh_shared_spot_candles(&connection, "0G-USD");
    assert_eq!(
        resolve_registered_candle_storage_symbol(
            &connection,
            "tokocrypto",
            "tokocrypto_spot",
            "0G-USD",
        )
        .unwrap()
        .as_deref(),
        Some("0G-USD"),
        "the selected signal's chart must be allowed to open its peer history"
    );

    let selected = open_scan_market_history(
        &connection,
        &path,
        "tokocrypto",
        "tokocrypto_spot",
        "crypto_spot",
        "TOKOCRYPTO:SPOT:0G_USDT",
        "0G",
    )
    .unwrap()
    .expect("verified peer reference should be available");
    assert_eq!(selected.1, "0G-USD");
    let price = product_candle_scan_state_at(
        &selected.0,
        &selected.1,
        "tokocrypto",
        "tokocrypto_spot",
        "15m",
        now_secs() as i64,
    )
    .unwrap()
    .expect("shared reference should expose scan freshness");
    assert!(!price.is_stale);
    assert_eq!(price.source, SHARED_SPOT_SOURCE);
    drop(selected);
    drop(connection);
    remove_temporary_database(&path);
}

#[test]
fn scan_never_shares_research_candles_across_asset_categories() {
    let path = temporary_database_path("scan-peer-category-boundary");
    let connection = open_db_minimal(&path).unwrap();
    add_registration(
        &connection,
        "tokocrypto",
        "tokocrypto_spot",
        "crypto_spot",
        "0g_usdt",
        "TOKOCRYPTO:SPOT:0G_USDT",
    );
    add_registration(
        &connection,
        "pintu",
        "pintu_retail_spot",
        "crypto_spot",
        "0g_usd",
        "0G-USD",
    );
    add_fresh_shared_spot_candles(&connection, "0G-USD");

    let selected = open_scan_market_history(
        &connection,
        &path,
        "pintu",
        "pintu_pro_futures",
        "crypto_perpetual",
        "PINTU:FUTURES:0G_USDT",
        "0G",
    )
    .unwrap();
    assert!(selected.is_none());
    drop(connection);
    remove_temporary_database(&path);
}
