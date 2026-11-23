//! Bounded, cached access to one-series FRED CSV endpoints.

use super::{
    treasury::valid_date, FetchStatus, MacroIndicatorId, MacroObservation, MacroSeriesProvider,
    ProviderSeriesSnapshot,
};
use crate::public_http::{get_bounded, RequestOptions};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

const FRED_CSV_URL: &str = "https://fred.stlouisfed.org/graph/fredgraph.csv?id=";
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const MAX_SERIES_OBSERVATIONS: usize = 3_000;
const HISTORY_DAYS: u64 = 3_653;
const CACHE_TTL_SECONDS: u64 = 6 * 60 * 60;
const MINIMUM_RETRY_INTERVAL_SECONDS: u64 = 60;
const MAX_REFRESH_ERROR_CHARS: usize = 240;

pub(super) static FRED_PROVIDER: FredProvider = FredProvider;

pub(super) struct FredProvider;

#[derive(Clone, Copy)]
struct FredSeriesConfig {
    series_id: &'static str,
    source_url: &'static str,
}

fn config(id: MacroIndicatorId) -> Option<FredSeriesConfig> {
    let (series_id, source_url) = match id {
        MacroIndicatorId::BroadUsDollarIndex => {
            ("DTWEXBGS", "https://fred.stlouisfed.org/series/DTWEXBGS")
        }
        MacroIndicatorId::TenYearTreasuryNominalYield => {
            ("DGS10", "https://fred.stlouisfed.org/series/DGS10")
        }
        MacroIndicatorId::TenYearBreakevenInflation => {
            ("T10YIE", "https://fred.stlouisfed.org/series/T10YIE")
        }
        MacroIndicatorId::UsEquityVolatilityIndex => {
            ("VIXCLS", "https://fred.stlouisfed.org/series/VIXCLS")
        }
        MacroIndicatorId::UsManufacturingIndustrialProduction => {
            ("IPMAN", "https://fred.stlouisfed.org/series/IPMAN")
        }
        MacroIndicatorId::BrentCrudeSpot => (
            "DCOILBRENTEU",
            "https://fred.stlouisfed.org/series/DCOILBRENTEU",
        ),
        MacroIndicatorId::WtiCrudeSpot => (
            "DCOILWTICO",
            "https://fred.stlouisfed.org/series/DCOILWTICO",
        ),
        MacroIndicatorId::CrudeOilVolatilityIndex => {
            ("OVXCLS", "https://fred.stlouisfed.org/series/OVXCLS")
        }
        _ => return None,
    };
    Some(FredSeriesConfig {
        series_id,
        source_url,
    })
}

pub(super) fn source_series_id(id: MacroIndicatorId) -> Option<&'static str> {
    config(id).map(|series| series.series_id)
}

pub(super) fn source_url(id: MacroIndicatorId) -> Option<&'static str> {
    config(id).map(|series| series.source_url)
}

impl MacroSeriesProvider for FredProvider {
    fn read_series(
        &self,
        id: MacroIndicatorId,
        force_refresh: bool,
    ) -> Result<ProviderSeriesSnapshot, String> {
        if config(id).is_none() {
            return Err("FRED does not provide the requested macro series.".to_string());
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
    let series = config(id)
        .ok_or_else(|| "FRED does not provide the requested macro series.".to_string())?;
    let start = history_start_date(now);
    let url = format!("{FRED_CSV_URL}{}&cosd={start}", series.series_id);
    let response = get_bounded(
        &url,
        RequestOptions {
            connect_timeout: Duration::from_secs(3),
            total_timeout: Duration::from_secs(8),
            max_body_bytes: MAX_RESPONSE_BYTES,
            max_redirects: 2,
            accept: "text/csv",
            user_agent: Some("Quant Desk macro indicators/1.0"),
            accept_encoding: None,
            referer: None,
        },
    )?;
    if response.status >= 400 {
        return Err(format!(
            "FRED returned HTTP {} for {}.",
            response.status, series.series_id
        ));
    }
    let csv = std::str::from_utf8(&response.body)
        .map_err(|_| format!("FRED returned invalid UTF-8 for {}.", series.series_id))?;
    parse_csv(csv, series.series_id, now)
}

fn parse_csv(csv: &str, series_id: &str, now: u64) -> Result<Vec<MacroObservation>, String> {
    let mut lines = csv.lines();
    let header = lines
        .next()
        .unwrap_or_default()
        .trim_start_matches('\u{feff}')
        .trim();
    if header != format!("observation_date,{series_id}") {
        return Err(format!(
            "FRED returned an unexpected CSV header for {series_id}."
        ));
    }
    let latest_allowed_date = current_date(now);
    let mut observations = Vec::new();
    for (index, line) in lines.enumerate() {
        if index >= MAX_SERIES_OBSERVATIONS {
            return Err(format!(
                "FRED returned too many observations for {series_id}."
            ));
        }
        let Some((date, raw_value)) = line.trim().split_once(',') else {
            return Err(format!("FRED returned a malformed row for {series_id}."));
        };
        let date = date.trim();
        let raw_value = raw_value.trim();
        if !valid_date(date) || date > latest_allowed_date.as_str() {
            return Err(format!(
                "FRED returned an invalid or future date for {series_id}."
            ));
        }
        if raw_value.is_empty() || raw_value == "." {
            continue;
        }
        let value = raw_value
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
            .ok_or_else(|| format!("FRED returned a nonnumeric value for {series_id}."))?;
        observations.push(MacroObservation {
            date: date.to_string(),
            value,
            classification: None,
        });
    }
    if observations.is_empty() {
        return Err(format!("FRED returned no observations for {series_id}."));
    }
    if observations
        .windows(2)
        .any(|pair| pair[0].date >= pair[1].date)
    {
        return Err(format!(
            "FRED returned unordered or duplicate dates for {series_id}."
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
#[path = "fred_tests.rs"]
mod tests;
