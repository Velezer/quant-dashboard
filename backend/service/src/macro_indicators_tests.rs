use super::{
    daily_change_basis_points, observation_age_limit_days, observation_is_stale,
    observation_is_stale_after_days, snapshot_is_stale, MacroIndicatorId, MacroObservation,
};

const TEST_NOW: u64 = 1_800_000_000;

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
fn treasury_parser_reads_the_daily_two_year_nominal_series() {
    let parsed = super::treasury::parse_treasury_year_xml(
        "<feed><entry><d:NEW_DATE>2026-11-18T00:00:00</d:NEW_DATE><d:BC_2YEAR>3.91</d:BC_2YEAR></entry><entry><d:NEW_DATE>2026-11-19T00:00:00</d:NEW_DATE><d:BC_2YEAR>3.88</d:BC_2YEAR></entry></feed>",
        2026,
        MacroIndicatorId::TwoYearTreasuryNominalYield,
    )
    .unwrap();
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[1].date, "2026-11-19");
    assert_eq!(parsed[1].value, 3.88);
}

#[test]
fn ecb_parser_builds_daily_usd_idr_cross_rates_and_rejects_bad_data() {
    const HEADER: &str =
        "KEY,FREQ,CURRENCY,CURRENCY_DENOM,EXR_TYPE,EXR_SUFFIX,TIME_PERIOD,OBS_VALUE";
    let idr = format!(
        "{HEADER}\r\nEXR.D.IDR.EUR.SP00.A,D,IDR,EUR,SP00,A,2026-11-18,20079.46\r\nEXR.D.IDR.EUR.SP00.A,D,IDR,EUR,SP00,A,2026-11-19,20070.25\r\n"
    );
    let usd = format!(
        "{HEADER}\r\nEXR.D.USD.EUR.SP00.A,D,USD,EUR,SP00,A,2026-11-18,1.0942\r\nEXR.D.USD.EUR.SP00.A,D,USD,EUR,SP00,A,2026-11-19,1.099\r\n"
    );
    let parsed = super::ecb::parse_cross_rate(&idr, &usd).unwrap();
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].date, "2026-11-18");
    assert!((parsed[0].value - (20_079.46 / 1.0942)).abs() < 1e-8);
    assert_eq!(parsed[1].date, "2026-11-19");
    assert!((parsed[1].value - (20_070.25 / 1.099)).abs() < 1e-8);
    assert!(super::ecb::parse_cross_rate(
        &idr.replace("TIME_PERIOD,OBS_VALUE", "WRONG,HEADER"),
        &usd,
    )
    .is_err());
    assert!(super::ecb::parse_cross_rate(
        &idr.replace("2026-11-18,20079.46", "2026-13-18,20079.46"),
        &usd,
    )
    .is_err());
    assert!(super::ecb::parse_cross_rate(&idr.replace("20079.46", "NaN"), &usd,).is_err());
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
fn bitcoin_etf_flow_parser_sorts_totals_and_checks_attribution_license_and_dates() {
    let payload = serde_json::json!({
        "license": "https://creativecommons.org/licenses/by/4.0/",
        "attribution": "The Commoner Bitcoin ETF Tracker by TFTC",
        "days": [
            {"date": "2026-11-19", "netFlowUsd": -125_500_000.0},
            {"date": "2026-11-18", "netFlowUsd": 75_000_000.0}
        ]
    });
    let parsed = super::bitcoin::parse_etf_flows(&payload, TEST_NOW).unwrap();
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].date, "2026-11-18");
    assert_eq!(parsed[0].value, 75_000_000.0);
    assert_eq!(parsed[1].date, "2026-11-19");
    assert_eq!(parsed[1].value, -125_500_000.0);
    assert!(parsed.iter().all(|row| row.classification.is_none()));

    let changed_license = serde_json::json!({
        "license": "all-rights-reserved",
        "attribution": "The Commoner Bitcoin ETF Tracker by TFTC",
        "days": [{"date": "2026-11-19", "netFlowUsd": 0}]
    });
    assert!(super::bitcoin::parse_etf_flows(&changed_license, TEST_NOW).is_err());
    let invalid_value = serde_json::json!({
        "license": "https://creativecommons.org/licenses/by/4.0/",
        "attribution": "The Commoner Bitcoin ETF Tracker by TFTC",
        "days": [{"date": "2026-11-19", "netFlowUsd": "pending"}]
    });
    assert!(super::bitcoin::parse_etf_flows(&invalid_value, TEST_NOW).is_err());
    let duplicate_date = serde_json::json!({
        "license": "https://creativecommons.org/licenses/by/4.0/",
        "attribution": "The Commoner Bitcoin ETF Tracker by TFTC",
        "days": [
            {"date": "2026-11-19", "netFlowUsd": 1},
            {"date": "2026-11-19", "netFlowUsd": 2}
        ]
    });
    assert!(super::bitcoin::parse_etf_flows(&duplicate_date, TEST_NOW).is_err());
    let future_date = serde_json::json!({
        "license": "https://creativecommons.org/licenses/by/4.0/",
        "attribution": "The Commoner Bitcoin ETF Tracker by TFTC",
        "days": [{"date": "2027-02-01", "netFlowUsd": 1}]
    });
    assert!(super::bitcoin::parse_etf_flows(&future_date, TEST_NOW).is_err());
}

