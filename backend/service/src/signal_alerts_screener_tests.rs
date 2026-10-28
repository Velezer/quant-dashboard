use super::{
    list_signal_screener, list_signal_screener_page, replace_signal_screener_snapshot,
    replace_signal_screener_snapshot_with_score_updates,
    replace_signal_screener_snapshot_with_status, signal_screener_row,
    signal_screener_score_update,
};
use rusqlite::Connection;
use serde_json::json;

fn stockbit_candidate(price_bar_time: i64) -> serde_json::Value {
    json!({
        "Signal_Direction": "LONG",
        "Exchange_ID": "stockbit",
        "Product_ID": "stockbit_regular",
        "Asset_Category_ID": "idx_stock",
        "Symbol": "AADI.JK",
        "Signal_Strategy_ID": "idx_open_low_rvol",
        "timeframe": "15m",
        "Current_Price": 6200.0,
        "Current_Price_Timeframe": "15m",
        "Current_Price_Bar_Time": price_bar_time,
        "Signal_Bar_Time": price_bar_time,
        "Signal_Strength": 8.0,
        "Signal_Confluence_Score": 7.0,
        "Signal_Confluence": {
            "dss_state": "NEAR_CROSS",
            "nadaraya_state": "LOWER_ENVELOPE_WICK_TOUCH",
            "dss_extreme_zone": true,
            "dss_points": 2.0,
            "dss_extreme_points": 3.0,
            "nadaraya_touch_points": 2.0,
            "nadaraya_near_line_points": 1.0,
            "nadaraya_reclaim_points": 0.0
        }
    })
}

fn stockbit_nwe_candidate(price_bar_time: i64, score: f64) -> serde_json::Value {
    let nwe_points = score.clamp(0.0, 10.0);
    let dss_points = (score - 10.0).clamp(0.0, 10.0);
    let volume_points = (score - 20.0).clamp(0.0, 10.0);
    let delta_points = (score - 30.0).clamp(0.0, 10.0);
    let price_score = nwe_points + dss_points;
    let volume_score = volume_points + delta_points;
    json!({
        "Signal_Direction": "SHORT",
        "Exchange_ID": "stockbit",
        "Product_ID": "stockbit_regular",
        "Asset_Category_ID": "idx_stock",
        "Symbol": "AADI.JK",
        "Signal_Strategy_ID": quant_core::STOCKBIT_NWE_4H_SIGNAL_STRATEGY_ID,
        "timeframe": "4h",
        "Current_Price": 6200.0,
        "Current_Price_Timeframe": "4h",
        "Current_Price_Bar_Time": price_bar_time,
        "Signal_Bar_Time": price_bar_time,
        "Signal_Strength": score,
        "Strategy_Strength": price_score,
        "Signal_Confluence_Score": volume_score,
        "Signal_Scoring_Model_ID": quant_core::STOCKBIT_NWE_4H_SIGNAL_SCORE_MODEL_ID,
        "Signal_Score_Components": [
            { "indicator": "Nadaraya-Watson Envelope", "state": "NWE_SCORE", "points": nwe_points },
            { "indicator": "DSS Bressert", "state": "DSS_SCORE", "points": dss_points },
            { "indicator": "Relative Volume", "state": "ALIGNED", "points": volume_points },
            { "indicator": "Estimated Volume Delta (OHLCV proxy)", "state": "ALIGNED_PROXY", "points": delta_points }
        ]
    })
}

