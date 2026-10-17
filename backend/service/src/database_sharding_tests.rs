use rusqlite::{params, Connection};
use std::path::Path;

use crate::main_test_support::{remove_temporary_database, temporary_database_path};

fn initialize_split_v2_market_schema(connection: &Connection) {
    connection
        .execute_batch(
            "CREATE TABLE main.storage_layout_meta(key TEXT PRIMARY KEY,value TEXT NOT NULL);
             CREATE TABLE main.bars (
                 symbol TEXT NOT NULL, interval TEXT NOT NULL, source TEXT NOT NULL DEFAULT 'unknown',
                 ts INTEGER NOT NULL, open REAL NOT NULL, high REAL NOT NULL, low REAL NOT NULL,
                 close REAL NOT NULL, volume REAL NOT NULL, adj_close REAL, dividends REAL,
                 splits REAL, ingested_at TEXT NOT NULL,
                 PRIMARY KEY(symbol,interval,source,ts)
             );
             CREATE TABLE main.series (
                 symbol TEXT NOT NULL, interval TEXT NOT NULL, source TEXT NOT NULL,
                 timezone TEXT NOT NULL, price_basis TEXT NOT NULL, instrument_kind TEXT NOT NULL,
                 last_success TEXT, last_attempt TEXT, last_error TEXT,
                 exchange_id TEXT NOT NULL DEFAULT 'unknown',
                 data_provider TEXT NOT NULL DEFAULT 'unknown', product_id TEXT NOT NULL DEFAULT '',
                 instrument_id TEXT,
                 PRIMARY KEY(symbol,interval,source)
             );
             CREATE TABLE main.ingestion_runs (
                 id INTEGER PRIMARY KEY, symbol TEXT, interval TEXT, attempted_at TEXT, status TEXT,
                 rows_received INTEGER, error TEXT, source TEXT,
                 exchange_id TEXT NOT NULL DEFAULT 'unknown',
                 data_provider TEXT NOT NULL DEFAULT 'unknown', product_id TEXT NOT NULL DEFAULT ''
             );",
        )
        .expect("initialize completed split-v2 source schema");
}

fn create_shared_fixture(label: &str) -> (String, String, String) {
    let source = temporary_database_path(label);
    let app = format!("{source}.app.sqlite3");
    let market_dir = format!("{source}.tickers");
    let connection = Connection::open(&source).expect("open shared market fixture");
    connection
        .execute("ATTACH DATABASE ?1 AS app", [&app])
        .expect("attach app fixture");
    connection
        .execute_batch("ATTACH DATABASE ':memory:' AS signals;")
        .expect("attach signal fixture");
    initialize_split_v2_market_schema(&connection);
    super::initialize_app_storage_schema(&connection).expect("initialize app tables");
    connection
        .execute(
            "INSERT INTO main.storage_layout_meta(key,value) VALUES('storage_layout_version','2')",
            [],
        )
        .expect("mark completed split source");
    connection
        .execute(
            "INSERT INTO app.storage_layout_meta(key,value) VALUES('storage_layout_version','2')",
            [],
        )
        .expect("mark completed split app database");

    for (symbol, close, exchange, product, provider) in [
        (
            "BTC-USD",
            42_000.0,
            "exchange_one",
            "product_one",
            "provider_one",
        ),
        (
            "btc-usd",
            41_000.0,
            "exchange_two",
            "product_two",
            "provider_two",
        ),
    ] {
        connection
            .execute(
                "INSERT INTO main.series(
                symbol,interval,source,exchange_id,product_id,data_provider,
                timezone,price_basis,instrument_kind
             ) VALUES(?1,'1h','test_provider',?2,?3,?4,'UTC','USD','crypto_spot')",
                params![symbol, exchange, product, provider],
            )
            .expect("insert series fixture");
        connection.execute(
            "INSERT INTO main.bars(symbol,interval,source,ts,open,high,low,close,volume,ingested_at)
             VALUES(?1,'1h','test_provider',1700000000,?2,?2,?2,?2,1.5,'fixture')",
            params![symbol, close],
        ).expect("insert bar fixture");
        connection.execute(
            "INSERT INTO main.ingestion_runs(id,symbol,interval,source,attempted_at,status,rows_received)
             VALUES(?1,?2,'1h','test_provider','fixture','success',1)",
            params![if symbol == "BTC-USD" { 1 } else { 2 }, symbol],
        ).expect("insert ingestion fixture");
    }
    connection
        .execute(
            "INSERT INTO main.series(
            symbol,interval,source,exchange_id,product_id,data_provider,
            timezone,price_basis,instrument_kind
         ) VALUES('BTC-USD','1d','second_provider','exchange_three','product_three',
                  'provider_three','UTC','USD','crypto_spot')",
            [],
        )
        .expect("insert same-symbol alternate product series");
    connection.execute(
        "INSERT INTO main.bars(symbol,interval,source,ts,open,high,low,close,volume,ingested_at)
         VALUES('BTC-USD','1d','second_provider',1700003600,42000,42500,41900,42400,2.5,'fixture')",
        [],
    ).expect("insert same-symbol alternate timeframe bar");
    connection.execute(
        "INSERT INTO main.ingestion_runs(id,symbol,interval,source,attempted_at,status,rows_received)
         VALUES(3,'BTC-USD','1d','second_provider','fixture','success',1)",
        [],
    ).expect("insert same-symbol alternate provider run");
    drop(connection);
    (source, app, market_dir)
}

