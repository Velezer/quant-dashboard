use super::*;
use std::sync::Arc;
use std::time::Duration;

const DAILY_REFRESH_INTERVAL_SECONDS: u64 = quant_core::SECONDS_PER_DAY as u64;
const DAILY_REFRESH_RETRY_SECONDS: u64 = 15 * quant_core::SECONDS_PER_MINUTE as u64;
const DAILY_REFRESH_WAIT_SECONDS: u64 = 5;
const DAILY_REFRESH_PAUSE_MILLISECONDS: u64 = 100;

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
            Ok(wait_seconds) => sleep_seconds(wait_seconds),
            Err(error) => {
                eprintln!("[EntryValidation] Daily refresh worker error: {error}");
                sleep_seconds(DAILY_REFRESH_RETRY_SECONDS);
            }
        }
    }
}

fn run_due_daily_refresh(state: &AppState) -> Result<u64, String> {
    let connection = open_db_minimal(&state.market_database).map_err(|error| error.to_string())?;
    let now = now_secs().min(i64::MAX as u64) as i64;
    let persisted = store::daily_refresh_state(&connection).map_err(|error| error.to_string())?;
    let unfinished = persisted.cycle_started_at > persisted.cycle_completed_at
        && !matches!(persisted.status.as_str(), "idle" | "complete" | "complete_with_errors");
    if unfinished && persisted.status == "error" && persisted.next_due_at > now {
        return Ok((persisted.next_due_at - now) as u64);
    }
    if !unfinished && persisted.next_due_at > now {
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

    let (cycle_started_at, cursor) = if unfinished {
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
        store::start_daily_refresh(&connection, now, targets.len(), next_due_at)
            .map_err(|error| error.to_string())?;
        (now, 0)
    };
    let cursor = cursor.min(targets.len());
    store::set_daily_refresh_total(&connection, targets.len(), now)
        .map_err(|error| error.to_string())?;
    eprintln!(
        "[EntryValidation] Daily refresh started: {} eligible ticker/timeframe reports, resuming at {}.",
        targets.len(), cursor
    );

    for (index, target) in targets.iter().enumerate().skip(cursor) {
        let now = now_secs().min(i64::MAX as u64) as i64;
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
    match super::evaluate_daily_scoring_profiles(
        &connection,
        cycle_started_at,
        completed_at,
        refresh_summary.failed_pairs,
    ) {
        Ok(evaluated) => eprintln!(
            "[EntryValidation] Evaluated {evaluated} shared live-scoring cohorts against the 70% after-fee target."
        ),
        Err(error) => eprintln!(
            "[EntryValidation] Live-scoring profile evaluation failed; existing active weights remain unchanged: {error}"
        ),
    }
    eprintln!(
        "[EntryValidation] Daily refresh complete: {} reports processed, {} failed.",
        targets.len(), refresh_summary.failed_pairs
    );
    Ok(next_due_at.saturating_sub(completed_at).max(1) as u64)
}