fn crypto_nwe_candidate(
    timeframe: &str,
    price_bar_time: i64,
    nwe_points: f64,
    dss_points: f64,
) -> serde_json::Value {
    let nwe_component = nwe_points;
    let dss_component = dss_points;
    let price_score = nwe_component + dss_component;
    json!({
        "Signal_Direction": "LONG",
        "Exchange_ID": "pintu",
        "Product_ID": "pintu_pro_futures",
        "Asset_Category_ID": "crypto_perpetual",
        "Symbol": "APT",
        "Data_Symbol": "APT-USD",
        "Signal_Strategy_ID": quant_core::crypto_nwe_signal_strategy_id(timeframe).unwrap(),
        "timeframe": timeframe,
        "Current_Price": 10.0,
        "Current_Price_Timeframe": timeframe,
        "Current_Price_Bar_Time": price_bar_time,
        "Signal_Bar_Time": price_bar_time,
        "Signal_Strength": price_score,
        "Strategy_Strength": price_score,
        "Signal_Confluence_Score": 0.0,
        "Signal_Scoring_Model_ID": quant_core::STOCKBIT_NWE_4H_SIGNAL_SCORE_MODEL_ID,
        "Signal_Score_Components": [
            { "indicator": "Nadaraya-Watson Envelope", "state": "NWE_SCORE", "points": nwe_component },
            { "indicator": "DSS Bressert", "state": "DSS_SCORE", "points": dss_component },
            { "indicator": "Relative Volume", "state": "UNAVAILABLE", "points": 0.0 },
            { "indicator": "Estimated Volume Delta (OHLCV proxy)", "state": "UNAVAILABLE", "points": 0.0 }
        ],
        "Current_Price_Data_Source": "pintu_futures_bybit_linear_proxy",
        "Current_Price_Data_Provider": "bybit",
        "Data_Quality": "RESEARCH_PROXY"
    })
}

#[test]
fn screener_hides_price_bars_that_went_stale_during_a_slow_scan() {
    let connection = rusqlite::Connection::open_in_memory().unwrap();
    connection
        .execute_batch("ATTACH DATABASE ':memory:' AS app;")
        .unwrap();
    crate::master_data::initialize(&connection).unwrap();
    let now_ns = i64::try_from(crate::now_secs())
        .unwrap_or(i64::MAX)
        .saturating_mul(1_000_000_000);
    let recent_completed_bar = now_ns.saturating_sub(16 * 60 * 1_000_000_000);
    let stale_completed_bar = now_ns.saturating_sub(46 * 60 * 1_000_000_000);

    assert!(signal_screener_row(&stockbit_candidate(recent_completed_bar)).is_some());
    assert!(signal_screener_row(&stockbit_candidate(stale_completed_bar)).is_none());
}

#[test]
fn stockbit_nwe_rows_require_the_catalogued_one_point_floor() {
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch("ATTACH DATABASE ':memory:' AS app;")
        .unwrap();
    crate::master_data::initialize(&connection).unwrap();
    let now_ns = i64::try_from(crate::now_secs())
        .unwrap_or(i64::MAX)
        .saturating_mul(1_000_000_000);
    let fresh_bar = now_ns.saturating_sub(5 * 60 * 60 * 1_000_000_000);

    assert!(signal_screener_row(&stockbit_nwe_candidate(fresh_bar, 0.99)).is_none());
    let row = signal_screener_row(&stockbit_nwe_candidate(fresh_bar, 1.0))
        .expect("the published score floor is inclusive");
    assert_eq!(
        row["signal_strategy_id"],
        quant_core::STOCKBIT_NWE_4H_SIGNAL_STRATEGY_ID
    );
    assert_eq!(row["score"], 1.0);
}

