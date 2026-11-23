use super::{parse_csv, source_series_id, HISTORY_DAYS};
use crate::macro_indicators::MacroIndicatorId;

const TEST_NOW: u64 = 1_800_000_000;

#[test]
fn fred_parser_skips_missing_values_and_preserves_signed_observations() {
    let parsed = parse_csv(
        "observation_date,T10YIE\r\n2026-11-16,2.42\r\n2026-11-17,.\r\n2026-11-18,-0.08\r\n",
        "T10YIE",
        TEST_NOW,
    )
    .unwrap();
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].date, "2026-11-16");
    assert_eq!(parsed[0].value, 2.42);
    assert_eq!(parsed[1].date, "2026-11-18");
    assert_eq!(parsed[1].value, -0.08);
    assert!(parsed.iter().all(|row| row.classification.is_none()));
}

#[test]
fn fred_parser_rejects_unexpected_headers_bad_values_and_invalid_dates() {
    assert!(parse_csv(
        "observation_date,wrong\n2026-11-18,2.8\n",
        "T10YIE",
        TEST_NOW
    )
    .is_err());
    assert!(parse_csv(
        "observation_date,T10YIE\n2026-13-18,2.8\n",
        "T10YIE",
        TEST_NOW
    )
    .is_err());
    assert!(parse_csv(
        "observation_date,T10YIE\n2026-11-18,NaN\n",
        "T10YIE",
        TEST_NOW
    )
    .is_err());
    assert!(parse_csv(
        "observation_date,T10YIE\n2026-11-18,2.8\n2026-11-18,2.9\n",
        "T10YIE",
        TEST_NOW
    )
    .is_err());
    assert!(parse_csv(
        "observation_date,T10YIE\n2026-11-18,2.8\n2026-11-17,2.9\n",
        "T10YIE",
        TEST_NOW
    )
    .is_err());
    assert!(parse_csv(
        "observation_date,T10YIE\n2027-02-01,2.8\n",
        "T10YIE",
        TEST_NOW
    )
    .is_err());
    assert!(parse_csv(
        "observation_date,T10YIE\n2026-11-18,.\n",
        "T10YIE",
        TEST_NOW
    )
    .is_err());
}

#[test]
fn fred_exposes_only_registered_source_series_and_bounded_history() {
    assert_eq!(
        source_series_id(MacroIndicatorId::BroadUsDollarIndex),
        Some("DTWEXBGS")
    );
    assert_eq!(
        source_series_id(MacroIndicatorId::UsManufacturingIndustrialProduction),
        Some("IPMAN")
    );
    assert_eq!(
        source_series_id(MacroIndicatorId::UsCommercialCrudeStocks),
        None
    );
    assert_eq!(HISTORY_DAYS, 3_653);
}
