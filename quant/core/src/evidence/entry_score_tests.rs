use super::*;
use crate::{
    generate_stockbit_nwe_4h_signals, score_entry_signal, SignalConfluenceScore, StrategySignal,
};

fn bar(index: usize, open: f64, close: f64) -> StrategyBar {
    StrategyBar {
        time: index as i64 * crate::NANOS_PER_DAY,
        open,
        high: open.max(close) + 1.0,
        low: open.min(close) - 1.0,
        close,
        volume: 1_000.0,
    }
}

fn alternating_bars(count: usize) -> Vec<StrategyBar> {
    (0..count)
        .map(|index| bar(index, 100.0, if index % 2 == 0 { 99.0 } else { 101.0 }))
        .collect()
}

fn wave_bars(count: usize) -> Vec<StrategyBar> {
    (0..count)
        .map(|index| {
            let open = 100.0 + (index as f64 * 0.12).sin() * 6.0;
            bar(index, open, 100.0 + ((index + 1) as f64 * 0.12).sin() * 6.0)
        })
        .collect()
}

fn signal() -> StrategySignal {
    StrategySignal {
        direction: "LONG".into(),
        strategy_strength: 8.0,
        regime: "LEAD_STATE".into(),
        confluence: Some(SignalConfluenceScore {
            score: 3.0,
            nadaraya_state: "NWE_SUPPORT".into(),
            dss_state: "DSS_SUPPORT".into(),
            dss_extreme_zone: false,
            dss_points: 0.0,
            dss_extreme_points: 0.0,
            nadaraya_touch_points: 0.0,
            nadaraya_near_line_points: 0.0,
            nadaraya_reclaim_points: 0.0,
        }),
        ..StrategySignal::default()
    }
}

fn policy(horizon: usize, fees_known: bool) -> EvaluationPolicy {
    EvaluationPolicy {
        horizon,
        exit_on_reverse_signal: false,
        weights: [10; 4],
        minimum_score: 0.0,
        buy_fee_pct: 0.1,
        sell_fee_pct: 0.2,
        fees_known,
        allow_short: true,
    }
}

#[test]
fn mapping_keeps_primary_and_support_on_the_correct_indicator() {
    let bars = alternating_bars(21);
    let nwe = score_entry_signal(&bars, &signal(), EntryModelKind::Nwe).unwrap();
    assert_eq!(
        (nwe.components[0].points, nwe.components[1].points),
        (8.0, 3.0)
    );
    assert_eq!(nwe.components[0].state, "LEAD_STATE");
    assert_eq!(nwe.components[1].state, "DSS_SUPPORT");
    let dss = score_entry_signal(&bars, &signal(), EntryModelKind::Dss).unwrap();
    assert_eq!(
        (dss.components[0].points, dss.components[1].points),
        (3.0, 8.0)
    );
    assert_eq!(dss.components[0].state, "NWE_SUPPORT");
    assert_eq!(dss.components[1].state, "LEAD_STATE");
    let mut without_support = signal();
    without_support.confluence = None;
    let score = score_entry_signal(&bars, &without_support, EntryModelKind::Dss).unwrap();
    assert_eq!(score.components[0].points, 0.0);
    assert_eq!(score.components[0].state, "UNAVAILABLE");
    assert_eq!(score.components[1].points, 8.0);
}

#[test]
fn replay_matches_live_latest_outputs_and_is_invariant_to_future_bars() {
    let prefix = wave_bars(560);
    let full = wave_bars(600);
    for kind in [EntryModelKind::Nwe, EntryModelKind::Dss] {
        let mut from_prefix = Vec::new();
        visit_entry_signals(&prefix, kind, |index, signal| {
            from_prefix.push((
                index,
                signal.clone(),
                score_entry_signal(&prefix[..=index], signal, kind),
            ));
        });
        let mut from_full = Vec::new();
        visit_entry_signals(&full, kind, |index, signal| {
            if index < prefix.len() {
                from_full.push((
                    index,
                    signal.clone(),
                    score_entry_signal(&full[..=index], signal, kind),
                ));
            }
        });
        assert_eq!(from_prefix, from_full);
        for (index, replay, score) in from_prefix.iter().rev().take(3) {
            let live = match kind {
                EntryModelKind::Nwe => generate_stockbit_nwe_4h_signals(&prefix[..=*index]),
                EntryModelKind::Dss => {
                    crate::generate_stockbit_dss_bressert_4h_signals(&prefix[..=*index])
                }
            };
            assert_eq!(Some(replay), live.last());
            assert_eq!(
                *score,
                score_entry_signal(&prefix[..=*index], live.last().unwrap(), kind)
            );
        }
    }
}

