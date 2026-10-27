use super::*;
use crate::main_test_support::*;
use std::collections::HashSet;

#[test]
fn nwe_dss_research_selection_preserves_app_category_and_timeframe_identity() {
    let path = temporary_database_path("nwe-dss-selection");
    let connection = open_db_minimal(&path).expect("temporary database should initialize");
    master_data::initialize(&connection).expect("canonical master data should initialize");
    drop(connection);
    remove_temporary_database(&path);

    let idx = nwe_dss_research_selection("stockbit", "idx_stock", "4h")
        .expect("Stockbit IDX NWE/DSS research is configured");
    assert_eq!(idx.exchange, "stockbit");
    assert_eq!(idx.asset_category_id, "idx_stock");
    assert_eq!(idx.timeframe, "4h");
    assert_eq!(idx.strategy, "nwe_dss_causal_bressert_hold_v1");

    let tokenized = nwe_dss_research_selection("tokocrypto", "tokenized_stock", "1d")
        .expect("Tokocrypto tokenized-stock research retains its own category");
    assert_eq!(tokenized.exchange, "tokocrypto");
    assert_eq!(tokenized.asset_category_id, "tokenized_stock");
    assert_eq!(tokenized.asset_class, "tokenized_stock");
    assert_eq!(tokenized.timeframe, "1d");
    assert!(nwe_dss_research_selection("tokocrypto", "tokenized_stock", "1M").is_err());

    let pintu = nwe_dss_research_selection("pintu", "crypto_perpetual", "4h")
        .expect("Pintu perpetual NWE/DSS research is configured");
    assert_eq!(pintu.exchange, "pintu");
    assert_eq!(
        normalize_research_symbol(pintu, "BTC-USD").unwrap(),
        "BTC-USD"
    );
    assert!(normalize_research_symbol(pintu, "TSLA").is_err());
}

#[test]
fn non_finite_indicator_values_are_not_serialized_as_nan() {
    let value = serde_json::json!({"value": finite_or_none(f64::NAN)});
    assert!(value.get("value").is_some_and(Value::is_null));
}

#[test]
fn tokenized_stock_uses_real_equity_for_fallback_without_changing_exchange_identity() {
    assert!(is_tokenized_stock("TSLA"));
    assert!(is_tokenized_stock("TSLA-USD"));
    assert!(is_tokenized_stock("TSLAUSDT"));
    assert!(!is_tokenized_stock("TSLA.JK"));
    assert_eq!(underlying_yfinance_symbol("TSLA-USD"), "TSLA");
    assert_eq!(underlying_yfinance_symbol("TSLAUSDT"), "TSLA");
    assert_eq!(underlying_yfinance_symbol("BTC-USD"), "BTC-USD");

    let mut plan = serde_json::json!({
        "Symbol": "TSLA",
        "Asset": "TSLA",
        "Exchange": "TOKOCRYPTO",
        "Class": "CRYPTO_FUT"
    });
    let path = temporary_database_path("tokenized-provider");
    let connection = open_db_minimal(&path).expect("database should initialize");
    connection
        .execute(
            "INSERT INTO series(symbol, interval, source, exchange_id, product_id, data_provider,
                                timezone, price_basis, instrument_kind)
             VALUES('TSLA', '15m', 'tokocrypto_tokenized_stock_yfinance_fallback', 'tokocrypto',
                    'tokocrypto_tokenized_stock', 'yfinance', 'UTC', 'yahoo_unadjusted', 'tokenized_stock')",
            [],
        )
        .expect("provider metadata should persist");
    connection.execute("INSERT INTO bars(symbol,interval,source,ts,open,high,low,close,volume,ingested_at)
        VALUES('TSLA','15m','tokocrypto_tokenized_stock_yfinance_fallback',0,100,101,99,100,10,'test')",[]).unwrap();
    annotate_plan_market_data(
        &connection,
        &mut plan,
        "TSLA",
        "tokocrypto_tokenized_stock",
        "15m",
        true,
    );
    assert_eq!(plan["Exchange"], "TOKOCRYPTO");
    assert_eq!(plan["Class"], "TOKENIZED_STOCK");
    assert_eq!(plan["Underlying_Symbol"], "TSLA");
    assert_eq!(plan["Data_Symbol"], "TSLA");
    assert_eq!(
        plan["Data_Source"],
        "tokocrypto_tokenized_stock_yfinance_fallback"
    );
    assert_eq!(plan["Data_Provider"], "yfinance");
    drop(connection);
    remove_temporary_database(&path);
}

