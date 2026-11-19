use super::gainer_forecast::{FORECAST_PROTOCOL, FORECAST_TIMEFRAME};
use super::*;
use std::sync::Arc;
use std::time::Duration;

const FORECAST_CHECK_INTERVAL_SECONDS: u64 = 30;
const FORECAST_CLOSE_GRACE_SECONDS: i64 = 5 * 60;
const COVERAGE_RETRY_SECONDS: i64 = 15 * 60;
const MAX_FORECAST_ATTEMPTS_PER_WINDOW: usize = 5;
const MAX_FORECAST_ERROR_BYTES: usize = 500;

#[derive(Clone)]
struct ForecastSchedule {
    protocol_id: String,
    forecast_cutoff: i64,
    status: String,
    attempt_count: usize,
    last_attempt_at: i64,
    next_attempt_at: i64,
    last_error: String,
    result_json: String,
}

fn interval_seconds() -> i64 {
    timeframe_duration_ns(FORECAST_TIMEFRAME).expect("supported forecast timeframe")
        / NANOS_PER_SECOND
}

fn window_cutoff(now: i64) -> i64 {
    let interval = interval_seconds();
    now - now.rem_euclid(interval)
}

fn forecast_cutoff_nanoseconds(cutoff_seconds: i64) -> Result<i64, String> {
    cutoff_seconds.checked_mul(NANOS_PER_SECOND).ok_or_else(|| {
        "The scheduled forecast cutoff exceeds the supported timestamp range.".to_owned()
    })
}

fn first_attempt_at(cutoff: i64, now: i64) -> i64 {
    (cutoff + FORECAST_CLOSE_GRACE_SECONDS).max(now)
}

fn next_window_attempt_at(cutoff: i64) -> i64 {
    cutoff + interval_seconds() + FORECAST_CLOSE_GRACE_SECONDS
}

fn bounded_error(error: String) -> String {
    if error.len() <= MAX_FORECAST_ERROR_BYTES {
        return error;
    }
    let mut end = MAX_FORECAST_ERROR_BYTES;
    while !error.is_char_boundary(end) {
        end -= 1;
    }
    error[..end].to_owned()
}

fn load_schedule(connection: &Connection) -> Result<Option<ForecastSchedule>, rusqlite::Error> {
    connection
        .query_row(
            "SELECT protocol_id,forecast_cutoff,status,attempt_count,last_attempt_at,
                    next_attempt_at,last_error,result_json
               FROM app.tokocrypto_gainer_forecast_schedule WHERE id=1",
            [],
            |row| {
                Ok(ForecastSchedule {
                    protocol_id: row.get(0)?,
                    forecast_cutoff: row.get(1)?,
                    status: row.get(2)?,
                    attempt_count: row.get(3)?,
                    last_attempt_at: row.get(4)?,
                    next_attempt_at: row.get(5)?,
                    last_error: row.get(6)?,
                    result_json: row.get(7)?,
                })
            },
        )
        .optional()
}

fn save_schedule(
    connection: &Connection,
    schedule: &ForecastSchedule,
    updated_at: i64,
) -> Result<(), rusqlite::Error> {
    connection.execute(
        "INSERT INTO app.tokocrypto_gainer_forecast_schedule
            (id,protocol_id,forecast_cutoff,status,attempt_count,last_attempt_at,
             next_attempt_at,last_error,result_json,updated_at)
         VALUES(1,?1,?2,?3,?4,?5,?6,?7,?8,?9)
         ON CONFLICT(id) DO UPDATE SET
            protocol_id=excluded.protocol_id,
            forecast_cutoff=excluded.forecast_cutoff,
            status=excluded.status,
            attempt_count=excluded.attempt_count,
            last_attempt_at=excluded.last_attempt_at,
            next_attempt_at=excluded.next_attempt_at,
            last_error=excluded.last_error,
            result_json=excluded.result_json,
            updated_at=excluded.updated_at",
        params![
            schedule.protocol_id,
            schedule.forecast_cutoff,
            schedule.status,
            schedule.attempt_count,
            schedule.last_attempt_at,
            schedule.next_attempt_at,
            schedule.last_error,
            schedule.result_json,
            updated_at,
        ],
    )?;
    Ok(())
}

