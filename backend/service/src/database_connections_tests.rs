use super::{
    decode_storage_symbol_filename, ticker_database_path, MarketConnectionGate,
    MarketConnectionKind, MAX_OPEN_REGULAR_MARKET_DATABASES,
};
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
