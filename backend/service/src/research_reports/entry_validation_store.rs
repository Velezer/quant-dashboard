use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

const MAX_PAGE_ROWS: usize = 100;
const MAX_REPORT_JSON_BYTES: usize = 256 * 1024;

fn invalid(message: &str) -> rusqlite::Error {
    rusqlite::Error::InvalidParameterName(message.to_string())
}

fn decode_json(encoded: String) -> Result<Value, rusqlite::Error> {
    serde_json::from_str(&encoded)
        .map_err(|error| rusqlite::Error::FromSqlConversionFailure(
            0, rusqlite::types::Type::Text, Box::new(error),
        ))
}

fn scope_values(exchange: &str, product: Option<&str>, search: Option<&str>) -> (String, Vec<String>) {
    let mut clause = "i.is_active=1 AND i.exchange_id=?1".to_string();
    let mut values = vec![exchange.to_string()];
    if let Some(product) = product.filter(|value| !value.is_empty() && *value != "all") {
        values.push(product.to_string());
        clause.push_str(&format!(" AND i.product_id=?{}", values.len()));
    }
    if let Some(search) = search.map(str::trim).filter(|value| !value.is_empty()) {
        let escaped = search.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
        values.push(format!("%{escaped}%"));
        let parameter = values.len();
        clause.push_str(&format!(
            " AND (i.symbol LIKE ?{parameter} ESCAPE '\\' COLLATE NOCASE
              OR i.venue_symbol LIKE ?{parameter} ESCAPE '\\' COLLATE NOCASE
              OR i.storage_symbol LIKE ?{parameter} ESCAPE '\\' COLLATE NOCASE)"
        ));
    }
    (clause, values)
}

fn read_instrument(row: &rusqlite::Row<'_>) -> Result<Value, rusqlite::Error> {
    Ok(json!({
        "instrument_id": row.get::<_, String>(0)?,
        "exchange_id": row.get::<_, String>(1)?,
        "product_id": row.get::<_, String>(2)?,
        "asset_category_id": row.get::<_, String>(3)?,
        "symbol": row.get::<_, String>(4)?,
        "storage_symbol": row.get::<_, String>(5)?,
        "venue_verified": row.get::<_, bool>(6)?,
        "listing_status": row.get::<_, String>(7)?,
        "venue_symbol": row.get::<_, String>(8)?,
        "quote_currency": row.get::<_, String>(9)?,
        "underlying_symbol": row.get::<_, String>(10)?,
        "price_source_policy": row.get::<_, String>(11)?,
        "execution_supported": row.get::<_, bool>(12)?,
    }))
}

fn annotate_eligibility(conn: &Connection, instrument: &mut Value) -> Result<(), rusqlite::Error> {
    let text = |key| instrument.get(key).and_then(Value::as_str).unwrap_or("");
    let eligible = crate::instrument_registry::research_eligibility(
        text("exchange_id"), text("listing_status"),
        instrument.get("venue_verified").and_then(Value::as_bool).unwrap_or(false),
    );
    let excluded = crate::instrument_registry::is_excluded(
        conn, text("exchange_id"), text("asset_category_id"), text("symbol"),
    )?;
    let status = if excluded { "excluded" } else if eligible == "pending_verification" {
        "pending_venue_verification"
    } else { eligible };
    instrument["research_eligibility"] = json!(status);
    Ok(())
}

const INSTRUMENT_COLUMNS: &str = "i.instrument_id,i.exchange_id,i.product_id,i.asset_category_id,
    i.symbol,i.storage_symbol,i.venue_verified,i.listing_status,i.venue_symbol,i.quote_currency,
    i.underlying_symbol,i.price_source_policy,i.execution_supported";

pub(super) fn instrument(conn: &Connection, instrument_id: &str) -> Result<Option<Value>, rusqlite::Error> {
    let mut instrument = conn.query_row(
        &format!("SELECT {INSTRUMENT_COLUMNS} FROM app.instruments i WHERE i.instrument_id=?1 AND i.is_active=1"),
        [instrument_id], read_instrument,
    ).optional()?;
    if let Some(instrument) = instrument.as_mut() { annotate_eligibility(conn, instrument)?; }
    Ok(instrument)
}

pub(super) fn verified_gainer_universe(
    conn: &Connection,
    max_instruments: usize,
) -> Result<Vec<Value>, rusqlite::Error> {
    let mut statement = conn.prepare(&format!(
        "SELECT {INSTRUMENT_COLUMNS} FROM app.instruments i
         WHERE i.is_active=1 AND i.exchange_id='tokocrypto' AND i.product_id='tokocrypto_spot'
           AND i.asset_category_id='crypto_spot' AND i.venue_verified=1
         ORDER BY i.instrument_id"
    ))?;
    let mut instruments = Vec::new();
    for row in statement.query_map([], read_instrument)? {
        let mut instrument = row?;
        annotate_eligibility(conn, &mut instrument)?;
        if instrument["research_eligibility"] == "eligible" {
            if instruments.len() == max_instruments {
                return Err(invalid("The Tokocrypto Spot universe exceeds the bounded forecast limit."));
            }
            instruments.push(instrument);
        }
    }
    Ok(instruments)
}

