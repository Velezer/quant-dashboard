use super::*;

fn fixture() -> Connection {
    let db = Connection::open_in_memory().unwrap();
    db.execute_batch(super::super::schema::MARKET_SCHEMA)
        .unwrap();
    db.execute_batch(
        "ATTACH DATABASE ':memory:' AS app;
         CREATE TABLE app.storage_shard_checkpoints(migration_id TEXT,storage_symbol TEXT,
             source_hash TEXT,migrated_row_counts_json TEXT,destination_hash TEXT,completed_at TEXT,
             PRIMARY KEY(migration_id,storage_symbol));",
    )
    .unwrap();
    db
}

fn series(db: &Connection, source: &str, app: &str, product: &str, category: &str, provider: &str) {
    let basis = if provider == "binance" {
        "binance_usdt_reference"
    } else {
        "venue_native"
    };
    db.execute(
        "INSERT INTO series(symbol,interval,source,exchange_id,product_id,data_provider,
            timezone,price_basis,instrument_kind,last_success,last_attempt)
         VALUES('BTC-USD','15m',?1,?2,?3,?4,'UTC',?5,?6,'2026-10-27 10:00:00 UTC','2026-10-27 10:00:00 UTC')",
        params![source,app,product,provider,basis,category],
    ).unwrap();
}

fn bar(db: &Connection, source: &str, time: i64, close: f64, ingested: &str) {
    db.execute(
        "INSERT INTO bars(symbol,interval,source,ts,open,high,low,close,volume,adj_close,dividends,splits,ingested_at)
         VALUES('BTC-USD','15m',?1,?2,?3,?3,?3,?3,2,?3,0,1,?4)",
        params![source,time,close,ingested],
    ).unwrap();
}

fn migrate(db: &Connection) -> Result<usize, String> {
    db.execute_batch("BEGIN IMMEDIATE").unwrap();
    let result = merge_ticker(db, "BTC-USD");
    db.execute_batch(if result.is_ok() { "COMMIT" } else { "ROLLBACK" })
        .unwrap();
    result
}

#[test]
fn migration_merges_history_by_category_and_preserves_native_data_and_app_diagnostics() {
    let db = fixture();
    let retail = "pintu_retail_spot_binance_spot_proxy";
    let pluang = "pluang_spot_binance_spot_proxy";
    let shared = "shared_crypto_spot_binance_spot_proxy";
    let perpetual = "pluang_perpetual_binance_usdm_futures_proxy";
    series(
        &db,
        retail,
        "pintu",
        "pintu_retail_spot",
        "crypto_spot",
        "binance",
    );
    series(
        &db,
        pluang,
        "pluang",
        "pluang_spot",
        "crypto_spot",
        "binance",
    );
    series(
        &db,
        shared,
        SOURCE_EXCHANGE_ID,
        "",
        "crypto_spot",
        "binance",
    );
    series(
        &db,
        perpetual,
        "pluang",
        "pluang_perpetual",
        "crypto_perpetual",
        "binance",
    );
    series(
        &db,
        "tokocrypto_native",
        "tokocrypto",
        "tokocrypto_spot",
        "crypto_spot",
        "tokocrypto",
    );
    bar(&db, retail, 1, 10.0, "2026-10-27 08:00:00 UTC");
    bar(&db, retail, 2, 12.0, "2026-10-27 08:00:00 UTC");
    bar(&db, pluang, 2, 20.0, "2026-10-27 09:00:00 UTC");
    bar(&db, pluang, 3, 22.0, "2026-10-27 09:00:00 UTC");
    bar(&db, shared, 3, 30.0, "2026-10-27 10:00:00 UTC");
    bar(&db, shared, 4, 40.0, "2026-10-27 10:00:00 UTC");
    bar(&db, perpetual, 2, 200.0, "2026-10-27 09:00:00 UTC");
    bar(
        &db,
        "tokocrypto_native",
        2,
        500.0,
        "2026-10-27 09:00:00 UTC",
    );
    db.execute("INSERT INTO ingestion_runs(symbol,interval,source,exchange_id,product_id,data_provider,status,rows_received)
        VALUES('BTC-USD','15m',?1,'pintu','pintu_retail_spot','binance','success',2)", [retail]).unwrap();
    assert_eq!(migrate(&db).unwrap(), 3);
    let values = db
        .prepare("SELECT ts,close FROM bars WHERE source=?1 ORDER BY ts")
        .unwrap()
        .query_map([shared], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, f64>(1)?))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(values, vec![(1, 10.0), (2, 20.0), (3, 30.0), (4, 40.0)]);
    let metadata: (String, String, Option<String>) = db
        .query_row(
            "SELECT exchange_id,product_id,instrument_id FROM series WHERE source=?1",
            [shared],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(metadata, (SOURCE_EXCHANGE_ID.into(), "".into(), None));
    let derivative: f64 = db.query_row("SELECT close FROM bars WHERE source='shared_crypto_perpetual_binance_usdm_futures_proxy'", [], |row| row.get(0)).unwrap();
    assert_eq!(derivative, 200.0);
    let native: f64 = db
        .query_row(
            "SELECT close FROM bars WHERE source='tokocrypto_native'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(native, 500.0);
    let run: (String, String, String) = db
        .query_row(
            "SELECT source,exchange_id,product_id FROM ingestion_runs",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        run,
        (shared.into(), "pintu".into(), "pintu_retail_spot".into())
    );
    let checkpoint: (String,String,String) = db.query_row("SELECT source_hash,destination_hash,migrated_row_counts_json FROM app.storage_shard_checkpoints", [],
        |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?))).unwrap();
    assert_ne!(checkpoint.0, checkpoint.1);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&checkpoint.2).unwrap()["merged_series"],
        3
    );
    assert_eq!(migrate(&db).unwrap(), 0);
}

