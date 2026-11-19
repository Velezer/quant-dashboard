use rusqlite::Connection;
use serde_json::{json, Value};

fn win_rate_confidence_interval_95_pct(wins: u64, trades: u64) -> Option<[f64; 2]> {
    if trades == 0 || wins > trades {
        return None;
    }
    let z = 1.959_963_984_540_054;
    let n = trades as f64;
    let p = wins as f64 / n;
    let denominator = 1.0 + z * z / n;
    let center = (p + z * z / (2.0 * n)) / denominator;
    let half_width = z * (p * (1.0 - p) / n + z * z / (4.0 * n * n)).sqrt() / denominator;
    Some([
        (center - half_width).max(0.0) * 100.0,
        (center + half_width).min(1.0) * 100.0,
    ])
}

fn ticker_holdout_win_rate(report: Option<&str>) -> Option<Value> {
    let report = serde_json::from_str::<Value>(report?).ok()?;
    let rate = report["win_rate_after_fees_pct"]
        .as_f64()
        .filter(|rate| rate.is_finite() && (0.0..=100.0).contains(rate))?;
    let trades = report["trades"].as_u64().filter(|trades| *trades > 0)?;
    let wins = report["wins"]
        .as_u64()
        .filter(|wins| *wins <= trades)
        .or_else(|| wins_from_exact_win_rate(rate, trades));
    Some(json!({
        "status": "TICKER_HOLDOUT_RATE",
        "metric_type": "historical_win_rate",
        "win_probability_pct": rate,
        "confidence_interval_95_pct": wins
            .and_then(|wins| win_rate_confidence_interval_95_pct(wins, trades)),
        "sample_trades": trades,
        "wins": wins,
        "basis": "Exact registered ticker, model and timeframe holdout after configured venue fees; historical win rate, not matched to this signal's score; long and short outcomes pooled; interval does not adjust for correlated outcomes; spread, slippage, market impact, funding and borrow excluded",
        "fees_applied": true,
        "direction_conditioned": false,
        "score_conditioned": false,
        "computed_at": report["computed_at"],
        "source_family": report["source_family"],
    }))
}

fn wins_from_exact_win_rate(rate: f64, trades: u64) -> Option<u64> {
    let exact_wins = rate * trades as f64 / 100.0;
    let rounded_wins = exact_wins.round();
    if !(0.0..=trades as f64).contains(&rounded_wins) {
        return None;
    }
    let reconstructed_rate = rounded_wins / trades as f64 * 100.0;
    ((reconstructed_rate - rate).abs() <= 1e-8).then_some(rounded_wins as u64)
}

pub(super) fn has_score_matched_win_probability(value: &Value) -> bool {
    matches!(
        value["status"].as_str(),
        Some("ESTIMATED" | "PROFILE_GATE_ESTIMATE")
    ) && value["win_probability_pct"]
        .as_f64()
        .is_some_and(|rate| rate.is_finite() && (0.0..=100.0).contains(&rate))
        && value["sample_trades"].as_u64().is_some_and(|trades| trades > 0)
}

pub(super) type TickerHoldoutLookupKey = (String, String, String, String, String, String);

pub(super) fn ticker_holdout_lookup_key(
    signal: &Value,
    strategies: &std::collections::HashMap<&str, &str>,
) -> Option<TickerHoldoutLookupKey> {
    let score_model_id = signal["score_basis"]["model_id"]
        .as_str()
        .or_else(|| {
            strategies
                .get(signal["signal_strategy_id"].as_str().unwrap_or_default())
                .copied()
        })?;
    Some((
        signal["exchange_id"].as_str()?.to_string(),
        signal["product_id"].as_str()?.to_string(),
        signal["asset_category_id"].as_str()?.to_string(),
        signal["symbol"].as_str()?.to_string(),
        signal["timeframe"].as_str()?.to_string(),
        score_model_id.to_string(),
    ))
}

pub(super) fn load_ticker_holdout_win_rates(
    connection: &Connection,
    lookup_keys: &std::collections::BTreeSet<TickerHoldoutLookupKey>,
) -> Result<std::collections::HashMap<TickerHoldoutLookupKey, Value>, rusqlite::Error> {
    if lookup_keys.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let requested = lookup_keys
        .iter()
        .map(|key| {
            json!({
                "exchange_id": key.0.as_str(),
                "product_id": key.1.as_str(),
                "asset_category_id": key.2.as_str(),
                "symbol": key.3.as_str(),
                "timeframe": key.4.as_str(),
                "score_model_id": key.5.as_str(),
            })
        })
        .collect::<Vec<_>>();
    let requested_json = Value::Array(requested).to_string();
    let mut statement = connection.prepare(
        "WITH requested AS (
             SELECT json_extract(value,'$.exchange_id') AS exchange_id,
                    json_extract(value,'$.product_id') AS product_id,
                    json_extract(value,'$.asset_category_id') AS asset_category_id,
                    json_extract(value,'$.symbol') AS symbol,
                    json_extract(value,'$.timeframe') AS timeframe,
                    json_extract(value,'$.score_model_id') AS score_model_id
               FROM json_each(?1)
         )
         SELECT requested.exchange_id,requested.product_id,requested.asset_category_id,
                requested.symbol,requested.timeframe,requested.score_model_id,
                json_object(
                    'win_rate_after_fees_pct',json_extract(model.value,'$.holdout_win_rate_pct'),
                    'trades',json_extract(model.value,'$.holdout_trades'),
                    'wins',json_extract(model.value,'$.holdout_wins'),
                    'computed_at',json_extract(validation_report.summary_json,'$.computed_at'),
                    'source_family',json_extract(validation_report.summary_json,'$.source'))
           FROM requested
           JOIN app.instruments AS validation_instrument
             ON validation_instrument.exchange_id=requested.exchange_id
            AND validation_instrument.product_id=requested.product_id
            AND validation_instrument.asset_category_id=requested.asset_category_id
            AND validation_instrument.symbol=requested.symbol
            AND validation_instrument.is_active=1
           JOIN app.entry_model_validation_reports AS validation_report
             ON validation_report.instrument_id=validation_instrument.instrument_id
            AND validation_report.timeframe=requested.timeframe
            AND validation_report.protocol_id=?2
           JOIN json_each(
                CASE WHEN json_valid(validation_report.summary_json)
                     THEN validation_report.summary_json ELSE '{\"models\":[]}' END,
                '$.models') AS model
             ON json_extract(model.value,'$.score_model_id')=requested.score_model_id
            AND CASE
                  WHEN json_type(model.value,'$.holdout_fees_applied') IS NOT NULL
                    THEN json_extract(model.value,'$.holdout_fees_applied')=1
                  ELSE json_type(model.value,'$.holdout_win_rate_pct') IN ('integer','real')
                END",
    )?;
    let rows = statement.query_map(
        rusqlite::params![requested_json, quant_core::ENTRY_SCORE_VALIDATION_PROTOCOL_ID],
        |row| {
            Ok((
                (
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ),
                row.get::<_, String>(6)?,
            ))
        },
    )?;
    let mut win_rates = std::collections::HashMap::with_capacity(lookup_keys.len());
    for row in rows {
        let (key, report) = row?;
        if let Some(win_rate) = ticker_holdout_win_rate(Some(&report)) {
            win_rates.insert(key, win_rate);
        }
    }
    Ok(win_rates)
}


#[cfg(test)]
#[path = "ticker_holdout_tests.rs"]
mod tests;
