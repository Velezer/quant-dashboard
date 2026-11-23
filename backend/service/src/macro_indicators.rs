//! Application-owned macro indicator data contract and provider selection.
//! Macro observations are descriptive context and do not enter ticker scores.

mod bitcoin;
mod ecb;
mod eia;
mod fred;
mod nbs;
mod treasury;

use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(test)]
#[path = "macro_indicators_tests.rs"]
mod tests;

const MAX_OBSERVATION_AGE_DAYS: i64 = 7;
const MAX_SNAPSHOT_AGE_SECONDS: u64 = MAX_OBSERVATION_AGE_DAYS as u64 * 86_400;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum MacroIndicatorId {
    TenYearTipsRealYield,
    TwoYearTreasuryNominalYield,
    UsdIdrReferenceRate,
    BitcoinSpotEtfNetFlows,
    BitcoinFearGreedIndex,
    BroadUsDollarIndex,
    TenYearTreasuryNominalYield,
    TenYearBreakevenInflation,
    UsEquityVolatilityIndex,
    UsManufacturingIndustrialProduction,
    ChinaManufacturingPmi,
    BrentCrudeSpot,
    WtiCrudeSpot,
    CrudeOilVolatilityIndex,
    UsCommercialCrudeStocks,
}

impl MacroIndicatorId {
    fn application_id(self) -> &'static str {
        match self {
            Self::TenYearTipsRealYield => "ten_year_tips_real_yield",
            Self::TwoYearTreasuryNominalYield => "two_year_treasury_nominal_yield",
            Self::UsdIdrReferenceRate => "usd_idr_daily_reference",
            Self::BitcoinSpotEtfNetFlows => "bitcoin_spot_etf_net_flows",
            Self::BitcoinFearGreedIndex => "bitcoin_fear_greed_index",
            Self::BroadUsDollarIndex => "broad_us_dollar_index",
            Self::TenYearTreasuryNominalYield => "ten_year_treasury_nominal_yield",
            Self::TenYearBreakevenInflation => "ten_year_breakeven_inflation",
            Self::UsEquityVolatilityIndex => "us_equity_volatility_index",
            Self::UsManufacturingIndustrialProduction => "us_manufacturing_industrial_production",
            Self::ChinaManufacturingPmi => "china_manufacturing_pmi",
            Self::BrentCrudeSpot => "brent_crude_spot",
            Self::WtiCrudeSpot => "wti_crude_spot",
            Self::CrudeOilVolatilityIndex => "crude_oil_volatility_index",
            Self::UsCommercialCrudeStocks => "us_commercial_crude_stocks",
        }
    }

    pub(crate) fn from_application_id(value: &str) -> Option<Self> {
        match value {
            "ten_year_tips_real_yield" => Some(Self::TenYearTipsRealYield),
            "two_year_treasury_nominal_yield" => Some(Self::TwoYearTreasuryNominalYield),
            "usd_idr_daily_reference" | "usd_idr_monthly_average" => {
                Some(Self::UsdIdrReferenceRate)
            }
            "bitcoin_spot_etf_net_flows" => Some(Self::BitcoinSpotEtfNetFlows),
            "bitcoin_fear_greed_index" => Some(Self::BitcoinFearGreedIndex),
            "broad_us_dollar_index" => Some(Self::BroadUsDollarIndex),
            "ten_year_treasury_nominal_yield" => Some(Self::TenYearTreasuryNominalYield),
            "ten_year_breakeven_inflation" => Some(Self::TenYearBreakevenInflation),
            "us_equity_volatility_index" => Some(Self::UsEquityVolatilityIndex),
            "us_manufacturing_industrial_production" => {
                Some(Self::UsManufacturingIndustrialProduction)
            }
            "china_manufacturing_pmi" => Some(Self::ChinaManufacturingPmi),
            "brent_crude_spot" => Some(Self::BrentCrudeSpot),
            "wti_crude_spot" => Some(Self::WtiCrudeSpot),
            "crude_oil_volatility_index" => Some(Self::CrudeOilVolatilityIndex),
            "us_commercial_crude_stocks" => Some(Self::UsCommercialCrudeStocks),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct MacroObservation {
    pub date: String,
    pub value: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub classification: Option<String>,
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
    pub latest_classification: Option<String>,
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
        MacroIndicatorId::TenYearTipsRealYield | MacroIndicatorId::TwoYearTreasuryNominalYield => {
            &treasury::TREASURY_PROVIDER
        }
        MacroIndicatorId::UsdIdrReferenceRate => &ecb::ECB_PROVIDER,
        MacroIndicatorId::BitcoinSpotEtfNetFlows | MacroIndicatorId::BitcoinFearGreedIndex => {
            &bitcoin::BITCOIN_PROVIDER
        }
        MacroIndicatorId::UsCommercialCrudeStocks => &eia::EIA_PROVIDER,
        MacroIndicatorId::ChinaManufacturingPmi => &nbs::NBS_PROVIDER,
        MacroIndicatorId::BroadUsDollarIndex
        | MacroIndicatorId::TenYearTreasuryNominalYield
        | MacroIndicatorId::TenYearBreakevenInflation
        | MacroIndicatorId::UsEquityVolatilityIndex
        | MacroIndicatorId::UsManufacturingIndustrialProduction
        | MacroIndicatorId::BrentCrudeSpot
        | MacroIndicatorId::WtiCrudeSpot
        | MacroIndicatorId::CrudeOilVolatilityIndex => &fred::FRED_PROVIDER,
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
        MacroIndicatorId::UsdIdrReferenceRate
        | MacroIndicatorId::BitcoinSpotEtfNetFlows
        | MacroIndicatorId::BitcoinFearGreedIndex
        | MacroIndicatorId::BroadUsDollarIndex
        | MacroIndicatorId::UsEquityVolatilityIndex
        | MacroIndicatorId::UsManufacturingIndustrialProduction
        | MacroIndicatorId::ChinaManufacturingPmi
        | MacroIndicatorId::BrentCrudeSpot
        | MacroIndicatorId::WtiCrudeSpot
        | MacroIndicatorId::CrudeOilVolatilityIndex
        | MacroIndicatorId::UsCommercialCrudeStocks => None,
        MacroIndicatorId::TenYearTipsRealYield
        | MacroIndicatorId::TwoYearTreasuryNominalYield
        | MacroIndicatorId::TenYearTreasuryNominalYield
        | MacroIndicatorId::TenYearBreakevenInflation => {
            observation_change.map(|change| change * 100.0)
        }
    };
    let now = now_secs();
    let too_old =
        observation_is_stale_after_days(&latest.date, now, observation_age_limit_days(id))
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
        Some(
            "The latest refresh failed; showing the last successfully retrieved observations."
                .to_string(),
        )
    } else if too_old {
        Some(match id {
            MacroIndicatorId::TenYearTipsRealYield | MacroIndicatorId::TwoYearTreasuryNominalYield =>
                "The latest Treasury observation is more than seven days old. Check the Treasury series again later.".to_string(),
            MacroIndicatorId::UsdIdrReferenceRate =>
                "The latest daily USD/IDR reference rate is more than seven days old. Check the ECB reference series again later.".to_string(),
            MacroIndicatorId::BitcoinSpotEtfNetFlows =>
                "The latest U.S. spot Bitcoin ETF flow observation is more than seven days old. Check the flow tracker again later.".to_string(),
            MacroIndicatorId::BitcoinFearGreedIndex =>
                "The latest Bitcoin Fear & Greed observation is more than seven days old. Check Alternative.me again later.".to_string(),
            MacroIndicatorId::UsManufacturingIndustrialProduction =>
                "The latest monthly manufacturing production observation is more than 62 days old. Check the Federal Reserve release again later.".to_string(),
            MacroIndicatorId::ChinaManufacturingPmi =>
                "The latest monthly China manufacturing PMI observation is more than 62 days old. Check the National Bureau of Statistics release again later.".to_string(),
            MacroIndicatorId::UsCommercialCrudeStocks =>
                "The latest weekly U.S. crude stock observation is more than 21 days old. Check the EIA Weekly Petroleum Status Report again later.".to_string(),
            _ => "The latest daily market observation is more than seven days old. Check the linked source again later.".to_string(),
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
            "A daily constant-maturity market yield series, not the yield on one specific Treasury note. Treasury release timing may differ from intraday trading.",
            "% per year",
            "percent",
            "Daily, business days",
            "U.S. Department of the Treasury",
            "BC_2YEAR",
            "https://home.treasury.gov/resource-center/data-chart-center/interest-rates/TextView?type=daily_treasury_yield_curve",
            "from the prior business day",
        ),
        MacroIndicatorId::UsdIdrReferenceRate => (
            "USD/IDR exchange rate",
            "Daily U.S.-dollar/Indonesian-rupiah cross-rate calculated from the European Central Bank's two euro reference rates. A higher value means more rupiah per dollar and a weaker rupiah.",
            "Calculated as IDR per EUR divided by USD per EUR from the ECB's 2:15 p.m. CET reference rates. It is not an intraday executable quote or the Bank Indonesia JISDOR fixing.",
            "IDR per USD",
            "idr_per_usd",
            "Daily, business days",
            "European Central Bank",
            "D.IDR.EUR.SP00.A / D.USD.EUR.SP00.A",
            "https://www.ecb.europa.eu/stats/policy_and_exchange_rates/euro_reference_exchange_rates/html/index.en.html",
            "from the prior business day",
        ),
        MacroIndicatorId::BitcoinSpotEtfNetFlows => (
            "U.S. spot Bitcoin ETF net flows",
            "Daily net creations and redemptions across U.S. spot Bitcoin ETFs. Positive values indicate net inflows; negative values indicate net outflows.",
            "The tracker aggregates all U.S. spot Bitcoin ETFs and may revise recent sessions. Published flows generally lag the U.S. market close; totals may differ from other trackers because data sources and revision methods vary.",
            "USD per reported date",
            "usd_flow",
            "Daily, as reported",
            "The Commoner Bitcoin ETF Tracker by TFTC",
            "netFlowUsd · all U.S. spot Bitcoin ETFs",
            "https://www.tftc.io/bitcoin-etf-flows",
            "from the prior reported date",
        ),
        MacroIndicatorId::BitcoinFearGreedIndex => (
            "Bitcoin Fear & Greed Index",
            "A daily 0–100 reading of crypto-market sentiment, from Extreme Fear to Extreme Greed.",
            "This is Alternative.me's broad crypto-market sentiment index, not a Bitcoin price forecast or a validated trading signal. The source may revise or update the latest daily value.",
            "Index points (0–100)",
            "index_0_100",
            "Daily",
            "Alternative.me",
            "Crypto Fear & Greed Index",
            "https://alternative.me/crypto/fear-and-greed-index/",
            "from the prior daily reading",
        ),
        MacroIndicatorId::BroadUsDollarIndex => (
            "Broad U.S. dollar index",
            "A Federal Reserve trade-weighted measure of the dollar against a broad group of U.S. trading partners. Dollar strength can weigh on dollar-priced metals and crude, although relationships can diverge during risk events.",
            "A trade-weighted index, not the ICE DXY or a tradable currency quote. The index is published on business days and can revise.",
            "Index points (Jan 2006 = 100)",
            "index_points",
            "Daily, business days",
            "Board of Governors of the Federal Reserve System, retrieved from FRED",
            "FRED series",
            "FRED source series",
            "from the prior business day",
        ),
        MacroIndicatorId::TenYearTreasuryNominalYield => (
            "10-Year U.S. Treasury yield",
            "The nominal 10-year constant-maturity Treasury rate. Compare it with the 10-year TIPS real yield and breakeven inflation to separate real-rate and inflation-compensation moves.",
            "A daily market-yield series, not the yield on one specific Treasury note. Treasury release timing may differ from intraday trading.",
            "% per year",
            "percent",
            "Daily, business days",
            "Board of Governors of the Federal Reserve System, retrieved from FRED",
            "FRED series",
            "FRED source series",
            "from the prior business day",
        ),
        MacroIndicatorId::TenYearBreakevenInflation => (
            "10-Year breakeven inflation rate",
            "The spread between 10-year nominal Treasury yields and 10-year TIPS real yields. It is a market-based measure of inflation compensation, not a pure survey forecast.",
            "Breakevens include inflation-risk and relative-liquidity premiums; they can move for reasons other than expected consumer-price inflation.",
            "% per year",
            "percent",
            "Daily, business days",
            "Board of Governors of the Federal Reserve System, retrieved from FRED",
            "FRED series",
            "FRED source series",
            "from the prior business day",
        ),
        MacroIndicatorId::UsEquityVolatilityIndex => (
            "Cboe VIX volatility index",
            "The Cboe options-implied measure of expected near-term U.S. equity-market volatility. A sharp rise can mark a risk event that also changes haven demand for gold.",
            "VIX measures S&P 500 option-implied volatility, not gold volatility or a direct geopolitical-risk score.",
            "Index points",
            "index_points",
            "Daily, business days",
            "Cboe Global Markets, retrieved from FRED",
            "FRED series",
            "FRED source series",
            "from the prior business day",
        ),
        MacroIndicatorId::UsManufacturingIndustrialProduction => (
            "U.S. manufacturing industrial production",
            "A monthly Federal Reserve index of U.S. manufacturing output. It is a cyclical activity proxy for industrial metals, including silver, rather than a direct measure of global demand.",
            "U.S.-only monthly output index (2017 = 100); it does not measure global PMI, photovoltaic demand, inventories, or silver use directly. Recent values may be revised.",
            "Index points (2017 = 100)",
            "index_points",
            "Monthly",
            "Board of Governors of the Federal Reserve System, retrieved from FRED",
            "FRED series",
            "FRED source series",
            "from the prior monthly observation",
        ),
        MacroIndicatorId::ChinaManufacturingPmi => (
            "China official manufacturing PMI",
            "The National Bureau of Statistics' monthly manufacturing purchasing managers' diffusion index. Readings above 50 indicate expansion and readings below 50 indicate contraction; new orders, production and other survey components can help track demand expectations for metals and oil.",
            "This is China's official NBS survey, not the separate Caixin, U.S. ISM, or global S&P Global PMI. It is a monthly diffusion index and does not measure output volume or confirm the direction of a specific asset.",
            "Index points",
            "index_points",
            "Monthly",
            "National Bureau of Statistics of China",
            "NBS nationwide manufacturing PMI",
            "https://data.stats.gov.cn/dg/website/publicrelease/web/external/query?search=PMI&pagenum=1&pageSize=10",
            "from the prior monthly observation",
        ),
        MacroIndicatorId::BrentCrudeSpot => (
            "Brent crude spot price",
            "The EIA Europe Brent spot-price benchmark in U.S. dollars per barrel. It is broad oil-market context for UKOUSD; a broker's CFD quote can differ by venue, contract, spread, and trading hours.",
            "Daily benchmark spot data is not an intraday UKOUSD CFD quote. EIA reporting and market holidays create publication gaps.",
            "USD per barrel",
            "usd_per_barrel",
            "Daily, as reported",
            "U.S. Energy Information Administration, retrieved from FRED",
            "FRED series",
            "FRED source series",
            "from the prior published observation",
        ),
        MacroIndicatorId::WtiCrudeSpot => (
            "WTI crude spot price",
            "The EIA Cushing, Oklahoma WTI spot-price benchmark. Comparing it with Brent helps track regional oil-market dislocations and changes in the U.S. supply picture.",
            "Daily benchmark spot data is not a Brent futures price or an intraday UKOUSD CFD quote. The Brent–WTI spread is calculated from separate same-date observations only when both are available.",
            "USD per barrel",
            "usd_per_barrel",
            "Daily, as reported",
            "U.S. Energy Information Administration, retrieved from FRED",
            "FRED series",
            "FRED source series",
            "from the prior published observation",
        ),
        MacroIndicatorId::CrudeOilVolatilityIndex => (
            "Cboe crude oil volatility index",
            "The Cboe OVX index tracks option-implied volatility tied to U.S. Oil Fund options. It can highlight rising uncertainty around oil supply, demand, or geopolitical events.",
            "An options-implied volatility measure based on a U.S. oil fund, not a directional forecast, physical inventory, or direct Brent volatility measure.",
            "Index points",
            "index_points",
            "Daily, business days",
            "Cboe Global Markets, retrieved from FRED",
            "FRED series",
            "FRED source series",
            "from the prior business day",
        ),
        MacroIndicatorId::UsCommercialCrudeStocks => (
            "U.S. commercial crude oil stocks",
            "Weekly U.S. crude oil ending stocks excluding the Strategic Petroleum Reserve. Builds can indicate looser balances and draws can indicate tighter balances, but prices also respond to expectations and global supply.",
            "EIA weekly U.S. inventory is not global oil inventory and excludes the SPR. It is published with a lag and can be revised. The public demonstration API key is rate-limited; successful history is cached and kept visible if a refresh fails.",
            "Million barrels",
            "million_barrels",
            "Weekly",
            "U.S. Energy Information Administration",
            "EIA series",
            "EIA source series",
            "from the prior weekly observation",
        ),
    };
    let source_series_id = fred::source_series_id(id)
        .or_else(|| eia::source_series_id(id))
        .or_else(|| nbs::source_series_id(id))
        .unwrap_or(source_series_id);
    let source_url = fred::source_url(id)
        .or_else(|| eia::source_url(id))
        .or_else(|| nbs::source_url(id))
        .unwrap_or(source_url);
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
        latest_classification: latest.classification.clone(),
        observation_change,
        change_period,
        daily_change_basis_points,
        retrieved_at_seconds: snapshot.retrieved_at_seconds,
        status_note,
        refresh_error: snapshot.refresh_error,
        observations: snapshot.observations,
    })
}

fn observation_age_limit_days(id: MacroIndicatorId) -> i64 {
    match id {
        MacroIndicatorId::UsManufacturingIndustrialProduction => 62,
        MacroIndicatorId::ChinaManufacturingPmi => 62,
        MacroIndicatorId::UsCommercialCrudeStocks => 21,
        _ => MAX_OBSERVATION_AGE_DAYS,
    }
}

#[cfg(test)]
fn daily_change_basis_points(observations: &[MacroObservation]) -> Option<f64> {
    observation_change(observations).map(|change| change * 100.0)
}

fn observation_change(observations: &[MacroObservation]) -> Option<f64> {
    let previous_index = observations.len().checked_sub(2)?;
    let latest = observations.last()?;
    let previous = &observations[previous_index];
    Some(latest.value - previous.value)
}

#[cfg(test)]
fn observation_is_stale(date: &str, now_seconds: u64) -> bool {
    observation_is_stale_after_days(date, now_seconds, MAX_OBSERVATION_AGE_DAYS)
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
