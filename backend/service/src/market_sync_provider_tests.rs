use super::targets::StorageSymbolSyncGate;
use super::{
    candle_sync_target_limit, candle_sync_worker_count, daily_candle_sync_due,
    should_continue_candle_provider_chain, CandleSourceClass,
    DAILY_CANDLE_SYNC_MINIMUM_INTERVAL_SECONDS, MAX_PARALLEL_CANDLE_SYNC_WORKERS,
};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Duration;

#[test]
fn registered_market_targets_fit_a_twelve_tick_sweep() {
    assert_eq!(candle_sync_target_limit(2_911), 243);
    assert_eq!(candle_sync_target_limit(9_000), 750);
    assert_eq!(candle_sync_target_limit(1), 1);
    assert_eq!(candle_sync_target_limit(0), 0);
}

#[test]
fn candle_refresh_workers_remain_bounded() {
    assert_eq!(
        candle_sync_worker_count(243),
        MAX_PARALLEL_CANDLE_SYNC_WORKERS
    );
    assert_eq!(candle_sync_worker_count(1), 1);
    assert_eq!(candle_sync_worker_count(0), 0);
}

#[test]
fn daily_candle_batches_are_paced_independently_of_intraday_ticks() {
    assert!(daily_candle_sync_due(1_000, 0));
    assert!(!daily_candle_sync_due(
        1_000 + DAILY_CANDLE_SYNC_MINIMUM_INTERVAL_SECONDS - 1,
        1_000
    ));
    assert!(daily_candle_sync_due(
        1_000 + DAILY_CANDLE_SYNC_MINIMUM_INTERVAL_SECONDS,
        1_000
    ));
}

#[test]
fn shared_storage_gate_serializes_daily_and_intraday_writes_for_one_symbol() {
    let gate = Arc::new(StorageSymbolSyncGate::default());
    let held = gate.lock("BTC-USD");
    let worker_gate = Arc::clone(&gate);
    let (started_tx, started_rx) = mpsc::channel();
    let (finished_tx, finished_rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        started_tx.send(()).unwrap();
        let _guard = worker_gate.lock("BTC-USD");
        finished_tx.send(()).unwrap();
    });

    started_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(matches!(
        finished_rx.recv_timeout(Duration::from_millis(50)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));
    drop(held);
    finished_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    worker.join().unwrap();
}

#[test]
fn native_success_stops_the_chain_but_native_failure_allows_proxy_fallback() {
    assert!(!should_continue_candle_provider_chain(
        CandleSourceClass::VenueNative,
        true
    ));
    assert!(should_continue_candle_provider_chain(
        CandleSourceClass::VenueNative,
        false
    ));
}

#[test]
fn research_proxy_chain_collects_its_sources_then_stops() {
    assert!(!should_continue_candle_provider_chain(
        CandleSourceClass::ResearchProxy,
        true
    ));
    assert!(!should_continue_candle_provider_chain(
        CandleSourceClass::ResearchProxy,
        false
    ));
}
