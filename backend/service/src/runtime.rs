use super::*;

const PINE_BACKTEST_RESULT_TTL_SECONDS: u64 = 30 * 60;
const MAX_PINE_BACKTEST_RESULT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone)]
pub(super) struct StaleSignalSyncPriority {
    pub(super) symbol: String,
    pub(super) exchange_id: String,
    pub(super) product_id: String,
}

pub(super) struct PineBacktestJobs {
    current: Mutex<Option<Arc<PineBacktestJob>>>,
    next_id: AtomicU64,
}

pub(super) struct PineBacktestJob {
    pub(super) id: String,
    state: Mutex<PineBacktestJobState>,
    progress_basis_points: AtomicU64,
    cancel_requested: AtomicBool,
    created_at: u64,
}

struct PineBacktestJobState {
    state: &'static str,
    phase: String,
    result: Option<Value>,
    error: Option<String>,
    finished_at: Option<u64>,
}

impl Default for PineBacktestJobs {
    fn default() -> Self {
        Self {
            current: Mutex::new(None),
            next_id: AtomicU64::new(1),
        }
    }
}

impl PineBacktestJobs {
    pub(super) fn start(&self) -> Result<Arc<PineBacktestJob>, &'static str> {
        let mut current = self
            .current
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(job) = current.as_ref() {
            if job.is_active() {
                return Err("A Pine backtest is already running. Wait for it to finish or cancel it before starting another.");
            }
        }
        let sequence = self.next_id.fetch_add(1, Ordering::Relaxed);
        let created_at = now_secs();
        let job = Arc::new(PineBacktestJob {
            id: format!("{created_at:x}-{sequence:x}"),
            state: Mutex::new(PineBacktestJobState {
                state: "queued",
                phase: "queued".to_string(),
                result: None,
                error: None,
                finished_at: None,
            }),
            progress_basis_points: AtomicU64::new(0),
            cancel_requested: AtomicBool::new(false),
            created_at,
        });
        *current = Some(Arc::clone(&job));
        Ok(job)
    }

    pub(super) fn get(&self, id: &str) -> Option<Arc<PineBacktestJob>> {
        let mut current = self
            .current
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if current.as_ref().is_some_and(|job| {
            job.id == id
                && job.finished_at().is_some_and(|finished| {
                    now_secs().saturating_sub(finished) >= PINE_BACKTEST_RESULT_TTL_SECONDS
                })
        }) {
            *current = None;
            return None;
        }
        current.as_ref().filter(|job| job.id == id).cloned()
    }
}

