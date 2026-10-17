use super::*;
use crate::main_test_support::{remove_temporary_database, temporary_database_path};

const PRODUCT: &str = "tokocrypto_spot";
const STRATEGY: &str = "tokocrypto_spot_day_trade";
const SYMBOL: &str = "BTC-USD";

fn add_market_history(connection: &Connection, latest_bar_open_ns: i64, count: i64) {
    connection
        .execute(
            "INSERT INTO series(symbol,interval,source,exchange_id,product_id,data_provider,
                                timezone,price_basis,instrument_kind,last_success)
             VALUES(?1,'15m','tokocrypto_native','tokocrypto',?2,'tokocrypto',
                    'UTC','last_trade','spot',strftime('%Y-%m-%d %H:%M:%S','now') || ' UTC')",
            params![SYMBOL, PRODUCT],
        )
        .expect("product-scoped 15m history should exist");
    let interval_ns = 15 * 60 * 1_000_000_000_i64;
    let first_bar = latest_bar_open_ns - (count - 1) * interval_ns;
    for index in 0..count {
        let close = 200.0 + index as f64;
        connection
            .execute(
                "INSERT INTO bars(symbol,interval,source,ts,open,high,low,close,volume,ingested_at)
                 VALUES(?1,'15m','tokocrypto_native',?2,?3,?4,?5,?6,100,'test')",
                params![
                    SYMBOL,
                    first_bar + index * interval_ns,
                    close - 0.25,
                    close + 1.0,
                    close - 1.0,
                    close
                ],
            )
            .expect("completed 15m candle should persist");
    }
}

fn run_frame_scan(connection: &Connection) -> (Vec<Value>, Vec<Value>) {
    let mut running = Vec::new();
    let mut potential = Vec::new();
    append_timeframe_signal_plans(
        TimeframeScanInput {
            app_connection: connection,
            market_connection: connection,
            sync_state: None,
            exchange_id: "tokocrypto",
            product_id: PRODUCT,
            category_id: "crypto_spot",
            symbol: SYMBOL,
            asset: "BTC",
            strategy: STRATEGY,
            evidence_class: "crypto_spot",
            class: "CRYPTO_SPOT",
            exchange: "TOKOCRYPTO",
            time_window: "Tokocrypto spot / 24/7",
            base_timeframe: "15m",
            is_tokenized: false,
            is_multibagger: false,
            retained_stockbit_signal_rows: &std::collections::BTreeSet::new(),
        },
        &mut running,
        &mut potential,
        &mut HashMap::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
    )
    .expect("timeframe scan should complete");
    (running, potential)
}

#[test]
fn fresh_screener_price_uses_the_latest_completed_15m_bar_without_watchlist_rows() {
    let path = temporary_database_path("scan-frame-current-price");
    let connection = open_db_minimal(&path).expect("storage schema should initialize");
    let interval_ns = 15 * 60 * 1_000_000_000_i64;
    let now_ns = i64::try_from(now_secs()).unwrap() * 1_000_000_000;
    let latest_open = now_ns.div_euclid(interval_ns) * interval_ns - interval_ns;
    add_market_history(&connection, latest_open, 40);
    let retained_stockbit_signal_rows = std::collections::BTreeSet::new();
    let input = TimeframeScanInput {
        app_connection: &connection,
        market_connection: &connection,
        sync_state: None,
        exchange_id: "tokocrypto",
        product_id: PRODUCT,
        category_id: "crypto_spot",
        symbol: SYMBOL,
        asset: "BTC",
        strategy: STRATEGY,
        evidence_class: "crypto_spot",
        class: "CRYPTO_SPOT",
        exchange: "TOKOCRYPTO",
        time_window: "Tokocrypto spot / 24/7",
        base_timeframe: "15m",
        is_tokenized: false,
        is_multibagger: false,
        retained_stockbit_signal_rows: &retained_stockbit_signal_rows,
    };
    let current_price =
        fresh_current_price(&input, None, now_ns).expect("fresh 15m data should provide a price");
    assert_eq!(current_price.value, 239.0);
    assert_eq!(current_price.signal_bar_time, latest_open);

    let (running, potential) = run_frame_scan(&connection);
    assert!(running.is_empty() && potential.is_empty());
    drop(connection);
    remove_temporary_database(&path);
}

