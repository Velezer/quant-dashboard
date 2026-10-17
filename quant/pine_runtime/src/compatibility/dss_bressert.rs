use super::{compact_executable, normalize_markup};
use crate::runtime::{
    align_numeric_series_to_chart, Candle, Diagnostic, IndicatorOutput, PlotOutput, Validation,
    MAX_CANDLES, MAX_SOURCE_BYTES,
};
use std::collections::HashMap;

const DSS_TITLE: &str = "DSS Bressert (Double Smoothed Stochastic)";
const DSS_COPYRIGHT: &str = "Copyright by HPotter v1.0 24/04/2014";
const DSS_SOURCE_PROFILE_NOTICE: &str = "This Pine v4 source is recognized by a dedicated HPotter DSS compatibility profile; the registered source remains unchanged and only the source's declared defaults are evaluated.";
const DSS_TIMEFRAME_NOTICE: &str = "The source requests 60-minute bars. The runtime uses stored 1h history from the selected app and aligns confirmed values to chart bars; on 4h and 1d charts it samples the last available 1h value per chart bar, matching lookahead_off history. Session, gaps, and realtime behavior may differ from TradingView.";
const DSS_EMA_NOTICE: &str = "EMA warm-up and missing-value edges use the first finite input as the seed and may differ from TradingView.";
const DSS_PANE_NOTICE: &str = "The DSS and Trigger plots use the dashboard's 0–100 momentum pane; the source's 80/20 hlines are retained.";

const SUPPORTED_EXECUTABLE: &str = concat!(
    "study(title=\"DSSBressert(DoubleSmoothedStochastic)\",shorttitle=\"DSSBressert\")",
    "PDS=input(10,minval=1)",
    "EMAlen=input(9,minval=1)",
    "TriggerLen=input(5,minval=1)",
    "Overbought=input(80,minval=1)",
    "Oversold=input(20,minval=1)",
    "res=input(title=\"Resolution\",type=input.resolution,defval=\"60\")",
    "hline(Overbought,color=color.green,linestyle=hline.style_solid)",
    "hline(Oversold,color=color.red,linestyle=hline.style_solid)",
    "[c,h,l]=security(syminfo.tickerid,res,[close,high,low])",
    "xPreCalc=ema(stoch(c,h,l,PDS),EMAlen)",
    "xDSS=ema(stoch(xPreCalc,xPreCalc,xPreCalc,PDS),EMAlen)",
    "xTrigger=ema(xDSS,TriggerLen)",
    "plot(xDSS,color=color.blue,title=\"DSS\")",
    "plot(xTrigger,color=color.red,title=\"Trigger\")",
);

pub(crate) fn validate_hpotter_dss(source: &str) -> Option<Validation> {
    if !source.contains(DSS_TITLE) {
        return None;
    }
    Some(match validate_profile_source(source) {
        Ok(()) => Validation {
            valid: true,
            script_type: "indicator".to_string(),
            version: 4,
            name: DSS_TITLE.to_string(),
            overlay: false,
            approximations: vec![
                DSS_SOURCE_PROFILE_NOTICE.to_string(),
                DSS_TIMEFRAME_NOTICE.to_string(),
                DSS_EMA_NOTICE.to_string(),
                DSS_PANE_NOTICE.to_string(),
            ],
            diagnostics: Vec::new(),
        },
        Err(message) => invalid_validation(message),
    })
}

pub(crate) fn run_hpotter_dss(
    source: &str,
    candles: &[Candle],
    chart_timeframe: &str,
    available_timeframes: &HashMap<String, Vec<Candle>>,
) -> Option<Result<IndicatorOutput, Vec<Diagnostic>>> {
    if !source.contains(DSS_TITLE) {
        return None;
    }
    Some(run_profile(
        source,
        candles,
        chart_timeframe,
        available_timeframes,
    ))
}