impl PineBacktestJob {
    pub(super) fn set_running(&self) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if state.state == "queued" {
            state.state = "running";
            state.phase = "loading history".to_string();
        }
    }

    pub(super) fn set_progress(
        &self,
        completed: usize,
        total: usize,
        phase: &str,
        start: u64,
        span: u64,
    ) -> bool {
        if self.cancel_requested.load(Ordering::Relaxed) {
            return false;
        }
        let fraction = if total == 0 {
            1.0
        } else {
            completed as f64 / total as f64
        };
        let value = start
            .saturating_add((fraction.clamp(0.0, 1.0) * span as f64) as u64)
            .min(10_000);
        self.progress_basis_points
            .fetch_max(value, Ordering::Relaxed);
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if state.state == "running" {
            state.phase = phase.to_string();
        }
        true
    }

    pub(super) fn set_phase(&self, phase: &str) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if state.state == "queued" || state.state == "running" {
            state.state = "running";
            state.phase = phase.to_string();
        }
    }

    pub(super) fn request_cancel(&self) -> &'static str {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        match state.state {
            "queued" | "running" => {
                self.cancel_requested.store(true, Ordering::Relaxed);
                state.state = "cancelling";
                state.phase = "cancellation requested".to_string();
                "cancelling"
            }
            "cancelling" => "cancelling",
            finished => finished,
        }
    }

    pub(super) fn is_cancel_requested(&self) -> bool {
        self.cancel_requested.load(Ordering::Relaxed)
    }

    pub(super) fn complete(&self, result: Value) -> Result<(), String> {
        let bytes = serde_json::to_vec(&result).map_err(|error| error.to_string())?;
        if bytes.len() > MAX_PINE_BACKTEST_RESULT_BYTES {
            self.fail(format!("Backtest result exceeded the {} MB response bound. Reduce history or strategy plots and try again.", MAX_PINE_BACKTEST_RESULT_BYTES / (1024 * 1024)));
            return Err("Backtest result exceeded the bounded response size.".to_string());
        }
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if self.cancel_requested.load(Ordering::Relaxed) {
            state.state = "cancelled";
            state.phase = "cancelled".to_string();
            state.result = None;
            state.error = None;
            state.finished_at = Some(now_secs());
            return Err("Backtest job was cancelled before result delivery.".to_string());
        }
        state.state = "completed";
        state.phase = "complete".to_string();
        state.result = Some(result);
        state.error = None;
        state.finished_at = Some(now_secs());
        self.progress_basis_points.store(10_000, Ordering::Relaxed);
        Ok(())
    }

    pub(super) fn fail(&self, message: String) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.state = if self.is_cancel_requested() {
            "cancelled"
        } else {
            "failed"
        };
        state.phase = state.state.to_string();
        state.error = (!self.is_cancel_requested()).then_some(message);
        state.finished_at = Some(now_secs());
    }

    pub(super) fn cancelled(&self) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.state = "cancelled";
        state.phase = "cancelled".to_string();
        state.finished_at = Some(now_secs());
    }

    pub(super) fn response(&self) -> Value {
        let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        serde_json::json!({
            "job_id": self.id,
            "state": state.state,
            "phase": state.phase,
            "progress_pct": self.progress_basis_points.load(Ordering::Relaxed) as f64 / 100.0,
            "created_at": self.created_at,
            "result": state.result,
            "error": state.error
        })
    }

    fn is_active(&self) -> bool {
        matches!(
            self.state
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .state,
            "queued" | "running" | "cancelling"
        )
    }

    fn finished_at(&self) -> Option<u64> {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .finished_at
    }
}

/// Shared application state — passed as Arc into every handler and background thread.
pub(super) struct AppState {
    pub(super) market_database: String,
    pub(super) pine_backtest_jobs: PineBacktestJobs,
    /// Scan interval in seconds; the control route accepts only the fixed 900-second cycle.
    pub(super) scan_interval_sec: AtomicU64,
    /// Unix-second timestamp of last POST /api/heartbeat
    pub(super) last_heartbeat: AtomicI64,
    pub(super) is_scanning: AtomicBool,
    /// Manual scan trigger: (Mutex<bool>, Condvar)
    pub(super) scan_trigger: (Mutex<bool>, Condvar),
    /// SSE broadcast channel — Vec of SyncSenders, each connected to one client thread.
    pub(super) sse_clients: Mutex<Vec<std::sync::mpsc::SyncSender<String>>>,
    pub(super) last_scan_time: Mutex<String>,
    pub(super) next_scan_time: Mutex<String>,
    /// Unix-second timestamp for the next scheduled scan. The string form is
    /// kept for diagnostics, while this numeric value lets clients resume a
    /// countdown after a page refresh without restarting the interval.
    pub(super) next_scan_epoch: AtomicU64,
    pub(super) current_scan_symbol: Mutex<String>,
    pub(super) scan_progress_pct: Mutex<f64>,
    /// Most recently viewed chart symbol gets first place in the next bounded
    /// hourly history-sync batch.
    pub(super) chart_sync_priority: Mutex<Option<String>>,
    /// A signal scan can promote one stale symbol and request a scan after its
    /// next successful refresh. Chart navigation remains an independent hint.
    pub(super) stale_intraday_sync_priority: Mutex<Option<StaleSignalSyncPriority>>,
    pub(super) stale_daily_sync_priority: Mutex<Option<StaleSignalSyncPriority>>,
    /// Unix-second epoch of the last daily registry sync. Zero means never run.
    pub(super) last_registry_sync_epoch: AtomicU64,
}

