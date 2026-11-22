//! U.S. Treasury daily real- and nominal-yield provider.

use super::{FetchStatus, MacroIndicatorId, MacroObservation, MacroSeriesProvider, ProviderSeriesSnapshot};
use crate::public_http::{get_bounded, RequestOptions};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

const TREASURY_XML_URL: &str = "https://home.treasury.gov/resource-center/data-chart-center/interest-rates/pages/xml?data=";
const MAX_RESPONSE_BYTES_PER_YEAR: usize = 512 * 1024;
const MAX_SERIES_OBSERVATIONS: usize = 3_000;
const CACHE_TTL_SECONDS: u64 = 6 * 60 * 60;
const MINIMUM_RETRY_INTERVAL_SECONDS: u64 = 60;
const MAX_REFRESH_ERROR_CHARS: usize = 240;

pub(super) static TREASURY_PROVIDER: TreasuryProvider = TreasuryProvider;

pub(super) struct TreasuryProvider;

impl MacroSeriesProvider for TreasuryProvider {
    fn read_series(
        &self,
        id: MacroIndicatorId,
        force_refresh: bool,
    ) -> Result<ProviderSeriesSnapshot, String> {
        match id {
            MacroIndicatorId::TenYearTipsRealYield | MacroIndicatorId::TwoYearTreasuryNominalYield => {
                load_series(id, force_refresh)
            }
            MacroIndicatorId::UsdIdrReferenceRate => {
                Err("Treasury does not provide the requested macro series.".to_string())
            }
            MacroIndicatorId::BitcoinSpotEtfNetFlows | MacroIndicatorId::BitcoinFearGreedIndex => {
                Err("Treasury does not provide the requested macro series.".to_string())
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
    let now_days = i64::try_from(now_secs() / 86_400).unwrap_or(i64::MAX);
    let (current_year, _, _) = quant_core::gregorian_date_from_unix_days(now_days);
    let current_year = i32::try_from(current_year)
        .map_err(|_| "The current year is outside the Treasury data range.".to_string())?;
    let years: Vec<i32> = (current_year - 10..=current_year).collect();
    let yearly_observations: Result<Vec<Vec<MacroObservation>>, String> =
        std::thread::scope(|scope| {
            let handles: Vec<_> = years
                .iter()
                .copied()
                .map(|year| scope.spawn(move || fetch_year(year, id)))
                .collect();
            handles
                .into_iter()
                .map(|handle| {
                    handle
                        .join()
                        .unwrap_or_else(|_| Err("Treasury data request panicked.".to_string()))
                })
                .collect()
        });

    let mut observations: Vec<MacroObservation> = yearly_observations?.into_iter().flatten().collect();
    observations.sort_by(|left, right| left.date.cmp(&right.date));
    if observations.windows(2).any(|pair| pair[0].date >= pair[1].date) {
        return Err("Treasury returned unordered or duplicate yield observations.".to_string());
    }
    if observations.len() > MAX_SERIES_OBSERVATIONS {
        observations.drain(..observations.len() - MAX_SERIES_OBSERVATIONS);
    }
    Ok(observations)
}

fn fetch_year(year: i32, id: MacroIndicatorId) -> Result<Vec<MacroObservation>, String> {
    let config = series_config(id)
        .ok_or_else(|| "Treasury does not provide the requested macro series.".to_string())?;
    let url = format!("{TREASURY_XML_URL}{}&field_tdr_date_value={year}", config.dataset);
    let response = get_bounded(
        &url,
        RequestOptions {
            connect_timeout: Duration::from_secs(3),
            total_timeout: Duration::from_secs(8),
            max_body_bytes: MAX_RESPONSE_BYTES_PER_YEAR,
            max_redirects: 2,
            accept: "application/atom+xml, application/xml, text/xml",
            user_agent: Some("Quant Desk macro indicators/1.0"),
            accept_encoding: None,
            referer: None,
        },
    )?;
    if response.status >= 400 {
        return Err(format!("Treasury returned HTTP {} for {year}.", response.status));
    }
    let xml = std::str::from_utf8(&response.body)
        .map_err(|_| format!("Treasury returned XML that was not valid UTF-8 for {year}."))?;
    parse_treasury_year_xml(xml, year, id)
}

pub(super) fn parse_treasury_year_xml(
    xml: &str,
    expected_year: i32,
    id: MacroIndicatorId,
) -> Result<Vec<MacroObservation>, String> {
    if !xml.contains("<feed") {
        return Err(format!("Treasury returned an unexpected XML feed for {expected_year}."));
    }
    let config = series_config(id)
        .ok_or_else(|| "Treasury does not provide the requested macro series.".to_string())?;
    let mut observations = Vec::new();
    let mut remaining = xml;
    while let Some(entry_start) = remaining.find("<entry") {
        let after_open = entry_start + "<entry".len();
        let tag_end = remaining[after_open..]
            .find('>')
            .map(|offset| after_open + offset + 1)
            .ok_or_else(|| "Treasury returned a malformed XML entry.".to_string())?;
        let entry_end = remaining[tag_end..]
            .find("</entry>")
            .map(|offset| tag_end + offset)
            .ok_or_else(|| "Treasury returned an incomplete XML entry.".to_string())?;
        let entry = &remaining[tag_end..entry_end];
        let raw_date = xml_property(entry, "NEW_DATE")
            .ok_or_else(|| "Treasury returned an entry without a publication date.".to_string())?;
        let date = raw_date
            .trim()
            .get(..10)
            .filter(|date| valid_date(date))
            .ok_or_else(|| "Treasury returned an invalid yield observation date.".to_string())?;
        let date_year = date[..4]
            .parse::<i32>()
            .map_err(|_| "Treasury returned an invalid yield observation year.".to_string())?;
        if date_year != expected_year {
            return Err(format!("Treasury returned an observation outside {expected_year}."));
        }
        if let Some(raw_value) = xml_property(entry, config.field) {
            let raw_value = raw_value.trim();
            if !raw_value.is_empty() && raw_value != "." {
                let value = raw_value
                    .parse::<f64>()
                    .ok()
                    .filter(|value| value.is_finite())
                    .ok_or_else(|| format!("Treasury returned an invalid {}.", config.value_label))?;
                observations.push(MacroObservation {
                    date: date.to_string(),
                    value,
                    classification: None,
                });
            }
        }
        remaining = &remaining[entry_end + "</entry>".len()..];
    }
    if observations.is_empty() {
        return Err(format!("Treasury returned no {} observations for {expected_year}.", config.value_label));
    }
    if observations.windows(2).any(|pair| pair[0].date >= pair[1].date) {
        return Err(format!("Treasury returned unordered or duplicate observations for {expected_year}."));
    }
    Ok(observations)
}

fn xml_property<'a>(entry: &'a str, name: &str) -> Option<&'a str> {
    let opening = format!("<d:{name}");
    let start = entry.find(&opening)?;
    let after_opening = &entry[start..];
    let value_start = after_opening.find('>')? + 1;
    let closing = format!("</d:{name}>");
    let value_end = after_opening[value_start..].find(&closing)? + value_start;
    Some(&after_opening[value_start..value_end])
}

struct TreasurySeriesConfig {
    dataset: &'static str,
    field: &'static str,
    value_label: &'static str,
}

fn series_config(id: MacroIndicatorId) -> Option<TreasurySeriesConfig> {
    match id {
        MacroIndicatorId::TenYearTipsRealYield => Some(TreasurySeriesConfig {
            dataset: "daily_treasury_real_yield_curve",
            field: "TC_10YEAR",
            value_label: "10-year real-yield",
        }),
        MacroIndicatorId::TwoYearTreasuryNominalYield => Some(TreasurySeriesConfig {
            dataset: "daily_treasury_yield_curve",
            field: "BC_2YEAR",
            value_label: "2-year nominal-yield",
        }),
        MacroIndicatorId::UsdIdrReferenceRate
        | MacroIndicatorId::BitcoinSpotEtfNetFlows
        | MacroIndicatorId::BitcoinFearGreedIndex => None,
    }
}

pub(super) fn valid_date(date: &str) -> bool {
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
