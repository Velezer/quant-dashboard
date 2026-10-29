use super::common::finite;
use super::common::StrategyBar;
use crate::{calculate_dss, DssConfig};
use serde::Serialize;

pub const DSS_BRESSERT_MTF_STRATEGY_ID: &str = "dss_bressert_mtf_screener_v1";
pub const DSS_BRESSERT_MINIMUM_HISTORY_BARS: usize = 50;
pub const DSS_BRESSERT_NEAR_SPREAD_THRESHOLD: f64 = 5.0;

/// Timeframes exposed by the signal screener and its per-frame research models.
/// The composite DSS diagnostic below keeps its separate, versioned frame set.
pub const SIGNAL_SCREENER_TIMEFRAMES: &[&str] =
    &["15m", "30m", "1h", "2h", "4h", "1d", "1w"];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DssBressertTimeframe {
    pub timeframe: &'static str,
    pub weight: f64,
}

/// Active frames and weights from the supplied screener. The disabled 1M
/// frame is intentionally excluded, matching its TF_CONFIG.
pub const DSS_BRESSERT_MTF_TIMEFRAMES: &[DssBressertTimeframe] = &[
    DssBressertTimeframe {
        timeframe: "15m",
        weight: 0.20,
    },
    DssBressertTimeframe {
        timeframe: "1h",
        weight: 0.40,
    },
    DssBressertTimeframe {
        timeframe: "2h",
        weight: 0.75,
    },
    DssBressertTimeframe {
        timeframe: "4h",
        weight: 1.25,
    },
    DssBressertTimeframe {
        timeframe: "1d",
        weight: 2.00,
    },
    DssBressertTimeframe {
        timeframe: "1w",
        weight: 2.50,
    },
];

pub struct DssBressertTimeframeInput<'a> {
    pub timeframe: &'a str,
    pub bars: &'a [StrategyBar],
}

