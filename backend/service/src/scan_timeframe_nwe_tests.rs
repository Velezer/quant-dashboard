use super::*;
use crate::main_test_support::{remove_temporary_database, temporary_database_path};

const PRODUCT: &str = "stockbit_regular";
const STORAGE_SYMBOL: &str = "TEST.JK";
const SOURCE: &str = "stockbit_regular_yfinance_fallback";
const FIFTEEN_MINUTES_NANOSECONDS: i64 = 15 * 60 * 1_000_000_000;
const HOUR_NANOSECONDS: i64 = 60 * 60 * 1_000_000_000;
const FOUR_HOUR_NANOSECONDS: i64 = 4 * HOUR_NANOSECONDS;

fn insert_stockbit_15m_history(connection: &rusqlite::Connection, closes: &[f64]) {
    let source_bars = i64::try_from(closes.len()).expect("fixture history fits in SQLite");
    let now_ns = i64::try_from(now_secs()).unwrap_or(i64::MAX) * 1_000_000_000;
    let latest_15m_open = now_ns - FIFTEEN_MINUTES_NANOSECONDS - 60_000_000_000;
    let first_15m_open = latest_15m_open - (source_bars - 1) * FIFTEEN_MINUTES_NANOSECONDS;
    connection
        .execute(
            "INSERT INTO series(symbol,interval,source,exchange_id,product_id,data_provider,
                                timezone,price_basis,instrument_kind,last_success)
             VALUES(?1,'15m',?2,'stockbit',?3,'yfinance','Asia/Jakarta','last_trade',
                    'equity_proxy',strftime('%Y-%m-%d %H:%M:%S','now') || ' UTC')",
            params![STORAGE_SYMBOL, SOURCE, PRODUCT],
        )
        .expect("product-scoped 15m research series should exist");
    for (index, close) in closes.iter().copied().enumerate() {
        let time = first_15m_open + index as i64 * FIFTEEN_MINUTES_NANOSECONDS;
        connection
            .execute(
                "INSERT INTO bars(symbol,interval,source,ts,open,high,low,close,volume,ingested_at)
                 VALUES(?1,'15m',?2,?3,100,?4,?5,?6,1000,'test')",
                params![
                    STORAGE_SYMBOL,
                    SOURCE,
                    time,
                    close.max(100.0) + 1.0,
                    close.min(100.0) - 1.0,
                    close,
                ],
            )
            .expect("15m fixture candles should persist");
    }
}

#[test]
fn stockbit_15m_upper_band_cross_emits_a_timeframe_specific_signal_from_fresh_bars() {
    let path = temporary_database_path("stockbit-nwe-15m");
    let connection = open_db_minimal(&path).expect("temporary storage should initialize");
    let required_bars = quant_core::stockbit_nwe_4h_minimum_history_bars();
    let mut closes = vec![100.0; required_bars];
    *closes.last_mut().expect("history includes a final candle") = 120.0;
    insert_stockbit_15m_history(&connection, &closes);

    let mut standalone_signals = Vec::new();
    let has_fresh_15m_price = append_timeframe_signal_plans(
        TimeframeScanInput {
            app_connection: &connection,
            market_connection: &connection,
            sync_state: None,
            exchange_id: quant_core::STOCKBIT_NWE_4H_EXCHANGE_ID,
            product_id: PRODUCT,
            category_id: quant_core::STOCKBIT_NWE_4H_ASSET_CATEGORY_ID,
            symbol: STORAGE_SYMBOL,
            asset: "TEST",
            strategy: "idx_stockbit_day_trade",
            evidence_class: "idx",
            class: "IDX_STOCK",
            exchange: "STOCKBIT",
            time_window: "Stockbit 15m research",
            base_timeframe: "15m",
            is_tokenized: false,
            is_multibagger: false,
            retained_stockbit_signal_rows: &std::collections::BTreeSet::new(),
        },
        &mut Vec::new(),
        &mut Vec::new(),
        &mut HashMap::new(),
        &mut Vec::new(),
        &mut standalone_signals,
        &mut Vec::new(),
    )
    .expect("the bounded timeframe scan should complete");

    assert!(has_fresh_15m_price);
    let signal = standalone_signals
        .iter()
        .find(|signal| signal["Signal_Strategy_ID"] == "stockbit_nwe_band_proximity_15m_v1")
        .expect("a qualifying 15m NWE setup should emit its own timeframe signal");
    let row = signal_alerts::signal_screener_row(signal)
        .expect("the qualifying 15m signal should be renderable");
    assert_eq!(row["timeframe"], "15m");
    assert_eq!(row["direction"], "SHORT");
    assert_eq!(
        row["score_model_id"],
        quant_core::STOCKBIT_NWE_4H_SIGNAL_SCORE_MODEL_ID
    );
    assert!(row["score"].as_f64().unwrap() >= 1.0);
    assert!(row["score"].as_f64().unwrap() <= 20.0);
    assert_eq!(row["data_quality"], "RESEARCH_PROXY");

    drop(connection);
    remove_temporary_database(&path);
}

