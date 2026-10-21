use super::*;
use crate::{generate_stockbit_nwe_4h_signals, score_entry_signal, SignalConfluenceScore, StrategySignal};

fn bar(index: usize, open: f64, close: f64) -> StrategyBar {
    StrategyBar { time: index as i64 * crate::NANOS_PER_DAY, open,
        high: open.max(close) + 1.0, low: open.min(close) - 1.0, close, volume: 1_000.0 }
}

fn alternating_bars(count: usize) -> Vec<StrategyBar> {
    (0..count).map(|index| bar(index, 100.0, if index % 2 == 0 { 99.0 } else { 101.0 })).collect()
}

fn wave_bars(count: usize) -> Vec<StrategyBar> {
    (0..count).map(|index| {
        let open = 100.0 + (index as f64 * 0.12).sin() * 6.0;
        bar(index, open, 100.0 + ((index + 1) as f64 * 0.12).sin() * 6.0)
    }).collect()
}

fn signal() -> StrategySignal {
    StrategySignal { direction: "LONG".into(), strategy_strength: 8.0, regime: "LEAD_STATE".into(),
        confluence: Some(SignalConfluenceScore { score: 3.0, nadaraya_state: "NWE_SUPPORT".into(),
            dss_state: "DSS_SUPPORT".into(), dss_extreme_zone: false, dss_points: 0.0,
            dss_extreme_points: 0.0, nadaraya_touch_points: 0.0,
            nadaraya_near_line_points: 0.0, nadaraya_reclaim_points: 0.0 }), ..StrategySignal::default() }
}

fn policy(horizon: usize, fees_known: bool) -> EvaluationPolicy {
    EvaluationPolicy { horizon, buy_fee_pct: 0.1, sell_fee_pct: 0.2, fees_known, allow_short: true }
}

#[test]
fn mapping_keeps_primary_and_support_on_the_correct_indicator() {
    let bars = alternating_bars(21);
    let nwe = score_entry_signal(&bars, &signal(), EntryModelKind::Nwe).unwrap();
    assert_eq!((nwe.components[0].points, nwe.components[1].points), (8.0, 3.0));
    assert_eq!(nwe.components[0].state, "LEAD_STATE");
    assert_eq!(nwe.components[1].state, "DSS_SUPPORT");
    let dss = score_entry_signal(&bars, &signal(), EntryModelKind::Dss).unwrap();
    assert_eq!((dss.components[0].points, dss.components[1].points), (3.0, 8.0));
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
            from_prefix.push((index, signal.clone(), score_entry_signal(&prefix[..=index], signal, kind)));
        });
        let mut from_full = Vec::new();
        visit_entry_signals(&full, kind, |index, signal| {
            if index < prefix.len() { from_full.push((index, signal.clone(), score_entry_signal(&full[..=index], signal, kind))); }
        });
        assert_eq!(from_prefix, from_full);
        for (index, replay, score) in from_prefix.iter().rev().take(3) {
            let live = match kind {
                EntryModelKind::Nwe => generate_stockbit_nwe_4h_signals(&prefix[..=*index]),
                EntryModelKind::Dss => crate::generate_stockbit_dss_bressert_4h_signals(&prefix[..=*index]),
            };
            assert_eq!(Some(replay), live.last());
            assert_eq!(*score, score_entry_signal(&prefix[..=*index], live.last().unwrap(), kind));
        }
    }
}

#[test]
fn actual_dss_lead_is_evaluated_without_the_old_composite_nwe_gate() {
    let bars = alternating_bars(400);
    let value = evaluate_entry_score_backtest(&bars, EntryModelKind::Dss, 0.1, 0.2, true, true);
    assert_eq!(value["score_model_id"], EntryModelKind::Dss.score_model_id());
    assert_eq!(value["protocol_id"], ENTRY_SCORE_VALIDATION_PROTOCOL_ID);
    assert_eq!(value["status"], "HOLDOUT_RESEARCH_COMPLETE");
    assert!(value["holdout"]["trades"].as_u64().unwrap() >= crate::MIN_EVIDENCE_SAMPLE_TRADES as u64);
    assert!(value["holdout_factor_bands"][0]["unavailable"]["trades"].as_u64().unwrap() > 0);
    assert_eq!(value["checks"]["holdout_span_sufficient"], false);
    assert!(value.get("sizing_ready").is_none());
}

