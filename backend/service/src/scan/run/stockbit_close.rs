use super::super::*;

#[derive(Clone)]
pub(super) struct StockbitCloseScanCheckpoint {
    product_id: String,
    asset_category_id: String,
    symbol: String,
    session_day: i64,
    final_bar_time: i64,
    source: String,
    input_fingerprint: String,
}


pub(super) fn expected_stockbit_closed_session_day(now_seconds: i64) -> Option<i64> {
    let local_seconds = now_seconds.saturating_add(quant_core::JAKARTA_UTC_OFFSET_SECONDS);
    let day = local_seconds.div_euclid(quant_core::SECONDS_PER_DAY);
    let weekday = (day + 4).rem_euclid(7); // Sunday = 0, Monday = 1.
    let second_of_day = local_seconds.rem_euclid(quant_core::SECONDS_PER_DAY);
    if (1..=5).contains(&weekday) {
        (second_of_day >= 16 * 60 * 60 + 15 * 60).then_some(day)
    } else if weekday == 6 {
        Some(day - 1)
    } else if weekday == 0 {
        Some(day - 2)
    } else {
        None
    }
}

#[cfg(not(test))]
pub(super) fn ensure_stockbit_close_checkpoint_schema(
    connection: &Connection,
) -> Result<(), rusqlite::Error> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS signals.stockbit_close_scan_checkpoints (
             product_id TEXT NOT NULL,
             asset_category_id TEXT NOT NULL,
             symbol TEXT NOT NULL,
             session_day INTEGER NOT NULL,
             final_bar_time INTEGER NOT NULL,
             source TEXT NOT NULL,
             input_fingerprint TEXT NOT NULL,
             scanned_at INTEGER NOT NULL,
             PRIMARY KEY(product_id,asset_category_id,symbol)
         ) WITHOUT ROWID;",
    )
}

pub(super) fn is_final_stockbit_close_bar(
    final_bar_time: i64,
    session_day: i64,
    now_seconds: i64,
) -> bool {
    let bar_seconds = final_bar_time.div_euclid(NANOS_PER_SECOND);
    let local_bar_seconds = bar_seconds.saturating_add(quant_core::JAKARTA_UTC_OFFSET_SECONDS);
    let second_of_day = local_bar_seconds.rem_euclid(quant_core::SECONDS_PER_DAY);
    local_bar_seconds.div_euclid(quant_core::SECONDS_PER_DAY) == session_day
        && (second_of_day == 15 * 60 * 60 + 45 * 60 || second_of_day == 16 * 60 * 60)
        && bar_seconds.saturating_add(15 * 60) <= now_seconds
}

