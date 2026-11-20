use super::{daily_change_basis_points, observation_is_stale, MacroIndicatorId, MacroObservation};

#[test]
fn fred_parser_skips_missing_values_and_keeps_negative_real_yields() {
    let parsed = super::fred::parse_fred_csv(
        "observation_date,DFII10\n2026-11-16,2.92\n2026-11-17,.\n2026-11-18,-0.08\n",
    )
    .unwrap();
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].date, "2026-11-16");
    assert_eq!(parsed[0].value, 2.92);
    assert_eq!(parsed[1].date, "2026-11-18");
    assert_eq!(parsed[1].value, -0.08);
}

#[test]
fn fred_parser_rejects_bad_headers_dates_values_and_ordering() {
    assert!(super::fred::parse_fred_csv("date,DFII10\n2026-11-18,2.8\n").is_err());
    assert!(super::fred::parse_fred_csv("observation_date,DFII10\n2026-13-18,2.8\n").is_err());
    assert!(super::fred::parse_fred_csv("observation_date,DFII10\n2026-11-18,NaN\n").is_err());
    assert!(super::fred::parse_fred_csv(
        "observation_date,DFII10\n2026-11-18,2.8\n2026-11-18,2.9\n"
    )
    .is_err());
}

#[test]
fn fred_parser_bounds_observations_to_the_latest_chart_window() {
    let mut csv = String::from("observation_date,DFII10\n");
    for index in 0..3_010 {
        let date = format!("{:04}-01-01", 2000 + index);
        csv.push_str(&format!("{date},{}.0\n", index));
    }
    let parsed = super::fred::parse_fred_csv(&csv).unwrap();
    assert_eq!(parsed.len(), 3_000);
    assert_eq!(parsed.first().map(|row| row.date.as_str()), Some("2010-01-01"));
    assert_eq!(parsed.last().map(|row| row.date.as_str()), Some("5009-01-01"));
}

#[test]
fn observation_freshness_allows_a_week_of_weekends_and_publish_lag() {
    let now = 1_704_844_800; // 2024-01-10 00:00:00 UTC
    assert!(!observation_is_stale("2024-01-03", now));
    assert!(observation_is_stale("2024-01-02", now));
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
}
