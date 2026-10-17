use super::{run_indicator_with_timeframes, validate_source, Candle};
use std::collections::HashMap;

const DSS_SOURCE: &str = r#"//@version=4
////////////////////////////////////////////////////////////
//  Copyright by HPotter v1.0 24/04/2014
// Double Smoothed Stochastics (DSS) is designed by William Blaw.
// It attempts to combine moving average methods with oscillator principles.
////////////////////////////////////////////////////////////
study(title="DSS Bressert (Double Smoothed Stochastic)", shorttitle="DSS Bressert")
PDS = input(10, minval=1)
EMAlen = input(9, minval=1)
TriggerLen = input(5, minval=1)
Overbought = input(80, minval=1)
Oversold = input(20, minval=1)
res = input(title="Resolution", type=input.resolution, defval="60")
hline(Overbought, color=color.green, linestyle=hline.style_solid)
hline(Oversold, color=color.red, linestyle=hline.style_solid)
[c,h,l] = security(syminfo.tickerid, res, [close,high,low])
xPreCalc = ema(stoch(c, h, l, PDS), EMAlen)
xDSS = ema(stoch(xPreCalc, xPreCalc, xPreCalc, PDS), EMAlen)
//xDSS = stoch(xPreCalc, xPreCalc, xPreCalc, PDS)
xTrigger = ema(xDSS, TriggerLen)
plot(xDSS, color=color.blue, title="DSS")
plot(xTrigger, color=color.red, title="Trigger")"#;

fn candle(index: usize, timeframe_seconds: i64) -> Candle {
    let close = 100.0 + index as f64 * 0.2 + ((index * 7) % 9) as f64 * 0.8;
    Candle {
        open: close - 0.1,
        high: close + 1.0,
        low: close - 1.0,
        close,
        volume: 1_000.0 + index as f64,
        time: Some(index as i64 * timeframe_seconds * 1_000_000_000),
    }
}

#[test]
fn hpotter_dss_reference_is_validated_as_v4_with_explicit_approximations() {
    let validation = validate_source(DSS_SOURCE);

    assert!(validation.valid, "{validation:?}");
    assert_eq!(validation.version, 4);
    assert_eq!(validation.name, "DSS Bressert (Double Smoothed Stochastic)");
    assert!(!validation.overlay);
    assert!(validation
        .approximations
        .iter()
        .any(|notice| notice.contains("stored 1h history")));
    assert!(validation
        .approximations
        .iter()
        .any(|notice| notice.contains("0–100 momentum pane")));
}

#[test]
fn hpotter_dss_profile_rejects_changed_executable_logic() {
    let changed = DSS_SOURCE.replace("plot(xDSS, color=color.blue", "plot(xDSS, color=color.red");
    let validation = validate_source(&changed);

    assert!(!validation.valid);
    assert!(validation.diagnostics[0]
        .message
        .contains("differs from the supported reference profile"));
}

#[test]
fn hpotter_dss_uses_confirmed_stored_one_hour_values_and_returns_pane_plots() {
    let chart_bars = (0..160)
        .map(|index| candle(index, 15 * 60))
        .collect::<Vec<_>>();
    let hourly_bars = (0..40)
        .map(|index| candle(index, 60 * 60))
        .collect::<Vec<_>>();
    let mut timeframes = HashMap::new();
    timeframes.insert("1h".to_string(), hourly_bars);

    let output = run_indicator_with_timeframes(DSS_SOURCE, &chart_bars, "15m", &timeframes)
        .expect("the supported DSS profile should evaluate");

    assert!(output.valid);
    assert!(!output.overlay);
    assert_eq!(output.requested_timeframes, vec!["1h"]);
    assert_eq!(output.plots.len(), 4);
    for plot in &output.plots {
        assert!(!plot.overlay);
        assert_eq!(plot.values.len(), chart_bars.len());
    }
    let dss = &output.plots[0].values;
    let trigger = &output.plots[1].values;
    assert!(dss
        .iter()
        .flatten()
        .all(|value| (0.0..=100.0).contains(value)));
    assert!(trigger.iter().any(Option::is_some));
    assert_eq!(output.plots[2].values, vec![Some(80.0); chart_bars.len()]);
    assert_eq!(output.plots[3].values, vec![Some(20.0); chart_bars.len()]);
}

#[test]
fn hpotter_dss_samples_last_confirmed_hour_on_four_hour_chart_bars() {
    let chart_bars = (0..40)
        .map(|index| candle(index, 4 * 60 * 60))
        .collect::<Vec<_>>();
    let hourly_bars = (0..160)
        .map(|index| candle(index, 60 * 60))
        .collect::<Vec<_>>();
    let mut timeframes = HashMap::new();
    timeframes.insert("1h".to_string(), hourly_bars);

    let four_hour_output =
        run_indicator_with_timeframes(DSS_SOURCE, &chart_bars, "4h", &timeframes)
            .expect("confirmed 1h values should be sampled at each 4h chart close");
    let hourly_output = run_indicator_with_timeframes(
        DSS_SOURCE,
        timeframes.get("1h").expect("hourly bars were inserted"),
        "1h",
        &timeframes,
    )
    .expect("the registered source should calculate on its declared 1h resolution");

    for (chart_index, hourly_index) in [(0, 3), (1, 7), (2, 11), (3, 15), (39, 159)] {
        assert_eq!(
            four_hour_output.plots[0].values[chart_index],
            hourly_output.plots[0].values[hourly_index]
        );
        assert_eq!(
            four_hour_output.plots[1].values[chart_index],
            hourly_output.plots[1].values[hourly_index]
        );
    }
    assert_eq!(four_hour_output.requested_timeframes, vec!["1h"]);
    assert_eq!(
        four_hour_output.plots[2].values,
        vec![Some(80.0); chart_bars.len()]
    );
    assert_eq!(
        four_hour_output.plots[3].values,
        vec![Some(20.0); chart_bars.len()]
    );
    assert!(four_hour_output
        .diagnostics
        .iter()
        .any(|item| item.message.contains("last available 1h value")));
}

#[test]
fn hpotter_dss_requires_approved_stored_one_hour_history() {
    let chart_bars = (0..40)
        .map(|index| candle(index, 15 * 60))
        .collect::<Vec<_>>();

    let diagnostics =
        run_indicator_with_timeframes(DSS_SOURCE, &chart_bars, "15m", &HashMap::new())
            .expect_err("the profile must not substitute another timeframe");

    assert!(diagnostics[0]
        .message
        .contains("No approved stored 1h candle series"));
}
