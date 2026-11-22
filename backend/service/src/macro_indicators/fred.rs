//! FRED CSV provider for U.S. 2-year rates and OECD monthly USD/IDR averages.

use super::{FetchStatus, MacroIndicatorId, MacroObservation, MacroSeriesProvider, ProviderSeriesSnapshot};
use crate::public_http::{get_bounded, RequestOptions};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

const FRED_CSV_URL: &str = "https://fred.stlouisfed.org/graph/fredgraph.csv";
const MAX_RESPONSE_BYTES: usize = 512 * 1024;
const MAX_SERIES_OBSERVATIONS: usize = 3_000;
const CACHE_TTL_SECONDS: u64 = 6 * 60 * 60;
const MINIMUM_RETRY_INTERVAL_SECONDS: u64 = 60;
const MAX_REFRESH_ERROR_CHARS: usize = 240;

pub(super) static FRED_PROVIDER: FredProvider = FredProvider;

pub(super) struct FredProvider;

impl MacroSeriesProvider for FredProvider {
    fn read_series(
        &self,
        id: MacroIndicatorId,
        force_refresh: bool,
    ) -> Result<ProviderSeriesSnapshot, String> {
        match id {
            MacroIndicatorId::TwoYearTreasuryNominalYield | MacroIndicatorId::UsdIdrMonthlyAverage => {
                load_series(id, force_refresh)
            }
            MacroIndicatorId::TenYearTipsRealYield => {
                Err("FRED does not provide the requested macro series.".to_string())
            }
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

    match fetch_series(id) {
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

fn fetch_series(id: MacroIndicatorId) -> Result<Vec<MacroObservation>, String> {
    let series_id = series_id(id)
        .ok_or_else(|| "FRED does not provide the requested macro series.".to_string())?;
    let start_date = history_start_date(now_secs());
    let url = format!("{FRED_CSV_URL}?id={series_id}&cosd={start_date}");
    let response = get_bounded(
        &url,
        RequestOptions {
            connect_timeout: Duration::from_secs(3),
            total_timeout: Duration::from_secs(8),
            max_body_bytes: MAX_RESPONSE_BYTES,
            max_redirects: 2,
            accept: "text/csv, application/csv",
            user_agent: Some("Quant Desk macro indicators/1.0"),
            accept_encoding: None,
            referer: None,
        },
    )?;
    if response.status >= 400 {
        return Err(format!("FRED returned HTTP {} for the {series_id} series.", response.status));
    }
    let csv = std::str::from_utf8(&response.body)
        .map_err(|_| "FRED returned CSV that was not valid UTF-8.".to_string())?;
    parse_fred_csv(csv, id)
}

pub(super) fn parse_fred_csv(
    csv: &str,
    id: MacroIndicatorId,
) -> Result<Vec<MacroObservation>, String> {
    let series_id = series_id(id)
        .ok_or_else(|| "FRED does not provide the requested macro series.".to_string())?;
    let expected_header = format!("observation_date,{series_id}");
    let mut lines = csv.lines();
    let header = lines
        .next()
        .unwrap_or_default()
        .trim_start_matches('\u{feff}')
        .trim_end_matches('\r');
    if header != expected_header {
        return Err(format!("FRED returned an unexpected {series_id} CSV header."));
    }

    let mut observations = Vec::new();
    for line in lines {
        let line = line.trim_end_matches('\r').trim();
        if line.is_empty() {
            continue;
        }
        let (date, raw_value) = line
            .split_once(',')
            .ok_or_else(|| format!("FRED returned a malformed {series_id} CSV row."))?;
        if !super::treasury::valid_date(date) {
            return Err(format!("FRED returned an invalid {series_id} observation date."));
        }
        let raw_value = raw_value.trim();
        if raw_value.is_empty() || raw_value == "." {
            continue;
        }
        let value = raw_value
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite() && (id != MacroIndicatorId::UsdIdrMonthlyAverage || *value > 0.0))
            .ok_or_else(|| format!("FRED returned an invalid {series_id} value."))?;
        let date = if id == MacroIndicatorId::UsdIdrMonthlyAverage {
            month_end_date(date)?
        } else {
            date.to_string()
        };
        observations.push(MacroObservation {
            date,
            value,
        });
    }

    if observations.is_empty() {
        return Err(format!("FRED returned no {series_id} observations."));
    }
    if observations.windows(2).any(|pair| pair[0].date >= pair[1].date) {
        return Err(format!("FRED returned unordered or duplicate {series_id} observations."));
    }
    if observations.len() > MAX_SERIES_OBSERVATIONS {
        observations.drain(..observations.len() - MAX_SERIES_OBSERVATIONS);
    }
    Ok(observations)
}

fn series_id(id: MacroIndicatorId) -> Option<&'static str> {
    match id {
        MacroIndicatorId::TwoYearTreasuryNominalYield => Some("DGS2"),
        MacroIndicatorId::UsdIdrMonthlyAverage => Some("CCUSMA02IDM618N"),
        MacroIndicatorId::TenYearTipsRealYield => None,
    }
}

fn month_end_date(date: &str) -> Result<String, String> {
    if date.get(8..) != Some("01") {
        return Err("FRED returned a USD/IDR observation outside a monthly period boundary.".to_string());
    }
    let year = date[..4]
        .parse::<i32>()
        .map_err(|_| "FRED returned an invalid USD/IDR observation year.".to_string())?;
    let month = date[5..7]
        .parse::<u32>()
        .map_err(|_| "FRED returned an invalid USD/IDR observation month.".to_string())?;
    let leap_year = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let last_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap_year => 29,
        2 => 28,
        _ => return Err("FRED returned an invalid USD/IDR observation month.".to_string()),
    };
    Ok(format!("{year:04}-{month:02}-{last_day:02}"))
}

fn history_start_date(now_seconds: u64) -> String {
    let days = i64::try_from(now_seconds / 86_400).unwrap_or(i64::MAX);
    let (year, _, _) = quant_core::gregorian_date_from_unix_days(days);
    format!("{:04}-01-01", year.saturating_sub(10))
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
