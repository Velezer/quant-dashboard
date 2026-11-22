//! Bitcoin ETF-flow and crypto-sentiment macro data providers.

use super::{
    treasury::valid_date, FetchStatus, MacroIndicatorId, MacroObservation, MacroSeriesProvider,
    ProviderSeriesSnapshot,
};
use crate::public_http::{get_bounded, RequestOptions};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

const ETF_FLOWS_URL: &str = "https://www.tftc.io/bitcoin-etf-flows/data.json";
const FEAR_GREED_URL: &str = "https://api.alternative.me/fng/?limit=0";
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const MAX_SERIES_OBSERVATIONS: usize = 5_000;
const CACHE_TTL_SECONDS: u64 = 6 * 60 * 60;
const MINIMUM_RETRY_INTERVAL_SECONDS: u64 = 60;
const MAX_REFRESH_ERROR_CHARS: usize = 240;
const ETF_FLOWS_LICENSE: &str = "https://creativecommons.org/licenses/by/4.0/";

pub(super) static BITCOIN_PROVIDER: BitcoinProvider = BitcoinProvider;

pub(super) struct BitcoinProvider;

impl MacroSeriesProvider for BitcoinProvider {
    fn read_series(
        &self,
        id: MacroIndicatorId,
        force_refresh: bool,
    ) -> Result<ProviderSeriesSnapshot, String> {
        match id {
            MacroIndicatorId::BitcoinSpotEtfNetFlows | MacroIndicatorId::BitcoinFearGreedIndex => {
                load_series(id, force_refresh)
            }
            _ => Err("The Bitcoin macro provider does not provide the requested series.".to_string()),
        }
    }
}

#[derive(Clone)]
struct CachedSeries {
    observations: Vec<MacroObservation>,
    retrieved_at_seconds: u64,
    last_attempt_seconds: u64,
    refresh_error: Option<String>,
}

static CACHE: OnceLock<Mutex<HashMap<MacroIndicatorId, CachedSeries>>> = OnceLock::new();

fn cache() -> &'static Mutex<HashMap<MacroIndicatorId, CachedSeries>> {
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn load_series(
    id: MacroIndicatorId,
    force_refresh: bool,
) -> Result<ProviderSeriesSnapshot, String> {
    let now = now_secs();
    let mut cache = cache().lock().unwrap_or_else(|error| error.into_inner());
    if let Some(current) = cache.get(&id) {
        let recently_retrieved = now.saturating_sub(current.retrieved_at_seconds) < CACHE_TTL_SECONDS;
        let retry_too_soon = now.saturating_sub(current.last_attempt_seconds)
            < MINIMUM_RETRY_INTERVAL_SECONDS;
        if (recently_retrieved && !force_refresh) || retry_too_soon {
            return Ok(snapshot_from_cache(current, current.refresh_error.is_some()));
        }
    }
    if let Some(current) = cache.get_mut(&id) {
        current.last_attempt_seconds = now;
    }

    match fetch_series(id, now) {
        Ok(observations) => {
            let refreshed = CachedSeries {
                observations,
                retrieved_at_seconds: now,
                last_attempt_seconds: now,
                refresh_error: None,
            };
            let snapshot = ProviderSeriesSnapshot {
                observations: refreshed.observations.clone(),
                retrieved_at_seconds: refreshed.retrieved_at_seconds,
                fetch_status: FetchStatus::Fresh,
                refresh_error: None,
            };
            cache.insert(id, refreshed);
            Ok(snapshot)
        }
        Err(error) => {
            if let Some(current) = cache.get_mut(&id) {
                current.last_attempt_seconds = now;
                current.refresh_error = Some(truncate_error(&error));
                Ok(snapshot_from_cache(current, true))
            } else {
                Err(error)
            }
        }
    }
}

fn snapshot_from_cache(cached: &CachedSeries, stale: bool) -> ProviderSeriesSnapshot {
    ProviderSeriesSnapshot {
        observations: cached.observations.clone(),
        retrieved_at_seconds: cached.retrieved_at_seconds,
        fetch_status: if stale {
            FetchStatus::Stale
        } else {
            FetchStatus::Cached
        },
        refresh_error: cached.refresh_error.clone(),
    }
}

fn fetch_series(id: MacroIndicatorId, now: u64) -> Result<Vec<MacroObservation>, String> {
    let (url, accept) = match id {
        MacroIndicatorId::BitcoinSpotEtfNetFlows => (ETF_FLOWS_URL, "application/json"),
        MacroIndicatorId::BitcoinFearGreedIndex => (FEAR_GREED_URL, "application/json"),
        _ => {
            return Err("The Bitcoin macro provider does not provide the requested series.".to_string())
        }
    };
    let response = get_bounded(
        url,
        RequestOptions {
            connect_timeout: Duration::from_secs(3),
            total_timeout: Duration::from_secs(8),
            max_body_bytes: MAX_RESPONSE_BYTES,
            max_redirects: 2,
            accept,
            user_agent: Some("Quant Desk macro indicators/1.0"),
            accept_encoding: None,
            referer: None,
        },
    )?;
    if response.status >= 400 {
        return Err(format!("The Bitcoin data provider returned HTTP {}.", response.status));
    }
    let payload: Value = serde_json::from_slice(&response.body)
        .map_err(|_| "The Bitcoin data provider returned invalid JSON.".to_string())?;
    match id {
        MacroIndicatorId::BitcoinSpotEtfNetFlows => parse_etf_flows(&payload, now),
        MacroIndicatorId::BitcoinFearGreedIndex => parse_fear_greed(&payload, now),
        _ => Err("The Bitcoin macro provider does not provide the requested series.".to_string()),
    }
}

