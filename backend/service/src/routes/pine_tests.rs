use super::{
    backtest_curve_sample_indexes, buy_and_hold_benchmark, summarize_chronological_validation,
    summarize_close_drawdown, summarize_monthly_marked_returns, summarize_strategy_trades,
    summarize_trade_return_distribution, utc_month_key, wilson_interval_95_pct,
    MAX_BACKTEST_CURVE_POINTS,
};
use quant_pine_runtime::StrategyTrade;

fn trade(
    direction: &str,
    gross_pnl: f64,
    net_pnl: f64,
    gross_return: f64,
    net_return: f64,
) -> StrategyTrade {
    StrategyTrade {
        entry_id: "entry".to_string(),
        direction: direction.to_string(),
        entry_bar: 1,
        exit_bar: 2,
        entry_time: Some(1),
        exit_time: Some(2),
        entry_price: 10.0,
        exit_price: 11.0,
        quantity: 1.0,
        gross_return_pct: gross_return,
        net_return_pct: net_return,
        gross_pnl,
        net_pnl,
        max_runup: 0.0,
        max_drawdown: 0.0,
        max_runup_percent: 0.0,
        max_drawdown_percent: 0.0,
    }
}

#[test]
fn buy_and_hold_benchmark_scales_the_same_sample_to_initial_capital() {
    let (return_pct, start_close) = buy_and_hold_benchmark(1_000.0, &[10.0, 12.0, 15.0])
        .expect("valid closes should produce a same-sample benchmark");

    assert_eq!(return_pct, 50.0);
    assert_eq!(start_close, 10.0);
}

#[test]
fn deep_backtest_curve_sampling_is_bounded_and_keeps_sample_endpoints() {
    let indexes = backtest_curve_sample_indexes(MAX_BACKTEST_CURVE_POINTS * 20);
    assert_eq!(indexes.len(), MAX_BACKTEST_CURVE_POINTS);
    assert_eq!(indexes.first(), Some(&0));
    assert_eq!(indexes.last(), Some(&(MAX_BACKTEST_CURVE_POINTS * 20 - 1)));
    assert!(indexes.windows(2).all(|pair| pair[0] < pair[1]));
}

#[test]
fn buy_and_hold_benchmark_reports_negative_and_flat_samples() {
    let (falling_return, falling_start) = buy_and_hold_benchmark(100.0, &[50.0, 40.0])
        .expect("falling prices should produce a negative price return");
    let (flat_return, flat_start) = buy_and_hold_benchmark(100.0, &[50.0, 50.0])
        .expect("flat prices should produce a flat reference");

    assert!((falling_return + 20.0).abs() < 1e-12);
    assert_eq!(falling_start, 50.0);
    assert_eq!(flat_return, 0.0);
    assert_eq!(flat_start, 50.0);
}

#[test]
fn buy_and_hold_benchmark_rejects_missing_or_invalid_inputs() {
    assert!(buy_and_hold_benchmark(100.0, &[]).is_none());
    assert!(buy_and_hold_benchmark(0.0, &[10.0, 12.0]).is_none());
    assert!(buy_and_hold_benchmark(100.0, &[10.0, f64::NAN]).is_none());
    assert!(buy_and_hold_benchmark(100.0, &[10.0, f64::INFINITY]).is_none());
    assert!(buy_and_hold_benchmark(100.0, &[10.0, 0.0]).is_none());
}

#[test]
fn strategy_trade_summary_uses_full_sample_and_selected_cost_basis() {
    let trades = vec![
        trade("LONG", 12.0, 10.0, 12.0, 10.0),
        trade("LONG", -3.0, -5.0, -3.0, -5.0),
        trade("SHORT", 5.0, 4.0, 10.0, 8.0),
        trade("SHORT", 0.0, 0.0, 0.0, 0.0),
    ];

    let all = summarize_strategy_trades(&trades, None, true);
    assert_eq!(all["trades"], 4);
    assert_eq!(all["wins"], 2);
    assert_eq!(all["losses"], 1);
    assert_eq!(all["breakeven"], 1);
    assert_eq!(all["win_rate_pct"], 50.0);
    assert_eq!(all["largest_win_pnl"], 10.0);
    assert_eq!(all["largest_loss_pnl"], -5.0);

    let long = summarize_strategy_trades(&trades, Some("LONG"), true);
    assert_eq!(long["trades"], 2);
    assert_eq!(long["average_win_return_pct"], 10.0);
    assert_eq!(long["average_loss_return_pct"], -5.0);

    let short_gross = summarize_strategy_trades(&trades, Some("SHORT"), false);
    assert_eq!(short_gross["trades"], 2);
    assert_eq!(short_gross["wins"], 1);
    assert_eq!(short_gross["breakeven"], 1);
    assert_eq!(short_gross["average_win_return_pct"], 10.0);
    assert!(short_gross["average_loss_return_pct"].is_null());
}

