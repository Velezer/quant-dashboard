use super::*;

fn schedule_database() -> Connection {
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch(
            "ATTACH DATABASE ':memory:' AS app;
             CREATE TABLE app.gainer_forecast_schedule (
                 scope_id TEXT PRIMARY KEY, protocol_id TEXT NOT NULL, forecast_cutoff INTEGER NOT NULL,
                 status TEXT NOT NULL, attempt_count INTEGER NOT NULL, last_attempt_at INTEGER NOT NULL,
                 next_attempt_at INTEGER NOT NULL, last_error TEXT NOT NULL, result_json TEXT NOT NULL,
                 universe_json TEXT NOT NULL, updated_at INTEGER NOT NULL
             );
             CREATE TABLE app.gainer_forecast_runs (
                 scope_id TEXT NOT NULL,protocol_id TEXT NOT NULL,forecast_cutoff INTEGER NOT NULL,
                 computed_at INTEGER NOT NULL,status TEXT NOT NULL,result_json TEXT NOT NULL,
                 PRIMARY KEY(scope_id,protocol_id,forecast_cutoff)
             );
             CREATE TABLE app.gainer_forecast_predictions (
                 scope_id TEXT NOT NULL,protocol_id TEXT NOT NULL,forecast_cutoff INTEGER NOT NULL,
                 instrument_id TEXT NOT NULL,rank INTEGER NOT NULL,source TEXT NOT NULL,
                 reference_bar_time INTEGER NOT NULL,reference_close REAL NOT NULL,
                 predicted_return_pct REAL NOT NULL,target_bar_time INTEGER,
                 realized_return_pct REAL,created_at INTEGER NOT NULL,settled_at INTEGER,
                 PRIMARY KEY(scope_id,protocol_id,forecast_cutoff,instrument_id)
             );",
        )
        .unwrap();
    connection
}

fn schedule(scope: ForecastScope, cutoff: i64, now: i64, attempt_count: usize) -> ForecastSchedule {
    ForecastSchedule {
        scope_id: scope.id().to_owned(),
        protocol_id: scope.protocol().to_owned(),
        forecast_cutoff: cutoff,
        status: "running".to_owned(),
        attempt_count,
        last_attempt_at: now,
        next_attempt_at: 0,
        last_error: String::new(),
        result_json: "{}".to_owned(),
        universe_json: "[]".to_owned(),
    }
}

fn local_day_start(weekday: i64) -> i64 {
    let day_index = (weekday - 4).rem_euclid(7) + 70;
    day_index * SECONDS_PER_DAY - JAKARTA_OFFSET_SECONDS
}

#[test]
fn tokocrypto_scans_start_five_minutes_after_each_aligned_four_hour_close() {
    let scope = ForecastScope::TokocryptoSpot;
    let interval = 4 * 60 * 60;
    assert_eq!(scope.scan_interval_seconds(), interval);
    assert_eq!(window_cutoff(scope, interval - 1), 0);
    assert_eq!(window_cutoff(scope, interval), interval);
    assert_eq!(first_attempt_at_for_scope(scope, interval), interval + 5 * 60);
    assert_eq!(next_window_attempt_at(scope, interval), 2 * interval + 5 * 60);
}

#[test]
fn stockbit_scans_use_jakarta_daily_windows_and_skip_sunday_and_monday_duplicates() {
    let scope = ForecastScope::StockbitIdx;
    let tuesday = local_day_start(2);
    let saturday = local_day_start(6);
    let sunday = local_day_start(0);
    let monday = local_day_start(1);
    assert_eq!(window_cutoff(scope, tuesday + 600), tuesday);
    assert_eq!(window_cutoff(scope, saturday + 600), saturday);
    assert_eq!(window_cutoff(scope, sunday + 600), saturday);
    assert_eq!(window_cutoff(scope, monday + 600), saturday);
    assert_eq!(
        first_attempt_at_for_scope(scope, tuesday),
        tuesday + 14 * 60 * 60
    );
    assert_eq!(next_window_attempt_at(scope, saturday), tuesday + 14 * 60 * 60);
    assert!(!stockbit_window_passed(saturday, sunday + 8 * 60 * 60));
    assert!(!stockbit_window_passed(saturday, monday + 15 * 60 * 60));
    assert!(stockbit_window_passed(saturday, monday + 16 * 60 * 60));
    assert!(!stockbit_window_passed(tuesday, tuesday + 15 * 60 * 60));
    assert!(stockbit_window_passed(tuesday, tuesday + 16 * 60 * 60));
}

