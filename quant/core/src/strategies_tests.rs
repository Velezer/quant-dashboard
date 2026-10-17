use super::common::idx_min_price_at;
use super::*;

fn bar(index: usize, close: f64, volume: f64) -> StrategyBar {
    StrategyBar {
        time: (1_750_000_000_i64 + index as i64 * 900) * 1_000_000_000,
        open: close,
        high: close + 1.0,
        low: close - 1.0,
        close,
        volume,
    }
}

#[test]
fn pintu_returns_one_signal_per_bar_and_is_neutral_during_warmup() {
    let bars: Vec<_> = (0..30).map(|index| bar(index, 100.0, 1.0)).collect();
    let signals = generate_pintu_signals(&bars, PintuStrategyConfig::default());
    assert_eq!(signals.len(), bars.len());
    assert!(signals[..20]
        .iter()
        .all(|signal| signal.direction == "NEUTRAL"));
}

#[test]
fn research_candidate_configurations_keep_their_versioned_parameters() {
    let quality = IdxStrategyConfig::quality_day_trade_v1();
    assert_eq!(quality.open_equal_low_tolerance, 0.003);
    assert_eq!(quality.min_rvol, 2.0);
    assert_eq!(quality.tp1_pct, 0.014);
    assert_eq!(quality.sl_pct, 0.010);

    let pullback = IdxStrategyConfig::pullback_day_trade_v1();
    assert_eq!(pullback.open_equal_low_tolerance, 0.006);
    assert_eq!(pullback.min_rvol, 1.7);
    assert_eq!(pullback.tp1_pct, 0.012);
    assert_eq!(pullback.sl_pct, 0.009);

    let momentum = IdxStrategyConfig::momentum_day_trade_v1();
    assert_eq!(momentum.min_rvol, 2.5);
    assert_eq!(momentum.min_ticks_to_ara, 3);
    assert_eq!(momentum.tp2_pct, 0.032);

    let pintu = PintuStrategyConfig::quality_day_trade_v1();
    assert_eq!(pintu.min_rvol, 2.2);
    assert_eq!(pintu.volume_ma_period, 24);
    assert_eq!(pintu.sl_underlying_pct, 0.008);

    let confluence = PintuConfluenceConfig::quality_day_trade_v1();
    assert_eq!(confluence.volume_ratio, 2.2);
    assert_eq!(confluence.tp_pct, 0.012);
    assert_eq!(confluence.sl_pct, 0.008);
}

#[test]
fn tsmom_is_neutral_through_warmup_then_long_in_a_rising_daily_series() {
    let bars: Vec<_> = (0..300)
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
        .collect();
    let signals = super::idx::generate_idx_time_series_momentum_signals_unconfirmed(
        &bars,
        IdxTimeSeriesMomentumConfig::default(),
    );

    assert!(signals[..252]
        .iter()
        .all(|signal| signal.direction == "NEUTRAL"));
    assert_eq!(signals[252].direction, "LONG");
    assert!(signals[252].suggested_sl < bars[252].close);
    assert!(signals[252].suggested_tp1 > bars[252].close);
}

#[test]
fn tsmom_marks_a_trend_exit_when_price_falls_below_the_sma() {
    let mut bars: Vec<_> = (0..270)
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
        .collect();
    let index = bars.len() - 1;
    bars[index] = StrategyBar {
        time: bars[index].time,
        open: 110.0,
        high: 111.0,
        low: 99.0,
        close: 100.0,
        volume: 1_000_000.0,
    };
    let signals =
        generate_idx_time_series_momentum_signals(&bars, IdxTimeSeriesMomentumConfig::default());

    assert_eq!(signals[index].direction, "NEUTRAL");
    assert_eq!(
        signals[index].exit_signal.as_deref(),
        Some("TSMOM_TREND_EXIT")
    );
}

#[test]
fn stockbit_cutoff_is_detected_in_wib() {
    let utc_day = 1_750_000_000_i64.div_euclid(86_400);
    let timestamp = (utc_day * 86_400 + 15 * 3_600 + 41 * 60 - 7 * 3_600) * 1_000_000_000;
    assert!(stockbit_cutoff_due(timestamp));
}

#[test]
fn idx_minimum_price_changes_at_the_jakarta_effective_time() {
    let effective = 1_790_614_800_i64 * 1_000_000_000;
    assert_eq!(idx_min_price_at(effective - 1), 50.0);
    assert_eq!(idx_min_price_at(effective), 1.0);
    assert_eq!(idx_min_price_at(1_798_736_400_i64 * 1_000_000_000), 1.0);

    let (ara_2026, arb_2026, _) = idx_ara_arb(1_000.0, 1_790_614_800_i64 * 1_000_000_000);
    assert_eq!((ara_2026, arb_2026), (1_250.0, 850.0));
    let (ara_2027, arb_2027, _) = idx_ara_arb(1_000.0, 1_798_736_400_i64 * 1_000_000_000);
    assert_eq!((ara_2027, arb_2027), (1_250.0, 750.0));
}

