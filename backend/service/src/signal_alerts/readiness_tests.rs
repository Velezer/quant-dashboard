use super::*;

fn connection() -> Connection {
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch("ATTACH DATABASE ':memory:' AS signals;")
        .unwrap();
    initialize_screener_schema(&connection).unwrap();
    connection
}

#[test]
fn readiness_keeps_app_product_counts_separate_for_a_shared_symbol() {
    let mut scan = SignalReadiness::new();
    for (app, product, status) in [
        ("pintu", "pintu_pro_spot", "fresh"),
        ("pluang", "pluang_spot", "stale"),
        ("pluang", "pluang_perpetual", "missing"),
    ] {
        scan.record(
            app,
            product,
            "BTC-USD",
            InstrumentReadiness {
                price_status: status,
                timeframes: vec![TimeframeReadiness {
                    timeframe: "15m".into(),
                    evaluated_count: usize::from(status == "fresh"),
                    ..TimeframeReadiness::default()
                }],
                ..InstrumentReadiness::default()
            },
            &BTreeMap::from([("15m".into(), usize::from(status == "fresh"))]),
            None,
        );
    }
    let value = scan.into_value();
    let products = value["products"].as_array().unwrap();
    assert_eq!(products.len(), 3);
    assert_eq!(products[0]["fresh_price_count"], 1);
    assert_eq!(products[0]["timeframes"][0]["qualifying_signal_count"], 1);
    assert_eq!(products[1]["missing_price_count"], 1);
    assert_eq!(products[2]["stale_price_count"], 1);
    for product in products {
        assert_eq!(product["instrument_count"], 1);
    }
}

#[test]
fn readiness_examples_are_bounded_and_errors_are_truncated() {
    let mut scan = SignalReadiness::new();
    for reason in [
        "stale_price",
        "data_error",
        "missing_history",
        "source_mismatch",
    ] {
        scan.record(
            "pintu",
            "pintu_pro_spot",
            "BTC-USD",
            InstrumentReadiness {
                price_status: "stale",
                issues: vec![ReadinessIssue {
                    symbol: "BTC".into(),
                    reason: reason.into(),
                    error: Some("x".repeat(500)),
                    ..ReadinessIssue::default()
                }],
                ..InstrumentReadiness::default()
            },
            &BTreeMap::new(),
            None,
        );
    }
    let value = scan.into_value();
    let examples = value["products"][0]["examples"].as_array().unwrap();
    assert_eq!(examples.len(), MAX_PRODUCT_ISSUE_EXAMPLES);
    assert_eq!(examples[0]["error"].as_str().unwrap().len(), 250);
}