fn report_summary(report: &Value) -> Value {
    let models = report
        .get("models")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
        .iter()
        .map(|model| {
            let backtest = model.get("backtest").unwrap_or(&Value::Null);
            let risk_reward_rule = super::risk_reward_rule(backtest);
            let risk_reward_protocol = risk_reward_rule
                .and_then(|rule| rule.get("protocol_id"))
                .and_then(Value::as_str);
            let risk_reward_is_current = risk_reward_protocol
                == Some(quant_core::ENTRY_SCORE_RISK_REWARD_PROTOCOL_ID);
            let risk_reward_qualified = risk_reward_rule
                .and_then(|rule| rule.pointer("/candidate/qualified"))
                .and_then(Value::as_bool)
                == Some(true);
            let risk_reward_blocks_validation = super::four_hour_risk_reward_blocks_validation(
                report.get("timeframe").and_then(Value::as_str).unwrap_or(""),
                risk_reward_protocol,
                risk_reward_qualified,
            );
            let risk_reward_candidate = risk_reward_rule.and_then(|rule| rule.get("candidate"));
            json!({
                "kind": model.get("kind"),
                "label": model.get("label"),
                "score_model_id": model.get("score_model_id"),
                "entry_validation_status": if risk_reward_blocks_validation {
                    Some(json!("VALIDATION_PENDING"))
                } else {
                    model.get("entry_validation_status").cloned()
                },
                "holdout_trades": model.pointer("/backtest/holdout/trades"),
                "holdout_win_rate_pct": model.pointer("/backtest/holdout/win_rate_after_fees_pct"),
                "holdout_payoff_ratio": model.pointer("/backtest/holdout/payoff_ratio_after_fees"),
                "holdout_mean_return_pct": model.pointer("/backtest/holdout/mean_net_return_pct"),
                "risk_reward_protocol_id": risk_reward_rule.and_then(|rule| rule.get("protocol_id")),
                "risk_reward_status": if risk_reward_is_current {
                    risk_reward_rule.and_then(|rule| rule.get("status"))
                } else {
                    None
                },
                "risk_reward_holdout_trades": risk_reward_candidate
                    .and_then(|candidate| candidate.pointer("/holdout/trades"))
                    .filter(|_| risk_reward_is_current),
                "risk_reward_validation_win_rate_pct": risk_reward_candidate
                    .and_then(|candidate| candidate.pointer("/validation/win_rate_after_fees_pct"))
                    .filter(|_| risk_reward_is_current),
                "risk_reward_holdout_win_rate_pct": risk_reward_candidate
                    .and_then(|candidate| candidate.pointer("/holdout/win_rate_after_fees_pct"))
                    .filter(|_| risk_reward_is_current),
                "risk_reward_holdout_payoff_ratio_after_fees": risk_reward_candidate
                    .and_then(|candidate| candidate.pointer("/holdout/payoff_ratio_after_fees"))
                    .or_else(|| risk_reward_candidate.and_then(|candidate| candidate.pointer("/holdout/payoff_ratio")))
                    .filter(|_| risk_reward_is_current),
                "risk_reward_holdout_mean_return_pct": risk_reward_candidate
                    .and_then(|candidate| candidate.pointer("/holdout/mean_net_return_pct"))
                    .filter(|_| risk_reward_is_current),
                "selected_exit_bars": model.pointer("/backtest/selected_exit_bars"),
                "blocking_reasons": model.get("blocking_reasons"),
            })
        })
        .collect::<Vec<_>>();
    json!({
        "computed_at": report.get("computed_at"), "protocol_id": report.get("protocol_id"),
        "status": report.get("status"), "source": report.pointer("/source/id"),
        "source_is_proxy": report.pointer("/source/is_proxy"),
        "history_bars": report.pointer("/history/bars"), "models": models,
        "sizing_eligible": false,
    })
}