#[test]
fn plan_app_identity_stays_independent_of_candle_provider() {
    let path = temporary_database_path("app-provider-identity");
    let connection = open_db_minimal(&path).expect("database should initialize");
    for (source, exchange, product, category, provider) in [
        (
            "tokocrypto_native",
            "tokocrypto",
            "tokocrypto_spot",
            "crypto_spot",
            "tokocrypto",
        ),
        (
            "shared_crypto_perpetual_binance_usdm_futures_proxy",
            "research_reference",
            "",
            "crypto_perpetual",
            "binance",
        ),
    ] {
        connection
            .execute(
                "INSERT INTO series(symbol,interval,source,exchange_id,product_id,data_provider,
                                    timezone,price_basis,instrument_kind)
                 VALUES('BTC-USD','15m',?1,?2,?3,?4,'UTC','binance_usdt_reference',?5)",
                params![source, exchange, product, provider, category],
            )
            .expect("source metadata should persist");
        connection
            .execute(
                "INSERT INTO bars(symbol,interval,source,ts,open,high,low,close,volume,ingested_at)
            VALUES('BTC-USD','15m',?1,0,100,101,99,100,10,'test')",
                [source],
            )
            .unwrap();
    }
    let mut plan = serde_json::json!({"Exchange":"PINTU_FUTURES","Symbol":"BTC-USD"});
    annotate_plan_market_data(
        &connection,
        &mut plan,
        "BTC-USD",
        "pintu_futures",
        "15m",
        false,
    );
    assert_eq!(plan["Exchange"], "PINTU_FUTURES");
    assert_eq!(
        plan["Data_Source"],
        "shared_crypto_perpetual_binance_usdm_futures_proxy"
    );
    assert_eq!(plan["Data_Provider"], "binance");
    assert_eq!(plan["Data_Symbol"], "BTC-USD");
    drop(connection);
    remove_temporary_database(&path);
}

#[test]
fn backtest_gate_demotes_unproven_plan_to_watchlist() {
    let mut stats = HashMap::new();
    stats.insert(
        backtest_key("MEDC.JK", "idx"),
        serde_json::json!({
            "trades": 4,
            "sample_days": 606,
            "cagr_validated": false,
            "win_rate_pct": 75.0,
            "profit_factor": 4.06
        }),
    );
    let mut plan = serde_json::json!({
        "Symbol": "MEDC.JK",
        "Asset": "MEDC",
        "Class": "IDX_STOCK",
        "Exchange": "STOCKBIT",
        "Current_Price": 100.0,
        "Price_Time_Zone": "99 - 101",
        "timeframe": "15m",
        "Action": "STOCKBIT_DAY_TRADE",
        "State": "IN_ZONE",
        "Zone_Status": "TRIGGERED",
        "Strategy_Status": "RESEARCH_UNVALIDATED"
    });

    assert!(!apply_backtest_gate(&mut plan, &stats));
    assert_eq!(plan["Backtest_Proof"]["status"], "REQUIRED");
    assert_eq!(plan["Action"], "MONITOR");
    assert_eq!(plan["State"], "WATCHLIST");
    assert_eq!(plan["Is_Watchlist"], true);
    assert_eq!(plan["Price_Position"], "IN_ZONE");
    assert_eq!(plan["Execution_Status"], "VALIDATION_PENDING");
    assert_eq!(plan["Zone_Timeframe"], "15m");
}