#[test]
fn stockbit_score_updates_renew_the_persisted_row_on_each_scan() {
    let connection = screener_connection();
    crate::master_data::initialize(&connection).expect("master data should initialize");
    let now_ns = i64::try_from(crate::now_secs())
        .unwrap_or(i64::MAX)
        .saturating_mul(1_000_000_000);
    let initial_bar = now_ns.saturating_sub(5 * 60 * 60 * 1_000_000_000);
    let initial_row = signal_screener_row(&stockbit_nwe_candidate(initial_bar, 4.0))
        .expect("initial qualifying signal should be persistable");
    replace_signal_screener_snapshot(&connection, &[initial_row])
        .expect("initial signal should persist");

    let mut next_scan = stockbit_nwe_candidate(initial_bar, 8.0);
    let timeframe_ns = 4 * 60 * 60 * 1_000_000_000;
    let forming_bar = now_ns.div_euclid(timeframe_ns) * timeframe_ns;
    next_scan["Score_Update_Only"] = json!(true);
    next_scan["Signal_Direction"] = json!("LONG");
    next_scan["Signal_Strength"] = json!(8.0);
    next_scan["Strategy_Strength"] = json!(6.0);
    next_scan["Signal_Confluence_Score"] = json!(2.0);
    next_scan["Signal_Score_Components"] = json!([
        {"indicator": "Nadaraya-Watson Envelope", "state": "NWE_SCORE", "points": 4.0},
        {"indicator": "DSS Bressert", "state": "DSS_SCORE", "points": 2.0},
        {"indicator": "Relative Volume", "state": "ALIGNED", "points": 1.0},
        {"indicator": "Estimated Volume Delta (OHLCV proxy)", "state": "ALIGNED_PROXY", "points": 1.0}
    ]);
    next_scan["Score_Update_Status"] = json!("qualifying");
    next_scan["Signal_Score_Bar_Time"] = json!(forming_bar);
    next_scan["Signal_Score_Bar_Is_Forming"] = json!(true);
    next_scan["Current_Price_Timeframe"] = json!("15m");
    let current_price_bar = now_ns.div_euclid(15 * 60 * 1_000_000_000) * (15 * 60 * 1_000_000_000)
        - 15 * 60 * 1_000_000_000;
    next_scan["Current_Price_Bar_Time"] = json!(current_price_bar);
    next_scan["Signal_Bar_Time"] = json!(forming_bar);
    next_scan["Data_Source"] = json!("stockbit_test");
    next_scan["Data_Provider"] = json!("stockbit");
    next_scan["Data_Quality"] = json!("APP_NATIVE");
    let score_update = signal_screener_score_update(&next_scan)
        .expect("fresh qualifying scan score should validate");

    replace_signal_screener_snapshot_with_score_updates(&connection, &[], &[score_update], 0, 1)
        .expect("scan should transactionally renew the saved score");

    let renewed = list_signal_screener(&connection, 900, Some("stockbit"), Some("4h"))
        .expect("renewed screener snapshot should be readable");
    let row = renewed["signals"]
        .as_array()
        .and_then(|rows| rows.first())
        .expect("saved signal should remain visible after the scan");
    assert_eq!(row["score"], 8.0);
    assert_eq!(row["direction"], "LONG");
    assert_eq!(row["score_status"], "qualifying");
    assert_eq!(row["score_bar_time"], forming_bar);
    assert_eq!(row["score_bar_is_forming"], true);
}

#[test]
fn four_indicator_components_each_cap_at_ten_and_add_to_forty() {
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch("ATTACH DATABASE ':memory:' AS app;")
        .unwrap();
    crate::master_data::initialize(&connection).unwrap();
    let now_ns = i64::try_from(crate::now_secs())
        .unwrap_or(i64::MAX)
        .saturating_mul(1_000_000_000);
    let fresh_bar = now_ns.saturating_sub(5 * 60 * 60 * 1_000_000_000);
    let mut candidate = stockbit_nwe_candidate(fresh_bar, 40.0);
    candidate["Strategy_Strength"] = json!(20.0);
    candidate["Signal_Confluence_Score"] = json!(20.0);
    for index in 0..4 {
        candidate["Signal_Score_Components"][index]["points"] = json!(10.0);
    }

    let row = signal_screener_row(&candidate).expect("four ten-point factors should sum to 40");
    assert_eq!(row["score"], 40.0);
    assert_eq!(row["score_scale_max"], 40.0);
    assert_eq!(
        row["minimum_score_out_of_40"],
        quant_core::FOUR_INDICATOR_MINIMUM_SCORE_OUT_OF_40
    );

    candidate["Strategy_Strength"] = json!(20.1);
    assert!(signal_screener_row(&candidate).is_none());

    candidate["Strategy_Strength"] = json!(20.0);
    candidate["Signal_Confluence_Score"] = json!(19.0);
    assert!(signal_screener_row(&candidate).is_none());
}

