use super::*;
use crate::main_test_support::*;

fn insert_registered_test_product(connection: &Connection, product_id: &str, instrument_id: &str) {
    connection
        .execute(
            "INSERT INTO instruments(
                 instrument_id,exchange_id,product_id,asset_category_id,symbol,
                 venue_symbol,underlying_symbol,storage_symbol,quote_currency,
                 listing_status,venue_verified,execution_supported,registration_source,
                 price_source_policy,registered_at,updated_at,is_active
             ) VALUES(?1,'pluang',?2,'crypto_spot','SAME','SAME','SAME','SAME-USD','USD',
                      'public_listed_snapshot',1,0,'pluang_listing_snapshot','research_proxy',
                      unixepoch(),unixepoch(),1)",
            params![instrument_id, product_id],
        )
        .expect("registered Pluang product should persist");
}

#[test]
fn no_signal_watchlist_explains_the_required_confirmation_wait() {
    let bars = [StrategyBar {
        time: 1,
        open: 100.0,
        high: 101.0,
        low: 99.0,
        close: 100.0,
        volume: 1_000.0,
    }];

    let plan = build_watchlist_plan(
        &bars,
        "TEST",
        "IDX_STOCK",
        "STOCKBIT",
        "idx_stockbit_day_trade",
        "15m",
    )
    .expect("current reference price should keep a watchlist row");
    let reason = plan["Decision_Reason"].as_str().unwrap_or_default();

    assert_eq!(plan["Direction"], "WAIT");
    assert_eq!(plan["Decision"], "WAIT");
    assert!(reason.contains("DSS Bressert"));
    assert!(reason.contains("Nadaraya-Watson"));
    assert!(reason.contains("500 completed candles"));
    assert!(reason.contains("matching validation evidence"));
}

fn insert_registered_stockbit_test_instrument(connection: &Connection) {
    connection
        .execute(
            "INSERT INTO instruments(
                 instrument_id,exchange_id,product_id,asset_category_id,symbol,
                 venue_symbol,underlying_symbol,storage_symbol,quote_currency,
                 listing_status,venue_verified,execution_supported,registration_source,
                 price_source_policy,registered_at,updated_at,is_active
             ) VALUES('stockbit.stockbit_regular.test','stockbit','stockbit_regular',
                      'idx_stock','TEST','TEST','TEST','TEST.JK','IDR',
                      'public_listed_snapshot',1,0,'idx_directory','research_proxy',
                      unixepoch(),unixepoch(),1)",
            [],
        )
        .expect("registered Stockbit instrument should persist");
    connection
        .execute(
            "INSERT INTO series(
                 symbol,interval,source,timezone,price_basis,instrument_kind,
                 last_success,last_attempt,last_error,exchange_id,data_provider,
                 product_id,instrument_id
             ) VALUES('TEST.JK','1d','stockbit_yfinance_fallback','Asia/Jakarta',
                      'unadjusted','idx_stock',
                      strftime('%Y-%m-%d %H:%M:%S','now') || ' UTC',
                      strftime('%Y-%m-%d %H:%M:%S','now') || ' UTC',NULL,
                      'stockbit','yfinance','stockbit_regular',
                      'stockbit.stockbit_regular.test')",
            [],
        )
        .expect("Stockbit research candle source should persist");

    let stale_after_seconds = freshness_threshold_seconds("TEST.JK", "1d");
    let latest_stale_candle = now_secs() as i64 - stale_after_seconds - 86_400;
    let first_candle_time_ns = (latest_stale_candle - 39 * 86_400) * 1_000_000_000;
    for index in 0..40_i64 {
        let close = 100.0 + index as f64;
        connection
            .execute(
                "INSERT INTO bars(
                     symbol,interval,source,ts,open,high,low,close,volume,ingested_at
                 ) VALUES('TEST.JK','1d','stockbit_yfinance_fallback',
                          ?1,?2,?3,?4,?2,100,'test')",
                params![
                    first_candle_time_ns + index * 86_400 * 1_000_000_000,
                    close,
                    close + 1.0,
                    close - 1.0
                ],
            )
            .expect("stale Stockbit daily candle should persist");
    }
}

