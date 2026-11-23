use super::requested_indicator;
use crate::macro_indicators::MacroIndicatorId;

#[test]
fn macro_route_defaults_to_tips_and_accepts_each_registered_indicator() {
    assert_eq!(
        requested_indicator("/api/macro/indicators").unwrap(),
        (MacroIndicatorId::TenYearTipsRealYield, false)
    );
    assert_eq!(
        requested_indicator("/api/macro/indicators?id=two_year_treasury_nominal_yield").unwrap(),
        (MacroIndicatorId::TwoYearTreasuryNominalYield, false)
    );
    assert_eq!(
        requested_indicator("/api/macro/indicators?id=usd_idr_daily_reference&refresh=TRUE")
            .unwrap(),
        (MacroIndicatorId::UsdIdrReferenceRate, true)
    );
    assert_eq!(
        requested_indicator("/api/macro/indicators?id=usd_idr_monthly_average").unwrap(),
        (MacroIndicatorId::UsdIdrReferenceRate, false)
    );
    assert_eq!(
        requested_indicator("/api/macro/indicators?id=bitcoin_spot_etf_net_flows&refresh=1")
            .unwrap(),
        (MacroIndicatorId::BitcoinSpotEtfNetFlows, true)
    );
    assert_eq!(
        requested_indicator("/api/macro/indicators?id=bitcoin_fear_greed_index").unwrap(),
        (MacroIndicatorId::BitcoinFearGreedIndex, false)
    );
    for (application_id, id) in [
        (
            "broad_us_dollar_index",
            MacroIndicatorId::BroadUsDollarIndex,
        ),
        (
            "ten_year_treasury_nominal_yield",
            MacroIndicatorId::TenYearTreasuryNominalYield,
        ),
        (
            "ten_year_breakeven_inflation",
            MacroIndicatorId::TenYearBreakevenInflation,
        ),
        (
            "us_equity_volatility_index",
            MacroIndicatorId::UsEquityVolatilityIndex,
        ),
        (
            "us_manufacturing_industrial_production",
            MacroIndicatorId::UsManufacturingIndustrialProduction,
        ),
        (
            "china_manufacturing_pmi",
            MacroIndicatorId::ChinaManufacturingPmi,
        ),
        ("brent_crude_spot", MacroIndicatorId::BrentCrudeSpot),
        ("wti_crude_spot", MacroIndicatorId::WtiCrudeSpot),
        (
            "crude_oil_volatility_index",
            MacroIndicatorId::CrudeOilVolatilityIndex,
        ),
        (
            "us_commercial_crude_stocks",
            MacroIndicatorId::UsCommercialCrudeStocks,
        ),
    ] {
        let path = format!("/api/macro/indicators?id={application_id}");
        assert_eq!(requested_indicator(&path).unwrap(), (id, false));
    }
}

#[test]
fn macro_route_rejects_unregistered_indicators() {
    assert!(requested_indicator("/api/macro/indicators?id=unknown").is_err());
}