#[test]
fn close_drawdown_tracks_peak_trough_duration_recovery_and_current_state() {
    let times = [10, 20, 30, 40, 50, 60, 70, 80];
    let analysis = summarize_close_drawdown(
        100.0,
        &[98.0, 95.0, 97.0, 100.0, 102.0, 96.0, 90.0, 99.0],
        &times,
    );

    assert_eq!(analysis["available"], true);
    assert!(
        (analysis["max_drawdown_pct"].as_f64().unwrap() - (12.0 / 102.0 * 100.0)).abs() < 1e-12
    );
    assert_eq!(analysis["max_drawdown_amount"], 12.0);
    assert_eq!(analysis["peak_time"], 50);
    assert_eq!(analysis["trough_time"], 70);
    assert!(analysis["recovery_time"].is_null());
    assert_eq!(analysis["duration_bars"], 2);
    assert!(analysis["recovery_bars"].is_null());
    assert_eq!(analysis["max_period_duration_bars"], 3);
    assert_eq!(analysis["completed_drawdown_periods"], 1);
    assert_eq!(analysis["average_recovered_drawdown_duration_bars"], 3.0);
    assert_eq!(analysis["current_duration_bars"], 3);
    assert!(
        (analysis["current_drawdown_pct"].as_f64().unwrap() - (3.0 / 102.0 * 100.0)).abs() < 1e-12
    );
    assert_eq!(analysis["series"].as_array().unwrap().len(), times.len());
}

#[test]
fn close_drawdown_reports_recovery_and_rejects_unaligned_inputs() {
    let recovered = summarize_close_drawdown(100.0, &[100.0, 90.0, 100.0], &[1, 2, 3]);
    assert_eq!(recovered["recovery_time"], 3);
    assert_eq!(recovered["recovery_bars"], 1);
    assert_eq!(recovered["max_period_duration_bars"], 2);
    assert_eq!(recovered["average_recovered_drawdown_duration_bars"], 2.0);
    assert!(summarize_close_drawdown(100.0, &[99.0], &[])["available"].as_bool() == Some(false));
    assert_eq!(
        summarize_close_drawdown(100.0, &[99.0, f64::NAN], &[1, 2])["available"],
        false
    );
}

#[test]
fn chronological_validation_restarts_flat_and_keeps_warmup_trades_out_of_tail() {
    use quant_pine_runtime::run_strategy_with_history_limit_and_magnifier_and_tick_size;
    use std::collections::HashMap;

    let source = "//@version=6\nstrategy(\"Tail split\", default_qty_type=strategy.fixed, default_qty_value=1)\nplot(ta.sma(close, 3))\nif bar_index == 1\n    strategy.entry(\"training\", strategy.long)\nif bar_index == 2\n    strategy.close_all(immediately=true)\nif bar_index == 4\n    strategy.entry(\"validation\", strategy.long)\nif bar_index == 5\n    strategy.close_all(immediately=true)";
    let closes = [10.0, 11.0, 12.0, 13.0, 14.0, 15.0];
    let times = closes
        .iter()
        .enumerate()
        .map(|(index, _)| 1_700_000_000_000_000_000_i64 + index as i64 * 60_000_000_000)
        .collect::<Vec<_>>();
    let candles = closes
        .iter()
        .enumerate()
        .map(|(index, close)| super::super::Candle {
            time: times[index],
            open: *close,
            high: *close + 1.0,
            low: *close - 1.0,
            close: *close,
            volume: 1.0,
            dss: None,
            trigger: None,
            gradient: None,
            accel: None,
            indicators: None,
        })
        .collect::<Vec<_>>();
    let runtime_candles = candles
        .iter()
        .map(|candle| quant_pine_runtime::Candle {
            open: candle.open,
            high: candle.high,
            low: candle.low,
            close: candle.close,
            volume: candle.volume,
            time: Some(candle.time),
        })
        .collect::<Vec<_>>();
    let empty_timeframes = HashMap::new();
    let full = run_strategy_with_history_limit_and_magnifier_and_tick_size(
        source,
        &runtime_candles,
        "1h",
        &empty_timeframes,
        &[],
        None,
        runtime_candles.len(),
        0.0,
        0.0,
        &HashMap::new(),
        None,
    )
    .expect("full sample strategy should run");

    let result = summarize_chronological_validation(
        source,
        50,
        &candles,
        &runtime_candles,
        "1h",
        &empty_timeframes,
        &[],
        None,
        runtime_candles.len(),
        0.0,
        0.0,
        &HashMap::new(),
        None,
        true,
        &full,
        None,
    );

    assert_eq!(result["available"], true);
    assert_eq!(result["training"]["closed_trades"], 1);
    assert_eq!(result["training"]["cross_boundary_trades_excluded"], 0);
    assert_eq!(result["validation"]["closed_trades"], 1);
    assert_eq!(result["validation"]["marked_return_pct"], 0.0);
    assert!(result["warning"]
        .as_str()
        .unwrap()
        .contains("single chronological tail check"));
}

