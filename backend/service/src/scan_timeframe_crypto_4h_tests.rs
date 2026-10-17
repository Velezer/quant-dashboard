use super::*;
use crate::main_test_support::{remove_temporary_database, temporary_database_path};

const PRODUCT: &str = "pintu_pro_spot";
const STORAGE_SYMBOL: &str = "BTC-USD";
const SOURCE: &str = "pintu_pro_spot_binance_spot_proxy";
const FIFTEEN_MINUTE_NANOSECONDS: i64 = 15 * 60 * 1_000_000_000;
const HOUR_NANOSECONDS: i64 = 60 * 60 * 1_000_000_000;
const FOUR_HOUR_NANOSECONDS: i64 = 4 * HOUR_NANOSECONDS;

fn closes_with_a_final_dss_near_cross(count: usize) -> Vec<f64> {
    for phase_index in 0..1_000 {
        let phase = phase_index as f64 / 37.0;
        let closes = (0..count)
            .map(|index| {
                let index = index as f64;
                100.0
                    + 5.0 * (index * 0.13 + phase).sin()
                    + 1.5 * (index * 0.31 + phase / 3.0).sin()
            })
            .collect::<Vec<_>>();
        let bars = closes
            .iter()
            .enumerate()
            .map(|(index, close)| quant_core::StrategyBar {
                time: index as i64 * FOUR_HOUR_NANOSECONDS,
                open: *close,
                high: close + 1.0,
                low: close - 1.0,
                close: *close,
                volume: 1_000.0,
            })
            .collect::<Vec<_>>();
        let signals = quant_core::generate_stockbit_dss_bressert_4h_signals(&bars);
        if signals
            .last()
            .is_some_and(|signal| matches!(signal.direction.as_str(), "LONG" | "SHORT"))
        {
            return closes;
        }
    }
    panic!("fixture search should find a completed DSS near-cross");
}

fn insert_crypto_4h_history(
    connection: &rusqlite::Connection,
    latest_4h_open: i64,
    closes: &[f64],
) {
    let bar_count = i64::try_from(closes.len()).expect("fixture history fits in SQLite");
    let first_4h_open = latest_4h_open - (bar_count - 1) * FOUR_HOUR_NANOSECONDS;
    connection
        .execute(
            "INSERT INTO series(symbol,interval,source,exchange_id,product_id,data_provider,
                                timezone,price_basis,instrument_kind,last_success)
             VALUES(?1,'4h',?2,'pintu',?3,'binance','UTC','last_trade','spot',
                    strftime('%Y-%m-%d %H:%M:%S','now') || ' UTC')",
            params![STORAGE_SYMBOL, SOURCE, PRODUCT],
        )
        .expect("product-scoped 4h proxy series should exist");
    for (index, close) in closes.iter().copied().enumerate() {
        let time = first_4h_open + index as i64 * FOUR_HOUR_NANOSECONDS;
        connection
            .execute(
                "INSERT INTO bars(symbol,interval,source,ts,open,high,low,close,volume,ingested_at)
                 VALUES(?1,'4h',?2,?3,?4,?5,?6,?7,1000,'test')",
                params![
                    STORAGE_SYMBOL,
                    SOURCE,
                    time,
                    close,
                    close + 1.0,
                    close - 1.0,
                    close
                ],
            )
            .expect("completed crypto fixture candle should persist");
    }
}

fn insert_crypto_15m_history(
    connection: &rusqlite::Connection,
    latest_15m_open: i64,
    closes: &[f64],
) {
    let bar_count = i64::try_from(closes.len()).expect("fixture history fits in SQLite");
    let first_15m_open = latest_15m_open - (bar_count - 1) * FIFTEEN_MINUTE_NANOSECONDS;
    connection
        .execute(
            "INSERT OR IGNORE INTO series(symbol,interval,source,exchange_id,product_id,data_provider,
                                          timezone,price_basis,instrument_kind,last_success)
             VALUES(?1,'15m',?2,'pintu',?3,'binance','UTC','last_trade','spot',
                    strftime('%Y-%m-%d %H:%M:%S','now') || ' UTC')",
            params![STORAGE_SYMBOL, SOURCE, PRODUCT],
        )
        .expect("product-scoped 15m proxy series should exist");
    connection
        .execute(
            "DELETE FROM bars WHERE symbol=?1 AND interval='15m' AND source=?2",
            params![STORAGE_SYMBOL, SOURCE],
        )
        .expect("the 15m fixture should replace its own history");
    for (index, close) in closes.iter().copied().enumerate() {
        let time = first_15m_open + index as i64 * FIFTEEN_MINUTE_NANOSECONDS;
        connection
            .execute(
                "INSERT INTO bars(symbol,interval,source,ts,open,high,low,close,volume,ingested_at)
                 VALUES(?1,'15m',?2,?3,?4,?5,?6,?7,1000,'test')",
                params![
                    STORAGE_SYMBOL,
                    SOURCE,
                    time,
                    close,
                    close + 1.0,
                    close - 1.0,
                    close
                ],
            )
            .expect("completed crypto 15m fixture candle should persist");
    }
}