#[test]
fn incompatible_destination_rolls_back_the_entire_ticker() {
    let db = fixture();
    let retail = "pintu_retail_spot_binance_spot_proxy";
    let pluang = "pluang_spot_binance_spot_proxy";
    series(
        &db,
        retail,
        "pintu",
        "pintu_retail_spot",
        "crypto_spot",
        "binance",
    );
    series(
        &db,
        pluang,
        "pluang",
        "pluang_spot",
        "crypto_spot",
        "binance",
    );
    series(
        &db,
        "shared_crypto_spot_binance_spot_proxy",
        SOURCE_EXCHANGE_ID,
        "",
        "crypto_spot",
        "binance",
    );
    db.execute("UPDATE series SET timezone='Asia/Jakarta' WHERE source='shared_crypto_spot_binance_spot_proxy'", []).unwrap();
    bar(&db, retail, 1, 10.0, "2026-10-27 08:00:00 UTC");
    bar(&db, pluang, 2, 20.0, "2026-10-27 09:00:00 UTC");
    let before =
        super::super::sharding::hash_tables(&db, "main", &TABLES, Some("BTC-USD")).unwrap();
    assert!(migrate(&db)
        .unwrap_err()
        .contains("incompatible provenance"));
    assert_eq!(
        before,
        super::super::sharding::hash_tables(&db, "main", &TABLES, Some("BTC-USD")).unwrap()
    );
    let checkpoints: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM app.storage_shard_checkpoints",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(checkpoints, 0);
}

#[test]
fn invalid_app_ownership_and_provider_metadata_are_never_relabelled() {
    let db = fixture();
    series(
        &db,
        "pintu_retail_spot_binance_spot_proxy",
        "pluang",
        "pluang_spot",
        "crypto_spot",
        "binance",
    );
    series(
        &db,
        "pluang_spot_binance_spot_proxy",
        "pluang",
        "pluang_spot",
        "crypto_spot",
        "tokocrypto",
    );
    bar(
        &db,
        "pintu_retail_spot_binance_spot_proxy",
        1,
        10.0,
        "2026-10-27 08:00:00 UTC",
    );
    assert_eq!(migrate(&db).unwrap(), 0);
    let count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM series WHERE exchange_id='pluang'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 2);
}

