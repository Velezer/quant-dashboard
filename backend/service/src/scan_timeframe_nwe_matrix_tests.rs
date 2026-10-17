use super::*;
use crate::main_test_support::{remove_temporary_database, temporary_database_path};

const PRODUCT: &str = "stockbit_regular";
const SYMBOL: &str = "MTF-TEST.JK";
const SOURCE: &str = "stockbit_regular_yfinance_fallback";
const NANOS_PER_SECOND: i64 = 1_000_000_000;
const FIFTEEN_MINUTES_NANOSECONDS: i64 = 15 * 60 * NANOS_PER_SECOND;

fn insert_timeframe_history(
    connection: &Connection,
    timeframe: &str,
    latest_open: i64,
    closes: &[f64],
) {
    let duration_ns = timeframe_duration_ns(timeframe).expect("supported timeframe duration");
    let first_open = latest_open - (closes.len() as i64 - 1) * duration_ns;

    connection
        .execute(
            "INSERT INTO series(symbol,interval,source,exchange_id,product_id,data_provider,
                                timezone,price_basis,instrument_kind,last_success)
             VALUES(?1,?2,?3,'stockbit',?4,'yfinance','Asia/Jakarta','last_trade',
                    'equity_proxy',strftime('%Y-%m-%d %H:%M:%S','now') || ' UTC')",
            params![SYMBOL, timeframe, SOURCE, PRODUCT],
        )
        .expect("timeframe research series should exist");
    for (index, close) in closes.iter().copied().enumerate() {
        let time = first_open + index as i64 * duration_ns;
        connection
            .execute(
                "INSERT INTO bars(symbol,interval,source,ts,open,high,low,close,volume,ingested_at)
                 VALUES(?1,?2,?3,?4,100,?5,?6,?7,1000,'test')",
                params![
                    SYMBOL,
                    timeframe,
                    SOURCE,
                    time,
                    close.max(100.0) + 1.0,
                    close.min(100.0) - 1.0,
                    close,
                ],
            )
            .expect("timeframe candles should persist");
    }
}

#[test]
fn stockbit_daily_and_weekly_strategy_ids_are_registered() {
    for timeframe in ["1d", "1w"] {
        assert!(quant_core::DSS_BRESSERT_MTF_TIMEFRAMES
            .iter()
            .any(|definition| definition.timeframe == timeframe));
        assert!(quant_core::stockbit_nwe_signal_strategy_id(timeframe).is_some());
        assert!(quant_core::stockbit_dss_bressert_signal_strategy_id(timeframe).is_some());
    }
}

#[test]
fn stockbit_nwe_can_emit_separate_qualifying_rows_for_every_supported_timeframe() {
    for definition in quant_core::DSS_BRESSERT_MTF_TIMEFRAMES {
        let timeframe = definition.timeframe;
        let now_ns = i64::try_from(now_secs()).unwrap_or(i64::MAX) * NANOS_PER_SECOND;
        let latest_15m_open = now_ns.div_euclid(FIFTEEN_MINUTES_NANOSECONDS)
            * FIFTEEN_MINUTES_NANOSECONDS
            - FIFTEEN_MINUTES_NANOSECONDS;
        let path = temporary_database_path(&format!("stockbit-nwe-matrix-{timeframe}"));
        let connection = open_db_minimal(&path).expect("temporary storage should initialize");
        let mut closes = vec![100.0; quant_core::stockbit_nwe_4h_minimum_history_bars()];
        *closes
            .last_mut()
            .expect("the setup requires a latest candle") = 120.0;
        if timeframe != "15m" {
            insert_timeframe_history(&connection, "15m", latest_15m_open, &[120.0]);
        }
        let latest_open = if timeframe == "15m" {
            latest_15m_open
        } else {
            aggregate_candles_from_base_source(&connection, SYMBOL, timeframe, "15m", 1, SOURCE)
                .expect("the completed 15m candle should aggregate")
                .last()
                .expect("the 15m candle should form a timeframe bucket")
                .time
        };
        insert_timeframe_history(&connection, timeframe, latest_open, &closes);

        let mut candidates = Vec::new();
        append_timeframe_signal_plans(
            TimeframeScanInput {
                app_connection: &connection,
                market_connection: &connection,
                sync_state: None,
                exchange_id: quant_core::STOCKBIT_NWE_4H_EXCHANGE_ID,
                product_id: PRODUCT,
                category_id: quant_core::STOCKBIT_NWE_4H_ASSET_CATEGORY_ID,
                symbol: SYMBOL,
                asset: "MTF-TEST",
                strategy: "idx_stockbit_day_trade",
                evidence_class: "idx",
                class: "IDX_STOCK",
                exchange: "STOCKBIT",
                time_window: "Stockbit NWE timeframe matrix",
                base_timeframe: timeframe,
                is_tokenized: false,
                is_multibagger: false,
                retained_stockbit_signal_rows: &std::collections::BTreeSet::new(),
            },
            &mut Vec::new(),
            &mut Vec::new(),
            &mut HashMap::new(),
            &mut Vec::new(),
            &mut candidates,
            &mut Vec::new(),
        )
        .expect("the bounded timeframe scan should complete");

        let strategy_id = quant_core::stockbit_nwe_signal_strategy_id(timeframe)
            .expect("each registered timeframe should have an NWE strategy");
        let candidate = candidates
            .iter()
            .find(|candidate| {
                candidate["Signal_Strategy_ID"] == strategy_id
                    && candidate["timeframe"] == timeframe
            })
            .unwrap_or_else(|| panic!("NWE should emit a separate {timeframe} row"));
        let row = signal_alerts::signal_screener_row(candidate)
            .unwrap_or_else(|| panic!("the {timeframe} NWE row should pass screener validation"));

        assert_eq!(row["timeframe"], timeframe);
        assert_eq!(row["signal_strategy_id"], strategy_id);
        assert_eq!(
            row["score_scale_max"],
            quant_core::FOUR_INDICATOR_SCORE_SCALE_MAX
        );
        assert_eq!(
            row["minimum_score_out_of_40"],
            quant_core::FOUR_INDICATOR_MINIMUM_SCORE_OUT_OF_40
        );
        assert!(
            row["score"].as_f64().unwrap() >= quant_core::FOUR_INDICATOR_MINIMUM_SCORE_OUT_OF_40
        );
        assert!(row["score_components"].as_array().unwrap().len() >= 1);
        drop(connection);
        remove_temporary_database(&path);
    }
}
