use super::{
    daily_change_basis_points, observation_is_stale, observation_is_stale_after_days,
    snapshot_is_stale, MacroIndicatorId, MacroObservation,
};

#[test]
fn treasury_parser_skips_missing_values_and_keeps_negative_real_yields() {
    let parsed = super::treasury::parse_treasury_year_xml(
        "<feed><entry><d:NEW_DATE>2026-11-16T00:00:00</d:NEW_DATE><d:TC_10YEAR>2.92</d:TC_10YEAR></entry><entry><d:NEW_DATE>2026-11-17T00:00:00</d:NEW_DATE><d:TC_10YEAR>.</d:TC_10YEAR></entry><entry><d:NEW_DATE>2026-11-18T00:00:00</d:NEW_DATE><d:TC_10YEAR>-0.08</d:TC_10YEAR></entry></feed>",
        2026,
        MacroIndicatorId::TenYearTipsRealYield,
    )
    .unwrap();
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].date, "2026-11-16");
    assert_eq!(parsed[0].value, 2.92);
    assert_eq!(parsed[1].date, "2026-11-18");
    assert_eq!(parsed[1].value, -0.08);
}

#[test]
fn treasury_parser_rejects_missing_feed_bad_dates_values_and_ordering() {
    assert!(super::treasury::parse_treasury_year_xml(
        "<html>unavailable</html>",
        2026,
        MacroIndicatorId::TenYearTipsRealYield,
    )
    .is_err());
    assert!(super::treasury::parse_treasury_year_xml(
        "<feed><entry><d:NEW_DATE>2026-13-18T00:00:00</d:NEW_DATE><d:TC_10YEAR>2.8</d:TC_10YEAR></entry></feed>",
        2026,
        MacroIndicatorId::TenYearTipsRealYield,
    )
    .is_err());
    assert!(super::treasury::parse_treasury_year_xml(
        "<feed><entry><d:NEW_DATE>2026-11-18T00:00:00</d:NEW_DATE><d:TC_10YEAR>NaN</d:TC_10YEAR></entry></feed>",
        2026,
        MacroIndicatorId::TenYearTipsRealYield,
    )
    .is_err());
    assert!(super::treasury::parse_treasury_year_xml(
        "<feed><entry><d:NEW_DATE>2026-11-18T00:00:00</d:NEW_DATE><d:TC_10YEAR>2.8</d:TC_10YEAR></entry><entry><d:NEW_DATE>2026-11-18T00:00:00</d:NEW_DATE><d:TC_10YEAR>2.9</d:TC_10YEAR></entry></feed>",
        2026,
        MacroIndicatorId::TenYearTipsRealYield,
    )
    .is_err());
}

#[test]
fn fred_csv_parser_keeps_valid_monthly_rates_and_rejects_bad_rows() {
    let parsed = super::fred::parse_fred_csv(
        "observation_date,CCUSMA02IDM618N\r\n2026-08-01,17844\r\n2026-09-01,17743.0454545455\r\n2026-10-01,17983.8181818182\r\n",
        MacroIndicatorId::UsdIdrMonthlyAverage,
    )
    .unwrap();
    assert_eq!(parsed.len(), 3);
    assert_eq!(parsed[0].date, "2026-08-31");
    assert_eq!(parsed[2].date, "2026-10-31");
    assert!((parsed[2].value - 17_983.8181818182).abs() < 1e-8);
    assert!(super::fred::parse_fred_csv(
        "observation_date,wrong_series\n2026-10-01,17983.8\n",
        MacroIndicatorId::UsdIdrMonthlyAverage,
    )
    .is_err());
    assert!(super::fred::parse_fred_csv(
        "observation_date,CCUSMA02IDM618N\n2026-10-01,0\n",
        MacroIndicatorId::UsdIdrMonthlyAverage,
    )
    .is_err());
}

#[test]
fn fred_parser_keeps_the_daily_two_year_treasury_series() {
    let parsed = super::fred::parse_fred_csv(
        "observation_date,DGS2\n2026-11-18,4.73\n2026-11-19,4.76\n",
        MacroIndicatorId::TwoYearTreasuryNominalYield,
    )
    .unwrap();
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[1].date, "2026-11-19");
    assert_eq!(parsed[1].value, 4.76);
}

#[test]
fn observation_freshness_allows_a_week_of_weekends_and_publish_lag() {
    let now = 1_704_844_800; // 2024-01-10 00:00:00 UTC
    assert!(!observation_is_stale("2024-01-03", now));
    assert!(observation_is_stale("2024-01-02", now));
    assert!(!snapshot_is_stale(now - 6 * 86_400, now));
    assert!(snapshot_is_stale(now - 8 * 86_400, now));
}

#[test]
fn monthly_exchange_rates_use_a_seventy_day_observation_window() {
    let now = 1_704_844_800; // 2024-01-10 00:00:00 UTC
    assert!(!observation_is_stale_after_days("2023-11-30", now, 70));
    assert!(observation_is_stale_after_days("2023-10-31", now, 70));
}

#[test]
fn monthly_fred_periods_normalize_to_the_calendar_month_end() {
    let leap_year = super::fred::parse_fred_csv(
        "observation_date,CCUSMA02IDM618N\n2024-02-01,16000",
        MacroIndicatorId::UsdIdrMonthlyAverage,
    )
    .unwrap();
    assert_eq!(leap_year[0].date, "2024-02-29");
    let non_leap_year = super::fred::parse_fred_csv(
        "observation_date,CCUSMA02IDM618N\n2026-02-01,16000",
        MacroIndicatorId::UsdIdrMonthlyAverage,
    )
    .unwrap();
    assert_eq!(non_leap_year[0].date, "2026-02-28");
    assert!(super::fred::parse_fred_csv(
        "observation_date,CCUSMA02IDM618N\n2026-02-02,16000",
        MacroIndicatorId::UsdIdrMonthlyAverage,
    )
    .is_err());
}

#[test]
fn daily_change_requires_a_prior_published_observation() {
    let one_row = [MacroObservation {
        date: "2026-11-18".to_string(),
        value: 2.88,
    }];
    assert_eq!(daily_change_basis_points(&[]), None);
    assert_eq!(daily_change_basis_points(&one_row), None);
    let two_rows = [
        MacroObservation {
            date: "2026-11-17".to_string(),
            value: 2.89,
        },
        one_row[0].clone(),
    ];
    assert!((daily_change_basis_points(&two_rows).unwrap() + 1.0).abs() < 1e-9);
}

#[test]
fn the_supported_indicator_has_a_stable_application_identity() {
    assert_eq!(
        MacroIndicatorId::TenYearTipsRealYield.application_id(),
        "ten_year_tips_real_yield"
    );
    assert_eq!(
        MacroIndicatorId::TwoYearTreasuryNominalYield.application_id(),
        "two_year_treasury_nominal_yield"
    );
    assert_eq!(
        MacroIndicatorId::UsdIdrMonthlyAverage.application_id(),
        "usd_idr_monthly_average"
    );
    assert_eq!(
        MacroIndicatorId::from_application_id("usd_idr_monthly_average"),
        Some(MacroIndicatorId::UsdIdrMonthlyAverage)
    );
    assert_eq!(MacroIndicatorId::from_application_id("unknown"), None);
}
