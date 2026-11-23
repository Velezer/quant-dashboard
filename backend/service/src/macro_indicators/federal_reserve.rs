//! Federal Reserve H.10 and G.17 macroeconomic series.

use super::{
    treasury::valid_date, FetchStatus, MacroIndicatorId, MacroObservation, MacroSeriesProvider,
    ProviderSeriesSnapshot,
};
use crate::public_http::{get_bounded, RequestOptions};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

const H10_CSV_URL: &str = "https://www.federalreserve.gov/datadownload/Output.aspx?rel=H10&series=122e3bcb627e8e53f1bf72a1a09cfb81&filetype=csv&label=include&layout=seriescolumn";
const G17_MANUFACTURING_URL: &str =
    "https://www.federalreserve.gov/releases/g17/current/ipdisk/ip_sa.txt";
const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
const MAX_SERIES_OBSERVATIONS: usize = 3_000;
const HISTORY_DAYS: u64 = 3_653;
const CACHE_TTL_SECONDS: u64 = 6 * 60 * 60;
const MINIMUM_RETRY_INTERVAL_SECONDS: u64 = 60;
const MAX_REFRESH_ERROR_CHARS: usize = 240;

pub(super) static FEDERAL_RESERVE_PROVIDER: FederalReserveProvider = FederalReserveProvider;

pub(super) struct FederalReserveProvider;

pub(super) fn source_series_id(id: MacroIndicatorId) -> Option<&'static str> {
    match id {
        MacroIndicatorId::BroadUsDollarIndex => Some("JRXWTFB_N.B"),
        MacroIndicatorId::UsManufacturingIndustrialProduction => Some("GMF"),
        _ => None,
    }
}

pub(super) fn source_url(id: MacroIndicatorId) -> Option<&'static str> {
    match id {
        MacroIndicatorId::BroadUsDollarIndex => {
            Some("https://www.federalreserve.gov/releases/h10/current/")
        }
        MacroIndicatorId::UsManufacturingIndustrialProduction => {
            Some("https://www.federalreserve.gov/releases/g17/download.htm")
        }
        _ => None,
    }
}