#[test]
fn fear_greed_parser_sorts_daily_index_rows_and_keeps_source_classification() {
    let payload = serde_json::json!({
        "name": "Fear and Greed Index",
        "data": [
            {"timestamp": "1795305600", "value": "11", "value_classification": "Extreme Fear"},
            {"timestamp": "1795219200", "value": "28", "value_classification": "Fear"}
        ]
    });
    let parsed = super::bitcoin::parse_fear_greed(&payload, TEST_NOW).unwrap();
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].date, "2026-11-21");
    assert_eq!(parsed[0].value, 28.0);
    assert_eq!(parsed[0].classification.as_deref(), Some("Fear"));
    assert_eq!(parsed[1].date, "2026-11-22");
    assert_eq!(parsed[1].value, 11.0);
    assert_eq!(parsed[1].classification.as_deref(), Some("Extreme Fear"));

    let invalid_value = serde_json::json!({
        "name": "Fear and Greed Index",
        "data": [{"timestamp": "1795305600", "value": "101", "value_classification": "Greed"}]
    });
    assert!(super::bitcoin::parse_fear_greed(&invalid_value, TEST_NOW).is_err());
    let invalid_classification = serde_json::json!({
        "name": "Fear and Greed Index",
        "data": [{"timestamp": "1795305600", "value": "75", "value_classification": "Bullish"}]
    });
    assert!(super::bitcoin::parse_fear_greed(&invalid_classification, TEST_NOW).is_err());
}

#[test]
fn daily_exchange_rates_use_the_same_freshness_window_as_daily_yields() {
    let now = 1_704_844_800; // 2024-01-10 00:00:00 UTC
    assert!(!observation_is_stale_after_days("2024-01-03", now, 7));
    assert!(observation_is_stale_after_days("2024-01-02", now, 7));
}

#[test]
fn weekly_and_monthly_observations_get_cadence_appropriate_freshness_windows() {
    let now = 1_704_844_800; // 2024-01-10 00:00:00 UTC
    assert_eq!(
        observation_age_limit_days(MacroIndicatorId::UsCommercialCrudeStocks),
        21
    );
    assert_eq!(
        observation_age_limit_days(MacroIndicatorId::UsManufacturingIndustrialProduction),
        62
    );
    assert_eq!(
        observation_age_limit_days(MacroIndicatorId::ChinaManufacturingPmi),
        62
    );
    assert_eq!(
        observation_age_limit_days(MacroIndicatorId::BroadUsDollarIndex),
        10
    );
    assert!(!observation_is_stale_after_days(
        "2023-12-01",
        now,
        observation_age_limit_days(MacroIndicatorId::UsManufacturingIndustrialProduction),
    ));
    assert!(observation_is_stale_after_days(
        "2023-12-01",
        now,
        observation_age_limit_days(MacroIndicatorId::UsCommercialCrudeStocks),
    ));
}

#[test]
fn daily_change_requires_a_prior_published_observation() {
    let one_row = [MacroObservation {
        date: "2026-11-18".to_string(),
        value: 2.88,
        classification: None,
    }];
    assert_eq!(daily_change_basis_points(&[]), None);
    assert_eq!(daily_change_basis_points(&one_row), None);
    let two_rows = [
        MacroObservation {
            date: "2026-11-17".to_string(),
            value: 2.89,
            classification: None,
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
        MacroIndicatorId::UsdIdrReferenceRate.application_id(),
        "usd_idr_daily_reference"
    );
    assert_eq!(
        MacroIndicatorId::BitcoinSpotEtfNetFlows.application_id(),
        "bitcoin_spot_etf_net_flows"
    );
    assert_eq!(
        MacroIndicatorId::BitcoinFearGreedIndex.application_id(),
        "bitcoin_fear_greed_index"
    );
    for (id, application_id) in [
        (
            MacroIndicatorId::BroadUsDollarIndex,
            "broad_us_dollar_index",
        ),
        (
            MacroIndicatorId::TenYearTreasuryNominalYield,
            "ten_year_treasury_nominal_yield",
        ),
        (
            MacroIndicatorId::TenYearBreakevenInflation,
            "ten_year_breakeven_inflation",
        ),
        (
            MacroIndicatorId::UsEquityVolatilityIndex,
            "us_equity_volatility_index",
        ),
        (
            MacroIndicatorId::UsManufacturingIndustrialProduction,
            "us_manufacturing_industrial_production",
        ),
        (
            MacroIndicatorId::ChinaManufacturingPmi,
            "china_manufacturing_pmi",
        ),
        (MacroIndicatorId::BrentCrudeSpot, "brent_crude_spot"),
        (MacroIndicatorId::WtiCrudeSpot, "wti_crude_spot"),
        (
            MacroIndicatorId::CrudeOilVolatilityIndex,
            "crude_oil_volatility_index",
        ),
        (
            MacroIndicatorId::UsCommercialCrudeStocks,
            "us_commercial_crude_stocks",
        ),
    ] {
        assert_eq!(id.application_id(), application_id);
        assert_eq!(
            MacroIndicatorId::from_application_id(application_id),
            Some(id)
        );
    }
    assert_eq!(
        MacroIndicatorId::from_application_id("usd_idr_daily_reference"),
        Some(MacroIndicatorId::UsdIdrReferenceRate)
    );
    assert_eq!(
        MacroIndicatorId::from_application_id("usd_idr_monthly_average"),
        Some(MacroIndicatorId::UsdIdrReferenceRate)
    );
    assert_eq!(
        MacroIndicatorId::from_application_id("bitcoin_spot_etf_net_flows"),
        Some(MacroIndicatorId::BitcoinSpotEtfNetFlows)
    );
    assert_eq!(
        MacroIndicatorId::from_application_id("bitcoin_fear_greed_index"),
        Some(MacroIndicatorId::BitcoinFearGreedIndex)
    );
    assert_eq!(MacroIndicatorId::from_application_id("unknown"), None);
}
