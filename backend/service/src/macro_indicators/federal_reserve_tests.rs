use super::{parse_g17_manufacturing, parse_h10_csv, source_series_id};
use crate::macro_indicators::MacroIndicatorId;

const TEST_NOW: u64 = 1_800_000_000;

#[test]
fn h10_parser_reads_recent_broad_dollar_values_and_skips_missing_values() {
    let csv = concat!(
        "\"Series Description\",\"Nominal Broad Dollar Index\",\"Nominal Advanced Foreign Economies Dollar Index\",\"Nominal Emerging Market Economies Dollar Index\"\n",
        "\"Unit:\",\"Index:_1997_Jan_100\",\"Index:_1997_Jan_100\",\"Index:_1997_Jan_100\"\n",
        "\"Multiplier:\",\"1\",\"1\",\"1\"\n",
        "\"Currency:\",\"NA\",\"NA\",\"NA\"\n",
        "\"Unique Identifier: \",\"H10/H10/JRXWTFB_N.B\",\"H10/H10/JRXWTFN_N.B\",\"H10/H10/JRXWTFO_N.B\"\n",
        "\"Time Period\",\"JRXWTFB_N.B\",\"JRXWTFN_N.B\",\"JRXWTFO_N.B\"\n",
        "2026-11-12,122.5433,116.0203,130.8987\n",
        "2026-11-13,122.6503,116.2360,130.8877\n",
        "2026-11-16,ND,ND,ND\n"
    );
    let parsed = parse_h10_csv(csv, TEST_NOW).unwrap();
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].date, "2026-11-12");
    assert_eq!(parsed[0].value, 122.5433);
    assert_eq!(parsed[1].date, "2026-11-13");
    assert_eq!(parsed[1].value, 122.6503);
}

#[test]
fn h10_parser_rejects_changed_headers_invalid_values_dates_and_ordering() {
    let header = concat!(
        "\"Series Description\",\"Nominal Broad Dollar Index\"\n",
        "\"Unit:\",\"Index:_1997_Jan_100\"\n",
        "\"Multiplier:\",\"1\"\n",
        "\"Currency:\",\"NA\"\n",
        "\"Unique Identifier:\",\"H10/H10/JRXWTFB_N.B\"\n",
        "\"Time Period\",\"JRXWTFB_N.B\"\n"
    );
    assert!(parse_h10_csv(&format!("{header}2026-11-13,NaN\n"), TEST_NOW).is_err());
    assert!(parse_h10_csv(&format!("{header}2026-13-13,122.65\n"), TEST_NOW).is_err());
    assert!(parse_h10_csv(&format!("{header}2027-02-01,122.65\n"), TEST_NOW).is_err());
    assert!(parse_h10_csv(
        &format!("{header}2026-11-13,122.65\n2026-11-12,122.55\n"),
        TEST_NOW
    )
    .is_err());
    assert!(parse_h10_csv(
        &format!(
            "{}2026-11-13,122.65\n",
            header.replace("JRXWTFB_N.B", "OTHER")
        ),
        TEST_NOW
    )
    .is_err());
}

#[test]
fn g17_parser_reads_naics_manufacturing_series_as_monthly_observations() {
    let text = concat!(
        "\"B50001: Total index\"\n",
        "\"GMF: Manufacturing (NAICS)\"\n",
        "\"GMF\"          2025  100.1000  100.2000  100.3000  100.4000  100.5000  100.6000  100.7000  100.8000  100.9000  101.0000  101.1000  101.2000\n",
        "\"GMF\"          2026  101.3000  101.4000  101.5000  101.6000  101.7000  101.8000  101.9000  102.0000  102.1000  102.2000  .  .\n"
    );
    let parsed = parse_g17_manufacturing(text, TEST_NOW).unwrap();
    assert_eq!(parsed.len(), 22);
    assert_eq!(parsed[0].date, "2025-01-01");
    assert_eq!(parsed[0].value, 100.1);
    assert_eq!(parsed.last().unwrap().date, "2026-10-01");
    assert_eq!(parsed.last().unwrap().value, 102.2);
}

#[test]
fn g17_parser_rejects_unexpected_series_bad_rows_and_future_values() {
    assert!(parse_g17_manufacturing("not g17", TEST_NOW).is_err());
    assert!(parse_g17_manufacturing(
        "Series YEAR\n\"OTHER: Manufacturing (NAICS)\"\n\"OTHER\" 2026 101 102\n",
        TEST_NOW
    )
    .is_err());
    assert!(parse_g17_manufacturing(
        "Series YEAR\n\"GMF: Manufacturing (NAICS)\"\n\"GMF\" 2026 broken 102\n",
        TEST_NOW
    )
    .is_err());
    assert!(parse_g17_manufacturing(
        "Series YEAR\n\"GMF: Manufacturing (NAICS)\"\n\"GMF\" 9999 101\n",
        TEST_NOW
    )
    .is_err());
}

#[test]
fn federal_reserve_adapter_registers_only_supported_series() {
    assert_eq!(
        source_series_id(MacroIndicatorId::BroadUsDollarIndex),
        Some("JRXWTFB_N.B")
    );
    assert_eq!(
        source_series_id(MacroIndicatorId::UsManufacturingIndustrialProduction),
        Some("GMF")
    );
    assert_eq!(
        source_series_id(MacroIndicatorId::UsCommercialCrudeStocks),
        None
    );
}
