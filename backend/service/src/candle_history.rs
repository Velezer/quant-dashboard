use super::*;
mod aggregation;
pub(super) use aggregation::{
    aggregate_candles_from_base_source, aggregate_candles_from_base_source_with_cap,
    aggregate_candles_from_base_source_with_cap_and_range, aggregate_candles_from_source,
};
#[cfg(test)]
pub(super) use aggregation::required_aggregation_source_candles;

pub(super) fn timezone_offset_from_name_ns(timezone: &str) -> i64 {
    if timezone == "Asia/Jakarta" {
        quant_core::JAKARTA_UTC_OFFSET_SECONDS * NANOS_PER_SECOND
    } else {
        0
    }
}

pub(super) fn timezone_offset_ns_from_source(
    connection: &Connection,
    symbol: &str,
    interval: &str,
    source: &str,
) -> Result<i64, rusqlite::Error> {
    let timezone = connection
        .query_row(
            "SELECT timezone FROM series WHERE symbol=?1 AND interval=?2 AND source=?3",
            params![symbol, interval, source],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    Ok(timezone
        .as_deref()
        .map(timezone_offset_from_name_ns)
        .unwrap_or(0))
}

pub(super) fn derived_spec(timeframe: &str) -> Option<(&'static str, i64)> {
    let hour = quant_core::SECONDS_PER_HOUR * NANOS_PER_SECOND;
    let day = NANOS_PER_DAY;
    match timeframe {
        "2h" => Some(("1h", 2 * hour)),
        "4h" => Some(("1h", 4 * hour)),
        "6h" => Some(("1h", 6 * hour)),
        "12h" => Some(("1h", 12 * hour)),
        "2d" => Some(("1d", 2 * day)),
        "3d" => Some(("1d", 3 * day)),
        "4d" => Some(("1d", 4 * day)),
        "5d" => Some(("1d", 5 * day)),
        "6d" => Some(("1d", 6 * day)),
        "2w" => Some(("1d", 14 * day)),
        _ => None,
    }
}

pub(super) fn timeframe_duration_ns(timeframe: &str) -> Option<i64> {
    let minute = quant_core::SECONDS_PER_MINUTE * NANOS_PER_SECOND;
    let hour = quant_core::SECONDS_PER_HOUR * NANOS_PER_SECOND;
    let day = NANOS_PER_DAY;
    match timeframe {
        "15m" => Some(15 * minute),
        "30m" => Some(30 * minute),
        "1h" => Some(hour),
        "2h" => Some(2 * hour),
        "4h" => Some(4 * hour),
        "6h" => Some(6 * hour),
        "12h" => Some(12 * hour),
        "1d" => Some(day),
        "2d" => Some(2 * day),
        "3d" => Some(3 * day),
        "4d" => Some(4 * day),
        "5d" => Some(5 * day),
        "6d" => Some(6 * day),
        "1w" => Some(7 * day),
        "2w" => Some(14 * day),
        _ => None,
    }
}

pub(super) fn is_removed_candle_timeframe(timeframe: &str) -> bool {
    timeframe.trim().eq_ignore_ascii_case("5m")
}

/// Stored intervals that can construct the requested timeframe. The 4H
/// loader prefers 1H before consulting this list; other loaders choose the
/// listed source with more history unless a finer source is materially fresher.
pub(super) fn lower_timeframe_candidates(timeframe: &str) -> &'static [&'static str] {
    match timeframe {
        "1h" => &["15m"],
        "2h" | "4h" | "6h" | "12h" => &["1h", "15m"],
        "1d" => &["1h", "15m"],
        "1w" => &["1d", "1h", "15m"],
        "2d" | "3d" | "4d" | "5d" | "6d" | "2w" => &["1d", "1h", "15m"],
        _ => &[],
    }
}