fn insert_stockbit_4h_history(
    connection: &rusqlite::Connection,
    latest_4h_open: i64,
    closes: &[f64],
) {
    let source_4h_bars = i64::try_from(closes.len()).expect("fixture history fits in SQLite");
    let first_4h_open = latest_4h_open - (source_4h_bars - 1) * FOUR_HOUR_NANOSECONDS;
    connection
        .execute(
            "INSERT INTO series(symbol,interval,source,exchange_id,product_id,data_provider,
                                timezone,price_basis,instrument_kind,last_success)
             VALUES(?1,'4h',?2,'stockbit',?3,'yfinance','Asia/Jakarta','last_trade',
                    'equity_proxy',strftime('%Y-%m-%d %H:%M:%S','now') || ' UTC')",
            params![STORAGE_SYMBOL, SOURCE, PRODUCT],
        )
        .expect("product-scoped 4h research series should exist");
    for (index, close) in closes.iter().copied().enumerate() {
        let time = first_4h_open + index as i64 * FOUR_HOUR_NANOSECONDS;
        connection
            .execute(
                "INSERT INTO bars(symbol,interval,source,ts,open,high,low,close,volume,ingested_at)
                 VALUES(?1,'4h',?2,?3,100,?4,?5,?6,1000,'test')",
                params![
                    STORAGE_SYMBOL,
                    SOURCE,
                    time,
                    close.max(100.0) + 1.0,
                    close.min(100.0) - 1.0,
                    close,
                ],
            )
            .expect("4h fixture candles should persist");
    }
}

