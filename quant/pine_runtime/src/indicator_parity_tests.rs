use crate::{run_indicator, run_strategy, Candle};
use quant_core::{
    calculate_dss, generate_stockbit_dss_bressert_4h_signals, DssConfig, StrategyBar,
};

#[test]
fn ema_uses_first_value_seed_while_rma_uses_full_window_sma_seed() {
    let source = "//@version=6\nindicator(\"Average seed parity\")\nplot(ta.ema(close, 3), title=\"EMA\")\nplot(ta.rma(close, 3), title=\"RMA\")";
    let closes = [1.0, 2.0, 10.0, 4.0];
    let candles = closes
        .iter()
        .enumerate()
        .map(|(index, close)| Candle {
            open: *close,
            high: *close,
            low: *close,
            close: *close,
            volume: 1.0,
            time: Some(index as i64),
        })
        .collect::<Vec<_>>();

    let output = run_indicator(source, &candles).expect("average seed example should run");
    let ema = &output
        .plots
        .iter()
        .find(|plot| plot.title == "EMA")
        .expect("EMA plot should exist")
        .values;
    let rma = &output
        .plots
        .iter()
        .find(|plot| plot.title == "RMA")
        .expect("RMA plot should exist")
        .values;
    let close_enough = |actual: Option<f64>, expected: f64| {
        actual.is_some_and(|value| (value - expected).abs() < 1e-12)
    };

    assert!(close_enough(ema[0], 1.0));
    assert!(close_enough(ema[1], 1.5));
    assert!(close_enough(ema[2], 5.75));
    assert!(close_enough(ema[3], 4.875));
    assert_eq!(rma[0], None);
    assert_eq!(rma[1], None);
    assert!(close_enough(rma[2], 13.0 / 3.0));
    assert!(close_enough(rma[3], 38.0 / 9.0));
}

#[test]
fn shipped_dss_formula_matches_core_after_the_strategy_warmup() {
    let source = "//@version=6\nindicator(\"DSS parity\")\nint dssPds = 10\nint dssEmaLength = 9\nint dssTriggerLength = 5\nfloat rangeLow = ta.lowest(low, dssPds)\nfloat rangeHigh = ta.highest(high, dssPds)\nfloat stoch = rangeHigh == rangeLow ? 50.0 : (close - rangeLow) / (rangeHigh - rangeLow) * 100.0\nfloat smooth1 = ta.ema(stoch, dssEmaLength)\nfloat smoothLow = ta.lowest(smooth1, dssPds)\nfloat smoothHigh = ta.highest(smooth1, dssPds)\nfloat secondRange = smoothHigh - smoothLow\nfloat stoch2 = not na(secondRange) ? (secondRange == 0 ? 50.0 : 100.0 * (smooth1 - smoothLow) / secondRange) : na\nfloat dss = ta.ema(stoch2, dssEmaLength)\nfloat trigger = ta.ema(dss, dssTriggerLength)\nplot(dss, title=\"DSS\")\nplot(trigger, title=\"Trigger\")";
    let candles = (0..520)
        .map(|index| {
            let close =
                100.0 + (index as f64 * 0.41).sin() * 7.0 + (index as f64 * 0.13).cos() * 2.0;
            Candle {
                open: close - 0.2,
                high: close + 1.1 + (index as f64 * 0.07).sin().abs(),
                low: close - 1.2 - (index as f64 * 0.11).cos().abs(),
                close,
                volume: 1.0,
                time: Some(index as i64),
            }
        })
        .collect::<Vec<_>>();
    let highs = candles.iter().map(|bar| bar.high).collect::<Vec<_>>();
    let lows = candles.iter().map(|bar| bar.low).collect::<Vec<_>>();
    let closes = candles.iter().map(|bar| bar.close).collect::<Vec<_>>();
    let reference = calculate_dss(&highs, &lows, &closes, &DssConfig::default());
    let output = run_indicator(source, &candles)
        .expect("DSS parity source should stay within evaluator bounds");
    let dss = &output
        .plots
        .iter()
        .find(|plot| plot.title == "DSS")
        .expect("DSS plot should exist")
        .values;
    let trigger = &output
        .plots
        .iter()
        .find(|plot| plot.title == "Trigger")
        .expect("trigger plot should exist")
        .values;

    for index in 500..candles.len() {
        let actual_dss = dss[index].expect("DSS should be available after warm-up");
        let actual_trigger = trigger[index].expect("trigger should be available after warm-up");
        assert!(
            (actual_dss - reference.dss[index]).abs() < 1e-6,
            "DSS mismatch at {index}: Pine {actual_dss}, core {}",
            reference.dss[index]
        );
        assert!(
            (actual_trigger - reference.trigger[index]).abs() < 1e-6,
            "trigger mismatch at {index}: Pine {actual_trigger}, core {}",
            reference.trigger[index]
        );
    }
}

