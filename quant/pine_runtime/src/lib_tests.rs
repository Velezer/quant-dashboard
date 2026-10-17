use super::{run_indicator, run_indicator_with_timeframes, validate_source, Candle};
use std::collections::HashMap;

fn bars(closes: &[f64]) -> Vec<Candle> {
    closes
        .iter()
        .enumerate()
        .map(|(index, close)| Candle {
            open: close - 0.5,
            high: close + 1.0,
            low: close - 1.0,
            close: *close,
            volume: 10.0 + index as f64,
            time: Some(index as i64),
        })
        .collect()
}

fn luxalgo_nwe_v5_profile() -> &'static str {
    r#"// This work is licensed under CC BY-NC-SA 4.0
// https://creativecommons.org/licenses/by-nc-sa/4.0/
// © LuxAlgo
//@version=5
indicator("Nadaraya-Watson Envelope [LuxAlgo]", "LuxAlgo - Nadaraya-Watson Envelope", overlay=true, max_lines_count=500, max_labels_count=500, max_bars_back=500)
h=input.float(8.,'Bandwidth', minval=0)
mult=input.float(3., minval=0)
src=input(close,'Source')
repaint=input(true,'Repainting Smoothing')
gauss(x,h)=>math.exp(-(math.pow(x,2)/(h*h*2)))
var ln=array.new_line(0)
if barstate.isfirst and repaint
    for i=0 to 499
        array.push(ln,line.new(na,na,na,na))
var coefs=array.new_float(0)
var den=0.
if barstate.isfirst and not repaint
    for i=0 to 499
        w=gauss(i,h)
        coefs.push(w)
    den:=coefs.sum()
out=0.
if not repaint
    for i=0 to 499
        out+=src[i]*coefs.get(i)
out/=den
mae=ta.sma(math.abs(src-out),499)*mult
float y2=na
float y1=na
nwe=array.new<float>(0)
if barstate.islast and repaint
    sae=0.
    for i=0 to math.min(499,n-1)
        sum=0.
        sumw=0.
        for j=0 to math.min(499,n-1)
            w=gauss(i-j,h)
            sum+=src[j]*w
            sumw+=w
        y2:=sum/sumw
        sae+=math.abs(src[i]-y2)
        nwe.push(y2)
    sae:=sae/math.min(499,n-1)*mult
    for i=0 to math.min(499,n-1)
        if i%2
            line.new(n-i+1,y1+sae,n-i,nwe.get(i)+sae,color=upCss)
            line.new(n-i+1,y1-sae,n-i,nwe.get(i)-sae,color=dnCss)
        if src[i]>nwe.get(i)+sae and src[i+1]<nwe.get(i)+sae
            label.new(n-i,src[i],'▼')
        if src[i]<nwe.get(i)-sae and src[i+1]>nwe.get(i)-sae
            label.new(n-i,src[i],'▲')
        y1:=nwe.get(i)
var tb=table.new(position.top_right,1,1)
if repaint
    tb.cell(0,0,'Repainting Mode Enabled')
plot(repaint?na:out+mae,'Upper',upCss)
plot(repaint?na:out-mae,'Lower',dnCss)
plotshape(ta.crossunder(close,out-mae)?low:na,"Crossunder",shape.labelup,location.absolute,color(na),0,text='▲',textcolor=upCss,size=size.tiny)
plotshape(ta.crossover(close,out+mae)?high:na,"Crossover",shape.labeldown,location.absolute,color(na),0,text='▼',textcolor=dnCss,size=size.tiny)
"#
}

#[test]
fn parses_indicator_metadata_and_named_plot_arguments() {
    let validation = validate_source("//@version=6\nindicator(\"MA\", overlay=true)\nplot(close, title=\"Close\", color=color.aqua)");
    assert!(validation.valid, "{:?}", validation.diagnostics);
    assert_eq!(validation.name, "MA");
    assert!(validation.overlay);
}

#[test]
fn rejects_indicators_without_any_output_call_with_ce10246() {
    let validation = validate_source("//@version=6\nindicator(\"Empty\")\nmean = ta.sma(close, 3)");

    assert!(!validation.valid);
    assert!(validation
        .diagnostics
        .iter()
        .any(|item| item.message.contains("CE10246")));
}

#[test]
fn accepts_each_supported_indicator_output_category() {
    let output_calls = [
        "plot(close)",
        "plotshape(close > open)",
        "plotchar(close > open)",
        "hline(50)",
        "alertcondition(close > open, title=\"Up\")",
        "barcolor(close > open ? color.green : color.red)",
        "bgcolor(close > open ? color.green : color.red)",
        "plotarrow(close > open ? 1 : -1)",
        "line.new(0, close, 1, close)",
        "label.new(0, close, \"signal\")",
        "box.new(0, close, 1, open)",
        "table.new(position.top_right, 1, 1)",
        "polyline.new(close)",
    ];

    for (index, output_call) in output_calls.iter().enumerate() {
        let source = format!("//@version=6\nindicator(\"Output {index}\")\n{output_call}");
        let validation = validate_source(&source);
        assert!(
            !validation
                .diagnostics
                .iter()
                .any(|item| item.message.contains("CE10246")),
            "output call `{output_call}` should satisfy CE10246: {validation:?}"
        );
    }
}

