//! FRED's CSV protocol and bounded transport adapter for the registered TIPS series.

use super::{FetchStatus, MacroIndicatorId, MacroObservation, MacroSeriesProvider, ProviderSeriesSnapshot};
use crate::public_http::{get_bounded, RequestOptions};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

const FRED_CSV_URL: &str =
    "https://fred.stlouisfed.org/graph/fredgraph.csv?id=DFII10&cosd=2015-01-01";
const MAX_CSV_BYTES: usize = 256 * 1024;
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
            MacroIndicatorId::TenYearTipsRealYield => load_ten_year_tips(force_refresh),
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

static CACHE: OnceLock<Mutex<Option<CachedSeries>>> = OnceLock::new();

fn cache() -> &'static Mutex<Option<CachedSeries>> {
    CACHE.get_or_init(|| Mutex::new(None))
}

fn load_ten_year_tips(force_refresh: bool) -> Result<ProviderSeriesSnapshot, String> {
    let now = now_secs();
    let mut cache = cache().lock().unwrap_or_else(|error| error.into_inner());
    if let Some(current) = cache.as_ref() {
        let recently_retrieved = now.saturating_sub(current.retrieved_at_seconds) < CACHE_TTL_SECONDS;
        let retry_too_soon = now.saturating_sub(current.last_attempt_seconds)
            < MINIMUM_RETRY_INTERVAL_SECONDS;
        if (recently_retrieved && !force_refresh) || retry_too_soon {
            return Ok(snapshot_from_cache(current, current.refresh_error.is_some()));
        }
    }
    if let Some(current) = cache.as_mut() {
        current.last_attempt_seconds = now;
    }

    match fetch_ten_year_tips() {
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
            *cache = Some(refreshed);
            Ok(snapshot)
        }
        Err(error) => {
            if let Some(current) = cache.as_mut() {
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

fn fetch_ten_year_tips() -> Result<Vec<MacroObservation>, String> {
    let response = get_bounded(
        FRED_CSV_URL,
        RequestOptions {
            connect_timeout: Duration::from_secs(3),
            total_timeout: Duration::from_secs(20),
            max_body_bytes: MAX_CSV_BYTES,
            max_redirects: 2,
            accept: "text/csv",
            user_agent: Some("Quant Desk macro indicators/1.0"),
            accept_encoding: None,
            referer: None,
        },
    )?;
    if response.status >= 400 {
        return Err(format!("FRED returned HTTP {}.", response.status));
    }
    let csv = std::str::from_utf8(&response.body)
        .map_err(|_| "FRED returned CSV that was not valid UTF-8.".to_string())?;
    parse_fred_csv(csv)
}

pub(super) fn parse_fred_csv(csv: &str) -> Result<Vec<MacroObservation>, String> {
    let mut lines = csv.lines();
    let header = lines
        .next()
        .map(|line| line.trim_start_matches('\u{feff}').trim())
        .unwrap_or_default();
    if header != "observation_date,DFII10" {
        return Err("FRED returned an unexpected DFII10 CSV header.".to_string());
    }

    let mut observations = Vec::with_capacity(MAX_SERIES_OBSERVATIONS);
    for line in lines {
        let Some((raw_date, raw_value)) = line.trim().split_once(',') else {
            if line.trim().is_empty() {
                continue;
            }
            return Err("FRED returned a malformed DFII10 observation row.".to_string());
        };
        let date = raw_date.trim();
        let value = raw_value.trim();
        if !valid_date(date) {
            return Err("FRED returned an invalid DFII10 observation date.".to_string());
        }
        if value == "." {
            continue;
        }
        let value = value
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
            .ok_or_else(|| "FRED returned an invalid DFII10 yield value.".to_string())?;
        observations.push(MacroObservation {
            date: date.to_string(),
            value,
        });
    }
    if observations.is_empty() {
        return Err("FRED returned no usable DFII10 observations.".to_string());
    }
    if observations.windows(2).any(|pair| pair[0].date >= pair[1].date) {
        return Err("FRED returned unordered or duplicate DFII10 dates.".to_string());
    }
    if observations.len() > MAX_SERIES_OBSERVATIONS {
        observations.drain(..observations.len() - MAX_SERIES_OBSERVATIONS);
    }
    Ok(observations)
}

fn valid_date(date: &str) -> bool {
    let bytes = date.as_bytes();
    let shape_is_valid = bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit());
    if !shape_is_valid {
        return false;
    }
    let year = date[0..4].parse::<i32>().ok();
    let month = date[5..7].parse::<u32>().ok();
    let day = date[8..10].parse::<u32>().ok();
    let (Some(year), Some(month), Some(day)) = (year, month, day) else {
        return false;
    };
    let leap_year = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days_in_month = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap_year => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days_in_month).contains(&day)
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
