use super::*;

fn bars_with_optional_final_decline(last_bar_time: i64, decline: bool) -> Vec<StrategyBar> {
    let duration = timeframe_duration_ns("4h").unwrap();
    let count = quant_core::ENTRY_SCORE_MINIMUM_HISTORY_BARS;
    let mut bars: Vec<_> = (0..count)
        .map(|index| StrategyBar {
            time: last_bar_time - (count - 1 - index) as i64 * duration,
            open: 100.0,
            high: 101.0,
            low: 99.0,
            close: 100.0,
            volume: 100.0,
        })
        .collect();
    if decline {
        let last = bars.last_mut().unwrap();
        last.close = 99.0;
        last.volume = 300.0;
    }
    bars
}

fn scan_input<'a>(
    connection: &'a Connection,
    is_stockbit: bool,
    retained: &'a std::collections::BTreeSet<(String, String)>,
) -> TimeframeScanInput<'a> {
    let (exchange, product, category, symbol, asset) = if is_stockbit {
        (
            "stockbit",
            "stockbit_regular",
            "idx_stock",
            "TEST.JK",
            "TEST.JK",
        )
    } else {
        (
            "pintu",
            "pintu_pro_futures",
            "crypto_perpetual",
            "BTC-USD",
            "BTC",
        )
    };
    TimeframeScanInput {
        app_connection: connection,
        market_connection: connection,
        sync_state: None,
        exchange_id: exchange,
        product_id: product,
        category_id: category,
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
        retained_stockbit_signal_rows: retained,
        active_scoring_profiles: None,
    }
}

#[test]
fn combined_score_publishes_one_signal_and_uses_signed_total_without_warmed_nwe() {
    let conn = Connection::open_in_memory().unwrap();
    let retained = Default::default();
    let input = scan_input(&conn, false, &retained);
    let timeframe = "4h";
    let last_bar_time = current_time_ns() - timeframe_duration_ns(timeframe).unwrap();
    let bars = bars_with_optional_final_decline(last_bar_time, true);
    let source = ("crypto_proxy".to_string(), "provider".to_string());
    let mut candidates = Vec::new();
    append_combined_entry(
        &input,
        false,
        timeframe,
        Some(&source),
        &bars,
        None,
        &mut candidates,
        &mut Vec::new(),
    );
    assert_eq!(candidates.len(), 1);
    let row = &candidates[0];
    let score =
        quant_core::combined_entry_score(&bars, quant_core::FOUR_INDICATOR_EQUAL_WEIGHTS).unwrap();
    assert!(score.total_score < -quant_core::SIX_INDICATOR_MINIMUM_SCORE_OUT_OF_60);
    assert_eq!(row["Signal_Direction"], "SHORT");
    assert_eq!(
        row["Signal_Scoring_Model_ID"],
        quant_core::ENTRY_SCORE_MODEL_ID
    );
    assert_eq!(
        row["Signal_Strategy_ID"],
        crate::master_data::combined_entry_strategy_id(timeframe, false).unwrap()
    );
    assert_eq!(
        row["Signal_Strength"].as_f64().unwrap(),
        score.total_score.abs()
    );
    let components = row["Signal_Score_Components"].as_array().unwrap();
    assert_eq!(components.len(), 6);
    assert_eq!(components[0]["state"], "UNAVAILABLE");
    assert_eq!(components[0]["weighted_points"], 0.0);
    assert_eq!(components[5]["indicator"], "Lag-one Return Autocorrelation");
}

#[test]
fn stockbit_4h_scores_without_a_15m_price_as_a_reference_only() {
    let conn = Connection::open_in_memory().unwrap();
    let retained = Default::default();
    let input = scan_input(&conn, true, &retained);
    let timeframe = "4h";
    let last_bar_time = current_time_ns() - timeframe_duration_ns(timeframe).unwrap();
    let bars = bars_with_optional_final_decline(last_bar_time, true);
    let source = ("stockbit_source".to_string(), "provider".to_string());
    let mut candidates = Vec::new();
    append_combined_entry(
        &input,
        true,
        timeframe,
        Some(&source),
        &bars,
        None,
        &mut candidates,
        &mut Vec::new(),
    );
    assert_eq!(candidates.len(), 1);
    let row = &candidates[0];
    assert_eq!(row["Current_Price_Reference_Only"], true);
    assert_eq!(row["Current_Price_Timeframe"], timeframe);
    assert_eq!(row["Current_Price_Bar_Time"], last_bar_time);
    assert_eq!(row["Current_Price_Data_Source"], source.0);
    assert_eq!(row["Signal_Score_Bar_Is_Forming"], false);
}

#[test]
fn neutral_stockbit_score_updates_only_a_retained_row() {
    let conn = Connection::open_in_memory().unwrap();
    let timeframe = "4h";
    let strategy_id = crate::master_data::combined_entry_strategy_id(timeframe, true).unwrap();
    let retained = [(strategy_id, timeframe.into())].into_iter().collect();
    let input = scan_input(&conn, true, &retained);
    let last_bar_time = current_time_ns() - timeframe_duration_ns(timeframe).unwrap();
    let bars = bars_with_optional_final_decline(last_bar_time, false);
    let source = ("stockbit_source".to_string(), "provider".to_string());
    let mut candidates = Vec::new();
    let mut updates = Vec::new();
    append_combined_entry(
        &input,
        true,
        timeframe,
        Some(&source),
        &bars,
        None,
        &mut candidates,
        &mut updates,
    );
    assert!(candidates.is_empty());
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0]["Score_Update_Status"], "not_qualifying");
    assert_eq!(updates[0]["Signal_Strength"], 0.0);
    assert!(updates[0]["Signal_Direction"].is_null());
}
