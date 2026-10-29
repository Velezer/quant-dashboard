use super::*;

#[test]
fn crypto_signal_uses_only_the_latest_completed_bar() {
    let bars = [
        StrategyBar {
            time: 1,
            open: 100.0,
            high: 101.0,
            low: 99.0,
            close: 100.0,
            volume: 1.0,
        },
        StrategyBar {
            time: 2,
            open: 100.0,
            high: 102.0,
            low: 99.0,
            close: 101.0,
            volume: 1.0,
        },
    ];
    let mut signals = vec![quant_core::StrategySignal::default(); bars.len()];
    signals[0].direction = "LONG".into();

    assert!(latest_crypto_signal_for_latest_bar(&signals, &bars).is_none());

    signals[1].direction = "SHORT".into();
    let (index, signal) = latest_crypto_signal_for_latest_bar(&signals, &bars).unwrap();
    assert_eq!(index, 1);
    assert_eq!(signal.direction, "SHORT");
}

#[test]
fn crypto_dss_rows_keep_dss_and_nwe_points_on_the_correct_indicators() {
    let mut bars: Vec<StrategyBar> = (0..quant_core::DSS_BRESSERT_MINIMUM_HISTORY_BARS + 20)
        .map(|index| {
            let close: f64 = if index % 2 == 0 { 99.0 } else { 101.0 };
            StrategyBar {
                time: index as i64 * 900 * NANOS_PER_SECOND,
                open: 100.0,
                high: close.max(100.0) + 1.0,
                low: close.min(100.0) - 1.0,
                close,
                volume: 1_000.0,
            }
        })
        .collect();
    let signals = quant_core::generate_stockbit_dss_bressert_4h_signals(&bars);
    let index = signals.iter().rposition(|signal| {
        matches!(signal.direction.as_str(), "LONG" | "SHORT")
            && signal.confluence.as_ref().is_some_and(|confluence| {
                (signal.strategy_strength - confluence.score).abs() > 0.01
            })
    }).expect("fixture must distinguish DSS strength from NWE confluence");
    bars.truncate(index + 1);
    let expected = &signals[index];
    let connection = Connection::open_in_memory().unwrap();
    let retained = std::collections::BTreeSet::new();
    for (exchange, product) in [("pintu", "pintu_pro_spot"), ("pluang", "pluang_spot"),
        ("tokocrypto", "tokocrypto_spot")] {
        let input = TimeframeScanInput {
            app_connection: &connection, market_connection: &connection, sync_state: None,
            exchange_id: exchange, product_id: product, category_id: "crypto_spot",
            symbol: "BTC-USD", asset: "BTC", strategy: "test", evidence_class: "crypto_spot",
            class: "crypto_spot", exchange, time_window: "test", base_timeframe: "15m",
            is_tokenized: false, is_multibagger: false, retained_stockbit_signal_rows: &retained,
        };
        let source = (format!("{exchange}_yfinance_fallback"), "yfinance".to_string());
        let price = CurrentPrice {
            value: bars.last().unwrap().close, signal_bar_time: bars.last().unwrap().time,
            source: source.0.clone(), provider: source.1.clone(),
        };
        let mut candidates = Vec::new();
        append_nwe_dss_model_signals(&input, false, "15m", Some(&source), &bars,
            Some(&price), &mut candidates, &mut Vec::new());
        let row = candidates.iter().find(|row| row["Signal_Strategy_ID"] ==
            quant_core::crypto_dss_bressert_signal_strategy_id("15m").unwrap())
            .expect("DSS fixture should generate a crypto signal");
        let components = row["Signal_Score_Components"].as_array().unwrap();
        let dss = components.iter().find(|item| item["indicator"] == "DSS Bressert").unwrap();
        let nwe = components.iter().find(|item| item["indicator"] == "Nadaraya-Watson Envelope").unwrap();
        assert_eq!(dss["points"], expected.strategy_strength);
        assert_eq!(dss["state"], expected.regime);
        assert_eq!(nwe["points"], expected.confluence.as_ref().unwrap().score);
        assert_eq!(nwe["state"], expected.confluence.as_ref().unwrap().nadaraya_state);
        assert_eq!(row["Exchange_ID"], exchange);
        assert_eq!(row["Product_ID"], product);
        assert_eq!(row["Data_Source"], source.0);
    }
}