#[test]
fn backtest_gate_accepts_only_matching_validated_class() {
    let mut stats = HashMap::new();
    stats.insert(
        backtest_key("BTC-USD_FUT", "crypto_futures"),
        serde_json::json!({
            "trades": 10,
            "sample_days": 730,
            "cagr_validated": true,
            "cagr_validation_status": "VALIDATED_UNTOUCHED_HOLDOUT",
            "holdout_cagr_pct": 12.0,
            "holdout_trades": 10,
            "holdout_sample_days": 730,
            "holdout_win_rate_pct": 70.0,
            "holdout_profit_factor": 1.4,
            "win_rate_pct": 70.0,
            "profit_factor": 1.4,
            "execution_scope": "venue_history"
        }),
    );
    let mut plan = serde_json::json!({
        "Symbol": "BTC-USD",
        "Asset": "BTC",
        "Class": "CRYPTO_FUT",
        "Exchange": "PINTU_FUTURES",
        "Current_Price": 100.0,
        "Price_Time_Zone": "99 - 101",
        "timeframe": "15m",
        "Action": "PINTU_FUTURES_DAY_TRADE",
        "State": "IN_ZONE"
    });

    assert!(apply_backtest_gate(&mut plan, &stats));
    assert_eq!(plan["Backtest_Proof"]["status"], "VALIDATED");
    assert_eq!(plan["State"], "IN_ZONE");
    assert_eq!(plan["Backtest_Stats"]["trades"], 10);
    assert_eq!(plan["Backtest_Proof"]["minimum_win_rate_pct"], 50.0);
    assert_eq!(plan["Price_Position"], "IN_ZONE");
    assert_eq!(plan["Execution_Status"], "READY_FOR_REVIEW");
    assert_eq!(plan["Zone_Timeframe"], "15m");
}

#[test]
fn previous_strategy_evidence_cannot_validate_confirmation_version() {
    let mut stats = HashMap::new();
    stats.insert(
        backtest_key("MEDC.JK", "idx"),
        serde_json::json!({
            "strategy_id": "idx_stockbit_day_trade",
            "trades": 10,
            "sample_days": 730,
            "cagr_validated": true,
            "cagr_validation_status": "VALIDATED_UNTOUCHED_HOLDOUT",
            "holdout_cagr_pct": 300.0,
            "holdout_trades": 10,
            "holdout_sample_days": 730,
            "holdout_win_rate_pct": 80.0,
            "holdout_profit_factor": 2.0,
            "win_rate_pct": 80.0,
            "profit_factor": 2.0,
            "execution_scope": "venue_history"
        }),
    );
    let strategy_id = quant_core::historical_evidence_profile("idx_stockbit_day_trade")
        .expect("confirmation evidence profile should resolve")
        .strategy_id;
    let mut plan = serde_json::json!({
        "Symbol": "MEDC.JK",
        "Asset": "MEDC",
        "Class": "IDX_STOCK",
        "Exchange": "STOCKBIT",
        "Strategy_ID": strategy_id,
        "Signal_Direction": "LONG",
        "is_day_trade": true,
        "State": "IN_ZONE"
    });

    assert!(!apply_backtest_gate(&mut plan, &stats));
    assert_eq!(plan["Decision"], "WAIT");
    assert!(plan["Backtest_Proof"]["reason"]
        .as_str()
        .unwrap_or_default()
        .contains("different strategy profile"));
}

