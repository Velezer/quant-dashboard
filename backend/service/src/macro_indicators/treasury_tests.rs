use super::{derive_breakeven, parse_treasury_year_xml, series_config, source_series_id};
use crate::macro_indicators::{MacroIndicatorId, MacroObservation};

fn observation(date: &str, value: f64) -> MacroObservation {
    MacroObservation {
        date: date.to_string(),
        value,
        classification: None,
    }
}

#[test]
fn treasury_reads_the_daily_ten_year_nominal_series() {
    let parsed = parse_treasury_year_xml(
        "<feed><entry><d:NEW_DATE>2026-11-18T00:00:00</d:NEW_DATE><d:BC_10YEAR>4.12</d:BC_10YEAR></entry><entry><d:NEW_DATE>2026-11-19T00:00:00</d:NEW_DATE><d:BC_10YEAR>4.08</d:BC_10YEAR></entry></feed>",
        2026,
        MacroIndicatorId::TenYearTreasuryNominalYield,
    )
    .unwrap();
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].value, 4.12);
    assert_eq!(parsed[1].value, 4.08);
    assert_eq!(
        source_series_id(MacroIndicatorId::TenYearTreasuryNominalYield),
        Some("BC_10YEAR")
    );
    assert_eq!(
        series_config(MacroIndicatorId::TenYearTreasuryNominalYield)
            .unwrap()
            .dataset,
        "daily_treasury_yield_curve"
    );
}

#[test]
fn treasury_breakeven_uses_only_matching_dates_for_nominal_minus_tips() {
    let nominal = [
        observation("2026-11-18", 4.12),
        observation("2026-11-19", 4.08),
    ];
    let real = [
        observation("2026-11-19", 1.97),
        observation("2026-11-20", 2.01),
    ];
    let breakeven = derive_breakeven(&nominal, &real).unwrap();
    assert_eq!(breakeven.len(), 1);
    assert_eq!(breakeven[0].date, "2026-11-19");
    assert!((breakeven[0].value - 2.11).abs() < 1e-9);
    assert!(derive_breakeven(&nominal, &[]).is_err());
    assert_eq!(
        source_series_id(MacroIndicatorId::TenYearBreakevenInflation),
        Some("BC_10YEAR − TC_10YEAR, matched by date")
    );
}
