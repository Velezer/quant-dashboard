//! FRED's time-stamped CSV snapshot adapter for the registered TIPS series.

use super::{FetchStatus, MacroIndicatorId, MacroObservation, MacroSeriesProvider, ProviderSeriesSnapshot};
use std::sync::{Mutex, OnceLock};

const FRED_CSV_SNAPSHOT: &str = include_str!("../../../../macrodata/DFII10.csv");
const MAX_SERIES_OBSERVATIONS: usize = 3_000;
const SNAPSHOT_RETRIEVED_AT_PREFIX: &str = "# retrieved_at_seconds=";

pub(super) static FRED_PROVIDER: FredProvider = FredProvider;

pub(super) struct FredProvider;

impl MacroSeriesProvider for FredProvider {
    fn read_series(&self, id: MacroIndicatorId) -> Result<ProviderSeriesSnapshot, String> {
        match id {
            MacroIndicatorId::TenYearTipsRealYield => load_ten_year_tips(),
        }
    }
}

#[derive(Clone)]
struct CachedSeries {
    observations: Vec<MacroObservation>,
    retrieved_at_seconds: u64,
}

static CACHE: OnceLock<Mutex<Option<CachedSeries>>> = OnceLock::new();

fn cache() -> &'static Mutex<Option<CachedSeries>> {
    CACHE.get_or_init(|| Mutex::new(None))
}

fn load_ten_year_tips() -> Result<ProviderSeriesSnapshot, String> {
    if let Some(current) = cache()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .as_ref()
        .cloned()
    {
        return Ok(snapshot_from_cache(&current));
    }

    let (retrieved_at_seconds, observations) = parse_fred_snapshot_csv(FRED_CSV_SNAPSHOT)?;
    let loaded = CachedSeries {
        observations,
        retrieved_at_seconds,
    };
    let mut cache = cache().lock().unwrap_or_else(|error| error.into_inner());
    if let Some(current) = cache.as_ref() {
        return Ok(snapshot_from_cache(current));
    }
    let snapshot = ProviderSeriesSnapshot {
        observations: loaded.observations.clone(),
        retrieved_at_seconds: loaded.retrieved_at_seconds,
        fetch_status: FetchStatus::Fresh,
        refresh_error: None,
    };
    *cache = Some(loaded);
    Ok(snapshot)
}

fn snapshot_from_cache(cached: &CachedSeries) -> ProviderSeriesSnapshot {
    ProviderSeriesSnapshot {
        observations: cached.observations.clone(),
        retrieved_at_seconds: cached.retrieved_at_seconds,
        fetch_status: FetchStatus::Cached,
        refresh_error: None,
    }
}

pub(super) fn parse_fred_snapshot_csv(csv: &str) -> Result<(u64, Vec<MacroObservation>), String> {
    let (metadata, csv) = csv
        .split_once('\n')
        .ok_or_else(|| "The DFII10 snapshot is missing retrieval metadata.".to_string())?;
    let retrieved_at_seconds = metadata
        .trim_end_matches('\r')
        .strip_prefix(SNAPSHOT_RETRIEVED_AT_PREFIX)
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .ok_or_else(|| "The DFII10 snapshot has invalid retrieval metadata.".to_string())?;
    let observations = parse_fred_csv(csv)?;
    Ok((retrieved_at_seconds, observations))
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
        if value.is_empty() || value == "." {
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