#[test]
fn catalog_free_crypto_timeframe_models_use_the_forty_point_scale() {
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch("ATTACH DATABASE ':memory:' AS app;")
        .unwrap();
    crate::master_data::initialize(&connection).unwrap();
    let now_ns = i64::try_from(crate::now_secs())
        .unwrap_or(i64::MAX)
        .saturating_mul(1_000_000_000);
    let fresh_15m_bar = now_ns.saturating_sub(16 * 60 * 1_000_000_000);

    let candidate = crypto_nwe_candidate("15m", fresh_15m_bar, 6.0, 5.0);
    let row = signal_screener_row(&candidate)
        .expect("catalog-free crypto NWE/DSS scores should render on the 40-point scale");
    assert_eq!(row["score"], 11.0);
    assert_eq!(row["score_scale_max"], 40.0);
    assert_eq!(
        row["minimum_score_out_of_40"],
        quant_core::FOUR_INDICATOR_MINIMUM_SCORE_OUT_OF_40
    );
    assert_eq!(row["component_score_scale_max"], 20.0);
    assert_eq!(row["timeframe"], "15m");

    let mut dss_candidate = candidate.clone();
    dss_candidate["Signal_Strategy_ID"] =
        json!(quant_core::crypto_dss_bressert_signal_strategy_id("15m").unwrap());
    dss_candidate["Signal_Scoring_Model_ID"] =
        json!(quant_core::STOCKBIT_DSS_BRESSERT_4H_SIGNAL_SCORE_MODEL_ID);
    dss_candidate["Signal_Score_Components"] = json!([
        { "indicator": "Nadaraya-Watson Envelope", "state": "NWE_SCORE", "points": 5.0 },
        { "indicator": "DSS Bressert", "state": "DSS_SCORE", "points": 6.0 },
        { "indicator": "Relative Volume", "state": "UNAVAILABLE", "points": 0.0 },
        { "indicator": "Estimated Volume Delta (OHLCV proxy)", "state": "UNAVAILABLE", "points": 0.0 }
    ]);
    dss_candidate["Signal_Strength"] = json!(11.0);
    dss_candidate["Strategy_Strength"] = json!(11.0);
    dss_candidate["Signal_Confluence_Score"] = json!(0.0);
    let dss_row = signal_screener_row(&dss_candidate)
        .expect("catalog-free crypto DSS scores must share the same 40-point scale");
    assert_eq!(dss_row["score"], 11.0);
    assert_eq!(dss_row["score_scale_max"], 40.0);
    assert_eq!(
        dss_row["minimum_score_out_of_40"],
        quant_core::FOUR_INDICATOR_MINIMUM_SCORE_OUT_OF_40
    );

    assert!(signal_screener_row(&crypto_nwe_candidate("15m", fresh_15m_bar, 10.1, 1.0)).is_none());
}

