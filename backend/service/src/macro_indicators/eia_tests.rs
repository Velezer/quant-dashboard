use super::{parse_series, source_series_id};
use crate::macro_indicators::MacroIndicatorId;

const TEST_NOW: u64 = 1_800_000_000;

fn stock_row(date: &str, value: &str) -> serde_json::Value {
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
            stock_row("2026-11-13", "417797"),
            stock_row("2026-11-06", "414417")
        ]}
    });
    let parsed = parse_series(
        &payload,
        TEST_NOW,
        MacroIndicatorId::UsCommercialCrudeStocks,
    )
    .unwrap();
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].date, "2026-11-06");
    assert!((parsed[0].value - 414.417).abs() < 1e-9);
    assert_eq!(parsed[1].date, "2026-11-13");
    assert!((parsed[1].value - 417.797).abs() < 1e-9);
}

#[test]
fn eia_parser_reads_brent_and_wti_daily_prices_without_inventory_conversion() {
    let brent_payload = serde_json::json!({"response":{"data":[
        {"period":"2026-11-17","series":"RBRTE","units":"$/BBL","value":"92.94"},
        {"period":"2026-11-16","series":"RBRTE","units":"$/BBL","value":"92.90"}
    ]}});
    let brent = parse_series(&brent_payload, TEST_NOW, MacroIndicatorId::BrentCrudeSpot).unwrap();
    assert_eq!(brent[0].date, "2026-11-16");
    assert_eq!(brent[0].value, 92.90);

    let wti_payload = serde_json::json!({"response":{"data":[
        {"period":"2026-11-17","series":"RWTC","units":"$/BBL","value":"88.10"}
    ]}});
    let wti = parse_series(&wti_payload, TEST_NOW, MacroIndicatorId::WtiCrudeSpot).unwrap();
    assert_eq!(wti[0].date, "2026-11-17");
    assert_eq!(wti[0].value, 88.10);

    let negative_wti = serde_json::json!({"response":{"data":[
        {"period":"2020-04-20","series":"RWTC","units":"$/BBL","value":"-36.98"}
    ]}});
    let negative = parse_series(&negative_wti, TEST_NOW, MacroIndicatorId::WtiCrudeSpot).unwrap();
    assert_eq!(negative[0].date, "2020-04-20");
    assert_eq!(negative[0].value, -36.98);

    let wrong_units = serde_json::json!({"response":{"data":[
        {"period":"2026-11-17","series":"RBRTE","units":"MBBL","value":"92.94"}
    ]}});
    assert!(parse_series(&wrong_units, TEST_NOW, MacroIndicatorId::BrentCrudeSpot).is_err());
}

#[test]
fn eia_parser_rejects_changed_series_units_bad_values_and_dates() {
    let valid = serde_json::json!({"response":{"data":[stock_row("2026-11-13", "417797")]}});
    assert!(parse_series(
        &serde_json::json!({"error":"rate limited"}),
        TEST_NOW,
        MacroIndicatorId::UsCommercialCrudeStocks,
    )
    .is_err());
    assert!(parse_series(
        &serde_json::json!({"response":{"data":[]}}),
        TEST_NOW,
        MacroIndicatorId::UsCommercialCrudeStocks,
    )
    .is_err());
    assert!(parse_series(
        &serde_json::json!({"response":{"data":[{
            "period":"2026-11-13","series":"OTHER","units":"MBBL","value":"417797"
        }]}}),
        TEST_NOW,
        MacroIndicatorId::UsCommercialCrudeStocks,
    )
    .is_err());
    assert!(parse_series(
        &serde_json::json!({"response":{"data":[{
            "period":"2026-11-13","series":"WCESTUS1","units":"BBL","value":"417797"
        }]}}),
        TEST_NOW,
        MacroIndicatorId::UsCommercialCrudeStocks,
    )
    .is_err());
    assert!(parse_series(
        &serde_json::json!({"response":{"data":[stock_row("2026-11-13", "NaN")]}}),
        TEST_NOW,
        MacroIndicatorId::UsCommercialCrudeStocks,
    )
    .is_err());
    assert!(parse_series(
        &serde_json::json!({"response":{"data":[stock_row("2027-02-01", "417797")]}}),
        TEST_NOW,
        MacroIndicatorId::UsCommercialCrudeStocks,
    )
    .is_err());
    assert!(parse_series(
        &serde_json::json!({"response":{"data":[
            stock_row("2026-11-13", "417797"),stock_row("2026-11-13", "418000")
        ]}}),
        TEST_NOW,
        MacroIndicatorId::UsCommercialCrudeStocks,
    )
    .is_err());
    assert!(parse_series(&valid, TEST_NOW, MacroIndicatorId::UsCommercialCrudeStocks,).is_ok());
}

#[test]
fn eia_provider_registers_inventory_and_benchmark_spot_series() {
    assert_eq!(
        source_series_id(MacroIndicatorId::UsCommercialCrudeStocks),
        Some("WCESTUS1")
    );
    assert_eq!(
        source_series_id(MacroIndicatorId::BrentCrudeSpot),
        Some("RBRTE")
    );
    assert_eq!(
        source_series_id(MacroIndicatorId::WtiCrudeSpot),
        Some("RWTC")
    );
}