#[test]
fn scheduled_cutoff_is_converted_to_nanoseconds_for_candle_reads() {
    let cutoff_seconds = 1_800_000_000;
    assert_eq!(
        forecast_cutoff_nanoseconds(cutoff_seconds).unwrap(),
        1_800_000_000_000_000_000
    );
    assert!(forecast_cutoff_nanoseconds(i64::MAX).is_err());
}

#[test]
fn insufficient_coverage_persists_a_bounded_retry_time() {
    let connection = schedule_database();
    let scope = ForecastScope::TokocryptoSpot;
    let cutoff = 1_800_000_000;
    let now = cutoff + FORECAST_CLOSE_GRACE_SECONDS;
    finish_attempt(
        &connection,
        schedule(scope, cutoff, now, 2),
        scope,
        Ok(json!({ "status": "INSUFFICIENT_COVERAGE" })),
        now,
    )
    .unwrap();
    let saved = load_schedule(&connection, scope).unwrap().unwrap();
    assert_eq!(saved.status, "waiting_for_coverage");
    assert_eq!(saved.next_attempt_at, now + COVERAGE_RETRY_SECONDS);
    assert_eq!(saved.attempt_count, 2);
    assert_eq!(
        serde_json::from_str::<Value>(&saved.result_json).unwrap()["status"],
        "INSUFFICIENT_COVERAGE"
    );
}

#[test]
fn exhausted_stockbit_coverage_retries_wait_until_the_next_session_window() {
    let connection = schedule_database();
    let scope = ForecastScope::StockbitIdx;
    let cutoff = local_day_start(6);
    let now = cutoff + FORECAST_CLOSE_GRACE_SECONDS + 4 * COVERAGE_RETRY_SECONDS;
    finish_attempt(
        &connection,
        schedule(scope, cutoff, now, MAX_FORECAST_ATTEMPTS_PER_WINDOW),
        scope,
        Ok(json!({ "status": "INSUFFICIENT_COVERAGE" })),
        now,
    )
    .unwrap();
    let saved = load_schedule(&connection, scope).unwrap().unwrap();
    assert_eq!(saved.status, "complete");
    assert_eq!(saved.next_attempt_at, next_window_attempt_at(scope, cutoff));
}

#[test]
fn universe_is_frozen_at_first_attempt_and_reused_for_retries() {
    let connection = schedule_database();
    connection.execute_batch(
        "CREATE TABLE app.instruments (
             instrument_id TEXT,exchange_id TEXT,product_id TEXT,asset_category_id TEXT,
             symbol TEXT,storage_symbol TEXT,venue_verified INTEGER,listing_status TEXT,
             venue_symbol TEXT,quote_currency TEXT,underlying_symbol TEXT,price_source_policy TEXT,
             execution_supported INTEGER,is_active INTEGER
         );
         CREATE TABLE instrument_exclusions (
             exchange_id TEXT,asset_category_id TEXT,symbol TEXT,is_active INTEGER
         );
         INSERT INTO app.instruments VALUES(
             'stockbit.stockbit_regular.bbca','stockbit','stockbit_regular','idx_stock',
             'BBCA','BBCA.JK',1,'public_listed_snapshot','BBCA.JK','IDR','BBCA',
             'research_proxy',0,1
         );"
    ).unwrap();
    let scope = ForecastScope::StockbitIdx;
    let now = 1_800_000_000;
    let mut saved_schedule = schedule(scope, now, now, 0);
    saved_schedule.universe_json = "null".to_owned();

    let first = ensure_universe_snapshot(&connection, &mut saved_schedule, scope, now).unwrap();
    assert_eq!(first.len(), 1);
    connection.execute("UPDATE app.instruments SET is_active=0", []).unwrap();
    let retry = ensure_universe_snapshot(&connection, &mut saved_schedule, scope, now + 900).unwrap();
    assert_eq!(retry, first);
}