fn reset_for_window(
    connection: &Connection,
    previous: Option<ForecastSchedule>,
    cutoff: i64,
    now: i64,
) -> Result<ForecastSchedule, rusqlite::Error> {
    let result_json = previous
        .as_ref()
        .filter(|schedule| schedule.protocol_id == FORECAST_PROTOCOL)
        .map(|schedule| schedule.result_json.clone())
        .unwrap_or_else(|| "{}".to_owned());
    let schedule = ForecastSchedule {
        protocol_id: FORECAST_PROTOCOL.to_owned(),
        forecast_cutoff: cutoff,
        status: "scheduled".to_owned(),
        attempt_count: 0,
        last_attempt_at: 0,
        next_attempt_at: first_attempt_at(cutoff, now),
        last_error: String::new(),
        result_json,
    };
    save_schedule(connection, &schedule, now)?;
    Ok(schedule)
}

fn recover_interrupted_schedule(
    connection: &Connection,
    mut schedule: ForecastSchedule,
    now: i64,
) -> Result<ForecastSchedule, rusqlite::Error> {
    if schedule.status != "running" {
        return Ok(schedule);
    }
    if schedule.attempt_count >= MAX_FORECAST_ATTEMPTS_PER_WINDOW {
        schedule.status = "complete".to_owned();
        schedule.next_attempt_at = next_window_attempt_at(schedule.forecast_cutoff);
    } else {
        schedule.status = "scheduled".to_owned();
        schedule.next_attempt_at = now;
        schedule.last_error =
            "The previous scheduled scan was interrupted; it has been requeued.".to_owned();
    }
    save_schedule(connection, &schedule, now)?;
    Ok(schedule)
}

fn mark_attempt_started(
    connection: &Connection,
    schedule: &mut ForecastSchedule,
    now: i64,
) -> Result<(), rusqlite::Error> {
    schedule.attempt_count += 1;
    schedule.status = "running".to_owned();
    schedule.last_attempt_at = now;
    schedule.next_attempt_at = 0;
    schedule.last_error.clear();
    save_schedule(connection, schedule, now)
}

fn finish_attempt(
    connection: &Connection,
    mut schedule: ForecastSchedule,
    result: Result<Value, String>,
    now: i64,
) -> Result<(), rusqlite::Error> {
    match result {
        Ok(payload) => {
            let forecast_status = payload.get("status").and_then(Value::as_str).unwrap_or("");
            schedule.result_json = serde_json::to_string(&payload)
                .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
            schedule.last_error.clear();
            let retry_coverage = forecast_status == "INSUFFICIENT_COVERAGE"
                && schedule.attempt_count < MAX_FORECAST_ATTEMPTS_PER_WINDOW;
            if retry_coverage {
                schedule.status = "waiting_for_coverage".to_owned();
                schedule.next_attempt_at = now + COVERAGE_RETRY_SECONDS;
            } else {
                schedule.status = "complete".to_owned();
                schedule.next_attempt_at = next_window_attempt_at(schedule.forecast_cutoff);
            }
        }
        Err(error) => {
            let retries_remain = schedule.attempt_count < MAX_FORECAST_ATTEMPTS_PER_WINDOW;
            schedule.status = "error".to_owned();
            schedule.next_attempt_at = if retries_remain {
                now + COVERAGE_RETRY_SECONDS
            } else {
                next_window_attempt_at(schedule.forecast_cutoff)
            };
            schedule.last_error = bounded_error(error);
        }
    }
    save_schedule(connection, &schedule, now)
}