fn clean_fixture(source: &str, app: &str, market_dir: &str) {
    remove_temporary_database(source);
    remove_temporary_database(app);
    let _ = std::fs::remove_dir_all(market_dir);
}

#[test]
fn sharding_rejects_signal_database_inside_ticker_directory() {
    let (source, app, market_dir) = create_shared_fixture("ticker-shard-signal-path");
    let signals = Path::new(&market_dir).join("signals.sqlite3");
    let error =
        super::shard_market_database(&source, &market_dir, &app, &signals.to_string_lossy())
            .expect_err("the ticker directory must contain only symbol history databases");

    assert!(error.contains("QUANT_SIGNAL_DB must be outside QUANT_MARKET_DIR"));
    assert!(
        Path::new(&source).is_file(),
        "rejected paths must preserve the source"
    );
    clean_fixture(&source, &app, &market_dir);
}

#[cfg(unix)]
#[test]
fn path_guard_resolves_symlinked_market_directory_for_new_files() {
    use std::os::unix::fs::symlink;

    let source = temporary_database_path("ticker-shard-signal-symlink");
    let market_dir = format!("{source}.tickers");
    let alias = format!("{source}.tickers-alias");
    std::fs::create_dir_all(&market_dir).expect("create ticker directory");
    symlink(&market_dir, &alias).expect("symlink ticker directory");
    let signal_database = format!("{alias}/new/signals.sqlite3");

    assert!(super::within(&signal_database, &market_dir).expect("normalize symlinked parent"));

    std::fs::remove_file(&alias).expect("remove ticker directory symlink");
    std::fs::remove_dir_all(&market_dir).expect("remove ticker directory fixture");
}

#[test]
fn sharding_rejects_using_the_market_source_as_the_signal_database() {
    let (source, app, market_dir) = create_shared_fixture("ticker-shard-signal-source");
    let error = super::shard_market_database(&source, &market_dir, &app, &source)
        .expect_err("the signal store must remain separate from the offline migration source");

    assert!(error.contains("QUANT_MARKET_DB and QUANT_SIGNAL_DB must point to different files"));
    assert!(
        Path::new(&source).is_file(),
        "rejected paths must preserve the source"
    );
    clean_fixture(&source, &app, &market_dir);
}

#[test]
fn sharding_initializes_the_configured_signal_database() {
    let (source, app, market_dir) = create_shared_fixture("ticker-shard-signal-schema");
    let signals = format!("{source}.signals.sqlite3");

    super::shard_market_database(&source, &market_dir, &app, &signals)
        .expect("sharding should attach and initialize the configured signal database");

    let connection = Connection::open(&signals).expect("open initialized signal database");
    let tables: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_schema
              WHERE type='table' AND name IN ('signal_screener_rows','signal_screener_status')",
            [],
            |row| row.get(0),
        )
        .expect("read persisted signal screener schema");
    assert_eq!(tables, 2);
    drop(connection);
    clean_fixture(&source, &app, &market_dir);
    remove_temporary_database(&signals);
}