#[test]
fn stockbit_orb_needs_momentum_breakout_rvol_and_turnover() {
    let mut bars = Vec::new();
    // Monday's stored bars provide a prior close and volume observations.
    for index in 0..7 {
        bars.push(StrategyBar {
            time: (1_790_560_800_i64 + index * 900) * 1_000_000_000,
            open: 1_000.0,
            high: 1_005.0,
            low: 995.0,
            close: 1_000.0,
            volume: 100_000.0,
        });
    }
    // Tuesday's 09:00 and 09:15 WIB bars define the opening range.
    for (seconds, high, low, close) in [
        (1_790_647_200_i64, 1_005.0, 1_000.0, 1_002.0),
        (1_790_648_100_i64, 1_010.0, 1_000.0, 1_005.0),
    ] {
        bars.push(StrategyBar {
            time: seconds * 1_000_000_000,
            open: 1_000.0,
            high,
            low,
            close,
            volume: 100_000.0,
        });
    }
    // A 09:30 breakout candle has both the required turnover and RVOL.
    bars.push(StrategyBar {
        time: 1_790_649_000_i64 * 1_000_000_000,
        open: 1_005.0,
        high: 1_025.0,
        low: 1_015.0,
        close: 1_020.0,
        volume: 500_000.0,
    });
    bars.push(StrategyBar {
        time: 1_790_649_900_i64 * 1_000_000_000,
        open: 1_020.0,
        high: 1_025.0,
        low: 1_015.0,
        close: 1_022.0,
        volume: 100_000.0,
    });

    let signals = super::idx::generate_idx_opening_range_signals_unconfirmed(
        &bars,
        IdxOpeningRangeConfig::default(),
    );
    assert_eq!(signals[9].direction, "LONG");
    assert_eq!(signals[9].regime, "STOCKBIT_ORB_INTRADAY_MOMENTUM_LONG");
    assert_eq!(signals[9].suggested_sl, 1_000.0);
    assert_eq!(signals[9].suggested_tp1, 1_050.0);
    assert_eq!(signals[9].suggested_tp2, 1_070.0);
    assert_eq!(signals[10].direction, "NEUTRAL");
}

#[test]
fn stockbit_first_half_hour_momentum_uses_positive_opening_return_without_breakout() {
    let mut bars = (0..7)
        .map(|index| StrategyBar {
            time: (1_790_560_800_i64 + index * 900) * 1_000_000_000,
            open: 1_000.0,
            high: 1_005.0,
            low: 995.0,
            close: 1_000.0,
            volume: 100_000.0,
        })
        .collect::<Vec<_>>();
    bars.extend([
        StrategyBar {
            time: 1_790_647_200_i64 * 1_000_000_000,
            open: 1_000.0,
            high: 1_005.0,
            low: 999.0,
            close: 1_000.0,
            volume: 100_000.0,
        },
        StrategyBar {
            time: 1_790_648_100_i64 * 1_000_000_000,
            open: 1_000.0,
            high: 1_010.0,
            low: 999.0,
            close: 1_005.0,
            volume: 500_000.0,
        },
    ]);

    let signals = super::idx::generate_idx_first_half_hour_momentum_signals_unconfirmed(
        &bars,
        IdxIntradayMomentumConfig::default(),
    );

    assert_eq!(signals[8].direction, "LONG");
    assert_eq!(signals[8].regime, "STOCKBIT_FIRST_30M_MOMENTUM_LONG");
    assert_eq!(signals[8].suggested_sl, 995.0);
    assert_eq!(signals[8].suggested_tp1, 1_020.0);
    assert_eq!(signals[8].suggested_tp2, 1_030.0);
}

#[test]
fn stockbit_time_of_day_momentum_waits_for_five_positive_prior_slots() {
    let current_slot = 1_790_649_000_i64;
    let mut bars = [7_i64, 6, 5, 4, 1]
        .into_iter()
        .map(|days_ago| StrategyBar {
            time: (current_slot - days_ago * 86_400) * 1_000_000_000,
            open: 1_000.0,
            high: 1_015.0,
            low: 995.0,
            close: 1_010.0,
            volume: 100_000.0,
        })
        .collect::<Vec<_>>();
    bars.push(StrategyBar {
        time: current_slot * 1_000_000_000,
        open: 1_010.0,
        high: 1_030.0,
        low: 1_005.0,
        close: 1_020.0,
        volume: 500_000.0,
    });

    let signals = super::idx::generate_idx_time_of_day_momentum_signals_unconfirmed(
        &bars,
        IdxIntradayMomentumConfig::default(),
    );

    assert!(signals[..5]
        .iter()
        .all(|signal| signal.direction == "NEUTRAL"));
    assert_eq!(signals[5].direction, "LONG");
    assert_eq!(signals[5].regime, "STOCKBIT_TIME_OF_DAY_CONTINUATION_LONG");
    assert_eq!(signals[5].suggested_sl, 1_005.0);
}
