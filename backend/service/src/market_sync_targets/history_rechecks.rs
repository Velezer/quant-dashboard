use super::selected_candle_source_for_product;
use rusqlite::Connection;
use std::collections::HashSet;

pub(super) fn record_received_intervals(
    received: &mut HashSet<String>,
    report: &super::super::CandleProviderReport,
) {
    received.extend(report.outcomes.iter().filter_map(|outcome| {
        outcome
            .result
            .as_ref()
            .is_ok_and(|rows| *rows > 0)
            .then(|| outcome.interval.to_string())
    }));
}

/// Find saved entry-validation timeframes whose source span may have changed
/// because this candle batch stored rows. Read only indexed source bounds;
/// materializing chart candles here would cap derived history at the 1,000-bar
/// display limit and miss the 1,000-day evidence threshold.
pub(in crate::market_sync) fn mature_entry_validation_timeframes(
    connection: &Connection,
    exchange_id: &str,
    product_id: &str,
    storage_symbol: &str,
    received_intervals: &HashSet<String>,
) -> Vec<String> {
    if received_intervals.is_empty() {
        return Vec::new();
    }

    crate::SIGNAL_SCREENER_TIMEFRAMES
        .iter()
        .copied()
        .filter(|timeframe| {
            received_intervals.contains(*timeframe)
                || crate::lower_timeframe_candidates(timeframe)
                    .iter()
                    .any(|interval| received_intervals.contains(*interval))
        })
        .filter_map(|timeframe| {
            let source = selected_candle_source_for_product(
                connection,
                storage_symbol,
                exchange_id,
                product_id,
                timeframe,
            )
            .ok()??;
            let storage_timeframe = crate::preferred_storage_timeframe_from_source(
                connection,
                storage_symbol,
                timeframe,
                &source,
            )
            .ok()?;
            let (first, last) = connection
                .query_row(
                    "SELECT MIN(ts),MAX(ts) FROM bars
                      WHERE symbol=?1 AND interval=?2 AND source=?3",
                    rusqlite::params![storage_symbol, storage_timeframe, source],
                    |row| Ok((row.get::<_, Option<i64>>(0)?, row.get::<_, Option<i64>>(1)?)),
                )
                .ok()?;
            let (Some(first), Some(last)) = (first, last) else {
                return None;
            };
            let stored_coverage_days = last
                .saturating_sub(first)
                .max(0)
                .div_euclid(crate::NANOS_PER_DAY)
                .saturating_add(1);
            let duration = crate::timeframe_duration_ns(timeframe)?;
            let alignment_margin_days = if storage_timeframe == timeframe {
                1
            } else {
                // A partial aggregate can be lost at both ends of the span.
                duration
                    .saturating_add(crate::NANOS_PER_DAY - 1)
                    .div_euclid(crate::NANOS_PER_DAY)
                    .saturating_mul(2)
            };
            (stored_coverage_days
                >= quant_core::ENTRY_SCORE_MIN_HISTORY_DAYS + alignment_margin_days)
                .then(|| timeframe.to_string())
        })
        .collect()
}
