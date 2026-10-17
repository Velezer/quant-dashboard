use super::{list_signal_screener, signal_screener_row};
use rusqlite::Connection;
use serde_json::json;

const HOUR_NANOSECONDS: i64 = 60 * 60 * 1_000_000_000;

fn stockbit_4h_candidate(signal_bar_time: i64, price_bar_time: i64) -> serde_json::Value {
    json!({
        "Signal_Direction": "LONG",
        "Exchange_ID": "stockbit",
        "Product_ID": "stockbit_regular",
        "Asset_Category_ID": "idx_stock",
        "Symbol": "TEST.JK",
        "Data_Symbol": "TEST.JK",
        "Signal_Strategy_ID": quant_core::STOCKBIT_NWE_4H_SIGNAL_STRATEGY_ID,
        "timeframe": "4h",
        "Current_Price": 100.0,
        "Current_Price_Timeframe": "4h",
        "Current_Price_Bar_Time": price_bar_time,
        "Signal_Bar_Time": signal_bar_time,
        "Signal_Strength": 5.0,
        "Signal_Scoring_Model_ID": quant_core::STOCKBIT_NWE_4H_SIGNAL_SCORE_MODEL_ID,
        "Signal_Score_Components": []
    })
}

#[test]
fn screener_candidate_expires_two_hours_after_its_signal_candle_closes() {
    let connection = Connection::open_in_memory().expect("in-memory app database should open");
    connection
        .execute_batch("ATTACH DATABASE ':memory:' AS app;")
        .expect("app schema should attach");
    crate::master_data::initialize(&connection).expect("signal catalog should initialize");

    let now_ns = i64::try_from(crate::now_secs())
        .unwrap_or(i64::MAX)
        .saturating_mul(1_000_000_000);
    let fresh_price_bar = now_ns.saturating_sub(5 * HOUR_NANOSECONDS);
    let expired_signal_bar = now_ns.saturating_sub(6 * HOUR_NANOSECONDS + 1_000_000_000);

    assert!(
        signal_screener_row(&stockbit_4h_candidate(expired_signal_bar, fresh_price_bar)).is_none()
    );
}

#[test]
fn list_signal_screener_keeps_expired_stockbit_score_rows_stale() {
    let connection = Connection::open_in_memory().expect("in-memory app database should open");
    connection
        .execute_batch("ATTACH DATABASE ':memory:' AS app; ATTACH DATABASE ':memory:' AS signals;")
        .expect("app schema should attach");
    super::super::initialize_schema(&connection).expect("screener schema should initialize");
    super::super::initialize_screener_schema(&connection)
        .expect("signal snapshot schema should initialize");

    let now = crate::now_secs() as i64;
    let now_ns = now.saturating_mul(1_000_000_000);
    let expired_signal_bar = now_ns
        .saturating_sub(6 * HOUR_NANOSECONDS)
        .saturating_sub(1_000_000_000);
    let fresh_price_bar = now_ns.saturating_sub(5 * HOUR_NANOSECONDS);
    connection
        .execute(
            "INSERT INTO signals.signal_screener_rows(
                exchange_id,product_id,asset_category_id,symbol,storage_symbol,
                signal_strategy_id,signal_strategy_name,timeframe,direction,score,
                strategy_strength,confluence_score,score_basis_json,current_price,
                price_timeframe,price_bar_time,signal_bar_time,data_source,data_provider,
                data_quality,evidence_status,evidence_source,last_updated_at
             ) VALUES('stockbit','stockbit_regular','idx_stock','TEST.JK','TEST.JK',
                ?1,'Stockbit NWE','4h','LONG',5.0,5.0,0.0,?2,100.0,
                '4h',?3,?4,'stockbit_regular_yfinance_fallback','yfinance',
                'RESEARCH_PROXY','VALIDATION_PENDING','',?5)",
            rusqlite::params![
                quant_core::STOCKBIT_NWE_4H_SIGNAL_STRATEGY_ID,
                serde_json::json!({
                    "model_id": quant_core::STOCKBIT_NWE_4H_SIGNAL_SCORE_MODEL_ID_LEGACY,
                    "score_scale_max": 10,
                    "minimum_score_out_of_10": 5,
                    "component_score_scale_max": 10,
                    "score_model_status": "legacy",
                    "status": "stale",
                })
                .to_string(),
                fresh_price_bar,
                expired_signal_bar,
                now
            ],
        )
        .expect("expired snapshot row should persist for the read-path check");

    let payload = list_signal_screener(&connection, 900, None, None)
        .expect("the screener snapshot should load");
    let signals = payload["signals"]
        .as_array()
        .expect("Stockbit score rows should remain visible");
    assert_eq!(signals.len(), 1);
    assert_eq!(signals[0]["score"], 5.0);
    assert_eq!(signals[0]["score_status"], "stale");
    assert_eq!(signals[0]["score_basis"]["score_scale_max"], 10);
    assert_eq!(signals[0]["score_basis"]["score_model_status"], "legacy");
    assert_eq!(payload["summary"]["stale_price_signal_count"], 0);
}
