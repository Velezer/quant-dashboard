//! Application-owned macro indicator data contract and provider selection.
//! Macro observations are descriptive context and do not enter ticker scores.

mod treasury;
mod fred;

use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(test)]
#[path = "macro_indicators_tests.rs"]
mod tests;

const MAX_OBSERVATION_AGE_DAYS: i64 = 7;
const MAX_MONTHLY_OBSERVATION_AGE_DAYS: i64 = 70;
const MAX_SNAPSHOT_AGE_SECONDS: u64 = MAX_OBSERVATION_AGE_DAYS as u64 * 86_400;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum MacroIndicatorId {
    TenYearTipsRealYield,
    TwoYearTreasuryNominalYield,
    UsdIdrMonthlyAverage,
}

impl MacroIndicatorId {
    fn application_id(self) -> &'static str {
        match self {
            Self::TenYearTipsRealYield => "ten_year_tips_real_yield",
            Self::TwoYearTreasuryNominalYield => "two_year_treasury_nominal_yield",
            Self::UsdIdrMonthlyAverage => "usd_idr_monthly_average",
        }
    }

    pub(crate) fn from_application_id(value: &str) -> Option<Self> {
        match value {
            "ten_year_tips_real_yield" => Some(Self::TenYearTipsRealYield),
            "two_year_treasury_nominal_yield" => Some(Self::TwoYearTreasuryNominalYield),
            "usd_idr_monthly_average" => Some(Self::UsdIdrMonthlyAverage),
            _ => None,
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
    pub limitations: &'static str,
    pub unit: &'static str,
    pub value_format: &'static str,
    pub frequency: &'static str,
    pub source: &'static str,
    pub source_series_id: &'static str,
    pub source_url: &'static str,
    pub freshness: DataFreshness,
    pub fetch_status: FetchStatus,
    pub observation_date: String,
    pub latest_value: f64,
    pub observation_change: Option<f64>,
    pub change_period: &'static str,
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

/// Narrow app-owned port: providers return dated numeric observations while
/// the source format and provider parsing remain inside their adapter.
pub(crate) trait MacroSeriesProvider: Sync {
    fn read_series(
        &self,
        id: MacroIndicatorId,
        force_refresh: bool,
    ) -> Result<ProviderSeriesSnapshot, String>;
}

fn provider_for(id: MacroIndicatorId) -> &'static dyn MacroSeriesProvider {
    match id {
        MacroIndicatorId::TenYearTipsRealYield => &treasury::TREASURY_PROVIDER,
        MacroIndicatorId::TwoYearTreasuryNominalYield | MacroIndicatorId::UsdIdrMonthlyAverage => {
            &fred::FRED_PROVIDER
        }
    }
}

/// Read the registered macro series and map it into the stable UI contract.
pub(crate) fn read(
    id: MacroIndicatorId,
    force_refresh: bool,
) -> Result<MacroIndicatorData, String> {
    let snapshot = provider_for(id).read_series(id, force_refresh)?;
    let latest = snapshot
        .observations
        .last()
        .ok_or_else(|| "The macro data provider returned no observations.".to_string())?;
    let observation_change = observation_change(&snapshot.observations);
    let daily_change_basis_points = match id {
        MacroIndicatorId::UsdIdrMonthlyAverage => None,
        MacroIndicatorId::TenYearTipsRealYield | MacroIndicatorId::TwoYearTreasuryNominalYield => {
            observation_change.map(|change| change * 100.0)
        }
    };
    let now = now_secs();
    let observation_age_days = max_observation_age_days(id);
    let too_old = observation_is_stale_after_days(&latest.date, now, observation_age_days)
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
        Some(match id {
            MacroIndicatorId::TenYearTipsRealYield | MacroIndicatorId::TwoYearTreasuryNominalYield =>
                "The latest Treasury observation is more than seven days old. Check the Treasury/FRED series again later.".to_string(),
            MacroIndicatorId::UsdIdrMonthlyAverage =>
                format!("The latest monthly USD/IDR observation is more than {MAX_MONTHLY_OBSERVATION_AGE_DAYS} days old. Check the OECD series again later."),
        })
    } else {
        None
    };
    let (title, description, limitations, unit, value_format, frequency, source, source_series_id, source_url, change_period) = match id {
        MacroIndicatorId::TenYearTipsRealYield => (
            "10-Year TIPS real yield",
            "The market yield on 10-year Treasury inflation-indexed securities. This is a real yield, not the nominal 10-year Treasury yield or a direct inflation forecast.",
            "Daily market pricing for an inflation-linked Treasury security; it is not a direct inflation forecast.",
            "% per year",
            "percent",
            "Daily, business days",
            "U.S. Department of the Treasury",
            "TC_10YEAR",
            "https://home.treasury.gov/resource-center/data-chart-center/interest-rates/TextView?type=daily_treasury_real_yield_curve",
            "from the prior business day",
        ),
        MacroIndicatorId::TwoYearTreasuryNominalYield => (
            "2-Year U.S. Treasury yield",
            "The nominal 2-year constant-maturity U.S. Treasury rate. It reflects market pricing and is not a guaranteed forecast of future policy rates or returns.",
            "A daily constant-maturity market yield series, not the yield on one specific Treasury note. Treasury and FRED release timing may differ from intraday trading.",
            "% per year",
            "percent",
            "Daily, business days",
            "Board of Governors of the Federal Reserve System via FRED (Federal Reserve Bank of St. Louis)",
            "DGS2",
            "https://fred.stlouisfed.org/series/DGS2",
            "from the prior business day",
        ),
        MacroIndicatorId::UsdIdrMonthlyAverage => (
            "USD/IDR exchange rate",
            "Monthly average of daily rupiah-per-U.S.-dollar rates. A higher value means more rupiah per dollar and a weaker rupiah; this is not the daily JISDOR fixing.",
            "Monthly averages can lag daily market moves and are not the daily Bank Indonesia JISDOR fixing.",
            "IDR per USD",
            "idr_per_usd",
            "Monthly average of daily rates",
            "OECD via FRED (Federal Reserve Bank of St. Louis)",
            "CCUSMA02IDM618N",
            "https://fred.stlouisfed.org/series/CCUSMA02IDM618N",
            "from the prior monthly average",
        ),
    };
    Ok(MacroIndicatorData {
        id: id.application_id(),
        title,
        description,
        limitations,
        unit,
        value_format,
        frequency,
        source,
        source_series_id,
        source_url,
        freshness,
        fetch_status,
        observation_date: latest.date.clone(),
        latest_value: latest.value,
        observation_change,
        change_period,
        daily_change_basis_points,
        retrieved_at_seconds: snapshot.retrieved_at_seconds,
        status_note,
        refresh_error: snapshot.refresh_error,
        observations: snapshot.observations,
    })
}

