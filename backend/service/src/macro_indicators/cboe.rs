//! Cboe daily historical volatility index files.

use super::{
    treasury::valid_date, FetchStatus, MacroIndicatorId, MacroObservation, MacroSeriesProvider,
    ProviderSeriesSnapshot,
};
use crate::public_http::{get_bounded, RequestOptions};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

const VIX_CSV_URL: &str = "https://cdn.cboe.com/api/global/us_indices/daily_prices/VIX_History.csv";
const OVX_CSV_URL: &str = "https://cdn.cboe.com/api/global/us_indices/daily_prices/OVX_History.csv";
const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
const MAX_SERIES_OBSERVATIONS: usize = 3_000;
const MAX_CSV_ROWS: usize = 20_000;
const HISTORY_DAYS: u64 = 3_653;
const CACHE_TTL_SECONDS: u64 = 6 * 60 * 60;
const MINIMUM_RETRY_INTERVAL_SECONDS: u64 = 60;
const MAX_REFRESH_ERROR_CHARS: usize = 240;

pub(super) static CBOE_PROVIDER: CboeProvider = CboeProvider;

pub(super) struct CboeProvider;

fn config(id: MacroIndicatorId) -> Option<(&'static str, &'static str, &'static str)> {
    match id {
        MacroIndicatorId::UsEquityVolatilityIndex => Some((VIX_CSV_URL, "CLOSE", "VIX")),
        MacroIndicatorId::CrudeOilVolatilityIndex => Some((OVX_CSV_URL, "OVX", "OVX")),
        _ => None,
    }
}

pub(super) fn source_series_id(id: MacroIndicatorId) -> Option<&'static str> {
    config(id).map(|(_, _, series_id)| series_id)
}

pub(super) fn source_url(id: MacroIndicatorId) -> Option<&'static str> {
    config(id).map(|(url, _, _)| url)
}

impl MacroSeriesProvider for CboeProvider {
    fn read_series(
        &self,
        id: MacroIndicatorId,
        force_refresh: bool,
    ) -> Result<ProviderSeriesSnapshot, String> {
        if config(id).is_none() {
            return Err("Cboe does not provide the requested volatility series.".to_string());
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
    let (url, column, series_id) = config(id)
        .ok_or_else(|| "Cboe does not provide the requested volatility series.".to_string())?;
    let response = get_bounded(
        url,
        RequestOptions {
            connect_timeout: Duration::from_secs(3),
            total_timeout: Duration::from_secs(10),
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
            "Cboe returned HTTP {} for {series_id}.",
            response.status
        ));
    }
    let csv = std::str::from_utf8(&response.body)
        .map_err(|_| format!("Cboe returned invalid UTF-8 for {series_id}."))?;
    parse_csv(csv, column, series_id, now)
}

fn parse_csv(
    csv: &str,
    value_column: &str,
    series_id: &str,
    now: u64,
) -> Result<Vec<MacroObservation>, String> {
    let mut lines = csv.lines();
    let header = lines
        .next()
        .unwrap_or_default()
        .trim_start_matches('\u{feff}')
        .trim();
    let columns: Vec<&str> = header.split(',').map(str::trim).collect();
    let date_index = columns.iter().position(|column| *column == "DATE");
    let value_index = columns.iter().position(|column| *column == value_column);
    let (Some(date_index), Some(value_index)) = (date_index, value_index) else {
        return Err(format!(
            "Cboe returned an unexpected CSV header for {series_id}."
        ));
    };
    let start = history_start_date(now);
    let latest_allowed_date = current_date(now);
    let mut previous_date: Option<String> = None;
    let mut observations = Vec::new();
    for (index, line) in lines.enumerate() {
        if index >= MAX_CSV_ROWS {
            return Err(format!("Cboe returned too many CSV rows for {series_id}."));
        }
        let fields: Vec<&str> = line.split(',').map(str::trim).collect();
        let raw_date = fields
            .get(date_index)
            .copied()
            .ok_or_else(|| format!("Cboe returned a malformed row for {series_id}."))?;
        let date = parse_us_date(raw_date)
            .ok_or_else(|| format!("Cboe returned an invalid date for {series_id}."))?;
        if date > latest_allowed_date
            || previous_date
                .as_deref()
                .is_some_and(|previous| previous >= date.as_str())
        {
            return Err(format!(
                "Cboe returned unordered or future data for {series_id}."
            ));
        }
        previous_date = Some(date.clone());
        if date < start {
            continue;
        }
        let raw_value = fields
            .get(value_index)
            .copied()
            .ok_or_else(|| format!("Cboe returned a malformed value for {series_id}."))?;
        if raw_value.is_empty() || raw_value == "." || raw_value == "-" {
            continue;
        }
        let value = raw_value
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite() && *value >= 0.0)
            .ok_or_else(|| format!("Cboe returned an invalid value for {series_id}."))?;
        observations.push(MacroObservation {
            date,
            value,
            classification: None,
        });
        if observations.len() > MAX_SERIES_OBSERVATIONS {
            return Err(format!(
                "Cboe returned too many recent observations for {series_id}."
            ));
        }
    }
    if observations.is_empty() {
        return Err(format!(
            "Cboe returned no recent observations for {series_id}."
        ));
    }
    Ok(observations)
}

fn parse_us_date(value: &str) -> Option<String> {
    let mut parts = value.split('/');
    let month = parts.next()?.parse::<u32>().ok()?;
    let day = parts.next()?.parse::<u32>().ok()?;
    let year = parts.next()?.parse::<i32>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    let date = format!("{year:04}-{month:02}-{day:02}");
    valid_date(&date).then_some(date)
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
#[path = "cboe_tests.rs"]
mod tests;
