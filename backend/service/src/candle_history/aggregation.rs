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
    let source = match range {
        Some((from_time, to_time)) => load_candles_from_source_between(
            connection,
            symbol,
            base,
            source_limit,
            source_id,
            from_time.saturating_sub(duration_ns),
            to_time,
        ),
        None => load_candles_from_source(connection, symbol, base, source_limit, source_id),
    }?;
    if source.is_empty() {
        return Ok(Vec::new());
    }
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
    let anchor_days = if timeframe == "1w" || timeframe == "2w" {
        4
    } else {
        origin_day
    };
    let mut aggregated: Vec<Candle> = Vec::new();
    let mut current_bucket: Option<i64> = None;
    let mut current_source_count = 0_i64;
    let mut current_source_contiguous = true;
    let mut previous_source_time: Option<i64> = None;
    for bar in source {
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
                aggregated.pop();
            }
            aggregated.push(new_candle(
                bucket, bar.open, bar.high, bar.low, bar.close, bar.volume,
            ));
            current_bucket = Some(bucket);
            current_source_count = 1;
            current_source_contiguous = bar.time == bucket;
        } else if let Some(last) = aggregated.last_mut() {
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
    }
    if require_complete_30m
        && current_bucket.is_some()
        && (current_source_count != expected_source_bars || !current_source_contiguous)
    {
        aggregated.pop();
    }
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
