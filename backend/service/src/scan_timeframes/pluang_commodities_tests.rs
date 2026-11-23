use super::*;

fn commodity_input<'a>(
    connection: &'a Connection,
    retained: &'a std::collections::BTreeSet<(String, String)>,
) -> TimeframeScanInput<'a> {
    TimeframeScanInput {
        app_connection: connection,
        market_connection: connection,
        sync_state: None,
        exchange_id: "pluang",
        product_id: "pluang_commodities",
        category_id: "commodity_derivative",
        quote_currency: "USD",
        symbol: "XAUUSD-USD",
        asset: "XAUUSD",
        strategy: "pluang_commodities_combined_entry_15m_v1",
        evidence_class: "commodity_derivative",
        class: "COMMODITY",
        exchange: "PLUANG",
        time_window: "15m",
        base_timeframe: "15m",
        is_tokenized: false,
        is_multibagger: false,
        retained_stockbit_signal_rows: retained,
        active_scoring_profiles: None,
    }
}

#[test]
fn pluang_commodities_score_emits_one_six_factor_signal_only_on_15m() {
    let connection = Connection::open_in_memory().unwrap();
    let retained = Default::default();
    let input = commodity_input(&connection, &retained);
    let timeframe = "15m";
    let duration = timeframe_duration_ns(timeframe).unwrap();
    let last_time = current_time_ns().saturating_sub(duration);
    let mut bars = (0..quant_core::ENTRY_SCORE_MINIMUM_HISTORY_BARS)
        .map(|index| StrategyBar {
            time: last_time
                - (quant_core::ENTRY_SCORE_MINIMUM_HISTORY_BARS - 1 - index) as i64 * duration,
            open: 100.0,
            high: 101.0,
            low: 99.0,
            close: 100.0,
            volume: 100.0,
        })
        .collect::<Vec<_>>();
    let last = bars.last_mut().unwrap();
    last.close = 99.0;
    last.volume = 300.0;

    let source = (
        "pluang_commodities_yfinance_fallback".to_string(),
        "yfinance".to_string(),
    );
    let mut candidates = Vec::new();
    append_combined_entry(
        &input,
        false,
        true,
        timeframe,
        Some(&source),
        &bars,
        None,
        &mut candidates,
        &mut Vec::new(),
    );

    assert_eq!(candidates.len(), 1);
    assert_eq!(
        candidates[0]["Signal_Strategy_ID"],
        "pluang_commodities_combined_entry_15m_v1"
    );
    assert_eq!(
        candidates[0]["Signal_Scoring_Model_ID"],
        quant_core::ENTRY_SCORE_MODEL_ID
    );
    assert_eq!(candidates[0]["timeframe"], "15m");
    assert_eq!(candidates[0]["Data_Quality"], "RESEARCH_PROXY");
    assert_eq!(
        candidates[0]["Signal_Score_Components"]
            .as_array()
            .unwrap()
            .len(),
        6
    );
    assert!(crate::master_data::pluang_commodities_combined_entry_strategy_id("1h").is_none());
}