pub(super) fn save_report(conn: &Connection, report: &Value) -> Result<(), rusqlite::Error> {
    let instrument_id = report.pointer("/instrument/instrument_id").and_then(Value::as_str)
        .filter(|value| !value.is_empty()).ok_or_else(|| invalid("report instrument_id is required"))?;
    let timeframe = report.get("timeframe").and_then(Value::as_str)
        .filter(|value| crate::SIGNAL_SCREENER_TIMEFRAMES.contains(value))
        .ok_or_else(|| invalid("report timeframe is not supported"))?;
    let protocol_id = report.get("protocol_id").and_then(Value::as_str)
        .filter(|value| *value == super::PROTOCOL_ID)
        .ok_or_else(|| invalid("report protocol does not match the current entry validation protocol"))?;
    let computed_at = report.get("computed_at").and_then(Value::as_i64)
        .filter(|value| *value > 0).ok_or_else(|| invalid("report computed_at is required"))?;
    let registered = instrument(conn, instrument_id)?
        .ok_or_else(|| invalid("report instrument is not actively registered"))?;
    for key in ["exchange_id", "product_id", "asset_category_id", "symbol", "storage_symbol"] {
        if report.get("instrument").and_then(|value| value.get(key)) != registered.get(key) {
            return Err(invalid("report identity does not match the exact registered instrument"));
        }
    }
    let report_json = serde_json::to_string(report).map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
    if report_json.len() > MAX_REPORT_JSON_BYTES { return Err(invalid("entry validation report exceeds its bounded size")); }
    let summary = report_summary(report);
    let backtest_complete = report.get("status").and_then(Value::as_str) == Some("BACKTEST_COMPLETE");
    let has_validated_model = report.get("models").and_then(Value::as_array).is_some_and(|models| {
        models.iter().any(|model| model.get("entry_validation_status").and_then(Value::as_str) == Some("VALIDATED_HOLDOUT"))
    });
    conn.execute(
        "INSERT INTO app.entry_model_validation_reports
         (instrument_id,timeframe,protocol_id,computed_at,report_json,summary_json,backtest_complete,has_validated_model)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8)
         ON CONFLICT(instrument_id,timeframe) DO UPDATE SET protocol_id=excluded.protocol_id,
             computed_at=excluded.computed_at,report_json=excluded.report_json,summary_json=excluded.summary_json,
             backtest_complete=excluded.backtest_complete,has_validated_model=excluded.has_validated_model",
        params![instrument_id,timeframe,protocol_id,computed_at,report_json,summary.to_string(),backtest_complete,has_validated_model],
    )?;
    Ok(())
}

pub(super) fn get_report(conn: &Connection, instrument_id: &str, timeframe: &str) -> Result<Option<Value>, rusqlite::Error> {
    let encoded = conn.query_row(
        "SELECT report_json FROM app.entry_model_validation_reports WHERE instrument_id=?1 AND timeframe=?2",
        params![instrument_id,timeframe], |row| row.get::<_, String>(0),
    ).optional()?;
    encoded.map(decode_json).transpose()
}

#[derive(Clone, Debug)]
pub(super) struct PendingHistoryRecheck {
    pub(super) instrument_id: String,
    pub(super) timeframe: String,
    pub(super) queued_at: i64,
    pub(super) next_attempt_at: i64,
}

/// Queue only reports that were calculated before any candles existed. Market
/// sync calls this after it stores provider rows, so a report cannot stay
/// `NO_HISTORY` for the rest of the daily cycle after its first usable history
/// arrives. Shared crypto aliases are rechecked together by category.
pub(super) fn queue_no_history_rechecks(
    conn: &Connection,
    exchange_id: &str,
    product_id: &str,
    storage_symbol: &str,
    underlying_symbol: &str,
    asset_category_id: &str,
    queued_at: i64,
) -> Result<usize, rusqlite::Error> {
    let exact_queued = conn.execute(
        "INSERT OR IGNORE INTO app.entry_model_validation_recheck_queue
            (instrument_id,timeframe,queued_at,next_attempt_at)
         SELECT i.instrument_id,r.timeframe,?1,?1
           FROM app.instruments AS i
           JOIN app.entry_model_validation_reports AS r ON r.instrument_id=i.instrument_id
          WHERE r.protocol_id=?2
            AND json_extract(r.report_json,'$.status')='NO_HISTORY'
            AND i.is_active=1 AND i.venue_verified=1
            AND i.listing_status IN ('venue_active','public_listed_snapshot')
            AND i.exchange_id=?3 AND i.product_id=?4 AND i.storage_symbol=?5",
        params![
            queued_at,
            super::PROTOCOL_ID,
            exchange_id,
            product_id,
            storage_symbol,
        ],
    )?;
    if !matches!(asset_category_id, "crypto_spot" | "crypto_perpetual" | "crypto_futures")
        || underlying_symbol.trim().is_empty()
    {
        return Ok(exact_queued);
    }
    let shared_queued = conn.execute(
        "INSERT OR IGNORE INTO app.entry_model_validation_recheck_queue
            (instrument_id,timeframe,queued_at,next_attempt_at)
         SELECT i.instrument_id,r.timeframe,?1,?1
           FROM app.instruments AS i
           JOIN app.entry_model_validation_reports AS r ON r.instrument_id=i.instrument_id
          WHERE r.protocol_id=?2
            AND json_extract(r.report_json,'$.status')='NO_HISTORY'
            AND i.is_active=1 AND i.venue_verified=1
            AND i.listing_status IN ('venue_active','public_listed_snapshot')
            AND i.asset_category_id=?3
            AND upper(trim(i.underlying_symbol))=upper(trim(?4))",
        params![queued_at, super::PROTOCOL_ID, asset_category_id, underlying_symbol],
    )?;
    Ok(exact_queued.saturating_add(shared_queued))
}

