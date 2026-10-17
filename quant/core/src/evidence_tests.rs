use super::*;

#[test]
fn historical_trade_fees_use_each_side_transaction_notional() {
    let long_return = net_return_pct(100.0, 110.0, "LONG", 0.15, 0.25);
    let short_return = net_return_pct(100.0, 90.0, "SHORT", 0.15, 0.25);
    let long_loss = net_return_pct(100.0, 90.0, "LONG", 0.15, 0.25);
    let short_loss = net_return_pct(100.0, 110.0, "SHORT", 0.15, 0.25);

    assert!((long_return - 9.575).abs() < 1e-12);
    assert!((short_return - 9.615).abs() < 1e-12);
    assert!((long_loss - (-10.375)).abs() < 1e-12);
    assert!((short_loss - (-10.415)).abs() < 1e-12);
}

#[test]
fn open_position_calendar_equity_marks_include_both_transaction_fees() {
    let bars = [
        (1_705_276_800_i64, 100.0),
        (1_707_955_200_i64, 100.0),
        (1_710_460_800_i64, 100.0),
    ]
    .into_iter()
    .map(|(time, close)| StrategyBar {
        time: time * 1_000_000_000,
        open: close,
        high: close,
        low: close,
        close,
        volume: 1.0,
    })
    .collect::<Vec<_>>();
    let short_trade = EvaluatedTrade {
        return_pct: -0.4,
        stop_risk_fraction: 0.02,
        entry_index: 1,
        exit_index: 2,
        entry_price: 100.0,
        exit_price: 100.0,
        is_long: false,
    };

    let result = calculate_performance(&bars, &[short_trade], "test", 0.15, 0.25, false);

    let february_return_pct = result["max_calendar_month_return_pct"]
        .as_f64()
        .expect("the middle calendar month should be complete");
    assert!(
        (february_return_pct - (-0.1)).abs() < 1e-12,
        "expected both short transaction fees; got {february_return_pct}"
    );

    let long_trade = EvaluatedTrade {
        return_pct: -0.4,
        stop_risk_fraction: 0.02,
        entry_index: 1,
        exit_index: 2,
        entry_price: 100.0,
        exit_price: 100.0,
        is_long: true,
    };
    let long_result = calculate_performance(&bars, &[long_trade], "test", 0.15, 0.25, false);
    let long_february_return_pct = long_result["max_calendar_month_return_pct"]
        .as_f64()
        .expect("the long middle calendar month should be complete");
    assert!((long_february_return_pct - (-0.1)).abs() < 1e-12);
}

#[test]
fn day_trade_quality_grade_uses_the_50_percent_user_floor() {
    assert_eq!(
        day_trade_minimum_win_rate_pct(Some(1.49)),
        MIN_DAY_TRADE_WIN_RATE_PCT
    );
    assert_eq!(
        day_trade_minimum_win_rate_pct(Some(1.5)),
        MIN_ASYMMETRIC_DAY_TRADE_WIN_RATE_PCT
    );
    assert_eq!(quality_grade(10, 49.9, 2.0, 50.0), "C");
    assert_eq!(
        quality_grade(10, 50.0, 1.49, MIN_DAY_TRADE_WIN_RATE_PCT),
        "B"
    );
    assert_eq!(
        quality_grade(10, 50.0, 1.5, MIN_DAY_TRADE_WIN_RATE_PCT),
        "A"
    );
}

#[test]
fn insufficient_history_is_explicit() {
    let bars = vec![StrategyBar {
        time: 1_000_000_000,
        open: 100.0,
        high: 101.0,
        low: 99.0,
        close: 100.0,
        volume: 1.0,
    }];
    let result = evaluate_historical_evidence(&bars, "idx", 25, 0.2);
    assert_eq!(result["evidence_status"], "INSUFFICIENT_HISTORY");
    assert_eq!(result["cagr_validated"], false);
}

