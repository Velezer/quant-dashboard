use super::*;
use serde_json::json;

#[test]
fn screener_accepts_only_the_pluang_commodities_15m_combined_score_model() {
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch("ATTACH DATABASE ':memory:' AS app;")
        .unwrap();
    crate::master_data::initialize(&connection).unwrap();
    let now_ns = crate::now_secs() as i64 * crate::NANOS_PER_SECOND;
    let fresh_bar = now_ns - 16 * quant_core::SECONDS_PER_MINUTE * crate::NANOS_PER_SECOND;
    let mut candidate = json!({
        "Signal_Direction": "LONG",
        "Exchange_ID": "pluang",
        "Product_ID": "pluang_commodities",
        "Asset_Category_ID": "commodity_derivative",
        "Symbol": "XAUUSD",
        "Data_Symbol": "XAUUSD-USD",
        "Signal_Strategy_ID": "pluang_commodities_combined_entry_15m_v1",
        "Signal_Scoring_Model_ID": quant_core::ENTRY_SCORE_MODEL_ID,
        "Signal_Strength": 40.0,
        "Strategy_Strength": 20.0,
        "Signal_Confluence_Score": 20.0,
        "Signal_Score_Components": super::screener_tests::combined_components([10.0, 10.0, 10.0, 10.0, 0.0, 0.0]),
        "Current_Price": 4000.0,
        "Current_Price_Timeframe": "15m",
        "Current_Price_Bar_Time": fresh_bar,
        "Signal_Bar_Time": fresh_bar,
        "Signal_Score_Bar_Time": fresh_bar,
        "timeframe": "15m",
        "Current_Price_Data_Source": "pluang_commodities_yfinance_fallback",
        "Current_Price_Data_Provider": "yfinance",
        "Data_Quality": "RESEARCH_PROXY"
    });

    let row = signal_screener_row(&candidate).expect("Pluang 15m score row qualifies");
    assert_eq!(row["score_model_id"], quant_core::ENTRY_SCORE_MODEL_ID);
    assert_eq!(
        row["score_scale_max"],
        quant_core::SIX_INDICATOR_SCORE_SCALE_MAX
    );
    assert_eq!(row["timeframe"], "15m");
    assert_eq!(row["data_quality"], "RESEARCH_PROXY");
    assert_eq!(row["score_components"].as_array().unwrap().len(), 6);

    candidate["timeframe"] = json!("1h");
    assert!(signal_screener_row(&candidate).is_none());
    candidate["timeframe"] = json!("15m");
    candidate["Signal_Strategy_ID"] = json!("pluang_nwe_dss_15m");
    assert!(signal_screener_row(&candidate).is_none());
}

#[test]
fn pluang_commodities_model_is_published_for_the_product_filter() {
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch("ATTACH DATABASE ':memory:' AS app;")
        .unwrap();
    crate::master_data::initialize(&connection).unwrap();

    let screener_master_data = crate::master_data::signal_screener_master_data_from_catalog(
        crate::master_data::master_data_catalog(),
    );
    let model = screener_master_data["signal_score_models"]
        .as_array()
        .unwrap()
        .iter()
        .find(|model| model["id"] == "pluang_commodities_combined_entry_15m_v1")
        .expect("the commodity score model is exposed to the screener");
    assert_eq!(model["exchange_ids"][0], "pluang");
    assert_eq!(model["product_ids"][0], "pluang_commodities");
    assert_eq!(model["asset_category_ids"][0], "commodity_derivative");
    assert_eq!(model["timeframe"], "15m");
}
