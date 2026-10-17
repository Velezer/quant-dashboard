use super::{run_indicator_with_inputs, Candle};
use std::collections::HashMap;

fn candles(closes: &[f64]) -> Vec<Candle> {
    closes
        .iter()
        .enumerate()
        .map(|(index, close)| Candle {
            open: close - 0.5,
            high: close + 1.0,
            low: close - 1.0,
            close: *close,
            volume: 1.0,
            time: Some(index as i64),
        })
        .collect()
}

#[test]
fn input_source_uses_the_selected_indicator_series_instead_of_its_default() {
    let source = "//@version=6\nindicator(\"Envelope source\", overlay=false)\nupper = input.source(close, \"NWE Upper\")\nplot(upper, title=\"Upper\")";
    let bars = candles(&[10.0, 12.0, 14.0]);
    let inputs = HashMap::from([(
        "NWE Upper".to_string(),
        vec![Some(11.0), Some(13.0), Some(15.0)],
    )]);

    let output = run_indicator_with_inputs(source, &bars, "1d", &HashMap::new(), &inputs)
        .expect("selected source series should evaluate");

    assert_eq!(
        output.plots[0].values,
        vec![Some(11.0), Some(13.0), Some(15.0)]
    );
}

#[test]
fn missing_input_source_values_remain_missing_and_do_not_fall_back_to_close() {
    let source = "//@version=6\nindicator(\"Envelope source\", overlay=false)\nupper = input.source(close, \"NWE Upper\")\nplot(upper, title=\"Upper\")";
    let bars = candles(&[10.0, 12.0, 14.0]);
    let inputs = HashMap::from([("NWE Upper".to_string(), vec![Some(11.0), None, Some(15.0)])]);

    let output = run_indicator_with_inputs(source, &bars, "1d", &HashMap::new(), &inputs)
        .expect("missing band values should remain unavailable");

    assert_eq!(output.plots[0].values, vec![Some(11.0), None, Some(15.0)]);
}

#[test]
fn rejects_input_source_series_that_do_not_align_with_chart_bars() {
    let source = "//@version=6\nindicator(\"Envelope source\", overlay=false)\nupper = input.source(close, \"NWE Upper\")\nplot(upper, title=\"Upper\")";
    let bars = candles(&[10.0, 12.0]);
    let inputs = HashMap::from([("NWE Upper".to_string(), vec![Some(11.0)])]);

    let diagnostics = run_indicator_with_inputs(source, &bars, "1d", &HashMap::new(), &inputs)
        .expect_err("misaligned source arrays must be rejected");

    assert!(diagnostics[0].message.contains("must align"));
}