impl AppState {
    pub(super) fn new(market_database: String, scan_interval_sec: u64) -> Arc<Self> {
        Arc::new(Self {
            market_database,
            pine_backtest_jobs: PineBacktestJobs::default(),
            scan_interval_sec: AtomicU64::new(scan_interval_sec),
            last_heartbeat: AtomicI64::new(0),
            is_scanning: AtomicBool::new(false),
            scan_trigger: (Mutex::new(false), Condvar::new()),
            sse_clients: Mutex::new(Vec::new()),
            last_scan_time: Mutex::new(String::new()),
            next_scan_time: Mutex::new(String::new()),
            next_scan_epoch: AtomicU64::new(now_secs().saturating_add(scan_interval_sec)),
            current_scan_symbol: Mutex::new(String::new()),
            scan_progress_pct: Mutex::new(0.0),
            chart_sync_priority: Mutex::new(None),
            stale_intraday_sync_priority: Mutex::new(None),
            stale_daily_sync_priority: Mutex::new(None),
            last_registry_sync_epoch: AtomicU64::new(0),
        })
    }

    /// Broadcast an SSE event to all connected clients; dead connections are pruned.
    pub(super) fn broadcast(&self, event: &str, data: &Value) {
        let Ok(data) = wire::encode(data) else {
            return;
        };
        let msg = format!("event: {event}\ndata: {data}\n\n");
        let mut clients = self.sse_clients.lock().unwrap_or_else(|e| e.into_inner());
        clients.retain(|tx| tx.try_send(msg.clone()).is_ok());
    }

    pub(super) fn prioritize_chart_sync(&self, symbol: &str) {
        let normalized = symbol.trim().to_uppercase();
        if normalized.is_empty() {
            return;
        }
        *self
            .chart_sync_priority
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Some(normalized);
    }

    pub(super) fn take_chart_sync_priority(&self) -> Option<String> {
        self.chart_sync_priority
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take()
    }

    pub(super) fn prioritize_stale_signal_sync(
        &self,
        symbol: &str,
        exchange_id: &str,
        product_id: &str,
        timeframe: &str,
    ) -> bool {
        let normalized = symbol.trim().to_uppercase();
        if normalized.is_empty() || exchange_id.is_empty() || product_id.is_empty() {
            return false;
        }
        let priority = match timeframe {
            "15m" => &self.stale_intraday_sync_priority,
            "1d" => &self.stale_daily_sync_priority,
            _ => return false,
        };
        let mut priority = priority.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(queued) = priority.as_ref() {
            return queued.symbol == normalized
                && queued.exchange_id == exchange_id
                && queued.product_id == product_id;
        }
        *priority = Some(StaleSignalSyncPriority {
            symbol: normalized,
            exchange_id: exchange_id.to_string(),
            product_id: product_id.to_string(),
        });
        true
    }

    pub(super) fn take_stale_signal_sync_priority(
        &self,
        timeframe: &str,
    ) -> Option<StaleSignalSyncPriority> {
        let priority = match timeframe {
            "15m" => &self.stale_intraday_sync_priority,
            "1d" => &self.stale_daily_sync_priority,
            _ => return None,
        };
        priority
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take()
    }

    pub(super) fn update_scan_progress(&self, symbol: &str, completed: usize, total: usize) {
        *self
            .current_scan_symbol
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = symbol.to_string();
        let progress = if total == 0 {
            0.0
        } else {
            (completed.min(total) as f64 / total as f64 * 100.0).clamp(0.0, 100.0)
        };
        *self
            .scan_progress_pct
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = progress;
    }

    pub(super) fn reset_scan_progress(&self) {
        self.update_scan_progress("", 0, 1);
    }

    pub(super) fn complete_scan_progress(&self) {
        self.update_scan_progress("", 1, 1);
    }

    pub(super) fn fail_scan_progress(&self) {
        self.reset_scan_progress();
    }

    pub(super) fn request_scan(&self) {
        let (lock, cvar) = &self.scan_trigger;
        *lock.lock().unwrap_or_else(|error| error.into_inner()) = true;
        cvar.notify_one();
    }
}

// ─── SSE connection handler (runs on its own thread) ──────────────────────────