#[test]
fn chronological_validation_reports_short_samples_as_unavailable() {
    use quant_pine_runtime::run_strategy_with_history_limit_and_magnifier_and_tick_size;
    use std::collections::HashMap;

    let source = "//@version=6\nstrategy(\"Short sample\")";
    let candle = super::super::Candle {
        time: 1,
        open: 10.0,
        high: 11.0,
        low: 9.0,
        close: 10.0,
        volume: 1.0,
        dss: None,
        trigger: None,
        gradient: None,
        accel: None,
        indicators: None,
    };
    let runtime = quant_pine_runtime::Candle {
        open: 10.0,
        high: 11.0,
        low: 9.0,
        close: 10.0,
        volume: 1.0,
        time: Some(1),
    };
    let timeframes = HashMap::new();
    let full = run_strategy_with_history_limit_and_magnifier_and_tick_size(
        source,
        &[runtime.clone()],
        "1h",
        &timeframes,
        &[],
        None,
        1,
        0.0,
        0.0,
        &HashMap::new(),
        None,
    )
    .expect("single-bar source should run without a split");
    let result = summarize_chronological_validation(
        source,
        70,
        &[candle],
        &[runtime],
        "1h",
        &timeframes,
        &[],
        None,
        1,
        0.0,
        0.0,
        &HashMap::new(),
        None,
        true,
        &full,
        None,
    );
    assert_eq!(result["available"], false);
    assert!(result["reason"].as_str().unwrap().contains("too short"));
}

#[test]
fn wilson_win_rate_interval_is_bounded_and_marks_small_samples_uncertain() {
    let interval = wilson_interval_95_pct(5, 10).expect("a nonempty valid sample gets an interval");
    assert!((interval[0] - 23.66).abs() < 0.02);
    assert!((interval[1] - 76.34).abs() < 0.02);
    let zero_win_interval =
        wilson_interval_95_pct(0, 10).expect("zero-win samples still get an interval");
    assert_eq!(zero_win_interval[0], 0.0);
    assert!((zero_win_interval[1] - 27.75).abs() < 0.01);
    assert_eq!(wilson_interval_95_pct(1, 0), None);
    assert_eq!(wilson_interval_95_pct(2, 1), None);
}

