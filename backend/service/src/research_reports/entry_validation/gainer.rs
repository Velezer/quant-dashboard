use super::*;

pub(crate) const GAINER_PROTOCOL: &str = "tokocrypto_signed_equal_weight_next_4h_rank_v2";
const SAMPLE_CADENCE_DAYS: i64 = 4;

/// All available history up to the existing read cap; next-open entry, next
/// 4h-open exit. Sampling uses a fixed epoch grid independent of outcomes.
pub(crate) fn gainer_observations(connection: &Connection, directory: &str, id: &str, cutoff: i64) -> Result<Value, String> {
    let instrument = store::instrument(connection, id).map_err(|error| error.to_string())?
        .ok_or("Registered instrument unavailable")?;
    if text(&instrument, "exchange_id") != "tokocrypto"
        || text(&instrument, "product_id") != "tokocrypto_spot"
        || text(&instrument, "research_eligibility") != "eligible" {
        return Err("Choose eligible verified Tokocrypto Spot crypto instruments".into());
    }
    let (mut bars, source, mut history) = read_history(directory, &instrument, "4h")?;
    let duration = timeframe_duration_ns("4h").ok_or("Unsupported timeframe")?;
    let cutoff_ns = cutoff.min(now_secs() as i64).saturating_mul(NANOS_PER_SECOND);
    bars.retain(|bar| bar.time.saturating_add(duration) <= cutoff_ns);
    let (first, last) = (bars.first().map(|bar| bar.time), bars.last().map(|bar| bar.time));
    history["first_ts"] = json!(first);
    history["last_ts"] = json!(last);
    history["bars"] = json!(bars.len());
    history["coverage_days"] = json!(history_coverage_days(first, last));
    history["data_fingerprint"] = json!(candle_fingerprint(&bars));
    history["validation_span_sufficient"] = json!(history["coverage_days"].as_i64().unwrap_or(0) >= quant_core::ENTRY_SCORE_MIN_HISTORY_DAYS);
    let registered_at: i64 = connection.query_row("SELECT registered_at FROM app.instruments WHERE instrument_id=?1", [id], |row| row.get(0)).map_err(|error| error.to_string())?;
    history["registered_at"] = json!(registered_at);
    history["listing_age_status"] = json!("UNKNOWN: registration time and earliest retained candle do not establish the venue listing date");
    use rusqlite::OptionalExtension;
    if let Some(market) = open_market_history_for_symbol(directory, text(&instrument, "storage_symbol"), false).map_err(|error| error.to_string())? {
        let state: Option<(String, Option<String>, Option<String>)> = market.query_row(
            "SELECT status,attempted_at,error FROM ingestion_runs WHERE symbol=?1 AND interval='1h_history_backfill' AND source=?2 ORDER BY id DESC LIMIT 1",
            params![text(&instrument, "storage_symbol"),crate::tokocrypto_storage::SOURCE], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?))).optional().map_err(|error| error.to_string())?;
        if let Some((status, attempted_at, error)) = state {
            history["native_backfill"] = json!({"status":status,"attempted_at":attempted_at,"error":error});
        }
    }
    let mut observations = Vec::new();
    let mut segment_start = 0;
    let mut gaps = 0;
    // Reset recursive studies after a gap; never mix disjoint candle segments.
    for end in 1..=bars.len() {
        if end < bars.len() && bars[end].time - bars[end - 1].time == duration { continue; }
        let segment = &bars[segment_start..end];
        quant_core::strategies::visit_sampled_entry_scores(segment,
            |index| index >= 1 && index + 2 < segment.len()
                && (segment[index].time + duration).rem_euclid(SAMPLE_CADENCE_DAYS * quant_core::NANOS_PER_DAY) == 0,
            |index, score| {
                let (signal, previous, entry, exit) = (&segment[index], &segment[index - 1], &segment[index + 1], &segment[index + 2]);
                if ![signal.open, signal.close, previous.close, entry.open, exit.open, signal.volume, entry.volume].iter().all(|value| value.is_finite() && *value > 0.0) { return; }
                observations.push(json!({"time":entry.time / NANOS_PER_SECOND,
                    "signal_time":signal.time / NANOS_PER_SECOND,"exit_time":exit.time / NANOS_PER_SECOND,
                    "score":score,"eligible":score >= quant_core::SIX_INDICATOR_MINIMUM_SCORE_OUT_OF_60,"momentum":signal.close / previous.close - 1.0,
                    "gross_return":exit.open / entry.open - 1.0}));
            });
        segment_start = end;
        if end < bars.len() { gaps += 1; }
    }
    history["gaps"] = json!(gaps);
    let costs = published_fee_metadata(text(&instrument, "quote_currency"))
        .unwrap_or_else(|| fee_metadata(&instrument).3);
    Ok(json!({"protocol_id":GAINER_PROTOCOL,"instrument":instrument,
        "history":history,"source":source,"costs":costs,"observations":observations,
        "sample_cadence_days":SAMPLE_CADENCE_DAYS,"minimum_history_days":quant_core::ENTRY_SCORE_MIN_HISTORY_DAYS,
        "cutoff":cutoff,"limits":"Current listings only; historical listing eligibility unavailable. No predictive qualification or live-profile promotion."}))
}

