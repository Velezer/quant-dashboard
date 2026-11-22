//! Daily USD/IDR cross-rates calculated from European Central Bank reference data.

use super::{FetchStatus, MacroIndicatorId, MacroObservation, MacroSeriesProvider, ProviderSeriesSnapshot};
use crate::public_http::{get_bounded, RequestOptions};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

const ECB_CSV_URL: &str = "https://data-api.ecb.europa.eu/service/data/EXR";
const IDR_EUR_SERIES: &str = "D.IDR.EUR.SP00.A";
const USD_EUR_SERIES: &str = "D.USD.EUR.SP00.A";
const MAX_RESPONSE_BYTES: usize = 512 * 1024;
const MAX_SERIES_OBSERVATIONS: usize = 3_000;
const CACHE_TTL_SECONDS: u64 = 6 * 60 * 60;
const MINIMUM_RETRY_INTERVAL_SECONDS: u64 = 60;
const MAX_REFRESH_ERROR_CHARS: usize = 240;

pub(super) static ECB_PROVIDER: EcbProvider = EcbProvider;

pub(super) struct EcbProvider;

impl MacroSeriesProvider for EcbProvider {
    fn read_series(
        &self,
        id: MacroIndicatorId,
        force_refresh: bool,
    ) -> Result<ProviderSeriesSnapshot, String> {
        match id {
            MacroIndicatorId::UsdIdrReferenceRate => load_series(force_refresh),
            MacroIndicatorId::TenYearTipsRealYield | MacroIndicatorId::TwoYearTreasuryNominalYield => {
                Err("The ECB does not provide the requested macro series.".to_string())
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

fn load_series(force_refresh: bool) -> Result<ProviderSeriesSnapshot, String> {
    let id = MacroIndicatorId::UsdIdrReferenceRate;
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
    let start_date = history_start_date(now);
    let (idr_csv, usd_csv) = std::thread::scope(|scope| {
        let idr_request = scope.spawn(|| fetch_csv(IDR_EUR_SERIES, &start_date));
        let usd_request = scope.spawn(|| fetch_csv(USD_EUR_SERIES, &start_date));
        let idr = idr_request
            .join()
            .map_err(|_| "The ECB IDR/EUR request panicked.".to_string())??;
        let usd = usd_request
            .join()
            .map_err(|_| "The ECB USD/EUR request panicked.".to_string())??;
        Ok::<_, String>((idr, usd))
    })?;
    parse_cross_rate(&idr_csv, &usd_csv)
}

fn fetch_csv(series_id: &str, start_date: &str) -> Result<String, String> {
    let url = format!("{ECB_CSV_URL}/{series_id}?startPeriod={start_date}&format=csvdata");
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
        return Err(format!("The ECB returned HTTP {} for {series_id}.", response.status));
    }
    String::from_utf8(response.body)
        .map_err(|_| format!("The ECB returned CSV that was not valid UTF-8 for {series_id}."))
}

pub(super) fn parse_cross_rate(
    idr_csv: &str,
    usd_csv: &str,
) -> Result<Vec<MacroObservation>, String> {
    let idr_rates = parse_ecb_csv(idr_csv, "EXR.D.IDR.EUR.SP00.A", "IDR")?;
    let usd_rates = parse_ecb_csv(usd_csv, "EXR.D.USD.EUR.SP00.A", "USD")?;
    let mut observations = Vec::new();
    for (date, idr_per_eur) in idr_rates {
        let Some(usd_per_eur) = usd_rates.get(&date) else {
            continue;
        };
        let idr_per_usd = idr_per_eur / usd_per_eur;
        if !idr_per_usd.is_finite() || idr_per_usd <= 0.0 {
            return Err("The ECB returned an invalid USD/IDR cross-rate.".to_string());
        }
        observations.push(MacroObservation {
            date,
            value: idr_per_usd,
        });
    }
    if observations.is_empty() {
        return Err("The ECB returned no matching USD/IDR reference observations.".to_string());
    }
    if observations.len() > MAX_SERIES_OBSERVATIONS {
        observations.drain(..observations.len() - MAX_SERIES_OBSERVATIONS);
    }
    Ok(observations)
}

fn parse_ecb_csv(
    csv: &str,
    expected_key: &str,
    expected_currency: &str,
) -> Result<BTreeMap<String, f64>, String> {
    const HEADER_FIELDS: [&str; 8] = [
        "KEY",
        "FREQ",
        "CURRENCY",
        "CURRENCY_DENOM",
        "EXR_TYPE",
        "EXR_SUFFIX",
        "TIME_PERIOD",
        "OBS_VALUE",
    ];
    let mut lines = csv.lines();
    let header = lines
        .next()
        .unwrap_or_default()
        .trim_start_matches('\u{feff}')
        .trim_end_matches('\r');
    let header_fields: Vec<_> = header.split(',').take(HEADER_FIELDS.len()).collect();
    if header_fields.as_slice() != HEADER_FIELDS.as_slice() {
        return Err("The ECB returned an unexpected exchange-rate CSV header.".to_string());
    }

    let mut rates = BTreeMap::new();
    for line in lines {
        let line = line.trim_end_matches('\r').trim();
        if line.is_empty() {
            continue;
        }
        let columns: Vec<_> = line.split(',').take(8).collect();
        if columns.len() < 8
            || columns[0] != expected_key
            || columns[1] != "D"
            || columns[2] != expected_currency
            || columns[3] != "EUR"
            || columns[4] != "SP00"
            || columns[5] != "A"
        {
            return Err("The ECB returned a malformed exchange-rate CSV row.".to_string());
        }
        let date = columns[6];
        if !super::treasury::valid_date(date) {
            return Err("The ECB returned an invalid exchange-rate observation date.".to_string());
        }
        let raw_value = columns[7].trim();
        if raw_value.is_empty() || raw_value == "." {
            continue;
        }
        let value = raw_value
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite() && *value > 0.0)
            .ok_or_else(|| "The ECB returned an invalid euro reference rate.".to_string())?;
        if rates.insert(date.to_string(), value).is_some() {
            return Err("The ECB returned duplicate exchange-rate dates.".to_string());
        }
    }
    if rates.is_empty() {
        return Err("The ECB returned no euro reference observations.".to_string());
    }
    Ok(rates)
}

fn history_start_date(now: u64) -> String {
    let days = i64::try_from(now / 86_400).unwrap_or(i64::MAX);
    let (year, month, day) = quant_core::gregorian_date_from_unix_days(days);
    let start_year = year.saturating_sub(10);
    let start_day = if month == 2 && day == 29 && !is_leap_year(start_year) {
        28
    } else {
        day
    };
    format!("{start_year:04}-{month:02}-{start_day:02}")
}

fn is_leap_year(year: i64) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
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