#[test]
fn monthly_marked_returns_use_each_run_equity_and_utc_calendar_boundaries() {
    let nov_30_utc = 1_732_966_800_000_000_000_i64;
    let dec_1_utc = 1_733_059_800_000_000_000_i64;
    let jan_31_utc = 1_738_358_400_000_000_000_i64;
    let feb_1_utc = 1_738_444_800_000_000_000_i64;
    assert_eq!(utc_month_key(nov_30_utc), "2024-11");
    assert_eq!(utc_month_key(dec_1_utc), "2024-12");
    assert_eq!(utc_month_key(jan_31_utc), "2025-01");
    assert_eq!(utc_month_key(feb_1_utc), "2025-02");

    let training =
        summarize_monthly_marked_returns(100.0, &[110.0, 120.0], &[nov_30_utc, dec_1_utc]);
    let validation =
        summarize_monthly_marked_returns(100.0, &[101.0, 98.0], &[jan_31_utc, feb_1_utc]);

    assert!((training[0]["return_pct"].as_f64().unwrap() - 10.0).abs() < 1e-12);
    assert!((validation[0]["return_pct"].as_f64().unwrap() - 1.0).abs() < 1e-12);
    assert!(
        (validation[1]["return_pct"].as_f64().unwrap() - (98.0 / 101.0 - 1.0) * 100.0).abs()
            < 1e-12
    );
    assert_ne!(training[1]["month"], validation[0]["month"]);
}

#[test]
fn trade_return_distribution_uses_full_sample_equal_width_bins_and_cost_basis() {
    let returns = [-10.0, -5.0, 0.0, 1.0, 2.0, 2.0, 10.0];
    let trades = returns
        .iter()
        .map(|value| trade("LONG", *value, *value, *value, *value))
        .collect::<Vec<_>>();
    let distribution = summarize_trade_return_distribution(&trades, true);

    assert_eq!(distribution["available"], true);
    assert_eq!(distribution["sample_trades"], 7);
    assert_eq!(distribution["basis"], "net after configured costs");
    assert_eq!(distribution["bins"].as_array().unwrap().len(), 3);
    assert_eq!(distribution["bins"][0]["count"], 2);
    assert_eq!(distribution["bins"][1]["count"], 4);
    assert_eq!(distribution["bins"][2]["count"], 1);
    assert_eq!(distribution["bins"][2]["includes_upper_bound"], true);
}

#[test]
fn trade_return_distribution_handles_constant_empty_and_unsupported_ranges() {
    let constant_trades = [
        trade("SHORT", 0.0, 0.0, 1.5, 1.5),
        trade("SHORT", 0.0, 0.0, 1.5, 1.5),
    ];
    let constant = summarize_trade_return_distribution(&constant_trades, false);
    assert_eq!(constant["bins"].as_array().unwrap().len(), 1);
    assert_eq!(constant["bins"][0]["count"], 2);
    assert_eq!(constant["basis"], "gross; venue fees unknown");

    assert_eq!(
        summarize_trade_return_distribution(&[], false)["available"],
        false
    );
    let extremes = [f64::MAX, -f64::MAX].map(|value| trade("LONG", value, value, value, value));
    let overflow = summarize_trade_return_distribution(&extremes, false);
    assert_eq!(overflow["available"], false);
    assert_eq!(overflow["sample_trades"], 2);
}

#[test]
fn strategy_trade_summary_reports_directional_full_sample_excursions() {
    let mut long_win = trade("LONG", 8.0, 7.0, 8.0, 7.0);
    long_win.max_runup = 12.0;
    long_win.max_drawdown = 3.0;
    long_win.max_runup_percent = 6.0;
    long_win.max_drawdown_percent = -1.5;
    let mut long_loss = trade("LONG", -4.0, -5.0, -4.0, -5.0);
    long_loss.max_runup = 4.0;
    long_loss.max_drawdown = 9.0;
    long_loss.max_runup_percent = 2.0;
    long_loss.max_drawdown_percent = -4.5;
    let short = trade("SHORT", 2.0, 1.0, 2.0, 1.0);

    let all = summarize_strategy_trades(&[long_win.clone(), long_loss, short.clone()], None, true);
    assert_eq!(all["max_favorable_excursion"]["average"], 16.0 / 3.0);
    assert_eq!(all["max_adverse_excursion"]["average"], 4.0);
    assert_eq!(all["favorable_excursion_pct"]["average"], 8.0 / 3.0);
    assert_eq!(all["adverse_excursion_pct"]["average"], -2.0);
    assert_eq!(all["max_favorable_excursion"]["sample_trades"], 3);

    let longs = summarize_strategy_trades(&[long_win, short], Some("LONG"), false);
    assert_eq!(longs["max_favorable_excursion"]["average"], 12.0);
    assert_eq!(longs["max_adverse_excursion"]["maximum"], 3.0);
    assert_eq!(longs["max_favorable_excursion"]["sample_trades"], 1);
}
