use super::common::StrategyBar;
use super::dss_bressert::classify_latest_spread;
use super::{calculate_dss_bressert_mtf, DssBressertTimeframeInput, DSS_BRESSERT_MTF_TIMEFRAMES};

fn bars(count: usize) -> Vec<StrategyBar> {
    (0..count)
        .map(|index| {
            let close = 100.0 + index as f64 * 0.25 + (index as f64 * 0.7).sin();
            StrategyBar {
                time: index as i64,
                open: close,
                high: close + 1.0,
                low: close - 1.0,
                close,
                volume: 1.0,
            }
        })
        .collect()
}

#[test]
fn dss_crosses_and_approaches_match_the_reference_screener_states() {
    assert_eq!(
        classify_latest_spread(&[-1.0, 2.0], &[0.0, 1.0]),
        Some((3, "BULLISH_CROSS"))
    );
    assert_eq!(
        classify_latest_spread(&[1.0, -2.0], &[0.0, -1.0]),
        Some((-3, "BEARISH_CROSS"))
    );
    assert_eq!(
        classify_latest_spread(&[-8.0, -3.0], &[0.0, 0.0]),
        Some((2, "APPROACHING_BULLISH_NEAR"))
    );
    assert_eq!(
        classify_latest_spread(&[8.0, 3.0], &[0.0, 0.0]),
        Some((-2, "APPROACHING_BEARISH_NEAR"))
    );
}

#[test]
fn screener_requires_every_active_timeframe_before_aggregating_bias() {
    let history = bars(80);
    let inputs = DSS_BRESSERT_MTF_TIMEFRAMES
        .iter()
        .take(DSS_BRESSERT_MTF_TIMEFRAMES.len() - 1)
        .map(|frame| DssBressertTimeframeInput {
            timeframe: frame.timeframe,
            bars: &history,
        })
        .collect::<Vec<_>>();

    let result = calculate_dss_bressert_mtf(&inputs);

    assert!(!result.complete);
    assert_eq!(result.score, None);
    assert_eq!(result.bias, "UNAVAILABLE");
    assert_eq!(result.timeframes.len(), DSS_BRESSERT_MTF_TIMEFRAMES.len());
    assert_eq!(result.timeframes.last().unwrap().availability, "MISSING");
}

#[test]
fn fully_populated_screener_reports_a_weighted_bias() {
    let history = bars(80);
    let inputs = DSS_BRESSERT_MTF_TIMEFRAMES
        .iter()
        .map(|frame| DssBressertTimeframeInput {
            timeframe: frame.timeframe,
            bars: &history,
        })
        .collect::<Vec<_>>();

    let result = calculate_dss_bressert_mtf(&inputs);

    assert!(result.complete);
    assert!(result.score.is_some());
    assert_ne!(result.bias, "UNAVAILABLE");
    assert!(result
        .timeframes
        .iter()
        .all(|frame| frame.weighted_score.is_some()));
}

#[test]
fn short_history_is_reported_without_inventing_a_frame_state() {
    let history = bars(49);
    let inputs = DSS_BRESSERT_MTF_TIMEFRAMES
        .iter()
        .map(|frame| DssBressertTimeframeInput {
            timeframe: frame.timeframe,
            bars: &history,
        })
        .collect::<Vec<_>>();

    let result = calculate_dss_bressert_mtf(&inputs);

    assert!(!result.complete);
    assert_eq!(result.score, None);
    assert!(result
        .timeframes
        .iter()
        .all(|frame| frame.availability == "INSUFFICIENT_HISTORY"));
}
