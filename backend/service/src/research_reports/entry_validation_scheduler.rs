use super::*;
use std::sync::{Arc, Condvar, Mutex as DailyRefreshMutex};
use std::time::Duration;

const DAILY_REFRESH_INTERVAL_SECONDS: u64 = quant_core::SECONDS_PER_DAY as u64;
const DAILY_REFRESH_RETRY_SECONDS: u64 = 15 * quant_core::SECONDS_PER_MINUTE as u64;
const DAILY_REFRESH_WAIT_SECONDS: u64 = 5;
const DAILY_REFRESH_PAUSE_MILLISECONDS: u64 = 100;
const PROFILE_EVALUATION_RETRY_PAIRS: usize = 500;

static DAILY_REFRESH_WAKE: (DailyRefreshMutex<bool>, Condvar) =
    (DailyRefreshMutex::new(false), Condvar::new());

pub(super) fn notify_daily_refresh_worker() {
    let mut notified = match DAILY_REFRESH_WAKE.0.lock() {
        Ok(guard) => guard,
        Err(error) => error.into_inner(),
    };
    *notified = true;
    DAILY_REFRESH_WAKE.1.notify_one();
}

fn wait_for_control_or_timeout(seconds: u64) {
    let notified = match DAILY_REFRESH_WAKE.0.lock() {
        Ok(guard) => guard,
        Err(error) => error.into_inner(),
    };
    if *notified {
        let mut notified = notified;
        *notified = false;
        return;
    }
    let (mut notified, _) = DAILY_REFRESH_WAKE
        .1
        .wait_timeout(notified, Duration::from_secs(seconds.max(1)))
        .unwrap_or_else(|error| error.into_inner());
    *notified = false;
}

fn required_profile_coverage_pairs(total_pairs: usize) -> usize {
    total_pairs
        .saturating_mul(PROFILE_MIN_COHORT_COVERAGE_PCT)
        .saturating_add(99)
        / 100
}

fn evaluate_scoring_profiles(
    connection: &Connection,
    cycle_started_at: i64,
    evaluated_at: i64,
    completed_pairs: usize,
    total_pairs: usize,
) -> bool {
    match super::evaluate_daily_scoring_profiles(
        connection,
        cycle_started_at,
        evaluated_at,
        completed_pairs,
        total_pairs,
    ) {
        Ok(evaluated) => {
            eprintln!(
                "[EntryValidation] Evaluated {evaluated} shared live-scoring cohorts after {completed_pairs}/{total_pairs} successful reports against the 50% after-fee target."
            );
            true
        }
        Err(error) => {
            eprintln!(
                "[EntryValidation] Live-scoring profile evaluation failed; existing active weights remain unchanged: {error}"
            );
            false
        }
    }
}

#[derive(Clone)]
struct RefreshTarget {
    instrument_id: String,
    timeframe: String,
    label: String,
}

fn refresh_targets(connection: &Connection) -> Result<Vec<RefreshTarget>, String> {
    let mut targets = Vec::new();
    let model_catalog = signal_screener_master_data_for_connection(connection)
        .map_err(|error| error.to_string())?;
    for instrument_id in store::active_verified_instrument_ids(connection)
        .map_err(|error| error.to_string())?
    {
        let Some(instrument) = store::instrument(connection, &instrument_id)
            .map_err(|error| error.to_string())?
        else {
            continue;
        };
        if text(&instrument, "research_eligibility") != "eligible" {
            continue;
        }
        for timeframe in SIGNAL_SCREENER_TIMEFRAMES {
            if matching_models_in_catalog(&model_catalog, &instrument, timeframe).is_empty() {
                continue;
            }
            let label = format!(
                "{} · {} · {} · {}",
                text(&instrument, "exchange_id"),
                text(&instrument, "product_id"),
                text(&instrument, "symbol"),
                timeframe.to_ascii_uppercase()
            );
            targets.push(RefreshTarget {
                instrument_id: instrument_id.clone(),
                timeframe: timeframe.to_string(),
                label,
            });
        }
    }
    Ok(targets)
}

fn sleep_seconds(seconds: u64) {
    thread::sleep(Duration::from_secs(seconds.max(1)));
}

pub fn background_daily_refresh_loop(state: Arc<AppState>) {
    eprintln!("[EntryValidation] Daily per-ticker backtest worker initialized.");
    // Do not freeze an empty or incomplete universe before the first registry pass.
    while state.last_registry_sync_epoch.load(Ordering::Relaxed) == 0 {
        sleep_seconds(15);
    }
    loop {
        match run_due_daily_refresh(&state) {
            Ok(wait_seconds) => wait_for_control_or_timeout(wait_seconds),
            Err(error) => {
                eprintln!("[EntryValidation] Daily refresh worker error: {error}");
                wait_for_control_or_timeout(DAILY_REFRESH_RETRY_SECONDS);
            }
        }
    }
}

