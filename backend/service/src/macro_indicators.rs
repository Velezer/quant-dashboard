//! Application-owned macro indicator data contract and provider selection.
//! Macro observations are descriptive context and do not enter ticker scores.

mod fred;

use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(test)]
#[path = "macro_indicators_tests.rs"]
mod tests;

const MAX_OBSERVATION_AGE_DAYS: i64 = 7;
const MAX_SNAPSHOT_AGE_SECONDS: u64 = MAX_OBSERVATION_AGE_DAYS as u64 * 86_400;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MacroIndicatorId {
    TenYearTipsRealYield,
}

impl MacroIndicatorId {
    fn application_id(self) -> &'static str {
        match self {
            Self::TenYearTipsRealYield => "ten_year_tips_real_yield",
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct MacroObservation {
    pub date: String,
    pub value: f64,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DataFreshness {
    Current,
    Stale,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FetchStatus {
    Fresh,
    Cached,
    Stale,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct MacroIndicatorData {
    pub id: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub unit: &'static str,
    pub frequency: &'static str,
    pub source: &'static str,
    pub source_series_id: &'static str,
    pub source_url: &'static str,
    pub freshness: DataFreshness,
    pub fetch_status: FetchStatus,
    pub observation_date: String,
    pub latest_value: f64,
    pub daily_change_basis_points: Option<f64>,
    pub retrieved_at_seconds: u64,
    pub status_note: Option<String>,
    pub refresh_error: Option<String>,
    pub observations: Vec<MacroObservation>,
}

#[derive(Clone, Debug)]
pub(crate) struct ProviderSeriesSnapshot {
    pub observations: Vec<MacroObservation>,
    pub retrieved_at_seconds: u64,
    pub fetch_status: FetchStatus,
    pub refresh_error: Option<String>,
}

/// Narrow app-owned port: providers return dated percentage observations while
/// the source format and snapshot parsing remain inside their adapter.
pub(crate) trait MacroSeriesProvider: Sync {
    fn read_series(&self, id: MacroIndicatorId) -> Result<ProviderSeriesSnapshot, String>;
}

fn provider_for(id: MacroIndicatorId) -> &'static dyn MacroSeriesProvider {
    match id {
        MacroIndicatorId::TenYearTipsRealYield => &fred::FRED_PROVIDER,
    }
}

/// Read the registered macro series and map it into the stable UI contract.
pub(crate) fn read(id: MacroIndicatorId) -> Result<MacroIndicatorData, String> {
    let snapshot = provider_for(id).read_series(id)?;
    let latest = snapshot
        .observations
        .last()
        .ok_or_else(|| "The macro data provider returned no observations.".to_string())?;
    let daily_change_basis_points = daily_change_basis_points(&snapshot.observations);
    let now = now_secs();
    let too_old = observation_is_stale(&latest.date, now)
        || snapshot_is_stale(snapshot.retrieved_at_seconds, now);
    let freshness = if too_old || snapshot.refresh_error.is_some() {
        DataFreshness::Stale
    } else {
        DataFreshness::Current
    };
    let fetch_status = if too_old {
        FetchStatus::Stale
    } else {
        snapshot.fetch_status
    };
    let status_note = if snapshot.refresh_error.is_some() {
        Some("The latest refresh failed; showing the last successfully retrieved observations.".to_string())
    } else if too_old {
        Some("The latest FRED snapshot or observation is more than seven days old. Check FRED again later.".to_string())
    } else {
        None
    };
    let (title, description, source_series_id, source_url) = match id {
        MacroIndicatorId::TenYearTipsRealYield => (
            "10-Year TIPS real yield",
            "The market yield on 10-year Treasury inflation-indexed securities. This is a real yield, not the nominal 10-year Treasury yield or a direct inflation forecast.",
            "DFII10",
            "https://fred.stlouisfed.org/series/DFII10",
        ),
    };
    Ok(MacroIndicatorData {
        id: id.application_id(),
        title,
        description,
        unit: "% per year",
        frequency: "Daily, business days",
        source: "Federal Reserve Bank of St. Louis · FRED",
        source_series_id,
        source_url,
        freshness,
        fetch_status,
        observation_date: latest.date.clone(),
        latest_value: latest.value,
        daily_change_basis_points,
        retrieved_at_seconds: snapshot.retrieved_at_seconds,
        status_note,
        refresh_error: snapshot.refresh_error,
        observations: snapshot.observations,
    })
}

fn daily_change_basis_points(observations: &[MacroObservation]) -> Option<f64> {
    let previous_index = observations.len().checked_sub(2)?;
    let latest = observations.last()?;
    let previous = &observations[previous_index];
    Some((latest.value - previous.value) * 100.0)
}

fn observation_is_stale(date: &str, now_seconds: u64) -> bool {
    let today_days = i64::try_from(now_seconds / 86_400).unwrap_or(i64::MAX);
    let cutoff_days = today_days.saturating_sub(MAX_OBSERVATION_AGE_DAYS);
    let (year, month, day) = quant_core::gregorian_date_from_unix_days(cutoff_days);
    let cutoff = format!("{year:04}-{month:02}-{day:02}");
    date < cutoff.as_str()
}

fn snapshot_is_stale(retrieved_at_seconds: u64, now_seconds: u64) -> bool {
    now_seconds.saturating_sub(retrieved_at_seconds) > MAX_SNAPSHOT_AGE_SECONDS
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