/// Requeue current reports when newly stored history has reached the same
/// evidence-span minimum used by the backtest. A report that already had some
/// candles can otherwise remain `VALIDATION_PENDING` until the next daily
/// full-universe cycle even after progressive provider pagination completes.
pub(super) fn queue_mature_history_rechecks(
    conn: &Connection,
    exchange_id: &str,
    product_id: &str,
    storage_symbol: &str,
    underlying_symbol: &str,
    asset_category_id: &str,
    mature_timeframes: &[String],
    queued_at: i64,
) -> Result<usize, rusqlite::Error> {
    if mature_timeframes.is_empty() {
        return Ok(0);
    }
    let timeframes = serde_json::to_string(mature_timeframes)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
    let exact_queued = conn.execute(
        "INSERT OR IGNORE INTO app.entry_model_validation_recheck_queue
            (instrument_id,timeframe,queued_at,next_attempt_at)
         SELECT i.instrument_id,r.timeframe,?1,?1
           FROM app.instruments AS i
           JOIN app.entry_model_validation_reports AS r ON r.instrument_id=i.instrument_id
          WHERE r.protocol_id=?2
            AND json_extract(r.report_json,'$.status') IN ('BACKTEST_COMPLETE','VALIDATION_PENDING')
            AND EXISTS (SELECT 1 FROM json_each(?3) AS mature WHERE mature.value=r.timeframe)
            AND COALESCE(json_extract(r.report_json,'$.history.coverage_days'),0) < ?4
            AND i.is_active=1 AND i.venue_verified=1
            AND i.listing_status IN ('venue_active','public_listed_snapshot')
            AND i.exchange_id=?5 AND i.product_id=?6 AND i.storage_symbol=?7",
        params![
            queued_at,
            super::PROTOCOL_ID,
            timeframes,
            quant_core::ENTRY_SCORE_MIN_HISTORY_DAYS,
            exchange_id,
            product_id,
            storage_symbol,
        ],
    )?;
    if !matches!(asset_category_id, "crypto_spot" | "crypto_perpetual" | "crypto_futures")
        || underlying_symbol.trim().is_empty()
    {
        return Ok(exact_queued);
    }
    let shared_queued = conn.execute(
        "INSERT OR IGNORE INTO app.entry_model_validation_recheck_queue
            (instrument_id,timeframe,queued_at,next_attempt_at)
         SELECT i.instrument_id,r.timeframe,?1,?1
           FROM app.instruments AS i
           JOIN app.entry_model_validation_reports AS r ON r.instrument_id=i.instrument_id
          WHERE r.protocol_id=?2
            AND json_extract(r.report_json,'$.status') IN ('BACKTEST_COMPLETE','VALIDATION_PENDING')
            AND EXISTS (SELECT 1 FROM json_each(?3) AS mature WHERE mature.value=r.timeframe)
            AND COALESCE(json_extract(r.report_json,'$.history.coverage_days'),0) < ?4
            AND i.is_active=1 AND i.venue_verified=1
            AND i.listing_status IN ('venue_active','public_listed_snapshot')
            AND i.asset_category_id=?5
            AND upper(trim(i.underlying_symbol))=upper(trim(?6))",
        params![
            queued_at,
            super::PROTOCOL_ID,
            timeframes,
            quant_core::ENTRY_SCORE_MIN_HISTORY_DAYS,
            asset_category_id,
            underlying_symbol,
        ],
    )?;
    Ok(exact_queued.saturating_add(shared_queued))
}

pub(super) fn next_pending_history_recheck(
    conn: &Connection,
) -> Result<Option<PendingHistoryRecheck>, rusqlite::Error> {
    conn.query_row(
        "SELECT instrument_id,timeframe,queued_at,next_attempt_at
           FROM app.entry_model_validation_recheck_queue
          ORDER BY next_attempt_at,queued_at,instrument_id,timeframe LIMIT 1",
        [],
        |row| {
            Ok(PendingHistoryRecheck {
                instrument_id: row.get(0)?,
                timeframe: row.get(1)?,
                queued_at: row.get(2)?,
                next_attempt_at: row.get(3)?,
            })
        },
    )
    .optional()
}

pub(super) fn pending_history_recheck_count(conn: &Connection) -> Result<usize, rusqlite::Error> {
    conn.query_row(
        "SELECT COUNT(*) FROM app.entry_model_validation_recheck_queue",
        [],
        |row| row.get::<_, i64>(0),
    )
    .map(|count| usize::try_from(count).unwrap_or(usize::MAX))
}

pub(super) fn complete_history_recheck(
    conn: &Connection,
    instrument_id: &str,
    timeframe: &str,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "DELETE FROM app.entry_model_validation_recheck_queue
          WHERE instrument_id=?1 AND timeframe=?2",
        params![instrument_id, timeframe],
    )?;
    Ok(())
}

pub(super) fn defer_history_recheck(
    conn: &Connection,
    instrument_id: &str,
    timeframe: &str,
    next_attempt_at: i64,
    error: &str,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE app.entry_model_validation_recheck_queue
            SET attempt_count=attempt_count+1,next_attempt_at=?3,last_error=?4
          WHERE instrument_id=?1 AND timeframe=?2",
        params![
            instrument_id,
            timeframe,
            next_attempt_at,
            error.chars().take(500).collect::<String>(),
        ],
    )?;
    Ok(())
}

