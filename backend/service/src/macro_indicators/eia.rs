//! EIA weekly U.S. crude inventory series.

use super::{
    treasury::valid_date, FetchStatus, MacroIndicatorId, MacroObservation, MacroSeriesProvider,
    ProviderSeriesSnapshot,
};
use crate::public_http::{get_bounded, RequestOptions};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

const CRUDE_STOCK_SERIES_ID: &str = "WCESTUS1";
const EIA_API_URL_PREFIX: &str = "https://api.eia.gov/v2/petroleum/stoc/wstk/data/?frequency=weekly&data%5B0%5D=value&facets%5Bseries%5D%5B%5D=";
const EIA_API_URL_SUFFIX: &str =
    "&sort%5B0%5D%5Bcolumn%5D=period&sort%5B0%5D%5Bdirection%5D=desc&offset=0&length=";
const DEMO_API_KEY: &str = "DEMO_KEY";
const MAX_RESPONSE_BYTES: usize = 512 * 1024;
const MAX_SERIES_OBSERVATIONS: usize = 600;
const CACHE_TTL_SECONDS: u64 = 6 * 60 * 60;
const MINIMUM_RETRY_INTERVAL_SECONDS: u64 = 60;
const MAX_REFRESH_ERROR_CHARS: usize = 240;

pub(super) static EIA_PROVIDER: EiaProvider = EiaProvider;

pub(super) struct EiaProvider;

pub(super) fn source_series_id(id: MacroIndicatorId) -> Option<&'static str> {
    (id == MacroIndicatorId::UsCommercialCrudeStocks).then_some(CRUDE_STOCK_SERIES_ID)
}

pub(super) fn source_url(id: MacroIndicatorId) -> Option<&'static str> {
    (id == MacroIndicatorId::UsCommercialCrudeStocks)
        .then_some("https://www.eia.gov/dnav/pet/hist/LeafHandler.ashx?f=W&n=PET&s=WCESTUS1")
}

impl MacroSeriesProvider for EiaProvider {
    fn read_series(
        &self,
        id: MacroIndicatorId,
        force_refresh: bool,
    ) -> Result<ProviderSeriesSnapshot, String> {
        if source_series_id(id).is_none() {
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
    if let Some(current) = cache.get_mut(&id) {
        current.last_attempt_seconds = now;
    }

    match fetch_series(now) {
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

fn fetch_series(now: u64) -> Result<Vec<MacroObservation>, String> {
    let response = get_bounded(
        &format!(
            "{EIA_API_URL_PREFIX}{CRUDE_STOCK_SERIES_ID}{EIA_API_URL_SUFFIX}{MAX_SERIES_OBSERVATIONS}&api_key={DEMO_API_KEY}"
        ),
        RequestOptions {
            connect_timeout: Duration::from_secs(3),
            total_timeout: Duration::from_secs(8),
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
            "EIA returned HTTP {} for {}.",
            response.status, CRUDE_STOCK_SERIES_ID
        ));
    }
    let payload: Value = serde_json::from_slice(&response.body)
        .map_err(|_| "EIA returned invalid JSON for its weekly crude stock series.".to_string())?;
    parse_series(&payload, now)
}

fn parse_series(payload: &Value, now: u64) -> Result<Vec<MacroObservation>, String> {
    if payload.get("error").is_some() {
        return Err("EIA returned an API error for its weekly crude stock series.".to_string());
    }
    let rows = payload
        .pointer("/response/data")
        .and_then(Value::as_array)
        .filter(|rows| !rows.is_empty() && rows.len() <= MAX_SERIES_OBSERVATIONS)
        .ok_or_else(|| "EIA returned no bounded weekly crude stock observations.".to_string())?;
    let latest_allowed_date = current_date(now);
    let mut observations = Vec::with_capacity(rows.len());
    for row in rows {
        if row.get("series").and_then(Value::as_str) != Some(CRUDE_STOCK_SERIES_ID)
            || row.get("units").and_then(Value::as_str) != Some("MBBL")
        {
            return Err("EIA returned a different crude stock series or unit.".to_string());
        }
        let date = row
            .get("period")
            .and_then(Value::as_str)
            .filter(|date| valid_date(date))
            .ok_or_else(|| "EIA returned an invalid weekly crude stock date.".to_string())?;
        if date > latest_allowed_date.as_str() {
            return Err("EIA returned a future weekly crude stock date.".to_string());
        }
        let value = row
            .get("value")
            .and_then(Value::as_str)
            .and_then(|value| value.parse::<f64>().ok())
            .filter(|value| value.is_finite() && *value >= 0.0)
            .ok_or_else(|| "EIA returned an invalid weekly crude stock value.".to_string())?;
        observations.push(MacroObservation {
            date: date.to_string(),
            value: value / 1_000.0,
            classification: None,
        });
    }
    observations.sort_by(|left, right| left.date.cmp(&right.date));
    if observations
        .windows(2)
        .any(|pair| pair[0].date >= pair[1].date)
    {
        return Err("EIA returned duplicate weekly crude stock dates.".to_string());
    }
    Ok(observations)
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