pub(super) fn handle_sse(mut stream: TcpStream, state: Arc<AppState>) {
    let (tx, rx) = std::sync::mpsc::sync_channel::<String>(64);
    {
        let mut clients = state.sse_clients.lock().unwrap_or_else(|e| e.into_inner());
        clients.push(tx);
    }

    let last_scan = state
        .last_scan_time
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let next_scan = state
        .next_scan_time
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let next_scan_epoch = state.next_scan_epoch.load(Ordering::Relaxed);
    let current_sym = state
        .current_scan_symbol
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let pct = *state
        .scan_progress_pct
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let is_scanning = state.is_scanning.load(Ordering::Relaxed);
    let secs = now_secs();
    let server_time = format_timestamp(secs);

    let init_data = serde_json::json!({
        "status": "connected",
        "server_time": server_time,
        "is_scanning": is_scanning,
        "current_scan_symbol": current_sym,
        "scan_progress_pct": pct,
        "last_scan_time": last_scan,
        "next_scan_time": next_scan,
        "next_scan_epoch": next_scan_epoch,
    });
    let header = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream; charset=utf-8\r\nCache-Control: no-cache, no-transform\r\nConnection: keep-alive\r\nAccess-Control-Allow-Origin: *\r\n\r\n";
    if stream.write_all(header.as_bytes()).is_err() {
        return;
    }
    let init_payload = wire::encode(&init_data).unwrap_or_default();
    let init_msg = format!("event: connected\ndata: {init_payload}\n\n");
    if stream.write_all(init_msg.as_bytes()).is_err() {
        return;
    }
    let _ = stream.flush();

    loop {
        match rx.recv_timeout(Duration::from_secs(15)) {
            Ok(msg) => {
                if stream.write_all(msg.as_bytes()).is_err() {
                    break;
                }
                let _ = stream.flush();
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                if stream.write_all(b": keepalive\n\n").is_err() {
                    break;
                }
                let _ = stream.flush();
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}

// ─── Background scan scheduler ─────────────────────────────────────────────────

const FAILED_SCAN_RETRY_DELAY_SECS: u64 = 60;

fn next_scan_epoch_after_completion(
    scan_started_epoch: u64,
    completed_epoch: u64,
    interval_secs: u64,
    succeeded: bool,
) -> u64 {
    if !succeeded {
        return completed_epoch.saturating_add(FAILED_SCAN_RETRY_DELAY_SECS);
    }

    scan_started_epoch.saturating_add(interval_secs)
}

fn publish_next_scan_time(state: &AppState, next_epoch: u64) {
    state.next_scan_epoch.store(next_epoch, Ordering::Relaxed);
    *state
        .next_scan_time
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = format_timestamp(next_epoch);
}

// ─── Daily instrument-registry sync ────────────────────────────────────────────

pub(super) const REGISTRY_SYNC_INTERVAL_SECS: u64 = quant_core::SECONDS_PER_DAY as u64; // 24 h

/// Background daemon that refreshes the instrument registry once per day for
/// every enabled exchange. The first run happens shortly after startup (after a
/// 30-second grace period so the main scan-daemon can initialize first).
///
/// Each exchange is synced independently. Failures for one exchange are logged
/// but do not block the others. Results are broadcast as a `registry_synced`
/// SSE event so the dashboard can refresh the Ticker Registry panel without a
/// page reload.
pub(super) fn background_registry_sync_loop(state: Arc<AppState>) {
    eprintln!("[Registry] Daily registry sync daemon started.");
    // Give the scan daemon a short head-start on first startup.
    thread::sleep(Duration::from_secs(30));

    loop {
        let mut sleep_seconds = 3600u64;
        let now = now_secs();
        let last = state.last_registry_sync_epoch.load(Ordering::Relaxed);
        let elapsed = now.saturating_sub(last);

        if last == 0 || elapsed >= REGISTRY_SYNC_INTERVAL_SECS {
            eprintln!("[Registry] Running daily instrument-registry sync...");
            match open_app_db_for_market_directory(&state.market_database) {
                Ok(conn) => {
                    let results = instrument_registry::sync_from_live_sources(&conn);
                    let mut exchanges = serde_json::json!([]);
                    let mut total_inserted = 0usize;
                    let mut total_deactivated = 0usize;
                    let mut any_error = false;
                    if let Some(arr) = exchanges.as_array_mut() {
                        for r in &results {
                            if let Some(ref err) = r.error {
                                eprintln!("[Registry] Sync error for {}: {}", r.exchange_id, err);
                                any_error = true;
                            } else {
                                eprintln!(
                                    "[Registry] {} — {} inserted, {} delisted",
                                    r.exchange_id, r.inserted, r.deactivated
                                );
                            }
                            total_inserted += r.inserted;
                            total_deactivated += r.deactivated;
                            arr.push(serde_json::json!({
                                "exchange_id": r.exchange_id,
                                "inserted": r.inserted,
                                "deactivated": r.deactivated,
                                "error": r.error,
                            }));
                        }
                    }
                    state.last_registry_sync_epoch.store(
                        if any_error {
                            sleep_seconds = 15 * quant_core::SECONDS_PER_MINUTE as u64;
                            now_secs().saturating_sub(REGISTRY_SYNC_INTERVAL_SECS - sleep_seconds)
                        } else {
                            now_secs()
                        },
                        Ordering::Relaxed,
                    );
                    let event = serde_json::json!({
                        "synced_at": format_timestamp(now_secs()),
                        "total_inserted": total_inserted,
                        "total_deactivated": total_deactivated,
                        "had_errors": any_error,
                        "exchanges": exchanges,
                    });
                    state.broadcast("registry_synced", &event);
                    let _ = conn.execute_batch("PRAGMA shrink_memory;");
                    eprintln!("[Registry] Daily sync complete.");
                }
                Err(e) => {
                    eprintln!("[Registry] Cannot open DB for registry sync: {e}");
                    sleep_seconds = 15 * quant_core::SECONDS_PER_MINUTE as u64;
                }
            }
        }

        // Failed refreshes retry after 15 minutes; successful full listings
        // retain the daily cadence.
        thread::sleep(Duration::from_secs(sleep_seconds));
    }
}

pub(super) fn background_scan_loop(state: Arc<AppState>) {
    eprintln!("[Daemon] Event-driven background scanner initialized.");
    thread::sleep(Duration::from_secs(1));

    // Load the latest successful screener scan timestamp from SQLite.
    if let Ok(conn) = open_app_db_for_market_directory(&state.market_database) {
        if let Ok(latest_ts) = conn.query_row(
            "SELECT last_success_at FROM signals.signal_screener_status WHERE id=1",
            [],
            |r| r.get::<_, Option<i64>>(0),
        ) {
            if let Some(ts) = latest_ts {
                let gen_at = format_timestamp(ts as u64);
                *state
                    .last_scan_time
                    .lock()
                    .unwrap_or_else(|e| e.into_inner()) = gen_at;
            }
        }
    }

    eprintln!("[Daemon] Scheduled signal scans are independent of dashboard heartbeats.");
    let mut next_scan_epoch = now_secs();

    loop {
        let interval_sec = state.scan_interval_sec.load(Ordering::Relaxed).max(1);
        publish_next_scan_time(&state, next_scan_epoch);
        let wait = next_scan_epoch.saturating_sub(now_secs()).max(1);

        // Wait up to `wait` seconds for a manual trigger
        let (lock, cvar) = &state.scan_trigger;
        let triggered = {
            let mut guard = lock.lock().unwrap_or_else(|e| e.into_inner());
            if *guard {
                *guard = false;
                true
            } else {
                let (mut guard, wait_result) = cvar
                    .wait_timeout(guard, Duration::from_secs(wait))
                    .unwrap_or_else(|e| e.into_inner());
                let was_triggered = *guard;
                *guard = false;
                if !was_triggered && !wait_result.timed_out() && now_secs() < next_scan_epoch {
                    continue;
                }
                was_triggered
            }
        };

        let trigger_mode = if triggered {
            "manual_event"
        } else {
            "scheduled_interval"
        };
        let scan_started_epoch = now_secs();
        next_scan_epoch = scan_started_epoch.saturating_add(interval_sec);
        publish_next_scan_time(&state, next_scan_epoch);
        eprintln!("\n[Daemon] Scan event received ({trigger_mode}). Running native Rust scan...");

        state.is_scanning.store(true, Ordering::Relaxed);
        state.reset_scan_progress();

        let started_data = serde_json::json!({
            "timestamp": format_timestamp(scan_started_epoch),
            "triggered_by": trigger_mode,
        });
        state.broadcast("scan_started", &started_data);

        let conn = open_app_db_for_market_directory(&state.market_database);
        match conn {
            Ok(connection) => match run_rust_scan_with_state(
                &connection,
                &state.market_database,
                state.scan_interval_sec.load(Ordering::Relaxed) / 60,
                &state,
            ) {
                Ok(payload) => {
                    let completed_epoch = now_secs();
                    let now_str = format_timestamp(completed_epoch);
                    next_scan_epoch = next_scan_epoch_after_completion(
                        scan_started_epoch,
                        completed_epoch,
                        interval_sec,
                        true,
                    );
                    publish_next_scan_time(&state, next_scan_epoch);
                    let next_str2 = format_timestamp(next_scan_epoch);
                    *state
                        .last_scan_time
                        .lock()
                        .unwrap_or_else(|e| e.into_inner()) = now_str.clone();
                    *state
                        .next_scan_time
                        .lock()
                        .unwrap_or_else(|e| e.into_inner()) = next_str2.clone();
                    state.is_scanning.store(false, Ordering::Relaxed);
                    state.complete_scan_progress();

                    let signal_count = payload["summary"]["total_signals"].as_u64().unwrap_or(0);
                    let summary = payload
                        .get("summary")
                        .cloned()
                        .unwrap_or(serde_json::Value::Null);

                    let completed_data = serde_json::json!({
                        "generated_at": now_str,
                        "next_scan_time": next_str2,
                        "next_scan_epoch": next_scan_epoch,
                        "signal_count": signal_count,
                        "summary": summary,
                        "new_signal_alerts": payload.get("new_signal_alerts").cloned().unwrap_or_else(|| serde_json::json!([])),
                        "signal_alert_status": payload.get("signal_alert_status").cloned().unwrap_or_else(|| serde_json::json!("ready")),
                        "sync": serde_json::Value::Null,
                    });
                    state.broadcast("scan_completed", &completed_data);
                    let _ = connection.execute_batch("PRAGMA shrink_memory;");
                    eprintln!("[Daemon] Scan cycle completed. Next run at {next_str2}.\n");
                }
                Err(e) => {
                    eprintln!("[Daemon] Scan cycle error: {e}");
                    if let Err(storage_error) =
                        signal_alerts::record_signal_screener_failure(&connection, &e.to_string())
                    {
                        eprintln!("[Daemon] Could not record screener error: {storage_error}");
                    }
                    let completed_epoch = now_secs();
                    next_scan_epoch = next_scan_epoch_after_completion(
                        scan_started_epoch,
                        completed_epoch,
                        interval_sec,
                        false,
                    );
                    publish_next_scan_time(&state, next_scan_epoch);
                    state.is_scanning.store(false, Ordering::Relaxed);
                    state.fail_scan_progress();
                    let failed_data = serde_json::json!({
                        "error": e.to_string(),
                        "state": "stale",
                        "next_scan_time": format_timestamp(next_scan_epoch),
                        "next_scan_epoch": next_scan_epoch,
                    });
                    state.broadcast("scan_failed", &failed_data);
                    eprintln!(
                        "[Daemon] Retrying failed scan at {}.",
                        format_timestamp(next_scan_epoch)
                    );
                }
            },
            Err(e) => {
                eprintln!("[Daemon] Cannot open DB: {e}");
                let completed_epoch = now_secs();
                next_scan_epoch = next_scan_epoch_after_completion(
                    scan_started_epoch,
                    completed_epoch,
                    interval_sec,
                    false,
                );
                publish_next_scan_time(&state, next_scan_epoch);
                state.is_scanning.store(false, Ordering::Relaxed);
                state.fail_scan_progress();
                let failed_data = serde_json::json!({
                    "error": e.to_string(),
                    "next_scan_time": format_timestamp(next_scan_epoch),
                    "next_scan_epoch": next_scan_epoch,
                });
                state.broadcast("scan_failed", &failed_data);
                eprintln!(
                    "[Daemon] Retrying failed scan at {}.",
                    format_timestamp(next_scan_epoch)
                );
            }
        }
    }
}

// ─── Backtest ticker detail ────────────────────────────────────────────────────

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;