#[test]
fn actual_dss_lead_is_evaluated_without_the_old_composite_nwe_gate() {
    let bars = alternating_bars(400);
    let value = evaluate_entry_score_backtest(
        &bars,
        EntryModelKind::Dss,
        crate::NANOS_PER_DAY,
        0.1,
        0.2,
        true,
        true,
    );
    assert_eq!(
        value["score_model_id"],
        EntryModelKind::Dss.score_model_id()
    );
    assert_eq!(value["protocol_id"], ENTRY_SCORE_VALIDATION_PROTOCOL_ID);
    assert_eq!(value["status"], "HOLDOUT_RESEARCH_COMPLETE");
    assert!(
        value["holdout"]["trades"].as_u64().unwrap() >= crate::MIN_EVIDENCE_SAMPLE_TRADES as u64
    );
    assert!(
        value["holdout_factor_bands"][0]["unavailable"]["trades"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert_eq!(value["checks"]["history_span_sufficient"], false);
    assert!(value.get("sizing_ready").is_none());
}

#[test]
fn calendar_minimum_applies_to_full_history_not_the_twenty_percent_holdout() {
    let bars = alternating_bars(crate::ENTRY_SCORE_MIN_HISTORY_DAYS as usize + 1);
    let value = evaluate_entry_score_backtest(
        &bars,
        EntryModelKind::Dss,
        crate::NANOS_PER_DAY,
        0.1,
        0.2,
        true,
        true,
    );

    assert_eq!(value["history_days"], crate::ENTRY_SCORE_MIN_HISTORY_DAYS);
    assert!(value["holdout_window_days"].as_i64().unwrap() < crate::ENTRY_SCORE_MIN_HISTORY_DAYS);
    assert_eq!(value["checks"]["history_span_sufficient"], true);
    assert_eq!(value["minimum_history_days"], crate::ENTRY_SCORE_MIN_HISTORY_DAYS);
    assert!(value.get("minimum_holdout_days").is_none());
}

#[test]
fn history_one_day_short_of_entry_validation_minimum_does_not_pass() {
    let bars = alternating_bars(crate::ENTRY_SCORE_MIN_HISTORY_DAYS as usize);
    let value = evaluate_entry_score_backtest(
        &bars,
        EntryModelKind::Dss,
        crate::NANOS_PER_DAY,
        0.1,
        0.2,
        true,
        true,
    );

    assert_eq!(value["history_days"], crate::ENTRY_SCORE_MIN_HISTORY_DAYS - 1);
    assert_eq!(value["checks"]["history_span_sufficient"], false);
}

#[test]
fn partitions_isolate_entries_exits_and_future_prices() {
    let mut bars: Vec<_> = (0..12).map(|index| bar(index, 100.0, 110.0)).collect();
    let observation = Observation {
        long: true,
        total: 10.0,
        factors: [2.5; 4],
        available: [true; 4],
    };
    let observations = vec![Some(observation); bars.len()];
    let window = evaluate_window(&bars, &observations, 2, 9, policy(4, true));
    assert_eq!(window.overall.trades, 1);
    assert_eq!(
        summary_json(window.overall, true)["first_entry_ts"],
        bars[3].time
    );
    assert_eq!(
        summary_json(window.overall, true)["last_exit_ts"],
        bars[6].time
    );
    for candle in &mut bars[9..] {
        candle.close = 1.0;
    }
    let changed = evaluate_window(&bars, &observations, 2, 9, policy(4, true));
    assert_eq!(
        summary_json(window.overall, true),
        summary_json(changed.overall, true)
    );
    assert_eq!(window.total_bands[0].trades, 0);
    assert_eq!(window.total_bands[1].trades, 1);
    assert_eq!(window.factor_bands[0][1].trades, 1);
}

#[test]
fn unknown_fees_are_explicitly_gross_and_never_invent_net_returns() {
    let bars: Vec<_> = (0..6).map(|index| bar(index, 100.0, 110.0)).collect();
    let observation = Observation {
        long: true,
        total: 40.0,
        factors: [10.0; 4],
        available: [true; 4],
    };
    let result = evaluate_window(
        &bars,
        &vec![Some(observation); bars.len()],
        0,
        bars.len(),
        policy(4, false),
    );
    let value = summary_json(result.overall, false);
    assert_eq!(value["mean_return_pct"], 10.0);
    assert_eq!(value["mean_net_return_pct"], Value::Null);
    assert_eq!(value["fees_applied"], false);
    assert_eq!(value["profit_factor"], Value::Null);
    assert_eq!(value["profit_factor_unbounded"], true);
    assert_eq!(result.total_bands[3].trades, 1);
    assert_eq!(result.factor_bands[0][3].trades, 1);
}

#[test]
fn weight_grid_is_bounded_and_preserves_the_forty_point_budget() {
    let candidates = score_weight_candidates();
    assert_eq!(candidates.len(), 165);
    assert!(candidates.iter().all(|weights| {
        weights.iter().map(|weight| u16::from(*weight)).sum::<u16>() == 40
            && weights.iter().all(|weight| weight % SCORE_WEIGHT_STEP == 0)
    }));
    assert!(candidates.contains(&BASELINE_WEIGHTS));
    assert_eq!(
        score_with_weights(
            Observation {
                long: true,
                total: 0.0,
                factors: [10.0, 0.0, 0.0, 0.0],
                available: [true; 4]
            },
            [40, 0, 0, 0],
        ),
        40.0
    );
}

#[test]
fn weight_replay_uses_the_existing_score_floor_and_does_not_redistribute_missing_factors() {
    let observation = Observation {
        long: true,
        total: 15.0,
        factors: [10.0, 0.0, 5.0, 0.0],
        available: [true, false, true, false],
    };
    let floor = EvaluationPolicy {
        minimum_score: 20.0,
        ..policy(4, true)
    };
    assert!(!qualifies(observation, floor));
    let nwe_focused = EvaluationPolicy {
        weights: [20, 10, 5, 5],
        ..floor
    };
    assert!(qualifies(observation, nwe_focused));
    assert_eq!(score_with_weights(observation, nwe_focused.weights), 22.5);
}

#[test]
fn reverse_exit_requires_the_opposite_signal_to_qualify_under_the_tested_weights() {
    let bars: Vec<_> = (0..8).map(|index| bar(index, 100.0, 101.0)).collect();
    let long = Observation {
        long: true,
        total: 40.0,
        factors: [10.0; 4],
        available: [true; 4],
    };
    let weak_short = Observation {
        long: false,
        total: 20.0,
        factors: [0.0, 10.0, 0.0, 0.0],
        available: [true; 4],
    };
    let mut observations = vec![None; bars.len()];
    observations[0] = Some(long);
    observations[2] = Some(weak_short);
    let selected = EvaluationPolicy {
        horizon: 4,
        exit_on_reverse_signal: true,
        weights: [40, 0, 0, 0],
        minimum_score: 20.0,
        ..policy(4, true)
    };
    let result = evaluate_window(&bars, &observations, 0, bars.len(), selected);
    assert_eq!(result.overall.trades, 1);
    assert_eq!(
        summary_json(result.overall, true)["last_exit_ts"],
        bars[4].time
    );
}

#[test]
fn learned_weights_need_known_fees_and_minimum_training_evidence() {
    let bars = alternating_bars(560);
    let fees_unknown = evaluate_entry_score_backtest(
        &bars,
        EntryModelKind::Dss,
        crate::NANOS_PER_DAY,
        0.0,
        0.0,
        false,
        true,
    );
    assert_eq!(fees_unknown["weight_learning"]["status"], "FEES_UNKNOWN");
    assert_eq!(
        fees_unknown["weight_learning"]["live_weights_changed"],
        false
    );
    assert_eq!(fees_unknown["weight_learning"]["candidate"], Value::Null);
}

#[test]
fn training_selects_a_weight_vector_without_reading_validation_or_holdout_prices() {
    let bars: Vec<_> = (0..600).map(|index| bar(index, 100.0, 110.0)).collect();
    let observations: Vec<_> = (0..bars.len())
        .map(|index| {
            (index % 9 == 0).then_some(Observation {
                long: true,
                total: 10.0,
                factors: [10.0, 0.0, 0.0, 0.0],
                available: [true; 4],
            })
        })
        .collect();
    let policy = EvaluationPolicy {
        minimum_score: 20.0,
        ..policy(8, true)
    };
    let (selected, eligible, summary) = fit_weights(&bars, &observations, 0, 360, policy);
    let selected = selected.expect("training sample has eligible candidates");
    assert_eq!(eligible > 0, true);
    assert_eq!(
        selected
            .iter()
            .map(|weight| u16::from(*weight))
            .sum::<u16>(),
        40
    );
    assert!(selected[0] >= 20);
    assert!(summary.trades >= MIN_WEIGHT_FIT_TRADES);

    let mut changed_future = bars.clone();
    for bar in &mut changed_future[360..] {
        bar.close = 1.0;
        bar.high = bar.open.max(bar.close);
        bar.low = bar.open.min(bar.close);
    }
    let replay = fit_weights(&changed_future, &observations, 0, 360, policy);
    assert_eq!(replay.0, Some(selected));
    assert_eq!(summary_json(replay.2, true), summary_json(summary, true));
}

#[test]
fn hour_pair_uses_next_bar_open_and_target_bar_close_in_wib() {
    let hour_ns = crate::SECONDS_PER_HOUR * crate::NANOS_PER_SECOND;
    let bars: Vec<_> = (0..20)
        .map(|index| bar(index as usize, 100.0, 101.0))
        .enumerate()
        .map(|(index, mut candle)| {
            candle.time = index as i64 * hour_ns;
            candle
        })
        .collect();
    let observation = Observation {
        long: true,
        total: 40.0,
        factors: [10.0; 4],
        available: [true; 4],
    };
    let mut observations = vec![None; bars.len()];
    observations[0] = Some(observation);
    let entry_hour = hour_of_wib(bars[1].time);
    let exit_hour = hour_of_wib(bars[2].time + hour_ns);
    assert_eq!((entry_hour, exit_hour), (8, 10));
    let summary = evaluate_hour_window(
        &bars,
        &observations,
        0,
        bars.len(),
        hour_ns,
        entry_hour,
        exit_hour,
        EvaluationPolicy {
            minimum_score: 20.0,
            ..policy(12, true)
        },
    );
    assert_eq!(summary.trades, 1);
    assert_eq!(summary_json(summary, true)["last_exit_ts"], bars[2].time);
}

#[test]
fn weighted_score_autocorrelation_uses_training_signal_events() {
    let observations: Vec<_> = (0..12)
        .map(|index| {
            Some(Observation {
                long: true,
                total: 0.0,
                factors: [index as f64, 0.0, 0.0, 0.0],
                available: [true; 4],
            })
        })
        .collect();
    let result =
        weighted_score_autocorrelation(&observations, 0, observations.len(), [40, 0, 0, 0]);
    assert_eq!(result["pairs"], 11);
    assert!(result["coefficient"].as_f64().unwrap() > 0.99);
}

#[test]
fn hour_search_selects_the_validation_pair_with_the_highest_fee_aware_return() {
    let hour_ns = crate::SECONDS_PER_HOUR * crate::NANOS_PER_SECOND;
    let bars: Vec<_> = (0..5_000)
        .map(|index| {
            let time = index as i64 * hour_ns;
            let close_hour = hour_of_wib(time + hour_ns);
            let close = if close_hour == 10 { 110.0 } else { 100.0 };
            let mut candle = bar(index, 100.0, close);
            candle.time = time;
            candle
        })
        .collect();
    let observation = Observation {
        long: true,
        total: 40.0,
        factors: [10.0; 4],
        available: [true; 4],
    };
    let observations = vec![Some(observation); bars.len()];
    let selected = select_hour_pair_on_validation(
        &bars,
        &observations,
        0,
        bars.len(),
        hour_ns,
        EvaluationPolicy {
            weights: [10; 4],
            minimum_score: 20.0,
            ..policy(12, true)
        },
    )
    .selected
    .expect("validation hour pair exceeds the minimum trade count");
    assert_eq!(selected.exit_hour, 10);
    assert!(selected.summary.mean_net().unwrap() > 0.0);
    assert!(selected.summary.trades >= MIN_HOUR_SELECTION_TRADES);
}

#[test]
fn low_training_sample_does_not_create_a_weight_candidate() {
    let bars: Vec<_> = (0..120).map(|index| bar(index, 100.0, 110.0)).collect();
    let observation = Observation {
        long: true,
        total: 40.0,
        factors: [10.0; 4],
        available: [true; 4],
    };
    let observations = vec![Some(observation); bars.len()];
    let (selected, eligible, _) = fit_weights(
        &bars,
        &observations,
        0,
        bars.len(),
        EvaluationPolicy {
            minimum_score: 20.0,
            ..policy(8, true)
        },
    );
    assert!(selected.is_none());
    assert_eq!(eligible, 0);
}

#[test]
fn daily_bars_report_intraday_hour_analysis_as_not_applicable() {
    let report = hour_rule_report(
        &[],
        &[],
        0,
        0,
        0,
        crate::NANOS_PER_DAY,
        None,
        policy(8, true),
    );
    assert_eq!(report["status"], "INTRADAY_HOURS_NOT_APPLICABLE");
}

#[test]
fn band_boundaries_include_exact_ten_and_forty_without_reclassifying_unavailable_factors() {
    for (score, expected) in [
        (0.0, 0),
        (9.999, 0),
        (10.0, 1),
        (20.0, 2),
        (30.0, 3),
        (40.0, 3),
    ] {
        assert_eq!(band_index(score, TOTAL_SCORE_BAND_WIDTH), expected);
    }
    let bars: Vec<_> = (0..6).map(|index| bar(index, 100.0, 110.0)).collect();
    let observation = Observation {
        long: true,
        total: 10.0,
        factors: [0.0, 10.0, 0.0, 0.0],
        available: [false, true, false, false],
    };
    let result = evaluate_window(
        &bars,
        &vec![Some(observation); bars.len()],
        0,
        bars.len(),
        policy(4, true),
    );
    assert_eq!(result.total_bands[1].trades, 1);
    assert_eq!(result.factor_bands[0][0].trades, 0);
    assert_eq!(result.unavailable_factors[0].trades, 1);
}

#[test]
fn discrimination_requires_two_sufficient_separated_bands_with_higher_mean_return() {
    let gains = [bar(0, 100.0, 110.0)];
    let losses = [bar(0, 100.0, 90.0)];
    let mut bands = [TradeSummary::default(); 4];
    for _ in 0..crate::MIN_EVIDENCE_SAMPLE_TRADES {
        bands[0].add(&losses, 0, 0, "LONG", 0.0, 0.0, true);
        bands[3].add(&gains, 0, 0, "LONG", 0.0, 0.0, true);
    }
    assert!(score_discrimination_supported(&bands));
    bands[3] = bands[0];
    assert!(!score_discrimination_supported(&bands));
    bands[3] = TradeSummary::default();
    assert!(!score_discrimination_supported(&bands));
}

#[test]
fn malformed_nonmonotonic_and_short_history_are_explicit() {
    let empty = evaluate_entry_score_backtest(
        &[],
        EntryModelKind::Nwe,
        crate::NANOS_PER_DAY,
        0.0,
        0.0,
        false,
        false,
    );
    assert_eq!(empty["status"], "INSUFFICIENT_HISTORY");
    assert_eq!(empty["holdout"]["trades"], 0);
    assert_eq!(empty["checks"]["holdout_sample_sufficient"], false);
    assert_eq!(empty["splits"]["holdout"]["start_ts"], Value::Null);
    let mut malformed = alternating_bars(2);
    malformed[1].time = malformed[0].time;
    assert_eq!(
        evaluate_entry_score_backtest(
            &malformed,
            EntryModelKind::Dss,
            crate::NANOS_PER_DAY,
            0.0,
            0.0,
            false,
            true
        )["status"],
        "INVALID_HISTORY"
    );
    malformed = alternating_bars(2);
    malformed[0].volume = f64::NAN;
    assert_eq!(
        evaluate_entry_score_backtest(
            &malformed,
            EntryModelKind::Dss,
            crate::NANOS_PER_DAY,
            0.0,
            0.0,
            false,
            true
        )["status"],
        "INVALID_HISTORY"
    );
    assert_eq!(
        evaluate_entry_score_backtest(
            &[],
            EntryModelKind::Nwe,
            crate::NANOS_PER_DAY,
            f64::NAN,
            0.0,
            false,
            false
        )["status"],
        "INVALID_HISTORY"
    );
}

#[test]
#[ignore = "Explicit bounded 10,000-bar release-profile runtime inspection"]
fn entry_score_ten_thousand_bar_runtime_inspection() {
    let bars = wave_bars(10_000);
    let started = std::time::Instant::now();
    let value = evaluate_entry_score_backtest(
        &bars,
        EntryModelKind::Nwe,
        crate::NANOS_PER_DAY,
        0.1,
        0.2,
        true,
        true,
    );
    println!(
        "10k NWE elapsed: {:?}; status={}; holdout_trades={}",
        started.elapsed(),
        value["status"],
        value["holdout"]["trades"]
    );
    if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
        for line in status
            .lines()
            .filter(|line| line.starts_with("VmHWM:") || line.starts_with("VmRSS:"))
        {
            println!("{line}");
        }
    }
}