#[test]
fn unsupported_strategy_does_not_create_evidence() {
    let bars = vec![
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
            high: 101.0,
            low: 99.0,
            close: 100.0,
            volume: 1.0,
        },
    ];
    let result = evaluate_historical_evidence(&bars, "unknown", 25, 0.2);
    assert_eq!(result["evidence_status"], "UNSUPPORTED_STRATEGY");
    assert_eq!(result["trades"], 0);
}

#[test]
fn required_signal_confirmations_receive_a_new_evidence_identity() {
    let profile = historical_evidence_profile("idx_stockbit_day_trade")
        .expect("registered Stockbit evidence profile should resolve");
    assert_eq!(
        profile.strategy_id,
        "stockbit_day_trade_dss_extreme_nwe_touch_v2"
    );
    let versioned_profile = historical_evidence_profile(profile.strategy_id)
        .expect("the new evidence identity should resolve directly");
    assert_eq!(versioned_profile.strategy_id, profile.strategy_id);
    assert!(profile
        .strategy_id
        .ends_with(REQUIRED_SIGNAL_CONFIRMATION_EVIDENCE_SUFFIX));
}

#[test]
fn confirmation_version_keeps_the_registered_day_trade_validation_floor() {
    let bars = [
        StrategyBar {
            time: 1,
            open: 100.0,
            high: 101.0,
            low: 99.0,
            close: 100.0,
            volume: 1_000.0,
        },
        StrategyBar {
            time: 2,
            open: 100.0,
            high: 101.0,
            low: 99.0,
            close: 100.0,
            volume: 1_000.0,
        },
    ];
    for strategy in [
        "idx_stockbit_day_trade",
        "pintu_futures_day_trade",
        "tokocrypto_spot_day_trade",
        "tokenized_stock_day_trade",
    ] {
        let result = evaluate_historical_evidence(&bars, strategy, 25, 0.2);
        assert_eq!(result["minimum_win_rate_pct"], 50.0, "{strategy}");
        assert!(result["strategy"]
            .as_str()
            .unwrap_or_default()
            .ends_with(REQUIRED_SIGNAL_CONFIRMATION_EVIDENCE_SUFFIX));
    }
}

#[test]
fn daily_tsmom_waits_for_required_nadaraya_history_before_recording_trades() {
    let bars = (0..320)
        .map(|index| {
            let close = 100.0 + index as f64;
            StrategyBar {
                time: (1_700_000_000_i64 + index as i64 * 86_400) * 1_000_000_000,
                open: close,
                high: close + 1.0,
                low: close - 1.0,
                close,
                volume: 1_000_000.0,
            }
        })
        .collect::<Vec<_>>();

    let result =
        evaluate_historical_evidence(&bars, "idx_stockbit_time_series_momentum_swing", 63, 0.25);

    assert_ne!(result["evidence_status"], "UNSUPPORTED_STRATEGY");
    assert_eq!(result["trades"].as_u64().unwrap_or(0), 0);
    assert_eq!(result["cagr_validated"], false);
    assert_eq!(result["execution_model"], "next_bar_open_stop_first");
}

#[test]
fn invented_quality_strategy_is_backtestable_without_becoming_validated() {
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
    let result = evaluate_historical_evidence(&bars, "idx_stockbit_quality_day_trade_v1", 25, 0.25);
    assert_eq!(
        result["strategy"],
        "idx_stockbit_quality_day_trade_v1_dss_extreme_nwe_touch_v2"
    );
    assert_ne!(result["evidence_status"], "UNSUPPORTED_STRATEGY");
    assert_eq!(result["cagr_validated"], false);
}

