use super::*;

fn schedule_database() -> Connection {
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch(
            "ATTACH DATABASE ':memory:' AS app;
             CREATE TABLE app.tokocrypto_gainer_forecast_schedule (
                 id INTEGER PRIMARY KEY, protocol_id TEXT NOT NULL, forecast_cutoff INTEGER NOT NULL,
                 status TEXT NOT NULL, attempt_count INTEGER NOT NULL, last_attempt_at INTEGER NOT NULL,
                 next_attempt_at INTEGER NOT NULL, last_error TEXT NOT NULL, result_json TEXT NOT NULL,
                 updated_at INTEGER NOT NULL
             );",
        )
        .unwrap();
    connection
}

#[test]
fn forecast_scans_start_five_minutes_after_each_aligned_four_hour_close() {
    let interval = 4 * 60 * 60;
    assert_eq!(interval_seconds(), interval);
    assert_eq!(window_cutoff(interval - 1), 0);
    assert_eq!(window_cutoff(interval), interval);
    assert_eq!(first_attempt_at(interval, interval + 60), interval + 5 * 60);
    assert_eq!(
        first_attempt_at(interval, interval + 10 * 60),
        interval + 10 * 60
    );
    assert_eq!(next_window_attempt_at(interval), 2 * interval + 5 * 60);
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
    let cutoff = 1_800_000_000;
    let now = cutoff + FORECAST_CLOSE_GRACE_SECONDS;
    let schedule = ForecastSchedule {
        protocol_id: FORECAST_PROTOCOL.to_owned(),
        forecast_cutoff: cutoff,
        status: "running".to_owned(),
        attempt_count: 2,
        last_attempt_at: now,
        next_attempt_at: 0,
        last_error: String::new(),
        result_json: "{}".to_owned(),
    };

    finish_attempt(
        &connection,
        schedule,
        Ok(json!({ "status": "INSUFFICIENT_COVERAGE" })),
        now,
    )
    .unwrap();

    let saved = load_schedule(&connection).unwrap().unwrap();
    assert_eq!(saved.status, "waiting_for_coverage");
    assert_eq!(saved.next_attempt_at, now + COVERAGE_RETRY_SECONDS);
    assert_eq!(saved.attempt_count, 2);
    assert_eq!(
        serde_json::from_str::<Value>(&saved.result_json).unwrap()["status"],
        "INSUFFICIENT_COVERAGE"
    );
}

#[test]
fn coverage_retry_budget_ends_at_the_next_four_hour_scan() {
    let connection = schedule_database();
    let cutoff = 1_800_000_000;
    let now = cutoff + FORECAST_CLOSE_GRACE_SECONDS + 4 * COVERAGE_RETRY_SECONDS;
    let schedule = ForecastSchedule {
        protocol_id: FORECAST_PROTOCOL.to_owned(),
        forecast_cutoff: cutoff,
        status: "running".to_owned(),
        attempt_count: MAX_FORECAST_ATTEMPTS_PER_WINDOW,
        last_attempt_at: now,
        next_attempt_at: 0,
        last_error: String::new(),
        result_json: "{}".to_owned(),
    };

    finish_attempt(
        &connection,
        schedule,
        Ok(json!({ "status": "INSUFFICIENT_COVERAGE" })),
        now,
    )
    .unwrap();

    let saved = load_schedule(&connection).unwrap().unwrap();
    assert_eq!(saved.status, "complete");
    assert_eq!(saved.next_attempt_at, next_window_attempt_at(cutoff));
    assert_eq!(saved.attempt_count, MAX_FORECAST_ATTEMPTS_PER_WINDOW);
}

#[test]
fn scan_error_is_saved_with_a_retry_and_a_utf8_safe_size_limit() {
    let connection = schedule_database();
    let cutoff = 1_800_000_000;
    let now = cutoff + FORECAST_CLOSE_GRACE_SECONDS;
    let schedule = ForecastSchedule {
        protocol_id: FORECAST_PROTOCOL.to_owned(),
        forecast_cutoff: cutoff,
        status: "running".to_owned(),
        attempt_count: 1,
        last_attempt_at: now,
        next_attempt_at: 0,
        last_error: String::new(),
        result_json: "{}".to_owned(),
    };

    finish_attempt(&connection, schedule, Err("é".repeat(300)), now).unwrap();

    let saved = load_schedule(&connection).unwrap().unwrap();
    assert_eq!(saved.status, "error");
    assert_eq!(saved.next_attempt_at, now + COVERAGE_RETRY_SECONDS);
    assert!(saved.last_error.len() <= MAX_FORECAST_ERROR_BYTES);
    assert_eq!(saved.last_error.chars().count(), 250);
}

#[test]
fn latest_result_includes_persisted_schedule_and_retry_status() {
    let connection = schedule_database();
    let result = json!({
        "status":"INSUFFICIENT_COVERAGE",
        "as_of":1_800_000_000,
        "forecast_until":1_800_014_400,
        "eligible_count":20,
        "scored_count":10,
        "minimum_scored_count":16,
        "candidates":[]
    });
    connection
        .execute(
            "INSERT INTO app.tokocrypto_gainer_forecast_schedule
                 VALUES(1,?1,1800000000,'waiting_for_coverage',2,1800000100,
                        1800001000,'',?2,1800000100)",
            params![FORECAST_PROTOCOL, serde_json::to_string(&result).unwrap()],
        )
        .unwrap();

    let latest = gainer_forecast_latest(&connection).unwrap();
    assert_eq!(latest["status"], "INSUFFICIENT_COVERAGE");
    assert_eq!(latest["schedule"]["status"], "waiting_for_coverage");
    assert_eq!(latest["schedule"]["attempt_count"], 2);
    assert_eq!(latest["schedule"]["max_attempts_per_window"], 5);
    assert_eq!(latest["schedule"]["next_scan_at"], 1_800_001_000);
}

#[test]
fn missing_schedule_returns_a_due_time_without_fabricating_a_forecast() {
    let connection = schedule_database();
    let latest = gainer_forecast_latest(&connection).unwrap();
    assert_eq!(latest["status"], "SCHEDULED");
    assert!(latest["as_of"].is_null());
    assert!(latest["candidates"].as_array().unwrap().is_empty());
    assert_eq!(latest["schedule"]["status"], "scheduled");
    assert!(latest["schedule"]["next_scan_at"].as_i64().unwrap() > 0);
}