fn published_fee_metadata(quote: &str) -> Option<Value> {
    if let VenueFeePolicy::Known { buy_fee_pct, sell_fee_pct, fee_model, official_fee_rules_url } =
        quant_core::venue_rules::tokocrypto_spot_published_fee_policy(quote) {
        Some(json!({"fees_known":true,"buy_fee_pct":buy_fee_pct,"sell_fee_pct":sell_fee_pct,
            "label":fee_model,"source_url":official_fee_rules_url,"published_date":"2026-06-18",
            "quote":quote,"basis":"Current published standard-rate scenario; historical fee changes and discounts excluded"}))
    } else { None }
}

pub(crate) fn gainer_saved(connection: &Connection, report: Option<Value>) -> Result<Value, String> {
    if let Some(mut report) = report {
        if report["protocol_id"] != GAINER_PROTOCOL || !report.is_object() {
            return Err("Ranking protocol changed. Reload and rerun.".into());
        }
        // Only aggregate research fields may cross the persistence boundary.
        let allowed = ["protocol_id", "cutoff", "coverage", "costs", "splits", "metrics", "status", "limitations", "provenance"];
        if report.as_object().unwrap().keys().any(|key| !allowed.contains(&key.as_str())) {
            return Err("Save aggregate ranking evidence only; per-ticker outcomes are not persisted.".into());
        }
        report["status"] = json!("UNVALIDATED_RESEARCH");
        validate_aggregate(&report)?;
        let computed_at = now_secs() as i64;
        let encoded = report.to_string();
        if encoded.len() > 256 * 1024 { return Err("Aggregate report exceeds its bounded storage limit".into()); }
        connection.execute("INSERT INTO app.gainer_research_reports(id,protocol_id,computed_at,report_json) VALUES(1,?1,?2,?3) ON CONFLICT(id) DO UPDATE SET protocol_id=excluded.protocol_id,computed_at=excluded.computed_at,report_json=excluded.report_json",
            params![GAINER_PROTOCOL,computed_at,encoded]).map_err(|error| error.to_string())?;
    }
    use rusqlite::OptionalExtension;
    let saved: Option<(i64,String)> = connection.query_row("SELECT computed_at,report_json FROM app.gainer_research_reports WHERE id=1", [], |row| Ok((row.get(0)?,row.get(1)?))).optional().map_err(|error| error.to_string())?;
    let fee_policies: Vec<Value> = ["IDR", "USDT"].into_iter().filter_map(published_fee_metadata).collect();
    match saved {
        Some((computed_at,encoded)) => Ok(json!({"computed_at":computed_at,"report":serde_json::from_str::<Value>(&encoded).map_err(|error| error.to_string())?,"minimum_history_days":quant_core::ENTRY_SCORE_MIN_HISTORY_DAYS,"fee_policies":fee_policies,"origin":"Browser-computed aggregate research; never used to promote profiles or signals"})),
        None => Ok(json!({"report":null,"minimum_history_days":quant_core::ENTRY_SCORE_MIN_HISTORY_DAYS,"fee_policies":fee_policies})),
    }
}