#[derive(Clone, Debug)]
pub(super) struct DailyRefreshState {
    pub(super) status: String,
    pub(super) cycle_started_at: i64,
    pub(super) cycle_completed_at: i64,
    pub(super) next_due_at: i64,
    pub(super) total_pairs: usize,
    pub(super) completed_pairs: usize,
    pub(super) failed_pairs: usize,
    pub(super) cursor: usize,
    pub(super) current_instrument_id: String,
    pub(super) current_timeframe: String,
    pub(super) current_label: String,
    pub(super) resume_instrument_id: String,
    pub(super) resume_timeframe: String,
    pub(super) last_failure_label: String,
    pub(super) last_error: String,
    pub(super) protocol_id: String,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct DailyRefreshControl {
    pub(super) paused: bool,
    pub(super) run_requested: bool,
}

pub(super) fn daily_refresh_control(
    conn: &Connection,
) -> Result<DailyRefreshControl, rusqlite::Error> {
    conn.query_row(
        "SELECT paused,run_requested FROM app.entry_model_validation_controls WHERE id=1",
        [],
        |row| Ok(DailyRefreshControl {
            paused: row.get::<_, i64>(0)? != 0,
            run_requested: row.get::<_, i64>(1)? != 0,
        }),
    )
}

pub(super) fn update_daily_refresh_control(
    conn: &Connection,
    action: &str,
    updated_at: i64,
) -> Result<(), rusqlite::Error> {
    match action {
        "pause" => {
            conn.execute(
                "UPDATE app.entry_model_validation_controls
                    SET paused=1,run_requested=0,updated_at=?1 WHERE id=1",
                [updated_at],
            )?;
        }
        "resume" => {
            conn.execute(
                "UPDATE app.entry_model_validation_controls
                    SET paused=0,updated_at=?1 WHERE id=1",
                [updated_at],
            )?;
        }
        "run_now" => {
            let state = daily_refresh_state(conn)?;
            let unfinished = state.cycle_started_at > state.cycle_completed_at
                && !matches!(state.status.as_str(), "idle" | "complete" | "complete_with_errors");
            let retry_after_error = state.status == "error";
            conn.execute(
                "UPDATE app.entry_model_validation_controls SET paused=0,
                 run_requested=CASE WHEN ?1 AND NOT ?2 THEN run_requested ELSE 1 END,
                 updated_at=?3 WHERE id=1",
                params![unfinished, retry_after_error, updated_at],
            )?;
        }
        _ => return Err(invalid("action must be pause, resume, or run_now")),
    }
    Ok(())
}

pub(super) fn consume_daily_refresh_request(
    conn: &Connection,
    updated_at: i64,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE app.entry_model_validation_controls SET run_requested=0,updated_at=?1
         WHERE id=1",
        [updated_at],
    )?;
    Ok(())
}

pub(super) fn daily_refresh_state(
    conn: &Connection,
) -> Result<DailyRefreshState, rusqlite::Error> {
    conn.query_row(
        "SELECT status,cycle_started_at,cycle_completed_at,next_due_at,total_pairs,
                completed_pairs,failed_pairs,cursor,current_instrument_id,current_timeframe,
                current_label,resume_instrument_id,resume_timeframe,last_failure_label,last_error,
                protocol_id
           FROM app.entry_model_validation_daily_refresh WHERE id=1",
        [],
        |row| Ok(DailyRefreshState {
            status: row.get(0)?, cycle_started_at: row.get(1)?,
            cycle_completed_at: row.get(2)?, next_due_at: row.get(3)?,
            total_pairs: row.get(4)?, completed_pairs: row.get(5)?,
            failed_pairs: row.get(6)?, cursor: row.get(7)?,
            current_instrument_id: row.get(8)?, current_timeframe: row.get(9)?,
            current_label: row.get(10)?, resume_instrument_id: row.get(11)?,
            resume_timeframe: row.get(12)?, last_failure_label: row.get(13)?,
            last_error: row.get(14)?, protocol_id: row.get(15)?,
        }),
    )
}

pub(super) fn daily_refresh_status(conn: &Connection) -> Result<Value, rusqlite::Error> {
    let state = daily_refresh_state(conn)?;
    let control = daily_refresh_control(conn)?;
    let unfinished = state.cycle_started_at > state.cycle_completed_at
        && !matches!(state.status.as_str(), "idle" | "complete" | "complete_with_errors");
    let status = if control.paused && unfinished {
        if state.status == "running" && !state.current_label.is_empty() {
            "pausing"
        } else {
            "paused"
        }
    } else if control.paused {
        "paused"
    } else {
        &state.status
    };
    let active = matches!(status, "running" | "waiting" | "pausing");
    let current = (!state.current_label.is_empty()).then(|| json!({
        "instrument_id":state.current_instrument_id,
        "timeframe":state.current_timeframe,
        "label":state.current_label,
    }));
    let pending_history_rechecks = pending_history_recheck_count(conn)?;
    Ok(json!({
        "status":status,"active":active,
        "control":{"paused":control.paused,"run_requested":control.run_requested},
        "cycle_started_at":(state.cycle_started_at > 0).then_some(state.cycle_started_at),
        "cycle_completed_at":(state.cycle_completed_at > 0).then_some(state.cycle_completed_at),
        "next_due_at":(state.next_due_at > 0).then_some(state.next_due_at),
        "total_pairs":state.total_pairs,"processed_pairs":state.cursor,
        "completed_pairs":state.completed_pairs,"failed_pairs":state.failed_pairs,
        "pending_history_rechecks":pending_history_rechecks,
        "progress_pct":if state.total_pairs == 0 {0.0} else {
            (state.cursor.min(state.total_pairs) as f64 / state.total_pairs as f64 * 100.0).clamp(0.0,100.0)
        },
        "current":current,"last_failure_label":state.last_failure_label,
        "last_error":state.last_error,
        "protocol_id":state.protocol_id,
    }))
}