#[test]
fn shipped_strategy_entries_and_timeout_exits_match_core_signals() {
    let source = include_str!("../examples/nwe_dss_bressert_backtest.pine");
    let candles = (0..650)
        .map(|index| {
            let close = 100.0 + (index as f64 * 0.07).sin() * 6.0 + (index as f64) * 0.01;
            let open = close - 0.15;
            Candle {
                open,
                high: close + 0.5,
                low: open - 0.5,
                close,
                volume: 1.0,
                time: Some(index as i64),
            }
        })
        .collect::<Vec<_>>();
    let bars = candles
        .iter()
        .map(|bar| StrategyBar {
            time: bar.time.unwrap_or_default(),
            open: bar.open,
            high: bar.high,
            low: bar.low,
            close: bar.close,
            volume: bar.volume,
        })
        .collect::<Vec<_>>();
    let signals = generate_stockbit_dss_bressert_4h_signals(&bars);
    let output =
        run_strategy(source, &candles, 0.0, 0.0).expect("shipped NWE + DSS strategy should run");
    let long_markers = &output
        .plots
        .iter()
        .find(|plot| plot.title == "NWE + DSS LONG entry")
        .expect("long entry plot should exist")
        .values;
    let short_markers = &output
        .plots
        .iter()
        .find(|plot| plot.title == "NWE + DSS SHORT entry")
        .expect("short entry plot should exist")
        .values;

    let mut expected_entries = Vec::new();
    let mut expected_exits = Vec::new();
    let mut signal_index = 0usize;
    while signal_index < signals.len() {
        let signal = &signals[signal_index];
        let direction = match signal.direction.as_str() {
            "LONG"
                if signal.strength >= 1.0
                    && signal.confluence.as_ref().is_some_and(|item| {
                        item.nadaraya_state.starts_with("NWE_LOWER_BAND_")
                            && item.dss_state != "NEUTRAL"
                    }) =>
            {
                1
            }
            "SHORT"
                if signal.strength >= 1.0
                    && signal.confluence.as_ref().is_some_and(|item| {
                        item.nadaraya_state.starts_with("NWE_UPPER_BAND_")
                            && item.dss_state != "NEUTRAL"
                    }) =>
            {
                -1
            }
            _ => 0,
        };
        if direction == 0 {
            signal_index += 1;
            continue;
        }
        let exit_bar = signal_index + 8;
        if exit_bar >= candles.len() {
            break;
        }
        expected_entries.push((signal_index, direction));
        expected_exits.push(exit_bar);
        signal_index = exit_bar + 1;
    }

    let actual_entries = (0..candles.len())
        .filter_map(|index| {
            long_markers[index]
                .map(|_| (index, 1))
                .or_else(|| short_markers[index].map(|_| (index, -1)))
        })
        .collect::<Vec<_>>();
    assert_eq!(actual_entries, expected_entries);
    assert_eq!(output.trades.len(), expected_entries.len());
    for (index, trade) in output.trades.iter().enumerate() {
        assert_eq!(trade.entry_bar, expected_entries[index].0 + 1);
        assert_eq!(trade.exit_bar, expected_exits[index]);
        assert_eq!(
            trade.direction,
            if expected_entries[index].1 > 0 {
                "LONG"
            } else {
                "SHORT"
            }
        );
        assert_eq!(trade.entry_price, candles[trade.entry_bar].open);
        assert_eq!(trade.exit_price, candles[trade.exit_bar].close);
    }
}
