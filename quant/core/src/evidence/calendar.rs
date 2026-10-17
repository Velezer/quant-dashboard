use crate::strategies::StrategyBar;

pub(super) fn local_trading_day(timestamp_ns: i64) -> i64 {
    timestamp_ns
        .div_euclid(crate::NANOS_PER_SECOND)
        .saturating_add(crate::JAKARTA_UTC_OFFSET_SECONDS)
        .div_euclid(crate::SECONDS_PER_DAY)
}

fn local_calendar_month(timestamp_ns: i64) -> i64 {
    let (year, month, _) = crate::gregorian_date_from_unix_days(local_trading_day(timestamp_ns));
    year * 12 + i64::from(month) - 1
}

pub(super) fn maximum_completed_calendar_month_return(
    bars: &[StrategyBar],
    equity_marks: &[f64],
) -> (Option<f64>, usize) {
    let mut month_ends: Vec<(i64, f64)> = Vec::new();
    for (bar, equity) in bars.iter().zip(equity_marks.iter().copied()) {
        if !equity.is_finite() || equity <= 0.0 {
            continue;
        }
        let month = local_calendar_month(bar.time);
        if let Some((last_month, last_equity)) = month_ends.last_mut() {
            if *last_month == month {
                *last_equity = equity;
                continue;
            }
        }
        month_ends.push((month, equity));
    }

    // Exclude both partial endpoints: a month is only eligible when stored
    // bars cover the preceding and following calendar months.
    let mut best_return_pct: Option<f64> = None;
    let mut completed_months = 0usize;
    for index in 1..month_ends.len().saturating_sub(1) {
        let (previous_month, start_equity) = month_ends[index - 1];
        let (month, end_equity) = month_ends[index];
        let (next_month, _) = month_ends[index + 1];
        if month != previous_month + 1 || next_month != month + 1 || start_equity <= 0.0 {
            continue;
        }
        let return_pct = (end_equity / start_equity - 1.0) * 100.0;
        if !return_pct.is_finite() {
            continue;
        }
        completed_months += 1;
        best_return_pct = Some(best_return_pct.map_or(return_pct, |best| best.max(return_pct)));
    }
    (best_return_pct, completed_months)
}