#[test]
fn sharding_preserves_exact_symbols_values_and_completion_record() {
    let (source, app, market_dir) = create_shared_fixture("ticker-shard-success");
    let summary = super::shard_market_database(&source, &market_dir, &app, ":memory:")
        .expect("migration should validate and retire the source");
    assert!(summary.contains("complete; source fnv1a64:"), "{summary}");
    assert!(
        !Path::new(&source).exists(),
        "validated source should be retired"
    );

    let upper_path = super::ticker_database_path(Path::new(&market_dir), "BTC-USD")
        .expect("encode upper-case symbol");
    let lower_path = super::ticker_database_path(Path::new(&market_dir), "btc-usd")
        .expect("encode lower-case symbol");
    assert_ne!(
        upper_path, lower_path,
        "case-sensitive symbols need distinct files"
    );
    assert_eq!(
        super::decode_storage_symbol_filename(&upper_path).as_deref(),
        Some("BTC-USD"),
    );
    assert_eq!(
        super::decode_storage_symbol_filename(&lower_path).as_deref(),
        Some("btc-usd"),
    );

    let paths = super::connections::StoragePaths::new(market_dir.clone(), app.clone(), ":memory:");
    for (symbol, expected_close) in [("BTC-USD", 42_000.0), ("btc-usd", 41_000.0)] {
        let connection = super::connections::open_market_history_for_symbol(&paths, symbol, false)
            .expect("open exact ticker database")
            .expect("ticker database exists");
        let close: f64 = connection
            .query_row(
                "SELECT close FROM main.bars WHERE symbol=?1 AND interval='1h'",
                [symbol],
                |row| row.get(0),
            )
            .expect("read migrated market value");
        assert_eq!(close, expected_close);
    }

    let paths = super::connections::StoragePaths::new(market_dir.clone(), app.clone(), ":memory:");
    let shared = super::connections::open_market_history_for_symbol(&paths, "BTC-USD", false)
        .expect("open shared exact-symbol database")
        .expect("shared ticker exists");
    let shared_series: i64 = shared
        .query_row(
            "SELECT COUNT(*) FROM main.series WHERE symbol='BTC-USD'",
            [],
            |row| row.get(0),
        )
        .expect("count same-symbol provider/timeframe series");
    assert_eq!(shared_series, 2);
    let second_identity: (String, String, String) = shared
        .query_row(
            "SELECT interval,product_id,data_provider FROM main.series
             WHERE symbol='BTC-USD' AND source='second_provider'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("read alternate product/provider series");
    assert_eq!(
        second_identity,
        ("1d".into(), "product_three".into(), "provider_three".into())
    );

    let app_connection = Connection::open(&app).expect("open migration record");
    let (source_hash, table_counts, symbol_counts): (String, String, String) = app_connection
        .query_row(
            "SELECT source_hash,source_table_row_counts_json,source_symbol_row_counts_json
             FROM storage_shard_migrations WHERE migration_id='market_database_to_ticker_files_v1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("read completion record");
    assert!(source_hash.starts_with("fnv1a64:"));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&table_counts).unwrap()["bars"],
        3
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&symbol_counts).unwrap()["BTC-USD"]["series"],
        2
    );
    drop(app_connection);

    let repeated = super::shard_market_database(&source, &market_dir, &app, ":memory:")
        .expect("repeat should detect completed migration");
    assert!(repeated.starts_with("already complete"), "{repeated}");
    clean_fixture(&source, &app, &market_dir);
}