#[test]
fn crypto_15m_nwe_and_dss_emit_separate_timeframe_specific_signals() {
    let path = temporary_database_path("crypto-indicators-15m");
    let connection = open_db_minimal(&path).expect("temporary storage should initialize");
    let now_ns = i64::try_from(now_secs()).unwrap_or(i64::MAX) * 1_000_000_000;
    let latest_15m_open = now_ns - FIFTEEN_MINUTE_NANOSECONDS - 60_000_000_000;
    let required_nwe_bars = quant_core::stockbit_nwe_4h_minimum_history_bars();
    let mut nwe_closes = vec![100.0; required_nwe_bars];
    *nwe_closes
        .last_mut()
        .expect("NWE history includes a final bar") = 120.0;
    insert_crypto_15m_history(&connection, latest_15m_open, &nwe_closes);

    let mut running = Vec::new();
    let mut potential = Vec::new();
    let mut standalone_signals = Vec::new();
    let has_fresh_15m_price = append_timeframe_signal_plans(
        TimeframeScanInput {
            app_connection: &connection,
            market_connection: &connection,
            sync_state: None,
            exchange_id: "pintu",
            product_id: PRODUCT,
            category_id: "crypto_spot",
            symbol: STORAGE_SYMBOL,
            asset: "BTC",
            strategy: "tokocrypto_spot_confluence_2_of_4_v1",
            evidence_class: "crypto_spot",
            class: "CRYPTO_SPOT",
            exchange: "PINTU",
            time_window: "Pintu 15m crypto research",
            base_timeframe: "15m",
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
    .expect("fresh 15m NWE scan should complete");
    assert!(has_fresh_15m_price);

    let nwe_signal = standalone_signals
        .iter()
        .find(|signal| signal["Signal_Strategy_ID"] == "crypto_nwe_band_proximity_15m_v1")
        .expect("the Pintu 15m NWE setup should get its own signal identity");
    let nwe_row = signal_alerts::signal_screener_row(nwe_signal)
        .expect("the fresh 15m NWE signal should be renderable");
    assert_eq!(nwe_row["timeframe"], "15m");
    assert_eq!(nwe_row["price_timeframe"], "15m");
    assert!(nwe_row["score"].as_f64().unwrap() >= 1.0);
    assert!(nwe_row["score"].as_f64().unwrap() <= 20.0);
    assert_eq!(nwe_row["data_source"], SOURCE);
    assert_eq!(nwe_row["data_quality"], "RESEARCH_PROXY");

    let dss_closes = closes_with_a_final_dss_near_cross(required_nwe_bars);
    insert_crypto_15m_history(&connection, latest_15m_open, &dss_closes);
    append_timeframe_signal_plans(
        TimeframeScanInput {
            app_connection: &connection,
            market_connection: &connection,
            sync_state: None,
            exchange_id: "pintu",
            product_id: PRODUCT,
            category_id: "crypto_spot",
            symbol: STORAGE_SYMBOL,
            asset: "BTC",
            strategy: "tokocrypto_spot_confluence_2_of_4_v1",
            evidence_class: "crypto_spot",
            class: "CRYPTO_SPOT",
            exchange: "PINTU",
            time_window: "Pintu 15m crypto research",
            base_timeframe: "15m",
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
    .expect("fresh 15m DSS scan should complete");

    let dss_signal = standalone_signals
        .iter()
        .find(|signal| signal["Signal_Strategy_ID"] == "crypto_dss_bressert_near_cross_15m_v1")
        .expect("the Pintu 15m DSS setup should get its own signal identity");
    let dss_row = signal_alerts::signal_screener_row(dss_signal)
        .expect("the fresh 15m DSS signal should be renderable");
    assert_eq!(dss_row["timeframe"], "15m");
    assert_eq!(dss_row["price_timeframe"], "15m");
    assert!(dss_row["score"].as_f64().unwrap() >= 1.0);
    assert!(dss_row["score"].as_f64().unwrap() <= 20.0);
    assert_eq!(dss_row["data_source"], SOURCE);
    assert_eq!(dss_row["data_quality"], "RESEARCH_PROXY");

    drop(connection);
    remove_temporary_database(&path);
}

#[test]
fn crypto_4h_dss_near_cross_uses_the_shared_rank_and_its_own_completed_price() {
    let path = temporary_database_path("crypto-dss-4h");
    let connection = open_db_minimal(&path).expect("temporary storage should initialize");
    let now_ns = i64::try_from(now_secs()).unwrap_or(i64::MAX) * 1_000_000_000;
    let latest_4h_open = now_ns - 5 * HOUR_NANOSECONDS;
    let closes =
        closes_with_a_final_dss_near_cross(quant_core::DSS_BRESSERT_MINIMUM_HISTORY_BARS + 20);
    let latest_15m_open = now_ns.div_euclid(FIFTEEN_MINUTE_NANOSECONDS)
        * FIFTEEN_MINUTE_NANOSECONDS
        - FIFTEEN_MINUTE_NANOSECONDS;
    insert_crypto_15m_history(&connection, latest_15m_open, &[closes[closes.len() - 1]]);
    insert_crypto_4h_history(&connection, latest_4h_open, &closes);

    let mut running = Vec::new();
    let mut potential = Vec::new();
    let mut standalone_signals = Vec::new();
    let has_fresh_15m_price = append_timeframe_signal_plans(
        TimeframeScanInput {
            app_connection: &connection,
            market_connection: &connection,
            sync_state: None,
            exchange_id: "pintu",
            product_id: PRODUCT,
            category_id: "crypto_spot",
            symbol: STORAGE_SYMBOL,
            asset: "BTC",
            strategy: "tokocrypto_spot_confluence_2_of_4_v1",
            evidence_class: "crypto_spot",
            class: "CRYPTO_SPOT",
            exchange: "PINTU",
            time_window: "Pintu 4h crypto research",
            base_timeframe: "15m",
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
    .expect("the bounded crypto timeframe scan should complete");

    assert!(has_fresh_15m_price);
    assert!(running.is_empty() && potential.is_empty());
    let candidate = standalone_signals
        .iter()
        .find(|signal| {
            signal["Signal_Strategy_ID"] == quant_core::CRYPTO_DSS_BRESSERT_4H_SIGNAL_STRATEGY_ID
        })
        .expect("the crypto 4h DSS model should emit its current near-cross");
    let row = signal_alerts::signal_screener_row(candidate)
        .expect("the completed, unexpired 4h crypto signal should enter the screener");
    assert_eq!(row["exchange_id"], "pintu");
    assert_eq!(row["product_id"], PRODUCT);
    assert_eq!(row["asset_category_id"], "crypto_spot");
    assert_eq!(row["timeframe"], "4h");
    assert_eq!(row["price_timeframe"], "15m");
    assert!(matches!(row["direction"].as_str(), Some("LONG" | "SHORT")));
    assert_eq!(
        row["signal_strategy_id"],
        quant_core::CRYPTO_DSS_BRESSERT_4H_SIGNAL_STRATEGY_ID
    );
    assert_eq!(
        row["score_model_id"],
        quant_core::STOCKBIT_DSS_BRESSERT_4H_SIGNAL_SCORE_MODEL_ID
    );
    assert_eq!(row["data_source"], SOURCE);
    assert_eq!(row["data_provider"], "binance");
    assert_eq!(row["data_quality"], "RESEARCH_PROXY");
    assert_eq!(row["evidence_status"], "VALIDATION_PENDING");
    assert_eq!(
        row["score_components"][0]["indicator"],
        "Nadaraya-Watson Envelope"
    );
    assert_eq!(row["score_components"][1]["indicator"], "DSS Bressert");

    drop(connection);
    remove_temporary_database(&path);
}