pub(super) fn start_daily_refresh(
    conn: &Connection,
    started_at: i64,
    total_pairs: usize,
    next_due_at: i64,
    protocol_id: &str,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE app.entry_model_validation_daily_refresh SET status='running',
         cycle_started_at=?1,next_due_at=?2,total_pairs=?3,
         completed_pairs=0,failed_pairs=0,cursor=0,current_instrument_id='',
         current_timeframe='',current_label='',resume_instrument_id='',resume_timeframe='',
         last_failure_label='',last_error='',protocol_id=?4,updated_at=?1
         WHERE id=1",
        params![started_at, next_due_at, total_pairs as i64, protocol_id],
    )?;
    Ok(())
}

pub(super) fn set_daily_refresh_current(
    conn: &Connection,
    index: usize,
    instrument_id: &str,
    timeframe: &str,
    label: &str,
    updated_at: i64,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE app.entry_model_validation_daily_refresh SET status='running',cursor=?1,
         current_instrument_id=?2,current_timeframe=?3,current_label=?4,
         resume_instrument_id=?2,resume_timeframe=?3,updated_at=?5 WHERE id=1",
        params![index as i64, instrument_id, timeframe, label, updated_at],
    )?;
    Ok(())
}

pub(super) fn set_daily_refresh_total(
    conn: &Connection,
    total_pairs: usize,
    updated_at: i64,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE app.entry_model_validation_daily_refresh SET total_pairs=?1,
         cursor=MIN(cursor,?1),updated_at=?2 WHERE id=1",
        params![total_pairs as i64, updated_at],
    )?;
    Ok(())
}

pub(super) fn wait_daily_refresh(
    conn: &Connection,
    updated_at: i64,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE app.entry_model_validation_daily_refresh SET status='waiting',updated_at=?1
         WHERE id=1",
        [updated_at],
    )?;
    Ok(())
}

pub(super) fn advance_daily_refresh(
    conn: &Connection,
    next_index: usize,
    resume_target: (&str, &str),
    failed: Option<(&str, &str)>,
    updated_at: i64,
) -> Result<(), rusqlite::Error> {
    match failed {
        Some((label, error)) => {
            conn.execute(
                "UPDATE app.entry_model_validation_daily_refresh SET status='running',
                 cursor=?1,failed_pairs=failed_pairs+1,current_instrument_id='',
                 current_timeframe='',current_label='',last_failure_label=?2,last_error=?3,
                 resume_instrument_id=?4,resume_timeframe=?5,updated_at=?6 WHERE id=1",
                params![next_index as i64, label, error.chars().take(500).collect::<String>(),
                    resume_target.0, resume_target.1, updated_at],
            )?;
        }
        None => {
            conn.execute(
                "UPDATE app.entry_model_validation_daily_refresh SET status='running',
                 cursor=?1,completed_pairs=completed_pairs+1,current_instrument_id='',
                 current_timeframe='',current_label='',resume_instrument_id=?2,
                 resume_timeframe=?3,updated_at=?4 WHERE id=1",
                params![next_index as i64, resume_target.0, resume_target.1, updated_at],
            )?;
        }
    }
    Ok(())
}

pub(super) fn fail_daily_refresh_cycle(
    conn: &Connection,
    error: &str,
    retry_at: i64,
    updated_at: i64,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE app.entry_model_validation_daily_refresh SET status='error',
         last_error=?1,next_due_at=?2,updated_at=?3 WHERE id=1",
        params![error.chars().take(500).collect::<String>(), retry_at, updated_at],
    )?;
    Ok(())
}

pub(super) fn finish_daily_refresh(
    conn: &Connection,
    completed_at: i64,
    next_due_at: i64,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE app.entry_model_validation_daily_refresh SET
         status=CASE WHEN failed_pairs=0 THEN 'complete' ELSE 'complete_with_errors' END,
         cycle_completed_at=?1,next_due_at=?2,current_instrument_id='',
         current_timeframe='',current_label='',resume_instrument_id='',
         resume_timeframe='',updated_at=?1 WHERE id=1",
        params![completed_at, next_due_at],
    )?;
    Ok(())
}

pub(super) fn active_verified_instrument_ids(
    conn: &Connection,
) -> Result<Vec<String>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT instrument_id FROM app.instruments
          WHERE is_active=1 AND venue_verified=1
            AND listing_status IN ('venue_active','public_listed_snapshot')
          ORDER BY instrument_id",
    )?;
    let rows = statement.query_map([], |row| row.get(0))?;
    rows.collect()
}