fn validate_profile_source(source: &str) -> Result<(), String> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(format!(
            "Pine source exceeds the {MAX_SOURCE_BYTES} byte limit."
        ));
    }
    let normalized = normalize_markup(source);
    let has_v4 = normalized.lines().any(|line| {
        line.trim_start_matches('*')
            .trim()
            .starts_with("//@version=4")
    });
    if !has_v4 {
        return Err("The HPotter DSS profile requires the supplied Pine v4 source.".to_string());
    }
    if !normalized.contains(DSS_COPYRIGHT) {
        return Err(
            "The HPotter DSS source must retain its original copyright notice.".to_string(),
        );
    }
    let compact = compact_executable(&normalized);
    if compact != SUPPORTED_EXECUTABLE {
        let mismatch = compact
            .bytes()
            .zip(SUPPORTED_EXECUTABLE.bytes())
            .position(|(actual, expected)| actual != expected)
            .unwrap_or(compact.len().min(SUPPORTED_EXECUTABLE.len()));
        let actual = compact.as_bytes().get(mismatch).copied();
        let expected = SUPPORTED_EXECUTABLE.as_bytes().get(mismatch).copied();
        return Err(format!("This DSS title matches HPotter's source, but its executable code differs from the supported reference profile at character {mismatch} (found {actual:?}, expected {expected:?}). No fallback interpretation was applied."));
    }
    Ok(())
}

fn run_profile(
    source: &str,
    candles: &[Candle],
    chart_timeframe: &str,
    available_timeframes: &HashMap<String, Vec<Candle>>,
) -> Result<IndicatorOutput, Vec<Diagnostic>> {
    validate_profile_source(source).map_err(|message| vec![Diagnostic::error(1, 1, message)])?;
    if candles.len() > MAX_CANDLES {
        return Err(vec![Diagnostic::error(
            1,
            1,
            format!("Indicator input exceeds the {MAX_CANDLES} candle limit."),
        )]);
    }
    if candles.iter().any(|candle| {
        ![
            candle.open,
            candle.high,
            candle.low,
            candle.close,
            candle.volume,
        ]
        .iter()
        .all(|value| value.is_finite())
    }) {
        return Err(vec![Diagnostic::error(
            1,
            1,
            "DSS input contains a non-finite candle value.",
        )]);
    }

    let requested_timeframe =
        crate::runtime::normalize_supported_timeframe("60").ok_or_else(|| {
            vec![Diagnostic::error(
                1,
                1,
                "The 60-minute source resolution is unavailable in the Pine history adapter.",
            )]
        })?;
    let normalized_chart_timeframe = crate::runtime::normalize_supported_timeframe(chart_timeframe)
        .ok_or_else(|| {
            vec![Diagnostic::error(
                1,
                1,
                "Selected chart timeframe is not supported by the Pine history adapter.",
            )]
        })?;
    let requested_bars = if requested_timeframe == normalized_chart_timeframe {
        available_timeframes
            .get(&requested_timeframe)
            .filter(|bars| !bars.is_empty())
            .map(Vec::as_slice)
            .unwrap_or(candles)
    } else {
        available_timeframes
            .get(&requested_timeframe)
            .filter(|bars| !bars.is_empty())
            .map(Vec::as_slice)
            .ok_or_else(|| {
                vec![Diagnostic::error(
                    1,
                    1,
                    "No approved stored 1h candle series is available for this selected app instrument. Load local 1h history and use a 15m chart to evaluate this source.",
                )]
            })?
    };
    if requested_bars.len() > MAX_CANDLES {
        return Err(vec![Diagnostic::error(
            1,
            1,
            format!("The requested 1h history exceeds the {MAX_CANDLES} candle limit."),
        )]);
    }
    if requested_bars.iter().any(|candle| {
        ![
            candle.open,
            candle.high,
            candle.low,
            candle.close,
            candle.volume,
        ]
        .iter()
        .all(|value| value.is_finite())
    }) {
        return Err(vec![Diagnostic::error(
            1,
            1,
            "Requested 1h history contains a non-finite candle value.",
        )]);
    }

    let raw_stochastic = stochastic(
        &requested_bars
            .iter()
            .map(|candle| Some(candle.close))
            .collect::<Vec<_>>(),
        &requested_bars
            .iter()
            .map(|candle| Some(candle.high))
            .collect::<Vec<_>>(),
        &requested_bars
            .iter()
            .map(|candle| Some(candle.low))
            .collect::<Vec<_>>(),
        10,
    );
    let pre_calculated = ema(&raw_stochastic, 9);
    let second_stochastic = stochastic(&pre_calculated, &pre_calculated, &pre_calculated, 10);
    let dss = ema(&second_stochastic, 9);
    let trigger = ema(&dss, 5);

    let dss = align_numeric_series_to_chart(
        candles,
        chart_timeframe,
        requested_bars,
        &requested_timeframe,
        dss,
    )?;
    let trigger = align_numeric_series_to_chart(
        candles,
        chart_timeframe,
        requested_bars,
        &requested_timeframe,
        trigger,
    )?;
    let chart_length = candles.len();
    let mut diagnostics = vec![
        Diagnostic::warning(DSS_SOURCE_PROFILE_NOTICE),
        Diagnostic::warning(DSS_TIMEFRAME_NOTICE),
        Diagnostic::warning(DSS_EMA_NOTICE),
        Diagnostic::warning(DSS_PANE_NOTICE),
    ];
    if candles.len() < 2 || requested_bars.len() < 20 {
        diagnostics.push(Diagnostic::warning(
            "The DSS plots need enough stored 1h candles to warm up both stochastic and EMA stages.",
        ));
    }
    Ok(IndicatorOutput {
        valid: true,
        name: DSS_TITLE.to_string(),
        overlay: false,
        plots: vec![
            plot("DSS", "#2962ff", dss),
            plot("Trigger", "#f23645", trigger),
            constant_plot("Overbought", "#089981", 80.0, chart_length),
            constant_plot("Oversold", "#f23645", 20.0, chart_length),
        ],
        diagnostics: std::mem::take(&mut diagnostics),
        requested_timeframes: vec![requested_timeframe],
    })
}