fn run_due_daily_refresh(state: &AppState) -> Result<u64, String> {
    let connection = open_db_minimal(&state.market_database).map_err(|error| error.to_string())?;
    let now = now_secs().min(i64::MAX as u64) as i64;
    let persisted = store::daily_refresh_state(&connection).map_err(|error| error.to_string())?;
    let control = store::daily_refresh_control(&connection).map_err(|error| error.to_string())?;
    let protocol_changed = persisted.protocol_id != PROTOCOL_ID;
    let unfinished = persisted.cycle_started_at > persisted.cycle_completed_at
        && !matches!(persisted.status.as_str(), "idle" | "complete" | "complete_with_errors");
    if control.paused {
        if unfinished && persisted.status != "waiting" {
            store::wait_daily_refresh(&connection, now)
                .map_err(|error| error.to_string())?;
        }
        return Ok(DAILY_REFRESH_WAIT_SECONDS);
    }
    if unfinished
        && !protocol_changed
        && persisted.status == "error"
        && persisted.next_due_at > now
        && !control.run_requested
    {
        return Ok((persisted.next_due_at - now) as u64);
    }
    if !unfinished && !protocol_changed && !control.run_requested && persisted.next_due_at > now {
        return Ok((persisted.next_due_at - now) as u64);
    }

    let targets = match refresh_targets(&connection) {
        Ok(targets) => targets,
        Err(error) => {
            let updated_at = now_secs().min(i64::MAX as u64) as i64;
            store::fail_daily_refresh_cycle(
                &connection,
                &error,
                updated_at.saturating_add(DAILY_REFRESH_RETRY_SECONDS as i64),
                updated_at,
            )
            .map_err(|save_error| save_error.to_string())?;
            return Ok(DAILY_REFRESH_RETRY_SECONDS);
        }
    };

    let (cycle_started_at, cursor) = if unfinished && !protocol_changed {
        let resume_instrument_id = if persisted.resume_instrument_id.is_empty() {
            &persisted.current_instrument_id
        } else {
            &persisted.resume_instrument_id
        };
        let resume_timeframe = if persisted.resume_timeframe.is_empty() {
            &persisted.current_timeframe
        } else {
            &persisted.resume_timeframe
        };
        let exact_position = targets.iter().position(|target| {
            target.instrument_id.as_str() == resume_instrument_id.as_str()
                && target.timeframe.as_str() == resume_timeframe.as_str()
        });
        let next_position = || {
            let frame_rank = SIGNAL_SCREENER_TIMEFRAMES
                .iter()
                .position(|frame| *frame == resume_timeframe.as_str())
                .unwrap_or(usize::MAX);
            targets.iter().position(|target| {
                target.instrument_id.as_str() > resume_instrument_id.as_str()
                    || (target.instrument_id.as_str() == resume_instrument_id.as_str()
                        && SIGNAL_SCREENER_TIMEFRAMES
                            .iter()
                            .position(|frame| *frame == target.timeframe.as_str())
                            .is_some_and(|rank| rank > frame_rank))
            })
        };
        (
            persisted.cycle_started_at,
            exact_position.or_else(next_position).unwrap_or(persisted.cursor),
        )
    } else {
        let next_due_at = now.saturating_add(DAILY_REFRESH_INTERVAL_SECONDS as i64);
        if protocol_changed && persisted.cycle_started_at > 0 {
            eprintln!(
                "[EntryValidation] Backtest protocol changed from {} to {}; restarting the daily refresh from the first eligible pair.",
                persisted.protocol_id,
                PROTOCOL_ID
            );
        }
        store::start_daily_refresh(&connection, now, targets.len(), next_due_at, PROTOCOL_ID)
            .map_err(|error| error.to_string())?;
        store::consume_daily_refresh_request(&connection, now)
            .map_err(|error| error.to_string())?;
        (now, 0)
    };
    if control.run_requested && unfinished && persisted.status == "error" {
        store::consume_daily_refresh_request(&connection, now)
            .map_err(|error| error.to_string())?;
    }
    let cursor = cursor.min(targets.len());
    store::set_daily_refresh_total(&connection, targets.len(), now)
        .map_err(|error| error.to_string())?;
    let mut completed_pairs = if unfinished {
        persisted.completed_pairs.min(targets.len())
    } else {
        0
    };
    let mut profiles_evaluated = false;
    let mut next_profile_evaluation_pair = required_profile_coverage_pairs(targets.len());
    if unfinished
        && next_profile_evaluation_pair > 0
        && completed_pairs >= next_profile_evaluation_pair
    {
        profiles_evaluated = evaluate_scoring_profiles(
            &connection,
            cycle_started_at,
            now,
            completed_pairs,
            targets.len(),
        );
        if !profiles_evaluated {
            next_profile_evaluation_pair =
                completed_pairs.saturating_add(PROFILE_EVALUATION_RETRY_PAIRS);
        }
    }
    eprintln!(
        "[EntryValidation] Daily refresh started: {} eligible ticker/timeframe reports, resuming at {}.",
        targets.len(), cursor
    );

    for (index, target) in targets.iter().enumerate().skip(cursor) {
        let now = now_secs().min(i64::MAX as u64) as i64;
        if store::daily_refresh_control(&connection)
            .map_err(|error| error.to_string())?
            .paused
        {
            store::wait_daily_refresh(&connection, now)
                .map_err(|error| error.to_string())?;
            return Ok(DAILY_REFRESH_WAIT_SECONDS);
        }
        store::set_daily_refresh_current(
            &connection,
            index,
            &target.instrument_id,
            &target.timeframe,
            &target.label,
            now,
        )
        .map_err(|error| error.to_string())?;

        let permit = loop {
            if store::daily_refresh_control(&connection)
                .map_err(|error| error.to_string())?
                .paused
            {
                store::wait_daily_refresh(&connection, now_secs() as i64)
                    .map_err(|error| error.to_string())?;
                return Ok(DAILY_REFRESH_WAIT_SECONDS);
            }
            if state.is_scanning.load(Ordering::Relaxed) {
                store::wait_daily_refresh(&connection, now_secs() as i64)
                    .map_err(|error| error.to_string())?;
                sleep_seconds(DAILY_REFRESH_WAIT_SECONDS);
                continue;
            }
            match ENTRY_VALIDATION_WORK.try_lock() {
                Ok(permit) if state.is_scanning.load(Ordering::Relaxed) => {
                    drop(permit);
                    store::wait_daily_refresh(&connection, now_secs() as i64)
                        .map_err(|error| error.to_string())?;
                    sleep_seconds(DAILY_REFRESH_WAIT_SECONDS);
                }
                Ok(permit) => break permit,
                Err(std::sync::TryLockError::Poisoned(error)) => break error.into_inner(),
                Err(std::sync::TryLockError::WouldBlock) => {
                    store::wait_daily_refresh(&connection, now_secs() as i64)
                        .map_err(|error| error.to_string())?;
                    sleep_seconds(DAILY_REFRESH_WAIT_SECONDS);
                }
            }
        };
        store::set_daily_refresh_current(
            &connection,
            index,
            &target.instrument_id,
            &target.timeframe,
            &target.label,
            now_secs() as i64,
        )
        .map_err(|error| error.to_string())?;
        let result = run_entry_validation(
            &connection,
            &state.market_database,
            &target.instrument_id,
            &target.timeframe,
        );
        drop(permit);

        let failed = result
            .as_ref()
            .err()
            .map(|error| (target.label.as_str(), error.as_str()));
        let pair_succeeded = result.is_ok();
        if let Some((label, error)) = failed {
            eprintln!("[EntryValidation] Daily backtest failed for {label}: {error}");
        }
        let updated_at = now_secs().min(i64::MAX as u64) as i64;
        let resume_target = targets.get(index + 1).unwrap_or(target);
        store::advance_daily_refresh(
            &connection,
            index + 1,
            (&resume_target.instrument_id, &resume_target.timeframe),
            failed,
            updated_at,
        )
        .map_err(|error| error.to_string())?;
        if pair_succeeded {
            completed_pairs = completed_pairs.saturating_add(1);
        }
        if !profiles_evaluated
            && next_profile_evaluation_pair > 0
            && completed_pairs >= next_profile_evaluation_pair
        {
            profiles_evaluated = evaluate_scoring_profiles(
                &connection,
                cycle_started_at,
                updated_at,
                completed_pairs,
                targets.len(),
            );
            if !profiles_evaluated {
                next_profile_evaluation_pair =
                    completed_pairs.saturating_add(PROFILE_EVALUATION_RETRY_PAIRS);
            }
        }
        drop(result);
        connection
            .execute_batch("PRAGMA shrink_memory;")
            .map_err(|error| error.to_string())?;
        thread::sleep(Duration::from_millis(DAILY_REFRESH_PAUSE_MILLISECONDS));
    }

    let completed_at = now_secs().min(i64::MAX as u64) as i64;
    // Keep the schedule start-to-start. If a run exceeds 24 hours, finish it
    // first; the returned one-second wait lets the next cycle start afterward.
    let next_due_at = cycle_started_at.saturating_add(DAILY_REFRESH_INTERVAL_SECONDS as i64);
    store::finish_daily_refresh(&connection, completed_at, next_due_at)
        .map_err(|error| error.to_string())?;
    let refresh_summary = store::daily_refresh_state(&connection)
        .map_err(|error| error.to_string())?;
    evaluate_scoring_profiles(
        &connection,
        cycle_started_at,
        completed_at,
        refresh_summary.completed_pairs,
        refresh_summary.total_pairs,
    );
    eprintln!(
        "[EntryValidation] Daily refresh complete: {} reports processed, {} failed.",
        targets.len(), refresh_summary.failed_pairs
    );
    Ok(next_due_at.saturating_sub(completed_at).max(1) as u64)
}
