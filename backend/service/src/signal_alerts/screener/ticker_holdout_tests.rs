use super::{load_ticker_holdout_win_rates, ticker_holdout_win_rate, TickerHoldoutLookupKey};
use serde_json::json;
use std::collections::BTreeSet;

fn stockbit_holdout_key() -> TickerHoldoutLookupKey {
    (
        "stockbit".to_string(),
        "stockbit_regular".to_string(),
        "idx_stock".to_string(),
        "AYAM".to_string(),
        "15m".to_string(),
        quant_core::ENTRY_SCORE_MODEL_ID.to_string(),
    )
}

#[test]
fn ticker_holdout_lookup_uses_compact_report_summary() {
    let connection = super::super::screener_tests::screener_connection();
    connection
        .execute(
            "INSERT INTO app.instruments (
                 instrument_id,exchange_id,product_id,asset_category_id,symbol,venue_symbol,
                 underlying_symbol,storage_symbol,quote_currency,listing_status,venue_verified,
                 execution_supported,registration_source,price_source_policy,registered_at,updated_at,is_active
             ) VALUES ('stockbit.ayam','stockbit','stockbit_regular','idx_stock','AYAM','AYAM.JK',
                 'AYAM','AYAM.JK','IDR','venue_active',1,0,'test','test',0,0,1)",
            [],
        )
        .unwrap();
    let summary = json!({
        "computed_at": 1234,
        "source": "stockbit_native",
        "models": [{
            "score_model_id": quant_core::ENTRY_SCORE_MODEL_ID,
            "holdout_trades": 20,
            "holdout_win_rate_pct": 60.0
        }]
    });
    connection
        .execute(
            "INSERT INTO app.entry_model_validation_reports (
                 instrument_id,timeframe,protocol_id,computed_at,report_json,summary_json,
                 backtest_complete,has_validated_model
             ) VALUES ('stockbit.ayam','15m',?1,1234,'large legacy report not parsed',?2,1,0)",
            rusqlite::params![
                quant_core::ENTRY_SCORE_VALIDATION_PROTOCOL_ID,
                summary.to_string()
            ],
        )
        .unwrap();

    let rates = load_ticker_holdout_win_rates(&connection, &BTreeSet::from([stockbit_holdout_key()]))
        .unwrap();
    let rate = &rates[&stockbit_holdout_key()];
    assert_eq!(rate["status"], "TICKER_HOLDOUT_RATE");
    assert_eq!(rate["win_probability_pct"], 60.0);
    assert_eq!(rate["sample_trades"], 20);
    assert_eq!(rate["wins"], 12);
    assert_eq!(rate["computed_at"], 1234);
    assert_eq!(rate["source_family"], "stockbit_native");
}

#[test]
fn rounded_legacy_holdout_rate_keeps_sample_without_inventing_interval() {
    let rate = ticker_holdout_win_rate(Some(
        r#"{"win_rate_after_fees_pct":53.3,"trades":30,"computed_at":1234}"#,
    ))
    .unwrap();
    assert_eq!(rate["win_probability_pct"], 53.3);
    assert_eq!(rate["sample_trades"], 30);
    assert!(rate["wins"].is_null());
    assert!(rate["confidence_interval_95_pct"].is_null());
}
