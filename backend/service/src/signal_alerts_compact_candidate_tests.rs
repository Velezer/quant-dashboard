use super::{
    compact_stockbit_signal_score_update, initialize_schema, record_scan_signal_alerts,
    signal_screener_score_update,
};
use rusqlite::Connection;
use serde_json::json;

#[test]
fn compact_screener_rows_preserve_alert_price_direction_and_validation() {
    let connection = Connection::open_in_memory().expect("in-memory app database should open");
    connection
        .execute_batch("ATTACH DATABASE ':memory:' AS app;")
        .expect("app schema should attach");
    crate::master_data::initialize(&connection)
        .expect("canonical signal catalog should initialize");
    initialize_schema(&connection).expect("alert tables should initialize");

    let alerts = record_scan_signal_alerts(
        &connection,
        [json!({
            "exchange_id": "stockbit",
            "product_id": "stockbit_regular",
            "asset_category_id": "idx_stock",
            "symbol": "AADI.JK",
            "signal_strategy_id": quant_core::STOCKBIT_NWE_4H_SIGNAL_STRATEGY_ID,
            "timeframe": "4h",
            "direction": "LONG",
            "current_price": 6200.0,
            "signal_bar_time": 1_797_972_000_000_000_000i64,
            "data_source": "stockbit_regular_yfinance_fallback",
            "data_provider": "yfinance",
            "evidence_status": "VALIDATED"
        })]
        .into_iter(),
    )
    .expect("compact row should enter the existing alert lifecycle");

    assert_eq!(alerts.len(), 1);
    assert_eq!(alerts[0]["direction"], "LONG");
    assert_eq!(alerts[0]["price"], 6200.0);
    assert_eq!(alerts[0]["validation_status"], "VALIDATED");
    assert_eq!(
        alerts[0]["data_source"],
        "stockbit_regular_yfinance_fallback"
    );
    assert_eq!(alerts[0]["data_provider"], "yfinance");
}

#[test]
fn compact_stockbit_score_updates_preserve_the_validated_basis() {
    let connection = Connection::open_in_memory().expect("in-memory app database should open");
    connection
        .execute_batch("ATTACH DATABASE ':memory:' AS app;")
        .expect("app schema should attach");
    crate::master_data::initialize(&connection)
        .expect("canonical score model catalog should initialize");
    let now_ns = i64::try_from(crate::now_secs()).unwrap_or(i64::MAX) * 1_000_000_000;
    let timeframe_ns = 4 * 60 * 60 * 1_000_000_000;
    let forming_bar = now_ns.div_euclid(timeframe_ns) * timeframe_ns;
    let candidate = json!({
        "Score_Update_Only": true,
        "Exchange_ID": "stockbit",
        "Product_ID": "stockbit_regular",
        "Asset_Category_ID": "idx_stock",
        "Symbol": "AADI.JK",
        "Data_Symbol": "AADI.JK",
        "Signal_Strategy_ID": quant_core::STOCKBIT_NWE_4H_SIGNAL_STRATEGY_ID,
        "Signal_Scoring_Model_ID": quant_core::STOCKBIT_NWE_4H_SIGNAL_SCORE_MODEL_ID,
        "timeframe": "4h",
        "Signal_Direction": "LONG",
        "Signal_Bar_Time": forming_bar,
        "Signal_Score_Bar_Time": forming_bar,
        "Signal_Score_Bar_Is_Forming": true,
        "Signal_Strength": 8.0,
        "Strategy_Strength": 6.0,
        "Signal_Confluence_Score": 2.0,
        "Signal_Score_Components": [
            { "indicator": "Nadaraya-Watson Envelope", "state": "NWE_SCORE", "points": 4.0 },
            { "indicator": "DSS Bressert", "state": "DSS_SCORE", "points": 2.0 },
            { "indicator": "Relative Volume", "state": "ALIGNED", "points": 1.0 },
            { "indicator": "Estimated Volume Delta (OHLCV proxy)", "state": "ALIGNED_PROXY", "points": 1.0 }
        ],
        "Score_Update_Status": "qualifying",
        "Current_Price": 6200.0,
        "Current_Price_Timeframe": "4h",
        "Current_Price_Bar_Time": forming_bar,
        "Data_Source": "stockbit_regular_yfinance_fallback",
        "Data_Provider": "yfinance",
        "Data_Quality": "RESEARCH_PROXY"
    });

    let compact = compact_stockbit_signal_score_update(&candidate);
    let compatibility_row = signal_screener_score_update(&candidate)
        .expect("existing score update view should remain available");

    assert!(
        compact.is_some(),
        "fresh, valid Stockbit score should be compacted"
    );
    assert_eq!(compatibility_row["score"], 8.0);
    assert_eq!(compatibility_row["direction"], "LONG");
    assert_eq!(compatibility_row["current_price"], 6200.0);
    assert_eq!(
        compatibility_row["data_source"],
        "stockbit_regular_yfinance_fallback"
    );
    assert_eq!(
        compatibility_row["score_basis"]["components"][0]["points"],
        4.0
    );
    assert_eq!(
        compatibility_row["score_basis"]["components"][1]["points"],
        2.0
    );
    assert_eq!(
        compatibility_row["score_basis"]["components"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    assert_eq!(
        compatibility_row["score_basis"]["score_bar_is_forming"],
        true
    );
}
