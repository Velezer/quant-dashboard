use super::*;
use crate::main_test_support::*;

fn register_fixture(conn: &Connection, id: &str, product: &str, verified: bool, category: &str) {
    conn.execute(
        "INSERT INTO app.instruments(instrument_id,exchange_id,product_id,asset_category_id,
            symbol,venue_symbol,underlying_symbol,storage_symbol,quote_currency,listing_status,
            venue_verified,registration_source,price_source_policy,registered_at,updated_at)
         VALUES(?1,'pluang',?2,?3,'ENTRYTEST','ENTRYTEST','ENTRYTEST','ENTRYTEST-USD','USD',
            'public_listed_snapshot',?4,'test','research_proxy',1,1)",
        params![id, product, category, verified],
    )
    .unwrap();
}

fn seed_history(conn: &Connection, source: &str, product: &str, close: f64) {
    conn.execute(
        "INSERT INTO series(symbol,interval,source,exchange_id,product_id,data_provider,
        timezone,price_basis,instrument_kind) VALUES('ENTRYTEST-USD','15m',?1,'pluang',?2,
        'binance','UTC','research_proxy','spot')",
        params![source, product],
    )
    .unwrap();
    let current_open = (now_secs() as i64).div_euclid(900) * 900 * NANOS_PER_SECOND;
    for index in 0..=32_i64 {
        conn.execute(
            "INSERT INTO bars(symbol,interval,source,ts,open,high,low,close,volume,ingested_at)
            VALUES('ENTRYTEST-USD','15m',?1,?2,?3,?4,?5,?3,100,'test')",
            params![
                source,
                current_open - (32 - index) * 900 * NANOS_PER_SECOND,
                close,
                close + 1.0,
                close - 1.0
            ],
        )
        .unwrap();
    }
}

#[test]
fn reports_keep_exact_product_source_completed_bars_and_durable_aggregate_identity() {
    let path = temporary_database_path("entry-product-evidence");
    let conn = open_db_minimal(&path).unwrap();
    register_fixture(&conn, "entry.spot", "pluang_spot", true, "crypto_spot");
    register_fixture(
        &conn,
        "entry.perp",
        "pluang_perpetual",
        true,
        "crypto_perpetual",
    );
    seed_history(
        &conn,
        "pluang_spot_binance_spot_proxy",
        "pluang_spot",
        100.0,
    );
    seed_history(
        &conn,
        "pluang_perpetual_binance_usdm_futures_proxy",
        "pluang_perpetual",
        200.0,
    );
    let spot = run_entry_validation(&conn, &path, "entry.spot", "15m").unwrap();
    let perp = run_entry_validation(&conn, &path, "entry.perp", "15m").unwrap();
    assert_eq!(spot["source"]["id"], "pluang_spot_binance_spot_proxy");
    assert_eq!(
        perp["source"]["id"],
        "pluang_perpetual_binance_usdm_futures_proxy"
    );
    assert_eq!(spot["history"]["bars"], 32);
    assert_eq!(perp["history"]["bars"], 32);
    assert_ne!(
        spot["history"]["data_fingerprint"],
        perp["history"]["data_fingerprint"]
    );
    assert_eq!(spot["models"].as_array().unwrap().len(), 2);
    assert_eq!(spot["sizing"]["status"], "BLOCKED");
    assert_eq!(spot["costs"]["execution_costs_complete"], false);
    for model in spot["models"].as_array().unwrap() {
        assert_eq!(model["entry_validation_status"], "VALIDATION_PENDING");
        assert_eq!(model["checks"]["venue_native_history"], false);
        assert_eq!(model["sizing_eligible"], false);
    }
    assert_eq!(
        entry_validation_saved(&conn, "entry.spot", "15m").unwrap(),
        Some(spot.clone())
    );
    drop(conn);
    let reopened = open_db_minimal(&path).unwrap();
    assert_eq!(
        entry_validation_saved(&reopened, "entry.spot", "15m").unwrap(),
        Some(spot)
    );
    drop(reopened);
    remove_temporary_database(&path);
    remove_temporary_database(&database::app_database_path(&path));
}