#[test]
fn stockbit_4h_upper_band_cross_emits_a_short_with_a_fresh_15m_price() {
    let path = temporary_database_path("stockbit-nwe-4h");
    let connection = open_db_minimal(&path).expect("temporary storage should initialize");
    let envelope_bars = quant_core::stockbit_nwe_4h_minimum_history_bars();
    let mut closes = vec![100.0; envelope_bars];
    *closes.last_mut().expect("history includes a final candle") = 120.0;
    insert_stockbit_15m_history(&connection, &[120.0]);
    let latest_4h_open =
        aggregate_candles_from_base_source(&connection, STORAGE_SYMBOL, "4h", "15m", 1, SOURCE)
            .expect("the fresh 15m candle should aggregate")
            .last()
            .expect("the 15m candle should form a 4h bucket")
            .time;
    insert_stockbit_4h_history(&connection, latest_4h_open, &closes);

    let mut running = Vec::new();
    let mut potential = Vec::new();
    let mut standalone_signals = Vec::new();
    let has_fresh_15m_price = append_timeframe_signal_plans(
        TimeframeScanInput {
            app_connection: &connection,
            market_connection: &connection,
            sync_state: None,
            exchange_id: quant_core::STOCKBIT_NWE_4H_EXCHANGE_ID,
            product_id: PRODUCT,
            category_id: quant_core::STOCKBIT_NWE_4H_ASSET_CATEGORY_ID,
            symbol: STORAGE_SYMBOL,
            asset: "TEST",
            strategy: "idx_stockbit_day_trade",
            evidence_class: "idx",
            class: "IDX_STOCK",
            exchange: "STOCKBIT",
            time_window: "Stockbit 4h research",
            base_timeframe: quant_core::STOCKBIT_NWE_4H_TIMEFRAME,
            is_tokenized: false,
            is_multibagger: false,
            retained_stockbit_signal_rows: &std::collections::BTreeSet::new(),
        },
        &mut running,
        &mut potential,
        &mut HashMap::new(),
        &mut Vec::new(),
        &mut standalone_signals,
        &mut Vec::new(),
    )
    .expect("the bounded timeframe scan should complete");

    assert!(has_fresh_15m_price);
    assert!(running.is_empty() && potential.is_empty());
    let nwe_signal = standalone_signals
        .iter()
        .find(|signal| {
            signal["Signal_Strategy_ID"] == quant_core::STOCKBIT_NWE_4H_SIGNAL_STRATEGY_ID
        })
        .expect("the NWE strategy should retain its own screener row");
    let row = signal_alerts::signal_screener_row(nwe_signal)
        .expect("the fresh completed 4h upper-band cross should enter the screener");
    assert_eq!(row["exchange_id"], "stockbit");
    assert_eq!(row["asset_category_id"], "idx_stock");
    assert_eq!(
        row["signal_strategy_id"],
        quant_core::STOCKBIT_NWE_4H_SIGNAL_STRATEGY_ID
    );
    assert_eq!(
        row["signal_strategy_name"],
        "Stockbit 4h Nadaraya-Watson Band Proximity · Research"
    );
    assert_eq!(row["timeframe"], "4h");
    assert_eq!(row["direction"], "SHORT");
    assert_eq!(row["current_price"], 120.0);
    assert!(row["score"].as_f64().unwrap() >= quant_core::FOUR_INDICATOR_MINIMUM_SCORE_OUT_OF_40);
    assert!(row["score"].as_f64().unwrap() <= quant_core::FOUR_INDICATOR_SCORE_SCALE_MAX);
    assert!(
        row["strategy_strength"].as_f64().unwrap()
            <= quant_core::FOUR_INDICATOR_GROUP_SCORE_SCALE_MAX
    );
    assert!(
        row["confluence_score"].as_f64().unwrap()
            <= quant_core::FOUR_INDICATOR_GROUP_SCORE_SCALE_MAX
    );
    assert!(
        (row["score"].as_f64().unwrap()
            - row["strategy_strength"].as_f64().unwrap()
            - row["confluence_score"].as_f64().unwrap())
        .abs()
            < 1e-10
    );
    assert_eq!(
        row["strategy_strength"],
        quant_core::FOUR_INDICATOR_COMPONENT_SCORE_SCALE_MAX
            + row["score_components"][1]["points"].as_f64().unwrap()
    );
    assert_eq!(
        row["score_model_id"],
        quant_core::STOCKBIT_NWE_4H_SIGNAL_SCORE_MODEL_ID
    );
    assert_eq!(row["price_timeframe"], "15m");
    assert_eq!(row["data_source"], SOURCE);
    assert_eq!(row["data_provider"], "yfinance");
    assert_eq!(row["data_quality"], "RESEARCH_PROXY");
    assert_eq!(row["evidence_status"], "VALIDATION_PENDING");
    assert_eq!(row["score_components"].as_array().unwrap().len(), 4);
    assert_eq!(
        row["score_components"][0]["indicator"],
        "Nadaraya-Watson Envelope"
    );
    assert_eq!(
        row["score_components"][0]["state"],
        "NWE_UPPER_BAND_CROSS_SHORT"
    );
    assert_eq!(row["score_components"][0]["points"], 10.0);
    assert_eq!(row["score_components"][1]["indicator"], "DSS Bressert");
    assert_eq!(row["score_components"][2]["indicator"], "Relative Volume");
    assert_eq!(
        row["score_components"][3]["indicator"],
        "Estimated Volume Delta (OHLCV proxy)"
    );
    assert!(row["score_components"]
        .as_array()
        .unwrap()
        .iter()
        .all(|item| {
            item["points"]
                .as_f64()
                .is_some_and(|points| (0.0..=10.0).contains(&points))
        }));
    assert!(row["confluence_score"].as_f64().is_some());
    let alerts =
        signal_alerts::record_scan_signal_alerts(&connection, standalone_signals.iter().cloned())
            .expect("the signal should use the existing alert lifecycle");
    assert!(
        alerts.iter().any(|alert| {
            alert["signal_strategy_id"] == quant_core::STOCKBIT_NWE_4H_SIGNAL_STRATEGY_ID
        }),
        "a fresh completed 4h NWE cross should enter the existing alert lifecycle"
    );

    drop(connection);
    remove_temporary_database(&path);
}

