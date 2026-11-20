use super::gainer_forecast::{
    forecast_universe, gainer_forecast_for_scope_at, observed_outcomes, scheduled_payload,
    settle_forecast_outcomes, store_forecast_run, ForecastScope,
};
use super::*;
use std::sync::Arc;
use std::time::Duration;

const FORECAST_CHECK_INTERVAL_SECONDS: u64 = 30;
const FORECAST_CLOSE_GRACE_SECONDS: i64 = 5 * 60;
const STOCKBIT_DAILY_DATA_GRACE_SECONDS: i64 = 14 * 60 * 60;
const COVERAGE_RETRY_SECONDS: i64 = 60;
const FORECAST_ERROR_RETRY_SECONDS: i64 = 15 * 60;
const MAX_FORECAST_ATTEMPTS_PER_WINDOW: usize = 15;
const MAX_FORECAST_ERROR_BYTES: usize = 500;
const JAKARTA_OFFSET_SECONDS: i64 = quant_core::JAKARTA_UTC_OFFSET_SECONDS;

#[derive(Clone)]
struct ForecastSchedule {
    scope_id: String,
    protocol_id: String,
    forecast_cutoff: i64,
    status: String,
    attempt_count: usize,
    last_attempt_at: i64,
    next_attempt_at: i64,
    last_error: String,
    result_json: String,
    universe_json: String,
}

fn stockbit_weekday(cutoff: i64) -> i64 {
    (cutoff + JAKARTA_OFFSET_SECONDS)
        .div_euclid(SECONDS_PER_DAY)
        .saturating_add(4)
        .rem_euclid(7)
}

fn stockbit_scan_day(weekday: i64) -> bool {
    (2..=6).contains(&weekday)
}

fn stockbit_window_passed(cutoff: i64, now: i64) -> bool {
    let target_day = if stockbit_weekday(cutoff) == 6 {
        cutoff + 2 * SECONDS_PER_DAY
    } else {
        cutoff
    };
    let local_now = now + JAKARTA_OFFSET_SECONDS;
    let local_day_start = local_now.div_euclid(SECONDS_PER_DAY) * SECONDS_PER_DAY
        - JAKARTA_OFFSET_SECONDS;
    local_day_start > target_day
        || (local_day_start == target_day
            && local_now.rem_euclid(SECONDS_PER_DAY) >= 16 * 60 * 60)
}

fn window_cutoff(scope: ForecastScope, now: i64) -> i64 {
    match scope {
        ForecastScope::TokocryptoSpot => {
            let interval = scope.scan_interval_seconds();
            now - now.rem_euclid(interval)
        }
        ForecastScope::StockbitIdx => {
            let local_day_start = (now + JAKARTA_OFFSET_SECONDS)
                .div_euclid(SECONDS_PER_DAY)
                .saturating_mul(SECONDS_PER_DAY)
                - JAKARTA_OFFSET_SECONDS;
            let weekday = stockbit_weekday(local_day_start);
            let scan_day = match weekday {
                0 => local_day_start - SECONDS_PER_DAY,
                1 => local_day_start - 2 * SECONDS_PER_DAY,
                _ => local_day_start,
            };
            scan_day
        }
    }
}

fn first_attempt_at_for_scope(scope: ForecastScope, cutoff: i64) -> i64 {
    cutoff
        + if scope == ForecastScope::TokocryptoSpot {
            FORECAST_CLOSE_GRACE_SECONDS
        } else {
            STOCKBIT_DAILY_DATA_GRACE_SECONDS
        }
}

fn next_window_attempt_at(scope: ForecastScope, cutoff: i64) -> i64 {
    match scope {
        ForecastScope::TokocryptoSpot => {
            first_attempt_at_for_scope(scope, cutoff + scope.scan_interval_seconds())
        }
        ForecastScope::StockbitIdx => {
            let mut next_cutoff = cutoff + SECONDS_PER_DAY;
            while !stockbit_scan_day(stockbit_weekday(next_cutoff)) {
                next_cutoff += SECONDS_PER_DAY;
            }
            first_attempt_at_for_scope(scope, next_cutoff)
        }
    }
}

