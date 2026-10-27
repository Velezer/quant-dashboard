use super::*;

/// The market-data scheduler runs independently of dashboard heartbeats. It
/// sizes intraday batches to sweep the registered universe within twelve
/// one-minute ticks, with spare time for slow providers. Daily history has a
/// separate bounded queue paced to finish the registered universe within a
/// UTC day without slowing each intraday tick.
pub(super) fn background_market_sync_loop(state: Arc<AppState>) {
    if !provider_sync_enabled() {
        eprintln!("[Daemon] Provider sync is disabled until a compatible adapter is available.");
        return;
    }
    eprintln!(
        "[Daemon] Background market sync scheduler initialized (15m intraday sweep plus daily registered-candle batches)."
    );
    thread::sleep(Duration::from_secs(1));
    let mut last_sync_epoch = now_secs().saturating_sub(MARKET_SYNC_TICK_SECONDS);
    let mut last_daily_sync_epoch = 0;
    let daily_sync_in_progress = Arc::new(AtomicBool::new(false));
    let storage_symbol_gate = Arc::new(targets::StorageSymbolSyncGate::default());

    loop {
        let elapsed = now_secs().saturating_sub(last_sync_epoch);
        if elapsed < MARKET_SYNC_TICK_SECONDS {
            thread::sleep(Duration::from_secs(
                (MARKET_SYNC_TICK_SECONDS - elapsed).min(30),
            ));
            continue;
        }

        let started = now_secs();
        eprintln!("[Daemon] Scheduled market sync started.");
        match open_app_db_for_market_directory(&state.market_database) {
            Ok(connection) => {
                let stale_daily_priority = state.take_stale_signal_sync_priority("1d");
                let priority_symbol = state.take_chart_sync_priority();
                match sync_hourly_chart_targets(
                    &connection,
                    &state.market_database,
                    priority_symbol.as_deref(),
                    Some(state.as_ref()),
                    storage_symbol_gate.as_ref(),
                ) {
                    Ok(refreshed) => {
                        last_sync_epoch = started;
                        let _ = connection.execute_batch("PRAGMA shrink_memory;");
                        let batch_duration_seconds = now_secs().saturating_sub(started);
                        eprintln!(
                            "[Daemon] Scheduled 15m market sync batch finished: {refreshed} interval series refreshed in {}s.",
                            batch_duration_seconds
                        );
                    }
                    Err(error) => {
                        last_sync_epoch = started;
                        eprintln!("[Daemon] Scheduled market sync failed: {error}");
                    }
                }
                if daily_candle_sync_due(started, last_daily_sync_epoch) {
                    last_daily_sync_epoch = started;
                    if daily_sync_in_progress
                        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                        .is_ok()
                    {
                        let daily_sync_worker_flag = Arc::clone(&daily_sync_in_progress);
                        let storage_symbol_gate = Arc::clone(&storage_symbol_gate);
                        let market_database = state.market_database.clone();
                        let stale_daily_priority = stale_daily_priority.clone();
                        let result = thread::Builder::new()
                            .name("quant-daily-candle-sync".to_string())
                            .spawn(move || {
                                let _worker_guard = DailyCandleSyncWorkerGuard {
                                    running: daily_sync_worker_flag,
                                };
                                let connection = match open_app_db_for_market_directory(
                                    &market_database,
                                ) {
                                    Ok(connection) => connection,
                                    Err(error) => {
                                        eprintln!(
                                            "[Daemon] Daily candle worker cannot open app database: {error}"
                                        );
                                        return;
                                    }
                                };
                                match sync_daily_chart_targets(
                                    &connection,
                                    &market_database,
                                    stale_daily_priority.as_ref(),
                                    storage_symbol_gate.as_ref(),
                                ) {
                                    Ok(attempted) if attempted > 0 => eprintln!(
                                        "[Daemon] Daily candle worker attempted {attempted} registered instruments."
                                    ),
                                    Ok(_) => {}
                                    Err(error) => eprintln!(
                                        "[Daemon] Daily candle worker failed: {error}"
                                    ),
                                }
                                let _ = connection.execute_batch("PRAGMA shrink_memory;");
                            });
                        if let Err(error) = result {
                            daily_sync_in_progress.store(false, Ordering::Release);
                            eprintln!("[Daemon] Daily candle worker could not start: {error}");
                        }
                    } else {
                        eprintln!(
                            "[Daemon] Daily candle worker is still draining its bounded batch."
                        );
                    }
                }
            }
            Err(error) => {
                last_sync_epoch = started;
                eprintln!("[Daemon] Scheduled market sync cannot open DB: {error}");
            }
        }
    }
}

struct DailyCandleSyncWorkerGuard {
    running: Arc<AtomicBool>,
}

impl Drop for DailyCandleSyncWorkerGuard {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Release);
    }
}
