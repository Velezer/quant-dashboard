use super::{
    decode_storage_symbol_filename, ticker_database_path, MarketConnectionGate,
    MarketConnectionKind, MAX_OPEN_REGULAR_MARKET_DATABASES,
};
use crate::main_test_support::{remove_temporary_database, temporary_database_path};
use std::{
    path::Path,
    sync::{mpsc, Arc},
    thread,
    time::Duration,
};

#[test]
fn ticker_filenames_reversibly_encode_windows_unsafe_symbols() {
    let symbols = [
        "BBCA.JK",
        "BTC/USDT",
        "venue:BTC/USDT",
        "quote USDT",
        "CON",
        "🙂",
    ];
    for symbol in symbols {
        let path = ticker_database_path(Path::new("data/market/tickers"), symbol)
            .expect("symbol should encode to a safe ticker filename");
        let filename = path.file_name().and_then(|value| value.to_str()).unwrap();
        assert!(filename.starts_with('s'));
        assert!(filename.ends_with(".sqlite3"));
        assert!(filename
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'.'));
        assert_eq!(
            decode_storage_symbol_filename(&path).as_deref(),
            Some(symbol)
        );
    }
}

#[test]
fn ticker_market_connections_do_not_open_the_signal_database() {
    let app_path = temporary_database_path("ticker-market-app-only");
    let market_directory = format!("{app_path}.tickers");
    let signal_path = format!("{app_path}.signals.sqlite3");
    let paths = super::connections::StoragePaths::new(
        market_directory.as_str(),
        app_path.as_str(),
        signal_path.as_str(),
    );
    let connection = super::connections::open_market_history_for_symbol(
        &paths,
        "LOCK-TEST-USD",
        true,
    )
    .expect("open market history without the signal store")
    .expect("create the market history database");
    let signal_schema_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM pragma_database_list WHERE name='signals'",
            [],
            |row| row.get(0),
        )
        .expect("inspect attached database schemas");

    assert_eq!(signal_schema_count, 0);
    assert!(!std::path::Path::new(&signal_path).exists());
    drop(connection);
    std::fs::remove_dir_all(&market_directory).expect("remove temporary ticker databases");
    remove_temporary_database(&app_path);
}

#[test]
fn empty_markerless_ticker_files_recover_but_files_with_data_are_refused() {
    let app_path = temporary_database_path("ticker-layout-recovery");
    let market_directory = format!("{app_path}.tickers");
    let signal_path = format!("{app_path}.signals.sqlite3");
    let symbol = "EMPTY-LAYOUT-USD";
    std::fs::create_dir_all(&market_directory).expect("create temporary ticker directory");
    let empty_ticker_path = ticker_database_path(Path::new(&market_directory), symbol)
        .expect("build ticker path");
    std::fs::File::create(&empty_ticker_path).expect("leave an interrupted empty database file");
    let paths = super::connections::StoragePaths::new(
        market_directory.as_str(),
        app_path.as_str(),
        signal_path.as_str(),
    );

    let connection = super::connections::open_market_history_for_symbol(&paths, symbol, false)
        .expect("empty markerless files are safe to initialize")
        .expect("existing ticker file remains available");
    let stored_symbol: String = connection
        .query_row(
            "SELECT value FROM main.storage_layout_meta WHERE key='storage_symbol'",
            [],
            |row| row.get(0),
        )
        .expect("write the ticker identity marker");
    let inventory_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM app.market_store_inventory WHERE storage_symbol=?1",
            [symbol],
            |row| row.get(0),
        )
        .expect("record the initialized ticker in the app inventory");
    assert_eq!(stored_symbol, symbol);
    assert_eq!(inventory_count, 1);
    drop(connection);

    let data_symbol = "UNMARKED-DATA-USD";
    let data_ticker_path = ticker_database_path(Path::new(&market_directory), data_symbol)
        .expect("build markerless data path");
    let legacy = rusqlite::Connection::open(&data_ticker_path).expect("create markerless fixture");
    legacy
        .execute_batch("CREATE TABLE legacy_bars(close REAL); INSERT INTO legacy_bars VALUES(42);")
        .expect("write data that must be preserved");
    drop(legacy);
    let error = match super::connections::open_market_history_for_symbol(&paths, data_symbol, true)
    {
        Err(error) => error,
        Ok(_) => panic!("markerless files with data must not be auto-migrated"),
    };
    assert!(error
        .to_string()
        .contains("user tables but no completed layout marker"));
    let preserved = rusqlite::Connection::open(&data_ticker_path).expect("reopen preserved data");
    let close: f64 = preserved
        .query_row("SELECT close FROM legacy_bars", [], |row| row.get(0))
        .expect("retain the existing ticker value");
    assert_eq!(close, 42.0);
    drop(preserved);

    std::fs::remove_dir_all(&market_directory).expect("remove temporary ticker databases");
    remove_temporary_database(&app_path);
}