#[test]
fn matching_app_categories_backtest_shared_reference_without_merging_perpetual_history() {
    let path = temporary_database_path("entry-shared-reference");
    let conn = open_db_minimal(&path).unwrap();
    register_fixture(
        &conn,
        "entry.pluang.spot",
        "pluang_spot",
        true,
        "crypto_spot",
    );
    register_fixture(
        &conn,
        "entry.pintu.spot",
        "pintu_pro_spot",
        true,
        "crypto_spot",
    );
    conn.execute(
        "UPDATE app.instruments SET exchange_id='pintu' WHERE instrument_id='entry.pintu.spot'",
        [],
    )
    .unwrap();
    register_fixture(
        &conn,
        "entry.perp",
        "pluang_perpetual",
        true,
        "crypto_perpetual",
    );
    let spot_source = "shared_crypto_spot_binance_spot_proxy";
    let perpetual_source = "shared_crypto_perpetual_binance_usdm_futures_proxy";
    for (source, category, close) in [
        (spot_source, "crypto_spot", 100.0),
        (perpetual_source, "crypto_perpetual", 200.0),
    ] {
        seed_history(&conn, source, "", close);
        conn.execute("UPDATE series SET exchange_id=?1,instrument_kind=?2,price_basis='binance_usdt_reference' WHERE source=?3",
            params![crate::research_proxy::shared::SOURCE_EXCHANGE_ID, category, source]).unwrap();
    }
    let pluang = run_entry_validation(&conn, &path, "entry.pluang.spot", "15m").unwrap();
    let pintu = run_entry_validation(&conn, &path, "entry.pintu.spot", "15m").unwrap();
    let perpetual = run_entry_validation(&conn, &path, "entry.perp", "15m").unwrap();
    assert_eq!(pluang["source"]["id"], spot_source);
    assert_eq!(pintu["source"]["id"], spot_source);
    assert_eq!(perpetual["source"]["id"], perpetual_source);
    assert_eq!(pluang["history"]["bars"], 32);
    assert_eq!(
        pluang["history"]["data_fingerprint"],
        pintu["history"]["data_fingerprint"]
    );
    assert_ne!(
        pluang["history"]["data_fingerprint"],
        perpetual["history"]["data_fingerprint"]
    );
    for report in [&pluang, &pintu, &perpetual] {
        assert_eq!(report["source"]["shared_reference"], true);
        assert_eq!(report["source"]["is_proxy"], true);
        assert_eq!(report["source"]["venue_native"], false);
        assert_eq!(report["sizing"]["status"], "BLOCKED");
    }
    drop(conn);
    remove_temporary_database(&path);
    remove_temporary_database(&database::app_database_path(&path));
}

#[test]
fn missing_history_pending_identity_and_unsupported_model_are_distinct_saved_states() {
    let path = temporary_database_path("entry-empty-evidence");
    let conn = open_db_minimal(&path).unwrap();
    register_fixture(&conn, "entry.none", "pluang_spot", true, "crypto_spot");
    register_fixture(
        &conn,
        "entry.pending",
        "pluang_perpetual",
        false,
        "crypto_perpetual",
    );
    register_fixture(
        &conn,
        "entry.unsupported",
        "pluang_test_other",
        true,
        "tokenized_stock",
    );
    let missing = run_entry_validation(&conn, &path, "entry.none", "15m").unwrap();
    assert_eq!(missing["status"], "NO_HISTORY");
    assert_eq!(missing["history"]["bars"], 0);
    assert_eq!(
        run_entry_validation(&conn, &path, "entry.pending", "15m").unwrap()["status"],
        "PENDING_VERIFICATION"
    );
    let unsupported = run_entry_validation(&conn, &path, "entry.unsupported", "15m").unwrap();
    assert_eq!(unsupported["status"], "UNSUPPORTED_MODEL");
    assert!(unsupported["models"].as_array().unwrap().is_empty());
    assert!(entry_validation_saved(&conn, "entry.none", "15m")
        .unwrap()
        .is_some());
    assert!(entry_validation_saved(&conn, "entry.none", "1h")
        .unwrap()
        .is_none());
    assert!(run_entry_validation(&conn, &path, "unknown", "15m").is_err());
    assert!(run_entry_validation(&conn, &path, "entry.none", "5m").is_err());
    drop(conn);
    remove_temporary_database(&path);
    remove_temporary_database(&database::app_database_path(&path));
}

#[test]
fn candle_change_detector_is_stable_and_covers_every_ohlcv_field() {
    let bar = StrategyBar {
        time: 1,
        open: 100.0,
        high: 102.0,
        low: 99.0,
        close: 101.0,
        volume: 1000.0,
    };
    let initial = candle_fingerprint(&[bar.clone()]);
    assert_eq!(initial, candle_fingerprint(&[bar.clone()]));
    for changed in [
        StrategyBar {
            time: 2,
            ..bar.clone()
        },
        StrategyBar {
            open: 100.1,
            ..bar.clone()
        },
        StrategyBar {
            high: 103.0,
            ..bar.clone()
        },
        StrategyBar {
            low: 98.0,
            ..bar.clone()
        },
        StrategyBar {
            close: 101.1,
            ..bar.clone()
        },
        StrategyBar {
            volume: 1001.0,
            ..bar.clone()
        },
    ] {
        assert_ne!(initial, candle_fingerprint(&[changed]));
    }
}