fn forecast_cutoff_nanoseconds(cutoff_seconds: i64) -> Result<i64, String> {
    cutoff_seconds.checked_mul(NANOS_PER_SECOND).ok_or_else(|| {
        "The scheduled forecast cutoff exceeds the supported timestamp range.".to_owned()
    })
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

fn load_schedule(
    connection: &Connection,
    scope: ForecastScope,
) -> Result<Option<ForecastSchedule>, rusqlite::Error> {
    connection
        .query_row(
            "SELECT scope_id,protocol_id,forecast_cutoff,status,attempt_count,last_attempt_at,
                    next_attempt_at,last_error,result_json,universe_json
               FROM app.gainer_forecast_schedule WHERE scope_id=?1",
            [scope.id()],
            |row| {
                Ok(ForecastSchedule {
                    scope_id: row.get(0)?,
                    protocol_id: row.get(1)?,
                    forecast_cutoff: row.get(2)?,
                    status: row.get(3)?,
                    attempt_count: row.get(4)?,
                    last_attempt_at: row.get(5)?,
                    next_attempt_at: row.get(6)?,
                    last_error: row.get(7)?,
                    result_json: row.get(8)?,
                    universe_json: row.get(9)?,
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
        "INSERT INTO app.gainer_forecast_schedule
            (scope_id,protocol_id,forecast_cutoff,status,attempt_count,last_attempt_at,
             next_attempt_at,last_error,result_json,universe_json,updated_at)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)
         ON CONFLICT(scope_id) DO UPDATE SET
            protocol_id=excluded.protocol_id,
            forecast_cutoff=excluded.forecast_cutoff,
            status=excluded.status,
            attempt_count=excluded.attempt_count,
            last_attempt_at=excluded.last_attempt_at,
            next_attempt_at=excluded.next_attempt_at,
            last_error=excluded.last_error,
            result_json=excluded.result_json,
            universe_json=excluded.universe_json,
            updated_at=excluded.updated_at",
        params![
            schedule.scope_id,
            schedule.protocol_id,
            schedule.forecast_cutoff,
            schedule.status,
            schedule.attempt_count,
            schedule.last_attempt_at,
            schedule.next_attempt_at,
            schedule.last_error,
            schedule.result_json,
            schedule.universe_json,
            updated_at,
        ],
    )?;
    Ok(())
}

fn encode_universe(universe: &[Value]) -> Result<String, rusqlite::Error> {
    serde_json::to_string(universe)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
}

fn reset_for_window(
    connection: &Connection,
    previous: Option<ForecastSchedule>,
    scope: ForecastScope,
    cutoff: i64,
    now: i64,
) -> Result<ForecastSchedule, rusqlite::Error> {
    let result_json = previous
        .as_ref()
        .filter(|schedule| schedule.protocol_id == scope.protocol())
        .map(|schedule| schedule.result_json.clone())
        .unwrap_or_else(|| "{}".to_owned());
    let schedule = ForecastSchedule {
        scope_id: scope.id().to_owned(),
        protocol_id: scope.protocol().to_owned(),
        forecast_cutoff: cutoff,
        status: "scheduled".to_owned(),
        attempt_count: 0,
        last_attempt_at: 0,
        next_attempt_at: first_attempt_at_for_scope(scope, cutoff),
        last_error: String::new(),
        result_json,
        universe_json: "null".to_owned(),
    };
    save_schedule(connection, &schedule, now)?;
    Ok(schedule)
}

fn ensure_universe_snapshot(
    connection: &Connection,
    schedule: &mut ForecastSchedule,
    scope: ForecastScope,
    now: i64,
) -> Result<Vec<Value>, rusqlite::Error> {
    let saved = serde_json::from_str::<Option<Vec<Value>>>(&schedule.universe_json)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
    if let Some(saved) = saved {
        return Ok(saved);
    }
    let universe = forecast_universe(connection, scope)?;
    schedule.universe_json = encode_universe(&universe)?;
    save_schedule(connection, schedule, now)?;
    Ok(universe)
}

fn recover_interrupted_schedule(
    connection: &Connection,
    scope: ForecastScope,
    mut schedule: ForecastSchedule,
    now: i64,
) -> Result<ForecastSchedule, rusqlite::Error> {
    if schedule.status != "running" {
        return Ok(schedule);
    }
    if schedule.attempt_count >= MAX_FORECAST_ATTEMPTS_PER_WINDOW {
        schedule.status = "complete".to_owned();
        schedule.next_attempt_at = next_window_attempt_at(scope, schedule.forecast_cutoff);
    } else {
        schedule.status = "scheduled".to_owned();
        schedule.next_attempt_at = now;
        schedule.last_error = "The previous scheduled scan was interrupted; it has been requeued.".to_owned();
    }
    save_schedule(connection, &schedule, now)?;
    Ok(schedule)
}

fn refresh_waiting_coverage_retry(
    connection: &Connection,
    schedule: &mut ForecastSchedule,
    now: i64,
) -> Result<(), rusqlite::Error> {
    if schedule.status != "waiting_for_coverage" || schedule.last_attempt_at <= 0 {
        return Ok(());
    }
    let retry_at = schedule.last_attempt_at.saturating_add(COVERAGE_RETRY_SECONDS);
    if schedule.next_attempt_at > retry_at {
        schedule.next_attempt_at = retry_at;
        save_schedule(connection, schedule, now)?;
    }
    Ok(())
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
    scope: ForecastScope,
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
                schedule.next_attempt_at = next_window_attempt_at(scope, schedule.forecast_cutoff);
            }
        }
        Err(error) => {
            let retries_remain = schedule.attempt_count < MAX_FORECAST_ATTEMPTS_PER_WINDOW;
            schedule.status = "error".to_owned();
            schedule.next_attempt_at = if retries_remain {
                now + FORECAST_ERROR_RETRY_SECONDS
            } else {
                next_window_attempt_at(scope, schedule.forecast_cutoff)
            };
            schedule.last_error = bounded_error(error);
        }
    }
    save_schedule(connection, &schedule, now)
}

