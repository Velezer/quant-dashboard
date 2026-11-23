//! EIA crude inventory and daily spot price series.

use super::{
    treasury::valid_date, FetchStatus, MacroIndicatorId, MacroObservation, MacroSeriesProvider,
    ProviderSeriesSnapshot,
};
use crate::public_http::{get_bounded, RequestOptions};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

const INVENTORY_SERIES_ID: &str = "WCESTUS1";
const BRENT_SERIES_ID: &str = "RBRTE";
const WTI_SERIES_ID: &str = "RWTC";
const EIA_INVENTORY_URL: &str = "https://api.eia.gov/v2/petroleum/stoc/wstk/data/";
const EIA_SPOT_URL: &str = "https://api.eia.gov/v2/petroleum/pri/spt/data/";
const DEMO_API_KEY: &str = "DEMO_KEY";
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const MAX_DAILY_OBSERVATIONS: usize = 3_000;
const MAX_WEEKLY_OBSERVATIONS: usize = 600;
const HISTORY_DAYS: u64 = 3_653;
const CACHE_TTL_SECONDS: u64 = 6 * 60 * 60;
const MINIMUM_RETRY_INTERVAL_SECONDS: u64 = 60;
const MAX_REFRESH_ERROR_CHARS: usize = 240;

pub(super) static EIA_PROVIDER: EiaProvider = EiaProvider;

pub(super) struct EiaProvider;

fn series_id(id: MacroIndicatorId) -> Option<&'static str> {
    match id {
        MacroIndicatorId::UsCommercialCrudeStocks => Some(INVENTORY_SERIES_ID),
        MacroIndicatorId::BrentCrudeSpot => Some(BRENT_SERIES_ID),
        MacroIndicatorId::WtiCrudeSpot => Some(WTI_SERIES_ID),
        _ => None,
    }
}

pub(super) fn source_series_id(id: MacroIndicatorId) -> Option<&'static str> {
    series_id(id)
}

pub(super) fn source_url(id: MacroIndicatorId) -> Option<&'static str> {
    match id {
        MacroIndicatorId::UsCommercialCrudeStocks => {
            Some("https://www.eia.gov/dnav/pet/hist/LeafHandler.ashx?f=W&n=PET&s=WCESTUS1")
        }
        MacroIndicatorId::BrentCrudeSpot => Some("https://www.eia.gov/dnav/pet/hist/RBRTEd.htm"),
        MacroIndicatorId::WtiCrudeSpot => Some("https://www.eia.gov/dnav/pet/hist/RWTCd.htm"),
        _ => None,
    }
}