#[test]
fn backtest_gate_rechecks_history_and_native_scope() {
    let mut stats = HashMap::new();
    stats.insert(
        backtest_key("BTC-USD_FUT", "crypto_futures"),
        serde_json::json!({
            "trades": 10,
            "sample_days": 730,
            "cagr_validated": true,
            "cagr_pct": 125.0,
            "win_rate_pct": 75.0,
            "profit_factor": 1.8,
            "execution_scope": "legacy_research"
        }),
    );
    let mut plan = serde_json::json!({
        "Symbol": "BTC-USD",
        "Asset": "BTC",
        "Class": "CRYPTO_FUT",
        "Exchange": "PINTU_FUTURES",
        "Action": "PINTU_FUTURES_DAY_TRADE",
        "State": "IN_ZONE"
    });

    assert!(!apply_backtest_gate(&mut plan, &stats));
    assert!(plan["Backtest_Proof"]["reason"]
        .as_str()
        .unwrap_or_default()
        .contains("venue-native"));

    stats.insert(
        backtest_key("BTC-USD_FUT", "crypto_futures"),
        serde_json::json!({
            "trades": 10,
            "sample_days": 365,
            "cagr_validated": true,
            "cagr_pct": 125.0,
            "win_rate_pct": 75.0,
            "profit_factor": 1.8,
            "execution_scope": "venue_history"
        }),
    );
    let mut short_history_plan = serde_json::json!({
        "Symbol": "BTC-USD",
        "Asset": "BTC",
        "Class": "CRYPTO_FUT",
        "Exchange": "PINTU_FUTURES",
        "Action": "PINTU_FUTURES_DAY_TRADE",
        "State": "IN_ZONE"
    });
    assert!(!apply_backtest_gate(&mut short_history_plan, &stats));
    assert_eq!(
        short_history_plan["Backtest_Proof"]["observed_sample_days"],
        365
    );
}

#[test]
fn backtest_gate_rejects_day_trade_below_fifty_percent_win_rate() {
    let mut stats = HashMap::new();
    stats.insert(
        backtest_key("BTC-USD_FUT", "crypto_futures"),
        serde_json::json!({
            "trades": 10,
            "sample_days": 730,
            "cagr_validated": true,
            "win_rate_pct": 49.9,
            "holdout_win_rate_pct": 49.9,
            "holdout_profit_factor": 1.8,
            "profit_factor": 1.8,
            "execution_scope": "venue_history"
        }),
    );
    let mut plan = serde_json::json!({
        "Symbol": "BTC-USD",
        "Asset": "BTC",
        "Class": "CRYPTO_FUT",
        "Exchange": "PINTU_FUTURES",
        "Current_Price": 100.0,
        "Action": "PINTU_FUTURES_DAY_TRADE",
        "State": "IN_ZONE"
    });

    assert!(!apply_backtest_gate(&mut plan, &stats));
    assert_eq!(plan["Backtest_Proof"]["status"], "REQUIRED");
    assert_eq!(plan["Backtest_Proof"]["minimum_win_rate_pct"], 50.0);
    assert_eq!(plan["Backtest_Proof"]["observed_win_rate_pct"], 49.9);
    assert!(plan["Backtest_Proof"]["reason"]
        .as_str()
        .unwrap_or_default()
        .contains("50% minimum win-rate goal"));
    assert_eq!(plan["Action"], "MONITOR");
}

#[test]
fn backtest_gate_accepts_fifty_percent_with_a_profitable_holdout() {
    let mut stats = HashMap::new();
    stats.insert(
        backtest_key("BTC-USD_FUT", "crypto_futures"),
        serde_json::json!({
            "trades": 20,
            "sample_days": 730,
            "cagr_validated": true,
            "cagr_validation_status": "VALIDATED_UNTOUCHED_HOLDOUT",
            "holdout_cagr_pct": 12.0,
            "holdout_trades": 20,
            "holdout_sample_days": 730,
            "holdout_win_rate_pct": 50.0,
            "holdout_profit_factor": 1.8,
            "win_rate_pct": 50.0,
            "profit_factor": 1.8,
            "observed_reward_risk_ratio": 1.8,
            "execution_scope": "venue_history"
        }),
    );
    let mut plan = serde_json::json!({
        "Symbol": "BTC-USD",
        "Asset": "BTC",
        "Class": "CRYPTO_FUT",
        "Exchange": "PINTU_FUTURES",
        "Current_Price": 100.0,
        "Action": "PINTU_FUTURES_DAY_TRADE",
        "State": "IN_ZONE"
    });

    assert!(apply_backtest_gate(&mut plan, &stats));
    assert_eq!(plan["Backtest_Proof"]["minimum_win_rate_pct"], 50.0);
    assert_eq!(plan["Backtest_Proof"]["observed_reward_risk_ratio"], 1.8);
    assert_eq!(plan["Execution_Status"], "READY_FOR_REVIEW");
}

