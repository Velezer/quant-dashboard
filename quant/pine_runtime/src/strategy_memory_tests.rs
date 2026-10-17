use crate::{
    run_strategy_with_history_limit, Candle, MAX_STRATEGY_CANDLES,
    MAX_STRATEGY_RETAINED_SERIES_VALUES,
};
use std::collections::HashMap;

#[test]
fn strategy_history_can_exceed_ten_thousand_bars_within_the_series_budget() {
    let bars = (0..10_001)
        .map(|index| Candle {
            open: 100.0,
            high: 101.0,
            low: 99.0,
            close: 100.0,
            volume: 1.0,
            time: Some(index * 86_400_000),
        })
        .collect::<Vec<_>>();
    let source = "//@version=6\nstrategy(\"Deep sample\")\nfloat close_series = close\nfloat high_series = high";

    let output = run_strategy_with_history_limit(
        source,
        &bars,
        "1d",
        &HashMap::new(),
        MAX_STRATEGY_CANDLES,
        0.0,
        0.0,
        &HashMap::new(),
    )
    .expect("a 10,001-bar strategy using two series should fit the bounded evaluator");

    assert_eq!(MAX_STRATEGY_CANDLES, 20_000);
    assert!(bars.len() > 10_000);
    assert!(bars.len() * 2 <= MAX_STRATEGY_RETAINED_SERIES_VALUES);
    assert_eq!(output.equity_curve.len(), bars.len());
}

#[test]
fn strategy_evaluator_rejects_history_that_exceeds_retained_series_budget() {
    let bars = (0..MAX_STRATEGY_CANDLES)
        .map(|index| Candle {
            open: 100.0,
            high: 101.0,
            low: 99.0,
            close: 100.0,
            volume: 1.0,
            time: Some(index as i64),
        })
        .collect::<Vec<_>>();
    let source = "//@version=6\nstrategy(\"Bounded sample\")\nfloat a = close\nfloat b = close\nfloat c = close\nfloat d = close\nfloat e = close\nfloat f = close";

    let diagnostics = run_strategy_with_history_limit(
        source,
        &bars,
        "1d",
        &HashMap::new(),
        MAX_STRATEGY_CANDLES,
        0.0,
        0.0,
        &HashMap::new(),
    )
    .expect_err("six full-history series must exceed the retained-value budget");

    assert!(diagnostics
        .iter()
        .any(|item| item.message.contains("retained series-value limit")));
}

#[test]
fn strategy_evaluator_rejects_an_oversized_retained_string() {
    let bars = [Candle {
        open: 100.0,
        high: 101.0,
        low: 99.0,
        close: 100.0,
        volume: 1.0,
        time: Some(0),
    }];
    let source = format!(
        "//@version=6\nstrategy(\"Bounded string\")\nstring label = \"{}\"",
        "x".repeat(513)
    );

    let diagnostics = run_strategy_with_history_limit(
        &source,
        &bars,
        "1d",
        &HashMap::new(),
        MAX_STRATEGY_CANDLES,
        0.0,
        0.0,
        &HashMap::new(),
    )
    .expect_err("retained Pine strings must fit the per-value byte bound");

    assert!(diagnostics
        .iter()
        .any(|item| item.message.contains("limited to 512 bytes per value")));
}