fn scheduled_forecast_tick(state: &AppState, scope: ForecastScope) -> Result<u64, String> {
    let connection = open_db_minimal(&state.market_database).map_err(|error| error.to_string())?;
    let now = now_secs().min(i64::MAX as u64) as i64;
    let cutoff = window_cutoff(scope, now);
    let previous = load_schedule(&connection, scope).map_err(|error| error.to_string())?;
    let mut schedule = match previous {
        Some(schedule)
            if schedule.protocol_id == scope.protocol() && schedule.forecast_cutoff == cutoff =>
        {
            schedule
        }
        other => reset_for_window(&connection, other, scope, cutoff, now)
            .map_err(|error| error.to_string())?,
    };
    schedule = recover_interrupted_schedule(&connection, scope, schedule, now)
        .map_err(|error| error.to_string())?;
    refresh_waiting_coverage_retry(&connection, &mut schedule, now)
        .map_err(|error| error.to_string())?;
    if schedule.next_attempt_at > now {
        return Ok((schedule.next_attempt_at - now).max(1) as u64);
    }
    if scope == ForecastScope::StockbitIdx && stockbit_window_passed(cutoff, now) {
        schedule.status = "complete".to_owned();
        schedule.next_attempt_at = next_window_attempt_at(scope, cutoff);
        save_schedule(&connection, &schedule, now).map_err(|error| error.to_string())?;
        return Ok((schedule.next_attempt_at - now).max(1) as u64);
    }
    if schedule.attempt_count >= MAX_FORECAST_ATTEMPTS_PER_WINDOW {
        schedule.status = "complete".to_owned();
        schedule.next_attempt_at = next_window_attempt_at(scope, cutoff);
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
    let instruments = ensure_universe_snapshot(&connection, &mut schedule, scope, now)
        .map_err(|error| error.to_string())?;
    let forecast_cutoff = forecast_cutoff_nanoseconds(cutoff)?;
    mark_attempt_started(&connection, &mut schedule, now).map_err(|error| error.to_string())?;
    let forecast = settle_forecast_outcomes(&connection, &state.market_database, scope, now)
        .map(|_| ())
        .map_err(|error| error.to_string())
        .and_then(|()| {
            gainer_forecast_for_scope_at(
                &state.market_database,
                scope,
                forecast_cutoff,
                &instruments,
            )
            .map_err(|error| error.to_string())
        })
        .and_then(|payload| {
            store_forecast_run(&connection, scope, cutoff, &payload, now)
                .map_err(|error| error.to_string())?;
            Ok(payload)
        });
    let result = finish_attempt(&connection, schedule, scope, forecast, now_secs() as i64)
        .map_err(|error| error.to_string());
    drop(permit);
    result?;
    Ok(1)
}

pub(crate) fn background_gainer_forecast_loop(state: Arc<AppState>) {
    eprintln!("[GainerForecast] Scheduled Tokocrypto Spot and Stockbit IDX forecast worker initialized.");
    while state.last_registry_sync_epoch.load(Ordering::Relaxed) == 0 {
        thread::sleep(Duration::from_secs(15));
    }
    loop {
        let mut wait_seconds = FORECAST_CHECK_INTERVAL_SECONDS;
        for scope in [ForecastScope::TokocryptoSpot, ForecastScope::StockbitIdx] {
            match scheduled_forecast_tick(&state, scope) {
                Ok(scope_wait) => wait_seconds = wait_seconds.min(scope_wait),
                Err(error) => eprintln!("[GainerForecast] {} scheduled scan failed: {error}", scope.id()),
            }
        }
        thread::sleep(Duration::from_secs(
            wait_seconds.clamp(1, FORECAST_CHECK_INTERVAL_SECONDS),
        ));
    }
}

pub(crate) fn gainer_forecast_latest(
    connection: &Connection,
    scope: ForecastScope,
) -> Result<Value, rusqlite::Error> {
    let now = now_secs().min(i64::MAX as u64) as i64;
    let cutoff = window_cutoff(scope, now);
    let schedule = load_schedule(connection, scope)?;
    let (mut payload, schedule_json) = match schedule {
        Some(schedule) => {
            let saved_payload = if schedule.result_json == "{}" {
                scheduled_payload(scope)
            } else {
                serde_json::from_str::<Value>(&schedule.result_json).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?
            };
            let payload = if saved_payload.get("protocol_id").and_then(Value::as_str)
                == Some(scope.protocol())
                && saved_payload.get("scope_id").and_then(Value::as_str) == Some(scope.id())
            {
                saved_payload
            } else {
                scheduled_payload(scope)
            };
            let next_scan_at = (schedule.next_attempt_at > 0).then_some(schedule.next_attempt_at);
            (
                payload,
                json!({
                    "status": schedule.status,
                    "attempt_count": schedule.attempt_count,
                    "max_attempts_per_window": MAX_FORECAST_ATTEMPTS_PER_WINDOW,
                    "last_attempt_at": (schedule.last_attempt_at > 0).then_some(schedule.last_attempt_at),
                    "next_scan_at": next_scan_at,
                    "last_error": (!schedule.last_error.is_empty()).then_some(schedule.last_error),
                    "forecast_cutoff": schedule.forecast_cutoff,
                    "target_window_passed": scope == ForecastScope::StockbitIdx
                        && (stockbit_window_passed(schedule.forecast_cutoff, now)
                            || schedule.forecast_cutoff != cutoff),
                    "scope_id": scope.id(),
                    "scan_interval_seconds": scope.scan_interval_seconds(),
                }),
            )
        }
        None => {
            let next_scan_at = first_attempt_at_for_scope(scope, cutoff);
            (
                scheduled_payload(scope),
                json!({
                    "status":"scheduled",
                    "attempt_count":0,
                    "max_attempts_per_window":MAX_FORECAST_ATTEMPTS_PER_WINDOW,
                    "last_attempt_at":null,
                    "next_scan_at":next_scan_at,
                    "last_error":null,
                    "forecast_cutoff":cutoff,
                    "target_window_passed":false,
                    "scope_id":scope.id(),
                    "scan_interval_seconds":scope.scan_interval_seconds(),
                }),
            )
        }
    };
    if let Some(object) = payload.as_object_mut() {
        object.insert("schedule".to_owned(), schedule_json);
        object.insert("observed_outcomes".to_owned(), observed_outcomes(connection, scope)?);
    }
    Ok(payload)
}

#[cfg(test)]
#[path = "gainer_forecast_scheduler_tests.rs"]
mod tests;