#[test]
fn low_win_rate_strategy_invention_stays_research_only() {
    let bars = (0..80)
        .map(|index| {
            let close = 100.0 + index as f64;
            StrategyBar {
                time: (1_700_000_000_i64 + index as i64 * 900) * 1_000_000_000,
                open: close,
                high: close + 2.0,
                low: close,
                close,
                volume: 1_000_000_000.0,
            }
        })
        .collect::<Vec<_>>();
    let baseline = serde_json::json!({
        "win_rate_pct": 38.9,
        "trades": 36,
        "execution_scope": "research_proxy",
        "source_label": "yfinance fallback proxy"
    });

    let report = build_strategy_invention_report(&bars, "idx_stockbit_day_trade", Some(&baseline))
        .expect("registered day-trade strategy should have bounded variants");
    assert_eq!(report["status"], "LOW_WR_TRIGGERED");
    assert_eq!(report["baseline"]["win_rate_pct"], 38.9);
    assert!(report["variants_tested"].as_u64().unwrap_or(0) >= 1);
    assert!(report["selected"].is_object());
    assert!(matches!(
        report["improvement_status"].as_str(),
        Some("IMPROVED_BASELINE") | Some("NO_IMPROVEMENT")
    ));
    assert_eq!(
        report["selected"]["improves_baseline"],
        report["improvement_status"] == "IMPROVED_BASELINE"
    );
    assert_eq!(report["execution_status"], "RESEARCH_ONLY");
    if report["improvement_status"] == "NO_IMPROVEMENT" {
        assert_eq!(report["selected"]["promotion_status"], "NO_IMPROVEMENT");
    }
}

#[test]
fn watchlist_without_current_signal_is_reference_only() {
    let mut plan = serde_json::json!({
        "Symbol": "BTC-USD",
        "Asset": "BTC",
        "Class": "CRYPTO_FUT",
        "Exchange": "PINTU_FUTURES",
        "Current_Price": 100.0,
        "Price_Time_Zone": "99 - 101",
        "State": "WATCHLIST",
        "Is_Watchlist": true,
        "Regime": "WATCHLIST_NO_CURRENT_SIGNAL",
        "Strategy_Status": "WATCHLIST_NO_CURRENT_SIGNAL"
    });

    assert!(!apply_backtest_gate(&mut plan, &HashMap::new()));
    assert_eq!(plan["Price_Position"], "REFERENCE_ONLY");
    assert_eq!(plan["Execution_Status"], "MONITOR_ONLY");
}

#[test]
fn stockbit_scan_does_not_add_watchlist_rows_below_rp60() {
    let connection = Connection::open_in_memory().expect("in-memory SQLite should open");
    let bars = vec![StrategyBar {
        time: 1_790_649_000_i64 * 1_000_000_000,
        open: 59.0,
        high: 60.0,
        low: 58.0,
        close: 59.0,
        volume: 1_000_000.0,
    }];
    let mut running = Vec::new();
    let mut potential = Vec::new();
    let mut stats = HashMap::new();

    append_scan_plan(
        &connection,
        &mut running,
        &mut potential,
        &bars,
        "ACRO.JK",
        "ACRO",
        "idx_stockbit_day_trade",
        "idx",
        "IDX_STOCK",
        "STOCKBIT",
        "Stockbit T+0",
        false,
        false,
        true,
        &mut stats,
        "",
    );

    assert!(running.is_empty());
    assert!(potential.is_empty());
}

