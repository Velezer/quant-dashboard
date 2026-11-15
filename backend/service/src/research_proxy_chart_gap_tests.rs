use super::{chart_window_needs_repair, Candle};

fn candle(time: i64) -> Candle {
    Candle {
        time,
        open: 1.0,
        high: 1.0,
        low: 1.0,
        close: 1.0,
        volume: 1.0,
        dss: None,
        trigger: None,
        gradient: None,
        accel: None,
        indicators: None,
    }
}

#[test]
fn contiguous_recent_history_does_not_request_a_repair() {
    let now = 1_700_000_000_000_000_000_i64;
    let interval = 15 * 60 * 1_000_000_000_i64;
    let candles = vec![candle(now - interval * 2), candle(now - interval), candle(now)];

    assert!(!chart_window_needs_repair(
        &candles,
        "15m",
        15 * 60,
        now,
        "APT-USD",
    ));
}

#[test]
fn a_large_history_gap_requests_a_repair_even_with_a_fresh_latest_candle() {
    let now = 1_700_000_000_000_000_000_i64;
    let interval = 15 * 60 * 1_000_000_000_i64;
    let candles = vec![candle(now - interval * 10), candle(now)];

    assert!(chart_window_needs_repair(
        &candles,
        "15m",
        15 * 60,
        now,
        "APT-USD",
    ));
}

#[test]
fn a_single_recent_candle_requests_a_repair() {
    let now = 1_700_000_000_000_000_000_i64;

    assert!(chart_window_needs_repair(
        &[candle(now)],
        "15m",
        15 * 60,
        now,
        "APT-USD",
    ));
}