#[test]
fn stockbit_4h_lower_band_proximity_emits_a_long_with_a_fresh_15m_price() {
    let path = temporary_database_path("stockbit-nwe-4h-near");
    let connection = open_db_minimal(&path).expect("temporary storage should initialize");
    let envelope_bars = quant_core::stockbit_nwe_4h_minimum_history_bars();
    let mut closes = (0..envelope_bars)
        .map(|index| if index % 2 == 0 { 99.0 } else { 101.0 })
        .collect::<Vec<_>>();
    *closes.last_mut().expect("history includes a final candle") = 97.5;
    insert_stockbit_15m_history(&connection, &[97.5]);
    let latest_4h_open =
        aggregate_candles_from_base_source(&connection, STORAGE_SYMBOL, "4h", "15m", 1, SOURCE)
            .expect("the fresh 15m candle should aggregate")
            .last()
            .expect("the 15m candle should form a 4h bucket")
            .time;
    insert_stockbit_4h_history(&connection, latest_4h_open, &closes);

    let mut running = Vec::new();
    let mut potential = Vec::new();
    let mut standalone_signals = Vec::new();
    let has_fresh_15m_price = append_timeframe_signal_plans(
        TimeframeScanInput {
            app_connection: &connection,
            market_connection: &connection,
            sync_state: None,
            exchange_id: quant_core::STOCKBIT_NWE_4H_EXCHANGE_ID,
            product_id: PRODUCT,
            category_id: quant_core::STOCKBIT_NWE_4H_ASSET_CATEGORY_ID,
            symbol: STORAGE_SYMBOL,
            asset: "TEST",
            strategy: "idx_stockbit_day_trade",
            evidence_class: "idx",
            class: "IDX_STOCK",
            exchange: "STOCKBIT",
            time_window: "Stockbit 4h research",
            base_timeframe: quant_core::STOCKBIT_NWE_4H_TIMEFRAME,
            is_tokenized: false,
            is_multibagger: false,
            retained_stockbit_signal_rows: &std::collections::BTreeSet::new(),
        },
        &mut running,
        &mut potential,
        &mut HashMap::new(),
        &mut Vec::new(),
        &mut standalone_signals,
        &mut Vec::new(),
    )
    .expect("the bounded timeframe scan should complete");

    assert!(has_fresh_15m_price);
    assert!(running.is_empty() && potential.is_empty());
    let nwe_signal = standalone_signals
        .iter()
        .find(|signal| {
            signal["Signal_Strategy_ID"] == quant_core::STOCKBIT_NWE_4H_SIGNAL_STRATEGY_ID
        })
        .expect("the NWE strategy should retain its own screener row");
    let row = signal_alerts::signal_screener_row(nwe_signal)
        .expect("a fresh near-band score at the 50% floor should enter the screener");
    assert_eq!(row["direction"], "LONG");
    assert_eq!(
        row["score_components"][0]["state"],
        "NWE_LOWER_BAND_NEAR_LONG"
    );
    assert!(row["score"].as_f64().unwrap() >= quant_core::FOUR_INDICATOR_MINIMUM_SCORE_OUT_OF_40);
    assert!(row["score"].as_f64().unwrap() <= quant_core::FOUR_INDICATOR_SCORE_SCALE_MAX);
    assert_eq!(row["data_quality"], "RESEARCH_PROXY");

    drop(connection);
    remove_temporary_database(&path);
}