fn validate_aggregate(report: &Value) -> Result<(), String> {
    let invalid = || "Malformed aggregate report; rerun research before saving".to_string();
    if report["cutoff"].as_i64().is_none_or(|value| value <= 0 || value > now_secs() as i64)
        || report["limitations"].as_str().is_none_or(|value| value.len() > 4096) { return Err(invalid()); }
    let provenance = report["provenance"].as_object().ok_or_else(invalid)?;
    if provenance.len() != 3 || provenance.keys().any(|key| !["fingerprint","sources","policies"].contains(&key.as_str()))
        || report["provenance"]["fingerprint"].as_str().is_none_or(|value| value.len() > 80) { return Err(invalid()); }
    let sources = report["provenance"]["sources"].as_object().ok_or_else(invalid)?;
    if sources.len() > 100 || sources.iter().any(|(key,value)| key.len() > 200 || value.as_u64().is_none()) { return Err(invalid()); }
    let policies = report["provenance"]["policies"].as_object().ok_or_else(invalid)?;
    if policies.len() > 10 { return Err(invalid()); }
    for (quote, policy) in policies {
        let fields = policy.as_object().ok_or_else(invalid)?;
        if quote.len() > 20 || fields.len() != 5 || fields.keys().any(|key| !["buy","sell","label","source","published"].contains(&key.as_str())) { return Err(invalid()); }
        for (key, value) in fields {
            if matches!(key.as_str(), "buy" | "sell") {
                if value.as_f64().is_none_or(|number| !(0.0..=10.0).contains(&number)) { return Err(invalid()); }
            } else if value.as_str().is_none_or(|text| text.len() > 500) { return Err(invalid()); }
        }
    }
    let coverage = report["coverage"].as_object().ok_or_else(invalid)?;
    let coverage_keys = ["universe","attempted","failed","missing","short","proxies","minimumHistoryDays","listingAgeUnknown","stopped","excludedCohorts","purged"];
    if coverage.len() != coverage_keys.len() || coverage.keys().any(|key| !coverage_keys.contains(&key.as_str()))
        || coverage.iter().any(|(key,value)| if key == "stopped" { !value.is_boolean() } else { value.as_u64().is_none() })
        || report["coverage"]["universe"].as_u64().unwrap_or(0) > 1000 { return Err(invalid()); }
    let costs = report["costs"].as_object().ok_or_else(invalid)?;
    let cost_keys = ["mode","buy","sell","spread","slippage"];
    if costs.len() != cost_keys.len() || costs.keys().any(|key| !cost_keys.contains(&key.as_str()))
        || !matches!(report["costs"]["mode"].as_str(), Some("published" | "manual")) { return Err(invalid()); }
    for key in ["buy", "sell", "spread", "slippage"] {
        if report["costs"]["mode"] == "published" && matches!(key, "buy" | "sell") && report["costs"][key].is_null() { continue; }
        if report["costs"][key].as_f64().is_none_or(|value| !value.is_finite() || !(0.0..=10.0).contains(&value)) { return Err(invalid()); }
    }
    for field in ["metrics", "splits"] {
        let object = report[field].as_object().ok_or_else(invalid)?;
        if object.len() != 3 || object.keys().any(|key| !["training","validation","holdout"].contains(&key.as_str())) { return Err(invalid()); }
    }
    for partition in ["training", "validation", "holdout"] {
        let split = report["splits"][partition].as_object().ok_or_else(invalid)?;
        if split.len() != 3 || split.keys().any(|key| !["count","first","last"].contains(&key.as_str()))
            || split.values().any(|value| !value.is_null() && value.as_u64().is_none()) { return Err(invalid()); }
        let metrics = report["metrics"][partition].as_object().ok_or_else(invalid)?;
        let metric_keys = ["top1","top5","grossTop5","universe","momentum","excessUniverse","excessMomentum","positiveFrequency","decilePrecision","rankCorrelation"];
        if metrics.len() != metric_keys.len() || metrics.keys().any(|key| !metric_keys.contains(&key.as_str())) { return Err(invalid()); }
        for metric in metrics.values() {
            let fields = metric.as_object().ok_or_else(invalid)?;
            if fields.len() != 4 || fields.keys().any(|key| !["count","mean","lower","upper"].contains(&key.as_str()))
                || fields.values().any(|value| !value.is_null() && value.as_f64().is_none_or(|number| !number.is_finite())) { return Err(invalid()); }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "gainer_tests.rs"]
mod gainer_tests;