#[test]
fn stockbit_scan_exposes_research_models_as_monitor_only_plans() {
    let path = temporary_database_path("stockbit-research-plans");
    let connection = open_db_minimal(&path).expect("storage schema should initialize");
    let utc_midnight = 1_700_000_000_i64 - (1_700_000_000_i64 % 86_400);
    let bars = (0..10)
        .flat_map(|day| {
            (0..8).map(move |slot| {
                let close = 1_000.0 + (day * 8 + slot) as f64;
                let seconds = utc_midnight + day * 86_400 + slot * 900;
                StrategyBar {
                    time: seconds * 1_000_000_000,
                    open: close,
                    high: close + 10.0,
                    low: close - 10.0,
                    close,
                    volume: 1_000_000.0,
                }
            })
        })
        .collect::<Vec<_>>();
    let mut running = Vec::new();
    let mut potential = Vec::new();
    let mut stats = HashMap::new();

    append_stockbit_day_trade_research_plans(
        &connection,
        &mut running,
        &mut potential,
        &bars,
        "MEDC.JK",
        "MEDC",
        &mut stats,
    );

    assert!(running.is_empty());
    assert_eq!(potential.len(), 3);
    let strategy_ids = potential
        .iter()
        .map(|plan| {
            assert_eq!(plan["Exchange"], "STOCKBIT");
            assert_eq!(plan["Trade_Style"], "day_trade");
            assert_eq!(plan["Execution_Status"], "MONITOR_ONLY");
            assert_eq!(plan["Is_Watchlist"], true);
            plan["Signal_Strategy_ID"].as_str().unwrap_or_default()
        })
        .collect::<HashSet<_>>();
    assert_eq!(
        strategy_ids,
        HashSet::from([
            "idx_opening_range_momentum_dss_extreme_nwe_touch_v2",
            "idx_first_half_hour_momentum_dss_extreme_nwe_touch_v2",
            "idx_time_of_day_momentum_dss_extreme_nwe_touch_v2"
        ])
    );

    drop(connection);
    remove_temporary_database(&path);
}

#[test]
fn research_proxy_evidence_stays_observed_until_native_history_exists() {
    let mut stats = HashMap::new();
    stats.insert(
        backtest_key("XRP-USD", "crypto_futures"),
        serde_json::json!({
            "trades": 20,
            "sample_days": 365,
            "cagr_validated": true,
            "cagr_pct": 125.0,
            "win_rate_pct": 60.0,
            "profit_factor": 1.5,
            "evidence_source": "sqlite_local_history",
            "source_label": "yfinance fallback proxy",
            "execution_scope": "research_proxy"
        }),
    );
    let mut plan = serde_json::json!({
        "Symbol": "XRP-USD",
        "Asset": "XRP",
        "Class": "CRYPTO_FUT",
        "Exchange": "PINTU_FUTURES",
        "Action": "PINTU_FUTURES_DAY_TRADE",
        "State": "IN_ZONE"
    });

    assert!(!apply_backtest_gate(&mut plan, &stats));
    assert_eq!(plan["Backtest_Proof"]["status"], "REQUIRED");
    assert!(plan["Backtest_Proof"]["reason"]
        .as_str()
        .unwrap_or_default()
        .contains("venue-native"));
}

#[test]
fn empty_symbol_falls_back_to_asset_for_evidence_lookup() {
    let plan = serde_json::json!({
        "Symbol": "",
        "Asset": "MEDC",
        "Class": "IDX_STOCK",
        "Exchange": "STOCKBIT"
    });
    assert_eq!(plan_backtest_symbol(&plan), "MEDC");
}