#[test]
fn sqlite_busy_writes_retry_a_few_times_then_return_the_result() {
    let mut attempts = 0;
    let value = super::retry_sqlite_busy(|| {
        attempts += 1;
        if attempts < 3 {
            Err(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_BUSY),
                None,
            ))
        } else {
            Ok("saved")
        }
    })
    .expect("transient busy errors should be retried");

    assert_eq!(value, "saved");
    assert_eq!(attempts, 3);
}

#[test]
fn market_connection_gate_caps_open_ticker_databases() {
    let gate = Arc::new(MarketConnectionGate::new());
    for _ in 0..MAX_OPEN_REGULAR_MARKET_DATABASES {
        gate.acquire(MarketConnectionKind::Regular);
    }

    let (started_sender, started_receiver) = mpsc::channel();
    let (opened_sender, opened_receiver) = mpsc::channel();
    let worker_gate = Arc::clone(&gate);
    let worker = thread::spawn(move || {
        started_sender.send(()).unwrap();
        worker_gate.acquire(MarketConnectionKind::Regular);
        opened_sender.send(()).unwrap();
        worker_gate.release(MarketConnectionKind::Regular);
    });
    started_receiver
        .recv_timeout(Duration::from_secs(1))
        .expect("fifth connection attempt should start");
    assert!(opened_receiver
        .recv_timeout(Duration::from_millis(50))
        .is_err());

    gate.release(MarketConnectionKind::Regular);
    opened_receiver
        .recv_timeout(Duration::from_secs(1))
        .expect("a released connection slot should admit the waiting ticker");
    worker
        .join()
        .expect("waiting connection thread should finish");
    for _ in 1..MAX_OPEN_REGULAR_MARKET_DATABASES {
        gate.release(MarketConnectionKind::Regular);
    }
    assert_eq!(gate.active_counts(), (0, 0));
}

#[test]
fn signal_scan_has_a_reserved_database_slot_when_sync_is_saturated() {
    let gate = Arc::new(MarketConnectionGate::new());
    for _ in 0..MAX_OPEN_REGULAR_MARKET_DATABASES {
        gate.acquire(MarketConnectionKind::Regular);
    }

    gate.acquire(MarketConnectionKind::SignalScan);
    assert_eq!(gate.active_counts(), (MAX_OPEN_REGULAR_MARKET_DATABASES, 1));

    let (started_sender, started_receiver) = mpsc::channel();
    let (opened_sender, opened_receiver) = mpsc::channel();
    let worker_gate = Arc::clone(&gate);
    let worker = thread::spawn(move || {
        started_sender.send(()).unwrap();
        worker_gate.acquire(MarketConnectionKind::Regular);
        opened_sender.send(()).unwrap();
        worker_gate.release(MarketConnectionKind::Regular);
    });
    started_receiver
        .recv_timeout(Duration::from_secs(1))
        .expect("regular waiter should start");
    assert!(opened_receiver
        .recv_timeout(Duration::from_millis(50))
        .is_err());

    gate.release(MarketConnectionKind::SignalScan);
    assert!(opened_receiver
        .recv_timeout(Duration::from_millis(50))
        .is_err());
    gate.release(MarketConnectionKind::Regular);
    opened_receiver
        .recv_timeout(Duration::from_secs(1))
        .expect("released regular slot should admit the waiting ticker");
    worker
        .join()
        .expect("waiting connection thread should finish");
    for _ in 1..MAX_OPEN_REGULAR_MARKET_DATABASES {
        gate.release(MarketConnectionKind::Regular);
    }
    assert_eq!(gate.active_counts(), (0, 0));
}
