use super::*;

pub(crate) fn aggregate_candles_from_base_source(
    connection: &Connection,
    symbol: &str,
    timeframe: &str,
    base: &str,
    limit: i64,
    source_id: &str,
) -> Result<Vec<Candle>, rusqlite::Error> {
    aggregate_candles_from_base_source_with_cap(
        connection,
        symbol,
        timeframe,
        base,
        limit,
        source_id,
        MAX_AGGREGATION_SOURCE_CANDLES,
    )
}

pub(crate) fn aggregate_candles_from_base_source_with_cap(
    connection: &Connection,
    symbol: &str,
    timeframe: &str,
    base: &str,
    limit: i64,
    source_id: &str,
    source_cap: i64,
) -> Result<Vec<Candle>, rusqlite::Error> {
    aggregate_candles_from_base_source_with_cap_and_range(
        connection, symbol, timeframe, base, limit, source_id, source_cap, None,
    )
}

pub(crate) fn aggregate_candles_from_base_source_with_cap_and_range(
    connection: &Connection,
    symbol: &str,
    timeframe: &str,
    base: &str,
    limit: i64,
    source_id: &str,
    source_cap: i64,
    range: Option<(i64, i64)>,
) -> Result<Vec<Candle>, rusqlite::Error> {
    if limit <= 0 {
        return Ok(Vec::new());
    }
    let Some(duration_ns) = timeframe_duration_ns(timeframe) else {
        return match range {
            Some((from_time, to_time)) => load_candles_from_source_between(
                connection, symbol, timeframe, limit, source_id, from_time, to_time,
            ),
            None => load_candles_from_source(connection, symbol, timeframe, limit, source_id),
        };
    };
    let Some(base_duration_ns) = timeframe_duration_ns(base) else {
        return Ok(Vec::new());
    };
    if duration_ns <= base_duration_ns {
        return Ok(Vec::new());
    }
    let require_complete_30m = timeframe == "30m" && base == "15m";
    let expected_source_bars = duration_ns / base_duration_ns;
    let source_limit =
        aggregation_source_candle_limit_with_cap(duration_ns, base_duration_ns, limit, source_cap);
    let offset = timezone_offset_ns_from_source(connection, symbol, base, source_id)?;
    let day_ns = NANOS_PER_DAY;
    let is_intraday = duration_ns < day_ns;
    let bucket_days = (duration_ns / day_ns).max(1);
    // Daily resampling uses the calendar day. Multi-day rules use the first
    // stored source day so paging the local history does not change buckets;
    // weekly rules are anchored to Monday (1970-01-05).
    let origin_day = if !is_intraday && timeframe != "1d" && timeframe != "1w" && timeframe != "2w"
    {
        connection
            .query_row(
                "SELECT MIN(ts) FROM bars WHERE symbol=?1 AND interval=?2 AND source=?3",
                params![symbol, base, source_id],
                |row| row.get::<_, Option<i64>>(0),
            )?
            .map(|timestamp| timestamp.saturating_add(offset).div_euclid(day_ns))
            .unwrap_or(0)
    } else {
        0
    };
    let source_range = range.map(|(from_time, to_time)| {
        (from_time.saturating_sub(duration_ns), to_time)
    });
    let Some(first_source_time) = first_source_time(
        connection,
        symbol,
        base,
        source_id,
        source_limit,
        source_range,
        offset,
        day_ns,
    )? else {
        return Ok(Vec::new());
    };
    let anchor_days = if timeframe == "1w" || timeframe == "2w" {
        4
    } else {
        origin_day
    };
    // Keep only the requested output bars plus one bucket for an incomplete
    // trailing 30m candle. Source rows are consumed directly from SQLite.
    let retained_limit = (limit as usize).saturating_add(1);
    let mut aggregated = std::collections::VecDeque::<Candle>::with_capacity(
        retained_limit.min(1_001),
    );
    let mut current_bucket: Option<i64> = None;
    let mut current_source_count = 0_i64;
    let mut current_source_contiguous = true;
    let mut previous_source_time: Option<i64> = None;
    for_each_source_candle(
        connection,
        symbol,
        base,
        source_id,
        first_source_time,
        source_range,
        offset,
        day_ns,
        |bar| {
            let local_ts = bar.time.saturating_add(offset);
            let local_bucket = if is_intraday {
                intraday_aggregation_bucket_start(local_ts, duration_ns, symbol)
            } else if timeframe == "1d" || timeframe == "1w" {
                let day_index = local_ts.div_euclid(day_ns);
                if timeframe == "1w" {
                    (day_index - anchor_days).div_euclid(7) * 7 * day_ns + anchor_days * day_ns
                } else {
                    day_index * day_ns
                }
            } else {
                let day_index = local_ts.div_euclid(day_ns);
                (day_index - anchor_days).div_euclid(bucket_days) * bucket_days * day_ns
                    + anchor_days * day_ns
            };
            let bucket = local_bucket.saturating_sub(offset);
            if current_bucket != Some(bucket) {
                if require_complete_30m
                    && current_bucket.is_some()
                    && (current_source_count != expected_source_bars || !current_source_contiguous)
                {
                    aggregated.pop_back();
                }
                aggregated.push_back(new_candle(
                    bucket, bar.open, bar.high, bar.low, bar.close, bar.volume,
                ));
                current_bucket = Some(bucket);
                current_source_count = 1;
                current_source_contiguous = bar.time == bucket;
                if aggregated.len() > retained_limit {
                    aggregated.pop_front();
                }
            } else if let Some(last) = aggregated.back_mut() {
                if require_complete_30m {
                    current_source_count += 1;
                    current_source_contiguous &= previous_source_time
                        .is_some_and(|previous| bar.time == previous.saturating_add(base_duration_ns));
                }
                last.high = last.high.max(bar.high);
                last.low = last.low.min(bar.low);
                last.close = bar.close;
                last.volume += bar.volume;
            }
            previous_source_time = Some(bar.time);
        },
    )?;
    if require_complete_30m
        && current_bucket.is_some()
        && (current_source_count != expected_source_bars || !current_source_contiguous)
    {
        aggregated.pop_back();
    }
    let mut aggregated = aggregated.into_iter().collect::<Vec<_>>();
    if aggregated.len() > limit as usize {
        let start = aggregated.len() - limit as usize;
        aggregated.drain(..start);
    }
    if let Some((from_time, to_time)) = range {
        aggregated.retain(|bar| bar.time >= from_time && bar.time < to_time);
    }
    Ok(aggregated)
}

