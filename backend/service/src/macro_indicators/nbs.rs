//! Bounded, cached access to the official China NBS manufacturing PMI series.

use super::{
    treasury::valid_date, FetchStatus, MacroIndicatorId, MacroObservation, MacroSeriesProvider,
    ProviderSeriesSnapshot,
};
use crate::public_http::{post_bounded, RequestOptions};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

const PMI_CATALOG_ID: &str = "93ffbb1aa85740d3aa2618371508b606";
const PMI_INDICATOR_ID: &str = "a09aa989bdcf4cffa2021795722eb916";
const PMI_SOURCE_SEARCH_URL: &str = "https://data.stats.gov.cn/dg/website/publicrelease/web/external/query?search=PMI&pagenum=1&pageSize=10";
const NBS_DATA_URL: &str =
    "https://data.stats.gov.cn/dg/website/publicrelease/web/external/stream/esData";
const MAX_RESPONSE_BYTES: usize = 256 * 1024;
const MAX_SERIES_OBSERVATIONS: usize = 300;
const CACHE_TTL_SECONDS: u64 = 6 * 60 * 60;
const MINIMUM_RETRY_INTERVAL_SECONDS: u64 = 60;
const MAX_REFRESH_ERROR_CHARS: usize = 240;

pub(super) static NBS_PROVIDER: NbsProvider = NbsProvider;

pub(super) struct NbsProvider;

pub(super) fn source_series_id(id: MacroIndicatorId) -> Option<&'static str> {
    (id == MacroIndicatorId::ChinaManufacturingPmi).then_some("NBS nationwide manufacturing PMI")
}

pub(super) fn source_url(id: MacroIndicatorId) -> Option<&'static str> {
    (id == MacroIndicatorId::ChinaManufacturingPmi).then_some(PMI_SOURCE_SEARCH_URL)
}

impl MacroSeriesProvider for NbsProvider {
    fn read_series(
        &self,
        id: MacroIndicatorId,
        force_refresh: bool,
    ) -> Result<ProviderSeriesSnapshot, String> {
        if source_series_id(id).is_none() {
            return Err("NBS does not provide the requested macro series.".to_string());
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
            cache()
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .insert(id, refreshed);
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

fn fetch_series(now: u64) -> Result<Vec<MacroObservation>, String> {
    let current_month = current_date(now)
        .get(..7)
        .ok_or_else(|| "The current date could not be formatted for the NBS request.".to_string())?
        .replace('-', "");
    let payload = json!({
        "cid": PMI_CATALOG_ID,
        "indicatorIds": [PMI_INDICATOR_ID],
        "das": [{ "text": "全国", "value": "000000000000" }],
        "dts": [format!("200501MM-{current_month}MM")],
        "showType": "1",
        "rootId": "fc982599aa684be7969d7b90b1bd0e84"
    })
    .to_string();
    let response = post_bounded(
        NBS_DATA_URL,
        &payload,
        RequestOptions {
            connect_timeout: Duration::from_secs(4),
            total_timeout: Duration::from_secs(12),
            max_body_bytes: MAX_RESPONSE_BYTES,
            max_redirects: 2,
            accept: "application/json",
            user_agent: Some("Quant Desk macro indicators/1.0"),
            accept_encoding: None,
            referer: Some("https://data.stats.gov.cn/"),
        },
    )?;
    if response.status >= 400 {
        return Err(format!(
            "China NBS returned HTTP {} for PMI.",
            response.status
        ));
    }
    let payload: Value = serde_json::from_slice(&response.body)
        .map_err(|_| "China NBS returned invalid JSON for the manufacturing PMI.".to_string())?;
    parse_series(&payload, now)
}

fn parse_series(payload: &Value, now: u64) -> Result<Vec<MacroObservation>, String> {
    let rows = payload
        .get("data")
        .and_then(Value::as_array)
        .filter(|rows| !rows.is_empty() && rows.len() <= MAX_SERIES_OBSERVATIONS)
        .ok_or_else(|| {
            "China NBS returned no bounded manufacturing PMI observations.".to_string()
        })?;
    let latest_allowed_date = current_date(now);
    let mut observations = Vec::with_capacity(rows.len());
    for row in rows {
        let period = row
            .get("code")
            .and_then(Value::as_str)
            .and_then(|code| code.strip_suffix("MM"))
            .filter(|period| period.len() == 6 && period.bytes().all(|byte| byte.is_ascii_digit()))
            .ok_or_else(|| "China NBS returned an invalid PMI period.".to_string())?;
        let date = format!("{}-{}-01", &period[..4], &period[4..]);
        if !valid_date(&date) || date > latest_allowed_date {
            return Err("China NBS returned an invalid or future PMI date.".to_string());
        }
        let values = row
            .get("values")
            .and_then(Value::as_array)
            .filter(|values| values.len() == 1)
            .ok_or_else(|| "China NBS returned an unexpected PMI observation shape.".to_string())?;
        let value = &values[0];
        if value.get("_id").and_then(Value::as_str) != Some(PMI_INDICATOR_ID)
            || value.get("catalogid").and_then(Value::as_str) != Some(PMI_CATALOG_ID)
            || value.get("da").and_then(Value::as_str) != Some("000000000000")
        {
            return Err("China NBS returned a different PMI series or geography.".to_string());
        }
        let raw_value = value
            .get("value")
            .and_then(Value::as_str)
            .map(str::trim)
            .ok_or_else(|| "China NBS returned an invalid manufacturing PMI value.".to_string())?;
        if raw_value.is_empty() || raw_value == "." {
            continue;
        }
        let numeric_value = raw_value
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite() && (0.0..=100.0).contains(value))
            .ok_or_else(|| "China NBS returned an invalid manufacturing PMI value.".to_string())?;
        observations.push(MacroObservation {
            date,
            value: numeric_value,
            classification: None,
        });
    }
    if observations.is_empty() {
        return Err("China NBS returned no published manufacturing PMI values.".to_string());
    }
    observations.sort_by(|left, right| left.date.cmp(&right.date));
    if observations
        .windows(2)
        .any(|pair| pair[0].date >= pair[1].date)
    {
        return Err("China NBS returned duplicate manufacturing PMI dates.".to_string());
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
#[path = "nbs_tests.rs"]
mod tests;
