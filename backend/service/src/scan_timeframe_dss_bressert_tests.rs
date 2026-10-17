use super::*;
use crate::main_test_support::{remove_temporary_database, temporary_database_path};

const PRODUCT: &str = "stockbit_regular";
const STORAGE_SYMBOL: &str = "DSS-TEST.JK";
const SOURCE: &str = "stockbit_regular_yfinance_fallback";
const FIFTEEN_MINUTES_NANOSECONDS: i64 = 15 * 60 * 1_000_000_000;
const HOUR_NANOSECONDS: i64 = 60 * 60 * 1_000_000_000;
const FOUR_HOUR_NANOSECONDS: i64 = 4 * HOUR_NANOSECONDS;

fn closes_with_a_final_dss_near_cross(count: usize, bar_duration_ns: i64) -> Vec<f64> {
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
                time: index as i64 * bar_duration_ns,
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

#[test]
fn stockbit_dss_can_emit_separate_qualifying_rows_for_every_supported_timeframe() {
    for definition in quant_core::DSS_BRESSERT_MTF_TIMEFRAMES {
        let timeframe = definition.timeframe;
        let duration_ns = timeframe_duration_ns(timeframe)
            .unwrap_or_else(|| panic!("{timeframe} should have a configured duration"));
        let path = temporary_database_path(&format!("stockbit-dss-matrix-{timeframe}"));
        let connection = open_db_minimal(&path).expect("temporary storage should initialize");
        let now_ns = i64::try_from(now_secs()).unwrap_or(i64::MAX) * 1_000_000_000;
        let latest_15m_open = now_ns.div_euclid(FIFTEEN_MINUTES_NANOSECONDS)
            * FIFTEEN_MINUTES_NANOSECONDS
            - FIFTEEN_MINUTES_NANOSECONDS;
        let closes = closes_with_a_final_dss_near_cross(
            quant_core::DSS_BRESSERT_MINIMUM_HISTORY_BARS + 20,
            duration_ns,
        );
        if timeframe != "15m" {
            insert_stockbit_history(
                &connection,
                "15m",
                FIFTEEN_MINUTES_NANOSECONDS,
                latest_15m_open,
                &[closes[closes.len() - 1]],
            );
        }
        let latest_open = if timeframe == "15m" {
            latest_15m_open
        } else {
            aggregate_candles_from_base_source(
                &connection,
                STORAGE_SYMBOL,
                timeframe,
                "15m",
                1,
                SOURCE,
            )
            .expect("the completed 15m candle should aggregate")
            .last()
            .expect("the 15m candle should form a timeframe bucket")
            .time
        };
        insert_stockbit_history(&connection, timeframe, duration_ns, latest_open, &closes);

        let mut standalone_signals = Vec::new();
        append_timeframe_signal_plans(
            TimeframeScanInput {
                app_connection: &connection,
                market_connection: &connection,
                sync_state: None,
                exchange_id: quant_core::STOCKBIT_NWE_4H_EXCHANGE_ID,
                product_id: PRODUCT,
                category_id: quant_core::STOCKBIT_NWE_4H_ASSET_CATEGORY_ID,
                symbol: STORAGE_SYMBOL,
                asset: "DSS-TEST",
                strategy: "idx_stockbit_day_trade",
                evidence_class: "idx",
                class: "IDX_STOCK",
                exchange: "STOCKBIT",
                time_window: "Stockbit DSS timeframe matrix",
                base_timeframe: timeframe,
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
        .unwrap_or_else(|error| panic!("{timeframe} scan should complete: {error}"));

        let strategy_id = quant_core::stockbit_dss_bressert_signal_strategy_id(timeframe)
            .unwrap_or_else(|| panic!("{timeframe} should have a DSS strategy"));
        let candidate = standalone_signals
            .iter()
            .find(|candidate| {
                candidate["Signal_Strategy_ID"] == strategy_id
                    && candidate["timeframe"] == timeframe
            })
            .unwrap_or_else(|| panic!("DSS should emit a separate {timeframe} row"));
        let row = signal_alerts::signal_screener_row(candidate)
            .unwrap_or_else(|| panic!("the {timeframe} DSS row should be renderable"));

        assert_eq!(row["timeframe"], timeframe);
        assert_eq!(row["signal_strategy_id"], strategy_id);
        assert_eq!(
            row["score_model_id"],
            quant_core::STOCKBIT_DSS_BRESSERT_4H_SIGNAL_SCORE_MODEL_ID
        );
        assert_eq!(
            row["minimum_score_out_of_40"],
            quant_core::FOUR_INDICATOR_MINIMUM_SCORE_OUT_OF_40
        );
        assert!(
            row["score"].as_f64().unwrap() >= quant_core::FOUR_INDICATOR_MINIMUM_SCORE_OUT_OF_40
        );
        assert!(row["score"].as_f64().unwrap() <= 20.0);
        drop(connection);
        remove_temporary_database(&path);
    }
}

fn insert_stockbit_history(
    connection: &rusqlite::Connection,
    timeframe: &str,
    timeframe_duration_ns: i64,
    latest_bar_open: i64,
    closes: &[f64],
) {
    let source_bars = i64::try_from(closes.len()).expect("fixture history fits in SQLite");
    let first_bar_open = latest_bar_open - (source_bars - 1) * timeframe_duration_ns;
    connection
        .execute(
            "INSERT INTO series(symbol,interval,source,exchange_id,product_id,data_provider,
                                timezone,price_basis,instrument_kind,last_success)
             VALUES(?1,?2,?3,'stockbit',?4,'yfinance','Asia/Jakarta','last_trade',
                    'equity_proxy',strftime('%Y-%m-%d %H:%M:%S','now') || ' UTC')",
            params![STORAGE_SYMBOL, timeframe, SOURCE, PRODUCT],
        )
        .expect("product-scoped research series should exist");
    for (index, close) in closes.iter().copied().enumerate() {
        let time = first_bar_open + index as i64 * timeframe_duration_ns;
        connection
            .execute(
                "INSERT INTO bars(symbol,interval,source,ts,open,high,low,close,volume,ingested_at)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,1000,'test')",
                params![
                    STORAGE_SYMBOL,
                    timeframe,
                    SOURCE,
                    time,
                    close,
                    close + 1.0,
                    close - 1.0,
                    close
                ],
            )
            .expect("fixture candles should persist");
    }
}

#[test]
fn stockbit_15m_dss_near_cross_emits_a_timeframe_specific_signal_from_fresh_bars() {
    let path = temporary_database_path("stockbit-dss-near-cross-15m");
    let connection = open_db_minimal(&path).expect("temporary storage should initialize");
    let now_ns = i64::try_from(now_secs()).unwrap_or(i64::MAX) * 1_000_000_000;
    let latest_15m_open = now_ns - FIFTEEN_MINUTES_NANOSECONDS - 60_000_000_000;
    let closes = closes_with_a_final_dss_near_cross(
        quant_core::stockbit_nwe_4h_minimum_history_bars(),
        FIFTEEN_MINUTES_NANOSECONDS,
    );
    insert_stockbit_history(
        &connection,
        "15m",
        FIFTEEN_MINUTES_NANOSECONDS,
        latest_15m_open,
        &closes,
    );

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
            asset: "DSS-TEST",
            strategy: "idx_stockbit_day_trade",
            evidence_class: "idx",
            class: "IDX_STOCK",
            exchange: "STOCKBIT",
            time_window: "Stockbit DSS 15m research",
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
        .find(|signal| signal["Signal_Strategy_ID"] == "stockbit_dss_bressert_near_cross_15m_v1")
        .expect("a qualifying 15m DSS setup should emit its own timeframe signal");
    let row = signal_alerts::signal_screener_row(signal)
        .expect("the qualifying 15m DSS signal should be renderable");
    assert_eq!(row["timeframe"], "15m");
    assert!(row["signal_strategy_id"].as_str().unwrap().contains("15m"));
    assert!(matches!(row["direction"].as_str(), Some("LONG" | "SHORT")));
    assert_eq!(
        row["score_model_id"],
        quant_core::STOCKBIT_DSS_BRESSERT_4H_SIGNAL_SCORE_MODEL_ID
    );
    assert!(row["score"].as_f64().unwrap() >= 1.0);
    assert_eq!(
        row["minimum_score_out_of_40"],
        quant_core::FOUR_INDICATOR_MINIMUM_SCORE_OUT_OF_40
    );
    assert!(row["score"].as_f64().unwrap() <= 20.0);
    assert_eq!(row["data_quality"], "RESEARCH_PROXY");

    drop(connection);
    remove_temporary_database(&path);
}

#[test]
fn stockbit_15m_dss_runs_with_its_own_history_when_nwe_history_is_short() {
    let path = temporary_database_path("stockbit-dss-15m-short-nwe");
    let connection = open_db_minimal(&path).expect("temporary storage should initialize");
    let now_ns = i64::try_from(now_secs()).unwrap_or(i64::MAX) * 1_000_000_000;
    let latest_15m_open = now_ns - FIFTEEN_MINUTES_NANOSECONDS - 60_000_000_000;
    let closes = closes_with_a_final_dss_near_cross(
        quant_core::DSS_BRESSERT_MINIMUM_HISTORY_BARS,
        FIFTEEN_MINUTES_NANOSECONDS,
    );
    insert_stockbit_history(
        &connection,
        "15m",
        FIFTEEN_MINUTES_NANOSECONDS,
        latest_15m_open,
        &closes,
    );

    let mut standalone_signals = Vec::new();
    append_timeframe_signal_plans(
        TimeframeScanInput {
            app_connection: &connection,
            market_connection: &connection,
            sync_state: None,
            exchange_id: quant_core::STOCKBIT_NWE_4H_EXCHANGE_ID,
            product_id: PRODUCT,
            category_id: quant_core::STOCKBIT_NWE_4H_ASSET_CATEGORY_ID,
            symbol: STORAGE_SYMBOL,
            asset: "DSS-TEST",
            strategy: "idx_stockbit_day_trade",
            evidence_class: "idx",
            class: "IDX_STOCK",
            exchange: "STOCKBIT",
            time_window: "Stockbit DSS 15m research",
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
    .expect("DSS should run without the longer NWE history");

    let dss_strategy_id = quant_core::stockbit_dss_bressert_signal_strategy_id("15m").unwrap();
    assert!(standalone_signals
        .iter()
        .any(|signal| signal["Signal_Strategy_ID"] == dss_strategy_id));
    assert!(!standalone_signals
        .iter()
        .any(|signal| { signal["Signal_Strategy_ID"] == "stockbit_nwe_band_proximity_15m_v1" }));

    drop(connection);
    remove_temporary_database(&path);
}

#[test]
fn stockbit_4h_dss_near_cross_emits_a_ranked_research_signal_with_a_fresh_15m_price() {
    let path = temporary_database_path("stockbit-dss-near-cross-4h");
    let connection = open_db_minimal(&path).expect("temporary storage should initialize");
    let now_ns = i64::try_from(now_secs()).unwrap_or(i64::MAX) * 1_000_000_000;
    let closes = closes_with_a_final_dss_near_cross(
        quant_core::DSS_BRESSERT_MINIMUM_HISTORY_BARS + 20,
        FOUR_HOUR_NANOSECONDS,
    );
    let latest_15m_open = now_ns.div_euclid(FIFTEEN_MINUTES_NANOSECONDS)
        * FIFTEEN_MINUTES_NANOSECONDS
        - FIFTEEN_MINUTES_NANOSECONDS;
    insert_stockbit_history(
        &connection,
        "15m",
        FIFTEEN_MINUTES_NANOSECONDS,
        latest_15m_open,
        &[closes[closes.len() - 1]],
    );
    let latest_4h_open =
        aggregate_candles_from_base_source(&connection, STORAGE_SYMBOL, "4h", "15m", 1, SOURCE)
            .expect("the fresh 15m candle should aggregate")
            .last()
            .expect("the 15m candle should form a 4h bucket")
            .time;
    insert_stockbit_history(
        &connection,
        "4h",
        FOUR_HOUR_NANOSECONDS,
        latest_4h_open,
        &closes,
    );

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
            asset: "DSS-TEST",
            strategy: "idx_stockbit_day_trade",
            evidence_class: "idx",
            class: "IDX_STOCK",
            exchange: "STOCKBIT",
            time_window: "Stockbit DSS 4h research",
            base_timeframe: quant_core::STOCKBIT_NWE_4H_TIMEFRAME,
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
    assert_eq!(standalone_signals.len(), 1);
    let row = signal_alerts::signal_screener_row(&standalone_signals[0])
        .expect("the completed DSS near-cross should enter the screener");
    assert_eq!(row["exchange_id"], "stockbit");
    assert_eq!(row["product_id"], PRODUCT);
    assert_eq!(row["asset_category_id"], "idx_stock");
    assert_eq!(
        row["signal_strategy_id"],
        quant_core::STOCKBIT_DSS_BRESSERT_4H_SIGNAL_STRATEGY_ID
    );
    assert_eq!(row["timeframe"], "4h");
    assert!(row["signal_strategy_id"].as_str().unwrap().contains("4h"));
    assert!(matches!(row["direction"].as_str(), Some("LONG" | "SHORT")));
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
        row["score_model_id"],
        quant_core::STOCKBIT_DSS_BRESSERT_4H_SIGNAL_SCORE_MODEL_ID
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
                .is_some_and(|points| (0.0..=5.0).contains(&points))
        }));

    drop(connection);
    remove_temporary_database(&path);
}

#[test]
fn stockbit_4h_dss_does_not_update_entry_score_when_15m_is_stale() {
    let path = temporary_database_path("stockbit-dss-4h-fresh-while-15m-stale");
    let connection = open_db_minimal(&path).expect("temporary storage should initialize");
    let now_ns = i64::try_from(now_secs()).unwrap_or(i64::MAX) * 1_000_000_000;
    let latest_4h_open = now_ns - 5 * HOUR_NANOSECONDS;
    let closes = closes_with_a_final_dss_near_cross(
        quant_core::DSS_BRESSERT_MINIMUM_HISTORY_BARS + 20,
        FOUR_HOUR_NANOSECONDS,
    );
    insert_stockbit_history(
        &connection,
        "4h",
        FOUR_HOUR_NANOSECONDS,
        latest_4h_open,
        &closes,
    );
    let stale_15m_open = now_ns - 6 * HOUR_NANOSECONDS;
    insert_stockbit_history(
        &connection,
        "15m",
        FIFTEEN_MINUTES_NANOSECONDS,
        stale_15m_open,
        &closes[..quant_core::DSS_BRESSERT_MINIMUM_HISTORY_BARS],
    );

    let strategy_id = quant_core::STOCKBIT_DSS_BRESSERT_4H_SIGNAL_STRATEGY_ID.to_string();
    let retained = std::collections::BTreeSet::from([(strategy_id, "4h".to_string())]);
    let mut score_updates = Vec::new();
    let fresh_15m_price = append_timeframe_signal_plans(
        TimeframeScanInput {
            app_connection: &connection,
            market_connection: &connection,
            sync_state: None,
            exchange_id: quant_core::STOCKBIT_NWE_4H_EXCHANGE_ID,
            product_id: PRODUCT,
            category_id: quant_core::STOCKBIT_NWE_4H_ASSET_CATEGORY_ID,
            symbol: STORAGE_SYMBOL,
            asset: "DSS-TEST",
            strategy: "idx_stockbit_day_trade",
            evidence_class: "idx",
            class: "IDX_STOCK",
            exchange: "STOCKBIT",
            time_window: "Stockbit 4h stale intraday recovery",
            base_timeframe: "4h",
            is_tokenized: false,
            is_multibagger: false,
            retained_stockbit_signal_rows: &retained,
        },
        &mut Vec::new(),
        &mut Vec::new(),
        &mut HashMap::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut score_updates,
    )
    .expect("the timeframe scan should complete");

    assert!(!fresh_15m_price);
    assert!(score_updates.is_empty());

    drop(connection);
    remove_temporary_database(&path);
}