fn products(conn: &Connection, exchange: &str) -> Result<Vec<Value>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT p.product_id,p.display_name FROM app.exchange_products p
           WHERE p.exchange_id=?1 AND p.status='enabled'
         UNION ALL SELECT DISTINCT i.product_id,i.product_id FROM app.instruments i
           WHERE i.exchange_id=?1 AND i.is_active=1 AND NOT EXISTS(
             SELECT 1 FROM app.exchange_products p WHERE p.exchange_id=i.exchange_id AND p.product_id=i.product_id)
         ORDER BY 1",
    )?;
    let rows = statement.query_map([exchange], |row| Ok(json!({"id":row.get::<_,String>(0)?,"label":row.get::<_,String>(1)?})))?
        .collect();
    rows
}

fn report_model_summary_projection(model_index: usize) -> String {
    let model = format!("$.models[{model_index}]");
    let rule = format!("{model}.backtest.weight_learning.risk_reward_rule");
    format!(
        "json_object(
            'entry_validation_status',json_extract(r.report_json,'{model}.entry_validation_status'),
            'risk_reward_protocol_id',json_extract(r.report_json,'{rule}.protocol_id'),
            'risk_reward_status',json_extract(r.report_json,'{rule}.status'),
            'risk_reward_qualified',CASE json_extract(r.report_json,'{rule}.candidate.qualified')
                WHEN 1 THEN json('true') WHEN 0 THEN json('false') END,
            'risk_reward_holdout_trades',json_extract(r.report_json,'{rule}.candidate.holdout.trades'),
            'risk_reward_validation_win_rate_pct',json_extract(r.report_json,'{rule}.candidate.validation.win_rate_after_fees_pct'),
            'risk_reward_holdout_win_rate_pct',json_extract(r.report_json,'{rule}.candidate.holdout.win_rate_after_fees_pct'),
            'risk_reward_holdout_payoff_ratio_after_fees',COALESCE(
                json_extract(r.report_json,'{rule}.candidate.holdout.payoff_ratio_after_fees'),
                json_extract(r.report_json,'{rule}.candidate.holdout.payoff_ratio')),
            'risk_reward_holdout_mean_return_pct',json_extract(r.report_json,'{rule}.candidate.holdout.mean_net_return_pct'))"
    )
}