#[cfg(test)]
pub(crate) fn required_aggregation_source_candles(
    timeframe: &str,
    base: &str,
    limit: i64,
) -> Option<i64> {
    let duration_ns = timeframe_duration_ns(timeframe)?;
    let base_duration_ns = timeframe_duration_ns(base)?;
    (duration_ns > base_duration_ns).then(|| {
        aggregation_source_candle_limit_with_cap(
            duration_ns,
            base_duration_ns,
            limit,
            MAX_AGGREGATION_SOURCE_CANDLES,
        )
    })
}

fn intraday_aggregation_bucket_start(local_time_ns: i64, duration_ns: i64, symbol: &str) -> i64 {
    let day_ns = NANOS_PER_DAY;
    let day_start = local_time_ns.div_euclid(day_ns) * day_ns;
    let session_anchored = symbol.ends_with(".JK")
        && duration_ns >= 2 * quant_core::SECONDS_PER_HOUR * NANOS_PER_SECOND;
    let anchor = if session_anchored {
        day_start + 9 * quant_core::SECONDS_PER_HOUR * NANOS_PER_SECOND
    } else {
        day_start
    };
    anchor + (local_time_ns - anchor).div_euclid(duration_ns) * duration_ns
}

#[cfg(test)]
mod intraday_bucket_tests {
    use super::*;

    #[test]
    fn idx_higher_timeframes_start_at_the_opening_session() {
        let hour = quant_core::SECONDS_PER_HOUR * NANOS_PER_SECOND;
        let four_hours = 4 * hour;
        assert_eq!(intraday_aggregation_bucket_start(9 * hour, four_hours, "BBCA.JK"), 9 * hour);
        assert_eq!(intraday_aggregation_bucket_start(13 * hour, four_hours, "BBCA.JK"), 13 * hour);
        assert_eq!(intraday_aggregation_bucket_start(16 * hour, four_hours, "BBCA.JK"), 13 * hour);
    }

    #[test]
    fn crypto_higher_timeframes_remain_utc_anchored() {
        let hour = quant_core::SECONDS_PER_HOUR * NANOS_PER_SECOND;
        let fifteen_minutes = 15 * quant_core::SECONDS_PER_MINUTE * NANOS_PER_SECOND;
        assert_eq!(intraday_aggregation_bucket_start(fifteen_minutes, 4 * hour, "BTCUSDT"), 0);
    }
}

