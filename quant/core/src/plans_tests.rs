use super::*;
use crate::{calculate_nadaraya_watson_envelope, NadarayaWatsonEnvelopeConfig};

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

fn add_flat_prior_history(bars: Vec<StrategyBar>, count: usize, close: f64) -> Vec<StrategyBar> {
    let first_time = bars.first().expect("current session bars exist").time;
    let prior = (0..count)
        .map(|index| StrategyBar {
            time: first_time - ((count - index) as i64 * 86_400 * 1_000_000_000),
            open: close,
            high: close + 1.0,
            low: close - 1.0,
            close,
            volume: 1_000_000.0,
        })
        .collect::<Vec<_>>();
    [prior, bars].concat()
}

fn stockbit_bars_with_recent_oversold_turn_and_band_touch(final_close: f64) -> Vec<StrategyBar> {
    let mut bars: Vec<_> = (0..31)
        .map(|index| StrategyBar {
            time: (1_767_312_000_i64 + index as i64 * 900) * 1_000_000_000,
            open: 100.0,
            high: 101.0,
            low: 99.0,
            close: 100.0,
            volume: 1_000_000.0,
        })
        .collect();
    for (bar, close) in bars[21..30]
        .iter_mut()
        .zip([91.0, 82.0, 73.0, 64.0, 55.0, 46.0, 37.0, 28.0, 19.0])
    {
        bar.open = close + 0.5;
        bar.high = close + 1.0;
        bar.low = close - 1.0;
        bar.close = close;
    }
    let last = bars.last_mut().expect("signal bar exists");
    last.close = final_close;
    last.high = final_close + 1.0;
    last.volume = 100_000_000.0;
    let mut bars = add_flat_prior_history(bars, 469, 100.0);
    let closes = bars.iter().map(|bar| bar.close).collect::<Vec<_>>();
    let endpoint =
        calculate_nadaraya_watson_envelope(&closes, &NadarayaWatsonEnvelopeConfig::default())
            .endpoint;
    let lower = *endpoint.lower.last().expect("endpoint point exists");
    assert!(lower.is_finite() && lower <= final_close);
    let last = bars.last_mut().expect("signal bar exists");
    last.open = lower;
    last.low = lower;
    bars
}

#[test]
fn neutral_series_has_no_plan() {
    let bars: Vec<_> = (0..80).map(|index| bar(index, 100.0, 1.0)).collect();
    assert!(
        build_registered_plan(&bars, "BTC-USD", Some("BTC"), "pintu_futures_confluence").is_none()
    );
}

#[test]
fn stockbit_plan_contains_tick_safe_levels() {
    let bars = stockbit_bars_with_recent_oversold_turn_and_band_touch(62.0);
    let result = build_registered_plan(&bars, "TEST.JK", Some("TEST"), "idx_stockbit_day_trade")
        .expect("open-low/volume confluence should produce a plan");
    assert_eq!(result.plan["Strategy_ID"], "idx_stockbit_day_trade");
    assert_eq!(result.plan["Stop_Loss"].as_f64().unwrap() % 1.0, 0.0);
    assert!(result.plan["TP1 (2.0R)"].as_f64().unwrap() > 62.0);
}

#[test]
fn stockbit_plan_filters_current_prices_below_rp60() {
    let passing_bars = stockbit_bars_with_recent_oversold_turn_and_band_touch(62.0);
    assert!(build_registered_plan(
        &passing_bars,
        "TEST.JK",
        Some("TEST"),
        "idx_stockbit_day_trade"
    )
    .is_some());

    let mut below_floor_bars = passing_bars.clone();
    let last = below_floor_bars.last_mut().expect("signal bar exists");
    last.open = 49.0;
    last.high = 50.0;
    last.low = 49.0;
    last.close = 49.0;
    assert!(build_registered_plan(
        &below_floor_bars,
        "TEST.JK",
        Some("TEST"),
        "idx_stockbit_day_trade"
    )
    .is_none());
}

#[test]
fn stockbit_tsmom_plan_waits_for_required_nadaraya_history() {
    let bars: Vec<_> = (0..260)
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

    let result = build_registered_plan(
        &bars,
        "TEST.JK",
        Some("TEST"),
        "idx_stockbit_time_series_momentum_swing",
    );

    assert!(
        result.is_none(),
        "260 daily bars do not warm the 500-bar endpoint"
    );
}

#[test]
fn stockbit_orb_plan_waits_when_breakout_is_not_near_nadaraya() {
    let mut bars: Vec<_> = (0..7)
        .map(|index| StrategyBar {
            time: (1_790_560_800_i64 + index * 900) * 1_000_000_000,
            open: 1_000.0,
            high: 1_005.0,
            low: 995.0,
            close: 1_000.0,
            volume: 100_000.0,
        })
        .collect();
    bars.extend([
        StrategyBar {
            time: 1_790_647_200_i64 * 1_000_000_000,
            open: 1_000.0,
            high: 1_005.0,
            low: 1_000.0,
            close: 1_002.0,
            volume: 100_000.0,
        },
        StrategyBar {
            time: 1_790_648_100_i64 * 1_000_000_000,
            open: 1_000.0,
            high: 1_010.0,
            low: 1_000.0,
            close: 1_005.0,
            volume: 100_000.0,
        },
        StrategyBar {
            time: 1_790_649_000_i64 * 1_000_000_000,
            open: 1_005.0,
            high: 1_025.0,
            low: 1_015.0,
            close: 1_020.0,
            volume: 500_000.0,
        },
    ]);
    let bars = add_flat_prior_history(bars, 490, 1_000.0);

    let result = build_registered_plan(
        &bars,
        "TEST.JK",
        Some("TEST"),
        "idx_stockbit_opening_range_momentum",
    );

    assert!(
        result.is_none(),
        "an extended breakout does not touch the Nadaraya outer envelope"
    );
}
