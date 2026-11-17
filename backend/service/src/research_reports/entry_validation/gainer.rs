use super::*;

/// Fixed equal-weight DSS-led ranking; no fitting on the displayed holdout.
/// Only a bounded recent sample is scored, with each prefix ending at its signal.
pub(crate) fn gainer_observations(connection: &Connection, directory: &str, id: &str) -> Result<Value, String> {
    let instrument = store::instrument(connection, id).map_err(|error| error.to_string())?
        .ok_or("Registered instrument unavailable")?;
    if text(&instrument, "exchange_id") != "tokocrypto"
        || text(&instrument, "product_id") != "tokocrypto_spot"
        || instrument["venue_verified"] != true {
        return Err("Choose verified Tokocrypto Spot crypto instruments".into());
    }
    let (bars, source, mut history) = read_history(directory, &instrument, "4h")?;
    history["validation_span_sufficient"] = json!(history["coverage_days"].as_i64().unwrap_or(0) >= quant_core::ENTRY_SCORE_MIN_HISTORY_DAYS);
    let duration = timeframe_duration_ns("4h").ok_or("Unsupported timeframe")?;
    let minimum = EntryModelKind::Dss.minimum_history_bars();
    let mut observations = Vec::new();
    for index in bars.len().saturating_sub(64).max(minimum)..bars.len().saturating_sub(2) {
        let start = index.saturating_sub(511);
        let prefix = &bars[start..=index];
        if prefix.windows(2).any(|pair| pair[1].time - pair[0].time != duration)
            || bars[index + 1].time - bars[index].time != duration
            || bars[index + 2].time - bars[index + 1].time != duration { continue; }
        let entry = &bars[index + 1];
        if prefix.iter().any(|bar| bar.volume <= 0.0 || !bar.close.is_finite())
            || entry.open <= 0.0 || bars[index + 2].open <= 0.0 { continue; }
        let Some(signal) = quant_core::generate_signed_entry_signal(prefix, EntryModelKind::Dss) else { continue };
        let Some(score) = quant_core::score_entry_signal(prefix, &signal, EntryModelKind::Dss) else { continue };
        if !score.total_score.is_finite() { continue; }
        observations.push(json!({"time":entry.time,"score":score.total_score,
            "momentum":bars[index].close / bars[index - 1].close - 1.0,
            "gross_return":bars[index + 2].open / entry.open - 1.0}));
    }
    Ok(json!({"protocol_id":"tokocrypto_dss_equal_weight_next_4h_rank_v1",
        "instrument":instrument,"history":history,"source":source,"costs":fee_metadata(&instrument).3,
        "observations":observations,"limits":"Recent 64 completed bars; fixed 512-bar causal lookback. Current listings only: survivorship bias. Research sample is insufficient to establish predictive ability."}))
}
