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