fn daily_change_basis_points(observations: &[MacroObservation]) -> Option<f64> {
    observation_change(observations).map(|change| change * 100.0)
}

fn observation_change(observations: &[MacroObservation]) -> Option<f64> {
    let previous_index = observations.len().checked_sub(2)?;
    let latest = observations.last()?;
    let previous = &observations[previous_index];
    Some(latest.value - previous.value)
}

fn observation_is_stale(date: &str, now_seconds: u64) -> bool {
    observation_is_stale_after_days(date, now_seconds, MAX_OBSERVATION_AGE_DAYS)
}

fn max_observation_age_days(id: MacroIndicatorId) -> i64 {
    match id {
        MacroIndicatorId::UsdIdrMonthlyAverage => MAX_MONTHLY_OBSERVATION_AGE_DAYS,
        MacroIndicatorId::TenYearTipsRealYield | MacroIndicatorId::TwoYearTreasuryNominalYield =>
            MAX_OBSERVATION_AGE_DAYS,
    }
}

fn observation_is_stale_after_days(date: &str, now_seconds: u64, max_age_days: i64) -> bool {
    let today_days = i64::try_from(now_seconds / 86_400).unwrap_or(i64::MAX);
    let cutoff_days = today_days.saturating_sub(max_age_days);
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