impl MacroSeriesProvider for FederalReserveProvider {
    fn read_series(
        &self,
        id: MacroIndicatorId,
        force_refresh: bool,
    ) -> Result<ProviderSeriesSnapshot, String> {
        if source_series_id(id).is_none() {
            return Err("The Federal Reserve does not provide the requested series.".to_string());
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
    let (url, accept) = match id {
        MacroIndicatorId::BroadUsDollarIndex => (H10_CSV_URL, "text/csv"),
        MacroIndicatorId::UsManufacturingIndustrialProduction => {
            (G17_MANUFACTURING_URL, "text/plain")
        }
        _ => return Err("The Federal Reserve does not provide the requested series.".to_string()),
    };
    let response = get_bounded(
        url,
        RequestOptions {
            connect_timeout: Duration::from_secs(3),
            total_timeout: Duration::from_secs(10),
            max_body_bytes: MAX_RESPONSE_BYTES,
            max_redirects: 2,
            accept,
            user_agent: Some("Quant Desk macro indicators/1.0"),
            accept_encoding: None,
            referer: None,
        },
    )?;
    if response.status >= 400 {
        return Err(format!(
            "The Federal Reserve returned HTTP {} for {}.",
            response.status,
            source_series_id(id).unwrap_or("the requested series")
        ));
    }
    let body = std::str::from_utf8(&response.body)
        .map_err(|_| "The Federal Reserve returned invalid UTF-8 macro data.".to_string())?;
    match id {
        MacroIndicatorId::BroadUsDollarIndex => parse_h10_csv(body, now),
        MacroIndicatorId::UsManufacturingIndustrialProduction => parse_g17_manufacturing(body, now),
        _ => Err("The Federal Reserve does not provide the requested series.".to_string()),
    }
}

fn parse_h10_csv(csv: &str, now: u64) -> Result<Vec<MacroObservation>, String> {
    let mut lines = csv.lines();
    let description = lines
        .next()
        .unwrap_or_default()
        .trim_start_matches('\u{feff}');
    if !description.contains("Nominal Broad Dollar Index") {
        return Err("The Federal Reserve H.10 download changed its series header.".to_string());
    }
    for _ in 0..4 {
        if lines.next().is_none() {
            return Err("The Federal Reserve H.10 download is incomplete.".to_string());
        }
    }
    let header = lines.next().unwrap_or_default();
    let columns: Vec<&str> = header
        .split(',')
        .map(|column| column.trim().trim_matches('"'))
        .collect();
    let value_index = columns
        .iter()
        .position(|column| *column == "JRXWTFB_N.B")
        .filter(|_| columns.first().copied() == Some("Time Period"))
        .ok_or_else(|| {
            "The Federal Reserve H.10 download is missing the broad dollar series.".to_string()
        })?;
    let start = history_start_date(now);
    let latest_allowed_date = current_date(now);
    let mut previous_date: Option<String> = None;
    let mut observations = Vec::new();
    for line in lines {
        let columns: Vec<&str> = line.split(',').map(str::trim).collect();
        let date = columns
            .first()
            .copied()
            .filter(|date| valid_date(date))
            .ok_or_else(|| {
                "The Federal Reserve H.10 download contains an invalid date.".to_string()
            })?;
        if date > latest_allowed_date.as_str()
            || previous_date
                .as_deref()
                .is_some_and(|previous| previous >= date)
        {
            return Err(
                "The Federal Reserve H.10 download is unordered or contains a future date."
                    .to_string(),
            );
        }
        previous_date = Some(date.to_string());
        if date < start.as_str() {
            continue;
        }
        let raw_value = columns.get(value_index).copied().ok_or_else(|| {
            "The Federal Reserve H.10 download contains a malformed row.".to_string()
        })?;
        if raw_value == "ND" || raw_value == "." || raw_value.is_empty() {
            continue;
        }
        let value = raw_value
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite() && *value > 0.0)
            .ok_or_else(|| {
                "The Federal Reserve H.10 download contains an invalid index value.".to_string()
            })?;
        observations.push(MacroObservation {
            date: date.to_string(),
            value,
            classification: None,
        });
        if observations.len() > MAX_SERIES_OBSERVATIONS {
            return Err(
                "The Federal Reserve H.10 download returned too many recent observations."
                    .to_string(),
            );
        }
    }
    if observations.is_empty() {
        return Err(
            "The Federal Reserve H.10 download contains no recent broad dollar observations."
                .to_string(),
        );
    }
    Ok(observations)
}

fn parse_g17_manufacturing(text: &str, now: u64) -> Result<Vec<MacroObservation>, String> {
    if !text.contains("\"GMF: Manufacturing (NAICS)\"") {
        return Err(
            "The Federal Reserve G.17 download changed its manufacturing series header."
                .to_string(),
        );
    }
    let start = history_start_date(now);
    let latest_allowed_date = current_date(now);
    let mut found_series = false;
    let mut previous_date: Option<String> = None;
    let mut observations = Vec::new();
    for line in text.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.first() != Some(&"\"GMF\"") {
            if line.trim() == "\"GMF: Manufacturing (NAICS)\"" {
                found_series = true;
            }
            continue;
        }
        if !found_series {
            return Err(
                "The Federal Reserve G.17 manufacturing section is out of order.".to_string(),
            );
        }
        let year = fields
            .get(1)
            .and_then(|year| year.parse::<i32>().ok())
            .filter(|year| (1900..=9999).contains(year))
            .ok_or_else(|| {
                "The Federal Reserve G.17 download contains an invalid year.".to_string()
            })?;
        for month in 1..=12 {
            let Some(raw_value) = fields.get(month + 1).copied() else {
                continue;
            };
            if raw_value == "." || raw_value == "-" {
                continue;
            }
            let value = raw_value
                .parse::<f64>()
                .ok()
                .filter(|value| value.is_finite() && *value > 0.0)
                .ok_or_else(|| {
                    "The Federal Reserve G.17 download contains an invalid production value."
                        .to_string()
                })?;
            let date = format!("{year:04}-{month:02}-01");
            if !valid_date(&date) {
                return Err(
                    "The Federal Reserve G.17 download contains an invalid monthly date."
                        .to_string(),
                );
            }
            if date > latest_allowed_date {
                continue;
            }
            if previous_date
                .as_deref()
                .is_some_and(|previous| previous >= date.as_str())
            {
                return Err("The Federal Reserve G.17 manufacturing observations are unordered or duplicated.".to_string());
            }
            previous_date = Some(date.clone());
            if date < start {
                continue;
            }
            observations.push(MacroObservation {
                date,
                value,
                classification: None,
            });
            if observations.len() > MAX_SERIES_OBSERVATIONS {
                return Err(
                    "The Federal Reserve G.17 download returned too many recent observations."
                        .to_string(),
                );
            }
        }
    }
    if observations.is_empty() {
        return Err(
            "The Federal Reserve G.17 download contains no recent manufacturing observations."
                .to_string(),
        );
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
#[path = "federal_reserve_tests.rs"]
mod tests;