fn stochastic(
    source: &[Option<f64>],
    highs: &[Option<f64>],
    lows: &[Option<f64>],
    period: usize,
) -> Vec<Option<f64>> {
    source
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let value = (*value)?;
            let start = index.saturating_add(1).saturating_sub(period);
            let high = highs[start..=index]
                .iter()
                .flatten()
                .copied()
                .reduce(f64::max)?;
            let low = lows[start..=index]
                .iter()
                .flatten()
                .copied()
                .reduce(f64::min)?;
            let range = high - low;
            (range.is_finite() && range != 0.0)
                .then_some(100.0 * (value - low) / range)
                .filter(|result| result.is_finite())
        })
        .collect()
}

fn ema(values: &[Option<f64>], period: usize) -> Vec<Option<f64>> {
    let alpha = 2.0 / (period as f64 + 1.0);
    let mut previous = None;
    values
        .iter()
        .map(|value| {
            let next = (*value).filter(|value| value.is_finite()).map(|value| {
                let next = previous.map_or(value, |prior| alpha * value + (1.0 - alpha) * prior);
                previous = Some(next);
                next
            });
            next.filter(|value| value.is_finite())
        })
        .collect()
}

fn plot(title: &str, color: &str, values: Vec<Option<f64>>) -> PlotOutput {
    PlotOutput {
        title: title.to_string(),
        overlay: false,
        color: color.to_string(),
        kind: "line".to_string(),
        values,
    }
}

fn constant_plot(title: &str, color: &str, value: f64, length: usize) -> PlotOutput {
    PlotOutput {
        title: title.to_string(),
        overlay: false,
        color: color.to_string(),
        kind: "hline".to_string(),
        values: vec![Some(value); length],
    }
}

fn invalid_validation(message: String) -> Validation {
    Validation {
        valid: false,
        script_type: "indicator".to_string(),
        version: 4,
        name: DSS_TITLE.to_string(),
        overlay: false,
        approximations: Vec::new(),
        diagnostics: vec![Diagnostic::error(1, 1, message)],
    }
}