#[test]
fn stockbit_momentum_variants_have_independent_evidence_routes() {
    let bars = [
        StrategyBar {
            time: 1_790_649_000_i64 * 1_000_000_000,
            open: 1_000.0,
            high: 1_010.0,
            low: 995.0,
            close: 1_005.0,
            volume: 500_000.0,
        },
        StrategyBar {
            time: 1_790_649_900_i64 * 1_000_000_000,
            open: 1_005.0,
            high: 1_010.0,
            low: 1_000.0,
            close: 1_005.0,
            volume: 100_000.0,
        },
    ];

    for (input, canonical) in [
        (
            "stockbit_first_half_hour_momentum",
            "idx_stockbit_first_half_hour_momentum",
        ),
        (
            "stockbit_time_of_day_momentum",
            "idx_stockbit_time_of_day_momentum",
        ),
    ] {
        let result = evaluate_historical_evidence_with_fees(&bars, input, 25, 0.15, 0.25);
        assert_eq!(
            result["strategy"],
            format!("{canonical}_dss_extreme_nwe_touch_v2")
        );
        assert_eq!(result["evidence_status"], "INSUFFICIENT_HISTORY");
        assert_eq!(result["minimum_win_rate_pct"], 50.0);
        assert_eq!(result["buy_fee_pct"], 0.15);
        assert_eq!(result["sell_fee_pct"], 0.25);
    }
}

#[test]
fn historical_profile_registry_keeps_aliases_and_evidence_identities_explicit() {
    let stockbit_alias = historical_evidence_profile("  IDX ").expect("Stockbit alias");
    let stockbit_canonical =
        historical_evidence_profile("idx_stockbit_day_trade").expect("Stockbit route");
    assert_eq!(stockbit_alias, stockbit_canonical);
    assert_eq!(
        stockbit_alias.strategy_id,
        "stockbit_day_trade_dss_extreme_nwe_touch_v2"
    );
    assert_eq!(stockbit_alias.evidence_class, "idx");
    assert_eq!(stockbit_alias.history_interval, "15m");
    assert_eq!(stockbit_alias.max_hold_bars, 25);
    assert_eq!(stockbit_alias.buy_fee_pct, 0.15);
    assert_eq!(stockbit_alias.sell_fee_pct, 0.25);

    let spot = historical_evidence_profile("tokocrypto_spot").expect("spot route");
    let tokenized = historical_evidence_profile("tokenized_stock").expect("tokenized route");
    assert_eq!(spot.evidence_class, tokenized.evidence_class);
    assert_ne!(spot.strategy_id, tokenized.strategy_id);
    assert_ne!(spot.fee_model, tokenized.fee_model);
    assert_eq!(spot.buy_fee_pct, 0.0);
    assert_eq!(tokenized.sell_fee_pct, 0.0);

    let previous_spot =
        historical_evidence_profile("tokocrypto_spot_day_trade").expect("previous spot identity");
    let confluence_spot = historical_evidence_profile("tokocrypto_spot_confluence_2_of_4_v1")
        .expect("versioned spot confluence identity");
    let previous_tokenized = historical_evidence_profile("tokenized_stock_day_trade")
        .expect("previous tokenized-stock identity");
    let confluence_tokenized = historical_evidence_profile("tokenized_stock_confluence_2_of_4_v1")
        .expect("versioned tokenized-stock confluence identity");
    assert_ne!(previous_spot.strategy_id, confluence_spot.strategy_id);
    assert_ne!(
        previous_tokenized.strategy_id,
        confluence_tokenized.strategy_id
    );

    assert_eq!(
        historical_evidence_profile("idx_stockbit_quality_day_trade_v1"),
        None,
        "research variants keep their existing separate evidence route"
    );
    assert_eq!(
        historical_evidence_profile("pintu_futures_golden_cross_v2_3r"),
        None
    );
}

#[test]
fn evidence_promotion_targets_come_from_the_resolved_strategy_identity() {
    assert_eq!(
        historical_evidence_minimum_cagr_pct("stockbit_swing"),
        MIN_SWING_CAGR_PCT
    );
    assert_eq!(
        historical_evidence_minimum_cagr_pct("idx_stockbit_quality_day_trade_v1"),
        MIN_STOCKBIT_DAY_TRADE_CAGR_PCT
    );
    assert_eq!(historical_evidence_minimum_cagr_pct("pintu"), 0.0);
    assert_eq!(
        historical_evidence_minimum_calendar_month_return_pct("multibagger"),
        MIN_MULTIBAGGER_CALENDAR_MONTH_RETURN_PCT
    );
    assert_eq!(
        historical_evidence_minimum_calendar_month_return_pct("stockbit_swing"),
        0.0
    );
}