fn first_source_time(
    connection: &Connection,
    symbol: &str,
    base: &str,
    source_id: &str,
    source_limit: i64,
    range: Option<(i64, i64)>,
    offset: i64,
    day_ns: i64,
) -> Result<Option<i64>, rusqlite::Error> {
    let daily = base == "1d";
    let latest = match (daily, range) {
        (false, None) => connection
            .query_row(
                "SELECT ts FROM bars WHERE symbol=?1 AND interval=?2 AND source=?3
                 ORDER BY ts DESC LIMIT 1 OFFSET ?4",
                params![symbol, base, source_id, source_limit.saturating_sub(1)],
                |row| row.get::<_, i64>(0),
            )
            .optional()?,
        (false, Some((from_time, to_time))) => connection
            .query_row(
                "SELECT ts FROM bars WHERE symbol=?1 AND interval=?2 AND source=?3
                   AND ts>=?4 AND ts<?5
                 ORDER BY ts DESC LIMIT 1 OFFSET ?6",
                params![symbol, base, source_id, from_time, to_time, source_limit.saturating_sub(1)],
                |row| row.get::<_, i64>(0),
            )
            .optional()?,
        (true, None) => connection
            .query_row(
                "SELECT b.ts FROM bars b
                 WHERE b.symbol=?1 AND b.interval=?2 AND b.source=?3
                   AND NOT EXISTS (
                     SELECT 1 FROM bars newer
                     WHERE newer.symbol=b.symbol AND newer.interval=b.interval
                       AND newer.source=b.source AND newer.ts>b.ts
                       AND newer.ts<((b.ts+?4)/?5+1)*?5-?4
                   )
                 ORDER BY b.ts DESC LIMIT 1 OFFSET ?6",
                params![symbol, base, source_id, offset, day_ns, source_limit.saturating_sub(1)],
                |row| row.get::<_, i64>(0),
            )
            .optional()?,
        (true, Some((from_time, to_time))) => connection
            .query_row(
                "SELECT b.ts FROM bars b
                 WHERE b.symbol=?1 AND b.interval=?2 AND b.source=?3
                   AND b.ts>=?4 AND b.ts<?5
                   AND NOT EXISTS (
                     SELECT 1 FROM bars newer
                     WHERE newer.symbol=b.symbol AND newer.interval=b.interval
                       AND newer.source=b.source AND newer.ts>b.ts AND newer.ts<?5
                       AND newer.ts<((b.ts+?6)/?7+1)*?7-?6
                   )
                 ORDER BY b.ts DESC LIMIT 1 OFFSET ?8",
                params![symbol, base, source_id, from_time, to_time, offset, day_ns, source_limit.saturating_sub(1)],
                |row| row.get::<_, i64>(0),
            )
            .optional()?,
    };
    if latest.is_some() {
        return Ok(latest);
    }

    let earliest: Option<i64> = match (daily, range) {
        (false, None) => connection.query_row(
            "SELECT MIN(ts) FROM bars WHERE symbol=?1 AND interval=?2 AND source=?3",
            params![symbol, base, source_id],
            |row| row.get(0),
        )?,
        (false, Some((from_time, to_time))) => connection.query_row(
            "SELECT MIN(ts) FROM bars WHERE symbol=?1 AND interval=?2 AND source=?3
               AND ts>=?4 AND ts<?5",
            params![symbol, base, source_id, from_time, to_time],
            |row| row.get(0),
        )?,
        (true, None) => connection.query_row(
            "SELECT MIN(b.ts) FROM bars b
             WHERE b.symbol=?1 AND b.interval=?2 AND b.source=?3
               AND NOT EXISTS (
                 SELECT 1 FROM bars newer
                 WHERE newer.symbol=b.symbol AND newer.interval=b.interval
                   AND newer.source=b.source AND newer.ts>b.ts
                   AND newer.ts<((b.ts+?4)/?5+1)*?5-?4
               )",
            params![symbol, base, source_id, offset, day_ns],
            |row| row.get(0),
        )?,
        (true, Some((from_time, to_time))) => connection.query_row(
            "SELECT MIN(b.ts) FROM bars b
             WHERE b.symbol=?1 AND b.interval=?2 AND b.source=?3
               AND b.ts>=?4 AND b.ts<?5
               AND NOT EXISTS (
                 SELECT 1 FROM bars newer
                 WHERE newer.symbol=b.symbol AND newer.interval=b.interval
                   AND newer.source=b.source AND newer.ts>b.ts AND newer.ts<?5
                   AND newer.ts<((b.ts+?6)/?7+1)*?7-?6
               )",
            params![symbol, base, source_id, from_time, to_time, offset, day_ns],
            |row| row.get(0),
        )?,
    };
    Ok(earliest)
}