#[test]
fn readiness_never_attributes_another_timeframes_refresh_failure_to_an_issue() {
    let connection = Connection::open_in_memory().unwrap();
    connection.execute_batch("CREATE TABLE series(symbol TEXT,source TEXT,interval TEXT,last_success TEXT,last_error TEXT);
        INSERT INTO series VALUES('BTC-USD','test_source','1d','2026-10-23 01:00:00 UTC','daily refresh failed');").unwrap();
    let mut scan = SignalReadiness::new();
    scan.record(
        "pintu",
        "pintu_pro_spot",
        "BTC-USD",
        InstrumentReadiness {
            issues: vec![ReadinessIssue {
                symbol: "BTC".into(),
                reason: "stale_history".into(),
                timeframe: "4h".into(),
                source: Some("test_source".into()),
                ..ReadinessIssue::default()
            }],
            ..InstrumentReadiness::default()
        },
        &BTreeMap::new(),
        Some(&connection),
    );
    let value = scan.into_value();
    assert!(value["products"][0]["examples"][0]["error"].is_null());
    assert!(value["products"][0]["examples"][0]["last_success_at"].is_null());
}

#[test]
fn readiness_is_unknown_before_a_scan_and_commits_with_the_snapshot() {
    let connection = connection();
    assert!(load_scan_readiness(&connection).unwrap().is_null());
    let readiness = json!({"version":1,"checked_at":123,"products":[]});
    super::super::screener::replace_signal_screener_snapshot_with_readiness(
        &connection,
        &[],
        &[],
        0,
        0,
        Some(&readiness),
    )
    .unwrap();
    assert_eq!(load_scan_readiness(&connection).unwrap(), readiness);
    super::super::record_signal_screener_failure(&connection, "provider unavailable").unwrap();
    assert_eq!(load_scan_readiness(&connection).unwrap(), readiness);
    connection
        .execute_batch(
            "CREATE TRIGGER signals.reject_scan BEFORE UPDATE ON signal_screener_status
        BEGIN SELECT RAISE(ABORT,'test rollback'); END;",
        )
        .unwrap();
    let next = json!({"version":1,"checked_at":456,"products":[]});
    assert!(
        super::super::screener::replace_signal_screener_snapshot_with_readiness(
            &connection,
            &[],
            &[],
            0,
            0,
            Some(&next),
        )
        .is_err()
    );
    assert_eq!(load_scan_readiness(&connection).unwrap(), readiness);
}

#[test]
fn merged_readiness_sums_product_and_timeframe_counts_and_preserves_time_bounds() {
    let key = ("stockbit".to_string(), "stockbit_regular".to_string());
    let other_key = ("pintu".to_string(), "pintu_pro_spot".to_string());
    let frame = |name: &str, count: usize| TimeframeReadiness {
        timeframe: name.into(),
        missing_history_count: count,
        stale_history_count: count,
        insufficient_history_count: count,
        nwe_insufficient_history_count: count,
        source_mismatch_count: count,
        data_error_count: count,
        evaluated_count: count,
        evaluated_model_count: count,
        qualifying_signal_count: count,
    };
    let mut left = SignalReadiness {
        scan_started_at: 90,
        products: BTreeMap::from([(
            key.clone(),
            ProductReadiness {
                exchange_id: key.0.clone(), product_id: key.1.clone(),
                instrument_count: 1, fresh_price_count: 1,
                first_checked_at: 100, checked_at: 120,
                timeframes: vec![frame("15m", 1)],
                ..ProductReadiness::default()
            },
        )]),
    };
    let right = SignalReadiness {
        scan_started_at: 80,
        products: BTreeMap::from([
            (key.clone(), ProductReadiness {
                exchange_id: key.0.clone(), product_id: key.1.clone(),
                instrument_count: 2, missing_price_count: 1, stale_price_count: 1,
                first_checked_at: 95, checked_at: 140,
                timeframes: vec![frame("15m", 2), frame("1h", 4)],
                ..ProductReadiness::default()
            }),
            (other_key.clone(), ProductReadiness {
                exchange_id: other_key.0.clone(), product_id: other_key.1.clone(),
                instrument_count: 1, data_error_count: 1,
                first_checked_at: 110, checked_at: 130,
                ..ProductReadiness::default()
            }),
        ]),
    };
    left.merge(right);
    assert_eq!(left.scan_started_at, 80);
    let stockbit = left.products.get(&key).unwrap();
    assert_eq!((stockbit.instrument_count, stockbit.fresh_price_count,
        stockbit.missing_price_count, stockbit.stale_price_count, stockbit.data_error_count),
        (3, 1, 1, 1, 0));
    assert_eq!((stockbit.first_checked_at, stockbit.checked_at), (95, 140));
    let fifteen = stockbit.timeframes.iter().find(|frame| frame.timeframe == "15m").unwrap();
    assert_eq!((fifteen.missing_history_count, fifteen.stale_history_count,
        fifteen.insufficient_history_count, fifteen.nwe_insufficient_history_count,
        fifteen.source_mismatch_count, fifteen.data_error_count, fifteen.evaluated_count,
        fifteen.evaluated_model_count, fifteen.qualifying_signal_count),
        (3, 3, 3, 3, 3, 3, 3, 3, 3));
    let hourly = stockbit.timeframes.iter().find(|frame| frame.timeframe == "1h").unwrap();
    assert_eq!(hourly.evaluated_count, 4);
    assert_eq!(left.products.get(&other_key).unwrap().data_error_count, 1);
}

#[test]
fn merged_readiness_examples_and_timeframes_are_bounded_and_order_independent() {
    let key = ("stockbit".to_string(), "stockbit_regular".to_string());
    let cases = [
        ("b_stale", "ZZZ"), ("a_error", "BBB"), ("b_stale", "AAA"),
        ("d_other", "DDD"), ("c_missing", "CCC"), ("b_stale", "AAA"),
    ];
    let fragment = |index: usize| {
        let (reason, symbol) = cases[index];
        SignalReadiness {
            scan_started_at: 100 + index as i64,
            products: BTreeMap::from([(
                key.clone(),
                ProductReadiness {
                    exchange_id: key.0.clone(), product_id: key.1.clone(),
                    instrument_count: 1, fresh_price_count: 1,
                    first_checked_at: 200 + index as i64,
                    checked_at: 300 + index as i64,
                    examples: vec![ReadinessIssue {
                        symbol: symbol.into(), reason: reason.into(), timeframe: "15m".into(),
                        error: (index == 5).then(|| "provider unavailable".into()),
                        ..ReadinessIssue::default()
                    }],
                    timeframes: vec![TimeframeReadiness {
                        timeframe: if index % 2 == 0 { "15m" } else { "1h" }.into(),
                        evaluated_count: 1,
                        ..TimeframeReadiness::default()
                    }],
                    ..ProductReadiness::default()
                },
            )]),
        }
    };
    let mut expected = None;
    for order in [[0, 1, 2, 3, 4, 5], [5, 4, 3, 2, 1, 0], [2, 0, 4, 1, 5, 3]] {
        let mut merged = SignalReadiness { scan_started_at: 0, products: BTreeMap::new() };
        for index in order { merged.merge(fragment(index)); }
        assert_eq!(merged.scan_started_at, 100);
        let product = merged.products.get(&key).unwrap();
        assert_eq!(product.instrument_count, 6);
        assert_eq!((product.first_checked_at, product.checked_at), (200, 305));
        assert_eq!(product.examples.len(), MAX_PRODUCT_ISSUE_EXAMPLES);
        assert_eq!(product.examples.iter().map(|issue| issue.reason.as_str()).collect::<Vec<_>>(),
            vec!["a_error", "b_stale", "c_missing"]);
        assert_eq!(product.examples[1].symbol, "AAA");
        assert_eq!(product.examples[1].error.as_deref(), Some("provider unavailable"));
        assert_eq!(product.timeframes.iter().find(|frame| frame.timeframe == "15m").unwrap().evaluated_count, 3);
        assert_eq!(product.timeframes.iter().find(|frame| frame.timeframe == "1h").unwrap().evaluated_count, 3);
        let value = serde_json::to_value(product).unwrap();
        if let Some(previous) = &expected { assert_eq!(&value, previous); }
        expected = Some(value);
    }
}
