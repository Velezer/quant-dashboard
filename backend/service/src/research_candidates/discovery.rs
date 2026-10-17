use super::*;

pub(super) fn percent_change(start: f64, end: f64) -> Option<f64> {
    if !start.is_finite() || !end.is_finite() || start <= 0.0 || end <= 0.0 {
        return None;
    }
    Some(((end / start) - 1.0) * 100.0)
}

/// Calculate a bounded, price-only discovery screen for the Stockbit daily
/// universe. This is a ranking aid, not a return forecast or an execution
/// validation. Strategy evidence and the 100% calendar-month account-return
/// goal remain separate.
pub fn multibagger_price_screen(
    connection: &Connection,
    symbol: &str,
) -> Result<Value, rusqlite::Error> {
    let candles = load_candles(connection, symbol, "1d", MAX_EVIDENCE_CANDLES)?;
    let valid: Vec<_> = candles
        .iter()
        .filter(|candle| {
            candle.close.is_finite()
                && candle.close > 0.0
                && candle.volume.is_finite()
                && candle.volume >= 0.0
        })
        .collect();
    let Some(first) = valid.first() else {
        return Ok(serde_json::json!({
            "status": "NO_PRICE_HISTORY",
            "bars": 0,
            "price_change_pct": null,
            "one_year_return_pct": null,
            "close_vs_sma_200_pct": null,
            "average_volume_20d": null
        }));
    };
    let last = valid.last().unwrap_or(first);
    let one_year_cutoff = last.time.saturating_sub(365 * NANOS_PER_DAY);
    let one_year_start = valid
        .iter()
        .find(|candle| candle.time >= one_year_cutoff)
        .copied();
    let recent = valid.iter().rev().take(200).collect::<Vec<_>>();
    let sma_200 = if recent.is_empty() {
        None
    } else {
        Some(recent.iter().map(|candle| candle.close).sum::<f64>() / recent.len() as f64)
    };
    let volume_window = valid.iter().rev().take(20).collect::<Vec<_>>();
    let average_volume_20d = if volume_window.is_empty() {
        None
    } else {
        Some(
            volume_window
                .iter()
                .map(|candle| candle.volume)
                .sum::<f64>()
                / volume_window.len() as f64,
        )
    };
    let one_year_return_pct =
        one_year_start.and_then(|start| percent_change(start.close, last.close));
    let close_vs_sma_200_pct = sma_200.and_then(|average| percent_change(average, last.close));

    Ok(serde_json::json!({
        "status": "PRICE_HISTORY_AVAILABLE",
        "bars": valid.len(),
        "price_change_pct": percent_change(first.close, last.close),
        "one_year_return_pct": one_year_return_pct,
        "close_vs_sma_200_pct": close_vs_sma_200_pct,
        "average_volume_20d": average_volume_20d,
        "last_close": last.close
    }))
}

pub fn multibagger_candidate_assessment(
    row: &Value,
    screen: &Value,
) -> (&'static str, f64, &'static str) {
    let current = row.get("freshness").and_then(Value::as_str) == Some("CURRENT");
    let sample_days = finite_json_number(row.get("sample_days")).unwrap_or(0.0);
    let trades = finite_json_number(row.get("trades")).unwrap_or(0.0);
    let profit_factor = finite_json_number(row.get("profit_factor")).unwrap_or(0.0);
    let win_rate = finite_json_number(row.get("win_rate_pct")).unwrap_or(0.0);
    let best_month_return = finite_json_number(row.get("holdout_max_calendar_month_return_pct"));
    let month_return_validated = has_validated_holdout_calendar_month_return(row);
    let one_year_return = finite_json_number(screen.get("one_year_return_pct"));
    let close_vs_sma = finite_json_number(screen.get("close_vs_sma_200_pct"));

    // The score is a bounded evidence ranking. It never becomes an order
    // permission and deliberately gives no credit for an unknown metric.
    let mut score = 0.0;
    if current {
        score += 10.0;
    }
    if sample_days >= MIN_EVIDENCE_SAMPLE_DAYS as f64 {
        score += 10.0;
    }
    if trades >= MIN_EVIDENCE_SAMPLE_TRADES as f64 {
        score += 10.0;
    }
    if let Some(return_pct) = one_year_return.filter(|value| *value > 0.0) {
        score += (return_pct / 100.0 * 20.0).clamp(0.0, 20.0);
    }
    if let Some(distance_pct) = close_vs_sma.filter(|value| *value > 0.0) {
        score += (distance_pct / 50.0 * 15.0).clamp(0.0, 15.0);
    }
    if profit_factor > 0.0 {
        score += (profit_factor / 2.0 * 10.0).clamp(0.0, 10.0);
    }
    if win_rate > 0.0 {
        score += (win_rate / 100.0 * 5.0).clamp(0.0, 5.0);
    }
    if month_return_validated {
        score += 10.0;
    }
    if let Some(return_pct) = best_month_return.filter(|value| *value > 0.0) {
        score += (return_pct / MIN_MULTIBAGGER_CALENDAR_MONTH_RETURN_PCT * 10.0).clamp(0.0, 10.0);
    }
    let score = score.clamp(0.0, 100.0);

    if month_return_validated
        && best_month_return.is_some_and(|value| value >= MIN_MULTIBAGGER_CALENDAR_MONTH_RETURN_PCT)
    {
        (
            "MONTHLY_RETURN_TARGET_MET",
            score,
            "An untouched holdout shows at least 100% fee-adjusted, risk-sized account growth in a complete calendar month; venue-native history is still required before execution.",
        )
    } else if current
        && sample_days >= MIN_EVIDENCE_SAMPLE_DAYS as f64
        && trades >= MIN_EVIDENCE_SAMPLE_TRADES as f64
        && one_year_return.is_some_and(|value| value > 0.0)
        && close_vs_sma.is_some_and(|value| value > 0.0)
        && profit_factor >= 1.0
    {
        (
            "POTENTIAL_CANDIDATE",
            score,
            "Positive one-year trend, above the 200-day average, and evidence screen passed; the 100% calendar-month return goal is not validated.",
        )
    } else if current && one_year_return.is_some_and(|value| value > 0.0) {
        (
            "MOMENTUM_WATCHLIST",
            score,
            "Price momentum is positive, but the complete compounder evidence screen is not met.",
        )
    } else {
        (
            "SCREENED_OUT",
            score,
            "The stored history does not pass the current compounder discovery screen.",
        )
    }
}