#[test]
fn crypto_indicator_signals_render_from_one_point_and_keep_indicator_timeframes_independent() {
    let connection = screener_connection();
    crate::master_data::initialize(&connection).unwrap();
    let now_ns = i64::try_from(crate::now_secs())
        .unwrap_or(i64::MAX)
        .saturating_mul(1_000_000_000);
    let fresh_bar = now_ns.saturating_sub(16 * 60 * 1_000_000_000);
    let fresh_hour_bar = now_ns.saturating_sub(2 * 60 * 60 * 1_000_000_000);
    let candidate = crypto_nwe_candidate("15m", fresh_bar, 1.0, 0.0);

    let row = signal_screener_row(&candidate)
        .expect("a one-point NWE signal should meet the inclusive 1/40 floor");
    assert_eq!(row["score"], 1.0);
    assert_eq!(row["score_scale_max"], 40.0);
    assert_eq!(
        row["minimum_score_out_of_40"],
        quant_core::FOUR_INDICATOR_MINIMUM_SCORE_OUT_OF_40
    );

    let mut unavailable_dss = candidate.clone();
    unavailable_dss["Signal_Confluence_Score"] = serde_json::Value::Null;
    unavailable_dss["Signal_Confluence"] = serde_json::Value::Null;
    unavailable_dss["Signal_Score_Components"] = json!([
        { "indicator": "Nadaraya-Watson Envelope", "state": "NWE_SCORE", "points": 1.0 },
        { "indicator": "DSS Bressert", "state": "UNAVAILABLE", "points": 0.0 },
        { "indicator": "Relative Volume", "state": "UNAVAILABLE", "points": 0.0 },
        { "indicator": "Estimated Volume Delta (OHLCV proxy)", "state": "UNAVAILABLE", "points": 0.0 }
    ]);
    let row = signal_screener_row(&unavailable_dss)
        .expect("a missing second indicator should contribute zero, not suppress NWE");
    assert_eq!(row["score"], 1.0);
    assert_eq!(row["confluence_score"], 0.0);
    assert_eq!(row["score_components"][1]["state"], "UNAVAILABLE");

    let other_timeframe = crypto_nwe_candidate("1h", fresh_hour_bar, 2.0, 0.0);
    let mut other_indicator = crypto_nwe_candidate("15m", fresh_bar, 2.0, 0.0);
    other_indicator["Signal_Strategy_ID"] =
        json!(quant_core::crypto_dss_bressert_signal_strategy_id("15m").unwrap());
    other_indicator["Signal_Scoring_Model_ID"] =
        json!(quant_core::STOCKBIT_DSS_BRESSERT_4H_SIGNAL_SCORE_MODEL_ID);
    other_indicator["Signal_Score_Components"] = json!([
        { "indicator": "Nadaraya-Watson Envelope", "state": "NWE_UNAVAILABLE", "points": 0.0 },
        { "indicator": "DSS Bressert", "state": "DSS_SCORE", "points": 1.0 },
        { "indicator": "Relative Volume", "state": "UNAVAILABLE", "points": 0.0 },
        { "indicator": "Estimated Volume Delta (OHLCV proxy)", "state": "UNAVAILABLE", "points": 0.0 }
    ]);
    other_indicator["Signal_Strength"] = json!(1.0);
    other_indicator["Strategy_Strength"] = json!(1.0);
    other_indicator["Signal_Confluence_Score"] = json!(0.0);

    let rows = [candidate, other_timeframe, other_indicator]
        .iter()
        .map(|candidate| {
            signal_screener_row(candidate)
                .unwrap_or_else(|| panic!("signal candidate did not qualify: {candidate}"))
        })
        .collect::<Vec<_>>();
    let identities = rows
        .iter()
        .map(|row| {
            (
                row["signal_strategy_id"].as_str().unwrap(),
                row["timeframe"].as_str().unwrap(),
            )
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(identities.len(), 3);
    assert!(identities.contains(&("crypto_nwe_band_proximity_15m_v1", "15m")));
    assert!(identities.contains(&("crypto_nwe_band_proximity_1h_v1", "1h")));
    assert!(identities.contains(&("crypto_dss_bressert_near_cross_15m_v1", "15m")));

    replace_signal_screener_snapshot(&connection, &rows)
        .expect("distinct indicator/timeframe rows for one ticker should persist");
    let payload = list_signal_screener(&connection, 900, None, None)
        .expect("persisted timeframe rows should load independently");
    let stored_rows = payload["signals"]
        .as_array()
        .expect("the screener response should contain signal rows");
    assert_eq!(stored_rows.len(), 3);
    assert!(stored_rows
        .iter()
        .all(|row| { row["symbol"] == "APT" && row["score_basis"]["score_scale_max"] == 40.0 }));
    let stored_identities = stored_rows
        .iter()
        .map(|row| {
            (
                row["signal_strategy_id"].as_str().unwrap(),
                row["timeframe"].as_str().unwrap(),
            )
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(stored_identities, identities);
}

#[test]
fn screener_rows_show_required_setup_and_numeric_score_contributions() {
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch("ATTACH DATABASE ':memory:' AS app;")
        .unwrap();
    crate::master_data::initialize(&connection).unwrap();
    let now_ns = i64::try_from(crate::now_secs())
        .unwrap_or(i64::MAX)
        .saturating_mul(1_000_000_000);
    let row = signal_screener_row(&stockbit_candidate(
        now_ns.saturating_sub(16 * 60 * 1_000_000_000),
    ))
    .expect("fresh directional candidates should enter the screener");
    let components = row["score_components"]
        .as_array()
        .expect("score components should be available");

    assert_eq!(components.len(), 4);
    assert_eq!(components[0]["state"], "NEAR_CROSS");
    assert_eq!(components[0]["points"], 2.0);
    assert_eq!(components[1]["state"], "EXTREME_ZONE_TURN");
    assert_eq!(components[1]["points"], 3.0);
    assert_eq!(components[2]["state"], "LOWER_ENVELOPE_WICK_TOUCH");
    assert_eq!(components[2]["points"], 2.0);
    assert_eq!(components[3]["state"], "NEAR_ENVELOPE_LINE");
    assert_eq!(components[3]["points"], 1.0);
}

fn stored_screener_row(symbol: &str, price_bar_time: i64) -> serde_json::Value {
    json!({
        "exchange_id": "stockbit",
        "product_id": "stockbit_regular",
        "asset_category_id": "idx_stock",
        "symbol": symbol,
        "storage_symbol": symbol,
        "signal_strategy_id": "idx_open_low_rvol",
        "signal_strategy_name": "IDX Open Low RVOL",
        "timeframe": "15m",
        "direction": "LONG",
        "score": 8.0,
        "strategy_strength": 8.0,
        "confluence_score": 7.0,
        "score_components": [],
        "current_price": 6200.0,
        "price_timeframe": "15m",
        "price_bar_time": price_bar_time,
        "signal_bar_time": price_bar_time,
        "data_source": "stockbit_research_proxy",
        "data_provider": "yfinance",
        "data_quality": "research proxy",
        "evidence_status": "VALIDATION_PENDING",
        "evidence_source": ""
    })
}

fn screener_connection() -> Connection {
    let connection = Connection::open_in_memory().expect("in-memory app database should open");
    connection
        .execute_batch("ATTACH DATABASE ':memory:' AS app; ATTACH DATABASE ':memory:' AS signals;")
        .expect("app schema should attach");
    super::super::initialize_schema(&connection).expect("screener schema should initialize");
    super::super::initialize_screener_schema(&connection)
        .expect("signal snapshot schema should initialize");
    crate::master_data::initialize(&connection).expect("master-data catalog should initialize");
    connection
}

#[test]
fn snapshot_stores_all_scan_rows_even_beyond_a_single_page() {
    let connection = screener_connection();
    let now_ns = i64::try_from(crate::now_secs()).unwrap() * crate::NANOS_PER_SECOND;
    let rows = (0..2_001)
        .map(|index| stored_screener_row(&format!("SIG-{index:04}.JK"), now_ns))
        .collect::<Vec<_>>();

    replace_signal_screener_snapshot(&connection, &rows).expect("all scan rows should persist");

    let stored: i64 = connection
        .query_row("SELECT COUNT(*) FROM signals.signal_screener_rows", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(stored, 2_001);
    let page = list_signal_screener_page(
        &connection,
        900,
        None,
        None,
        None,
        Some("SIG-2000"),
        0,
        50,
    )
    .expect("the row beyond the old cutoff should be searchable");
    assert_eq!(page["signals"][0]["symbol"], "SIG-2000.JK");
}

#[test]
fn screener_read_retains_unexpired_setups_with_stale_price_bars() {
    let connection = screener_connection();
    let now_ns = i64::try_from(crate::now_secs())
        .unwrap_or(i64::MAX)
        .saturating_mul(1_000_000_000);
    let fresh_bar = now_ns.saturating_sub(16 * 60 * 1_000_000_000);
    let stale_bar = now_ns.saturating_sub(46 * 60 * 1_000_000_000);
    replace_signal_screener_snapshot(
        &connection,
        &[
            stored_screener_row("FRESH.JK", fresh_bar),
            stored_screener_row("STALE.JK", stale_bar),
        ],
    )
    .expect("screener rows should save");

    let payload =
        list_signal_screener(&connection, 900, None, None).expect("screener snapshot should load");
    assert_eq!(payload["signals"].as_array().unwrap().len(), 2);
    assert_eq!(payload["signals"][0]["symbol"], "FRESH.JK");
    assert_eq!(payload["signals"][1]["symbol"], "STALE.JK");
    assert_eq!(payload["signals"][1]["price_is_fresh"], false);
    assert_eq!(payload["summary"]["stale_price_signal_count"], 1);
    assert_eq!(payload["summary"]["status"]["last_attempt_status"], "ready");
}

#[test]
fn screener_pages_filter_and_reach_rows_beyond_the_first_page() {
    let connection = screener_connection();
    let now_ns = i64::try_from(crate::now_secs())
        .unwrap_or(i64::MAX)
        .saturating_mul(1_000_000_000);
    let fresh_bar = now_ns.saturating_sub(16 * 60 * 1_000_000_000);
    let rows = (0..250)
        .map(|index| stored_screener_row(&format!("SIG-{index:04}.JK"), fresh_bar))
        .collect::<Vec<_>>();
    replace_signal_screener_snapshot(&connection, &rows).expect("test rows should persist");

    let first = list_signal_screener_page(
        &connection,
        900,
        Some("stockbit"),
        Some("15m"),
        Some("stockbit_regular"),
        None,
        0,
        50,
    )
    .expect("first page should load");
    let second = list_signal_screener_page(
        &connection,
        900,
        Some("stockbit"),
        Some("15m"),
        Some("stockbit_regular"),
        None,
        50,
        50,
    )
    .expect("second page should load");
    let search = list_signal_screener_page(
        &connection,
        900,
        Some("stockbit"),
        Some("15m"),
        Some("stockbit_regular"),
        Some("SIG-0249"),
        0,
        50,
    )
    .expect("filtered search should load");
    let strategy_id_search = list_signal_screener_page(
        &connection,
        900,
        Some("stockbit"),
        Some("15m"),
        Some("stockbit_regular"),
        Some("open_low_rvol"),
        0,
        50,
    )
    .expect("strategy-ID search should load");
    let wildcard_search = list_signal_screener_page(
        &connection,
        900,
        Some("stockbit"),
        Some("15m"),
        Some("stockbit_regular"),
        Some("%"),
        0,
        50,
    )
    .expect("literal wildcard search should load");

    assert_eq!(first["signals"].as_array().unwrap().len(), 50);
    assert_eq!(first["summary"]["matching_rows"], 250);
    assert_eq!(first["summary"]["next_offset"], 50);
    assert_eq!(first["summary"]["has_more"], true);
    assert_eq!(second["signals"].as_array().unwrap().len(), 50);
    assert_eq!(second["summary"]["next_offset"], 100);
    assert_eq!(search["signals"].as_array().unwrap().len(), 1);
    assert_eq!(search["signals"][0]["symbol"], "SIG-0249.JK");
    assert_eq!(search["summary"]["has_more"], false);
    assert_eq!(strategy_id_search["signals"].as_array().unwrap().len(), 50);
    assert_eq!(strategy_id_search["summary"]["matching_rows"], 250);
    assert_eq!(wildcard_search["signals"].as_array().unwrap().len(), 0);
    assert_eq!(wildcard_search["summary"]["matching_rows"], 0);
}

#[test]
fn screener_freshness_uses_the_exact_idx_storage_symbol() {
    let connection = screener_connection();
    let now_ns = i64::try_from(crate::now_secs())
        .unwrap_or(i64::MAX)
        .saturating_mul(1_000_000_000);
    let completed_idx_bar = now_ns.saturating_sub(30 * 3_600 * 1_000_000_000);
    let recent_signal_bar = now_ns.saturating_sub(5 * 3_600 * 1_000_000_000);
    let mut row = stored_screener_row("MGNA", completed_idx_bar);
    row["storage_symbol"] = json!("MGNA.JK");
    row["timeframe"] = json!("4h");
    row["price_timeframe"] = json!("4h");
    row["signal_bar_time"] = json!(recent_signal_bar);
    replace_signal_screener_snapshot(&connection, &[row]).expect("the IDX signal row should save");

    let payload =
        list_signal_screener(&connection, 900, None, None).expect("screener snapshot should load");

    assert_eq!(payload["signals"].as_array().unwrap().len(), 1);
    assert_eq!(payload["signals"][0]["symbol"], "MGNA");
    assert_eq!(payload["signals"][0]["storage_symbol"], "MGNA.JK");
    assert_eq!(payload["summary"]["stale_price_signal_count"], 0);
}

#[test]
fn screener_read_prioritizes_15m_rows_when_response_is_capped() {
    let connection = screener_connection();
    crate::master_data::initialize(&connection).expect("master-data catalog should initialize");
    let now_ns = i64::try_from(crate::now_secs())
        .unwrap_or(i64::MAX)
        .saturating_mul(1_000_000_000);
    let stockbit_bar = now_ns.saturating_sub(25 * 60 * 60 * 1_000_000_000);
    let mut stockbit_rows = Vec::with_capacity(2_001);
    for index in 0..2_001 {
        let mut row = stockbit_nwe_candidate(stockbit_bar, 10.0);
        row["Signal_Direction"] = json!("SHORT");
        row["Symbol"] = json!(format!("CAP{index:04}.JK"));
        row["Signal_Strategy_ID"] =
            json!(quant_core::stockbit_nwe_signal_strategy_id("1d").unwrap());
        row["timeframe"] = json!("1d");
        row["Current_Price_Timeframe"] = json!("1d");
        stockbit_rows.push(signal_screener_row(&row).expect("Stockbit row should qualify"));
    }
    let mut pluang_15m = crypto_nwe_candidate(
        "15m",
        now_ns.saturating_sub(16 * 60 * 1_000_000_000),
        3.0,
        2.0,
    );
    pluang_15m["Signal_Direction"] = json!("LONG");
    pluang_15m["Exchange_ID"] = json!("pluang");
    pluang_15m["Product_ID"] = json!("pluang_spot");
    pluang_15m["Asset_Category_ID"] = json!("crypto_spot");
    let pluang_row = signal_screener_row(&pluang_15m).expect("Pluang 15m row should qualify");
    stockbit_rows.insert(0, pluang_row);

    replace_signal_screener_snapshot(&connection, &stockbit_rows)
        .expect("crowded screener snapshot should save");

    let payload = list_signal_screener(&connection, 900, None, None)
        .expect("capped screener response should load");
    let signals = payload["signals"].as_array().unwrap();
    assert_eq!(signals.len(), 2_000);
    assert_eq!(signals[0]["exchange_id"], "pluang");
    assert_eq!(signals[0]["timeframe"], "15m");
    assert!(signals[0]["score"].as_f64().unwrap_or_default() >= 1.0);
}

#[test]
fn screener_read_deletes_snapshot_rows_after_the_two_hour_retention() {
    let connection = screener_connection();
    let now = crate::now_secs() as i64;
    let fresh_bar = now
        .saturating_mul(1_000_000_000)
        .saturating_sub(16 * 60 * 1_000_000_000);
    replace_signal_screener_snapshot(&connection, &[stored_screener_row("FRESH.JK", fresh_bar)])
        .expect("screener row should save");
    connection
        .execute(
            "UPDATE signals.signal_screener_rows SET last_updated_at=?1",
            [now.saturating_sub(super::super::SIGNAL_MAX_AGE_SECONDS)],
        )
        .expect("row should age to its expiry boundary");

    let payload = list_signal_screener(&connection, 900, None, None)
        .expect("expired rows should be cleaned during screener read");
    let stored_rows = connection
        .query_row(
            "SELECT COUNT(*) FROM signals.signal_screener_rows",
            [],
            |row| row.get::<_, i64>(0),
        )
        .expect("row count should be readable");
    assert!(payload["signals"].as_array().unwrap().is_empty());
    assert_eq!(stored_rows, 0);
}

#[test]
fn screener_status_explains_when_fresh_price_data_withholds_the_universe() {
    let connection = screener_connection();
    replace_signal_screener_snapshot_with_status(&connection, &[], 3, 3)
        .expect("stale price status should save");

    let payload =
        list_signal_screener(&connection, 900, None, None).expect("screener status should load");
    assert_eq!(payload["summary"]["total_signals"], 0);
    assert_eq!(
        payload["summary"]["status"]["price_data_status"],
        "unavailable"
    );
    assert_eq!(
        payload["summary"]["status"]["price_data_unavailable_count"],
        3
    );
}
