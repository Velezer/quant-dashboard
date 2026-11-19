use super::*;
use crate::main_test_support::{remove_temporary_database, temporary_database_path};
use std::collections::BTreeSet;

fn seed_history(
    input: &TimeframeScanInput<'_>,
    timeframe: &str,
    source: &str,
    latest_open: i64,
    count: usize,
) {
    let connection = input.market_connection;
    let duration = timeframe_duration_ns(timeframe).unwrap();
    connection
        .execute(
            "INSERT INTO series(symbol,interval,source,exchange_id,product_id,data_provider,
                            timezone,price_basis,instrument_kind,last_success)
         VALUES(?1,?2,?3,?4,?5,'binance','UTC','last_trade',?6,
                strftime('%Y-%m-%d %H:%M:%S','now') || ' UTC')",
            params![
                input.symbol,
                timeframe,
                source,
                input.exchange_id,
                input.product_id,
                input.category_id
            ],
        )
        .unwrap();
    for index in 0..count {
        let time = latest_open - (count - 1 - index) as i64 * duration;
        let (close, volume) = if index + 1 == count {
            (99.0, 300.0)
        } else {
            (100.0, 100.0)
        };
        connection
            .execute(
                "INSERT INTO bars(symbol,interval,source,ts,open,high,low,close,volume,ingested_at)
             VALUES(?1,?2,?3,?4,100,101,99,?5,?6,'test')",
                params![input.symbol, timeframe, source, time, close, volume],
            )
            .unwrap();
    }
}

#[test]
fn combined_scores_scan_and_publish_one_row_per_app_product_timeframe() {
    for is_stockbit in [false, true] {
        let path = temporary_database_path("combined-score-scan");
        let connection = open_db_minimal(&path).unwrap();
        let (exchange, product, category, symbol, asset, source) = if is_stockbit {
            (
                "stockbit",
                "stockbit_regular",
                "idx_stock",
                "COMBO.JK",
                "COMBO",
                "stockbit_regular_yfinance_fallback",
            )
        } else {
            (
                "pintu",
                "pintu_pro_spot",
                "crypto_spot",
                "COMBO-USD",
                "COMBO",
                "pintu_pro_spot_binance_spot_proxy",
            )
        };
        let retained = BTreeSet::new();
        let input = TimeframeScanInput {
            app_connection: &connection,
            market_connection: &connection,
            sync_state: None,
            exchange_id: exchange,
            product_id: product,
            category_id: category,
            quote_currency: "USDT",
            symbol,
            asset,
            strategy: "",
            evidence_class: "",
            class: "",
            exchange,
            time_window: "",
            base_timeframe: "4h",
            is_tokenized: false,
            is_multibagger: false,
            retained_stockbit_signal_rows: &retained,
            active_scoring_profiles: None,
        };
        let minute_duration = timeframe_duration_ns("15m").unwrap();
        let latest_15m =
            current_time_ns().div_euclid(minute_duration) * minute_duration - minute_duration;
        seed_history(
            &input,
            "15m",
            source,
            latest_15m,
            quant_core::ENTRY_SCORE_MINIMUM_HISTORY_BARS,
        );
        let latest_4h =
            aggregate_candles_from_base_source(&connection, symbol, "4h", "15m", 1, source)
                .unwrap()
                .last()
                .unwrap()
                .time;
        seed_history(
            &input,
            "4h",
            source,
            latest_4h,
            quant_core::ENTRY_SCORE_MINIMUM_HISTORY_BARS + 1,
        );

        let mut candidates = Vec::new();
        let has_fresh_price = append_timeframe_signal_plans(
            input,
            &mut Vec::new(),
            &mut Vec::new(),
            &mut HashMap::new(),
            &mut Vec::new(),
            &mut candidates,
            &mut Vec::new(),
        )
        .unwrap();
        assert!(has_fresh_price);

        for timeframe in ["15m", "4h"] {
            let strategy =
                crate::master_data::combined_entry_strategy_id(timeframe, is_stockbit).unwrap();
            let scoped: Vec<_> = candidates
                .iter()
                .filter(|candidate| candidate["timeframe"] == timeframe)
                .collect();
            assert_eq!(scoped.len(), 1, "{product}/{timeframe}");
            let row =
                signal_alerts::signal_screener_row(scoped[0]).expect("combined row should publish");
            assert_eq!(row["exchange_id"], exchange);
            assert_eq!(row["product_id"], product);
            assert_eq!(row["asset_category_id"], category);
            assert_eq!(row["signal_strategy_id"], strategy);
            assert!(row["signal_strategy_name"]
                .as_str()
                .unwrap()
                .contains("Combined Score"));
            assert_eq!(row["score_model_id"], quant_core::ENTRY_SCORE_MODEL_ID);
            assert_eq!(row["score_components"].as_array().unwrap().len(), 6);
            assert_eq!(row["price_timeframe"], "15m");
            assert_eq!(row["price_bar_time"], latest_15m);
            assert_eq!(row["current_price"], 99.0);
            assert_eq!(row["data_source"], source);
            assert_eq!(row["data_quality"], "RESEARCH_PROXY");
            let forming = timeframe == "4h"
                && latest_4h.saturating_add(timeframe_duration_ns("4h").unwrap())
                    > current_time_ns();
            assert_eq!(
                row["score_bar_is_forming"], forming,
                "{product}/{timeframe}"
            );
        }

        let alerts =
            signal_alerts::record_scan_signal_alerts(&connection, candidates.iter()).unwrap();
        assert_eq!(
            alerts
                .iter()
                .filter(|alert| alert["timeframe"] == "4h")
                .count(),
            1
        );
        drop(connection);
        remove_temporary_database(&path);
    }
}
