use super::{parse_series, source_series_id};
use crate::macro_indicators::MacroIndicatorId;

const TEST_NOW: u64 = 1_800_000_000;

fn row(period: &str, value: &str) -> serde_json::Value {
    serde_json::json!({
        "code": format!("{period}MM"),
        "values": [{
            "_id": "a09aa989bdcf4cffa2021795722eb916",
            "catalogid": "93ffbb1aa85740d3aa2618371508b606",
            "da": "000000000000",
            "value": value
        }]
    })
}

#[test]
fn nbs_parser_sorts_monthly_pmi_rows_and_keeps_index_values() {
    let payload = serde_json::json!({
        "data": [row("202610", "49.3"), row("202609", "50.1")]
    });
    let parsed = parse_series(&payload, TEST_NOW).unwrap();
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].date, "2026-09-01");
    assert_eq!(parsed[0].value, 50.1);
    assert_eq!(parsed[1].date, "2026-10-01");
    assert_eq!(parsed[1].value, 49.3);

    let unpublished_month = serde_json::json!({
        "data": [row("202611", ""), row("202610", "49.3")]
    });
    let parsed = parse_series(&unpublished_month, TEST_NOW).unwrap();
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].date, "2026-10-01");
}

#[test]
fn nbs_parser_rejects_wrong_series_bad_periods_values_and_duplicates() {
    let wrong_series = serde_json::json!({
        "data": [{"code":"202610MM","values":[{
            "_id":"other","catalogid":"93ffbb1aa85740d3aa2618371508b606",
            "da":"000000000000","value":"49.3"
        }]}]
    });
    let wrong_geography = serde_json::json!({
        "data": [{"code":"202610MM","values":[{
            "_id":"a09aa989bdcf4cffa2021795722eb916",
            "catalogid":"93ffbb1aa85740d3aa2618371508b606",
            "da":"110000000000","value":"49.3"
        }]}]
    });
    assert!(parse_series(&serde_json::json!({"data": []}), TEST_NOW).is_err());
    assert!(parse_series(&wrong_series, TEST_NOW).is_err());
    assert!(parse_series(&wrong_geography, TEST_NOW).is_err());
    assert!(parse_series(
        &serde_json::json!({"data":[row("202613", "49.3")]}),
        TEST_NOW
    )
    .is_err());
    assert!(parse_series(
        &serde_json::json!({"data":[row("202610", "101")]}),
        TEST_NOW
    )
    .is_err());
    assert!(parse_series(
        &serde_json::json!({"data":[row("202610", "NaN")]}),
        TEST_NOW
    )
    .is_err());
    assert!(parse_series(
        &serde_json::json!({"data":[row("202710", "49.3")]}),
        TEST_NOW
    )
    .is_err());
    assert!(parse_series(
        &serde_json::json!({"data":[row("202610", "49.3"),row("202610", "49.4")]}),
        TEST_NOW
    )
    .is_err());
}

#[test]
fn nbs_provider_is_registered_for_china_manufacturing_pmi() {
    assert_eq!(
        source_series_id(MacroIndicatorId::ChinaManufacturingPmi),
        Some("NBS nationwide manufacturing PMI")
    );
    assert_eq!(source_series_id(MacroIndicatorId::BrentCrudeSpot), None);
}