#[test]
fn detects_drawing_calls_and_ignores_mentions_in_comments() {
    let comment_only = validate_source("//@version=6\nindicator(\"Comment only\")\n// plot(close)");
    assert!(comment_only
        .diagnostics
        .iter()
        .any(|item| item.message.contains("CE10246")));

    let drawing =
        validate_source("//@version=6\nindicator(\"Drawing\")\nline.new(0, close, 1, close)");
    assert!(!drawing
        .diagnostics
        .iter()
        .any(|item| item.message.contains("CE10246")));
}

#[test]
fn requires_exactly_one_declaration_statement_with_ce10243() {
    let cases = [
        ("//@version=6\nplot(close)", 0usize),
        (
            "//@version=6\nindicator(\"First\")\nplot(close)\nindicator(\"Second\")",
            2,
        ),
        (
            "//@version=6\nindicator(\"Indicator\")\nplot(close)\nstrategy(\"Strategy\")",
            2,
        ),
    ];

    for (source, expected_count) in cases {
        let validation = validate_source(source);
        assert!(
            !validation.valid,
            "source should fail declaration count: {source}"
        );
        assert!(
            validation.diagnostics.iter().any(|item| {
                item.message.contains("CE10243")
                    && item.message.contains(&expected_count.to_string())
            }),
            "missing CE10243 count {expected_count}: {validation:?}"
        );
    }
}

#[test]
fn one_indicator_declaration_does_not_trigger_ce10243() {
    let validation = validate_source("//@version=6\nindicator(\"One\")\nplot(close)");

    assert!(!validation
        .diagnostics
        .iter()
        .any(|item| item.message.contains("CE10243")));
}

#[test]
fn nwe_dss_backtest_pine_has_one_strategy_declaration_and_chart_outputs() {
    let source = include_str!("../examples/nwe_dss_bressert_backtest.pine");
    let validation = validate_source(source);

    assert!(source.contains("strategy(\"NWE + DSS Bressert"));
    assert!(!source.contains("indicator("));
    assert!(source.contains("plot(showEnvelope"));
    assert!(source.contains("plotshape(longEntry"));
    assert!(
        !validation
            .diagnostics
            .iter()
            .any(|item| { item.message.contains("CE10243") || item.message.contains("CE10246") }),
        "NWE/DSS Pine must satisfy declaration/output requirements: {validation:?}"
    );
}

#[test]
fn evaluates_series_history_and_sma() {
    let output = run_indicator(
        "//@version=6\nindicator(\"Mean\", overlay=true)\nmean = ta.sma(close, 3)\nplot(mean, title=\"Mean\")",
        &bars(&[1.0, 2.0, 3.0, 4.0]),
    ).expect("supported Pine indicator should run");
    assert_eq!(output.plots.len(), 1);
    assert_eq!(
        output.plots[0].values,
        vec![None, None, Some(2.0), Some(3.0)]
    );
}

#[test]
fn calculates_crossover_without_turning_it_into_execution_evidence() {
    let output = run_indicator(
        "//@version=6\nindicator(\"Cross\", overlay=true)\nplotshape(ta.crossover(close, ta.sma(close, 2)), title=\"Cross\")",
        &bars(&[1.0, 1.0, 3.0, 4.0]),
    ).expect("supported indicator should run");
    assert_eq!(output.plots[0].kind, "shape");
    assert_eq!(output.plots[0].values[2], Some(3.0));
}

#[test]
fn accepts_strategy_and_rejects_unavailable_multitimeframe_requests() {
    let strategy = validate_source("//@version=6\nstrategy(\"Orders\")");
    assert!(strategy.valid, "{:?}", strategy.diagnostics);
    assert_eq!(strategy.name, "Orders");

    let request = validate_source(
        "//@version=6\nindicator(\"MTF\")\nplot(request.security(syminfo.tickerid, \"1D\", close))",
    );
    assert!(request.valid, "{:?}", request.diagnostics);
    let no_history = run_indicator_with_timeframes(
        "//@version=6\nindicator(\"MTF\")\nplot(request.security(syminfo.tickerid, \"4h\", close))",
        &bars(&[10.0]),
        "15m",
        &HashMap::new(),
    );
    assert!(no_history.is_err());
    assert!(no_history
        .unwrap_err()
        .iter()
        .any(|item| item.message.contains("No approved stored 4h")));
}