#[test]
fn stale_signal_history_emits_no_screener_row_and_queues_its_product() {
    let path = temporary_database_path("stale-scan-refresh");
    let connection = open_db_minimal(&path).expect("storage schema should initialize");
    connection
        .execute("DELETE FROM instruments", [])
        .expect("fixture should isolate the registered universe");
    insert_registered_test_product(&connection, "pluang_spot", "pluang.pluang_spot.same");
    connection
        .execute(
            "INSERT INTO series(
                 symbol,interval,source,timezone,price_basis,instrument_kind,
                 last_success,last_attempt,last_error,exchange_id,data_provider,
                 product_id,instrument_id
             ) VALUES('SAME-USD','15m','pluang_spot_binance_spot_proxy','UTC',
                      'binance_usdt_reference','spot',
                      strftime('%Y-%m-%d %H:%M:%S','now') || ' UTC',
                      strftime('%Y-%m-%d %H:%M:%S','now') || ' UTC',NULL,
                      'pluang','binance','pluang_spot','pluang.pluang_spot.same')",
            [],
        )
        .expect("product candle source should persist");

    let stale_after_seconds = freshness_threshold_seconds("SAME-USD", "15m");
    let latest_stale_candle = ((now_secs() as i64 - stale_after_seconds - 1_800) / 900) * 900;
    let first_candle_time_ns = (latest_stale_candle - 31 * 900) * 1_000_000_000;
    for index in 0..32_i64 {
        let close = 100.0 + index as f64;
        connection
            .execute(
                "INSERT INTO bars(
                     symbol,interval,source,ts,open,high,low,close,volume,ingested_at
                 ) VALUES('SAME-USD','15m','pluang_spot_binance_spot_proxy',
                          ?1,?2,?3,?4,?2,100,'test')",
                params![
                    first_candle_time_ns + index * 900 * 1_000_000_000,
                    close,
                    close + 1.0,
                    close - 1.0
                ],
            )
            .expect("stale product candle should persist");
    }

    let state = AppState::new(path.clone(), 3_600);
    let result = run_rust_scan_with_state(&connection, &path, 60, &state)
        .expect("stale scan should complete from stored history");
    assert!(result["signals"].as_array().unwrap().is_empty());
    assert!(result.get("potential_plans").is_none());

    let priority = state
        .take_stale_signal_sync_priority("15m")
        .expect("stale signal scan should queue its product for refresh");
    assert_eq!(priority.symbol, "SAME-USD");
    assert_eq!(priority.exchange_id, "pluang");
    assert_eq!(priority.product_id, "pluang_spot");

    drop(connection);
    remove_temporary_database(&path);
}

#[test]
fn sync_priority_selects_the_exact_product_when_storage_symbol_is_shared() {
    let path = temporary_database_path("stale-product-priority");
    let connection = open_db_minimal(&path).expect("storage schema should initialize");
    connection
        .execute("DELETE FROM instruments", [])
        .expect("fixture should isolate the registered universe");
    insert_registered_test_product(&connection, "pluang_spot", "pluang.pluang_spot.same");
    connection
        .execute(
            "INSERT INTO instruments(
                 instrument_id,exchange_id,product_id,asset_category_id,symbol,
                 venue_symbol,underlying_symbol,storage_symbol,quote_currency,
                 listing_status,venue_verified,execution_supported,registration_source,
                 price_source_policy,registered_at,updated_at,is_active
             ) VALUES('pluang.pluang_perpetual.same','pluang','pluang_perpetual',
                      'crypto_perpetual','SAME','SAME','SAME','SAME-USD','USD',
                      'public_listed_snapshot',1,0,'pluang_listing_snapshot','research_proxy',
                      unixepoch(),unixepoch(),1)",
            [],
        )
        .expect("second app product should persist");

    let priority = StaleSignalSyncPriority {
        symbol: "SAME-USD".to_string(),
        exchange_id: "pluang".to_string(),
        product_id: "pluang_perpetual".to_string(),
    };
    let hourly = registered_hourly_targets(&connection, None, Some(&priority), 512)
        .expect("hourly targets should honor exact product priority");
    let daily = registered_daily_targets(&connection, &path, Some(&priority))
        .expect("daily targets should honor exact product priority");

    assert_eq!(
        hourly.first().map(|target| target.2.as_str()),
        Some("pluang_perpetual")
    );
    assert_eq!(
        daily.first().map(|target| target.1.as_str()),
        Some("pluang_perpetual")
    );

    drop(connection);
    remove_temporary_database(&path);
}

#[test]
fn stale_refresh_status_stays_queued_for_an_already_pending_product() {
    let state = AppState::new(":memory:".to_string(), 3_600);

    assert!(state.prioritize_stale_signal_sync("same-usd", "pluang", "pluang_spot", "15m"));
    assert!(state.prioritize_stale_signal_sync("SAME-USD", "pluang", "pluang_spot", "15m"));
    assert!(!state.prioritize_stale_signal_sync("SAME-USD", "pluang", "pluang_perpetual", "15m"));

    let queued = state
        .take_stale_signal_sync_priority("15m")
        .expect("pending refresh should remain available to the worker");
    assert_eq!(queued.product_id, "pluang_spot");
}

#[test]
fn stale_stockbit_daily_history_queues_without_emitting_a_screener_row() {
    let path = temporary_database_path("stale-stockbit-daily-refresh");
    let connection = open_db_minimal(&path).expect("storage schema should initialize");
    connection
        .execute("DELETE FROM instruments", [])
        .expect("fixture should isolate the registered universe");
    insert_registered_stockbit_test_instrument(&connection);

    let state = AppState::new(path.clone(), 3_600);
    let result = run_rust_scan_with_state(&connection, &path, 60, &state)
        .expect("Stockbit stale scan should complete from stored history");
    assert!(result["signals"].as_array().unwrap().is_empty());
    assert!(result.get("potential_plans").is_none());

    let priority = state
        .take_stale_signal_sync_priority("1d")
        .expect("stale Stockbit swing history should queue daily refresh");
    assert_eq!(priority.exchange_id, "stockbit");
    assert_eq!(priority.product_id, "stockbit_regular");
    assert_eq!(priority.symbol, "TEST.JK");

    drop(connection);
    remove_temporary_database(&path);
}