#[test]
fn multibagger_requires_one_hundred_percent_in_a_complete_month() {
    let mut stats = HashMap::new();
    stats.insert(
        backtest_key("TRUK.JK", "idx"),
        serde_json::json!({
            "trades": 12,
            "sample_days": 730,
            "cagr_validated": true,
            "cagr_validation_status": "VALIDATED_UNTOUCHED_HOLDOUT",
            "holdout_cagr_pct": 12.0,
            "holdout_trades": 12,
            "holdout_sample_days": 730,
            "cagr_pct": 99.9,
            "max_calendar_month_return_pct": 98.0,
            "calendar_month_return_validated": true,
            "calendar_month_return_validation_status": "VALIDATED_UNTOUCHED_HOLDOUT",
            "holdout_max_calendar_month_return_pct": 99.9,
            "holdout_completed_calendar_months": 24,
            "win_rate_pct": 66.7,
            "profit_factor": 1.8,
            "execution_scope": "venue_history"
        }),
    );
    let mut plan = serde_json::json!({
        "Symbol": "TRUK.JK",
        "Asset": "TRUK",
        "Class": "IDX_STOCK",
        "Exchange": "STOCKBIT",
        "Strategy_ID": "multibagger",
        "Action": "MULTIBAGGER_CANDIDATE",
        "State": "IN_ZONE"
    });

    assert!(!apply_backtest_gate(&mut plan, &stats));
    assert_eq!(plan["Backtest_Proof"]["minimum_cagr_pct"], 0.0);
    assert_eq!(
        plan["Backtest_Proof"]["minimum_calendar_month_return_pct"],
        100.0
    );
    assert_eq!(plan["Backtest_Proof"]["observed_cagr_pct"], 99.9);
    assert_eq!(
        plan["Backtest_Proof"]["holdout_max_calendar_month_return_pct"],
        99.9
    );
    assert_eq!(plan["Backtest_Proof"]["status"], "REQUIRED");
    assert_eq!(plan["Action"], "MONITOR");
}

#[test]
fn registered_stockbit_multibagger_symbols_resolve() {
    assert!(is_multibagger_symbol("TRUK"));
    assert!(is_multibagger_symbol("TRUK.JK"));
    assert!(is_multibagger_symbol("AGAR"));
    assert!(is_multibagger_symbol("AGAR.JK"));
    assert!(!is_multibagger_symbol("MEDC.JK"));
}

#[test]
fn multibagger_discovery_keeps_target_gate_separate_from_potential_screen() {
    let potential = serde_json::json!({
        "freshness": "CURRENT",
        "sample_days": 730,
        "trades": 18,
        "profit_factor": 1.25,
        "win_rate_pct": 61.0,
        "calendar_month_return_validated": false,
        "max_calendar_month_return_pct": 35.0
    });
    let positive_screen = serde_json::json!({
        "one_year_return_pct": 48.0,
        "close_vs_sma_200_pct": 12.0
    });
    let (status, score, reason) = multibagger_candidate_assessment(&potential, &positive_screen);
    assert_eq!(status, "POTENTIAL_CANDIDATE");
    assert!(score > 0.0 && score < 100.0);
    assert!(reason.contains("100% calendar-month return goal is not validated"));

    let target = serde_json::json!({
        "freshness": "CURRENT",
        "sample_days": 730,
        "trades": 18,
        "profit_factor": 1.25,
        "win_rate_pct": 61.0,
        "calendar_month_return_validated": true,
        "calendar_month_return_validation_status": "VALIDATED_UNTOUCHED_HOLDOUT",
        "holdout_max_calendar_month_return_pct": 125.0,
        "holdout_completed_calendar_months": 24
    });
    let (target_status, _, _) = multibagger_candidate_assessment(&target, &positive_screen);
    assert_eq!(target_status, "MONTHLY_RETURN_TARGET_MET");
}