#[test]
fn scan_error_is_saved_with_a_retry_and_a_utf8_safe_size_limit() {
    let connection = schedule_database();
    let scope = ForecastScope::TokocryptoSpot;
    let cutoff = 1_800_000_000;
    let now = cutoff + FORECAST_CLOSE_GRACE_SECONDS;
    finish_attempt(
        &connection,
        schedule(scope, cutoff, now, 1),
        scope,
        Err("é".repeat(300)),
        now,
    )
    .unwrap();
    let saved = load_schedule(&connection, scope).unwrap().unwrap();
    assert_eq!(saved.status, "error");
    assert_eq!(saved.next_attempt_at, now + COVERAGE_RETRY_SECONDS);
    assert!(saved.last_error.len() <= MAX_FORECAST_ERROR_BYTES);
    assert_eq!(saved.last_error.chars().count(), 250);
}

#[test]
fn latest_result_includes_scope_schedule_and_observed_outcomes() {
    let connection = schedule_database();
    let scope = ForecastScope::TokocryptoSpot;
    let result = json!({
        "status":"INSUFFICIENT_COVERAGE",
        "scope_id":"tokocrypto_spot",
        "protocol_id":scope.protocol(),
        "target_kind":"calendar_24h",
        "forecast_target":"Next 24h close-to-close return (%)",
        "forecast_horizon_seconds":86400,
        "source_family":"tokocrypto_native",
        "data_provider":"tokocrypto",
        "price_basis":"venue_native",
        "as_of":1_800_000_000,
        "forecast_until":1_800_086_400,
        "eligible_count":20,
        "forecasted_count":10,
        "minimum_forecasted_count":16,
        "model":"Naive 24h momentum extrapolation",
        "model_description":"Unvalidated research.",
        "candidates":[]
    });
    connection.execute(
        "INSERT INTO app.gainer_forecast_schedule
             VALUES(?1,?2,1800000000,'waiting_for_coverage',2,1800000100,
                    1800001000,'',?3,'[]',1800000100)",
        params![scope.id(), scope.protocol(), serde_json::to_string(&result).unwrap()],
    ).unwrap();

    let latest = gainer_forecast_latest(&connection, scope).unwrap();
    assert_eq!(latest["scope_id"], "tokocrypto_spot");
    assert_eq!(latest["status"], "INSUFFICIENT_COVERAGE");
    assert_eq!(latest["schedule"]["status"], "waiting_for_coverage");
    assert_eq!(latest["schedule"]["attempt_count"], 2);
    assert_eq!(latest["schedule"]["max_attempts_per_window"], 5);
    assert_eq!(latest["schedule"]["next_scan_at"], 1_800_001_000);
    assert_eq!(latest["observed_outcomes"]["sample_count"], 0);
}

#[test]
fn missing_scope_schedule_returns_a_due_time_without_fabricating_a_forecast() {
    let connection = schedule_database();
    let latest = gainer_forecast_latest(&connection, ForecastScope::StockbitIdx).unwrap();
    assert_eq!(latest["scope_id"], "stockbit_idx");
    assert_eq!(latest["status"], "SCHEDULED");
    assert!(latest["as_of"].is_null());
    assert!(latest["candidates"].as_array().unwrap().is_empty());
    assert_eq!(latest["schedule"]["status"], "scheduled");
    assert!(latest["schedule"]["next_scan_at"].as_i64().unwrap() > 0);
}