pub(super) fn parse_etf_flows(payload: &Value, now: u64) -> Result<Vec<MacroObservation>, String> {
    let license = payload
        .get("license")
        .and_then(Value::as_str)
        .ok_or_else(|| "The ETF flow source did not publish its data license.".to_string())?;
    let attribution = payload
        .get("attribution")
        .and_then(Value::as_str)
        .ok_or_else(|| "The ETF flow source did not publish its attribution.".to_string())?;
    if license != ETF_FLOWS_LICENSE || !attribution.contains("TFTC") {
        return Err(
            "The ETF flow source license or attribution changed; the feed is withheld.".to_string(),
        );
    }
    let rows = payload
        .get("days")
        .and_then(Value::as_array)
        .filter(|rows| !rows.is_empty() && rows.len() <= MAX_SERIES_OBSERVATIONS)
        .ok_or_else(|| "The ETF flow source returned no bounded daily history.".to_string())?;
    let latest_allowed_date = current_date(now);
    let mut observations = Vec::with_capacity(rows.len());
    for row in rows {
        let date = row
            .get("date")
            .and_then(Value::as_str)
            .filter(|date| valid_date(date))
            .ok_or_else(|| "The ETF flow source returned an invalid observation date.".to_string())?;
        if date > latest_allowed_date.as_str() {
            return Err("The ETF flow source returned an observation dated in the future.".to_string());
        }
        let value = row
            .get("netFlowUsd")
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite())
            .ok_or_else(|| "The ETF flow source returned an invalid daily net flow.".to_string())?;
        observations.push(MacroObservation {
            date: date.to_string(),
            value,
            classification: None,
        });
    }
    sort_and_validate_dates(&mut observations, "ETF flow")?;
    Ok(observations)
}

pub(super) fn parse_fear_greed(payload: &Value, now: u64) -> Result<Vec<MacroObservation>, String> {
    if payload.get("name").and_then(Value::as_str) != Some("Fear and Greed Index") {
        return Err("Alternative.me returned an unexpected Fear & Greed payload.".to_string());
    }
    let rows = payload
        .get("data")
        .and_then(Value::as_array)
        .filter(|rows| !rows.is_empty() && rows.len() <= MAX_SERIES_OBSERVATIONS)
        .ok_or_else(|| "Alternative.me returned no bounded Fear & Greed history.".to_string())?;
    let latest_allowed_day = i64::try_from(now / 86_400).unwrap_or(i64::MAX);
    let mut observations = Vec::with_capacity(rows.len());
    for row in rows {
        let timestamp = row
            .get("timestamp")
            .and_then(value_as_integer)
            .filter(|timestamp| *timestamp > 0)
            .ok_or_else(|| {
                "Alternative.me returned an invalid Fear & Greed timestamp.".to_string()
            })?;
        let days = i64::try_from(timestamp / 86_400)
            .map_err(|_| {
                "Alternative.me returned a timestamp outside its supported range.".to_string()
            })?;
        if days > latest_allowed_day {
            return Err("Alternative.me returned an observation dated in the future.".to_string());
        }
        let (year, month, day) = quant_core::gregorian_date_from_unix_days(days);
        let date = format!("{year:04}-{month:02}-{day:02}");
        let value = row
            .get("value")
            .and_then(value_as_integer)
            .filter(|value| (0..=100).contains(value))
            .ok_or_else(|| "Alternative.me returned an index value outside 0–100.".to_string())?;
        let classification = row
            .get("value_classification")
            .and_then(Value::as_str)
            .filter(|label| valid_classification(label))
            .ok_or_else(|| "Alternative.me returned an unknown sentiment classification.".to_string())?;
        observations.push(MacroObservation {
            date,
            value: value as f64,
            classification: Some(classification.to_string()),
        });
    }
    sort_and_validate_dates(&mut observations, "Fear & Greed")?;
    Ok(observations)
}

fn value_as_integer(value: &Value) -> Option<i64> {
    value.as_i64().or_else(|| value.as_str()?.parse::<i64>().ok())
}

fn valid_classification(value: &str) -> bool {
    matches!(
        value,
        "Extreme Fear" | "Fear" | "Neutral" | "Greed" | "Extreme Greed"
    )
}

fn sort_and_validate_dates(
    observations: &mut Vec<MacroObservation>,
    series_name: &str,
) -> Result<(), String> {
    observations.sort_by(|left, right| left.date.cmp(&right.date));
    if observations.windows(2).any(|pair| pair[0].date >= pair[1].date) {
        return Err(format!("The {series_name} source returned duplicate observation dates."));
    }
    Ok(())
}

fn current_date(now: u64) -> String {
    let days = i64::try_from(now / 86_400).unwrap_or(i64::MAX);
    let (year, month, day) = quant_core::gregorian_date_from_unix_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

fn truncate_error(error: &str) -> String {
    error.chars().take(MAX_REFRESH_ERROR_CHARS).collect()
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
