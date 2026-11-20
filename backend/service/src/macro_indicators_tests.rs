use super::{daily_change_basis_points, observation_is_stale, snapshot_is_stale, MacroIndicatorId, MacroObservation};

#[test]
fn treasury_parser_skips_missing_values_and_keeps_negative_real_yields() {
    let parsed = super::treasury::parse_treasury_year_xml(
        "<feed><entry><d:NEW_DATE>2026-11-16T00:00:00</d:NEW_DATE><d:TC_10YEAR>2.92</d:TC_10YEAR></entry><entry><d:NEW_DATE>2026-11-17T00:00:00</d:NEW_DATE><d:TC_10YEAR>.</d:TC_10YEAR></entry><entry><d:NEW_DATE>2026-11-18T00:00:00</d:NEW_DATE><d:TC_10YEAR>-0.08</d:TC_10YEAR></entry></feed>",
        2026,
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
    assert!(super::treasury::parse_treasury_year_xml("<html>unavailable</html>", 2026).is_err());
    assert!(super::treasury::parse_treasury_year_xml(
        "<feed><entry><d:NEW_DATE>2026-13-18T00:00:00</d:NEW_DATE><d:TC_10YEAR>2.8</d:TC_10YEAR></entry></feed>",
        2026,
    )
    .is_err());
    assert!(super::treasury::parse_treasury_year_xml(
        "<feed><entry><d:NEW_DATE>2026-11-18T00:00:00</d:NEW_DATE><d:TC_10YEAR>NaN</d:TC_10YEAR></entry></feed>",
        2026,
    )
    .is_err());
    assert!(super::treasury::parse_treasury_year_xml(
        "<feed><entry><d:NEW_DATE>2026-11-18T00:00:00</d:NEW_DATE><d:TC_10YEAR>2.8</d:TC_10YEAR></entry><entry><d:NEW_DATE>2026-11-18T00:00:00</d:NEW_DATE><d:TC_10YEAR>2.9</d:TC_10YEAR></entry></feed>",
        2026,
    )
    .is_err());
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