#[test]
fn equal_ingestion_timestamps_use_stable_source_order() {
    let db = fixture();
    let retail = "pintu_retail_spot_binance_spot_proxy";
    let pluang = "pluang_spot_binance_spot_proxy";
    series(
        &db,
        retail,
        "pintu",
        "pintu_retail_spot",
        "crypto_spot",
        "binance",
    );
    series(
        &db,
        pluang,
        "pluang",
        "pluang_spot",
        "crypto_spot",
        "binance",
    );
    bar(&db, retail, 1, 10.0, "2026-10-27 08:00:00 UTC");
    bar(&db, pluang, 1, 20.0, "2026-10-27 08:00:00 UTC");
    assert_eq!(migrate(&db).unwrap(), 2);
    let close: f64 = db
        .query_row(
            "SELECT close FROM bars WHERE source='shared_crypto_spot_binance_spot_proxy'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(close, 20.0);
}

#[test]
fn newer_fractional_ingestions_win_in_both_directions() {
    let db = fixture();
    let retail = "pintu_retail_spot_binance_spot_proxy";
    let shared = "shared_crypto_spot_binance_spot_proxy";
    series(
        &db,
        retail,
        "pintu",
        "pintu_retail_spot",
        "crypto_spot",
        "binance",
    );
    series(
        &db,
        shared,
        SOURCE_EXCHANGE_ID,
        "",
        "crypto_spot",
        "binance",
    );
    bar(&db, retail, 1, 10.0, "2026-10-27T08:00:00.100Z");
    bar(&db, shared, 1, 90.0, "2026-10-27T08:00:00.900Z");
    bar(&db, retail, 2, 90.0, "2026-10-27T08:00:00.900Z");
    bar(&db, shared, 2, 10.0, "2026-10-27T08:00:00.100Z");
    assert_eq!(migrate(&db).unwrap(), 1);
    let wrong: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM bars WHERE source=?1 AND close<>90",
            [shared],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(wrong, 0);
}

#[test]
fn empty_failure_series_can_receive_known_history_without_losing_the_later_error() {
    let db = fixture();
    let retail = "pintu_retail_spot_binance_spot_proxy";
    let shared = "shared_crypto_spot_binance_spot_proxy";
    series(
        &db,
        retail,
        "pintu",
        "pintu_retail_spot",
        "crypto_spot",
        "binance",
    );
    series(
        &db,
        shared,
        SOURCE_EXCHANGE_ID,
        "",
        "crypto_spot",
        "binance",
    );
    db.execute(
        "UPDATE series SET price_basis='research_proxy',last_success=NULL,
        last_attempt='2026-10-27 11:00:00 UTC',last_error='rate limited' WHERE source=?1",
        [shared],
    )
    .unwrap();
    bar(&db, retail, 1, 10.0, "2026-10-27 10:00:00 UTC");
    assert_eq!(migrate(&db).unwrap(), 1);
    let metadata: (String, String, String) = db
        .query_row(
            "SELECT price_basis,last_success,last_error FROM series WHERE source=?1",
            [shared],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        metadata,
        (
            "binance_usdt_reference".into(),
            "2026-10-27 10:00:00 UTC".into(),
            "rate limited".into()
        )
    );
}

#[test]
fn explicit_file_migration_uses_inventory_and_commits_a_repeatable_checkpoint() {
    let directory = std::env::temp_dir().join(format!(
        "quant-shared-crypto-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let market = directory.join("tickers");
    std::fs::create_dir_all(&market).unwrap();
    let app_path = directory.join("app.sqlite3");
    let app = Connection::open(&app_path).unwrap();
    app.execute_batch("CREATE TABLE storage_layout_meta(key TEXT PRIMARY KEY,value TEXT);
        CREATE TABLE market_store_inventory(storage_symbol TEXT PRIMARY KEY,database_filename TEXT,updated_at INTEGER);
        CREATE TABLE storage_shard_checkpoints(migration_id TEXT,storage_symbol TEXT,source_hash TEXT,
            migrated_row_counts_json TEXT,destination_hash TEXT,completed_at TEXT,PRIMARY KEY(migration_id,storage_symbol));").unwrap();
    app.execute(
        "INSERT INTO storage_layout_meta VALUES('storage_layout_version',?1)",
        [super::super::LAYOUT_VERSION],
    )
    .unwrap();
    let ticker_path = super::super::connections::ticker_database_path(&market, "BTC-USD").unwrap();
    app.execute(
        "INSERT INTO market_store_inventory VALUES('BTC-USD',?1,0)",
        [ticker_path.file_name().unwrap().to_str().unwrap()],
    )
    .unwrap();
    let ticker = Connection::open(&ticker_path).unwrap();
    super::super::connections::initialize_ticker_storage(&ticker, "BTC-USD").unwrap();
    let retail = "pintu_retail_spot_binance_spot_proxy";
    series(
        &ticker,
        retail,
        "pintu",
        "pintu_retail_spot",
        "crypto_spot",
        "binance",
    );
    bar(&ticker, retail, 1, 10.0, "2026-10-27 08:00:00 UTC");
    drop(ticker);
    drop(app);
    let first = share_crypto_candles(market.to_str().unwrap(), app_path.to_str().unwrap()).unwrap();
    assert!(first.contains("1 legacy series merged in 1 ticker"));
    let second =
        share_crypto_candles(market.to_str().unwrap(), app_path.to_str().unwrap()).unwrap();
    assert!(second.contains("0 legacy series merged in 0 ticker"));
    let app = Connection::open(&app_path).unwrap();
    let checkpoints: i64 = app
        .query_row(
            "SELECT COUNT(*) FROM storage_shard_checkpoints WHERE migration_id=?1",
            [MIGRATION_ID],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(checkpoints, 1);
    drop(app);
    std::fs::remove_dir_all(directory).unwrap();
}
