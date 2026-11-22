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
        requested_indicator("/api/macro/indicators?id=usd_idr_daily_reference&refresh=TRUE").unwrap(),
        (MacroIndicatorId::UsdIdrReferenceRate, true)
    );
    assert_eq!(
        requested_indicator("/api/macro/indicators?id=usd_idr_monthly_average").unwrap(),
        (MacroIndicatorId::UsdIdrReferenceRate, false)
    );
}

#[test]
fn macro_route_rejects_unregistered_indicators() {
    assert!(requested_indicator("/api/macro/indicators?id=unknown").is_err());
}
