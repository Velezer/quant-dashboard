use super::{parse_series, source_series_id};
use crate::macro_indicators::MacroIndicatorId;

const TEST_NOW: u64 = 1_800_000_000;

fn row(date: &str, value: &str) -> serde_json::Value {
    serde_json::json!({
        "period": date,
        "series": "WCESTUS1",
        "units": "MBBL",
        "value": value,
    })
}

#[test]
fn eia_parser_sorts_weekly_rows_and_converts_thousand_barrels_to_millions() {
    let payload = serde_json::json!({
        "response": {"data": [
            row("2026-11-13", "417797"),
            row("2026-11-06", "414417")
        ]}
    });
    let parsed = parse_series(&payload, TEST_NOW).unwrap();
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].date, "2026-11-06");
    assert!((parsed[0].value - 414.417).abs() < 1e-9);
    assert_eq!(parsed[1].date, "2026-11-13");
    assert!((parsed[1].value - 417.797).abs() < 1e-9);
}

#[test]
fn eia_parser_rejects_changed_series_units_bad_values_and_dates() {
    let valid = serde_json::json!({"response":{"data":[row("2026-11-13", "417797")]}});
    assert!(parse_series(&serde_json::json!({"error":"rate limited"}), TEST_NOW).is_err());
    assert!(parse_series(&serde_json::json!({"response":{"data":[]}}), TEST_NOW).is_err());
    assert!(parse_series(
        &serde_json::json!({"response":{"data":[{
            "period":"2026-11-13","series":"OTHER","units":"MBBL","value":"417797"
        }]}}),
        TEST_NOW
    )
    .is_err());
    assert!(parse_series(
        &serde_json::json!({"response":{"data":[{
            "period":"2026-11-13","series":"WCESTUS1","units":"BBL","value":"417797"
        }]}}),
        TEST_NOW
    )
    .is_err());
    let invalid_value = serde_json::json!({"response":{"data":[row("2026-11-13", "NaN")]}});
    assert!(parse_series(&invalid_value, TEST_NOW).is_err());
    let future_date = serde_json::json!({"response":{"data":[row("2027-02-01", "417797")]}});
    assert!(parse_series(&future_date, TEST_NOW).is_err());
    let duplicate_date = serde_json::json!({"response":{"data":[
        row("2026-11-13", "417797"),row("2026-11-13", "418000")
    ]}});
    assert!(parse_series(&duplicate_date, TEST_NOW).is_err());
    assert!(parse_series(&valid, TEST_NOW).is_ok());
}

#[test]
fn eia_provider_has_one_stable_commercial_inventory_series() {
    assert_eq!(
        source_series_id(MacroIndicatorId::UsCommercialCrudeStocks),
        Some("WCESTUS1")
    );
    assert_eq!(source_series_id(MacroIndicatorId::BrentCrudeSpot), None);
}