pub(super) fn list(
    conn: &Connection, exchange: &str, product: Option<&str>, timeframe: &str,
    search: Option<&str>, offset: usize, limit: usize,
) -> Result<Value, rusqlite::Error> {
    let (filter, values) = scope_values(exchange, product, search);
    let mut count_values = values.clone();
    count_values.push(timeframe.to_string());
    let frame_parameter = count_values.len();
    count_values.push(super::PROTOCOL_ID.to_string());
    let protocol_parameter = count_values.len();
    let validation_scope = if timeframe == "4h" {
        count_values.push(quant_core::ENTRY_SCORE_RISK_REWARD_PROTOCOL_ID.to_string());
        let risk_reward_parameter = count_values.len();
        format!(
            " AND EXISTS (SELECT 1 FROM json_each(r.report_json,'$.models') AS model
                 WHERE json_extract(model.value,'$.entry_validation_status')='VALIDATED_HOLDOUT'
                   AND json_extract(model.value,'$.backtest.weight_learning.risk_reward_rule.protocol_id')=?{risk_reward_parameter}
                   AND json_extract(model.value,'$.backtest.weight_learning.risk_reward_rule.candidate.qualified')=1)"
        )
    } else {
        String::new()
    };
    let (total, backtested, validated) = conn.query_row(
        &format!("SELECT COUNT(*),
            COALESCE(SUM(CASE WHEN r.protocol_id=?{protocol_parameter} THEN r.backtest_complete ELSE 0 END),0),
            COALESCE(SUM(CASE WHEN r.protocol_id=?{protocol_parameter} AND r.has_validated_model=1{validation_scope} THEN 1 ELSE 0 END),0)
          FROM app.instruments i LEFT JOIN app.entry_model_validation_reports r
            ON r.instrument_id=i.instrument_id AND r.timeframe=?{frame_parameter}
          WHERE {filter}"),
        rusqlite::params_from_iter(count_values.iter()),
        |row| Ok((row.get::<_, usize>(0)?,row.get::<_, usize>(1)?,row.get::<_, usize>(2)?)),
    )?;
    let page_limit = limit.clamp(1, MAX_PAGE_ROWS);
    let page_offset = offset.min(total);
    let mut row_values = values;
    row_values.push(timeframe.to_string());
    let frame_parameter = row_values.len();
    let report_model_0 = report_model_summary_projection(0);
    let report_model_1 = report_model_summary_projection(1);
    let mut statement = conn.prepare(&format!(
        "SELECT {INSTRUMENT_COLUMNS},r.summary_json,r.protocol_id,
                json_extract(r.report_json,'$.models[0].backtest.holdout.win_rate_after_fees_pct'),
                json_extract(r.report_json,'$.models[0].backtest.holdout.payoff_ratio_after_fees'),
                json_extract(r.report_json,'$.models[0].backtest.holdout.mean_net_return_pct'),
                json_extract(r.report_json,'$.models[1].backtest.holdout.win_rate_after_fees_pct'),
                json_extract(r.report_json,'$.models[1].backtest.holdout.payoff_ratio_after_fees'),
                json_extract(r.report_json,'$.models[1].backtest.holdout.mean_net_return_pct'),
                {report_model_0},{report_model_1}
           FROM app.instruments i LEFT JOIN app.entry_model_validation_reports r
             ON r.instrument_id=i.instrument_id AND r.timeframe=?{frame_parameter}
           WHERE {filter} ORDER BY i.exchange_id,i.product_id,i.symbol,i.instrument_id
           LIMIT {page_limit} OFFSET {page_offset}"
    ))?;
    let rows = statement.query_map(rusqlite::params_from_iter(row_values.iter()), |row| {
        let mut instrument = read_instrument(row)?;
        let encoded: Option<String> = row.get(13)?;
        let protocol: Option<String> = row.get(14)?;
        // Keep old saved summaries honest after the after-fee display contract
        // changed, without loading the full report JSON into the response.
        let after_fee_metrics = [
            (
                row.get::<_, Option<f64>>(15)?,
                row.get::<_, Option<f64>>(16)?,
                row.get::<_, Option<f64>>(17)?,
            ),
            (
                row.get::<_, Option<f64>>(18)?,
                row.get::<_, Option<f64>>(19)?,
                row.get::<_, Option<f64>>(20)?,
            ),
        ];
        let report_model_details = [
            row.get::<_, Option<String>>(21)?
                .map(decode_json)
                .transpose()?
                .unwrap_or(Value::Null),
            row.get::<_, Option<String>>(22)?
                .map(decode_json)
                .transpose()?
                .unwrap_or(Value::Null),
        ];
        let mut evidence = encoded.map(decode_json).transpose()?.unwrap_or(Value::Null);
        if evidence.is_object() {
            evidence["protocol_current"] = json!(protocol.as_deref() == Some(super::PROTOCOL_ID));
            if let Some(models) = evidence["models"].as_array_mut() {
                for (index, model) in models.iter_mut().enumerate() {
                    if let Some((win_rate, payoff, mean_net_return)) = after_fee_metrics.get(index) {
                        model["holdout_win_rate_pct"] = json!(win_rate);
                        model["holdout_payoff_ratio"] = json!(payoff);
                        model["holdout_mean_return_pct"] = json!(mean_net_return);
                    }
                    let Some(details) = report_model_details.get(index).filter(|value| value.is_object()) else {
                        continue;
                    };
                    let risk_reward_protocol = details
                        .get("risk_reward_protocol_id")
                        .and_then(Value::as_str);
                    let risk_reward_is_current = risk_reward_protocol
                        == Some(quant_core::ENTRY_SCORE_RISK_REWARD_PROTOCOL_ID);
                    let risk_reward_qualified = details
                        .get("risk_reward_qualified")
                        .and_then(Value::as_bool)
                        == Some(true);
                    let risk_reward_blocks_validation =
                        super::four_hour_risk_reward_blocks_validation(
                            timeframe,
                            risk_reward_protocol,
                            risk_reward_qualified,
                        );
                    if risk_reward_blocks_validation {
                        model["entry_validation_status"] = json!("VALIDATION_PENDING");
                    } else if let Some(status) = details
                        .get("entry_validation_status")
                        .and_then(Value::as_str)
                    {
                        model["entry_validation_status"] = json!(status);
                    }
                    model["risk_reward_protocol_id"] =
                        json!(details.get("risk_reward_protocol_id").and_then(Value::as_str));
                    model["risk_reward_status"] = if risk_reward_is_current {
                        details.get("risk_reward_status").cloned().unwrap_or(Value::Null)
                    } else {
                        Value::Null
                    };
                    for field in [
                        "risk_reward_holdout_trades",
                        "risk_reward_validation_win_rate_pct",
                        "risk_reward_holdout_win_rate_pct",
                        "risk_reward_holdout_payoff_ratio_after_fees",
                        "risk_reward_holdout_mean_return_pct",
                    ] {
                        model[field] = if risk_reward_is_current {
                            details.get(field).cloned().unwrap_or(Value::Null)
                        } else {
                            Value::Null
                        };
                    }
                }
            }
        }
        instrument["evidence"] = evidence;
        Ok(instrument)
    })?.collect::<Result<Vec<_>, _>>()?;
    let rows = rows.into_iter().map(|mut row| {
        annotate_eligibility(conn, &mut row)?;
        Ok(row)
    }).collect::<Result<Vec<_>, rusqlite::Error>>()?;
    let next_offset = page_offset + rows.len();
    Ok(json!({
        "rows":rows,"total":total,"offset":page_offset,"limit":page_limit,
        "has_more":next_offset < total,"next_offset":next_offset,
        "summary":{"registered":total,"backtested":backtested,"validated":validated},
        "products":products(conn,exchange)?,
        "timeframes":crate::SIGNAL_SCREENER_TIMEFRAMES,
        "protocol_id":super::PROTOCOL_ID,
    }))
}

#[cfg(test)]
#[path = "entry_validation_store_tests.rs"]
mod tests;