#[cfg(not(test))]
pub(super) fn stockbit_closed_session_checkpoint(
    connection: &Connection,
    storage_symbol: &str,
    exchange_id: &str,
    product_id: &str,
    asset_category_id: &str,
    symbol: &str,
    now_seconds: i64,
) -> Result<Option<StockbitCloseScanCheckpoint>, rusqlite::Error> {
    let Some(session_day) = expected_stockbit_closed_session_day(now_seconds) else {
        return Ok(None);
    };
    let Some(state) = product_candle_scan_state_at(
        connection, storage_symbol, exchange_id, product_id, "15m", now_seconds,
    )? else {
        return Ok(None);
    };
    let Some(final_bar_time) = state.latest_bar_time else {
        return Ok(None);
    };
    if !is_final_stockbit_close_bar(final_bar_time, session_day, now_seconds) {
        return Ok(None);
    }

    let latest = connection
        .query_row(
            "SELECT ts,open,high,low,close,volume FROM bars
              WHERE symbol=?1 AND interval='15m' AND source=?2
              ORDER BY ts DESC LIMIT 1",
            params![storage_symbol, state.source],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, f64>(1)?,
                    row.get::<_, f64>(2)?,
                    row.get::<_, f64>(3)?,
                    row.get::<_, f64>(4)?,
                    row.get::<_, f64>(5)?,
                ))
            },
        )
        .optional()?;
    let Some((timestamp, open, high, low, close, volume)) = latest else {
        return Ok(None);
    };
    if timestamp != final_bar_time
        || !open.is_finite()
        || !high.is_finite()
        || !low.is_finite()
        || !close.is_finite()
        || !volume.is_finite()
        || open <= 0.0
        || high <= 0.0
        || low <= 0.0
        || close <= 0.0
        || volume < 0.0
        || high < low
    {
        return Ok(None);
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::hash::Hash::hash(&state.source, &mut hasher);
    std::hash::Hash::hash(&state.data_provider, &mut hasher);
    std::hash::Hash::hash(&timestamp, &mut hasher);
    std::hash::Hash::hash(&open.to_bits(), &mut hasher);
    std::hash::Hash::hash(&high.to_bits(), &mut hasher);
    std::hash::Hash::hash(&low.to_bits(), &mut hasher);
    std::hash::Hash::hash(&close.to_bits(), &mut hasher);
    std::hash::Hash::hash(&volume.to_bits(), &mut hasher);
    // An unchanged closing candle can still produce a different score after
    // older or higher-timeframe history is refreshed. Include series revisions
    // so the close checkpoint only suppresses a scan when all stored inputs
    // remain unchanged, without hashing every candle on the scan hot path.
    let mut statement = connection.prepare(
        "SELECT interval,source,data_provider,last_success FROM series
          WHERE symbol=?1 AND exchange_id=?2 AND (product_id=?3 OR product_id='')
          ORDER BY interval,source",
    )?;
    let mut rows = statement.query(params![storage_symbol, exchange_id, product_id])?;
    while let Some(row) = rows.next()? {
        let interval = row.get::<_, String>(0)?;
        let source = row.get::<_, String>(1)?;
        let provider = row.get::<_, String>(2)?;
        let last_success = row.get::<_, Option<String>>(3)?;
        std::hash::Hash::hash(&interval, &mut hasher);
        std::hash::Hash::hash(&source, &mut hasher);
        std::hash::Hash::hash(&provider, &mut hasher);
        std::hash::Hash::hash(&last_success, &mut hasher);
    }
    Ok(Some(StockbitCloseScanCheckpoint {
        product_id: product_id.to_string(),
        asset_category_id: asset_category_id.to_string(),
        symbol: symbol.to_string(),
        session_day,
        final_bar_time,
        source: state.source,
        input_fingerprint: format!("{:016x}", std::hash::Hasher::finish(&hasher)),
    }))
}

#[cfg(not(test))]
pub(super) fn stockbit_close_checkpoint_is_saved(
    connection: &Connection,
    checkpoint: &StockbitCloseScanCheckpoint,
) -> Result<bool, rusqlite::Error> {
    connection.query_row(
        "SELECT EXISTS(
             SELECT 1 FROM signals.stockbit_close_scan_checkpoints
              WHERE product_id=?1 AND asset_category_id=?2 AND symbol=?3
                AND session_day=?4 AND final_bar_time=?5 AND source=?6
                AND input_fingerprint=?7
         )",
        params![
            checkpoint.product_id,
            checkpoint.asset_category_id,
            checkpoint.symbol,
            checkpoint.session_day,
            checkpoint.final_bar_time,
            checkpoint.source,
            checkpoint.input_fingerprint,
        ],
        |row| row.get(0),
    )
}

#[cfg(not(test))]
pub(super) fn save_stockbit_close_scan_checkpoint(
    connection: &Connection,
    checkpoint: &StockbitCloseScanCheckpoint,
) -> Result<(), rusqlite::Error> {
    connection.execute(
        "INSERT INTO signals.stockbit_close_scan_checkpoints(
             product_id,asset_category_id,symbol,session_day,final_bar_time,source,
             input_fingerprint,scanned_at
         ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)
         ON CONFLICT(product_id,asset_category_id,symbol) DO UPDATE SET
             session_day=excluded.session_day, final_bar_time=excluded.final_bar_time,
             source=excluded.source, input_fingerprint=excluded.input_fingerprint,
             scanned_at=excluded.scanned_at",
        params![
            checkpoint.product_id,
            checkpoint.asset_category_id,
            checkpoint.symbol,
            checkpoint.session_day,
            checkpoint.final_bar_time,
            checkpoint.source,
            checkpoint.input_fingerprint,
            now_secs() as i64,
        ],
    )?;
    Ok(())
}