#[derive(Debug, Clone, Serialize)]
pub struct DssBressertFrameResult {
    pub timeframe: &'static str,
    pub state: &'static str,
    pub score: Option<i8>,
    pub weight: f64,
    pub weighted_score: Option<f64>,
    pub availability: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct DssBressertMtfResult {
    pub strategy_id: &'static str,
    pub complete: bool,
    pub score: Option<f64>,
    pub alignment_score: i8,
    pub bias: &'static str,
    pub timeframes: Vec<DssBressertFrameResult>,
}

pub(super) fn classify_latest_spread(dss: &[f64], trigger: &[f64]) -> Option<(i8, &'static str)> {
    if dss.len() < 2 || trigger.len() < 2 {
        return None;
    }
    let dss_now = dss[dss.len() - 1];
    let dss_previous = dss[dss.len() - 2];
    let trigger_now = trigger[trigger.len() - 1];
    let trigger_previous = trigger[trigger.len() - 2];
    if ![dss_now, dss_previous, trigger_now, trigger_previous]
        .into_iter()
        .all(finite)
    {
        return None;
    }

    let spread_now = dss_now - trigger_now;
    let spread_previous = dss_previous - trigger_previous;
    let approaching_bullish = spread_now < 0.0 && spread_now > spread_previous;
    let approaching_bearish = spread_now > 0.0 && spread_now < spread_previous;
    let near = spread_now.abs() <= DSS_BRESSERT_NEAR_SPREAD_THRESHOLD;

    Some(if spread_previous <= 0.0 && spread_now > 0.0 {
        (3, "BULLISH_CROSS")
    } else if spread_previous >= 0.0 && spread_now < 0.0 {
        (-3, "BEARISH_CROSS")
    } else if near && approaching_bullish {
        (2, "APPROACHING_BULLISH_NEAR")
    } else if near && approaching_bearish {
        (-2, "APPROACHING_BEARISH_NEAR")
    } else if approaching_bullish {
        (1, "APPROACHING_BULLISH")
    } else if approaching_bearish {
        (-1, "APPROACHING_BEARISH")
    } else {
        (0, "NEUTRAL")
    })
}

fn aggregate_frame_scores(frames: &[DssBressertFrameResult]) -> (Option<f64>, i8, &'static str) {
    if frames.len() != DSS_BRESSERT_MTF_TIMEFRAMES.len()
        || frames.iter().any(|frame| frame.score.is_none())
    {
        return (None, 0, "UNAVAILABLE");
    }

    let raw_score = frames
        .iter()
        .filter_map(|frame| frame.weighted_score)
        .sum::<f64>();
    let bullish = frames
        .iter()
        .filter(|frame| frame.score.is_some_and(|score| score > 0))
        .count();
    let bearish = frames
        .iter()
        .filter(|frame| frame.score.is_some_and(|score| score < 0))
        .count();
    let frame_count = frames.len();
    let alignment_score = if bullish > frame_count / 2 {
        if bullish == frame_count {
            2
        } else {
            1
        }
    } else if bearish > frame_count / 2 {
        if bearish == frame_count {
            -2
        } else {
            -1
        }
    } else {
        0
    };
    let score = raw_score + f64::from(alignment_score);
    let bias = if score > 0.0 {
        "BULLISH_BIAS"
    } else if score < 0.0 {
        "BEARISH_BIAS"
    } else {
        "MIXED_BIAS"
    };
    (Some(score), alignment_score, bias)
}

/// Calculate the supplied screener's latest per-timeframe states and weighted
/// alignment. This is a research diagnostic, not an order signal.
pub fn calculate_dss_bressert_mtf(
    inputs: &[DssBressertTimeframeInput<'_>],
) -> DssBressertMtfResult {
    let dss_config = DssConfig::default();
    let mut timeframes = Vec::with_capacity(DSS_BRESSERT_MTF_TIMEFRAMES.len());

    for definition in DSS_BRESSERT_MTF_TIMEFRAMES {
        let input = inputs
            .iter()
            .find(|input| input.timeframe == definition.timeframe);
        let Some(input) = input else {
            timeframes.push(DssBressertFrameResult {
                timeframe: definition.timeframe,
                state: "UNAVAILABLE",
                score: None,
                weight: definition.weight,
                weighted_score: None,
                availability: "MISSING",
            });
            continue;
        };
        if input.bars.len() < DSS_BRESSERT_MINIMUM_HISTORY_BARS {
            timeframes.push(DssBressertFrameResult {
                timeframe: definition.timeframe,
                state: "WARMING_UP",
                score: None,
                weight: definition.weight,
                weighted_score: None,
                availability: "INSUFFICIENT_HISTORY",
            });
            continue;
        }
        if input.bars.iter().any(|bar| {
            ![bar.open, bar.high, bar.low, bar.close]
                .into_iter()
                .all(finite)
                || bar.high < bar.low
        }) {
            timeframes.push(DssBressertFrameResult {
                timeframe: definition.timeframe,
                state: "UNAVAILABLE",
                score: None,
                weight: definition.weight,
                weighted_score: None,
                availability: "INVALID_DATA",
            });
            continue;
        }

        let highs: Vec<_> = input.bars.iter().map(|bar| bar.high).collect();
        let lows: Vec<_> = input.bars.iter().map(|bar| bar.low).collect();
        let closes: Vec<_> = input.bars.iter().map(|bar| bar.close).collect();
        let result = calculate_dss(&highs, &lows, &closes, &dss_config);
        let classified = classify_latest_spread(&result.dss, &result.trigger);
        let (score, state, availability) = match classified {
            Some((score, state)) => (Some(score), state, "READY"),
            None => (None, "WARMING_UP", "INDICATOR_UNAVAILABLE"),
        };
        timeframes.push(DssBressertFrameResult {
            timeframe: definition.timeframe,
            state,
            score,
            weight: definition.weight,
            weighted_score: score.map(|value| f64::from(value) * definition.weight),
            availability,
        });
    }

    let complete = timeframes.iter().all(|frame| frame.score.is_some());
    let (score, alignment_score, bias) = aggregate_frame_scores(&timeframes);
    DssBressertMtfResult {
        strategy_id: DSS_BRESSERT_MTF_STRATEGY_ID,
        complete,
        score,
        alignment_score,
        bias,
        timeframes,
    }
}