#[test]
fn stale_15m_price_withholds_all_signal_frames_without_changing_history() {
    let path = temporary_database_path("scan-frame-stale-price");
    let connection = open_db_minimal(&path).expect("storage schema should initialize");
    let interval_ns = 15 * 60 * 1_000_000_000_i64;
    let now_ns = i64::try_from(now_secs()).unwrap() * 1_000_000_000;
    let latest_open = now_ns.div_euclid(interval_ns) * interval_ns - 5 * interval_ns;
    add_market_history(&connection, latest_open, 40);
    let (running, potential) = run_frame_scan(&connection);

    assert!(running.is_empty() && potential.is_empty());
    drop(connection);
    remove_temporary_database(&path);
}

#[test]
fn stockbit_scan_checks_exclusions_in_the_app_database() {
    let app_connection = Connection::open_in_memory().expect("app database should open");
    app_connection
        .execute_batch("ATTACH DATABASE ':memory:' AS app;")
        .expect("app schema should attach");
    crate::master_data::initialize(&app_connection)
        .expect("app reference catalog should initialize");
    crate::instrument_registry::initialize_schema(&app_connection)
        .expect("app registry schema should initialize");
    let market_connection = Connection::open_in_memory().expect("market database should open");
    assert!(crate::instrument_registry::is_excluded(
        &market_connection,
        "stockbit",
        "idx_stock",
        "TEST"
    )
    .is_err());

    let bars = [StrategyBar {
        time: 0,
        open: 99.0,
        high: 101.0,
        low: 98.0,
        close: 100.0,
        volume: 1_000.0,
    }];
    let mut running = Vec::new();
    let mut potential = Vec::new();
    let mut cagr_stats = HashMap::new();
    super::super::append_scan_plan_with_options(
        &market_connection,
        &app_connection,
        &mut running,
        &mut potential,
        &bars,
        "TEST.JK",
        "TEST",
        "idx_stockbit_day_trade",
        "idx",
        "IDX_STOCK",
        "STOCKBIT",
        "Stockbit test",
        false,
        false,
        true,
        &mut cagr_stats,
        "stockbit_regular",
        super::super::ScanPlanOptions::default(),
    );

    assert!(running.is_empty());
    assert_eq!(potential.len(), 1);
    assert_eq!(potential[0]["Direction"], "WAIT");
    drop(market_connection);
}

#[test]
fn timeframe_diagnostics_do_not_duplicate_the_full_mtf_payload_per_plan() {
    let diagnostic = serde_json::json!({
        "status": "CURRENT",
        "timeframes": [
            {"timeframe": "15m", "score": 2},
            {"timeframe": "1h", "score": 1}
        ]
    });
    let mut running = Vec::new();
    let mut potential = vec![
        serde_json::json!({"timeframe": "15m"}),
        serde_json::json!({"timeframe": "1h"}),
    ];

    annotate_new_dss_screener_rows(&mut running, &mut potential, 0, 0, "15m", &diagnostic);

    assert_eq!(potential[0]["DSS_Bressert_MTF"]["status"], "CURRENT");
    assert_eq!(potential[0]["DSS_Bressert_Frame"], Value::Null);
    assert_eq!(potential[1]["DSS_Bressert_Frame"]["score"], 1);
    assert_eq!(potential[1]["DSS_Bressert_MTF"], Value::Null);
}

#[test]
fn plan_rule_descriptions_follow_the_selected_signal_timeframe() {
    let mut plan = serde_json::json!({
        "timeframe": "15m",
        "Invalidation": "15m close below the setup",
        "Catalyst_Thesis": "15m momentum confirmation",
        "DSS_Regime": "15m DSS near-cross"
    });

    annotate_plan_signal_timeframe(&mut plan, "4h");

    assert_eq!(plan["timeframe"], "4h");
    assert_eq!(plan["Zone_Timeframe"], "4h");
    assert_eq!(plan["Invalidation"], "4h close below the setup");
    assert_eq!(plan["Catalyst_Thesis"], "4h momentum confirmation");
    assert_eq!(plan["DSS_Regime"], "4h DSS near-cross");
}

#[test]
fn secondary_timeframe_gets_a_distinct_plan_id_while_base_id_stays_stable() {
    let base_plan = serde_json::json!({"Plan_ID": "tokocrypto::spot::btc"});
    let mut secondary_plan = base_plan.clone();

    append_timeframe_to_plan_id(&mut secondary_plan, "1h");

    assert_eq!(base_plan["Plan_ID"], "tokocrypto::spot::btc");
    assert_eq!(secondary_plan["Plan_ID"], "tokocrypto::spot::btc::1h");
}