#[test]
fn sharding_resumes_after_a_ticker_conflict_and_retires_only_after_validation() {
    let (source, app, market_dir) = create_shared_fixture("ticker-shard-failure");
    let ticker_path = super::ticker_database_path(Path::new(&market_dir), "btc-usd")
        .expect("resolve ticker file");
    std::fs::create_dir_all(&market_dir).expect("create ticker directory");
    let conflicting = Connection::open(&ticker_path).expect("create conflicting destination");
    conflicting
        .execute_batch("CREATE TABLE untracked_destination_data(value TEXT)")
        .expect("seed unexpected destination");
    drop(conflicting);

    let error = super::shard_market_database(&source, &market_dir, &app, ":memory:")
        .expect_err("unexpected destination data must stop migration");
    assert!(error.contains("without a checkpoint"), "{error}");
    assert!(
        Path::new(&source).is_file(),
        "source must remain available after validation failure"
    );
    let source_connection = Connection::open(&source).expect("reopen retained source");
    let source_bars: i64 = source_connection
        .query_row("SELECT COUNT(*) FROM bars", [], |row| row.get(0))
        .expect("source candle rows remain intact");
    assert_eq!(source_bars, 3);
    drop(source_connection);

    let first_path = super::ticker_database_path(Path::new(&market_dir), "BTC-USD")
        .expect("resolve first completed ticker");
    assert!(
        first_path.is_file(),
        "the first ticker transaction should be checkpointed"
    );
    let first = Connection::open(&first_path).expect("open first completed ticker");
    let first_bars: i64 = first
        .query_row("SELECT COUNT(*) FROM bars", [], |row| row.get(0))
        .expect("count first ticker bars");
    assert_eq!(first_bars, 2);
    drop(first);

    std::fs::remove_file(ticker_path).expect("remove only the conflicting temporary destination");
    let resumed = super::shard_market_database(&source, &market_dir, &app, ":memory:")
        .expect("resume at the next ticker and validate all outputs");
    assert!(resumed.starts_with("complete;"), "{resumed}");
    assert!(
        !Path::new(&source).exists(),
        "source retires after resumed validation"
    );
    clean_fixture(&source, &app, &market_dir);
}

#[test]
fn source_changes_conflict_with_prior_ticker_checkpoints() {
    let (source, app, market_dir) = create_shared_fixture("ticker-shard-source-change");
    let conflicting_path = super::ticker_database_path(Path::new(&market_dir), "btc-usd")
        .expect("resolve second ticker path");
    std::fs::create_dir_all(&market_dir).expect("create ticker directory");
    let conflicting = Connection::open(&conflicting_path).expect("create conflicting destination");
    conflicting
        .execute_batch("CREATE TABLE untracked_destination_data(value TEXT)")
        .expect("seed unexpected destination");
    drop(conflicting);
    super::shard_market_database(&source, &market_dir, &app, ":memory:")
        .expect_err("second ticker conflict should leave the first checkpoint");
    std::fs::remove_file(conflicting_path).expect("remove temporary conflict");

    let source_connection = Connection::open(&source).expect("open retained shared source");
    source_connection
        .execute(
            "UPDATE bars SET close=close+1 WHERE symbol='BTC-USD' AND interval='1h'",
            [],
        )
        .expect("change source after a ticker checkpoint");
    drop(source_connection);
    let error = super::shard_market_database(&source, &market_dir, &app, ":memory:")
        .expect_err("a changed source must not reuse the stale checkpoint");
    assert!(error.contains("checkpoint"), "{error}");
    assert!(
        Path::new(&source).is_file(),
        "source remains after checkpoint conflict"
    );
    clean_fixture(&source, &app, &market_dir);
}

#[test]
fn missing_ticker_read_does_not_create_a_database() {
    let (source, app, market_dir) = create_shared_fixture("ticker-shard-read-missing");
    super::shard_market_database(&source, &market_dir, &app, ":memory:")
        .expect("finish fixture migration");
    let paths = super::connections::StoragePaths::new(market_dir.clone(), app.clone(), ":memory:");
    let missing = super::connections::ticker_database_path(Path::new(&market_dir), "NO-HISTORY")
        .expect("encode missing symbol");
    assert!(
        super::connections::open_market_history_for_symbol(&paths, "NO-HISTORY", false)
            .expect("missing history is a valid empty result")
            .is_none()
    );
    assert!(
        !missing.exists(),
        "read-only miss must not create a ticker file"
    );
    clean_fixture(&source, &app, &market_dir);
}