#[test]
fn partitions_isolate_entries_exits_and_future_prices() {
    let mut bars: Vec<_> = (0..12).map(|index| bar(index, 100.0, 110.0)).collect();
    let observation = Observation { long: true, total: 10.0, factors: [2.5; 4], available: [true; 4] };
    let observations = vec![Some(observation); bars.len()];
    let window = evaluate_window(&bars, &observations, 2, 9, policy(4, true));
    assert_eq!(window.overall.trades, 1);
    assert_eq!(summary_json(window.overall, true)["first_entry_ts"], bars[3].time);
    assert_eq!(summary_json(window.overall, true)["last_exit_ts"], bars[6].time);
    for candle in &mut bars[9..] { candle.close = 1.0; }
    let changed = evaluate_window(&bars, &observations, 2, 9, policy(4, true));
    assert_eq!(summary_json(window.overall, true), summary_json(changed.overall, true));
    assert_eq!(window.total_bands[0].trades, 0);
    assert_eq!(window.total_bands[1].trades, 1);
    assert_eq!(window.factor_bands[0][1].trades, 1);
}

#[test]
fn unknown_fees_are_explicitly_gross_and_never_invent_net_returns() {
    let bars: Vec<_> = (0..6).map(|index| bar(index, 100.0, 110.0)).collect();
    let observation = Observation { long: true, total: 40.0, factors: [10.0; 4], available: [true; 4] };
    let result = evaluate_window(&bars, &vec![Some(observation); bars.len()], 0, bars.len(), policy(4, false));
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
fn band_boundaries_include_exact_ten_and_forty_without_reclassifying_unavailable_factors() {
    for (score, expected) in [(0.0, 0), (9.999, 0), (10.0, 1), (20.0, 2), (30.0, 3), (40.0, 3)] {
        assert_eq!(band_index(score, TOTAL_SCORE_BAND_WIDTH), expected);
    }
    let bars: Vec<_> = (0..6).map(|index| bar(index, 100.0, 110.0)).collect();
    let observation = Observation { long: true, total: 10.0, factors: [0.0, 10.0, 0.0, 0.0], available: [false, true, false, false] };
    let result = evaluate_window(&bars, &vec![Some(observation); bars.len()], 0, bars.len(), policy(4, true));
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
    let empty = evaluate_entry_score_backtest(&[], EntryModelKind::Nwe, 0.0, 0.0, false, false);
    assert_eq!(empty["status"], "INSUFFICIENT_HISTORY");
    assert_eq!(empty["holdout"]["trades"], 0);
    assert_eq!(empty["checks"]["holdout_sample_sufficient"], false);
    assert_eq!(empty["splits"]["holdout"]["start_ts"], Value::Null);
    let mut malformed = alternating_bars(2);
    malformed[1].time = malformed[0].time;
    assert_eq!(evaluate_entry_score_backtest(&malformed, EntryModelKind::Dss, 0.0, 0.0, false, true)["status"], "INVALID_HISTORY");
    malformed = alternating_bars(2);
    malformed[0].volume = f64::NAN;
    assert_eq!(evaluate_entry_score_backtest(&malformed, EntryModelKind::Dss, 0.0, 0.0, false, true)["status"], "INVALID_HISTORY");
    assert_eq!(evaluate_entry_score_backtest(&[], EntryModelKind::Nwe, f64::NAN, 0.0, false, false)["status"], "INVALID_HISTORY");
}

#[test]
#[ignore = "Explicit bounded 10,000-bar release-profile runtime inspection"]
fn entry_score_ten_thousand_bar_runtime_inspection() {
    let bars = wave_bars(10_000);
    let started = std::time::Instant::now();
    let value = evaluate_entry_score_backtest(&bars, EntryModelKind::Nwe, 0.1, 0.2, true, true);
    println!("10k NWE elapsed: {:?}; status={}; holdout_trades={}", started.elapsed(), value["status"], value["holdout"]["trades"]);
    if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
        for line in status.lines().filter(|line| line.starts_with("VmHWM:") || line.starts_with("VmRSS:")) { println!("{line}"); }
    }
}