impl MacroSeriesProvider for EiaProvider {
    fn read_series(
        &self,
        id: MacroIndicatorId,
        force_refresh: bool,
    ) -> Result<ProviderSeriesSnapshot, String> {
        if series_id(id).is_none() {
            return Err("EIA does not provide the requested macro series.".to_string());
        }
        load_series(id, force_refresh)
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
static REFRESH_LOCKS: OnceLock<Mutex<HashMap<MacroIndicatorId, Arc<Mutex<()>>>>> = OnceLock::new();

fn cache() -> &'static Mutex<HashMap<MacroIndicatorId, CachedSeries>> {
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn refresh_lock(id: MacroIndicatorId) -> Arc<Mutex<()>> {
    let locks = REFRESH_LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut locks = locks.lock().unwrap_or_else(|error| error.into_inner());
    Arc::clone(locks.entry(id).or_insert_with(|| Arc::new(Mutex::new(()))))
}

fn load_series(
    id: MacroIndicatorId,
    force_refresh: bool,
) -> Result<ProviderSeriesSnapshot, String> {
    let refresh_lock = refresh_lock(id);
    let _refresh_guard = refresh_lock
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let now = now_secs();
    let mut cache_guard = cache().lock().unwrap_or_else(|error| error.into_inner());
    if let Some(current) = cache_guard.get(&id) {
        let recently_retrieved =
            now.saturating_sub(current.retrieved_at_seconds) < CACHE_TTL_SECONDS;
        let retry_too_soon =
            now.saturating_sub(current.last_attempt_seconds) < MINIMUM_RETRY_INTERVAL_SECONDS;
        if (recently_retrieved && !force_refresh) || retry_too_soon {
            return Ok(snapshot_from_cache(
                current,
                current.refresh_error.is_some(),
            ));
        }
    }
    if let Some(current) = cache_guard.get_mut(&id) {
        current.last_attempt_seconds = now;
    }
    drop(cache_guard);

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
            let mut cache_guard = cache().lock().unwrap_or_else(|error| error.into_inner());
            cache_guard.insert(id, refreshed);
            Ok(snapshot)
        }
        Err(error) => {
            let mut cache_guard = cache().lock().unwrap_or_else(|error| error.into_inner());
            if let Some(current) = cache_guard.get_mut(&id) {
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
    let upstream_series = series_id(id)
        .ok_or_else(|| "EIA does not provide the requested macro series.".to_string())?;
    let is_inventory = id == MacroIndicatorId::UsCommercialCrudeStocks;
    let endpoint = if is_inventory {
        EIA_INVENTORY_URL
    } else {
        EIA_SPOT_URL
    };
    let frequency = if is_inventory { "weekly" } else { "daily" };
    let length = if is_inventory {
        MAX_WEEKLY_OBSERVATIONS
    } else {
        MAX_DAILY_OBSERVATIONS
    };
    let url = format!(
        "{endpoint}?frequency={frequency}&data%5B0%5D=value&facets%5Bseries%5D%5B%5D={upstream_series}&sort%5B0%5D%5Bcolumn%5D=period&sort%5B0%5D%5Bdirection%5D=desc&offset=0&length={length}&api_key={DEMO_API_KEY}"
    );
    let response = get_bounded(
        &url,
        RequestOptions {
            connect_timeout: Duration::from_secs(3),
            total_timeout: Duration::from_secs(10),
            max_body_bytes: MAX_RESPONSE_BYTES,
            max_redirects: 2,
            accept: "application/json",
            user_agent: Some("Quant Desk macro indicators/1.0"),
            accept_encoding: None,
            referer: None,
        },
    )?;
    if response.status >= 400 {
        return Err(format!(
            "EIA returned HTTP {} for {upstream_series}.",
            response.status
        ));
    }
    let payload: Value = serde_json::from_slice(&response.body)
        .map_err(|_| format!("EIA returned invalid JSON for {upstream_series}."))?;
    parse_series(&payload, now, id)
}

fn parse_series(
    payload: &Value,
    now: u64,
    id: MacroIndicatorId,
) -> Result<Vec<MacroObservation>, String> {
    let upstream_series = series_id(id)
        .ok_or_else(|| "EIA does not provide the requested macro series.".to_string())?;
    if payload.get("error").is_some() {
        return Err(format!("EIA returned an API error for {upstream_series}."));
    }
    let max_observations = if id == MacroIndicatorId::UsCommercialCrudeStocks {
        MAX_WEEKLY_OBSERVATIONS
    } else {
        MAX_DAILY_OBSERVATIONS
    };
    let rows = payload
        .pointer("/response/data")
        .and_then(Value::as_array)
        .filter(|rows| !rows.is_empty() && rows.len() <= max_observations)
        .ok_or_else(|| format!("EIA returned no bounded observations for {upstream_series}."))?;
    let latest_allowed_date = current_date(now);
    let expected_units = if id == MacroIndicatorId::UsCommercialCrudeStocks {
        "MBBL"
    } else {
        "$/BBL"
    };
    let mut observations = Vec::with_capacity(rows.len());
    for row in rows {
        if row.get("series").and_then(Value::as_str) != Some(upstream_series)
            || row.get("units").and_then(Value::as_str) != Some(expected_units)
        {
            return Err(format!(
                "EIA returned a different series or unit for {upstream_series}."
            ));
        }
        let date = row
            .get("period")
            .and_then(Value::as_str)
            .filter(|date| valid_date(date))
            .ok_or_else(|| format!("EIA returned an invalid date for {upstream_series}."))?;
        if date > latest_allowed_date.as_str() {
            return Err(format!("EIA returned a future date for {upstream_series}."));
        }
        let value = row
            .get("value")
            .and_then(Value::as_str)
            .and_then(|value| value.parse::<f64>().ok())
            .filter(|value| {
                value.is_finite()
                    && (*value >= 0.0
                        || matches!(
                            id,
                            MacroIndicatorId::BrentCrudeSpot | MacroIndicatorId::WtiCrudeSpot
                        ))
            })
            .ok_or_else(|| format!("EIA returned an invalid value for {upstream_series}."))?;
        observations.push(MacroObservation {
            date: date.to_string(),
            value: if id == MacroIndicatorId::UsCommercialCrudeStocks {
                value / 1_000.0
            } else {
                value
            },
            classification: None,
        });
    }
    observations.sort_by(|left, right| left.date.cmp(&right.date));
    if id != MacroIndicatorId::UsCommercialCrudeStocks {
        let start = history_start_date(now);
        observations.retain(|observation| observation.date >= start);
    }
    if observations
        .windows(2)
        .any(|pair| pair[0].date >= pair[1].date)
    {
        return Err(format!(
            "EIA returned duplicate or unordered dates for {upstream_series}."
        ));
    }
    Ok(observations)
}

fn history_start_date(now: u64) -> String {
    let now_days = i64::try_from(now / 86_400).unwrap_or(i64::MAX);
    let start_days = now_days.saturating_sub(i64::try_from(HISTORY_DAYS).unwrap_or(i64::MAX));
    let (year, month, day) = quant_core::gregorian_date_from_unix_days(start_days);
    format!("{year:04}-{month:02}-{day:02}")
}

fn current_date(now: u64) -> String {
    let now_days = i64::try_from(now / 86_400).unwrap_or(i64::MAX);
    let (year, month, day) = quant_core::gregorian_date_from_unix_days(now_days);
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

#[cfg(test)]
#[path = "eia_tests.rs"]
mod tests;