#[test]
fn higher_timeframe_security_aligns_confirmed_same_instrument_bars() {
    let quarter_hour_ns = 15_i64 * 60 * 1_000_000_000;
    let four_hour_ns = 4_i64 * 60 * 60 * 1_000_000_000;
    let mut chart = bars(&(1..=16).map(|value| value as f64).collect::<Vec<_>>());
    for (index, bar) in chart.iter_mut().enumerate() {
        bar.time = Some(index as i64 * quarter_hour_ns);
    }
    let mut four_hour = bars(&[100.0, 110.0]);
    four_hour[0].time = Some(0);
    four_hour[1].time = Some(four_hour_ns);
    let mut available = HashMap::new();
    available.insert("4h".to_string(), four_hour);

    let output = run_indicator_with_timeframes(
        "//@version=6\nindicator(\"HTF\", overlay=true)\nplot(request.security(syminfo.tickerid, \"240\", close), title=\"4H close\")",
        &chart,
        "15m",
        &available,
    ).expect("confirmed higher timeframe series should run");
    assert!(output.plots[0].values[..15].iter().all(Option::is_none));
    assert_eq!(output.plots[0].values[15], Some(100.0));
    assert_eq!(output.requested_timeframes, vec!["4h"]);
    assert!(output
        .diagnostics
        .iter()
        .any(|item| item.severity == "approximation"));
}

#[test]
fn rejects_lookahead_and_lower_timeframe_security() {
    let mut quarter_hour = bars(&[10.0]);
    quarter_hour[0].time = Some(0);
    let mut four_hour_bars = bars(&[100.0]);
    four_hour_bars[0].time = Some(0);
    let higher_tf = HashMap::from([("4h".to_string(), four_hour_bars)]);
    let lookahead = run_indicator_with_timeframes(
        "//@version=6\nindicator(\"Future\")\nplot(request.security(syminfo.tickerid, \"4h\", close, lookahead=barmerge.lookahead_on))",
        &quarter_hour,
        "15m",
        &higher_tf,
    ).expect_err("lookahead-on can leak future values");
    assert!(lookahead
        .iter()
        .any(|item| item.message.contains("lookahead_on")));

    let mut four_hour_chart = bars(&[10.0]);
    four_hour_chart[0].time = Some(0);
    let lower_tf = HashMap::from([("15m".to_string(), quarter_hour.clone())]);
    let lower = run_indicator_with_timeframes(
        "//@version=6\nindicator(\"Lower\")\nplot(request.security(syminfo.tickerid, \"15m\", close))",
        &four_hour_chart,
        "4h",
        &lower_tf,
    ).expect_err("lower timeframe selection is not supported");
    assert!(lower
        .iter()
        .any(|item| item.message.contains("Lower-timeframe")));
}

#[test]
fn identifies_historical_only_barstate_and_executes_supported_bounded_loops() {
    let historical = run_indicator(
        "//@version=6\nindicator(\"State\")\nplot(barstate.isrealtime ? 1 : 0)",
        &bars(&[10.0]),
    )
    .expect("barstate.isrealtime should be deterministic for historical input");
    assert_eq!(historical.plots[0].values, vec![Some(0.0)]);

    let loop_output = run_indicator(
        "//@version=6\nindicator(\"Loop\")\nfor i = 0 to 2\n    plot(i)",
        &bars(&[10.0]),
    )
    .expect("bounded for loops should execute");
    assert_eq!(loop_output.plots[0].values, vec![Some(2.0)]);
}

#[test]
fn refuses_unknown_functions_and_identifiers_instead_of_emitting_empty_series() {
    let unknown_call =
        validate_source("//@version=6\nindicator(\"Unknown\")\nplot(my_custom_function(close))");
    assert!(!unknown_call.valid);
    assert!(unknown_call
        .diagnostics
        .iter()
        .any(|item| item.message.contains("Unsupported function")));

    let unknown_name = validate_source("//@version=6\nindicator(\"Typo\")\nplot(clsoe)");
    assert!(!unknown_name.valid);
    assert!(unknown_name
        .diagnostics
        .iter()
        .any(|item| item.message.contains("Unsupported Pine identifier")));
}

#[test]
fn supplied_luxalgo_nwe_v5_profile_runs_with_license_diagnostics() {
    let source = luxalgo_nwe_v5_profile();
    let validation = validate_source(source);
    assert!(validation.valid, "{:?}", validation.diagnostics);
    assert_eq!(validation.version, 5);
    assert_eq!(validation.name, "Nadaraya-Watson Envelope [LuxAlgo]");
    assert!(validation
        .approximations
        .iter()
        .any(|item| item.contains("noncommercial")));

    let candles = bars(
        &(0..520)
            .map(|index| 100.0 + (index as f64 * 0.13).sin() * 2.0)
            .collect::<Vec<_>>(),
    );
    let output = run_indicator(source, &candles).expect("recognized NWE profile runs");
    assert!(output.valid);
    assert_eq!(output.plots.len(), 4);
    assert_eq!(output.plots[0].title, "Upper");
    assert_eq!(output.plots[0].kind, "dashed");
    assert_eq!(output.plots[2].kind, "shape_down");
    assert_eq!(output.plots[0].values.len(), candles.len());
    assert!(output.plots[0].values.iter().any(Option::is_some));
}

#[test]
fn luxalgo_profile_rejects_changed_math_and_missing_license_notices() {
    let changed_math = luxalgo_nwe_v5_profile().replace("gauss(x,h)=>", "gauss(x,h):=");
    assert!(!validate_source(&changed_math).valid);

    let missing_license = luxalgo_nwe_v5_profile().replace("CC BY-NC-SA 4.0", "License removed");
    assert!(!validate_source(&missing_license).valid);
}