#[allow(clippy::too_many_arguments)]
fn for_each_source_candle(
    connection: &Connection,
    symbol: &str,
    base: &str,
    source_id: &str,
    first_source_time: i64,
    range: Option<(i64, i64)>,
    offset: i64,
    day_ns: i64,
    mut visit: impl FnMut(Candle),
) -> Result<(), rusqlite::Error> {
    let daily = base == "1d";
    match (daily, range) {
        (false, None) => {
            let mut statement = connection.prepare(
                "SELECT ts,open,high,low,close,volume FROM bars
                 WHERE symbol=?1 AND interval=?2 AND source=?3 AND ts>=?4 ORDER BY ts ASC",
            )?;
            for row in statement.query_map(
                params![symbol, base, source_id, first_source_time],
                candle_from_row,
            )? {
                visit(row?);
            }
        }
        (false, Some((from_time, to_time))) => {
            let mut statement = connection.prepare(
                "SELECT ts,open,high,low,close,volume FROM bars
                 WHERE symbol=?1 AND interval=?2 AND source=?3
                   AND ts>=?4 AND ts<?5 AND ts>=?6 ORDER BY ts ASC",
            )?;
            for row in statement.query_map(
                params![symbol, base, source_id, from_time, to_time, first_source_time],
                candle_from_row,
            )? {
                visit(row?);
            }
        }
        (true, None) => {
            let mut statement = connection.prepare(
                "SELECT b.ts,b.open,b.high,b.low,b.close,b.volume FROM bars b
                 WHERE b.symbol=?1 AND b.interval=?2 AND b.source=?3 AND b.ts>=?4
                   AND NOT EXISTS (
                     SELECT 1 FROM bars newer
                     WHERE newer.symbol=b.symbol AND newer.interval=b.interval
                       AND newer.source=b.source AND newer.ts>b.ts
                       AND newer.ts<((b.ts+?5)/?6+1)*?6-?5
                   ) ORDER BY b.ts ASC",
            )?;
            for row in statement.query_map(
                params![symbol, base, source_id, first_source_time, offset, day_ns],
                candle_from_row,
            )? {
                visit(row?);
            }
        }
        (true, Some((from_time, to_time))) => {
            let mut statement = connection.prepare(
                "SELECT b.ts,b.open,b.high,b.low,b.close,b.volume FROM bars b
                 WHERE b.symbol=?1 AND b.interval=?2 AND b.source=?3
                   AND b.ts>=?4 AND b.ts<?5 AND b.ts>=?6
                   AND NOT EXISTS (
                     SELECT 1 FROM bars newer
                     WHERE newer.symbol=b.symbol AND newer.interval=b.interval
                       AND newer.source=b.source AND newer.ts>b.ts AND newer.ts<?5
                       AND newer.ts<((b.ts+?7)/?8+1)*?8-?7
                   ) ORDER BY b.ts ASC",
            )?;
            for row in statement.query_map(
                params![symbol, base, source_id, from_time, to_time, first_source_time, offset, day_ns],
                candle_from_row,
            )? {
                visit(row?);
            }
        }
    }
    Ok(())
}

fn aggregation_source_candle_limit_with_cap(
    duration_ns: i64,
    base_duration_ns: i64,
    limit: i64,
    source_cap: i64,
) -> i64 {
    let source_multiplier = (duration_ns + base_duration_ns - 1) / base_duration_ns;
    (limit.saturating_mul(source_multiplier) + 120).clamp(1, source_cap)
}

pub(crate) fn aggregate_candles_from_source(
    connection: &Connection,
    symbol: &str,
    timeframe: &str,
    limit: i64,
    source: &str,
) -> Result<Vec<Candle>, rusqlite::Error> {
    let Some((base, _)) = derived_spec(timeframe) else {
        return load_candles_from_source(connection, symbol, timeframe, limit, source);
    };
    aggregate_candles_from_base_source(connection, symbol, timeframe, base, limit, source)
}