fn scheduled_forecast_tick(state: &AppState) -> Result<u64, String> {
    let connection = open_db_minimal(&state.market_database).map_err(|error| error.to_string())?;
    let now = now_secs().min(i64::MAX as u64) as i64;
    let cutoff = window_cutoff(now);
    let schedule = load_schedule(&connection).map_err(|error| error.to_string())?;
    let mut schedule = match schedule {
        Some(schedule)
            if schedule.protocol_id == FORECAST_PROTOCOL && schedule.forecast_cutoff == cutoff =>
        {
            schedule
        }
        other => {
            reset_for_window(&connection, other, cutoff, now).map_err(|error| error.to_string())?
        }
    };
    schedule = recover_interrupted_schedule(&connection, schedule, now)
        .map_err(|error| error.to_string())?;
    if schedule.next_attempt_at > now {
        return Ok((schedule.next_attempt_at - now).max(1) as u64);
    }
    if schedule.attempt_count >= MAX_FORECAST_ATTEMPTS_PER_WINDOW {
        schedule.status = "complete".to_owned();
        schedule.next_attempt_at = next_window_attempt_at(cutoff);
        save_schedule(&connection, &schedule, now).map_err(|error| error.to_string())?;
        return Ok((schedule.next_attempt_at - now).max(1) as u64);
    }
    if state.is_scanning.load(Ordering::Relaxed) {
        return Ok(FORECAST_CHECK_INTERVAL_SECONDS);
    }
    let permit = match ENTRY_VALIDATION_WORK.try_lock() {
        Ok(permit) if state.is_scanning.load(Ordering::Relaxed) => {
            drop(permit);
            return Ok(FORECAST_CHECK_INTERVAL_SECONDS);
        }
        Ok(permit) => permit,
        Err(std::sync::TryLockError::Poisoned(error)) => error.into_inner(),
        Err(std::sync::TryLockError::WouldBlock) => {
            return Ok(FORECAST_CHECK_INTERVAL_SECONDS);
        }
    };
    let forecast_cutoff = forecast_cutoff_nanoseconds(cutoff)?;
    mark_attempt_started(&connection, &mut schedule, now).map_err(|error| error.to_string())?;
    let forecast = gainer_forecast_at(&connection, &state.market_database, forecast_cutoff)
        .map_err(|error| error.to_string());
    let result = finish_attempt(&connection, schedule, forecast, now_secs() as i64)
        .map_err(|error| error.to_string());
    drop(permit);
    result?;
    Ok(1)
}

pub(crate) fn background_gainer_forecast_loop(state: Arc<AppState>) {
    eprintln!("[GainerForecast] Scheduled Tokocrypto 4h forecast worker initialized.");
    while state.last_registry_sync_epoch.load(Ordering::Relaxed) == 0 {
        thread::sleep(Duration::from_secs(15));
    }
    loop {
        match scheduled_forecast_tick(&state) {
            Ok(wait_seconds) => thread::sleep(Duration::from_secs(
                wait_seconds.clamp(1, FORECAST_CHECK_INTERVAL_SECONDS),
            )),
            Err(error) => {
                eprintln!("[GainerForecast] Scheduled scan failed: {error}");
                thread::sleep(Duration::from_secs(FORECAST_CHECK_INTERVAL_SECONDS));
            }
        }
    }
}

pub(crate) fn gainer_forecast_latest(connection: &Connection) -> Result<Value, rusqlite::Error> {
    let now = now_secs().min(i64::MAX as u64) as i64;
    let cutoff = window_cutoff(now);
    let schedule = load_schedule(connection)?;
    let (mut payload, schedule_json) = match schedule {
        Some(schedule) => {
            let payload = if schedule.result_json == "{}" {
                serde_json::json!({"status":"SCHEDULED","candidates":[]})
            } else {
                serde_json::from_str::<Value>(&schedule.result_json).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?
            };
            let next_scan_at = if schedule.next_attempt_at > 0 {
                Some(schedule.next_attempt_at)
            } else {
                None
            };
            (
                payload,
                serde_json::json!({
                    "status": schedule.status,
                    "attempt_count": schedule.attempt_count,
                    "max_attempts_per_window": MAX_FORECAST_ATTEMPTS_PER_WINDOW,
                    "last_attempt_at": (schedule.last_attempt_at > 0).then_some(schedule.last_attempt_at),
                    "next_scan_at": next_scan_at,
                    "last_error": (!schedule.last_error.is_empty()).then_some(schedule.last_error),
                    "forecast_cutoff": schedule.forecast_cutoff,
                }),
            )
        }
        None => {
            let next_scan_at = first_attempt_at(cutoff, now);
            (
                serde_json::json!({"status":"SCHEDULED","candidates":[]}),
                serde_json::json!({
                    "status":"scheduled",
                    "attempt_count":0,
                    "max_attempts_per_window":MAX_FORECAST_ATTEMPTS_PER_WINDOW,
                    "last_attempt_at":null,
                    "next_scan_at":next_scan_at,
                    "last_error":null,
                    "forecast_cutoff":cutoff,
                }),
            )
        }
    };
    if let Some(object) = payload.as_object_mut() {
        object.insert("schedule".to_owned(), schedule_json);
    }
    Ok(payload)
}

#[cfg(test)]
#[path = "gainer_forecast_scheduler_tests.rs"]
mod tests;
