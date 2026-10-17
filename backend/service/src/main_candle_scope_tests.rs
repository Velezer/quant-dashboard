use super::*;
use crate::main_test_support::*;

fn insert_product_series(
    connection: &Connection,
    product_id: &str,
    source: &str,
    first_close: f64,
) {
    connection
        .execute(
            "INSERT INTO series(
                 symbol,interval,source,timezone,price_basis,instrument_kind,
                 last_success,last_attempt,last_error,exchange_id,data_provider,
                 product_id,instrument_id
             ) VALUES('SAME-USD','15m',?1,'UTC','binance_usdt_reference',
                      'spot',strftime('%Y-%m-%d %H:%M:%S','now') || ' UTC',
                      strftime('%Y-%m-%d %H:%M:%S','now') || ' UTC',NULL,
                      'pluang','binance',?2,?3)",
            params![source, product_id, format!("pluang.{product_id}.same")],
        )
        .expect("product-scoped series should insert");
    let latest_completed_candle = (now_secs() as i64).div_euclid(900) * 900 - 900;
    let first_candle_time_ns = (latest_completed_candle - 31 * 900) * 1_000_000_000;
    for index in 0..32_i64 {
        let close = first_close + index as f64;
        connection
            .execute(
                "INSERT INTO bars(
                     symbol,interval,source,ts,open,high,low,close,volume,ingested_at
                 ) VALUES('SAME-USD','15m',?1,?2,?3,?4,?5,?3,100,'test')",
                params![
                    source,
                    first_candle_time_ns + index * 900 * 1_000_000_000,
                    close,
                    close + 1.0,
                    close - 1.0
                ],
            )
            .expect("product-specific candle should insert");
    }
}

#[test]
fn same_ticker_uses_the_requested_pluang_product_history() {
    let path = temporary_database_path("product-candle-scope");
    let connection = open_db_minimal(&path).expect("storage schema should initialize");
    insert_product_series(
        &connection,
        "pluang_spot",
        "pluang_spot_binance_spot_proxy",
        100.0,
    );
    insert_product_series(
        &connection,
        "pluang_perpetual",
        "pluang_perpetual_binance_usdm_futures_proxy",
        200.0,
    );

    let spot = load_series_candles_for_product(
        &connection,
        "SAME-USD",
        "15m",
        720,
        "pluang",
        "pluang_spot",
    )
    .expect("Pluang Spot history should load");
    let perpetual = load_series_candles_for_product(
        &connection,
        "SAME-USD",
        "15m",
        720,
        "pluang",
        "pluang_perpetual",
    )
    .expect("Pluang Perpetual history should load");

    assert_eq!(spot.len(), 32);
    assert_eq!(perpetual.len(), 32);
    assert_eq!(spot[0].close, 100.0);
    assert_eq!(perpetual[0].close, 200.0);

    drop(connection);
    remove_temporary_database(&path);
}

#[test]
fn registered_product_does_not_fall_through_to_another_products_bars() {
    let path = temporary_database_path("no-cross-product-candle-fallback");
    let connection = open_db_minimal(&path).expect("storage schema should initialize");
    insert_product_series(
        &connection,
        "pluang_spot",
        "pluang_spot_binance_spot_proxy",
        100.0,
    );

    let candles = load_series_candles_for_product(
        &connection,
        "SAME-USD",
        "15m",
        720,
        "pluang",
        "pluang_perpetual",
    )
    .expect("missing product history should be a valid empty state");

    assert!(candles.is_empty());
    drop(connection);
    remove_temporary_database(&path);
}

#[test]
fn scan_returns_only_screener_rows_for_a_registered_product() {
    let path = temporary_database_path("pluang-signal-screener");
    let connection = open_db_minimal(&path).expect("storage schema should initialize");
    connection
        .execute("DELETE FROM instruments", [])
        .expect("fixture should isolate its registered universe");
    connection
        .execute(
            "INSERT INTO instruments(
                 instrument_id,exchange_id,product_id,asset_category_id,symbol,
                 venue_symbol,underlying_symbol,storage_symbol,quote_currency,
                 listing_status,venue_verified,execution_supported,registration_source,
                 price_source_policy,registered_at,updated_at,is_active
             ) VALUES('pluang.pluang_spot.same','pluang','pluang_spot','crypto_spot',
                      'SAME','SAME','SAME','SAME-USD','USD','public_listed_snapshot',
                      1,0,'pluang_listing_snapshot','research_proxy',unixepoch(),unixepoch(),1)",
            [],
        )
        .expect("app-listed Pluang instrument should register");
    insert_product_series(
        &connection,
        "pluang_spot",
        "pluang_spot_binance_spot_proxy",
        100.0,
    );
    let result = run_rust_scan(&connection, &path, 60).expect("registered scan should complete");
    assert!(result["signals"].is_array(), "scan output: {result}");
    assert!(result["signals"].as_array().unwrap().is_empty());
    assert!(result.get("running_plans").is_none());
    assert!(result.get("potential_plans").is_none());
    let persisted_plan_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM trade_plans
              WHERE exchange_id='pluang' AND product_id='pluang_spot'",
            [],
            |row| row.get(0),
        )
        .expect("legacy history table remains available");
    assert_eq!(persisted_plan_count, 0);

    drop(connection);
    remove_temporary_database(&path);
}