#[test]
fn research_selection_keeps_exchange_and_category_bound_together() {
    let stockbit = research_selection("stockbit", "idx", "multibagger")
        .expect("Stockbit multibagger should be a valid research selection");
    assert_eq!(stockbit.exchange, "stockbit");
    assert_eq!(stockbit.asset_class, "idx");
    assert_eq!(
        stockbit.strategy,
        "stockbit_multibagger_dss_extreme_nwe_touch_v2"
    );
    assert_eq!(stockbit.timeframe, "1d");

    let tsmom = research_selection("stockbit", "idx", "stockbit_tsmom_swing")
        .expect("Stockbit momentum swing should be a valid research selection");
    assert_eq!(tsmom.exchange, "stockbit");
    assert_eq!(tsmom.asset_class, "idx");
    assert_eq!(tsmom.profile_id, "stockbit_tsmom_swing");
    assert_eq!(
        tsmom.signal_strategy_id,
        "idx_time_series_momentum_dss_extreme_nwe_touch_v2"
    );
    assert_eq!(tsmom.timeframe, "1d");

    let pintu = research_selection("pintu", "crypto_futures", "pintu_futures_confluence")
        .expect("Pintu Futures confluence should be a valid research selection");
    assert_eq!(pintu.exchange, "pintu");
    assert_eq!(pintu.evidence_class, "crypto_futures");

    let tokenized = research_selection(
        "tokocrypto",
        "tokenized_stock",
        TOKENIZED_STOCK_CONFLUENCE_STRATEGY_ID,
    )
    .expect("Tokocrypto tokenized stocks should be a valid research selection");
    assert_eq!(tokenized.exchange, "tokocrypto");
    assert_eq!(tokenized.evidence_class, "crypto_spot");

    let spot = research_selection(
        "tokocrypto",
        "crypto_spot",
        TOKOCRYPTO_SPOT_CONFLUENCE_STRATEGY_ID,
    )
    .expect("Tokocrypto crypto spot should be a valid research selection");
    assert_eq!(spot.asset_class, "crypto_spot");
    assert_eq!(normalize_research_symbol(spot, "BTC").unwrap(), "BTC-USD");
}

#[test]
fn stockbit_day_trade_selector_exposes_all_registered_models() {
    for (profile, model, strategy) in [
        (
            "stockbit_orb_momentum",
            "idx_opening_range_momentum_dss_extreme_nwe_touch_v2",
            "idx_stockbit_opening_range_momentum_dss_extreme_nwe_touch_v2",
        ),
        (
            "stockbit_first_half_hour_momentum",
            "idx_first_half_hour_momentum_dss_extreme_nwe_touch_v2",
            "idx_stockbit_first_half_hour_momentum_dss_extreme_nwe_touch_v2",
        ),
        (
            "stockbit_time_of_day_momentum",
            "idx_time_of_day_momentum_dss_extreme_nwe_touch_v2",
            "idx_stockbit_time_of_day_momentum_dss_extreme_nwe_touch_v2",
        ),
        (
            "stockbit_day_trade",
            "idx_open_low_rvol_dss_extreme_nwe_touch_v2",
            "stockbit_day_trade_dss_extreme_nwe_touch_v2",
        ),
    ] {
        let selection = research_selection("stockbit", "idx_stock", profile)
            .unwrap_or_else(|_| panic!("{profile} should be selectable for Stockbit IDX"));
        assert_eq!(selection.profile_id, profile);
        assert_eq!(selection.signal_strategy_id, model);
        assert_eq!(selection.strategy, strategy);
        assert_eq!(selection.trade_style, "day_trade");
        assert_eq!(selection.timeframe, "15m");
    }
}

#[test]
fn research_selection_rejects_incompatible_app_mapping() {
    assert!(research_selection("stockbit", "crypto_futures", "pintu").is_err());
    assert!(research_selection("tokocrypto", "crypto_futures", "pintu").is_err());
    assert!(research_selection("unknown", "idx", "stockbit").is_err());

    let tokocrypto = research_selection(
        "tokocrypto",
        "tokenized_stock",
        TOKENIZED_STOCK_CONFLUENCE_STRATEGY_ID,
    )
    .unwrap();
    assert!(normalize_research_symbol(tokocrypto, "BTC-USD").is_err());
    assert_eq!(
        normalize_research_symbol(tokocrypto, "TSLA").unwrap(),
        "TSLA"
    );

    let spot = research_selection(
        "tokocrypto",
        "crypto_spot",
        TOKOCRYPTO_SPOT_CONFLUENCE_STRATEGY_ID,
    )
    .unwrap();
    assert!(normalize_research_symbol(spot, "TSLA").is_err());
}