pub(super) fn latest_stored_bar_time_from_source(
    connection: &Connection,
    symbol: &str,
    timeframe: &str,
    source: &str,
) -> Result<Option<i64>, rusqlite::Error> {
    connection.query_row(
        "SELECT MAX(ts) FROM bars WHERE symbol=?1 AND interval=?2 AND source=?3",
        params![symbol, timeframe, source],
        |row| row.get::<_, Option<i64>>(0),
    )
}

/// Pick the stored series that should supply a derived chart. The 4H chart
/// prefers stored 1H candles whenever they exist so its OHLC values are built
/// from hourly bars. Other native higher intervals remain authoritative. A
/// fallback-backed higher interval may be replaced or extended when a finer
/// stored series is materially newer, so a failed provider retry cannot pin
/// the chart to an old direct series.
#[cfg(test)]
pub(super) fn preferred_storage_timeframe(
    connection: &Connection,
    symbol: &str,
    requested_timeframe: &str,
) -> Result<String, rusqlite::Error> {
    let Some(source) = selected_candle_source(connection, symbol, None)? else {
        return Ok(requested_timeframe.to_string());
    };
    preferred_storage_timeframe_from_source(connection, symbol, requested_timeframe, &source)
}

pub(super) fn preferred_storage_timeframe_from_source(
    connection: &Connection,
    symbol: &str,
    requested_timeframe: &str,
    source: &str,
) -> Result<String, rusqlite::Error> {
    if requested_timeframe == "4h"
        && latest_stored_bar_time_from_source(connection, symbol, "1h", source)?.is_some()
    {
        return Ok("1h".to_string());
    }

    let direct_last =
        latest_stored_bar_time_from_source(connection, symbol, requested_timeframe, source)?;
    let direct_source = connection
        .query_row(
            "SELECT source FROM series WHERE symbol=?1 AND interval=?2 AND source=?3",
            params![symbol, requested_timeframe, source],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let direct_is_native = direct_source
        .as_deref()
        .is_some_and(|source| !is_research_proxy_source(source));
    let should_check_lower = direct_last.is_none() || !direct_is_native;
    if !should_check_lower {
        return Ok(requested_timeframe.to_string());
    }

    let direct_is_empty = direct_last.is_none();
    let direct_duration = timeframe_duration_ns(requested_timeframe).unwrap_or(i64::MAX);
    let mut best: Option<(&str, i64, i64)> = None;
    for candidate in lower_timeframe_candidates(requested_timeframe) {
        let Some(last) = latest_stored_bar_time_from_source(connection, symbol, candidate, source)?
        else {
            continue;
        };
        if let Some(direct) = direct_last {
            // Daily charts may append a newer partial day. Other fallback
            // charts switch to a lower source only after it is at least one
            // requested candle newer than the direct series.
            if requested_timeframe != "1d" && last.saturating_sub(direct) < direct_duration {
                continue;
            }
        }
        let candidate_duration = timeframe_duration_ns(candidate).unwrap_or(i64::MAX);
        let is_better = best
            .map(|(_, best_last, best_duration)| {
                last > best_last && last.saturating_sub(best_last) >= best_duration
            })
            .unwrap_or(true);
        if is_better {
            best = Some((candidate, last, candidate_duration));
        }
    }

    if direct_is_empty {
        if let Some((candidate, _, _)) = best {
            return Ok(candidate.to_string());
        }
    } else if let Some((candidate, _, _)) = best {
        return Ok(candidate.to_string());
    }
    Ok(requested_timeframe.to_string())
}

pub(super) fn load_series_candles(
    connection: &Connection,
    symbol: &str,
    timeframe: &str,
    limit: i64,
) -> Result<Vec<Candle>, rusqlite::Error> {
    load_series_candles_from_selected_source(
        connection,
        symbol,
        timeframe,
        limit,
        selected_candle_source(connection, symbol, None)?,
    )
}

pub(super) fn load_series_candles_for_exchange(
    connection: &Connection,
    symbol: &str,
    timeframe: &str,
    limit: i64,
    exchange: &str,
) -> Result<Vec<Candle>, rusqlite::Error> {
    load_series_candles_from_selected_source(
        connection,
        symbol,
        timeframe,
        limit,
        selected_candle_source_for_timeframe(connection, symbol, Some(exchange), timeframe)?,
    )
}

#[cfg(test)]
pub(super) fn load_series_candles_for_product(
    connection: &Connection,
    symbol: &str,
    timeframe: &str,
    limit: i64,
    exchange: &str,
    product_id: &str,
) -> Result<Vec<Candle>, rusqlite::Error> {
    load_series_candles_from_selected_source(
        connection,
        symbol,
        timeframe,
        limit,
        selected_candle_source_for_product(connection, symbol, exchange, product_id, timeframe)?,
    )
}

fn load_series_candles_from_selected_source(
    connection: &Connection,
    symbol: &str,
    timeframe: &str,
    limit: i64,
    source: Option<String>,
) -> Result<Vec<Candle>, rusqlite::Error> {
    match source {
        Some(source) => {
            load_series_candles_from_source(connection, symbol, timeframe, limit, &source)
        }
        None => Ok(Vec::new()),
    }
}

pub(super) fn load_series_candles_from_source(
    connection: &Connection,
    symbol: &str,
    timeframe: &str,
    limit: i64,
    source: &str,
) -> Result<Vec<Candle>, rusqlite::Error> {
    load_series_candles_from_source_with_cap(
        connection,
        symbol,
        timeframe,
        limit,
        source,
        MAX_AGGREGATION_SOURCE_CANDLES,
    )
}

pub(super) fn load_series_candles_from_source_with_cap(
    connection: &Connection,
    symbol: &str,
    timeframe: &str,
    limit: i64,
    source: &str,
    source_cap: i64,
) -> Result<Vec<Candle>, rusqlite::Error> {
    let storage_timeframe =
        preferred_storage_timeframe_from_source(connection, symbol, timeframe, source)?;
    if timeframe == "4h" && storage_timeframe == "1h" {
        return aggregate_candles_from_base_source_with_cap(
            connection, symbol, timeframe, "1h", limit, source, source_cap,
        );
    }

    let direct = load_candles_from_source(connection, symbol, timeframe, limit, source)?;
    if storage_timeframe != timeframe {
        let derived = aggregate_candles_from_base_source_with_cap(
            connection,
            symbol,
            timeframe,
            &storage_timeframe,
            limit,
            source,
            source_cap,
        )?;
        if !derived.is_empty() {
            // A selected lower source is either filling a missing direct
            // series or is materially newer than a fallback direct series.
            // Preserve older direct history before the rollup boundary.
            let should_merge = direct.is_empty()
                || derived.last().map(|bar| bar.time) >= direct.last().map(|bar| bar.time);
            if should_merge {
                let first_derived = derived.first().map(|bar| bar.time).unwrap_or(i64::MAX);
                let mut merged: Vec<Candle> = direct
                    .into_iter()
                    .filter(|bar| bar.time < first_derived)
                    .collect();
                merged.extend(derived);
                if merged.len() > limit as usize {
                    let start = merged.len() - limit as usize;
                    merged.drain(..start);
                }
                return Ok(merged);
            }
        }
    }
    if !direct.is_empty() {
        Ok(direct)
    } else if derived_spec(timeframe).is_some() {
        aggregate_candles_from_source(connection, symbol, timeframe, limit, source)
    } else {
        Ok(direct)
    }
}

pub(super) fn load_series_candles_from_source_between_with_cap(
    connection: &Connection,
    symbol: &str,
    timeframe: &str,
    limit: i64,
    source: &str,
    source_cap: i64,
    from_time: i64,
    to_time_exclusive: i64,
) -> Result<Vec<Candle>, rusqlite::Error> {
    if limit <= 0 || from_time >= to_time_exclusive {
        return Ok(Vec::new());
    }
    let range = Some((from_time, to_time_exclusive));
    let storage_timeframe =
        preferred_storage_timeframe_from_source(connection, symbol, timeframe, source)?;
    if timeframe == "4h" && storage_timeframe == "1h" {
        return aggregate_candles_from_base_source_with_cap_and_range(
            connection, symbol, timeframe, "1h", limit, source, source_cap, range,
        );
    }
    let direct = load_candles_from_source_between(
        connection,
        symbol,
        timeframe,
        limit,
        source,
        from_time,
        to_time_exclusive,
    )?;
    if storage_timeframe != timeframe {
        let derived = aggregate_candles_from_base_source_with_cap_and_range(
            connection,
            symbol,
            timeframe,
            &storage_timeframe,
            limit,
            source,
            source_cap,
            range,
        )?;
        if !derived.is_empty() {
            let should_merge = direct.is_empty()
                || derived.last().map(|bar| bar.time) >= direct.last().map(|bar| bar.time);
            if should_merge {
                let first_derived = derived.first().map(|bar| bar.time).unwrap_or(i64::MAX);
                let mut merged = direct
                    .into_iter()
                    .filter(|bar| bar.time < first_derived)
                    .collect::<Vec<_>>();
                merged.extend(derived);
                if merged.len() > limit as usize {
                    merged.drain(..merged.len() - limit as usize);
                }
                return Ok(merged);
            }
        }
    }
    if !direct.is_empty() {
        Ok(direct)
    } else if let Some((base, _)) = derived_spec(timeframe) {
        aggregate_candles_from_base_source_with_cap_and_range(
            connection, symbol, timeframe, base, limit, source, source_cap, range,
        )
    } else {
        Ok(direct)
    }
}

pub(super) fn freshness_threshold_seconds(symbol: &str, timeframe: &str) -> i64 {
    let day = SECONDS_PER_DAY;
    let is_idx = symbol.ends_with(".JK");
    match timeframe {
        // A completed 15m candle is normally at most one interval old by its
        // open timestamp. Allow one missed interval, then withhold signals and
        // show the chart as stale, including when a market is closed.
        "15m" => i64::try_from(super::SIGNAL_REFRESH_INTERVAL_SECONDS).unwrap_or(900) * 2,
        "30m" => {
            if is_idx {
                3 * day
            } else {
                3 * quant_core::SECONDS_PER_HOUR
            }
        }
        "1h" => {
            if is_idx {
                5 * day
            } else {
                6 * quant_core::SECONDS_PER_HOUR
            }
        }
        "2h" => freshness_threshold_seconds(symbol, "4h"),
        "4h" => {
            if is_idx {
                5 * day
            } else {
                18 * quant_core::SECONDS_PER_HOUR
            }
        }
        "6h" => {
            if is_idx {
                5 * day
            } else {
                day
            }
        }
        "12h" => {
            if is_idx {
                5 * day
            } else {
                36 * quant_core::SECONDS_PER_HOUR
            }
        }
        "1d" | "2d" | "3d" | "4d" | "5d" | "6d" => {
            if is_idx {
                3 * day
            } else {
                4 * day
            }
        }
        "1w" | "2w" => 21 * day,
        _ => 3 * day,
    }
}

pub(super) fn completed_bar_is_fresh_at(
    symbol: &str,
    timeframe: &str,
    bar_open_time_ns: i64,
    now_ns: i64,
) -> bool {
    let Some(duration_ns) = timeframe_duration_ns(timeframe) else {
        return false;
    };
    if bar_open_time_ns > now_ns || bar_open_time_ns.saturating_add(duration_ns) > now_ns {
        return false;
    }
    let maximum_age_ns = freshness_threshold_seconds(symbol, timeframe)
        .max(0)
        .saturating_mul(NANOS_PER_SECOND);
    now_ns.saturating_sub(bar_open_time_ns) <= maximum_age_ns
}

pub(super) fn history_target_days(_timeframe: &str) -> i64 {
    MIN_ENGINE_HISTORY_DAYS
}

pub(super) fn history_coverage_days(first_ts: Option<i64>, last_ts: Option<i64>) -> i64 {
    match (first_ts, last_ts) {
        (Some(first), Some(last)) => last.saturating_sub(first).max(0).div_euclid(NANOS_PER_DAY),
        _ => 0,
    }
}

pub(super) fn daily_history_coverage_days(
    first_ts: Option<i64>,
    last_ts: Option<i64>,
    timezone: &str,
) -> i64 {
    match (first_ts, last_ts) {
        (Some(first), Some(last)) => {
            let offset = timezone_offset_from_name_ns(timezone);
            let first_day = first.saturating_add(offset).div_euclid(NANOS_PER_DAY);
            let last_day = last.saturating_add(offset).div_euclid(NANOS_PER_DAY);
            last_day.saturating_sub(first_day).max(0)
        }
        _ => 0,
    }
}

pub(super) fn history_coverage_status(rows: i64, coverage_days: i64) -> &'static str {
    if rows <= 0 {
        "MISSING"
    } else if coverage_days >= MIN_ENGINE_HISTORY_DAYS {
        "TARGET_MET"
    } else {
        "UNDER_TARGET"
    }
}

