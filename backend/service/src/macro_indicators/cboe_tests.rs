use super::{parse_csv, source_series_id};
use crate::macro_indicators::MacroIndicatorId;

const TEST_NOW: u64 = 1_800_000_000;

#[test]
fn cboe_parser_reads_vix_and_ovx_closes_with_daily_dates() {
    let vix = concat!(
        "DATE,OPEN,HIGH,LOW,CLOSE\n",
        "11/12/2026,18.10,19.20,17.90,18.50\n",
        "11/13/2026,18.50,19.40,18.20,19.10\n"
    );
    let parsed = parse_csv(vix, "CLOSE", "VIX", TEST_NOW).unwrap();
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].date, "2026-11-12");
    assert_eq!(parsed[0].value, 18.5);

    let ovx = "DATE,OVX\n11/13/2026,41.67\n";
    let parsed = parse_csv(ovx, "OVX", "OVX", TEST_NOW).unwrap();
    assert_eq!(parsed[0].date, "2026-11-13");
    assert_eq!(parsed[0].value, 41.67);
}

#[test]
fn cboe_parser_rejects_unexpected_headers_invalid_values_dates_and_ordering() {
    assert!(parse_csv("date,value\n11/13/2026,1\n", "CLOSE", "VIX", TEST_NOW).is_err());
    assert!(parse_csv("DATE,OVX\n13/13/2026,41\n", "OVX", "OVX", TEST_NOW).is_err());
    assert!(parse_csv("DATE,OVX\n11/13/2026,NaN\n", "OVX", "OVX", TEST_NOW).is_err());
    assert!(parse_csv(
        "DATE,OVX\n11/13/2026,41\n11/12/2026,42\n",
        "OVX",
        "OVX",
        TEST_NOW
    )
    .is_err());
    assert!(parse_csv("DATE,OVX\n02/01/2027,41\n", "OVX", "OVX", TEST_NOW).is_err());
}

#[test]
fn cboe_registers_only_the_requested_market_volatility_indices() {
    assert_eq!(
        source_series_id(MacroIndicatorId::UsEquityVolatilityIndex),
        Some("VIX")
    );
    assert_eq!(
        source_series_id(MacroIndicatorId::CrudeOilVolatilityIndex),
        Some("OVX")
    );
    assert_eq!(source_series_id(MacroIndicatorId::BrentCrudeSpot), None);
}
