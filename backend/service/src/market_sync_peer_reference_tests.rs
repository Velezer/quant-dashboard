use super::*;
use crate::market_sync::RegisteredCandleInstrument;
use crate::main_test_support::{remove_temporary_database, temporary_database_path};

const SHARED_SPOT_SOURCE: &str = "shared_crypto_spot_binance_spot_proxy";

fn add_registration(
    connection: &Connection,
    exchange: &str,
    product: &str,
    category: &str,
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
                format!("{exchange}.{product}.{storage_symbol}"),
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
    let duration = super::super::timeframe_duration_ns("15m").unwrap();
    let now_ns = super::super::now_millis().saturating_mul(quant_core::NANOS_PER_MILLISECOND);
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
    let daily_duration = super::super::timeframe_duration_ns("1d").unwrap();
    let latest_daily = now_ns.div_euclid(daily_duration) * daily_duration - daily_duration;
    connection
        .execute(
            "INSERT INTO series(
                 symbol,interval,source,exchange_id,product_id,instrument_id,data_provider,
                 timezone,price_basis,instrument_kind,last_success
             ) VALUES(?1,'1d',?2,'research_reference','',NULL,'binance','UTC',
                      'binance_usdt_reference','crypto_spot',datetime('now'))",
            params![storage_symbol, SHARED_SPOT_SOURCE],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO bars(
                 symbol,interval,source,ts,open,high,low,close,volume,ingested_at
             ) VALUES(?1,'1d',?2,?3,1,1.1,0.9,1,100,'test')",
            params![storage_symbol, SHARED_SPOT_SOURCE, latest_daily],
        )
        .unwrap();
}

#[test]
fn shared_fresh_interval_is_not_refetched_and_other_interval_remains_needed() {
    let path = temporary_database_path("sync-peer-shared-spot");
    let connection = crate::open_db_minimal(&path).unwrap();
    add_registration(
        &connection,
        "tokocrypto",
        "tokocrypto_spot",
        "crypto_spot",
        "TOKOCRYPTO:SPOT:0G_USDT",
    );
    add_registration(
        &connection,
        "pintu",
        "pintu_retail_spot",
        "crypto_spot",
        "0G-USD",
    );
    add_fresh_shared_spot_candles(&connection, "0G-USD");

    let tokocrypto = RegisteredCandleInstrument {
        exchange_id: "tokocrypto",
        storage_symbol: "TOKOCRYPTO:SPOT:0G_USDT",
        product_id: "tokocrypto_spot",
        underlying_symbol: "0G",
        asset_category_id: "crypto_spot",
        tokocrypto_market: None,
    };
    let shared_symbols = shared_reference_storage_symbols(
        &connection,
        tokocrypto.exchange_id,
        tokocrypto.storage_symbol,
        tokocrypto.underlying_symbol,
        tokocrypto.asset_category_id,
    )
    .unwrap();
    assert_eq!(shared_symbols, vec!["0G-USD"]);
    let reused = fresh_shared_reference_intervals(
        &path,
        tokocrypto,
        &shared_symbols,
        &["15m", "1h", "1d"],
    );
    assert_eq!(reused, HashSet::from(["15m", "1d"]));

    let local_toko_rows: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM bars WHERE symbol='TOKOCRYPTO:SPOT:0G_USDT'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(local_toko_rows, 0, "reuse must not copy candles between shards");

    let futures = RegisteredCandleInstrument {
        exchange_id: "pintu",
        storage_symbol: "PINTU:FUTURES:0G_USDT",
        product_id: "pintu_pro_futures",
        underlying_symbol: "0G",
        asset_category_id: "crypto_perpetual",
        tokocrypto_market: None,
    };
    let futures_symbols = shared_reference_storage_symbols(
        &connection,
        futures.exchange_id,
        futures.storage_symbol,
        futures.underlying_symbol,
        futures.asset_category_id,
    )
    .unwrap();
    let reused_futures = fresh_shared_reference_intervals(&path, futures, &futures_symbols, &["15m"]);
    assert!(reused_futures.is_empty(), "spot candles must not cross categories");

    drop(connection);
    remove_temporary_database(&path);
}