pub(super) fn history_acquisition_policy(timeframe: &str) -> &'static str {
    match timeframe {
        "1d" | "1w" => "provider_backfill_then_local_retention",
        "15m" | "30m" | "1h" | "2h" | "4h" | "6h" | "12h" => "local_accumulation_or_native_feed",
        _ => "local_retention",
    }
}

pub(super) fn history_coverage_warning(
    timeframe: &str,
    rows: i64,
    coverage_days: i64,
) -> Option<String> {
    if rows <= 0 || coverage_days >= MIN_ENGINE_HISTORY_DAYS {
        return None;
    }
    let target = MIN_ENGINE_HISTORY_DAYS;
    if matches!(timeframe, "15m" | "30m" | "1h" | "2h" | "4h" | "6h" | "12h") {
        Some(format!(
            "Stored {coverage_days}/{target} calendar days. Intraday fallback history is provider-window limited and must accumulate locally or come from a native app feed."
        ))
    } else {
        Some(format!(
            "Stored {coverage_days}/{target} calendar days. Run the selected asset sync when its compatible provider is reachable to extend local history."
        ))
    }
}

pub(super) fn daily_series_span_from_source(
    connection: &Connection,
    symbol: &str,
    source: &str,
) -> Result<(i64, Option<i64>, Option<i64>), rusqlite::Error> {
    let offset = timezone_offset_ns_from_source(connection, symbol, "1d", source)?;
    connection.query_row(
        "SELECT COUNT(*), MIN(b.ts), MAX(b.ts)
         FROM bars b
         WHERE b.symbol=?1 AND b.interval='1d' AND b.source=?4
           AND NOT EXISTS (
               SELECT 1 FROM bars newer
               WHERE newer.symbol=b.symbol AND newer.interval=b.interval
                 AND newer.source=b.source
                 AND newer.ts>b.ts
                 AND newer.ts<((b.ts+?2)/?3+1)*?3-?2
           )",
        params![symbol, offset, NANOS_PER_DAY, source],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )
}

/// Describe the local history behind one requested chart timeframe. Derived
/// timeframes report the lower-resolution series that supplies their bars.
#[path = "candle_history_metadata.rs"]
mod metadata;
pub(super) use metadata::{
    load_product_candle_metadata, load_series_metadata, load_series_metadata_for_exchange,
    load_series_metadata_from_source, product_candle_scan_state_at,
    product_candle_scan_state_at_with_preferred_source, trim_candles, ProductCandleScanState,
};
